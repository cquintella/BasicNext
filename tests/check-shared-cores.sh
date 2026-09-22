#!/usr/bin/env bash
# Construction gate for bucket 0.5.2a (Fragilidades 3 + 5): every HOST
# capability with a native twin has ONE core, execution policy is ONE type
# behind ONE static, and the policy environment is parsed in ONE place.
# Fails closed: a missing `rg` is a failure, never a pass.
set -euo pipefail

root=$(pwd -P)
if (($# == 2)) && [[ $1 == --root ]]; then
  root=$(cd "$2" && pwd -P)
fi
cd "$root"

command -v rg >/dev/null 2>&1 || {
  echo "check-shared-cores: ripgrep (rg) is required; refusing to pass without a scan" >&2
  exit 2
}

status=0
fail() {
  echo "check-shared-cores: $1" >&2
  status=1
}

# (a) Process spawning: HOST.Exec core, trusted-path neighbor lookup, the
# compilation driver (clang / wasm-ld / brew, bucket 0.6.0 1.3), the bn
# dispatcher (spawns the sibling bni/bnc, 0.6.0 2.3), and test helpers that
# re-execute the test binary. Integration tests under crates/*/tests drive
# the executables and are not scanned. Nothing else may spawn.
allowed_spawn='^(crates/bn_host_exec/src/lib\.rs|crates/bn_rt/src/net/neighbor\.rs|crates/bn_compile_driver/src/(toolchain|artifact|tests)\.rs|src/main\.rs|.*_tests\.rs)$'
while IFS= read -r file; do
  [[ $file =~ $allowed_spawn ]] || fail "process spawn outside the shared cores: $file"
done < <(rg -l 'process::Command|Command::new\(' src crates --type rust -g '!crates/*/tests/**' \
  | while IFS= read -r f; do rg -q 'current_exe\(\)' "$f" && [[ $f == crates/bn_rt/src/*_abi.rs ]] || echo "$f"; done)

# (b) The Net core is not duplicated in the interpreter provider crate.
for name in icmp reverse neighbor; do
  [[ -e "crates/bn_host_net/src/net/$name.rs" ]] && fail "duplicated Net core: crates/bn_host_net/src/net/$name.rs"
done

# (c) One module-level policy static in the native runtime (a test-only lock
# scoped inside a #[cfg(test)] fn is indented and not counted).
count=$(rg -c '^(pub(\(crate\))?\s+)?static\s' crates/bn_rt/src/policy.rs || true)
[[ ${count:-0} == 1 ]] || fail "expected exactly 1 static in crates/bn_rt/src/policy.rs, found ${count:-0}"

# (d) The policy environment (BN_FS_POLICY, BN_EXEC_*) is read by the parser
# in bn_rt::policy; the drivers hand it a closure over `env::var` and never
# name the variables. No other reader.
allowed_env='^(crates/bn_rt/src/policy\.rs)$'
while IFS= read -r file; do
  [[ $file =~ $allowed_env ]] || fail "policy environment read outside the parser: $file"
done < <(rg -l '^\s*[^/]*"BN_(FS_POLICY|EXEC_POLICY|EXEC_CAPTURE_LIMIT|EXEC_TIMEOUT_MS)"' src crates --type rust -g '!*_tests.rs' -g '!**/tests.rs' -g '!crates/*/tests/**')

# (e) The filesystem decision has one implementation: no OpenOptions in the
# interpreter core or its providers.
while IFS= read -r file; do
  fail "filesystem open bypasses bn_rt::FsPolicy: $file"
done < <(rg -l 'OpenOptions::new\(\)' crates/bn_interp/src crates/bn_host_fs/src crates/bn_lib_log/src --type rust -g '!*_tests.rs' -g '!**/tests.rs' || true)

if ((status == 0)); then
  echo "shared-cores check passed"
fi
exit $status
