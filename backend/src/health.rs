use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};

use crate::AppState;

/// Status layanan untuk monitoring dan Docker HEALTHCHECK.
/// 200 bila database bisa dibaca; 503 bila tidak.
pub async fn check(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let db_ok = sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();

    let (code, status) = if db_ok {
        (StatusCode::OK, "ok")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "error")
    };

    (
        code,
        Json(json!({
            "status": status,
            "database": if db_ok { "ok" } else { "error" },
            "version": env!("CARGO_PKG_VERSION"),
        })),
    )
}

/// Dipanggil dari dalam container: `psk-backend healthcheck`.
/// Mengetes endpoint /api/health di alamat lokal dan mengembalikan exit code 0 bila 200.
/// Dibuat tanpa curl agar image tetap kecil.
pub fn run_cli_check() -> i32 {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let bind = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let port = bind.rsplit(':').next().unwrap_or("8080");
    let Ok(addr) = format!("127.0.0.1:{port}").parse::<SocketAddr>() else {
        return 1;
    };

    let timeout = Duration::from_secs(3);
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, timeout) else {
        return 1;
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let request = "GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    if stream.write_all(request.as_bytes()).is_err() {
        return 1;
    }

    let mut response = String::new();
    if stream.read_to_string(&mut response).is_err() {
        return 1;
    }
    let status_line = response.lines().next().unwrap_or("");
    if status_line.split_whitespace().nth(1) == Some("200") {
        0
    } else {
        1
    }
}
