mod access;
mod comments;
mod auth;
mod dashboard;
mod events;
mod drive;
mod error;
mod fields;
mod files;
mod health;
mod mail;
mod members;
mod profile;
mod projects;
mod report;
mod requirements;
mod tickets;
mod types;

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
    /// None jika SMTP tidak diatur; email lalu tidak dikirim.
    pub mail: Option<mail::SmtpConfig>,
    /// Alamat publik aplikasi, dipakai untuk tautan di email.
    pub app_url: String,
    /// Saluran perubahan data project untuk Server-Sent Events.
    pub events: tokio::sync::broadcast::Sender<events::ChangeEvent>,
}

/// Waktu sekarang dalam format RFC3339 tetap (detik, UTC) agar bisa dibandingkan sebagai string.
pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `psk-backend healthcheck` hanya mengetes layanan yang sedang berjalan, lalu keluar.
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        std::process::exit(health::run_cli_check());
    }
    // `psk-backend send-test-email <alamat>` memeriksa pengaturan SMTP dari environment.
    if std::env::args().nth(1).as_deref() == Some("send-test-email") {
        let to = std::env::args().nth(2);
        std::process::exit(mail::cli_send_test(to.as_deref()).await);
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/app.db".into());
    let upload_dir = PathBuf::from(std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "data/uploads".into()));
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());

    let base = SqliteConnectOptions::from_str(&database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    // Migrasi berjalan dengan foreign key mati: sebagian migrasi membangun ulang tabel,
    // dan DROP TABLE dengan FK aktif akan menghapus data turunannya (ON DELETE CASCADE).
    let migrate_pool = SqlitePool::connect_with(base.clone().foreign_keys(false)).await?;
    sqlx::migrate!("./migrations").run(&migrate_pool).await?;
    migrate_pool.close().await;

    let pool = SqlitePool::connect_with(base.foreign_keys(true)).await?;
    tokio::fs::create_dir_all(&upload_dir).await?;

    let mail = match mail::from_env() {
        Ok(cfg) => {
            if cfg.is_none() {
                tracing::info!("SMTP_HOST kosong: email dinonaktifkan");
            }
            cfg
        }
        Err(e) => {
            tracing::warn!("konfigurasi SMTP tidak valid, email dinonaktifkan: {e}");
            None
        }
    };
    let app_url = std::env::var("APP_URL")
        .ok()
        .map(|v| v.trim().trim_end_matches('/').to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "http://localhost:4321".into());

    let state = AppState { pool, upload_dir, mail, app_url, events: events::channel() };

    let app = Router::new()
        .route("/api/health", get(health::check))
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::me))
        .route("/api/profile", put(profile::update))
        .route("/api/profile/password", put(profile::change_password))
        .route("/api/dashboard", get(dashboard::summary))
        .route("/api/reports/projects", get(report::json))
        .route("/api/reports/projects.csv", get(report::csv))
        .route("/api/project-types", get(types::list).post(types::create))
        .route("/api/project-types/{id}", put(types::update).delete(types::delete))
        .route(
            "/api/projects",
            get(projects::list).post(projects::create),
        )
        .route("/api/projects/{id}/events", get(events::stream_project))
        .route(
            "/api/projects/{id}/files/from-url",
            post(files::upload_from_url),
        )
        .route(
            "/api/projects/{id}/duplicate",
            post(projects::duplicate),
        )
        .route(
            "/api/projects/{id}/comments",
            get(comments::list)
                .post(comments::create)
                .layer(DefaultBodyLimit::max(
                    comments::MAX_ATTACHMENTS * comments::MAX_ATTACHMENT_BYTES + 1024 * 1024,
                )),
        )
        .route("/api/comment-attachments/{id}", get(comments::download))
        .route("/api/comments/{id}/vote", post(comments::vote))
        .route(
            "/api/projects/{id}/tickets",
            get(tickets::list).post(tickets::create),
        )
        .route(
            "/api/tickets/{id}",
            put(tickets::update).delete(tickets::delete),
        )
        .route("/api/comments/{id}", axum::routing::delete(comments::delete))
        .route(
            "/api/projects/{id}/members",
            get(members::list).post(members::add),
        )
        .route(
            "/api/projects/{id}/members/{uid}",
            put(members::update_role).delete(members::remove),
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
            "/api/projects/{id}/fields",
            get(fields::list).post(fields::create),
        )
        .route(
            "/api/fields/{id}",
            put(fields::update).delete(fields::delete),
        )
        .route("/api/fields/{id}/move", post(fields::move_field))
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
