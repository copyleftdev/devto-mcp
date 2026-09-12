#!/bin/sh
# Phase gate for devto-mcp. Every check is fatal; nothing is allowed to print green
# on a non-zero exit, so no step is written as `cmd && echo ok`.
set -eu

cd "$(dirname "$0")/.."

# hegel derandomizes and disables its failure database when CI is set. The gate needs
# that: with random draws, a property that fails for its own reasons gets counted as a
# mutant kill it never earned, and `cargo mutants` reports a score that means nothing.
export CI=1

# The box has 64 cores and is usually busy with something else.
JOBS="${JOBS:-8}"
export CARGO_BUILD_JOBS="$JOBS"

# Per-mutant timeout. Deliberately generous rather than tuned to one machine: when a mutant
# breaks a property, hegel shrinks the counterexample, and that work is the test doing its
# job. The slowest mutant here spends ~41s shrinking on a 64-core box and more than 90s on a
# 4-core CI runner — a tight timeout turns those kills into "timeout", which cargo-mutants
# reports as uncertain rather than caught. Nothing here hangs, so the only cost of a loose
# timeout is patience.
MUTANT_TIMEOUT="${MUTANT_TIMEOUT:-300}"

echo "== fmt =="
cargo fmt --check

echo "== clippy =="
cargo clippy --all-targets --all-features -- -D warnings

echo "== test =="
cargo test --all-features

echo "== mutants =="
# crates/devto-client/src/net.rs is the ureq + wall-clock adapter. Nothing in it can be
# killed by a unit test, which is exactly why it is kept as thin as possible and excluded
# here rather than allowed to sit in the survivor list forever.
# Two files are excluded, both for the same reason: nothing in them can be killed by a
# unit test, and both are kept deliberately thin because of it.
#   net.rs  — the ureq and wall-clock adapter behind the Transport/Clock traits.
#   main.rs — the stdio read/write loop around Server::handle_line.
cargo mutants -j "$JOBS" --timeout "$MUTANT_TIMEOUT" \
    --exclude 'crates/devto-client/src/net.rs' \
    --exclude 'crates/devto-mcp/src/main.rs'

echo "== gate passed =="
