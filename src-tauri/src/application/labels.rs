//! Label Studio project persistence use cases.

use crate::domain::clock::Clock;
use crate::domain::entities::{GameId, LabelProjectId};
use crate::domain::errors::DomainError;
use crate::domain::label::{
    validate_project_name, LabelProject, LabelScene, ThumbnailMetadata, SCENE_VERSION,
};
use crate::domain::ports::LabelProjectRepository;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SaveLabelProject {
    pub id: Option<LabelProjectId>,
    pub game_id: Option<GameId>,
    pub name: String,
    pub scene: LabelScene,
    pub thumbnail: Option<ThumbnailMetadata>,
    pub is_template: bool,
    /// Required for updates and absent for creates.
    pub expected_revision: Option<u64>,
}

pub struct LabelProjectService {
    repository: Arc<dyn LabelProjectRepository>,
    clock: Arc<dyn Clock>,
}

impl LabelProjectService {
    pub fn new(repository: Arc<dyn LabelProjectRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repository, clock }
    }

    pub async fn list(&self) -> Result<Vec<LabelProject>, DomainError> {
        self.repository.list().await
    }

    pub async fn get(&self, id: &LabelProjectId) -> Result<LabelProject, DomainError> {
        self.repository
            .find_by_id(id)
            .await?
            .ok_or(DomainError::LabelProjectNotFound)
    }

    pub async fn save(&self, request: SaveLabelProject) -> Result<LabelProject, DomainError> {
        validate_project_name(&request.name)?;
        request.scene.validate()?;
        if request.scene.schema_version != SCENE_VERSION {
            return Err(DomainError::LabelSceneInvalid);
        }
        if let Some(thumbnail) = &request.thumbnail {
            thumbnail.validate()?;
        }

        let now = self.clock.now_utc();
        let (id, created_at, revision) = match request.id {
            Some(id) => {
                let existing = self
                    .repository
                    .find_by_id(&id)
                    .await?
                    .ok_or(DomainError::LabelProjectNotFound)?;
                let expected = request
                    .expected_revision
                    .ok_or(DomainError::LabelRevisionConflict)?;
                if expected != existing.scene.revision {
                    return Err(DomainError::LabelRevisionConflict);
                }
                (id, existing.created_at, expected.saturating_add(1))
            }
            None => {
                if request.expected_revision.is_some() {
                    return Err(DomainError::LabelRevisionConflict);
                }
                (LabelProjectId::new(), now, 1)
            }
        };

        let mut scene = request.scene;
        scene.revision = revision;
        let thumbnail = request.thumbnail.map(|mut thumbnail| {
            thumbnail.updated_at = now;
            thumbnail
        });
        let project = LabelProject {
            id,
            game_id: request.game_id,
            name: request.name.trim().to_owned(),
            scene,
            thumbnail,
            is_template: request.is_template,
            created_at,
            updated_at: now,
        };
        self.repository
            .upsert(&project, request.expected_revision)
            .await?;
        self.get(&id).await
    }

    pub async fn delete(
        &self,
        id: &LabelProjectId,
        expected_revision: u64,
    ) -> Result<(), DomainError> {
        if self.repository.delete(id, expected_revision).await? {
            Ok(())
        } else if self.repository.find_by_id(id).await?.is_some() {
            Err(DomainError::LabelRevisionConflict)
        } else {
            Err(DomainError::LabelProjectNotFound)
        }
    }
}
