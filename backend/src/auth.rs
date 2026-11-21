use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand_core::OsRng;
use axum::{
    extract::{FromRef, FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts, HeaderMap, StatusCode},
    Json,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{error::AppError, now_iso, AppState};

const SESSION_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: String,
    pub created_at: String,
}

#[derive(FromRow)]
struct UserRecord {
    id: String,
    name: String,
    email: String,
    created_at: String,
    password_hash: String,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    name: String,
    email: String,
    password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    token: String,
    user: User,
}

/// Pengguna yang sudah terautentikasi dari header `Authorization: Bearer <token>`.
pub struct AuthUser(pub User);

impl<S> FromRequestParts<S> for AuthUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app = AppState::from_ref(state);
        let token = bearer(&parts.headers)
            .ok_or_else(|| AppError::Unauthorized("Silakan masuk terlebih dahulu".into()))?;

        let user = sqlx::query_as::<_, User>(
            "SELECT u.id, u.name, u.email, u.created_at
             FROM sessions s JOIN users u ON u.id = s.user_id
             WHERE s.token_hash = ? AND s.expires_at > ?",
        )
        .bind(hash_token(token))
        .bind(now_iso())
        .fetch_optional(&app.pool)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Sesi sudah berakhir, silakan masuk ulang".into()))?;

        Ok(AuthUser(user))
    }
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .filter(|t| !t.is_empty())
}

fn make_hash(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(e.to_string()))
}

fn verify(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

fn new_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn valid_email(email: &str) -> bool {
    if email.len() > 255 {
        return false;
    }
    match email.split_once('@') {
        Some((local, domain)) => !local.is_empty() && domain.contains('.') && !domain.ends_with('.'),
        None => false,
    }
}

async fn create_session(state: &AppState, user_id: &str) -> Result<String, AppError> {
    let token = new_token();
    let now = Utc::now();
    let expires = now + Duration::days(SESSION_DAYS);
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(hash_token(&token))
    .bind(user_id)
    .bind(expires.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
    .bind(now_iso())
    .execute(&state.pool)
    .await?;
    Ok(token)
}

pub async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), AppError> {
    let name = body.name.trim().to_string();
    let email = body.email.trim().to_lowercase();

    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::BadRequest("Nama wajib diisi (maks. 100 karakter)".into()));
    }
    if !valid_email(&email) {
        return Err(AppError::BadRequest("Format email tidak valid".into()));
    }
    if body.password.len() < 8 {
        return Err(AppError::BadRequest("Password minimal 8 karakter".into()));
    }

    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_some() {
        return Err(AppError::Conflict("Email sudah terdaftar".into()));
    }

    let id = Uuid::new_v4().to_string();
    let hash = make_hash(&body.password)?;
    let now = now_iso();
    sqlx::query(
        "INSERT INTO users (id, name, email, password_hash, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&name)
    .bind(&email)
    .bind(&hash)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    let token = create_session(&state, &id).await?;
    let user = User {
        id,
        name,
        email,
        created_at: now,
    };
    Ok((StatusCode::CREATED, Json(AuthResponse { token, user })))
}

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let email = body.email.trim().to_lowercase();
    let invalid = || AppError::Unauthorized("Email atau password salah".into());

    let record = sqlx::query_as::<_, UserRecord>(
        "SELECT id, name, email, created_at, password_hash FROM users WHERE email = ?",
    )
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(invalid)?;

    if !verify(&body.password, &record.password_hash) {
        return Err(invalid());
    }

    let user = User {
        id: record.id,
        name: record.name,
        email: record.email,
        created_at: record.created_at,
    };
    let token = create_session(&state, &user.id).await?;
    Ok(Json(AuthResponse { token, user }))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, AppError> {
    if let Some(token) = bearer(&headers) {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(hash_token(token))
            .execute(&state.pool)
            .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(AuthUser(user): AuthUser) -> Json<User> {
    Json(user)
}
