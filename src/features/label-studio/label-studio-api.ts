import { invoke } from '@tauri-apps/api/core';
import type {
  ArtworkAsset,
  ExportReceipt,
  LabelProject,
  LabelScene,
} from './types';

export type SaveLabelProject = {
  id: string | null;
  game_id: string | null;
  name: string;
  scene: LabelScene;
  thumbnail_data_url: string | null;
  is_template: boolean;
  expected_revision: number | null;
};

export const labelStudioApi = {
  list: () => invoke<LabelProject[]>('label_project_list'),
  load: (id: string) =>
    invoke<LabelProject>('label_project_get', { request: { id } }),
  save: (request: SaveLabelProject) =>
    invoke<LabelProject>('label_project_upsert', { request }),
  remove: (id: string, expectedRevision: number) =>
    invoke<void>('label_project_delete', {
      request: { id, expected_revision: expectedRevision },
    }),
  artwork: (gameId: string) =>
    invoke<ArtworkAsset[]>('label_artwork_list', {
      request: { game_id: gameId },
    }),
  artworkById: (artworkId: string) =>
    invoke<ArtworkAsset>('label_artwork_get', {
      request: { artwork_id: artworkId },
    }),
  importClipboardArtwork: (
    gameId: string,
    dataUrl: string,
    purpose: 'editor_source' | 'launcher_cover',
  ) =>
    invoke<ArtworkAsset>('label_artwork_import_clipboard', {
      request: { game_id: gameId, data_url: dataUrl, purpose },
    }),
  export: (
    projectName: string,
    scene: LabelScene,
    format: 'png' | 'pdf',
    dataUrl: string,
    destinationPath: string,
  ) =>
    invoke<ExportReceipt>('label_export', {
      request: {
        project_name: projectName,
        scene,
        format,
        data_url: dataUrl,
        destination_path: destinationPath,
      },
    }),
  calibration: () => invoke<ExportReceipt>('label_calibration_export'),
  thumbnail: (id: string) =>
    invoke<{ mime_type: string; bytes: number[] }>('label_thumbnail_get', {
      request: { id },
    }),
};
