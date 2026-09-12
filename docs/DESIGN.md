# `devto-mcp` — Design

**Status:** design, pre-implementation
**Grounded in:** [`FINDINGS.md`](FINDINGS.md) — every constraint cited here was read from `forem/forem` @ `ac54b3b` or observed live on 2026-09-12.

## Decisions

| | |
|---|---|
| **Scope** | Author's own workflow **+ research over other people's writing** |
| **Publish authority** | Read and draft freely; **publishing requires a separately-configured capability** |
| **Language** | Rust |
| **Reach** | dev.to is the tested target; base URL and instance settings are not hard-coded |

---

## 1. Thesis

Ninety-nine endpoints are not the product. The product is the component that knows everything a language model gets wrong about publishing to dev.to — and there are a lot of those, because the platform's real rules live in Rails validators, Pundit policies and a Rack::Attack initializer, none of which appear in the OpenAPI description a code generator would consume.

Four properties follow, and everything below is in service of them:

1. **The transport is correct by construction** — right version header, honest identity, paced inside a budget that is genuinely small.
2. **A write is never spent on a preventable 422** — the constraint table is mirrored locally and checked first.
3. **Governance is in the schema, not the README** — disclosure is a required argument; publishing is a capability, not a default.
4. **The knowledge the schema can't carry is served as resources and prompts** — 79 liquid tags, the front-matter mapping, the scheduling rules.

## 2. Crate layout

A pure core with I/O pushed to the edges, matching the house pattern.

```
devto-core       types + the validation engine. No I/O, no async, no clock.
                 The constraint table from FINDINGS.md §5 lives here and
                 nowhere else. Property-testable in isolation.

devto-knowledge  liquid tag catalogue, front-matter ↔ API field mapping,
                 the governance text. Compiled in via include_str!, with a
                 refresh task that re-derives it from a Forem checkout.

devto-client     HTTP. Version header, User-Agent, token bucket, Retry-After,
                 response caching, error mapping. Knows nothing about MCP.

devto-mcp        the server: dual-era protocol, tool/resource/prompt surface,
                 the capability gate.

devto-cli        thin binary over devto-client for exercising the thing
                 without a model in the loop. Also how the live test suite runs.
```

`devto-core` having no clock matters: `published_at` validation is time-dependent (future, or within the last 15 minutes), so the current time is an argument, which makes the rule testable at boundaries instead of flaky.

## 3. Transport

### Version and identity

Every request carries, without exception:

```
Accept: application/vnd.forem.api-v1+json
User-Agent: devto-mcp/<version> (+<repo url>)
api-key: <secret>            # authenticated calls only
```

The version header is asserted in a test, not left to a constant that someone edits. Getting it wrong doesn't fail loudly — it silently downgrades to deprecated V0, which is the worst failure mode available.

### Budget

One shared limiter, because dev.to throttles the IP and the key *simultaneously* — two processes on one machine do not get two budgets.

| Bucket | Rate | Source |
|---|---|---|
| reads | 30/min, burst 3/s | `api_throttle`, `api_key_throttle` |
| writes | 1/s | `api_write_throttle` |

On top of that, the per-user application limits (publish 9/30s, or **1 per 5 min** on accounts under three days old; update 30/30s) are tracked optimistically and surfaced rather than enforced — they're instance-configurable and we can't read the real numbers from outside.

`Retry-After` is obeyed exactly. A 429 is not retried silently more than once; the second one is reported to the model as a tool error with the wait time, because a model that knows it's rate-limited can do something useful with that, and a model that's being stalled inside a tool call cannot.

**Coalescing is a design rule, not an optimization.** `/api/analytics/dashboard` returns five panels in one request and exists in Forem specifically to keep clients under the 3 GET/sec throttle. Any tool that would issue N requests where a bundled endpoint exists uses the bundled endpoint.

### Caching

Read-through, in-memory, keyed on the full request:

| Data | TTL | Why |
|---|---|---|
| `/api/users/me` | session | Identity doesn't change mid-session |
| `/api/tags` | 24 h | Taxonomy moves slowly and is needed on every tag validation |
| `/api/instance`, `/api/subforems` | 24 h | Instance shape |
| article reads | 60 s | Enough to absorb a model re-reading what it just fetched |
| analytics | none | `Cache-Control: no-store` upstream; honour it |

## 4. Validation engine — `devto-core`

The flagship. A pure function from a draft payload plus a clock to a verdict, mirroring what Rails will do.

```rust
pub struct Draft { /* title, body_markdown, tags, … */ }

pub enum Severity { Blocking, Warning }

pub struct Finding {
    pub field: Field,
    pub severity: Severity,
    pub rule: RuleId,        // stable id, e.g. TAG_NON_ALNUM
    pub message: String,     // what is wrong
    pub remedy: String,      // what to do instead
}

pub fn validate(draft: &Draft, now: DateTime<Utc>, ctx: &Context) -> Vec<Finding>;
```

Rules implemented, each traceable to a source line (see `FINDINGS.md` §5):

**Blocking** — body over 800 KB *measured in bytes*; title over 128 chars *measured with all whitespace stripped* (256 for `status`); more than four tags; a tag that isn't `[[:alnum:]]{1,30}` — which makes `machine-learning` invalid and is the single most likely thing a model gets wrong; joined tag list over 126 chars; a canonical URL that is non-http(s), local, or contains whitespace; a cover image that isn't an http(s) URL; a `video_source_url` outside the YouTube / Mux / Twitch allowlist; `published_at` in the past by more than 15 minutes on a create.

**Warning** — `published_at` supplied for an already-published article, because `Articles::Updater` *silently drops* it and the caller will otherwise believe a reschedule happened; a canonical URL that collides with one of the author's own published posts (checkable, and a guaranteed 422); a title matching one of the author's posts from the last five minutes, which turns an innocent retry into a duplicate-title rejection; front matter carrying `cover_image` while the payload carries `main_image`, or vice versa.

Two properties the test suite should hold, in the `hegel` style:

- *validate-then-send never produces a 422 from a modelled rule.* Generate drafts, keep the ones `validate` passes, assert the recorded-cassette responses for those payloads are 2xx.
- *every finding's remedy, applied, clears that finding.* This is what makes the tool useful to a model iterating rather than merely correct.

Gate: `cargo mutants` at **0 survivors** on `devto-core`, per house standard. The validator is exactly the kind of code that passes tests while being wrong.

## 5. Tool surface

Twelve tools. Grouped by what they cost, because that's what the model needs to reason about.

### Free — no network

| Tool | Notes |
|---|---|
| `validate_draft` | Runs the engine standalone. **Zero quota.** A model can iterate on a draft until clean before spending a single write. Also runs implicitly inside every write tool. |
| `whoami` | Identity, org memberships, **which capabilities this server has enabled**, and remaining rate budget. A model that knows it may not publish gives the user a useful answer instead of a 401. |

### Reads

| Tool | Endpoint | Notes |
|---|---|---|
| `my_articles` | `/articles/me/{status}` | `status: published \| unpublished \| all`. The only route that returns drafts. |
| `get_article` | `/articles/{id}` or `/{username}/{slug}` | Returns `body_markdown` and metadata. |
| `search_articles` | `mode: semantic \| keyword \| feed` | One tool, three backends. `semantic` needs V1 + auth and returns cosine distance and similarity; `feed` takes `tag`/`tags_exclude`/`username`/`state`/`top`/`collection_id`. Collapsing these into one tool keeps the model from picking the wrong search. |
| `read_comments` | `/comments?a_id=` | Read only — the API has no write path, and llms.txt asks clients not to automate commenting. The tool description says so, so a model doesn't go looking for a reply tool. |
| `my_analytics` | `/analytics/dashboard` et al | `view: dashboard \| historical \| referrers \| heatmap`. Defaults to the bundled endpoint. |
| `list_tags` | `/tags` | Taxonomy, cached 24 h, feeds tag validation. |

### Writes — 1/sec, and validated first

| Tool | Notes |
|---|---|
| `create_draft` | `published: false` always. `ai_disclosure_level` **required, no default**. |
| `update_article` | Partial update. Refuses to send `published_at` on a published article and says why rather than letting Forem drop it. |
| `publish_article` | **Gated.** Sets `published: true`, optionally schedules via `published_at`. Returns the live URL. |
| `unpublish_article` | `PUT published: false` — *not* the `/unpublish` endpoint, which is admin-only. |

Deliberately absent: reactions (admin-only on a normal key — shipping it would only produce 401s), comment writing (no endpoint exists), image upload (session-auth only), delete (no route), co-authors (not in API params).

## 6. Governance, encoded

### Disclosure is a required argument

`ai_disclosure_level` has no default on `create_draft` or `publish_article`. The caller must state it, and the tool description carries dev.to's own definitions verbatim rather than paraphrased.

**`no_ai` requires its own configuration allowance**, defaulting off. The reasoning: a model calling this tool cannot honestly certify that the content was written without meaningful AI assistance, and llms.txt is explicit that a human merely reviewing autonomous output doesn't make it human-authored. If a human genuinely drafted the piece and is using the server only as a transport, they turn the allowance on deliberately. Refusing by default costs an honest human one config line; allowing by default makes the server a tool for laundering provenance.

### Publishing is a capability

```
DEVTO_API_KEY=...              # required for anything authenticated
DEVTO_PUBLISH=enabled          # default: disabled
DEVTO_ALLOW_NO_AI_CLAIM=true   # default: false
DEVTO_BASE_URL=https://dev.to  # default
```

The platform offers no scoped key — one `api-key` carries the account's entire identity and every privilege it holds. The server is therefore the only place a least-privilege boundary can exist, and that's the whole argument for building this rather than handing a model a `curl` wrapper.

`publish_article` stays **listed** when disabled and returns a tool error naming the config change. Hiding it makes the model conclude publishing is impossible; listing it lets the model tell the user exactly what to turn on.

### Identity

The `User-Agent` names the server and links the repo. llms.txt asks clients to identify accurately; this costs nothing and is the difference between a good-faith client and an anonymous one.

## 7. Resources and prompts

The half of MCP every existing dev.to server left empty, and where the research pays off most.

**Resources**

| URI | Contents |
|---|---|
| `devto://liquid-tags` | All 79 tags with argument shapes and accepted URL forms, plus the `unified_embed` fallback. Nothing in the OpenAPI description hints these exist. |
| `devto://frontmatter` | The eight front-matter keys and their API-field equivalents — including the `cover_image` / `main_image` asymmetry that silently loses images. |
| `devto://constraints` | The validator's rule table, human-readable, so a model can reason about limits before drafting instead of after failing. |
| `devto://governance` | Live `llms.txt` plus the disclosure ladder, cached, with the captured 2026-09-12 text as a pinned fallback. |
| `devto://tags` | Current taxonomy snapshot with popularity and which tags require approval. |

**Prompts**

- `draft-post` — scaffolds a piece: tags validated against the taxonomy, disclosure level asked for explicitly, front matter emitted in the correct spelling.
- `pre-publish-review` — runs `validate_draft`, checks canonical URL collisions and series placement, and surfaces the scheduling rules before the write.
- `post-performance` — pulls the analytics bundle in one call and reads it against the author's own baseline rather than in the abstract.

## 8. Protocol era

Ship **dual-era**. The current revision is `2026-07-28`, but Claude Code still opens with `initialize` at `2025-11-25` and has no fall-forward, so a spec-perfect modern-only server fails to connect outright. Pick the era from how the client opens and hold it for the life of the stdio process; put the same `instructions` string in both the `initialize` result and `server/discover`.

Verify with `claude mcp list`, which actually spawns and probes the server. Hand-rolled protocol tests pass while the real client can't connect.

Business-logic failures — a validation finding, a rate-limit wait, a disabled capability — return `isError: true` in a normal result, not a JSON-RPC error, so the client hands them back to the model to self-correct. Protocol errors are reserved for what a model cannot fix.

## 9. Testing

| Layer | Approach | Gate |
|---|---|---|
| `devto-core` | Property tests over generated drafts | `cargo mutants` **0 survivors** |
| `devto-client` | Recorded cassettes from real dev.to responses; a small live suite behind a feature flag | Deterministic replay |
| Rate limiter | Simulated clock; assert the bucket never exceeds 30/min or 1 write/s under load | — |
| Protocol | `claude mcp list` against the built binary, both eras | Connects |
| End-to-end | `devto-cli` drafts → validates → publishes → unpublishes against a real account | Manual, gated on the key |

## 10. Phasing

| Phase | Deliverable | Blocked on |
|---|---|---|
| **0** | `devto-core`: types, validator, property tests, 0 mutation survivors | — |
| **1** | `devto-client`: transport, budget, caching, error mapping; verified against public read endpoints | — |
| **2** | `devto-mcp`: dual-era protocol, read tools only, `claude mcp list` green | — |
| **3** | Write tools behind the capability gate; governance enforcement | **DEV API key** |
| **4** | `devto-knowledge`: resources and prompts | — |
| **5** | Public release: README, install, TokenTip badge | — |

**Phase 0–2 are unblocked and can start now.** Phase 3 needs a human step nothing here can automate: minting a key at `https://dev.to/settings/extensions`. There is no DEV credential on this machine today — `~/.creds` has nothing for dev.to or Forem.

## 11. Risks

| Risk | Mitigation |
|---|---|
| Instance settings are invisible from outside — real rate-limit numerals, `enable_ai_disclosure`, `enable_agent_sessions` | Treat source defaults as estimates; probe `/api/instance` at startup; degrade rather than assume |
| Forem ships fast; `main` moved the day we cloned it | Pin the researched commit; a refresh task re-derives the liquid-tag catalogue and constraint table from a checkout and diffs them |
| The OpenAPI description omits writable fields (`published_at`, `subforem_id`, `language`, `video_source_url`) | Never generate the client from the spec. The spec is a cross-check, the source is the contract |
| Images can't be uploaded through the API at all | Document the wall; `validate_draft` flags a non-URL cover image with a remedy. An uploader adapter is a later, separable decision |
| Disclosure enforcement is unenforceable in principle — a caller can always lie | Don't pretend otherwise. Require the field, default the honest-human claim to off, and document the position |
