# Bucket 0.6.9-4 — Paridade Nativa em HOST.Net (Address Predicates, TCP Shutdown) e Conversão de Fuso Horário (TIMEZONE)

**Status:** Planned (`todo/bucket-0.6.9-4-native-net-gaps-and-timezone.md`)
**Repositório Alvo:** `/Users/caq/src/bn/basicnext`
**Dependências:** 0.6.8 fechado; pode ser executado em paralelo ou após os buckets de REPL da série 0.6.9.

**Restrições Arquiteturais (BDFL Locks):**
* **Paridade Estrita entre Interpretador e Compilador:** todo programa aceito por `bni` nas rotinas de rede e conversão de fuso horário deve compilar e produzir o mesmo resultado determinístico em `bnc` (`Native::Same`), conforme `docs/architecture/conformance.md`.
* **Zero Acoplamento Indevido:** o runtime nativo básico `crates/bn_rt` permanece livre de dependências externas pesadas; tabelas e algoritmos temporais residem em crate dedicado ou em `crates/bn_core_text` / `crates/bn_rt/src/civil.rs`, e o código C ABI adere a `language/0.6/value-memory-abi.md`.
* **Invariantes W1–W5 e Clean Code:** código novo em Rust com tipagem estrita, sem strings soltas nem números mágicos (constantes nomeadas nos módulos de domínio), respeitando orçamentos de linhas (`tests/source_size.rs`).

**Claim:** Eliminar as duas limitações históricas documentadas em `README.md` ("Known limitations"):
1. Habilitar no compilador nativo `bnc` a compilação e execução de todos os predicados de `HOST.Net.Address` (`IsIPv4`, `IsIPv6`, `IsLoopback`, `IsPrivate`, `IsLinkLocal`, `IsMulticast`) e métodos de encerramento parcial de conexões TCP de `HOST.Net.TCPStream` (`ShutdownRead`, `ShutdownWrite`), alcançando paridade total com o interpretador `bni`.
2. Especificar e implementar a conversão de fuso horário para o tipo temporal `TIMEZONE` e tipos correlatos (`TIMESTAMP`, `DATE`, `TIME`), suportando cálculo de offset de fuso IANA, transições de horário de verão e conversão entre fusos horários de forma idêntica em `bni` e `bnc`.
3. Remover as limitações de `README.md` e atualizar os testes de conformidade para `Native::Same`.

---

## Cluster A — Predicados de `HOST.Net.Address` no Compilador Nativo `bnc`

A lógica de predicados de endereço IP já existe em `crates/bn_rt/src/net/addressing.rs`, mas é rejeitada no frontend/LLVM com `TARGET_UNSUPPORTED_HOST`.

- [ ] **A.1** Expor os símbolos C ABI em [`crates/bn_rt/src/net/addressing.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/net/addressing.rs) para cada predicado de endereço:
  * `bn_rt_net_address_is_ipv4(handle: u64) -> i32`
  * `bn_rt_net_address_is_ipv6(handle: u64) -> i32`
  * `bn_rt_net_address_is_loopback(handle: u64) -> i32`
  * `bn_rt_net_address_is_private(handle: u64) -> i32`
  * `bn_rt_net_address_is_link_local(handle: u64) -> i32`
  * `bn_rt_net_address_is_multicast(handle: u64) -> i32`
- [ ] **A.2** Registrar os novos símbolos na tabela de runtime LLVM em [`crates/bn_llvm/src/llvm/runtime_abi.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/runtime_abi.rs) e [`crates/bn_llvm/src/llvm/runtime.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/runtime.rs), além do contrato normativo [`language/0.6/value-memory-abi.md`](file:///Users/caq/src/bn/basicnext/language/0.6/value-memory-abi.md).
- [ ] **A.3** Atualizar o analisador de chamadas e emissão em [`crates/bn_llvm/src/llvm/analysis_calls.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/analysis_calls.rs) e [`crates/bn_llvm/src/llvm/calls.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/calls.rs), removendo a rejeição `TARGET_UNSUPPORTED_HOST` e rebaixando as invocações de método em receptores `HOST.Net.Address` para os símbolos C ABI correspondentes.
- [ ] **A.4** Adicionar testes unitários em `crates/bn_rt` e testes de paridade em `tests/programs/parity.rs` cobrindo IPv4 (loopback, private, multicast, público) e IPv6 (loopback, link-local, global).

---

## Cluster B — Encerramento de Conexões TCP (`TCPStream.Shutdown*`) no `bnc`

`TCPStream.SetTimeouts` já foi suportado nativamente; faltam os métodos de desligamento seletivo de canal `ShutdownRead` e `ShutdownWrite`.

- [ ] **B.1** Implementar e exportar na C ABI de [`crates/bn_rt/src/net/tcp.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/net/tcp.rs):
  * `bn_rt_net_tcp_shutdown_read(handle: u64) -> i32`
  * `bn_rt_net_tcp_shutdown_write(handle: u64) -> i32`
  * Mapeando internamente para `std::net::TcpStream::shutdown(Shutdown::Read)` e `Shutdown::Write`, reportando erros de rede via `bn_rt_error_set_report`.
- [ ] **B.2** Registrar os símbolos em [`crates/bn_llvm/src/llvm/runtime_abi.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/runtime_abi.rs), [`crates/bn_llvm/src/llvm/runtime.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/runtime.rs) e [`language/0.6/value-memory-abi.md`](file:///Users/caq/src/bn/basicnext/language/0.6/value-memory-abi.md).
- [ ] **B.3** Habilitar a emissão de código nativo em `bn_llvm` para as chamadas a métodos `ShutdownRead` e `ShutdownWrite` em instâncias de `HOST.Net.TCPStream`.
- [ ] **B.4** Adicionar testes de integração de paridade em `tests/programs/parity.rs`: conexão cliente-servidor TCP local onde o emissor executa `ShutdownWrite`, o receptor consome até EOF e confirma recebimento completo, mantendo a via reversa aberta.

---

## Cluster C — Conversão de Fuso Horário (`TIMEZONE` Zone Conversion)

Atualmente `TIMEZONE` valida e preserva a string IANA canônica, mas a conversão temporal não está implementada.

- [ ] **C.1** Especificar normativamente em [`language/0.6/temporal.md`](file:///Users/caq/src/bn/basicnext/language/0.6/temporal.md) e [`language/0.6/0.6.md`](file:///Users/caq/src/bn/basicnext/language/0.6/0.6.md) a interface de conversão de fuso horário da linguagem:
  * Função estática `TimeZone.Offset(tz AS TIMEZONE, ts AS TIMESTAMP) AS INT` (retorna o deslocamento em segundos em relação a UTC no instante `ts`, considerando horário de verão).
  * Função estática `TimeZone.IsDST(tz AS TIMEZONE, ts AS TIMESTAMP) AS BOOLEAN` (indica se horário de verão está ativo no fuso no instante `ts`).
  * Função estática `Timestamp.AtZone(ts AS TIMESTAMP, tz AS TIMEZONE) AS STRING` (ou componentes civis locais formatados no fuso indicado).
  * Conversão de data e hora local em determinado fuso para `TIMESTAMP` UTC.
- [ ] **C.2** Implementar o motor de cálculo de offsets IANA e transições de horário de verão em módulo compartilhado (`crates/bn_core_text/src/temporal/timezone.rs` ou crate de domínio correspondente), consumindo a base canônica de dados de fuso sem alocação dinâmica excessiva nem dependências externas instáveis.
- [ ] **C.3** Integrar a resolução no interpretador `bni` ([`crates/bn_interp/src/temporal/`](file:///Users/caq/src/bn/basicnext/crates/bn_interp/src/temporal.rs)).
- [ ] **C.4** Implementar o runtime nativo C ABI em [`crates/bn_rt/src/temporal_abi.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_rt/src/temporal_abi.rs) e a respectiva emissão de código LLVM em [`crates/bn_llvm/src/llvm/temporal_emission.rs`](file:///Users/caq/src/bn/basicnext/crates/bn_llvm/src/llvm/temporal_emission.rs) (ou equivalente), garantindo paridade total de resultados entre `bni` e `bnc`.
- [ ] **C.5** Bateria de testes exaustiva para múltiplos fusos IANA canônicos (`UTC`, `America/Sao_Paulo`, `Europe/London`, `America/New_York`, `Asia/Tokyo`), cobrindo instantes limítrofes de transição de horário de verão e anos bissextos.

---

## Cluster D — Gates de Qualidade, Limpeza de Documentação e Fechamento

- [ ] **D.1** Atualizar [`tests/host_conformance.rs`](file:///Users/caq/src/bn/basicnext/tests/host_conformance.rs), migrando os testes de `Net.Address` e `TCPStream.Shutdown*` de `Native::Unsupported` para `Native::Same`.
- [ ] **D.2** Atualizar [`basicnext/README.md`](file:///Users/caq/src/bn/basicnext/README.md): remover os itens da seção `## Known limitations` (ou remover a seção se não houver mais limitações em aberto).
- [ ] **D.3** Atualizar [`todo/critical-problems.md`](file:///Users/caq/src/bn/basicnext/todo/critical-problems.md): marcar como resolvidos os itens de predicados `HOST.Net.Address` e `TCPStream.Shutdown*`.
- [ ] **D.4** Executar `./scripts/test-battery.sh --gates` garantindo que todos os gates de repositório, lint, formatação e paridade passem com sucesso.
- [ ] **D.5** Mover o bucket para `done/bucket-0.6.9-4-native-net-gaps-and-timezone.md` ao concluir a execução.
