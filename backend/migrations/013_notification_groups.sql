-- Notifikasi yang dikelompokkan (mis. jumlah suka pada satu komentar) memakai kunci ref_key.
ALTER TABLE notifications ADD COLUMN ref_key TEXT;
CREATE INDEX idx_notifications_ref ON notifications(user_id, ref_key);
