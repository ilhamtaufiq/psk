-- Lampiran pada tiket. Berkasnya di disk: <upload_dir>/<project>/tickets/<tiket>/<id>.
CREATE TABLE ticket_attachments (
    id TEXT PRIMARY KEY,
    ticket_id TEXT NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    original_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_ticket_att_ticket ON ticket_attachments(ticket_id);
