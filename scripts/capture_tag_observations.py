#!/usr/bin/env python3
"""Capture what dev.to actually says about a sample of tags.

The text tools are checked against textstat, pyphen, Ruby and dev.to's own reading time —
four things that are not this server. The tag tools had nothing: `check_tag_fit` and
`author_profile` were verified only against mocks written from the same assumptions as the
code. Every wrong-answer bug in them lived in that gap, including the one that reported
`emacs` and `devsecops` as invented tags because `/api/tags` is a popularity ranking and not
a census.

This records dev.to's own answers so the classification logic can be tested against them.

**What is asserted and what is not.** A tag's exact rank moves daily and is captured for
information only. What the tests hold to is the *classification*, which is stable: `webdev`
will not stop being ranked, `emacs` will not stop having articles, and a string of keyboard
noise will not start having them. A fixture that pinned exact ranks would fail every week and
teach the reader to ignore it.

Run through scripts/refresh-tag-oracle.sh, which sources the API key.
"""

from __future__ import annotations

import json
import os
import pathlib
import sys
import time
import urllib.parse
import urllib.request

BASE = "https://dev.to"
UA = "devto-mcp-oracle/0.2 (+https://github.com/copyleftdev/devto-mcp)"
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates/devto-mcp/fixtures/tag_observations.json"

# Chosen to cover every case the classification has to get right, not to be a random sample:
# the very top, the middle, the far tail, tags that are real but unranked, and strings nobody
# has ever published under.
SAMPLE = [
    # expected ranked, high
    "webdev", "ai", "programming", "javascript", "python", "devops",
    # ranked, middle and lower
    "rust", "testing", "security", "mcp", "llm", "architecture",
    "distributedsystems", "softwaredevelopment",
    # real but outside the ranked head — the case that was got wrong
    "emacs", "devsecops", "healthcare", "engineering", "osint", "redteam",
    "formalmethods", "streamprocessing",
    # nothing has ever been published under these
    "qwertyuiopnonsense", "zzzznotarealtagatall", "wombat",
]

PAUSE = 0.45  # dev.to allows 3 GET/s; stay well under it


def get(path: str):
    key = os.environ.get("DEVTO_API_KEY", "")
    headers = {"Accept": "application/vnd.forem.api-v1+json", "User-Agent": UA}
    if key:
        headers["api-key"] = key
    request = urllib.request.Request(BASE + path, headers=headers)
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def taxonomy() -> list[str]:
    """Every tag dev.to ranks, in its order. Ends when a page comes back short."""
    names: list[str] = []
    page = 1
    while page <= 40:
        batch = get(f"/api/tags?per_page=100&page={page}")
        names += [t["name"] for t in batch]
        if len(batch) < 100:
            break
        page += 1
        time.sleep(PAUSE)
    return names


def article_count(tag: str) -> int:
    """How many of the first hundred articles carry this tag. Zero means nothing does."""
    quoted = urllib.parse.quote(tag)
    return len(get(f"/api/articles?tag={quoted}&per_page=100"))


def main() -> int:
    print("reading the ranked taxonomy…", file=sys.stderr)
    ranked = taxonomy()
    rank_of = {name: i + 1 for i, name in enumerate(ranked)}
    print(f"  {len(ranked)} tags ranked", file=sys.stderr)

    observations = []
    for tag in SAMPLE:
        time.sleep(PAUSE)
        count = article_count(tag)
        rank = rank_of.get(tag)
        observations.append({
            "tag": tag,
            "ranked": rank is not None,
            "rank": rank,
            "articles_in_first_100": count,
            "used": count > 0,
        })
        state = f"rank {rank}" if rank else ("unranked, used" if count else "unranked, unused")
        print(f"  {tag:22} {state}", file=sys.stderr)

    OUT.write_text(json.dumps({
        "captured": time.strftime("%Y-%m-%d"),
        "source": "dev.to /api/tags and /api/articles?tag=",
        "note": (
            "Ranks move daily and are recorded for information. The tests assert the "
            "classification — ranked, unranked-but-used, unused — which does not."
        ),
        "taxonomy_size": len(ranked),
        "top_ten": ranked[:10],
        "observations": observations,
    }, indent=2) + "\n")
    print(f"wrote {OUT}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
