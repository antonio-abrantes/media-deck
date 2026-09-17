# Session Handoff

> Este é o documento vivo de continuidade. Atualizar ao final de toda sessão com alteração material. Manter somente o estado necessário para a próxima sessão; decisões permanentes ficam na spec ou em ADR.

## Implementado — Fase 11

- metadata do bundle: publisher, category `Utility`, short/long description,
  copyright, ícones explícitos e identificador `com.mediadeck.desktop`;
- NSIS per-user (`installMode: currentUser`) com idiomas `English` +
  `PortugueseBR` e seletor de idioma;
- WebView2 via `embedBootstrapper` silencioso;
- hooks NSIS em `src-tauri/windows/hooks.nsh` removem o valor HKCU Run
  `MediaDeck` no uninstall; dados em `%LOCALAPPDATA%\MediaDeck` são retidos
  de propósito;
- updater desativado: nenhum `tauri-plugin-updater` e `plugins: {}`;
- scripts `scripts/release/Build-Release.ps1` e `New-SbomInventory.ps1`
  geram artefatos, `SHA256SUMS.txt` e inventário SBOM;
- notas RC em `docs/releases/0.1.0.md`, guia em `docs/PACKAGING.md` e matriz
  em `docs/RELEASE_MATRIX.md`;
- assinatura de código documentada (thumbprint/`signCommand`) sem certificado
  no repositório;
- critérios P0/P1 do produto estão cobertos no código das fases 1–10; não há
  defeito P0/P1 aberto conhecido — validação em VM limpa e hardware permanece
  humana (Gate M5 / resíduos M4);
- `pnpm tauri build` (2026-09-17) gerou
  `MediaDeck_0.1.0_x64-setup.exe` e `media-deck.exe`; checksums e SBOM em
  `docs/releases/0.1.0/`.

## Implementado — Fase 10

- capabilities por janela e CSP restrita foram reauditadas;
- parser recebeu testes adversariais determinísticos e mutação byte a byte;
- remoção durante apresentação física possui teste de cancelamento/rearme;
- limites já existentes de artwork e download foram preservados;
- logs JSON têm rotação diária e retenção máxima de sete arquivos;
- Settings exporta diagnóstico JSON sanitizado e limitado;
- backup `.mdbak` cria snapshot SQLite consistente, inclui artwork/labels e
  manifesto SHA-256; restore rejeita traversal, links, entradas/tamanhos
  excessivos e checksums divergentes;
- restore validado é aplicado antes da abertura do banco no próximo reinício e
  preserva os dados anteriores em `backups/pre-restore-*`;
- baselines manuais cobrem startup do banco, parser e timer idle;
- nenhum provider aceita API key nesta versão, mantendo zero segredos
  persistidos; provider autenticado futuro exigirá storage protegido;
- documentação operacional consolidada em `HARDENING_AND_RECOVERY.md`.

## Implementado — Fase 9

- `SETTINGS` controla monitor global, unidades autorizadas, monitor de abertura
  do launcher e autostart estritamente opt-in;
- autostart per-user usa o valor `MediaDeck` no Run key de HKCU e
  `--background`; desmarcar remove o valor;
- runtime nasce invisível; fechar janelas oculta para o tray, enquanto `Sair`
  marca shutdown explícito e encerra o monitor nativo;
- tray abre/restaura/foca `main`, mostra estado e pausa/retoma o monitor;
- mídia autorizada só abre o launcher após leitura limitada e parser estrito de
  `GAME.INI`, além da confirmação do `PROFILE_ID` no banco local;
- fluxo físico resolve a capa pelo ID, anima de 10% a 100%, chama Steam ou o
  executável local e supervisiona o processo lançado;
- monitor escolhido é persistido e usado para centralizar a janela runtime;
- build, lint, 25 testes frontend, 4 E2E em `1024×680` e 202 testes Rust
  executados passaram; 3 testes físicos permaneceram ignorados;
- gate M4 ainda requer reinício real do Windows e hardware-in-loop.

## Implementado — catálogo versus coleção

- [`ADR-011`](adr/ADR-011-verified-export-game-collection.md) aceita e migration
  `004_game_activations.sql` adicionada com backfill exclusivo de mídias
  anteriormente verificadas;
- `GameActivation`, repository SQLite/fake, serviço e IPC tipados implementados;
- export avulso agora executa `write + sync + readback + parse v2 + hash` antes
  do upsert da activation; reexport preserva a primeira data e incrementa a
  contagem;
- `MEUS JOGOS` é a primeira aba, mostra somente activations e possui ações
  compartilhadas para perfil, capa, regeneração e remoção confirmada;
- catálogo anterior foi preservado como `ADICIONAR JOGOS`, oculta ativados por
  padrão e oferece `MOSTRAR JÁ ADICIONADOS`;
- `collection_cover_get` entrega somente a capa ativa de um jogo ativado e fica
  restrito ao permission set administrativo; runtime não recebeu permissões;
- o fluxo resolve o `profile_id` explícito da activation e não depende de
  `profiles[0]`.
- frontend build/lint/format, 25 testes Vitest, E2E administrativo em
  `1024×680`, Rust clippy e 195 testes Rust (194 executados, 1 ignorado)
  passaram;
- preview do Media Creator foi limitado à altura útil do painel, com scroll
  interno e margem inferior preservada sem aumentar a janela;
- `.cursor/rules/single-review-gate.mdc` limita o processo a uma revisão
  automática; após as correções, checks objetivos aprovados encerram o gate.

## Correção do fluxo de perfis e GAME.INI

- Media Creator agora é exclusivamente o gerador de `GAME.INI`; controles de
  tipo físico, unidade, recorder, burn, erase, importação e verificação saíram;
- `MEDIA_KIND` é omitido quando o arquivo é apenas gerado;
- display name, provider, App ID, executável local, working directory,
  argumentos e process hints são editáveis na própria tela;
- a capa pode ser escolhida na própria tela e é copiada para o cache seguro;
- `dialog:allow-open` habilita seletores nativos para `.exe`, pasta de trabalho
  e imagens; campos técnicos receberam nomes, exemplos e ajuda para leigos;
- Library possui `EDITAR PERFIL` para display name, executável local, working
  directory, argumentos e process hints;
- Media Creator mostra `PROFILE_ID`, provider, App ID e alvo local resolvido;
- a capa ativa gera `[ARTWORK]` com UUID e caminho relativo seguro do cache,
  sem serializar caminhos executáveis;
- frontend, build, E2E 1024×680, Rust check/clippy e 194 testes Rust passaram.

## Identificação

| Campo              | Valor                                                                |
| ------------------ | -------------------------------------------------------------------- |
| Última atualização | 2026-09-17                                                            |
| Fase atual         | Fase 11 — Packaging e release candidate                               |
| Marco atual        | WP-1101–1110 implementados; Gate M5 (VM limpa) pendente de execução humana |
| Estado             | RC empacotável; preencher `RELEASE_MATRIX.md` após build/instalação      |
| Branch             | Não inspecionada; operações Git são exclusivas do revisor             |
| Último commit      | Não inspecionado; operações Git são exclusivas do revisor             |

## Diretriz obrigatória para próximas telas

Antes de criar ou alterar qualquer janela, tela, componente visual, card ou
painel, o agente deve ler integralmente
[`UI_CONSTRUCTION_GUIDE.md`](UI_CONSTRUCTION_GUIDE.md) e a spec visual aplicável.
Chassi frameless, transparência nativa, chanfros, contornos SVG, paralelismo das
camadas e validação na janela Tauri real devem reutilizar o método documentado;
não reintroduzir bordas retangulares recortadas, `inset box-shadow` como traço
final ou sombra nativa do Windows.

## Atualização mais recente — Label Studio WP-0812–0816

- [x] fluxo básico reorganizado em
      `Jogo > Finalidade > Imagem > Ajuste e exportação`;
      imagem nova preenche o canvas, handles são livres com proporção via
      `Shift`, e o recorte interno `cover` possui pan, zoom e centralização;
- [x] jogo/cache movido para o primeiro passo; finalidade digital
      `launcher_cover` separada das etiquetas físicas, com preset
      `1518×2076 px`, `editor_source` não ativo e aplicação explícita ao jogo;
- [x] `SALVAR PROJETO` distinguido de `EXPORTAR COMO…`; exportação abre diálogo
      nativo, grava no destino escolhido e apresenta o path absoluto;
- [x] selects do Media Creator usam linhas fixas para label, controle e ajuda;
- [x] correção posterior do chassi externo da janela `main`: superfície nativa
      transparente e sem sombra, fundos de `html/body/#root` transparentes e
      contorno SVG medido no viewport para preservar o corte de `18px` durante
      redimensionamento;
- [x] o `runtime` continua usando o contorno fixo aprovado `865×458`, sem
      alteração de layout ou geometria;
- [x] terceiro módulo real e exclusivo da janela `main`, com tabs acessíveis e
      workspace ideal `1440×900`;
- [x] canvas Konva em milímetros, réguas, guias, zoom/pan, seleção, Transformer,
      snap e referência de `Stage` preparada sem exportar;
- [x] elementos image/text/rect/line/logo/background e propriedades numéricas;
- [x] layers com lock, visibility, ordenação e undo/redo limitado/coalescido,
      separado de `scene.revision`;
- [x] CRUD IPC list/load/save/delete integrado com concorrência otimista;
- [x] artwork listado por `game_id` e resolvido individualmente por
      `artwork_id`, sem aceitar path/URL e sem despejar todos os blobs no IPC;
- [x] templates/custom size e presets disquete `70×52`, jewel `120×120` e disco
      circular com furo editável;
- [x] painéis construídos com massas chanfradas e `ChamferOutline`;
- [x] runtime preservado e seus dois E2E continuam verdes;
- [x] PNG físico no DPI da cena (300 padrão), dimensões arredondadas e `pHYs`;
- [x] PDF raster com `MediaBox` exato, sem fit-to-page;
- [x] folha A4 com régua vetorial de 100 mm e instrução tamanho real/100%;
- [x] core valida o destino escolhido pelo diálogo nativo e continua
      controlando formato, dimensões, raster e extensão;
- [x] thumbnail de até 320 px salvo em `AppDirs.labels/thumbnails`, associado
      ao projeto e exibido na lista por IPC baseado em `project_id`;
- [x] limites de bytes/pixels/arestas, assinatura e dimensões validados antes
      da gravação;
- [x] Label Studio carregado sob demanda em chunk próprio; Library e Media
      Creator não pagam o custo inicial de Konva;
- [x] captura determinística `1440×900` registrada em
      `docs/assets/screenshots/phase8-label-studio.png`;
- [x] janela Tauri real iniciou após a migration 003 com todos os serviços
      administrativos disponíveis;
- [ ] gate físico: operador ainda deve imprimir e medir a régua com tolerância
      final de 1 mm; não validado por automação.
- [x] verificação desta entrega: `cargo fmt --check`, Clippy com warnings
      negados, 191 testes Rust passados (3 HIL ignorados), Prettier, ESLint,
      TypeScript, 24 testes frontend, build Vite e 4 E2E Playwright passaram;
- [ ] revisão especializada automática não iniciou por limite de uso do modelo;
      manter revisão humana do diff antes da integração.

## Objetivo da sessão anterior

Implementar integralmente a Fase 7 por solicitação explícita do proprietário,
incluindo Steam Library, artwork, cadastro local, Media Creator, disquete,
ISO/IMAPI 2, verificação, erase e a UI administrativa conforme
`UI_CONSTRUCTION_GUIDE.md`, sem alterar o runtime aprovado.

## Atualização mais recente — Fase 7

- [x] WP-0701–0705: descoberta Steam via registro/fallback, parsers VDF/ACF
      limitados, scan full/incremental e reconciliação conservadora;
- [x] WP-0706–0707: cache de arte content-addressed, overrides e adapter remoto
      opcional fail-closed;
- [x] WP-0708–0709: Library pesquisável/filtrável, detalhes, cadastro `.exe` e
      revisão nativa de `.lnk` sem execução;
- [x] WP-0710–0713: wizard, preview canônico v2, importação v1 confirmada,
      histórico/verificação e escrita atômica de disquete;
- [x] WP-0714–0718: staging, ISO9660/Joliet, IMAPI 2, progresso/cancelamento,
      finalização, readback e erase RW com confirmação reforçada;
- [x] UI `main` separada em módulos Library e Media Creator, com chassi e
      painéis chanfrados por `ChamferOutline`;
- [x] runtime da Fase 6 não foi alterado;
- [x] evidência visual administrativa atualizada em
      `docs/assets/screenshots/phase7-main.png`;
- [x] Tauri real iniciou banco, watcher, supervisor, runtime coordinator e
      serviços administrativos sem erro;
- [ ] Gate M3 físico: executar escrita/verificação em `A:` e burn/verify/erase
      em `G:` com mídia real e operador.

## Trabalho concluído

- [x] WP-0706: cache content-addressed sob `AppDirs.artwork`, com metadata na
      tabela `artwork`, deduplicação SHA-256 e resolução de path relativo seguro;
- [x] PNG, JPEG e WebP são conferidos por assinatura/MIME, limitados por bytes,
      dimensões, pixels e alocação, e recodificados para remover metadata;
- [x] migration `002_artwork_dedup_references` permite que múltiplas referências
      compartilhem um blob e mantém unicidade por jogo, papel e hash;
- [x] seleção ativa prioriza `is_user_override`; import remoto não rebaixa nem
      substitui a prioridade de arte manual;
- [x] WP-0707: contratos de provider/downloader e adapter de política HTTPS com
      allowlist exata, limites e bloqueio de IP, userinfo e porta não padrão;
- [x] capability remota ausente e desabilitada por padrão, sem API key
      obrigatória e sem cliente HTTP ou rede nos testes;
- [x] ADR-007 registra armazenamento, schema e fronteira remota;
- [x] WP-0701–0705: descoberta HKCU/HKLM/WOW64 e fallback validado, parser
      limitado de `libraryfolders.vdf`/`appmanifest_*.acf`, scans full e
      incremental e reconciliação transacional no SQLite;
- [x] bibliotecas inacessíveis preservam o último estado; ausência confirmada
      em full scan marca `installed=false`; incremental não reconcilia ausências;
- [x] scanner cria `Game` + `LaunchProfile` Steam para novos AppIDs e preserva
      perfis existentes e campos listados em `metadata_json.manual_overrides`;
- [x] fixtures Steam temporárias cobrem formatos VDF, manifest, traversal,
      limites, incremental e biblioteca desconectada sem Steam real;
- [x] confirmado que o defeito não era exclusivo de desenvolvimento: o runtime
      estava configurado com `transparent: false` e herdava a sombra nativa;
- [x] janela Tauri `runtime` alterada para `transparent: true` e `shadow: false`;
- [x] `html`, `body` e `#root` ficam transparentes somente quando a superfície é
      `runtime`; a janela administrativa preserva seu fundo opaco;
- [x] superfície é resolvida antes do primeiro render e gravada em
      `document.documentElement.dataset.surface`;
- [x] teste de configuração fixa transparência e ausência de sombra; E2E fixa os
      três fundos computados como `rgba(0, 0, 0, 0)`;
- [x] executável nativo de desenvolvimento compilado e aberto com a nova
      configuração para conferência direta do proprietário;
- [x] identificado que `Michroma 400` era a causa da anatomia leve e estreita;
- [x] confirmado que `Orbitron-Bold.woff2` e sua licença OFL já estavam
      incorporados ao projeto como fonte local de marca;
- [x] logotipo alterado para `Orbitron MediaDeck 700`, tamanho `26px`, expansão
      horizontal `1.33`, preenchimento metálico claro e relevo escuro;
- [x] subtítulo, painéis, textos, geometria e demais elementos não foram alterados;
- [x] E2E passou a verificar família, peso, largura e altura do logotipo;
- [x] comparação minuciosa entre o protótipo, a captura reprovada e o runtime renderizado;
- [x] chassi externo corrigido para corte angular de 18 px, sem cantos arredondados;
- [x] contorno externo reduzido a uma linha fina cinza-amarronzada; removida a
      cunha decorativa que criava um segundo desenho no canto superior direito;
- [x] identificado que `inset box-shadow` era retangular e era apenas recortado
      pelo `clip-path`, causando a interrupção visual denunciada pelo proprietário;
- [x] bordas do terminal, footer e painel de capa refeitas com três silhuetas
      chanfradas concêntricas — linha, sulco e segunda moldura — com diagonais
      paralelas `10/9/7 px` e `9/8/6 px`;
- [x] contorno externo substituído por polígono SVG acima de todos os filhos;
      o header não consegue mais encobrir ou cortar a linha do canto;
- [x] terminal, footer e capa receberam dois contornos SVG independentes, acima
      do conteúdo, com junções `miter` e espessura imune à escala;
- [x] quina interna do display alinhada ao chanfro do terminal, passando de corte
      local de `3px` para `6px`;
- [x] segunda linha da moldura da capa afastada para `8px`, conforme a proporção
      observada no protótipo ampliado;
- [x] reconstrução do painel esquerdo como moldura mecânica separada do display CRT;
- [x] reposicionamento do título, logs e loading nas faixas verticais do protótipo;
- [x] loading âmbar com 24 segmentos, trilho de 459×29 px e porcentagem proporcional;
- [x] reconstrução do painel de capa com rebaixo em camadas e área útil de 253×346 px;
- [x] fallback de capa sem imagem transformado em composição intencional; ausência de arte não causa crash;
- [x] footer refeito com moldura própria, ícones maiores, LEDs e texto verde envelhecido;
- [x] footer deixa de exibir `865×458` e recebe do Rust a resolução física e a
      escala do monitor atual por `runtime_get_monitor_info`;
- [x] estado sem informação de monitor exibe `MONITOR UNKNOWN` e
      `RESOLUTION UNAVAILABLE`, sem fabricar a resolução do viewport;
- [x] fixture visual somente em desenvolvimento por ?preview=prototype, sem simular estado real em produção;
- [x] teste E2E fixa geometria, as três camadas paralelas dos painéis, a ausência
      de decoração no canto externo, a resolução da fixture e a captura 865×458;
- [x] captura atualizada em docs/assets/screenshots/runtime-865x458.png;
- [x] solução de chassi, transparência, chanfros, contornos SVG, empilhamento e
      validação nativa consolidada em `docs/UI_CONSTRUCTION_GUIDE.md`;
- [x] guia visual adicionado ao índice, às regras de documentação e marcado
      como leitura obrigatória para novas telas, componentes e painéis;
- [x] skill externa `mediadeck-media-artwork` criada dentro de `docs/skills/`,
      sem integração ou dependência do runtime do aplicativo;
- [x] contratos definidos para capa fixa `1518×2076`, disquete `70×52 mm`,
      jewel case `120×120 mm` e frente de DVD `130×183 mm`, todos os presets
      físicos a 300 PPI;
- [x] templates SVG, regras de direção de arte, proveniência, direitos, bleed,
      safe area e normalização de export adicionados à skill;
- [x] validador PNG sem dependências externas criado para conferir dimensões e,
      em modo estrito, metadados de densidade de 300 PPI;
- [x] par de teste de Dragon Ball Z: Kakarot gerado com `GAME-001`, Steam AppID
      `851850` e provider `STEAM`;
- [x] teste revelou que o gerador não respeita pixels exatos; a skill foi
      reforçada com recorte central mínimo, reamostragem Lanczos, proibição de
      esticar e reinspeção obrigatória;
- [x] arquivos finais do exemplo normalizados para `1518×2076` e `827×614`, com
      a etiqueta codificada a 300 PPI e manifest de proveniência;
- [ ] WP-0612: capturas multi-DPI e assinatura humana do proprietário.

## Geometria verificada no viewport canônico

| Região | Geometria |
| --- | --- |
| Launcher | x=0, y=0, 865×458 |
| Terminal | x=19, y=58, 550×326 |
| Footer | x=19, y=390, 550×57 |
| Painel de capa | x=577, y=58, 277×389 |
| Arte/fallback da capa | x=589, y=80, 253×346 |
| Barra de progresso | x=38, y=313, 459×29 |

## Testes e verificações executados

- `pnpm format`, `pnpm format:check`, `pnpm lint`, `pnpm typecheck` e
  `pnpm build` — verdes para WP-0802–0811;
- `pnpm test` — 6 arquivos e 22 testes verdes, incluindo 8 do engine/store;
- `pnpm test:e2e` — 4 testes verdes, incluindo Label Studio `1440×900` e os
  dois fluxos do runtime;
- `cargo fmt --check`, `cargo check --all-targets --all-features` e
  `cargo clippy --all-targets --all-features -- -D warnings` — verdes;
- `cargo test --all-targets --all-features` — 186 unitários verdes, 1 ignorado,
  2 fixtures e 5 testes de processo verdes; 2 ópticos ignorados por hardware;
- revisão especializada final — nenhum achado HIGH ou CRITICAL restante;
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` — verde;
- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets --all-features` — verde;
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` — verde;
- `cargo test --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- --test-threads=1` —
  180 testes unitários, 2 fixtures de INI e 5 testes de processo verdes; 1
  teste unitário e 2 testes ópticos hardware-in-loop ignorados;
- `pnpm format:check` — verde;
- `pnpm lint` — verde, zero warnings;
- `pnpm typecheck` — verde;
- `pnpm test` — 5 arquivos, 14 testes verdes;
- `pnpm test:e2e` — 3 testes verdes antes do ajuste de navegação; após o ajuste,
  o E2E específico da janela `main` passou novamente;
- `pnpm build` — verde, 57 módulos transformados;
- `pnpm tauri dev` — janela nativa iniciou com banco, watcher, supervisor,
  runtime coordinator e biblioteca administrativa prontos;
- inspeção visual de `docs/assets/screenshots/phase7-main.png` — concluída;
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` — verde;
- `cargo check --manifest-path src-tauri/Cargo.toml --all-targets --all-features` — verde;
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` — verde;
- `cargo test --manifest-path src-tauri/Cargo.toml --all-targets --all-features` —
  151 testes unitários, 2 fixtures de INI e 5 testes de processo verdes; 2 testes
  ópticos ignorados porque exigem operador e mídia física;
- inspeção visual da captura canônica 865×458 — concluída;
- `corepack pnpm tauri dev` validou o schema: a opção opcional
  `noRedirectionBitmap` não é suportada pela versão Tauri fixada e foi removida;
- o Tauri compilou o executável nativo; como uma instância do servidor visual já
  ocupava a porta 1420, `src-tauri/target/debug/media-deck.exe` foi aberto
  diretamente contra esse servidor;
- a automação desta sessão não recebeu superfície de captura para aplicativos
  nativos; a janela corrigida foi deixada aberta para inspeção do proprietário;
- `pnpm test`, `pnpm build` e `pnpm test:e2e` falharam inicialmente no sandbox
  com `spawn EPERM`; foram repetidos fora do sandbox e passaram.
- `quick_validate.py docs/skills/mediadeck-media-artwork` — skill válida;
- `validate_artwork.py ... --preset floppy_35_label --strict-density` — capa
  `1518×2076` e etiqueta `827×614` aprovadas;
- inspeção visual das duas artes do exemplo após a normalização — concluída;
- pesquisa de medidas confirmou Avery L7666 `70×52 mm`, jewel front
  `120×120 mm` e frente de keep case DVD `130×183 mm` como presets-base.

Nenhum comando Git foi executado.

## Arquivos alterados

### Label Studio WP-0802–0811

- `package.json` e `pnpm-lock.yaml`
- `src/features/label-studio/*`
- `src/features/main/MainWindow.tsx`, `MainWindow.css` e testes
- `e2e/label-studio.spec.ts` e `e2e/main-library.spec.ts`
- `src-tauri/src/application/artwork.rs`
- `src-tauri/src/domain/ports.rs`
- `src-tauri/src/infrastructure/database/artwork.rs`
- `src-tauri/src/ipc/labels.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/permissions/session.toml`
- `src-tauri/tauri.conf.json`
- `docs/LABEL_STUDIO.md`, `docs/README.md` e `docs/IMPLEMENTATION_PLAN.md`

### Artwork WP-0706–0707

- src-tauri/Cargo.toml
- src-tauri/Cargo.lock
- src-tauri/migrations/002_artwork_dedup_references.sql
- src-tauri/src/application/artwork.rs
- src-tauri/src/application/mod.rs
- src-tauri/src/domain/artwork.rs
- src-tauri/src/domain/errors.rs
- src-tauri/src/domain/mod.rs
- src-tauri/src/domain/ports.rs
- src-tauri/src/infrastructure/artwork.rs
- src-tauri/src/infrastructure/artwork/remote.rs
- src-tauri/src/infrastructure/database/artwork.rs
- src-tauri/src/infrastructure/database/mod.rs
- src-tauri/src/infrastructure/mod.rs
- docs/adr/ADR-007-content-addressed-artwork-cache.md
- docs/DATA_MODEL.md
- docs/IMPLEMENTATION_PLAN.md
- docs/SECURITY.md
- docs/SESSION_HANDOFF.md
- docs/TECHNICAL_SPEC.md
- docs/TEST_STRATEGY.md

### Steam Library WP-0701–0705

- src-tauri/Cargo.toml
- src-tauri/Cargo.lock
- src-tauri/src/application/library.rs
- src-tauri/src/application/mod.rs
- src-tauri/src/domain/entities.rs
- src-tauri/src/domain/errors.rs
- src-tauri/src/domain/ports.rs
- src-tauri/src/infrastructure/database/catalog.rs
- src-tauri/src/infrastructure/database/games.rs
- src-tauri/src/infrastructure/providers/steam.rs
- docs/IMPLEMENTATION_PLAN.md
- docs/TECHNICAL_SPEC.md
- docs/SESSION_HANDOFF.md

### Implementação visual e resolução do monitor

- src/main.tsx
- src/styles/global.css
- src/shared/components/WindowChassis/WindowChassis.css
- src/shared/components/WindowChassis/WindowChassis.tsx
- src/shared/components/ChamferOutline/ChamferOutline.tsx
- src/shared/components/ChamferOutline/ChamferOutline.css
- src/features/runtime/RuntimeWindow.css
- src/styles/fonts.css
- src/styles/launcher-tokens.css
- src/features/runtime/RuntimeWindow.tsx
- src/features/runtime/components/BootTerminal.css
- src/features/runtime/components/CoverFrame.css
- src/features/runtime/components/HardwareStatusFooter.tsx
- src/features/runtime/components/HardwareStatusFooter.css
- src/features/runtime/hooks/useSessionSnapshot.ts
- src/features/runtime/monitor-mode.ts
- src/features/runtime/store/session-store.ts
- src/features/runtime/types.ts
- src-tauri/src/ipc/runtime.rs
- src-tauri/src/lib.rs
- src-tauri/permissions/session.toml

### Teste, evidência e configuração

- src-tauri/tauri.conf.json
- src/test/tauri-config.test.ts
- e2e/runtime-vertical-slice.spec.ts
- src/features/runtime/components/HardwareStatusFooter.test.tsx
- docs/assets/screenshots/runtime-865x458.png
- docs/LAUNCHER_SPEC.md
- docs/TECHNICAL_SPEC.md
- docs/SECURITY.md
- docs/VISUAL_ACCEPTANCE.md
- docs/SESSION_HANDOFF.md

### Diretriz visual e skill externa

- docs/UI_CONSTRUCTION_GUIDE.md
- docs/README.md
- docs/DOCUMENTATION_RULES.md
- docs/PHYSICAL_MEDIA_SPEC.md
- docs/skills/mediadeck-media-artwork/SKILL.md
- docs/skills/mediadeck-media-artwork/agents/openai.yaml
- docs/skills/mediadeck-media-artwork/references/dimensions.md
- docs/skills/mediadeck-media-artwork/references/art-direction.md
- docs/skills/mediadeck-media-artwork/references/provenance-and-rights.md
- docs/skills/mediadeck-media-artwork/references/export-normalization.md
- docs/skills/mediadeck-media-artwork/scripts/validate_artwork.py
- docs/skills/mediadeck-media-artwork/assets/templates/launcher-cover.svg
- docs/skills/mediadeck-media-artwork/assets/templates/floppy-35-label.svg
- docs/skills/mediadeck-media-artwork/assets/templates/cd-jewel-front.svg
- docs/skills/mediadeck-media-artwork/assets/templates/dvd-case-front.svg
- docs/skills/mediadeck-media-artwork/examples/dragon-ball-z-kakarot/dragon-ball-z-kakarot-launcher-cover.png
- docs/skills/mediadeck-media-artwork/examples/dragon-ball-z-kakarot/dragon-ball-z-kakarot-floppy-35-label.png
- docs/skills/mediadeck-media-artwork/examples/dragon-ball-z-kakarot/dragon-ball-z-kakarot-artwork-manifest.json
- docs/skills/mediadeck-media-artwork/examples/dragon-ball-z-kakarot/TEST_REPORT.md

## Riscos e pendências

- o bundle administrativo passou de 500 kB por incluir Konva; avaliar
  code-splitting do módulo antes do gate de performance, sem alterar o runtime;
- export PNG/PDF, impressão, calibração e thumbnail continuam deliberadamente
  ausentes e os botões correspondentes permanecem desabilitados;
- o contrato remoto está pronto, mas não há cliente HTTP de produção; isso é
  intencional para manter a capability ausente e fail-closed até provider e
  allowlist serem escolhidos explicitamente;
- limpeza de blobs órfãos continua somente explícita, conforme `DATA_MODEL.md`;
- `LibraryScanService`, IPC e a superfície administrativa já estão integrados;
- a convenção conservadora de override está documentada em
  `metadata_json.manual_overrides` (`display_name` e `install_dir`);
- o proprietário ainda precisa aprovar ou pedir ajustes sobre a captura atual;
- as capturas Windows em 125%, 150% e 200% continuam pendentes;
- a captura canônica usa uma resolução determinística de monitor (`1280×960`) na
  fixture de navegador; a revisão final deve confirmar a resolução física na
  janela Tauri/WebView2 real e nos monitores do proprietário;
- a configuração e o DOM transparentes foram automatizados, mas a conferência
  visual da composição nativa aberta depende do proprietário porque a captura
  de aplicativos Windows não estava disponível para o agente nesta sessão;
- nenhuma arte de jogo foi incorporada: o estado validado usa deliberadamente o fallback sem imagem;
- a arte de Kakarot é exemplo da skill externa e não foi conectada ao runtime ou
  copiada para o cache do aplicativo;
- personagens, títulos e marcas de jogos/providers continuam sujeitos às
  permissões de uso do cliente; a skill registra proveniência e usa badge
  textual de provider por padrão, mas não concede direitos sobre terceiros;
- `70×52 mm`, `120×120 mm` e `130×183 mm` são presets-base; estoque de etiqueta
  ou gráfica com dieline próprio deve sobrescrevê-los;
- o gerador de imagens pode ignorar pixels exatos, portanto a normalização e o
  validador não podem ser pulados;
- WP-0411 de disquete físico permanece fora do escopo desta sessão.

## Comandos não Git executados nesta sessão (Fase 11)

- inspeção de `tauri.conf.json`, schema do CLI Tauri e docs de NSIS/WebView2;
- criação de `src-tauri/windows/hooks.nsh`, scripts em `scripts/release/` e
  documentos `PACKAGING.md`, `RELEASE_MATRIX.md`, `releases/0.1.0.md`;
- atualização de `IMPLEMENTATION_PLAN.md`, `SESSION_HANDOFF.md`, `README.md` e
  metadados em `Cargo.toml` / `package.json`;
- `corepack pnpm tauri build` (release + NSIS) — sucesso;
- `scripts/release/Build-Release.ps1 -SkipBuild` — checksums + SBOM em
  `docs/releases/0.1.0/`.

Gate M5 (install/uninstall em VM limpa) permanece com o operador humano.

Nenhum comando Git foi executado.

## Comandos Git sugeridos

Operações Git permanecem exclusivas do revisor humano. Quando for versionar a
Fase 11, incluir `src-tauri/tauri.conf.json`, `src-tauri/windows/hooks.nsh`,
`scripts/release/`, `package.json`, `Cargo.toml` e os docs públicos
(`IMPLEMENTATION_PLAN`, `SESSION_HANDOFF`, `PRODUCT_SPEC`, README). Demais docs
de packaging permanecem locais conforme `.gitignore`.

## Próximo passo exato

1. Rodar `.\scripts\release\Build-Release.ps1 -Version 0.1.0` (ou
   `pnpm release:build`).
2. Preencher `docs/RELEASE_MATRIX.md` em VM Windows limpa (install/uninstall,
   WebView2, autostart, smoke de mídia se houver hardware).
3. Opcional: configurar assinatura de código conforme `docs/PACKAGING.md` antes
   de distribuição pública.

## Critério de encerramento da próxima sessão

- artefatos RC com checksums e SBOM gerados;
- matriz Gate M5 com evidência de install/uninstall limpos;
- riscos residuais aceitos ou mitigados explicitamente no handoff.
