//! Limited, deterministic parser and canonical writer for `GAME.INI`.

use crate::domain::entities::{
    ArtworkId, Game, GameProvider as CatalogProvider, LaunchKind, LaunchProfile, MediaKey,
    MediaKind, ProfileId,
};
use crate::domain::errors::DomainError;
use crate::domain::ids::IdGenerator;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MAX_PROFILE_BYTES: usize = 16 * 1024;
const MAX_DISPLAY_NAME_CHARS: usize = 160;
const MAX_PROCESS_HINTS: usize = 32;
const MAX_PROCESS_HINT_CHARS: usize = 260;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSourceVersion {
    V1,
    V2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaProvider {
    Steam,
    Executable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaProfile {
    pub media_id: MediaKey,
    pub media_kind: Option<MediaKind>,
    pub profile_id: ProfileId,
    pub created_at: Option<DateTime<Utc>>,
    pub provider: MediaProvider,
    pub app_id: Option<u64>,
    pub display_name: String,
    pub process_hints: Vec<String>,
    pub cover_artwork_id: Option<ArtworkId>,
    pub cover_cache_path: Option<String>,
    /// Filename-only hint imported from v1. Never interpreted as a path.
    pub artwork_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProfileErrorCode {
    TooLarge,
    InvalidUtf8,
    ContainsNul,
    ControlCharacter,
    MalformedLine,
    DuplicateSection,
    DuplicateKey,
    MissingSection,
    MissingField,
    UnsupportedSchema,
    InvalidField,
    ForbiddenField,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileParseError {
    pub code: ProfileErrorCode,
    pub line: Option<usize>,
    pub field: Option<String>,
}

impl ProfileParseError {
    fn new(code: ProfileErrorCode, line: Option<usize>, field: Option<&str>) -> Self {
        Self {
            code,
            line,
            field: field.map(str::to_owned),
        }
    }
}

impl fmt::Display for ProfileParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "GAME.INI validation failed: {:?}", self.code)
    }
}

impl std::error::Error for ProfileParseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProfileWarningCode {
    UnknownSectionIgnored,
    UnknownFieldIgnored,
    LegacyV1Imported,
    LegacyCoverIgnored,
    LegacyMediaIdGenerated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileWarning {
    pub code: ProfileWarningCode,
    pub line: Option<usize>,
    pub field: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedMediaProfile {
    pub source_version: ProfileSourceVersion,
    pub profile: MediaProfile,
    pub warnings: Vec<ProfileWarning>,
}

#[derive(Debug)]
struct Entry {
    value: String,
    line: usize,
}

#[derive(Debug)]
struct Section {
    line: usize,
    entries: BTreeMap<String, Entry>,
}

type IniDocument = BTreeMap<String, Section>;

pub fn parse_profile(
    bytes: &[u8],
    id_generator: &dyn IdGenerator,
) -> Result<ParsedMediaProfile, ProfileParseError> {
    if bytes.len() > MAX_PROFILE_BYTES {
        return Err(ProfileParseError::new(
            ProfileErrorCode::TooLarge,
            None,
            None,
        ));
    }
    if bytes.contains(&0) {
        return Err(ProfileParseError::new(
            ProfileErrorCode::ContainsNul,
            None,
            None,
        ));
    }

    let decoded = std::str::from_utf8(bytes)
        .map_err(|_| ProfileParseError::new(ProfileErrorCode::InvalidUtf8, None, None))?;
    let text = decoded.strip_prefix('\u{feff}').unwrap_or(decoded);
    let document = parse_ini(text)?;

    if document.contains_key("MEDIA") {
        parse_v2(&document)
    } else {
        parse_v1(&document, id_generator)
    }
}

pub fn write_canonical_v2(profile: &MediaProfile) -> Result<String, ProfileParseError> {
    validate_profile(profile)?;

    let mut lines = vec![
        "[MEDIA]".to_owned(),
        "SCHEMA=2".to_owned(),
        format!("MEDIA_ID={}", profile.media_id),
    ];
    if let Some(kind) = &profile.media_kind {
        lines.push(format!("MEDIA_KIND={}", media_kind_name(kind)));
    }
    lines.push(format!("PROFILE_ID={}", profile.profile_id));
    if let Some(created_at) = profile.created_at {
        lines.push(format!(
            "CREATED_AT={}",
            created_at.to_rfc3339_opts(SecondsFormat::Secs, true)
        ));
    }
    lines.push(String::new());
    lines.push("[GAME]".to_owned());
    lines.push(format!("PROVIDER={}", provider_name(profile.provider)));
    if let Some(app_id) = profile.app_id {
        lines.push(format!("APP_ID={app_id}"));
    }
    lines.push(format!("DISPLAY_NAME={}", profile.display_name));
    if !profile.process_hints.is_empty() {
        lines.push(format!("PROCESS_HINTS={}", profile.process_hints.join(";")));
    }
    if let (Some(id), Some(path)) = (&profile.cover_artwork_id, &profile.cover_cache_path) {
        lines.push(String::new());
        lines.push("[ARTWORK]".to_owned());
        lines.push(format!("COVER_ARTWORK_ID={id}"));
        lines.push(format!("COVER_CACHE_PATH={path}"));
    }
    Ok(format!("{}\r\n", lines.join("\r\n")))
}

/// Build the portable v2 profile generated by Media Creator from trusted local
/// catalog data. Executable paths, working directories and arguments remain in
/// the local launch profile and are deliberately not copied to the media.
pub fn build_v2_profile(
    game: &Game,
    launch_profile: &LaunchProfile,
    media_id: MediaKey,
    media_kind: Option<MediaKind>,
    created_at: DateTime<Utc>,
) -> Result<MediaProfile, DomainError> {
    launch_profile.validate()?;
    if !game.installed || !launch_profile.enabled || launch_profile.game_id != game.id {
        return Err(DomainError::LaunchProfileInvalid);
    }

    let (provider, app_id) = match (&game.provider, &launch_profile.launch_kind) {
        (CatalogProvider::Steam, LaunchKind::Steam { app_id }) => {
            let expected = app_id.to_string();
            if game.provider_game_id.as_deref() != Some(expected.as_str()) {
                return Err(DomainError::LaunchProfileInvalid);
            }
            (MediaProvider::Steam, Some(*app_id))
        }
        (CatalogProvider::Executable, LaunchKind::Executable)
            if game.provider_game_id.is_none() =>
        {
            (MediaProvider::Executable, None)
        }
        _ => return Err(DomainError::LaunchProfileInvalid),
    };

    let mut seen_hints = BTreeSet::new();
    let process_hints = launch_profile
        .process_hints
        .iter()
        .map(|hint| {
            hint.rsplit(['\\', '/'])
                .next()
                .filter(|name| valid_process_hint(name))
                .map(str::to_owned)
                .ok_or(DomainError::LaunchProfileInvalid)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|hint| seen_hints.insert(hint.to_ascii_lowercase()))
        .collect();

    let profile = MediaProfile {
        media_id,
        media_kind,
        profile_id: launch_profile.id,
        created_at: Some(created_at),
        provider,
        app_id,
        display_name: game.display_name.clone(),
        process_hints,
        cover_artwork_id: None,
        cover_cache_path: None,
        artwork_hint: None,
    };
    validate_profile(&profile).map_err(|_| DomainError::MediaProfileInvalid)?;
    Ok(profile)
}

fn parse_ini(text: &str) -> Result<IniDocument, ProfileParseError> {
    let mut document = IniDocument::new();
    let mut current_section: Option<String> = None;

    for (index, raw_line) in text.lines().enumerate() {
        let line_number = index + 1;
        if raw_line
            .chars()
            .any(|character| character.is_control() && character != '\t')
        {
            return Err(ProfileParseError::new(
                ProfileErrorCode::ControlCharacter,
                Some(line_number),
                None,
            ));
        }
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') || line.len() < 3 {
                return Err(ProfileParseError::new(
                    ProfileErrorCode::MalformedLine,
                    Some(line_number),
                    None,
                ));
            }
            let section_name = line[1..line.len() - 1].trim().to_ascii_uppercase();
            if section_name.is_empty()
                || section_name.chars().any(char::is_whitespace)
                || !section_name.is_ascii()
            {
                return Err(ProfileParseError::new(
                    ProfileErrorCode::MalformedLine,
                    Some(line_number),
                    None,
                ));
            }
            if document.contains_key(&section_name) {
                return Err(ProfileParseError::new(
                    ProfileErrorCode::DuplicateSection,
                    Some(line_number),
                    Some(&section_name),
                ));
            }
            document.insert(
                section_name.clone(),
                Section {
                    line: line_number,
                    entries: BTreeMap::new(),
                },
            );
            current_section = Some(section_name);
            continue;
        }

        let (raw_key, raw_value) = line.split_once('=').ok_or_else(|| {
            ProfileParseError::new(ProfileErrorCode::MalformedLine, Some(line_number), None)
        })?;
        let key = raw_key.trim().to_ascii_uppercase();
        if key.is_empty() || key.chars().any(char::is_whitespace) || !key.is_ascii() {
            return Err(ProfileParseError::new(
                ProfileErrorCode::MalformedLine,
                Some(line_number),
                None,
            ));
        }
        if is_forbidden_key(&key) {
            return Err(ProfileParseError::new(
                ProfileErrorCode::ForbiddenField,
                Some(line_number),
                Some(&key),
            ));
        }
        let section_name = current_section.as_ref().ok_or_else(|| {
            ProfileParseError::new(
                ProfileErrorCode::MalformedLine,
                Some(line_number),
                Some(&key),
            )
        })?;
        let section = document.get_mut(section_name).ok_or_else(|| {
            ProfileParseError::new(
                ProfileErrorCode::MalformedLine,
                Some(line_number),
                Some(&key),
            )
        })?;
        if section.entries.contains_key(&key) {
            return Err(ProfileParseError::new(
                ProfileErrorCode::DuplicateKey,
                Some(line_number),
                Some(&key),
            ));
        }
        section.entries.insert(
            key,
            Entry {
                value: raw_value.trim().to_owned(),
                line: line_number,
            },
        );
    }
    Ok(document)
}

fn parse_v2(document: &IniDocument) -> Result<ParsedMediaProfile, ProfileParseError> {
    let media = required_section(document, "MEDIA")?;
    let game = required_section(document, "GAME")?;
    let schema = required_entry(media, "SCHEMA")?;
    if schema.value != "2" {
        return Err(ProfileParseError::new(
            ProfileErrorCode::UnsupportedSchema,
            Some(schema.line),
            Some("SCHEMA"),
        ));
    }

    let media_id_entry = required_entry(media, "MEDIA_ID")?;
    let media_id = MediaKey::new(&media_id_entry.value).map_err(|_| {
        ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(media_id_entry.line),
            Some("MEDIA_ID"),
        )
    })?;
    let profile_id_entry = required_entry(media, "PROFILE_ID")?;
    let profile_id: ProfileId = profile_id_entry.value.parse().map_err(|_| {
        ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(profile_id_entry.line),
            Some("PROFILE_ID"),
        )
    })?;
    if profile_id.to_string() != profile_id_entry.value
        || profile_id.0.as_uuid().get_version_num() != 7
    {
        return Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(profile_id_entry.line),
            Some("PROFILE_ID"),
        ));
    }

    let provider_entry = required_entry(game, "PROVIDER")?;
    let provider = parse_provider(provider_entry)?;
    let app_id = match provider {
        MediaProvider::Steam => Some(parse_app_id(required_entry(game, "APP_ID")?)?),
        MediaProvider::Executable => {
            if let Some(entry) = game.entries.get("APP_ID") {
                return Err(ProfileParseError::new(
                    ProfileErrorCode::InvalidField,
                    Some(entry.line),
                    Some("APP_ID"),
                ));
            }
            None
        }
    };
    let display_name = parse_display_name(required_entry(game, "DISPLAY_NAME")?)?;
    let process_hints = game
        .entries
        .get("PROCESS_HINTS")
        .map(parse_process_hints)
        .transpose()?
        .unwrap_or_default();
    let media_kind = media
        .entries
        .get("MEDIA_KIND")
        .map(parse_media_kind)
        .transpose()?;
    let created_at = media
        .entries
        .get("CREATED_AT")
        .map(parse_created_at)
        .transpose()?;
    let (cover_artwork_id, cover_cache_path) = match document.get("ARTWORK") {
        Some(artwork) => {
            let id_entry = required_entry(artwork, "COVER_ARTWORK_ID")?;
            let id: ArtworkId = id_entry.value.parse().map_err(|_| {
                ProfileParseError::new(
                    ProfileErrorCode::InvalidField,
                    Some(id_entry.line),
                    Some("COVER_ARTWORK_ID"),
                )
            })?;
            let path_entry = required_entry(artwork, "COVER_CACHE_PATH")?;
            validate_cover_cache_path(&path_entry.value, Some(path_entry.line))?;
            (Some(id), Some(path_entry.value.clone()))
        }
        None => (None, None),
    };

    let warnings = collect_unknown_warnings(
        document,
        &[
            (
                "MEDIA",
                &[
                    "SCHEMA",
                    "MEDIA_ID",
                    "MEDIA_KIND",
                    "PROFILE_ID",
                    "CREATED_AT",
                ],
            ),
            (
                "GAME",
                &["PROVIDER", "APP_ID", "DISPLAY_NAME", "PROCESS_HINTS"],
            ),
            ("ARTWORK", &["COVER_ARTWORK_ID", "COVER_CACHE_PATH"]),
        ],
    );
    let profile = MediaProfile {
        media_id,
        media_kind,
        profile_id,
        created_at,
        provider,
        app_id,
        display_name,
        process_hints,
        cover_artwork_id,
        cover_cache_path,
        artwork_hint: None,
    };
    validate_profile(&profile)?;
    Ok(ParsedMediaProfile {
        source_version: ProfileSourceVersion::V2,
        profile,
        warnings,
    })
}

fn parse_v1(
    document: &IniDocument,
    id_generator: &dyn IdGenerator,
) -> Result<ParsedMediaProfile, ProfileParseError> {
    let game = required_section(document, "GAME")?;
    let app_id = parse_app_id(required_entry(game, "STEAMID")?)?;
    let display_name = parse_display_name(required_entry(game, "NAME")?)?;
    let process_hints = vec![parse_v1_process_hint(required_entry(game, "PROCESS")?)?];
    let mut warnings = collect_unknown_warnings(
        document,
        &[("GAME", &["NAME", "STEAMID", "PROCESS", "COVER", "DISKID"])],
    );
    let media_id = game
        .entries
        .get("DISKID")
        .and_then(|entry| MediaKey::new(&entry.value).ok())
        .unwrap_or_else(|| {
            warnings.push(ProfileWarning {
                code: ProfileWarningCode::LegacyMediaIdGenerated,
                line: game.entries.get("DISKID").map(|entry| entry.line),
                field: Some("DISKID".to_owned()),
            });
            MediaKey::new(format!("LEGACY-STEAM-{app_id}"))
                .expect("generated legacy media ID is valid")
        });
    warnings.push(ProfileWarning {
        code: ProfileWarningCode::LegacyV1Imported,
        line: Some(game.line),
        field: None,
    });
    let artwork_hint = game
        .entries
        .get("COVER")
        .and_then(|entry| sanitize_artwork_hint(entry, &mut warnings));

    let profile = MediaProfile {
        media_id,
        media_kind: Some(MediaKind::Floppy),
        profile_id: ProfileId::generate_with(id_generator),
        created_at: None,
        provider: MediaProvider::Steam,
        app_id: Some(app_id),
        display_name,
        process_hints,
        cover_artwork_id: None,
        cover_cache_path: None,
        artwork_hint,
    };
    validate_profile(&profile)?;
    Ok(ParsedMediaProfile {
        source_version: ProfileSourceVersion::V1,
        profile,
        warnings,
    })
}

fn required_section<'a>(
    document: &'a IniDocument,
    name: &str,
) -> Result<&'a Section, ProfileParseError> {
    document
        .get(name)
        .ok_or_else(|| ProfileParseError::new(ProfileErrorCode::MissingSection, None, Some(name)))
}

fn required_entry<'a>(section: &'a Section, key: &str) -> Result<&'a Entry, ProfileParseError> {
    section.entries.get(key).ok_or_else(|| {
        ProfileParseError::new(
            ProfileErrorCode::MissingField,
            Some(section.line),
            Some(key),
        )
    })
}

fn parse_provider(entry: &Entry) -> Result<MediaProvider, ProfileParseError> {
    match entry.value.to_ascii_lowercase().as_str() {
        "steam" => Ok(MediaProvider::Steam),
        "executable" => Ok(MediaProvider::Executable),
        _ => Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(entry.line),
            Some("PROVIDER"),
        )),
    }
}

fn parse_app_id(entry: &Entry) -> Result<u64, ProfileParseError> {
    if entry.value.is_empty()
        || !entry.value.bytes().all(|byte| byte.is_ascii_digit())
        || entry.value.starts_with('0')
    {
        return Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(entry.line),
            Some("APP_ID"),
        ));
    }
    entry.value.parse::<u64>().map_err(|_| {
        ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(entry.line),
            Some("APP_ID"),
        )
    })
}

fn parse_display_name(entry: &Entry) -> Result<String, ProfileParseError> {
    let count = entry.value.chars().count();
    if count == 0 || count > MAX_DISPLAY_NAME_CHARS {
        return Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(entry.line),
            Some("DISPLAY_NAME"),
        ));
    }
    Ok(entry.value.clone())
}

fn parse_process_hints(entry: &Entry) -> Result<Vec<String>, ProfileParseError> {
    let raw_hints = entry.value.split(';').map(str::trim).collect::<Vec<_>>();
    if raw_hints.iter().any(|hint| hint.is_empty()) {
        return Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(entry.line),
            Some("PROCESS_HINTS"),
        ));
    }
    let hints = raw_hints.into_iter().map(str::to_owned).collect::<Vec<_>>();
    if hints.len() > MAX_PROCESS_HINTS || hints.iter().any(|hint| !valid_process_hint(hint)) {
        return Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(entry.line),
            Some("PROCESS_HINTS"),
        ));
    }
    Ok(hints)
}

fn parse_v1_process_hint(entry: &Entry) -> Result<String, ProfileParseError> {
    let mut hint = entry.value.trim().to_owned();
    if !hint.to_ascii_lowercase().ends_with(".exe") {
        hint.push_str(".exe");
    }
    if !valid_process_hint(&hint) {
        return Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(entry.line),
            Some("PROCESS"),
        ));
    }
    Ok(hint)
}

fn valid_process_hint(hint: &str) -> bool {
    !hint.is_empty()
        && hint.chars().count() <= MAX_PROCESS_HINT_CHARS
        && hint.is_ascii()
        && !hint.contains(['\\', '/', ':', ';'])
        && !hint.contains("..")
        && !hint.chars().any(char::is_control)
        && hint.to_ascii_lowercase().ends_with(".exe")
}

/// Windows process image name used for post-launch classification.
///
/// `PROCESS=Cyberpunk2077` and `PROCESS_HINTS=Cyberpunk2077.exe` both map to
/// `Cyberpunk2077`. This is a filter, not authorization to terminate by name.
pub fn process_image_name(hint: &str) -> &str {
    let name = hint.rsplit(['\\', '/']).next().unwrap_or(hint);
    if name.len() >= 4 && name[name.len() - 4..].eq_ignore_ascii_case(".exe") {
        &name[..name.len() - 4]
    } else {
        name
    }
}

fn parse_media_kind(entry: &Entry) -> Result<MediaKind, ProfileParseError> {
    match entry.value.to_ascii_lowercase().as_str() {
        "floppy" => Ok(MediaKind::Floppy),
        "optical" => Ok(MediaKind::Optical),
        "removable" => Ok(MediaKind::Removable),
        _ => Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(entry.line),
            Some("MEDIA_KIND"),
        )),
    }
}

fn parse_created_at(entry: &Entry) -> Result<DateTime<Utc>, ProfileParseError> {
    let parsed = DateTime::parse_from_rfc3339(&entry.value).map_err(|_| {
        ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(entry.line),
            Some("CREATED_AT"),
        )
    })?;
    if parsed.offset().local_minus_utc() != 0 {
        return Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            Some(entry.line),
            Some("CREATED_AT"),
        ));
    }
    Ok(parsed.with_timezone(&Utc))
}

fn sanitize_artwork_hint(entry: &Entry, warnings: &mut Vec<ProfileWarning>) -> Option<String> {
    let hint = entry.value.trim();
    let valid = !hint.is_empty()
        && hint.len() <= 255
        && hint
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && !hint.contains("..");
    if valid {
        Some(hint.to_owned())
    } else {
        warnings.push(ProfileWarning {
            code: ProfileWarningCode::LegacyCoverIgnored,
            line: Some(entry.line),
            field: Some("COVER".to_owned()),
        });
        None
    }
}

fn collect_unknown_warnings(
    document: &IniDocument,
    allowed: &[(&str, &[&str])],
) -> Vec<ProfileWarning> {
    let allowed_sections = allowed
        .iter()
        .map(|(section, _)| *section)
        .collect::<BTreeSet<_>>();
    let mut warnings = Vec::new();
    for (section_name, section) in document {
        if !allowed_sections.contains(section_name.as_str()) {
            warnings.push(ProfileWarning {
                code: ProfileWarningCode::UnknownSectionIgnored,
                line: Some(section.line),
                field: Some(section_name.clone()),
            });
            continue;
        }
        let allowed_keys = allowed
            .iter()
            .find(|(name, _)| name == section_name)
            .map(|(_, keys)| *keys)
            .unwrap_or_default();
        for (key, entry) in &section.entries {
            if !allowed_keys.contains(&key.as_str()) {
                warnings.push(ProfileWarning {
                    code: ProfileWarningCode::UnknownFieldIgnored,
                    line: Some(entry.line),
                    field: Some(key.clone()),
                });
            }
        }
    }
    warnings
}

fn validate_profile(profile: &MediaProfile) -> Result<(), ProfileParseError> {
    let display_count = profile.display_name.chars().count();
    if display_count == 0 || display_count > MAX_DISPLAY_NAME_CHARS {
        return Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            None,
            Some("DISPLAY_NAME"),
        ));
    }
    if profile.process_hints.len() > MAX_PROCESS_HINTS
        || profile
            .process_hints
            .iter()
            .any(|hint| !valid_process_hint(hint))
    {
        return Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            None,
            Some("PROCESS_HINTS"),
        ));
    }
    match (profile.provider, profile.app_id) {
        (MediaProvider::Steam, Some(app_id)) if app_id > 0 => {}
        (MediaProvider::Executable, None) => {}
        _ => {
            return Err(ProfileParseError::new(
                ProfileErrorCode::InvalidField,
                None,
                Some("APP_ID"),
            ))
        }
    }
    match (&profile.cover_artwork_id, &profile.cover_cache_path) {
        (None, None) => Ok(()),
        (Some(_), Some(path)) => validate_cover_cache_path(path, None),
        _ => Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            None,
            Some("COVER_ARTWORK_ID"),
        )),
    }
}

fn validate_cover_cache_path(value: &str, line: Option<usize>) -> Result<(), ProfileParseError> {
    let valid = !value.is_empty()
        && value.len() <= 255
        && !value.starts_with('/')
        && !value.contains('\\')
        && !value.contains(':')
        && !value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        && !value.chars().any(char::is_control);
    if valid {
        Ok(())
    } else {
        Err(ProfileParseError::new(
            ProfileErrorCode::InvalidField,
            line,
            Some("COVER_CACHE_PATH"),
        ))
    }
}

fn is_forbidden_key(key: &str) -> bool {
    matches!(
        key,
        "EXECUTABLE"
            | "EXECUTABLE_PATH"
            | "EXEC"
            | "PATH"
            | "COMMAND"
            | "CMD"
            | "RUN"
            | "LAUNCH"
            | "SHELL"
            | "ARGUMENTS"
            | "ARGS"
            | "URL"
            | "SCRIPT"
            | "POWERSHELL"
            | "WSCRIPT"
            | "VBSCRIPT"
            | "BAT"
            | "INVOKE"
            | "CALL"
            | "WORKING_DIRECTORY"
    )
}

fn provider_name(provider: MediaProvider) -> &'static str {
    match provider {
        MediaProvider::Steam => "steam",
        MediaProvider::Executable => "executable",
    }
}

fn media_kind_name(kind: &MediaKind) -> &'static str {
    match kind {
        MediaKind::Floppy => "floppy",
        MediaKind::Optical => "optical",
        MediaKind::Removable => "removable",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ids::MediaDeckId;
    use std::str::FromStr;

    struct FixedIdGenerator;

    impl IdGenerator for FixedIdGenerator {
        fn next_id(&self) -> MediaDeckId {
            MediaDeckId::from_str("01994a56-69d7-7ef4-a137-94808fa24131").expect("fixture UUID")
        }
    }

    const MINIMAL_V2: &str = "[MEDIA]\nSCHEMA=2\nMEDIA_ID=CP77-001\nPROFILE_ID=01994a56-69d7-7ef4-a137-94808fa24131\n[GAME]\nPROVIDER=steam\nAPP_ID=1091500\nDISPLAY_NAME=Cyberpunk 2077\n";

    #[test]
    fn parses_minimal_v2() {
        let parsed = parse_profile(MINIMAL_V2.as_bytes(), &FixedIdGenerator).unwrap();
        assert_eq!(parsed.source_version, ProfileSourceVersion::V2);
        assert_eq!(parsed.profile.media_id.as_str(), "CP77-001");
        assert_eq!(parsed.profile.app_id, Some(1_091_500));
        assert!(parsed.warnings.is_empty());
    }

    #[test]
    fn parses_complete_v2_with_bom_crlf_and_case_insensitive_names() {
        let input = "\u{feff}[media]\r\nschema=2\r\nmedia_id=CP77-CD-001\r\nmedia_kind=optical\r\nprofile_id=01994a56-69d7-7ef4-a137-94808fa24131\r\ncreated_at=2026-09-15T13:00:00Z\r\n\r\n[game]\r\nprovider=steam\r\napp_id=1091500\r\ndisplay_name=Cyberpunk 2077\r\nprocess_hints=Cyberpunk2077.exe; REDlauncher.exe\r\n";
        let parsed = parse_profile(input.as_bytes(), &FixedIdGenerator).unwrap();
        assert_eq!(parsed.profile.media_kind, Some(MediaKind::Optical));
        assert_eq!(parsed.profile.process_hints.len(), 2);
        assert_eq!(
            parsed.profile.created_at.unwrap().to_rfc3339(),
            "2026-09-15T13:00:00+00:00"
        );
    }

    #[test]
    fn imports_v1_and_generates_profile_id() {
        let input = "[GAME]\nNAME=CYBERPUNK 2077\nSTEAMID=1091500\nPROCESS=Cyberpunk2077\nCOVER=cyberpunk2077.png\nDISKID=CP77-001\n";
        let parsed = parse_profile(input.as_bytes(), &FixedIdGenerator).unwrap();
        assert_eq!(parsed.source_version, ProfileSourceVersion::V1);
        assert_eq!(
            parsed.profile.profile_id.to_string(),
            "01994a56-69d7-7ef4-a137-94808fa24131"
        );
        assert_eq!(parsed.profile.process_hints, ["Cyberpunk2077.exe"]);
        assert_eq!(
            parsed.profile.artwork_hint.as_deref(),
            Some("cyberpunk2077.png")
        );
        assert!(parsed
            .warnings
            .iter()
            .any(|warning| warning.code == ProfileWarningCode::LegacyV1Imported));
    }

    #[test]
    fn ignores_unsafe_v1_cover_path_with_warning() {
        let input =
            "[GAME]\nNAME=Game\nSTEAMID=10\nPROCESS=game\nCOVER=..\\secret.png\nDISKID=GAME-1\n";
        let parsed = parse_profile(input.as_bytes(), &FixedIdGenerator).unwrap();
        assert_eq!(parsed.profile.artwork_hint, None);
        assert!(parsed
            .warnings
            .iter()
            .any(|warning| warning.code == ProfileWarningCode::LegacyCoverIgnored));
    }

    #[test]
    fn v1_requires_process_and_generates_missing_or_invalid_media_id() {
        let missing_process = "[GAME]\nNAME=Game\nSTEAMID=10\nDISKID=GAME-1\n";
        let error = parse_profile(missing_process.as_bytes(), &FixedIdGenerator).unwrap_err();
        assert_eq!(error.code, ProfileErrorCode::MissingField);
        assert_eq!(error.field.as_deref(), Some("PROCESS"));

        for disk_id in ["", "DISKID=Legacy ID With Spaces\n"] {
            let input = format!("[GAME]\nNAME=Game\nSTEAMID=10\nPROCESS=game\n{disk_id}");
            let parsed = parse_profile(input.as_bytes(), &FixedIdGenerator).unwrap();
            assert_eq!(parsed.profile.media_id.as_str(), "LEGACY-STEAM-10");
            assert!(parsed
                .warnings
                .iter()
                .any(|warning| { warning.code == ProfileWarningCode::LegacyMediaIdGenerated }));
        }
    }

    #[test]
    fn original_tutorial_ini_and_spaced_process_names_are_imported() {
        let input = "[GAME]\nNAME=CYBERPUNK 2077\nSTEAMID=1091500\nPROCESS=Cyberpunk2077\nCOVER=cyberpunk2077.png\nDISKID=CP77-001\n";
        let parsed = parse_profile(input.as_bytes(), &FixedIdGenerator).unwrap();
        assert_eq!(parsed.source_version, ProfileSourceVersion::V1);
        assert_eq!(parsed.profile.display_name, "CYBERPUNK 2077");
        assert_eq!(parsed.profile.app_id, Some(1_091_500));
        assert_eq!(parsed.profile.media_id.as_str(), "CP77-001");
        assert_eq!(parsed.profile.process_hints, ["Cyberpunk2077.exe"]);
        assert_eq!(
            parsed.profile.artwork_hint.as_deref(),
            Some("cyberpunk2077.png")
        );
        assert_eq!(
            process_image_name(&parsed.profile.process_hints[0]),
            "Cyberpunk2077"
        );

        let spaced = "[GAME]\nNAME=Punch Lunch\nSTEAMID=10\nPROCESS=Punch Lunch\nDISKID=PL-001\n";
        let parsed = parse_profile(spaced.as_bytes(), &FixedIdGenerator).unwrap();
        assert_eq!(parsed.profile.process_hints, ["Punch Lunch.exe"]);
        assert_eq!(process_image_name("Punch Lunch.exe"), "Punch Lunch");
        assert_eq!(process_image_name(r"D:\Games\NFSU\Speed.EXE"), "Speed");
    }

    #[test]
    fn rejects_oversized_binary_and_invalid_utf8() {
        let oversized = vec![b'A'; MAX_PROFILE_BYTES + 1];
        assert_eq!(
            parse_profile(&oversized, &FixedIdGenerator)
                .unwrap_err()
                .code,
            ProfileErrorCode::TooLarge
        );
        assert_eq!(
            parse_profile(&[0xff], &FixedIdGenerator).unwrap_err().code,
            ProfileErrorCode::InvalidUtf8
        );
    }

    #[test]
    fn rejects_nul_controls_duplicates_and_bad_app_id() {
        let nul = MINIMAL_V2.replace("Cyberpunk", "Cyber\0punk");
        assert_eq!(
            parse_profile(nul.as_bytes(), &FixedIdGenerator)
                .unwrap_err()
                .code,
            ProfileErrorCode::ContainsNul
        );
        let control = MINIMAL_V2.replace("Cyberpunk", "Cyber\u{0007}punk");
        assert_eq!(
            parse_profile(control.as_bytes(), &FixedIdGenerator)
                .unwrap_err()
                .code,
            ProfileErrorCode::ControlCharacter
        );
        let duplicate = MINIMAL_V2.replace("SCHEMA=2", "SCHEMA=2\nSCHEMA=2");
        assert_eq!(
            parse_profile(duplicate.as_bytes(), &FixedIdGenerator)
                .unwrap_err()
                .code,
            ProfileErrorCode::DuplicateKey
        );
        let bad_app_id = MINIMAL_V2.replace("1091500", "steam://1091500");
        assert_eq!(
            parse_profile(bad_app_id.as_bytes(), &FixedIdGenerator)
                .unwrap_err()
                .field
                .as_deref(),
            Some("APP_ID")
        );
    }

    #[test]
    fn rejects_duplicate_sections_missing_fields_and_non_v7_profile_id() {
        let duplicate_section = format!("{MINIMAL_V2}[GAME]\n");
        assert_eq!(
            parse_profile(duplicate_section.as_bytes(), &FixedIdGenerator)
                .unwrap_err()
                .code,
            ProfileErrorCode::DuplicateSection
        );
        let missing_schema = MINIMAL_V2.replace("SCHEMA=2\n", "");
        let missing_error =
            parse_profile(missing_schema.as_bytes(), &FixedIdGenerator).unwrap_err();
        assert_eq!(missing_error.code, ProfileErrorCode::MissingField);
        assert_eq!(missing_error.field.as_deref(), Some("SCHEMA"));
        let uuid_v4 = MINIMAL_V2.replace(
            "01994a56-69d7-7ef4-a137-94808fa24131",
            "550e8400-e29b-41d4-a716-446655440000",
        );
        let id_error = parse_profile(uuid_v4.as_bytes(), &FixedIdGenerator).unwrap_err();
        assert_eq!(id_error.field.as_deref(), Some("PROFILE_ID"));
    }

    #[test]
    fn parses_executable_profile_without_execution_data() {
        let input = MINIMAL_V2.replace("PROVIDER=steam\nAPP_ID=1091500", "PROVIDER=executable");
        let parsed = parse_profile(input.as_bytes(), &FixedIdGenerator).unwrap();
        assert_eq!(parsed.profile.provider, MediaProvider::Executable);
        assert_eq!(parsed.profile.app_id, None);
    }

    #[test]
    fn rejects_process_paths_and_warns_for_unknown_sections() {
        let path_hint = MINIMAL_V2.replace(
            "DISPLAY_NAME=Cyberpunk 2077",
            "DISPLAY_NAME=Cyberpunk 2077\nPROCESS_HINTS=C:\\Games\\game.exe",
        );
        let error = parse_profile(path_hint.as_bytes(), &FixedIdGenerator).unwrap_err();
        assert_eq!(error.field.as_deref(), Some("PROCESS_HINTS"));

        let unknown_section = format!("{MINIMAL_V2}[FUTURE]\nVALUE=1\n");
        let parsed = parse_profile(unknown_section.as_bytes(), &FixedIdGenerator).unwrap();
        assert!(parsed.warnings.iter().any(|warning| {
            warning.code == ProfileWarningCode::UnknownSectionIgnored
                && warning.field.as_deref() == Some("FUTURE")
        }));
    }

    #[test]
    fn rejects_forbidden_execution_fields_with_line_number() {
        let input = MINIMAL_V2.replace(
            "DISPLAY_NAME=Cyberpunk 2077",
            "DISPLAY_NAME=Cyberpunk 2077\nCOMMAND=calc.exe",
        );
        let error = parse_profile(input.as_bytes(), &FixedIdGenerator).unwrap_err();
        assert_eq!(error.code, ProfileErrorCode::ForbiddenField);
        assert_eq!(error.field.as_deref(), Some("COMMAND"));
        assert_eq!(error.line, Some(9));
    }

    #[test]
    fn rejects_whitespace_inside_keys_and_semicolons_inside_single_hints() {
        let disguised_key = MINIMAL_V2.replace(
            "DISPLAY_NAME=Cyberpunk 2077",
            "DISPLAY_NAME=Cyberpunk 2077\nCOM\tMAND=calc.exe",
        );
        assert_eq!(
            parse_profile(disguised_key.as_bytes(), &FixedIdGenerator)
                .unwrap_err()
                .code,
            ProfileErrorCode::MalformedLine
        );

        let mut profile = parse_profile(MINIMAL_V2.as_bytes(), &FixedIdGenerator)
            .unwrap()
            .profile;
        profile.process_hints = vec!["one.exe;two.exe".to_owned()];
        let error = write_canonical_v2(&profile).unwrap_err();
        assert_eq!(error.field.as_deref(), Some("PROCESS_HINTS"));

        let empty_hint = MINIMAL_V2.replace(
            "DISPLAY_NAME=Cyberpunk 2077",
            "DISPLAY_NAME=Cyberpunk 2077\nPROCESS_HINTS=one.exe;;two.exe",
        );
        assert_eq!(
            parse_profile(empty_hint.as_bytes(), &FixedIdGenerator)
                .unwrap_err()
                .field
                .as_deref(),
            Some("PROCESS_HINTS")
        );
    }

    #[test]
    fn unknown_fields_are_warnings_without_echoing_values() {
        let input = MINIMAL_V2.replace("SCHEMA=2", "SCHEMA=2\nFUTURE_FIELD=secret");
        let parsed = parse_profile(input.as_bytes(), &FixedIdGenerator).unwrap();
        assert_eq!(parsed.warnings.len(), 1);
        assert_eq!(
            parsed.warnings[0].code,
            ProfileWarningCode::UnknownFieldIgnored
        );
        assert_eq!(parsed.warnings[0].field.as_deref(), Some("FUTURE_FIELD"));
    }

    #[test]
    fn canonical_writer_is_deterministic_and_round_trips() {
        let profile = parse_profile(MINIMAL_V2.as_bytes(), &FixedIdGenerator)
            .unwrap()
            .profile;
        let first = write_canonical_v2(&profile).unwrap();
        let second = write_canonical_v2(&profile).unwrap();
        assert_eq!(first, second);
        assert!(first.contains("\r\n"));
        assert!(!first.replace("\r\n", "").contains('\n'));

        let reparsed = parse_profile(first.as_bytes(), &FixedIdGenerator).unwrap();
        assert_eq!(reparsed.profile, profile);
    }

    #[test]
    fn canonical_writer_round_trips_portable_cover_reference() {
        let mut profile = parse_profile(MINIMAL_V2.as_bytes(), &FixedIdGenerator)
            .unwrap()
            .profile;
        profile.cover_artwork_id = Some("01994a56-69d7-7ef4-a137-94808fa24139".parse().unwrap());
        profile.cover_cache_path = Some("ab/abcdef.webp".into());
        let ini = write_canonical_v2(&profile).unwrap();
        assert!(ini.contains("[ARTWORK]\r\n"));
        assert!(ini.contains("COVER_CACHE_PATH=ab/abcdef.webp"));
        assert_eq!(
            parse_profile(ini.as_bytes(), &FixedIdGenerator)
                .unwrap()
                .profile,
            profile
        );
    }

    #[test]
    fn media_creator_builds_executable_ini_without_local_launch_secrets() {
        let now = Utc::now();
        let mut game = Game::new(
            CatalogProvider::Executable,
            None,
            "Need for Speed Underground",
            now,
        );
        game.installed = true;
        game.install_dir = Some(r"D:\Games\NFSU".to_owned());
        let launch_profile = LaunchProfile::new(game.id, "Speed.exe", LaunchKind::Executable, now)
            .with_executable_target(
                r"D:\Games\NFSU\Speed.exe",
                Some(r"D:\Games\NFSU".to_owned()),
                vec!["-windowed".to_owned()],
                vec![r"D:\Games\NFSU\Speed.exe".to_owned()],
            )
            .unwrap();

        let media_profile = build_v2_profile(
            &game,
            &launch_profile,
            MediaKey::new("NFSU-001").unwrap(),
            Some(MediaKind::Floppy),
            now,
        )
        .unwrap();
        let ini = write_canonical_v2(&media_profile).unwrap();

        assert!(ini.contains("PROVIDER=executable"));
        assert!(ini.contains("PROCESS_HINTS=Speed.exe"));
        assert!(!ini.contains(r"D:\Games"));
        assert!(!ini.contains("-windowed"));
        assert!(!ini.contains("EXECUTABLE_PATH"));
        assert!(!ini.contains("ARGUMENTS"));
    }
}
