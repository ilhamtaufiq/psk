use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{auth::AuthUser, error::AppError, now_iso, AppState};

const PAGE_SIZE: i64 = 20;
const RETENTION_DAYS: i64 = 90;

#[derive(Serialize, FromRow)]
pub struct Notification {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub link: String,
    pub created_at: String,
    pub read: bool,
}

#[derive(Serialize)]
pub struct NotificationList {
    pub unread: i64,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub items: Vec<Notification>,
}

#[derive(serde::Deserialize)]
pub struct ListQuery {
    page: Option<i64>,
}

#[derive(Serialize)]
pub struct OpenedLink {
    pub link: String,
}

/// Menyimpan notifikasi untuk tiap penerima, kecuali pelaku perubahan. Kesalahan tidak menggagalkan aksi utama.
pub async fn add(
    pool: &SqlitePool,
    recipients: &[String],
    actor: &str,
    kind: &str,
    title: &str,
    body: &str,
    link: &str,
) {
    let now = now_iso();
    let mut seen: Vec<&String> = Vec::new();
    for uid in recipients {
        if uid == actor || seen.contains(&uid) {
            continue;
        }
        seen.push(uid);
        let result = sqlx::query(
            "INSERT INTO notifications (id, user_id, kind, title, body, link, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(uid)
        .bind(kind)
        .bind(title)
        .bind(body)
        .bind(link)
        .bind(&now)
        .execute(pool)
        .await;
        if let Err(e) = result {
            tracing::warn!("gagal menyimpan notifikasi: {e}");
        }
    }
}

/// Menyimpan notifikasi berkelompok: bila sudah ada untuk `ref_key` ini, isinya diperbarui dan
/// ditandai belum dibaca lagi; bila belum ada, dibuat baru.
pub async fn upsert_grouped(
    pool: &SqlitePool,
    user_id: &str,
    kind: &str,
    ref_key: &str,
    title: &str,
    body: &str,
    link: &str,
) {
    let now = now_iso();
    let updated = sqlx::query(
        "UPDATE notifications SET title = ?, body = ?, link = ?, created_at = ?, read_at = NULL
         WHERE user_id = ? AND ref_key = ?",
    )
    .bind(title)
    .bind(body)
    .bind(link)
    .bind(&now)
    .bind(user_id)
    .bind(ref_key)
    .execute(pool)
    .await;
    match updated {
        Ok(r) if r.rows_affected() > 0 => {}
        Ok(_) => {
            let inserted = sqlx::query(
                "INSERT INTO notifications (id, user_id, kind, title, body, link, created_at, ref_key)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(user_id)
            .bind(kind)
            .bind(title)
            .bind(body)
            .bind(link)
            .bind(&now)
            .bind(ref_key)
            .execute(pool)
            .await;
            if let Err(e) = inserted {
                tracing::warn!("gagal menyimpan notifikasi berkelompok: {e}");
            }
        }
        Err(e) => tracing::warn!("gagal memperbarui notifikasi: {e}"),
    }
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> Result<Json<NotificationList>, AppError> {
    // Notifikasi yang sudah dibaca dan lebih lama dari batas simpan dihapus saat daftar dibuka.
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(RETENTION_DAYS)).to_rfc3339();
    sqlx::query("DELETE FROM notifications WHERE user_id = ? AND read_at IS NOT NULL AND read_at < ?")
        .bind(&user.id)
        .bind(&cutoff)
        .execute(&state.pool)
        .await?;

    let page = q.page.unwrap_or(1).max(1);
    let items = sqlx::query_as::<_, Notification>(
        "SELECT id, kind, title, body, link, created_at, (read_at IS NOT NULL) AS read
         FROM notifications WHERE user_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?",
    )
    .bind(&user.id)
    .bind(PAGE_SIZE)
    .bind((page - 1) * PAGE_SIZE)
    .fetch_all(&state.pool)
    .await?;
    let (total,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM notifications WHERE user_id = ?")
        .bind(&user.id)
        .fetch_one(&state.pool)
        .await?;
    let (unread,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM notifications WHERE user_id = ? AND read_at IS NULL")
            .bind(&user.id)
            .fetch_one(&state.pool)
            .await?;
    Ok(Json(NotificationList { unread, total, page, page_size: PAGE_SIZE, items }))
}

/// Menandai satu notifikasi sudah dibaca dan mengembalikan tujuannya.
pub async fn read(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<Json<OpenedLink>, AppError> {
    let link: Option<(String,)> = sqlx::query_as("SELECT link FROM notifications WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user.id)
        .fetch_optional(&state.pool)
        .await?;
    let (link,) = link.ok_or_else(|| AppError::NotFound("Notifikasi tidak ditemukan".into()))?;
    sqlx::query("UPDATE notifications SET read_at = COALESCE(read_at, ?) WHERE id = ? AND user_id = ?")
        .bind(now_iso())
        .bind(&id)
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    Ok(Json(OpenedLink { link }))
}

pub async fn read_all(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<StatusCode, AppError> {
    sqlx::query("UPDATE notifications SET read_at = ? WHERE user_id = ? AND read_at IS NULL")
        .bind(now_iso())
        .bind(&user.id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
