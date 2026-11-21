use axum::{extract::State, Json};
use chrono::{Datelike, Duration, FixedOffset, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};

use crate::{auth::AuthUser, error::AppError, projects::{PROJECT_TYPES, STATUSES}, AppState};

/// Zona waktu tampilan (WIB). Tanggal "hari ini" mengikuti zona ini.
const LOCAL_OFFSET_SECS: i32 = 7 * 3600;
const DUE_SOON_DAYS: i64 = 7;
const UPLOAD_MONTHS: u32 = 6;

#[derive(Serialize)]
pub struct Bucket {
    pub key: String,
    pub count: i64,
}

#[derive(Serialize, FromRow)]
pub struct DeadlineItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub due_date: String,
}

#[derive(Serialize, FromRow)]
pub struct RecentItem {
    pub id: String,
    pub name: String,
    pub status: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct Dashboard {
    pub today: String,
    pub total_projects: i64,
    pub by_status: Vec<Bucket>,
    pub by_type: Vec<Bucket>,
    pub overdue: i64,
    pub due_soon: Vec<DeadlineItem>,
    pub total_files: i64,
    pub total_bytes: i64,
    pub requirements_total: i64,
    pub requirements_done: i64,
    pub uploads_by_month: Vec<Bucket>,
    pub recent: Vec<RecentItem>,
}

fn local_today() -> NaiveDate {
    let offset = FixedOffset::east_opt(LOCAL_OFFSET_SECS).expect("offset valid");
    Utc::now().with_timezone(&offset).date_naive()
}

/// Enam bulan terakhir (YYYY-MM), termasuk bulan ini, diurutkan dari yang paling lama.
fn last_months(today: NaiveDate) -> Vec<String> {
    let mut keys = Vec::new();
    let mut year = today.year();
    let mut month = today.month();
    for _ in 0..UPLOAD_MONTHS {
        keys.push(format!("{year:04}-{month:02}"));
        if month == 1 {
            month = 12;
            year -= 1;
        } else {
            month -= 1;
        }
    }
    keys.reverse();
    keys
}

/// Mengisi kategori yang tidak punya data dengan 0, agar grafik selalu lengkap dan urutannya tetap.
fn fill(keys: &[&str], rows: &[(String, i64)]) -> Vec<Bucket> {
    keys.iter()
        .map(|k| Bucket {
            key: (*k).to_string(),
            count: rows.iter().find(|(rk, _)| rk == k).map(|(_, c)| *c).unwrap_or(0),
        })
        .collect()
}

async fn count_by(pool: &SqlitePool, owner: &str, column: &str) -> Result<Vec<(String, i64)>, AppError> {
    // `column` hanya berasal dari konstanta di kode, bukan dari input pengguna.
    let sql = format!("SELECT {column} AS key, COUNT(*) AS count FROM projects WHERE owner_id = ? GROUP BY {column}");
    Ok(sqlx::query_as::<_, (String, i64)>(&sql).bind(owner).fetch_all(pool).await?)
}

pub async fn summary(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<Json<Dashboard>, AppError> {
    let pool = &state.pool;
    let owner = user.id.as_str();
    let today = local_today();
    let today_s = today.to_string();
    let soon_s = (today + Duration::days(DUE_SOON_DAYS)).to_string();

    let (total_projects,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects WHERE owner_id = ?")
        .bind(owner)
        .fetch_one(pool)
        .await?;

    let status_rows = count_by(pool, owner, "status").await?;
    let type_rows = count_by(pool, owner, "project_type").await?;

    let (overdue,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM projects
         WHERE owner_id = ? AND status <> 'completed' AND due_date IS NOT NULL AND due_date < ?",
    )
    .bind(owner)
    .bind(&today_s)
    .fetch_one(pool)
    .await?;

    // Tenggat yang sudah lewat atau jatuh dalam 7 hari ke depan, paling mendesak di atas.
    let due_soon = sqlx::query_as::<_, DeadlineItem>(
        "SELECT id, name, status, due_date AS due_date FROM projects
         WHERE owner_id = ? AND status <> 'completed' AND due_date IS NOT NULL AND due_date <= ?
         ORDER BY due_date ASC LIMIT 6",
    )
    .bind(owner)
    .bind(&soon_s)
    .fetch_all(pool)
    .await?;

    let (total_files, total_bytes): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(f.size_bytes), 0)
         FROM project_files f JOIN projects p ON p.id = f.project_id
         WHERE p.owner_id = ?",
    )
    .bind(owner)
    .fetch_one(pool)
    .await?;

    let (requirements_total, requirements_done): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN EXISTS (SELECT 1 FROM project_files f WHERE f.requirement_id = r.id)
                                  THEN 1 ELSE 0 END), 0)
         FROM project_requirements r JOIN projects p ON p.id = r.project_id
         WHERE p.owner_id = ?",
    )
    .bind(owner)
    .fetch_one(pool)
    .await?;

    let months = last_months(today);
    let first_month_start = format!("{}-01T00:00:00Z", months[0]);
    let upload_rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT substr(f.created_at, 1, 7) AS key, COUNT(*) AS count
         FROM project_files f JOIN projects p ON p.id = f.project_id
         WHERE p.owner_id = ? AND f.created_at >= ?
         GROUP BY key",
    )
    .bind(owner)
    .bind(&first_month_start)
    .fetch_all(pool)
    .await?;
    let month_refs: Vec<&str> = months.iter().map(String::as_str).collect();

    let recent = sqlx::query_as::<_, RecentItem>(
        "SELECT id, name, status, updated_at FROM projects
         WHERE owner_id = ? ORDER BY updated_at DESC LIMIT 5",
    )
    .bind(owner)
    .fetch_all(pool)
    .await?;

    Ok(Json(Dashboard {
        today: today_s,
        total_projects,
        by_status: fill(&STATUSES, &status_rows),
        by_type: fill(&PROJECT_TYPES, &type_rows),
        overdue,
        due_soon,
        total_files,
        total_bytes,
        requirements_total,
        requirements_done,
        uploads_by_month: fill(&month_refs, &upload_rows),
        recent,
    }))
}
