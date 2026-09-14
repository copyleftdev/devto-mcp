#!/bin/sh
# Re-capture what dev.to says about a sample of tags.
#
# The fixture is committed so the gate stays hermetic and CI needs no key. Re-running this is
# how you find out whether dev.to changed its mind: regenerate, then `git diff`.
#
# Roughly 40 requests, paced under the documented 3 GET/s. A key is optional — everything read
# here is public — but set DEVTO_API_KEY if you have one, since the anonymous budget is
# tighter.
set -eu
cd "$(dirname "$0")/.."
python3 scripts/capture_tag_observations.py
echo "regenerated. review with: git diff crates/devto-mcp/fixtures"
