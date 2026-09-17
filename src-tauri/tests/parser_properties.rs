use media_deck_lib::domain::ids::SystemIdGenerator;
use media_deck_lib::domain::media_profile::{parse_profile, MAX_PROFILE_BYTES};

#[test]
fn arbitrary_bytes_never_panic_or_bypass_size_limit() {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    for case in 0..2_000usize {
        let length = case % (MAX_PROFILE_BYTES + 1024);
        let mut bytes = vec![0u8; length];
        for byte in &mut bytes {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }
        let result = std::panic::catch_unwind(|| parse_profile(&bytes, &SystemIdGenerator));
        assert!(result.is_ok(), "parser panicked for case {case}");
        if length > MAX_PROFILE_BYTES {
            assert!(result.unwrap().is_err());
        }
    }
}

#[test]
fn single_byte_mutations_never_panic() {
    let source = include_bytes!("fixtures/game_ini/valid/v2-minimal.ini");
    for index in 0..source.len() {
        for replacement in [0, b'\n', b'=', b'[', 0xff] {
            let mut mutated = source.to_vec();
            mutated[index] = replacement;
            assert!(
                std::panic::catch_unwind(|| parse_profile(&mutated, &SystemIdGenerator)).is_ok()
            );
        }
    }
}
