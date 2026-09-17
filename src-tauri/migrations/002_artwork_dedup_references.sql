-- One content-addressed blob may be referenced by multiple games or artwork roles.
DROP INDEX IF EXISTS idx_artwork_sha256;
CREATE INDEX IF NOT EXISTS idx_artwork_sha256 ON artwork (sha256);
CREATE UNIQUE INDEX IF NOT EXISTS idx_artwork_game_kind_sha256
    ON artwork (game_id, kind, sha256);
