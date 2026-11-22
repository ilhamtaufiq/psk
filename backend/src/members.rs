use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::{
    access::{self, Role},
    auth::AuthUser,
    error::AppError,
    now_iso, AppState,
};

#[derive(Debug, Serialize, FromRow)]
pub struct Member {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub role: String,
}

#[derive(Serialize)]
pub struct MemberList {
    pub owner: Member,
    pub members: Vec<Member>,
    /// Peran pengguna yang sedang melihat, agar UI tahu apa yang boleh diubah.
    pub my_role: String,
}

#[derive(Deserialize)]
pub struct AddMemberInput {
    email: String,
    role: String,
}

#[derive(Deserialize)]
pub struct RoleInput {
    role: String,
}

fn check_member_role(role: &str) -> Result<String, AppError> {
    match role {
        "editor" | "viewer" => Ok(role.to_string()),
        _ => Err(AppError::BadRequest("Peran harus 'editor' atau 'viewer'".into())),
    }
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
) -> Result<Json<MemberList>, AppError> {
    let my_role = access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;

    let owner = sqlx::query_as::<_, Member>(
        "SELECT u.id AS user_id, u.name, u.email, 'owner' AS role
         FROM projects p JOIN users u ON u.id = p.owner_id WHERE p.id = ?",
    )
    .bind(&project_id)
    .fetch_one(&state.pool)
    .await?;

    let members = sqlx::query_as::<_, Member>(
        "SELECT u.id AS user_id, u.name, u.email, m.role
         FROM project_members m JOIN users u ON u.id = m.user_id
         WHERE m.project_id = ? ORDER BY u.name COLLATE NOCASE",
    )
    .bind(&project_id)
    .fetch_all(&state.pool)
    .await?;

    let my_role = match my_role {
        Role::Owner => "owner",
        Role::Editor => "editor",
        Role::Viewer => "viewer",
    };
    Ok(Json(MemberList { owner, members, my_role: my_role.into() }))
}

/// Mengirim email pemberitahuan undangan di latar belakang. Kegagalan SMTP tidak membatalkan undangan.
async fn notify_invite(state: &AppState, project_id: &str, inviter: &str, member: &Member) {
    let Some(cfg) = state.mail.clone() else {
        tracing::info!("SMTP belum diatur; undangan tidak dikirim lewat email");
        return;
    };

    let project_name: String = sqlx::query_scalar("SELECT name FROM projects WHERE id = ?")
        .bind(project_id)
        .fetch_one(&state.pool)
        .await
        .unwrap_or_else(|_| "tanpa nama".into());
    let role = if member.role == "editor" { "editor" } else { "viewer" };
    let url = format!("{}/projects/{}", state.app_url, project_id);
    let subject = format!("Anda diundang ke project \"{project_name}\"");
    let body = format!(
        "Halo {},\n\n{inviter} mengundang Anda ke project \"{project_name}\" sebagai {role}.\n\nBuka project:\n{url}\n\nSalam,\nProject Selepas Kerja\n",
        member.name
    );
    let to = member.email.clone();
    tokio::spawn(async move {
        if let Err(e) = crate::mail::send(&cfg, &to, &subject, &body).await {
            tracing::warn!("gagal mengirim email undangan: {e}");
        }
    });
}

/// Pemilik mengundang akun yang sudah terdaftar lewat email. Bila sudah jadi anggota, perannya diperbarui.
pub async fn add(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(project_id): Path<String>,
    Json(input): Json<AddMemberInput>,
) -> Result<(StatusCode, Json<Member>), AppError> {
    access::require(&state.pool, &project_id, &user.id, Role::Owner).await?;
    let role = check_member_role(&input.role)?;

    let email = input.email.trim().to_lowercase();
    let target: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?")
        .bind(&email)
        .fetch_optional(&state.pool)
        .await?;
    let Some((target_id,)) = target else {
        return Err(AppError::NotFound(
            "Belum ada akun dengan email ini. Minta dia mendaftar dulu.".into(),
        ));
    };
    if target_id == user.id {
        return Err(AppError::BadRequest("Anda sudah pemilik project ini".into()));
    }

    sqlx::query(
        "INSERT INTO project_members (project_id, user_id, role, created_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(project_id, user_id) DO UPDATE SET role = excluded.role",
    )
    .bind(&project_id)
    .bind(&target_id)
    .bind(&role)
    .bind(now_iso())
    .execute(&state.pool)
    .await?;

    let member = sqlx::query_as::<_, Member>(
        "SELECT u.id AS user_id, u.name, u.email, m.role
         FROM project_members m JOIN users u ON u.id = m.user_id
         WHERE m.project_id = ? AND m.user_id = ?",
    )
    .bind(&project_id)
    .bind(&target_id)
    .fetch_one(&state.pool)
    .await?;
    notify_invite(&state, &project_id, &user.name, &member).await;
    crate::events::publish(&state, &project_id, "members");
    Ok((StatusCode::CREATED, Json(member)))
}

pub async fn update_role(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((project_id, member_id)): Path<(String, String)>,
    Json(input): Json<RoleInput>,
) -> Result<Json<Member>, AppError> {
    access::require(&state.pool, &project_id, &user.id, Role::Owner).await?;
    let role = check_member_role(&input.role)?;

    let result = sqlx::query("UPDATE project_members SET role = ? WHERE project_id = ? AND user_id = ?")
        .bind(&role)
        .bind(&project_id)
        .bind(&member_id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Anggota tidak ditemukan".into()));
    }

    let member = sqlx::query_as::<_, Member>(
        "SELECT u.id AS user_id, u.name, u.email, m.role
         FROM project_members m JOIN users u ON u.id = m.user_id
         WHERE m.project_id = ? AND m.user_id = ?",
    )
    .bind(&project_id)
    .bind(&member_id)
    .fetch_one(&state.pool)
    .await?;
    crate::events::publish(&state, &project_id, "members");
    Ok(Json(member))
}

/// Pemilik bisa mengeluarkan anggota mana pun; anggota boleh keluar sendiri.
pub async fn remove(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((project_id, member_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let my_role = access::require(&state.pool, &project_id, &user.id, Role::Viewer).await?;
    if member_id != user.id {
        if my_role != Role::Owner {
            return Err(AppError::Forbidden("Hanya pemilik yang bisa mengeluarkan anggota".into()));
        }
    } else if my_role == Role::Owner {
        return Err(AppError::BadRequest("Pemilik tidak bisa keluar dari projectnya sendiri".into()));
    }

    let result = sqlx::query("DELETE FROM project_members WHERE project_id = ? AND user_id = ?")
        .bind(&project_id)
        .bind(&member_id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Anggota tidak ditemukan".into()));
    }
    crate::events::publish(&state, &project_id, "members");
    Ok(StatusCode::NO_CONTENT)
}
