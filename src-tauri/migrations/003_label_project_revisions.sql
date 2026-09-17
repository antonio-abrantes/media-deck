ALTER TABLE label_projects ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE label_projects ADD COLUMN thumbnail_width INTEGER;
ALTER TABLE label_projects ADD COLUMN thumbnail_height INTEGER;
ALTER TABLE label_projects ADD COLUMN thumbnail_mime_type TEXT;
ALTER TABLE label_projects ADD COLUMN thumbnail_updated_at TEXT;

CREATE INDEX IF NOT EXISTS idx_label_projects_updated
    ON label_projects (updated_at DESC, id ASC);
