use media_deck_lib::domain::ids::{IdGenerator, MediaDeckId};
use media_deck_lib::domain::media_profile::{
    parse_profile, ProfileErrorCode, ProfileSourceVersion, ProfileWarningCode,
};
use std::str::FromStr;

struct FixtureIdGenerator;

impl IdGenerator for FixtureIdGenerator {
    fn next_id(&self) -> MediaDeckId {
        MediaDeckId::from_str("01994a56-69d7-7ef4-a137-94808fa24131").unwrap()
    }
}

#[test]
fn valid_fixtures_parse_to_expected_versions() {
    let v2 = parse_profile(
        include_bytes!("fixtures/game_ini/valid/v2-minimal.ini"),
        &FixtureIdGenerator,
    )
    .unwrap();
    let v1 = parse_profile(
        include_bytes!("fixtures/game_ini/valid/v1-minimal.ini"),
        &FixtureIdGenerator,
    )
    .unwrap();

    assert_eq!(v2.source_version, ProfileSourceVersion::V2);
    assert_eq!(v1.source_version, ProfileSourceVersion::V1);
    assert_eq!(v1.profile.media_id.as_str(), "PORTAL2-001");
    assert_eq!(v1.profile.app_id, Some(620));
    assert_eq!(v1.profile.process_hints, ["portal2.exe"]);
    assert!(v1
        .warnings
        .iter()
        .any(|warning| warning.code == ProfileWarningCode::LegacyV1Imported));

    let kakarot = parse_profile(
        include_bytes!("fixtures/game_ini/valid/dragon-ball-z-kakarot/GAME.INI"),
        &FixtureIdGenerator,
    )
    .unwrap();
    assert_eq!(kakarot.source_version, ProfileSourceVersion::V2);
    assert_eq!(kakarot.profile.media_id.as_str(), "DBZK-CD-001");
    assert_eq!(kakarot.profile.app_id, Some(851_850));
    assert_eq!(kakarot.profile.process_hints, ["AT.exe"]);
    assert_eq!(kakarot.profile.artwork_hint, None);
    assert!(kakarot.warnings.is_empty());

    let original = parse_profile(
        include_bytes!("fixtures/game_ini/valid/v1-original-cyberpunk.ini"),
        &FixtureIdGenerator,
    )
    .unwrap();
    assert_eq!(original.source_version, ProfileSourceVersion::V1);
    assert_eq!(original.profile.media_id.as_str(), "CP77-001");
    assert_eq!(original.profile.app_id, Some(1_091_500));
    assert_eq!(original.profile.process_hints, ["Cyberpunk2077.exe"]);
    assert_eq!(
        original.profile.artwork_hint.as_deref(),
        Some("cyberpunk2077.png")
    );
}

#[test]
fn invalid_fixtures_return_stable_codes() {
    let fixtures: &[(&[u8], ProfileErrorCode, Option<&str>)] = &[
        (
            include_bytes!("fixtures/game_ini/invalid/forbidden-command.ini"),
            ProfileErrorCode::ForbiddenField,
            Some("COMMAND"),
        ),
        (
            include_bytes!("fixtures/game_ini/invalid/duplicate-key.ini"),
            ProfileErrorCode::DuplicateKey,
            Some("MEDIA_ID"),
        ),
        (
            include_bytes!("fixtures/game_ini/invalid/missing-schema.ini"),
            ProfileErrorCode::MissingField,
            Some("SCHEMA"),
        ),
        (
            include_bytes!("fixtures/game_ini/invalid/invalid-profile-id.ini"),
            ProfileErrorCode::InvalidField,
            Some("PROFILE_ID"),
        ),
        (
            include_bytes!("fixtures/game_ini/invalid/media-id-traversal.ini"),
            ProfileErrorCode::InvalidField,
            Some("MEDIA_ID"),
        ),
        (
            include_bytes!("fixtures/game_ini/invalid/process-path.ini"),
            ProfileErrorCode::InvalidField,
            Some("PROCESS_HINTS"),
        ),
        (
            include_bytes!("fixtures/game_ini/invalid/unknown-provider.ini"),
            ProfileErrorCode::InvalidField,
            Some("PROVIDER"),
        ),
    ];

    for (content, expected_code, expected_field) in fixtures {
        let error = parse_profile(content, &FixtureIdGenerator).unwrap_err();
        assert_eq!(&error.code, expected_code);
        assert_eq!(error.field.as_deref(), *expected_field);
    }
}
