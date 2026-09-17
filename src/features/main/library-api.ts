import { invoke } from '@tauri-apps/api/core';

export type LibraryEntry = {
  id: string;
  provider: 'steam' | 'executable';
  provider_game_id: string | null;
  display_name: string;
  install_dir: string | null;
  installed: boolean;
  offline: boolean;
};

export type LibraryDetails = {
  game: LibraryEntry;
  active_cover: {
    id: string;
    relative_path: string;
  } | null;
  profiles: Array<{
    id: string;
    name: string;
    kind: 'steam' | 'executable';
    steam_launch_option?: number | null;
    executable_path: string | null;
    working_directory: string | null;
    arguments: string[];
    process_hints: string[];
  }>;
};

export type ManualRegistration = {
  display_name: string;
  executable_path: string;
  working_directory: string | null;
  arguments: string[];
  process_hints: string[];
  cover_path: string | null;
};

export type ProfileUpdate = {
  profile_id: string;
  display_name: string;
  provider: 'steam' | 'executable';
  app_id: number | null;
  steam_launch_option: number | null;
  executable_path: string | null;
  working_directory: string | null;
  arguments: string[];
  process_hints: string[];
};

export const libraryApi = {
  list: () => invoke<LibraryEntry[]>('library_list'),
  details: (gameId: string) =>
    invoke<LibraryDetails>('library_get', { gameId }),
  scanSteam: () =>
    invoke<{
      discovered: number;
      inserted: number;
      updated: number;
      marked_uninstalled: number;
      unavailable_libraries: number;
    }>('library_scan_steam'),
  register: (request: ManualRegistration) =>
    invoke<LibraryEntry>('library_register_executable', { request }),
  updateProfile: (request: ProfileUpdate) =>
    invoke<LibraryEntry>('library_update_profile', { request }),
  setCover: (gameId: string, coverPath: string) =>
    invoke<void>('library_set_cover', {
      request: { game_id: gameId, cover_path: coverPath },
    }),
  reviewShortcut: (shortcutPath: string) =>
    invoke<{
      target_path: string;
      working_directory: string | null;
      arguments: string;
      resolved: boolean;
    }>('library_review_shortcut', { request: { shortcut_path: shortcutPath } }),
};
