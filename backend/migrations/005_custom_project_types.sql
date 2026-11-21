-- Jenis proyek kini milik tiap pengguna dan bisa ditambah, diubah, dan dihapus.
-- Tabel projects dibangun ulang untuk menghapus CHECK bawaan pada project_type.
-- Migrasi ini dijalankan dengan foreign_keys mati (lihat main.rs), agar DROP TABLE tidak
-- memicu ON DELETE CASCADE pada berkas dan kelengkapan.

CREATE TABLE project_types (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    slug TEXT NOT NULL,
    label TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_project_types_owner_slug ON project_types(owner_id, slug);

WITH defaults(slug, label) AS (VALUES
    ('administrasi', 'Administrasi'),
    ('konstruksi', 'Konstruksi'),
    ('lainnya', 'Lainnya'),
    ('operasional', 'Operasional'),
    ('pemasaran', 'Pemasaran'),
    ('riset', 'Riset & Pengembangan'),
    ('teknologi', 'Teknologi / IT'))
INSERT INTO project_types (id, owner_id, slug, label, created_at)
SELECT lower(hex(randomblob(16))), u.id, d.slug, d.label, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
FROM users u CROSS JOIN defaults d;

CREATE TABLE projects_new (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'planning'
        CHECK (status IN ('planning', 'active', 'on_hold', 'completed')),
    project_type TEXT NOT NULL DEFAULT 'lainnya',
    start_date TEXT,
    due_date TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO projects_new (id, owner_id, name, description, status, project_type, start_date, due_date, created_at, updated_at)
SELECT id, owner_id, name, description, status, project_type, start_date, due_date, created_at, updated_at
FROM projects;

DROP TABLE projects;
ALTER TABLE projects_new RENAME TO projects;

CREATE INDEX idx_projects_owner ON projects(owner_id, updated_at);
CREATE INDEX idx_projects_owner_type ON projects(owner_id, project_type);

