# Proposal: BNText Markdown Module

**Status:** Proposed for 0.3. This document is not normative.

## Motivation

Markdown is a portable text format for terminal output, notebooks, generated
documentation, and future renderers. Basic Next should model it as text data,
not as RTF or terminal escape sequences.

## Proposed surface

```basic
IMPORT BNText AS Text

LET message AS Text.Markdown = Text.Markdown("**error** in `file.bn`")
PRINT message
```

`Text.Markdown(source AS STRING) AS Markdown` creates an immutable Markdown
value. `PRINT` and `HOST.Console.PrintAt` may accept `STRING OR Markdown`.
Each host renders the value in its native representation; a host without a
Markdown renderer emits its plain-text content.

## Language impact

No new grammar is proposed. `BNText` follows the existing explicit-import
standard-module model. The semantic and runtime changes are limited to the
new `Markdown` type and accepting it at the existing text-output boundaries.

## Deliberate exclusions

- No `RTF` type or RTF parser.
- No ANSI escapes in source text.
- No color API in the initial proposal.

TODO: choose the accepted Markdown profile, escaping API, composition API,
and the exact rendering contract for terminal and Jupyter hosts.
