import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type Device = {
  id: string;
  friendly_name: string;
  drive_type: 'removable' | 'cd_rom';
  current_mount_point: string | null;
  enabled: boolean;
};

export type CreatorSelection = {
  game_id: string;
  profile_id: string;
  device_id: string | null;
  media_id: string;
  media_kind: 'floppy' | 'optical' | null;
  include_artwork: boolean;
};

export type CreatorPreview = {
  profile: {
    media_id: string;
    media_kind: string | null;
    profile_id: string;
    provider: 'steam' | 'executable';
    app_id: number | null;
    display_name: string;
    process_hints: string[];
    cover_artwork_id: string | null;
    cover_cache_path: string | null;
  };
  canonical_ini: string;
  device_name: string | null;
  mount_point: string | null;
  include_artwork: boolean;
};

export type IniExportReceipt = {
  path: string;
  content_hash: string;
  game_id: string;
  profile_id: string;
  export_count: number;
  last_exported_at: string;
};

export type MediaRecord = {
  id: string;
  media_id: string;
  profile_id: string;
  media_kind: 'floppy' | 'optical' | 'removable';
  device_id: string | null;
  schema_version: number;
  content_hash: string;
  last_drive: string | null;
  created_at: string;
  last_verified_at: string | null;
  status: 'active' | 'damaged' | 'replaced' | 'missing';
};

export type LegacyImport = {
  source_version: 'v1' | 'v2';
  profile: { media_id: string; display_name: string };
  warnings: Array<{ code: string; line: number | null; field: string | null }>;
  source_hash: string;
};

export type OpticalRecorder = {
  unique_id: string;
  vendor: string;
  product: string;
  volume_paths: string[];
};

export type OpticalProgress = {
  operation_id: string;
  phase:
    | 'validating'
    | 'building_image'
    | 'initializing_hardware'
    | 'formatting_media'
    | 'calibrating_power'
    | 'writing_data'
    | 'finalizing'
    | 'verifying'
    | 'completed'
    | 'cancelled';
  percent: number;
  elapsed_seconds: number;
  remaining_seconds: number | null;
  cancellation_safe: boolean;
};

export type EraseChallenge = {
  recorder_id: string;
  target: string;
  confirmation_text: string;
};

export const mediaCreatorApi = {
  devices: () => invoke<Device[]>('device_list'),
  preview: (request: CreatorSelection) =>
    invoke<CreatorPreview>('media_creator_preview', { request }),
  exportIni: (request: CreatorSelection, destinationPath: string) =>
    invoke<IniExportReceipt>('media_creator_export_ini', {
      request: { ...request, destination_path: destinationPath },
    }),
  write: (request: CreatorSelection) =>
    invoke<MediaRecord>('media_creator_write', {
      request: { ...request, confirmed: true },
    }),
  inspectImport: (deviceId: string) =>
    invoke<LegacyImport>('media_import_inspect', {
      request: { device_id: deviceId },
    }),
  upgradeImport: (request: CreatorSelection, expectedSourceHash: string) =>
    invoke<MediaRecord>('media_import_upgrade', {
      request: {
        ...request,
        expected_source_hash: expectedSourceHash,
        confirmed: true,
      },
    }),
  history: () => invoke<MediaRecord[]>('media_history_list'),
  verify: (mediaId: string, deviceId: string) =>
    invoke<MediaRecord>('media_history_verify', {
      request: { media_id: mediaId, device_id: deviceId },
    }),
  opticalRecorders: (deviceId: string) =>
    invoke<OpticalRecorder[]>('media_optical_recorders', {
      request: { device_id: deviceId },
    }),
  exportIso: (request: CreatorSelection) =>
    invoke<{
      operation_id: string;
      path: string;
      canonical_hash: string;
      artwork_included: boolean;
    }>('media_optical_export_iso', { request }),
  burn: (request: CreatorSelection, recorderId: string) =>
    invoke<MediaRecord>('media_optical_burn', {
      request: { ...request, recorder_id: recorderId, confirmed: true },
    }),
  cancelBurn: (operationId: string) =>
    invoke<void>('media_optical_cancel', {
      request: { operation_id: operationId },
    }),
  eraseChallenge: (deviceId: string) =>
    invoke<EraseChallenge>('media_optical_erase_challenge', {
      request: { device_id: deviceId },
    }),
  erase: (deviceId: string, recorderId: string, typedConfirmation: string) =>
    invoke<void>('media_optical_erase', {
      request: {
        device_id: deviceId,
        recorder_id: recorderId,
        typed_confirmation: typedConfirmation,
      },
    }),
  onOpticalProgress: (
    callback: (progress: OpticalProgress) => void,
  ): Promise<UnlistenFn> =>
    listen<OpticalProgress>('media://optical_progress', (event) =>
      callback(event.payload),
    ),
};
