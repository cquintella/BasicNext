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

## Lacunas no Suporte Nativo a Recursos HOST
* O compilador nativo rejeita chamadas mapeadas em [todo/bucket-0.6.2f-native-host-and-cleanup.md](file:///Users/caq/src/bn/basicnext/todo/bucket-0.6.2f-native-host-and-cleanup.md#L10-L20) com `TARGET_UNSUPPORTED_HOST`:
* `HOST.NumProcs` não possui emissão LLVM.
* Predicados de endereço de rede `IsIPv4`, `IsIPv6`, `IsLoopback`, `IsPrivate`, `IsLinkLocal` e `IsMulticast` existem no runtime [bn_rt](file:///Users/caq/src/bn/basicnext/crates/bn_rt), mas não são aceitos no backend nativo.
* A classe `HOST.Net.CIDR` e o método estático `Parse` não são emitidos em código nativo.
* Métodos `SetTimeouts`, `ShutdownRead` e `ShutdownWrite` em `HOST.Net.TCPStream` não possuem emissão nativa.
* A função `HOST.Random.Random()` exige no modo nativo que `HOST.Random.Seed` resida no mesmo corpo de função.

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

## Pânico no Compilador LLVM com Tipos Padrão Escalares
* Em [analysis_validate.rs](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/analysis_validate.rs#L38), qualquer tipo escalar cujo [llvm_type](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/lib.rs#L507-L570) retorne `Some` é validado como aceito para a instrução [Instruction::Default](file:///Users/caq/src/bn/basicnext/crates/bn_ir/src/model.rs).
* Entretanto, em [emission1.rs](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/emission1.rs#L156-L220), o casamento de padrões trata apenas tipos primitivos e um subconjunto de tuplas, alcançando `unreachable!("validated scalar default type")` para representações válidas como `"{ i1, i32 }"`, `"{ i1, ptr }"` e `"{ i1, ptr, i32 }"`. Declarações locais sem valor inicial nesses formatos causam pânico e aborto do compilador.

## Confusão de Largura de Inteiros no Módulo SQLite
* Em [crates/bn_lib_sqlite/src/lib.rs](file:///Users/caq/src/bn/basicnext/crates/bn_lib_sqlite/src/lib.rs#L251), ao converter registros de consultas SQL para [Value::Integer](file:///Users/caq/src/bn/basicnext/crates/bn_value/src/lib.rs#L103), inteiros de 64 bits de `SQLITE_INTEGER` são instanciados como [IntegerType::Int32](file:///Users/caq/src/bn/basicnext/crates/bn_types/src/lib.rs).
* Qualquer chave primária, timestamp ou identificador numérico que exceda $2^{31}-1$ fica com tipo declarado inconsistente, provocando traps de overflow ou truncamento em rotinas aritméticas do runtime. A tipagem correta é `IntegerType::Int64`.

## Cast Inseguro de Sinal em Contadores C FFI
* Em [crates/bn_rt/src/sqlite_abi.rs](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/sqlite_abi.rs#L481-L486) e [crates/bn_rt/src/sqlite_abi.rs](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/sqlite_abi.rs#L493-L499), a rotina `sqlite3_column_bytes` retorna `c_int` (i32 assinado), e o código aplica cast direto `as usize`.
* Retornos negativos de erro no SQLite são promovidos a valores próximos a $2^{64}-1$. Ao repassar esse comprimento para [std::slice::from_raw_parts](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/sqlite_abi.rs#L486), ocorre violação de memória imediata (Segmentation Fault). O comprimento deve ser validado via `usize::try_from(count).unwrap_or(0)`.

## Armazenamento Incorreto de Dados SQLITE_BLOB
* Em [crates/bn_rt/src/sqlite_abi.rs](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/sqlite_abi.rs#L490-L503), colunas do tipo BLOB são armazenadas no enum de transporte como [StoredValue::String](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/dataframe_abi.rs#L68) em vez de [StoredValue::Bytes](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/dataframe_abi.rs#L69).
* Dados binários arbitrários contendo bytes nulos ou sequências não-UTF-8 podem sofrer corrupção de payload quando processados em consumidores de cadeias de caracteres.

---

# 3. Falhas e Vulnerabilidades de Segurança

## Escape de Sandbox e Condição de Corrida TOCTOU no Driver SQLite
* Enquanto o módulo [HOST.FileSystem](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/secure_fs.rs) utiliza descritores abertos e travados (`openat` com flag `O_NOFOLLOW` e validação comparativa de `dev` e `ino`), a rotina [open_connection](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/sqlite_abi.rs#L137-L165) repassa a string de caminho diretamente para a biblioteca C SQLite via [Connection::open_with_flags](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/sqlite_abi.rs#L165).
* A validação prévia em [allows_path](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/policy.rs#L150) ocorre desacoplada da abertura física, permitindo ataques de substituição de links simbólicos (TOCTOU).
* O motor SQLite gera arquivos complementares (`-wal`, `-shm`, `-journal`) cujos caminhos não são submetidos às regras de confinamento do sandbox.

## Risco de Deadlock em Subprocessos no HOST.Exec.Run
* Em [crates/bn_host_exec/src/lib.rs](file:///Users/caq/src/bn/basicnext/crates/bn_host_exec/src/lib.rs#L187-L211), as threads coletoras de saída `out_thread.join()` e `err_thread.join()` são aguardadas de forma síncrona após o processo filho ser terminado.
* Caso o processo filho tenha gerado processos netos que herdaram seus descritores de saída ou erro padrão, os pipes permanecem abertos no sistema operacional. Consequentemente, as threads leitoras não atingem EOF e a chamada `run` trava indefinidamente, neutralizando o timeout configurado.

## Alocação Artificial de Memória em Overflow de Captura
* Em [crates/bn_host_exec/src/lib.rs](file:///Users/caq/src/bn/basicnext/crates/bn_host_exec/src/lib.rs#L119-L123), ao detectar overflow no pipe de captura, a função [read_pipe](file:///Users/caq/src/bn/basicnext/crates/bn_host_exec/src/lib.rs#L96) aloca até 16 MiB de bytes zerados exclusivamente para reportar o comprimento sentinela (`capture_limit + 1`).
* Sob tempestade de dados ou tentativas de sobrecarga de memória via subprocessos, essa alocação induz pressão desnecessária sobre o alocador de páginas do sistema.

## Descompasso no Estado de Transações de Banco de Dados
* Em [crates/bn_rt/src/sqlite_abi.rs](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/sqlite_abi.rs#L538-L595), o controle transacional depende unicamente do campo booleano `in_transaction`.
* Comandos diretos de controle de transação executados via `Exec("BEGIN")`, `Exec("ROLLBACK")` ou comandos que falhem e disparem rollback implícito do motor SQLite deixam a flag interna desincronizada.
* O estado da transação deve ser inspecionado diretamente no motor por meio de `sqlite3_get_autocommit`.

---

# 4. Falta de Boas Práticas e Débito Técnico

## Vulnerabilidade a Envenenamento de Mutex (Mutex Poisoning)
* Em [crates/bn_rt/src/sqlite_abi.rs](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/sqlite_abi.rs#L119) e múltiplos pontos de manipulação de tabelas globais do runtime, o acesso aos locks utiliza `.lock().unwrap()`.
* Se uma thread entrar em pânico enquanto retém o mutex, qualquer acesso subsequente de outra thread ao runtime resulta em pânico imediato em cadeia. O padrão robusto requer recuperação via `.unwrap_or_else(PoisonError::into_inner)` ou conversão para erro reportável.

## Ausência de Configuração de Lints Centralizada no Workspace
* No arquivo raiz [Cargo.toml](file:///Users/caq/src/bn/basicnext/Cargo.toml#L22-L28), as regras `[lints.rust]` e `[lints.clippy]` estão atribuídas somente ao pacote raiz `bn`, e não na tabela global `[workspace.lints]`.
* Novos crates introduzidos no repositório podem deixar de herdar a negação rigorosa de código inseguro (`unsafe_code = "deny"`) e as diretivas pedantes de compilação.

## Menções Residuais ao Executável Obsoleto bn
* Vários pontos da base de código mantêm referências ao comando legado `bn`:
* Mensagem de erro do compilador em [crates/bn_llvm/src/llvm/helpers.rs](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/helpers.rs#L364): `(use 'bn run' or inline the call)`.
* Mensagens de ajuda de linha de comando em [crates/bn_interpret_driver/src/eval.rs](file:///Users/caq/src/bn/basicnext/crates/bn_interpret_driver/src/eval.rs#L110) e [crates/bn_cli/src/options.rs](file:///Users/caq/src/bn/basicnext/crates/bn_cli/src/options.rs#L149).
* Documentação de layout em [src/README.md](file:///Users/caq/src/bn/basicnext/src/README.md), descrevendo uma arquitetura monolítica obsoleta.
