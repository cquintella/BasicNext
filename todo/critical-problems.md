# Levantamento de Rotinas Inacabadas, Stubs e Lacunas de Implementação

Pesquisa sistemática realizada na base de código (`crates/`, `src/`, `modules/`, `language/`, `todo/`, `ongoing/`).

Não há macros `todo!` ou `unimplemented!` soltas no código de produção. No entanto, há rotinas incompletas, declarações sem correspondência no backend nativo, stubs em protocolos de ferramentas e lacunas normativas registradas.

---

## 1. Lacunas de Suporte no Backend Nativo `bnc` (LLVM)

O compilador nativo rejeita deliberadamente ou deixa de implementar um conjunto de rotinas aceitas pelo interpretador `bni`:

* **`BNWeb` (Ausência total de backend nativo):**
* Nenhuma função ou método de `BNWeb` é compilado por `bnc`.
* Toda chamada a `BNWeb` cai no filtro de rejeição `crates/bn_llvm/src/llvm/helpers.rs:375-379` e é rejeitada com `TARGET_UNSUPPORTED_OP`.

* **`BNDispatch` (Métodos de `Ticket` não emitidos):**
* Em `modules/bn/BNDispatch.bn:22-43`, os métodos `Ticket.Id()`, `Ticket.Status()`, `Ticket.Wait()`, `Ticket.Cancel()`, `Ticket.Error()` e `Ticket.IsDone()` são declarados e funcionam em `bni`.
* No `bnc`, esses métodos não foram adicionados à lista de chamadas suportadas em `crates/bn_llvm/src/llvm/analysis_calls.rs:22-40`.
* A função C ABI `bn_rt_dispatch_cancel` está declarada no preâmbulo LLVM em `crates/bn_llvm/src/llvm/runtime.rs:182`, mas nunca é emitida.

* **`HOST` (Operações nativas ausentes documentadas em `todo/bucket-0.6.2f-native-host-and-cleanup.md`):**
* `HOST.NumProcs`: Não suportado nativamente.
* Predicados de `HOST.Net.Address`: `IsIPv4`, `IsIPv6`, `IsLoopback`, `IsPrivate`, `IsLinkLocal`, `IsMulticast` existem em `bn_rt::net::addressing`, mas `bnc` rejeita com `TARGET_UNSUPPORTED_HOST`.
* `HOST.Net.CIDR`: `Net.CIDR.Parse` e métodos da classe `CIDR` não são emitidos nativamente.
* `HOST.Net.TCPStream`: `SetTimeouts`, `ShutdownRead` e `ShutdownWrite` não são emitidos nativamente.
* `HOST.Random`: `R.Random()` em modo nativo exige que `R.Seed` esteja na mesma função para ser compilado.

* **Funções de Usuário:**
* O backend nativo rejeita chamadas a funções gerais definidas pelo usuário com `TARGET_UNSUPPORTED_OP` em `crates/bn_llvm/src/llvm/helpers.rs:363-366`, aceitando apenas chamadas inlined ou intrinsics dos módulos autorizados.

---

## 2. Inconsistências e Comportamentos Divergentes (Bugs e Stubs Semânticos)

Defeitos e lacunas de rotina mapeados nos arquivos de controle do projeto:

* **Declaração `Error` sem inicializador:**
* `LET e AS Error` sem valor inicial é aceito por `bni` (inicializando com código `0`), mas gera pânico no compilador nativo `bnc` em `crates/bn_llvm/src/llvm/emission1.rs:204` (`unreachable!("validated scalar default type")`).

* **Resolução de `EXPORT CONST` dentro do próprio módulo:**
* Um módulo que lê sua própria constante `EXPORT CONST` por nome direto passa em `bni check`, mas no `bni run` aborta com `UNINITIALIZED_VALUE`, e no binário nativo gera valor `0`.

* **Aliases de Builtins/Funções `HOST`:**
* `IMPORT HOST.NumProcs AS N` seguido de `N()` passa no `check`, mas causa `UNINITIALIZED_VALUE` em `bni` e trava com status 133 no executável nativo.

* **Colisão no analisador de abertura de arquivos:**
* O analisador semântico aplica a checagem de modo de abertura de `HOST.FileSystem.Open` a qualquer método chamado `Open` em qualquer classe do usuário ou módulo externo.

---

## 3. Protocolos de Ferramentas (DAP e LSP) com Comandos Não Implementados

* **Debug Adapter Protocol (`bn_dap`):**
* Em `crates/bn_dap/src/lib.rs:175-195`, os únicos comandos DAP implementados são `scopes`, `variables`, `evaluate`, `disconnect`, `terminate`, `setBreakpoints` e `setExceptionBreakpoints`.
* Qualquer outro comando padrão do protocolo cai na cláusula `_ => (false, json!({}), Some("request is not implemented".into()))`.

* **Language Server Protocol (`bn_lsp`):**
* Em `crates/bn_lsp/src/lib.rs:177-186`, requisições não mapeadas do protocolo retornam o erro JSON-RPC `-32601` (`method not implemented`).

---

## 4. Arquivos de Assinatura nos Módulos da Linguagem (`modules/bn/*.bn`)

* Os arquivos `modules/bn/*.bn` (como `BNWeb.bn`, `BNData.bn`, `BNDispatch.bn`, `BNLog.bn`) possuem funções e métodos com corpos declarativos mínimos (`RETURN 0`, `RETURN ""`, `RETURN`, `END FUNCTION`).
* Isso faz parte da arquitetura: eles servem exclusivamente como tabelas de tipagem para o frontend da linguagem, enquanto o código real reside nos provedores Rust (`bn_lib_*`) ou no runtime nativo (`bn_rt`).
* No entanto, quando um método é adicionado em `modules/bn/*.bn` sem que ambos os backends o implementem (como ocorreu historicamente com `Ticket.Cancel` ou `BNWeb`), o método fica utilizável apenas no interpretador, atuando como um protótipo não finalizado no compilador.

---

## 5. Resíduos de Documentação e Nomenclatura do Executável Antigo

* **Documentação obsoleta em `src/README.md`:**
* O arquivo `src/README.md` documenta um layout monolítico de 2024/2025 que não existe mais (`src/main.rs`, `src/lexer.rs`, `src/parser.rs`, etc.), enquanto o código real foi migrado para `crates/*`.

* **Menções residuais ao binário unificado `bn`:**
* Conforme registrado em `todo/bucket-0.6.2f-native-host-and-cleanup.md`, mensagens de ajuda em `bn_interpret_driver/src/eval.rs:110`, `bn_cli/src/options.rs:149`, dez arquivos em `examples/` e `scripts/install.sh:262` ainda faziam referência a `bn run` ou `bn --help` em vez dos comandos separados `bni` e `bnc` (os arquivos de `examples/` foram atualizados para `bni`).
