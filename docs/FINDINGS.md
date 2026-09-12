# DEV / Forem — Platform Capability Map for an Authorship MCP Server

**Research date:** 2026-09-12
**Primary source:** `forem/forem` @ `ac54b3b29af145ea947eb7f5d6acba8a86c9d109` (main, 2026-09-10), AGPL-3.0
**Corroborating source:** live probes against `https://dev.to` (unauthenticated)
**Evidence:** `evidence/spec/api_v1.json`, `evidence/live/*`, hashes in `evidence/SHA256SUMS`

---

## 0. What was verified, and how

Three independent source layers were read, and they agree:

| Layer | Artifact | What it settles |
|---|---|---|
| Published contract | `GET https://dev.to/api/v1/openapi.json` — OpenAPI 3.0.3, 99 paths, 40 schemas | The advertised surface |
| Source of truth | Rails controllers, models, policies, services, routes | What the server actually enforces |
| Live instance | HTTP probes of dev.to | Whether dev.to runs this build |

**dev.to tracks `main` closely.** The live `openapi.json` is byte-for-byte the same size (255,465 B) as the one in the repo at the commit above. Everything in this document is therefore live behaviour, not aspirational upstream code.

**Two caveats worth carrying forward.** Forem is a multi-tenant product: several behaviours are driven by per-instance `Settings::*` rows that are not visible from outside (rate-limit numerals, `enable_ai_disclosure`, `enable_agent_sessions`). Source-read defaults are stated as defaults, not as dev.to's configured values. And dev.to's limits were observed from the anonymous edge only — an authenticated probe was not run, because no DEV API key exists on this box yet.

---

## 1. The single most consequential fact: API versioning is opt-in via `Accept`

```ruby
# app/lib/api_constraints.rb
def matches?(req)
  @default || req.headers["Accept"]&.include?("application/vnd.forem.api-v#{@version}+json")
end
```

- **V0 is the default.** Any request without the header is routed to `Api::V0::*` and comes back with a `Warning: 299` deprecation header — confirmed live.
- **V1 requires** `Accept: application/vnd.forem.api-v1+json`. There is no `/api/v1/...` path prefix; the URL is identical. Getting the header wrong silently downgrades you to the deprecated controller.
- V1-only capabilities that V0 simply does not route: `articles#unpublish`, `articles#semantic_search`, `users#search`, `reactions`, `billboards`, `segments`, `pages`, `agent_sessions`, `concepts`, `recommended_articles_lists`, `feedback_messages`, the whole `admin/*` namespace, and `organizations` create/update/destroy.

> **Design implication #1.** The version header is a correctness invariant, not a configuration option. It belongs in a single transport layer that every tool goes through, asserted in tests. Both existing dev.to MCP servers on GitHub speak bare V0.

## 2. Authentication and authorization

```ruby
def authenticate_with_api_key
  api_key = request.headers["api-key"]
  api_secret = ApiSecret.includes(:user).find_by(secret: api_key)
  ActiveSupport::SecurityUtils.secure_compare(api_secret.secret, api_key) && api_secret.user
end
```

- Single header: `api-key: <secret>`. No OAuth, no refresh, no expiry, no scopes.
- Keys are minted by hand at `https://dev.to/settings/extensions`. **This is a manual, human, out-of-band step** — the MCP server cannot bootstrap itself.
- **The key is all-or-nothing.** It carries the user's full identity and every privilege that account holds. There is no read-only key, and no way to grant "may draft but may not publish."
- Authenticated endpoints are CORS-disabled by design — the key is for non-browser clients.
- `spam_or_suspended?` users are rejected at the controller regardless of key validity.

> **Design implication #2.** Because the platform offers no scoping, *the MCP server is the only place a least-privilege boundary can exist.* An expert server should implement its own capability gate (e.g. read-only by default; publish requires an explicit, separately-configured allowance), rather than exposing a raw key to a model.

## 3. Complete capability map

Legend: **W** = writable with a normal user key · **R** = readable · **A** = admin key only · **—** = not exposed to the API at all.

### Authorship core

| Capability | Endpoint | Access | Notes |
|---|---|---|---|
| Create article/draft | `POST /api/articles` | **W** | Drafts are `published: false` |
| Update article | `PUT /api/articles/{id}` | **W** | Also the publish/unpublish lever via `published` |
| Unpublish | `PUT /api/articles/{id}/unpublish` | **A** | `ArticlePolicy#revoke_publication?` requires an elevated user — *normal authors unpublish by `PUT` with `published: false`* |
| List own articles | `GET /api/articles/me[/published\|/unpublished\|/all]` | **R** | The only way to enumerate drafts; includes `page_views_count` |
| Read published article | `GET /api/articles/{id}`, `/{username}/{slug}` | **R** | Public; returns `body_markdown` + `processed_html` |
| Schedule | `published_at` on create | **W** | Undocumented in the OpenAPI schema; see §5 |
| Series | `series` param → `collection_id` | **W** | Created on demand by name |
| Post as organization | `organization_id` | **W** | Requires `OrganizationMembership` or org-admin |
| Cover image | `main_image` (URL) | **W** | URL only — see §7 |
| Video post | `video_source_url` | **W** | YouTube / Mux / Twitch URLs only, hard-coded regex |
| Co-authors | `co_author_ids_list` | **—** | Permitted in the web controller, **not** in the API params |

### Audience & measurement

| Capability | Endpoint | Access | Notes |
|---|---|---|---|
| Analytics totals / historical / referrers / top-contributors / follower-engagement | `GET /api/analytics/*` | **R** (auth) | Per-article or per-org scoping; `start` required on `historical` |
| Bundled dashboard | `GET /api/analytics/dashboard` | **R** (auth) | One call replaces five — explicitly built to dodge the 3 GET/sec throttle |
| 365-day activity heatmap | `GET /api/analytics/heatmap` | **R** (auth) | Always personal-scoped |
| Followers | `GET /api/followers/users`, `/organizations` | **R** (auth) | |
| Reading list | `GET /api/readinglist` | **R** (auth) | |
| Own profile | `GET /api/users/me` | **R** (auth) | |

### Discovery & research

| Capability | Endpoint | Access | Notes |
|---|---|---|---|
| Article feed / filters | `GET /api/articles` | **R** | `tag`, `tags`, `tags_exclude`, `username`, `state=fresh\|rising\|all`, `top=<days>`, `collection_id`, `sort` |
| Keyword search | `GET /api/articles/search` | **R** | Built for the old ChatGPT plugin; returns `body_markdown` only when exactly one hit |
| **Semantic search** | `GET /api/articles/semantic_search` | **R** (auth, V1) | Embeddings + Algolia, fused by Reciprocal Rank Fusion (k=60) with recency and quality boosts; returns `distance` + `similarity`; `threshold` 0.0–2.0 cosine |
| Tags | `GET /api/tags` | **R** | Ordered by popularity |
| Followed tags | `GET /api/follows/tags` | **R** (auth) | Returns follow `points` — i.e. personal tag weighting |
| Trends / concepts | `GET /api/trends`, `/api/concepts` | **R** | Newer editorial-clustering surface |
| Subforems | `GET /api/subforems` | **R** | dev.to now hosts sub-communities; `subforem_id` is a writable article param |
| Comments | `GET /api/comments?a_id=` | **R** | **read-only, see §7** |
| Videos, podcast episodes, org articles, profile images, instance meta | various | **R** | |

### Adjacent / platform

| Capability | Access | Notes |
|---|---|---|
| Follow users & orgs (`POST /api/follows`) | **W** | Async via worker; 500/day default cap |
| Surveys & polls (`/api/surveys`) | **W** | Create polls, read `poll_votes` / `poll_text_responses` — a genuine authorship feature, embedded via `{% poll %}` / `{% survey %}` |
| Reactions (`POST /api/reactions`, `/toggle`) | **A** | `ReactionPolicy#api?` returns true **only for admins** — see §7 |
| Billboards / display ads, audience segments, pages, badges | **A** | Publisher-operator surface, not author surface |
| Agent sessions (`/api/agent_sessions`) | **W** (if enabled) | Uploads an agent transcript to S3, gets a slug, embeddable via `{% agent_session %}` — see §6 |
| Admin: users, notes, identities, redirects, feedback | **A** | |

## 4. The article payload, exactly

`params.require(:article).permit(...)` in `app/controllers/concerns/api/articles_controller.rb`:

| Field | Type | In OpenAPI? | Notes |
|---|---|---|---|
| `title` | string | yes | Required |
| `body_markdown` | string | yes | May itself carry YAML front matter |
| `published` | bool | yes | Default `false` |
| `series` | string | yes | Series (collection) name |
| `main_image` | URL | yes | Cover image |
| `canonical_url` | URL | yes | |
| `description` | string | yes | SEO / card blurb |
| `tags` | array | yes | Joined to `tag_list` internally |
| `organization_id` | int | yes | Gated on membership |
| `ai_disclosure_level` | enum | yes | Gated on `Settings::General.enable_ai_disclosure` |
| **`published_at`** | datetime | **no** | **Scheduling. Undocumented.** |
| **`subforem_id`** | int | **no** | Target sub-community |
| **`language`** | string | **no** | |
| **`video_source_url`** | URL | **no** | Conditionally permitted by regex |
| `clickbait_score`, `compellingness_score`, `labels` | — | no | Super-admin only |

Three writable fields are absent from the published spec. A spec-generated client misses scheduling entirely.

### Front matter is a second, equivalent input path

`ContentRenderer` parses Jekyll front matter out of `body_markdown` (`FrontMatterParser::Parser.new(:md)`). Documented keys: `title`, `published`, `description`, `tags`, `canonical_url`, `cover_image`, `series`, `ai_disclosure_level` (alias `ai_disclosure`).

Note the asymmetry: front matter says **`cover_image`**, the API field is **`main_image`**. Two spellings for one concept, and a client that mixes them silently loses the image.

## 5. Constraint catalogue — the pre-flight table

Every one of these is a 422 the model can avoid. Read from `app/models/article.rb`, `app/models/tag.rb`, `app/models/concerns/taggable.rb`.

| Constraint | Rule |
|---|---|
| `body_markdown` | ≤ **800 KB** (bytesize, not chars) |
| `title` — `full_post` | ≤ **128 chars**, measured with **all whitespace stripped** |
| `title` — `status` | ≤ 256 chars |
| `title` uniqueness | Same user + same title within **5 minutes** → rejected |
| Tag count | ≤ **4** (`MAX_TAG_LIST_SIZE`) |
| Tag name | ≤ **30 chars**, `/\A[[:alnum:]]+\z/i` — **no hyphens, no diacritics, no dots** |
| `cached_tag_list` | ≤ 126 chars total |
| `canonical_url` | http/https, no local hosts, **no whitespace**, **unique among published articles** |
| `main_image` | http/https URL |
| `slug` | `/\A[0-9a-z\-_]*\z/`, unique per user, required once published |
| `published_at` on **create** | Must be future or within the last 15 minutes |
| `published_at` on **update** | **Immutable once published** (±60 s tolerance). Freely settable while a draft or scheduled. `Articles::Updater` silently *drops* the param for an already-published article |
| `video_source_url` | Must match YouTube, `player.mux.com`, or `twitch.tv/videos/` |
| `type_of: fullscreen_embed` | Admins only |
| `type_of: status` | `body_markdown` rejected unless it is purely embeds derived from title URLs; body is immutable after creation |
| Article index `page` | `> 1000` requires an API key |
| `per_page` | Capped at `API_PER_PAGE_MAX`, default 1000 |
| Tag filter params | Must match `/\A[[:alnum:]\-]+\z/` or the endpoint 404s |

## 6. Governance is machine-readable, and it is a hard design input

dev.to publishes **`https://dev.to/llms.txt`** (captured in `evidence/live/live_llms.txt`). It is not marketing copy; it is an explicit contract addressed to automated clients, and it is the strongest signal in this whole research pass about what "expert-level" means here.

Its operative clauses:

1. *"Only publish, edit, comment, react, follow, or send other mutations when the account holder has explicitly authorized that action."*
2. *"identify your client accurately, honor rate limits, and do not evade access controls or infer private endpoints."*
3. *"Before commenting, also follow any preferences stated by the article's author; article disclosure does not grant permission to automate comments."*
4. `ai_disclosure_level` must be sent **accurately**, with definitions given:
   - `no_ai` — human, without meaningful AI assistance
   - `some_ai` — human-authored with meaningful AI assistance (drafting, code generation, major editing, translation)
   - `fully_autonomous` — produced primarily or entirely by an agent or model, **even when a human requested or approved it**
   - Omission records `not_disclosed`
5. *"A human merely reviewing or approving autonomously generated content does not make it human-authored."*

The platform backs this in code: `ai_disclosure_level` is an enum on both `Article` and `Comment`; the update path returns a `@warnings` array nudging clients off `not_disclosed`; and `{% agent_session %}` exists as a first-class liquid tag so an agent's working transcript can be attached to a post as evidence.

> **Design implication #3 — this is the differentiator.** An MCP server that writes to dev.to on a model's behalf is *exactly* the actor llms.txt addresses. Every competing implementation predates this file and ignores it. An expert server should: make `ai_disclosure_level` a **required** argument on create (no default — force the caller to state it), refuse `no_ai` on any tool call where the model authored the body, identify itself in `User-Agent`, and surface the author-consent requirement before any mutation.

## 7. What the API **cannot** do — the hard gaps

These are the walls. Each one shapes scope more than any feature does.

1. **No comment writing.** `resources :comments, only: %i[index show]`. There is no create, reply, edit, or delete in either V0 or V1. Replying to readers is a session-authenticated web action only. *An "authorship" server cannot close the loop on discussion.*
2. **Reactions are admin-only.** `ReactionPolicy#api?` → `return true if user_any_admin?` (falls through to `nil`). A normal key gets `401` on `POST /api/reactions`. Verified in the policy, not inferred.
3. **No image upload.** `/image_uploads` is `before_action :authenticate_user!` — Devise **session** auth, not api-key. Cover and inline images must already be hosted somewhere public. This is the biggest practical friction in an authoring workflow.
4. **No draft read by slug.** Unpublished articles surface only through `GET /api/articles/me/unpublished`; the public show routes are scoped `Article.published`.
5. **No notifications API.** `namespace :notifications` sits outside `/api` and is session-auth.
6. **No co-authors** through the API.
7. **No webhooks for authors.** `incoming_webhooks` is Mailchimp/Stripe inbound only. Anything reactive must poll.
8. **No delete.** Articles can be unpublished, never removed.

## 8. Rate limiting — two independent layers, and they are strict

### Layer 1 — Rack::Attack, at the edge (`config/initializers/rack_attack.rb`)

| Throttle | Limit | Keyed on |
|---|---|---|
| `api_throttle` | **3 GET/sec** | IP |
| `api_throttle_per_minute` | **30 GET/min** | IP |
| `api_key_throttle` | **3 GET/sec** | api-key |
| `api_key_throttle_per_minute` | **30 GET/min** | api-key |
| `api_write_throttle` | **1 write/sec** | IP |
| `api_write_key_throttle` | **1 write/sec** | api-key |

Admin keys are exempt from all six. `Retry-After` is returned. A 429 was triggered live during this research with fewer than ten sequential probes — **30 GET/min is the binding constraint in practice.**

### Layer 2 — application `RateLimitChecker` (per user, default values)

| Action | Default limit | Window / retry |
|---|---|---|
| `published_article_creation` | 9 | 30 s |
| `published_article_antispam_creation` | **1** | **5 min** — applies to accounts < 3 days old |
| `article_update` | 30 | 30 s |
| `image_upload` | 9 | 30 s |
| `reaction_creation` | 10 | 30 s |
| `follow_count_daily` | 500 | 1 day |
| `agent_session_creation` | 5 | 60 s |
| `comment_antispam_creation` | 1 | 5 min |

`Articles::Creator` picks the antispam limit over the normal one when `user.decorate.considered_new?`.

> **Design implication #4.** 30 GET/min across *both* IP and key means a naive "fan out and enumerate" tool design fails immediately. The server needs a real client-side token-bucket, request coalescing (prefer `/analytics/dashboard` over five analytics calls — Forem built that endpoint for precisely this reason), and caching. This is the reason the bundled endpoint exists at all, and a server that ignores it burns the user's quota.

## 9. Authoring richness: 79 Liquid tags

`app/liquid_tags/` — this is the vocabulary that makes a dev.to post a dev.to post, and it is entirely invisible to an API-schema-driven client.

- **Code & demos:** `codepen`, `codesandbox`, `replit`, `stackblitz`, `jsfiddle`, `jsitor`, `glitch`, `dotnetfiddle`, `livecodes`, `gist`, `github`, `asciinema`, `katex`, `kotlin`, `bolt`, `lovable`, `neon`, `netlify`, `cloud_run`, `huggingface`, `warp`
- **Media:** `youtube`, `vimeo`, `twitch`, `loom`, `spotify`, `soundcloud`, `bandcamp`, `mux`, `descript`, `blogcast`, `slideshare`, `speakerdeck`, `slides`
- **Social:** `tweet`, `bluesky`, `instagram`, `reddit`, `medium`, `stackexchange`, `wikipedia`, `parler`
- **Forem-native:** `user`, `organization`, `comment`, `link`, `podcast`, `tag`, `poll`, `survey`, `cta`, `details`, `row`/`col`, `feed`, `forem`, `agent_session`, `user_subscription`, `org_lead_form`, `org_lead_gate`, `org_posts`, `org_team`, `github_tag`, `unified_embed`

> **Design implication #5.** Structured knowledge of these tags — argument shapes, which URL forms each accepts, and the `unified_embed` fallback — is high-value MCP content that no OpenAPI generator can produce. It is arguably better delivered as MCP **resources** and **prompts** than as tools.

## 10. Competitive landscape

| Repo | Stars | Language | Last push | Shape |
|---|---|---|---|---|
| `Arindam200/devto-mcp` | 62 | Python | 2025-05 | 10 tools, V0 only, thin CRUD |
| `rawveg/devtomcp` | 1 | Python | 2026-03 | ~18 tools, V0 only, adds by-title lookup + drafts/scheduled listing + a REST mirror |
| `yasir-dev-lab/forem-mcp` | 2 | JavaScript | 2026-04 | Vercel-hosted, minimal, no license |

**None of them** send the V1 `Accept` header, send `ai_disclosure_level`, acknowledge `llms.txt`, pace against the documented throttles, validate against Forem's real validators before spending a write, or expose analytics, semantic search, series, subforems, surveys, or liquid tags.

Forem's own `AGENTS.md` names this audience directly: *"Outdated API specifications cause integration failures for external services, gateway clients, and LLM MCP servers."* There is no first-party Forem MCP server.

---

## 11. Design thesis for the server

The research points to one coherent position. **The value is not in wrapping endpoints — it is in being the component that knows everything an LLM gets wrong about publishing to dev.to.** Four pillars:

1. **A correct transport.** V1 `Accept` header, honest `User-Agent`, token-bucket pacing tuned to 30 GET/min and 1 write/sec, `Retry-After` obedience, coalescing to bundled endpoints, caching of tag/user/instance reads.
2. **Pre-flight validation as a first-class tool.** Mirror §5's constraint table locally and validate *before* spending a write against a 1/sec budget. `validate_article` should be callable on its own and should run implicitly inside `create`/`update`. This is where the 800 KB bytesize, the whitespace-stripped 128-char title, alnum-only tags, and the published_at immutability rule get caught for free.
3. **Governance encoded, not documented.** `ai_disclosure_level` required at the tool boundary. Publish gated behind an explicit, separately-configured capability (the platform gives no scoped key, so the server must be the boundary). No comment-automation tool at all — the API doesn't offer one, and llms.txt asks clients not to build one.
4. **The knowledge layer.** Liquid tags, front matter vs. API field mapping, series semantics, the scheduling rules, subforems, the tag taxonomy — served as MCP resources and prompts, which is the half of the protocol every competitor left empty.

Against the current MCP revision, `2026-07-28`, the server should ship **dual-era** (per `[[reference-mcp-2026-07-28]]`): Claude Code still opens with `initialize` at `2025-11-25`, and a modern-only server fails to connect at all.

## 12. Open questions before design

1. **Scope** — author's-own-workflow tool (draft → validate → schedule → measure), or a general dev.to client including research/discovery on other people's content?
2. **Publish authority** — should the server be able to publish at all, or draft-only with the human pressing publish on the site? (llms.txt pushes toward explicit per-action authorization.)
3. **Images** — accept the constraint and require pre-hosted URLs, or solve it with an uploader adapter (the user already hosts article images on `raw.githubusercontent.com`)?
4. **Implementation language** — Rust is the house default across this user's kernels; Python/TS have shorter paths to a published MCP package.
5. **Multi-instance** — dev.to only, or any Forem instance (the code is one codebase; base URL and settings differ)?

---

### Provenance note

`sources/forem` is a shallow clone and is git-ignored; re-create with
`git clone --depth 1 https://github.com/forem/forem.git sources/forem`.
Live captures in `evidence/live/` were taken 2026-09-12 unauthenticated.
No DEV API key exists on this machine — authenticated endpoints (`/api/users/me`, `/api/analytics/*`, `/api/articles/me`, `semantic_search`) were confirmed to exist and to require auth (401), but their response shapes are taken from the spec and source, not from a live authenticated call.
