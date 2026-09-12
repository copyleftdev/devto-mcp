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
cargo mutants -j "$JOBS" --timeout 90 --exclude 'crates/devto-client/src/net.rs'

echo "== gate passed =="
