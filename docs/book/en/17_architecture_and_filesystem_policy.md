# Architecture: Filesystem Policy and Optional Sandboxing

Basic Next keeps filesystem access explicit without forcing every program into
a sandbox. The default build remains compatible with ordinary operating-system
permissions. Sandboxing is an opt-in artifact profile selected when the
program is built or run.

## Two filesystem profiles

Without a sandbox option, the program uses the normal filesystem permissions of
the process that launches it:

```text
bn build app.bn
```

This is the `unrestricted` profile. It does not mean that the process has
superuser access. The operating system, user account, container, and service
manager still apply their ordinary permissions. Basic Next simply does not add
directory restrictions in this profile.

The optional `sandboxed` profile records a filesystem policy in the compiled
artifact:

```text
bn build app.bn --sandbox \
    --read-root ./input \
    --write-root ./output
```

The sandboxed artifact is limited to its declared roots. A read root permits
reading files below that directory. A write root permits creating, truncating,
appending, renaming, or deleting files below that directory. Read and write
roots are separate so a program can consume input without being able to alter
it.

If the selected target cannot enforce the requested profile, the build fails.
It must not silently produce an unrestricted artifact.

## Artifact ceiling and execution policy

The sandbox profile is an artifact ceiling: the maximum authority recorded in
the binary. A particular execution may reduce that authority, but it cannot
increase it.

```text
effective policy = artifact ceiling INTERSECT execution policy
```

For example, an artifact may be built with read access to `input` and write
access to `output`. A deployment can run that artifact read-only or with all
filesystem access disabled. It cannot grant access to a different directory.

Execution restrictions may be supplied by the launcher or environment:

```text
BN_FS_POLICY=deny ./app
BN_FS_POLICY=read-only ./app
```

Environment configuration is therefore a narrowing mechanism. It is not a
replacement for the artifact ceiling, and `BN_FS_POLICY=unrestricted` cannot
remove restrictions from a sandboxed binary.

The same policy model applies to `bn run` so that interpretation and native
compilation have the same observable authorization behavior:

```text
bn run app.bn --sandbox --read-root ./input --write-root ./output
```

## What “unrestricted” means

`unrestricted` does not promise access to every path. It means that Basic Next
does not impose a root-based policy. The process still runs under the host
operating system's account, ACLs, container, MAC policy, and other controls.
Programs that need stronger isolation can use a container, WASI runtime,
operating-system sandbox, or service-manager policy in addition to Basic Next.

## Runtime enforcement

Capability checks happen at the runtime boundary. `HOST.FileSystem` operations
must re-check the effective policy when they open, read, write, append, rename,
or delete a resource. Checking only during compilation is insufficient because
the policy can be narrowed after a process starts.

Authorization failure is distinct from other failures:

```text
EXECUTION_POLICY_DENIED  the policy forbids this operation
FILE_ERROR               the policy permits it, but I/O failed
TARGET_UNSUPPORTED_*     the selected backend cannot provide the operation
```

Closing a previously opened handle remains available after a policy is
narrowed, so a denied operation does not prevent cleanup.

## Sandboxed path handling

The sandboxed profile compares path components, not textual prefixes. A path
such as `/data2/file` is not below `/data`, and `..` cannot escape a root.
Symlinks, junctions, and other reparse mechanisms must be handled by the
target's filesystem APIs; a `canonicalize()` call followed by a later `open()`
is not sufficient on its own because the filesystem can change between those
operations.

Sandbox support is consequently target-specific. The implementation must use
directory-relative, handle-based, or equivalent race-resistant operations on
each supported platform. Unsupported platforms reject `--sandbox` rather than
downgrading to `unrestricted`.

## Policy precedence

The precedence is intentionally one-way:

```text
compile-time artifact ceiling
        ↓ can only narrow
execution policy
        ↓ checked at every HOST.FileSystem call
effective operation
```

The following outcomes are expected:

| Artifact | Execution policy | Result |
| --- | --- | --- |
| unrestricted | default | Normal operating-system permissions |
| unrestricted | deny | No filesystem operations through `HOST.FileSystem` |
| unrestricted | read-only | Reads allowed; writes denied |
| sandboxed: `input`/`output` | default | Only declared roots are available |
| sandboxed: `input`/`output` | deny | Filesystem operations denied |
| sandboxed: `input`/`output` | another root | Cannot enlarge the artifact ceiling |

This design keeps the compatibility default simple while making stronger
filesystem isolation explicit, reviewable, and impossible to enable or disable
accidentally through an environment override.
