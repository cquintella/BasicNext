# Bucket 0.6.9-3 — Funcionalidades Avançadas e Estado da Arte no `bnr` (Parte 3 de 3 do REPL)

**Status:** Planned (`todo/bucket-0.6.9-3-bnr-state-of-the-art.md`)
**Repositório Alvo:** `/Users/caq/src/bn/basicnext-REPL` (`cquintella/basicnext-REPL`)
**Dependências:** [`todo/bucket-0.6.9-1-bni-session-infra.md`](file:///Users/caq/src/bn/basicnext/todo/bucket-0.6.9-1-bni-session-infra.md) e [`todo/bucket-0.6.9-2-bnr-basic.md`](file:///Users/caq/src/bn/basicnext/todo/bucket-0.6.9-2-bnr-basic.md) concluídos.
**Restrições Arquiteturais (BDFL Locks):**
* **Zero alteração em `/Users/caq/src/bn/basicnext`:** toda a implementação ocorre em `/Users/caq/src/bn/basicnext-REPL`, consumindo exclusivamente os dados e endpoints expostos pelo `bni` no Bucket 1.
* **Zero alteração na linguagem Basic Next.**

**Claim:** Elevar o `bnr` ao estado da arte de ambientes interativos de sistemas e ciência de dados, adicionando em `/Users/caq/src/bn/basicnext-REPL`: renderização rica em tabela/árvore para `BNData.DataFrame`, `BNJson`, `STRUCT` e vetores; inspetor interativo de memória ARC e handles nativos (`:arc`); inspetor de BN IR e CFG (`:ir`, `:cfg`); micro-benchmarking com auditoria de alocação ARC (`:bench`); inteligência semântica em tempo real no prompt (realce léxico, pareamento de blocos e autocompletar `Tab` guiado por tipos da sessão); consulta de documentação (`:doc`) e exportação limpa da sessão para programa compilável por `bnc` (`:export <arquivo.bn>`).

---

## Cluster A — Renderização Estruturada de Dados no Terminal (`DataFrame`, `JSON`, `STRUCT`, Vetores)

- [ ] **A.1** Implementar em `bnr` o renderizador tabular Unicode/ASCII para resultados `BNData.DataFrame` (retornados por `BNData` ou `BNSqlite.Query`), exibindo nomes de colunas, tipos Basic Next (`INTEGER`, `FLOAT`, `STRING`), alinhamento numérico/textual, truncamento automático (`head`/`tail`) para frames grandes e rodapé de dimensões (`rows × columns`).
- [ ] **A.2** Implementar a formatação estruturada em árvore/grade para valores `BNJson`, instâncias de `STRUCT`/`CLASS` e vetores `T[N]` / regiões `POINTER TO T[]`, além do histórico numerado de resultados na interface (`_1`, `_2`, `_` e cartão diagnóstico detalhado para `_err`).

---

## Cluster B — Introspecção de Compilador, Memória ARC e Micro-Benchmarking (`:arc`, `:ir`, `:cfg`, `:bench`)

- [ ] **B.1** Implementar o comando `:arc [<binding>]` exibindo o painel de memória da sessão: contagens de referências fortes/fracas, capacidade em bytes de regiões `POINTER TO T[]`, estado temporal (`ALIVE` vs `RELEASED`) e tabela de handles nativos ativos por subsistema (`BNSqlite`, `BNFfi`, `BNData`, `HOST.Fs`, `BNWeb`).
- [ ] **B.2** Implementar os comandos `:ir <fragmento>` e `:cfg <funcao>` com coloração sintática de instruções BN IR, tipos e operações de retenção/liberação ARC.
- [ ] **B.3** Implementar o comando `:bench <expressao>` exibindo estatísticas de tempo (mínimo, mediano, médio, desvio e número de amostras) junto com a auditoria de memória por iteração (alocações ARC, bytes alocados e prova de *zero-leak*).

---

## Cluster C — Inteligência Semântica no Prompt, Documentação e Exportação para `bnc` (`:doc`, `:export`)

- [ ] **C.1** Integrar no editor de linha do `bnr` o realce léxico em tempo real, indicação visual de fechamento de blocos (`IF`/`END IF`, `FUNCTION`/`END FUNCTION`) e autocompletar contextual via `Tab` (consumindo o endpoint `complete` do `bni` para sugerir métodos de objetos/módulos com assinatura de tipos).
- [ ] **C.2** Implementar o comando `:doc <Simbolo | Modulo.Membro>` formatando no terminal as assinaturas, tipos de retorno (`T OR Error`) e constantes de erro dos módulos `BN*` e `HOST`.
- [ ] **C.3** Implementar o comando `:export <arquivo.bn>` (consumindo o endpoint `export` da sessão), gerando um arquivo `.bn` limpo e canônico com `FUNCTION Start() AS VOID` e validando por testes end-to-end que uma sessão exploratória no `bnr` exportada via `:export` passa em `bni check` e compila para executável nativo em `bnc`.

---

## Cluster D — Qualidade, Documentação e Release do `basicnext-REPL`

- [ ] **D.1** Adicionar testes automatizados para todos os recursos avançados (`DataFrame` table renderer, `:arc`, `:ir`, `:cfg`, `:bench`, autocompletar `Tab`, `:doc` e `:export` → `bnc`).
- [ ] **D.2** Atualizar `README.md` e `man/bnr.1` em `/Users/caq/src/bn/basicnext-REPL`, executar `cargo fmt --check`, `cargo clippy --workspace --tests -- -D warnings` e `cargo test --workspace`, e fechar o bucket em `done/`.
