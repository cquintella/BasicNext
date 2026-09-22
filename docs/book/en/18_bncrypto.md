# Appendix K: BNCrypto

[← Previous: Appendix J: `BNDispatch`](16_bndispatch.md) · [Contents](toc.md)

`BNCrypto` is an external provider-backed module. It is not part of the Basic
Next core and every consumer must import it explicitly:

```basic
IMPORT BNCrypto AS Crypto
```

The normative contract is [`docs/library/bncrypto.md`](../../library/bncrypto.md).

## Digests

`SHA256` and `SHA512` hash the UTF-8 bytes of a `STRING` and return lowercase
hexadecimal — 64 characters and 128 characters respectively. The width is fixed
and does not depend on the input:

```basic
IMPORT BNCrypto AS Crypto

FUNCTION Start() AS VOID
    PRINT Crypto.SHA256("abc")
END FUNCTION
```

```
ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
```

Digests are pure. They need no `HOST` capability, touch no file and no socket,
and the same argument always produces the same result. That is why they are
available to both the interpreter and compiled binaries without a permission
decision.

## Why the digest is a `STRING` but a key is not

A digest is meant to be shown: printed, compared, written to a log. A key is
the opposite — showing it is the bug. `BNCrypto` encodes that difference in the
types rather than in a comment.

`Crypto.Bytes` is an **opaque** buffer. There is no implicit conversion from
`Bytes` to `STRING`, so key material cannot be printed, concatenated, or logged
by accident. When you genuinely want to see the bytes, you ask for it:

```basic
IMPORT BNCrypto AS Crypto

FUNCTION Start() AS VOID
    LET secret AS Crypto.Bytes = Crypto.FromText("abc")
    PRINT secret.Length()
    PRINT secret.ToHex()
    RELEASE secret
END FUNCTION
```

```
3
616263
```

`Bytes` is a resource, like a file or a socket: it is owned, and `RELEASE` ends
that ownership. Releasing twice is reported, not ignored.

## Reading malformed input

`FromHex` returns `Bytes OR Error` rather than guessing. An odd-length string is
not a byte sequence, so it is rejected instead of being silently truncated:

```basic
LET raw AS Crypto.Bytes OR Error = Crypto.FromHex("abc")
IF raw IS Error THEN
    PRINT "odd-length-rejected"
END IF
```

This is the same fail-closed posture the rest of the module takes: when an input
cannot be interpreted, you get an `Error` you must handle, never a partial
result that looks plausible.

## Encrypting something

Encryption in `BNCrypto` is always *authenticated*: you do not get a mode that
encrypts without also proving the message was not altered. `Seal` produces a
ciphertext with a tag appended; `Open` verifies that tag before giving you
anything back.

```basic
IMPORT BNCrypto AS Crypto

FUNCTION Start() AS VOID
    LET key AS Crypto.Bytes = Crypto.FromText("0123456789abcdef0123456789abcdef")
    LET nonce AS Crypto.Bytes = Crypto.FromText("0123456789ab")
    LET plain AS Crypto.Bytes = Crypto.FromText("abc")
    LET aad AS Crypto.Bytes = Crypto.FromText("hdr")

    LET sealed AS Crypto.Bytes OR Error = Crypto.SealAesGcm(key, nonce, plain, aad)
    IF sealed IS Error THEN
        PRINT "seal failed"
    ELSE
        PRINT sealed.ToHex()
    END IF
END FUNCTION
```

The key is 32 bytes and the nonce is 12. Those are not suggestions: a wrong
length gives you an `Error`, not a padded guess.

The `aad` — additional authenticated data — is covered by the tag but is *not*
encrypted. It is for the parts of a message that must travel in the clear yet
must not be swapped, such as a header or a record id.

### The failure you want

Change one byte of the ciphertext, or open with a different `aad`, and `Open`
returns `Error`. Not a shorter plaintext, not garbage — nothing. This is the
whole point of authenticated encryption, and it is why `Open` returns
`Bytes OR Error` and forces you to write the `IF`:

```basic
LET opened AS Crypto.Bytes OR Error = Crypto.OpenAesGcm(key, nonce, sealed, wrongAad)
IF opened IS Error THEN
    PRINT "rejected"        // this is the branch that runs
END IF
```

### One rule the language cannot enforce

Never use the same nonce twice with the same key. With AES-GCM in particular,
repeating a nonce does not merely weaken the message — it can expose the
authentication key itself. `BNCrypto` cannot check this for you, because it
never sees your other messages. Derive nonces from `HOST.Random`, or from a
counter you are certain never repeats.

## One implementation, two backends

The digests do not live in the interpreter. They live in `bn_rt` behind the C
ABI, which is what compiled binaries link against. `bni` and `bnc` therefore run
the *same* code, and the test suite asserts that a compiled program prints
byte-for-byte what the interpreted one prints. A backend that drifted would fail
that test rather than quietly disagree.

`Bytes` works the same way: the handle travels as a pointer into a `bn_rt`
table, so a compiled program creates, measures, renders and releases the buffer
through the same code the interpreter uses.

That includes the parts that can fail. `FromHex` returns `Bytes OR Error`, and
when you narrow it with `IF raw IS Error … ELSE`, the buffer you get in the
`ELSE` branch works the same compiled as interpreted — the handle is simply
read out of the result value. Basic Next keeps the distinction between *"this
program is wrong"* and *"this target cannot do it yet"*, and here there is no
gap to report: both backends run the whole surface.

## What is not here yet

Authenticated encryption, message authentication, password hashing, signatures,
and the post-quantum primitives are named in the module's roadmap but are **not**
importable today. `BNCrypto` currently exposes digests and `Bytes`. The contract
document lists the full intended surface and marks what has landed.

---

[Contents →](toc.md)
