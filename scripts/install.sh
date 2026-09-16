#!/usr/bin/env bash
# Basic Next installer for Linux and macOS.
#
# Installs the `bn` and `bnc` binaries plus the runtime files they discover
# (stdlib .bn modules, diagnostics catalog, man page) under a FHS prefix:
#
#   $PREFIX/bin/bn                              executable
#   $PREFIX/bin/bnc                             compiler front-door
#   $PREFIX/share/bn/modules/bn/*.bn            standard library modules
#   $PREFIX/share/bn/diagnostics/en-US/*.ftl    diagnostics catalog (optional;
#                                               an identical catalog is embedded)
#   $PREFIX/share/man/man1/bn.1                 Unix manual page
#
# `bn` finds modules/bn and the catalog by walking upward from its own location,
# so this layout is zero-config after install.
#
# Usage:
#   ./scripts/install.sh                  # build from source, install to /usr/local (sudo if needed)
#   PREFIX="$HOME/.local" ./scripts/install.sh   # user-local, no sudo
#   ./scripts/install.sh --prefix /opt/basicnext
#   ./scripts/install.sh --no-build       # install already-built target/release binaries
set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"
BUILD=1

while (($# > 0)); do
  case "$1" in
    --prefix)
      [[ $# -ge 2 ]] || { echo "error: --prefix needs a path" >&2; exit 2; }
      PREFIX="$2"; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    -h|--help)
      sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "error: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

repo_root=$(cd "$(dirname "$0")/.." && pwd -P)
cd "$repo_root"

if ((BUILD)); then
  command -v cargo >/dev/null 2>&1 || { echo "error: cargo (Rust 1.97+) is required to build; use --no-build to install prebuilt binaries" >&2; exit 1; }
  echo "==> Building release binaries (cargo build --release --bins)"
  cargo build --release --bins
fi

bn_bin="$repo_root/target/release/bn"
bnc_bin="$repo_root/target/release/bnc"
for bin in "$bn_bin" "$bnc_bin"; do
  [[ -x "$bin" ]] || { echo "error: missing binary '$bin' (run without --no-build, or 'cargo build --release --bins' first)" >&2; exit 1; }
done

# Choose sudo automatically only when the destination is not writable.
SUDO=""
parent_of_prefix=$(dirname "$PREFIX")
if [[ -e "$PREFIX" && ! -w "$PREFIX" ]] || [[ ! -e "$PREFIX" && ! -w "$parent_of_prefix" ]]; then
  if command -v sudo >/dev/null 2>&1; then
    SUDO="sudo"
    echo "==> $PREFIX is not writable; using sudo"
  else
    echo "error: $PREFIX is not writable and sudo is unavailable; set PREFIX to a writable path" >&2
    exit 1
  fi
fi

bindir="$PREFIX/bin"
moddir="$PREFIX/share/bn/modules/bn"
diagdir="$PREFIX/share/bn/diagnostics"
mandir="$PREFIX/share/man/man1"

echo "==> Installing to $PREFIX"
$SUDO install -d "$bindir" "$moddir" "$diagdir" "$mandir"
$SUDO install -m 0755 "$bn_bin" "$bindir/bn"
$SUDO install -m 0755 "$bnc_bin" "$bindir/bnc"

# Standard library modules (arch-independent .bn source).
$SUDO install -m 0644 "$repo_root/modules/bn/"*.bn "$moddir/"

# Diagnostics catalog (optional at runtime — an identical copy is embedded).
$SUDO cp -R "$repo_root/share/bn/diagnostics/." "$diagdir/"

# Man page.
$SUDO install -m 0644 "$repo_root/docs/man/bn.1" "$mandir/bn.1"

echo "==> Installed:"
echo "    $bindir/bn, $bindir/bnc"
echo "    $moddir/ ($(ls "$repo_root/modules/bn/"*.bn | wc -l | tr -d ' ') modules)"
echo "    $diagdir/, $mandir/bn.1"

# Verify against the installed copy (not the build tree).
echo "==> Verifying"
"$bindir/bn" --version
tmp=$(mktemp -d)
printf 'IMPORT BNMath AS M\nFUNCTION Start() AS VOID\nPRINT M.ABS(-7.0)\nEND FUNCTION\n' > "$tmp/check.bn"
if out=$(cd "$tmp" && "$bindir/bn" run check.bn 2>&1) && [[ "$out" == "7.0" ]]; then
  echo "    stdlib module resolution OK (BNMath.ABS(-7.0) = 7.0)"
else
  echo "    warning: stdlib check did not return the expected value; output was: $out" >&2
fi
rm -rf "$tmp"

case ":$PATH:" in
  *":$bindir:"*) ;;
  *) echo "note: $bindir is not on your PATH; add it, e.g.: export PATH=\"$bindir:\$PATH\"" ;;
esac
echo "==> Done."
