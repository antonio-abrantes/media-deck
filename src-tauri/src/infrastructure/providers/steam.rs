//! Offline-first Steam adapter: registry discovery, bounded VDF parsing and launch.

use crate::domain::entities::{
    Game, GameProvider as GameProviderKind, LaunchKind, LaunchProfile, LibraryScanMode,
    LibraryScanSnapshot,
};
use crate::domain::errors::DomainError;
use crate::domain::ports::{GameProvider, LocalLibrarySource, PortResult};
use chrono::{DateTime, TimeZone, Utc};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};
use winreg::RegKey;

const MAX_LIBRARY_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const MAX_VDF_DEPTH: usize = 16;
const MAX_VDF_ENTRIES: usize = 100_000;

#[derive(Debug, Default)]
pub struct SteamProvider {
    install_root: Option<PathBuf>,
}

impl SteamProvider {
    pub fn from_install_root(install_root: PathBuf) -> Self {
        Self {
            install_root: Some(install_root),
        }
    }

    /// Registry is authoritative. Known Program Files locations are only an
    /// explicit fallback and must contain both `steam.exe` and `steamapps`.
    pub fn detect_install_root() -> Result<PathBuf, DomainError> {
        registry_candidates()
            .into_iter()
            .chain(fallback_candidates())
            .find(|path| is_valid_install_root(path))
            .ok_or(DomainError::SteamNotFound)
    }

    fn resolved_root(&self) -> Result<PathBuf, DomainError> {
        self.install_root
            .clone()
            .map(Ok)
            .unwrap_or_else(Self::detect_install_root)
    }

    fn scan_sync(&self, mode: LibraryScanMode) -> Result<LibraryScanSnapshot, DomainError> {
        let root = self.resolved_root()?;
        if !root.is_dir() {
            return Err(DomainError::SteamNotFound);
        }
        let library_file = root.join("steamapps").join("libraryfolders.vdf");
        let mut libraries = if library_file.is_file() {
            parse_library_folders_file(&library_file)?
        } else {
            Vec::new()
        };
        libraries.push(root);
        deduplicate_paths(&mut libraries);

        let configured_libraries = libraries.iter().map(|path| path_string(path)).collect();
        let mut scanned_libraries = Vec::new();
        let mut unavailable_libraries = Vec::new();
        let mut games = Vec::new();
        for library in libraries {
            let library_label = path_string(&library);
            let steamapps = library.join("steamapps");
            let entries = match fs::read_dir(&steamapps) {
                Ok(entries) => entries,
                Err(_) => {
                    unavailable_libraries.push(library_label);
                    continue;
                }
            };
            scanned_libraries.push(library_label.clone());
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                    continue;
                }
                let modified = entry
                    .metadata()
                    .ok()
                    .and_then(|metadata| metadata.modified().ok())
                    .map(DateTime::<Utc>::from);
                if matches!(
                    mode,
                    LibraryScanMode::Incremental { since }
                        if modified.is_some_and(|value| value <= since)
                ) {
                    continue;
                }
                match parse_app_manifest_file(&path, &library, modified) {
                    Ok(game) => games.push(game),
                    Err(error) => tracing::warn!(
                        manifest = %name,
                        error_code = error.code(),
                        "ignored invalid Steam manifest"
                    ),
                }
            }
        }
        Ok(LibraryScanSnapshot {
            games,
            configured_libraries,
            scanned_libraries,
            unavailable_libraries,
        })
    }

    fn launch_sync(profile: &LaunchProfile) -> Result<(), DomainError> {
        profile.validate()?;
        let LaunchKind::Steam { app_id } = profile.launch_kind else {
            return Err(DomainError::LaunchProfileInvalid);
        };
        if app_id == 0 {
            return Err(DomainError::LaunchProfileInvalid);
        }
        let uri = HSTRING::from(steam_launch_uri(app_id, profile.steam_launch_option));
        let operation = HSTRING::from("open");
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(uri.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecute returns > 32 on success.
        if result.0 as isize <= 32 {
            tracing::error!(code = result.0 as isize, "ShellExecute steam URI failed");
            return Err(DomainError::LaunchFailed);
        }
        Ok(())
    }
}

fn steam_launch_uri(app_id: u64, option: Option<u8>) -> String {
    match option {
        Some(option) => format!("steam://launch/{app_id}/option{option}"),
        None => format!("steam://run/{app_id}"),
    }
}

impl GameProvider for SteamProvider {
    fn scan_local(&self) -> PortResult<'_, Vec<Game>> {
        Box::pin(async move {
            self.scan_sync(LibraryScanMode::Full)
                .map(|snapshot| snapshot.games)
        })
    }

    fn launch<'a>(&'a self, profile: &'a LaunchProfile) -> PortResult<'a, ()> {
        Box::pin(async move { Self::launch_sync(profile) })
    }
}

impl LocalLibrarySource for SteamProvider {
    fn scan<'a>(&'a self, mode: LibraryScanMode) -> PortResult<'a, LibraryScanSnapshot> {
        Box::pin(async move { self.scan_sync(mode) })
    }
}

fn registry_candidates() -> Vec<PathBuf> {
    let locations = [
        (HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
        (HKEY_CURRENT_USER, r"Software\Valve\Steam", "InstallPath"),
        (HKEY_LOCAL_MACHINE, r"Software\Valve\Steam", "InstallPath"),
        (HKEY_LOCAL_MACHINE, r"Software\Valve\Steam", "SteamPath"),
    ];
    let views = [
        KEY_READ,
        KEY_READ | KEY_WOW64_64KEY,
        KEY_READ | KEY_WOW64_32KEY,
    ];
    let mut paths = Vec::new();
    for (hive, key_path, value_name) in locations {
        let hive = RegKey::predef(hive);
        for view in views {
            if let Ok(key) = hive.open_subkey_with_flags(key_path, view) {
                if let Ok(value) = key.get_value::<String, _>(value_name) {
                    let trimmed = value.trim().trim_matches('"');
                    if !trimmed.is_empty() {
                        paths.push(PathBuf::from(trimmed));
                    }
                }
            }
        }
    }
    paths
}

fn fallback_candidates() -> impl Iterator<Item = PathBuf> {
    ["ProgramFiles(x86)", "ProgramFiles"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .map(|path| path.join("Steam"))
}

fn is_valid_install_root(path: &Path) -> bool {
    path.is_dir() && path.join("steam.exe").is_file() && path.join("steamapps").is_dir()
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn deduplicate_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(path_string(path).to_lowercase()));
}

fn read_bounded(path: &Path, max_bytes: u64) -> Result<String, DomainError> {
    let metadata = fs::metadata(path).map_err(|_| DomainError::SteamLibraryInvalid)?;
    if metadata.len() > max_bytes {
        return Err(DomainError::SteamLibraryInvalid);
    }
    fs::read_to_string(path).map_err(|_| DomainError::SteamLibraryInvalid)
}

/// Parse the legacy and current forms used by `libraryfolders.vdf`.
pub fn parse_library_folders_file(path: &Path) -> Result<Vec<PathBuf>, DomainError> {
    let content = read_bounded(path, MAX_LIBRARY_FILE_BYTES)?;
    let root = parse_key_values(&content)?;
    let folders = root
        .get_object("libraryfolders")
        .ok_or(DomainError::SteamLibraryInvalid)?;
    let mut result = Vec::new();
    for (key, value) in folders {
        if !key.chars().all(|character| character.is_ascii_digit()) {
            continue;
        }
        let path = match value {
            VdfValue::String(path) => Some(path.as_str()),
            VdfValue::Object(object) => object.get_string("path"),
        };
        if let Some(path) = path.filter(|path| !path.trim().is_empty()) {
            result.push(PathBuf::from(path));
        }
    }
    Ok(result)
}

/// Parse only the bounded AppState fields needed for local discovery.
pub fn parse_app_manifest_file(
    path: &Path,
    library_root: &Path,
    filesystem_modified: Option<DateTime<Utc>>,
) -> Result<Game, DomainError> {
    let content = read_bounded(path, MAX_MANIFEST_BYTES)?;
    let root = parse_key_values(&content)?;
    let state = root
        .get_object("AppState")
        .ok_or(DomainError::SteamLibraryInvalid)?;
    let app_id = state
        .get_string("appid")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .ok_or(DomainError::SteamLibraryInvalid)?;
    let name = state
        .get_string("name")
        .filter(|value| !value.trim().is_empty() && value.chars().count() <= 512)
        .ok_or(DomainError::SteamLibraryInvalid)?;
    let install_dir_name = state
        .get_string("installdir")
        .filter(|value| is_safe_install_dir_name(value))
        .ok_or(DomainError::SteamLibraryInvalid)?;
    let state_flags = state
        .get_string("StateFlags")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let source_updated_at = state
        .get_string("LastUpdated")
        .and_then(|value| value.parse::<i64>().ok())
        .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single())
        .or(filesystem_modified);
    let now = filesystem_modified.unwrap_or_else(Utc::now);
    let mut game = Game::new(GameProviderKind::Steam, Some(app_id.to_string()), name, now);
    game.install_dir = Some(path_string(
        &library_root
            .join("steamapps")
            .join("common")
            .join(install_dir_name),
    ));
    game.installed = state_flags & 4 != 0;
    game.source_updated_at = source_updated_at;
    game.metadata_json = serde_json::json!({
        "_steam_scan": {
            "source_library": path_string(library_root),
            "state_flags": state_flags
        }
    });
    Ok(game)
}

fn is_safe_install_dir_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.chars().count() <= 255
        && !value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':'))
}

#[derive(Debug)]
enum VdfValue {
    String(String),
    Object(BTreeMap<String, VdfValue>),
}

impl VdfValue {
    fn get_object(&self, key: &str) -> Option<&BTreeMap<String, VdfValue>> {
        let VdfValue::Object(values) = self else {
            return None;
        };
        values.iter().find_map(|(candidate, value)| {
            if candidate.eq_ignore_ascii_case(key) {
                match value {
                    VdfValue::Object(object) => Some(object),
                    VdfValue::String(_) => None,
                }
            } else {
                None
            }
        })
    }
}

trait VdfObjectExt {
    fn get_string(&self, key: &str) -> Option<&str>;
}

impl VdfObjectExt for BTreeMap<String, VdfValue> {
    fn get_string(&self, key: &str) -> Option<&str> {
        self.iter().find_map(|(candidate, value)| {
            if candidate.eq_ignore_ascii_case(key) {
                match value {
                    VdfValue::String(value) => Some(value.as_str()),
                    VdfValue::Object(_) => None,
                }
            } else {
                None
            }
        })
    }
}

fn parse_key_values(input: &str) -> Result<VdfValue, DomainError> {
    let mut parser = VdfParser {
        bytes: input.as_bytes(),
        position: 0,
        entries: 0,
    };
    let object = parser.parse_object(0, false)?;
    parser.skip_space_and_comments();
    if parser.position != parser.bytes.len() {
        return Err(DomainError::SteamLibraryInvalid);
    }
    Ok(VdfValue::Object(object))
}

struct VdfParser<'a> {
    bytes: &'a [u8],
    position: usize,
    entries: usize,
}

impl VdfParser<'_> {
    fn parse_object(
        &mut self,
        depth: usize,
        expects_closing_brace: bool,
    ) -> Result<BTreeMap<String, VdfValue>, DomainError> {
        if depth > MAX_VDF_DEPTH {
            return Err(DomainError::SteamLibraryInvalid);
        }
        let mut values = BTreeMap::new();
        loop {
            self.skip_space_and_comments();
            if self.peek() == Some(b'}') {
                if !expects_closing_brace {
                    return Err(DomainError::SteamLibraryInvalid);
                }
                self.position += 1;
                return Ok(values);
            }
            if self.peek().is_none() {
                return if expects_closing_brace {
                    Err(DomainError::SteamLibraryInvalid)
                } else {
                    Ok(values)
                };
            }
            let key = self.parse_string()?;
            self.skip_space_and_comments();
            let value = if self.peek() == Some(b'{') {
                self.position += 1;
                VdfValue::Object(self.parse_object(depth + 1, true)?)
            } else {
                VdfValue::String(self.parse_string()?)
            };
            self.entries += 1;
            if self.entries > MAX_VDF_ENTRIES {
                return Err(DomainError::SteamLibraryInvalid);
            }
            values.insert(key, value);
        }
    }

    fn parse_string(&mut self) -> Result<String, DomainError> {
        if self.peek() != Some(b'"') {
            return Err(DomainError::SteamLibraryInvalid);
        }
        self.position += 1;
        let mut result = Vec::new();
        while let Some(byte) = self.peek() {
            self.position += 1;
            match byte {
                b'"' => {
                    return String::from_utf8(result).map_err(|_| DomainError::SteamLibraryInvalid)
                }
                b'\\' => {
                    let escaped = self.peek().ok_or(DomainError::SteamLibraryInvalid)?;
                    self.position += 1;
                    match escaped {
                        b'\\' | b'"' => result.push(escaped),
                        _ => {
                            result.push(b'\\');
                            result.push(escaped);
                        }
                    }
                }
                0..=31 => return Err(DomainError::SteamLibraryInvalid),
                _ => result.push(byte),
            }
        }
        Err(DomainError::SteamLibraryInvalid)
    }

    fn skip_space_and_comments(&mut self) {
        loop {
            while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
                self.position += 1;
            }
            if self.peek() == Some(b'/') && self.bytes.get(self.position + 1) == Some(&b'/') {
                self.position += 2;
                while self.peek().is_some_and(|byte| byte != b'\n') {
                    self.position += 1;
                }
            } else {
                return;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::{GameId, LaunchKind};
    use chrono::Utc;
    use tempfile::tempdir;

    #[test]
    fn builds_only_allowlisted_steam_launch_uris() {
        assert_eq!(steam_launch_uri(851850, None), "steam://run/851850");
        assert_eq!(
            steam_launch_uri(851850, Some(1)),
            "steam://launch/851850/option1"
        );
    }

    #[tokio::test]
    async fn rejects_executable_profiles_and_zero_app_id() {
        let provider = SteamProvider::default();
        let executable =
            LaunchProfile::new(GameId::new(), "Local", LaunchKind::Executable, Utc::now());
        assert_eq!(
            provider.launch(&executable).await,
            Err(DomainError::LaunchProfileInvalid)
        );

        let zero = LaunchProfile::new(
            GameId::new(),
            "Zero",
            LaunchKind::Steam { app_id: 0 },
            Utc::now(),
        );
        assert_eq!(
            provider.launch(&zero).await,
            Err(DomainError::LaunchProfileInvalid)
        );
    }

    #[test]
    fn parses_modern_and_legacy_library_folders() {
        let directory = tempdir().expect("tempdir");
        let file = directory.path().join("libraryfolders.vdf");
        fs::write(
            &file,
            r#""libraryfolders"
{
  "0" "C:\\Program Files (x86)\\Steam"
  "1" { "path" "D:\\Jogos Steam" "label" "" }
  "contentstatsid" "123"
}"#,
        )
        .expect("fixture");
        let parsed = parse_library_folders_file(&file).expect("parse");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1], PathBuf::from(r"D:\Jogos Steam"));
    }

    #[test]
    fn fallback_root_requires_client_and_steamapps() {
        let directory = tempdir().expect("tempdir");
        assert!(!is_valid_install_root(directory.path()));
        fs::write(directory.path().join("steam.exe"), b"fixture").expect("client");
        assert!(!is_valid_install_root(directory.path()));
        fs::create_dir(directory.path().join("steamapps")).expect("steamapps");
        assert!(is_valid_install_root(directory.path()));
    }

    #[test]
    fn parses_manifest_without_real_steam() {
        let directory = tempdir().expect("tempdir");
        let file = directory.path().join("appmanifest_620.acf");
        fs::write(
            &file,
            r#""AppState"
{
  "appid" "620"
  "name" "Portal 2"
  "StateFlags" "4"
  "installdir" "Portal 2"
  "LastUpdated" "1704067200"
}"#,
        )
        .expect("fixture");
        let game =
            parse_app_manifest_file(&file, Path::new(r"D:\SteamLibrary"), None).expect("parse");
        assert_eq!(game.provider_game_id.as_deref(), Some("620"));
        assert_eq!(game.display_name, "Portal 2");
        assert!(game.installed);
        assert!(game
            .install_dir
            .as_deref()
            .is_some_and(|path| path.ends_with(r"steamapps\common\Portal 2")));
    }

    #[test]
    fn rejects_manifest_path_traversal_and_excessive_input() {
        let directory = tempdir().expect("tempdir");
        let traversal = directory.path().join("appmanifest_1.acf");
        fs::write(
            &traversal,
            r#""AppState" { "appid" "1" "name" "Bad" "installdir" "..\\escape" }"#,
        )
        .expect("fixture");
        assert!(matches!(
            parse_app_manifest_file(&traversal, directory.path(), None),
            Err(DomainError::SteamLibraryInvalid)
        ));

        let oversized = directory.path().join("libraryfolders.vdf");
        let file = fs::File::create(&oversized).expect("fixture");
        file.set_len(MAX_LIBRARY_FILE_BYTES + 1).expect("size");
        assert!(matches!(
            parse_library_folders_file(&oversized),
            Err(DomainError::SteamLibraryInvalid)
        ));
    }

    #[tokio::test]
    async fn full_and_incremental_scans_use_temporary_fixtures() {
        let directory = tempdir().expect("tempdir");
        let steamapps = directory.path().join("steamapps");
        fs::create_dir_all(&steamapps).expect("steamapps");
        fs::write(
            steamapps.join("appmanifest_440.acf"),
            r#""AppState" { "appid" "440" "name" "TF2" "StateFlags" "4" "installdir" "TF2" }"#,
        )
        .expect("manifest");
        let provider = SteamProvider::from_install_root(directory.path().to_path_buf());
        let full = provider.scan(LibraryScanMode::Full).await.expect("full");
        assert_eq!(full.games.len(), 1);
        let future = Utc::now() + chrono::Duration::hours(1);
        let incremental = provider
            .scan(LibraryScanMode::Incremental { since: future })
            .await
            .expect("incremental");
        assert!(incremental.games.is_empty());
    }
}
