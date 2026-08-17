# WBS — Basic Next 0.1

Esta WBS aplica uma versão enxuta do PMI: organiza o escopo por entregáveis,
não por pessoas ou datas. Cada item de nível 2 é um card do GitHub Project.

## Objetivo da entrega

Publicar o Basic Next 0.1 com especificação revisada, exemplos executáveis e
uma implementação de referência capaz de validar e executar os programas do
escopo 0.1. Compilação nativa, GUI, concorrência e integrações de IA estão fora
desta entrega.

## Critérios de aceite

- A especificação 0.1 define sintaxe e semântica sem depender de uma VM.
- Os exemplos oficiais executam na implementação de referência.
- Erros léxicos, sintáticos e de tipo produzem diagnósticos compreensíveis.
- O repositório contém instruções para executar exemplos e reproduzir a suíte
  de conformidade.
- Uma release `v0.1.0` é publicada com notas de versão.

## Estrutura analítica do trabalho

### 1. Gestão da entrega

- **1.1 Escopo e critérios de aceite:** congelar o conteúdo da 0.1 e registrar
  explicitamente o que fica fora.
- **1.2 Controle de entrega:** manter Kanban, decisões e notas de release.

### 2. Especificação da linguagem

- **2.1 Léxico e gramática:** palavras reservadas, tokens, comentários, strings
  e gramática EBNF.
- **2.2 Tipos e expressões:** tipos primitivos, operadores, precedência,
  atribuição e conversões permitidas.
- **2.3 Declarações e controle de fluxo:** variáveis, `SUB`, `FUNCTION`,
  `CLASS`, `IF`, `WHILE`, `REPEAT`, `FOR` e `RETURN`.
- **2.4 Módulos e ambiente:** `IMPORT`, módulo executável, `SUB Start()` e
  contrato de `HOST.main`.
- **2.5 Diagnósticos e conformidade:** erros definidos e exemplos normativos.

### 3. Implementação de referência

- **3.1 Gate de arquitetura:** escolher o motor de execução sem alterar a
  semântica da linguagem (interpretador de AST, VM própria ou Wasm/WAMR).
- **3.2 Front-end:** lexer, parser e AST para toda a gramática 0.1.
- **3.3 Análise semântica:** escopo, resolução de nomes, tipos e diagnósticos.
- **3.4 Execução:** executar os programas 0.1 no motor aprovado no item 3.1.
- **3.5 Ferramenta de linha de comando:** comandos mínimos para verificar e
  executar arquivos Basic Next.

### 4. Qualidade e publicação

- **4.1 Conformidade:** suíte de exemplos e casos de erro.
- **4.2 Documentação de uso:** instalação, primeiro programa e contribuição.
- **4.3 Release 0.1.0:** versionamento, notas e publicação do artefato.

## Dependências principais

```text
2.1–2.5 ──→ 3.2 ──→ 3.3 ──→ 3.4 ──→ 4.1 ──→ 4.3
                 ↑
                3.1
```

O item 3.1 é um gate: ele bloqueia a implementação de execução, mas não o
avanço da especificação.
