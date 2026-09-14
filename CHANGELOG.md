# Changelog

## 0.2.2

### An oracle for the tag tools

The text tools are checked against four things that are not this server — Python `textstat`,
`pyphen`, Ruby's own `split`, and the reading time dev.to reports for thirty real articles.
They have been essentially bug-free. The tag tools were checked only against mocks written
from the same assumptions as the code, and every wrong answer they have given lived in exactly
that gap.

`crates/devto-mcp/fixtures/tag_observations.json` now records what dev.to says about
twenty-five real tags: fourteen ranked, eight real but outside the ranked head, and three that
nothing has ever been published under. `scripts/refresh-tag-oracle.sh` re-captures it, and
`git diff` afterwards is dev.to changing its mind.

The parity test requires our classification to reproduce dev.to's, tag for tag. Restoring the
0.2.0 behaviour makes it fail with *"emacs has articles on dev.to but was not reported as
real"* — the bug named by the tag it got wrong, rather than by a mock.

Ranks are recorded but deliberately not asserted. They move daily, and a fixture that fails
every week teaches everyone to ignore it; the classification is what holds still.

### Fixed

- **Grounding matched inside longer words.** `ai` was reported three times in an article that
  never uses the word — it occurs inside "again" and "against". The squashed comparison that
  lets `machinelearning` match "machine learning" now has to line up with word boundaries in
  the original. Found by running the tool on its own tutorial.
- **Tag overlap was a yes-or-no and told you nothing.** Asking whether *any* article carries
  both tags is true of almost every popular pair — it flagged five of six. It reports a share
  now: `mcp` and `ai` co-occur on 69% of the sample, `api` and `writing` on none.

## 0.2.1

### Fixed: `/api/tags` is a ranking, not a census

0.2.0 treated absence from `/api/tags` as proof a tag did not exist. It is not. The endpoint
returns roughly 1,285 tags ordered by popularity and stops; dev.to has many more. `emacs`,
`devsecops`, `healthcare` and `engineering` all carry hundreds of articles and none of them
appear in it.

So `check_tag_fit` reported real, working tags as invented — confidently, and with a remedy
attached. It now distinguishes three outcomes rather than two: **ranked** (with its
position), **real but unranked** (confirmed by asking whether any article carries the tag,
one request, only for tags that are not ranked), and **unused** (nothing carries it, so
publishing invents a dead tag). `author_profile` reports "outside the ranked head" and says
plainly that this is not the same as unused.

The claim that shipped with 0.2.0 — that 24 of one author's 311 tag slots had gone to tags
that do not exist — was wrong. Checked properly, every one of those tags is real and in use,
fourteen of them on 100+ articles. The correct figure for that author is zero.

## 0.2.0

### `check_tag_fit`

An article gets four tags and they drive nearly all of its discovery. dev.to creates a tag on
demand rather than refusing one it has never seen, so an invented tag looks exactly like a
working one and reaches nobody — and nothing on the platform says otherwise.

The tool reports what only a server with both halves can know: where a tag ranks among the
~1,285 dev.to ranks by popularity (position is the only reach signal the API carries — there
are no article or follower counts), whether the article's own prose actually uses the term,
and how often two candidates appear together on real articles. Two tags with heavy overlap
are buying one audience with two slots.

It reports and does not choose, the same rule the text tools follow.

### Fixed

- **Draft bodies were unreachable.** `my_articles` discarded `body_markdown`, which the API
  does send for unpublished work, and `get_article` returns 404 for a draft. That left the
  text tools unable to see a draft at all — exactly when they are worth running. Now behind
  `include_body`, because a hundred articles' markdown is an enormous reply.
- **A tag note fired for the wrong reason.** "Forem downcases every tag" compared
  `raw.to_lowercase()` against the stored value, which hides the change it describes; it could
  only ever trigger when quotes had been stripped.
- **Releases shipped an unlabelled binary.** v0.1.0 published a `devto-mcp` asset with no
  platform in its name, and it was Mach-O x86_64. Four targets build a file with that exact
  name and the release job collected the raw artifacts alongside the packaged ones. The job
  now takes only archives and the bundle, and refuses to publish anything else.
- **Intel macOS builds never ran.** `macos-13` was retired on 2025-12-04; a retired label does
  not fail, it queues forever, so the release never reached its publish step. Now
  `macos-15-intel`, and every job has a timeout so the next retirement fails loudly.

## 0.1.0

First release. An MCP server for authorship on DEV (dev.to), with the platform's rules built
in so a rejection costs no request.

### Tools

**Free — no network, no rate budget**

- `validate_draft` — 26 rules dev.to enforces, checked offline
- `analyze_readability` — six scores over the prose, with the share measured stated
- `analyze_structure` — outline and its gaps, paragraph lengths, code and link density
- `forem_reading_time` — the figure dev.to will print, and the one over prose alone
- `whoami` — account, permissions, remaining rate budget

**Reading** — `my_articles` (the only route to your own drafts), `get_article`,
`read_comments`, `list_tags`, `my_analytics`, `search_articles`, and `check_tag_fit`, which
measures candidate tags against the live taxonomy: whether they exist at all, how far they
reach, whether the article's own prose supports them, and whether two of them are buying the
same audience twice

**Writing** — `create_draft`, `update_article`, `publish_article`, `unpublish_article`, each
validated locally first

Deliberately absent, because they are impossible rather than unimplemented: writing comments,
reactions, image upload, and delete. See the README for why each one is out.

### What it is built on

- **Dual-era MCP.** Revision `2026-07-28` removed the `initialize` handshake, but the clients
  that exist today still open with it and cannot fall forward. The era is decided by how the
  client opens and held for the process.
- **Verified against independent oracles, not against itself.** Text metrics reproduce Python
  `textstat` 0.7.13 byte-for-byte over 58 documents, hyphenation matches `pyphen` over 20,854
  words, Forem's word count matches Ruby's `split(/\W+/)`, and the reading time matches what
  dev.to itself reports on 30 published articles — 30 of 30.
- **A mutation gate at zero survivors.** 1,121 mutants, none missed, none timed out, and no
  exclusions: every mutant is killed by a test or gone because the code it sat on was dead.

### Notes

- Self-contained: CMUdict and the en_US hyphenation patterns are compiled into the binary, so
  there is nothing to fetch at runtime. That is most of the 5.3 MB.
- Gunning Fog, Dale–Chall and Spache are absent on purpose — all three need a word list whose
  1948 provenance is not worth the licensing question for one metric.
- An API key is generated by hand at dev.to/settings/extensions; there is no programmatic way
  to create one. Without a key the server still runs, serving public reads and the validator.
