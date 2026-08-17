# Basic Next

Uma linguagem de programação moderna com sintaxe inspirada em BASIC, projetada
como projeto open source e laboratório didático para disciplinas de Compiladores.

O repositório começa pela especificação: uma implementação só entra quando a
semântica correspondente estiver definida e revisada.

## Estado

**Pré-implementação.** A versão de linguagem em discussão é a 0.1; ainda não
há compilador, runtime ou API estável.

## Estrutura

- `docs/language/0.1.md` — especificação mínima da linguagem.
- `docs/proposals/` — propostas que ainda não fazem parte da linguagem.
- `examples/` — programas que guiam a especificação.
- `PHILOSOPHY.md` — princípios de projeto.
- `ROADMAP.md` — entregas incrementais para o compilador didático.
- `GOVERNANCE.md` — como decisões são tomadas.
- `TRADEMARK.md` — uso do nome do projeto.

## Exemplo

```basic
CLASS Main
    SUB Start(system AS SYSTEM)
        LET contador AS INTEGER = 0

        WHILE contador < 10
            PRINT "Basic Next", contador
            contador += 1
        WEND
    END SUB
END CLASS
```

## Participação

Leia [CONTRIBUTING.md](CONTRIBUTING.md). Discussões de evolução começam como
propostas em `docs/proposals/`; mudanças na especificação exigem exemplos.

## Licença

[MIT](LICENSE).
