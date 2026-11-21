use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{auth::AuthUser, error::AppError, now_iso, AppState};

pub const STATUSES: [&str; 4] = ["planning", "active", "on_hold", "completed"];
pub const PROJECT_TYPES: [&str; 7] = [
    "administrasi",
    "konstruksi",
    "lainnya",
    "operasional",
    "pemasaran",
    "riset",
    "teknologi",
];

/// Pengurutan yang diizinkan. Nilai dari klien hanya dipetakan ke klausa ini, tidak pernah langsung ke SQL.
fn order_clause(sort: Option<&str>) -> &'static str {
    match sort {
        Some("name") => "p.name COLLATE NOCASE ASC",
        Some("created") => "p.created_at DESC",
        Some("due") => "(p.due_date IS NULL), p.due_date ASC, p.name COLLATE NOCASE ASC",
        Some("type") => "p.project_type ASC, p.name COLLATE NOCASE ASC",
        Some("status") => "CASE p.status WHEN 'active' THEN 0 WHEN 'planning' THEN 1 WHEN 'on_hold' THEN 2 ELSE 3 END, p.updated_at DESC",
        _ => "p.updated_at DESC",
    }
}

#[derive(Debug, Serialize, FromRow)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub project_type: String,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub file_count: i64,
    pub req_total: i64,
    pub req_done: i64,
}

const PROJECT_SELECT: &str = "SELECT p.id, p.name, p.description, p.status, p.project_type, p.start_date, p.due_date,
    p.created_at, p.updated_at,
    (SELECT COUNT(*) FROM project_files f WHERE f.project_id = p.id) AS file_count,
    (SELECT COUNT(*) FROM project_requirements r WHERE r.project_id = p.id) AS req_total,
    (SELECT COUNT(*) FROM project_requirements r
        WHERE r.project_id = p.id
          AND EXISTS (SELECT 1 FROM project_files f WHERE f.requirement_id = r.id)) AS req_done
    FROM projects p";

#[derive(Deserialize)]
pub struct ListQuery {
    status: Option<String>,
    /// Di URL disebut `type`, sesuai dengan nama parameter di halaman daftar.
    #[serde(rename = "type")]
    project_type: Option<String>,
    sort: Option<String>,
}

#[derive(Deserialize)]
pub struct ProjectInput {
    name: String,
    description: Option<String>,
    status: Option<String>,
    project_type: Option<String>,
    start_date: Option<String>,
    due_date: Option<String>,
}

struct Valid {
    name: String,
    description: String,
    status: String,
    project_type: String,
    start_date: Option<String>,
    due_date: Option<String>,
}

fn parse_date(value: Option<String>, label: &str) -> Result<Option<NaiveDate>, AppError> {
    match value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) => NaiveDate::parse_from_str(&v, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| AppError::BadRequest(format!("{label} tidak valid (format YYYY-MM-DD)"))),
    }
}

fn validate(input: ProjectInput) -> Result<Valid, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 150 {
        return Err(AppError::BadRequest(
            "Nama proyek wajib diisi (maks. 150 karakter)".into(),
        ));
    }

    let description = input.description.unwrap_or_default().trim().to_string();
    if description.chars().count() > 5000 {
        return Err(AppError::BadRequest(
            "Deskripsi maksimal 5000 karakter".into(),
        ));
    }

    let status = input.status.unwrap_or_else(|| "planning".into());
    if !STATUSES.contains(&status.as_str()) {
        return Err(AppError::BadRequest("Status proyek tidak dikenal".into()));
    }

    let project_type = input.project_type.unwrap_or_else(|| "lainnya".into());
    if !PROJECT_TYPES.contains(&project_type.as_str()) {
        return Err(AppError::BadRequest("Jenis proyek tidak dikenal".into()));
    }

    let start = parse_date(input.start_date, "Tanggal mulai")?;
    let due = parse_date(input.due_date, "Tanggal selesai")?;
    if let (Some(s), Some(d)) = (start, due) {
        if d < s {
            return Err(AppError::BadRequest(
                "Tanggal selesai tidak boleh sebelum tanggal mulai".into(),
            ));
        }
    }

    Ok(Valid {
        name,
        description,
        status,
        project_type,
        start_date: start.map(|d| d.to_string()),
        due_date: due.map(|d| d.to_string()),
    })
}

/// Mengambil proyek milik pengguna; mengembalikan 404 jika tidak ada atau bukan miliknya.
pub async fn fetch_project(
    pool: &SqlitePool,
    id: &str,
    owner_id: &str,
) -> Result<Project, AppError> {
    let sql = format!("{PROJECT_SELECT} WHERE p.id = ? AND p.owner_id = ?");
    sqlx::query_as::<_, Project>(&sql)
        .bind(id)
        .bind(owner_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Proyek tidak ditemukan".into()))
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Project>>, AppError> {
    let status = q.status.filter(|s| STATUSES.contains(&s.as_str()));
    let project_type = q.project_type.filter(|t| PROJECT_TYPES.contains(&t.as_str()));
    let sql = format!(
        "{PROJECT_SELECT} WHERE p.owner_id = ?
           AND (? IS NULL OR p.status = ?)
           AND (? IS NULL OR p.project_type = ?)
         ORDER BY {}",
        order_clause(q.sort.as_deref())
    );
    let rows = sqlx::query_as::<_, Project>(&sql)
        .bind(&user.id)
        .bind(&status)
        .bind(&status)
        .bind(&project_type)
        .bind(&project_type)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<ProjectInput>,
) -> Result<(StatusCode, Json<Project>), AppError> {
    let v = validate(input)?;
    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    sqlx::query(
        "INSERT INTO projects (id, owner_id, name, description, status, project_type, start_date, due_date, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&v.name)
    .bind(&v.description)
    .bind(&v.status)
    .bind(&v.project_type)
    .bind(&v.start_date)
    .bind(&v.due_date)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    let project = fetch_project(&state.pool, &id, &user.id).await?;
    Ok((StatusCode::CREATED, Json(project)))
}

pub async fn get(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<Project>, AppError> {
    Ok(Json(fetch_project(&state.pool, &id, &user.id).await?))
}

pub async fn update(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(input): Json<ProjectInput>,
) -> Result<Json<Project>, AppError> {
    fetch_project(&state.pool, &id, &user.id).await?;
    let v = validate(input)?;

    sqlx::query(
        "UPDATE projects
         SET name = ?, description = ?, status = ?, project_type = ?, start_date = ?, due_date = ?, updated_at = ?
         WHERE id = ? AND owner_id = ?",
    )
    .bind(&v.name)
    .bind(&v.description)
    .bind(&v.status)
    .bind(&v.project_type)
    .bind(&v.start_date)
    .bind(&v.due_date)
    .bind(now_iso())
    .bind(&id)
    .bind(&user.id)
    .execute(&state.pool)
    .await?;

    Ok(Json(fetch_project(&state.pool, &id, &user.id).await?))
}

pub async fn delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM projects WHERE id = ? AND owner_id = ?")
        .bind(&id)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Proyek tidak ditemukan".into()));
    }

    // Berkas di disk ikut dihapus; baris berkas sudah terhapus lewat ON DELETE CASCADE.
    let _ = tokio::fs::remove_dir_all(state.upload_dir.join(&id)).await;
    Ok(StatusCode::NO_CONTENT)
}

const COPY_SUFFIX: &str = " (salinan)";

/// Nama salinan: "<nama> (salinan)", dipotong agar tetap di bawah batas 150 karakter.
fn copy_name(original: &str) -> String {
    let keep = 150 - COPY_SUFFIX.chars().count();
    let base: String = original.chars().take(keep).collect();
    format!("{base}{COPY_SUFFIX}")
}

/// Menduplikat proyek: detail, informasi detail, dan daftar kelengkapan ikut disalin; berkas tidak.
/// Status salinan kembali ke "Perencanaan" karena ini proyek baru.
pub async fn duplicate(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Project>), AppError> {
    let source = fetch_project(&state.pool, &id, &user.id).await?;

    let new_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let name = copy_name(&source.name);

    // Satu transaksi: salinan tidak pernah tersimpan setengah jadi.
    let mut tx = state.pool.begin().await?;

    sqlx::query(
        "INSERT INTO projects (id, owner_id, name, description, status, project_type, start_date, due_date, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'planning', ?, ?, ?, ?, ?)",
    )
    .bind(&new_id)
    .bind(&user.id)
    .bind(&name)
    .bind(&source.description)
    .bind(&source.project_type)
    .bind(&source.start_date)
    .bind(&source.due_date)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    let items: Vec<(String, String)> = sqlx::query_as(
        "SELECT title, created_at FROM project_requirements WHERE project_id = ? ORDER BY created_at ASC",
    )
    .bind(&id)
    .fetch_all(&mut *tx)
    .await?;

    for (title, created_at) in items {
        sqlx::query(
            "INSERT INTO project_requirements (id, project_id, title, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&new_id)
        .bind(&title)
        .bind(&created_at)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    // Informasi detail ikut disalin (nilainya tetap, bisa diubah di salinan).
    let details: Vec<(String, String, String, i64, String)> = sqlx::query_as(
        "SELECT label, field_type, value, position, created_at FROM project_fields WHERE project_id = ?",
    )
    .bind(&id)
    .fetch_all(&mut *tx)
    .await?;

    for (label, field_type, value, position, created_at) in details {
        sqlx::query(
            "INSERT INTO project_fields (id, project_id, label, field_type, value, position, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&new_id)
        .bind(&label)
        .bind(&field_type)
        .bind(&value)
        .bind(position)
        .bind(&created_at)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let project = fetch_project(&state.pool, &new_id, &user.id).await?;
    Ok((StatusCode::CREATED, Json(project)))
}
