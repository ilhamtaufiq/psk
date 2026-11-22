use std::collections::HashMap;

use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    access::{self, Role},
    auth::AuthUser,
    error::AppError,
    files::{content_disposition, sanitize_filename},
    now_iso, AppState,
};

pub const MAX_COMMENT_CHARS: usize = 2000;
pub const MAX_ATTACHMENTS: usize = 5;
pub const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;
const LIST_LIMIT: i64 = 200;

#[derive(FromRow)]
struct CommentRow {
    id: String,
    user_id: String,
    author_name: String,
    body: String,
    created_at: String,
}

#[derive(FromRow)]
struct AttachmentRow {
    id: String,
    comment_id: String,
    original_name: String,
    mime_type: String,
    size_bytes: i64,
}

#[derive(Serialize)]
pub struct AttachmentView {
    pub id: String,
    pub original_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
}

#[derive(Serialize)]
pub struct CommentView {
    pub id: String,
    pub author_name: String,
    pub body: String,
    pub created_at: String,
    /// Benar bila pengguna yang melihat adalah penulis, pemilik, atau editor.
    pub can_delete: bool,
    pub attachments: Vec<AttachmentView>,
}

fn can_moderate(role: &Role) -> bool {
    matches!(role, Role::Owner | Role::Editor)
}

fn comment_dir(state: &AppState, project_id: &str, comment_id: &str) -> std::path::PathBuf {
    state.upload_dir.join(project_id).join("comments").join(comment_id)
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

    let attachment_rows = sqlx::query_as::<_, AttachmentRow>(
        "SELECT a.id, a.comment_id, a.original_name, a.mime_type, a.size_bytes
         FROM comment_attachments a
         WHERE a.project_id = ?
         ORDER BY a.created_at",
    )
    .bind(&project_id)
    .fetch_all(&state.pool)
    .await?;
    let mut by_comment: HashMap<String, Vec<AttachmentView>> = HashMap::new();
    for a in attachment_rows {
        by_comment.entry(a.comment_id).or_default().push(AttachmentView {
            id: a.id,
            original_name: a.original_name,
            mime_type: a.mime_type,
            size_bytes: a.size_bytes,
        });
    }

    let mod_ok = can_moderate(&my_role);
    Ok(Json(
        rows.into_iter()
            .map(|r| CommentView {
                can_delete: mod_ok || r.user_id == user.id,
                attachments: by_comment.remove(&r.id).unwrap_or_default(),
                id: r.id,
                author_name: r.author_name,
                body: r.body,
                created_at: r.created_at,
            })
            .collect(),
    ))
}

struct PendingFile {
    name: String,
    mime: String,
    data: Vec<u8>,
}

/// Membuat komentar dari formulir multipart: field `body` (teks, boleh kosong bila ada lampiran)
/// dan field `attachment` (satu atau lebih berkas, termasuk gambar yang ditempel).
pub async fn create(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<CommentView>), AppError> {
    access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;

    let bad_form = || AppError::BadRequest("Formulir komentar tidak valid".into());
    let mut body = String::new();
    let mut files: Vec<PendingFile> = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|_| bad_form())? {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "body" => body = field.text().await.map_err(|_| bad_form())?,
            "attachment" => {
                let name = sanitize_filename(field.file_name().unwrap_or("lampiran"));
                let declared = field.content_type().unwrap_or("").to_string();
                let data = field.bytes().await.map_err(|_| bad_form())?.to_vec();
                if data.is_empty() {
                    continue; // input kosong dari formulir tidak dihitung sebagai lampiran
                }
                if data.len() > MAX_ATTACHMENT_BYTES {
                    return Err(AppError::BadRequest("Lampiran maksimal 10 MB per berkas".into()));
                }
                if files.len() >= MAX_ATTACHMENTS {
                    return Err(AppError::BadRequest(format!(
                        "Maksimal {MAX_ATTACHMENTS} lampiran per komentar"
                    )));
                }
                let mime = if declared.is_empty() || declared == "application/octet-stream" {
                    mime_guess::from_path(&name).first_or_octet_stream().to_string()
                } else {
                    declared
                };
                files.push(PendingFile { name, mime, data });
            }
            _ => {}
        }
    }

    let body = body.trim().to_string();
    if body.chars().count() > MAX_COMMENT_CHARS {
        return Err(AppError::BadRequest(format!(
            "Komentar maksimal {MAX_COMMENT_CHARS} karakter"
        )));
    }
    if body.is_empty() && files.is_empty() {
        return Err(AppError::BadRequest("Tulis komentar atau lampirkan berkas".into()));
    }

    let comment_id = Uuid::new_v4().to_string();
    let now = now_iso();
    let dir = comment_dir(&state, &project_id, &comment_id);
    let mut stored: Vec<(String, PendingFile)> = Vec::with_capacity(files.len());
    tokio::fs::create_dir_all(&dir).await?;
    for file in files {
        let att_id = Uuid::new_v4().to_string();
        if let Err(e) = tokio::fs::write(dir.join(&att_id), &file.data).await {
            let _ = tokio::fs::remove_dir_all(&dir).await;
            return Err(e.into());
        }
        stored.push((att_id, file));
    }

    let result: Result<(), AppError> = async {
        let mut tx = state.pool.begin().await?;
        sqlx::query(
            "INSERT INTO project_comments (id, project_id, user_id, body, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&comment_id)
        .bind(&project_id)
        .bind(&user.id)
        .bind(&body)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        for (att_id, f) in &stored {
            sqlx::query(
                "INSERT INTO comment_attachments (id, comment_id, project_id, original_name, mime_type, size_bytes, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(att_id)
            .bind(&comment_id)
            .bind(&project_id)
            .bind(&f.name)
            .bind(&f.mime)
            .bind(f.data.len() as i64)
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

    crate::events::publish(&state, &project_id, "comment");
    let attachments = stored
        .iter()
        .map(|(att_id, f)| AttachmentView {
            id: att_id.clone(),
            original_name: f.name.clone(),
            mime_type: f.mime.clone(),
            size_bytes: f.data.len() as i64,
        })
        .collect();
    Ok((
        StatusCode::CREATED,
        Json(CommentView {
            id: comment_id,
            author_name: user.name.clone(),
            body,
            created_at: now,
            can_delete: true,
            attachments,
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

    // Baris lampiran ikut terhapus lewat ON DELETE CASCADE; berkasnya dihapus dari disk.
    sqlx::query("DELETE FROM project_comments WHERE id = ?")
        .bind(&id)
        .execute(&state.pool)
        .await?;
    let _ = tokio::fs::remove_dir_all(comment_dir(&state, &project_id, &id)).await;
    crate::events::publish(&state, &project_id, "comment");
    Ok(StatusCode::NO_CONTENT)
}

/// Unduhan lampiran. Selalu sebagai lampiran (bukan dirender di halaman), dengan CSP sandbox.
pub async fn download(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT project_id, comment_id, original_name, mime_type FROM comment_attachments WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await?;
    let (project_id, comment_id, name, mime) =
        row.ok_or_else(|| AppError::NotFound("Lampiran tidak ditemukan".into()))?;

    access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;
    let path = comment_dir(&state, &project_id, &comment_id).join(&id);
    let data = tokio::fs::read(&path)
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
