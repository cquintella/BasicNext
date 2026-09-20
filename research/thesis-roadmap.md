# Roadmap de Tese: Duas RQs, Uma Narrativa

**Timeline:** 4 anos (até defesa)  
**RQs:** 2 candidatas (IA-spec alignment + cognitive load)  
**Artefato central:** BasicNext

---

## Conexão entre RQs

```
RQ1: Como validar alinhamento IA-Especificação?
  └─> Metodologia de audit, protocol de validação
      └─> Aplicado em BasicNext durante desenvolvimento

RQ2: Como a carga cognitiva de BasicNext compara com Python?
  └─> Estudo UX/empírico com 6 participants
      └─> Valida hipótese de design que guiou BasicNext
          └─> Que hipótese? A de que "especificação normativa
              + tipagem explícita = menor carga cognitiva"
```

**Insight:** RQ1 garante que o artefato (BasicNext) foi desenvolvido com rigor (aligned com spec). RQ2 valida que o design dessa spec funciona empiricamente (lower cognitive load).

---

## Arquiteturas de Tese Possíveis

### Opção A: "Two-Study Dissertation" (Recomendado)
- **Capítulo 1–3:** Fundamentação + related work (IA, DSR, cognitive load, language design)
- **Capítulo 4:** RQ1 — Framework de validação IA-Spec (metodológico)
  - Formalizar critérios de alinhamento
  - Protocolo de audit (aplicado retroativamente em 15–20 commits reais de BasicNext)
  - Evidência: taxa de detecção, false negatives, custo computacional
- **Capítulo 5:** RQ2 — Cognitive Load Study (empírico)
  - Design de estudo (6 participants, tasks, métricas)
  - Resultados: NASA-TLX, SUS, performance, qualitative insights
- **Capítulo 6:** Síntese — "Designing and Validating Low-Cognitive-Load Languages via AI-Assisted Development"
  - Como RQ1 + RQ2 se reforçam mutuamente
  - Implicações para language design + AI-assisted software engineering
  - Limitations, future work

**Tamanho:** ~150–180 páginas (típico)  
**Força:** narrativa coerente, duas contribuições complementares, publicável como 2 papers

---

### Opção B: "RQ1-Focused" (Mais Deep)
- Aprofundar validação IA-Spec em 50+ commits/PRs
- RQ2 como "motivation + pilot study" (menor escala, ilustrativo)
- Mais contribution teórica em framework de audit

**Quando:** se o gap de "como validar IA outputs" virar obsessão de pesquisa

---

### Opção C: "RQ2-Focused" (Mais Empírico)
- Cognitive load como centerpiece
- RQ1 como "how we ensured rigor in our design artifact"
- Talvez 12–15 participants (não 6) para maior power estatístico

**Quando:** se quer fazer contribuição em HCI/UX de linguagens

---

## Refinamento de RQ1 (4 meses)

### Fase 1: Formalização (Mês 1–2)
- [ ] Inventariar desvios históricos em BasicNext (10–15 casos)
  - O que deu errado? (bug semanticamente, violou gate, divergiu de spec)
  - Quando foi detectado? (imediatamente vs tarde)
  - Como foi detectado? (human review, test failure, later bug report)
- [ ] Categorizar tipos de desvio
  - Typ 1: "IA ignorou AGENTS.md"
  - Typ 2: "IA seguiu old spec (0.4) vs new (0.5)"
  - Typ 3: "IA cobriu casos, mas lost edge case"
  - ...
- [ ] Propor **critérios operacionais** de alinhamento
  - Ex: "commit deve estar dentro scope de bucket X"
  - Ex: "código não deve quebrar gates GC-*, W1–W5"
  - Ex: "aderência a `docs/language/0.5/` (não ad-hoc)"

### Fase 2: Protocolo (Mês 2–3)
- [ ] Desenhar **checklist de audit**
  - Mecânico: EBNF conformance, gate checks, imports
  - Semântico: alinhamento com AGENTS.md, bucket scope, normative intent
  - Arquitetural: dependency rules (IR ≠ depender frontend, etc)
- [ ] Implementar ferramental
  - Scripts que rodam em CI: grep rules, AST checks, spec violations
  - Manual review protocol: o que human deve verificar, em que ordem
- [ ] Definir **severity levels**
  - Critical: quebra spec
  - Major: viola gate
  - Minor: estilo, documentation

### Fase 3: Validação Retroativa (Mês 3–4)
- [ ] Aplica protocolo em 15–20 commits reais
  - Já foram merged? Sim — então estavam "aligned enough"
  - Mas o protocolo detecta problemas latentes? (false negatives)
  - Quanto trabalho de audit por commit? (custo)
- [ ] Análise: qual % de problemas seria detectado pre-merge?
  - Se 100%: protocol é completo (high value)
  - Se 60%: há gaps (quais?)
  - Se 20%: protocol é muito rígido (false positives)

---

## Refinamento de RQ2 (6 meses)

### Fase 1: Design (Mês 1–2)
- [ ] Finalizar tasks (pair programming com 1 dev, iterate)
  - Task 1–4 devem ser **isomórficas** (mesma lógica em ambas linguagens)
  - Timing deve ser realistic (não muito fácil/hard)
- [ ] Preparar estímulos (código de exemplo)
  - Beginner-friendly BasicNext code (com comentários)
  - Equivalent Python code
- [ ] Setup environment
  - Online playground? (rápido, sem setup friction)
  - Local dev environment? (mais realista, mais setup)
  - Proposta: hybrid — browser-based BasicNext playground + local Python

### Fase 2: Piloto (Mês 2–3)
- [ ] Recrutar 1–2 participants
- [ ] Rodar sessions completas
  - Timing realistic?
  - Tasks claras?
  - Metrics coletáveis?
  - NASA-TLX faz sentido?
- [ ] Refinar protocol baseado em feedback

### Fase 3: Full Study (Mês 3–5)
- [ ] Recrutar 6 participants
- [ ] Rodar 6 sessions (~60 min cada = 6 horas data collection)
- [ ] Transcrever think-aloud protocols

### Fase 4: Analysis (Mês 5–6)
- [ ] Quantitativa: t-tests, NASA-TLX por dimensão, correlations
- [ ] Qualitativa: coding de think-aloud (thematic analysis)
- [ ] Escrita de findings

---

## Artefatos BasicNext que Precisa Desenvolver

### Para RQ1
- [ ] AGENTS.md refinado (normativo, unambíguo)
- [ ] Checklist de audit (em `research/`)
- [ ] Scripts de validação (em `scripts/validate-spec-alignment.sh` ou similar)
- [ ] Histórico de desvios catalogado

### Para RQ2
- [ ] Tutorial de BasicNext (5–10 min, beginner-friendly)
- [ ] Online playground (browser REPL)
- [ ] 4 conjuntos de tasks (code examples, pré-escrito)
- [ ] Protocol document (study design, consent, debriefing)

---

## Timeline Comprimido (4 anos)

```
Year 1 (Mês 1–12):
  - Refinar RQ1 + RQ2 (mês 1–2) ✓ (agora)
  - Desenvolver RQ1 (mês 2–5)
  - Desenhar + pilotar RQ2 (mês 3–7)
  - Primeira escrita (capítulo 4) (mês 6–10)
  - Publicar paper sobre RQ1 (mês 9–12)

Year 2 (Mês 13–24):
  - Full RQ2 study (mês 1–3)
  - Analysis + escrita RQ2 (mês 4–8)
  - Síntese de tese (mês 8–12)
  - Publicar paper sobre RQ2 (mês 10–12)

Year 3–4:
  - Escrita final de tese
  - Defesa
```

---

## Próximos Passos Imediatos

1. **Escolher arquitetura:** Opção A (two-study) recomendada
2. **Refinar RQ1:** Inventariar 10–15 desvios históricos reais
3. **Refinar RQ2:** Desenhar tasks com 1 dev piloto
4. **Criar `research/protocol-rq1.md`** e **`research/protocol-rq2.md`**
5. **Definir: quando começa data collection?** (RQ2 empírico exige recruitment)

---

## Publicabilidade

**Esperado:**
- 1 paper sobre RQ1 (validation framework) — venue: ICSE/ASE/FSE ou journal
- 1 paper sobre RQ2 (cognitive load) — venue: ICSE/CHI/LangDev workshop
- Capítulos 4–5 da tese redigem como papers
- Capítulo 6 (synthesis) é contribution única da tese
