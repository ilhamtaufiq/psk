use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    access::{self, Role},
    auth::AuthUser,
    error::AppError,
    now_iso, AppState,
};

const STATUSES: [&str; 3] = ["open", "in_progress", "done"];
const PRIORITIES: [&str; 3] = ["low", "normal", "high"];
const MAX_TITLE_CHARS: usize = 150;
const MAX_DESCRIPTION_CHARS: usize = 2000;

fn status_text(status: &str) -> &'static str {
    match status {
        "in_progress" => "Dikerjakan",
        "done" => "Selesai",
        _ => "Terbuka",
    }
}

/// Email ke pengguna terkait tiket. Dikirim di latar belakang; tanpa SMTP tidak ada yang dikirim.
/// Pelaku perubahan (`actor`) tidak menerima email tentang tindakannya sendiri.
async fn notify(
    state: &AppState,
    project_id: &str,
    recipients: &[String],
    actor: &str,
    subject: String,
    text: String,
) {
    let Some(cfg) = state.mail.clone() else { return };
    let project_name: String = sqlx::query_scalar("SELECT name FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or_else(|_| "tanpa nama".into());
    let url = format!("{}/projects/{}#tiket", state.app_url, project_id);

    let mut seen: Vec<&String> = Vec::new();
    for uid in recipients {
        if uid == actor || seen.contains(&uid) {
            continue;
        }
        seen.push(uid);
        let contact: Option<(String, String)> =
            sqlx::query_as("SELECT email, name FROM users WHERE id = ?")
                .bind(uid)
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten();
        let Some((email, name)) = contact else { continue };
        let body = format!(
            "Halo {name},\n\n{text}\n\nProject: {project_name}\nBuka tiket:\n{url}\n\nSalam,\nProject Selepas Kerja\n"
        );
        let cfg = cfg.clone();
        let subject = subject.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::mail::send(&cfg, &email, &subject, &body).await {
                tracing::warn!("gagal mengirim email tiket: {e}");
            }
        });
    }
}

/// Pemilik dan editor project.
async fn staff_ids(pool: &sqlx::SqlitePool, project_id: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT owner_id FROM projects WHERE id = ?
         UNION SELECT user_id FROM project_members WHERE project_id = ? AND role = 'editor'",
    )
    .bind(project_id)
    .bind(project_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

#[derive(Serialize, FromRow)]
pub struct Ticket {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: String,
    pub assignee_id: Option<String>,
    pub assignee_name: Option<String>,
    pub created_by: String,
    pub created_by_name: String,
    pub created_at: String,
    pub updated_at: String,
    /// Pemilik dan editor boleh mengubah tiket.
    #[sqlx(skip)]
    #[serde(default)]
    pub can_manage: bool,
    #[sqlx(skip)]
    #[serde(default)]
    pub can_delete: bool,
}

const TICKET_SELECT: &str = "SELECT t.id, t.title, t.description, t.status, t.priority, t.assignee_id,
    a.name AS assignee_name, t.created_by, c.name AS created_by_name, t.created_at, t.updated_at
    FROM tickets t
    JOIN users c ON c.id = t.created_by
    LEFT JOIN users a ON a.id = t.assignee_id";

#[derive(Deserialize)]
pub struct TicketInput {
    title: String,
    description: Option<String>,
    priority: Option<String>,
}

#[derive(Deserialize)]
pub struct TicketUpdate {
    status: Option<String>,
    priority: Option<String>,
    /// Kosong atau null melepas penanggung jawab.
    assignee_id: Option<String>,
}

fn validate_text(input: &TicketInput) -> Result<(String, String, String), AppError> {
    let title = input.title.trim().to_string();
    if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
        return Err(AppError::BadRequest(format!(
            "Judul tiket wajib diisi (maks. {MAX_TITLE_CHARS} karakter)"
        )));
    }
    let description = input.description.clone().unwrap_or_default().trim().to_string();
    if description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(AppError::BadRequest(format!(
            "Deskripsi maksimal {MAX_DESCRIPTION_CHARS} karakter"
        )));
    }
    let priority = input.priority.clone().unwrap_or_else(|| "normal".into());
    if !PRIORITIES.contains(&priority.as_str()) {
        return Err(AppError::BadRequest("Prioritas tidak dikenal".into()));
    }
    Ok((title, description, priority))
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<Ticket>>, AppError> {
    let my_role = access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;
    let sql = format!("{TICKET_SELECT} WHERE t.project_id = ? ORDER BY t.created_at DESC");
    let mut rows = sqlx::query_as::<_, Ticket>(&sql)
        .bind(&project_id)
        .fetch_all(&state.pool)
        .await?;
    let editor = matches!(my_role, Role::Owner | Role::Editor);
    for t in &mut rows {
        let author = t.created_by == user.id;
        // Status, prioritas, dan penanggung jawab hanya bisa diubah pemilik dan editor.
        t.can_manage = editor;
        t.can_delete = editor || author;
    }
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
    Json(input): Json<TicketInput>,
) -> Result<(StatusCode, Json<Ticket>), AppError> {
    let my_role = access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;
    let (title, description, priority) = validate_text(&input)?;
    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO tickets (id, project_id, title, description, status, priority, created_by, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'open', ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&title)
    .bind(&description)
    .bind(&priority)
    .bind(&user.id)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    crate::events::publish(&state, &project_id, "ticket");
    notify(
        &state,
        &project_id,
        &staff_ids(&state.pool, &project_id).await,
        &user.id,
        format!("Tiket baru: {title}"),
        format!(
            "{} membuat tiket baru.\n\nJudul: {title}\nPrioritas: {priority}\nStatus: Terbuka",
            user.name
        ),
    )
    .await;
    let sql = format!("{TICKET_SELECT} WHERE t.id = ?");
    let mut ticket = sqlx::query_as::<_, Ticket>(&sql).bind(&id).fetch_one(&state.pool).await?;
    // Sama seperti daftar: hanya pemilik dan editor yang bisa mengubah; pembuat selalu bisa menghapus.
    ticket.can_manage = matches!(my_role, Role::Owner | Role::Editor);
    ticket.can_delete = true;
    Ok((StatusCode::CREATED, Json(ticket)))
}

/// Mengubah status, prioritas, atau penanggung jawab. Hanya pemilik dan editor.
pub async fn update(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(input): Json<TicketUpdate>,
) -> Result<Json<Ticket>, AppError> {
    let project_id: Option<(String,)> = sqlx::query_as("SELECT project_id FROM tickets WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.pool)
        .await?;
    let (project_id,) = project_id.ok_or_else(|| AppError::NotFound("Tiket tidak ditemukan".into()))?;
    access::require(&state.pool, &project_id, &user.id, Role::Editor).await?;
    // Keadaan sebelum perubahan, untuk mendeteksi apa yang berubah.
    let before: (String, String, Option<String>, String) = sqlx::query_as(
        "SELECT title, status, assignee_id, created_by FROM tickets WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.pool)
    .await?;

    if let Some(status) = &input.status {
        if !STATUSES.contains(&status.as_str()) {
            return Err(AppError::BadRequest("Status tiket tidak dikenal".into()));
        }
    }
    if let Some(priority) = &input.priority {
        if !PRIORITIES.contains(&priority.as_str()) {
            return Err(AppError::BadRequest("Prioritas tidak dikenal".into()));
        }
    }

    // Penanggung jawab harus pemilik atau anggota project.
    let assignee = input.assignee_id.clone().filter(|s| !s.trim().is_empty());
    if let Some(aid) = &assignee {
        let ok: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM projects p WHERE p.id = ? AND (p.owner_id = ? OR p.id IN (SELECT project_id FROM project_members WHERE user_id = ?))",
        )
        .bind(&project_id)
        .bind(aid)
        .bind(aid)
        .fetch_optional(&state.pool)
        .await?;
        if ok.is_none() {
            return Err(AppError::BadRequest("Penanggung jawab harus anggota project".into()));
        }
    }

    // Field yang tidak dikirim tetap; penanggung jawab dikirim eksplisit (kosong = lepas).
    sqlx::query(
        "UPDATE tickets SET
            status = COALESCE(?, status),
            priority = COALESCE(?, priority),
            assignee_id = ?,
            updated_at = ?
         WHERE id = ?",
    )
    .bind(&input.status)
    .bind(&input.priority)
    .bind(&assignee)
    .bind(now_iso())
    .bind(&id)
    .execute(&state.pool)
    .await?;

    crate::events::publish(&state, &project_id, "ticket");
    let (title, old_status, old_assignee, created_by) = before;
    let new_status: String = sqlx::query_scalar("SELECT status FROM tickets WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;

    // Penanggung jawab baru diberi tahu bila berganti.
    if assignee != old_assignee {
        if let Some(aid) = &assignee {
            notify(
                &state,
                &project_id,
                std::slice::from_ref(aid),
                &user.id,
                format!("Anda ditugaskan: {title}"),
                format!("{} menugaskan Anda ke tiket \"{title}\".", user.name),
            )
            .await;
        }
    }
    // Pembuat dan penanggung jawab diberi tahu bila status berubah.
    if new_status != old_status {
        let mut to = vec![created_by.clone()];
        if let Some(aid) = assignee.clone().or(old_assignee) {
            to.push(aid);
        }
        notify(
            &state,
            &project_id,
            &to,
            &user.id,
            format!("Status tiket berubah: {title}"),
            format!(
                "{} mengubah status tiket \"{title}\" menjadi {}.",
                user.name,
                status_text(&new_status)
            ),
        )
        .await;
    }

    let sql = format!("{TICKET_SELECT} WHERE t.id = ?");
    let mut ticket = sqlx::query_as::<_, Ticket>(&sql).bind(&id).fetch_one(&state.pool).await?;
    ticket.can_manage = true;
    ticket.can_delete = true;
    Ok(Json(ticket))
}

pub async fn delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT project_id, created_by FROM tickets WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await?;
    let (project_id, author) = row.ok_or_else(|| AppError::NotFound("Tiket tidak ditemukan".into()))?;
    let my_role = access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;
    if author != user.id && !matches!(my_role, Role::Owner | Role::Editor) {
        return Err(AppError::Forbidden("Hanya pembuat, pemilik, atau editor yang bisa menghapus tiket".into()));
    }
    sqlx::query("DELETE FROM tickets WHERE id = ?").bind(&id).execute(&state.pool).await?;
    crate::events::publish(&state, &project_id, "ticket");
    Ok(StatusCode::NO_CONTENT)
}
