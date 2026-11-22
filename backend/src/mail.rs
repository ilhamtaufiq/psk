//! Pengiriman email lewat SMTP. Semua pengaturan dibaca dari environment (diatur di Coolify).
//! Jika SMTP_HOST kosong, email dinonaktifkan dan aplikasi tetap berjalan normal.

use std::time::Duration;

use lettre::{
    message::{header::ContentType, Mailbox},
    transport::smtp::{authentication::Credentials, AsyncSmtpTransport},
    AsyncTransport, Message, Tokio1Executor,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsMode {
    /// Koneksi biasa lalu upgrade ke TLS (port 587 umumnya).
    StartTls,
    /// TLS sejak awal koneksi (SMTPS, port 465 umumnya).
    Tls,
    /// Tanpa TLS. Hanya untuk server lokal atau jaringan tepercaya.
    None,
}

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: Mailbox,
    pub tls: TlsMode,
}

// Nilai kosong di environment (mis. dari `${SMTP_PORT:-}` di compose) dianggap tidak diisi.
fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Membaca konfigurasi SMTP. Ok(None) berarti email sengaja dinonaktifkan.
pub fn from_env() -> Result<Option<SmtpConfig>, String> {
    let Some(host) = env_opt("SMTP_HOST") else {
        return Ok(None);
    };

    let tls = match env_opt("SMTP_TLS").unwrap_or_else(|| "starttls".into()).to_lowercase().as_str() {
        "starttls" => TlsMode::StartTls,
        "tls" | "ssl" => TlsMode::Tls,
        "none" => TlsMode::None,
        other => return Err(format!("SMTP_TLS harus starttls, tls, atau none (bukan '{other}')")),
    };

    let port = match env_opt("SMTP_PORT") {
        Some(p) => p.parse::<u16>().map_err(|_| "SMTP_PORT harus angka".to_string())?,
        None if tls == TlsMode::Tls => 465,
        None => 587,
    };

    let from_raw = env_opt("SMTP_FROM").ok_or("SMTP_FROM wajib diisi jika SMTP_HOST diatur")?;
    let from: Mailbox = from_raw
        .parse()
        .map_err(|e| format!("SMTP_FROM tidak valid ({e}); contoh: Project <noreply@contoh.com>"))?;

    Ok(Some(SmtpConfig {
        host,
        port,
        username: env_opt("SMTP_USERNAME"),
        password: env_opt("SMTP_PASSWORD"),
        from,
        tls,
    }))
}

fn transport(cfg: &SmtpConfig) -> Result<AsyncSmtpTransport<Tokio1Executor>, String> {
    let builder = match cfg.tls {
        TlsMode::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)
            .map_err(|e| e.to_string())?,
        TlsMode::Tls => {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.host).map_err(|e| e.to_string())?
        }
        TlsMode::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.host),
    };

    let mut builder = builder.port(cfg.port).timeout(Some(Duration::from_secs(20)));
    if let (Some(user), Some(pass)) = (&cfg.username, &cfg.password) {
        builder = builder.credentials(Credentials::new(user.clone(), pass.clone()));
    }
    Ok(builder.build())
}

/// Mengirim email teks biasa. Pesan galat tidak berisi kata sandi.
pub async fn send(cfg: &SmtpConfig, to: &str, subject: &str, body: &str) -> Result<(), String> {
    let to: Mailbox = to.parse().map_err(|_| "alamat tujuan tidak valid".to_string())?;
    let email = Message::builder()
        .from(cfg.from.clone())
        .to(to)
        .subject(subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())
        .map_err(|e| e.to_string())?;

    transport(cfg)?
        .send(email)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Perintah `psk-backend send-test-email <alamat>`: memeriksa konfigurasi SMTP dari environment.
/// Mengembalikan exit code (0 berhasil).
pub async fn cli_send_test(to: Option<&str>) -> i32 {
    let Some(to) = to else {
        eprintln!("pemakaian: psk-backend send-test-email <alamat-tujuan>");
        return 2;
    };
    let cfg = match from_env() {
        Ok(Some(cfg)) => cfg,
        Ok(None) => {
            eprintln!("SMTP belum diatur: isi SMTP_HOST (dan SMTP_FROM) di environment.");
            return 1;
        }
        Err(e) => {
            eprintln!("konfigurasi SMTP tidak valid: {e}");
            return 1;
        }
    };
    println!("mengirim email uji ke {to} lewat {}:{} ({:?})", cfg.host, cfg.port, cfg.tls);
    match send(
        &cfg,
        to,
        "Uji SMTP Proyek Selepas Kerja",
        "Ini email uji. Jika Anda menerimanya, pengaturan SMTP sudah benar.",
    )
    .await
    {
        Ok(()) => {
            println!("berhasil");
            0
        }
        Err(e) => {
            eprintln!("gagal: {e}");
            1
        }
    }
}
