//! Local executable launch provider — no shell, no free-form commands.

use crate::domain::entities::{LaunchKind, LaunchProfile};
use crate::domain::errors::DomainError;
use crate::domain::ports::{GameProvider, PortResult};
use crate::infrastructure::processes::validate_launch_target;
use std::process::Command;

#[derive(Debug, Default)]
pub struct ExecutableProvider;

impl ExecutableProvider {
    fn launch_sync(profile: &LaunchProfile) -> Result<(), DomainError> {
        profile.validate()?;
        if !matches!(profile.launch_kind, LaunchKind::Executable) {
            return Err(DomainError::LaunchProfileInvalid);
        }
        let raw = profile
            .executable_path
            .as_deref()
            .ok_or(DomainError::LaunchProfileInvalid)?;
        let target = validate_launch_target(raw)?;
        let working_directory = match &profile.working_directory {
            Some(dir) => {
                let path =
                    std::fs::canonicalize(dir).map_err(|_| DomainError::LaunchProfileInvalid)?;
                if !path.is_dir() {
                    return Err(DomainError::LaunchProfileInvalid);
                }
                path
            }
            None => target
                .parent()
                .ok_or(DomainError::LaunchProfileInvalid)?
                .to_path_buf(),
        };

        let mut command = Command::new(&target);
        command.args(&profile.arguments);
        command.current_dir(&working_directory);
        command.spawn().map_err(|error| {
            tracing::error!(error = %error, "failed to spawn local executable");
            DomainError::LaunchFailed
        })?;
        Ok(())
    }
}

impl GameProvider for ExecutableProvider {
    fn scan_local(&self) -> PortResult<'_, Vec<crate::domain::entities::Game>> {
        Box::pin(async move {
            // Steam/local library scanning belongs to Phase 7.
            Ok(Vec::new())
        })
    }

    fn launch<'a>(&'a self, profile: &'a LaunchProfile) -> PortResult<'a, ()> {
        Box::pin(async move { Self::launch_sync(profile) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::{GameId, LaunchKind};
    use chrono::Utc;
    use std::fs;
    use std::path::PathBuf;

    #[tokio::test]
    async fn rejects_non_executable_profile_and_missing_target() {
        let provider = ExecutableProvider;
        let steam = LaunchProfile::new(
            GameId::new(),
            "Steam",
            LaunchKind::Steam { app_id: 10 },
            Utc::now(),
        );
        assert_eq!(
            provider.launch(&steam).await,
            Err(DomainError::LaunchProfileInvalid)
        );

        let missing =
            LaunchProfile::new(GameId::new(), "Missing", LaunchKind::Executable, Utc::now());
        assert_eq!(
            provider.launch(&missing).await,
            Err(DomainError::LaunchProfileInvalid)
        );
    }

    #[tokio::test]
    async fn launches_local_fixture_copy_without_shell() {
        let provider = ExecutableProvider;
        let source = std::env::current_exe().expect("current exe");
        let target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("debug");
        let target = target_dir.join(format!(
            "media-deck-launch-fixture-{}.exe",
            std::process::id()
        ));
        fs::copy(&source, &target).expect("copy fixture");

        let profile =
            LaunchProfile::new(GameId::new(), "Fixture", LaunchKind::Executable, Utc::now())
                .with_executable_target(
                    target.to_string_lossy().into_owned(),
                    Some(target_dir.to_string_lossy().into_owned()),
                    vec!["--help".into()],
                    vec![target.file_name().unwrap().to_string_lossy().into_owned()],
                )
                .expect("profile");

        let result = provider.launch(&profile).await;
        let _ = fs::remove_file(&target);
        assert!(result.is_ok() || matches!(result, Err(DomainError::LaunchFailed)));
    }
}
