# Bucket 0.6.9-2 — REPL Básico `bnr` em Repositório Autônomo (Parte 2 de 3 do REPL)

**Status:** Planned (`todo/bucket-0.6.9-2-bnr-basic.md`)
**Repositório Alvo:** `/Users/caq/src/bn/basicnext-REPL` (`cquintella/basicnext-REPL`)
**Dependência:** [`todo/bucket-0.6.9-1-bni-session-infra.md`](file:///Users/caq/src/bn/basicnext/todo/bucket-0.6.9-1-bni-session-infra.md) concluído e instalado.
**Restrições Arquiteturais (BDFL Locks):**
* **Zero alteração em `/Users/caq/src/bn/basicnext`:** este bucket opera exclusivamente em `/Users/caq/src/bn/basicnext-REPL`.
* **Desacoplamento total de código-fonte:** `bnr` não possui dependência Cargo `path` ou `git` para crates internos de `basicnext`; comunica-se exclusivamente com o binário `bni` instalado no sistema (`PATH` ou `BNI_PATH`) via protocolo `bni eval --session --format json`.
* **Instalação opcional e independente:** `basicnext-REPL` possui seu próprio `install.sh` / `install.ps1` e workflow de release no GitHub para instalar `/usr/local/bin/bnr`.

**Claim:** Implementar em `/Users/caq/src/bn/basicnext-REPL` o executável `bnr` e sua biblioteca cliente headless (`bnr_client`), entregando o shell interativo básico completo: descoberta e gerenciamento do processo `bni`, coleta multilinha guiada por `classify` (`bnr> ` / `...> `), edição de linha com histórico persistente, tratamento seguro de `Ctrl-C` / `Ctrl-D`, repasse de flags de política (`--sandbox`, `--read-root`, `--write-root`, `--no-filesystem`, `--module-path`, `--ffi-path`), e os meta-comandos fundamentais (`:help`, `:quit`, `:reset`, `:bindings`, `:type`, `:load`, `:policy`, `:clear`).

---

## Cluster A — Fundação do Workspace `basicnext-REPL` e Cliente de Sessão Headless

- [ ] **A.1** Estruturar o Cargo workspace em `/Users/caq/src/bn/basicnext-REPL` (`crates/bnr_client` para o motor de protocolo JSON sem TTY e `crates/bnr` para o binário de terminal), com `Cargo.toml`, licença MPL-2.0 e configuração de lints estrita (`clippy -D warnings`).
- [ ] **A.2** Implementar em `bnr_client` o gerenciador do subprocesso `bni eval --session --format json`, incluindo verificação de versão compatível de `bni` (>= 0.6.8), serialização/desserialização tipada de requisições e respostas NDJSON, e máquina de estados (`Ready`, `Collecting`, `Executing`, `Invalidated`).
- [ ] **A.3** Implementar o repasse transparente de opções de CLI do `bnr` para o subprocesso `bni` (`--module-path`, `--ffi-path`, `--sandbox`, `--read-root`, `--write-root`, `--no-filesystem`, `--config`, `-V`, `-h`), exibindo no banner de abertura a versão do `bnr`, a versão do `bni` conectado e o resumo da política efetiva.

---

## Cluster B — Interface de Terminal (TTY), Prompt Multilinha e Sinais

- [ ] **B.1** Implementar o loop interativo de terminal com suporte a edição de linha, navegação por setas, busca no histórico (`Ctrl-R`) e persistência de histórico em disco (`~/.bn_history` ou `BNR_HISTORY_FILE`).
- [ ] **B.2** Integrar o prompt adaptativo (`bnr> ` no estado `Ready` e `...> ` no estado `Collecting`) consultando a ação `classify` de `bni` a cada quebra de linha: submissões `Incomplete` acumulam no buffer; submissões `Invalid` exibem o diagnóstico imediato e voltam para `Ready`; submissões `Complete` disparam `submit`.
- [ ] **B.3** Implementar o tratamento determinístico de sinais:
* `Ctrl-C` durante `Collecting`: descarta o buffer multilinha parcial e retorna a `bnr> `.
* `Ctrl-C` durante `Executing`: envia sinal de interrupção da submissão atual ao `bni` sem encerrar a sessão nem perder os bindings anteriores.
* `Ctrl-D` (EOF) em linha vazia: executa shutdown limpo da sessão e encerra com código `0`.

---

## Cluster C — Meta-Comandos Básicos e Carregamento de Arquivos

- [ ] **C.1** Implementar os meta-comandos fundamentais:
* `:help` (resumo dos comandos e atalhos)
* `:quit` / `:q` (encerramento limpo)
* `:reset` (limpa estado da sessão no `bni` mantendo a política)
* `:bindings` / `:vars` (lista variáveis, constantes, funções, classes e imports ativos)
* `:type <expr>` (exibe o tipo estático resolvido pelo `bni` sem executar `<expr>`)
* `:policy` (exibe a política de segurança ativa da sessão)
* `:clear` (limpa a tela do terminal)
- [ ] **C.2** Implementar `:load <arquivo.bn>` lendo o arquivo do disco e submetendo suas declarações/imports para a sessão ativa do `bni` com mapeamento correto de nome de arquivo e linhas nos diagnósticos.

---

## Cluster D — Instaladores, CI e Testes End-to-End do `basicnext-REPL`

- [ ] **D.1** Criar suíte de testes automatizados em `/Users/caq/src/bn/basicnext-REPL/tests/` exercitando `bnr_client` e o binário `bnr` contra o `bni` instalado: sessões multi-turno, blocos multilinha (`IF`, `FOR`, `FUNCTION`, `CLASS`), preservação de estado após erros sintáticos/semânticos/runtime, `:type`, `:bindings`, `:load`, `:reset` e interrupção `Ctrl-C`.
- [ ] **D.2** Criar `man/bnr.1`, `scripts/install.sh`, `scripts/install.ps1` e `.github/workflows/ci.yml` em `/Users/caq/src/bn/basicnext-REPL` para instalação opcional independente a partir do GitHub.
- [ ] **D.3** Validar `cargo fmt --check`, `cargo clippy --workspace --tests -- -D warnings` e `cargo test --workspace` em `/Users/caq/src/bn/basicnext-REPL` e mover o bucket para `done/`.
