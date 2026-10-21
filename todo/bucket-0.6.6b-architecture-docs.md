# Bucket 0.6.6b — Architecture Documentation Overhaul (0.6)

**Status:** Planned (todo). Not started.
**Origem:** Decisão BDFL (2026-10-20), desmembrado de `done/bucket-0.6.5-remainings.md` (antigos itens `D.2.a`–`D.2.d` e `D.C.6`).
**Inicia após:** Conclusão de `ongoing/bucket-0.6.6-test-system.md` (reestruturação do sistema de testes, oráculos e consolidação).
**Precede:** `todo/bucket-0.6.7.md` (BNFfi).
**Tag:** `docs/releases/0.6.6b.md` on close (documentação e alinhamento arquitetural da linhagem 0.6; sem alteração normativa da linguagem).

---

## 1. Contexto e Diagnóstico

A documentação arquitetural sob `bn/docs/architecture/` (26 documentos) encontra-se defasada em relação ao estado real do compilador e do runtime da linhagem 0.6:
- Cerca de 20 documentos não são revisados substancialmente desde 0.4 / 0.6.2.
- Permanecem pelo menos 16 menções a comandos obsoletos da ferramenta unificada antiga (`bn run` e `bn build`), substituída em 0.6.0 pelos executáveis dedicados `bni` e `bnc`.
- Não refletem a introdução do pipeline com emissor LLVM tipado (`bn_llvm`), `bnc -g`, o modelo de cabeçalho de 16 bytes na `STRING` (Stage 1), os layouts de alternativas consolidados, a extração de bibliotecas (`libraries/bn_web`) e a governança de módulos opcionais (`bnstdlib`).

---

## 2. Atividades e Roteiro de Execução

### Fase 1 — Inventário Completo e Mapeamento de Lacunas
- [ ] **A.1** Mapeamento arquivo por arquivo dos 26 documentos em `bn/docs/architecture/`, confrontando cada asserção contra a implementação atual em `crates/` e a especificação normativa `language/0.6/`.
- [ ] **A.2** Eliminação sistemática de terminologia obsoleta: substituir todas as ocorrências de `bn run` por `bni run`, e de `bn build` por `bnc` nos 9 documentos afetados (`host-traits.md`, `ci-toolchain-notes.md`, `conformance.md`, `glossary.md`, `target-architecture.md`, `library-first-drivers.md`, `native-stdlib-binding.md`, `to-close.md`, `basic-next-architecture-report.md`).

### Fase 2 — Atualização dos Documentos Estruturais do Pipeline
- [ ] **B.1** Atualizar `target-architecture.md` e `driver-sequence.md`: descrever os drivers baseados em biblioteca (`bn_compile_driver`, `bn_interpret_driver`, `bn_cli`) e a separação estrita `bni` x `bnc`.
- [ ] **B.2** Atualizar `ir-contract.md`: formalizar o contrato pós-emissor tipado, modelo de definite assignment, W1–W5 e validação de invariantes.
- [ ] **B.3** Atualizar `conformance.md`: registrar o fim dos testes legados em Python, a bateria em Rust e as matrizes de paridade backend (`bni` x `bnc`).
- [ ] **B.4** Atualizar `host-traits.md` e `native-stdlib-binding.md`: registrar a separação de bibliotecas opcionais em `libraries/`, o padrão Provider/Adapter e a ausência de acoplamento com stacks de terceiros em `bn_rt`.

### Fase 3 — ABI, Layout de Memória e Referências Cruzadas
- [ ] **C.1** Sincronização definitiva de `docs/architecture/value-memory-abi.md`: atualizar as seções de "Alternative values" e propriedade explícita (ARC) para o estado estritamente implementado em 0.6.
- [ ] **C.2** Auditoria de links e referências cruzadas: validar todos os ponteiros e caminhos em `bn/AGENTS.md`, `bn/README.md`, páginas de manual (`man/bni.1`, `man/bnc.1`) e no tutorial book (`bn/docs/book/`).
- [ ] **C.3** Verificação automática: rodar `tests/compiler_capabilities.rs` para assegurar que nenhuma alteração textual quebrou a verificação da ABI.
