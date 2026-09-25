#!/usr/bin/env bash
# Basic Next installer for Linux and macOS.
#
# Installs the `bni` (interpreter) and `bnc` (compiler) binaries plus the
# runtime files they discover
# (stdlib .bn modules, diagnostics catalog, man pages, native runtime lib)
# under a FHS prefix:
#
#   $PREFIX/bin/bni                             interpreter, check, lsp, dap
#   $PREFIX/bin/bnc                             compiler
#   $PREFIX/lib/libbn_rt.a                      native runtime for `bnc`
#   $PREFIX/share/bn/modules/bn/*.bn            standard library modules
#   $PREFIX/share/bn/diagnostics/en-US/*.ftl    diagnostics catalog (optional;
#                                               an identical catalog is embedded)
#   $PREFIX/share/man/man1/{bni,bnc}.1          Unix manual pages
#
# `bni` finds modules/bn and the catalog by walking upward from its own location,
# and finds libbn_rt.a next to the binary, in $PREFIX/, or in $PREFIX/lib/
# (override with BN_RT_LIB). Do not point BN_RT_LIB at a source-tree
# target/ directory for a normal install — use the prefix lib.
#
# After a successful install this script also writes, under the invoking
# user's home (override with BN_STATE_DIR):
#
#   $HOME/.basicnext/install.log     append-only log of operations performed
#   $HOME/.basicnext/uninstall.sh    script that removes the files this run installed
#
# Usage:
#   ./scripts/install.sh                  # asks where to install (menu 1–4)
#   ./scripts/install.sh --prefix DIR     # non-interactive prefix
#   PREFIX=DIR ./scripts/install.sh       # same (skips the menu)
#   ./scripts/install.sh --no-build       # install already-built target/release binaries
#
# Install-location menu (when PREFIX / --prefix are unset):
#   1) $HOME/basicnext
#   2) /opt/basicnext
#   3) /usr/local
#   4) Other path…
# The prompt reads /dev/tty so a verified downloaded release asset remains
# interactive even when standard input is redirected.
#
# Outside a checkout (a verified release asset) the script bootstraps itself: it resolves the
# latest release (or $BN_VERSION, e.g. v0.6.1), downloads the release's
# checksummed source payload for modules/catalog/man pages, then the prebuilt
# bni/bnc for this OS/arch. Every downloaded asset is checked against the
# release's SHA256SUMS; if the release has no binary for this platform, the
# installer builds from the verified source payload with cargo instead.
#
# Download this script and the release's SHA256SUMS, verify it, then run it.
set -euo pipefail

# PREFIX left unset until --prefix / PREFIX= / the install-location menu resolves it.
PREFIX_SET=0
if [[ -n "${PREFIX:-}" ]]; then
  PREFIX_SET=1
else
  PREFIX=""
fi
BUILD=1
REPO="cquintella/BasicNext"
BN_STATE_DIR="${BN_STATE_DIR:-$HOME/.basicnext}"
INSTALL_LOG="$BN_STATE_DIR/install.log"
UNINSTALL_SCRIPT="$BN_STATE_DIR/uninstall.sh"
MANIFEST=$(mktemp)
trap 'rm -f "$MANIFEST"' EXIT

mkdir -p "$BN_STATE_DIR"

log() {
  echo "$*"
  echo "$*" >>"$INSTALL_LOG"
}

record() {
  printf '%s\n' "$1" >>"$MANIFEST"
  echo "    installed: $1" >>"$INSTALL_LOG"
}

file_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    echo "error: sha256sum or shasum is required" >&2
    return 1
  fi
}

verify_release_asset() {
  local checksums=$1 asset=$2 name=${3:-} expected actual
  [[ -n "$name" ]] || name=$(basename "$asset")
  expected=$(awk -v n="$name" '$2 == n { print $1 }' "$checksums")
  actual=$(file_sha256 "$asset")
  [[ -n "$expected" && "$expected" == "$actual" ]] \
    || { echo "error: SHA256 mismatch for $name (expected ${expected:-<absent>}, got $actual)" >&2; return 1; }
}

ask_prefix() {
  local choice custom
  if [[ ! -r /dev/tty || ! -w /dev/tty ]]; then
    echo "error: no TTY for the install-location menu; pass --prefix DIR or PREFIX=DIR" >&2
    exit 2
  fi
  {
    echo
    echo "Where should Basic Next be installed?"
    echo "  1) $HOME/basicnext"
    echo "  2) /opt/basicnext"
    echo "  3) /usr/local"
    echo "  4) Other path…"
  } > /dev/tty
  while true; do
    printf 'Choose [1-4]: ' > /dev/tty
    IFS= read -r choice < /dev/tty || true
    case "$choice" in
      1) PREFIX="$HOME/basicnext"; break ;;
      2) PREFIX="/opt/basicnext"; break ;;
      3) PREFIX="/usr/local"; break ;;
      4)
        printf 'Prefix path: ' > /dev/tty
        IFS= read -r custom < /dev/tty || true
        custom="${custom/#\~/$HOME}"
        custom="${custom%%/}"
        if [[ -z "$custom" ]]; then
          echo "error: empty path" > /dev/tty
          continue
        fi
        PREFIX="$custom"
        break
        ;;
      *) echo "Please enter 1, 2, 3, or 4." > /dev/tty ;;
    esac
  done
  echo "Installing to $PREFIX" > /dev/tty
}

while (($# > 0)); do
  case "$1" in
    --prefix)
      [[ $# -ge 2 ]] || { echo "error: --prefix needs a path" >&2; exit 2; }
      PREFIX="$2"; PREFIX_SET=1; shift 2 ;;
    --no-build) BUILD=0; shift ;;
    -h|--help)
      if [[ -f "$0" ]]; then sed -n '2,45p' "$0" | sed 's/^# \{0,1\}//'
      else echo "usage: install.sh [--prefix DIR] [--no-build]  (PREFIX, BN_VERSION, BN_STATE_DIR env also honoured)"; fi
      exit 0 ;;
    *) echo "error: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

if ((PREFIX_SET == 0)); then
  ask_prefix
fi
[[ -n "$PREFIX" ]] || { echo "error: empty PREFIX" >&2; exit 2; }

{
  echo "===== Basic Next install $(date '+%Y-%m-%d %H:%M:%S %z') ====="
  echo "user=$(id -un) host=$(uname -n) os=$(uname -s) arch=$(uname -m)"
  echo "PREFIX=$PREFIX BN_STATE_DIR=$BN_STATE_DIR BUILD=$BUILD BN_VERSION=${BN_VERSION:-}"
} >>"$INSTALL_LOG"

# --- bootstrap: not running from a checkout -----------------------------------
# Only skip bootstrap when
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
  trap 'rm -rf "$work"; rm -f "$MANIFEST"' EXIT
  base="https://github.com/$REPO/releases/download/$tag"
  checksums="$work/SHA256SUMS"
  source_archive="$work/basicnext-source-$tag.tar.gz"
  log "==> Basic Next $tag"
  log "==> Downloading verified source payload (modules, catalog, man pages)"
  curl -fsSL -o "$checksums" "$base/SHA256SUMS"
  curl -fsSL -o "$source_archive" "$base/basicnext-source-$tag.tar.gz"
  verify_release_asset "$checksums" "$source_archive"
  tar -xzf "$source_archive" -C "$work"
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
  prebuilt=0
  if [[ -n "$os" && -n "$arch" ]]; then
    log "==> Downloading prebuilt executables for $os-$arch"
    mkdir -p "$src/target/release"
    # 0.6 releases ship bni + bnc; 0.5 releases ship bn + bnc (bn was the
    # interpreter). Probe for bni and fall back so pinned old tags keep working.
    binaries="bni bnc"
    if ! curl -fsSL -o "$src/target/release/bni" "$base/bni-$os-$arch"; then
      rm -f "$src/target/release/bni"
      binaries="bn bnc"
      log "    no bni asset in $tag: installing the 0.5 layout (bn + bnc)"
    fi
    downloaded=1
    for name in $binaries; do
      [[ $name == bni ]] && continue
      curl -fsSL -o "$src/target/release/$name" "$base/$name-$os-$arch" || downloaded=0
    done
    if ((downloaded)); then
      for name in $binaries; do
        verify_release_asset "$checksums" "$src/target/release/$name" "$name-$os-$arch"
        chmod +x "$src/target/release/$name"
      done
      # Native `bnc` needs libbn_rt.a in the install prefix (arch-specific).
      if curl -fsSL -o "$src/target/release/libbn_rt.a" "$base/libbn_rt-$os-$arch.a"; then
        expected=$(awk -v n="libbn_rt-$os-$arch.a" '$2 == n { print $1 }' "$checksums")
        actual=$(file_sha256 "$src/target/release/libbn_rt.a")
        if [[ -n "$expected" && "$expected" == "$actual" ]]; then
          log "    libbn_rt.a checksum OK"
        else
          log "    warning: libbn_rt.a SHA256 mismatch or missing from SHA256SUMS; will try cargo -p bn_rt"
          rm -f "$src/target/release/libbn_rt.a"
        fi
      else
        log "    note: no libbn_rt-$os-$arch.a asset in $tag (will try cargo -p bn_rt if available)"
      fi
      log "    checksums OK"
      prebuilt=1
    else
      log "    no prebuilt asset for $os-$arch in $tag"
    fi
  fi
  if ((prebuilt)); then
    BUILD=0
  elif command -v cargo >/dev/null 2>&1; then
    log "==> Falling back to a source build (cargo)"
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
  log "==> Building release binaries (cargo build --release --workspace --bins)"
  cargo build --release --workspace --bins
  log "==> Building native runtime (cargo build -p bn_rt --release)"
  cargo build -p bn_rt --release
fi

bni_bin="$repo_root/target/release/bni"
bnc_bin="$repo_root/target/release/bnc"
bn_bin="$repo_root/target/release/bn"
bn_rt_lib="$repo_root/target/release/libbn_rt.a"
# 0.6 layout: bni + bnc. 0.5 layout (older tags): bn + bnc, bn being the
# interpreter itself.
if [[ -x "$bni_bin" ]]; then
  layout=0.6; interpreter="bni"; binaries="bni bnc"
else
  layout=0.5; interpreter="bn"; binaries="bn bnc"
fi
for name in $binaries; do
  [[ -x "$repo_root/target/release/$name" ]] || { echo "error: missing binary 'target/release/$name' (run without --no-build, or 'cargo build --release --workspace --bins' first)" >&2; exit 1; }
done
if [[ ! -f "$bn_rt_lib" ]]; then
  if command -v cargo >/dev/null 2>&1; then
    log "==> libbn_rt.a missing; building cargo -p bn_rt --release"
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
    log "==> $PREFIX is not writable; using sudo"
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

log "==> Installing to $PREFIX"
$SUDO install -d "$bindir" "$libdir" "$moddir" "$diagdir" "$mandir"
echo "    ensured dirs: $bindir $libdir $moddir $diagdir $mandir" >>"$INSTALL_LOG"
for name in $binaries; do
  $SUDO install -m 0755 "$repo_root/target/release/$name" "$bindir/$name"
  record "$bindir/$name"
done

# Native runtime for `bnc` (arch-specific staticlib). Discovered as
# $PREFIX/lib/libbn_rt.a when bnc lives in $PREFIX/bin (bn_compile_driver::toolchain).
$SUDO install -m 0644 "$bn_rt_lib" "$libdir/libbn_rt.a"
record "$libdir/libbn_rt.a"

# Standard library modules (arch-independent .bn source).
for mod in "$repo_root/modules/bn/"*.bn; do
  base=$(basename "$mod")
  $SUDO install -m 0644 "$mod" "$moddir/$base"
  record "$moddir/$base"
done

# Diagnostics catalog (optional at runtime — an identical copy is embedded).
# Record every file copied so uninstall removes only what this run wrote.
while IFS= read -r -d '' f; do
  rel="${f#"$repo_root/share/bn/diagnostics/"}"
  dest="$diagdir/$rel"
  $SUDO mkdir -p "$(dirname "$dest")"
  $SUDO install -m 0644 "$f" "$dest"
  record "$dest"
done < <(find "$repo_root/share/bn/diagnostics" -type f -print0 2>/dev/null)

# Man pages (whichever this tree has: bni.1/bnc.1 in 0.6, bn.1 in 0.5).
for page in bni bnc bn; do
  if [[ -f "$repo_root/docs/man/$page.1" ]]; then
    $SUDO install -m 0644 "$repo_root/docs/man/$page.1" "$mandir/$page.1"
    record "$mandir/$page.1"
  fi
done

log "==> Installed:"
log "    $(for name in $binaries; do printf '%s ' "$bindir/$name"; done)($layout layout)"
log "    $libdir/libbn_rt.a"
log "    $moddir/ ($(ls "$repo_root/modules/bn/"*.bn | wc -l | tr -d ' ') modules)"
log "    $diagdir/, $mandir/"

# Verify against the installed copy (not the build tree).
log "==> Verifying"
"$bindir/$interpreter" --version | tee -a "$INSTALL_LOG"
"$bindir/bnc" --version | tee -a "$INSTALL_LOG"
tmp=$(mktemp -d)
printf 'IMPORT BNMath AS M\nFUNCTION Start() AS VOID\nPRINT M.ABS(-7.0)\nEND FUNCTION\n' > "$tmp/check.bn"
if out=$(cd "$tmp" && "$bindir/$interpreter" run check.bn 2>&1) && [[ "$out" == "7.0" ]]; then
  log "    stdlib module resolution OK (BNMath.ABS(-7.0) = 7.0)"
else
  log "    warning: stdlib check did not return the expected value; output was: $out"
fi
rm -rf "$tmp"

# --- uninstall script for this install (home of the invoking user) ------------
{
  echo '#!/usr/bin/env bash'
  echo "# Generated by Basic Next install.sh on $(date '+%Y-%m-%d %H:%M:%S %z')"
  echo "# Removes only the files recorded for PREFIX=$PREFIX"
  echo "# Matching install log: $INSTALL_LOG"
  echo 'set -euo pipefail'
  printf 'PREFIX=%q\n' "$PREFIX"
  cat <<'EOS'
SUDO=""
parent=$(dirname "$PREFIX")
if [[ -e "$PREFIX" && ! -w "$PREFIX" ]] || [[ ! -e "$PREFIX" && ! -w "$parent" ]]; then
  if command -v sudo >/dev/null 2>&1; then SUDO="sudo"
  else
    echo "error: $PREFIX is not writable and sudo is unavailable" >&2
    exit 1
  fi
fi
echo "==> Uninstalling Basic Next files under $PREFIX"
EOS
  if command -v tac >/dev/null 2>&1; then
    rev_list=$(tac "$MANIFEST")
  else
    rev_list=$(tail -r "$MANIFEST")
  fi
  while IFS= read -r path; do
    [[ -n "$path" ]] || continue
    printf 'if [[ -e %q || -L %q ]]; then $SUDO rm -f %q; echo "    removed %s"; fi\n' \
      "$path" "$path" "$path" "$path"
  done <<<"$rev_list"
  printf 'for d in %q %q %q %q %q; do\n' \
    "$moddir" "$PREFIX/share/bn/modules" "$diagdir" "$PREFIX/share/bn" "$mandir"
  cat <<'EOS'
  [[ -d "$d" ]] || continue
  if [[ -z "$(ls -A "$d" 2>/dev/null || true)" ]]; then
    $SUDO rmdir "$d" 2>/dev/null && echo "    removed empty $d" || true
  fi
done
echo "==> Uninstall finished."
echo "note: install.log under BN_STATE_DIR was kept; delete it manually if you want."
EOS
} >"$UNINSTALL_SCRIPT"
chmod +x "$UNINSTALL_SCRIPT"
log "==> Wrote uninstall script: $UNINSTALL_SCRIPT"
log "==> Install log: $INSTALL_LOG"

case ":$PATH:" in
  *":$bindir:"*) ;;
  *) log "note: $bindir is not on your PATH; add it, e.g.: export PATH=\"$bindir:\$PATH\"" ;;
esac
log "==> Done."
echo "===== end install $(date '+%Y-%m-%d %H:%M:%S %z') =====" >>"$INSTALL_LOG"
