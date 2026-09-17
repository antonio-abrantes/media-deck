//! Pure domain entities for MediaDeck.
//!
//! Aggregate fields remain serializable for persistence, so repository and
//! application boundaries must call the provided validation methods.
//! No infrastructure dependency (no SQLite, no Tauri, no Win32) is allowed here.

use crate::domain::errors::DomainError;
use crate::domain::ids::{IdGenerator, MediaDeckId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ─── Typed ID newtypes ────────────────────────────────────────────────────────
// Each aggregate root has its own ID newtype so IDs are never confused at
// the type level (e.g. passing a SessionId where a GameId is expected).

macro_rules! id_newtype {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub MediaDeckId);

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl $name {
            pub fn new() -> Self {
                Self(MediaDeckId::new())
            }

            pub fn generate_with(generator: &dyn IdGenerator) -> Self {
                Self(MediaDeckId::generate_with(generator))
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.parse()?))
            }
        }
    };
}

id_newtype!(GameId, "Stable identifier for a [`Game`] aggregate.");
id_newtype!(ProfileId, "Stable identifier for a [`LaunchProfile`].");
id_newtype!(MediaId, "Stable identifier for a [`MediaDescriptor`].");
id_newtype!(DeviceId, "Stable identifier for a [`MediaDevice`].");
id_newtype!(SessionId, "Stable identifier for a [`GameSession`].");
id_newtype!(
    CorrelationId,
    "Identifier linking one operation across logs and errors."
);
id_newtype!(ArtworkId, "Stable identifier for an artwork record.");
id_newtype!(LabelProjectId, "Stable identifier for a label project.");

// ─── Value objects ────────────────────────────────────────────────────────────

/// The human-readable key written to the physical media (MEDIA_ID field in GAME.INI).
///
/// Validation rules (SECURITY.md §3):
/// - must not contain `\`, `/`, `:`, `..`, or ASCII control characters;
/// - must not be empty;
/// - max 64 characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MediaKey(String);

impl MediaKey {
    /// Validate and construct a `MediaKey`.
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let s: String = raw.into();
        if s.is_empty() || s.len() > 64 {
            return Err(DomainError::MediaProfileInvalid);
        }
        let allowed = s
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'));
        if !allowed || s.contains("..") {
            return Err(DomainError::MediaProfileInvalid);
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for MediaKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Lowercase SHA-256 digest of canonical `GAME.INI` bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentHash(String);

impl ContentHash {
    pub fn new(raw: impl Into<String>) -> Result<Self, DomainError> {
        let value = raw.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(DomainError::MediaProfileInvalid);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// How a game should be launched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LaunchKind {
    /// Launch via Steam using a numeric AppID.
    /// The AppID is validated to be > 0 at construction time.
    Steam { app_id: u64 },
    /// Launch an executable registered locally by the user.
    /// The executable path is stored in the DB; only the profile ID travels.
    Executable,
}

/// Policy for closing a game when the physical media is removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClosePolicy {
    /// Request graceful shutdown, wait up to `timeout_secs`, then ask.
    WaitAndAsk { timeout_secs: u32 },
    /// Request graceful shutdown, force after timeout without prompting.
    ForceAfterTimeout { timeout_secs: u32 },
    /// Detach the session without attempting to close the process.
    Detach,
}

impl Default for ClosePolicy {
    /// Default: wait 15 s then ask the user.
    fn default() -> Self {
        ClosePolicy::WaitAndAsk { timeout_secs: 15 }
    }
}

/// Where the game metadata originated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameProvider {
    Steam,
    Executable,
}

impl std::fmt::Display for GameProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GameProvider::Steam => write!(f, "steam"),
            GameProvider::Executable => write!(f, "executable"),
        }
    }
}

/// Physical media kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Floppy,
    Optical,
    Removable,
}

/// Drive type as returned by the OS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveType {
    Removable,
    CdRom,
    Fixed,
    Remote,
    RamDisk,
    Unknown,
}

/// Monitor policy for a [`MediaDevice`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorPolicy {
    /// Only this specific device instance may start a session.
    ExactDevice,
    /// Any optical device can start a session (used for CD/DVD without stable instance ID).
    AnyOptical,
    /// This device is disabled and will be ignored.
    Disabled,
}

// ─── Aggregates ───────────────────────────────────────────────────────────────

/// A game entry in the local library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: GameId,
    pub provider: GameProvider,
    /// Provider-specific identifier (Steam AppID as string, etc.).
    pub provider_game_id: Option<String>,
    pub display_name: String,
    /// Normalised lowercase name for sorting.
    pub sort_name: String,
    pub install_dir: Option<String>,
    pub installed: bool,
    /// Arbitrary provider metadata (not used for execution decisions).
    pub metadata_json: serde_json::Value,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Requested depth for a local provider scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryScanMode {
    /// Read every manifest in every configured library.
    Full,
    /// Ignore manifests whose filesystem timestamp is not newer than this value.
    Incremental { since: DateTime<Utc> },
}

/// A provider scan result. Unavailable libraries are deliberately kept separate:
/// their games have unknown state and must not be reconciled as uninstalled.
#[derive(Debug, Clone)]
pub struct LibraryScanSnapshot {
    pub games: Vec<Game>,
    pub configured_libraries: Vec<String>,
    pub scanned_libraries: Vec<String>,
    pub unavailable_libraries: Vec<String>,
}

/// Summary returned after a scan has been persisted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LibraryScanReport {
    pub discovered: usize,
    pub inserted: usize,
    pub updated: usize,
    pub marked_uninstalled: usize,
    pub unavailable_libraries: usize,
}

impl Game {
    /// Construct a minimal game entry.
    pub fn new(
        provider: GameProvider,
        provider_game_id: Option<String>,
        display_name: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Self {
        let display_name = display_name.into();
        let sort_name = display_name.to_lowercase();
        Self {
            id: GameId::new(),
            provider,
            provider_game_id,
            display_name,
            sort_name,
            install_dir: None,
            installed: false,
            metadata_json: serde_json::Value::Null,
            source_updated_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// A configured launch profile for a game.
///
/// The executable path and arguments are stored locally; they never originate
/// from the physical media.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchProfile {
    pub id: ProfileId,
    pub game_id: GameId,
    pub name: String,
    pub launch_kind: LaunchKind,
    /// Zero-based Steam launch option. `None` uses `steam://run/<app_id>`.
    pub steam_launch_option: Option<u8>,
    /// Absolute, canonicalised path — set by file picker, stored in DB.
    pub executable_path: Option<String>,
    pub working_directory: Option<String>,
    /// Arguments defined locally by the user (never from media).
    pub arguments: Vec<String>,
    /// Expected process names/paths for association heuristics.
    pub process_hints: Vec<String>,
    /// Override for this profile; `None` inherits global setting.
    pub close_policy_override: Option<ClosePolicy>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LaunchProfile {
    pub fn new(
        game_id: GameId,
        name: impl Into<String>,
        launch_kind: LaunchKind,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: ProfileId::new(),
            game_id,
            name: name.into(),
            launch_kind,
            steam_launch_option: None,
            executable_path: None,
            working_directory: None,
            arguments: Vec::new(),
            process_hints: Vec::new(),
            close_policy_override: None,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// Configure a local executable target. The application layer must first
    /// obtain and canonicalise the path through a trusted Windows file picker.
    pub fn with_executable_target(
        mut self,
        executable_path: impl Into<String>,
        working_directory: Option<String>,
        arguments: Vec<String>,
        process_hints: Vec<String>,
    ) -> Result<Self, DomainError> {
        if self.launch_kind != LaunchKind::Executable {
            return Err(DomainError::LaunchProfileInvalid);
        }
        let executable_path = executable_path.into();
        let process_hints = if process_hints.is_empty() {
            executable_path
                .rsplit('\\')
                .next()
                .map(str::to_owned)
                .into_iter()
                .collect()
        } else {
            process_hints
        };
        self.executable_path = Some(executable_path);
        self.working_directory = working_directory;
        self.arguments = arguments;
        self.process_hints = process_hints;
        self.validate()?;
        Ok(self)
    }

    /// Validate provider-specific invariants without touching the filesystem.
    ///
    /// Existence, file identity and drive type are revalidated by the Windows
    /// adapter immediately before launch.
    pub fn validate(&self) -> Result<(), DomainError> {
        let name_length = self.name.trim().chars().count();
        if name_length == 0
            || name_length > 80
            || self.name.chars().any(char::is_control)
            || self.arguments.len() > 64
            || self.arguments.iter().any(|argument| {
                argument.chars().count() > 4096 || argument.chars().any(char::is_control)
            })
            || self.process_hints.len() > 32
            || self
                .process_hints
                .iter()
                .any(|hint| !is_valid_local_process_hint(hint))
            || matches!(
                self.close_policy_override,
                Some(ClosePolicy::WaitAndAsk { timeout_secs })
                    | Some(ClosePolicy::ForceAfterTimeout { timeout_secs })
                    if !(1..=300).contains(&timeout_secs)
            )
        {
            return Err(DomainError::LaunchProfileInvalid);
        }

        match self.launch_kind {
            LaunchKind::Steam { app_id } => {
                if app_id == 0
                    || self.steam_launch_option.is_some_and(|option| option > 31)
                    || self.executable_path.is_some()
                    || self.working_directory.is_some()
                    || !self.arguments.is_empty()
                {
                    return Err(DomainError::LaunchProfileInvalid);
                }
            }
            LaunchKind::Executable => {
                if self.steam_launch_option.is_some() {
                    return Err(DomainError::LaunchProfileInvalid);
                }
                let executable_path = self
                    .executable_path
                    .as_deref()
                    .ok_or(DomainError::LaunchProfileInvalid)?;
                if !is_safe_local_windows_path(executable_path, true)
                    || self
                        .working_directory
                        .as_deref()
                        .is_some_and(|path| !is_safe_local_windows_path(path, false))
                {
                    return Err(DomainError::LaunchProfileInvalid);
                }
            }
        }
        Ok(())
    }
}

fn is_safe_local_windows_path(raw: &str, require_executable: bool) -> bool {
    let bytes = raw.as_bytes();
    if raw.starts_with(r"\\")
        || bytes.len() < 3
        || bytes.len() > 32_767
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
        || raw.contains('/')
        || raw[2..].contains(':')
        || raw.chars().any(char::is_control)
        || raw.contains('"')
    {
        return false;
    }

    let components = raw[3..].split('\\').collect::<Vec<_>>();
    if components.iter().any(|component| {
        matches!(*component, "." | "..")
            || (component.is_empty() && raw.len() > 3)
            || component.chars().count() > 255
            || component.ends_with('.')
            || component.ends_with(' ')
            || is_windows_reserved_name(component)
    }) {
        return false;
    }

    !require_executable || raw.to_ascii_lowercase().ends_with(".exe")
}

fn is_windows_reserved_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches(' ')
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn is_valid_local_process_hint(hint: &str) -> bool {
    if hint.is_empty() || hint.chars().any(char::is_control) {
        return false;
    }
    if hint.contains(['\\', '/', ':']) {
        return is_safe_local_windows_path(hint, true);
    }
    hint.chars().count() <= 260
        && hint.is_ascii()
        && !hint.contains("..")
        && !hint.contains([';', '"'])
        && hint.to_ascii_lowercase().ends_with(".exe")
}

/// Describes a physical media unit that has been written by MediaDeck.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaDescriptor {
    pub id: MediaId,
    /// The MEDIA_ID value on the physical media (validated `MediaKey`).
    pub media_key: MediaKey,
    pub profile_id: ProfileId,
    pub media_kind: MediaKind,
    pub last_device_id: Option<DeviceId>,
    pub schema_version: u32,
    /// SHA-256 of the normalised GAME.INI content for deduplication/diagnostics.
    pub content_hash: ContentHash,
    pub volume_serial: Option<String>,
    pub last_drive: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub status: MediaStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaStatus {
    Active,
    Damaged,
    Replaced,
    Missing,
}

/// How the most recent verified GAME.INI was exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    IniFile,
    Floppy,
    Optical,
}

impl ExportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IniFile => "ini_file",
            Self::Floppy => "floppy",
            Self::Optical => "optical",
        }
    }

    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "ini_file" => Ok(Self::IniFile),
            "floppy" => Ok(Self::Floppy),
            "optical" => Ok(Self::Optical),
            _ => Err(DomainError::DatabaseOperationFailed(
                "invalid game activation export kind".into(),
            )),
        }
    }
}

/// Verified membership fact backing the user's personal game collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameActivation {
    pub game_id: GameId,
    pub profile_id: ProfileId,
    pub last_export_kind: ExportKind,
    pub last_media_key: MediaKey,
    pub last_content_hash: ContentHash,
    pub schema_version: u32,
    pub export_count: u32,
    pub first_activated_at: DateTime<Utc>,
    pub last_exported_at: DateTime<Utc>,
}

/// A physical drive that the user has explicitly configured for monitoring.
///
/// The `current_mount_point` (drive letter) is mutable and never used as identity.
/// Identity comes from `device_instance_id` or `interface_path`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaDevice {
    pub id: DeviceId,
    /// Windows device instance ID (stable across reboots when available).
    pub device_instance_id: Option<String>,
    /// Normalised interface path (fallback identity).
    pub interface_path: Option<String>,
    pub friendly_name: String,
    pub drive_type: DriveType,
    /// Current drive letter (e.g. `"A:"`, `"G:"`). Mutable; not identity.
    pub current_mount_point: Option<String>,
    /// Capabilities: read, write, erase, etc.
    pub capabilities: serde_json::Value,
    pub monitor_policy: MonitorPolicy,
    /// Whether this device is in the user's allow-list.
    pub enabled: bool,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MediaDevice {
    pub fn new(
        friendly_name: impl Into<String>,
        drive_type: DriveType,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            id: DeviceId::new(),
            device_instance_id: None,
            interface_path: None,
            friendly_name: friendly_name.into(),
            drive_type,
            current_mount_point: None,
            capabilities: serde_json::Value::Object(Default::default()),
            monitor_policy: MonitorPolicy::ExactDevice,
            enabled: false,
            last_seen_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn media_key_accepts_valid_keys() {
        assert!(MediaKey::new("NR-001").is_ok());
        assert!(MediaKey::new("CP77-ALPHA").is_ok());
        assert!(MediaKey::new("A").is_ok());
        assert!(MediaKey::new("A".repeat(64)).is_ok());
    }

    #[test]
    fn media_key_rejects_empty() {
        assert_eq!(MediaKey::new(""), Err(DomainError::MediaProfileInvalid));
    }

    #[test]
    fn media_key_rejects_too_long() {
        assert_eq!(
            MediaKey::new("A".repeat(65)),
            Err(DomainError::MediaProfileInvalid)
        );
    }

    #[test]
    fn media_key_rejects_path_traversal() {
        assert!(MediaKey::new("..").is_err());
        assert!(MediaKey::new("foo/../bar").is_err());
        assert!(MediaKey::new("C:\\key").is_err());
        assert!(MediaKey::new("/etc/passwd").is_err());
        assert!(MediaKey::new("key:value").is_err());
    }

    #[test]
    fn media_key_rejects_control_characters() {
        assert!(MediaKey::new("key\x00bad").is_err());
        assert!(MediaKey::new("key\nbad").is_err());
        assert!(MediaKey::new("key\tbad").is_err());
    }

    #[test]
    fn media_key_rejects_non_ascii_and_spaces() {
        assert!(MediaKey::new("jogo ação").is_err());
        assert!(MediaKey::new("jogo-ação").is_err());
    }

    #[test]
    fn content_hash_requires_lowercase_sha256() {
        assert!(ContentHash::new("a".repeat(64)).is_ok());
        assert!(ContentHash::new("A".repeat(64)).is_err());
        assert!(ContentHash::new("abc123").is_err());
    }

    #[test]
    fn game_new_sets_sort_name() {
        let g = Game::new(
            GameProvider::Steam,
            Some("440".into()),
            "Team Fortress 2",
            Utc::now(),
        );
        assert_eq!(g.sort_name, "team fortress 2");
    }

    #[test]
    fn typed_ids_are_distinct_types() {
        let gid = GameId::new();
        let sid = SessionId::new();
        // This test exists to document that mixing GameId/SessionId is a compile
        // error. We just verify runtime construction works.
        assert_ne!(gid.to_string(), sid.to_string()); // extremely unlikely to collide
    }

    #[test]
    fn close_policy_default_is_wait_and_ask_15s() {
        assert_eq!(
            ClosePolicy::default(),
            ClosePolicy::WaitAndAsk { timeout_secs: 15 }
        );
    }

    #[test]
    fn media_device_new_is_disabled_by_default() {
        let d = MediaDevice::new("USB Floppy", DriveType::Removable, Utc::now());
        assert!(!d.enabled);
        assert_eq!(d.monitor_policy, MonitorPolicy::ExactDevice);
    }

    #[test]
    fn executable_profile_accepts_local_exe_arguments_and_hints() {
        let now = Utc::now();
        let profile = LaunchProfile::new(
            GameId::new(),
            "NFS Underground",
            LaunchKind::Executable,
            now,
        )
        .with_executable_target(
            r"D:\Games\NFSU\Speed.exe",
            Some(r"D:\Games\NFSU".to_owned()),
            vec!["-windowed".to_owned(), "-lang=en".to_owned()],
            Vec::new(),
        )
        .expect("valid executable profile");

        assert_eq!(
            profile.executable_path.as_deref(),
            Some(r"D:\Games\NFSU\Speed.exe")
        );
        assert_eq!(profile.arguments.len(), 2);
        assert_eq!(profile.process_hints, ["Speed.exe"]);
    }

    #[test]
    fn executable_profile_rejects_missing_unc_or_traversing_target() {
        let now = Utc::now();
        let empty = LaunchProfile::new(GameId::new(), "Missing", LaunchKind::Executable, now);
        assert_eq!(empty.validate(), Err(DomainError::LaunchProfileInvalid));

        for path in [
            r"\\server\games\game.exe",
            r"D:\Games\..\Windows\bad.exe",
            r"D:\Games\script.bat",
            r"D:\Games\game.exe:stream.exe",
            r"D:\Games\CON\game.exe",
            r"D:\Games\NUL.exe",
        ] {
            let result = LaunchProfile::new(GameId::new(), "Unsafe", LaunchKind::Executable, now)
                .with_executable_target(path, None, Vec::new(), vec!["game.exe".to_owned()]);
            assert!(matches!(result, Err(DomainError::LaunchProfileInvalid)));
        }
    }

    #[test]
    fn steam_profile_rejects_local_executable_fields() {
        let now = Utc::now();
        let result = LaunchProfile::new(
            GameId::new(),
            "Steam",
            LaunchKind::Steam { app_id: 620 },
            now,
        )
        .with_executable_target(
            r"D:\Games\Portal2\portal2.exe",
            None,
            Vec::new(),
            vec!["portal2.exe".to_owned()],
        );
        assert!(matches!(result, Err(DomainError::LaunchProfileInvalid)));
    }

    #[test]
    fn launch_profile_rejects_close_timeout_outside_global_bounds() {
        let now = Utc::now();
        for timeout_secs in [0, 301] {
            let mut profile = LaunchProfile::new(
                GameId::new(),
                "Steam",
                LaunchKind::Steam { app_id: 620 },
                now,
            );
            profile.close_policy_override = Some(ClosePolicy::WaitAndAsk { timeout_secs });
            assert_eq!(profile.validate(), Err(DomainError::LaunchProfileInvalid));
        }
    }
}
