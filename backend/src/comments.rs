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
    error::AppError,
    files::{content_disposition, sanitize_filename},
    now_iso, AppState,
};

pub const MAX_COMMENT_CHARS: usize = 2000;
pub const MAX_ATTACHMENTS: usize = 5;
/// Kedalaman maksimum balasan (komentar utama = 1).
pub const MAX_DEPTH: i64 = 8;
pub const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;
const LIST_LIMIT: i64 = 200;

#[derive(FromRow)]
struct CommentRow {
    id: String,
    parent_id: Option<String>,
    user_id: String,
    author_name: String,
    body: String,
    created_at: String,
    deleted_at: Option<String>,
    score: i64,
    my_vote: Option<i64>,
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
    pub parent_id: Option<String>,
    pub author_name: String,
    pub body: String,
    pub created_at: String,
    /// Benar bila pengguna yang melihat adalah penulis, pemilik, atau editor.
    pub can_delete: bool,
    pub attachments: Vec<AttachmentView>,
    /// Jumlah suara naik dikurangi suara turun.
    pub score: i64,
    /// Suara pengguna yang melihat: 1, -1, atau 0 bila belum memilih.
    pub my_vote: i64,
    /// Komentar yang dihapus tetapi masih dipertahankan karena punya balasan.
    pub deleted: bool,
}

#[derive(Deserialize)]
pub struct VoteInput {
    value: i64,
}

/// Jumlah tingkat dari komentar sampai komentar utama (komentar utama = 1).
async fn depth_of(pool: &sqlx::SqlitePool, id: &str) -> Result<i64, AppError> {
    let mut depth = 0;
    let mut current = Some(id.to_string());
    while let Some(cur) = current {
        depth += 1;
        if depth > MAX_DEPTH + 1 {
            break;
        }
        let parent: Option<(Option<String>,)> =
            sqlx::query_as("SELECT parent_id FROM project_comments WHERE id = ?")
                .bind(&cur)
                .fetch_optional(pool)
                .await?;
        current = parent.and_then(|(p,)| p);
    }
    Ok(depth)
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
        "SELECT c.id, c.parent_id, c.user_id, u.name AS author_name, c.body, c.created_at, c.deleted_at,
            (SELECT COALESCE(SUM(v.value), 0) FROM comment_votes v WHERE v.comment_id = c.id) AS score,
            (SELECT v.value FROM comment_votes v WHERE v.comment_id = c.id AND v.user_id = ?) AS my_vote
         FROM project_comments c JOIN users u ON u.id = c.user_id
         WHERE c.project_id = ?
         ORDER BY c.created_at DESC
         LIMIT ?",
    )
    .bind(&user.id)
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
            .map(|r| {
                let deleted = r.deleted_at.is_some();
                CommentView {
                    can_delete: !deleted && (mod_ok || r.user_id == user.id),
                    attachments: if deleted { Vec::new() } else { by_comment.remove(&r.id).unwrap_or_default() },
                    author_name: if deleted { "[dihapus]".into() } else { r.author_name },
                    body: if deleted { String::new() } else { r.body },
                    parent_id: r.parent_id,
                    score: r.score,
                    my_vote: r.my_vote.unwrap_or(0),
                    deleted,
                    id: r.id,
                    created_at: r.created_at,
                }
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
    let mut parent_id: Option<String> = None;
    let mut files: Vec<PendingFile> = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|_| bad_form())? {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "body" => body = field.text().await.map_err(|_| bad_form())?,
            "parent_id" => {
                let value = field.text().await.map_err(|_| bad_form())?;
                let value = value.trim().to_string();
                if !value.is_empty() {
                    parent_id = Some(value);
                }
            }
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

    if let Some(pid) = &parent_id {
        let parent: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT project_id, deleted_at FROM project_comments WHERE id = ?")
                .bind(pid)
                .fetch_optional(&state.pool)
                .await?;
        match parent {
            Some((pp, _)) if pp != project_id => {
                return Err(AppError::BadRequest("Komentar yang dibalas tidak ditemukan".into()))
            }
            None => return Err(AppError::BadRequest("Komentar yang dibalas tidak ditemukan".into())),
            Some((_, Some(_))) => return Err(AppError::BadRequest("Komentar ini sudah dihapus".into())),
            Some(_) => {}
        }
        if depth_of(&state.pool, pid).await? >= MAX_DEPTH {
            return Err(AppError::BadRequest("Balasan sudah mencapai batas kedalaman".into()));
        }
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
            "INSERT INTO project_comments (id, project_id, user_id, body, created_at, parent_id) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&comment_id)
        .bind(&project_id)
        .bind(&user.id)
        .bind(&body)
        .bind(&now)
        .bind(&parent_id)
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
    if parent_id.is_none() {
        let staff: Vec<String> = sqlx::query_scalar(
            "SELECT owner_id FROM projects WHERE id = ?
             UNION SELECT user_id FROM project_members WHERE project_id = ? AND role = 'editor'",
        )
        .bind(&project_id)
        .bind(&project_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();
        let link = format!("/projects/{project_id}#c-{comment_id}");
        let preview: String = body.chars().take(140).collect();
        crate::notifications::add(
            &state.pool,
            &staff,
            &user.id,
            "comment",
            &format!("Komentar baru dari {}", user.name),
            &preview,
            &link,
        )
        .await;
    }
    if let Some(pid) = &parent_id {
        let parent_author: Option<(String,)> =
            sqlx::query_as("SELECT user_id FROM project_comments WHERE id = ?")
                .bind(pid)
                .fetch_optional(&state.pool)
                .await?;
        if let Some((author,)) = parent_author {
            let link = format!("/projects/{project_id}#c-{comment_id}");
            crate::notifications::add(
                &state.pool,
                &[author],
                &user.id,
                "reply",
                &format!("{} membalas komentar Anda", user.name),
                &body.chars().take(140).collect::<String>(),
                &link,
            )
            .await;
        }
    }
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
            parent_id,
            author_name: user.name.clone(),
            body,
            created_at: now,
            can_delete: true,
            attachments,
            score: 0,
            my_vote: 0,
            deleted: false,
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

    let (replies,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM project_comments WHERE parent_id = ?")
        .bind(&id)
        .fetch_one(&state.pool)
        .await?;
    if replies > 0 {
        // Punya balasan: thread dipertahankan sebagai tombstone, isi dan lampiran dibuang.
        sqlx::query("UPDATE project_comments SET deleted_at = ?, body = '' WHERE id = ?")
            .bind(now_iso())
            .bind(&id)
            .execute(&state.pool)
            .await?;
        sqlx::query("DELETE FROM comment_attachments WHERE comment_id = ?")
            .bind(&id)
            .execute(&state.pool)
            .await?;
    } else {
        // Baris lampiran dan vote ikut terhapus lewat ON DELETE CASCADE.
        sqlx::query("DELETE FROM project_comments WHERE id = ?")
            .bind(&id)
            .execute(&state.pool)
            .await?;
    }
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

/// Memberi, mengubah, atau mencabut suara. Nilai 1 (naik), -1 (turun), atau 0 (cabut).
pub async fn vote(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Json(input): Json<VoteInput>,
) -> Result<StatusCode, AppError> {
    if !(-1..=1).contains(&input.value) {
        return Err(AppError::BadRequest("Nilai suara tidak valid".into()));
    }
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT project_id, deleted_at FROM project_comments WHERE id = ?")
            .bind(&id)
            .fetch_optional(&state.pool)
            .await?;
    let (project_id, deleted) =
        row.ok_or_else(|| AppError::NotFound("Komentar tidak ditemukan".into()))?;
    access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;
    if deleted.is_some() {
        return Err(AppError::BadRequest("Komentar sudah dihapus".into()));
    }

    if input.value == 0 {
        sqlx::query("DELETE FROM comment_votes WHERE comment_id = ? AND user_id = ?")
            .bind(&id)
            .bind(&user.id)
            .execute(&state.pool)
            .await?;
    } else {
        sqlx::query(
            "INSERT INTO comment_votes (comment_id, user_id, value) VALUES (?, ?, ?)
             ON CONFLICT (comment_id, user_id) DO UPDATE SET value = excluded.value",
        )
        .bind(&id)
        .bind(&user.id)
        .bind(input.value)
        .execute(&state.pool)
        .await?;
    }
    crate::events::publish(&state, &project_id, "comment");
    crate::events::publish(&state, &project_id, "comment");
    if input.value == 1 {
        let author: Option<(String,)> =
            sqlx::query_as("SELECT user_id FROM project_comments WHERE id = ?")
                .bind(&id)
                .fetch_optional(&state.pool)
                .await?;
        if let Some((author,)) = author {
            let (likes,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM comment_votes WHERE comment_id = ? AND value = 1 AND user_id <> ?",
            )
            .bind(&id)
            .bind(&author)
            .fetch_one(&state.pool)
            .await?;
            let title = if likes == 1 {
                "1 orang menyukai komentar Anda".to_string()
            } else {
                format!("{likes} orang menyukai komentar Anda")
            };
            let link = format!("/projects/{project_id}#c-{id}");
            crate::notifications::upsert_grouped(
                &state.pool,
                &author,
                "vote",
                &format!("vote:{id}"),
                &title,
                "",
                &link,
            )
            .await;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}
