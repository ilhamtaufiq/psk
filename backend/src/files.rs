use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::{
    access::{self, Role},
    auth::AuthUser,
    error::AppError,
    now_iso,
    projects::fetch_project,
    AppState,
};

pub const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, Serialize, FromRow)]
pub struct ProjectFile {
    pub id: String,
    pub project_id: String,
    pub requirement_id: Option<String>,
    pub original_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub uploaded_by_name: String,
    pub created_at: String,
}

#[derive(FromRow)]
struct StoredFile {
    id: String,
    project_id: String,
    original_name: String,
    mime_type: String,
    size_bytes: i64,
}

const FILE_SELECT: &str = "SELECT f.id, f.project_id, f.requirement_id, f.original_name, f.mime_type,
    f.size_bytes, u.name AS uploaded_by_name, f.created_at
    FROM project_files f JOIN users u ON u.id = f.uploaded_by";

/// Berkas diambil lalu peran pengguna di projectnya diperiksa (lihat, atau ubah untuk hapus).
async fn find_file_for(
    pool: &SqlitePool,
    file_id: &str,
    user_id: &str,
    min: Role,
) -> Result<StoredFile, AppError> {
    let file = sqlx::query_as::<_, StoredFile>(
        "SELECT f.id, f.project_id, f.original_name, f.mime_type, f.size_bytes
         FROM project_files f WHERE f.id = ?",
    )
    .bind(file_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Berkas tidak ditemukan".into()))?;
    access::require(pool, &file.project_id, user_id, min).await?;
    Ok(file)
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ProjectFile>>, AppError> {
    fetch_project(&state.pool, &project_id, &user.id).await?;
    let sql = format!("{FILE_SELECT} WHERE f.project_id = ? ORDER BY f.created_at DESC");
    let rows = sqlx::query_as::<_, ProjectFile>(&sql)
        .bind(&project_id)
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(rows))
}

/// Menyimpan berkas ke disk dan mencatatnya di database. Dipakai unggahan langsung
/// dan unduhan dari URL Google Drive.
pub async fn store_file(
    state: &AppState,
    project_id: &str,
    requirement_id: Option<String>,
    user_id: &str,
    original_name: String,
    mime_type: String,
    data: Vec<u8>,
) -> Result<ProjectFile, AppError> {
    // Nama di disk memakai UUID, jadi nama asli dari klien tidak pernah dipakai sebagai path.
    let file_id = Uuid::new_v4().to_string();
    let dir = state.upload_dir.join(&project_id);
    tokio::fs::create_dir_all(&dir).await?;
    tokio::fs::write(dir.join(&file_id), &data).await?;

    sqlx::query(
        "INSERT INTO project_files (id, project_id, requirement_id, uploaded_by, original_name, mime_type, size_bytes, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&file_id)
    .bind(&project_id)
    .bind(requirement_id.as_deref())
    .bind(&user_id)
    .bind(&original_name)
    .bind(&mime_type)
    .bind(data.len() as i64)
    .bind(now_iso())
    .execute(&state.pool)
    .await?;

    let sql = format!("{FILE_SELECT} WHERE f.id = ?");
    let file = sqlx::query_as::<_, ProjectFile>(&sql)
        .bind(&file_id)
        .fetch_one(&state.pool)
        .await?;
    Ok(file)
}

pub async fn upload(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<ProjectFile>), AppError> {
    access::require(&state.pool, &project_id, &user.id, Role::Editor).await?;

    // Field bisa datang dalam urutan apa pun, jadi semua dibaca dulu.
    let mut requirement_id: Option<String> = None;
    let mut upload: Option<(String, String, Vec<u8>)> = None;

    while let Some(field) = multipart.next_field().await? {
        let name = field.name().map(str::to_string);
        match name.as_deref() {
            Some("requirement_id") => {
                let value = field.text().await?.trim().to_string();
                if !value.is_empty() {
                    requirement_id = Some(value);
                }
            }
            Some("file") if upload.is_none() => {
                let original_name = sanitize_filename(field.file_name().unwrap_or(""));
                let mime_type = mime_for(&original_name, field.content_type());
                let data = field.bytes().await?.to_vec();
                upload = Some((original_name, mime_type, data));
            }
            _ => {}
        }
    }

    let (original_name, mime_type, data) = upload.ok_or_else(|| {
        AppError::BadRequest("Berkas tidak ditemukan dalam permintaan (field 'file')".into())
    })?;

    if data.is_empty() {
        return Err(AppError::BadRequest("Berkas kosong".into()));
    }
    if data.len() > MAX_UPLOAD_BYTES {
        return Err(AppError::BadRequest("Ukuran berkas maksimal 25 MB".into()));
    }

    // Item daftar harus milik project yang sama.
    if let Some(rid) = &requirement_id {
        let (found,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM project_requirements WHERE id = ? AND project_id = ?",
        )
        .bind(rid)
        .bind(&project_id)
        .fetch_one(&state.pool)
        .await?;
        if found == 0 {
            return Err(AppError::BadRequest(
                "Item daftar berkas tidak valid untuk project ini".into(),
            ));
        }
    }

    let file = store_file(
        &state,
        &project_id,
        requirement_id,
        &user.id,
        original_name,
        mime_type,
        data,
    )
    .await?;
    crate::events::publish(&state, &project_id, "files");
    Ok((StatusCode::CREATED, Json(file)))
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    /// `?inline=1` meminta pratinjau di peramban (hanya untuk tipe yang aman, lihat `INLINE_SAFE`).
    inline: Option<String>,
}

/// Tipe yang boleh ditampilkan langsung di peramban. SVG, HTML, dan lainnya selalu diunduh.
const INLINE_SAFE: [&str; 7] = [
    "application/pdf",
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/avif",
    "image/bmp",
];

/// Tipe MIME dari nama berkas (lebih andal daripada yang dikirim peramban), jika tidak dikenali pakai bawaan klien.
pub fn mime_for(name: &str, client: Option<&str>) -> String {
    if let Some(guess) = mime_guess::from_path(name).first() {
        return guess.essence_str().to_string();
    }
    client
        .filter(|m| !m.trim().is_empty())
        .unwrap_or("application/octet-stream")
        .to_string()
}

pub async fn download(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(file_id): Path<String>,
    Query(query): Query<DownloadQuery>,
) -> Result<Response, AppError> {
    let file = find_file_for(&state.pool, &file_id, &user.id, Role::Viewer).await?;
    let path = state.upload_dir.join(&file.project_id).join(&file.id);
    let handle = tokio::fs::File::open(&path)
        .await
        .map_err(|_| AppError::NotFound("Berkas tidak ditemukan di penyimpanan".into()))?;

    let wants_inline = matches!(query.inline.as_deref(), Some("1") | Some("true"));
    let inline = wants_inline && INLINE_SAFE.contains(&file.mime_type.as_str());

    let mut res = Response::new(Body::from_stream(ReaderStream::new(handle)));
    let headers = res.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        header_value(&file.mime_type, "application/octet-stream"),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        header_value(&file.size_bytes.to_string(), "0"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        content_disposition(&file.original_name, inline),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // Berkas dari pengguna tidak boleh menjalankan skrip, apa pun tipenya.
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("sandbox; default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(res)
}

pub async fn delete(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(file_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let file = find_file_for(&state.pool, &file_id, &user.id, Role::Editor).await?;
    sqlx::query("DELETE FROM project_files WHERE id = ?")
        .bind(&file.id)
        .execute(&state.pool)
        .await?;
    let _ = tokio::fs::remove_file(state.upload_dir.join(&file.project_id).join(&file.id)).await;
    crate::events::publish(&state, &file.project_id, "files");
    Ok(StatusCode::NO_CONTENT)
}

pub fn sanitize_filename(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("").trim();
    let cleaned: String = base.chars().filter(|c| !c.is_control()).take(200).collect();
    if cleaned.is_empty() {
        "berkas".into()
    } else {
        cleaned
    }
}

fn header_value(value: &str, fallback: &'static str) -> HeaderValue {
    HeaderValue::from_str(value).unwrap_or_else(|_| HeaderValue::from_static(fallback))
}

/// Header `Content-Disposition` dengan nama ASCII fallback dan `filename*` (RFC 5987) untuk nama Unicode.
fn content_disposition(name: &str, inline: bool) -> HeaderValue {
    let kind = if inline { "inline" } else { "attachment" };
    let ascii: String = name
        .chars()
        .map(|c| if c.is_ascii() && c != '"' && !c.is_control() { c } else { '_' })
        .collect();
    let encoded: String = name
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{:02X}", b)
            }
        })
        .collect();
    HeaderValue::from_str(&format!(
        "{kind}; filename=\"{ascii}\"; filename*=UTF-8''{encoded}"
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("attachment"))
}

#[derive(Deserialize)]
pub struct UrlUploadInput {
    url: String,
    requirement_id: Option<String>,
}

/// Mengunggah berkas dari link Google Drive yang dibagikan. Validasi link dilakukan sebelum
/// server menghubungi Google, supaya input yang salah tidak memicu permintaan jaringan.
pub async fn upload_from_url(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
    Json(input): Json<UrlUploadInput>,
) -> Result<(StatusCode, Json<ProjectFile>), AppError> {
    access::require(&state.pool, &project_id, &user.id, Role::Editor).await?;
    let file_id = crate::drive::parse_file_id(&input.url)?;

    let requirement_id = input.requirement_id.filter(|r| !r.trim().is_empty());
    if let Some(rid) = &requirement_id {
        let (found,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM project_requirements WHERE id = ? AND project_id = ?",
        )
        .bind(rid)
        .bind(&project_id)
        .fetch_one(&state.pool)
        .await?;
        if found == 0 {
            return Err(AppError::BadRequest(
                "Item daftar berkas tidak valid untuk project ini".into(),
            ));
        }
    }

    let (data, name, mime) = crate::drive::download(&file_id).await?;
    let file = store_file(&state, &project_id, requirement_id, &user.id, name, mime, data).await?;
    crate::events::publish(&state, &project_id, "files");
    Ok((StatusCode::CREATED, Json(file)))
}
