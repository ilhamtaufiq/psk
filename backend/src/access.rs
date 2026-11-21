use sqlx::SqlitePool;

use crate::error::AppError;

/// Peran pengguna pada sebuah proyek, diurutkan dari yang paling terbatas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer,
    Editor,
    Owner,
}

impl Role {
    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "viewer" => Some(Role::Viewer),
            "editor" => Some(Role::Editor),
            "owner" => Some(Role::Owner),
            _ => None,
        }
    }
}

/// Peran pengguna di proyek, atau None bila tidak punya akses sama sekali.
pub async fn role_of(pool: &SqlitePool, project_id: &str, user_id: &str) -> Result<Option<Role>, AppError> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT p.owner_id,
                (SELECT m.role FROM project_members m WHERE m.project_id = p.id AND m.user_id = ?)
         FROM projects p WHERE p.id = ?",
    )
    .bind(user_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        None => None,
        Some((owner, _)) if owner == user_id => Some(Role::Owner),
        Some((_, Some(role))) => Role::parse(&role),
        Some(_) => None,
    })
}

/// Memastikan peran minimal. Tanpa akses sama sekali: 404 (proyek dianggap tidak ada).
/// Akses ada tetapi kurang: 403.
pub async fn require(
    pool: &SqlitePool,
    project_id: &str,
    user_id: &str,
    min: Role,
) -> Result<Role, AppError> {
    match role_of(pool, project_id, user_id).await? {
        None => Err(AppError::NotFound("Proyek tidak ditemukan".into())),
        Some(role) if role >= min => Ok(role),
        Some(_) => Err(AppError::Forbidden(
            "Peran Anda di proyek ini tidak mengizinkan tindakan tersebut".into(),
        )),
    }
}
