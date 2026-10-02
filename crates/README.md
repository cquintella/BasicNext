# Basic Next Workspace Crates

Este diretório contém os crates modulares que compõem o compilador, interpretador, runtime e ferramentas de suporte do Basic Next.
A arquitetura do projeto impõe um fluxo de compilação estritamente desacoplado e unidirecional:

`Source -> Frontend -> Lowering -> BN IR -> validate -> interpret(IR) | compile(IR)`

---

# Visão Geral dos Crates

## 1. Núcleo e Pipeline de Compilação

### bn_source
* **Propósito**: Gerenciamento de código-fonte, spans de caracteres e resolução de coordenadas de texto.
* **Componentes principais**: Estruturas `SourceFile`, `Span`, identificadores de arquivo `FileId` e utilitários para tradução de deslocamentos de bytes em números de linha e coluna (1-indexed).
* **Papel no pipeline**: Base textual compartilhada por todo o compilador, interpretador, analisadores semânticos e servidores LSP para relatar diagnósticos precisos.

### bn_diag
* **Propósito**: Sistema unificado de diagnósticos, emissão de avisos e erros estruturados.
* **Componentes principais**: Tipos `Diagnostic`, `Severity`, `DiagnosticSink` e códigos de diagnóstico padronizados (ex: `E0101`, `E0201`, `E0450`).
* **Integração**: Integração nativa com o motor de localização Fluent (`share/bn/diagnostics/`), validação contra o registro canônico (`tests/registry-golden.tsv`) e sobreposição de políticas configuráveis (`--warn`, `--deny`, `--allow`).

### bn_types
* **Propósito**: Definição do sistema de tipos estático fundamental da linguagem.
* **Componentes principais**: Enum `Type` e variantes como `IntegerType` (I8 a I64, U8 a U64), `FloatType` (F32, F64), `ClassType`, `InterfaceType`, `PointerType`, `AlternativeType` e `ErrorType`.
* **Papel no pipeline**: Define as regras estáticas de compatibilidade, sub-tipagem nominal, larguras de bytes escalares e mapeamento canônico de códigos de erro de bibliotecas padrão (`ErrorCodes`).

### bn_value
* **Propósito**: Modelo de valores dinâmicos em tempo de execução para avaliação direta e interpretação.
* **Componentes principais**: Enum `Value`, ponteiros com contagem de referências (`Arc`), representações escalares, cadeias de caracteres UTF-8, dicionários de objetos e vetores multidimensionais.
* **Papel no pipeline**: Fornece o substrato de dados manipulado pelo interpretador e pelas rotinas de conversão e exibição textual.

### bn_ir
* **Propósito**: Representação intermediária tipada (BN IR) em forma de grafo de fluxo de controle (CFG) e forma de atribuição estática única (SSA).
* **Componentes principais**: Módulos `IrModule`, funções `IrFunction`, blocos básicos `BasicBlock`, instruções tipadas `Instruction` e validador formal independente de alvo `validate`.
* **Papel no pipeline**: É a barreira canônica de validação da linguagem. Nenhum código é emitido ou interpretado sem antes passar com sucesso por `bn_ir::validate`.

### bn_frontend
* **Propósito**: Pipeline completo de front-end, análise léxica, sintática e semântica.
* **Componentes principais**: Lexer (`lexer`), parser, árvore de sintaxe abstrata (AST), resolução do grafo de módulos (`module_graph`), analisadores semânticos de múltiplas passagens (`analyzer1` a `analyzer8`) e rebaixamento para IR (`lowering`).
* **Papel no pipeline**: Lê o código-fonte bruto e arquivos importados, valida regras léxicas/sintáticas/semânticas e constrói o modelo `bn_ir::IrModule`.

---

## 2. Motores de Execução e Backends

### bn_runtime
* **Propósito**: Motor de execução e interpretação direta do modelo de IR.
* **Componentes principais**: Alocador de pilha/heap para o interpretador, gerenciamento de escopos de chamada e despachante direto de instruções IR.
* **Papel no pipeline**: Serve como substrato fundamental para a execução de funções e avaliação rápida de expressões.

### bn_interp
* **Propósito**: Coordenação da execução do interpretador de referência e laço de avaliação.
* **Componentes principais**: Laço avaliador `Interpreter`, controle de chamadas intrínsecas, resolução de provedores de biblioteca e emulação semântica exata.
* **Papel no pipeline**: Implementa o comportamento de referência da linguagem; qualquer divergência de execução em relação ao código nativo compilado é considerada um defeito de conformidade.

### bn_rt
* **Propósito**: Biblioteca estática de tempo de execução nativo (`libbn_rt.a` em Unix e `bn_rt.lib` em Windows) com interface C ABI.
* **Componentes principais**: Estruturas de suporte a `DataFrame`, drivers nativos para SQLite 3, subsistema de sockets de rede assíncronos (`mio`), inicialização e confinamento de sandbox (`bn_rt_policy_init`), gerenciamento de sinais e alocação de memória segura.
* **Papel no pipeline**: Vinculado estaticamente pelo backend LLVM aos binários compilados nativos para fornecer acesso aos recursos de sistema e bibliotecas padrão.

### bn_llvm
* **Propósito**: Backend de geração de código otimizado utilizando LLVM (`inkwell`).
* **Componentes principais**: Módulos de emissão de instruções (`emission1`), validação de suporte a alvos (`analysis_validate`), emissão de chamadas e objetos, pontes com o runtime nativo `bn_rt` e orquestração de Clang/LLVM.
* **Alvos suportados**: Executáveis nativos para a plataforma hospedeira (x86_64, aarch64) e módulos WebAssembly (`wasm32`).

---

## 3. Provedores de HOST e Bibliotecas Padrão

### bn_host_exec
* **Propósito**: Suporte a execução de processos filhos externos (`HOST.Exec`).
* **Componentes principais**: Controle de ciclo de vida de subprocessos, limites de captura de saída (`stdout`/`stderr`), controle de timeouts de execução e conversão de sinais/códigos de saída.

### bn_host_fs
* **Propósito**: Suporte a operações de sistema de arquivos hospedeiro (`HOST.FileSystem`).
* **Componentes principais**: Criação, leitura, escrita, travamento atômico de arquivos, navegação de diretórios e aplicação rigorosa das restrições de sandbox (`--read-root`, `--write-root`).

### bn_host_net
* **Propósito**: Suporte a conectividade de rede e endereçamento IP (`HOST.Net`).
* **Componentes principais**: Clientes e servidores TCP (`TCPStream`, `TCPListener`), sockets UDP (`UDPSocket`), resolução de DNS, cálculo de sub-redes CIDR e predicados de classificação de endereços IPv4/IPv6.

### bn_lib_crypto
* **Propósito**: Módulo de segurança e criptografia (`BNCrypto`).
* **Componentes principais**: Funções hash criptográficas (SHA-256, SHA-512, SHA3-256), HMAC, derivação de chaves HKDF, cifras autenticadas AEAD (ChaCha20-Poly1305, AES-256-GCM) e assinaturas pós-quânticas (ML-DSA / Dilithium).

### bn_lib_data
* **Propósito**: Manipulação e análise estruturada de dados (`BNData`).
* **Componentes principais**: Implementação em memória de `DataFrame`, vetores colunares fortemente tipados, transformações de projeção/filtragem e parser/gerador de arquivos CSV.

### bn_lib_dispatch
* **Propósito**: Orquestração de concorrência e tarefas paralelas (`BNDispatch`).
* **Componentes principais**: Pools de threads trabalhadoras, filas de despacho de funções, retorno assíncrono controlado por bilhetes (`Ticket`) e primitivas de sincronização com controle de timeout.

### bn_lib_json
* **Propósito**: Manipulação, análise sintática e serialização de documentos JSON (`BNJson`).
* **Componentes principais**: DOM estruturado para objetos e arrays, serialização determinística, parser com suporte a UTF-8 e mapeamento para tipos de domínio (companion pattern).

### bn_lib_log
* **Propósito**: Subsistema de logging estruturado de alto desempenho (`BNLog`).
* **Componentes principais**: Canais de saída independentes (console e arquivo), controle de severidade (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`), formatação textual estruturada ou em envelopes JSON.

### bn_lib_math
* **Propósito**: Funções matemáticas e constantes numéricas (`BNMath`).
* **Componentes principais**: Operações escalares e trigonométricas avançadas, álgebra de vetores de ponto flutuante, manipulação de matrizes e geradores de números pseudoaleatórios.

### bn_lib_sqlite
* **Propósito**: Integração nativa com banco de dados embutido SQLite 3 (`BNSqlite`).
* **Componentes principais**: Abertura de bancos de dados em arquivo e em memória (`:memory:`), execução de comandos DDL/DML, controle explícito de transações (`Begin`, `Commit`, `Rollback`), consultas parametrizadas e conversão de resultados para `DataFrame`.

### bn_lib_web
* **Propósito**: Utilitários para comunicação Web e HTTP (`BNWeb`).
* **Componentes principais**: Cliente HTTP para requisições GET/POST, manipulação de cabeçalhos e decodificação/codificação de URLs (atualmente suportado no interpretador).

### bn_limits
* **Propósito**: Definição centralizada de limites de recursos e tetos de segurança.
* **Componentes principais**: Limites de profundidade de recursão, capacidades máximas de vetores/matrizes, limites de tamanho de payloads de captura e tamanhos de buffers em memória para evitar esgotamento de recursos.

---

## 4. Drivers e Executáveis do Toolchain

### bn_cli
* **Propósito**: Infraestrutura compartilhada de interface de linha de comando.
* **Componentes principais**: Analisador unificado de opções CLI, tratamento de arquivos de configuração (`config.toml`), detecção de terminais com suporte a cores e resolução de caminhos padrão.

### bn_interpret_driver
* **Propósito**: Driver de orquestração do fluxo de execução do interpretador.
* **Componentes principais**: Configura as sessões de front-end, resolve a inclusão de bibliotecas, prepara as capacidades hospedeiras sob política de sandbox e aciona o interpretador para os comandos `run`, `eval`, `check` e `lex`.

### bn_compile_driver
* **Propósito**: Driver de orquestração do fluxo de compilação nativa.
* **Componentes principais**: Valida o suporte da arquitetura de destino, invoca a geração de código LLVM, gerencia a chamada de ferramentas externas (Clang, lld, wasm-ld), registra logs de compilação complementares e assegura a limpeza de artefatos temporários em caso de falha.

### bn_lsp
* **Propósito**: Servidor do protocolo Language Server Protocol (LSP) para IDEs e editores.
* **Componentes principais**: Comunicação JSON-RPC via stdio, publicação contínua de diagnósticos em tempo de edição, sincronização de documentos e resolução de símbolos.

### bn_dap
* **Propósito**: Servidor do protocolo Debug Adapter Protocol (DAP) para integração com depuradores.
* **Componentes principais**: Tratamento de requisições de depuração via stdio, configuração de pontos de parada, inspeção de variáveis e controle do fluxo de execução.

### bni
* **Propósito**: Executável principal unificado do interpretador e ferramentas de front-end.
* **Entrada**: Binário interativo `bni <subcomando> [opções] <arquivo.bn>` com suporte a execução direta padrão (`bni <arquivo.bn>`).

### bnc
* **Propósito**: Compilador Ahead-of-Time (AOT) para geração de executáveis nativos e WebAssembly.
* **Entrada**: Binário de compilação `bnc [opções] <entrada.bn> -o <saída>`.

---

## Regras Arquiteturais Obrigatórias

* **Especificação Precede a Implementação**: Todos os crates seguem rigorosamente os contratos documentados em `docs/architecture/` e as especificações ativas em `language/0.6/`.
* **IR Canônica Única**: Tanto o interpretador (`bn_runtime`/`bn_interp`) quanto o compilador nativo (`bn_llvm`) operam sobre a mesma representação validada produzida por `bn_ir::validate`.
* **Grafo de Dependências Acíclico**: Não são permitidas dependências circulares entre crates. A hierarquia estrita é verificada continuamente pelo script `scripts/check-forbidden-deps.sh`.
