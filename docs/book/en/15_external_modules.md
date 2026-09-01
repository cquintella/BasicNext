# Appendix I: External module conventions

Every `BN*` facility is an external module backed by a host/provider
interface. External modules:

- are never implicitly available in the language core;
- require an explicit `IMPORT` and alias;
- must expose bounded, deterministic errors when a provider is unavailable;
- must keep their normative API and fixtures separate from core grammar;
- may be implemented by Rust providers without changing BN syntax.

`HOST` is the sole built-in interface object in the language specification. The
planned `BNThreads`, `BNCrypto`, and other future modules follow these same
rules.
