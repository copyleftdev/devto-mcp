# What dev.to will reject, and what it will accept while doing something else

Every rule here is enforced in Forem's Rails models, not in the API description. The
`validate_draft` tool checks all of them offline, which is worth doing first: the write
budget is one request per second.

## Rejections

| Field | Rule | The part that surprises |
|---|---|---|
| `body_markdown` | ≤ 800 KB | Counted in **bytes**, not characters. Non-ASCII content costs more than it looks. |
| `title` | ≤ 128 characters | Measured with **every whitespace character removed** first. |
| `title` (status posts) | ≤ 256 characters | A different post type with a longer title and no body. |
| `title` uniqueness | Not the same as one of your own posts from the last **5 minutes** | An innocent retry looks like a duplicate. |
| `tags` | At most **4** | A comma inside one array entry splits it into several tags, so three entries can become five. |
| tag name | ≤ 30 characters, letters and digits only | **No hyphens, underscores, dots or spaces.** `machine-learning` is invalid; `machinelearning` is not. Diacritics are fine — `español` is a real dev.to tag. Stored lowercased. |
| tag list | The joined list ≤ 126 characters | Four tags at the maximum length land exactly on this limit. |
| `canonical_url` | http(s), no local hosts, no whitespace, unique across your published articles | The uniqueness check is against your own posts, so reusing one is a guaranteed rejection. |
| `main_image` | Absolute http(s) URL | There is no image upload on the API at all. The file has to be hosted somewhere public already. |
| `published_at` on create | Future, or within the last 15 minutes | Backdating a post being published is refused. |
| `video_source_url` | YouTube, `player.mux.com` or `twitch.tv/videos` | And **https only** — the controller accepts http and then the model rejects it. |
| `slug` | Lowercase letters, digits, `-` and `_` | Generated for you; only matters if you set it. |
| pagination | `page` above 1000 needs an API key | Even on public reads. |

## Accepted, but not what you meant

These produce no error. The article saves, and does something other than what the payload
said.

- **Front matter in the body overrides the payload.** See the front matter reference; this is
  the largest single source of silent wrong results.
- **`published_at` is discarded once an article is published.** `Articles::Updater` deletes
  the parameter before the model sees it. The publication time is frozen at first publish and
  cannot be moved afterwards.
- **A tags entry containing a comma becomes several tags.** Forem joins the array and
  re-splits it, so `["rust,zig"]` is two tags.
- **Tags are lowercased on save.** `Rust` is stored as `rust`.
- **An entry that is only whitespace vanishes.** No tag, no error.

## Things that are impossible rather than merely hard

- **Comments cannot be written.** No create, reply, edit or delete exists in any API version.
- **Reactions are admin-only.** A normal key gets 401.
- **Images cannot be uploaded.** The upload endpoint uses browser session authentication.
- **Articles cannot be deleted.** Unpublishing is the only retraction.
- **Co-authors cannot be set through the API.**
