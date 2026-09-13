#!/usr/bin/env python3
"""Regenerate the parity fixtures from textstat and pyphen.

Run through ``scripts/refresh-parity.sh``, which builds the pinned virtualenv first.

**The inputs are the source of truth, not the expectations.** Each fixture already holds the
texts and words it covers; this script reads those back, recomputes every expectation from the
oracle, and rewrites the file. So re-running it after a textstat release answers exactly one
question — did an answer change? — and ``git diff crates/devto-text/tests/fixtures`` is the
answer.

Two fixtures are not regenerated here because their oracle is not Python:

  forem_reading_time.json   dev.to's own API; see scripts/refresh-knowledge.sh
  ruby_word_counts.json     Ruby's String#split; regenerated below only if `ruby` is present

New cases go in EXTRA_CASES. Nothing else in this file needs editing to add one.
"""

from __future__ import annotations

import json
import pathlib
import shutil
import subprocess
import sys

FIXTURES = pathlib.Path(__file__).resolve().parent.parent / "crates/devto-text/tests/fixtures"

# Texts to add to the textstat fixtures if they are not already covered. Degenerate inputs
# belong here: they are where an implementation and its oracle part company, and where the
# guards in readability.rs either fire or do not.
EXTRA_CASES: list[str] = [
    "...",           # punctuation only: zero words, but one sentence
    "' ' '",         # apostrophes only: textstat counts no words at all
    "'",
    "   ",           # whitespace only
    "!?!?",
    "a",             # a single one-letter word
    "I am.",         # the shortest thing with two real words
]


def load(name: str):
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


def dump(name: str, value) -> None:
    """One entry per line.

    The whole point of this script is that `git diff` afterwards tells you whether an oracle
    changed its mind. A single-line JSON blob reports that as one changed line covering
    twenty thousand words, which answers nothing; indented JSON spreads one number over four
    lines. One entry per line puts exactly the changed cases in the diff.
    """
    if isinstance(value, dict):
        body = ",\n".join(
            f"{json.dumps(k, ensure_ascii=False)}: {json.dumps(v, ensure_ascii=False)}"
            for k, v in value.items()
        )
        text = "{\n" + body + "\n}\n"
    else:
        body = ",\n".join(json.dumps(item, ensure_ascii=False) for item in value)
        text = "[\n" + body + "\n]\n"
    (FIXTURES / name).write_text(text, encoding="utf-8")
    print(f"  wrote {name}")


def texts_to_cover() -> list[str]:
    """Every text the counts fixture already covers, plus any new ones."""
    seen = [case["text"] for case in load("textstat_counts.json")]
    for text in EXTRA_CASES:
        if text not in seen:
            seen.append(text)
    return seen


def regenerate_textstat(texts: list[str]) -> None:
    import textstat

    counts = [
        {
            "text": t,
            "words": textstat.lexicon_count(t),
            "sentences": textstat.sentence_count(t),
            "syllables": textstat.syllable_count(t),
            "letters": textstat.letter_count(t),
            "polysyllables": textstat.polysyllabcount(t),
            "monosyllables": textstat.monosyllabcount(t),
        }
        for t in texts
    ]
    dump("textstat_counts.json", counts)

    readability = [
        {
            "text": t,
            "flesch_reading_ease": textstat.flesch_reading_ease(t),
            "flesch_kincaid_grade": textstat.flesch_kincaid_grade(t),
            "smog_index": textstat.smog_index(t),
            "coleman_liau_index": textstat.coleman_liau_index(t),
            "automated_readability_index": textstat.automated_readability_index(t),
            "mcalpine_eflaw": textstat.mcalpine_eflaw(t),
        }
        for t in texts
    ]
    dump("textstat_readability.json", readability)


def regenerate_pyphen() -> None:
    import pyphen

    dic = pyphen.Pyphen(lang="en_US")
    words = sorted(load("pyphen_positions.json"))
    dump("pyphen_positions.json", {w: dic.positions(w) for w in words})


def regenerate_ruby() -> None:
    """Forem's word count is Ruby's, so Ruby is the only oracle that settles it."""
    if not shutil.which("ruby"):
        print("  skipped ruby_word_counts.json — no `ruby` on PATH")
        return
    cases = load("ruby_word_counts.json")
    texts = [c["text"] for c in cases]
    script = (
        "require 'json'\n"
        "STDOUT.write(JSON.generate(JSON.parse(STDIN.read).map { |t| "
        "{ 'text' => t, 'count' => t.split(/\\W+/).count } }))\n"
    )
    out = subprocess.run(
        ["ruby", "-e", script],
        input=json.dumps(texts),
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    dump("ruby_word_counts.json", json.loads(out))


def main() -> int:
    print(f"regenerating fixtures in {FIXTURES}")
    texts = texts_to_cover()
    print(f"  {len(texts)} textstat cases ({len(texts) - 52 if len(texts) > 52 else 0} new)")
    regenerate_textstat(texts)
    regenerate_pyphen()
    regenerate_ruby()
    print("done — review with: git diff crates/devto-text/tests/fixtures")
    return 0


if __name__ == "__main__":
    sys.exit(main())
