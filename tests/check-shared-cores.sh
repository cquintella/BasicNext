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
# compilation driver (clang / wasm-ld / brew, bucket 0.6.0 1.3), and test
# helpers that re-execute the test binary. Integration tests under
# crates/*/tests drive the executables and are not scanned. Nothing else
# may spawn.
allowed_spawn='^(crates/bn_host_exec/src/lib\.rs|crates/bn_rt/src/net/neighbor\.rs|crates/bn_compile_driver/src/(toolchain|artifact|tests)\.rs|.*_tests\.rs)$'
while IFS= read -r file; do
  file=${file//\\/\/}
  [[ $file =~ $allowed_spawn ]] || fail "process spawn outside the shared cores: $file"
done < <(rg -l 'process::Command|Command::new\(' src crates --type rust -g '!crates/*/tests/**' \
  | while IFS= read -r f; do f_norm=${f//\\/\/}; rg -q 'current_exe\(\)' "$f" && [[ $f_norm == crates/bn_rt/src/*_abi.rs ]] || echo "$f_norm"; done)

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
  file=${file//\\/\/}
  [[ $file =~ $allowed_env ]] || fail "policy environment read outside the parser: $file"
done < <(rg -l '^\s*[^/]*"BN_(FS_POLICY|EXEC_POLICY|EXEC_CAPTURE_LIMIT|EXEC_TIMEOUT_MS|ENV_POLICY)"' src crates --type rust -g '!*_tests.rs' -g '!**/tests.rs' -g '!crates/*/tests/**')

# (e) The filesystem decision has one implementation: no OpenOptions in the
# interpreter core or its providers.
while IFS= read -r file; do
  file=${file//\\/\/}
  fail "filesystem open bypasses bn_rt::FsPolicy: $file"
done < <(rg -l 'OpenOptions::new\(\)' crates/bn_interp/src crates/bn_host_fs/src crates/bn_lib_log/src --type rust -g '!*_tests.rs' -g '!**/tests.rs' || true)

# (f) ARC has one mechanism (proposal arc-shared-core-0.6.5): strong counts,
# liveness, and weak references live in bn_rt::arc (C ABI in arc_abi). No
# other source keeps a strong count, a weak-reference table, or a second
# core; the backends only apply the IR's operations through it.
allowed_arc='^crates/bn_rt/src/arc(_abi)?\.rs$'
while IFS= read -r file; do
  file=${file//\\/\/}
  [[ $file =~ $allowed_arc ]] || fail "ARC mechanism outside bn_rt::arc: $file"
done < <(rg -l 'strong_count|strong \+=|strong -=|weak_(register|unregister|invalidate)|bn_arc_weak_(objects|locations)|struct ArcCore|fn bn_rt_arc_' src crates --type rust || true)

# (g) HOST.Env has one environment reader (bucket 0.6.5b): a source that
# implements a HOST.Env operation reads the environment only through
# bn_host_env; it never calls `env::var` or `var_os` itself.
allowed_env_reader='^crates/bn_host_env/src/lib\.rs$'
while IFS= read -r file; do
  file=${file//\\/\/}
  [[ $file =~ $allowed_env_reader ]] && continue
  rg -q 'var_os\(|env::var\(' "$file" && fail "HOST.Env reads the environment outside bn_host_env: $file"
done < <(rg -l 'HOST\.Env\.' src crates --type rust -g '!*_tests.rs' -g '!**/tests.rs' -g '!crates/*/tests/**' || true)

# (h) BNDispatch queue worker pool and pending queue state live only in
# bn_core_dispatch (bucket 0.6.5c S1).
allowed_dispatch_core='^crates/bn_core_dispatch/src/.*\.rs$'
while IFS= read -r file; do
  file=${file//\\/\/}
  [[ $file =~ $allowed_dispatch_core ]] && continue
  fail "BNDispatch worker pool or queue state outside bn_core_dispatch: $file"
done < <(rg -l 'struct QueueInner|worker_handles' src crates --type rust -g '!*_tests.rs' -g '!**/tests.rs' -g '!crates/*/tests/**' || true)

# (i) Core text / civil temporal arithmetic lives only in bn_core_text (bucket 0.6.5c S2).
allowed_text_core='^crates/bn_core_text/src/.*\.rs$'
while IFS= read -r file; do
  file=${file//\\/\/}
  [[ $file =~ $allowed_text_core ]] && continue
  fail "civil calendar conversions outside bn_core_text: $file"
done < <(rg -l 'fn days_from_civil\(|fn civil_from_days\(' src crates --type rust -g '!*_tests.rs' -g '!**/tests.rs' -g '!crates/*/tests/**' || true)

# (j) Statistical reductions live only in bn_core_math (bucket 0.6.5c S2).
allowed_math_core='^crates/bn_core_math/src/.*\.rs$'
while IFS= read -r file; do
  file=${file//\\/\/}
  [[ $file =~ $allowed_math_core ]] && continue
  fail "statistical reduction core outside bn_core_math: $file"
done < <(rg -l 'fn reduce_f64\(name: &str' src crates --type rust -g '!*_tests.rs' -g '!**/tests.rs' -g '!crates/*/tests/**' || true)

# (k) BNLog record redaction and dispatch live only in bn_core_log (bucket 0.6.5c S3).
allowed_log_core='^crates/bn_core_log/src/.*\.rs$'
while IFS= read -r file; do
  file=${file//\\/\/}
  [[ $file =~ $allowed_log_core ]] && continue
  fail "log dispatch logic outside bn_core_log: $file"
done < <(rg -l 'fn dispatch_log\(' src crates --type rust -g '!*_tests.rs' -g '!**/tests.rs' -g '!crates/*/tests/**' || true)

# (l) A core is safe Rust for both adapters (bucket 0.6.5c AC4): no
# bn_value, no extern "C", no policy variable of its own (the
# caller passes the policy). Every bn_core_* crate is a core; so are the HOST
# cores bn_host_exec and bn_host_env.
for core in crates/bn_core_* crates/bn_host_exec crates/bn_host_env; do
  [[ -d $core/src ]] || continue
  while IFS= read -r file; do
    file=${file//\\/\/}
    fail "core uses an adapter facility (bn_value, extern \"C\", or a policy variable): $file"
  done < <(rg -l 'bn_value|extern "C"|"BN_[A-Z_]*POLICY"' "$core/src" --type rust -g '!*_tests.rs' -g '!**/tests.rs' || true)
done

# (m) bn_rt shrinks to the C ABI (bucket 0.6.5c S5.b, §5): its modules that
# are not `*_abi` are listed in tests/bn_rt_modules.list. A module not
# listed fails (no new domain logic in bn_rt), and so does a listed one that
# is gone (the list only shrinks).
if [[ -d crates/bn_rt/src ]]; then
  listed=$(grep -v '^#' tests/bn_rt_modules.list 2>/dev/null | sort)
  present=$(ls crates/bn_rt/src | grep -Ev '(_abi(\.rs)?|^lib\.rs|_tests\.rs|^tests\.rs)$' | sort)
  while IFS= read -r module; do
    [[ -n $module ]] && fail "bn_rt module not in tests/bn_rt_modules.list (move the logic to a core): $module"
  done < <(comm -13 <(printf '%s\n' "$listed") <(printf '%s\n' "$present"))
  while IFS= read -r module; do
    [[ -n $module ]] && fail "tests/bn_rt_modules.list names a module that is gone; remove it: $module"
  done < <(comm -23 <(printf '%s\n' "$listed") <(printf '%s\n' "$present"))
fi

if ((status == 0)); then
  echo "shared-cores check passed"
fi
exit $status
