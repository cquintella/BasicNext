# Bucket 0.6.11 — BNCrypto Security Hardening, BNCompress and Numeric Extensions

**Status:** ongoing, started 2026-10-27.
**Source:** Issue #20 (BNCrypto security), BNCompress proposal, `todo/proposals/bnmath-bndata-statistics-0.6.10.md` and `todo/proposals/bndata-expansion.md`.

Every row is an extension: spec text, red tests on `bni` and `bnc`, then one implementation adapted by both backends.

---

## Sprint 1: BNCrypto Security Hardening (Prioridade Máxima — Issue #20)

### Objetivo

Eliminar a vulnerabilidade crítica de geração de nonces previsíveis via `HOST.Random`, garantir zeroização segura de chaves em memória (`zeroize`) e introduzir o tipo de chave dedicado `Key`.

### Atividades

- [x] **S1.1 (Spec & Declaração):** Retificar `language/0.6/bncrypto.md` revogando expressamente o uso de `HOST.Random` para nonces. Declarar a classe `EXPORT CLASS Key` em `modules/bn/BNCrypto.bn` e as fábricas `GenerateKey`, `ImportKey`, `RandomBytes` e `GenerateNonce`.
- [x] **S1.2 (CSPRNG Nativo):** Implementar geração de entropia criptográfica segura via SO (`getrandom` / `SecRandomCopyBytes` / `BCryptGenRandom`) no runtime nativo (`crates/bn_rt/src/crypto.rs`) e no provider interpretado (`libraries/bn_lib_crypto`).
- [x] **S1.3 (Tipo Key e Zeroização):** Implementar o gerenciador de handles de `Key` com `zeroize::ZeroizeOnDrop` em `bn_rt` e `bn_lib_crypto`. Ligar a limpeza ao destrutor ARC e expor o método explícito `Key.Zeroize()`.
- [x] **S1.4 (Migração de Assinaturas):** Atualizar métodos de cifra simétrica (`SealAesGcm`, `OpenAesGcm`, `SealChaCha20`, `OpenChaCha20`), autenticação (`HmacSha256`, `VerifyHmacSha256`), assinaturas (`Ed25519Sign`, `EcdsaP256Sign`, `MlDsaSign`) e decapsulação (`MlKemDecapsulate`) para exigirem `key AS Key`.
- [x] **S1.5 (Testes de Paridade e Rejeição):** Adicionar testes provando:
  - Nonces gerados por CSPRNG são imprevisíveis e únicos.
  - Chaves com tamanhos inválidos são rejeitadas com `INVALID_ARGUMENT`.
  - Tentativas de imprimir ou vazar `Key` diretamente não compilam ou não expõem bytes.
  - Zeroização de memória limpa os dados sensíveis no descarte.
  - Paridade exata `bni` x `bnc` em todos os algoritmos.

---

## Sprint 2: BNCompress — Deflate, Gzip e Arquivos ZIP

### Objetivo

Fornecer suporte nativo a compressão de dados em memória/stream (Deflate/Gzip) e contêineres de arquivo (`ZipArchive`), eliminando a necessidade de rotinas manuais de compressão em Basic Next.

### Atividades

- [x] **S2.1 (Spec & Módulo):** Criar especificação normativa `language/0.6/bncompress.md` e declarações de módulo em `modules/bn/BNCompress.bn`.
- [x] **S2.2 (Core de Domínio):** Criar o crate `libraries/bn_core_compress` encapsulando `flate2`/`miniz_oxide` e `zip`.
- [x] **S2.3 (Compressão em Memória):** Implementar funções de compressão e descompressão de buffers: `Deflate`, `Inflate`, `Gzip`, `Gunzip`.
- [x] **S2.4 (Contêiner ZipArchive):** Implementar a classe `ZipArchive` para leitura, criação e extração de arquivos compactados (`Open`, `Create`, `Entries`, `ExtractBytes`, `ExtractAll`, `AddBytes`, `AddFile`, `Close`).
- [x] **S2.5 (Runtime e Driver):** Expor símbolos C ABI em `libraries/bn_stdlib/src/compress_abi.rs` e registrar o provider em `libraries/bn_lib_compress` para `bni`.
- [x] **S2.6 (Testes de Paridade):** Validar compressão, descompressão, criação e extração de arquivos ZIP com round-trip e paridade estrita nos dois backends.

---

## Sprint 3: Extensões Numéricas e Estatísticas (BNMath / BNData)

### Objetivo

Completar a inferência estatística (cálculo de p-valor para a distribuição t de Student) e operações essenciais diferidas do 0.6.10.

### Atividades

- [ ] **S3.1 (Distribuição t de Student & p-valor):** Implementar `BNMath.StudentT(t, df)` (função cumulativa CDF) e cálculo do p-valor bicaudal utilizando a função beta incompleta regularizada em `bn_core_math` (determinística, sem desvio entre plataformas).
- [ ] **S3.2 (Quantil t de Student):** Implementar `BNMath.StudentTQuantile(p, df)` validado contra valores de referência do R 4.5.1.
- [ ] **S3.3 (Reduções Vetoriais Restantes):** Implementar `Sum`, `Product`, `Dot` com semântica de overflow estável (`±INF` ou erro numérico) sem alocações adicionais.
- [ ] **S3.4 (Testes Numéricos):** Paridade `bni` x `bnc` e validação numérica contra suites de teste NIST/R.

---

## Sprint 4: Documentação, Livro, Dogfooding e Quality Gates

### Objetivo

Concluir o release 0.6.11 integrando documentação, atualizando exemplos reais de código e homologando todos os quality gates.

### Atividades

O Livro sofreu reorganizações então atenção a nova estrutura.

- [ ] **S4.1 (Dogfooding book_packages.bn):** Atualizar `examples/book_packages.bn` para utilizar `BNCompress.ZipArchive` e `Deflate`, substituindo as rotinas manuais de ZIP STORED e CRC-32.
- [ ] **S4.2 (Livro do Basic Next):** Atualizar `docs/book/en/` com o novo módulo `BNCompress`, o uso seguro do tipo `Key` em `BNCrypto` e as funções estatísticas em `BNMath`.
- [ ] **S4.3 (Quality Gates & Releases):** Rodar `./scripts/test-battery.sh --gates` e o ciclo completo de quality gates, redigir `docs/releases/0.6.11.md`.

---

## Sprint 5: Conformidade Numérica e Lexical (Defeitos de Literais e Identificadores)

### Objetivo

Corrigir discrepâncias normativas e bugs no frontend e interpretador identificados em testes de literais numéricos e palavras reservadas.

### Atividades

- [ ] **S5.1 (Diagnóstico de Faixa para Literais Inteiros):** Em `analyzer3.rs` e `helpers1.rs`, quando a atribuição de um `Type::IntegerLiteral` a um tipo inteiro falhar por limites de faixa (`!integer_literal_fits`), emitir `NUMERIC_OVERFLOW` (ou diagnóstico específico de valor fora da faixa do tipo destino) em vez de `TYPE_MISMATCH` ("Expected INT32, but found INT32").
- [ ] **S5.2 (Limites de Literais e Expressões Inteiras Não Tipadas):** Corrigir a dobragem e inferência estática para que literais inteiros sem tipo declarado respeitem estritamente `INT32`. Eliminar o bypass de checagem de faixa em `bn_interp::checked_integer` para `Type::IntegerLiteral` e assegurar que expressões como `10 ** 30` sem tipo sofram checagem de overflow estático em vez de responder `INT32` silenciosamente.
- [ ] **S5.3 (Validação Estática de Literais Float e FLOAT32):** Validar no frontend literais de ponto flutuante: diagnosticar como `NUMERIC_OVERFLOW` literais que excedam `FLOAT64` (impedindo a conversão silenciosa para `INF` via `parse_float`), e validar em tempo de compilação atribuições de literais a destinos `FLOAT32` que excedam `f32::MAX`.
- [ ] **S5.4 (Implementação de RESERVED_IDENTIFIER):** Implementar a decisão normativa do BDFL (2026-10-20, `language/0.6/0.6.md:757` e `keywords.md:148-159`), diagnosticando estaticamente como `RESERVED_IDENTIFIER` qualquer identificador que colida em ASCII case-insensitive com uma palavra reservada (como `true`, `while`, `let`), tanto em declarações quanto em referências em expressões.

---

## Sprint 6: Correções Críticas de Toolchain, Runtime e Empacotamento

### Objetivo

Corrigir defeitos de segurança e divergências de conformidade entre backends (`bni` x `bnc`), além de falha de empacotamento no release de `libbn_stdlib.a`.

### Atividades

- [x] **S6.1 (Segurança: Respeito a BN_FS_POLICY=deny sob --sandbox no bnc):** Em `crates/bn_llvm/src/llvm/functions.rs` e `crates/bn_rt/src/policy.rs`, corrigir a ordem de inicialização e aplicação de políticas de sandbox para que `bn_rt_policy_filesystem_root` e a configuração de raízes de sandbox não anulem nem sobreponham `BN_FS_POLICY=deny` avaliado por `narrow_from_env`. Assegurar que `narrow_from_env` sempre tenha precedência fail-closed estrita sobre qualquer raiz de sandbox em executáveis compilados, reproduzindo a mesma garantia do `bni`.
- [x] **S6.2 (Formatação de Impressão de TIMEZONE no bnc):** Em `crates/bn_llvm/src/llvm/emission2.rs`, retificar `lower_print_value` para excluir `"TIMEZONE"` da impressão genérica de nomes de tipo de classes (`type_name_global`), emitindo a rotina de impressão da string de fuso horário apontada pelo registrador (ex: `"UTC"`), alinhando a saída com o comportamento de referência do `bni`.
- [x] **S6.3 (Preenchimento e Formatação de Error conforme error.md):** Em `crates/bn_interp/src/executor.rs`, `crates/bn_value/src/lib.rs` e funções auxiliares de erro do interpretador, migrar emissões de erro genéricas (`Value::error`) para `Value::error_report`, preenchendo obrigatoriamente os campos `Operation` (nome canônico da operação, ex: `"ASC"`, `"CHR"`) e `Cause` (motivo/regra violada). Garantir que o `PRINT` do objeto `Error` obedeça estritamente ao contrato normativo de `language/0.6/error.md` (`Error <Code> in <Operation>: <Message> (cause: <Cause>)`).
- [x] **S6.4 (Empacotamento e Distribuição de libbn_stdlib.a):** Em `.github/workflows/ci.yml` e `scripts/install.sh`, adicionar a compilação, coleta de artefatos e checksum de `libbn_stdlib-<platform>.a` nos releases do GitHub, bem como seu download e instalação em `$PREFIX/lib/libbn_stdlib.a` pelo script de instalação, eliminando o erro de linkagem em tempo de compilação do `bnc` em instalações limpas.

