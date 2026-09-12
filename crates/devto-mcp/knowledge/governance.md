# What dev.to asks of automated clients

dev.to publishes obligations for automated clients at <https://dev.to/llms.txt>. It is not
marketing copy: it is addressed specifically to software acting on an account holder's
behalf, which is exactly what this server is.

## The obligations, in dev.to's own words

> Only publish, edit, comment, react, follow, or send other mutations when the account holder
> has explicitly authorized that action. Prefer the documented public API, identify your
> client accurately, honor rate limits, and do not evade access controls or infer private
> endpoints.

> Before commenting, also follow any preferences stated by the article's author; article
> disclosure does not grant permission to automate comments.

> AI-assisted and fully autonomous articles are allowed only when they comply with the linked
> community policies. Disclosure provides context; it is not an exemption from quality,
> originality, accuracy, or accountability requirements.

## The disclosure ladder

`ai_disclosure_level` is a field on both articles and comments. Omitting it records
`not_disclosed`.

| Value | dev.to's definition |
|---|---|
| `no_ai` | Written by a human without meaningful assistance from AI generation tools. |
| `some_ai` | Human-authored with meaningful AI assistance, including drafting, code generation, major editing, or translation. |
| `fully_autonomous` | Produced primarily or entirely by an agent or language model, **even when a human requested or approved it**. |

> A human meaningfully rewriting an AI draft may use `some_ai`. A human merely reviewing or
> approving autonomously generated content does not make it human-authored. Never use `no_ai`
> or leave the field undisclosed for AI-assisted or autonomous content. The account holder or
> supervising human remains accountable for verifying facts, code, citations, originality,
> and the value of the article.

## How this server holds to them

**Disclosure is a required argument with no default.** Every write tool demands it. Silence
would record `not_disclosed` on the article itself, which is a claim about provenance made by
saying nothing.

**`no_ai` needs its own permission,** and is off by default. A tool call cannot certify that a
human wrote something without meaningful AI assistance. An account holder who did write it
themselves sets `DEVTO_ALLOW_NO_AI_CLAIM=true` and the claim goes through.

**Publishing needs its own permission** (`DEVTO_PUBLISH=true`), and is off by default. dev.to
issues one unscoped API key — it carries the account's whole identity and there is no
read-only variant — so this server is the only place a "may draft but may not publish"
boundary can exist.

**There is no tool for writing comments.** The API has no endpoint for it, and llms.txt asks
clients not to automate commenting. Both reasons stand on their own.

**The client identifies itself** in its `User-Agent`, and paces itself inside the documented
rate limits rather than discovering them by being throttled.

## Other policies

- Terms of use: <https://dev.to/terms>
- Code of conduct: <https://dev.to/code-of-conduct>
- Robots directives: <https://dev.to/robots.txt>
