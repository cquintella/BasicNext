# Appendices

Because Basic Next specifies behavior transparently, the exact language specifications and technical lists are maintained in their respective normative files within the repository.

## Appendix A: Keywords Reference

Basic Next maintains a strict registry of reserved words to guarantee backward compatibility. A word is only reserved in its exact uppercase spelling.

For the complete list of keywords, their semantic meanings, and decision statuses, see the normative document:
- [0.3 Keyword Registry](../../language/0.3/keywords.md)

## Appendix B: Language Diagnostics

Basic Next is designed with a zero-warning policy. Diagnostics either reject the source entirely or report a clear runtime failure. 

Diagnostic behavior follows the accepted language contract and command
reference:

- [0.5 language specification](../../language/0.5/0.5.md)
- [`bn(1)`](../../man/bn.1)

## Appendix C: Accepted Syntax (EBNF)

The structural grammar of Basic Next is strictly defined using Extended Backus-Naur Form (EBNF). The EBNF focuses exclusively on parsing valid syntax, while semantic rules (such as return analysis) are enforced by the compiler.

The normative grammar for the current 0.5 line is:

- [`0.5.ebnf`](../../language/0.5/0.5.ebnf) — grammar
- [`0.5.md`](../../language/0.5/0.5.md) — semantics
- [`keywords.md`](../../language/0.5/keywords.md) — reserved words

## Appendix D: The `bn` Tool

The Unix manual for the reference tool is [`bn(1)`](../../man/bn.1).
Installation and troubleshooting are in
[`docs/project/usage.md`](../../project/usage.md). The normative language text
is [`0.5.md`](../../language/0.5/0.5.md).

External provider-backed modules are documented in separate appendices:

- [`BNJson`](11_bnjson.md)
- [`BNLog`](12_bnlog.md)
- [`BNWeb`](13_bnweb.md)
- [`BNData`](14_bndata.md)
- [External module conventions](15_external_modules.md)
- [`BNDispatch`](16_bndispatch.md)
