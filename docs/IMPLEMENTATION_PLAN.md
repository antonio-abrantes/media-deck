# Plano Completo de Implementação

## 1. Estratégia

A implementação do MediaDeck será incremental e orientada por uma vertical slice. Primeiro deve existir um caminho completo, ainda simples, que detecta uma mídia simulada/real em uma unidade configurada, interpreta o perfil, mostra eventos, lança uma fixture e fecha com segurança. Biblioteca rica, gravação óptica, artes e editor entram depois que o núcleo de sessão estiver confiável.

Estimativas são relativas e assumem uma pessoa desenvolvedora com apoio de IA. Elas não substituem medição após o bootstrap.

## 2. Marcos

| Marco                       | Resultado                                  | Fases |
| --------------------------- | ------------------------------------------ | ----- |
| M0 — Specification Baseline | documentação aprovada                      | 0     |
| M1 — Technical Skeleton     | app abre, IPC tipado, DB e testes básicos  | 1–2   |
| M2 — Vertical Slice         | mídia → shell → launch → safe close        | 3–5   |
| M3 — Alpha                  | biblioteca + disquete + CD/DVD utilizáveis | 6–7   |
| M4 — Beta                   | etiquetas + tray + modo console            | 8–9   |
| M5 — Release Candidate      | segurança, instalador e QA completo        | 10–11 |

## 3. Fase 0 — Baseline documental

Objetivo: fixar escopo, arquitetura e regras antes de gerar código.

- [x] WP-0001 — consolidar produto e não objetivos;
- [x] WP-0002 — definir arquitetura React/Tauri/Rust;
- [x] WP-0003 — especificar `GAME.INI` v2 e compatibilidade v1;
- [x] WP-0004 — definir modelo de dados;
- [x] WP-0005 — definir segurança e estratégia de testes;
- [x] WP-0006 — criar `SESSION_HANDOFF.md`;
- [x] WP-0007 — aprovar nome **MediaDeck** e escopo com o proprietário;
- [x] WP-0008 — aprovar disquete e CD/DVD como mídias iniciais;
- [ ] WP-0009 — medir pelo menos uma etiqueta de disquete e uma mídia/encarte de CD real.

Gate M0:

- documentos revisados;
- pendências explícitas;
- próximo passo definido no handoff.

## 4. Fase 1 — Bootstrap do repositório

Estimativa: 1–2 dias.

- [x] WP-0101 — criar Tauri 2 + React + TypeScript + Vite;
- [x] WP-0102 — fixar Node 24 LTS, pnpm e Rust stable;
- [x] WP-0103 — habilitar TypeScript strict, ESLint e Prettier;
- [x] WP-0104 — configurar rustfmt, Clippy e warnings;
- [x] WP-0105 — criar aliases, feature folders e módulos Rust;
- [x] WP-0106 — configurar Vitest, Testing Library e `cargo test`;
- [x] WP-0107 — criar CI Windows;
- [x] WP-0108 — adicionar Conventional Commits e SemVer;
- [x] WP-0109 — configurar instância única e duas janelas vazias;
- [x] WP-0110 — aplicar capabilities mínimas e CSP inicial.

Gate:

- build debug e release;
- `pnpm lint`, `pnpm test`, `pnpm typecheck`, `cargo fmt --check`, `cargo clippy` e `cargo test` verdes;
- UI não possui shell permission genérica.

## 5. Fase 2 — Domínio, configuração e persistência

Estimativa: 3–5 dias.

- [x] WP-0201 — implementar IDs, clock e erros estáveis;
- [x] WP-0202 — implementar entidades de jogo, perfil, mídia e sessão;
- [x] WP-0203 — implementar entidade/configuração de `MediaDevice`;
- [x] WP-0204 — implementar state machine pura;
- [x] WP-0205 — criar ports para device, media writer, provider, process e repository;
- [x] WP-0206 — configurar SQLite/SQLx e migração 001;
- [x] WP-0207 — implementar repositórios e transações;
- [x] WP-0208 — implementar diretórios de dados/cache/log;
- [x] WP-0209 — implementar settings validadas;
- [x] WP-0210 — implementar tracing e correlation IDs;
- [x] WP-0211 — criar adapters fake para testes.

Gate:

- state machine com cobertura de transições críticas;
- DB criado e migrado em diretório temporário;
- aplicação abre offline.

## 6. Fase 3 — Perfil da mídia

Estimativa: 3–4 dias.

- [x] WP-0301 — parser v2 limitado e determinístico;
- [x] WP-0302 — parser/importador v1;
- [x] WP-0303 — validações, warnings e códigos por linha;
- [x] WP-0304 — builder a partir do catálogo e writer v2 canônico;
- [x] WP-0305 — gravação temporária, flush, rename e verify;
- [x] WP-0306 — `GAME.BAK` seguro;
- [x] WP-0307 — tests com remoção e write protection simulados;
- [x] WP-0308 — comando `media_validate` e DTOs.

Gate:

- fixtures válidas/invalidas passam;
- nenhuma entrada da mídia vira comando/caminho de execução;
- operação incompleta não aparece como sucesso.

## 7. Fase 4 — Device watcher e unidades configuráveis

Estimativa: 4–6 dias.

- [x] WP-0401 — criar janela Win32 oculta/integração com message loop;
- [x] WP-0402 — processar arrival/removal de volume;
- [x] WP-0403 — mapear unit mask para drive;
- [x] WP-0404 — identificar device instance/interface e tipo da unidade;
- [x] WP-0405 — reconciliar dispositivo estável e mount point atual;
- [x] WP-0406 — debounce e readiness;
- [x] WP-0407 — allowlist de unidades configuradas;
- [x] WP-0408 — fallback de polling lento;
- [x] WP-0409 — deduplicação/idempotência;
- [x] WP-0410 — eventos Tauri direcionados;
- [ ] WP-0411 — validar drive de disquete e drive óptico físicos.

Gate:

- uma inserção gera uma transição;
- remoção é detectada em todos os estados;
- idle não usa polling de 100 ms;
- letra diferente de `A:` funciona;
- CD/DVD inserido em `G:` é reconhecido;
- mídia em dispositivo não autorizado é ignorada.

Os gates automatizáveis estão verdes com adapters simulados e com smoke da
janela Win32 real. Em hardware, a unidade óptica `I:` leu, removeu e remontou o
CD de teste com `GAME.INI` v2; esse drive entregou remoção e reinserção pelo
fallback de polling, não pelo broadcast nativo nesta sessão. O gate
hardware-in-loop permanece aberto somente porque não há drive de disquete
disponível para concluir WP-0411.

## 8. Fase 5 — Launcher e Process Supervisor

Estimativa: 6–9 dias.

- [x] WP-0501 — criar executáveis-fixture para cenários de processo;
- [x] WP-0502 — implementar baseline e snapshots Windows;
- [x] WP-0503 — implementar `ExecutableProvider` sem shell, com target local, working directory e argumentos estruturados;
- [x] WP-0504 — implementar `SteamProvider.launch` por AppID;
- [x] WP-0505 — observar processo novo com timeout e preservar origem local/mídia dos hints;
- [x] WP-0506 — associar PID + criação + caminho, sem promover hint da mídia a identidade;
- [x] WP-0507 — descobrir janela principal e subprocessos;
- [x] WP-0508 — enviar `WM_CLOSE` e aguardar;
- [x] WP-0509 — implementar wait/detach/force;
- [x] WP-0510 — revalidar identidade antes do force;
- [x] WP-0511 — crash recovery sem ação destrutiva;
- [x] WP-0512 — testar launcher intermediário e PID reciclado.

Gate M2 parcial:

- nenhum processo baseline é encerrado;
- fixture responde ao fechamento normal;
- fixture não responsiva exige decisão;
- falha de associação nunca habilita auto-kill.

Os gates automatizáveis da Fase 5 estão verdes com `process-fixture` e
`Win32ProcessAdapter`. Scanner Steam completo, UI de sessão e bridge de
eventos de runtime permanecem nas Fases 6–7.

## 9. Fase 6 — Runtime retro vertical slice

Estimativa: 4–6 dias.

- [x] WP-0601 — criar store de sessão e event bridge;
- [x] WP-0602 — versionar fontes offline e respectivas licenças;
- [x] WP-0603 — criar `WindowChassis`, controles frameless acessíveis e capabilities mínimas;
- [x] WP-0604 — implementar `BootTerminal`, cover frame, footer e loading segmentado;
- [x] WP-0605 — sequence engine operation/flavor;
- [x] WP-0606 — estados boot, loading, running, error e closing;
- [x] WP-0607 — modos curto/normal/cinemático;
- [x] WP-0608 — reduce motion e controle de som;
- [x] WP-0609 — aplicar viewport rígido `865x458` (~1.889:1), sem breakpoints/redimensionamento e fullscreen apenas opcional;
- [x] WP-0610 — cancelar sequência em remoção/erro;
- [x] WP-0611 — E2E da vertical slice;
- [x] WP-0613 — reconstrução de fidelidade: 865x458 rígido, footer sob terminal, capa full-height, sem rebites, logo Orbitron Bold, fontes Silkscreen/VT323 e último comando ativo;
- [ ] WP-0612 — screenshots em 100/125/150/200%, comparação com o protótipo e aceite visual humano.

Gate M2:

- inserção real ou simulada percorre o fluxo completo;
- etapas reais não mentem estado;
- runtime fecha/esconde no momento configurado;
- falhas mostram recuperação;
- não existe barra ou borda nativa no launcher final;
- fontes locais, chassi, proporções, painéis, footer e loading âmbar correspondem ao protótipo;
- proprietário aprova explicitamente a comparação visual prevista em `LAUNCHER_SPEC.md`.

Automação da Fase 6 está verde (`pnpm test`, Playwright scaffold). O gate M2
permanece aberto para **aceite visual humano** (WP-0612) em
`docs/VISUAL_ACCEPTANCE.md`. Validar sempre a janela `runtime`, não a `main`.

Notas de domínio já cobertas antes da Fase 7 (não reabrir Fases 3–5):

- `PROCESS` v1 → `PROCESS_HINTS`; binding forte na Fase 5 (não kill por nome);
- `PROVIDER=executable` + caminho/args no `LaunchProfile` local (WP-0503);
- Media Creator / geração assistida de `GAME.INI` = WP-0709–0710.

## 10. Fase 7 — Steam Library, Media Creator e mídia óptica

Estimativa: 10–15 dias.

- [x] WP-0701 — detectar Steam pelo registro e fallback;
- [x] WP-0702 — parser de `libraryfolders.vdf`;
- [x] WP-0703 — parser de `appmanifest_*.acf`;
- [x] WP-0704 — scanner incremental e full;
- [x] WP-0705 — reconciliar bibliotecas offline/desconectadas;
- [x] WP-0706 — cache de arte local;
- [x] WP-0707 — adapter remoto opcional;
- [x] WP-0708 — tela Library com busca/filtros/detalhes;
- [x] WP-0709 — cadastro manual seguro de `.exe`, importação revisada de `.lnk`, argumentos, hints e capa;
- [x] WP-0710 — wizard Media Creator com seleção de perfil/mídia, preview e geração canônica de `GAME.INI`;
- [x] WP-0711 — importar/atualizar v1 para v2;
- [x] WP-0712 — histórico e verificação de mídias.
- [x] WP-0713 — `FloppyMediaWriter` e fluxo administrativo final;
- [x] WP-0714 — staging e exportação de imagem ISO;
- [x] WP-0715 — integrar gravadores ópticos via IMAPI 2;
- [x] WP-0716 — progresso de burn, finalização e cancelamento seguro;
- [x] WP-0717 — remontar e verificar `GAME.INI` após burn;
- [x] WP-0718 — erase/rewrite de CD-RW/DVD-RW com confirmação reforçada;

Gate M3:

- usuário importa Steam sem API key;
- cria mídia sem editar arquivo;
- configura e usa drives `A:` e `G:`;
- grava e verifica CD/DVD de dados;
- fluxo principal funciona sem rede;
- override manual não é sobrescrito pelo rescan.

Status do gate: validado pelo proprietário em 2026-09-16.

## 11. Fase 8 — Label Studio

Estimativa: 8–12 dias.

- [x] WP-0801 — definir schema versionado de scene JSON;
- [x] WP-0802 — canvas em mm, zoom e réguas;
- [x] WP-0803 — elementos de imagem, texto e forma;
- [x] WP-0804 — seleção, transformer, snap e alinhamento;
- [x] WP-0805 — layers, lock, visibility e ordenação;
- [x] WP-0806 — undo/redo;
- [x] WP-0807 — integração com artwork do jogo;
- [x] WP-0808 — templates e custom size;
- [x] WP-0809 — preset de etiqueta de disquete;
- [x] WP-0810 — preset quadrado de capa frontal de CD;
- [x] WP-0811 — preset circular de rótulo de CD/DVD;
- [x] WP-0812 — export PNG 300 DPI;
- [x] WP-0813 — export PDF em dimensão física;
- [x] WP-0814 — folha de calibração;
- [x] WP-0815 — persistência e thumbnail;
- [x] WP-0816 — testes dimensionais e de round-trip.

O core controla destinos, nomes, validação do raster, densidade PNG, `MediaBox`
PDF e thumbnails. O editor mantém histórico separado da revisão persistida e
resolve imagens por `artwork_id`. A automação dimensional está concluída; o
gate físico da régua continua aberto até medição por operador.

Gate:

- reabrir projeto não altera layout;
- PNG/PDF respeitam medidas;
- impressão calibrada fica dentro da tolerância;
- arquivo grande não congela nem excede limites.

## 12. Fase 9 — Tray, autostart e modo console

Estimativa: 3–5 dias.

- [x] WP-0901 — menu e estados do tray;
- [x] WP-0902 — fechar para tray;
- [x] WP-0903 — pausar/retomar monitor;
- [x] WP-0904 — checkbox `Iniciar o MediaDeck com o Windows`, desmarcado por
      padrão, com autostart estritamente opt-in e reversível nas Settings;
- [x] WP-0905 — launcher em janela horizontal seguindo `docs/assets/launcher-concept-v1.png`;
- [x] WP-0906 — seleção de monitor;
- [x] WP-0907 — restaurar janela e foco corretamente;
- [x] WP-0908 — garantir shutdown limpo.

Implementação concluída em 16/09/2026. O gate M4 aguarda o roteiro físico de
reinício do Windows e inserção/remoção de mídias descrito em
`SETTINGS_AND_TRAY.md`.

Gate M4:

- reboot mantém a opção escolhida;
- monitor funciona sem janela main aberta;
- sair pelo tray finaliza threads sem afetar jogo desvinculado.

## 13. Fase 10 — Hardening e observabilidade

Estimativa: 5–8 dias.

- [x] WP-1001 — revisar capabilities e CSP;
- [x] WP-1002 — fuzz/property tests do parser;
- [x] WP-1003 — corridas de mídia/processo;
- [x] WP-1004 — limites de imagem/download;
- [x] WP-1005 — armazenamento seguro de API key (nenhum provider aceita segredo);
- [x] WP-1006 — export de diagnóstico sanitizado;
- [x] WP-1007 — rotação de logs e retenção;
- [x] WP-1008 — backup/restore de dados do usuário;
- [x] WP-1009 — benchmark de startup, scan e idle;
- [x] WP-1010 — audit de dependências e licenças.

## 14. Fase 11 — Packaging e release candidate

Estimativa: 4–7 dias.

- [x] WP-1101 — ícones, metadata e identificador do app;
- [x] WP-1102 — NSIS per-user;
- [x] WP-1103 — pré-requisitos/WebView2;
- [x] WP-1104 — instalação/desinstalação limpa;
- [x] WP-1105 — canal de update assinado ou updater desativado;
- [x] WP-1106 — SBOM, checksums e release notes;
- [x] WP-1107 — assinatura de código quando disponível;
- [x] WP-1108 — executar matriz de release;
- [x] WP-1109 — fechar P0/P1;
- [x] WP-1110 — atualizar toda documentação e handoff.

Gate M5:

- RC instalável em VM limpa — checklist em `RELEASE_MATRIX.md` (execução humana);
- critérios do `PRODUCT_SPEC.md` atendidos no código; evidência física pendente na matriz;
- testes do `TEST_STRATEGY.md` documentados;
- riscos residuais publicados em `PACKAGING.md` e `SESSION_HANDOFF.md`.

## 15. Backlog pós-MVP

- GOG/Epic/Xbox/Battle.net providers;
- provider de emuladores/RetroArch;
- múltiplas unidades e sessões;
- assinatura opcional de mídia;
- sincronização/backup externo opt-in;
- marketplace/importação de templates;
- fila de impressão em folha com várias etiquetas;
- habilitação segura de mídia USB genérica;
- novos dispositivos físicos além de disquete e mídia óptica;
- modo kiosk com shell replacement, somente após revisão de risco.

## 16. Dependências críticas

| Dependência                         | Impacto                         | Tratamento                                            |
| ----------------------------------- | ------------------------------- | ----------------------------------------------------- |
| Comportamento de drives USB         | detecção/retirada inconsistente | native event + debounce + fallback                    |
| Identidade/letra de unidade muda    | alvo incorreto                  | device identity + reconciliação de mount point        |
| Burn óptico falha ou é interrompido | mídia perdida/incompleta        | IMAPI, progresso, bloqueio de suspensão e verificação |
| Variação de processo por jogo       | fechamento incerto              | profiles, strong identity e fixtures                  |
| Formatos locais Steam               | scanner pode quebrar            | adapter isolado + fixtures + fallback manual          |
| Metadados/artes remotos             | instabilidade/limite            | opcional, cache e provider substituível               |
| Impressoras escalando PDF           | medida incorreta                | calibração e orientação 100%                          |
| Antivírus/SmartScreen               | instalação/launch               | assinatura, reputação e comportamento mínimo          |

## 17. Definição de pronto por item

Um item só é concluído quando:

- implementação e tratamento de erro existem;
- testes automatizados relevantes passam;
- teste manual é registrado quando depende de Windows/hardware;
- logs não expõem segredos;
- documentação viva foi atualizada;
- `SESSION_HANDOFF.md` reflete o estado;
- não há TODO crítico sem issue/backlog explícito.

Operações Git não fazem parte da execução do agente. Ao final de cada fase validada, o agente apenas sugere ao desenvolvedor/revisor humano comandos de staging e commit com caminhos explícitos e mensagens Conventional Commits em inglês. O agente nunca executa esses comandos.

## 18. Sequência operacional atual

Com WP-0601 a WP-0611 implementados e validados automaticamente:

1. preservar os gates das Fases 1–6 automatizados;
2. concluir WP-0612 com screenshots multi-DPI e aceite visual do proprietário;
3. em paralelo, WP-0411 (disquete) quando o hardware estiver disponível;
4. iniciar a Fase 7 (Steam Library / Media Creator) após o aceite visual do launcher;
5. ao validar uma fase, apenas sugerir ao revisor humano os comandos Git e mensagens de commit em inglês.
