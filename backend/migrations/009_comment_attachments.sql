-- Lampiran pada komentar. Berkasnya disimpan di disk: <upload_dir>/<project>/comments/<komentar>/<id>.
CREATE TABLE comment_attachments (
    id TEXT PRIMARY KEY,
    comment_id TEXT NOT NULL REFERENCES project_comments(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    original_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_comment_att_comment ON comment_attachments(comment_id);
