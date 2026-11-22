-- Satu project bisa punya lebih dari satu jenis. projects.project_type tetap dipakai
-- sebagai jenis utama (untuk pengurutan); daftar lengkapnya ada di tabel ini.
CREATE TABLE project_type_links (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    type_slug TEXT NOT NULL,
    PRIMARY KEY (project_id, type_slug)
);

CREATE INDEX idx_type_links_slug ON project_type_links(type_slug);

INSERT INTO project_type_links (project_id, type_slug)
    SELECT id, project_type FROM projects WHERE project_type IS NOT NULL AND project_type <> '';

-- Pemilik project: nama pemilik atau klien, diisi bebas.
ALTER TABLE projects ADD COLUMN owner_name TEXT NOT NULL DEFAULT '';
