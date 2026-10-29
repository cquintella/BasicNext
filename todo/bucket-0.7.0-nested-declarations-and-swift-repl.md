# Bucket 0.7.0 — Declarações Aninhadas (Funções e Classes Locais) e REPL Padrão Swift

**Status:** Planned (`todo/bucket-0.7.0-nested-declarations-and-swift-repl.md`)
**Repositório Alvo:** `/Users/caq/src/bn/basicnext`
**Decisão BDFL:** Carlos Quintella (@cquintella) em 2026-10-25: mudança significativa na gramática e semântica da linguagem com transição de versão para **Basic Next 0.7.0**. Adoção integral do modelo do Swift para declarações aninhadas e comportamento de execução do REPL.
**Dependências:** 0.6.15 e 0.6.16 fechados. 0.6.16 é a mudança de padrão da linha 0.6 (escapes e arrays) e precede a gramática 0.7. Se o 0.6.17 começar, ele fecha antes do 0.7.0. Recusa do CRY-1 não cria o 0.6.17 e não bloqueia. 0.6.13 não existe. Não espera 0.6.9b, 0.6.14b nem 0.6.15b. A série 0.6.9 não é o predecessor.

## Revisão de planejamento (2026-10-25)

O bucket não está pronto para execução. A ordem especificação, frontend, IR está certa, e a paridade `bni`/`bnc` está no lugar certo. O problema é de arquitetura: o claim e os locks descrevem duas linguagens, e vários contratos de 0.6 são reutilizados sem regra de desempate. Nenhum cluster deve começar antes das decisões da seção [Decisões que travam a execução](#decisões-que-travam-a-execução) estarem escritas em `language/0.7/0.7.md` como decisão BDFL.

Classificação (language guardian): extensão de gramática, de escopo e do que um programa válido faz. A direção está autorizada no cabeçalho deste bucket (Carlos, 2026-10-25). Isso não fecha captura, sombreamento, escape de tipo local nem o nome do entry point. Norma vigente que o plano tem de enfrentar, não copiar por cima:

- `language/0.6/0.6.md`, "Function values": "There are no closures: a function value captures nothing."
- `language/0.6/0.6.md`, "Scopes and bindings": o nome vale da declaração ao fim do bloco; "One name per function" e `DUPLICATE_NAME`, sem sombreamento.
- `language/0.6/0.6.ebnf`: `statement` não inclui declaração; `named-type` já é `identifier { "." identifier }` e hoje nomeia export de módulo (`Pets.Named`).
- `language/0.6/0.6.md`, "Jupyter host": célula é programa completo com `FUNCTION Start()`; "There are no top-level statements".
- `crates/bn_ir/src/names.rs`: nome emitido é `Dono.Membro` com no máximo um ponto depois do prefixo de módulo. `classify` devolve `None` para `Outer.Inner.Method` e para qualquer nome com `$` no meio. `None` está documentado como bug de lowering ou extensão não documentada. `Start` é o único `EmittedNameKind::Entry`.

### Problemas

1. **"Modelo integral do Swift" e "lambda lifting sem mudar o IR" não cabem no mesmo lock.** Função aninhada no Swift lê e altera locais da função mãe. Isso é closure, mesmo quando a função não é valor de primeira classe. O item C.1 só cria nomes qualificados. Isso implementa função aninhada que não captura nada, e chama o resultado de Swift. Tipo local no Swift também não captura variável local. O plano trata função local e tipo local como o mesmo rebaixamento.

2. **Declaração metida em `statement` alarga o lugar errado.** `statement` aparece em todo `statement-block`, inclusive `IF`, `WHILE` e `FOR`, e o `IF` de uma linha usa `simple-statement`. Uma classe declarada dentro de um laço é um tipo só, no texto, não um tipo novo por iteração. O executor não deve "executar" uma declaração. O plano não diz isso.

3. **`Outer.Inner` já é sintaxe de módulo.** A gramática não precisa de um operador novo. Falta a regra de resolução: o primeiro identificador é um tipo no escopo que tem membro-tipo, ou é um alias de `IMPORT`. Sem essa ordem, `Outer.Inner` passa a ter dois significados.

4. **O esquema `Outer$Inner$Method` quebra o contrato de nomes do IR.** `$` já é o marcador de intrínsecos e de sufixos sintéticos (`$for_condition`, `Counter.$init`). O IR já tem `Function.owner` e o padrão `Dono.Membro`. O plano inventa um mangling paralelo em vez de estender `classify` para um caminho de tipos.

5. **Tipo local que escapa da função não tem regra.** Se `Local` só existe dentro de `F`, o retorno de `F`, o parâmetro de uma função de fora e o campo de um tipo de módulo não podem ser `Local`. Sem isso, o nome mangled vira ABI e aparece em `PRINT` do objeto fora do escopo. Os testes de E.2 cobrem instanciação local e não cobrem escape.

6. **Sombreamento.** 0.6 proíbe dois nomes iguais na mesma função, inclusive num bloco interno. O Swift sombreia. Adotar o Swift aqui muda todo programa que hoje é `DUPLICATE_NAME`, não só os aninhados.

7. **O REPL está acoplado a uma mudança de linguagem que ele não precisa.** O acumulador envolve o turno em `FUNCTION Start()` (`session_accumulator.rs`). Se o usuário declara `Start` e o turno chama `Start()`, a chamada entra no invólucro. Renomear o invólucro resolve isso. Declaração aninhada não resolve isso. Juntos, os dois viram um programa cuja entrada não é `Start`, enquanto `classify("Start")` continua sendo `Entry` e o Jupyter 0.6 continua exigindo `Start`.

8. **`__session_entry` é um identificador válido.** O usuário pode declará-lo. Marcá-lo como `FunctionKind::Entry` cria um segundo entry quando o usuário também declara `Start`. `bnc` precisa de um único entry.

9. **A.1 troca `SPEC_EBNF` no primeiro item.** `build.rs` e `tests/frontend/u1.rs` comparam o registry com a gramática ativa. Apontar o registry para `0.7.ebnf` antes do parser aceitar a gramática nova derruba o gate no meio do Cluster A. Copiar a árvore 0.6 inteira também duplica host, diagnóstico e palavras reservadas: qualquer correção de 0.6 durante a migração diverge em silêncio.

10. **E.4 testa o nome errado.** `FUNCTION start()` não é o ponto de entrada. `Start` é identificador, `FUNCTION` é a palavra reservada. Palavra reservada minúscula é outro identificador.

11. **Superfície incompleta em relação ao próprio claim.** O claim cita classe e struct. Não cita `INTERFACE` nem `ENUM`. Não diz se tipo aninhado tem `PUBLIC` / `PRIVATE` / `PROTECTED`, se pode ser `EXPORT`, se `EXTENDS` pode apontar para o tipo pai, nem se duas funções locais podem recursar uma na outra. "One name per function" e "visível a partir da declaração" respondem a recursão mútua de um jeito. O Swift responde de outro.

### Sugestões

1. **Partir o bucket em dois cortes.** Corte 1: função aninhada, tipo aninhado e tipo local, sem captura de local. Corte 2, só com decisão explícita: closure. O REPL é um terceiro corte, ou o primeiro, porque o defeito do `Start` sintético já está no acumulador e não depende de declaração aninhada. Não implementar os três no mesmo cluster.

2. **Escrever o modelo sem captura como o default que preserva 0.6, e pedir confirmação para furar isso.** Função local vê os próprios parâmetros, nomes de módulo e tipos visíveis no ponto da declaração. Não vê local, parâmetro nem `CONST` da função mãe. Valor de função continua sem captura. Tipo local e tipo aninhado seguem a mesma regra: método não lê local da função onde o tipo foi declarado. Isso é o único modelo compatível com C.1 como está escrito. Se a decisão de 2026-10-25 exigir captura no estilo Swift, C.1 está errado e o lock de IR cai: passa a existir um ambiente, com vida, ARC e paridade. Essa escolha fica na seção de decisões, antes de A.2.

3. **Não pôr a declaração em `statement`.** Acrescentar `local-declaration` como alternativa de `statement-block`, nunca de `simple-statement`. A declaração não é instrução: não corre, não participa de `STOP` e existe uma vez por ocorrência no texto. O nome segue a regra de bloco já vigente: da declaração até o fim daquele bloco, e `DUPLICATE_NAME` se repetir na função. Recursão da própria função local é permitida, como já é no módulo. Recursão mútua fica na decisão abaixo. Não adotar sombreamento do Swift neste bucket.

4. **Tipo local não escapa.** Proibir o nome de um tipo declarado dentro da função em retorno, parâmetro e campo de qualquer tipo declarado fora dessa função. A instância só sai se o tipo estático visível do lado de fora for uma classe ou interface de módulo que o tipo local estende ou implementa. `PRINT` de um objeto continua mostrando o nome de tipo. Definir qual nome: o caminho fonte `Outer.Inner`, não o símbolo interno do IR.

5. **Estender o nome que já existe.** Caminho de tipo com pontos, `Outer.Inner.Method`, `Outer.Inner.CONSTRUCTOR`, `Outer.Inner.$init`, mais o prefixo de módulo `#N.` quando houver. Atualizar `classify` e os testes em `names.rs` no mesmo corte que introduz o caminho. Não usar `$` como separador de aninhamento. `Function.owner` carrega o tipo dono. A chave de ordenação estável é esse caminho, não um mangling paralelo.

6. **Resolver `Outer.Inner` pelo binding do primeiro nome.** Se `Outer` é classe ou struct no escopo e `Inner` é tipo declarado nele, o caminho é esse tipo. Se `Outer` é alias de `IMPORT`, vale a regra de export de hoje. Um nome não pode ser as duas coisas. Isso já cai em `DUPLICATE_NAME` se alguém tentar. Visibilidade do tipo aninhado é a mesma de um membro: o que está escrito, e a omissão precisa de uma frase normativa, não de um default inventado na implementação. `PRIVATE` vê-se no tipo pai e nos tipos declarados dentro dele. Não se vê em subclasse, igual ao campo privado. Tipo aninhado não é campo e não entra no layout. `EXPORT` aplica-se ao tipo de módulo, não ao tipo aninhado. `INTERFACE` entra no mesmo corte. `ENUM` fica de fora até existir decisão. Ciclo `Inner EXTENDS Outer` é o mesmo erro de herança cíclica já especificado.

7. **REPL sem segunda linguagem.** Arquivo `.bn` continua sem statement de topo e com um único `FUNCTION Start` como entry, quando o programa tem entry. A sessão do `bnr` é outra gramática, escrita na spec: declarações de módulo persistem. Statements de um turno vão para o corpo de uma função sintética cujo nome é reservado, rejeitado se o usuário o declarar. Essa função é o entry daquele turno. `Start` declarado pelo usuário é função comum. O turno que contém a chamada `Start()` gera um corpo que chama essa função. Não há recursão. `export_program` emite um programa 0.7 válido: o `Start` do usuário, se existir, é o entry do arquivo exportado. Os statements acumulados precisam de um lugar honesto nesse arquivo, ou o export deixa de prometer programa executável. Não marcar duas funções como `FunctionKind::Entry`.

8. **Trocar o registry por último.** A.1 cria `language/0.7/` como cópia e não altera `SPEC_EBNF`. O parser e os testes negativos passam a conhecer a gramática nova ainda com a árvore 0.6 ativa, ou a troca do ponteiro é o último item do Cluster B, com a bateria verde no mesmo corte. Até a troca, 0.6 permanece a norma do toolchain. Congelar 0.6 no momento da cópia e não editar as duas árvores.

9. **Testes mínimos que faltam em E.2 e E.4, como negativos.** Função local lendo local da mãe, rejeitada no corte sem captura. Tipo local no retorno da função mãe, rejeitado. `Outer.Inner` com `Outer` alias de módulo e com `Outer` classe, um programa cada. Nome com mais de um ponto aceito por `classify`. Declaração dentro de `IF` de uma linha, rejeitada. Dois `Start` no sentido de entry, impossível. `FUNCTION Start()` com esse spelling, não `start`. `STEP` de sessão: declarar `Start`, chamar `Start()` no turno seguinte, a pilha não cresce com o turno. Paridade `bni` e `bnc` nesses programas, não só nos positivos.

### Decisões que travam a execução

Enquanto a linha correspondente não estiver em `0.7.md` com data, o cluster indicado não abre.

| # | Decisão | Recomendação para reduzir risco | Trava |
| --- | --- | --- | --- |
| G1 | Função aninhada captura local da função mãe? | Não neste bucket. Captura é outro bucket, com ambiente no IR. | A.2, Cluster C |
| G2 | Tipo local e tipo aninhado capturam local ou parâmetro de tipo da função? | Não. Basic Next não tem genéricos. Não importar esse pedaço do Swift. | A.3 |
| G3 | Sombreamento de nome? | Manter `DUPLICATE_NAME`. | A.3, B.2 |
| G4 | Recursão mútua entre funções locais do mesmo bloco? | Só a própria função, mais chamar uma função local já declarada. Não içar nomes. | A.3 |
| G5 | Instância de tipo local pode escapar com o tipo local no assinatura? | Não. Só por upcast para tipo de módulo. | A.3, E.2 |
| G6 | Statement de topo em arquivo `.bn`? | Não. Só na sessão do `bnr`, com a gramática de sessão escrita na spec. | Cluster D |
| G7 | Nome reservado do entry sintético da sessão, e o que `export_program` produz? | Um nome reservado. Um entry. Export é programa com `Start` se o usuário declarou `Start`. | D.1, D.2 |

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

## Cluster F — Coerência do `HOST` (decisões BDFL, 2026-10-27)

Decisões de Carlos Quintella, 2026-10-27:

1. **O `HOST` nasce com o programa e nunca é importado.** Toda capacidade é escrita `HOST.<capacidade>` (`HOST.Clock.Now()`, `HOST.FileSystem.Open(...)`, `HOST.Console.Cls()`), em qualquer módulo. `IMPORT HOST.x AS y` deixa de existir; `IMPORT` serve só a módulos `.bn` e à biblioteca `BN*`. Substitui a decisão de 2026-10-07 ("Which capabilities need `IMPORT`", `0.6.md`).
2. **Entradas do processo só no módulo executável.** `HOST.Args` e `HOST.Env` são usados apenas no módulo que declara `Start`; bibliotecas recebem o que precisam como parâmetros (ocultação de informação). Estende a regra que já valia para `HOST.Args` (`host.md`, "Command-line environment") ao `HOST.Env`.

Consequência de 1: os requisitos de capacidade que a política confere antes do `Start` (`HOST_CAPABILITY_UNAVAILABLE`) passam a vir dos usos de `HOST.x` no programa, não dos `IMPORT`.

Pendente: **TODO (decisão BDFL)** a transição. Opção recomendada: no 0.6.x, `IMPORT HOST...` gera aviso com a forma nova, e no 0.7.0 vira erro. Alternativa: erro direto no 0.7.0.

- [ ] **F.1 (Spec):** `language/0.7/` (ou `0.6.md`, conforme a transição): substituir a regra de 2026-10-07; retirar `HOST` de `module-import-path` em `0.7.ebnf`; reescrever os exemplos de `host.md`, `host-net.md`, `host-exec.md`, `host-env.md` e `console.md` sem `IMPORT`; remover a nota de `EXPORT IMPORT` sobre `HOST`; estender a restrição de `HOST.Args` a `HOST.Env` em `host-env.md`; registrar as duas decisões na tabela de versões e no histórico.
- [ ] **F.2 (Frontend):** rejeitar `IMPORT HOST...` com diagnóstico que mostra a forma `HOST.<capacidade>` (catálogo); aceitar `HOST.<capacidade>` sem import em qualquer módulo; rejeitar `HOST.Args`/`HOST.Env` fora do módulo com `Start` (diagnóstico próprio no catálogo).
- [ ] **F.3 (Requisitos e política):** derivar a lista de capacidades do programa dos usos de `HOST.x` (no lugar dos imports), para `bni` e `bnc`; a recusa antes do `Start` continua igual nos dois backends.
- [ ] **F.4 (Fixtures):** positivas (cada capacidade sem import, nos dois backends) e negativas (`IMPORT HOST.Clock AS Clock`; `HOST.Env` e `HOST.Args` numa biblioteca); paridade `bni` × `bnc`.
- [ ] **F.5 (Migração):** reescrever sem `IMPORT HOST` os exemplos, as fixtures, `tests/modules/*` e o programa de teste do BNCompress; atualizar as sugestões de autocompletar do LSP (`bn_lsp`) e do `bnr`.
- [ ] **F.6 (Livro):** caps. 7, 8, 12, 13 e 14 e a seção de sandbox sem `IMPORT HOST`; cap. 12 explica que o `HOST` nasce com o programa e por que `Args` e `Env` ficam no módulo com `Start`.

## Cluster G — Nomes de `BNMath` em PascalCase (decisão BDFL, 2026-10-28)

Decisão de Carlos Quintella, 2026-10-28: `MEAN` foi erro de grafia. Método de `BNMath` segue PascalCase, como `Gcd`, `Sum` e `BNData.Mean`. Identificadores são sensíveis a caixa (`0.6.md`, "Reserved identifiers and letter case"), então o nome antigo deixa de existir no 0.7.0. Não há alias.

O mesmo erro está nos outros `EXPORT FUNCTION` de `modules/bn/BNMath.bn` escritos só em maiúsculas. Os que já estão em PascalCase não mudam: `Gcd`, `Lcm`, `DegreeToRad`, `RadToDegree`, `PVariance`, `PStdev`, `Skewness`, `Kurtosis`, `PSkewness`, `PKurtosis`, `Quantile`, `Covariance`, `Correlation`, `Sum`, `Product`, `Dot`, `StudentT`, `StudentTPvalue`, `StudentTQuantile`, e as constantes `Pi` e `Euler`.

Os membros de `Limits` (`MAX_FLOAT`, `MIN_INT64`, …) ficam de fora: são constantes de faixa em `UPPER_SNAKE`, não métodos.

| Hoje | 0.7.0 |
| --- | --- |
| `ABS` | `Abs` |
| `MIN` | `Min` |
| `MAX` | `Max` |
| `SIGN` | `Sign` |
| `FLOOR` | `Floor` |
| `CEIL` | `Ceil` |
| `TRUNC` | `Trunc` |
| `ROUND` | `Round` |
| `EXP` | `Exp` |
| `LOG` | `Log` |
| `LOG10` | `Log10` |
| `LOG2` | `Log2` |
| `POW` | `Pow` |
| `SIN` | `Sin` |
| `COS` | `Cos` |
| `TAN` | `Tan` |
| `ASIN` | `Asin` |
| `ACOS` | `Acos` |
| `ATAN` | `Atan` |
| `ATAN2` | `Atan2` |
| `SQRT` | `Sqrt` |
| `HYPOT` | `Hypot` |
| `FMA` | `Fma` |
| `VAL` | `Val` |
| `MEAN` | `Mean` |
| `MEDIAN` | `Median` |
| `QUARTILE1` | `Quartile1` |
| `QUARTILE3` | `Quartile3` |
| `MODE` | `Mode` |
| `STDEV` | `Stdev` |
| `VARIANCE` | `Variance` |
| `RANGE` | `Range` |
| `TOHOUR` | `ToHour` |
| `TOWEEKDAY` | `ToWeekday` |
| `TODATE` | `ToDate` |
| `TOTIME` | `ToTime` |
| `TOTIMESTAMP` | `ToTimestamp` |

- [ ] **G.1 (Spec):** em `language/0.7/math.md` (a cópia de A.1), trocar cada nome da tabela. O nome antigo é identificador inexistente, não diagnóstico especial.
- [ ] **G.2 (Módulo e toolchain):** `modules/bn/BNMath.bn`, o casamento de nome em `bn_frontend` (`math_call` e o lowering), `bn_lib_math` e `bn_llvm`. Os símbolos `bn_rt_math_*` não mudam: são ABI, não o nome da linguagem.
- [ ] **G.3 (Programas):** exemplos, livro, fixtures e testes que chamam o nome antigo passam ao nome novo. Um programa 0.7 com `Math.MEAN` não resolve o método.

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
