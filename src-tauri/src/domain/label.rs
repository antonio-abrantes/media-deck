//! Versioned, strictly validated Label Studio scene and project contracts.

use crate::domain::entities::{ArtworkId, GameId, LabelProjectId};
use crate::domain::errors::DomainError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Component, Path};

pub const SCENE_VERSION: u32 = 1;
pub const MAX_SCENE_JSON_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_ELEMENTS: usize = 512;
const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_NAME_CHARS: usize = 120;
const MAX_FONT_CHARS: usize = 100;
const MAX_DIMENSION_MM: f64 = 2_000.0;
const MAX_COORDINATE_MM: f64 = 10_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelPresetKind {
    LauncherCover,
    FloppyLabel,
    CdJewelFront,
    CdDiscLabel,
    Custom,
}

impl LabelPresetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LauncherCover => "launcher_cover",
            Self::FloppyLabel => "floppy_label",
            Self::CdJewelFront => "cd_jewel_front",
            Self::CdDiscLabel => "cd_disc_label",
            Self::Custom => "custom",
        }
    }

    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "launcher_cover" => Ok(Self::LauncherCover),
            "floppy_label" => Ok(Self::FloppyLabel),
            "cd_jewel_front" => Ok(Self::CdJewelFront),
            "cd_disc_label" => Ok(Self::CdDiscLabel),
            "custom" => Ok(Self::Custom),
            _ => Err(DomainError::LabelProjectCorrupt),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabelScene {
    pub schema_version: u32,
    /// Monotonic persisted revision. Frontend history is intentionally separate.
    pub revision: u64,
    pub preset: PresetReference,
    pub physical: PhysicalCanvas,
    pub elements: Vec<SceneElement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresetReference {
    pub kind: LabelPresetKind,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicalCanvas {
    pub width_mm: f64,
    pub height_mm: f64,
    pub bleed_mm: f64,
    pub safe_margin_mm: f64,
    pub dpi: u32,
    pub shape: CanvasShape,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CanvasShape {
    Rectangle,
    Circle { center_hole_mm: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneElement {
    Image {
        #[serde(flatten)]
        frame: ElementFrame,
        artwork_id: ArtworkId,
        fit: ImageFit,
        crop: Option<NormalizedCrop>,
    },
    Text {
        #[serde(flatten)]
        frame: ElementFrame,
        text: String,
        font_family: String,
        font_size_mm: f64,
        color: String,
        align: TextAlign,
    },
    Rect {
        #[serde(flatten)]
        frame: ElementFrame,
        fill: String,
        stroke: Option<String>,
        stroke_width_mm: f64,
        corner_radius_mm: f64,
    },
    Line {
        #[serde(flatten)]
        frame: ElementFrame,
        color: String,
        stroke_width_mm: f64,
    },
    Logo {
        #[serde(flatten)]
        frame: ElementFrame,
        artwork_id: ArtworkId,
        fit: ImageFit,
        tint: Option<String>,
    },
    Background {
        #[serde(flatten)]
        frame: ElementFrame,
        color: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ElementFrame {
    pub id: String,
    pub x_mm: f64,
    pub y_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
    pub rotation_deg: f64,
    pub opacity: f64,
    pub z_order: i32,
    pub locked: bool,
    pub visible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    Contain,
    Cover,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedCrop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThumbnailMetadata {
    /// Relative path under the backend-owned thumbnail root.
    pub relative_path: String,
    pub width_px: u32,
    pub height_px: u32,
    pub mime_type: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelProject {
    pub id: LabelProjectId,
    pub game_id: Option<GameId>,
    pub name: String,
    pub scene: LabelScene,
    pub thumbnail: Option<ThumbnailMetadata>,
    pub is_template: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LabelScene {
    pub fn parse_strict(raw: &str) -> Result<Self, DomainError> {
        if raw.len() > MAX_SCENE_JSON_BYTES {
            return Err(DomainError::LabelSceneTooLarge);
        }
        let scene: Self = serde_json::from_str(raw).map_err(|_| DomainError::LabelSceneInvalid)?;
        scene.validate()?;
        Ok(scene)
    }

    pub fn to_canonical_json(&self) -> Result<String, DomainError> {
        self.validate()?;
        let json = serde_json::to_string(self).map_err(|_| DomainError::LabelSceneInvalid)?;
        if json.len() > MAX_SCENE_JSON_BYTES {
            return Err(DomainError::LabelSceneTooLarge);
        }
        Ok(json)
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if self.schema_version != SCENE_VERSION
            || self.preset.version == 0
            || self.preset.version > 100
            || self.elements.len() > MAX_ELEMENTS
        {
            return Err(DomainError::LabelSceneInvalid);
        }
        self.physical.validate(self.preset.kind)?;
        let mut ids = HashSet::with_capacity(self.elements.len());
        let mut z_orders = HashSet::with_capacity(self.elements.len());
        let mut background_seen = false;
        for element in &self.elements {
            let frame = element.frame();
            frame.validate()?;
            if !ids.insert(frame.id.as_str()) || !z_orders.insert(frame.z_order) {
                return Err(DomainError::LabelSceneInvalid);
            }
            element.validate_specific()?;
            if matches!(element, SceneElement::Background { .. }) {
                if background_seen
                    || frame.x_mm != 0.0
                    || frame.y_mm != 0.0
                    || frame.width_mm != self.physical.width_mm
                    || frame.height_mm != self.physical.height_mm
                {
                    return Err(DomainError::LabelSceneInvalid);
                }
                background_seen = true;
            }
        }
        Ok(())
    }

    /// Typed artwork references to be resolved by the artwork application service.
    pub fn referenced_artwork_ids(&self) -> Vec<ArtworkId> {
        self.elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Image { artwork_id, .. } | SceneElement::Logo { artwork_id, .. } => {
                    Some(*artwork_id)
                }
                _ => None,
            })
            .collect()
    }
}

impl PhysicalCanvas {
    fn validate(&self, preset: LabelPresetKind) -> Result<(), DomainError> {
        finite_range(self.width_mm, 0.1, MAX_DIMENSION_MM)?;
        finite_range(self.height_mm, 0.1, MAX_DIMENSION_MM)?;
        finite_range(self.bleed_mm, 0.0, 50.0)?;
        finite_range(self.safe_margin_mm, 0.0, 100.0)?;
        if !(72..=1_200).contains(&self.dpi)
            || self.safe_margin_mm * 2.0 >= self.width_mm.min(self.height_mm)
        {
            return Err(DomainError::LabelSceneInvalid);
        }
        match (&self.shape, preset) {
            (CanvasShape::Circle { center_hole_mm }, LabelPresetKind::CdDiscLabel) => {
                finite_range(
                    *center_hole_mm,
                    0.0,
                    self.width_mm.min(self.height_mm) - 0.1,
                )?;
                if (self.width_mm - self.height_mm).abs() > 0.001 {
                    return Err(DomainError::LabelSceneInvalid);
                }
            }
            (CanvasShape::Circle { .. }, _) => return Err(DomainError::LabelSceneInvalid),
            (CanvasShape::Rectangle, LabelPresetKind::CdDiscLabel) => {
                return Err(DomainError::LabelSceneInvalid)
            }
            _ => {}
        }
        Ok(())
    }
}

impl SceneElement {
    fn frame(&self) -> &ElementFrame {
        match self {
            Self::Image { frame, .. }
            | Self::Text { frame, .. }
            | Self::Rect { frame, .. }
            | Self::Line { frame, .. }
            | Self::Logo { frame, .. }
            | Self::Background { frame, .. } => frame,
        }
    }

    fn validate_specific(&self) -> Result<(), DomainError> {
        match self {
            Self::Image { crop, .. } => {
                if let Some(crop) = crop {
                    crop.validate()?;
                }
            }
            Self::Text {
                text,
                font_family,
                font_size_mm,
                color,
                ..
            } => {
                if text.len() > MAX_TEXT_BYTES
                    || font_family.is_empty()
                    || font_family.chars().count() > MAX_FONT_CHARS
                {
                    return Err(DomainError::LabelSceneInvalid);
                }
                finite_range(*font_size_mm, 0.5, 500.0)?;
                validate_color(color)?;
            }
            Self::Rect {
                fill,
                stroke,
                stroke_width_mm,
                corner_radius_mm,
                ..
            } => {
                validate_color(fill)?;
                if let Some(stroke) = stroke {
                    validate_color(stroke)?;
                }
                finite_range(*stroke_width_mm, 0.0, 100.0)?;
                finite_range(*corner_radius_mm, 0.0, MAX_DIMENSION_MM)?;
            }
            Self::Line {
                color,
                stroke_width_mm,
                ..
            } => {
                validate_color(color)?;
                finite_range(*stroke_width_mm, 0.01, 100.0)?;
            }
            Self::Logo { tint, .. } => {
                if let Some(tint) = tint {
                    validate_color(tint)?;
                }
            }
            Self::Background { color, frame } => {
                validate_color(color)?;
                if frame.rotation_deg != 0.0 {
                    return Err(DomainError::LabelSceneInvalid);
                }
            }
        }
        Ok(())
    }
}

impl ElementFrame {
    fn validate(&self) -> Result<(), DomainError> {
        if self.id.is_empty()
            || self.id.len() > 64
            || !self
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
            || !(-10_000..=10_000).contains(&self.z_order)
        {
            return Err(DomainError::LabelSceneInvalid);
        }
        finite_range(self.x_mm, -MAX_COORDINATE_MM, MAX_COORDINATE_MM)?;
        finite_range(self.y_mm, -MAX_COORDINATE_MM, MAX_COORDINATE_MM)?;
        finite_range(self.width_mm, 0.01, MAX_DIMENSION_MM)?;
        finite_range(self.height_mm, 0.01, MAX_DIMENSION_MM)?;
        finite_range(self.rotation_deg, -360.0, 360.0)?;
        finite_range(self.opacity, 0.0, 1.0)
    }
}

impl NormalizedCrop {
    fn validate(&self) -> Result<(), DomainError> {
        finite_range(self.x, 0.0, 1.0)?;
        finite_range(self.y, 0.0, 1.0)?;
        finite_range(self.width, f64::EPSILON, 1.0)?;
        finite_range(self.height, f64::EPSILON, 1.0)?;
        if self.x + self.width > 1.0 || self.y + self.height > 1.0 {
            return Err(DomainError::LabelSceneInvalid);
        }
        Ok(())
    }
}

impl ThumbnailMetadata {
    pub fn validate(&self) -> Result<(), DomainError> {
        let path = Path::new(&self.relative_path);
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        let extension_matches_mime = matches!(
            (extension.as_deref(), self.mime_type.as_str()),
            (Some("png"), "image/png")
                | (Some("jpg" | "jpeg"), "image/jpeg")
                | (Some("webp"), "image/webp")
        );
        if self.relative_path.is_empty()
            || self.relative_path.len() > 240
            || self
                .relative_path
                .chars()
                .any(|value| value.is_control() || value == ':')
            || path.is_absolute()
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            || !(1..=4_096).contains(&self.width_px)
            || !(1..=4_096).contains(&self.height_px)
            || !matches!(
                self.mime_type.as_str(),
                "image/png" | "image/jpeg" | "image/webp"
            )
            || !extension_matches_mime
        {
            return Err(DomainError::LabelThumbnailInvalid);
        }
        Ok(())
    }
}

pub fn validate_project_name(name: &str) -> Result<(), DomainError> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > MAX_NAME_CHARS
        || trimmed.chars().any(char::is_control)
    {
        return Err(DomainError::LabelProjectInvalid);
    }
    Ok(())
}

fn validate_color(value: &str) -> Result<(), DomainError> {
    let valid_length = value.len() == 7 || value.len() == 9;
    if !valid_length
        || !value.starts_with('#')
        || !value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(DomainError::LabelSceneInvalid);
    }
    Ok(())
}

fn finite_range(value: f64, min: f64, max: f64) -> Result<(), DomainError> {
    if value.is_finite() && value >= min && value <= max {
        Ok(())
    } else {
        Err(DomainError::LabelSceneInvalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> LabelScene {
        LabelScene {
            schema_version: SCENE_VERSION,
            revision: 3,
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
            elements: vec![SceneElement::Background {
                frame: ElementFrame {
                    id: "background".into(),
                    x_mm: 0.0,
                    y_mm: 0.0,
                    width_mm: 120.0,
                    height_mm: 120.0,
                    rotation_deg: 0.0,
                    opacity: 1.0,
                    z_order: -1,
                    locked: true,
                    visible: true,
                },
                color: "#112233FF".into(),
            }],
        }
    }

    #[test]
    fn canonical_scene_round_trip_is_byte_stable() {
        let first = scene().to_canonical_json().unwrap();
        let parsed = LabelScene::parse_strict(&first).unwrap();
        let second = parsed.to_canonical_json().unwrap();
        assert_eq!(first, second);
        assert_eq!(parsed, scene());
    }

    #[test]
    fn all_element_variants_round_trip_with_typed_artwork_references() {
        let artwork = ArtworkId::new();
        let frame = |id: &str, z_order| ElementFrame {
            id: id.into(),
            x_mm: 1.0,
            y_mm: 2.0,
            width_mm: 30.0,
            height_mm: 20.0,
            rotation_deg: 0.0,
            opacity: 0.8,
            z_order,
            locked: false,
            visible: true,
        };
        let mut value = scene();
        value.elements.extend([
            SceneElement::Image {
                frame: frame("image", 1),
                artwork_id: artwork,
                fit: ImageFit::Cover,
                crop: Some(NormalizedCrop {
                    x: 0.1,
                    y: 0.1,
                    width: 0.8,
                    height: 0.8,
                }),
            },
            SceneElement::Text {
                frame: frame("text", 2),
                text: "MediaDeck".into(),
                font_family: "Arial".into(),
                font_size_mm: 4.0,
                color: "#FFFFFF".into(),
                align: TextAlign::Center,
            },
            SceneElement::Rect {
                frame: frame("rect", 3),
                fill: "#00000000".into(),
                stroke: Some("#FFFFFF".into()),
                stroke_width_mm: 0.2,
                corner_radius_mm: 1.0,
            },
            SceneElement::Line {
                frame: frame("line", 4),
                color: "#FFFFFF".into(),
                stroke_width_mm: 0.2,
            },
            SceneElement::Logo {
                frame: frame("logo", 5),
                artwork_id: artwork,
                fit: ImageFit::Contain,
                tint: None,
            },
        ]);

        let json = value.to_canonical_json().unwrap();
        assert_eq!(LabelScene::parse_strict(&json).unwrap(), value);
        assert_eq!(value.referenced_artwork_ids(), vec![artwork, artwork]);

        let unknown_element_field =
            json.replacen("\"artwork_id\":", "\"unknown\":true,\"artwork_id\":", 1);
        assert_eq!(
            LabelScene::parse_strict(&unknown_element_field),
            Err(DomainError::LabelSceneInvalid)
        );
    }

    #[test]
    fn rejects_unknown_fields_and_unsupported_versions() {
        let raw = scene().to_canonical_json().unwrap();
        let with_unknown = raw.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"unexpected\":true",
            1,
        );
        assert_eq!(
            LabelScene::parse_strict(&with_unknown),
            Err(DomainError::LabelSceneInvalid)
        );
        let unsupported = raw.replacen("\"schema_version\":1", "\"schema_version\":2", 1);
        assert_eq!(
            LabelScene::parse_strict(&unsupported),
            Err(DomainError::LabelSceneInvalid)
        );
    }

    #[test]
    fn rejects_duplicate_z_order_and_oversized_payloads() {
        let mut duplicate = scene();
        duplicate.elements.push(duplicate.elements[0].clone());
        assert_eq!(duplicate.validate(), Err(DomainError::LabelSceneInvalid));
        let huge = " ".repeat(MAX_SCENE_JSON_BYTES + 1);
        assert_eq!(
            LabelScene::parse_strict(&huge),
            Err(DomainError::LabelSceneTooLarge)
        );
    }

    #[test]
    fn thumbnail_paths_cannot_escape_backend_root() {
        let metadata = ThumbnailMetadata {
            relative_path: "../secret.png".into(),
            width_px: 320,
            height_px: 320,
            mime_type: "image/png".into(),
            updated_at: Utc::now(),
        };
        assert_eq!(metadata.validate(), Err(DomainError::LabelThumbnailInvalid));

        let ads = ThumbnailMetadata {
            relative_path: "preview.png:payload".into(),
            width_px: 320,
            height_px: 320,
            mime_type: "image/png".into(),
            updated_at: Utc::now(),
        };
        assert_eq!(ads.validate(), Err(DomainError::LabelThumbnailInvalid));
    }
}
