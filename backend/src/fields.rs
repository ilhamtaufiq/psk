use axum::{
    extract::{Path, State},
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
    now_iso,
    projects::fetch_project,
    AppState,
};

pub const FIELD_TYPES: [&str; 7] = ["text", "multiline", "date", "number", "email", "phone", "url"];
const MAX_FIELDS_PER_PROJECT: i64 = 50;

#[derive(Debug, Serialize, FromRow)]
pub struct ProjectField {
    pub id: String,
    pub project_id: String,
    pub label: String,
    pub field_type: String,
    pub value: String,
    pub position: i64,
}

const FIELD_SELECT: &str = "SELECT f.id, f.project_id, f.label, f.field_type, f.value, f.position
    FROM project_fields f";

#[derive(Deserialize)]
pub struct FieldInput {
    label: String,
    field_type: Option<String>,
    value: Option<String>,
}

#[derive(Deserialize)]
pub struct MoveInput {
    direction: String,
}

/// Memeriksa dan merapikan input. Nilai kosong diperbolehkan (field bisa diisi nanti).
fn validate(input: FieldInput) -> Result<(String, String, String), AppError> {
    let label = input.label.trim().to_string();
    if label.is_empty() || label.chars().count() > 100 {
        return Err(AppError::BadRequest(
            "Nama informasi wajib diisi (maks. 100 karakter)".into(),
        ));
    }

    let field_type = input.field_type.unwrap_or_else(|| "text".into());
    if !FIELD_TYPES.contains(&field_type.as_str()) {
        return Err(AppError::BadRequest("Jenis informasi tidak dikenal".into()));
    }

    let value = input.value.unwrap_or_default().trim().to_string();
    let max = if field_type == "multiline" { 5000 } else { 500 };
    if value.chars().count() > max {
        return Err(AppError::BadRequest(format!(
            "Isi informasi maksimal {max} karakter"
        )));
    }
    if !value.is_empty() {
        check_value(&field_type, &value)?;
    }

    Ok((label, field_type, value))
}

/// Validasi format sesuai jenis. Hanya format dasar; bukan verifikasi keberadaan.
fn check_value(field_type: &str, value: &str) -> Result<(), AppError> {
    let bad = |msg: &str| Err(AppError::BadRequest(msg.into()));
    match field_type {
        "date" => {
            if NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err() {
                return bad("Tanggal harus berformat YYYY-MM-DD");
            }
        }
        "number" => match value.replace(',', ".").parse::<f64>() {
            Ok(n) if n.is_finite() => {}
            _ => return bad("Angka tidak valid"),
        },
        "email" => {
            let ok = match value.split_once('@') {
                Some((local, domain)) => !local.is_empty() && domain.contains('.') && !value.contains(' '),
                None => false,
            };
            if !ok {
                return bad("Format email tidak valid");
            }
        }
        "phone" => {
            let allowed = value.chars().all(|c| c.is_ascii_digit() || " +-()".contains(c));
            let digits = value.chars().filter(|c| c.is_ascii_digit()).count();
            if !allowed || !(6..=20).contains(&digits) {
                return bad("Nomor telepon tidak valid");
            }
        }
        "url" => {
            if !(value.starts_with("http://") || value.starts_with("https://")) || value.contains(' ') {
                return bad("Tautan harus diawali http:// atau https://");
            }
        }
        _ => {}
    }
    Ok(())
}

/// Mengambil field lalu memeriksa peran pengguna di projectnya.
async fn find_owned_field(pool: &SqlitePool, id: &str, user_id: &str, min: Role) -> Result<ProjectField, AppError> {
    let sql = format!("{FIELD_SELECT} WHERE f.id = ?");
    let field = sqlx::query_as::<_, ProjectField>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Informasi tidak ditemukan".into()))?;
    access::require(pool, &field.project_id, user_id, min).await?;
    Ok(field)
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ProjectField>>, AppError> {
    fetch_project(&state.pool, &project_id, &user.id).await?;
    let sql = format!("{FIELD_SELECT} WHERE f.project_id = ? ORDER BY f.position ASC");
    let rows = sqlx::query_as::<_, ProjectField>(&sql)
        .bind(&project_id)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(rows))
}

pub async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
    Json(input): Json<FieldInput>,
) -> Result<(StatusCode, Json<ProjectField>), AppError> {
    access::require(&state.pool, &project_id, &user.id, Role::Editor).await?;
    let (label, field_type, value) = validate(input)?;

    let (count, max_pos): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(MAX(position), 0) FROM project_fields WHERE project_id = ?",
    )
    .bind(&project_id)
    .fetch_one(&state.pool)
    .await?;
    if count >= MAX_FIELDS_PER_PROJECT {
        return Err(AppError::BadRequest(format!(
            "Maksimal {MAX_FIELDS_PER_PROJECT} informasi per project"
        )));
    }

    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO project_fields (id, project_id, label, field_type, value, position, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&label)
    .bind(&field_type)
    .bind(&value)
    .bind(max_pos + 1)
    .bind(&now)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    let sql = format!("{FIELD_SELECT} WHERE f.id = ?");
    let created = sqlx::query_as::<_, ProjectField>(&sql)
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    crate::events::publish(&state, &project_id, "fields");
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(input): Json<FieldInput>,
) -> Result<Json<ProjectField>, AppError> {
    find_owned_field(&state.pool, &id, &user.id, Role::Editor).await?;
    let (label, field_type, value) = validate(input)?;

    sqlx::query("UPDATE project_fields SET label = ?, field_type = ?, value = ?, updated_at = ? WHERE id = ?")
        .bind(&label)
        .bind(&field_type)
        .bind(&value)
        .bind(now_iso())
        .bind(&id)
        .execute(&state.pool)
        .await?;

    let updated = find_owned_field(&state.pool, &id, &user.id, Role::Viewer).await?;
    crate::events::publish(&state, &updated.project_id, "fields");
    Ok(Json(updated))
}

pub async fn delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let removed = find_owned_field(&state.pool, &id, &user.id, Role::Editor).await?;
    sqlx::query("DELETE FROM project_fields WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    crate::events::publish(&state, &removed.project_id, "fields");
    Ok(StatusCode::NO_CONTENT)
}

/// Menukar urutan dengan tetangga atas/bawah. Posisi tetap unik karena selalu ditukar berpasangan.
pub async fn move_field(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(input): Json<MoveInput>,
) -> Result<StatusCode, AppError> {
    let field = find_owned_field(&state.pool, &id, &user.id, Role::Editor).await?;
    let (op, order) = match input.direction.as_str() {
        "up" => ("<", "DESC"),
        "down" => (">", "ASC"),
        _ => return Err(AppError::BadRequest("Arah harus 'up' atau 'down'".into())),
    };

    let neighbor: Option<(String, i64)> = sqlx::query_as(&format!(
        "SELECT id, position FROM project_fields WHERE project_id = ? AND position {op} ? ORDER BY position {order} LIMIT 1"
    ))
    .bind(&field.project_id)
    .bind(field.position)
    .fetch_optional(&state.pool)
    .await?;

    if let Some((neighbor_id, neighbor_pos)) = neighbor {
        let mut tx = state.pool.begin().await?;
        sqlx::query("UPDATE project_fields SET position = ? WHERE id = ?")
            .bind(neighbor_pos)
            .bind(&field.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE project_fields SET position = ? WHERE id = ?")
            .bind(field.position)
            .bind(&neighbor_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    crate::events::publish(&state, &field.project_id, "fields");
    Ok(StatusCode::NO_CONTENT)
}
