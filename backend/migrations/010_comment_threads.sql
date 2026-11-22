-- Komentar bertingkat (balasan), tombol hapus yang meninggalkan tombstone, dan vote.
ALTER TABLE project_comments ADD COLUMN parent_id TEXT REFERENCES project_comments(id) ON DELETE CASCADE;
ALTER TABLE project_comments ADD COLUMN deleted_at TEXT;

CREATE INDEX idx_comments_parent ON project_comments(parent_id);

CREATE TABLE comment_votes (
    comment_id TEXT NOT NULL REFERENCES project_comments(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    value INTEGER NOT NULL CHECK (value IN (-1, 1)),
    PRIMARY KEY (comment_id, user_id)
);
