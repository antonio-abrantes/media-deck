-- MediaDeck — Migration 001: Initial Schema
-- This migration is immutable once published. Never alter it; add 002 instead.
-- All IDs are UUID v7 stored as TEXT (hyphenated lowercase).
-- All timestamps are UTC ISO-8601 TEXT (e.g. "2026-09-15T13:00:00.000Z").

PRAGMA journal_mode = DELETE; -- WAL deferred until validated for local storage
PRAGMA foreign_keys = ON;

-- ─── Games ───────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS games (
    id                  TEXT PRIMARY KEY NOT NULL,
    provider            TEXT NOT NULL,              -- 'steam' | 'executable'
    provider_game_id    TEXT,                       -- AppID or external ID
    display_name        TEXT NOT NULL,
    sort_name           TEXT NOT NULL,              -- lowercase, for sorting
    install_dir         TEXT,
    installed           INTEGER NOT NULL DEFAULT 0, -- boolean (0/1)
    metadata_json       TEXT NOT NULL DEFAULT 'null',
    source_updated_at   TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

-- Partial unique index: one game per provider+id when the id is set.
CREATE UNIQUE INDEX IF NOT EXISTS idx_games_provider_id
    ON games (provider, provider_game_id)
    WHERE provider_game_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_games_sort_name ON games (sort_name);

-- ─── Launch profiles ─────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS launch_profiles (
    id                      TEXT PRIMARY KEY NOT NULL,
    game_id                 TEXT NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    name                    TEXT NOT NULL,
    launch_kind             TEXT NOT NULL,          -- 'steam' | 'executable'
    executable_path         TEXT,                   -- local only; never from media
    working_directory       TEXT,
    arguments_json          TEXT NOT NULL DEFAULT '[]',  -- validated local array
    process_hints_json      TEXT NOT NULL DEFAULT '[]',
    close_policy_override   TEXT,                   -- NULL → inherit global
    enabled                 INTEGER NOT NULL DEFAULT 1,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_profiles_game_id ON launch_profiles (game_id);

-- ─── Media devices ────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS media_devices (
    id                  TEXT PRIMARY KEY NOT NULL,
    device_instance_id  TEXT,     -- stable Windows identity when available
    interface_path      TEXT,     -- normalized interface path (fallback)
    friendly_name       TEXT NOT NULL,
    drive_type          TEXT NOT NULL,  -- 'removable' | 'cdrom' | 'fixed' | ...
    current_mount_point TEXT,          -- drive letter (mutable, NOT identity)
    capabilities_json   TEXT NOT NULL DEFAULT '{}',
    monitor_policy      TEXT NOT NULL DEFAULT 'exact_device',
    enabled             INTEGER NOT NULL DEFAULT 0,
    last_seen_at        TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

-- ─── Physical media ───────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS media (
    id                  TEXT PRIMARY KEY NOT NULL,
    media_key           TEXT NOT NULL UNIQUE,       -- MEDIA_ID on the physical media
    profile_id          TEXT NOT NULL REFERENCES launch_profiles (id),
    media_kind          TEXT NOT NULL,              -- 'floppy' | 'optical' | 'removable'
    last_device_id      TEXT REFERENCES media_devices (id),
    schema_version      INTEGER NOT NULL DEFAULT 2,
    content_hash        TEXT NOT NULL,              -- SHA-256 of normalised GAME.INI
    volume_serial       TEXT,                       -- diagnostics only, not identity
    last_drive          TEXT,                       -- last observed drive letter
    created_at          TEXT NOT NULL,
    last_seen_at        TEXT,
    last_verified_at    TEXT,
    status              TEXT NOT NULL DEFAULT 'active'  -- 'active'|'damaged'|'replaced'|'missing'
);

CREATE INDEX IF NOT EXISTS idx_media_profile_id ON media (profile_id);

-- ─── Artwork ──────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS artwork (
    id              TEXT PRIMARY KEY NOT NULL,
    game_id         TEXT NOT NULL REFERENCES games (id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,   -- 'launcher_cover'|'hero'|'logo'|'jewel_front'|'disc_label'|'icon'
    provider        TEXT NOT NULL,   -- 'local' | 'steam' | 'manual'
    relative_path   TEXT NOT NULL,   -- relative to artwork/ dir; no credentials
    source_url      TEXT,
    sha256          TEXT NOT NULL,
    width           INTEGER NOT NULL,
    height          INTEGER NOT NULL,
    mime_type       TEXT NOT NULL,
    is_user_override INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_artwork_game_id ON artwork (game_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_artwork_sha256 ON artwork (sha256);

-- ─── Label projects ───────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS label_projects (
    id              TEXT PRIMARY KEY NOT NULL,
    game_id         TEXT REFERENCES games (id) ON DELETE SET NULL,
    name            TEXT NOT NULL,
    preset_kind     TEXT NOT NULL,   -- 'floppy_label'|'cd_jewel_front'|'cd_disc_label'|'custom'
    width_mm        REAL NOT NULL,
    height_mm       REAL NOT NULL,
    shape_json      TEXT,            -- cutouts, circle, center hole
    bleed_mm        REAL NOT NULL DEFAULT 3.0,
    safe_margin_mm  REAL NOT NULL DEFAULT 3.0,
    dpi             INTEGER NOT NULL DEFAULT 300,
    scene_version   INTEGER NOT NULL DEFAULT 1,
    scene_json      TEXT NOT NULL DEFAULT '{}',
    thumbnail_path  TEXT,
    is_template     INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- ─── Game sessions ────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS game_sessions (
    id                  TEXT PRIMARY KEY NOT NULL,  -- correlation/session ID
    media_id            TEXT REFERENCES media (id),
    media_key           TEXT NOT NULL,              -- snapshot of MEDIA_ID at session start
    profile_id          TEXT NOT NULL,              -- snapshot
    state               TEXT NOT NULL DEFAULT 'idle',
    launch_requested_at TEXT,
    running_at          TEXT,
    media_removed_at    TEXT,
    closed_at           TEXT,
    close_result        TEXT,    -- 'graceful'|'forced'|'detached'|'not_bound'
    error_code          TEXT,    -- stable error code
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_media_id ON game_sessions (media_id);
CREATE INDEX IF NOT EXISTS idx_sessions_state    ON game_sessions (state);

-- ─── Session processes ────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS session_processes (
    session_id          TEXT NOT NULL REFERENCES game_sessions (id) ON DELETE CASCADE,
    pid                 INTEGER NOT NULL,
    created_time        TEXT NOT NULL,          -- part of process identity (FILETIME as ISO or hex)
    executable_path     TEXT NOT NULL,          -- canonicalised
    role                TEXT NOT NULL DEFAULT 'main',  -- 'main'|'launcher'|'child'|'anticheat'
    window_handle       TEXT,                   -- snapshot only, never sole identity
    observed_exit_at    TEXT,
    PRIMARY KEY (session_id, pid, created_time)
);

-- ─── Settings ─────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS settings (
    key         TEXT PRIMARY KEY NOT NULL,
    value_json  TEXT NOT NULL,   -- validated, non-secret value
    updated_at  TEXT NOT NULL
);

-- Seed known default settings so the app can read them without NULL checks.
INSERT OR IGNORE INTO settings (key, value_json, updated_at) VALUES
    ('monitor_active',        'true',      '2026-09-15T00:00:00Z'),
    ('autostart',             'false',     '2026-09-15T00:00:00Z'),
    ('presentation_duration', '"normal"',  '2026-09-15T00:00:00Z'),
    ('sound_enabled',         'false',     '2026-09-15T00:00:00Z'),
    ('reduce_motion',         'false',     '2026-09-15T00:00:00Z'),
    ('close_policy',          '"wait_and_ask"', '2026-09-15T00:00:00Z'),
    ('close_timeout_secs',    '15',        '2026-09-15T00:00:00Z'),
    ('log_level',             '"info"',    '2026-09-15T00:00:00Z'),
    ('session_retention_days','180',       '2026-09-15T00:00:00Z'),
    ('language',              '"pt-BR"',   '2026-09-15T00:00:00Z');
