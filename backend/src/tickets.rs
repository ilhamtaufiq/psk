use std::collections::HashMap;

use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    access::{self, Role},
    auth::AuthUser,
    comments::{MAX_ATTACHMENTS, MAX_ATTACHMENT_BYTES},
    error::AppError,
    files::{content_disposition, sanitize_filename},
    now_iso, AppState,
};

#[derive(Serialize, Clone)]
pub struct TicketAttachment {
    pub id: String,
    pub original_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
}

#[derive(FromRow)]
struct TicketAttachmentRow {
    id: String,
    ticket_id: String,
    original_name: String,
    mime_type: String,
    size_bytes: i64,
}

fn ticket_dir(state: &AppState, project_id: &str, ticket_id: &str) -> std::path::PathBuf {
    state.upload_dir.join(project_id).join("tickets").join(ticket_id)
}

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
    // Notifikasi di dalam aplikasi selalu disimpan, dengan atau tanpa SMTP.
    let link = format!("/projects/{project_id}?status=open#tiket");
    crate::notifications::add(&state.pool, recipients, actor, "ticket", &subject, &text, &link).await;

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
    #[sqlx(skip)]
    #[serde(default)]
    pub attachments: Vec<TicketAttachment>,
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
    let attachment_rows = sqlx::query_as::<_, TicketAttachmentRow>(
        "SELECT id, ticket_id, original_name, mime_type, size_bytes FROM ticket_attachments
         WHERE project_id = ? ORDER BY created_at",
    )
    .bind(&project_id)
    .fetch_all(&state.pool)
    .await?;
    let mut by_ticket: HashMap<String, Vec<TicketAttachment>> = HashMap::new();
    for a in attachment_rows {
        by_ticket.entry(a.ticket_id).or_default().push(TicketAttachment {
            id: a.id,
            original_name: a.original_name,
            mime_type: a.mime_type,
            size_bytes: a.size_bytes,
        });
    }
    for t in &mut rows {
        let author = t.created_by == user.id;
        // Status, prioritas, dan penanggung jawab hanya bisa diubah pemilik dan editor.
        t.can_manage = editor;
        t.can_delete = editor || author;
        t.attachments = by_ticket.remove(&t.id).unwrap_or_default();
    }
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<Ticket>), AppError> {
    let my_role = access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;

    // Formulir multipart: title, description, priority, dan nol atau lebih field attachment.
    let bad_form = || AppError::BadRequest("Formulir tiket tidak valid".into());
    let mut title = String::new();
    let mut description = String::new();
    let mut priority = String::new();
    let mut files: Vec<(String, String, Vec<u8>)> = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|_| bad_form())? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "title" => title = field.text().await.map_err(|_| bad_form())?,
            "description" => description = field.text().await.map_err(|_| bad_form())?,
            "priority" => priority = field.text().await.map_err(|_| bad_form())?,
            "attachment" => {
                let file_name = sanitize_filename(field.file_name().unwrap_or("lampiran"));
                let declared = field.content_type().unwrap_or("").to_string();
                let data = field.bytes().await.map_err(|_| bad_form())?.to_vec();
                if data.is_empty() {
                    continue;
                }
                if data.len() > MAX_ATTACHMENT_BYTES {
                    return Err(AppError::BadRequest("Lampiran maksimal 10 MB per berkas".into()));
                }
                if files.len() >= MAX_ATTACHMENTS {
                    return Err(AppError::BadRequest(format!(
                        "Maksimal {MAX_ATTACHMENTS} lampiran per tiket"
                    )));
                }
                let mime = if declared.is_empty() || declared == "application/octet-stream" {
                    mime_guess::from_path(&file_name).first_or_octet_stream().to_string()
                } else {
                    declared
                };
                files.push((file_name, mime, data));
            }
            _ => {}
        }
    }

    let input = TicketInput {
        title,
        description: Some(description),
        priority: if priority.trim().is_empty() { None } else { Some(priority) },
    };
    let (title, description, priority) = validate_text(&input)?;
    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    // Berkas ditulis ke disk dulu; bila penyimpanan database gagal, folder dibersihkan lagi.
    let dir = ticket_dir(&state, &project_id, &id);
    let mut stored: Vec<(String, String, String, Vec<u8>)> = Vec::with_capacity(files.len());
    if !files.is_empty() {
        tokio::fs::create_dir_all(&dir).await?;
    }
    for (file_name, mime, data) in files {
        let att_id = Uuid::new_v4().to_string();
        if let Err(e) = tokio::fs::write(dir.join(&att_id), &data).await {
            let _ = tokio::fs::remove_dir_all(&dir).await;
            return Err(e.into());
        }
        stored.push((att_id, file_name, mime, data));
    }

    let result: Result<(), AppError> = async {
        let mut tx = state.pool.begin().await?;
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
        .execute(&mut *tx)
        .await?;
        for (att_id, file_name, mime, data) in &stored {
            sqlx::query(
                "INSERT INTO ticket_attachments (id, ticket_id, project_id, original_name, mime_type, size_bytes, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(att_id)
            .bind(&id)
            .bind(&project_id)
            .bind(file_name)
            .bind(mime)
            .bind(data.len() as i64)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
    .await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(e);
    }

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
    // Pemilik dan editor boleh mengubah; pembuat (viewer) hanya boleh menghapus.
    ticket.can_manage = matches!(my_role, Role::Owner | Role::Editor);
    ticket.can_delete = true;
    ticket.attachments = stored
        .iter()
        .map(|(att_id, file_name, mime, data)| TicketAttachment {
            id: att_id.clone(),
            original_name: file_name.clone(),
            mime_type: mime.clone(),
            size_bytes: data.len() as i64,
        })
        .collect();
    Ok((StatusCode::CREATED, Json(ticket)))
}

/// Unduhan lampiran tiket: selalu sebagai lampiran, dengan nosniff dan CSP sandbox.
pub async fn download(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT project_id, ticket_id, original_name, mime_type FROM ticket_attachments WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;
    let (project_id, ticket_id, name, mime) =
        row.ok_or_else(|| AppError::NotFound("Lampiran tidak ditemukan".into()))?;
    access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;
    let data = tokio::fs::read(ticket_dir(&state, &project_id, &ticket_id).join(&id))
        .await
        .map_err(|_| AppError::NotFound("Berkas lampiran tidak ditemukan di disk".into()))?;

    let content_type = HeaderValue::from_str(&mime)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, content_disposition(&name, false))
        .header(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"))
        .header(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("sandbox; default-src 'none'"),
        )
        .body(Body::from(data))
        .map(IntoResponse::into_response)
        .map_err(|e| AppError::Internal(e.to_string()))
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
    let _ = tokio::fs::remove_dir_all(ticket_dir(&state, &project_id, &id)).await;
    crate::events::publish(&state, &project_id, "ticket");
    Ok(StatusCode::NO_CONTENT)
}
