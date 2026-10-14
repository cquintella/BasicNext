# Questionário — completar a especificação 0.6

Objetivo: fechar a seção S do bucket 0.6.5 (o `0.6.md` ainda remete a uma
0.1 que nunca existiu para escopos, vetores, ponteiros, structs, módulos,
`IS` e controle de fluxo) e os `TODO (normative gap)` já marcados.
Comportamento atual medido em 2026-10-12 nos dois backends (`bni` = referência
executável; `bnc` = nativo). (R) = recomendação.

Critério para o que ficou aberto (Carlos, 2026-10-12): o mais seguro, depois
o mais compatível, sem deixar lacuna. Questionário completo em 2026-10-12.

Já decidido nesta sessão: `STRING`, `STRUCT` e vetor fixo são **valores**
(toda gravação copia; gravar num lugar altera o lugar no dono).

## A — Valores (S.3, S.5)

- **A1** `=` / `<>` entre `STRUCT`s. Hoje: `bni` compara campo a campo
  (`TRUE`); `bnc` falha ao compilar. (R) campo a campo.
  **Decidido (Carlos, 2026-10-12): campo a campo.**
- **A2** `=` / `<>` entre vetores. Hoje: `bni` elemento a elemento; `bnc` não
  compila. (R) elemento a elemento, mesmas dimensões.
  **Decidido: elemento a elemento, mesmas dimensões.**
- **A3** `PRINT` de `STRUCT` / objeto. Hoje: `bni` imprime o nome do tipo
  (`P`, `Box`); `bnc` não imprime o nome. (R) nome do tipo nos dois.
  **Decidido: nome do tipo nos dois backends.**
- **A4** `STRUCT` com métodos. Hoje: a gramática só aceita campos. (R) manter
  só dados.
  **Decidido: só dados.**
- **A5** Ordem de `STRING` (`<`, `>`). Hoje: `TYPE_MISMATCH`. (R) manter
  proibido (comparação ordenada via `BNString`).
  **Decidido: permitir `<`, `>`, `<=`, `>=` em `STRING`, ordem por code point.**
- **A6** Unidade de `LEN` / `s[i]` em `STRING`. Hoje: code point
  (`LEN("ção") = 3`). (R) code point.
  **Decidido: code point.**
- **A7** U+0000 dentro de `STRING`. Hoje: `bni` guarda; nativo para com
  `TARGET_UNSUPPORTED_TYPE` (K1). (R) caractere válido em ambos (nativo passa
  a ter comprimento — G.S1).
  **Decidido: NUL é caractere válido nos dois backends.**
- **A8** Parâmetro vetor sem tamanho (`INTEGER[]`). Hoje: não existe; só
  tamanho fixo. (R) manter fora da 0.6 (proposta futura).
  **Decidido: entra na 0.6** (detalhes em A8.1–A8.3).
  - **A8.1 Decidido:** só a primeira dimensão é aberta (`INTEGER[]`, `INTEGER[][3]`).
  - **A8.2 Decidido:** só em parâmetros.
  - **A8.3 Decidido:** passado sem cópia (o parâmetro é constante).
- **A9** Vetor local com dimensão por expressão (`INTEGER[n]`). Hoje: gramática
  aceita, `bni` roda, `bnc` recusa (K12). (R) faz parte da linguagem.
  **Decidido: faz parte da linguagem.**
- **A10** Dimensão `0` válida (`LEN = 0`); negativa em execução é
  `INVALID_VECTOR_DIMENSION`. (R) confirmar.
  **Decidido: confirmado.**

## B — Escopos e bindings (S.2)

- **B1** Sombreamento em bloco interno. Hoje: `DUPLICATE_NAME` (um nome por
  função). (R) manter proibido.
  **Decidido: proibido.**
- **B2** Variável do `FOR` depois do laço. Hoje: visível (`PRINT i` → `4`),
  embora um `LET` dentro de `IF` não vaze. (R) local ao laço.
  **Decidido: local ao laço.**
- **B3** `LET` sem inicializador num laço reinicia a cada volta. Hoje: sim.
  (R) confirmar.
  **Decidido: confirmado.**
- **B4** `CONST` de topo não exportado. Hoje: erro de sintaxe (só `EXPORT
  CONST`). (R) permitir `CONST` privado do módulo.
  **Decidido: permitir `CONST` privado no topo.**
- **B5** `CONST` com expressão de execução (`CONST m = n * 2`). Hoje: aceito
  (binding imutável). (R) manter.
  **Decidido: manter (`CONST` = nome imutável).**

## C — Controle de fluxo (S.8)

- **C1** Atribuir à variável do `FOR` no corpo. Hoje: permitido e altera a
  contagem (`1 3 5`). (R) erro estático.
  **Decidido: permitir** (altera a contagem, como hoje).
- **C2** Limite do `FOR` avaliado uma vez. Hoje: sim. (R) confirmar.
  **Decidido: confirmado.**
- **C3** `FOR i AS INT8 = 125 TO 127`. Hoje: `NUMERIC_OVERFLOW` na última
  volta. (R) laço válido, termina sem estouro.
  **Decidido: laço válido, sem estouro.**
- **C4** `STEP` que vale 0 em execução. Hoje: `bni` `INVALID_FOR_STEP`; `bnc`
  pula o laço. (R) erro de execução `INVALID_FOR_STEP` nos dois.
  **Decidido: `INVALID_FOR_STEP` nos dois backends.**
- **C5** `EXIT` / `CONTINUE` em `IF` de uma linha. Hoje: proibido. (R) permitir.
  **Decidido: permitir.**
- **C6** `STOP n` termina com código `n`; faixa válida. (R) `0..255`, fora disso
  erro estático (literal) / de execução.
  **Decidido: 0..255; fora disso erro estático (literal) ou de execução.**

- **C7** `FOR EACH` sobre `STRING`. Hoje: recusado ("Expected fixed-length
  vector"). **Decidido (Carlos, 2026-10-12): entra na spec**, um code point
  por volta (`FOR EACH c AS STRING IN texto`).
- **C8** `FOR EACH` em vetor 2D. Hoje: percorre todos os elementos escalares
  (achatado); `FOR EACH row AS INTEGER[2] IN g` é recusado. (R) por linhas.
  **Decidido: manter achatado** (o elemento de `INTEGER[2][3]` é `INTEGER`).
- **C9** `FOR` contado com `FLOAT`. Hoje: recusado. (R) manter só inteiros.
  **Decidido: só inteiros.**
- **C10** **Decidido (Carlos, 2026-10-12):** no `FOR` contado o `AS <tipo>`
  da variável é opcional; sem ele a variável é `INTEGER`
  (`FOR i = 1 TO 10`).
- **C11** **Decidido (Carlos, 2026-10-12):** no `FOR EACH` o tipo da variável
  vem do elemento do vetor (ou é `STRING` para uma `STRING`); não precisa de
  `AS` (`FOR EACH x IN v`). `AS` continua aceito, e tem de bater com o tipo
  do elemento.
- **C12** **Decidido (Carlos, 2026-10-12):** sem `AS` é `INTEGER`; um limite
  literal que não cabe em `INTEGER` é `TYPE_MISMATCH` — quem precisa escreve
  `AS INT64`.

## DS — Desestruturação e literal de `STRUCT`

**Decidido (Carlos, 2026-10-12) e escrito na spec** (`0.6.md` "Destructuring
`LET` and `STRUCT` literals (0.6.5)", `0.6.ebnf`; bucket S.11): DS1 tipo
deduzido, `AS` opcional; DS2 `STRUCT` por nome de campo; DS3 vetor com
contagem exata, `STRUCT` qualquer subconjunto; DS4 tamanho em execução
diferente → `INDEX_OUT_OF_BOUNDS`; DS5 objetos fora; DS6 referência retida;
DS7 literal de `STRUCT` com um elemento por campo; DS8 literal nunca cria
objeto; DS9 literal em qualquer destino declarado; DS10 na 0.6.5.

## D — `IS` e `NULL` (S.7)

- **D1** Estreitamento através de `AND` (`x IS INTEGER AND x > 2`). Hoje:
  `TYPE_MISMATCH`. (R) estreita à direita do `AND` e no `THEN`.
  **Decidido: estreita à direita do `AND` e no `THEN`.**
- **D2** Membro de `Box OR NULL` sem `IS`. Hoje: passa na análise; em execução
  `bni` `NAME_NOT_FOUND`, `bnc` `USE_AFTER_RELEASE`. (R) erro estático exigindo
  `IS`.
  **Decidido: erro estático exigindo `IS`.**

## E — Módulos (S.6)

- **E1** Ciclo de `IMPORT`. Hoje: `IMPORT_CYCLE`. (R) manter proibido.
  **Decidido: proibido.**
- **E2** Sem `EXPORT` = privado do módulo. Hoje: sim. (R) confirmar.
  **Decidido: confirmado.**
- **E3** Reexportar um import. Hoje: não existe. (R) fora da 0.6.
  **Decidido: entra na 0.6** (detalhes em E3.1–E3.3).
  - **E3.1 Decidido:** `EXPORT IMPORT Caminho AS Nome`.
  - **E3.2 Decidido:** transitivo.
  - **E3.3 Decidido:** conta para `IMPORT_CYCLE`.

## F — Diagnósticos e saída (S.9)

- **F1** Códigos: `0` ok; `1` erro BN (análise ou execução); `2` uso inválido
  ou falha de ferramenta; `STOP n` → `n`. (R) confirmar.
  **Decidido: confirmado.**
- **F2** Avisos. Hoje: `bni` emite `warning[UNUSED_BINDING]` (código 0), mas
  a spec diz "errors, not warnings". (R) avisos existem e não mudam o código.
  **Decidido: avisos existem e não mudam o código.**

## G — `TODO (normative gap)` já marcados

- **G1** §2.6: `a--b` é erro de sintaxe. (R) confirmar.
  **Decidido: confirmado.**
- **G2** §2.3: `PROTECTED` em `STATIC` permitido; acesso por referência base
  de fora da hierarquia = não. (R) confirmar.
  **Decidido: confirmado.**
- **G3** §2.2: ordem `PUBLIC OVERRIDE FUNCTION`; métodos de `IMPLEMENTS` sem
  `OVERRIDE`. (R) confirmar.
  **Decidido: confirmado.**
- **G4** §2.1: downcast (`AS` vs `TRYCAST`). (R) manter adiado.
  **Decidido: continua adiado.**
- **G5** K32(a): `BOOLEAN AS BOOLEAN`, `STRING AS STRING` recusados. (R)
  aceitar a identidade, como nos números.
  **Decidido: aceitar a identidade em todos os tipos.**
- **G6** K32(b): `NAN AS BOOLEAN` = `TRUE`. (R) confirmar.
  **Decidido: confirmado (`TRUE`).**
- **G7** K32(c): `FLOAT64` fora do `FLOAT32` → `INF` (IEEE). (R)
  `NUMERIC_OVERFLOW`, como as demais conversões fora da faixa.
  **Decidido: `NUMERIC_OVERFLOW`.**
- **G8** K32(d): `PRINT` de `TIMESTAMP` = valor `INT64`. (R) ISO 8601.
  **Decidido: manter o valor `INT64` (compatibilidade).**
- **G9** BNLog: `request_body` e `passwd_hash` são escritos; `token_count` não.
  (R) casar por segmento do nome (`request_body` → `body`: sensível).
  **Decidido: casar por segmento.**
- **G10** Tabelas de códigos de erro dos módulos `BN*` (`error.md`). (R) cada
  módulo ganha sua tabela agora.
  **Decidido: cada módulo ganha sua tabela agora.**
- **G11** `FS.File` inalcançável sem `Close`. (R) fecha quando a última
  referência some (como ARC).
  **Decidido: fecha quando a última referência some.**
- **G12** Nome de tipo em diagnósticos: como o usuário escreveu (`INT32 OR
  NULL`) ou canônico (`INTEGER(INT32) OR NULL`). (R) como escrito.
  **Decidido: como o usuário escreveu.**

## Defeitos achados pelas sondas (não são perguntas)

Registrados em `ongoing/known_issues.md` (K67, K68, K69) e no bucket S.13.

- `STRUCT` recursivo (`next AS N`) derruba o `bni` (estouro de pilha no
  check); deve ser erro estático.
- `PRINT` de objeto / `STRUCT` no nativo não imprime o nome do tipo.
- `=` entre `STRUCT`s / vetores: `bnc` não compila.
- `STEP` zero em execução: nativo pula o laço.
- `error.md`: o TODO de `Box[2]` sem inicializador está resolvido (BDFL
  2026-10-07; os dois backends dão `TYPE_MISMATCH`) — remover o TODO.
- Comparar o mesmo objeto (`a = c` com `c = a`) dá `FALSE` no nativo.
- `STOP c` com `c = -1` sai com 255 no nativo (o `bni` dá `INVALID_EXIT_CODE`).
- `b[0].x = 7` perde a gravação no `bni` (K67).

## Rodada 2 — revisão da spec (2026-10-12)

Lacunas achadas lendo `0.6.ebnf` × `0.6.md` × `keywords.md` × código, com o
comportamento medido nos dois backends.

- **R1** `AND`/`OR`/`XOR`/`NOT`: lógicos em `BOOLEAN`, de bits em inteiros
  (`5 XOR 3 = 6`, `NOT 0 = -1`); `AND`/`OR` com curto-circuito em `BOOLEAN`.
  (R) confirmar; `XOR` avalia os dois lados.
- **R2** `SHL`/`SHR`: contagem `0..largura-1` (fora: `INVALID_SHIFT_COUNT`);
  `SHL` que estoura → `NUMERIC_OVERFLOW`; `SHR` lógico mesmo em sinalizado
  (`INT8 -8 SHR 1 = 124`). (R) confirmar.
- **R3** `**`: inteiro verificado (`NUMERIC_OVERFLOW`), expoente negativo →
  `INVALID_EXPONENT`; ponto flutuante IEEE. (R) confirmar.
- **R4** Promoção aritmética (hoje implícita, não escrita): mesmo sinal →
  o mais largo; `BYTE` + sinalizado → ≥ `INT16`; sinalizado + sem sinal →
  `TYPE_MISMATCH`; `FLOAT32 + FLOAT64` → `FLOAT64`; inteiro + float →
  `TYPE_MISMATCH`; `/` → `FLOAT64`. (R) escrever como está; atribuição
  continua sem conversão implícita.
- **R5** `+ - *` e menos unário verificados (`NUMERIC_OVERFLOW`);
  `a op= b` ≡ `a = a op b`, resultado cabendo no tipo de `a`; `+` / `+=`
  concatenam `STRING`. (R) confirmar.
- **R6** `DIV`/`%` euclidianos (resto ≥ 0), os dois backends iguais. (R)
  confirmar (já corrigido no texto).
- **R7** `IS NAN`, `IS INF`, `IS -INF` em ponto flutuante; `NAN = NAN` é
  `FALSE` (IEEE). (R) confirmar.
- **R8** `NULL` (sem objeto/valor), `NA` (dado ausente, numérico/estatístico),
  `EOF` (fim de entrada): só como membros de alternativa; `x = NULL` e
  `x IS NULL` equivalentes; `PRINT` escreve `NULL`/`NA`/`EOF`. (R) confirmar.
- **R9** Valores de função: `FUNCTION(T) AS R`, atribuir pelo nome, passar,
  chamar; sem closures. Hoje `g = Twice` é `FALSE` no `bni` e o nativo não
  compila. (R) comparação por identidade da função (`TRUE`); nativo é
  defeito.
- **R10** `INPUT alvo` / `INPUT "prompt", alvo`: hoje só alvo `STRING`. (R)
  manter só `STRING` (número via `BNMath.VAL`), `EOF` como no `INPUT()`.
- **R11** `SIZEOF(x)`: bytes do valor (`INT64` 8, `STRING` bytes UTF-8, `STRUCT`
  soma dos campos). (R) escrever: bytes da representação portátil, definidos
  por tipo; erro estático em ponteiro, objeto e alternativa.
- **R12** `SYSTEM`: reservado e na gramática, mas sem uso (`NAME_NOT_FOUND`).
  (R) tirar da gramática como tipo; continua reservado.

**Decidido (Carlos, 2026-10-12), escrito em `0.6.md` "Operators", "`NULL`,
`NA`, `EOF`", "Function values", "`INPUT` and `SIZEOF`":** R1, R2, R4, R5,
R7–R11 conforme recomendado; R3 potência matemática (expoente negativo
constante → `FLOAT64`; inteiro com float continua `TYPE_MISMATCH`, como R4);
R6 `DIV` quociente e `%` resto da divisão inteira, euclidiana; R12 `SYSTEM`
retirado na 0.2 → fora da gramática como tipo, continua reservado.
