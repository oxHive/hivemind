ALTER TABLE edges ADD COLUMN link_text TEXT;

CREATE INDEX IF NOT EXISTS idx_edges_target_id ON edges(target_id);
