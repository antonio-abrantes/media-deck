//! Session close decision IPC boundary.

use crate::application::session::{CloseDecision, SessionService};
use crate::domain::entities::SessionId;
use crate::domain::errors::{AppError, DomainError};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionResolveCloseRequest {
    pub session_id: String,
    pub media_key: String,
    pub decision: CloseDecision,
}

#[derive(Debug, Serialize)]
pub struct SessionResolveCloseResponse {
    pub state: String,
}

pub struct SessionServiceState(pub Arc<SessionService>);

/// Apply wait/detach/force. Force revalidates process identity before kill.
#[tauri::command]
pub async fn session_resolve_close(
    state: tauri::State<'_, SessionServiceState>,
    request: SessionResolveCloseRequest,
) -> Result<SessionResolveCloseResponse, AppError> {
    let session_id = SessionId::from_str(&request.session_id)
        .map_err(|_| AppError::from_domain(&DomainError::ProcessNotBound))?;
    let next = state
        .0
        .resolve_close(&session_id, &request.media_key, request.decision)
        .await
        .map_err(|error| AppError::from_domain(&error))?;
    Ok(SessionResolveCloseResponse {
        state: next.label().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::session::apply_close_decision;
    use crate::domain::entities::MediaKey;
    use crate::domain::session::SessionState;

    #[test]
    fn resolve_force_maps_state_machine_to_forced_closing() {
        let current = SessionState::CloseDecision {
            media_key: MediaKey::new("GAME-1").unwrap(),
        };
        let next = apply_close_decision(&current, CloseDecision::Force).unwrap();
        assert_eq!(next.label(), "forced_closing");
    }
}
