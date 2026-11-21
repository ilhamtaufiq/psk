use axum::{
    extract::{Path, State},
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
    now_iso,
    projects::fetch_project,
    AppState,
};

const MAX_REQUIREMENTS_PER_PROJECT: i64 = 200;

#[derive(Debug, Serialize, FromRow)]
pub struct Requirement {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub created_at: String,
    pub file_count: i64,
}

const REQUIREMENT_SELECT: &str = "SELECT r.id, r.project_id, r.title, r.created_at,
    (SELECT COUNT(*) FROM project_files f WHERE f.requirement_id = r.id) AS file_count
    FROM project_requirements r";

#[derive(Deserialize)]
pub struct TitleInput {
    title: String,
}

fn clean_title(raw: &str) -> Result<String, AppError> {
    let title = raw.trim().to_string();
    if title.is_empty() || title.chars().count() > 150 {
        return Err(AppError::BadRequest(
            "Nama berkas wajib diisi (maks. 150 karakter)".into(),
        ));
    }
    Ok(title)
}

/// Mengambil item daftar lalu memeriksa peran pengguna di proyeknya.
async fn find_owned_requirement(
    pool: &SqlitePool,
    id: &str,
    user_id: &str,
    min: Role,
) -> Result<Requirement, AppError> {
    let sql = format!("{REQUIREMENT_SELECT} WHERE r.id = ?");
    let req = sqlx::query_as::<_, Requirement>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Item daftar berkas tidak ditemukan".into()))?;
    access::require(pool, &req.project_id, user_id, min).await?;
    Ok(req)
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<Requirement>>, AppError> {
    fetch_project(&state.pool, &project_id, &user.id).await?;
    let sql = format!("{REQUIREMENT_SELECT} WHERE r.project_id = ? ORDER BY r.created_at ASC");
    let rows = sqlx::query_as::<_, Requirement>(&sql)
        .bind(&project_id)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
    Json(input): Json<TitleInput>,
) -> Result<(StatusCode, Json<Requirement>), AppError> {
    access::require(&state.pool, &project_id, &user.id, Role::Editor).await?;
    let title = clean_title(&input.title)?;

    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM project_requirements WHERE project_id = ?")
            .bind(&project_id)
            .fetch_one(&state.pool)
            .await?;
    if count >= MAX_REQUIREMENTS_PER_PROJECT {
        return Err(AppError::BadRequest(format!(
            "Maksimal {MAX_REQUIREMENTS_PER_PROJECT} item per proyek"
        )));
    }

    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO project_requirements (id, project_id, title, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&title)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    let created = find_owned_requirement(&state.pool, &id, &user.id, Role::Viewer).await?;
    crate::events::publish(&state, &project_id, "requirements");
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(input): Json<TitleInput>,
) -> Result<Json<Requirement>, AppError> {
    find_owned_requirement(&state.pool, &id, &user.id, Role::Editor).await?;
    let title = clean_title(&input.title)?;

    sqlx::query("UPDATE project_requirements SET title = ?, updated_at = ? WHERE id = ?")
        .bind(&title)
        .bind(now_iso())
        .bind(&id)
        .execute(&state.pool)
        .await?;

    let updated = find_owned_requirement(&state.pool, &id, &user.id, Role::Viewer).await?;
    crate::events::publish(&state, &updated.project_id, "requirements");
    Ok(Json(updated))
}

/// Menghapus item daftar. Berkas yang sudah terunggah tetap ada (kaitannya di-NULL-kan).
pub async fn delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let removed = find_owned_requirement(&state.pool, &id, &user.id, Role::Editor).await?;
    sqlx::query("DELETE FROM project_requirements WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    crate::events::publish(&state, &removed.project_id, "requirements");
    Ok(StatusCode::NO_CONTENT)
}
