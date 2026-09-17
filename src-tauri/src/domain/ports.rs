//! Port traits — the boundary between the domain and all infrastructure.
//!
//! The domain defines *what* it needs; infrastructure provides *how*.
//! No implementation lives here. Each trait is implemented by exactly one
//! production adapter and optionally a fake adapter for tests.
//!
//! All traits require `Send + Sync` so they can be held behind `Arc<dyn Trait>`
//! in async Tauri commands.

use crate::domain::artwork::{
    Artwork, ArtworkDownloadRequest, ArtworkKind, CachedArtwork, DownloadedArtwork,
};
use crate::domain::entities::{ArtworkId, LabelProjectId};
use crate::domain::entities::{
    DeviceId, Game, GameActivation, GameId, LaunchProfile, LibraryScanMode, LibraryScanReport,
    LibraryScanSnapshot, MediaDescriptor, MediaDevice, ProfileId, SessionId,
};
use crate::domain::errors::DomainError;
use crate::domain::label::LabelProject;
use std::future::Future;
use std::pin::Pin;

/// Convenience alias for boxed async results returned by port methods.
pub type PortResult<'a, T> = Pin<Box<dyn Future<Output = Result<T, DomainError>> + Send + 'a>>;

// ─── Device port ──────────────────────────────────────────────────────────────

/// Interacts with physical drives attached to the OS.
///
/// Implemented by `infrastructure::devices::Win32DeviceAdapter` in production
/// and `infrastructure::database::FakeDevicePort` in tests.
pub trait DevicePort: Send + Sync {
    /// Return all physical drives currently visible to the OS.
    fn list_drives(&self) -> PortResult<'_, Vec<MediaDevice>>;

    /// Inspect one drive using its current mount point.
    ///
    /// Implementations must derive identity and drive type from the OS; callers
    /// must never construct this answer from values supplied by physical media.
    fn inspect_drive<'a>(&'a self, mount_point: &'a str) -> PortResult<'a, MediaDevice>;

    /// Check whether the drive at `mount_point` is ready for reading.
    fn is_ready<'a>(&'a self, mount_point: &'a str) -> PortResult<'a, bool>;
}

// ─── Media writer port ────────────────────────────────────────────────────────

/// Writes and verifies a `GAME.INI` profile on physical media.
///
/// Implemented differently for floppy, optical, and staging adapters.
pub trait MediaWriter: Send + Sync {
    /// Write the profile to the media at `mount_point` atomically.
    /// Must verify after write and never report success if verification fails.
    fn write_profile<'a>(&'a self, mount_point: &'a str, content: &'a str) -> PortResult<'a, ()>;

    /// Read and return the raw profile content from `mount_point`.
    fn read_profile<'a>(&'a self, mount_point: &'a str) -> PortResult<'a, String>;
}

// ─── Game provider port ───────────────────────────────────────────────────────

/// Discovers and launches games through a specific platform (Steam, executable).
pub trait GameProvider: Send + Sync {
    /// Scan local installations and return discovered games.
    fn scan_local(&self) -> PortResult<'_, Vec<Game>>;

    /// Launch the game identified by the given profile.
    /// Must not accept free-form commands from the media.
    fn launch<'a>(&'a self, profile: &'a LaunchProfile) -> PortResult<'a, ()>;
}

/// Reads a provider's local library without using network services.
pub trait LocalLibrarySource: Send + Sync {
    fn scan<'a>(&'a self, mode: LibraryScanMode) -> PortResult<'a, LibraryScanSnapshot>;
}

// ─── Process port ─────────────────────────────────────────────────────────────

/// Observes and controls OS processes after a game is launched.
pub trait ProcessPort: Send + Sync {
    /// Snapshot all currently running process IDs and creation times.
    fn snapshot_baseline(&self) -> PortResult<'_, Vec<ProcessSnapshot>>;

    /// Observe the current process table (same shape as the baseline snapshot).
    fn list_processes(&self) -> PortResult<'_, Vec<ProcessSnapshot>>;

    /// Confirm that PID + creation time + path still identify the same process.
    fn revalidate<'a>(&'a self, snapshot: &'a ProcessSnapshot) -> PortResult<'a, bool>;

    /// Locate a top-level window owned by the process, when one exists.
    fn find_main_window<'a>(&'a self, snapshot: &'a ProcessSnapshot)
        -> PortResult<'a, Option<u64>>;

    /// List direct children of the process using the current process table.
    fn list_children<'a>(
        &'a self,
        snapshot: &'a ProcessSnapshot,
    ) -> PortResult<'a, Vec<ProcessSnapshot>>;

    /// Send WM_CLOSE to the main window of the process and wait for exit.
    fn request_close<'a>(
        &'a self,
        snapshot: &'a ProcessSnapshot,
        timeout_secs: u32,
    ) -> PortResult<'a, CloseResult>;

    /// Force-terminate the process. Requires the snapshot to match identity.
    fn force_kill<'a>(&'a self, snapshot: &'a ProcessSnapshot) -> PortResult<'a, ()>;
}

/// Stable identity for a tracked OS process.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProcessSnapshot {
    pub pid: u32,
    /// Process creation time as a Windows FILETIME (u64) for strong identity.
    pub creation_time: u64,
    /// Canonicalised executable path.
    pub executable_path: String,
}

/// Outcome of a graceful close request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloseResult {
    Exited,
    TimedOut,
    AlreadyGone,
}

// ─── Repository ports ─────────────────────────────────────────────────────────

/// Persists and retrieves [`Game`] aggregates.
pub trait GameRepository: Send + Sync {
    fn find_by_id<'a>(&'a self, id: &'a GameId) -> PortResult<'a, Option<Game>>;
    fn find_by_provider_id<'a>(
        &'a self,
        provider: &'a str,
        provider_id: &'a str,
    ) -> PortResult<'a, Option<Game>>;
    fn upsert<'a>(&'a self, game: &'a Game) -> PortResult<'a, ()>;
    fn list(&self, page: u32, per_page: u32) -> PortResult<'_, Vec<Game>>;
}

/// Persists and retrieves [`LaunchProfile`] aggregates.
pub trait ProfileRepository: Send + Sync {
    fn find_by_id<'a>(&'a self, id: &'a ProfileId) -> PortResult<'a, Option<LaunchProfile>>;
    fn find_by_game_id<'a>(&'a self, game_id: &'a GameId) -> PortResult<'a, Vec<LaunchProfile>>;
    fn upsert<'a>(&'a self, profile: &'a LaunchProfile) -> PortResult<'a, ()>;
}

/// Persists verified membership in the user's personal game collection.
pub trait ActivationRepository: Send + Sync {
    fn find_by_game_id<'a>(&'a self, game_id: &'a GameId)
        -> PortResult<'a, Option<GameActivation>>;
    fn list(&self) -> PortResult<'_, Vec<GameActivation>>;
    /// Insert the first activation or increment its persisted export count.
    /// Implementations preserve the original `first_activated_at`.
    fn upsert<'a>(&'a self, activation: &'a GameActivation) -> PortResult<'a, ()>;
    fn delete<'a>(&'a self, game_id: &'a GameId) -> PortResult<'a, bool>;
}

/// Atomically reconciles Steam discoveries into games and launch profiles.
pub trait SteamCatalogRepository: Send + Sync {
    fn reconcile<'a>(
        &'a self,
        snapshot: &'a LibraryScanSnapshot,
        mode: LibraryScanMode,
    ) -> PortResult<'a, LibraryScanReport>;
}

/// Persists and retrieves [`MediaDescriptor`] aggregates.
pub trait MediaRepository: Send + Sync {
    fn find_by_key<'a>(&'a self, key: &'a str) -> PortResult<'a, Option<MediaDescriptor>>;
    fn list(&self) -> PortResult<'_, Vec<MediaDescriptor>>;
    fn upsert<'a>(&'a self, media: &'a MediaDescriptor) -> PortResult<'a, ()>;
}

/// Persists and retrieves [`MediaDevice`] configurations.
pub trait DeviceRepository: Send + Sync {
    fn list_all(&self) -> PortResult<'_, Vec<MediaDevice>>;
    fn find_configured(&self) -> PortResult<'_, Vec<MediaDevice>>;
    fn find_by_id<'a>(&'a self, id: &'a DeviceId) -> PortResult<'a, Option<MediaDevice>>;
    fn upsert<'a>(&'a self, device: &'a MediaDevice) -> PortResult<'a, ()>;
    fn update_mount_point<'a>(
        &'a self,
        id: &'a DeviceId,
        mount_point: Option<&'a str>,
    ) -> PortResult<'a, ()>;
}

/// Persists and retrieves session records.
pub trait SessionRepository: Send + Sync {
    fn find_by_id<'a>(&'a self, id: &'a SessionId) -> PortResult<'a, Option<SessionRecord>>;
    fn upsert<'a>(&'a self, record: &'a SessionRecord) -> PortResult<'a, ()>;
    /// Sessions left in a live state across an unexpected app exit.
    fn find_interrupted(&self) -> PortResult<'_, Vec<SessionRecord>>;
    /// Replace the strongly identified processes bound to a session.
    fn replace_processes<'a>(
        &'a self,
        session_id: &'a SessionId,
        processes: &'a [TrackedProcessRecord],
    ) -> PortResult<'a, ()>;
}

/// A snapshot of a session for persistence (not the live state machine).
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: SessionId,
    pub media_key: String,
    pub profile_id: ProfileId,
    pub state_label: String,
    pub launch_requested_at: Option<String>,
    pub close_result: Option<String>,
}

/// Persisted process identity for a supervised session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedProcessRecord {
    pub pid: u32,
    pub creation_time: u64,
    pub executable_path: String,
    pub role: String,
    pub window_handle: Option<u64>,
}

/// Persists and retrieves typed application settings.
pub trait SettingsRepository: Send + Sync {
    fn get<'a>(&'a self, key: &'a str) -> PortResult<'a, Option<serde_json::Value>>;
    fn set<'a>(&'a self, key: &'a str, value: serde_json::Value) -> PortResult<'a, ()>;
}

/// Persists validated artwork metadata.
pub trait ArtworkRepository: Send + Sync {
    fn find_by_id<'a>(&'a self, id: &'a ArtworkId) -> PortResult<'a, Option<Artwork>>;
    fn find_by_game_kind_hash<'a>(
        &'a self,
        game_id: &'a GameId,
        kind: ArtworkKind,
        sha256: &'a str,
    ) -> PortResult<'a, Option<Artwork>>;
    fn list_for_game_kind<'a>(
        &'a self,
        game_id: &'a GameId,
        kind: ArtworkKind,
    ) -> PortResult<'a, Vec<Artwork>>;
    fn upsert<'a>(&'a self, artwork: &'a Artwork) -> PortResult<'a, ()>;
}

/// Persists validated, revisioned Label Studio projects.
pub trait LabelProjectRepository: Send + Sync {
    fn list(&self) -> PortResult<'_, Vec<LabelProject>>;
    fn find_by_id<'a>(&'a self, id: &'a LabelProjectId) -> PortResult<'a, Option<LabelProject>>;
    /// Insert or atomically replace when `expected_revision` matches.
    fn upsert<'a>(
        &'a self,
        project: &'a LabelProject,
        expected_revision: Option<u64>,
    ) -> PortResult<'a, ()>;
    /// Delete only when the current revision matches.
    fn delete<'a>(&'a self, id: &'a LabelProjectId, expected_revision: u64)
        -> PortResult<'a, bool>;
}

/// Validates, normalizes and stores image bytes under the artwork data root.
pub trait ArtworkBlobStore: Send + Sync {
    fn store<'a>(
        &'a self,
        bytes: &'a [u8],
        declared_mime: &'a str,
    ) -> PortResult<'a, CachedArtwork>;
    fn resolve<'a>(&'a self, relative_path: &'a str) -> PortResult<'a, String>;
}

/// Fetches remote artwork and metadata for a game.
pub trait ArtworkProvider: Send + Sync {
    /// Return a list of candidate artwork URLs for the given game.
    fn fetch_artwork_urls<'a>(&'a self, game_id: &'a GameId) -> PortResult<'a, Vec<String>>;
}

/// Optional HTTP capability. No production implementation is enabled by default.
pub trait ArtworkDownloader: Send + Sync {
    fn download<'a>(
        &'a self,
        request: &'a ArtworkDownloadRequest,
    ) -> PortResult<'a, DownloadedArtwork>;
}
