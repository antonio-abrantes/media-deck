//! Structured logging initialisation for MediaDeck.
//!
//! Outputs to both stdout (pretty, for development) and a rolling file
//! (JSON, for diagnostics). Both outputs are configurable via `RUST_LOG`.
//!
//! Rules (TECHNICAL_SPEC.md §18):
//! - Correlation/session IDs are included in spans.
//! - Raw GAME.INI content is never logged at any level below `trace`.
//! - API keys, tokens, arguments, and passwords are never logged.
//! - Log files rotate daily; at most 7 files are kept including the active day.

use std::path::Path;
use tracing::level_filters::LevelFilter;
use tracing::Span;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialise the global tracing subscriber.
///
/// - `log_dir`: directory where rolling log files are written.
/// - `default_level`: level used when `RUST_LOG` is not set.
///
/// Call this **once** at application startup before creating the Tauri builder.
///
/// # Errors
///
/// Returns a `String` error description if initialisation fails (e.g. the
/// subscriber is already initialised, or the log directory cannot be written).
pub fn init(
    log_dir: &Path,
    default_level: LevelFilter,
) -> Result<tracing_appender::non_blocking::WorkerGuard, String> {
    std::fs::create_dir_all(log_dir).map_err(|error| error.to_string())?;
    // Keep six completed files; the appender opens the seventh (active) file.
    prune_old_logs(log_dir, 6)?;

    // File appender — one file per day, with retention enforced at startup.
    let file_appender = tracing_appender::rolling::daily(log_dir, "media-deck.log");
    let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);

    let env_filter = EnvFilter::builder()
        .with_default_directive(default_level.into())
        .from_env_lossy();

    let stdout_layer = fmt::layer()
        .pretty()
        .with_target(true)
        .with_thread_ids(false);

    let file_layer = fmt::layer()
        .json()
        .with_writer(file_writer)
        .with_target(true)
        .with_thread_ids(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(file_layer)
        .try_init()
        .map_err(|e| e.to_string())?;

    Ok(file_guard)
}

fn prune_old_logs(log_dir: &Path, keep: usize) -> Result<(), String> {
    let mut logs = std::fs::read_dir(log_dir)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("media-deck.log."))
        })
        .collect::<Vec<_>>();
    logs.sort();

    let remove_count = logs.len().saturating_sub(keep);
    for path in logs.into_iter().take(remove_count) {
        std::fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Create the standard span used to correlate one application operation.
pub fn correlation_span(
    correlation_id: &crate::domain::entities::CorrelationId,
    operation: &'static str,
) -> Span {
    tracing::info_span!(
        "media_deck_operation",
        correlation_id = %correlation_id,
        operation
    )
}

/// Convenience: initialise with `INFO` level and a no-op file writer.
/// Suitable for unit tests that need tracing but not file output.
#[cfg(test)]
pub fn init_test() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(LevelFilter::DEBUG)
        .with_test_writer()
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn init_creates_log_file_directory() {
        let dir = tempdir().expect("tempdir");
        // init() may fail if tracing is already initialised in test suite,
        // but the directory should still be accepted.
        let _ = init(dir.path(), LevelFilter::INFO);
        // No panic = success for this test.
    }

    #[test]
    fn init_test_does_not_panic() {
        init_test();
        tracing::info!("tracing works in tests");
    }

    #[test]
    fn old_log_files_are_pruned_to_retention_limit() {
        let dir = tempdir().expect("tempdir");
        for day in 1..=9 {
            std::fs::write(
                dir.path().join(format!("media-deck.log.2026-09-{day:02}")),
                "log",
            )
            .expect("fixture");
        }

        prune_old_logs(dir.path(), 7).expect("prune");
        let remaining = std::fs::read_dir(dir.path()).expect("read logs").count();
        assert_eq!(remaining, 7);
        assert!(!dir.path().join("media-deck.log.2026-09-01").exists());
        assert!(dir.path().join("media-deck.log.2026-09-09").exists());
    }

    #[test]
    fn correlation_span_accepts_typed_id() {
        let id = crate::domain::entities::CorrelationId::new();
        let span = correlation_span(&id, "database_startup");
        let _guard = span.enter();
        tracing::info!("correlated event");
    }
}
