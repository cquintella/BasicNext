# Issues — Auditoria de Segurança, Bugs, Concorrência e Qualidade (`BNFfi` e Toolchain 0.6.7)

Catálogo normativo de defeitos, vulnerabilidades de segurança, condições de corrida, problemas de qualidade de código e pendências de snapshot do CI identificados na auditoria da versão `0.6.7` (2026-10-24).

Bucket vinculado: [`done/bucket-0.6.7a-bugfixes.md`](file:///Users/caq/src/bn/basicnext/done/bucket-0.6.7a-bugfixes.md)

---

## Críticos e Alta Severidade (Segurança de Memória e Bugs Funcionais)

### ISS-01 — [CRÍTICO] Use-After-Free (Dangling Pointer) no Empréstimo de Regiões para Arrays C no Compilador (`bnc`)

* **Status**: RESOLVED
* **Severidade**: Crítica (Segurança de Memória / Corrupção de Heap)
* **Tarefa no Bucket**: `B.1`, `C.1`
* **Localização**: [`lower_constructor`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/ffi_calls.rs#L336-L368), [`bn_rt_ffi_new_array`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_abi.rs#L116-L130) e [`CValuePayload::Array`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi.rs#L114-L120).
* **Causa Raiz**: No backend LLVM (`bnc`), a construção de `C.*Array(region)` extrai imediatamente o ponteiro bruto de dados (`ffiarrptr`) e a capacidade (`ffiarrcap64`) do descritor de região em tempo de construção e armazena `*mut u8` diretamente em [`CValuePayload::Array`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi.rs#L114-L120). Não há verificação posterior de que a região Basic Next continua viva no momento de `C.Call`, `ReadFrom` ou `WriteTo`.
* **Cenário de Disparo**: O programa instancia `LET arr = NEW C.Int32Array(region)`, executa `DELETE region` e em seguida invoca `C.Call`, `arr.ReadFrom()` ou `arr.WriteTo()`. No `bni`, o interpretador verifica `core.memory().len(region_h, span)?` e aborta com segurança; no `bnc`, o binário lê ou escreve em memória já liberada na heap.
* **Correção Exigida**: Validar a vivacidade da alocação ARC no runtime nativo (`arc::check_live` / registro da região) antes de qualquer acesso em `ReadFrom`, `WriteTo` e `C.Call`, ou gerenciar um buffer próprio sincronizado explicitamente com checagem de vivacidade da região em cada operação.

### ISS-02 — [ALTO] Overflow Inteiro em Multiplicação de Tamanho de Buffer (`element_size * count`)

* **Status**: RESOLVED
* **Severidade**: Alta (Segurança de Memória / Heap Buffer Overflow)
* **Tarefa no Bucket**: `C.2`, `D.1`
* **Localização**: [`array_read_from`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi.rs#L765), [`array_write_to`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi.rs#L796), [`bn_lib_ffi::read_from`](file:///Users/caq/src/bn/basicnext/crates/bn_lib_ffi/src/lib.rs#L441), [`bn_lib_ffi::write_to`](file:///Users/caq/src/bn/basicnext/crates/bn_lib_ffi/src/lib.rs#L515), [`bn_lib_ffi::call_invoke`](file:///Users/caq/src/bn/basicnext/crates/bn_lib_ffi/src/lib.rs#L600) e [`ffi_calls.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/ffi_calls.rs#L356).
* **Causa Raiz**: O cálculo `let total_bytes = element_size * count;` usa multiplicação sem verificação de overflow (`checked_mul`). Em [`ffi_calls.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/ffi_calls.rs#L356), a capacidade `i32` sofre extensão de sinal (`SExt` para `i64`) antes de ser convertida para `usize` em [`bn_rt_ffi_new_array`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_abi.rs#L121).
* **Cenário de Disparo**: Em builds `release` ou alvos de 32 bits, uma contagem negativa estendida com sinal ou grande o suficiente para estourar `usize` trunca `total_bytes` ou passa `usize::MAX` para `std::ptr::copy_nonoverlapping`.
* **Correção Exigida**: Usar `element_size.checked_mul(count)` em todos os pontos de alocação e cópia, e validar que o comprimento não é negativo antes de converter para `usize`.

### ISS-03 — [ALTO] Flag `--ffi-path` Ignorada em Binários Compilados (`bnc`)

* **Status**: RESOLVED
* **Severidade**: Alta (Quebra de Paridade `bni` × `bnc`)
* **Tarefa no Bucket**: `B.2`, `C.3`
* **Localização**: [`bn_rt_ffi_call`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_abi.rs#L267) e [`artifact.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_compile_driver/src/artifact.rs#L1-L120).
* **Causa Raiz**: O `bnc` aceita `--ffi-path <dir>` na linha de comando ([`options.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_cli/src/options.rs#L40-L43)), mas não propaga os diretórios configurados para o binário compilado, e [`bn_rt_ffi_call`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_abi.rs#L267) passa `&[]` fixo para [`call_c_function_with_args`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi.rs#L226).
* **Cenário de Disparo**: Compilar um programa com `bnc --ffi-path ./libs prog.bn -o prog` e executar `./prog` falha com `C.LIBRARY_NOT_FOUND`, enquanto `bni run --ffi-path ./libs prog.bn` funciona.
* **Correção Exigida**: Adicionar suporte no runtime nativo (`bn_rt_ffi_add_search_path` ou tabela configurada no prólogo de `main` emitido pelo `bn_llvm` a partir de `CompileOptions::ffi_paths`) para que binários compilados com `--ffi-path` pesquisem os diretórios fornecidos na mesma ordem do `bni`.

### ISS-04 — [ALTO] Vazamento de Memória (Memory Leak) em `HandleResult.Value()` no Compilador (`bnc`)

* **Status**: RESOLVED
* **Severidade**: Alta (Vazamento de Recursos em Runtime)
* **Tarefa no Bucket**: `B.3`, `C.4`
* **Localização**: [`bn_rt_ffi_handle_result_value`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_abi.rs#L282-L290) e [`ownership_emission.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/ownership_emission.rs#L51-L111).
* **Causa Raiz**: Cada chamada a [`bn_rt_ffi_handle_result_value`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_abi.rs#L282-L290) aloca um novo registro `TYPE_HANDLE` na tabela global `RECORDS`. Além disso, `Value()` retorna `C.Handle OR NULL` (`Type::Alternative`), que [`ownership_emission.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/ownership_emission.rs#L53) trata como `general_alternative` sem chamada de destrutor quando a alternativa sai de escopo sem estreitamento.
* **Cenário de Disparo**: Chamar `hr.Value()` repetidamente em um laço ou descartar o resultado `C.Handle OR NULL` acumula registros eternos no mapa global `RECORDS`.
* **Correção Exigida**: Reutilizar um handle filho associado ao `HandleResult` (liberado quando o `HandleResult` ou o `Handle` é destruído) e garantir liberação adequada no ciclo de vida de `C.Handle`.

### ISS-05 — [ALTO] Vazamento de Registros Globais em Retorno Antecipado no Interpretador (`bn_lib_ffi`)

* **Status**: RESOLVED
* **Severidade**: Alta (Vazamento de Memória / Fragilidade de Ponteiro)
* **Tarefa no Bucket**: `D.2`
* **Localização**: [`call_invoke`](file:///Users/caq/src/bn/basicnext/crates/bn_lib_ffi/src/lib.rs#L594-L643).
* **Causa Raiz**: Durante a montagem dos argumentos em `call_invoke`, cada `C.*Array` aloca um registro temporário em `bn_rt::ffi::RECORDS`. Se um argumento posterior falhar na validação (linhas 599, 603 ou 619), a função retorna antes de executar o laço de `release_record` na linha 639. Adicionalmente, `let data_ptr = buf.as_mut_ptr();` (linha 607) é obtido antes de mover `buf` para `temp_array_records`.
* **Correção Exigida**: Introduzir um guard RAII (`Drop`) para liberar todos os registros temporários em qualquer saída da função e extrair o ponteiro após estabilizar o buffer no vetor.

---

## Média Severidade (Condições de Corrida, Concorrência e Conformidade de ABI)

### ISS-06 — [MÉDIO] Condição de Corrida (TOCTOU e `dlclose` Concorrente) no Carregador Dinâmico (`ffi_loader.rs`)

* **Status**: RESOLVED
* **Severidade**: Média (Condição de Corrida / Violação da Especificação)
* **Tarefa no Bucket**: `C.5`
* **Localização**: [`resolve_library`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_loader.rs#L54-L60) e [`cache_library`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_loader.rs#L112-L120).
* **Causa Raiz**: Duas threads que resolvam a mesma biblioteca simultaneamente verão cache miss em `resolve_library`, carregarão `Library::new` em paralelo e a segunda sobrescreverá a entrada em `cache.insert(key, Arc::clone(&lib))`, causando `Drop` (`dlclose` / `FreeLibrary`) da primeira instância quando a primeira thread concluir, violando a regra de [`bnffi.md`](file:///Users/caq/src/bn/basicnext/language/0.6/bnffi.md#L120) de que bibliotecas nunca são descarregadas.
* **Correção Exigida**: Usar `entry(key).or_insert_with(...)` no momento da inserção no cache e, caso outra thread já tenha inserido a biblioteca durante a corrida, reter ou esquecer (`std::mem::forget`) a instância duplicada ou retornar a instância canônica já registrada antes de resolver símbolos.

### ISS-07 — [MÉDIO] Colisão de Chave de Cache em `LIBRARIES` e `SYMBOLS` Ignorando `ffi_paths`

* **Status**: RESOLVED
* **Severidade**: Média (Isolamento de Execução / Resolução Incorreta)
* **Tarefa no Bucket**: `C.6`
* **Localização**: [`LIBRARIES`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_loader.rs#L22) e [`SYMBOLS`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_loader.rs#L26).
* **Causa Raiz**: As tabelas `LIBRARIES` e `SYMBOLS` indexam apenas pelo nome lógico da biblioteca (`String`), ignorando a lista de diretórios `ffi_paths` ativa na chamada.
* **Correção Exigida**: Incluir os diretórios de busca (`ffi_paths`) na chave de cache de resolução de bibliotecas e símbolos.

### ISS-08 — [MÉDIO] Pânico de Processo por Envenenamento de Mutex (`Mutex Poisoning`)

* **Status**: RESOLVED
* **Severidade**: Média (Resiliência de Concorrência)
* **Tarefa no Bucket**: `C.7`
* **Localização**: [`ffi.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi.rs#L159-L176), [`ffi.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi.rs#L859-L890) e [`ffi_loader.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_loader.rs#L55-L220).
* **Causa Raiz**: Uso de `.lock().expect(...)` em todos os mutexes globais de FFI (`RECORDS`, `LIBRARIES`, `SYMBOLS`).
* **Correção Exigida**: Substituir todas as aquisições de lock por `.unwrap_or_else( std::sync::PoisonError::into_inner )`.

### ISS-09 — [MÉDIO] Ausência de Verificação de Faixa (`C.VALUE_OUT_OF_RANGE`) para `Long`, `ULong` e `SizeT` em Alvos 32 bits

* **Status**: RESOLVED
* **Severidade**: Média (Conformidade com Especificação / Truncamento Silencioso)
* **Tarefa no Bucket**: `C.8`
* **Localização**: [`call_c_function_with_args`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi.rs#L417-L448).
* **Causa Raiz**: A validação de faixa de `TYPE_LONG` e `TYPE_ULONG` usa `#[cfg(target_os = "windows")]` em vez de verificar os limites reais de `c_long` e `c_ulong` da plataforma, e `TYPE_SIZE_T` faz cast direto `v as usize` sem verificar overflow em plataformas de 32 bits.
* **Correção Exigida**: Utilizar `std::os::raw::c_long::try_from(v)`, `std::os::raw::c_ulong::try_from(v)` e `usize::try_from(v)` para retornar `ERR_VALUE_OUT_OF_RANGE` de forma portátil em qualquer arquitetura (LP64, LLP64, ILP32).

---

## Baixa Severidade e Qualidade de Código (Clean Code / `GEMINI.md`)

### ISS-10 — [QUALIDADE] Função Monolítica de 500 Linhas e 16 Vetores Paralelos em `call_c_function_with_args`

* **Status**: RESOLVED
* **Severidade**: Baixa (Manutenibilidade / Risco de Desalinhamento de Índices)
* **Tarefa no Bucket**: `E.1`
* **Localização**: [`call_c_function_with_args`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi.rs#L226-L725).
* **Causa Raiz**: Uso de 16 vetores `Vec` separados e 15 contadores de índice manuais em duas passagens para alimentar `libffi`, totalizando 500 linhas na mesma função.
* **Correção Exigida**: Refatorar o armazenamento temporário de argumentos FFI em uma estrutura coesa por argumento e dividir a preparação de argumentos, execução `ffi_call` e gravação de resultado em funções menores e legíveis, removendo a necessidade de `#![allow(clippy::too_many_lines)]`.

### ISS-11 — [QUALIDADE] Duplicação de Constantes `TYPE_*` entre `bn_rt` e `bn_llvm`

* **Status**: RESOLVED
* **Severidade**: Baixa (Violação de DRY)
* **Tarefa no Bucket**: `E.2`
* **Localização**: [`crates/bn_rt/src/ffi.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi.rs#L47-L80) e [`crates/bn_llvm/src/llvm/ffi_calls.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/ffi_calls.rs#L13-L43).
* **Causa Raiz**: As 29 constantes `TYPE_INT8..TYPE_ARGS` estão duplicadas nos dois arquivos.
* **Correção Exigida**: Centralizar os códigos de contrato FFI ou garantir um teste de contrato cruzado / fonte única compartilhada.

### ISS-12 — [QUALIDADE] Falha Silenciosa em `bn_rt_ffi_set_string`

* **Status**: RESOLVED
* **Severidade**: Baixa (Robustez de Diagnóstico)
* **Tarefa no Bucket**: `E.3`
* **Localização**: [`bn_rt_ffi_set_string`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/ffi_abi.rs#L110-L113).
* **Causa Raiz**: `c_str_to_string(val).unwrap_or_default()` converte silenciosamente um ponteiro inválido em string vazia `""`.
* **Correção Exigida**: Tratar ponteiro nulo/inválido de forma consistente com `bn_rt_ffi_new_string` e registrar erro ou rejeitar entradas inválidas.

---

## Testes de Snapshot e Gold Files do Pipeline de CI (`v0.6.7`)

### ISS-13 — [CI] Cinco Falhas de Snapshot e Gold Files no GitHub Actions (`run 40081105564`)

* **Status**: RESOLVED
* **Severidade**: Alta (Bloqueio de Publicação dos Binários `v0.6.7`)
* **Tarefa no Bucket**: `F.1`, `F.2`, `F.3`, `F.4`
* **Localização**:
* [`tests/gold/llvm_typed_cfg.ll`](file:///Users/caq/src/bn/basicnext/tests/gold/llvm_typed_cfg.ll) e [`tests/codegen/codegen_tests.rs`](file:///Users/caq/src/bn/basicnext/tests/codegen/codegen_tests.rs#L195) (`lower_module_emits_contract_types_and_cfg`)
* [`tests/gold/llvm_i32_overflow.ll`](file:///Users/caq/src/bn/basicnext/tests/gold/llvm_i32_overflow.ll) e [`tests/codegen/codegen_tests.rs`](file:///Users/caq/src/bn/basicnext/tests/codegen/codegen_tests.rs#L236) (`lower_module_emits_i32_overflow_trap`)
* [`tests/frontend/ir_names.rs`](file:///Users/caq/src/bn/basicnext/tests/frontend/ir_names.rs) (`backends_only_decode_documented_name_shapes`)
* [`tests/programs/cli_snapshots.rs`](file:///Users/caq/src/bn/basicnext/tests/programs/cli_snapshots.rs) (`help_text_is_unchanged` e `option_errors_are_unchanged`)
* **Causa Raiz**: A adição das declarações `@bn_rt_ffi_*` no preâmbulo LLVM, da decodificação de nomes FFI e da flag `--ffi-path <dir>` no texto de ajuda do CLI alterou a saída esperada por esses 5 testes de regressão.
* **Correção Exigida**: Atualizar os arquivos gold e snapshots de CLI, adequar o gate `ir_names` às convenções documentadas e reexecutar a bateria completa antes de publicar a tag `v0.6.7`.
