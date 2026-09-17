//! Manual release-gate baselines. Run with:
//! `cargo test --test performance_baselines -- --ignored --nocapture`

use media_deck_lib::domain::ids::SystemIdGenerator;
use media_deck_lib::domain::media_profile::parse_profile;
use media_deck_lib::infrastructure::database::open_and_migrate;
use std::time::{Duration, Instant};
use tempfile::tempdir;

#[tokio::test]
#[ignore = "release benchmark"]
async fn cold_database_startup_stays_below_two_seconds() {
    let root = tempdir().unwrap();
    let started = Instant::now();
    let pool = open_and_migrate(&root.path().join("benchmark.db"))
        .await
        .unwrap();
    let elapsed = started.elapsed();
    pool.close().await;
    eprintln!("cold_database_startup_ms={}", elapsed.as_millis());
    assert!(elapsed < Duration::from_secs(2));
}

#[test]
#[ignore = "release benchmark"]
fn ten_thousand_manifest_scans_stay_below_one_second() {
    let profile = include_bytes!("fixtures/game_ini/valid/v2-minimal.ini");
    let started = Instant::now();
    for _ in 0..10_000 {
        parse_profile(profile, &SystemIdGenerator).unwrap();
    }
    let elapsed = started.elapsed();
    eprintln!("manifest_scan_10000_ms={}", elapsed.as_millis());
    assert!(elapsed < Duration::from_secs(1));
}

#[tokio::test]
#[ignore = "release benchmark"]
async fn idle_timer_does_not_busy_spin() {
    let started = Instant::now();
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let elapsed = started.elapsed();
    eprintln!("idle_timer_10_ticks_ms={}", elapsed.as_millis());
    assert!(elapsed >= Duration::from_millis(950));
}
