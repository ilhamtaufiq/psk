-- Daftar berkas yang wajib ada dalam sebuah proyek (checklist kelengkapan).
CREATE TABLE project_requirements (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_requirements_project ON project_requirements(project_id, created_at);

-- Berkas bisa dikaitkan ke satu item daftar. Jika item dihapus, berkasnya tetap ada (tanpa kaitan).
ALTER TABLE project_files
    ADD COLUMN requirement_id TEXT REFERENCES project_requirements(id) ON DELETE SET NULL;

CREATE INDEX idx_files_requirement ON project_files(requirement_id);
