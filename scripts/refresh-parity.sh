#!/bin/sh
# Regenerate the parity fixtures from textstat and pyphen.
#
# The fixtures are committed so the gate stays hermetic and CI needs no Python. This script
# is how they are produced, and re-running it is how you find out whether a textstat release
# changed an answer: regenerate, then `git diff tests/fixtures`.
set -eu

cd "$(dirname "$0")/.."
VENV="${VENV:-/tmp/devto-text-parity-venv}"

python3 -m venv "$VENV"
"$VENV/bin/pip" install -q 'textstat==0.7.13' nltk

"$VENV/bin/python" scripts/generate_parity_fixtures.py

echo "regenerated. review with: git diff crates/devto-text/tests/fixtures"
