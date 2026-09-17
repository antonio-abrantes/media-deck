import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';
import { save as chooseExportPath } from '@tauri-apps/plugin-dialog';
import { ResponsiveChamferOutline } from '@/shared/components/ChamferOutline/ResponsiveChamferOutline';
import { libraryApi, type LibraryEntry } from '@/features/main/library-api';
import { PRESETS, sceneValidation } from './engine';
import { LabelCanvas, type LabelStageHandle } from './LabelCanvas';
import { labelStudioApi } from './label-studio-api';
import { labelStudioStrings as text } from './strings';
import { selectedElement, useLabelStudioStore } from './store';
import type {
  ArtworkAsset,
  LabelPresetKind,
  LabelProject,
  SceneElement,
} from './types';
import './LabelStudio.css';

export function LabelStudio() {
  const stageRef = useRef<LabelStageHandle>(null);
  const [notice, setNotice] = useState<string>(text.projectReady);
  const [customWidth, setCustomWidth] = useState(100);
  const [customHeight, setCustomHeight] = useState(70);
  const [games, setGames] = useState<LibraryEntry[]>([]);
  const [artworkId, setArtworkId] = useState('');
  const [exportFormat, setExportFormat] = useState<'png' | 'pdf'>('png');
  const [busy, setBusy] = useState(false);
  const [pendingDeletion, setPendingDeletion] = useState<
    'project' | 'element' | null
  >(null);
  const state = useLabelStudioStore();
  const isLauncherCover = state.scene.preset.kind === 'launcher_cover';
  const hasTemporaryAssets = state.assets.some(
    (asset) => asset.game_id === '' && asset.bytes,
  );
  const { cacheAsset, setAssets, setProjects, undo, redo, removeSelected } =
    state;
  const selected = selectedElement();
  const errors = sceneValidation(state.scene);
  const activeArtworkId = state.assets.some((asset) => asset.id === artworkId)
    ? artworkId
    : (state.assets[0]?.id ?? '');
  const missingArtworkIds = state.scene.elements
    .filter(
      (element) =>
        (element.type === 'image' || element.type === 'logo') &&
        !state.assets.some(
          (asset) => asset.id === element.artwork_id && asset.bytes,
        ),
    )
    .map((element) =>
      element.type === 'image' || element.type === 'logo'
        ? element.artwork_id
        : '',
    )
    .join('|');

  useEffect(() => {
    void labelStudioApi
      .list()
      .then(setProjects)
      .catch(() => setNotice(text.browserMode));
    void libraryApi
      .list()
      .then(setGames)
      .catch(() => undefined);
  }, [setProjects]);

  useEffect(() => {
    if (!state.gameId) {
      setAssets([]);
      return;
    }
    void labelStudioApi
      .artwork(state.gameId)
      .then((assets) => {
        setAssets(assets);
        setArtworkId(assets[0]?.id ?? '');
      })
      .catch(() => {
        setAssets([]);
      });
  }, [setAssets, state.gameId]);

  useEffect(() => {
    for (const id of missingArtworkIds.split('|').filter(Boolean)) {
      void labelStudioApi
        .artworkById(id)
        .then(cacheAsset)
        .catch(() => undefined);
    }
  }, [cacheAsset, missingArtworkIds]);

  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.matches('input, textarea, select')) return;
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z') {
        event.preventDefault();
        if (event.shiftKey) redo();
        else undo();
      } else if (
        (event.ctrlKey || event.metaKey) &&
        event.key.toLowerCase() === 'y'
      ) {
        event.preventDefault();
        redo();
      } else if (event.key === 'Delete') {
        if (selected && !selected.locked && selected.type !== 'background') {
          event.preventDefault();
          setPendingDeletion('element');
        }
      }
    };
    window.addEventListener('keydown', keydown);
    return () => window.removeEventListener('keydown', keydown);
  }, [redo, selected, undo]);

  const importClipboardBlob = useCallback(
    async (blob: Blob) => {
      if (
        !['image/png', 'image/jpeg', 'image/webp'].includes(blob.type) ||
        blob.size === 0 ||
        blob.size > 16 * 1024 * 1024
      ) {
        setNotice('Cole uma imagem PNG, JPEG ou WEBP de até 16 MB.');
        return;
      }
      try {
        setBusy(true);
        if (!state.gameId) {
          const asset = await temporaryAssetFromBlob(blob);
          state.cacheAsset(asset);
          setArtworkId(asset.id);
          state.addElement('image', asset.id);
          setNotice(
            'Imagem avulsa adicionada temporariamente. Exporte o PNG; para salvar o projeto, associe um jogo e cole novamente.',
          );
          return;
        }
        const asset = await labelStudioApi.importClipboardArtwork(
          state.gameId,
          await blobToDataUrl(blob),
          'editor_source',
        );
        state.cacheAsset(asset);
        setArtworkId(asset.id);
        state.addElement('image', asset.id);
        setNotice('Imagem colada, persistida no cache e adicionada ao canvas.');
      } catch {
        setNotice('A imagem colada foi recusada pelo cache seguro.');
      } finally {
        setBusy(false);
      }
    },
    [state],
  );

  useEffect(() => {
    const paste = (event: ClipboardEvent) => {
      const image = Array.from(event.clipboardData?.items ?? [])
        .find((item) =>
          ['image/png', 'image/jpeg', 'image/webp'].includes(item.type),
        )
        ?.getAsFile();
      if (!image) return;
      event.preventDefault();
      void importClipboardBlob(image);
    };
    window.addEventListener('paste', paste);
    return () => window.removeEventListener('paste', paste);
  }, [importClipboardBlob]);

  const pasteFromClipboard = async () => {
    try {
      const entries = await navigator.clipboard.read();
      const entry = entries.find((item) =>
        item.types.some((type) =>
          ['image/png', 'image/jpeg', 'image/webp'].includes(type),
        ),
      );
      const mime = entry?.types.find((type) =>
        ['image/png', 'image/jpeg', 'image/webp'].includes(type),
      );
      if (!entry || !mime) {
        setNotice('A área de transferência não contém PNG, JPEG ou WEBP.');
        return;
      }
      await importClipboardBlob(await entry.getType(mime));
    } catch {
      setNotice('Não foi possível ler a imagem da área de transferência.');
    }
  };

  const save = async () => {
    if (hasTemporaryAssets) {
      setNotice(
        'Imagem avulsa não é persistida. Exporte agora ou associe um jogo e cole a imagem novamente antes de salvar.',
      );
      return;
    }
    if (errors.length) {
      setNotice(errors.join(' · '));
      return;
    }
    try {
      setBusy(true);
      const project = await labelStudioApi.save({
        id: state.projectId,
        game_id: state.gameId,
        name: state.projectName,
        scene: state.scene,
        thumbnail_data_url: stageRef.current?.renderThumbnail() ?? null,
        is_template: false,
        expected_revision: state.projectId ? state.scene.revision : null,
      });
      state.markSaved(project);
      setNotice(text.saved);
    } catch {
      setNotice(text.saveFailed);
    } finally {
      setBusy(false);
    }
  };

  const exportProject = async () => {
    if (errors.length) {
      setNotice(errors.join(' · '));
      return;
    }
    try {
      const destination = await chooseExportPath({
        defaultPath: `${safeFileName(state.projectName)}.${exportFormat}`,
        filters: [
          {
            name: exportFormat === 'png' ? 'Imagem PNG' : 'Documento PDF',
            extensions: [exportFormat],
          },
        ],
      });
      if (!destination) {
        setNotice('Exportação cancelada; nenhum arquivo foi criado.');
        return;
      }
      setBusy(true);
      setNotice('Renderizando exportação…');
      const dataUrl = stageRef.current?.renderExport(state.scene.physical.dpi);
      if (!dataUrl) throw new Error('Stage indisponível');
      const receipt = await labelStudioApi.export(
        state.projectName,
        state.scene,
        exportFormat,
        dataUrl,
        destination,
      );
      setNotice(
        `CAPA EXPORTADA: ${receipt.output_path} · ${receipt.width_px}×${receipt.height_px}px @ ${receipt.dpi} DPI.`,
      );
    } catch {
      setNotice('Exportação recusada: confira dimensões, imagens e limites.');
    } finally {
      setBusy(false);
    }
  };

  const exportCalibration = async () => {
    try {
      setBusy(true);
      const receipt = await labelStudioApi.calibration();
      setNotice(
        `${receipt.file_name} gravado. Imprima em tamanho real/100%; não use “ajustar à página”.`,
      );
    } catch {
      setNotice('Não foi possível gerar a folha de calibração.');
    } finally {
      setBusy(false);
    }
  };

  const applyLauncherCover = async () => {
    if (!state.gameId) {
      setNotice('Escolha o jogo que receberá esta capa.');
      return;
    }
    if (state.scene.preset.kind !== 'launcher_cover') {
      setNotice('Escolha primeiro o formato “Capa do launcher”.');
      return;
    }
    try {
      setBusy(true);
      const dataUrl = stageRef.current?.renderExport(300);
      if (!dataUrl) throw new Error('Stage indisponível');
      const asset = await labelStudioApi.importClipboardArtwork(
        state.gameId,
        dataUrl,
        'launcher_cover',
      );
      state.cacheAsset(asset);
      setArtworkId(asset.id);
      setNotice(
        `CAPA DO LAUNCHER APLICADA A ${games.find((game) => game.id === state.gameId)?.display_name ?? 'JOGO SELECIONADO'}.`,
      );
    } catch {
      setNotice('Não foi possível aplicar a capa ao launcher.');
    } finally {
      setBusy(false);
    }
  };

  const removeProject = async () => {
    if (!state.projectId) return;
    try {
      await labelStudioApi.remove(state.projectId, state.scene.revision);
      state.newProject('floppy_label');
      state.setProjects(await labelStudioApi.list());
      setNotice('Projeto excluído.');
    } catch {
      setNotice('Exclusão recusada por revisão concorrente.');
    }
  };

  const choosePreset = (kind: LabelPresetKind) => {
    state.newProject(
      kind,
      kind === 'custom'
        ? { width_mm: customWidth, height_mm: customHeight }
        : undefined,
    );
    if (kind === 'launcher_cover') setExportFormat('png');
    setNotice(`Preset ${PRESETS[kind].label} aplicado.`);
  };

  return (
    <section
      id="label-studio-panel"
      className="label-studio"
      aria-label="Label Studio"
      role="tabpanel"
    >
      <MechanicalPanel className="label-toolbar" cut={9}>
        <label className="label-project-name">
          <span>NOME DO PROJETO</span>
          <input
            value={state.projectName}
            maxLength={120}
            onChange={(event) => state.setProjectName(event.target.value)}
          />
        </label>
        <button type="button" disabled={busy} onClick={() => void save()}>
          SALVAR PROJETO
        </button>
        <button
          type="button"
          disabled={!state.past.length}
          onClick={state.undo}
        >
          {text.undo}
        </button>
        <button
          type="button"
          disabled={!state.future.length}
          onClick={state.redo}
        >
          {text.redo}
        </button>
        <button type="button" onClick={state.toggleGuides}>
          {state.guidesVisible ? 'OCULTAR GUIAS' : 'MOSTRAR GUIAS'}
        </button>
        <select
          aria-label="Formato de exportação"
          value={exportFormat}
          onChange={(event) =>
            setExportFormat(event.target.value as 'png' | 'pdf')
          }
        >
          <option value="png">PNG</option>
          {!isLauncherCover && <option value="pdf">PDF</option>}
        </select>
        <button
          type="button"
          disabled={busy || errors.length > 0}
          onClick={() => void exportProject()}
        >
          {busy ? 'PROCESSANDO…' : 'EXPORTAR COMO…'}
        </button>
        {!isLauncherCover && (
          <button
            type="button"
            disabled={busy}
            onClick={() => void exportCalibration()}
            title="Gera PDF com régua física de 100 mm"
          >
            CALIBRAR IMPRESSÃO
          </button>
        )}
        <span className={state.dirty ? 'label-dirty' : 'label-saved'}>
          {state.dirty ? 'ALTERAÇÕES LOCAIS' : 'SALVO'}
        </span>
      </MechanicalPanel>

      <div className="label-workspace">
        <MechanicalPanel className="label-sidebar label-sidebar--left">
          <section className="label-target-step">
            <h2>1 · JOGO E DESTINO</h2>
            <label>
              <span>JOGO ASSOCIADO</span>
              <select
                aria-label="Jogo associado ao projeto"
                value={state.gameId ?? ''}
                onChange={(event) =>
                  state.setGameId(event.target.value || null)
                }
              >
                <option value="">Sem jogo · exportação avulsa</option>
                {games.map((game) => (
                  <option key={game.id} value={game.id}>
                    {game.display_name}
                  </option>
                ))}
              </select>
            </label>
            <p className="label-step-help">
              Define qual jogo receberá a capa. “Sem jogo” apenas exporta uma
              composição já criada.
            </p>
            {state.scene.preset.kind === 'launcher_cover' && (
              <button
                type="button"
                disabled={busy || !state.gameId}
                onClick={() => void applyLauncherCover()}
              >
                APLICAR CAPA AO JOGO
              </button>
            )}
          </section>
          <section className="label-image-step">
            <h2>3 · IMAGEM E ELEMENTOS</h2>
            <p className="label-step-help">
              Escolha o jogo e cole a imagem. Ela já entra no tamanho da capa.
            </p>
            <div className="label-tool-grid">
              <button type="button" onClick={() => state.addElement('text')}>
                TEXTO
              </button>
              <button type="button" onClick={() => state.addElement('rect')}>
                RETÂNGULO
              </button>
              <button type="button" onClick={() => state.addElement('line')}>
                LINHA
              </button>
              <select
                aria-label="Imagem existente do jogo"
                value={activeArtworkId}
                onChange={(event) => setArtworkId(event.target.value)}
              >
                <option value="">Escolha uma imagem existente</option>
                {state.assets.map((asset) => (
                  <option key={asset.id} value={asset.id}>
                    {artworkKindLabel(asset.kind)} · {asset.width}×
                    {asset.height}
                  </option>
                ))}
              </select>
              <button
                type="button"
                disabled={!activeArtworkId}
                onClick={() => state.addElement('image', activeArtworkId)}
              >
                IMAGEM
              </button>
              <button
                type="button"
                disabled={!activeArtworkId}
                onClick={() => state.addElement('logo', activeArtworkId)}
              >
                LOGO
              </button>
              <button
                type="button"
                className="label-paste-image"
                onClick={() => void pasteFromClipboard()}
              >
                COLAR IMAGEM
              </button>
            </div>
          </section>
          <section className="label-format-step">
            <h2>2 · FINALIDADE E FORMATO</h2>
            <div className="label-template-list">
              <button
                type="button"
                className={
                  state.scene.preset.kind === 'launcher_cover'
                    ? 'is-selected'
                    : ''
                }
                onClick={() => choosePreset('launcher_cover')}
              >
                {PRESETS.launcher_cover.label}
              </button>
              <p className="label-purpose-divider">ETIQUETAS PARA IMPRESSÃO</p>
              {(
                ['floppy_label', 'cd_jewel_front', 'cd_disc_label'] as const
              ).map((kind) => (
                <button
                  key={kind}
                  type="button"
                  onClick={() => choosePreset(kind)}
                >
                  {PRESETS[kind].label}
                </button>
              ))}
              <div className="label-custom-size">
                <NumberField
                  label="LARGURA MM"
                  value={customWidth}
                  min={0.1}
                  onChange={setCustomWidth}
                />
                <NumberField
                  label="ALTURA MM"
                  value={customHeight}
                  min={0.1}
                  onChange={setCustomHeight}
                />
                <button type="button" onClick={() => choosePreset('custom')}>
                  CRIAR CUSTOM
                </button>
              </div>
            </div>
          </section>
          <section className="label-layers">
            <h2>{text.layers}</h2>
            {[...state.scene.elements]
              .sort((a, b) => b.z_order - a.z_order)
              .map((element) => (
                <article
                  key={element.id}
                  className={
                    state.selectedId === element.id ? 'is-selected' : ''
                  }
                >
                  <button
                    type="button"
                    onClick={() => state.select(element.id)}
                  >
                    {element.type.toUpperCase()} · {element.id.slice(-5)}
                  </button>
                  <button
                    type="button"
                    aria-label={`${element.visible ? 'Ocultar' : 'Mostrar'} ${element.id}`}
                    onClick={() => state.toggleVisibility(element.id)}
                  >
                    {element.visible ? '◉' : '○'}
                  </button>
                  <button
                    type="button"
                    aria-label={`${element.locked ? 'Desbloquear' : 'Bloquear'} ${element.id}`}
                    onClick={() => state.toggleLock(element.id)}
                  >
                    {element.locked ? 'L' : 'U'}
                  </button>
                  <button
                    type="button"
                    aria-label={`Subir ${element.id}`}
                    onClick={() => state.reorder(element.id, 1)}
                  >
                    ↑
                  </button>
                  <button
                    type="button"
                    aria-label={`Descer ${element.id}`}
                    onClick={() => state.reorder(element.id, -1)}
                  >
                    ↓
                  </button>
                </article>
              ))}
          </section>
        </MechanicalPanel>

        <MechanicalPanel className="label-canvas-panel">
          <LabelCanvas ref={stageRef} />
        </MechanicalPanel>

        <MechanicalPanel className="label-sidebar label-sidebar--right">
          <h2>4 · AJUSTE E EXPORTAÇÃO</h2>
          {selected ? (
            <ElementProperties
              element={selected}
              onRequestRemove={() => setPendingDeletion('element')}
            />
          ) : (
            <CanvasProperties />
          )}
          <section className="label-projects">
            <h2>PROJETOS</h2>
            <div className="label-project-list" aria-label="Projetos salvos">
              {state.projects.map((project) => (
                <ProjectButton
                  key={`${project.id}-${project.thumbnail?.updated_at ?? ''}`}
                  project={project}
                  active={state.projectId === project.id}
                  onLoad={() => {
                    void labelStudioApi
                      .load(project.id)
                      .then(state.loadProject)
                      .catch(() =>
                        setNotice('Projeto não pôde ser carregado.'),
                      );
                  }}
                />
              ))}
            </div>
            <button
              type="button"
              disabled={!state.projectId}
              onClick={() => setPendingDeletion('project')}
            >
              EXCLUIR PROJETO
            </button>
          </section>
        </MechanicalPanel>
      </div>

      {pendingDeletion && (
        <aside
          className="label-confirm"
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="label-confirm-title"
          onKeyDown={(event) => {
            if (event.key === 'Escape' && !busy) setPendingDeletion(null);
          }}
        >
          <MechanicalPanel className="label-confirm__panel" cut={9}>
            <h2 id="label-confirm-title">
              {pendingDeletion === 'project'
                ? 'EXCLUIR PROJETO?'
                : 'EXCLUIR ELEMENTO?'}
            </h2>
            <p>
              {pendingDeletion === 'project'
                ? 'O projeto salvo será removido permanentemente. Esta ação não pode ser desfeita.'
                : 'O elemento selecionado será removido da composição. Você poderá recuperá-lo com Desfazer.'}
            </p>
            <div>
              <button
                type="button"
                autoFocus
                disabled={busy}
                onClick={() => setPendingDeletion(null)}
              >
                CANCELAR
              </button>
              <button
                type="button"
                className="label-danger-action"
                disabled={busy}
                onClick={() => {
                  if (pendingDeletion === 'project') {
                    void removeProject().finally(() =>
                      setPendingDeletion(null),
                    );
                  } else {
                    removeSelected();
                    setPendingDeletion(null);
                    setNotice(
                      'Elemento excluído. Use Desfazer para restaurá-lo.',
                    );
                  }
                }}
              >
                CONFIRMAR EXCLUSÃO
              </button>
            </div>
          </MechanicalPanel>
        </aside>
      )}

      <MechanicalPanel className="label-status" cut={7}>
        <span>{notice}</span>
        <span>
          {isLauncherCover
            ? '1518 × 2076 px'
            : `${state.scene.physical.width_mm} × ${state.scene.physical.height_mm} mm`}
        </span>
        <span>{state.scene.physical.dpi} DPI</span>
        <label>
          ZOOM
          <input
            aria-label="Zoom do canvas"
            type="range"
            min={25}
            max={400}
            value={state.zoom * 100}
            onChange={(event) =>
              state.setZoom(Number(event.target.value) / 100)
            }
          />
          {Math.round(state.zoom * 100)}%
        </label>
        <span className={errors.length ? 'label-invalid' : 'label-valid'}>
          {errors.length ? `${errors.length} ERRO(S)` : 'SCENE V1 VÁLIDA'}
        </span>
      </MechanicalPanel>
    </section>
  );
}

function ProjectButton({
  project,
  active,
  onLoad,
}: {
  project: LabelProject;
  active: boolean;
  onLoad: () => void;
}) {
  const [thumbnailUrl, setThumbnailUrl] = useState<string>();
  useEffect(() => {
    if (!project.thumbnail) return;
    let activeRequest = true;
    let objectUrl: string | undefined;
    void labelStudioApi
      .thumbnail(project.id)
      .then((thumbnail) => {
        if (!activeRequest) return;
        objectUrl = URL.createObjectURL(
          new Blob([new Uint8Array(thumbnail.bytes)], {
            type: thumbnail.mime_type,
          }),
        );
        setThumbnailUrl(objectUrl);
      })
      .catch(() => setThumbnailUrl(undefined));
    return () => {
      activeRequest = false;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [project.id, project.thumbnail]);
  return (
    <button
      type="button"
      className={active ? 'is-selected' : ''}
      onClick={onLoad}
    >
      {thumbnailUrl ? (
        <img src={thumbnailUrl} alt="" width={48} height={48} />
      ) : (
        <span className="label-project-placeholder" aria-hidden="true">
          —
        </span>
      )}
      <span>{project.name}</span>
      <small>R{project.scene.revision}</small>
    </button>
  );
}

function MechanicalPanel({
  children,
  className,
  cut = 10,
}: {
  children: ReactNode;
  className: string;
  cut?: number;
}) {
  return (
    <section className={`${className} mechanical-panel`}>
      <ResponsiveChamferOutline
        cut={cut}
        layers={[
          { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
          { inset: 4.5, color: '#343c44', strokeWidth: 1 },
        ]}
      />
      {children}
    </section>
  );
}

function safeFileName(value: string) {
  const normalized = value
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[<>:"/\\|?*]/g, '-')
    .split('')
    .map((character) => (character.charCodeAt(0) < 32 ? '-' : character))
    .join('')
    .replace(/[.\s]+$/g, '')
    .trim();
  return normalized || 'capa-mediadeck';
}

function artworkKindLabel(kind: ArtworkAsset['kind']) {
  const labels: Record<ArtworkAsset['kind'], string> = {
    editor_source: 'Imagem importada para edição',
    launcher_cover: 'Capa atual do launcher',
    hero: 'Imagem panorâmica',
    logo: 'Logotipo',
    jewel_front: 'Capa frontal de CD',
    disc_label: 'Arte de CD/DVD',
    icon: 'Ícone',
  };
  return labels[kind];
}

async function temporaryAssetFromBlob(blob: Blob): Promise<ArtworkAsset> {
  const bitmap = await createImageBitmap(blob);
  try {
    return {
      id: crypto.randomUUID(),
      game_id: '',
      kind: 'editor_source',
      width: bitmap.width,
      height: bitmap.height,
      mime_type: blob.type,
      bytes: Array.from(new Uint8Array(await blob.arrayBuffer())),
    };
  } finally {
    bitmap.close();
  }
}

function blobToDataUrl(blob: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () =>
      reject(
        new Error('Clipboard image encoding failed', { cause: reader.error }),
      );
    reader.onload = () =>
      typeof reader.result === 'string'
        ? resolve(reader.result)
        : reject(new Error('Clipboard image encoding failed'));
    reader.readAsDataURL(blob);
  });
}

function ElementProperties({
  element,
  onRequestRemove,
}: {
  element: SceneElement;
  onRequestRemove: () => void;
}) {
  const update = useLabelStudioStore((state) => state.updateElement);
  const physical = useLabelStudioStore((state) => state.scene.physical);
  const assets = useLabelStudioStore((state) => state.assets);
  const isArtwork = element.type === 'image' || element.type === 'logo';
  const imageAsset =
    element.type === 'image'
      ? assets.find((asset) => asset.id === element.artwork_id)
      : undefined;
  const cropZoom =
    element.type === 'image' && imageAsset
      ? imageCropZoom(element, imageAsset.width, imageAsset.height)
      : 1;
  useEffect(() => {
    if (isArtwork && element.fit === 'stretch') {
      update(element.id, { fit: 'contain' });
    }
  }, [element, isArtwork, update]);
  return (
    <div className="label-properties">
      <strong>{element.type.toUpperCase()}</strong>
      <NumberField
        label="X MM"
        value={element.x_mm}
        onChange={(x_mm) => update(element.id, { x_mm })}
      />
      <NumberField
        label="Y MM"
        value={element.y_mm}
        onChange={(y_mm) => update(element.id, { y_mm })}
      />
      <NumberField
        label="LARGURA MM"
        min={0.01}
        value={element.width_mm}
        onChange={(width_mm) => update(element.id, { width_mm })}
      />
      <NumberField
        label="ALTURA MM"
        min={0.01}
        value={element.height_mm}
        onChange={(height_mm) => update(element.id, { height_mm })}
      />
      <NumberField
        label="ROTAÇÃO °"
        min={-360}
        max={360}
        value={element.rotation_deg}
        onChange={(rotation_deg) => update(element.id, { rotation_deg })}
      />
      <NumberField
        label="OPACIDADE"
        min={0}
        max={1}
        step={0.05}
        value={element.opacity}
        onChange={(opacity) => update(element.id, { opacity })}
      />
      {element.type === 'text' && (
        <>
          <label>
            <span>TEXTO</span>
            <textarea
              value={element.text}
              maxLength={16384}
              onChange={(event) =>
                update(element.id, { text: event.target.value })
              }
            />
          </label>
          <label>
            <span>COR</span>
            <input
              type="color"
              value={element.color.slice(0, 7)}
              onChange={(event) =>
                update(element.id, { color: `${event.target.value}FF` })
              }
            />
          </label>
          <NumberField
            label="FONTE MM"
            min={0.5}
            value={element.font_size_mm}
            onChange={(font_size_mm) => update(element.id, { font_size_mm })}
          />
          <label>
            <span>FAMÍLIA</span>
            <input
              value={element.font_family}
              maxLength={100}
              onChange={(event) =>
                update(element.id, { font_family: event.target.value })
              }
            />
          </label>
          <label>
            <span>ALINHAMENTO</span>
            <select
              value={element.align}
              onChange={(event) =>
                update(element.id, {
                  align: event.target.value as 'left' | 'center' | 'right',
                })
              }
            >
              <option value="left">Esquerda</option>
              <option value="center">Centro</option>
              <option value="right">Direita</option>
            </select>
          </label>
        </>
      )}
      {element.type === 'rect' && (
        <>
          <label>
            <span>PREENCHIMENTO</span>
            <input
              type="color"
              value={element.fill.slice(0, 7)}
              onChange={(event) =>
                update(element.id, { fill: `${event.target.value}FF` })
              }
            />
          </label>
          <NumberField
            label="TRAÇO MM"
            min={0}
            value={element.stroke_width_mm}
            onChange={(stroke_width_mm) =>
              update(element.id, { stroke_width_mm })
            }
          />
          <NumberField
            label="RAIO MM"
            min={0}
            value={element.corner_radius_mm}
            onChange={(corner_radius_mm) =>
              update(element.id, { corner_radius_mm })
            }
          />
        </>
      )}
      {element.type === 'line' && (
        <NumberField
          label="TRAÇO MM"
          min={0.01}
          value={element.stroke_width_mm}
          onChange={(stroke_width_mm) =>
            update(element.id, { stroke_width_mm })
          }
        />
      )}
      {element.type === 'background' && (
        <label>
          <span>COR</span>
          <input
            type="color"
            value={element.color.slice(0, 7)}
            onChange={(event) =>
              update(element.id, { color: `${event.target.value}FF` })
            }
          />
        </label>
      )}
      {(element.type === 'image' || element.type === 'logo') && (
        <>
          {element.type === 'image' && (
            <p className="label-crop-help">
              Arraste a foto dentro da moldura para escolher o recorte.
            </p>
          )}
          <div className="label-image-actions">
            <button
              type="button"
              onClick={() =>
                update(element.id, {
                  x_mm: 0,
                  y_mm: 0,
                  width_mm: physical.width_mm,
                  height_mm: physical.height_mm,
                  fit: 'cover',
                  rotation_deg: 0,
                })
              }
            >
              PREENCHER CAPA
            </button>
            <button
              type="button"
              onClick={() =>
                update(element.id, {
                  x_mm: 0,
                  y_mm: 0,
                  width_mm: physical.width_mm,
                  height_mm: physical.height_mm,
                  fit: 'contain',
                  rotation_deg: 0,
                })
              }
            >
              MOSTRAR INTEIRA
            </button>
            {element.type === 'image' && (
              <button
                type="button"
                onClick={() =>
                  update(element.id, {
                    crop: element.crop
                      ? {
                          ...element.crop,
                          x: (1 - element.crop.width) / 2,
                          y: (1 - element.crop.height) / 2,
                        }
                      : null,
                  })
                }
              >
                CENTRALIZAR RECORTE
              </button>
            )}
          </div>
          {element.type === 'image' && imageAsset && (
            <label>
              <span>ZOOM DA FOTO · {Math.round(cropZoom * 100)}%</span>
              <input
                type="range"
                min={100}
                max={400}
                value={Math.round(cropZoom * 100)}
                onChange={(event) =>
                  update(element.id, {
                    crop: cropAtZoom(
                      element,
                      imageAsset.width,
                      imageAsset.height,
                      Number(event.target.value) / 100,
                    ),
                  })
                }
              />
            </label>
          )}
          <label>
            <span>ENQUADRAMENTO (SEM DISTORÇÃO)</span>
            <select
              value={element.fit}
              onChange={(event) =>
                update(element.id, {
                  fit: event.target.value as 'contain' | 'cover',
                })
              }
            >
              <option value="cover">Preencher e recortar</option>
              <option value="contain">Mostrar imagem inteira</option>
            </select>
          </label>
        </>
      )}
      <button
        type="button"
        disabled={element.locked || element.type === 'background'}
        onClick={onRequestRemove}
      >
        EXCLUIR ELEMENTO
      </button>
    </div>
  );
}

function imageCropBase(
  element: Extract<SceneElement, { type: 'image' }>,
  imageWidth: number,
  imageHeight: number,
) {
  const sourceAspect = imageWidth / imageHeight;
  const frameAspect = element.width_mm / element.height_mm;
  return sourceAspect > frameAspect
    ? { width: frameAspect / sourceAspect, height: 1 }
    : { width: 1, height: sourceAspect / frameAspect };
}

function imageCropZoom(
  element: Extract<SceneElement, { type: 'image' }>,
  imageWidth: number,
  imageHeight: number,
) {
  if (!element.crop) return 1;
  const base = imageCropBase(element, imageWidth, imageHeight);
  return Math.max(
    base.width / element.crop.width,
    base.height / element.crop.height,
    1,
  );
}

function cropAtZoom(
  element: Extract<SceneElement, { type: 'image' }>,
  imageWidth: number,
  imageHeight: number,
  zoom: number,
) {
  const base = imageCropBase(element, imageWidth, imageHeight);
  const width = base.width / zoom;
  const height = base.height / zoom;
  const centerX = element.crop ? element.crop.x + element.crop.width / 2 : 0.5;
  const centerY = element.crop ? element.crop.y + element.crop.height / 2 : 0.5;
  return {
    x: Math.min(1 - width, Math.max(0, centerX - width / 2)),
    y: Math.min(1 - height, Math.max(0, centerY - height / 2)),
    width,
    height,
  };
}

function CanvasProperties() {
  const scene = useLabelStudioStore((state) => state.scene);
  const update = useLabelStudioStore((state) => state.updatePhysical);
  if (scene.preset.kind === 'launcher_cover') {
    return (
      <div className="label-properties">
        <strong>CAPA DIGITAL DO LAUNCHER</strong>
        <p className="label-crop-help">1518 × 2076 px · proporção 253:346</p>
        <p className="label-step-help">
          Exporte para obter um PNG avulso ou use “Aplicar capa ao jogo” para
          torná-la a capa ativa do jogo selecionado.
        </p>
      </div>
    );
  }
  const circle = scene.physical.shape.type === 'circle';
  return (
    <div className="label-properties">
      <strong>CANVAS · MM CANÔNICO</strong>
      <NumberField
        label="LARGURA MM"
        min={0.1}
        max={2000}
        value={scene.physical.width_mm}
        onChange={(width_mm) =>
          update(circle ? { width_mm, height_mm: width_mm } : { width_mm })
        }
      />
      <NumberField
        label="ALTURA MM"
        min={0.1}
        max={2000}
        value={scene.physical.height_mm}
        disabled={circle}
        onChange={(height_mm) => update({ height_mm })}
      />
      <NumberField
        label="SANGRIA MM"
        min={0}
        max={50}
        value={scene.physical.bleed_mm}
        onChange={(bleed_mm) => update({ bleed_mm })}
      />
      <NumberField
        label="MARGEM SEGURA MM"
        min={0}
        value={scene.physical.safe_margin_mm}
        onChange={(safe_margin_mm) => update({ safe_margin_mm })}
      />
      <NumberField
        label="DPI"
        min={72}
        max={1200}
        step={1}
        value={scene.physical.dpi}
        onChange={(dpi) => update({ dpi })}
      />
      {scene.physical.shape.type === 'circle' && (
        <NumberField
          label="FURO CENTRAL MM"
          min={0}
          max={scene.physical.width_mm - 0.1}
          value={scene.physical.shape.center_hole_mm}
          onChange={(center_hole_mm) =>
            update({ shape: { type: 'circle', center_hole_mm } })
          }
        />
      )}
    </div>
  );
}

function NumberField({
  label,
  value,
  onChange,
  min,
  max,
  step = 0.1,
  disabled = false,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
}) {
  return (
    <label>
      <span>{label}</span>
      <input
        type="number"
        value={Number(value.toFixed(3))}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onChange={(event) => {
          const parsed = Number(event.target.value);
          if (!Number.isFinite(parsed)) return;
          onChange(
            Math.min(max ?? Infinity, Math.max(min ?? -Infinity, parsed)),
          );
        }}
      />
    </label>
  );
}
