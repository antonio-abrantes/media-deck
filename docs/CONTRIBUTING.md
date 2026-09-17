# Contribuindo com o MediaDeck

## Fronteira de trabalho

A raiz Git `media-deck` é a única raiz de implementação. Não criar nem alterar arquivos do projeto na pasta pai, em pastas vizinhas ou em diretórios de apoio e backup.

## Autoridade sobre Git

Agentes não executam comandos Git. Inspeção, staging, commits, branches, merges, tags e push são responsabilidade exclusiva do desenvolvedor/revisor humano.

Depois que uma fase estiver validada, o agente entrega apenas sugestões de comandos com caminhos explícitos. O revisor confere e executa essas sugestões por conta própria.

## Commits sugeridos

O histórico segue Conventional Commits. Exemplos:

```text
feat(runtime): add sequence state
fix(media): reject unauthorized device
test(processes): cover recycled pid
docs: update session handoff
```

Use escopos curtos quando ajudarem a localizar a mudança. Breaking changes usam `!` no tipo/escopo ou o rodapé `BREAKING CHANGE:`.

As mensagens sugeridas são sempre escritas em inglês e descrevem somente o conteúdo efetivamente validado. Um exemplo de entrega ao final de fase é:

```text
git add docs/ src/ src-tauri/
git commit -m "feat(runtime): implement the validated launcher interface"
```

Essas linhas são instruções para o revisor; o agente não as executa.

## Versões

O produto segue Semantic Versioning. Enquanto a API e os fluxos ainda estiverem em formação, as versões permanecem na série `0.x`; mudanças incompatíveis devem ser descritas no changelog ou nas notas de release correspondentes.

`package.json`, `src-tauri/Cargo.toml` e `src-tauri/tauri.conf.json` devem manter a mesma versão do aplicativo.

## Gate local

Antes de integrar uma mudança, executar os comandos do gate vigente em `IMPLEMENTATION_PLAN.md` e registrar no `SESSION_HANDOFF.md` somente os resultados realmente observados.
