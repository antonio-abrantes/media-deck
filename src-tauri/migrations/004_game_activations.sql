-- MediaDeck — Migration 004: separate discovered catalog from activated collection.

CREATE TABLE IF NOT EXISTS game_activations (
    game_id            TEXT PRIMARY KEY NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    profile_id         TEXT NOT NULL REFERENCES launch_profiles (id) ON DELETE CASCADE,
    last_export_kind   TEXT NOT NULL CHECK (last_export_kind IN ('ini_file', 'floppy', 'optical')),
    last_media_key     TEXT NOT NULL CHECK (length(last_media_key) BETWEEN 1 AND 64),
    last_content_hash  TEXT NOT NULL CHECK (
        length(last_content_hash) = 64
        AND last_content_hash NOT GLOB '*[^0-9a-f]*'
    ),
    schema_version     INTEGER NOT NULL,
    export_count       INTEGER NOT NULL DEFAULT 1 CHECK (export_count > 0),
    first_activated_at TEXT NOT NULL,
    last_exported_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_game_activations_exported_at
    ON game_activations (last_exported_at);

-- Existing verified physical media is reliable evidence of activation.
WITH verified AS (
    SELECT
        p.game_id,
        m.profile_id,
        CASE
            WHEN m.media_kind = 'optical' THEN 'optical'
            ELSE 'floppy'
        END AS export_kind,
        m.media_key,
        m.content_hash,
        m.schema_version,
        MIN(m.created_at) OVER (PARTITION BY p.game_id) AS first_activated_at,
        m.last_verified_at AS last_exported_at,
        COUNT(*) OVER (PARTITION BY p.game_id) AS export_count,
        ROW_NUMBER() OVER (
            PARTITION BY p.game_id
            ORDER BY m.last_verified_at DESC, m.id DESC
        ) AS position
    FROM media m
    JOIN launch_profiles p ON p.id = m.profile_id
    WHERE m.last_verified_at IS NOT NULL
)
INSERT OR IGNORE INTO game_activations (
    game_id,
    profile_id,
    last_export_kind,
    last_media_key,
    last_content_hash,
    schema_version,
    export_count,
    first_activated_at,
    last_exported_at
)
SELECT
    game_id,
    profile_id,
    export_kind,
    media_key,
    content_hash,
    schema_version,
    export_count,
    first_activated_at,
    last_exported_at
FROM verified
WHERE position = 1;
