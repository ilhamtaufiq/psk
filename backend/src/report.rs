use axum::{
    extract::{Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{FixedOffset, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{
    auth::AuthUser,
    error::AppError,
    projects::{Project, PROJECT_SELECT},
    AppState,
};

#[derive(Deserialize)]
pub struct ReportQuery {
    status: Option<String>,
    #[serde(rename = "type")]
    project_type: Option<String>,
}

#[derive(Serialize)]
pub struct StatusCount {
    pub key: String,
    pub label: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct ReportSummary {
    pub generated_at: String,
    pub today: String,
    pub total: i64,
    pub overdue: i64,
    pub completed: i64,
    pub by_status: Vec<StatusCount>,
    pub files_total: i64,
    pub requirements_total: i64,
    pub requirements_done: i64,
}

/// Project di laporan, ditambah daftar item kelengkapan yang belum punya berkas.
#[derive(Serialize)]
pub struct ReportProject {
    #[serde(flatten)]
    pub project: Project,
    pub missing_requirements: Vec<String>,
}

/// Rekap satu item kelengkapan lintas project: berapa project punya item ini, dan berapa sudah ada berkasnya.
#[derive(Serialize)]
pub struct RecapItem {
    pub label: String,
    pub total: i64,
    pub done: i64,
    pub percent: i64,
}

#[derive(Serialize)]
pub struct ReportResponse {
    pub summary: ReportSummary,
    pub projects: Vec<ReportProject>,
    pub recap: Vec<RecapItem>,
}

pub fn status_label(status: &str) -> &'static str {
    match status {
        "planning" => "Perencanaan",
        "active" => "Sedang berjalan",
        "on_hold" => "Ditunda",
        "completed" => "Selesai",
        _ => "-",
    }
}

fn role_label(role: &str) -> &'static str {
    match role {
        "owner" => "Pemilik",
        "editor" => "Editor",
        _ => "Viewer",
    }
}

/// Tanggal hari ini di WIB, sama dengan aturan tenggat di dashboard.
fn today_wib() -> NaiveDate {
    let offset = FixedOffset::east_opt(7 * 3600).expect("offset valid");
    Utc::now().with_timezone(&offset).date_naive()
}

fn is_overdue(p: &Project, today: NaiveDate) -> bool {
    if p.status == "completed" {
        return false;
    }
    match p.due_date.as_deref().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) {
        Some(due) => due < today,
        None => false,
    }
}

/// Semua project yang bisa diakses pengguna, dengan filter opsional. Urutan: nama.
async fn load(state: &AppState, user_id: &str, q: &ReportQuery) -> Result<Vec<Project>, AppError> {
    let status = q.status.clone().filter(|s| !s.trim().is_empty());
    let project_type = q
        .project_type
        .clone()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty());
    let sql = format!(
        "{PROJECT_SELECT} WHERE (p.owner_id = ? OR p.id IN (SELECT project_id FROM project_members WHERE user_id = ?))
           AND (? IS NULL OR p.status = ?)
           AND (? IS NULL OR p.project_type = ?)
         ORDER BY p.name COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, Project>(&sql)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(user_id)
        .bind(&status)
        .bind(&status)
        .bind(&project_type)
        .bind(&project_type)
        .fetch_all(&state.pool)
        .await?)
}

/// Item kelengkapan yang belum punya berkas, dikelompokkan per project (urut dibuat).
async fn missing_requirements(
    state: &AppState,
    user_id: &str,
) -> Result<HashMap<String, Vec<String>>, AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT r.project_id, r.title
         FROM project_requirements r
         WHERE r.project_id IN (
             SELECT id FROM projects
             WHERE owner_id = ? OR id IN (SELECT project_id FROM project_members WHERE user_id = ?))
           AND NOT EXISTS (SELECT 1 FROM project_files f WHERE f.requirement_id = r.id)
         ORDER BY r.created_at ASC",
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;

    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for (project_id, title) in rows {
        map.entry(project_id).or_default().push(title);
    }
    Ok(map)
}

fn summarize(projects: &[Project]) -> ReportSummary {
    let today = today_wib();
    let statuses = ["planning", "active", "on_hold", "completed"];
    let by_status = statuses
        .iter()
        .map(|s| StatusCount {
            key: (*s).into(),
            label: status_label(s).into(),
            count: projects.iter().filter(|p| p.status == *s).count() as i64,
        })
        .collect();
    ReportSummary {
        generated_at: crate::now_iso(),
        today: today.to_string(),
        total: projects.len() as i64,
        overdue: projects.iter().filter(|p| is_overdue(p, today)).count() as i64,
        completed: projects.iter().filter(|p| p.status == "completed").count() as i64,
        by_status,
        files_total: projects.iter().map(|p| p.file_count).sum(),
        requirements_total: projects.iter().map(|p| p.req_total).sum(),
        requirements_done: projects.iter().map(|p| p.req_done).sum(),
    }
}

/// Rekap kelengkapan per nama item (tanpa membedakan huruf besar/kecil), mengikuti filter laporan.
async fn requirement_recap(state: &AppState, user_id: &str, q: &ReportQuery) -> Result<Vec<RecapItem>, AppError> {
    let status = q.status.clone().filter(|s| !s.trim().is_empty());
    let project_type = q
        .project_type
        .clone()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty());
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT MIN(TRIM(r.title)) AS label,
                COUNT(*) AS total,
                SUM(CASE WHEN EXISTS (SELECT 1 FROM project_files f WHERE f.requirement_id = r.id)
                         THEN 1 ELSE 0 END) AS done
         FROM project_requirements r
         JOIN projects p ON p.id = r.project_id
         WHERE (p.owner_id = ? OR p.id IN (SELECT project_id FROM project_members WHERE user_id = ?))
           AND (? IS NULL OR p.status = ?)
           AND (? IS NULL OR p.project_type = ?)
         GROUP BY LOWER(TRIM(r.title))
         ORDER BY total DESC, label COLLATE NOCASE",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(&status)
    .bind(&status)
    .bind(&project_type)
    .bind(&project_type)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(label, total, done)| RecapItem {
            label,
            total,
            done,
            percent: if total > 0 { (done * 100 + total / 2) / total } else { 0 },
        })
        .collect())
}

pub async fn json(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<ReportQuery>,
) -> Result<Json<ReportResponse>, AppError> {
    let recap = requirement_recap(&state, &user.id, &q).await?;
    let projects = load(&state, &user.id, &q).await?;
    let summary = summarize(&projects);
    let mut missing = missing_requirements(&state, &user.id).await?;
    let projects = projects
        .into_iter()
        .map(|p| {
            let missing_requirements = missing.remove(&p.id).unwrap_or_default();
            ReportProject { project: p, missing_requirements }
        })
        .collect();
    Ok(Json(ReportResponse { summary, projects, recap }))
}

/// Mencegah nilai sel dibaca sebagai rumus oleh Excel/Sheets (CSV injection).
fn csv_cell(raw: &str) -> String {
    let mut value = raw.replace('\r', " ").replace('\n', " ");
    if value.starts_with(['=', '+', '-', '@']) {
        value.insert(0, '\'');
    }
    if value.contains([',', '"', ';']) || value.starts_with(' ') || value.starts_with('\'') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

pub async fn csv(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(q): Query<ReportQuery>,
) -> Result<Response, AppError> {
    let projects = load(&state, &user.id, &q).await?;
    let today = today_wib();

    let mut out = String::from("\u{feff}");
    out.push_str("Nama,Jenis,Status,Peran Anda,Tanggal mulai,Tanggal selesai,Terlambat,Kelengkapan,Belum ada berkas,Jumlah berkas\r\n");
    let mut missing = missing_requirements(&state, &user.id).await?;
    for p in &projects {
        let missing_items = missing.remove(&p.id).unwrap_or_default().join("; ");
        let row = [
            csv_cell(&p.name),
            csv_cell(&p.project_type_label),
            csv_cell(status_label(&p.status)),
            csv_cell(role_label(&p.role)),
            csv_cell(p.start_date.as_deref().unwrap_or("")),
            csv_cell(p.due_date.as_deref().unwrap_or("")),
            csv_cell(if is_overdue(p, today) { "Ya" } else { "Tidak" }),
            csv_cell(&format!("{}/{}", p.req_done, p.req_total)),
            csv_cell(&missing_items),
            csv_cell(&p.file_count.to_string()),
        ];
        out.push_str(&row.join(","));
        out.push_str("\r\n");
    }

    let filename = format!("laporan-project-{}.csv", today);
    let mut res = (StatusCode::OK, out).into_response();
    let headers = res.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/csv; charset=utf-8"));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    Ok(res)
}
