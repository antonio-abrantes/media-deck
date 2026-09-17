//! Stable, serializable error types for the MediaDeck domain.
//!
//! [`DomainError`] uses stable string codes that never change between
//! releases, allowing the frontend and logs to react to specific failures
//! without parsing free-form messages.
//!
//! [`AppError`] is the serializable envelope sent over IPC: it wraps any
//! domain error with a correlation ID and a sanitised `details` payload.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── Domain-level errors ─────────────────────────────────────────────────────

/// Errors that originate inside the pure domain layer.
///
/// Each variant maps to a stable string code defined in TECHNICAL_SPEC §17.
/// The `#[error]` message is for Rust logs only; the frontend receives the
/// serialized [`AppError`] code, not the raw message.
#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DomainError {
    // ── Media ────────────────────────────────────────────────────────────────
    #[error("drive is not yet ready for reading")]
    MediaNotReady,

    #[error("no GAME.INI found on the inserted media")]
    MediaProfileMissing,

    #[error("GAME.INI schema or content is invalid")]
    MediaProfileInvalid,

    #[error("media is write-protected")]
    MediaWriteProtected,

    #[error("media or drive changed during an ongoing operation")]
    MediaChanged,

    #[error("media I/O failed")]
    MediaIoFailed,

    #[error("written media profile did not pass verification")]
    MediaVerificationFailed,

    #[error("GAME.INI was saved but collection activation persistence failed")]
    ActivationPersistFailed,

    #[error("device is not in the configured allow-list")]
    MediaDeviceNotAllowed,

    #[error("optical media does not accept writing")]
    OpticalMediaNotWritable,

    #[error("optical disc burn failed")]
    OpticalBurnFailed,

    #[error("verification of burned disc failed")]
    OpticalVerifyFailed,

    // ── Steam / launch ───────────────────────────────────────────────────────
    #[error("Steam client was not found on this system")]
    SteamNotFound,

    #[error("Steam local library data could not be read safely")]
    SteamLibraryInvalid,

    #[error("the game is not installed locally")]
    GameNotInstalled,

    #[error("the game provider failed to launch the title")]
    LaunchFailed,

    #[error("the local launch profile is invalid")]
    LaunchProfileInvalid,

    // ── Artwork ─────────────────────────────────────────────────────────────
    #[error("artwork image or metadata is invalid")]
    ArtworkInvalid,

    #[error("artwork exceeds configured limits")]
    ArtworkTooLarge,

    #[error("remote artwork is disabled")]
    ArtworkRemoteDisabled,

    #[error("remote artwork source was rejected by policy")]
    ArtworkRemoteRejected,

    #[error("local artwork storage failed")]
    ArtworkIoFailed,

    // ── Label Studio ─────────────────────────────────────────────────────────
    #[error("label scene schema or content is invalid")]
    LabelSceneInvalid,

    #[error("label scene exceeds configured limits")]
    LabelSceneTooLarge,

    #[error("label project metadata is invalid")]
    LabelProjectInvalid,

    #[error("label project was not found")]
    LabelProjectNotFound,

    #[error("label project revision conflicts with persisted state")]
    LabelRevisionConflict,

    #[error("label thumbnail metadata is invalid")]
    LabelThumbnailInvalid,

    #[error("persisted label project is corrupt")]
    LabelProjectCorrupt,

    #[error("label export payload or dimensions are invalid")]
    LabelExportInvalid,

    #[error("label export exceeds configured limits")]
    LabelExportTooLarge,

    #[error("label export storage failed")]
    LabelExportIoFailed,

    // ── Process supervision ───────────────────────────────────────────────────
    #[error("game process could not be bound with sufficient identity")]
    ProcessNotBound,

    #[error("game process did not close within the configured timeout")]
    CloseTimeout,

    // ── Infrastructure ────────────────────────────────────────────────────────
    #[error("database migration failed: {0}")]
    DatabaseMigrationFailed(String),

    #[error("database operation failed: {0}")]
    DatabaseOperationFailed(String),

    #[error("invalid value for setting '{0}'")]
    SettingInvalid(String),

    // ── Session state machine ─────────────────────────────────────────────────
    #[error("event '{event}' is not valid in state '{state}'")]
    InvalidTransition { state: String, event: String },

    #[error("a session is already active; only one session is allowed per instance")]
    SessionAlreadyActive,
}

impl DomainError {
    /// Stable error code for IPC and log correlation.
    /// These strings must never change between releases.
    pub fn code(&self) -> &'static str {
        match self {
            DomainError::MediaNotReady => "MEDIA_NOT_READY",
            DomainError::MediaProfileMissing => "MEDIA_PROFILE_MISSING",
            DomainError::MediaProfileInvalid => "MEDIA_PROFILE_INVALID",
            DomainError::MediaWriteProtected => "MEDIA_WRITE_PROTECTED",
            DomainError::MediaChanged => "MEDIA_CHANGED",
            DomainError::MediaIoFailed => "MEDIA_IO_FAILED",
            DomainError::MediaVerificationFailed => "MEDIA_VERIFICATION_FAILED",
            DomainError::ActivationPersistFailed => "ACTIVATION_PERSIST_FAILED",
            DomainError::MediaDeviceNotAllowed => "MEDIA_DEVICE_NOT_ALLOWED",
            DomainError::OpticalMediaNotWritable => "OPTICAL_MEDIA_NOT_WRITABLE",
            DomainError::OpticalBurnFailed => "OPTICAL_BURN_FAILED",
            DomainError::OpticalVerifyFailed => "OPTICAL_VERIFY_FAILED",
            DomainError::SteamNotFound => "STEAM_NOT_FOUND",
            DomainError::SteamLibraryInvalid => "STEAM_LIBRARY_INVALID",
            DomainError::GameNotInstalled => "GAME_NOT_INSTALLED",
            DomainError::LaunchFailed => "LAUNCH_FAILED",
            DomainError::LaunchProfileInvalid => "LAUNCH_PROFILE_INVALID",
            DomainError::ArtworkInvalid => "ARTWORK_INVALID",
            DomainError::ArtworkTooLarge => "ARTWORK_TOO_LARGE",
            DomainError::ArtworkRemoteDisabled => "ARTWORK_REMOTE_DISABLED",
            DomainError::ArtworkRemoteRejected => "ARTWORK_REMOTE_REJECTED",
            DomainError::ArtworkIoFailed => "ARTWORK_IO_FAILED",
            DomainError::LabelSceneInvalid => "LABEL_SCENE_INVALID",
            DomainError::LabelSceneTooLarge => "LABEL_SCENE_TOO_LARGE",
            DomainError::LabelProjectInvalid => "LABEL_PROJECT_INVALID",
            DomainError::LabelProjectNotFound => "LABEL_PROJECT_NOT_FOUND",
            DomainError::LabelRevisionConflict => "LABEL_REVISION_CONFLICT",
            DomainError::LabelThumbnailInvalid => "LABEL_THUMBNAIL_INVALID",
            DomainError::LabelProjectCorrupt => "LABEL_PROJECT_CORRUPT",
            DomainError::LabelExportInvalid => "LABEL_EXPORT_INVALID",
            DomainError::LabelExportTooLarge => "LABEL_EXPORT_TOO_LARGE",
            DomainError::LabelExportIoFailed => "LABEL_EXPORT_IO_FAILED",
            DomainError::ProcessNotBound => "PROCESS_NOT_BOUND",
            DomainError::CloseTimeout => "CLOSE_TIMEOUT",
            DomainError::DatabaseMigrationFailed(_) => "DATABASE_MIGRATION_FAILED",
            DomainError::DatabaseOperationFailed(_) => "DATABASE_OPERATION_FAILED",
            DomainError::SettingInvalid(_) => "SETTING_INVALID",
            DomainError::InvalidTransition { .. } => "INVALID_TRANSITION",
            DomainError::SessionAlreadyActive => "SESSION_ALREADY_ACTIVE",
        }
    }

    /// Whether the user or system can retry / recover from this error.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            DomainError::MediaNotReady
                | DomainError::MediaChanged
                | DomainError::MediaIoFailed
                | DomainError::ActivationPersistFailed
                | DomainError::SteamNotFound
                | DomainError::SteamLibraryInvalid
                | DomainError::LaunchFailed
                | DomainError::ArtworkIoFailed
                | DomainError::LabelRevisionConflict
                | DomainError::CloseTimeout
        )
    }
}

// ─── IPC envelope ─────────────────────────────────────────────────────────────

/// Serialisable error envelope sent over Tauri IPC to the frontend.
///
/// Contains only sanitised information — no raw OS error messages,
/// no file paths beyond what is strictly necessary, no API keys or tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppError {
    /// Stable error code (matches `DomainError::code()`).
    pub code: String,
    /// Human-readable message (safe for display, no sensitive data).
    pub message: String,
    /// Whether the operation can be retried or the user can recover.
    pub recoverable: bool,
    /// Optional structured details for the UI (sanitised).
    pub details: Option<serde_json::Value>,
    /// Correlation ID linking this error to a specific session or operation log.
    pub correlation_id: Option<String>,
}

impl AppError {
    /// Create an `AppError` without a correlation ID or extra details.
    pub fn from_domain(err: &DomainError) -> Self {
        Self {
            code: err.code().to_owned(),
            message: safe_message(err).to_owned(),
            recoverable: err.is_recoverable(),
            details: None,
            correlation_id: None,
        }
    }

    /// Attach a correlation ID to this error.
    pub fn with_correlation(mut self, id: impl Into<String>) -> Self {
        self.correlation_id = Some(id.into());
        self
    }

    /// Attach structured details to this error (must be sanitised by caller).
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

fn safe_message(err: &DomainError) -> &'static str {
    match err {
        DomainError::MediaNotReady => "A unidade ainda não está pronta.",
        DomainError::MediaProfileMissing => "Nenhum perfil MediaDeck foi encontrado na mídia.",
        DomainError::MediaProfileInvalid => "O perfil da mídia é inválido.",
        DomainError::MediaWriteProtected => "A mídia está protegida contra gravação.",
        DomainError::MediaChanged => "A mídia ou unidade mudou durante a operação.",
        DomainError::MediaIoFailed => "Não foi possível acessar o perfil na mídia.",
        DomainError::MediaVerificationFailed => "O perfil gravado não passou na verificação.",
        DomainError::ActivationPersistFailed => {
            "O GAME.INI foi salvo, mas o jogo não pôde ser adicionado à coleção."
        }
        DomainError::MediaDeviceNotAllowed => "A unidade não está autorizada.",
        DomainError::OpticalMediaNotWritable => "A mídia óptica não aceita gravação.",
        DomainError::OpticalBurnFailed => "Não foi possível gravar a mídia óptica.",
        DomainError::OpticalVerifyFailed => "A mídia gravada não pôde ser verificada.",
        DomainError::SteamNotFound => "O cliente Steam não foi encontrado.",
        DomainError::SteamLibraryInvalid => "A biblioteca local da Steam não pôde ser lida.",
        DomainError::GameNotInstalled => "O jogo não está instalado.",
        DomainError::LaunchFailed => "Não foi possível iniciar o jogo.",
        DomainError::LaunchProfileInvalid => "O perfil local de inicialização é inválido.",
        DomainError::ArtworkInvalid => "A imagem de capa é inválida.",
        DomainError::ArtworkTooLarge => "A imagem de capa excede os limites permitidos.",
        DomainError::ArtworkRemoteDisabled => "A busca remota de capas está desativada.",
        DomainError::ArtworkRemoteRejected => "A origem remota da capa não é permitida.",
        DomainError::ArtworkIoFailed => "Não foi possível armazenar a capa localmente.",
        DomainError::LabelSceneInvalid => "A cena da etiqueta é inválida.",
        DomainError::LabelSceneTooLarge => "A cena da etiqueta excede os limites permitidos.",
        DomainError::LabelProjectInvalid => "O projeto de etiqueta é inválido.",
        DomainError::LabelProjectNotFound => "O projeto de etiqueta não foi encontrado.",
        DomainError::LabelRevisionConflict => "O projeto foi alterado por outra operação.",
        DomainError::LabelThumbnailInvalid => "Os metadados da miniatura são inválidos.",
        DomainError::LabelProjectCorrupt => "O projeto salvo não pôde ser lido.",
        DomainError::LabelExportInvalid => "A imagem de exportação é inválida.",
        DomainError::LabelExportTooLarge => "A exportação excede os limites permitidos.",
        DomainError::LabelExportIoFailed => "Não foi possível gravar a exportação.",
        DomainError::ProcessNotBound => {
            "Não foi possível associar o processo do jogo com segurança."
        }
        DomainError::CloseTimeout => "O jogo não encerrou dentro do tempo configurado.",
        DomainError::DatabaseMigrationFailed(_) => "Não foi possível preparar o banco de dados.",
        DomainError::DatabaseOperationFailed(_) => "Não foi possível acessar os dados locais.",
        DomainError::SettingInvalid(_) => "Uma configuração possui valor inválido.",
        DomainError::InvalidTransition { .. } => "A operação não é válida no estado atual.",
        DomainError::SessionAlreadyActive => "Já existe uma sessão ativa.",
    }
}

impl From<DomainError> for AppError {
    fn from(err: DomainError) -> Self {
        AppError::from_domain(&err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_a_stable_code() {
        // Ensure code() is exhaustive — if a new variant is added without
        // a code arm the match above will fail to compile.
        let variants: &[DomainError] = &[
            DomainError::MediaNotReady,
            DomainError::MediaProfileMissing,
            DomainError::MediaProfileInvalid,
            DomainError::MediaWriteProtected,
            DomainError::MediaChanged,
            DomainError::MediaIoFailed,
            DomainError::MediaVerificationFailed,
            DomainError::ActivationPersistFailed,
            DomainError::MediaDeviceNotAllowed,
            DomainError::OpticalMediaNotWritable,
            DomainError::OpticalBurnFailed,
            DomainError::OpticalVerifyFailed,
            DomainError::SteamNotFound,
            DomainError::SteamLibraryInvalid,
            DomainError::GameNotInstalled,
            DomainError::LaunchFailed,
            DomainError::LaunchProfileInvalid,
            DomainError::ArtworkInvalid,
            DomainError::ArtworkTooLarge,
            DomainError::ArtworkRemoteDisabled,
            DomainError::ArtworkRemoteRejected,
            DomainError::ArtworkIoFailed,
            DomainError::LabelSceneInvalid,
            DomainError::LabelSceneTooLarge,
            DomainError::LabelProjectInvalid,
            DomainError::LabelProjectNotFound,
            DomainError::LabelRevisionConflict,
            DomainError::LabelThumbnailInvalid,
            DomainError::LabelProjectCorrupt,
            DomainError::LabelExportInvalid,
            DomainError::LabelExportTooLarge,
            DomainError::LabelExportIoFailed,
            DomainError::ProcessNotBound,
            DomainError::CloseTimeout,
            DomainError::DatabaseMigrationFailed("test".into()),
            DomainError::DatabaseOperationFailed("test".into()),
            DomainError::SettingInvalid("sound_enabled".into()),
            DomainError::InvalidTransition {
                state: "Idle".into(),
                event: "ProcessExited".into(),
            },
            DomainError::SessionAlreadyActive,
        ];

        for v in variants {
            let code = v.code();
            assert!(!code.is_empty(), "code must not be empty for {v:?}");
            assert!(
                code.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
                "code must be UPPER_SNAKE_CASE, got '{code}'"
            );
        }
    }

    #[test]
    fn recoverable_errors_are_marked() {
        assert!(DomainError::MediaNotReady.is_recoverable());
        assert!(DomainError::CloseTimeout.is_recoverable());
        assert!(!DomainError::MediaProfileInvalid.is_recoverable());
        assert!(!DomainError::MediaDeviceNotAllowed.is_recoverable());
    }

    #[test]
    fn from_domain_error_preserves_code() {
        let err = DomainError::SteamNotFound;
        let app: AppError = err.clone().into();
        assert_eq!(app.code, err.code());
        assert_eq!(app.recoverable, err.is_recoverable());
    }

    #[test]
    fn app_error_serde_round_trip() {
        let app = AppError::from_domain(&DomainError::LaunchFailed)
            .with_correlation("sess-123")
            .with_details(serde_json::json!({ "provider": "steam" }));

        let json = serde_json::to_string(&app).expect("serialize");
        let back: AppError = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.code, "LAUNCH_FAILED");
        assert_eq!(back.correlation_id.as_deref(), Some("sess-123"));
    }

    #[test]
    fn database_details_are_not_exposed_to_ipc() {
        let app = AppError::from_domain(&DomainError::DatabaseOperationFailed(
            r#"failed near C:\Users\private\media-deck.db"#.into(),
        ));
        assert_eq!(app.message, "Não foi possível acessar os dados locais.");
        assert!(!app.message.contains("private"));
    }
}
