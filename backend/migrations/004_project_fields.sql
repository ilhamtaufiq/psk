-- Informasi detail proyek yang didefinisikan pengguna (mis. nama pemohon, alamat).
CREATE TABLE project_fields (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    field_type TEXT NOT NULL DEFAULT 'text'
        CHECK (field_type IN ('text', 'multiline', 'date', 'number', 'email', 'phone', 'url')),
    value TEXT NOT NULL DEFAULT '',
    position INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_fields_project ON project_fields(project_id, position);
