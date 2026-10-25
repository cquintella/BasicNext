# Bucket 0.7.0 — Declarações Aninhadas (Funções e Classes Locais) e REPL Padrão Swift

**Status:** Planned (`todo/bucket-0.7.0-nested-declarations-and-swift-repl.md`)
**Repositório Alvo:** `/Users/caq/src/bn/basicnext`
**Decisão BDFL:** Carlos Quintella (@cquintella) em 2026-10-25: mudança significativa na gramática e semântica da linguagem com transição de versão para **Basic Next 0.7.0**. Adoção integral do modelo do Swift para declarações aninhadas e comportamento de execução do REPL.
**Dependências:** Série 0.6.9 concluída.

**Restrições Arquiteturais (BDFL Locks):**
* **Paridade Estrita entre Interpretador e Compilador:** todo programa aceito por `bni` com funções e classes aninhadas compila e produz o mesmo resultado determinístico em `bnc` (`Native::Same`), conforme `docs/architecture/conformance.md`.
* **Zero Quebra de Invariantes W1–W5:** as declarações aninhadas são normalizadas no frontend via rebaixamento canônico determinístico (desaninhamento / lambda lifting com qualificação léxica de símbolos), preservando o modelo do BN IR e sem tipos de frontend no IR (`GC-DEP`).
* **Especificação Normativa 0.7:** abertura da árvore normativa `language/0.7/` (`0.7.ebnf`, `0.7.md`, `keywords.md`), mantendo `crates/bn_frontend/src/keyword_registry.rs` apontando para a versão ativa da especificação.
* **Clean Code & Linhas:** sem números mágicos nem strings literais expostas (mensagens catalogadas em `share/bn/catalog/`), respeitando o orçamento de linhas (`tests/source_size.rs`).

**Claim:**
1. **Linguagem (Padrão Swift — 0.7.0):**
   * Permitir a declaração de funções locais dentro de funções (`nested functions`).
   * Permitir a declaração de classes e structs locais dentro de funções (`local types`).
   * Permitir a declaração de classes e structs dentro de classes e structs (`nested types`, acessíveis via qualificação `Outer.Inner`).
2. **REPL (Padrão Swift — 0.7.0):**
   * O REPL `bnr` opera estritamente no escopo de módulo (`Top-Level Module Scope`), idêntico ao Swift REPL.
   * O ponto de entrada de despacho sintético de cada turno é desacoplado do nome do usuário (`__bnr_eval` / `__session_entry`), permitindo que o usuário declare e execute `FUNCTION Start()` livremente sem qualquer recursão ou `STACK_OVERFLOW`.

---

## Verificação dos Recursos no Modelo Swift

* **Função dentro de função:** Suportado no Swift (`nested functions`). Escopo léxico estrito visível no corpo da função mãe.
* **Classe / Struct dentro de função:** Suportado no Swift (`local types`). Tipo utilitário com escopo restrito à função mãe.
* **Classe / Struct dentro de classe:** Suportado no Swift (`nested types`). Tipo aninhado nomeado, instanciável e referenciável por escopo qualificado (`Outer.Inner`).

---

## Cluster A — Especificação Normativa da Linguagem 0.7

- [ ] **A.1** Criar a árvore normativa `language/0.7/` a partir de `language/0.6/`, atualizando `SPEC_KEYWORDS` e `SPEC_EBNF` em [`crates/bn_frontend/src/keyword_registry.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_frontend/src/keyword_registry.rs).
- [ ] **A.2** Atualizar a gramática normativa `language/0.7/0.7.ebnf`:
  * Permitir `function-declaration`, `class-declaration` e `struct-declaration` como opções válidas dentro de `statement` (para funções e tipos locais).
  * Permitir `class-declaration` e `struct-declaration` dentro de `class-body` e `struct-body` (para tipos aninhados).
- [ ] **A.3** Atualizar o documento semântico normativo `language/0.7/0.7.md`:
  * Especificar as regras de visibilidade e resolução de tipos locais e funções locais.
  * Especificar as regras de acesso e qualificação para tipos aninhados (`Outer.Inner`).
  * Manter `keywords.md` em conformidade.

---

## Cluster B — Frontend e AST (Parser & Resolução Semântica)

- [ ] **B.1** Ajustar o parser recursivo em [`crates/bn_frontend/src/parser/`](file:///Users/caq/src/bn/basicnext/crates/bn_frontend/src/parser/):
  * Permitir que o parser de instruções (`statement`) reconheça e construa nós de AST para declarações locais de funções, classes e structs.
  * Permitir que o parser de classes e structs reconheça declarações de classes/structs aninhadas no corpo do tipo.
- [ ] **B.2** Análise Semântica e Escopos em [`crates/bn_frontend/src/semantic/`](file:///Users/caq/src/bn/basicnext/crates/bn_frontend/src/semantic/):
  * Tabela de símbolos: suportar pilha de escopos léxicos aninhados.
  * Resolução de nomes qualificados: permitir `Outer.Inner` para instanciação e tipagem (`LET x AS Outer.Inner = NEW Outer.Inner()`).
  * Atribuição definitiva e checagem de tipos semântica para declarações internas.

---

## Cluster C — Rebaixamento (Lowering) e Paridade nos Backends (`bni` & `bnc`)

- [ ] **C.1** Rebaixamento Determinístico em [`crates/bn_frontend/src/lowering/`](file:///Users/caq/src/bn/basicnext/crates/bn_frontend/src/lowering/):
  * Mapear funções locais e métodos de tipos aninhados em funções IR (`Function`) com nomes qualificados canônicos estáveis (ex: `Outer$Inner$Method` e `EnclosingFunc$LocalFunc`).
  * Garantir ordenação estável por chave única para preservar determinismo de compilação.
- [ ] **C.2** Validação do IR ([`crates/bn_ir/src/validate/`](file:///Users/caq/src/bn/basicnext/crates/bn_ir/src/validate/)):
  * Garantir que todas as chamadas e acessos a tipos locais/aninhados passem pelos contratos invariantes W1–W5.
- [ ] **C.3** Execução no Interpretador [`crates/bn_interp`](file:///Users/caq/src/bn/basicnext/crates/bn_interp):
  * Suportar resolução e invocação das funções locais e instanciação de classes locais/aninhadas no executor do `bni`.
- [ ] **C.4** Emissão LLVM no Compilador Nativo [`crates/bn_llvm`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm):
  * Emissão de layouts e chamadas diretas para tipos aninhados e funções locais com exportação C ABI conforme contrato de runtime nativo.

---

## Cluster D — REPL Padrão Swift (`bnr` & `session_accumulator`)

- [ ] **D.1** Desacoplamento do Ponto de Entrada da Sessão:
  * Em [`crates/bn_frontend/src/session_accumulator.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_frontend/src/session_accumulator.rs), substituir o invólucro rígido `FUNCTION Start() AS VOID` por uma função interna sintética de despacho (ex: `FUNCTION __session_entry() AS VOID`).
  * No rebaixamento (`lowering`), marcar `__session_entry` como `FunctionKind::Entry` quando presente.
- [ ] **D.2** Suporte Livre a `FUNCTION Start()` no REPL:
  * Permitir que o usuário declare `FUNCTION Start(...)` sem colidir com o invólucro do REPL.
  * Se o usuário digitar `Start()` no prompt do REPL, o despachante sintético do turno (`__session_entry`) invoca o `Start()` do usuário sem recursão nem estouro de pilha (`STACK_OVERFLOW`).
- [ ] **D.3** Avaliação de Top-Level Code no REPL:
  * Garantir que variáveis globais declaradas no REPL permaneçam acessíveis por funções e classes declaradas em turnos posteriores.

---

## Cluster E — Versão 0.7.0, Bateria de Testes, Qualidade e Fechamento

- [ ] **E.1** Atualização de versão no workspace Cargo (`Cargo.toml`, man pages `man/bni.1`, `man/bnc.1`, banners de CLI).
- [ ] **E.2** Testes Unitários de Linguagem:
  * Função dentro de função (chamada direta, recursão local, múltiplos níveis).
  * Classe dentro de função (instanciação local, métodos, ciclo de vida com ARC).
  * Classe/struct dentro de classe/struct (acesso qualificado `Outer.Inner`, instanciação mútua).
- [ ] **E.3** Testes de Paridade `bni` × `bnc` em [`tests/programs/parity.rs`](file:///Users/caq/src/bn/basicnext/tests/programs/parity.rs) sob conformidade `Native::Same`.
- [ ] **E.4** Testes de Integração do `bnr` cobrindo definição de `FUNCTION start() AS VOID` e chamada interativa `Start()`.
- [ ] **E.5** Execução completa de `./scripts/test-battery.sh --gates` garantindo 100% de sucesso nos 34 quality gates.
- [ ] **E.6** Atualização de `docs/releases/0.7.0.md` e movimentação do bucket para `done/`.
