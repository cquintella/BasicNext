# Bucket 0.6.11 — BNCrypto security hardening and numeric extensions

**Status:** todo, updated 2026-10-27 (Carlos: "Antes das operações novas, o mais urgente: a BNCrypto não tem gerador aleatório seguro... definir como resolver isso e colocar no bucket da 0.6.11"). Starts after 0.6.10.
**Source:** Issue #20 (BNCrypto security), `todo/proposals/bnmath-bndata-statistics-0.6.10.md` (sections 3–6) and `todo/proposals/bndata-expansion.md`.

Every row is an extension: spec text, red tests on `bni` and `bnc`, then one implementation adapted by both backends.

---

## Priority: BNCrypto Security Hardening (Carlos: "o mais urgente", Issue #20)

### Problema Diagnosticado
1. **Falta de CSPRNG seguro:** A especificação (`language/0.6/bncrypto.md` linha 86) orienta: *"derive nonces from HOST.Random or a counter you control"*. `HOST.Random` é um PRNG determinístico com semente (`Seed`), gerando nonces previsíveis e quebrando a segurança de cifras simétricas (AES-GCM, ChaCha20-Poly1305).
2. **Ausência de zeroização de chaves:** O descarte de buffers de chaves e material sensível no runtime ARC não limpa os bytes (`zeroize`), deixando segredos expostos em memória heap/stack.
3. **Falta de tipo de chave dedicado:** Qualquer instância de `Bytes` serve como chave para cifras, HMAC e assinaturas, permitindo tamanhos incorretos, misturas acidentais de buffers (chave x nonce x dados) e ausência de invariantes de ciclo de vida.

### Resolução Técnica Definida
- **CSPRNG Seguro:**
  - Adicionar em `BNCrypto` o gerador aleatório criptográfico provido diretamente pelo SO (`getrandom` em Linux, `SecRandomCopyBytes` em macOS, `BCryptGenRandom` em Windows):
    - `EXPORT FUNCTION RandomBytes(length AS INTEGER) AS Bytes OR Error`
    - `EXPORT FUNCTION GenerateNonce(cipherAlgorithm AS INTEGER) AS Bytes OR Error`
  - Retificar `language/0.6/bncrypto.md`: revogar formalmente qualquer menção a `HOST.Random` para derivação de nonces; estipular uso exclusivo do CSPRNG seguro.
- **Zeroização de Memória (`Zeroize`):**
  - Implementar no runtime Rust (`libraries/bn_crypto` e `crates/bn_rt/src/crypto.rs`) a trait `zeroize::Zeroize` / `ZeroizeOnDrop` (com barreira de otimização de compilador) para todo buffer de chave, semente e estado intermediário de cifras.
  - Expor no ciclo de vida do ARC: o `DESTRUCTOR` da classe de chave invoca a rotina nativa de zeroização antes de desalocar o handle.
  - Adicionar método explícito: `PUBLIC FUNCTION Zeroize() AS VOID` na classe de chave.
- **Tipo de Chave Dedicado (`Key`):**
  - Introduzir em `modules/bn/BNCrypto.bn` a classe opaca `EXPORT CLASS Key` (distinta de `Bytes`):
    - `PUBLIC FUNCTION Algorithm() AS INTEGER`
    - `PUBLIC FUNCTION Length() AS INTEGER`
    - `PUBLIC FUNCTION Zeroize() AS VOID`
    - Não expõe `ToHex()` nem conversão implícita para `STRING` (opaca contra vazamento acidental em logs/console).
  - Fábricas seguras:
    - `EXPORT FUNCTION GenerateKey(algorithm AS INTEGER) AS Key OR Error` (gera chave a partir do CSPRNG nativo validando o tamanho exato do algoritmo).
    - `EXPORT FUNCTION ImportKey(material AS Bytes, algorithm AS INTEGER) AS Key OR Error` (valida o tamanho exato: 32 bytes para AES-256 / ChaCha20 / Ed25519; consome ou copia com zeroização garantida).
  - Atualização das assinaturas criptográficas em `BNCrypto.bn` e `language/0.6/bncrypto.md`:
    - `SealAesGcm(key AS Key, nonce, plaintext, aad AS Bytes) AS Bytes OR Error`
    - `OpenAesGcm(key AS Key, nonce, ciphertext, aad AS Bytes) AS Bytes OR Error`
    - `SealChaCha20(key AS Key, nonce, plaintext, aad AS Bytes) AS Bytes OR Error`
    - `OpenChaCha20(key AS Key, nonce, ciphertext, aad AS Bytes) AS Bytes OR Error`
    - `HmacSha256(key AS Key, data AS Bytes) AS Bytes OR Error`
    - `VerifyHmacSha256(key AS Key, data, tag AS Bytes) AS BOOLEAN`
    - `Ed25519Sign(key AS Key, message AS Bytes) AS Bytes OR Error`
    - `EcdsaP256Sign(key AS Key, message AS Bytes) AS Bytes OR Error`
    - `MlDsaSign(key AS Key, message AS Bytes) AS Bytes OR Error`
    - `MlKemDecapsulate(key AS Key, ciphertext AS Bytes) AS Bytes OR Error`

### Lista de Atividades BNCrypto
- [ ] **CRYPTO-1 (Spec & Interface):** Atualizar `language/0.6/bncrypto.md` e `modules/bn/BNCrypto.bn` com a declaração da classe `Key`, as fábricas `GenerateKey` / `ImportKey`, as funções de geração de entropia `RandomBytes` / `GenerateNonce`, e a revogação do uso de `HOST.Random` para nonces.
- [ ] **CRYPTO-2 (CSPRNG nativo):** Implementar `RandomBytes` e `GenerateNonce` no runtime nativo (`crates/bn_rt/src/crypto.rs`) e no provider interpretado (`libraries/bn_lib_crypto`), consumindo a fonte criptográfica segura do SO (`getrandom` / `ring::rand::SystemRandom`).
- [ ] **CRYPTO-3 (Tipo Key e Zeroize):** Implementar o gerenciador de handles de `Key` no runtime nativo com `zeroize::ZeroizeOnDrop`, integrando ao destrutor ARC em `bnc` e `bni`. Adicionar método explícito `.Zeroize()`.
- [ ] **CRYPTO-4 (Migração de Assinaturas):** Atualizar lowering de `bnc` e adaptadores de `bni` para exigirem `Key` nos métodos de cifra simétrica, HMAC, assinatura assimétrica e decapsulação pós-quântica.
- [ ] **CRYPTO-5 (Testes de Paridade e Segurança):** Fixtures com KATs (Known Answer Tests), testes de rejeição para tamanhos inválidos de chaves, teste de paridade cruzada `bni` x `bnc`, e verificação da impossibilidade de vazamento de chave via `PRINT` ou interpolação.

---

## Numeric Extensions Deferred by Carlos (2026-10-27)

- [ ] **Factorial, Binomial.** Returning `FLOAT` for an integer result is a design choice that
  needs its own discussion (the proposal follows R: exact to `22!`, then rounded; `n > 170` is
  `INF`). Decide the result type first.
- [ ] **Sinh, Cosh, Tanh, Log1p, Expm1.** They come from each platform's libm, so `bni` and `bnc`
  can differ between macOS, Linux and Windows (question Q6). Decide: accept the caveat, as for
  `SIN`, or ship correctly rounded versions in `bn_core_math`.
- [ ] **Spearman, WeightedMean.** Competing definitions (tie handling) need a choice; `WeightedMean`
  with a negative weight is question Q4. (`Skewness`, `Kurtosis` and their population forms shipped
  in 0.6.10, Carlos 2026-10-27.)
- [ ] **HOST.Random.Normal.** New `HOST` surface; depends on Q5 (the error code for `sigma < 0`)
  and Q6 (`LOG` from libm).
- [ ] **CumSum, NormalVector.** Depend on Q3: the language cannot yet return a vector whose
  length is computed from the argument. Options recorded in bucket 0.6.10: methods on the E.5
  `Series`, a same-length vector return (a language extension), or an output vector.
- [ ] **E.5 (`bndata-expansion.md`).** A separate proposal, not yet approved. It must be updated
  first: it uses `DELETE`, which the language does not have (ARC and `RELEASE`), and its
  `ReadCSVPath`/`WriteCSVPath` must go through `HOST.FileSystem` and its policy.

## Inference (Carlos, 2026-10-27: "t-student depois, já p-valor para 0.6.11")

- [ ] **p-value: the Student t distribution.** `StudentT(t, df)` (the CDF, R `pt`) and its two-sided
  p-value, through the regularized incomplete beta `I(df / (df + t²); df/2, 1/2)` with `lgamma`. Open:
  libm `exp`/`log` (the Q6 caveat: the last digit may differ between platforms) or an own
  implementation in `bn_core_math` from `+ − × ÷ √` (the same bits everywhere; recommended), and the
  quantile `StudentTQuantile` (R `qt`). Validate against R 4.5.1 within a few units.
- [ ] **t-test (later than the p-value).** One sample, Welch, pooled and paired; the statistic and
  degrees of freedom from the existing means and variances; the result form (an Excel-style `TTest`
  returning the p-value, or a `STRUCT` with statistic, degrees of freedom and p-value) is open.

## Also left out of 0.6.10 by G-5

- [ ] **Sum, Product, Dot.** Open: the overflow rule (recommendation `±INF`, as `MEAN`) and the
  result type for integer vectors (`FLOAT`, or exact integer with `NUMERIC_OVERFLOW`).
- [ ] **Fmod.** Open: the sign of the result (sign of `a`, as C `fmod`, or Euclidean, as `%`).
- [ ] **Q-NAMES.** The legacy upper-case `BNMath` names (`SIN`, `SQRT`, `MEAN`, …): PascalCase
  aliases, with or without a deprecation warning, or a rename.
- [ ] **Q-IS.** `x IS INTEGER = TRUE` is accepted though the grammar has no `=` after `IS type`;
  fix the parser to the grammar or keep it.

## Closure

- [ ] Each shipped row: spec (`language/0.6/math.md`, `bndata.md`, `host.md`), `modules/bn/*.bn`,
  tests on both backends, book (`docs/book/en/`), release notes `docs/releases/0.6.11.md`.
- [ ] Full battery, `--gates`, the Windows batch; `git mv` to `done/`.
