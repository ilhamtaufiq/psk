-- Jenis proyek, dipakai untuk pengelompokan, filter, dan pengurutan.
ALTER TABLE projects
    ADD COLUMN project_type TEXT NOT NULL DEFAULT 'lainnya'
        CHECK (project_type IN ('administrasi', 'konstruksi', 'lainnya', 'operasional', 'pemasaran', 'riset', 'teknologi'));

CREATE INDEX idx_projects_owner_type ON projects(owner_id, project_type);
