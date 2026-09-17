export type LabelPresetKind =
  | 'launcher_cover'
  | 'floppy_label'
  | 'cd_jewel_front'
  | 'cd_disc_label'
  | 'custom';

export type CanvasShape =
  { type: 'rectangle' } | { type: 'circle'; center_hole_mm: number };

export type ElementFrame = {
  id: string;
  x_mm: number;
  y_mm: number;
  width_mm: number;
  height_mm: number;
  rotation_deg: number;
  opacity: number;
  z_order: number;
  locked: boolean;
  visible: boolean;
};

export type NormalizedCrop = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type SceneElement =
  | ({
      type: 'image';
      artwork_id: string;
      fit: ImageFit;
      crop: NormalizedCrop | null;
    } & ElementFrame)
  | ({
      type: 'text';
      text: string;
      font_family: string;
      font_size_mm: number;
      color: string;
      align: TextAlign;
    } & ElementFrame)
  | ({
      type: 'rect';
      fill: string;
      stroke: string | null;
      stroke_width_mm: number;
      corner_radius_mm: number;
    } & ElementFrame)
  | ({ type: 'line'; color: string; stroke_width_mm: number } & ElementFrame)
  | ({
      type: 'logo';
      artwork_id: string;
      fit: ImageFit;
      tint: string | null;
    } & ElementFrame)
  | ({ type: 'background'; color: string } & ElementFrame);

export type ImageFit = 'contain' | 'cover' | 'stretch';
export type TextAlign = 'left' | 'center' | 'right';

export type LabelScene = {
  schema_version: 1;
  revision: number;
  preset: { kind: LabelPresetKind; version: number };
  physical: {
    width_mm: number;
    height_mm: number;
    bleed_mm: number;
    safe_margin_mm: number;
    dpi: number;
    shape: CanvasShape;
  };
  elements: SceneElement[];
};

export type LabelProject = {
  id: string;
  game_id: string | null;
  name: string;
  scene: LabelScene;
  thumbnail: ThumbnailMetadata | null;
  is_template: boolean;
  created_at: string;
  updated_at: string;
};

export type ThumbnailMetadata = {
  relative_path: string;
  width_px: number;
  height_px: number;
  mime_type: 'image/png' | 'image/jpeg' | 'image/webp';
  updated_at: string;
};

export type ExportReceipt = {
  file_name: string;
  output_path: string;
  format: 'png' | 'pdf';
  width_px: number;
  height_px: number;
  dpi: number;
};

export type ArtworkAsset = {
  id: string;
  game_id: string;
  kind:
    | 'editor_source'
    | 'launcher_cover'
    | 'hero'
    | 'logo'
    | 'jewel_front'
    | 'disc_label'
    | 'icon';
  width: number;
  height: number;
  mime_type: string;
  bytes: number[] | null;
};

export type ToolKind = SceneElement['type'];
