# Anthropic directory submission packet

Everything the desktop-extension form asks for, ready to paste. Submitted through
<https://clau.de/desktop-extention-submission> — MCPB bundles do not use the remote-server
portal and need no Team or Enterprise organisation.

Regenerate the figures here with `./scripts/gate.sh` and the release you are submitting.

---

## Artifact

| Field | Value |
|---|---|
| Bundle | `devto-mcp-v0.2.1.mcpb` from [the v0.2.1 release](https://github.com/copyleftdev/devto-mcp/releases/tag/v0.2.1) |
| SHA-256 | `32725d84a5e28447c51e0dd73df2ada4dda7f4e4b94a65ea1002386d19f50e13` |
| Icon | `mcpb/icon.png` — 512×512 PNG, the size the bundle validator recommends. `mcpb/icon-1024.png` if a larger one is wanted |
| Repository | <https://github.com/copyleftdev/devto-mcp> (public; MCPB requires open source) |
| Licence | MIT OR Apache-2.0 |

## Listing

**Name**

    DEV (dev.to) authorship

**Tagline** (55 characters max — this is 52)

    Write for dev.to with its rules checked offline

**Description**

    An MCP server for writing on DEV (dev.to), built around two things the platform does not
    give you.

    The first is that every rule dev.to enforces is checked on your machine before anything is
    sent. Tag characters, the byte-counted body limit, the title limit measured with whitespace
    stripped, front matter that silently overrides the payload — 26 rules, offline, costing no
    request and no rate budget. You iterate until the report is clean instead of discovering the
    rules one rejection at a time.

    The second is that its text tools measure the prose rather than the document. A readability
    score computed over a dev.to post is mostly measuring shell transcripts, so every score
    arrives with the share of the article it was computed over. Across one author's 30 published
    articles, 23.8% of the words dev.to counts as text are not prose, and counting them inflates
    Coleman-Liau by 2.26 grade levels on average.

    The numbers are checked against something other than themselves. Counts and readability
    reproduce Python textstat 0.7.13 byte-for-byte over 58 documents, hyphenation matches pyphen
    over 20,854 words, the Forem word count matches Ruby's own split, and the reading time
    matches what dev.to reports for 30 real articles — 30 of 30. The fixtures are in the
    repository and CI re-checks them.

    Writing tools validate locally first, so a rejection costs no request. Publishing is off
    unless you turn it on. Four things are deliberately absent because they are impossible
    rather than unimplemented: writing comments, reactions, image upload and delete — dev.to has
    no endpoint for any of them.

**Categories** — Developer tools; Productivity; Writing

**Documentation** <https://github.com/copyleftdev/devto-mcp#readme>

**Privacy policy** <https://github.com/copyleftdev/devto-mcp/blob/main/PRIVACY.md>

**Support** <https://github.com/copyleftdev/devto-mcp/issues>

## Tools — 17, all annotated

Read-only (13): `analyze_readability`, `analyze_structure`, `author_profile`, `check_tag_fit`,
`forem_reading_time`, `get_article`, `list_tags`, `my_analytics`, `my_articles`,
`read_comments`, `search_articles`, `validate_draft`, `whoami`

Write (4): `create_draft` (not destructive — it creates), `update_article`, `publish_article`
and `unpublish_article` (all destructive: an overwritten body cannot be recovered through the
API, and publishing cannot un-send the feed and RSS entries that unpublishing hides).

Four of the reads touch no network at all: `validate_draft` and the three text tools.

## Example prompts

Each exercises a different tool, and the first three need no account.

1. *"Check this draft before I send it: [paste title, body and tags]"* — `validate_draft`.
   Offline; reports every rule dev.to would reject on, plus the traps that do not error at all.
2. *"How readable is this post, and how much of it is actually prose rather than code?"* —
   `analyze_readability` and `analyze_structure`.
3. *"dev.to says this is a 14 minute read. Is it?"* — `forem_reading_time`, which reproduces
   Forem's own calculation and reports the prose-only figure beside it.
4. *"I'm considering tagging this rust, devsecops, ai and webdev. Which are worth a slot?"* —
   `check_tag_fit`. Needs a key.
5. *"What does copyleftdev write about, and where do their tags land?"* — `author_profile`.
   Needs a key.
6. *"Create this as a draft on my account"* — `create_draft`, validated locally first and
   always unpublished.

## Test account

The reviewer needs a dev.to API key, from **Settings → Extensions → DEV Community API Keys**.
There is no programmatic way to create one.

Supply a key for an account with **at least one published article and one unpublished draft** —
`my_articles`, `my_analytics` and `author_profile` all read thinner than they should on an
empty account, and `get_article` returns 404 for a draft by design.

Without a key the server still runs and serves public reads plus the whole offline validator,
so prompts 1–3 above work with no credentials at all.

Publishing is gated separately: `DEVTO_PUBLISH` must be set before `publish_article` will do
anything, so a reviewer cannot publish to the account by accident.

## Data handling — read this before submitting

**The underlying API is not ours.** This is an independent client of dev.to's public,
documented API. It is not affiliated with DEV or Forem, proxies nothing, and holds no
credentials of its own — the user supplies their own key, which goes only to the instance they
configure.

The review criteria say a server "must call your own first-party APIs, or APIs you legitimately
proxy". The remote-server portal has an explicit option for a third party's API you do not
control, so the category exists; whether it is accepted for an MCPB bundle is not stated
publicly. **Worth asking `mcp-review@anthropic.com` before submitting** rather than spending a
review cycle on it.

Nothing else in the unsupported list applies: no financial transactions, no AI media generation,
no conversation data collected, no access to memory or chat history.
