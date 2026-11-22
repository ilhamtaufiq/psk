//! Mengambil berkas dari link Google Drive yang dibagikan ("Siapa saja yang memiliki link").
//! Keamanan: hanya https dan hanya host Google yang diizinkan, termasuk untuk setiap redirect,
//! supaya server tidak bisa dimanipulasi untuk mengakses alamat internal.

use std::time::Duration;

use reqwest::{redirect, Url};

use crate::{error::AppError, files::{mime_for, sanitize_filename}};

pub const MAX_DOWNLOAD_BYTES: usize = 25 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

/// Host yang boleh menjadi sumber link dari pengguna.
fn is_drive_host(host: &str) -> bool {
    host == "drive.google.com"
}

/// Host yang boleh dihubungi server (termasuk tujuan redirect Google).
fn is_allowed_fetch_host(host: &str) -> bool {
    host == "drive.google.com"
        || host == "drive.usercontent.google.com"
        || host.ends_with(".googleusercontent.com")
}

/// Mengambil ID berkas dari bentuk link umum: /file/d/ID/..., /open?id=ID, /uc?id=ID.
pub fn parse_file_id(raw: &str) -> Result<String, AppError> {
    let bad = || AppError::BadRequest(
        "Link Google Drive tidak dikenali. Gunakan link berbagi berkas, mis. https://drive.google.com/file/d/…/view".into(),
    );
    let url = Url::parse(raw.trim()).map_err(|_| bad())?;
    if url.scheme() != "https" || !url.host_str().map(is_drive_host).unwrap_or(false) {
        return Err(bad());
    }

    let segments: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
    let from_path = segments
        .windows(2)
        .find(|w| w[0] == "d")
        .map(|w| w[1].to_string());
    let from_query = url
        .query_pairs()
        .find(|(k, _)| k == "id")
        .map(|(_, v)| v.into_owned());

    let id = from_path.or(from_query).ok_or_else(bad)?;
    let valid = id.len() >= 10
        && id.len() <= 200
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid {
        return Err(bad());
    }
    Ok(id)
}

/// Mengunduh berkas. Mengembalikan (isi, nama berkas, tipe MIME).
pub async fn download(file_id: &str) -> Result<(Vec<u8>, String, String), AppError> {
    let client = reqwest::Client::builder()
        .redirect(redirect::Policy::custom(|attempt| {
            let allowed = attempt.url().scheme() == "https"
                && attempt.url().host_str().map(is_allowed_fetch_host).unwrap_or(false);
            if !allowed {
                attempt.error("redirect ke host yang tidak diizinkan")
            } else if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("terlalu banyak redirect")
            } else {
                attempt.follow()
            }
        }))
        .timeout(Duration::from_secs(60))
        .user_agent("ProjectSelepasKerja/0.1")
        .build()
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let url = format!(
        "https://drive.usercontent.google.com/download?id={file_id}&export=download&confirm=t"
    );
    let mut res = client
        .get(&url)
        .send()
        .await
        .map_err(|_| AppError::BadRequest("Tidak bisa menghubungi Google Drive. Coba lagi nanti.".into()))?;

    let not_public = || AppError::BadRequest(
        "Berkas tidak bisa diunduh. Pastikan berbagi diatur ke \"Siapa saja yang memiliki link\".".into(),
    );
    if !res.status().is_success() {
        return Err(not_public());
    }

    let content_type = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    // Halaman HTML berarti Google menampilkan halaman login/izin, bukan berkasnya.
    if content_type.starts_with("text/html") {
        return Err(not_public());
    }

    let disposition = res
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    if let Some(len) = res.content_length() {
        if len as usize > MAX_DOWNLOAD_BYTES {
            return Err(AppError::BadRequest("Ukuran berkas maksimal 25 MB".into()));
        }
    }

    let mut data: Vec<u8> = Vec::new();
    while let Some(chunk) = res
        .chunk()
        .await
        .map_err(|_| AppError::BadRequest("Unduhan dari Google Drive terputus".into()))?
    {
        if data.len() + chunk.len() > MAX_DOWNLOAD_BYTES {
            return Err(AppError::BadRequest("Ukuran berkas maksimal 25 MB".into()));
        }
        data.extend_from_slice(&chunk);
    }
    if data.is_empty() {
        return Err(AppError::BadRequest("Berkas dari Google Drive kosong".into()));
    }

    let name = disposition
        .as_deref()
        .and_then(filename_from_disposition)
        .map(|n| sanitize_filename(&n))
        .unwrap_or_else(|| format!("drive-{}", &file_id[..8.min(file_id.len())]));
    let mime = if content_type.is_empty() || content_type == "application/octet-stream" {
        mime_for(&name, None)
    } else {
        content_type
    };
    Ok((data, name, mime))
}

/// Nama dari Content-Disposition: filename*=UTF-8''... atau filename="...".
fn filename_from_disposition(value: &str) -> Option<String> {
    if let Some(pos) = value.find("filename*=") {
        let rest = &value[pos + "filename*=".len()..];
        let raw = rest.split(';').next()?.trim();
        let encoded = raw.rsplit("''").next()?;
        return Some(percent_decode(encoded));
    }
    let pos = value.find("filename=")?;
    let rest = &value[pos + "filename=".len()..];
    let raw = rest.split(';').next()?.trim().trim_matches('"');
    Some(raw.to_string())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_drive_link_forms() {
        let id = "1AbCdEfGhIjKlMnOp";
        assert_eq!(parse_file_id(&format!("https://drive.google.com/file/d/{id}/view?usp=sharing")).unwrap(), id);
        assert_eq!(parse_file_id(&format!("https://drive.google.com/open?id={id}")).unwrap(), id);
        assert_eq!(parse_file_id(&format!("https://drive.google.com/uc?export=download&id={id}")).unwrap(), id);
    }

    #[test]
    fn rejects_unsafe_or_unknown_links() {
        assert!(parse_file_id("http://drive.google.com/file/d/1AbCdEfGhIjKlMnOp/view").is_err());
        assert!(parse_file_id("https://evil.example.com/file/d/1AbCdEfGhIjKlMnOp/view").is_err());
        assert!(parse_file_id("https://drive.google.com.evil.com/file/d/1AbCdEfGhIjKlMnOp/view").is_err());
        assert!(parse_file_id("https://drive.google.com/file/d/x/view").is_err());
        assert!(parse_file_id("https://drive.google.com/drive/folders/1AbCdEfGhIjKlMnOp").is_err());
        assert!(parse_file_id("https://drive.google.com/file/d/../../etc/passwd").is_err());
    }

    #[test]
    fn reads_filename_from_content_disposition() {
        assert_eq!(filename_from_disposition("attachment; filename=\"Laporan.pdf\"").unwrap(), "Laporan.pdf");
        assert_eq!(
            filename_from_disposition("attachment; filename*=UTF-8''Laporan%20Keuangan%20Q3.pdf").unwrap(),
            "Laporan Keuangan Q3.pdf"
        );
    }
}
