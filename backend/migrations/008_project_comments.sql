-- Komentar dan masukan pengguna pada project. Viewer boleh menulis; penghapusan lihat comments.rs.
CREATE TABLE project_comments (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_comments_project ON project_comments(project_id, created_at);
