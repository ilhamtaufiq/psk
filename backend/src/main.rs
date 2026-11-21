mod auth;
mod dashboard;
mod error;
mod files;
mod projects;
mod requirements;

use std::{path::PathBuf, str::FromStr};

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post, put},
    Router,
};
use chrono::{SecondsFormat, Utc};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    SqlitePool,
};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub upload_dir: PathBuf,
}

/// Waktu sekarang dalam format RFC3339 tetap (detik, UTC) agar bisa dibandingkan sebagai string.
pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/app.db".into());
    let upload_dir = PathBuf::from(std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "data/uploads".into()));
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());

    let options = SqliteConnectOptions::from_str(&database_url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePool::connect_with(options).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    tokio::fs::create_dir_all(&upload_dir).await?;

    let state = AppState { pool, upload_dir };

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::me))
        .route("/api/dashboard", get(dashboard::summary))
        .route(
            "/api/projects",
            get(projects::list).post(projects::create),
        )
        .route(
            "/api/projects/{id}",
            get(projects::get)
                .put(projects::update)
                .delete(projects::delete),
        )
        .route(
            "/api/projects/{id}/files",
            get(files::list)
                .post(files::upload)
                .layer(DefaultBodyLimit::max(files::MAX_UPLOAD_BYTES + 1024 * 1024)),
        )
        .route(
            "/api/projects/{id}/requirements",
            get(requirements::list).post(requirements::create),
        )
        .route(
            "/api/requirements/{id}",
            put(requirements::update).delete(requirements::delete),
        )
        .route(
            "/api/files/{id}",
            get(files::download).delete(files::delete),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("backend berjalan di {bind_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
