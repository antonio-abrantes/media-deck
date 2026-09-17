import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MainWindow } from './MainWindow';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const games = [
  {
    id: 'game-1',
    provider: 'steam',
    provider_game_id: '620',
    display_name: 'Portal 2',
    install_dir: 'D:\\Steam\\Portal 2',
    installed: true,
    offline: false,
  },
  {
    id: 'game-2',
    provider: 'steam',
    provider_game_id: '10',
    display_name: 'Counter-Strike',
    install_dir: 'E:\\Steam\\Counter-Strike',
    installed: true,
    offline: true,
  },
];

const collectionEntry = {
  game_id: 'game-1',
  display_name: 'Portal 2',
  provider: 'steam',
  provider_game_id: '620',
  install_dir: 'D:\\Steam\\Portal 2',
  installed: true,
  offline: false,
  profile_id: '01994a56-69d7-7ef4-a137-94808fa24131',
  profile_name: 'Steam',
  last_export_kind: 'ini_file',
  last_media_key: 'PORTAL-001',
  last_content_hash: 'a'.repeat(64),
  schema_version: 2,
  export_count: 2,
  first_activated_at: '2026-09-16T12:00:00Z',
  last_exported_at: '2026-09-16T13:00:00Z',
  active_cover: null,
};

describe('MainWindow library', () => {
  beforeEach(() => {
    cleanup();
    invoke.mockReset();
    invoke.mockImplementation(
      (command: string, payload?: { gameId?: string }) => {
        if (command === 'collection_list') return Promise.resolve([]);
        if (command === 'library_list') return Promise.resolve(games);
        if (command === 'library_get') {
          const game =
            games.find((entry) => entry.id === payload?.gameId) ?? games[0];
          return Promise.resolve({
            game,
            profiles: [
              {
                id: '01994a56-69d7-7ef4-a137-94808fa24131',
                name: 'Steam',
                kind: 'steam',
                executable_path: null,
                working_directory: null,
                arguments: [],
                process_hints: [],
              },
            ],
          });
        }
        if (command === 'device_list') return Promise.resolve([]);
        if (command === 'media_history_list') return Promise.resolve([]);
        return Promise.resolve({});
      },
    );
  });

  it('loads, searches and filters local entries', async () => {
    render(<MainWindow />);
    fireEvent.click(screen.getByRole('tab', { name: 'ADICIONAR JOGOS' }));
    await screen.findAllByText('Portal 2');
    const list = screen.getByRole('region', { name: 'Jogos' });
    expect(within(list).getByText('Counter-Strike')).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText('Nome do jogo'), {
      target: { value: 'portal' },
    });
    expect(screen.getAllByText('Portal 2').length).toBeGreaterThan(0);
    expect(within(list).queryByText('Counter-Strike')).not.toBeInTheDocument();
  });

  it('opens Meus Jogos first and removes only confirmed membership', async () => {
    invoke.mockImplementation((command: string) => {
      if (command === 'collection_list')
        return Promise.resolve([collectionEntry]);
      if (command === 'collection_deactivate') return Promise.resolve();
      if (command === 'library_list') return Promise.resolve(games);
      return Promise.resolve({});
    });
    render(<MainWindow />);

    expect(
      await screen.findByRole('heading', { name: 'Portal 2' }),
    ).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'MEUS JOGOS' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    fireEvent.click(
      screen.getByRole('button', { name: 'REMOVER DOS MEUS JOGOS' }),
    );
    expect(invoke).not.toHaveBeenCalledWith(
      'collection_deactivate',
      expect.anything(),
    );
    fireEvent.click(screen.getByRole('button', { name: 'CANCELAR' }));
    expect(
      screen.getByRole('heading', { name: 'Portal 2' }),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole('button', { name: 'REMOVER DOS MEUS JOGOS' }),
    );
    fireEvent.click(screen.getByRole('button', { name: 'CONFIRMAR REMOÇÃO' }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('collection_deactivate', {
        request: { game_id: 'game-1' },
      }),
    );
    expect(screen.getByText(/Jogo removido da coleção/i)).toBeInTheDocument();
  });

  it('retries the initial collection load while native services start', async () => {
    let attempts = 0;
    invoke.mockImplementation((command: string) => {
      if (command === 'collection_list') {
        attempts += 1;
        return attempts === 1
          ? Promise.reject(new Error('state not ready'))
          : Promise.resolve([collectionEntry]);
      }
      if (command === 'library_list') return Promise.resolve(games);
      return Promise.resolve({});
    });
    render(<MainWindow />);

    expect(
      await screen.findByRole('heading', { name: 'Portal 2' }),
    ).toBeInTheDocument();
    expect(attempts).toBeGreaterThanOrEqual(2);
  });

  it('previews the selected cover with the runtime labels', async () => {
    invoke.mockImplementation((command: string) => {
      if (command === 'collection_list')
        return Promise.resolve([
          {
            ...collectionEntry,
            active_cover: { id: 'cover-1', relative_path: 'aa/cover.png' },
          },
        ]);
      if (command === 'collection_cover_get')
        return Promise.resolve({
          mime_type: 'image/png',
          bytes: [137, 80, 78, 71],
        });
      if (command === 'library_list') return Promise.resolve(games);
      return Promise.resolve({});
    });
    render(<MainWindow />);

    fireEvent.click(
      await screen.findByRole('button', { name: 'PRÉ-VISUALIZAR' }),
    );
    const dialog = screen.getByRole('dialog', {
      name: 'Portal 2',
    });
    expect(within(dialog).getByText('PORTAL-001')).toBeInTheDocument();
    expect(within(dialog).getByText('PROVIDED BY STEAM')).toBeInTheDocument();
    expect(
      within(dialog).getByText('FLOPPY DISK GAME SERIES'),
    ).toBeInTheDocument();
  });

  it('only fills resolved shortcut fields for explicit review', async () => {
    invoke.mockImplementation((command: string) => {
      if (command === 'collection_list') return Promise.resolve([]);
      if (command === 'library_list') return Promise.resolve([]);
      if (command === 'library_review_shortcut') {
        return Promise.resolve({
          target_path: 'D:\\Games\\Game.exe',
          working_directory: 'D:\\Games',
          arguments: '-windowed',
          resolved: true,
        });
      }
      return Promise.resolve({});
    });
    render(<MainWindow />);
    fireEvent.click(screen.getByRole('tab', { name: 'ADICIONAR JOGOS' }));
    fireEvent.click(screen.getByRole('button', { name: '+ CADASTRAR LOCAL' }));
    fireEvent.change(screen.getByLabelText('Caminho do atalho .lnk'), {
      target: { value: 'D:\\Games\\Game.lnk' },
    });
    fireEvent.click(
      screen.getByRole('button', { name: 'RESOLVER SEM EXECUTAR' }),
    );

    await waitFor(() =>
      expect(
        screen.getByDisplayValue('D:\\Games\\Game.exe'),
      ).toBeInTheDocument(),
    );
    expect(screen.getByText(/não salvos automaticamente/i)).toBeInTheDocument();
    expect(screen.queryByDisplayValue('-windowed')).not.toBeInTheDocument();
  });

  it('renders SVG outlines for administrative mechanical panels', () => {
    const { container } = render(<MainWindow />);
    fireEvent.click(screen.getByRole('tab', { name: 'ADICIONAR JOGOS' }));
    expect(
      container.querySelectorAll('[data-chamfer-outline]').length,
    ).toBeGreaterThanOrEqual(4);
  });

  it('generates the manifest without physical-media fields', async () => {
    invoke.mockImplementation(
      (
        command: string,
        payload?: {
          request?: {
            device_id?: string | null;
            media_kind?: string | null;
          };
        },
      ) => {
        if (command === 'collection_list') return Promise.resolve([]);
        if (command === 'library_list') return Promise.resolve(games);
        if (command === 'library_get') {
          return Promise.resolve({
            game: games[0],
            profiles: [
              {
                id: '01994a56-69d7-7ef4-a137-94808fa24131',
                name: 'Steam',
                kind: 'steam',
                executable_path: null,
                working_directory: null,
                arguments: [],
                process_hints: [],
              },
            ],
          });
        }
        if (command === 'library_update_profile') return Promise.resolve({});
        if (command === 'media_creator_preview') {
          expect(payload?.request?.device_id).toBeNull();
          expect(payload?.request?.media_kind).toBeNull();
          return Promise.resolve({
            profile: {
              media_id: 'PORTAL-001',
              media_kind: null,
              profile_id: '01994a56-69d7-7ef4-a137-94808fa24131',
              provider: 'steam',
              app_id: 400,
              display_name: 'Portal',
              process_hints: [],
              cover_artwork_id: null,
              cover_cache_path: null,
            },
            canonical_ini: '[MEDIA]\r\nSCHEMA=2\r\nMEDIA_ID=PORTAL-001\r\n',
            device_name: null,
            mount_point: null,
            include_artwork: false,
          });
        }
        return Promise.resolve({});
      },
    );
    render(<MainWindow />);
    fireEvent.click(screen.getByRole('tab', { name: 'MEDIA CREATOR' }));
    await screen.findAllByDisplayValue('Portal 2');
    fireEvent.change(screen.getByPlaceholderText('GAME-001'), {
      target: { value: 'PORTAL-001' },
    });
    fireEvent.click(
      screen.getByRole('button', { name: 'SALVAR PERFIL E GERAR PREVIEW' }),
    );
    await screen.findByLabelText('Preview do GAME.INI');
    expect(screen.queryByText(/RECORDER IMAPI/i)).not.toBeInTheDocument();
    expect(
      screen.queryByText(/UNIDADE PARA GRAVAÇÃO/i),
    ).not.toBeInTheDocument();
  });
});
