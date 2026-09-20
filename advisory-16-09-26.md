# Análise arquitetural — BasicNext 0.5.1

**Data:** 2026-09-16
**Revisão analisada:** `main` = `7cd54f2` (release 0.5.1 publicado)
**Escopo:** problemas e fragilidades reais de arquitetura; advisory de modularização,
isolamento, boas práticas Rust e de construção de compiladores.
**Método:** inspeção do código com evidência (`arquivo:linha`); nada aqui é especulação.
**Acompanhamento (2026-09-17):** status por fragilidade no título de cada seção — ✅ atendida, 🟡 parcial, sem marca = não endereçada (4, 6, 7). Detalhe em `done/bucket-0.5.1{a,b,c,d}.md`.

---

## O que está genuinamente bom (contexto)

Barreira única de validação de IR compartilhada por `run`/`build`; `unsafe_code = "deny"`
em 10 crates com opt-in por função na ABI; subsistema de diagnósticos com fatos separados
de apresentação; gates de processo reais (fmt, clippy all-targets, forbidden-deps,
inventário). Isso está acima da média e **não** deve ser sacrificado nas refatorações
abaixo.

---

## Fragilidade 1 — Isolamento de camadas violado no núcleo (a mais grave) — ✅ ATENDIDA (bucket 0.5.1c, 2026-09-17)

**Evidência:** `src/lib.rs:28` reexporta `bn_frontend::{semantic, parser, lexer, ...}`,
e o backend importa isso de volta: `src/runtime_impl.rs:62`, `src/llvm.rs:21`,
`src/runtime/compare.rs:6` etc. — **~40 arestas backend→frontend quarentenadas** em
`scripts/forbidden-deps.allowlist`. A allowlist cresce em vez de encolher (a sessão de
release 0.5.1 adicionou mais uma linha — sintoma).

**O problema real:** `bn_ir` já migrou para `bn_types::Type`
(`crates/bn_ir/src/model.rs:8` — a extração W5 aconteceu lá), mas o **interpretador e o
emissor LLVM em `src/` ainda usam o modelo de tipos do analisador semântico** via
re-export. Consequência: o "IR contract" não é um contrato — qualquer mudança no
analisador recompila e pode quebrar runtime e backend; o frontend não pode evoluir seu
modelo interno de tipos sem tocar em tudo.

**Correção:** terminar a migração. `src/runtime/**` e `src/llvm/**` importam `bn_types` +
`bn_value`, nunca `crate::semantic`. Apagar `semantic` do re-export em `lib.rs`.
Meta mensurável: **allowlist = 0 linhas**, transformando `check-forbidden-deps` em guard
de regressão de verdade. Isso destrava todo o resto.

## Fragilidade 2 — O crate raiz `bn` é um god-crate — ✅ ATENDIDA (bucket 0.5.1d fechado 2026-09-17: seam, `bn_interp`, um crate por capacidade, `--no-default-features`)

**Evidência:** 23,5k linhas em `src/` misturando interpretador, todas as HOST libs
(net/http/tls/web/dataframe/temporal/json/log), CLI, LSP, DAP e cola Jupyter. `tokio`,
`hyper`, `rustls`, `ring`, `lsp-server` são dependências do **mesmo crate** que contém o
interpretador — e o `bnc` (front-door de 545K) linka tudo isso junto.

**Consequências:** tempo de compilação (~1m22 debug), superfície de ataque desnecessária
em cada binário, impossibilidade de reusar o interpretador como biblioteca sem arrastar
rede/TLS, e testes serializados por estado global.

**Correção — grafo alvo, espelhando a filosofia HOST da própria linguagem
(capability = crate):**

```
bn_types ─ bn_source ─ bn_value ─ bn_diag        (base, como hoje)
bn_frontend → bn_ir                              (como hoje)
bn_interp        ← interpretador puro (executor, heap), sem I/O
bn_host_fs / bn_host_net / bn_host_web / bn_host_exec / bn_host_data
                 ← uma capability por crate, cada uma com core seguro
bn_rt            ← só a casca C-ABI sobre os mesmos cores
bn_cli (bin bn, bnc) / bn_lsp / bn_dap           ← consumidores finais
```

Com features no `bn_cli` (`--no-default-features` produz um `bn` só-interpretador).
Não é big-bang: extrair `bn_interp` primeiro (é o miolo), depois uma capability por vez.

## Fragilidade 3 — Paridade interpretador↔nativo por vigilância, não por construção — 🟡 PARCIAL (Exec unificado em `bn_host_exec`, 2026-09-17; demais capacidades pendentes)

**Evidência:** `HOST.Exec` existe **duas vezes**: `src/runtime/executor/part7.rs:274`
(interpretado) e `crates/bn_rt/src/exec.rs` (nativo). São implementações paralelas do
mesmo contrato — a semântica de drain/overflow/timeout é mantida idêntica **na mão**
(no fechamento do 0.5.1 o espelhamento foi manual). A política também é dupla:
`HostEnv.exec_allowed/timeout/capture_limit` vs. statics de `bn_rt::policy` + env vars,
com `BN_EXEC_POLICY` lido em **dois lugares** (`src/main.rs` e `bn_rt::policy`).

**O risco:** a matriz E01–E14 pega divergência *depois* que ela existe. Cada capability
nova dobra esse custo.

**Correção:** um core por capability (crate `bn_host_exec` da Fragilidade 2), consumido
diretamente pelo interpretador e envolvido por uma casca C-ABI fina em `bn_rt`.
A política vira um **tipo** (`ExecPolicy { allow, capture_limit, timeout }`) passado
explicitamente. Os testes de paridade continuam existindo, mas passam a verificar a
*casca*, não duas semânticas.

## Fragilidade 4 — Backend LLVM emite IR como texto

**Evidência:** `crates/bn_llvm/src/llvm/runtime.rs` tem 54 `format!` concatenando LLVM IR
textual (`"  %execrc{dest} = call i32 @bn_rt_exec_run(...)"`); nomes SSA gerenciados por
contadores interpolados em strings; 13,6k linhas nesse estilo.

**O risco:** nenhuma checagem de tipo/aridade do IR emitido em compile-time do
compilador; um typo vira erro do clang (diagnóstico péssimo, longe da causa); refactors
no runtime C-ABI exigem grep por strings.

**Correção pragmática (não reescrever em inkwell agora):** introduzir um **builder
tipado interno** — um `enum Inst`/`struct FnBuilder` que valida aridade e tipos ao
construir e implementa `Display` para o texto. Centraliza a sintaxe num lugar, dá
verificação em teste unitário, e a migração é incremental (arquivo por arquivo).
Avaliar `inkwell`/llvm-sys só quando a matriz de targets estabilizar — o builder tipado
captura 80% do valor com 20% do custo.

## Fragilidade 5 — Estado global mutável como plano de controle — 🟡 PARCIAL (interpretador sem globals, env lido uma vez no CLI; statics do `bn_rt` pendentes)

**Evidência:** 17 statics (`Atomic`/`Mutex`/`OnceLock`) em `bn_rt`; `reset_for_tests()`
e o `host_exec_test_lock()` em `tests/runtime.rs:2939` existem **porque** o estado é
global — testes serializados é o sintoma clássico. Env vars (`BN_FS_POLICY`,
`BN_EXEC_*`, `BN_DIAGNOSTICS_DIR`) como canal de política = autoridade ambiente, difícil
de auditar.

**Correção:** no nativo o C-ABI força *algum* global — mas reduzir a **um**:
`OnceLock<Policy>` instalado uma vez por `bn_rt_policy_init`; todo o resto vira
parâmetro/campo. No interpretador, zero globals: `HostEnv` já é o veículo certo, falta
os env vars serem lidos **uma vez** no CLI e traduzidos para o tipo.

## Fragilidade 6 — Modelo de valores do interpretador

**Evidência:** `bn_value::Value::Integer(i128, IntegerType)` — 16 bytes + tag para todo
inteiro, com checked-ops reimplementadas por largura no executor;
`Value::Record { fields: HashMap<String, Value> }` (`crates/bn_value/src/lib.rs:68`) —
**hash de string em cada acesso a campo**, sendo que o analisador semântico já conhece
estaticamente o layout; `Value::Error { code, message }` stringly. E `src/json.rs:4`
define um `enum Value` *paralelo* com `serde_json` já disponível no crate.

**Correção (localizada, alto retorno):**
- (a) interning de nomes de campo — `SymbolId` já existe em `bn_ir`;
- (b) records como `Box<[Value]>` com índice de campo resolvido na análise/lowering
  (acesso O(1) sem hash);
- (c) `Arc<str>` para strings imutáveis compartilhadas;
- (d) apagar o Value de `json.rs` em favor de `serde_json::Value`.

Para um interpretador de referência isso não é prematuro — é a diferença entre
"referência" e "inutilizável para programas reais".

## Fragilidade 7 — Higiene de workspace e código

- **Drift de versões:** crates em `0.4.5`/`0.5.0`, raiz em `0.5.1`. Correção:
  `[workspace.package] version = "0.5.1"` + `version.workspace = true` em todos, e
  `[workspace.dependencies]` para centralizar versões de deps externas.
- **37 ocorrências de `Result<_, String>`** como contrato público (ex.:
  `Diagnostic::structured`) — erros não tipados que o chamador só pode imprimir.
  `thiserror` nos crates de fronteira; `String` só na borda do CLI.
- **Fatiamento por volume:** `executor/part1..17.rs` (8,2k linhas), `analyzer2..8.rs`,
  `phase4.rs` — nomes sem semântica; `bn_diag` inteiro num arquivo de 2,4k linhas.
  O próprio skill do repo (`.agents/skills/rust-low-level-development`: máx. 500 linhas,
  anti-god-module) é violado em massa. Correção incremental: **renomear ao tocar**
  (`part7.rs` → `host_exec.rs`, `part17.rs` → `web_guards.rs`), nunca big-bang.

## Fragilidade 8 — Práticas de construção de compiladores — 🟡 PARCIAL (registro declarativo, fuzzing e BN_HOME em 0.5.1a; HIR, error recovery e pirâmide de testes pendentes)

- **Registro de diagnósticos hand-coded:** `DiagId` é um enum gigante com schemas em
  ~10 braços de `match` (`DETAIL_SCHEMA`), e a exaustividade registry↔catálogo é
  garantida por um script **Python em CI-time**. Correção: tabela declarativa (macro ou
  `build.rs` gerando de um manifesto) que torna o descompasso um **erro de compilação**
  do próprio compilador.
- **Falta um HIR:** o lowering vai de AST + side-tables (`models`) → IR; essas
  side-tables são exatamente as arestas semantic→backend que sobraram. Uma AST tipada
  explícita (HIR) entre análise e lowering eliminaria a última razão estrutural para a
  allowlist.
- **Error recovery no parser:** hoje o probing produz 1 diagnóstico bom (correto para
  CLI), mas para o LSP importa reportar múltiplos erros por arquivo — investir em
  sync-tokens/recovery quando o LSP virar prioridade.
- **Fuzzing ausente:** lexer/parser são o alvo ideal de `cargo-fuzz`. Para uma linguagem
  cuja tese é diagnóstico de qualidade, um panic no parser é o pior resultado possível —
  e fuzzing é barato (2 targets, corpus = `examples/` + `tests/grammar/`).
- **Pirâmide de testes parcialmente invertida:** `cli.rs` (100K) e `runtime.rs` (128K)
  testam por subprocesso comportamentos que são unitários (ex.: classificação
  expressão/statement do eval). Empurrar para baixo o que não precisa do seam.
- **Descoberta de módulos por ancestrais** (cwd e exe): conveniente, mas um `modules/bn`
  num diretório pai sequestra a resolução silenciosamente. Manter, mas logar a
  *proveniência* de cada entrada no process-log (o snapshot MP1 já existe) e considerar
  `BN_HOME` explícito como override auditável.

---

## Roadmap recomendado (ordem de ataque)

| # | Ação | Custo | Destrava |
|---|---|---|---|
| 1 | Higiene de workspace (versões, workspace-deps) | horas | baseline limpo |
| 2 | **Allowlist → 0**: migrar `src/runtime`+`src/llvm` para `bn_types`/`bn_value` | dias | Fragilidades 1, 2 |
| 3 | Extrair `bn_interp` e `bn_cli`; capabilities viram crates com features | 1–2 semanas | 2, 5 |
| 4 | Unificar exec (piloto do padrão core-compartilhado), depois demais capabilities | dias/capability | 3, 5 |
| 5 | Builder tipado no `bn_llvm` | incremental | 4 |
| 6 | Value model (interning + layout de record) | dias | 6 |
| 7 | Fuzzing + error recovery + registro declarativo de diagnósticos | incremental | 8 |

A ordem importa: **2 antes de 3** (não dá para extrair `bn_interp` limpo enquanto ele
importa `semantic`), e **4 usa o Exec como piloto** porque é a capability mais nova e
mais bem testada (E01–E14 dos dois lados) — se o padrão core-compartilhado funciona lá,
replica para net/fs/web com confiança.
