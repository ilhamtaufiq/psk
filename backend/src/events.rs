//! Perubahan data proyek disiarkan ke browser yang sedang melihat proyek itu (Server-Sent Events).
//! Event hanya berisi "apa yang berubah", bukan datanya; browser lalu mengambil ulang halaman.

use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::{stream, Stream};
use tokio::sync::broadcast::{self, error::RecvError};

use crate::{
    access::{self, Role},
    auth::AuthUser,
    error::AppError,
    AppState,
};

#[derive(Clone, Debug)]
pub struct ChangeEvent {
    pub project_id: String,
    pub kind: &'static str,
}

pub fn channel() -> broadcast::Sender<ChangeEvent> {
    // Kapasitas kecil cukup: browser yang tertinggal akan menerima "resync" dan mengambil ulang.
    broadcast::channel(256).0
}

/// Menyiarkan perubahan. Tidak ada subscriber bukan error; diabaikan.
pub fn publish(state: &AppState, project_id: &str, kind: &'static str) {
    let _ = state.events.send(ChangeEvent {
        project_id: project_id.to_string(),
        kind,
    });
}

pub async fn stream_project(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;

    let rx = state.events.subscribe();
    let stream = stream::unfold((rx, project_id), |(mut rx, pid)| async move {
        loop {
            match rx.recv().await {
                Ok(ev) if ev.project_id == pid => {
                    let event = Event::default().event("change").data(ev.kind);
                    return Some((Ok(event), (rx, pid)));
                }
                Ok(_) => continue,
                Err(RecvError::Lagged(_)) => {
                    let event = Event::default().event("change").data("resync");
                    return Some((Ok(event), (rx, pid)));
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
