#!/bin/sh
# Re-derive the liquid tag catalogue from a Forem checkout.
#
# The tag list is compiled into the server, so it can drift from upstream without anything
# failing. This regenerates it; `git diff` afterwards is the drift.
#
#   git clone --depth 1 https://github.com/forem/forem.git /tmp/forem
#   ./scripts/refresh-knowledge.sh /tmp/forem
set -eu

FOREM="${1:?usage: refresh-knowledge.sh <path to a forem checkout>}"
cd "$(dirname "$0")/.."
OUT=crates/devto-mcp/knowledge/liquid-tags.md

test -d "$FOREM/app/liquid_tags" || {
    echo "no app/liquid_tags in $FOREM — is that a Forem checkout?" >&2
    exit 1
}

COMMIT=$(git -C "$FOREM" rev-parse --short HEAD)
DATE=$(git -C "$FOREM" log -1 --format=%ad --date=short)

FOREM="$FOREM" COMMIT="$COMMIT" DATE="$DATE" OUT="$OUT" python3 scripts/derive_liquid_tags.py

echo "regenerated $OUT from forem @ $COMMIT ($DATE)"
echo "review with: git diff $OUT"
