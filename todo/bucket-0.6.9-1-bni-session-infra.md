# Bucket 0.6.9-1 — Infraestrutura de Sessão Interativa e Introspecção em `bni` (Parte 1 de 3 do REPL)

**Status:** Planned (`todo/bucket-0.6.9-1-bni-session-infra.md`)
**Repositório Alvo:** `/Users/caq/src/bn/basicnext` (**único bucket das 3 partes que altera o repositório principal `basicnext`**)
**Restrições Arquiteturais (BDFL Locks):**
* **Zero alteração na superfície da linguagem:** `language/0.6/` (`0.6.ebnf`, `0.6.md`, `keywords.md`) não muda. `FUNCTION Start()` permanece o único ponto de entrada de programas em `bni run` e `bnc`.
* **`bni` e `bnc` continuam funcionando exatamente como hoje:** todas as flags, subcomandos e comportamentos existentes de `bni` e `bnc` são preservados.
* **Isolamento do REPL:** o executável `bnr` vive exclusivamente no repositório irmão `/Users/caq/src/bn/basicnext-REPL` e é instalado opcionalmente a partir do seu próprio repositório no GitHub.
* **Toda a base necessária no `basicnext` entra neste Bucket 1:** as Partes 2 e 3 serão implementadas exclusivamente em `/Users/caq/src/bn/basicnext-REPL` consumindo o protocolo de sessão de `bni`, sem tocar novamente em `/Users/caq/src/bn/basicnext`.

**Claim:** Habilitar em `bni eval --session --format json` (em `crates/bn_interpret_driver` e `crates/bn_frontend`) um servidor de sessão interativa persistente via protocolo JSON linha-a-linha (`stdin`/`stdout`), capaz de classificar completude sintática (`Complete`/`Incomplete`/`Invalid`), compilar/validar (`bn_ir::validate`) e executar submissões incrementais preservando bindings, declarações, imports e heap ARC entre turnos sem reexecutar histórico, e fornecer todos os endpoints de introspecção (`type`, `bindings`, `ir`, `cfg`, `arc`, `bench`, `complete`, `doc`, `export`, `policy`, `reset`) necessários para as Partes 2 e 3 do `bnr`.

---

## Arquitetura da Ponte `bni eval --session --format json`

O processo `bnr` (em `basicnext-REPL`) instancia um único subprocesso persistente `bni eval --session --format json [policy-flags]` e conversa com ele via requisições e respostas NDJSON (uma linha JSON por turno):

### Operações do Protocolo de Sessão Suportadas por `bni`
* `classify` — recebe o buffer parcial de texto e retorna `Complete`, `Incomplete` ou `Invalid`, junto com a lista de tokens/spans léxicos (para realce de sintaxe e pareamento de blocos em tempo real no terminal do `bnr`) e diagnósticos caso `Invalid`.
* `submit` — analisa, rebaixa para BN IR, executa `bn_ir::validate` e interpreta a submissão contra o ambiente vivo da sessão (`SessionState`):
* Preserva variáveis (`LET`/`CONST`), funções (`FUNCTION`), tipos (`CLASS`/`STRUCT`/`INTERFACE`/`ENUM`), módulos importados (`IMPORT`) e objetos/regiões ARC vivos entre turnos, sem reexecutar submissões anteriores.
* Garante atomicidade: se a análise falhar ou se a execução de uma inicialização falhar com erro ou pânico/aborto recuperável (incluindo `USE_AFTER_RELEASE` ou divisão por zero), o estado anterior da sessão permanece intacto, o binding falho não é publicado e o processo `bni` continua vivo para o próximo turno.
* Retorna no envelope JSON: `stdout`, `stderr`, valor resultante formatado, tipo estático resolvido, metadados estruturados quando o valor for `BNData.DataFrame` (colunas, tipos, dimensões e linhas de preview), `BNJson`, `STRUCT` ou vetor, e diagnósticos estruturados.
* `type` — resolve o tipo estático de uma expressão no contexto da sessão sem executá-la.
* `bindings` — retorna a lista de variáveis, constantes, funções, tipos e imports ativos na sessão com seus tipos e status ARC (`ALIVE` vs `RELEASED`).
* `ir` / `cfg` — retorna o texto determinístico do BN IR validado (`.bnir`) e o resumo de blocos CFG / *definite assignment* para uma submissão ou função da sessão.
* `arc` — retorna o painel de memória da sessão: contagem de objetos/regiões ARC vivos, bytes alocados, detalhes de um binding específico (`arc_id`, `strong_count`, `weak_count`, status temporal) e contagem de handles nativos ativos por subsistema (`bn_rt`).
* `bench` — executa uma expressão/chamada `N` vezes dentro do processo `bni`, medindo tempo mínimo/mediano/médio (em nanosegundos) e o delta exato de alocações/liberações ARC por iteração.
* `complete` — recebe um prefixo ou expressão parcial (` prefix ` ou `obj.`) e consulta a tabela de símbolos da sessão e dos módulos `modules/bn/*.bn`, retornando candidatos de autocompletar com assinatura e categoria (keyword, binding, função, método de classe/módulo).
* `doc` — consulta as declarações carregadas/embutidas de `modules/bn/*.bn` e retorna assinatura, métodos e constantes associadas ao símbolo consultado.
* `export` — reconstrói a partir das submissões válidas da sessão um programa Basic Next completo e canônico (`IMPORT` no topo, declarações de módulo e `FUNCTION Start() AS VOID`), pronto para `bni check` e `bnc`.
* `policy` — retorna o teto efetivo da política de execução (`bn_rt::Policy`, `sandbox`, `read-root`, `write-root`, `no-filesystem`, `module-path`, `ffi-path`).
* `reset` — fecha todos os recursos nativos dos providers (`close_all`), limpa o heap ARC e reinicia a sessão mantendo apenas a política inicial de CLI.

---

## Cluster A — Classificador de Completude e Sessão Incremental no Frontend (`crates/bn_frontend`)

- [ ] **A.1** Implementar em [`crates/bn_frontend/src/`](file:///Users/caq/src/bn/basicnext/crates/bn_frontend/src/lib.rs) o classificador `classify_submission(source: &str) -> SubmissionCompleteness` (`Complete`, `Incomplete`, `Invalid`) com spans precisos e stream de tokens para realce léxico, distinguindo blocos abertos (`IF`, `FOR`, `WHILE`, `FUNCTION`, `CLASS`, `STRUCT`, `INTERFACE`, `ENUM`, `[`, `(`) de erros sintáticos imediatos.
- [ ] **A.2** Implementar o acumulador incremental de símbolos e declarações de sessão em `bn_frontend` (permitindo submissões de topo com `IMPORT`, declarações `FUNCTION`/`CLASS`/`STRUCT`/`ENUM`, `LET`/`CONST`, atribuições, comandos e expressões soltas sem exigir que o usuário escreva `FUNCTION Start()`), preservando `bn_ir::validate` antes de qualquer execução (invariantes W1–W5).
- [ ] **A.3** Adicionar testes unitários em `crates/bn_frontend` cobrindo strings multilinha/incompletas, comentários, blocos aninhados, rebinding de variáveis de sessão e rejeição de sintaxe inválida.

---

## Cluster B — Persistência de Estado e Recuperação de Falhas no Interpretador (`crates/bn_interp`, `crates/bn_rt`)

- [ ] **B.1** Expor em [`crates/bn_interp/src/`](file:///Users/caq/src/bn/basicnext/crates/bn_interp/src/lib.rs) a API de execução incremental `InteractiveSession` que preserva a memória global/sessão, objetos ARC e estado dos providers (`CoreContext`) entre submissões sem reexecutar submissões passadas, e suporta flag atômica de cancelamento (para `Ctrl-C` durante loops).
- [ ] **B.2** Garantir atomicidade transacional por turno: se uma submissão falhar na validação ou sofrer erro em tempo de execução (incluindo falha em inicializador `LET`, divisão por zero ou violação temporal `USE_AFTER_RELEASE` capturada no modo sessão), descartar bindings parciais do turno sem encerrar o processo `bni`.
- [ ] **B.3** Expor os seletores de inspeção de memória ARC e contadores de handles nativos em `bn_interp` / `bn_rt` para alimentar os comandos `:arc` e `:bench`.

---

## Cluster C — Servidor de Sessão JSON (`bni eval --session --format json`) e Introspecção (`crates/bn_interpret_driver`)

- [ ] **C.1** Substituir o stub `--session is not available` em [`crates/bn_interpret_driver/src/eval.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_interpret_driver/src/eval.rs#L52-L54) pelo loop de sessão NDJSON `run_interactive_session`, suportando as ações `classify`, `submit`, `type`, `bindings`, `policy`, `reset` e `quit`.
- [ ] **C.2** Implementar no servidor de sessão os endpoints avançados de introspecção para o Bucket 3: `ir`, `cfg`, `arc`, `bench`, `complete`, `doc`, `export` e serialização estruturada de `BNData.DataFrame` / `BNJson` / `STRUCT` / vetores nos resultados de `submit`.
- [ ] **C.3** Atualizar `--help` e `man/bni.1` se necessário (mantendo a paridade de opções verificada por `tests/programs/parity.rs`).

---

## Cluster D — Verificação nas 5 Camadas e Fechamento (`tests/`, `scripts/`)

- [ ] **D.1** Adicionar suíte de testes de integração em memória e via subprocesso para `bni eval --session --format json` cobrindo: persistência de variáveis/funções/classes/imports entre turnos, atomicidade após erro estático e de execução, inversão temporal ARC entre turnos (`RELEASE` no turno 1, acesso no turno 2 reportando `USE_AFTER_RELEASE` sem derrubar a sessão), `:type`, `:ir`, `:arc`, `:bench`, `:complete`, `:export` (validando que o código exportado compila em `bnc`) e `:reset`.
- [ ] **D.2** Executar `./scripts/test-battery.sh --gates` e fechar o bucket em `done/bucket-0.6.9-1-bni-session-infra.md`.
