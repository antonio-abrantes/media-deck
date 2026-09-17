import { invoke } from '@tauri-apps/api/core';

export type CollectionEntry = {
  game_id: string;
  display_name: string;
  provider: 'steam' | 'executable';
  provider_game_id: string | null;
  install_dir: string | null;
  installed: boolean;
  offline: boolean;
  profile_id: string;
  profile_name: string;
  last_export_kind: 'ini_file' | 'floppy' | 'optical';
  last_media_key: string;
  last_content_hash: string;
  schema_version: number;
  export_count: number;
  first_activated_at: string;
  last_exported_at: string;
  active_cover: { id: string; relative_path: string } | null;
};

export const collectionApi = {
  list: () => invoke<CollectionEntry[]>('collection_list'),
  get: (gameId: string) =>
    invoke<CollectionEntry | null>('collection_get', {
      request: { game_id: gameId },
    }),
  deactivate: (gameId: string) =>
    invoke<void>('collection_deactivate', {
      request: { game_id: gameId },
    }),
  cover: (gameId: string) =>
    invoke<{
      mime_type: string;
      bytes: number[];
    } | null>('collection_cover_get', {
      request: { game_id: gameId },
    }),
};
