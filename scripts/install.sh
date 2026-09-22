#!/usr/bin/env bash
# Basic Next installer for Linux and macOS.
#
# Installs the `bni` (interpreter), `bnc` (compiler) and `bn` (compatibility
# dispatcher, retires in 0.7) binaries plus the runtime files they discover
# (stdlib .bn modules, diagnostics catalog, man pages, native runtime lib)
# under a FHS prefix:
#
#   $PREFIX/bin/bni                             interpreter, check, lsp, dap
#   $PREFIX/bin/bnc                             compiler
#   $PREFIX/bin/bn                              dispatcher (bn run|build … → bni/bnc)
#   $PREFIX/lib/libbn_rt.a                      native runtime for `bnc`
#   $PREFIX/share/bn/modules/bn/*.bn            standard library modules
#   $PREFIX/share/bn/diagnostics/en-US/*.ftl    diagnostics catalog (optional;
#                                               an identical catalog is embedded)
#   $PREFIX/share/man/man1/{bni,bnc,bn}.1       Unix manual pages
#
# `bni` finds modules/bn and the catalog by walking upward from its own location,
# and finds libbn_rt.a next to the binary, in $PREFIX/, or in $PREFIX/lib/
# (override with BN_RT_LIB). Do not point BN_RT_LIB at a source-tree
# target/ directory for a normal install — use the prefix lib.
#
# Usage:
#   ./scripts/install.sh                  # build from source, install to /usr/local (sudo if needed)
#   PREFIX="$HOME/.local" ./scripts/install.sh   # user-local, no sudo
#   ./scripts/install.sh --prefix /opt/basicnext
#   ./scripts/install.sh --no-build       # install already-built target/release binaries
#
# Outside a checkout (curl | bash) the script bootstraps itself: it resolves the
# latest release (or $BN_VERSION, e.g. v0.5.0), downloads that tag's source
# tarball for the modules/catalog/man pages, then the prebuilt bni/bnc/bn for this
# OS/arch verified against the release's SHA256SUMS; if the release has no
# asset for this platform it builds from the tarball with cargo instead.
#
#   curl -fsSL https://raw.githubusercontent.com/cquintella/BasicNext/main/scripts/install.sh | bash
#   curl -fsSL .../install.sh | PREFIX="$HOME/.local" bash
#   curl -fsSL .../install.sh | bash -s -- --prefix /opt/basicnext
set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"
BUILD=1
REPO="cquintella/BasicNext"

while (($# > 0)); do
  case "$1" in
    --prefix)
      [[ $# -ge 2 ]] || { echo "error: --prefix needs a path" >&2; exit 2; }
      PREFIX="$2"; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    -h|--help)
      if [[ -f "$0" ]]; then sed -n '2,31p' "$0" | sed 's/^# \{0,1\}//'
      else echo "usage: install.sh [--prefix DIR] [--no-build]  (PREFIX and BN_VERSION env also honoured)"; fi
      exit 0 ;;
    *) echo "error: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

# --- bootstrap: not running from a checkout (curl | bash) ----------------------
# When piped (`curl | bash`), $0 is often "bash" and BASH_SOURCE[0] is empty or a
# /dev/fd path — never treat that as a repo checkout. Only skip bootstrap when
# this file lives next to ../Cargo.toml (./scripts/install.sh from a clone).
_self="${BASH_SOURCE[0]:-}"
_bootstrap=0
case "$_self" in
  "" | /dev/fd/* | /proc/self/fd/*) _bootstrap=1 ;;
  *)
    if [[ ! -f "$_self" ]]; then
      _bootstrap=1
    else
      _dir=$(cd "$(dirname "$_self")" && pwd -P)
      [[ -f "$_dir/../Cargo.toml" ]] || _bootstrap=1
    fi
    ;;
esac
if ((_bootstrap)); then
  for tool in curl tar; do
    command -v "$tool" >/dev/null 2>&1 || { echo "error: $tool is required" >&2; exit 1; }
  done
  tag="${BN_VERSION:-}"
  if [[ -z "$tag" ]]; then
    tag=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
      | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)
    [[ -n "$tag" ]] || { echo "error: could not resolve the latest release of $REPO" >&2; exit 1; }
  fi
  work=$(mktemp -d)
  trap 'rm -rf "$work"' EXIT
  echo "==> Basic Next $tag"
  echo "==> Downloading source tree (modules, catalog, man page)"
  curl -fsSL "https://github.com/$REPO/archive/refs/tags/$tag.tar.gz" | tar -xzf - -C "$work"
  src=$(find "$work" -mindepth 1 -maxdepth 1 -type d | head -n1)
  [[ -f "$src/Cargo.toml" ]] || { echo "error: unexpected source tarball layout" >&2; exit 1; }

  case "$(uname -s)" in
    Darwin) os=macos ;;
    Linux) os=linux ;;
    *) os="" ;;
  esac
  case "$(uname -m)" in
    x86_64|amd64) arch=x86_64 ;;
    arm64|aarch64) arch=aarch64 ;;
    *) arch="" ;;
  esac
  base="https://github.com/$REPO/releases/download/$tag"
  prebuilt=0
  if [[ -n "$os" && -n "$arch" ]]; then
    echo "==> Downloading prebuilt bni/bnc/bn for $os-$arch"
    mkdir -p "$src/target/release"
    if curl -fsSL -o "$src/target/release/bni" "$base/bni-$os-$arch" \
      && curl -fsSL -o "$src/target/release/bnc" "$base/bnc-$os-$arch" \
      && curl -fsSL -o "$src/target/release/bn" "$base/bn-$os-$arch" \
      && curl -fsSL -o "$work/SHA256SUMS" "$base/SHA256SUMS"; then
      if command -v sha256sum >/dev/null 2>&1; then sum() { sha256sum "$1" | cut -d' ' -f1; }
      else sum() { shasum -a 256 "$1" | cut -d' ' -f1; }; fi
      for name in "bni-$os-$arch" "bnc-$os-$arch" "bn-$os-$arch"; do
        expected=$(awk -v n="$name" '$2 == n { print $1 }' "$work/SHA256SUMS")
        actual=$(sum "$src/target/release/${name%%-*}")
        [[ -n "$expected" && "$expected" == "$actual" ]] \
          || { echo "error: SHA256 mismatch for $name (expected ${expected:-<absent>}, got $actual)" >&2; exit 1; }
      done
      chmod +x "$src/target/release/bni" "$src/target/release/bnc" "$src/target/release/bn"
      # Native `bnc` needs libbn_rt.a in the install prefix (arch-specific).
      if curl -fsSL -o "$src/target/release/libbn_rt.a" "$base/libbn_rt-$os-$arch.a"; then
        expected=$(awk -v n="libbn_rt-$os-$arch.a" '$2 == n { print $1 }' "$work/SHA256SUMS")
        actual=$(sum "$src/target/release/libbn_rt.a")
        if [[ -n "$expected" && "$expected" == "$actual" ]]; then
          echo "    libbn_rt.a checksum OK"
        else
          echo "    warning: libbn_rt.a SHA256 mismatch or missing from SHA256SUMS; will try cargo -p bn_rt" >&2
          rm -f "$src/target/release/libbn_rt.a"
        fi
      else
        echo "    note: no libbn_rt-$os-$arch.a asset in $tag (will try cargo -p bn_rt if available)"
      fi
      echo "    checksums OK"
      prebuilt=1
    else
      echo "    no prebuilt asset for $os-$arch in $tag"
    fi
  fi
  if ((prebuilt)); then
    BUILD=0
  elif command -v cargo >/dev/null 2>&1; then
    echo "==> Falling back to a source build (cargo)"
    BUILD=1
  else
    echo "error: no prebuilt binaries for this platform and cargo is not installed (install Rust 1.97+ and retry)" >&2
    exit 1
  fi
  repo_root="$src"   # the rest of this script installs from the downloaded tree
else
  repo_root=$(cd "$(dirname "$_self")/.." && pwd -P)
fi

# ------------------------------------------------------------------------------

cd "$repo_root"

if ((BUILD)); then
  command -v cargo >/dev/null 2>&1 || { echo "error: cargo (Rust 1.97+) is required to build; use --no-build to install prebuilt binaries" >&2; exit 1; }
  echo "==> Building release binaries (cargo build --release --workspace --bins)"
  cargo build --release --workspace --bins
  echo "==> Building native runtime (cargo build -p bn_rt --release)"
  cargo build -p bn_rt --release
fi

bni_bin="$repo_root/target/release/bni"
bnc_bin="$repo_root/target/release/bnc"
bn_bin="$repo_root/target/release/bn"
bn_rt_lib="$repo_root/target/release/libbn_rt.a"
for bin in "$bni_bin" "$bnc_bin" "$bn_bin"; do
  [[ -x "$bin" ]] || { echo "error: missing binary '$bin' (run without --no-build, or 'cargo build --release --workspace --bins' first)" >&2; exit 1; }
done
if [[ ! -f "$bn_rt_lib" ]]; then
  if command -v cargo >/dev/null 2>&1; then
    echo "==> libbn_rt.a missing; building cargo -p bn_rt --release"
    cargo build -p bn_rt --release
  fi
fi
[[ -f "$bn_rt_lib" ]] || {
  echo "error: missing '$bn_rt_lib' (needed for bnc). Build with cargo -p bn_rt --release, or install a release that ships libbn_rt-*.a" >&2
  exit 1
}

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
libdir="$PREFIX/lib"
moddir="$PREFIX/share/bn/modules/bn"
diagdir="$PREFIX/share/bn/diagnostics"
mandir="$PREFIX/share/man/man1"

echo "==> Installing to $PREFIX"
$SUDO install -d "$bindir" "$libdir" "$moddir" "$diagdir" "$mandir"
$SUDO install -m 0755 "$bni_bin" "$bindir/bni"
$SUDO install -m 0755 "$bnc_bin" "$bindir/bnc"
$SUDO install -m 0755 "$bn_bin" "$bindir/bn"

# Native runtime for `bnc` (arch-specific staticlib). Discovered as
# $PREFIX/lib/libbn_rt.a when bnc lives in $PREFIX/bin (bn_compile_driver::toolchain).
$SUDO install -m 0644 "$bn_rt_lib" "$libdir/libbn_rt.a"

# Standard library modules (arch-independent .bn source).
$SUDO install -m 0644 "$repo_root/modules/bn/"*.bn "$moddir/"

# Diagnostics catalog (optional at runtime — an identical copy is embedded).
$SUDO cp -R "$repo_root/share/bn/diagnostics/." "$diagdir/"

# Man pages.
for page in bni bnc bn; do
  $SUDO install -m 0644 "$repo_root/docs/man/$page.1" "$mandir/$page.1"
done

echo "==> Installed:"
echo "    $bindir/bni, $bindir/bnc, $bindir/bn"
echo "    $libdir/libbn_rt.a"
echo "    $moddir/ ($(ls "$repo_root/modules/bn/"*.bn | wc -l | tr -d ' ') modules)"
echo "    $diagdir/, $mandir/{bni,bnc,bn}.1"

# Verify against the installed copy (not the build tree).
echo "==> Verifying"
"$bindir/bni" --version
"$bindir/bnc" --version
tmp=$(mktemp -d)
printf 'IMPORT BNMath AS M\nFUNCTION Start() AS VOID\nPRINT M.ABS(-7.0)\nEND FUNCTION\n' > "$tmp/check.bn"
if out=$(cd "$tmp" && "$bindir/bni" run check.bn 2>&1) && [[ "$out" == "7.0" ]]; then
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
