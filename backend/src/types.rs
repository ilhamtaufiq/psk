use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{
    access::{self, Role},
    auth::AuthUser,
    error::AppError,
    now_iso, AppState,
};

/// Jenis bawaan yang di-seed untuk setiap pengguna: (slug, label).
pub const DEFAULT_TYPES: [(&str, &str); 7] = [
    ("administrasi", "Administrasi"),
    ("konstruksi", "Konstruksi"),
    ("lainnya", "Lainnya"),
    ("operasional", "Operasional"),
    ("pemasaran", "Pemasaran"),
    ("riset", "Riset & Pengembangan"),
    ("teknologi", "Teknologi / IT"),
];

const MAX_TYPES_PER_USER: i64 = 50;
const MAX_LABEL_CHARS: usize = 60;

#[derive(Debug, Serialize, FromRow)]
pub struct ProjectType {
    pub id: String,
    pub slug: String,
    pub label: String,
    pub project_count: i64,
}

const TYPE_SELECT: &str = "SELECT t.id, t.slug, t.label,
    (SELECT COUNT(*) FROM projects p WHERE p.owner_id = t.owner_id AND p.project_type = t.slug) AS project_count
    FROM project_types t";

#[derive(Deserialize)]
pub struct LabelInput {
    label: String,
}

/// Mengisi jenis bawaan untuk pengguna baru. Aman dipanggil berulang (INSERT OR IGNORE).
pub async fn seed_defaults(pool: &SqlitePool, owner_id: &str) -> Result<(), AppError> {
    let now = now_iso();
    for (slug, label) in DEFAULT_TYPES {
        sqlx::query(
            "INSERT OR IGNORE INTO project_types (id, owner_id, slug, label, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(owner_id)
        .bind(slug)
        .bind(label)
        .bind(&now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Memastikan slug jenis ada di daftar pengguna. Dipanggil saat membuat atau mengubah project.
pub async fn ensure_exists(pool: &SqlitePool, owner_id: &str, slug: &str) -> Result<(), AppError> {
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM project_types WHERE owner_id = ? AND slug = ?")
            .bind(owner_id)
            .bind(slug)
            .fetch_one(pool)
            .await?;
    if count == 0 {
        return Err(AppError::BadRequest(
            "Jenis project tidak dikenal. Tambahkan dulu di menu Jenis project.".into(),
        ));
    }
    Ok(())
}

/// Membuat slug dari nama: huruf kecil ASCII dan tanda hubung.
fn slugify(label: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in label.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            out.push(c);
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }
    let out: String = out.chars().take(40).collect();
    if out.is_empty() {
        "jenis".into()
    } else {
        out
    }
}

fn clean_label(raw: &str) -> Result<String, AppError> {
    let label = raw.trim().to_string();
    if label.is_empty() || label.chars().count() > MAX_LABEL_CHARS {
        return Err(AppError::BadRequest(format!(
            "Nama jenis wajib diisi (maks. {MAX_LABEL_CHARS} karakter)"
        )));
    }
    Ok(label)
}

/// Slug unik per pengguna: "konstruksi", lalu "konstruksi-2", dan seterusnya.
async fn unique_slug(pool: &SqlitePool, owner_id: &str, base: &str) -> Result<String, AppError> {
    let mut candidate = base.to_string();
    let mut n = 2;
    loop {
        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM project_types WHERE owner_id = ? AND slug = ?")
                .bind(owner_id)
                .bind(&candidate)
                .fetch_one(pool)
                .await?;
        if count == 0 {
            return Ok(candidate);
        }
        candidate = format!("{base}-{n}");
        n += 1;
    }
}

async fn label_taken(pool: &SqlitePool, owner_id: &str, label: &str, except_id: &str) -> Result<bool, AppError> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM project_types WHERE owner_id = ? AND lower(label) = lower(?) AND id <> ?",
    )
    .bind(owner_id)
    .bind(label)
    .bind(except_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

async fn find_owned(pool: &SqlitePool, id: &str, owner_id: &str) -> Result<ProjectType, AppError> {
    let sql = format!("{TYPE_SELECT} WHERE t.id = ? AND t.owner_id = ?");
    sqlx::query_as::<_, ProjectType>(&sql)
        .bind(id)
        .bind(owner_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Jenis project tidak ditemukan".into()))
}

#[derive(Deserialize)]
pub struct ListQuery {
    /// Bila diisi, daftar yang dikembalikan adalah jenis milik pemilik project itu
    /// (dipakai pengedit yang bukan pemilik).
    project_id: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ProjectType>>, AppError> {
    let owner = match q.project_id {
        Some(pid) => {
            access::require(&state.pool, &pid, &user.id, Role::Viewer).await?;
            crate::projects::project_owner(&state.pool, &pid).await?
        }
        None => user.id.clone(),
    };
    let sql = format!("{TYPE_SELECT} WHERE t.owner_id = ? ORDER BY t.label COLLATE NOCASE");
    let rows = sqlx::query_as::<_, ProjectType>(&sql)
        .bind(&owner)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<LabelInput>,
) -> Result<(StatusCode, Json<ProjectType>), AppError> {
    let label = clean_label(&input.label)?;

    let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM project_types WHERE owner_id = ?")
        .bind(&user.id)
        .fetch_one(&state.pool)
        .await?;
    if total >= MAX_TYPES_PER_USER {
        return Err(AppError::BadRequest(format!(
            "Maksimal {MAX_TYPES_PER_USER} jenis project"
        )));
    }
    if label_taken(&state.pool, &user.id, &label, "").await? {
        return Err(AppError::Conflict("Jenis dengan nama ini sudah ada".into()));
    }

    let slug = unique_slug(&state.pool, &user.id, &slugify(&label)).await?;
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO project_types (id, owner_id, slug, label, created_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&id)
        .bind(&user.id)
        .bind(&slug)
        .bind(&label)
        .bind(now_iso())
        .execute(&state.pool)
        .await?;

    Ok((StatusCode::CREATED, Json(find_owned(&state.pool, &id, &user.id).await?)))
}

/// Mengubah nama tampilan saja. Slug tetap, jadi project yang memakai jenis ini tidak terpengaruh.
pub async fn update(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(input): Json<LabelInput>,
) -> Result<Json<ProjectType>, AppError> {
    find_owned(&state.pool, &id, &user.id).await?;
    let label = clean_label(&input.label)?;
    if label_taken(&state.pool, &user.id, &label, &id).await? {
        return Err(AppError::Conflict("Jenis dengan nama ini sudah ada".into()));
    }

    sqlx::query("UPDATE project_types SET label = ? WHERE id = ? AND owner_id = ?")
        .bind(&label)
        .bind(&id)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;

    Ok(Json(find_owned(&state.pool, &id, &user.id).await?))
}

/// Jenis yang masih dipakai project tidak bisa dihapus, supaya tidak ada project dengan jenis yatim.
pub async fn delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let existing = find_owned(&state.pool, &id, &user.id).await?;
    if existing.project_count > 0 {
        return Err(AppError::Conflict(format!(
            "Jenis ini masih dipakai {} project. Pindahkan project tersebut ke jenis lain dulu.",
            existing.project_count
        )));
    }

    sqlx::query("DELETE FROM project_types WHERE id = ? AND owner_id = ?")
        .bind(&id)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
