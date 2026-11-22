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

pub const MAX_COMMENT_CHARS: usize = 2000;
const LIST_LIMIT: i64 = 200;

#[derive(FromRow)]
struct CommentRow {
    id: String,
    user_id: String,
    author_name: String,
    body: String,
    created_at: String,
}

#[derive(Serialize)]
pub struct CommentView {
    pub id: String,
    pub author_name: String,
    pub body: String,
    pub created_at: String,
    /// Benar bila pengguna yang melihat adalah penulis, pemilik, atau editor.
    pub can_delete: bool,
}

#[derive(Deserialize)]
pub struct CommentInput {
    body: String,
}

fn can_moderate(role: &Role) -> bool {
    matches!(role, Role::Owner | Role::Editor)
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<CommentView>>, AppError> {
    let my_role = access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;
    let rows = sqlx::query_as::<_, CommentRow>(
        "SELECT c.id, c.user_id, u.name AS author_name, c.body, c.created_at
         FROM project_comments c JOIN users u ON u.id = c.user_id
         WHERE c.project_id = ?
         ORDER BY c.created_at DESC
         LIMIT ?",
    )
    .bind(&project_id)
    .bind(LIST_LIMIT)
    .fetch_all(&state.pool)
    .await?;

    let mod_ok = can_moderate(&my_role);
    Ok(Json(
        rows.into_iter()
            .map(|r| CommentView {
                can_delete: mod_ok || r.user_id == user.id,
                id: r.id,
                author_name: r.author_name,
                body: r.body,
                created_at: r.created_at,
            })
            .collect(),
    ))
}

pub async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
    Json(input): Json<CommentInput>,
) -> Result<(StatusCode, Json<CommentView>), AppError> {
    access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;

    let body = input.body.trim().to_string();
    if body.is_empty() {
        return Err(AppError::BadRequest("Komentar tidak boleh kosong".into()));
    }
    if body.chars().count() > MAX_COMMENT_CHARS {
        return Err(AppError::BadRequest(format!(
            "Komentar maksimal {MAX_COMMENT_CHARS} karakter"
        )));
    }

    let id = Uuid::new_v4().to_string();
    let now = now_iso();
    sqlx::query(
        "INSERT INTO project_comments (id, project_id, user_id, body, created_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&user.id)
    .bind(&body)
    .bind(&now)
    .execute(&state.pool)
    .await?;

    crate::events::publish(&state, &project_id, "comment");
    Ok((
        StatusCode::CREATED,
        Json(CommentView {
            id,
            author_name: user.name.clone(),
            body,
            created_at: now,
            can_delete: true,
        }),
    ))
}

pub async fn delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT project_id, user_id FROM project_comments WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await?;
    let (project_id, author_id) =
        row.ok_or_else(|| AppError::NotFound("Komentar tidak ditemukan".into()))?;

    let my_role = access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;
    if author_id != user.id && !can_moderate(&my_role) {
        return Err(AppError::Forbidden(
            "Hanya penulis, pemilik, atau editor yang bisa menghapus komentar".into(),
        ));
    }

    sqlx::query("DELETE FROM project_comments WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    crate::events::publish(&state, &project_id, "comment");
    Ok(StatusCode::NO_CONTENT)
}
