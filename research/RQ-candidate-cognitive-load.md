# RQ Candidata: Carga Cognitiva e Usabilidade de BasicNext

**Data:** 2026-09-18  
**Status:** Candidata para desenvolvimento
**Tamanho amostral:** 6 participants (metodologia UX qualitativa)

---

## Pergunta de Pesquisa

**Como a carga cognitiva de BasicNext (tipagem explícita, IR normalizado, HOST determinístico) compara com Python em tasks de compreensão, manutenção e revisão de código?**

### Subperguntas

1. Programadores percebem menor carga cognitiva em BasicNext vs Python para tasks típicas?
2. Há diferença significativa em tempo de compreensão de código escrito em ambas?
3. Taxas de erro (lógica, tipo, design) diferem entre linguagens?
4. A revisão de código é mais eficiente/confiável em uma linguagem vs outra?
5. Quais features de BasicNext contributêm positivamente para reduzir carga?

---

## Fundamentação

### Hipótese de Design
BasicNext foi intencionalmente desenhada com:
- **Tipagem explícita** — menos surpresas, mais clareza
- **IR bem-formado** — sintaxe não-ambígua, erros detectáveis
- **HOST determinístico** — comportamento previsível em system calls
- **Sintaxe próxima a C/Rust** — menos "magic"

**Expectativa:** menor carga cognitiva em compreensão e manutenção.

**Gap:** Nenhuma validação empírica exists.

---

## Design de Estudo (Piloto)

### Participants
- **N = 6** (metodologia UX qualitativa, Nielsen et al.)
- **Perfil:** desenvolvedores com experiência em Python e disposição de aprender BasicNext
- **Recrutamento:** comunidade local, academic contacts, ou open call

### Tasks (Balanced)

#### Task 1: Implementação (20–30 min)
- **Python:** escrever função que processa lista + filtra por condição
- **BasicNext:** mesma função em BasicNext
- **Medidas:** tempo, erros de sintaxe/lógica, número de compilações/runs

#### Task 2: Compreensão (10–15 min)
- Ler código pré-escrito (média ~15 linhas)
- Responder 5 perguntas sobre comportamento/output
- **Medidas:** tempo, acurácia, confiança subjetiva

#### Task 3: Revisão + Debug (15–20 min)
- Ler código com 1–2 bugs intencionais
- Identificar e descrever problema
- **Medidas:** detecção correta, tempo, justificativa da raiz

#### Task 4: Manutenção (10 min)
- Adicionar feature pequena a código existente
- **Medidas:** tempo, erros introduzidos

### Métricas

#### Objetivas
- **Tempo de task**
- **Taxa de erro** (compilação, lógica, tipo)
- **Acurácia de compreensão** (% perguntas corretas)
- **Taxa de detecção de bugs**

#### Subjetivas
- **NASA-TLX** (6 dimensões: mental demand, physical, temporal, performance, effort, frustration) — after each task
- **SUS (System Usability Scale)** — after language pair
- **Think-aloud protocol** — durante tasks, transcrito para análise qualitativa

---

## Artefatos BasicNext Necessários

- [ ] Tutorial/quickstart de 5 min (para participants pré-estudarem)
- [ ] 3–4 exemplos idênticos (Python + BasicNext) como warm-up
- [ ] Código pré-escrito (compreensão + debug) em ambas linguagens
- [ ] Environment setup (playground online ou local — sem friction)

---

## Análise

### Quantitativa
- Comparação de médias (t-test ou Mann-Whitney, N=6)
- Análise de NASA-TLX por dimensão
- Correlação: carga ↔ performance

### Qualitativa
- Coding de think-aloud: quais padrões de raciocínio diferem?
- Quais features de BasicNext ajudaram/prejudicaram?
- Percepção de "vibe" (intuitividade, naturalidade)

---

## Contribuições Esperadas

1. **Evidência empírica** de diferença de carga cognitiva (ou lack thereof)
2. **Feature analysis** — quais aspectos de BasicNext funcionam
3. **UX insights** — surpresas, affordances, pain points
4. **Padrão para estudos de linguagem** — design reproduzível

---

## Cronograma Estimado

- **Design refinado:** 1–2 semanas
- **Recruitment + setup:** 2 semanas
- **Data collection:** 1–2 semanas (6 sessions × 60 min)
- **Analysis:** 2–3 semanas
- **Writing:** 3–4 semanas

---

## Próximos Passos

- [ ] Refinar tasks com Carlos
- [ ] Desenhar estímulos (código examples)
- [ ] Preparar environment (online playground vs local)
- [ ] Elaborar protocol de consentimento
- [ ] Piloto com 1 participant (validar timing, clareza de tasks)
