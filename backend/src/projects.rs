use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{
    access::{self, Role},
    auth::AuthUser,
    error::AppError,
    now_iso, AppState,
};

pub const STATUSES: [&str; 4] = ["planning", "active", "on_hold", "completed"];
/// Pengurutan yang diizinkan. Nilai dari klien hanya dipetakan ke klausa ini, tidak pernah langsung ke SQL.
fn order_clause(sort: Option<&str>) -> &'static str {
    match sort {
        Some("name") => "p.name COLLATE NOCASE ASC",
        Some("created") => "p.created_at DESC",
        Some("due") => "(p.due_date IS NULL), p.due_date ASC, p.name COLLATE NOCASE ASC",
        Some("type") => "COALESCE((SELECT t.label FROM project_types t WHERE t.owner_id = p.owner_id AND t.slug = p.project_type), p.project_type) COLLATE NOCASE ASC, p.name COLLATE NOCASE ASC",
        Some("status") => "CASE p.status WHEN 'active' THEN 0 WHEN 'planning' THEN 1 WHEN 'on_hold' THEN 2 ELSE 3 END, p.updated_at DESC",
        _ => "p.updated_at DESC",
    }
}

#[derive(Debug, Serialize, FromRow)]
pub struct Project {
    pub id: String,
    #[serde(skip)]
    #[allow(dead_code)]
    pub owner_id: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub project_type: String,
    /// Semua jenis project, dalam urutan abjad label. Diisi dari `type_slugs`.
    #[sqlx(skip)]
    pub project_types: Vec<String>,
    #[serde(skip)]
    pub type_slugs: Option<String>,
    pub project_type_label: String,
    pub owner_name: String,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub file_count: i64,
    pub req_total: i64,
    pub req_done: i64,
    /// owner, editor, atau viewer, sesuai pengguna yang sedang melihat.
    pub role: String,
}

pub const PROJECT_SELECT: &str = "SELECT p.id, p.owner_id, p.name, p.description, p.status, p.project_type,
    (SELECT GROUP_CONCAT(slug, ',') FROM (SELECT l.type_slug AS slug FROM project_type_links l WHERE l.project_id = p.id ORDER BY l.type_slug)) AS type_slugs,
    COALESCE((SELECT GROUP_CONCAT(label, ' · ') FROM (
        SELECT t.label AS label FROM project_type_links l
        JOIN project_types t ON t.owner_id = p.owner_id AND t.slug = l.type_slug
        WHERE l.project_id = p.id ORDER BY t.label COLLATE NOCASE)), p.project_type) AS project_type_label,
    p.owner_name, p.start_date, p.due_date, p.created_at, p.updated_at,
    (SELECT COUNT(*) FROM project_files f WHERE f.project_id = p.id) AS file_count,
    (SELECT COUNT(*) FROM project_requirements r WHERE r.project_id = p.id) AS req_total,
    (SELECT COUNT(*) FROM project_requirements r
        WHERE r.project_id = p.id
          AND EXISTS (SELECT 1 FROM project_files f WHERE f.requirement_id = r.id)) AS req_done,
    (CASE WHEN p.owner_id = ? THEN 'owner'
          ELSE COALESCE((SELECT m.role FROM project_members m WHERE m.project_id = p.id AND m.user_id = ?), 'viewer')
     END) AS role
    FROM projects p";

#[derive(Deserialize)]
pub struct ListQuery {
    status: Option<String>,
    /// Di URL disebut `type`, sesuai dengan nama parameter di halaman daftar.
    #[serde(rename = "type")]
    project_type: Option<String>,
    sort: Option<String>,
}

impl Project {
    /// Mengisi `project_types` dari kolom gabungan `type_slugs`.
    pub fn finish(mut self) -> Self {
        let mut slugs: Vec<String> = self
            .type_slugs
            .take()
            .unwrap_or_default()
            .split(',')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        slugs.sort();
        self.project_types = slugs;
        self
    }
}

const MAX_TYPES_PER_PROJECT: usize = 8;

#[derive(Deserialize)]
pub struct ProjectInput {
    name: String,
    description: Option<String>,
    status: Option<String>,
    /// Bentuk lama (satu jenis); tetap diterima agar klien lama tidak rusak.
    project_type: Option<String>,
    #[serde(default)]
    project_types: Vec<String>,
    owner_name: Option<String>,
    start_date: Option<String>,
    due_date: Option<String>,
}

struct Valid {
    name: String,
    description: String,
    status: String,
    /// Jenis utama (pertama) dan seluruh daftar jenis.
    project_type: String,
    project_types: Vec<String>,
    owner_name: String,
    start_date: Option<String>,
    due_date: Option<String>,
}

/// Menyimpan daftar jenis project, menggantikan yang lama. Harus dipanggil dalam transaksi.
pub async fn replace_types(
    conn: &mut sqlx::SqliteConnection,
    project_id: &str,
    slugs: &[String],
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM project_type_links WHERE project_id = ?")
        .bind(project_id)
        .execute(&mut *conn)
        .await?;
    for slug in slugs {
        sqlx::query("INSERT INTO project_type_links (project_id, type_slug) VALUES (?, ?)")
            .bind(project_id)
            .bind(slug)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
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
            "Nama project wajib diisi (maks. 150 karakter)".into(),
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
        return Err(AppError::BadRequest("Status project tidak dikenal".into()));
    }

    // Gabungkan bentuk baru (daftar) dan bentuk lama (satu jenis), buang duplikat.
    let mut project_types: Vec<String> = input
        .project_types
        .into_iter()
        .chain(input.project_type)
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    project_types.sort();
    project_types.dedup();
    if project_types.is_empty() {
        project_types.push("lainnya".into());
    }
    if project_types.len() > MAX_TYPES_PER_PROJECT {
        return Err(AppError::BadRequest(format!(
            "Maksimal {MAX_TYPES_PER_PROJECT} jenis per project"
        )));
    }
    if project_types.iter().any(|t| t.chars().count() > 60) {
        return Err(AppError::BadRequest("Jenis project tidak valid".into()));
    }

    let owner_name = input.owner_name.unwrap_or_default().trim().to_string();
    if owner_name.chars().count() > 150 {
        return Err(AppError::BadRequest(
            "Nama pemilik maksimal 150 karakter".into(),
        ));
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
        project_type: project_types[0].clone(),
        project_types,
        owner_name,
        start_date: start.map(|d| d.to_string()),
        due_date: due.map(|d| d.to_string()),
    })
}

/// Mengambil project milik pengguna; mengembalikan 404 jika tidak ada atau bukan miliknya.
/// Project yang bisa diakses pengguna (pemilik atau anggota). Di luar itu: 404.
pub async fn fetch_project(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
) -> Result<Project, AppError> {
    let sql = format!(
        "{PROJECT_SELECT} WHERE p.id = ? AND (p.owner_id = ? OR p.id IN (SELECT project_id FROM project_members WHERE user_id = ?))"
    );
    sqlx::query_as::<_, Project>(&sql)
        .bind(user_id)
        .bind(user_id)
        .bind(id)
        .bind(user_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .map(Project::finish)
        .ok_or_else(|| AppError::NotFound("Project tidak ditemukan".into()))
}

pub async fn project_owner(pool: &SqlitePool, id: &str) -> Result<String, AppError> {
    let row: Option<(String,)> = sqlx::query_as("SELECT owner_id FROM projects WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.map(|(o,)| o)
        .ok_or_else(|| AppError::NotFound("Project tidak ditemukan".into()))
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Project>>, AppError> {
    let status = q.status.filter(|s| STATUSES.contains(&s.as_str()));
    let project_type = q.project_type.map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty());
    let sql = format!(
        "{PROJECT_SELECT} WHERE (p.owner_id = ? OR p.id IN (SELECT project_id FROM project_members WHERE user_id = ?))
           AND (? IS NULL OR p.status = ?)
           AND (? IS NULL OR EXISTS (SELECT 1 FROM project_type_links l WHERE l.project_id = p.id AND l.type_slug = ?))
         ORDER BY {}",
        order_clause(q.sort.as_deref())
    );
    let rows = sqlx::query_as::<_, Project>(&sql)
        .bind(&user.id)
        .bind(&user.id)
        .bind(&user.id)
        .bind(&user.id)
        .bind(&status)
        .bind(&status)
        .bind(&project_type)
        .bind(&project_type)
        .fetch_all(&state.pool)
        .await?
        .into_iter()
        .map(Project::finish)
        .collect();
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<ProjectInput>,
) -> Result<(StatusCode, Json<Project>), AppError> {
    let v = validate(input)?;
    for slug in &v.project_types {
        crate::types::ensure_exists(&state.pool, &user.id, slug).await?;
    }
    let id = Uuid::new_v4().to_string();
    let now = now_iso();

    // Satu transaksi: project dan daftar jenisnya tersimpan bersama.
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO projects (id, owner_id, name, description, status, project_type, owner_name, start_date, due_date, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&v.name)
    .bind(&v.description)
    .bind(&v.status)
    .bind(&v.project_type)
    .bind(&v.owner_name)
    .bind(&v.start_date)
    .bind(&v.due_date)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    replace_types(&mut *tx, &id, &v.project_types).await?;
    tx.commit().await?;

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
    access::require(&state.pool, &id, &user.id, Role::Editor).await?;
    // Jenis dicek terhadap daftar pemilik project, bukan daftar pengedit.
    let owner = project_owner(&state.pool, &id).await?;
    let v = validate(input)?;
    for slug in &v.project_types {
        crate::types::ensure_exists(&state.pool, &owner, slug).await?;
    }

    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE projects
         SET name = ?, description = ?, status = ?, project_type = ?, owner_name = ?, start_date = ?, due_date = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(&v.name)
    .bind(&v.description)
    .bind(&v.status)
    .bind(&v.project_type)
    .bind(&v.owner_name)
    .bind(&v.start_date)
    .bind(&v.due_date)
    .bind(now_iso())
    .bind(&id)
    .execute(&mut *tx)
    .await?;
    replace_types(&mut *tx, &id, &v.project_types).await?;
    tx.commit().await?;

    crate::events::publish(&state, &id, "project");
    Ok(Json(fetch_project(&state.pool, &id, &user.id).await?))
}

pub async fn delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    access::require(&state.pool, &id, &user.id, Role::Owner).await?;
    let result = sqlx::query("DELETE FROM projects WHERE id = ? AND owner_id = ?")
        .bind(&id)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Project tidak ditemukan".into()));
    }

    // Berkas di disk ikut dihapus; baris berkas sudah terhapus lewat ON DELETE CASCADE.
    let _ = tokio::fs::remove_dir_all(state.upload_dir.join(&id)).await;
    crate::events::publish(&state, &id, "project");
    Ok(StatusCode::NO_CONTENT)
}

const COPY_SUFFIX: &str = " (salinan)";

/// Nama salinan: "<nama> (salinan)", dipotong agar tetap di bawah batas 150 karakter.
fn copy_name(original: &str) -> String {
    let keep = 150 - COPY_SUFFIX.chars().count();
    let base: String = original.chars().take(keep).collect();
    format!("{base}{COPY_SUFFIX}")
}

/// Menduplikat project: detail, informasi detail, dan daftar kelengkapan ikut disalin; berkas tidak.
/// Status salinan kembali ke "Perencanaan" karena ini project baru.
pub async fn duplicate(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Project>), AppError> {
    access::require(&state.pool, &id, &user.id, Role::Editor).await?;
    let source = fetch_project(&state.pool, &id, &user.id).await?;
    // Jenis salinan: hanya jenis yang masih ada di daftar pengguna; bila tidak ada, "lainnya".
    let mut dup_types: Vec<String> = Vec::new();
    for slug in &source.project_types {
        if crate::types::ensure_exists(&state.pool, &user.id, slug).await.is_ok() {
            dup_types.push(slug.clone());
        }
    }
    if dup_types.is_empty() {
        dup_types.push("lainnya".to_string());
    }
    let dup_primary = dup_types[0].clone();

    let new_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let name = copy_name(&source.name);

    // Satu transaksi: salinan tidak pernah tersimpan setengah jadi.
    let mut tx = state.pool.begin().await?;

    sqlx::query(
        "INSERT INTO projects (id, owner_id, name, description, status, project_type, owner_name, start_date, due_date, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'planning', ?, ?, ?, ?, ?, ?)",
    )
    .bind(&new_id)
    .bind(&user.id)
    .bind(&name)
    .bind(&source.description)
    .bind(&dup_primary)
    .bind(&source.owner_name)
    .bind(&source.start_date)
    .bind(&source.due_date)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    replace_types(&mut *tx, &new_id, &dup_types).await?;

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
