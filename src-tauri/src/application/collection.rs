use crate::application::artwork::{ArtworkService, ResolvedArtwork};
use crate::domain::artwork::ArtworkKind;
use crate::domain::entities::{Game, GameActivation, GameId, LaunchProfile};
use crate::domain::errors::DomainError;
use crate::domain::ports::{ActivationRepository, GameRepository, ProfileRepository};
use std::sync::Arc;

pub struct CollectionEntry {
    pub activation: GameActivation,
    pub game: Game,
    pub profile: LaunchProfile,
    pub active_cover: Option<ResolvedArtwork>,
}

pub struct CollectionCover {
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

pub struct GameCollectionService {
    activations: Arc<dyn ActivationRepository>,
    games: Arc<dyn GameRepository>,
    profiles: Arc<dyn ProfileRepository>,
    artwork: Arc<ArtworkService>,
}

impl GameCollectionService {
    pub fn new(
        activations: Arc<dyn ActivationRepository>,
        games: Arc<dyn GameRepository>,
        profiles: Arc<dyn ProfileRepository>,
        artwork: Arc<ArtworkService>,
    ) -> Self {
        Self {
            activations,
            games,
            profiles,
            artwork,
        }
    }

    pub async fn list(&self) -> Result<Vec<CollectionEntry>, DomainError> {
        let activations = self.activations.list().await?;
        let mut entries = Vec::with_capacity(activations.len());
        for activation in activations {
            entries.push(self.resolve(activation).await?);
        }
        Ok(entries)
    }

    pub async fn get(&self, game_id: &GameId) -> Result<Option<CollectionEntry>, DomainError> {
        let Some(activation) = self.activations.find_by_game_id(game_id).await? else {
            return Ok(None);
        };
        self.resolve(activation).await.map(Some)
    }

    pub async fn deactivate(&self, game_id: &GameId) -> Result<(), DomainError> {
        self.activations.delete(game_id).await?;
        Ok(())
    }

    pub async fn cover(&self, game_id: &GameId) -> Result<Option<CollectionCover>, DomainError> {
        if self.activations.find_by_game_id(game_id).await?.is_none() {
            return Ok(None);
        }
        let Some(resolved) = self
            .artwork
            .resolve_active(game_id, ArtworkKind::LauncherCover)
            .await?
        else {
            return Ok(None);
        };
        let bytes = tokio::fs::read(&resolved.absolute_path)
            .await
            .map_err(|_| DomainError::ArtworkIoFailed)?;
        if bytes.is_empty() || bytes.len() > 16 * 1024 * 1024 {
            return Err(DomainError::ArtworkTooLarge);
        }
        Ok(Some(CollectionCover {
            mime_type: resolved.artwork.mime_type,
            bytes,
        }))
    }

    async fn resolve(&self, activation: GameActivation) -> Result<CollectionEntry, DomainError> {
        let game = self
            .games
            .find_by_id(&activation.game_id)
            .await?
            .ok_or_else(collection_corrupt)?;
        let profile = self
            .profiles
            .find_by_id(&activation.profile_id)
            .await?
            .filter(|profile| profile.game_id == activation.game_id)
            .ok_or_else(collection_corrupt)?;
        let active_cover = self
            .artwork
            .resolve_active(&activation.game_id, ArtworkKind::LauncherCover)
            .await?;
        Ok(CollectionEntry {
            activation,
            game,
            profile,
            active_cover,
        })
    }
}

fn collection_corrupt() -> DomainError {
    DomainError::DatabaseOperationFailed("game collection references are invalid".into())
}
