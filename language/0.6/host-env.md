# HOST.Env (0.6.5)

`HOST.Env` provides read-only access to the process environment through
`IMPORT HOST.Env AS <alias>`.

## Surface

```basic
IMPORT HOST.Env AS Env

FUNCTION Start() AS INTEGER
    LET home AS STRING OR Error = Env.Get("HOME")
    IF home IS Error THEN
        PRINT home.Message
        RETURN 1
    END IF
    PRINT home

    LET known AS BOOLEAN OR Error = Env.Has("BN_LIBRARY_PATH")
    RETURN 0
END FUNCTION
```

| Member | Result |
| --- | --- |
| `Env.Get(name AS STRING)` | `STRING OR Error`: the value; `Error` when the variable is not set |
| `Env.Has(name AS STRING)` | `BOOLEAN OR Error`: `TRUE` if set, `FALSE` if absent |

- **Read-only:** Changing the environment of a running process is not thread-safe.
  No `Set` or `Remove` is provided.
- **No enumeration:** Listing all environment variables is omitted in this release.
- An empty value (`""`) is a set variable.

## Errors

Each failure is an `Error` with a portable code exposed as a member on the capability:

| Condition | Constant | Value |
| --- | --- | ---: |
| The variable is not set (`Get` only) | `Env.NOT_SET` | 1 |
| `name` is empty or contains `=` or NUL | `Env.INVALID_NAME` | 2 |
| The value is not valid UTF-8 | `Env.INVALID_UTF8` | 3 |
| Execution policy denies the read | `Env.POLICY_DENIED` | 4 |

A host without the capability fails at the call with `HOST_CAPABILITY_UNAVAILABLE`,
distinct from policy denial.

## Policy

- `BN_ENV_POLICY=deny` denies every read (`Env.POLICY_DENIED`), honoured identically
  by `bni run` and a compiled program.
- **Allowed by default.** Restricted profiles (such as the notebook kernel) deny by default.
- Any malformed `BN_ENV_POLICY` value causes `CONFIG_INVALID` (exit status 2 before `Start`).
