import {
  lazy,
  Suspense,
  type FormEvent,
  useCallback,
  useEffect,
  useMemo,
  useState,
} from 'react';
import {
  open as chooseLocalPath,
  save as chooseExportPath,
} from '@tauri-apps/plugin-dialog';
import { WindowChassis } from '@/shared/components/WindowChassis';
import { ResponsiveChamferOutline } from '@/shared/components/ChamferOutline/ResponsiveChamferOutline';
import {
  libraryApi,
  type LibraryDetails,
  type LibraryEntry,
  type ManualRegistration,
} from './library-api';
import { mediaCreatorApi, type CreatorPreview } from './media-creator-api';
import { CollectionPanel } from './CollectionPanel';
import { collectionApi, type CollectionEntry } from './collection-api';
import { SettingsPanel } from './SettingsPanel';
import './MainWindow.css';

const LabelStudio = lazy(() =>
  import('@/features/label-studio/LabelStudio').then((module) => ({
    default: module.LabelStudio,
  })),
);

type MainModule = 'collection' | 'library' | 'creator' | 'label' | 'settings';

function initialMainModule(): MainModule {
  const requested = new URLSearchParams(window.location.search).get('module');
  return requested === 'library' ||
    requested === 'creator' ||
    requested === 'label' ||
    requested === 'settings'
    ? requested
    : 'collection';
}

const EMPTY_FORM: ManualRegistration = {
  display_name: '',
  executable_path: '',
  working_directory: '',
  arguments: [],
  process_hints: [],
  cover_path: '',
};

type CreatorProfileForm = {
  displayName: string;
  provider: 'steam' | 'executable';
  appId: string;
  steamLaunchOption: string;
  executablePath: string;
  workingDirectory: string;
  arguments: string;
  processHints: string;
};

const EMPTY_CREATOR_PROFILE: CreatorProfileForm = {
  displayName: '',
  provider: 'executable',
  appId: '',
  steamLaunchOption: '',
  executablePath: '',
  workingDirectory: '',
  arguments: '',
  processHints: '',
};

export function MainWindow() {
  const [activeModule, setActiveModule] =
    useState<MainModule>(initialMainModule);
  const [games, setGames] = useState<LibraryEntry[]>([]);
  const [selected, setSelected] = useState<LibraryDetails | null>(null);
  const [query, setQuery] = useState('');
  const [provider, setProvider] = useState('all');
  const [availability, setAvailability] = useState('all');
  const [form, setForm] = useState(EMPTY_FORM);
  const [shortcutPath, setShortcutPath] = useState('');
  const [shortcutArgs, setShortcutArgs] = useState('');
  const [notice, setNotice] = useState('Biblioteca local pronta.');
  const [busy, setBusy] = useState(false);
  const [mediaId, setMediaId] = useState('');
  const [preview, setPreview] = useState<CreatorPreview | null>(null);
  const [creatorProfile, setCreatorProfile] = useState(EMPTY_CREATOR_PROFILE);
  const [selectedCoverPath, setSelectedCoverPath] = useState('');
  const [registerOpen, setRegisterOpen] = useState(false);
  const [profileEditOpen, setProfileEditOpen] = useState(false);
  const [profileEdit, setProfileEdit] = useState(EMPTY_FORM);
  const [profileEditSteamOption, setProfileEditSteamOption] = useState('');
  const [selectedProfileId, setSelectedProfileId] = useState('');
  const [collectionRefreshToken, setCollectionRefreshToken] = useState(0);
  const [activatedGameIds, setActivatedGameIds] = useState<string[]>([]);
  const [showActivated, setShowActivated] = useState(false);
  const currentProfile =
    selected?.profiles.find((profile) => profile.id === selectedProfileId) ??
    selected?.profiles[0] ??
    null;

  const applySelected = useCallback(
    (details: LibraryDetails, explicitProfileId?: string) => {
      const profile =
        details.profiles.find((item) => item.id === explicitProfileId) ??
        details.profiles[0];
      setSelectedProfileId(profile?.id ?? '');
      setSelected(details);
      setCreatorProfile(
        profile
          ? {
              displayName: details.game.display_name,
              provider: details.game.provider,
              appId: details.game.provider_game_id ?? '',
              steamLaunchOption:
                profile.steam_launch_option?.toString() ?? '',
              executablePath: profile.executable_path ?? '',
              workingDirectory: profile.working_directory ?? '',
              arguments: profile.arguments.join('\n'),
              processHints: profile.process_hints.join('\n'),
            }
          : EMPTY_CREATOR_PROFILE,
      );
      setSelectedCoverPath(details.active_cover?.relative_path ?? '');
    },
    [],
  );

  const refresh = async () => {
    const entries = await libraryApi.list();
    setGames(entries);
    if (entries.length && !selected) {
      applySelected(await libraryApi.details(entries[0].id));
    }
  };

  const openProfileEditor = () => {
    const profile = currentProfile;
    if (!selected || !profile) return;
    setProfileEdit({
      display_name: selected.game.display_name,
      executable_path: profile.executable_path ?? '',
      working_directory: profile.working_directory,
      arguments: profile.arguments,
      process_hints: profile.process_hints,
      cover_path: '',
    });
    setProfileEditSteamOption(profile.steam_launch_option?.toString() ?? '');
    setProfileEditOpen(true);
  };

  const updateProfile = async (event: FormEvent) => {
    event.preventDefault();
    const profile = currentProfile;
    if (!selected || !profile) return;
    setBusy(true);
    try {
      await libraryApi.updateProfile({
        profile_id: profile.id,
        display_name: profileEdit.display_name,
        provider: selected.game.provider,
        app_id:
          selected.game.provider === 'steam' &&
          selected.game.provider_game_id !== null
            ? Number(selected.game.provider_game_id)
            : null,
        steam_launch_option:
          profile.kind === 'steam' && profileEditSteamOption !== ''
            ? Number(profileEditSteamOption)
            : null,
        executable_path:
          profile.kind === 'executable'
            ? profileEdit.executable_path || null
            : null,
        working_directory:
          profile.kind === 'executable'
            ? profileEdit.working_directory || null
            : null,
        arguments: profile.kind === 'executable' ? profileEdit.arguments : [],
        process_hints: profileEdit.process_hints,
      });
      applySelected(await libraryApi.details(selected.game.id));
      setGames(await libraryApi.list());
      setProfileEditOpen(false);
      setNotice('Perfil atualizado e pronto para gerar GAME.INI.');
    } catch {
      setNotice('Perfil recusado. Confira executável, argumentos e hints.');
    } finally {
      setBusy(false);
    }
  };

  useEffect(() => {
    void libraryApi
      .list()
      .then(async (entries) => {
        setGames(entries);
        if (entries.length) {
          applySelected(await libraryApi.details(entries[0].id));
        }
      })
      .catch(() =>
        setNotice('Abra no MediaDeck para carregar a biblioteca local.'),
      );
    void collectionApi
      .list()
      .then((entries) =>
        setActivatedGameIds(entries.map((entry) => entry.game_id)),
      )
      .catch(() => undefined);
    // Initial IPC load only.
  }, [applySelected]);

  const manifestSelection = () => {
    const profile = currentProfile;
    if (!selected || !profile) return null;
    return {
      game_id: selected.game.id,
      profile_id: profile.id,
      device_id: null,
      media_id: mediaId,
      media_kind: null,
      include_artwork: false,
    };
  };

  const buildPreview = async () => {
    const request = manifestSelection();
    if (!request) return;
    if (!mediaIdIsValid(mediaId)) {
      setNotice(
        'MEDIA_ID inválido. Use de 1 a 64 letras, números, ponto, hífen ou sublinhado, sem espaços.',
      );
      return;
    }
    if (!creatorProfileIsValid(creatorProfile)) {
      setNotice('Informe um APP_ID Steam numérico maior que zero.');
      return;
    }
    setBusy(true);
    try {
      await persistCreatorProfile();
      setPreview(await mediaCreatorApi.preview(request));
      setNotice('Perfil salvo e preview canônico gerado pelo core.');
    } catch (error) {
      setPreview(null);
      setNotice(
        ipcErrorNotice(
          'Preview rejeitado. Confira o perfil e o MEDIA_ID.',
          error,
        ),
      );
    } finally {
      setBusy(false);
    }
  };

  const exportIni = async () => {
    const request = manifestSelection();
    if (!request) return;
    if (!mediaIdIsValid(mediaId)) {
      setNotice(
        'MEDIA_ID inválido. Use de 1 a 64 letras, números, ponto, hífen ou sublinhado, sem espaços.',
      );
      return;
    }
    if (!creatorProfileIsValid(creatorProfile)) {
      setNotice('Informe um APP_ID Steam numérico maior que zero.');
      return;
    }
    setBusy(true);
    try {
      const destination = await chooseExportPath({
        defaultPath: 'GAME.INI',
        filters: [{ name: 'Manifesto MediaDeck', extensions: ['ini'] }],
      });
      if (!destination) {
        setNotice('Exportação do GAME.INI cancelada.');
        return;
      }
      await persistCreatorProfile();
      const receipt = await mediaCreatorApi.exportIni(request, destination);
      setActivatedGameIds((current) =>
        current.includes(receipt.game_id)
          ? current
          : [...current, receipt.game_id],
      );
      setCollectionRefreshToken((value) => value + 1);
      setNotice(
        `GAME.INI verificado em ${receipt.path}. O jogo agora está em Meus Jogos.`,
      );
    } catch (error) {
      setNotice(ipcErrorNotice('Não foi possível exportar o GAME.INI.', error));
    } finally {
      setBusy(false);
    }
  };

  const persistCreatorProfile = async () => {
    const profile = currentProfile;
    if (!selected || !profile) throw new Error('Perfil ausente');
    await libraryApi.updateProfile({
      profile_id: profile.id,
      display_name: creatorProfile.displayName,
      provider: creatorProfile.provider,
      app_id:
        creatorProfile.provider === 'steam'
          ? Number(creatorProfile.appId)
          : null,
      steam_launch_option:
        creatorProfile.provider === 'steam' &&
        creatorProfile.steamLaunchOption !== ''
          ? Number(creatorProfile.steamLaunchOption)
          : null,
      executable_path:
        creatorProfile.provider === 'executable'
          ? creatorProfile.executablePath
          : null,
      working_directory:
        creatorProfile.provider === 'executable'
          ? creatorProfile.workingDirectory || null
          : null,
      arguments:
        creatorProfile.provider === 'executable'
          ? splitStructuredLines(creatorProfile.arguments)
          : [],
      process_hints: splitStructuredLines(creatorProfile.processHints),
    });
  };

  const selectCoverForGame = async (gameId: string) => {
    const path = await chooseLocalPath({
      multiple: false,
      directory: false,
      filters: [
        {
          name: 'Imagem de capa',
          extensions: ['png', 'jpg', 'jpeg', 'webp'],
        },
      ],
    });
    if (typeof path !== 'string') return;
    setBusy(true);
    try {
      await libraryApi.setCover(gameId, path);
      const details = await libraryApi.details(gameId);
      if (selected?.game.id === gameId) {
        setSelected((current) =>
          current
            ? { ...current, active_cover: details.active_cover }
            : details,
        );
      }
      setSelectedCoverPath(details.active_cover?.relative_path ?? path);
      setPreview(null);
      setCollectionRefreshToken((value) => value + 1);
      setNotice('Capa copiada para o cache e associada ao jogo.');
    } catch {
      setNotice('Capa recusada. Use PNG, JPEG ou WEBP local de até 16 MB.');
    } finally {
      setBusy(false);
    }
  };

  const selectCreatorCover = async () => {
    if (selected) await selectCoverForGame(selected.game.id);
  };

  const editCollectionProfile = async (entry: CollectionEntry) => {
    const details = await libraryApi.details(entry.game_id);
    applySelected(details, entry.profile_id);
    const profile = details.profiles.find(
      (candidate) => candidate.id === entry.profile_id,
    );
    if (!profile) return;
    setProfileEdit({
      display_name: details.game.display_name,
      executable_path: profile.executable_path ?? '',
      working_directory: profile.working_directory,
      arguments: profile.arguments,
      process_hints: profile.process_hints,
      cover_path: '',
    });
    setActiveModule('library');
    setProfileEditOpen(true);
  };

  const regenerateCollectionIni = async (entry: CollectionEntry) => {
    const details = await libraryApi.details(entry.game_id);
    applySelected(details, entry.profile_id);
    setMediaId(entry.last_media_key);
    setPreview(null);
    setActiveModule('creator');
  };

  const visibleGames = useMemo(
    () =>
      games.filter((game) => {
        const searchMatch = game.display_name
          .toLocaleLowerCase()
          .includes(query.trim().toLocaleLowerCase());
        const providerMatch = provider === 'all' || game.provider === provider;
        const stateMatch =
          availability === 'all' ||
          (availability === 'installed' && game.installed && !game.offline) ||
          (availability === 'offline' && game.offline) ||
          (availability === 'missing' && !game.installed);
        const activationMatch =
          showActivated || !activatedGameIds.includes(game.id);
        return searchMatch && providerMatch && stateMatch && activationMatch;
      }),
    [activatedGameIds, availability, games, provider, query, showActivated],
  );

  const scan = async () => {
    setBusy(true);
    try {
      const report = await libraryApi.scanSteam();
      await refresh();
      setNotice(
        `Scan concluído: ${report.discovered} encontrados, ${report.inserted} novos, ${report.unavailable_libraries} bibliotecas offline.`,
      );
    } catch {
      setNotice('Não foi possível ler a biblioteca Steam local.');
    } finally {
      setBusy(false);
    }
  };

  const inspectShortcut = async () => {
    setBusy(true);
    try {
      const review = await libraryApi.reviewShortcut(shortcutPath);
      setForm((current) => ({
        ...current,
        executable_path: review.target_path,
        working_directory: review.working_directory ?? '',
      }));
      setShortcutArgs(review.arguments);
      setNotice(
        'Target resolvido. Revise o executável e converta cada argumento em um item antes de salvar.',
      );
    } catch {
      setNotice('Atalho rejeitado: nenhum target .exe local foi resolvido.');
    } finally {
      setBusy(false);
    }
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    try {
      const created = await libraryApi.register({
        ...form,
        working_directory: form.working_directory || null,
        cover_path: form.cover_path || null,
        arguments: form.arguments.filter(Boolean),
        process_hints: form.process_hints.filter(Boolean),
      });
      setForm(EMPTY_FORM);
      setShortcutArgs('');
      await refresh();
      applySelected(await libraryApi.details(created.id));
      setNotice('Executável cadastrado com perfil local seguro.');
      setRegisterOpen(false);
    } catch {
      setNotice('Cadastro rejeitado. Confira caminhos locais, .exe e campos.');
    } finally {
      setBusy(false);
    }
  };

  return (
    <WindowChassis
      className="main-window"
      outlineFollowsViewport
      subtitle={
        activeModule === 'collection'
          ? 'PERSONAL GAME COLLECTION'
          : activeModule === 'library'
            ? 'GAME DISCOVERY'
            : activeModule === 'creator'
              ? 'MEDIA CREATOR'
              : activeModule === 'label'
                ? 'LABEL STUDIO'
                : 'BACKGROUND & DEVICES'
      }
    >
      <main
        className={`main-shell ${activeModule === 'label' ? 'main-shell--studio' : ''}`}
        aria-label={
          activeModule === 'collection'
            ? 'Meus Jogos'
            : activeModule === 'library'
              ? 'Adicionar jogos'
              : activeModule === 'creator'
                ? 'Media Creator'
                : activeModule === 'label'
                  ? 'Label Studio'
                  : 'Settings'
        }
      >
        <nav
          className="main-nav"
          aria-label="Módulos administrativos"
          role="tablist"
        >
          <button
            type="button"
            role="tab"
            aria-selected={activeModule === 'collection'}
            aria-current={activeModule === 'collection' ? 'page' : undefined}
            onClick={() => setActiveModule('collection')}
          >
            MEUS JOGOS
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeModule === 'library'}
            aria-current={activeModule === 'library' ? 'page' : undefined}
            onClick={() => setActiveModule('library')}
          >
            ADICIONAR JOGOS
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeModule === 'creator'}
            aria-current={activeModule === 'creator' ? 'page' : undefined}
            onClick={() => setActiveModule('creator')}
          >
            MEDIA CREATOR
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeModule === 'label'}
            aria-current={activeModule === 'label' ? 'page' : undefined}
            onClick={() => setActiveModule('label')}
          >
            LABEL STUDIO
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeModule === 'settings'}
            aria-current={activeModule === 'settings' ? 'page' : undefined}
            onClick={() => setActiveModule('settings')}
          >
            SETTINGS
          </button>
        </nav>

        {activeModule === 'settings' ? (
          <SettingsPanel />
        ) : activeModule === 'collection' ? (
          <CollectionPanel
            refreshToken={collectionRefreshToken}
            onAddGames={() => setActiveModule('library')}
            onEditProfile={(entry) => void editCollectionProfile(entry)}
            onChangeCover={(entry) => void selectCoverForGame(entry.game_id)}
            onRegenerate={(entry) => void regenerateCollectionIni(entry)}
            onDeactivated={(gameId) =>
              setActivatedGameIds((current) =>
                current.filter((id) => id !== gameId),
              )
            }
          />
        ) : activeModule === 'label' ? (
          <Suspense
            fallback={
              <p className="label-studio-loading" role="status">
                CARREGANDO LABEL STUDIO…
              </p>
            }
          >
            <LabelStudio />
          </Suspense>
        ) : activeModule === 'creator' ? (
          <section
            id="media-creator"
            className="media-creator mechanical-panel"
            aria-label="Media Creator"
            role="tabpanel"
          >
            <ResponsiveChamferOutline
              cut={10}
              layers={[
                { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
                { inset: 4.5, color: '#343c44', strokeWidth: 1 },
              ]}
            />
            <header>
              <p className="eyebrow">WP-0710–0718</p>
              <h2>MEDIA CREATOR</h2>
            </header>
            <div className="creator-grid">
              <h3 className="creator-step">1 · JOGO E PERFIL</h3>
              <label>
                <span>JOGO</span>
                <select
                  value={selected?.game.id ?? ''}
                  onChange={(event) =>
                    void libraryApi
                      .details(event.target.value)
                      .then((details) => {
                        applySelected(details);
                        setPreview(null);
                      })
                  }
                >
                  {games.map((game) => (
                    <option key={game.id} value={game.id}>
                      {game.display_name}
                    </option>
                  ))}
                </select>
                <small>{currentProfile?.name ?? 'Nenhum perfil válido'}</small>
              </label>
              <label>
                <span>ID INTERNO DO PERFIL (PROFILE_ID)</span>
                <input
                  value={currentProfile?.id ?? ''}
                  readOnly
                  aria-readonly="true"
                />
                <small>Gerado automaticamente pelo MediaDeck.</small>
              </label>
              <label>
                <span>DISPLAY_NAME</span>
                <input
                  value={creatorProfile.displayName}
                  onChange={(event) =>
                    setCreatorProfile({
                      ...creatorProfile,
                      displayName: event.target.value,
                    })
                  }
                />
              </label>
              <label>
                <span>FORMA DE ABERTURA (PROVIDER)</span>
                <select
                  value={creatorProfile.provider}
                  onChange={(event) => {
                    setCreatorProfile({
                      ...creatorProfile,
                      provider: event.target.value as 'steam' | 'executable',
                    });
                    setPreview(null);
                  }}
                >
                  <option value="steam">Steam</option>
                  <option value="executable">Executável local</option>
                </select>
                <small>Escolha Steam ou um arquivo .exe instalado.</small>
              </label>
              {creatorProfile.provider === 'steam' ? (
                <>
                  <label>
                    <span>ID DO JOGO NA STEAM (APP_ID)</span>
                    <input
                      inputMode="numeric"
                      value={creatorProfile.appId}
                      onChange={(event) =>
                        setCreatorProfile({
                          ...creatorProfile,
                          appId: event.target.value.replace(/\D/g, ''),
                        })
                      }
                    />
                    <small>Número do jogo na Steam, por exemplo 851850.</small>
                  </label>
                  <SteamLaunchOptionSelect
                    value={creatorProfile.steamLaunchOption}
                    onChange={(steamLaunchOption) =>
                      setCreatorProfile({
                        ...creatorProfile,
                        steamLaunchOption,
                      })
                    }
                  />
                </>
              ) : (
                <>
                  <label className="creator-path-field">
                    <span>EXECUTÁVEL DO JOGO</span>
                    <input
                      value={creatorProfile.executablePath}
                      onChange={(event) =>
                        setCreatorProfile({
                          ...creatorProfile,
                          executablePath: event.target.value,
                        })
                      }
                    />
                    <button
                      type="button"
                      onClick={() =>
                        void pickLocalPath({
                          name: 'Executável Windows',
                          extensions: ['exe'],
                        }).then((path) => {
                          if (!path) return;
                          setCreatorProfile({
                            ...creatorProfile,
                            executablePath: path,
                            workingDirectory:
                              creatorProfile.workingDirectory ||
                              parentDirectory(path),
                            processHints:
                              creatorProfile.processHints || fileName(path),
                          });
                        })
                      }
                    >
                      PROCURAR .EXE…
                    </button>
                    <small>Selecione o arquivo que inicia o jogo.</small>
                  </label>
                  <label className="creator-path-field">
                    <span>PASTA DE TRABALHO · OPCIONAL</span>
                    <input
                      value={creatorProfile.workingDirectory}
                      onChange={(event) =>
                        setCreatorProfile({
                          ...creatorProfile,
                          workingDirectory: event.target.value,
                        })
                      }
                    />
                    <button
                      type="button"
                      onClick={() =>
                        void pickLocalDirectory().then((path) => {
                          if (!path) return;
                          setCreatorProfile({
                            ...creatorProfile,
                            workingDirectory: path,
                          });
                        })
                      }
                    >
                      ESCOLHER PASTA…
                    </button>
                    <small>
                      Pasta usada pelo jogo ao iniciar. Deixe vazia para usar a
                      pasta do executável.
                    </small>
                  </label>
                  <label className="creator-multiline">
                    <span>OPÇÕES DE INICIALIZAÇÃO · OPCIONAL</span>
                    <textarea
                      placeholder="-windowed"
                      value={creatorProfile.arguments}
                      onChange={(event) =>
                        setCreatorProfile({
                          ...creatorProfile,
                          arguments: event.target.value,
                        })
                      }
                    />
                    <small>
                      Uma opção por linha. Normalmente pode ficar vazio.
                    </small>
                  </label>
                </>
              )}
              <label className="creator-multiline">
                <span>PROCESSOS ESPERADOS DO JOGO</span>
                <textarea
                  placeholder="Speed.exe"
                  value={creatorProfile.processHints}
                  onChange={(event) =>
                    setCreatorProfile({
                      ...creatorProfile,
                      processHints: event.target.value,
                    })
                  }
                />
                <small>
                  Nome do processo aberto pelo jogo, por exemplo Speed.exe. Um
                  por linha.
                </small>
              </label>
              <h3 className="creator-step">2 · MANIFESTO E CAPA</h3>
              <label>
                <span>IDENTIFICADOR DO ARQUIVO (MEDIA_ID)</span>
                <input
                  value={mediaId}
                  onChange={(event) => {
                    setMediaId(event.target.value.toUpperCase());
                    setPreview(null);
                  }}
                  pattern="[A-Za-z0-9._-]{1,64}"
                  placeholder="GAME-001"
                />
                <small>Nome curto único, sem espaços. Ex.: NFSU-001.</small>
              </label>
              <section className="creator-cover-field">
                <span>CAPA DO JOGO</span>
                <strong>
                  {selectedCoverPath ||
                    preview?.profile.cover_cache_path ||
                    'Nenhuma capa ativa'}
                </strong>
                <button
                  type="button"
                  disabled={busy || !selected}
                  onClick={() => void selectCreatorCover()}
                >
                  ESCOLHER CAPA…
                </button>
              </section>
              <h3 className="creator-step">3 · GERAR ARQUIVO</h3>
              <div className="creator-actions">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void buildPreview()}
                >
                  SALVAR PERFIL E GERAR PREVIEW
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void exportIni()}
                >
                  SALVAR PERFIL E GAME.INI…
                </button>
              </div>
            </div>
            {preview && (
              <section
                className="creator-confirmation"
                aria-label="Preview do manifesto"
              >
                <div>
                  <p>
                    MANIFESTO PORTÁTIL ·{' '}
                    {preview.profile.cover_artwork_id
                      ? `CAPA ${preview.profile.cover_artwork_id}`
                      : 'SEM CAPA ATIVA'}
                  </p>
                  {preview.profile.cover_cache_path && (
                    <p>CACHE RELATIVO: {preview.profile.cover_cache_path}</p>
                  )}
                  <pre aria-label="Preview do GAME.INI">
                    {preview.canonical_ini}
                  </pre>
                </div>
              </section>
            )}
          </section>
        ) : (
          <div id="discovery-panel" role="tabpanel">
            <section className="library-toolbar mechanical-panel">
              <ResponsiveChamferOutline
                cut={9}
                layers={[
                  { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
                  { inset: 4.5, color: '#343c44', strokeWidth: 1 },
                ]}
              />
              <label>
                <span>BUSCAR</span>
                <input
                  type="search"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder="Nome do jogo"
                />
              </label>
              <label>
                <span>PROVIDER</span>
                <select
                  value={provider}
                  onChange={(event) => setProvider(event.target.value)}
                >
                  <option value="all">Todos</option>
                  <option value="steam">Steam</option>
                  <option value="executable">Executável</option>
                </select>
              </label>
              <label>
                <span>ESTADO</span>
                <select
                  value={availability}
                  onChange={(event) => setAvailability(event.target.value)}
                >
                  <option value="all">Todos</option>
                  <option value="installed">Instalado</option>
                  <option value="offline">Offline</option>
                  <option value="missing">Não instalado</option>
                </select>
              </label>
              <button type="button" onClick={() => void scan()} disabled={busy}>
                SCAN STEAM
              </button>
              <button
                type="button"
                className="register-trigger"
                onClick={() => setRegisterOpen(true)}
              >
                + CADASTRAR LOCAL
              </button>
              <label className="discovery-toggle">
                <input
                  type="checkbox"
                  checked={showActivated}
                  onChange={(event) => setShowActivated(event.target.checked)}
                />
                <span>MOSTRAR JÁ ADICIONADOS</span>
              </label>
            </section>

            <section className="library-workspace">
              <section
                className="game-list mechanical-panel"
                aria-label="Jogos"
              >
                <ResponsiveChamferOutline
                  cut={10}
                  layers={[
                    { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
                    { inset: 4.5, color: '#343c44', strokeWidth: 1 },
                  ]}
                />
                <header>
                  <h2>LOCAL ARCHIVE</h2>
                  <span>{visibleGames.length.toString().padStart(3, '0')}</span>
                </header>
                <div className="game-list__scroll">
                  {visibleGames.map((game) => (
                    <button
                      key={game.id}
                      type="button"
                      className={
                        selected?.game.id === game.id ? 'is-selected' : ''
                      }
                      onClick={() =>
                        void libraryApi.details(game.id).then(applySelected)
                      }
                    >
                      <span>{game.display_name}</span>
                      <small>
                        {activatedGameIds.includes(game.id)
                          ? `EM MEUS JOGOS · ${stateLabel(game)}`
                          : stateLabel(game)}
                      </small>
                    </button>
                  ))}
                  {!visibleGames.length && <p>Nenhum título neste filtro.</p>}
                </div>
              </section>

              <section
                className="game-details mechanical-panel"
                aria-label="Detalhes do jogo"
              >
                <ResponsiveChamferOutline
                  cut={10}
                  layers={[
                    { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
                    { inset: 4.5, color: '#343c44', strokeWidth: 1 },
                  ]}
                />
                {selected ? (
                  <>
                    <p className="eyebrow">
                      {selected.game.provider.toUpperCase()}
                    </p>
                    <h2>{selected.game.display_name}</h2>
                    <p
                      className={`availability availability--${stateKey(selected.game)}`}
                    >
                      {stateLabel(selected.game)}
                    </p>
                    <dl>
                      <div>
                        <dt>APP ID</dt>
                        <dd>{selected.game.provider_game_id ?? 'LOCAL'}</dd>
                      </div>
                      <div>
                        <dt>INSTALL DIRECTORY</dt>
                        <dd>{selected.game.install_dir ?? 'Não informado'}</dd>
                      </div>
                    </dl>
                    {selected.profiles.map((profile) => (
                      <article className="profile-details" key={profile.id}>
                        <h3>{profile.name}</h3>
                        <p>
                          {profile.executable_path ??
                            'Steam URI validada pelo core'}
                        </p>
                        <p>
                          Working dir: {profile.working_directory ?? 'provider'}
                        </p>
                        <p>Args: {profile.arguments.join(' · ') || 'nenhum'}</p>
                        <p>
                          Hints:{' '}
                          {profile.process_hints.join(' · ') || 'provider'}
                        </p>
                      </article>
                    ))}
                    <button type="button" onClick={openProfileEditor}>
                      EDITAR PERFIL
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        setPreview(null);
                        setActiveModule('creator');
                      }}
                    >
                      CONFIGURAR E GERAR INI
                    </button>
                  </>
                ) : (
                  <p>Selecione um jogo para ver os detalhes.</p>
                )}
              </section>
            </section>

            {profileEditOpen && selected && currentProfile && (
              <aside
                className="register-drawer"
                role="dialog"
                aria-modal="true"
                aria-labelledby="profile-edit-title"
                onKeyDown={(event) => {
                  if (event.key === 'Escape') setProfileEditOpen(false);
                }}
              >
                <button
                  type="button"
                  className="register-backdrop"
                  aria-label="Fechar editor de perfil"
                  onClick={() => setProfileEditOpen(false)}
                />
                <section className="manual-register mechanical-panel">
                  <ResponsiveChamferOutline
                    cut={10}
                    layers={[
                      { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
                      { inset: 4.5, color: '#343c44', strokeWidth: 1 },
                    ]}
                  />
                  <header>
                    <h2 id="profile-edit-title">EDITAR PERFIL DE EXECUÇÃO</h2>
                    <button
                      type="button"
                      aria-label="Fechar editor de perfil"
                      onClick={() => setProfileEditOpen(false)}
                      autoFocus
                    >
                      ×
                    </button>
                  </header>
                  <form onSubmit={(event) => void updateProfile(event)}>
                    <PathField
                      label="DISPLAY NAME"
                      value={profileEdit.display_name}
                      onChange={(value) =>
                        setProfileEdit({ ...profileEdit, display_name: value })
                      }
                      required
                    />
                    <PathField
                      label="PROVIDER"
                      value={selected.game.provider}
                      onChange={() => undefined}
                      disabled
                    />
                    <PathField
                      label="APP ID"
                      value={selected.game.provider_game_id ?? 'Não aplicável'}
                      onChange={() => undefined}
                      disabled
                    />
                    {currentProfile.kind === 'steam' && (
                      <SteamLaunchOptionSelect
                        value={profileEditSteamOption}
                        onChange={setProfileEditSteamOption}
                      />
                    )}
                    {currentProfile.kind === 'executable' && (
                      <>
                        <PathField
                          label="CAMINHO .EXE ABSOLUTO"
                          value={profileEdit.executable_path}
                          onChange={(value) =>
                            setProfileEdit({
                              ...profileEdit,
                              executable_path: value,
                            })
                          }
                          required
                          browseLabel="PROCURAR .EXE…"
                          onBrowse={() =>
                            void pickLocalPath({
                              name: 'Executável Windows',
                              extensions: ['exe'],
                            }).then((path) => {
                              if (!path) return;
                              setProfileEdit({
                                ...profileEdit,
                                executable_path: path,
                                working_directory:
                                  profileEdit.working_directory ||
                                  parentDirectory(path),
                                process_hints:
                                  profileEdit.process_hints.length > 0
                                    ? profileEdit.process_hints
                                    : [fileName(path)],
                              });
                            })
                          }
                          help="Selecione o arquivo que inicia o jogo."
                        />
                        <PathField
                          label="PASTA DE TRABALHO · OPCIONAL"
                          value={profileEdit.working_directory ?? ''}
                          onChange={(value) =>
                            setProfileEdit({
                              ...profileEdit,
                              working_directory: value,
                            })
                          }
                          browseLabel="ESCOLHER PASTA…"
                          onBrowse={() =>
                            void pickLocalDirectory().then((path) => {
                              if (!path) return;
                              setProfileEdit({
                                ...profileEdit,
                                working_directory: path,
                              });
                            })
                          }
                          help="Deixe vazia para usar automaticamente a pasta do executável."
                        />
                        <StringList
                          label="OPÇÕES DE INICIALIZAÇÃO · OPCIONAL"
                          values={profileEdit.arguments}
                          onChange={(argumentsList) =>
                            setProfileEdit({
                              ...profileEdit,
                              arguments: argumentsList,
                            })
                          }
                        />
                      </>
                    )}
                    <StringList
                      label="PROCESSOS ESPERADOS DO JOGO (.EXE)"
                      values={profileEdit.process_hints}
                      onChange={(processHints) =>
                        setProfileEdit({
                          ...profileEdit,
                          process_hints: processHints,
                        })
                      }
                    />
                    <p className="security-note">
                      A capa ativa é configurada no Label Studio em “Aplicar
                      capa ao jogo”. Caminhos executáveis permanecem locais e
                      nunca são aceitos da mídia.
                    </p>
                    <button type="submit" disabled={busy}>
                      SALVAR PERFIL
                    </button>
                  </form>
                </section>
              </aside>
            )}

            {registerOpen && (
              <aside
                className="register-drawer"
                role="dialog"
                aria-modal="true"
                aria-labelledby="register-title"
                onKeyDown={(event) => {
                  if (event.key === 'Escape') setRegisterOpen(false);
                }}
              >
                <button
                  type="button"
                  className="register-backdrop"
                  aria-label="Fechar cadastro"
                  onClick={() => setRegisterOpen(false)}
                />
                <section className="manual-register mechanical-panel">
                  <ResponsiveChamferOutline
                    cut={10}
                    layers={[
                      { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
                      { inset: 4.5, color: '#343c44', strokeWidth: 1 },
                    ]}
                  />
                  <header>
                    <h2 id="register-title">CADASTRAR EXECUTÁVEL LOCAL</h2>
                    <button
                      type="button"
                      aria-label="Fechar cadastro"
                      onClick={() => setRegisterOpen(false)}
                      autoFocus
                    >
                      ×
                    </button>
                  </header>
                  <form onSubmit={(event) => void submit(event)}>
                    <PathField
                      label="DISPLAY NAME"
                      value={form.display_name}
                      onChange={(value) =>
                        setForm({ ...form, display_name: value })
                      }
                      required
                    />
                    <PathField
                      label="EXECUTÁVEL DO JOGO"
                      value={form.executable_path}
                      onChange={(value) =>
                        setForm({ ...form, executable_path: value })
                      }
                      required
                      browseLabel="PROCURAR .EXE…"
                      onBrowse={() =>
                        void pickLocalPath({
                          name: 'Executável Windows',
                          extensions: ['exe'],
                        }).then((path) => {
                          if (!path) return;
                          setForm({
                            ...form,
                            executable_path: path,
                            working_directory:
                              form.working_directory || parentDirectory(path),
                            process_hints:
                              form.process_hints.length > 0
                                ? form.process_hints
                                : [fileName(path)],
                          });
                        })
                      }
                      help="Selecione o arquivo .exe que inicia o jogo."
                    />
                    <PathField
                      label="PASTA DE TRABALHO · OPCIONAL"
                      value={form.working_directory ?? ''}
                      onChange={(value) =>
                        setForm({ ...form, working_directory: value })
                      }
                      browseLabel="ESCOLHER PASTA…"
                      onBrowse={() =>
                        void pickLocalDirectory().then((path) => {
                          if (path)
                            setForm({ ...form, working_directory: path });
                        })
                      }
                      help="Pasta usada durante a inicialização. Deixe vazia para usar a pasta do executável."
                    />
                    <PathField
                      label="CAPA DO JOGO · OPCIONAL"
                      value={form.cover_path ?? ''}
                      onChange={(value) =>
                        setForm({ ...form, cover_path: value })
                      }
                      browseLabel="ESCOLHER IMAGEM…"
                      onBrowse={() =>
                        void pickLocalPath({
                          name: 'Imagem de capa',
                          extensions: ['png', 'jpg', 'jpeg', 'webp'],
                        }).then((path) => {
                          if (path) setForm({ ...form, cover_path: path });
                        })
                      }
                      help="PNG, JPEG ou WEBP local de até 16 MB."
                    />
                    <StringList
                      label="OPÇÕES DE INICIALIZAÇÃO · OPCIONAL"
                      values={form.arguments}
                      onChange={(argumentsList) =>
                        setForm({ ...form, arguments: argumentsList })
                      }
                    />
                    <StringList
                      label="PROCESSOS ESPERADOS DO JOGO (.EXE)"
                      values={form.process_hints}
                      onChange={(processHints) =>
                        setForm({ ...form, process_hints: processHints })
                      }
                    />
                    <fieldset className="shortcut-review">
                      <legend>IMPORTAR .LNK PARA REVISÃO</legend>
                      <input
                        aria-label="Caminho do atalho .lnk"
                        value={shortcutPath}
                        onChange={(event) =>
                          setShortcutPath(event.target.value)
                        }
                        placeholder="C:\\Jogos\\Atalho.lnk"
                      />
                      <button
                        type="button"
                        onClick={() => void inspectShortcut()}
                        disabled={busy}
                      >
                        RESOLVER SEM EXECUTAR
                      </button>
                      {shortcutArgs && (
                        <p>
                          Argumentos encontrados (não salvos automaticamente):{' '}
                          <code>{shortcutArgs}</code>
                        </p>
                      )}
                    </fieldset>
                    <button type="submit" disabled={busy}>
                      VALIDAR E SALVAR PERFIL
                    </button>
                  </form>
                </section>
              </aside>
            )}
          </div>
        )}

        <footer className="security-note" role="status">
          {notice} · sem shell · dados locais
        </footer>
      </main>
    </WindowChassis>
  );
}

function stateKey(game: LibraryEntry) {
  if (game.offline) return 'offline';
  return game.installed ? 'installed' : 'missing';
}

function stateLabel(game: LibraryEntry) {
  const key = stateKey(game);
  return key === 'installed'
    ? 'INSTALLED'
    : key === 'offline'
      ? 'LIBRARY OFFLINE'
      : 'NOT INSTALLED';
}

function splitStructuredLines(value: string) {
  return value
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function creatorProfileIsValid(profile: CreatorProfileForm) {
  return (
    profile.provider !== 'steam' || /^[1-9]\d*$/.test(profile.appId.trim())
  );
}

function mediaIdIsValid(value: string) {
  return (
    value.length >= 1 &&
    value.length <= 64 &&
    /^[A-Za-z0-9._-]+$/.test(value) &&
    !value.includes('..')
  );
}

function ipcErrorNotice(fallback: string, error: unknown) {
  if (typeof error === 'string' && error.trim()) {
    return `${fallback} · ${error}`;
  }
  if (error && typeof error === 'object') {
    const candidate = error as { code?: unknown; message?: unknown };
    const message =
      typeof candidate.message === 'string' ? candidate.message : '';
    const code = typeof candidate.code === 'string' ? candidate.code : '';
    if (message || code) {
      return `${fallback} · ${message || code}${message && code ? ` (${code})` : ''}`;
    }
  }
  return fallback;
}

async function pickLocalPath(filter: { name: string; extensions: string[] }) {
  const selected = await chooseLocalPath({
    multiple: false,
    directory: false,
    filters: [filter],
  });
  return typeof selected === 'string' ? selected : null;
}

async function pickLocalDirectory() {
  const selected = await chooseLocalPath({
    multiple: false,
    directory: true,
  });
  return typeof selected === 'string' ? selected : null;
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? '';
}

function parentDirectory(path: string) {
  const separator = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
  return separator > 0 ? path.slice(0, separator) : '';
}

function SteamLaunchOptionSelect({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <label>
      <span>FORMA DE INICIALIZAÇÃO NA STEAM</span>
      <select value={value} onChange={(event) => onChange(event.target.value)}>
        <option value="">Automática padrão · steam://run</option>
        {Array.from({ length: 16 }, (_, option) => (
          <option key={option} value={option}>
            Opção específica {option} · steam://launch/…/option{option}
          </option>
        ))}
      </select>
      <small>
        Use uma opção específica apenas quando “Jogar” na Steam oferecer versões
        diferentes.
      </small>
    </label>
  );
}

function PathField({
  label,
  value,
  onChange,
  required = false,
  disabled = false,
  browseLabel,
  onBrowse,
  help,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  required?: boolean;
  disabled?: boolean;
  browseLabel?: string;
  onBrowse?: () => void;
  help?: string;
}) {
  return (
    <label className={onBrowse ? 'path-field' : undefined}>
      <span>{label}</span>
      <div>
        <input
          value={value}
          onChange={(event) => onChange(event.target.value)}
          required={required}
          disabled={disabled}
        />
        {onBrowse && (
          <button type="button" onClick={onBrowse}>
            {browseLabel ?? 'PROCURAR…'}
          </button>
        )}
      </div>
      {help && <small>{help}</small>}
    </label>
  );
}

function StringList({
  label,
  values,
  onChange,
}: {
  label: string;
  values: string[];
  onChange: (values: string[]) => void;
}) {
  return (
    <fieldset className="string-list">
      <legend>{label}</legend>
      {values.map((value, index) => (
        <div key={`${label}-${index}`}>
          <input
            aria-label={`${label} ${index + 1}`}
            value={value}
            onChange={(event) =>
              onChange(
                values.map((item, itemIndex) =>
                  itemIndex === index ? event.target.value : item,
                ),
              )
            }
          />
          <button
            type="button"
            aria-label={`Remover ${label.toLocaleLowerCase()} ${index + 1}`}
            onClick={() =>
              onChange(values.filter((_, itemIndex) => itemIndex !== index))
            }
          >
            ×
          </button>
        </div>
      ))}
      <button type="button" onClick={() => onChange([...values, ''])}>
        + ADICIONAR
      </button>
    </fieldset>
  );
}
