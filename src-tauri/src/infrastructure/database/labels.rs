//! SQLite adapter for revisioned Label Studio projects.

use crate::domain::entities::LabelProjectId;
use crate::domain::errors::DomainError;
use crate::domain::label::{
    LabelPresetKind, LabelProject, LabelScene, ThumbnailMetadata, SCENE_VERSION,
};
use crate::domain::ports::{LabelProjectRepository, PortResult};
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};

const SELECT_COLUMNS: &str = "id, game_id, name, preset_kind, width_mm, height_mm,
    shape_json, bleed_mm, safe_margin_mm, dpi, scene_version, scene_json,
    thumbnail_path, is_template, created_at, updated_at, revision,
    thumbnail_width, thumbnail_height, thumbnail_mime_type, thumbnail_updated_at";

pub struct SqliteLabelProjectRepository {
    pool: SqlitePool,
}

impl SqliteLabelProjectRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn list_inner(&self) -> Result<Vec<LabelProject>, DomainError> {
        let query =
            format!("SELECT {SELECT_COLUMNS} FROM label_projects ORDER BY updated_at DESC, id ASC");
        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(db_error)?;
        rows.iter().map(map_row).collect()
    }

    async fn find_inner(&self, id: &LabelProjectId) -> Result<Option<LabelProject>, DomainError> {
        let query = format!("SELECT {SELECT_COLUMNS} FROM label_projects WHERE id = ?");
        sqlx::query(&query)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(db_error)?
            .as_ref()
            .map(map_row)
            .transpose()
    }

    async fn upsert_inner(
        &self,
        project: &LabelProject,
        expected_revision: Option<u64>,
    ) -> Result<(), DomainError> {
        let scene_json = project.scene.to_canonical_json()?;
        let shape_json = serde_json::to_string(&project.scene.physical.shape)
            .map_err(|_| DomainError::LabelSceneInvalid)?;
        let revision =
            i64::try_from(project.scene.revision).map_err(|_| DomainError::LabelSceneInvalid)?;
        let thumbnail = project.thumbnail.as_ref();

        let result = if let Some(expected) = expected_revision {
            let expected =
                i64::try_from(expected).map_err(|_| DomainError::LabelRevisionConflict)?;
            sqlx::query(
                "UPDATE label_projects SET
                    game_id = ?, name = ?, preset_kind = ?, width_mm = ?, height_mm = ?,
                    shape_json = ?, bleed_mm = ?, safe_margin_mm = ?, dpi = ?,
                    scene_version = ?, scene_json = ?, thumbnail_path = ?,
                    is_template = ?, updated_at = ?, revision = ?,
                    thumbnail_width = ?, thumbnail_height = ?, thumbnail_mime_type = ?,
                    thumbnail_updated_at = ?
                 WHERE id = ? AND revision = ?",
            )
            .bind(project.game_id.map(|id| id.to_string()))
            .bind(&project.name)
            .bind(project.scene.preset.kind.as_str())
            .bind(project.scene.physical.width_mm)
            .bind(project.scene.physical.height_mm)
            .bind(shape_json)
            .bind(project.scene.physical.bleed_mm)
            .bind(project.scene.physical.safe_margin_mm)
            .bind(i64::from(project.scene.physical.dpi))
            .bind(i64::from(project.scene.schema_version))
            .bind(scene_json)
            .bind(thumbnail.map(|value| &value.relative_path))
            .bind(project.is_template)
            .bind(project.updated_at.to_rfc3339())
            .bind(revision)
            .bind(thumbnail.map(|value| i64::from(value.width_px)))
            .bind(thumbnail.map(|value| i64::from(value.height_px)))
            .bind(thumbnail.map(|value| &value.mime_type))
            .bind(thumbnail.map(|value| value.updated_at.to_rfc3339()))
            .bind(project.id.to_string())
            .bind(expected)
            .execute(&self.pool)
            .await
            .map_err(db_error)?
        } else {
            sqlx::query(
                "INSERT INTO label_projects
                    (id, game_id, name, preset_kind, width_mm, height_mm, shape_json,
                     bleed_mm, safe_margin_mm, dpi, scene_version, scene_json,
                     thumbnail_path, is_template, created_at, updated_at, revision,
                     thumbnail_width, thumbnail_height, thumbnail_mime_type,
                     thumbnail_updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(project.id.to_string())
            .bind(project.game_id.map(|id| id.to_string()))
            .bind(&project.name)
            .bind(project.scene.preset.kind.as_str())
            .bind(project.scene.physical.width_mm)
            .bind(project.scene.physical.height_mm)
            .bind(shape_json)
            .bind(project.scene.physical.bleed_mm)
            .bind(project.scene.physical.safe_margin_mm)
            .bind(i64::from(project.scene.physical.dpi))
            .bind(i64::from(project.scene.schema_version))
            .bind(scene_json)
            .bind(thumbnail.map(|value| &value.relative_path))
            .bind(project.is_template)
            .bind(project.created_at.to_rfc3339())
            .bind(project.updated_at.to_rfc3339())
            .bind(revision)
            .bind(thumbnail.map(|value| i64::from(value.width_px)))
            .bind(thumbnail.map(|value| i64::from(value.height_px)))
            .bind(thumbnail.map(|value| &value.mime_type))
            .bind(thumbnail.map(|value| value.updated_at.to_rfc3339()))
            .execute(&self.pool)
            .await
            .map_err(|error| {
                if is_constraint(&error) {
                    DomainError::LabelRevisionConflict
                } else {
                    db_error(error)
                }
            })?
        };
        if result.rows_affected() != 1 {
            return Err(DomainError::LabelRevisionConflict);
        }
        Ok(())
    }

    async fn delete_inner(
        &self,
        id: &LabelProjectId,
        expected_revision: u64,
    ) -> Result<bool, DomainError> {
        let revision =
            i64::try_from(expected_revision).map_err(|_| DomainError::LabelRevisionConflict)?;
        let result = sqlx::query("DELETE FROM label_projects WHERE id = ? AND revision = ?")
            .bind(id.to_string())
            .bind(revision)
            .execute(&self.pool)
            .await
            .map_err(db_error)?;
        Ok(result.rows_affected() == 1)
    }
}

impl LabelProjectRepository for SqliteLabelProjectRepository {
    fn list(&self) -> PortResult<'_, Vec<LabelProject>> {
        Box::pin(self.list_inner())
    }

    fn find_by_id<'a>(&'a self, id: &'a LabelProjectId) -> PortResult<'a, Option<LabelProject>> {
        Box::pin(self.find_inner(id))
    }

    fn upsert<'a>(
        &'a self,
        project: &'a LabelProject,
        expected_revision: Option<u64>,
    ) -> PortResult<'a, ()> {
        Box::pin(self.upsert_inner(project, expected_revision))
    }

    fn delete<'a>(
        &'a self,
        id: &'a LabelProjectId,
        expected_revision: u64,
    ) -> PortResult<'a, bool> {
        Box::pin(self.delete_inner(id, expected_revision))
    }
}

fn map_row(row: &sqlx::sqlite::SqliteRow) -> Result<LabelProject, DomainError> {
    let scene_raw: String = row.try_get("scene_json").map_err(db_error)?;
    let scene =
        LabelScene::parse_strict(&scene_raw).map_err(|_| DomainError::LabelProjectCorrupt)?;
    let revision = read_u64(row, "revision")?;
    let scene_version = read_u32(row, "scene_version")?;
    let preset =
        LabelPresetKind::from_db(&row.try_get::<String, _>("preset_kind").map_err(db_error)?)?;
    let shape_raw: String = row.try_get("shape_json").map_err(db_error)?;
    let shape: crate::domain::label::CanvasShape =
        serde_json::from_str(&shape_raw).map_err(|_| DomainError::LabelProjectCorrupt)?;

    if revision != scene.revision
        || scene_version != SCENE_VERSION
        || scene_version != scene.schema_version
        || preset != scene.preset.kind
        || shape != scene.physical.shape
        || row.try_get::<f64, _>("width_mm").map_err(db_error)? != scene.physical.width_mm
        || row.try_get::<f64, _>("height_mm").map_err(db_error)? != scene.physical.height_mm
        || row.try_get::<f64, _>("bleed_mm").map_err(db_error)? != scene.physical.bleed_mm
        || row.try_get::<f64, _>("safe_margin_mm").map_err(db_error)?
            != scene.physical.safe_margin_mm
        || read_u32(row, "dpi")? != scene.physical.dpi
    {
        return Err(DomainError::LabelProjectCorrupt);
    }

    let thumbnail_path: Option<String> = row.try_get("thumbnail_path").map_err(db_error)?;
    let thumbnail = match thumbnail_path {
        Some(relative_path) => {
            let metadata = ThumbnailMetadata {
                relative_path,
                width_px: read_u32(row, "thumbnail_width")?,
                height_px: read_u32(row, "thumbnail_height")?,
                mime_type: required_optional(row, "thumbnail_mime_type")?,
                updated_at: parse_datetime(&required_optional(row, "thumbnail_updated_at")?)?,
            };
            metadata
                .validate()
                .map_err(|_| DomainError::LabelProjectCorrupt)?;
            Some(metadata)
        }
        None => {
            let extras: (Option<i64>, Option<i64>, Option<String>, Option<String>) = (
                row.try_get("thumbnail_width").map_err(db_error)?,
                row.try_get("thumbnail_height").map_err(db_error)?,
                row.try_get("thumbnail_mime_type").map_err(db_error)?,
                row.try_get("thumbnail_updated_at").map_err(db_error)?,
            );
            if extras != (None, None, None, None) {
                return Err(DomainError::LabelProjectCorrupt);
            }
            None
        }
    };

    let game_id = row
        .try_get::<Option<String>, _>("game_id")
        .map_err(db_error)?
        .map(|value| value.parse().map_err(|_| DomainError::LabelProjectCorrupt))
        .transpose()?;
    let project = LabelProject {
        id: parse_id(&row.try_get::<String, _>("id").map_err(db_error)?)?,
        game_id,
        name: row.try_get("name").map_err(db_error)?,
        scene,
        thumbnail,
        is_template: row.try_get::<i64, _>("is_template").map_err(db_error)? != 0,
        created_at: parse_datetime(&row.try_get::<String, _>("created_at").map_err(db_error)?)?,
        updated_at: parse_datetime(&row.try_get::<String, _>("updated_at").map_err(db_error)?)?,
    };
    crate::domain::label::validate_project_name(&project.name)
        .map_err(|_| DomainError::LabelProjectCorrupt)?;
    Ok(project)
}

fn read_u32(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<u32, DomainError> {
    let value: Option<i64> = row.try_get(column).map_err(db_error)?;
    value
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(DomainError::LabelProjectCorrupt)
}

fn read_u64(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<u64, DomainError> {
    u64::try_from(row.try_get::<i64, _>(column).map_err(db_error)?)
        .map_err(|_| DomainError::LabelProjectCorrupt)
}

fn required_optional(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<String, DomainError> {
    row.try_get::<Option<String>, _>(column)
        .map_err(db_error)?
        .ok_or(DomainError::LabelProjectCorrupt)
}

fn parse_id<T: std::str::FromStr>(value: &str) -> Result<T, DomainError> {
    value.parse().map_err(|_| DomainError::LabelProjectCorrupt)
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>, DomainError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| DomainError::LabelProjectCorrupt)
}

fn is_constraint(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(error) if error.is_unique_violation())
}

fn db_error(error: impl std::fmt::Display) -> DomainError {
    DomainError::DatabaseOperationFailed(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::label::{CanvasShape, LabelPresetKind, PhysicalCanvas, PresetReference};
    use crate::infrastructure::database::open_and_migrate;
    use tempfile::tempdir;

    fn project() -> LabelProject {
        let now = Utc::now();
        LabelProject {
            id: LabelProjectId::new(),
            game_id: None,
            name: "CD cover".into(),
            scene: LabelScene {
                schema_version: SCENE_VERSION,
                revision: 1,
                preset: PresetReference {
                    kind: LabelPresetKind::CdJewelFront,
                    version: 1,
                },
                physical: PhysicalCanvas {
                    width_mm: 120.0,
                    height_mm: 120.0,
                    bleed_mm: 3.0,
                    safe_margin_mm: 3.0,
                    dpi: 300,
                    shape: CanvasShape::Rectangle,
                },
                elements: Vec::new(),
            },
            thumbnail: Some(ThumbnailMetadata {
                relative_path: "thumbnails/project.png".into(),
                width_px: 320,
                height_px: 240,
                mime_type: "image/png".into(),
                updated_at: now,
            }),
            is_template: false,
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn round_trip_and_revision_guard_are_deterministic() {
        let directory = tempdir().unwrap();
        let pool = open_and_migrate(&directory.path().join("labels.db"))
            .await
            .unwrap();
        let repository = SqliteLabelProjectRepository::new(pool);
        let mut value = project();
        repository.upsert_inner(&value, None).await.unwrap();
        assert_eq!(
            repository.find_inner(&value.id).await.unwrap(),
            Some(value.clone())
        );

        value.scene.revision = 2;
        value.name = "Updated".into();
        repository.upsert_inner(&value, Some(1)).await.unwrap();
        assert_eq!(
            repository.upsert_inner(&value, Some(1)).await,
            Err(DomainError::LabelRevisionConflict)
        );
        assert!(!repository.delete_inner(&value.id, 1).await.unwrap());
        assert!(repository.delete_inner(&value.id, 2).await.unwrap());
    }
}
