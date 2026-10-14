# Relatório de Auditoria de Código: Basic Next (issues.md)

Relatório consolidado de auditoria de código cobrindo recursos inacabados, falhas e fragilidades no sistema de tipos, vulnerabilidades de segurança e desvios de boas práticas no compilador, interpretador e runtime do Basic Next.

---

# 1. Features Não Terminadas e Lacunas de Paridade

## Ausência Total de Backend Nativo para BNWeb
* No compilador nativo [bnc](file:///Users/caq/src/bn/basicnext/crates/bnc), todas as chamadas para o módulo `BNWeb` são explicitamente rejeitadas.
* Em [unsupported_call_detail](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/helpers.rs#L355-L380), qualquer chamada para `BNWeb` dispara `TARGET_UNSUPPORTED_OP`. O recurso opera exclusivamente no interpretador [bni](file:///Users/caq/src/bn/basicnext/crates/bni).

## Rejeição de Funções de Usuário no Backend LLVM
* O compilador nativo não suporta a compilação de chamadas a funções gerais definidas pelo usuário.
* Em [unsupported_call_detail](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/helpers.rs#L362-L367), chamadas não inlined são barradas com mensagem orientando a utilizar o interpretador.

## Lacunas no Suporte Nativo a Recursos HOST (Resolvido)
* Resolvido em `crates/bn_llvm/`: `HOST.NumProcs` (em `runtime_process.rs`), predicados de rede `IsIPv4`, `IsIPv6`, `IsLoopback`, `IsPrivate`, `IsLinkLocal`, `IsMulticast`, classe `HOST.Net.CIDR` com `Parse`, métodos `SetTimeouts`, `ShutdownRead`, `ShutdownWrite` em `TCPStream` (em `net_members.rs` e `runtime_net.rs`) e `HOST.Random` possuem emissão completa LLVM implementada e testada no backend nativo.

## Comandos Não Implementados no Protocolo DAP
* Em [crates/bn_dap/src/lib.rs](file:///Users/caq/src/bn/basicnext/crates/bn_dap/src/lib.rs#L170-L195), os comandos padrão de controle de depuração (`next`, `stepIn`, `stepOut`, `continue`, `pause`, `restart`) caem na cláusula curinga e retornam `"request is not implemented"`.

## Métodos Não Implementados no Servidor LSP
* Em [crates/bn_lsp/src/lib.rs](file:///Users/caq/src/bn/basicnext/crates/bn_lsp/src/lib.rs#L85-L101), requisições usuais do protocolo como formatação (`textDocument/formatting`), renomeação (`textDocument/rename`), ações de código (`textDocument/codeAction`) e dobramento de blocos (`textDocument/foldingRange`) retornam erro JSON-RPC `-32601` (`method not implemented`).

## Inconsistências de Resolução em Tempo de Execução
* Conforme documentado em [todo/bucket-0.6.2f-native-host-and-cleanup.md](file:///Users/caq/src/bn/basicnext/todo/bucket-0.6.2f-native-host-and-cleanup.md#L23-L30):
* Um módulo que consome sua própria constante `EXPORT CONST` pelo nome direto passa pelo `check`, mas gera `UNINITIALIZED_VALUE` no interpretador e retorna `0` no binário compilado.
* A instrução `IMPORT HOST.NumProcs AS N` seguida de `N()` passa pelo `check`, mas falha com `UNINITIALIZED_VALUE` no interpretador e finaliza com sinal 133 no binário nativo.

---

# 2. Problemas com Tipos e Conversões

## Pânico no Compilador LLVM com Tipos Padrão Escalares (Resolvido)
* Resolvido em `crates/bn_llvm/src/llvm/emission1.rs`: todas as representações agregadas e tuplas suportadas por `llvm_type` (`{ i1, ptr }` via `status_pointer_ty()`, `{ i1, ptr, i32 }` via `status_endpoint_ty()`, `{ i1, ptr, i64 }` via `handle_result_ty()`, `{ ptr, i32 }` via `vector_ty()` e `{ ptr, i32, i32 }` via `region_ty()`) possuem emissão de valor padrão campo a campo via `I::insert`, e tuplas gerais passam por `general_alternative::emit_default`.

## Confusão de Largura de Inteiros no Módulo SQLite (Resolvido)
* Resolvido em `crates/bn_lib_sqlite/src/lib.rs`: valores de `StoredValue::Integer(i)` são mapeados como `Value::Integer(i.into(), IntegerType::Int64)`, garantindo representação completa de 64 bits para chaves primárias e timestamps.

## Cast Inseguro de Sinal em Contadores C FFI (Resolvido)
* Resolvido em `crates/bn_rt/src/sqlite_abi.rs`: `raw_bytes` obtido de `sqlite3_column_bytes` é convertido de forma segura via `usize::try_from(raw_bytes).unwrap_or(0)` antes de criar slices em `std::slice::from_raw_parts`.

## Armazenamento Incorreto de Dados SQLITE_BLOB (Resolvido)
* Resolvido em `crates/bn_rt/src/sqlite_abi.rs`: colunas `SQLITE_BLOB` são salvas como `StoredValue::Bytes(slice.to_vec())`, preservando dados binários arbitrários sem conversão indevida para string.

---

# 3. Falhas e Vulnerabilidades de Segurança

## Escape de Sandbox e Condição de Corrida TOCTOU no Driver SQLite (Resolvido)
* Resolvido em `crates/bn_rt/src/sqlite_abi.rs`:
  * Abertura canônica com verificação estrita de path traversal (`..`) e bytes nulos.
  * Validação de caminhos de arquivos derivados/sidecar (`-wal`, `-shm`, `-journal`) contra a política do sandbox antes de permitir a abertura do banco de dados principal.
  * Mitigação TOCTOU com captura de identidade do arquivo `(dev, ino)` pré-abertura e verificação comparativa pós-abertura (`post_meta.dev() == expected_dev && post_meta.ino() == expected_ino`), abortando e retornando `POLICY_DENIED` sob substituição concorrente.
  * Eliminação de magic literal 5000ms via constante tipada `SQLITE_BUSY_TIMEOUT`.
  * Validação atestada pelo teste de regressão `sqlite_sandbox_symlink_escape_is_rejected`.

## Risco de Deadlock em Subprocessos no HOST.Exec.Run (Resolvido)
* Resolvido em `crates/bn_host_exec/src/lib.rs`: o leitor drena pipes com timeout vinculado à deadline do processo filho via canais `mpsc` com `recv_timeout` e bounded deadlines. Testado e validado em `tests::background_grandchild_pipe_does_not_deadlock_execution` e `tests::output_held_open_by_a_grandchild_is_kept`.

## Alocação Artificial de Memória em Overflow de Captura (Resolvido)
* Resolvido em `crates/bn_host_exec/src/lib.rs`: a flag booleana `overflow` sinaliza o estouro de limite sem alocar sentinelas de bytes vazios em memória (`bytes.clear()`), eliminando pressão no alocador de páginas.

## Descompasso no Estado de Transações de Banco de Dados (Resolvido)
* Resolvido em `crates/bn_rt/src/sqlite_abi.rs`: a função `is_in_transaction` inspeciona o estado de autocommit diretamente no motor via `sqlite3_get_autocommit(db_handle) == 0`, sincronizando com comandos SQL arbitrários.

---

# 4. Falta de Boas Práticas e Débito Técnico

## Vulnerabilidade a Envenenamento de Mutex (Mutex Poisoning) (Resolvido)
* Resolvido em `crates/bn_rt/src/sqlite_abi.rs`: a estrutura `ConnectionRegistry` encapsula o lock via método `.lock()`, utilizando `.unwrap_or_else(PoisonError::into_inner)` para recuperar o acesso ao mapa de conexões mesmo após pânicos em threads concorrentes. Validado com teste unitário `mutex_poison_recovery_in_connection_registry`.

## Ausência de Configuração de Lints Centralizada no Workspace (Resolvido)
* Resolvido em `Cargo.toml`: as seções `[workspace.lints.rust]` e `[workspace.lints.clippy]` estão configuradas globalmente no workspace com `unsafe_code = "deny"`, e o pacote raiz e demais crates herdam via `[lints] workspace = true`.

## Menções Residuais ao Executável Obsoleto bn (Resolvido)
* Resolvido: as mensagens de CLI e erros de compilação em `bn_llvm`, `bn_interpret_driver` e `bn_cli` foram migradas para o catálogo Fluent/textos ou eliminadas, referenciando exclusivamente `bni` e `bnc`.
