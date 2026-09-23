# RQ Candidata: Validação de Alinhamento IA-Especificação

**Data:** 2026-09-18  
**Status:** Candidata para desenvolvimento

---

## Pergunta de Pesquisa

**Como validar que outputs de IA generativa estão alinhados com especificações normativas de engenharia de software?**

### Subperguntas

1. Quais critérios definem "alinhado com spec"?
2. Como detectar divergência não-óbvia (código que compila mas viola invariante de design)?
3. Pode um protocolo de validação (fixtures, gates, audit) fazer isto reproduzível e escalável?
4. Qual é a taxa de falso-negativo de tal validação?
5. O framework é generalizável além de BasicNext?

---

## Por que é relevante

- **Problema real:** Desenvolvimento de linguagens/toolchains com IA requer garantias de conformidade a especificação normativa
- **Gap de pesquisa:** Não existe metodologia formal para validar alinhamento IA-spec
- **Escala crescente:** À medida que IA participa de decisões de design, o risco de divergência silenciosa aumenta
- **Generalizável:** Aplica a projetos além de linguagens (arquitetura de software, safety-critical systems)

---

## Artefatos em BasicNext

### Especificação Normativa
- `AGENTS.md` — brief autoritário para agentes e contribuidores
- `language/0.5/0.5.ebnf`, `0.5.md` — gramática e semântica
- `docs/architecture/` — decisões arquiteturais (IR contract, conformance, support matrix)
- `docs/governance.md` — autoridade final (Carlos como BDFL)

### Critérios de Validação (Existentes)
- `completion-gates.md` — GC-IR, GC-SUP, GC-DEP (e outros)
- W1–W5 — well-formed IR requirements
- `docs/architecture/ir-contract.md` — handoff contract
- Buckets + WBS — "o que foi pedido" com acceptance criteria

### Histórico Empírico
- Commits e PRs (alinhados vs. desalinhados com spec)
- Revisões de Carlos (padrão de correções, divergências detectadas)
- Meu histórico como IA (claude-haiku-4-5, claude-opus-5)
- `test_compiler_parity.py`, negatives fixtures (exemplos de validação em prática)

---

## Possível Design de Pesquisa (DSR)

### Fase 1: Formalização
- Definir "alinhamento" operacionalmente (ex: conformidade a EBNF, respeito a gates, aderência a bucket scope)
- Inventariar desvios históricos (tipos, causas, detecção tardia)
- Propor critérios explícitos (checklist, fixtures, regras de audit)

### Fase 2: Protocolo de Validação
- Desenhar protocolo de validação (o que testar, em que ordem, com que profundidade)
- Implementar ferramental de audit (scripts, bots, CI hooks)
- Aplicar a BasicNext: validar commits/PRs retroativamente

### Fase 3: Avaliação Empírica
- Medir taxa de detecção e falso-negativo
- Quantificar: quantos desvios foram detectados pelo protocolo?
- Análise de custo: overhead de validação vs. benefício

### Fase 4: Generalização
- Testar protocolo em outro projeto/linguagem (prova de conceito)
- Documentar padrões transferíveis
- Propor framework genérico

---

## Contribuições Esperadas

1. **Framework de Validação IA-to-Spec** — critérios, protocolo, tooling
2. **Catálogo de Desvios** — tipologia de erros, raízes comuns, sinais de alerta
3. **Evidência Empírica** — taxa de sucesso/falha em BasicNext
4. **Padrão de Audit** — reproduzível, escalável, generalizável

---

## Próximos Passos

- [ ] Refinar pergunta com Carlos
- [ ] Inventariar desvios históricos em BasicNext
- [ ] Definir operacionalmente "alinhado"
- [ ] Desenhar protocolo piloto
- [ ] Validar em 3–5 commits/PRs reais
