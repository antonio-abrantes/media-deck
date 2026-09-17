import { useEffect, useMemo, useState } from 'react';
import { ResponsiveChamferOutline } from '@/shared/components/ChamferOutline/ResponsiveChamferOutline';
import { CoverFrame } from '@/features/runtime/components/CoverFrame';
import { collectionApi, type CollectionEntry } from './collection-api';

type CollectionPanelProps = {
  refreshToken: number;
  onAddGames: () => void;
  onEditProfile: (entry: CollectionEntry) => void;
  onChangeCover: (entry: CollectionEntry) => void;
  onRegenerate: (entry: CollectionEntry) => void;
  onDeactivated: (gameId: string) => void;
};

export function CollectionPanel({
  refreshToken,
  onAddGames,
  onEditProfile,
  onChangeCover,
  onRegenerate,
  onDeactivated,
}: CollectionPanelProps) {
  const [entries, setEntries] = useState<CollectionEntry[]>([]);
  const [selectedId, setSelectedId] = useState('');
  const [query, setQuery] = useState('');
  const [notice, setNotice] = useState('Coleção carregada.');
  const [busy, setBusy] = useState(false);
  const [coverPreviewOpen, setCoverPreviewOpen] = useState(false);
  const [selectedCover, setSelectedCover] = useState<{
    gameId: string;
    source: string;
  } | null>(null);
  const [pendingRemoval, setPendingRemoval] = useState<CollectionEntry | null>(
    null,
  );

  useEffect(() => {
    let active = true;
    const load = async () => {
      for (let attempt = 0; attempt < 5 && active; attempt += 1) {
        try {
          const items = await collectionApi.list();
          if (!active) return;
          setEntries(items);
          setSelectedId((current) =>
            items.some((entry) => entry.game_id === current)
              ? current
              : (items[0]?.game_id ?? ''),
          );
          return;
        } catch {
          if (attempt < 4) {
            await new Promise((resolve) => window.setTimeout(resolve, 150));
          }
        }
      }
      if (active) setNotice('Não foi possível carregar Meus Jogos.');
    };
    void load();
    return () => {
      active = false;
    };
  }, [refreshToken]);

  const visible = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return normalized
      ? entries.filter((entry) =>
          entry.display_name.toLocaleLowerCase().includes(normalized),
        )
      : entries;
  }, [entries, query]);
  const selected =
    entries.find((entry) => entry.game_id === selectedId) ?? null;
  const selectedCoverSource =
    selectedCover && selectedCover.gameId === selected?.game_id
      ? selectedCover.source
      : '';

  useEffect(() => {
    let active = true;
    if (!selected?.active_cover) return;
    void collectionApi
      .cover(selected.game_id)
      .then((artwork) => {
        if (!active || !artwork) return;
        setSelectedCover({
          gameId: selected.game_id,
          source: artworkDataUrl(artwork),
        });
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [selected?.active_cover, selected?.game_id]);

  const deactivate = async (entry: CollectionEntry) => {
    setBusy(true);
    try {
      await collectionApi.deactivate(entry.game_id);
      const updated = entries.filter(
        (candidate) => candidate.game_id !== entry.game_id,
      );
      setEntries(updated);
      setSelectedId(updated[0]?.game_id ?? '');
      onDeactivated(entry.game_id);
      setPendingRemoval(null);
      setNotice(
        'Jogo removido da coleção. O cadastro continua em Adicionar Jogos.',
      );
    } catch {
      setNotice('Não foi possível remover o jogo da coleção.');
    } finally {
      setBusy(false);
    }
  };

  if (!entries.length) {
    return (
      <section
        id="collection-panel"
        className="collection-empty mechanical-panel"
        role="tabpanel"
        aria-label="Meus Jogos"
      >
        <ResponsiveChamferOutline
          cut={10}
          layers={[
            { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
            { inset: 4.5, color: '#343c44', strokeWidth: 1 },
          ]}
        />
        <p className="eyebrow">COLEÇÃO PESSOAL · 000</p>
        <h2>SUA COLEÇÃO AINDA ESTÁ VAZIA.</h2>
        <p>Encontre um jogo, configure o perfil e salve o primeiro GAME.INI.</p>
        <button type="button" onClick={onAddGames}>
          ADICIONAR JOGOS
        </button>
        <small role="status">{notice}</small>
      </section>
    );
  }

  return (
    <div id="collection-panel" className="collection-panel" role="tabpanel">
      <section className="collection-toolbar mechanical-panel">
        <ResponsiveChamferOutline
          cut={9}
          layers={[
            { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
            { inset: 4.5, color: '#343c44', strokeWidth: 1 },
          ]}
        />
        <label>
          <span>BUSCAR EM MEUS JOGOS</span>
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Nome do jogo"
          />
        </label>
        <strong>{entries.length.toString().padStart(3, '0')} ATIVADOS</strong>
        <button type="button" onClick={onAddGames}>
          + ADICIONAR JOGOS
        </button>
      </section>

      <section className="collection-workspace">
        <section
          className="collection-list mechanical-panel"
          aria-label="Meus Jogos"
        >
          <ResponsiveChamferOutline
            cut={10}
            layers={[
              { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
              { inset: 4.5, color: '#343c44', strokeWidth: 1 },
            ]}
          />
          <header>
            <h2>MEUS JOGOS</h2>
            <span>{visible.length.toString().padStart(3, '0')}</span>
          </header>
          <div className="collection-list__scroll">
            {visible.map((entry) => (
              <button
                key={entry.game_id}
                type="button"
                className={selectedId === entry.game_id ? 'is-selected' : ''}
                onClick={() => {
                  setSelectedId(entry.game_id);
                  setCoverPreviewOpen(false);
                }}
              >
                <CollectionCover entry={entry} />
                <span>
                  <strong>{entry.display_name}</strong>
                  <small>
                    {providerLabel(entry.provider)} · {stateLabel(entry)}
                  </small>
                </span>
              </button>
            ))}
            {!visible.length && <p>Nenhum jogo neste filtro.</p>}
          </div>
        </section>

        <section
          className="collection-details mechanical-panel"
          aria-label="Detalhes do jogo ativado"
        >
          <ResponsiveChamferOutline
            cut={10}
            layers={[
              { inset: 0.8, color: '#887866', strokeWidth: 1.2 },
              { inset: 4.5, color: '#343c44', strokeWidth: 1 },
            ]}
          />
          {selected && (
            <>
              <p className="eyebrow">
                {providerLabel(selected.provider)} · {stateLabel(selected)}
              </p>
              <h2>{selected.display_name}</h2>
              <dl>
                <div>
                  <dt>PERFIL DE EXECUÇÃO</dt>
                  <dd>{selected.profile_name}</dd>
                </div>
                <div>
                  <dt>ÚLTIMO MEDIA_ID</dt>
                  <dd>{selected.last_media_key}</dd>
                </div>
                <div>
                  <dt>PRIMEIRA ATIVAÇÃO</dt>
                  <dd>{formatDate(selected.first_activated_at)}</dd>
                </div>
                <div>
                  <dt>ÚLTIMA EXPORTAÇÃO</dt>
                  <dd>{formatDate(selected.last_exported_at)}</dd>
                </div>
                <div>
                  <dt>EXPORTAÇÕES VERIFICADAS</dt>
                  <dd>{selected.export_count}</dd>
                </div>
                <div>
                  <dt>CAPA ATIVA</dt>
                  <dd>
                    {selected.active_cover?.relative_path ??
                      'Nenhuma capa ativa'}
                  </dd>
                </div>
              </dl>
              <div className="collection-actions">
                <button type="button" onClick={() => onEditProfile(selected)}>
                  EDITAR PERFIL
                </button>
                <button type="button" onClick={() => onChangeCover(selected)}>
                  TROCAR CAPA
                </button>
                <button type="button" onClick={() => onRegenerate(selected)}>
                  GERAR GAME.INI NOVAMENTE
                </button>
                <button
                  type="button"
                  className="danger-action"
                  disabled={busy}
                  onClick={() => setPendingRemoval(selected)}
                >
                  REMOVER DOS MEUS JOGOS
                </button>
              </div>
              <section
                className="collection-cover-preview"
                aria-label="Pré-visualização da capa no launcher"
              >
                <div className="collection-cover-preview__thumbnail">
                  {selectedCoverSource ? (
                    <img src={selectedCoverSource} alt="" />
                  ) : (
                    <span aria-hidden="true">MD</span>
                  )}
                  <div>
                    <strong>{selected.last_media_key}</strong>
                    <small>FLOPPY DISK GAME SERIES</small>
                    <small>
                      PROVIDED BY{' '}
                      {selected.provider === 'steam' ? 'STEAM' : 'LOCAL'}
                    </small>
                  </div>
                </div>
                <div>
                  <strong>CAPA DO LAUNCHER</strong>
                  <small>
                    Confira a arte e os labels nas dimensões reais do launcher.
                  </small>
                  <button
                    type="button"
                    onClick={() => setCoverPreviewOpen(true)}
                  >
                    PRÉ-VISUALIZAR
                  </button>
                </div>
              </section>
            </>
          )}
        </section>
      </section>
      <p className="collection-status" role="status">
        {notice}
      </p>
      {pendingRemoval && (
        <aside
          className="collection-confirm"
          role="alertdialog"
          aria-modal="true"
          aria-labelledby="collection-confirm-title"
          onKeyDown={(event) => {
            if (event.key === 'Escape' && !busy) setPendingRemoval(null);
          }}
        >
          <section className="mechanical-panel">
            <ResponsiveChamferOutline
              cut={9}
              layers={[
                { inset: 0.8, color: '#a65e57', strokeWidth: 1.2 },
                { inset: 4.5, color: '#343c44', strokeWidth: 1 },
              ]}
            />
            <h2 id="collection-confirm-title">REMOVER DOS MEUS JOGOS?</h2>
            <p>
              {pendingRemoval.display_name} sairá da coleção. Cadastro, perfil,
              capa e histórico serão preservados.
            </p>
            <div>
              <button
                type="button"
                onClick={() => setPendingRemoval(null)}
                disabled={busy}
                autoFocus
              >
                CANCELAR
              </button>
              <button
                type="button"
                className="danger-action"
                onClick={() => void deactivate(pendingRemoval)}
                disabled={busy}
              >
                CONFIRMAR REMOÇÃO
              </button>
            </div>
          </section>
        </aside>
      )}
      {coverPreviewOpen && selected && (
        <aside
          className="collection-preview-dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby="collection-preview-title"
          onKeyDown={(event) => {
            if (event.key === 'Escape') setCoverPreviewOpen(false);
          }}
        >
          <button
            type="button"
            className="collection-preview-dialog__backdrop"
            aria-label="Fechar pré-visualização"
            onClick={() => setCoverPreviewOpen(false)}
          />
          <section className="mechanical-panel">
            <header>
              <div>
                <p className="eyebrow">PREVIEW EXATO DO RUNTIME</p>
                <h2 id="collection-preview-title">{selected.display_name}</h2>
              </div>
              <button
                type="button"
                aria-label="Fechar pré-visualização"
                onClick={() => setCoverPreviewOpen(false)}
                autoFocus
              >
                ×
              </button>
            </header>
            <CoverFrame
              title={selected.display_name}
              mediaKey={selected.last_media_key}
              providerBadge={selected.provider === 'steam' ? 'STEAM' : 'LOCAL'}
              coverUrl={selectedCoverSource}
            />
            <footer>
              <button type="button" onClick={() => setCoverPreviewOpen(false)}>
                VOLTAR PARA MEUS JOGOS
              </button>
            </footer>
          </section>
        </aside>
      )}
    </div>
  );
}

function CollectionCover({ entry }: { entry: CollectionEntry }) {
  const [source, setSource] = useState('');
  useEffect(() => {
    let active = true;
    if (!entry.active_cover) return;
    void collectionApi
      .cover(entry.game_id)
      .then((artwork) => {
        if (!active || !artwork) return;
        setSource(artworkDataUrl(artwork));
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [entry.active_cover, entry.game_id]);
  return source ? (
    <img src={source} alt="" />
  ) : (
    <span className="collection-cover-placeholder" aria-hidden="true">
      MD
    </span>
  );
}

function artworkDataUrl(artwork: { mime_type: string; bytes: number[] }) {
  const binary = artwork.bytes
    .map((byte) => String.fromCharCode(byte))
    .join('');
  return `data:${artwork.mime_type};base64,${window.btoa(binary)}`;
}

function providerLabel(provider: CollectionEntry['provider']) {
  return provider === 'steam' ? 'STEAM' : 'EXECUTÁVEL LOCAL';
}

function stateLabel(entry: CollectionEntry) {
  if (entry.offline) return 'BIBLIOTECA OFFLINE';
  if (!entry.installed) return 'NÃO ENCONTRADO';
  return 'INSTALADO';
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat('pt-BR', {
    dateStyle: 'short',
    timeStyle: 'short',
  }).format(new Date(value));
}
