use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;

use crate::{
    auth::{self, AuthUser, User},
    error::AppError,
    AppState,
};

#[derive(Deserialize)]
pub struct ProfileInput {
    name: String,
    email: String,
}

#[derive(Deserialize)]
pub struct PasswordInput {
    current_password: String,
    new_password: String,
}

/// Mengubah nama dan email. Email harus unik di seluruh akun.
pub async fn update(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Json(input): Json<ProfileInput>,
) -> Result<Json<User>, AppError> {
    let name = input.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::BadRequest("Nama wajib diisi (maks. 100 karakter)".into()));
    }
    let email = input.email.trim().to_lowercase();
    if !auth::valid_email(&email) {
        return Err(AppError::BadRequest("Format email tidak valid".into()));
    }

    let taken: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE email = ? AND id <> ?")
            .bind(&email)
            .bind(&user.id)
            .fetch_optional(&state.pool)
            .await?;
    if taken.is_some() {
        return Err(AppError::Conflict("Email sudah dipakai akun lain".into()));
    }

    sqlx::query("UPDATE users SET name = ?, email = ? WHERE id = ?")
        .bind(&name)
        .bind(&email)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;

    let updated = sqlx::query_as::<_, User>("SELECT id, name, email, created_at FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(updated))
}

/// Mengganti password. Sesi lain milik akun ini dicabut; sesi yang dipakai sekarang tetap aktif.
pub async fn change_password(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    headers: HeaderMap,
    Json(input): Json<PasswordInput>,
) -> Result<StatusCode, AppError> {
    if input.new_password.chars().count() < 8 {
        return Err(AppError::BadRequest("Password baru minimal 8 karakter".into()));
    }

    let (hash,): (String,) = sqlx::query_as("SELECT password_hash FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&state.pool)
        .await?;
    if !auth::verify(&input.current_password, &hash) {
        return Err(AppError::BadRequest("Password saat ini salah".into()));
    }

    let new_hash = auth::make_hash(&input.new_password)?;
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(&new_hash)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;

    if let Some(token) = auth::bearer(&headers) {
        sqlx::query("DELETE FROM sessions WHERE user_id = ? AND token_hash <> ?")
            .bind(&user.id)
            .bind(auth::hash_token(token))
            .execute(&state.pool)
            .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}
