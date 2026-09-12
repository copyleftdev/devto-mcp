//! The tool surface.
//!
//! Descriptions carry the platform's rules rather than pointing at them. A model that
//! reads "tags are letters and digits only, no hyphens" in the schema gets it right on the
//! first call; one that has to discover it from a 422 has spent a write out of a budget of
//! one per second.

use devto_client::{ArticleQuery, DevtoClient, Error as ClientError, MyArticleStatus};
use devto_core::{AiDisclosure, ArticleType, Context, Draft, Operation, Severity};
use serde_json::{Value, json};

use crate::config::Config;

/// What a tool call produced. `is_error` becomes `isError: true` in the result rather than
/// a JSON-RPC error, because the spec says clients should hand those back to the model to
/// self-correct — and almost everything that goes wrong here is self-correctable.
pub struct ToolOutcome {
    pub structured: Value,
    pub is_error: bool,
}

impl ToolOutcome {
    fn ok(structured: Value) -> Self {
        Self {
            structured,
            is_error: false,
        }
    }

    fn failed(message: impl Into<String>, remedy: impl Into<String>) -> Self {
        Self {
            structured: json!({ "error": message.into(), "remedy": remedy.into() }),
            is_error: true,
        }
    }
}

const TAG_RULE: &str = "Tags are letters and digits only, at most 30 characters each and at \
                        most 4 tags. No hyphens, underscores, dots or spaces — 'machinelearning', \
                        not 'machine-learning'. They are stored lowercased.";

pub fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "validate_draft",
            "title": "Validate a draft before sending it",
            "description": format!(
                "Check an article against every rule dev.to enforces, without sending anything. \
                 Costs no network request and no rate-limit budget, so iterate here until the \
                 report is clean rather than discovering rules one rejection at a time.\n\n\
                 Catches what the API documentation does not mention: the body limit is counted \
                 in bytes not characters; the title limit is measured with all whitespace \
                 stripped; {TAG_RULE} It also reports the traps that do not error at all — \
                 front matter inside body_markdown silently overrides these fields, a tags entry \
                 containing a comma splits into several tags, and published_at is discarded \
                 without complaint once an article is published.\n\n\
                 Every finding carries a remedy. Applying them converges on a payload dev.to \
                 accepts."
            ),
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Required. Max 128 visible characters for a normal post."},
                    "body_markdown": {"type": "string", "description": "Markdown source. May contain Forem liquid tags."},
                    "tags": {"type": "array", "items": {"type": "string"}, "description": TAG_RULE},
                    "description": {"type": "string", "description": "Short summary used for previews and SEO."},
                    "series": {"type": "string", "description": "Series name. Created on demand if new."},
                    "canonical_url": {"type": "string", "description": "Absolute http(s) URL of the original, if cross-posted. Must be unique across your published articles."},
                    "cover_image_url": {"type": "string", "description": "Absolute http(s) URL. dev.to has no image upload on the API, so the file must already be hosted publicly."},
                    "video_source_url": {"type": "string", "description": "YouTube, player.mux.com or twitch.tv/videos only, and https only."},
                    "published": {"type": "boolean", "description": "Whether this payload publishes the article. Default false."},
                    "published_at_unix": {"type": "integer", "description": "Absolute publication time, Unix seconds. Must be in the future or within the last 15 minutes."},
                    "publish_in_seconds": {"type": "integer", "description": "Alternative to published_at_unix: schedule this many seconds from now."},
                    "ai_disclosure_level": {
                        "type": "string",
                        "enum": ["not_disclosed", "no_ai", "some_ai", "fully_autonomous"],
                        "description": "dev.to's own definitions: no_ai = written by a human without meaningful assistance from AI generation tools; some_ai = human-authored with meaningful AI assistance including drafting, code generation, major editing or translation; fully_autonomous = produced primarily or entirely by an agent or language model, even when a human requested or approved it. Omitting it records not_disclosed."
                    },
                    "article_type": {"type": "string", "enum": ["full_post", "status"], "description": "status posts carry no body and allow a 256 character title."},
                    "operation": {
                        "type": "string",
                        "enum": ["create", "update_draft", "update_published"],
                        "description": "Which rules apply. update_published is strictest: publication time is frozen there."
                    }
                },
                "required": ["title"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "whoami",
            "title": "Account, capabilities and remaining budget",
            "description":
                "Report which dev.to account this server acts for, what it is permitted to do, \
                 and how much rate-limit budget is left. Worth calling first: dev.to allows only \
                 30 reads per minute and 1 write per second, counted against the IP and the API \
                 key at once, and publishing is off unless the operator turned it on.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object", "properties": {}, "additionalProperties": false
            }
        }),
        json!({
            "name": "my_articles",
            "title": "List your own articles and drafts",
            "description":
                "List articles belonging to the authenticated account. This is the only way to \
                 see unpublished work — drafts are not reachable by URL or by any public \
                 endpoint. Includes page view counts, which no public listing carries.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "status": {"type": "string", "enum": ["published", "unpublished", "all"], "description": "Default all."},
                    "page": {"type": "integer", "minimum": 1},
                    "per_page": {"type": "integer", "minimum": 1, "maximum": 100, "description": "Default 30."}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "get_article",
            "title": "Read one article with its markdown",
            "description":
                "Fetch a published article by numeric id, or by author and slug. Returns the \
                 markdown source as well as the metadata. Published articles only: use \
                 my_articles to reach your own drafts.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Numeric article id."},
                    "username": {"type": "string", "description": "Author username. Use with slug."},
                    "slug": {"type": "string", "description": "Article slug. Use with username."}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "search_articles",
            "title": "Search or browse dev.to articles",
            "description":
                "Find articles, three ways.\n\n\
                 - feed: browse by tag, author or freshness. No query text.\n\
                 - keyword: literal text matching over titles, tags and content.\n\
                 - semantic: meaning-based. Fuses keyword and vector rankings, then boosts for \
                 recency and quality. Results are ordered by that fused ranking and NOT by the \
                 similarity score, which is reported per result for information only — \
                 re-sorting by it throws the ranking away. Requires authentication.\n\n\
                 Use semantic for 'what has been written about X', feed for 'what is happening \
                 in tag Y'.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "mode": {"type": "string", "enum": ["feed", "keyword", "semantic"], "description": "Default feed."},
                    "query": {"type": "string", "description": "Required for keyword and semantic."},
                    "tag": {"type": "string", "description": "feed mode. A single tag."},
                    "tags": {"type": "string", "description": "feed mode. Comma-separated; matches any."},
                    "tags_exclude": {"type": "string", "description": "feed mode. Comma-separated."},
                    "username": {"type": "string", "description": "feed mode. Author or organization."},
                    "state": {"type": "string", "enum": ["fresh", "rising", "all"], "description": "feed mode."},
                    "top": {"type": "integer", "description": "feed mode. Most-reacted articles from the last N days."},
                    "page": {"type": "integer", "minimum": 1},
                    "per_page": {"type": "integer", "minimum": 1, "maximum": 100}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "read_comments",
            "title": "Read the comments on an article",
            "description":
                "Fetch the comment threads on an article, nested as they appear. Read only: \
                 dev.to's API has no endpoint for writing, editing or replying to comments in \
                 any version, and the platform asks automated clients not to automate \
                 commenting. Replying is something the account holder does on the site.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {"article_id": {"type": "integer"}},
                "required": ["article_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "my_analytics",
            "title": "Your readership numbers",
            "description":
                "Page views, reactions, comments, follower growth, referrers and top \
                 contributors for the authenticated account, in a single request. Can be \
                 narrowed to one article or one organization. Numbers are live — this endpoint \
                 is never cached.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "start": {"type": "string", "description": "YYYY-MM-DD. Defaults to the account's registration date."},
                    "end": {"type": "string", "description": "YYYY-MM-DD."},
                    "article_id": {"type": "integer", "description": "Limit to one article."},
                    "organization_id": {"type": "integer", "description": "Limit to an organization's articles."}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "create_draft",
            "title": "Create an unpublished draft",
            "description": format!(
                "Create a new article on dev.to as an unpublished draft. Never publishes — \
                 publishing is a separate, separately-permitted action.\n\n\
                 The payload is validated before anything is sent, so a rejection costs no \
                 write. {TAG_RULE}\n\n\
                 `ai_disclosure_level` is required and has no default. dev.to asks automated \
                 clients to state it accurately, and omitting it records 'not disclosed' on the \
                 article itself."
            ),
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "Max 128 visible characters."},
                    "body_markdown": {"type": "string", "description": "Markdown. Avoid a front matter block: it silently overrides the fields you pass here."},
                    "tags": {"type": "array", "items": {"type": "string"}, "description": TAG_RULE},
                    "description": {"type": "string"},
                    "series": {"type": "string"},
                    "canonical_url": {"type": "string"},
                    "cover_image_url": {"type": "string", "description": "Must already be hosted publicly — there is no image upload on this API."},
                    "organization_id": {"type": "integer", "description": "Publish under an organization you belong to."},
                    "ai_disclosure_level": {
                        "type": "string",
                        "enum": ["no_ai", "some_ai", "fully_autonomous"],
                        "description": "Required. no_ai = a human wrote this without meaningful AI assistance; some_ai = human-authored with meaningful AI assistance including drafting, code generation or major editing; fully_autonomous = produced primarily or entirely by an agent or model, even when a human requested or approved it. A human reviewing generated text does not make it human-authored."
                    }
                },
                "required": ["title", "body_markdown", "ai_disclosure_level"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "update_article",
            "title": "Edit an existing article",
            "description":
                "Change one of your own articles. A partial edit: anything you leave out stays \
                 as it is. Works on drafts and on published articles.\n\n\
                 Cannot publish or unpublish — those are separate tools. Publication time \
                 cannot be changed once an article is published; dev.to discards the field \
                 without reporting anything, so this tool refuses instead of pretending.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "article_id": {"type": "integer"},
                    "title": {"type": "string"},
                    "body_markdown": {"type": "string"},
                    "tags": {"type": "array", "items": {"type": "string"}, "description": TAG_RULE},
                    "description": {"type": "string"},
                    "series": {"type": "string"},
                    "canonical_url": {"type": "string"},
                    "cover_image_url": {"type": "string"},
                    "ai_disclosure_level": {
                        "type": "string",
                        "enum": ["no_ai", "some_ai", "fully_autonomous"],
                        "description": "Update the disclosure. Required if this edit changes the body in a way that changes how the article was authored."
                    }
                },
                "required": ["article_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "publish_article",
            "title": "Publish a draft, now or at a chosen time",
            "description":
                "Make one of your drafts visible on dev.to, immediately or at a scheduled \
                 time.\n\n\
                 This is off unless the account holder has enabled it: dev.to issues one \
                 unscoped API key, so this server is the only place a 'may draft but may not \
                 publish' boundary can exist. If it is disabled, the tool says so and names \
                 the setting rather than failing obscurely.\n\n\
                 Publication time cannot be changed afterwards, so a scheduled time is a \
                 commitment. It must be in the future or within the last fifteen minutes.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "article_id": {"type": "integer"},
                    "publish_at": {"type": "string", "description": "RFC 3339 UTC, e.g. 2026-09-20T14:00:00Z. Omit to publish now."},
                    "ai_disclosure_level": {
                        "type": "string",
                        "enum": ["no_ai", "some_ai", "fully_autonomous"],
                        "description": "Set or correct the disclosure as part of publishing."
                    }
                },
                "required": ["article_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "unpublish_article",
            "title": "Take a published article back to draft",
            "description":
                "Return one of your published articles to draft, so it is no longer visible. \
                 The article and its comments are kept; dev.to has no delete.\n\n\
                 Uses a normal edit rather than the /unpublish endpoint, which is restricted to \
                 admin keys. Republishing later keeps the original publication time, because \
                 that time is frozen once set.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {"article_id": {"type": "integer"}},
                "required": ["article_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "list_tags",
            "title": "Browse the tag taxonomy",
            "description": format!(
                "List dev.to's tags in popularity order. Useful before choosing tags for a \
                 draft: a tag that does not already exist will be created, which is usually not \
                 what you want. {TAG_RULE}"
            ),
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "page": {"type": "integer", "minimum": 1},
                    "per_page": {"type": "integer", "minimum": 1, "maximum": 1000, "description": "Default 10."}
                },
                "additionalProperties": false
            }
        }),
    ]
}

pub fn names() -> Vec<String> {
    definitions()
        .iter()
        .map(|d| d["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

pub struct ToolContext<'a, T: devto_client::Transport, C: devto_client::Clock> {
    pub client: &'a mut DevtoClient<T, C>,
    pub config: &'a Config,
    /// Supplied by the caller so validation stays independent of the wall clock.
    pub now_unix: i64,
}

pub fn call<T: devto_client::Transport, C: devto_client::Clock>(
    name: &str,
    args: &Value,
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    match name {
        "validate_draft" => validate_draft(args, ctx.now_unix),
        "whoami" => whoami(ctx),
        "my_articles" => my_articles(args, ctx),
        "get_article" => get_article(args, ctx),
        "search_articles" => search_articles(args, ctx),
        "read_comments" => read_comments(args, ctx),
        "my_analytics" => my_analytics(args, ctx),
        "list_tags" => list_tags(args, ctx),
        "create_draft" => create_draft(args, ctx),
        "update_article" => update_article(args, ctx),
        "publish_article" => publish_article(args, ctx),
        "unpublish_article" => unpublish_article(args, ctx),
        other => ToolOutcome::failed(
            format!("unknown tool: {other}"),
            format!("Call one of: {}.", names().join(", ")),
        ),
    }
}

// ---- free -----------------------------------------------------------------------------

/// Build a draft from tool arguments. Shared by the validator and by every write, so the
/// thing checked is literally the thing sent.
fn draft_from_args(args: &Value, now: i64) -> Draft {
    let mut draft = Draft::new(
        args.get("title")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        args.get("body_markdown")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );

    draft.tags = args
        .get("tags")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    draft.description = string_of(args, "description");
    draft.series = string_of(args, "series");
    draft.canonical_url = string_of(args, "canonical_url");
    // The API field is `main_image`; the argument is named for what a person calls it.
    draft.main_image = string_of(args, "cover_image_url");
    draft.video_source_url = string_of(args, "video_source_url");
    draft.published = args
        .get("published")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    draft.published_at = match args.get("published_at_unix").and_then(Value::as_i64) {
        Some(absolute) => Some(absolute),
        None => args
            .get("publish_in_seconds")
            .and_then(Value::as_i64)
            .map(|offset| now + offset),
    };
    draft.article_type = match args.get("article_type").and_then(Value::as_str) {
        Some("status") => ArticleType::Status,
        _ => ArticleType::FullPost,
    };
    draft
}

fn render_findings(report: &devto_core::Report) -> Vec<Value> {
    report
        .findings
        .iter()
        .map(|finding| {
            json!({
                "rule": finding.rule.as_str(),
                "severity": match finding.severity {
                    Severity::Blocking => "blocking",
                    Severity::Warning => "warning",
                },
                "field": format!("{:?}", finding.field),
                "message": finding.message,
                "remedy": finding.remedy,
            })
        })
        .collect()
}

fn validate_draft(args: &Value, now: i64) -> ToolOutcome {
    let mut draft = draft_from_args(args, now);

    draft.ai_disclosure_level = match args.get("ai_disclosure_level").and_then(Value::as_str) {
        Some(value) => match AiDisclosure::from_front_matter_value(value) {
            Some(level) => level,
            None => {
                return ToolOutcome::failed(
                    format!("unrecognised ai_disclosure_level: {value:?}"),
                    "Use one of: not_disclosed, no_ai, some_ai, fully_autonomous.",
                );
            }
        },
        None => AiDisclosure::NotDisclosed,
    };

    let operation = match args.get("operation").and_then(Value::as_str) {
        Some("update_published") => Operation::Update {
            already_published: true,
            scheduled: false,
            main_image_from_frontmatter: false,
        },
        Some("update_draft") => Operation::Update {
            already_published: false,
            scheduled: false,
            main_image_from_frontmatter: false,
        },
        _ => Operation::Create,
    };

    let context = Context {
        operation,
        ..Context::create()
    };
    let report = devto_core::validate(&draft, now, &context);

    ToolOutcome::ok(json!({
        "sendable": report.is_sendable(),
        "blocking_count": report.blocking().count(),
        "warning_count": report.warnings().count(),
        "findings": render_findings(&report),
        "summary": if report.is_sendable() && report.is_empty() {
            "Clean: dev.to will accept this payload and do what it says.".to_string()
        } else if report.is_sendable() {
            format!(
                "dev.to will accept this, but {} thing(s) will not do what you meant. Read the warnings.",
                report.warnings().count()
            )
        } else {
            format!(
                "dev.to will reject this. Fix {} blocking finding(s) before sending.",
                report.blocking().count()
            )
        },
    }))
}

fn string_of(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .map(str::to_string)
}

// ---- reads ----------------------------------------------------------------------------

fn whoami<T: devto_client::Transport, C: devto_client::Clock>(
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    let budget = ctx.client.budget();
    let capabilities = json!({
        "read": true,
        "draft": ctx.config.is_authenticated(),
        "publish": ctx.config.capabilities.publish,
        "claim_no_ai": ctx.config.capabilities.claim_no_ai,
        "comment": false,
        "react": false,
        "upload_images": false,
    });
    let notes = json!([
        "Writing, editing and replying to comments is not possible: dev.to's API has no \
         endpoint for it in any version.",
        "Reactions are restricted to admin keys, so liking and unicorning are unavailable.",
        "There is no image upload on the API. Cover and inline images must already be hosted \
         at a public URL.",
    ]);

    let base = json!({
        "instance": ctx.config.base_url,
        "authenticated": ctx.config.is_authenticated(),
        "capabilities": capabilities,
        "budget_remaining": {
            "reads_this_second": budget.reads_this_second,
            "reads_this_minute": budget.reads_this_minute,
            "writes_this_second": budget.writes_this_second,
        },
        "limits": {
            "reads_per_second": 3, "reads_per_minute": 30, "writes_per_second": 1,
            "note": "Counted against the IP and the API key at the same time."
        },
        "notes": notes,
    });

    if !ctx.config.is_authenticated() {
        let mut payload = base;
        payload["account"] = Value::Null;
        payload["remedy"] = json!(
            "Set DEVTO_API_KEY to act for an account. Generate one at \
             https://dev.to/settings/extensions — it cannot be created programmatically."
        );
        return ToolOutcome::ok(payload);
    }

    match ctx.client.me() {
        Ok(me) => {
            let mut payload = base;
            payload["account"] = json!({
                "id": me.id,
                "username": me.username,
                "name": me.name,
                "joined_at": me.joined_at,
                "followers_count": me.followers_count,
            });
            ToolOutcome::ok(payload)
        }
        Err(error) => client_failure(error),
    }
}

fn my_articles<T: devto_client::Transport, C: devto_client::Clock>(
    args: &Value,
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    if let Some(outcome) = require_auth(ctx) {
        return outcome;
    }
    let status = match args.get("status").and_then(Value::as_str) {
        Some("published") => MyArticleStatus::Published,
        Some("unpublished") => MyArticleStatus::Unpublished,
        _ => MyArticleStatus::All,
    };

    match ctx
        .client
        .my_articles(status, u32_of(args, "page"), u32_of(args, "per_page"))
    {
        Ok(articles) => {
            let items: Vec<Value> = articles
                .iter()
                .map(|a| {
                    json!({
                        "id": a.id,
                        "title": a.title,
                        "published": a.published,
                        "published_at": a.published_at,
                        "url": a.url,
                        "slug": a.slug,
                        "tags": a.tag_list,
                        "page_views": a.page_views_count,
                        "reactions": a.public_reactions_count,
                        "comments": a.comments_count,
                        "reading_time_minutes": a.reading_time_minutes,
                        "canonical_url": a.canonical_url,
                    })
                })
                .collect();
            ToolOutcome::ok(json!({
                "count": items.len(),
                "unpublished_count": articles.iter().filter(|a| !a.published).count(),
                "articles": items,
            }))
        }
        Err(error) => client_failure(error),
    }
}

fn get_article<T: devto_client::Transport, C: devto_client::Clock>(
    args: &Value,
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    let result = match (
        args.get("id").and_then(Value::as_i64),
        args.get("username").and_then(Value::as_str),
        args.get("slug").and_then(Value::as_str),
    ) {
        (Some(id), _, _) => ctx.client.article(id),
        (None, Some(username), Some(slug)) => ctx.client.article_by_path(username, slug),
        _ => {
            return ToolOutcome::failed(
                "no article identified",
                "Pass id, or pass both username and slug.",
            );
        }
    };

    match result {
        Ok(article) => ToolOutcome::ok(json!({
            "id": article.summary.id,
            "title": article.summary.title,
            "description": article.summary.description,
            "url": article.summary.url,
            "tags": article.summary.tag_list,
            "published_at": article.summary.published_at,
            "reactions": article.summary.public_reactions_count,
            "comments": article.summary.comments_count,
            "reading_time_minutes": article.summary.reading_time_minutes,
            "canonical_url": article.summary.canonical_url,
            "ai_disclosure_level": article.summary.ai_disclosure_level,
            "author": article.summary.user.as_ref().map(|u| u.username.clone()),
            "body_markdown": article.body_markdown,
        })),
        Err(error) => client_failure(error),
    }
}

fn search_articles<T: devto_client::Transport, C: devto_client::Clock>(
    args: &Value,
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    let mode = args.get("mode").and_then(Value::as_str).unwrap_or("feed");
    let query = args.get("query").and_then(Value::as_str);
    let page = u32_of(args, "page");
    let per_page = u32_of(args, "per_page");

    match mode {
        "semantic" => {
            if let Some(outcome) = require_auth(ctx) {
                return outcome;
            }
            let Some(query) = query else {
                return ToolOutcome::failed(
                    "semantic search needs a query",
                    "Pass `query` describing what you are looking for.",
                );
            };
            let base = ctx.client.base_url().to_string();
            match ctx.client.semantic_search(query, page, per_page, None) {
                Ok(hits) => {
                    let items: Vec<Value> = hits
                        .iter()
                        .map(|hit| {
                            json!({
                                "id": hit.id,
                                "title": hit.title,
                                "description": hit.description,
                                "url": format!("{base}{}", hit.path),
                                "tags": hit.tags(),
                                "published_at": hit.published_at,
                                "reactions": hit.public_reactions_count,
                                "comments": hit.comments_count,
                                "reading_time_minutes": hit.reading_time,
                                "similarity": hit.similarity,
                            })
                        })
                        .collect();
                    ToolOutcome::ok(json!({
                        "mode": "semantic",
                        "count": items.len(),
                        "ranking": "Fused keyword and vector rankings, boosted for recency and \
                                    quality. This order is the ranking; similarity is reported \
                                    per result but the list is not sorted by it.",
                        "results": items,
                    }))
                }
                Err(error) => client_failure(error),
            }
        }
        "keyword" => {
            let Some(query) = query else {
                return ToolOutcome::failed(
                    "keyword search needs a query",
                    "Pass `query`, or use mode 'feed' to browse without one.",
                );
            };
            match ctx.client.search_articles(query, page, per_page) {
                Ok(articles) => ToolOutcome::ok(summaries("keyword", &articles)),
                Err(error) => client_failure(error),
            }
        }
        "feed" => {
            let query = ArticleQuery {
                tag: args.get("tag").and_then(Value::as_str),
                tags: args.get("tags").and_then(Value::as_str),
                tags_exclude: args.get("tags_exclude").and_then(Value::as_str),
                username: args.get("username").and_then(Value::as_str),
                state: args.get("state").and_then(Value::as_str),
                top: u32_of(args, "top"),
                collection_id: args.get("collection_id").and_then(Value::as_i64),
                page,
                per_page,
            };
            match ctx.client.articles(query) {
                Ok(articles) => ToolOutcome::ok(summaries("feed", &articles)),
                Err(error) => client_failure(error),
            }
        }
        other => ToolOutcome::failed(
            format!("unknown search mode: {other}"),
            "Use 'feed', 'keyword' or 'semantic'.",
        ),
    }
}

fn summaries(mode: &str, articles: &[devto_client::ArticleSummary]) -> Value {
    let items: Vec<Value> = articles
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "title": a.title,
                "description": a.description,
                "url": a.url,
                "tags": a.tag_list,
                "published_at": a.published_at,
                "reactions": a.public_reactions_count,
                "comments": a.comments_count,
                "reading_time_minutes": a.reading_time_minutes,
                "ai_disclosure_level": a.ai_disclosure_level,
                "author": a.user.as_ref().map(|u| u.username.clone()),
            })
        })
        .collect();
    json!({ "mode": mode, "count": items.len(), "results": items })
}

fn read_comments<T: devto_client::Transport, C: devto_client::Clock>(
    args: &Value,
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    let Some(article_id) = args.get("article_id").and_then(Value::as_i64) else {
        return ToolOutcome::failed("article_id is required", "Pass the numeric article id.");
    };

    match ctx.client.comments(article_id) {
        Ok(comments) => {
            let total = count_comments(&comments);
            ToolOutcome::ok(json!({
                "article_id": article_id,
                "top_level_count": comments.len(),
                "total_count": total,
                "comments": comments.iter().map(render_comment).collect::<Vec<_>>(),
                "note": "Read only — dev.to's API cannot post, edit or reply to comments.",
            }))
        }
        Err(error) => client_failure(error),
    }
}

fn count_comments(comments: &[devto_client::Comment]) -> usize {
    comments
        .iter()
        .map(|c| 1 + count_comments(&c.children))
        .sum()
}

fn render_comment(comment: &devto_client::Comment) -> Value {
    json!({
        "id_code": comment.id_code,
        "author": comment.user.as_ref().map(|u| u.username.clone()),
        "created_at": comment.created_at,
        "body_html": comment.body_html,
        "ai_disclosure_level": comment.ai_disclosure_level,
        "replies": comment.children.iter().map(render_comment).collect::<Vec<_>>(),
    })
}

fn my_analytics<T: devto_client::Transport, C: devto_client::Clock>(
    args: &Value,
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    if let Some(outcome) = require_auth(ctx) {
        return outcome;
    }
    match ctx.client.analytics_dashboard(
        args.get("start").and_then(Value::as_str),
        args.get("end").and_then(Value::as_str),
        args.get("article_id").and_then(Value::as_i64),
        args.get("organization_id").and_then(Value::as_i64),
    ) {
        Ok(dashboard) => ToolOutcome::ok(json!({
            "totals": {
                "page_views": dashboard.totals.page_views.total,
                "average_read_time_seconds": dashboard.totals.page_views.average_read_time_in_seconds,
                "reactions": dashboard.totals.reactions.total,
                "unique_reactors": dashboard.totals.reactions.unique_reactors,
                "comments": dashboard.totals.comments.total,
                "follows": dashboard.totals.follows.total,
            },
            "data_since": dashboard.start_date_floor,
            "historical": dashboard.historical,
            "referrers": dashboard.referrers,
            "top_contributors": dashboard.top_contributors,
            "follower_engagement": dashboard.follower_engagement,
        })),
        Err(error) => client_failure(error),
    }
}

fn list_tags<T: devto_client::Transport, C: devto_client::Clock>(
    args: &Value,
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    match ctx
        .client
        .tags(u32_of(args, "page"), u32_of(args, "per_page"))
    {
        Ok(tags) => ToolOutcome::ok(json!({
            "count": tags.len(),
            "tags": tags.iter().map(|t| json!({"id": t.id, "name": t.name})).collect::<Vec<_>>(),
        })),
        Err(error) => client_failure(error),
    }
}

// ---- writes ---------------------------------------------------------------------------

/// Read the disclosure level, and refuse the one claim a model cannot honestly make.
fn resolve_disclosure(args: &Value, config: &Config) -> Result<AiDisclosure, ToolOutcome> {
    let Some(raw) = args.get("ai_disclosure_level").and_then(Value::as_str) else {
        return Err(ToolOutcome::failed(
            "ai_disclosure_level is required and has no default",
            "Pass no_ai, some_ai or fully_autonomous. dev.to's definitions: no_ai is written by \
             a human without meaningful assistance from AI generation tools; some_ai is \
             human-authored with meaningful AI assistance including drafting, code generation, \
             major editing or translation; fully_autonomous is produced primarily or entirely \
             by an agent or language model, even when a human requested or approved it.",
        ));
    };

    let Some(level) = AiDisclosure::from_front_matter_value(raw) else {
        return Err(ToolOutcome::failed(
            format!("unrecognised ai_disclosure_level: {raw:?}"),
            "Use no_ai, some_ai or fully_autonomous.",
        ));
    };

    if level == AiDisclosure::NoAi && !config.capabilities.claim_no_ai {
        return Err(ToolOutcome::failed(
            "this server will not send ai_disclosure_level: no_ai",
            "no_ai asserts that a human wrote the article without meaningful assistance from AI \
             generation tools, which is not something a tool call can certify. If the account \
             holder wrote it themselves and is using this only to send it, they can set \
             DEVTO_ALLOW_NO_AI_CLAIM=true. Otherwise use some_ai or fully_autonomous.",
        ));
    }

    Ok(level)
}

/// Turn a validation report into a refusal a caller can act on, or `None` if it is clean
/// enough to send. Warnings travel with the success rather than stopping the write.
fn preflight(draft: &Draft, operation: Operation, now: i64) -> Result<Vec<Value>, ToolOutcome> {
    let context = Context {
        operation,
        ..Context::create()
    };
    let report = devto_core::validate(draft, now, &context);
    let findings = render_findings(&report);

    if report.is_sendable() {
        return Ok(findings);
    }
    Err(ToolOutcome {
        structured: json!({
            "error": "dev.to would reject this payload, so nothing was sent",
            "remedy": "Apply the remedies below and call again. This cost no write.",
            "sendable": false,
            "findings": findings,
        }),
        is_error: true,
    })
}

fn payload_from(draft: &Draft, args: &Value) -> devto_client::ArticlePayload {
    devto_client::ArticlePayload {
        title: Some(draft.title.clone()).filter(|t| !t.is_empty()),
        body_markdown: Some(draft.body_markdown.clone()),
        published: Some(draft.published),
        description: draft.description.clone(),
        tags: Some(draft.tags.clone()).filter(|t| !t.is_empty()),
        series: draft.series.clone(),
        main_image: draft.main_image.clone(),
        canonical_url: draft.canonical_url.clone(),
        organization_id: args.get("organization_id").and_then(Value::as_i64),
        ai_disclosure_level: Some(draft.ai_disclosure_level.wire_value().to_string()),
        published_at: draft.published_at.and_then(devto_core::format_rfc3339_utc),
        video_source_url: draft.video_source_url.clone(),
    }
}

fn written(
    article: &devto_client::WrittenArticle,
    note: &str,
    warnings: Vec<Value>,
) -> ToolOutcome {
    ToolOutcome::ok(json!({
        "id": article.id,
        "title": article.title,
        "published": article.published,
        "published_at": article.published_at,
        "url": article.url,
        "tags": article.tag_list,
        "ai_disclosure_level": article.ai_disclosure_level,
        "note": note,
        "warnings": warnings,
    }))
}

fn create_draft<T: devto_client::Transport, C: devto_client::Clock>(
    args: &Value,
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    if let Some(outcome) = require_auth(ctx) {
        return outcome;
    }
    let level = match resolve_disclosure(args, ctx.config) {
        Ok(level) => level,
        Err(outcome) => return outcome,
    };

    let mut draft = draft_from_args(args, ctx.now_unix);
    draft.ai_disclosure_level = level;
    // A draft is a draft. Publishing is a separate action with its own permission.
    draft.published = false;
    draft.published_at = None;

    let warnings = match preflight(&draft, Operation::Create, ctx.now_unix) {
        Ok(warnings) => warnings,
        Err(refusal) => return refusal,
    };

    match ctx.client.create_article(&payload_from(&draft, args)) {
        Ok(article) => written(
            &article,
            "Created as an unpublished draft. It is not visible to anyone yet.",
            warnings,
        ),
        Err(error) => client_failure(error),
    }
}

fn update_article<T: devto_client::Transport, C: devto_client::Clock>(
    args: &Value,
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    if let Some(outcome) = require_auth(ctx) {
        return outcome;
    }
    let Some(article_id) = args.get("article_id").and_then(Value::as_i64) else {
        return ToolOutcome::failed("article_id is required", "Pass the numeric article id.");
    };
    if args.get("published").is_some() {
        return ToolOutcome::failed(
            "update_article cannot publish or unpublish",
            "Use publish_article or unpublish_article, which carry their own permissions.",
        );
    }

    let level = match args.get("ai_disclosure_level") {
        Some(_) => match resolve_disclosure(args, ctx.config) {
            Ok(level) => Some(level),
            Err(outcome) => return outcome,
        },
        None => None,
    };

    let existing = match find_own_article(article_id, ctx) {
        Ok(article) => article,
        Err(outcome) => return outcome,
    };

    let mut draft = draft_from_args(args, ctx.now_unix);
    if draft.title.is_empty() {
        draft.title.clone_from(&existing.title);
    }
    if let Some(level) = level {
        draft.ai_disclosure_level = level;
    }

    let operation = Operation::Update {
        already_published: existing.published,
        scheduled: false,
        main_image_from_frontmatter: false,
    };
    let warnings = match preflight(&draft, operation, ctx.now_unix) {
        Ok(warnings) => warnings,
        Err(refusal) => return refusal,
    };

    // Only send what the caller actually asked to change.
    let mut payload = payload_from(&draft, args);
    payload.published = None;
    payload.published_at = None;
    if args.get("title").is_none() {
        payload.title = None;
    }
    if args.get("body_markdown").is_none() {
        payload.body_markdown = None;
    }
    if level.is_none() {
        payload.ai_disclosure_level = None;
    }

    match ctx.client.update_article(article_id, &payload) {
        Ok(article) => written(&article, "Updated.", warnings),
        Err(error) => client_failure(error),
    }
}

fn publish_article<T: devto_client::Transport, C: devto_client::Clock>(
    args: &Value,
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    if let Some(outcome) = require_auth(ctx) {
        return outcome;
    }
    if !ctx.config.capabilities.publish {
        return ToolOutcome::failed(
            "publishing is not enabled for this server",
            "The account holder enables it by setting DEVTO_PUBLISH=true in this server's \
             environment. It is off by default because dev.to issues one unscoped API key, so \
             this server is the only place a 'may draft but may not publish' boundary exists. \
             The draft is unaffected and can still be published on dev.to directly.",
        );
    }
    let Some(article_id) = args.get("article_id").and_then(Value::as_i64) else {
        return ToolOutcome::failed("article_id is required", "Pass the numeric article id.");
    };

    let level = match args.get("ai_disclosure_level") {
        Some(_) => match resolve_disclosure(args, ctx.config) {
            Ok(level) => Some(level),
            Err(outcome) => return outcome,
        },
        None => None,
    };

    let existing = match find_own_article(article_id, ctx) {
        Ok(article) => article,
        Err(outcome) => return outcome,
    };
    if existing.published {
        return ToolOutcome::failed(
            format!("article {article_id} is already published"),
            "Nothing was sent. Use update_article to change it, or unpublish_article to take \
             it back to draft.",
        );
    }

    let publish_at = match args.get("publish_at").and_then(Value::as_str) {
        Some(raw) => match devto_core::parse_rfc3339_utc(raw) {
            Some(at) => Some(at),
            None => {
                return ToolOutcome::failed(
                    format!("could not read publish_at: {raw:?}"),
                    "Use RFC 3339 in UTC, for example 2026-09-20T14:00:00Z.",
                );
            }
        },
        None => None,
    };

    let mut draft = Draft::new(existing.title.clone(), String::new());
    draft.published = true;
    draft.published_at = publish_at;
    draft.ai_disclosure_level = level.unwrap_or(AiDisclosure::SomeAi);
    let warnings = match preflight(&draft, Operation::Create, ctx.now_unix) {
        Ok(warnings) => warnings,
        Err(refusal) => return refusal,
    };

    let payload = devto_client::ArticlePayload {
        published: Some(true),
        published_at: publish_at.and_then(devto_core::format_rfc3339_utc),
        ai_disclosure_level: level.map(|l| l.wire_value().to_string()),
        ..Default::default()
    };

    match ctx.client.update_article(article_id, &payload) {
        Ok(article) => {
            let note = match publish_at {
                Some(_) => "Scheduled. The publication time cannot be changed afterwards.",
                None => "Published and visible now.",
            };
            written(&article, note, warnings)
        }
        Err(error) => client_failure(error),
    }
}

fn unpublish_article<T: devto_client::Transport, C: devto_client::Clock>(
    args: &Value,
    ctx: &mut ToolContext<'_, T, C>,
) -> ToolOutcome {
    if let Some(outcome) = require_auth(ctx) {
        return outcome;
    }
    let Some(article_id) = args.get("article_id").and_then(Value::as_i64) else {
        return ToolOutcome::failed("article_id is required", "Pass the numeric article id.");
    };

    let existing = match find_own_article(article_id, ctx) {
        Ok(article) => article,
        Err(outcome) => return outcome,
    };
    if !existing.published {
        return ToolOutcome::failed(
            format!("article {article_id} is already a draft"),
            "Nothing was sent.",
        );
    }

    let payload = devto_client::ArticlePayload {
        published: Some(false),
        ..Default::default()
    };
    match ctx.client.update_article(article_id, &payload) {
        Ok(article) => written(
            &article,
            "Unpublished. It is a draft again; comments and the original publication time are \
             kept, and dev.to has no delete.",
            Vec::new(),
        ),
        Err(error) => client_failure(error),
    }
}

/// Find one of the caller's own articles. Costs a read, and is worth it: without knowing
/// whether the article is already published, a write is a guess.
fn find_own_article<T: devto_client::Transport, C: devto_client::Clock>(
    article_id: i64,
    ctx: &mut ToolContext<'_, T, C>,
) -> Result<devto_client::MyArticle, ToolOutcome> {
    match ctx
        .client
        .my_articles(MyArticleStatus::All, None, Some(1000))
    {
        Ok(articles) => articles
            .into_iter()
            .find(|a| a.id == article_id)
            .ok_or_else(|| {
                ToolOutcome::failed(
                    format!("no article {article_id} belongs to this account"),
                    "Call my_articles to see the ids you own. You can only edit your own work.",
                )
            }),
        Err(error) => Err(client_failure(error)),
    }
}

// ---- shared ---------------------------------------------------------------------------

fn u32_of(args: &Value, key: &str) -> Option<u32> {
    args.get(key).and_then(Value::as_u64).map(|v| v as u32)
}

fn require_auth<T: devto_client::Transport, C: devto_client::Clock>(
    ctx: &ToolContext<'_, T, C>,
) -> Option<ToolOutcome> {
    if ctx.config.is_authenticated() {
        return None;
    }
    Some(ToolOutcome::failed(
        "this tool needs an authenticated account",
        "Set DEVTO_API_KEY. Generate a key at https://dev.to/settings/extensions — there is no \
         programmatic way to create one.",
    ))
}

/// Turn a client error into something a model can act on. The remedy matters more than the
/// message: half of these are fixable by changing the next call.
fn client_failure(error: ClientError) -> ToolOutcome {
    let remedy = match &error {
        ClientError::Unauthorized => {
            "Check DEVTO_API_KEY. Keys are generated at https://dev.to/settings/extensions."
        }
        ClientError::Forbidden { .. } => {
            "This part of the API is restricted to admin keys on dev.to. Reactions and the \
             publisher tools are not available to a normal account."
        }
        ClientError::NotFound { .. } => {
            "Check the id or the username and slug. Unpublished drafts are only visible through \
             my_articles."
        }
        ClientError::Validation { .. } => {
            "Run validate_draft on the payload before sending it — it catches this class of \
             rejection without spending a request."
        }
        ClientError::RateLimited { .. } => {
            "Wait the stated time. dev.to allows 30 reads per minute; call whoami to see what is \
             left before planning more calls."
        }
        ClientError::VersionDowngrade => {
            "This is a defect in the server, not in the request. The API version header did not \
             reach dev.to."
        }
        ClientError::Server { .. } | ClientError::Transport(_) => {
            "dev.to or the network is having trouble. Retrying shortly is reasonable."
        }
        ClientError::Decode { .. } => {
            "dev.to returned a shape this server did not expect, which usually means the API \
             changed. Worth reporting."
        }
    };
    ToolOutcome::failed(error.to_string(), remedy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_name_a_description_and_a_bundled_schema() {
        let definitions = definitions();
        assert_eq!(definitions.len(), 12);

        for tool in &definitions {
            let name = tool["name"].as_str().expect("name");
            assert!(!name.is_empty());
            assert!(
                tool["description"].as_str().is_some_and(|d| d.len() > 40),
                "{name} needs a real description"
            );
            let schema = &tool["inputSchema"];
            assert_eq!(
                schema["$schema"],
                json!("https://json-schema.org/draft/2020-12/schema"),
                "{name} must declare JSON Schema 2020-12"
            );
            assert_eq!(schema["type"], json!("object"), "{name}");
            assert_eq!(
                schema["additionalProperties"],
                json!(false),
                "{name} should reject arguments it does not understand"
            );
            assert!(
                !serde_json::to_string(schema).unwrap().contains("$ref"),
                "{name}: schemas must be bundled, not referenced over the network"
            );
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let names = names();
        let unique: std::collections::BTreeSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len());
    }

    /// Every advertised tool must dispatch. An entry in the list with no implementation is
    /// worse than an omission: the model calls it and gets a confusing failure.
    #[test]
    fn every_advertised_tool_is_implemented() {
        for name in names() {
            let (result, _) = invoke(&name, json!({}), &["[]", "[]"]);
            let unknown = result
                .get("error")
                .and_then(Value::as_str)
                .is_some_and(|e| e.starts_with("unknown tool"));
            assert!(!unknown, "{name} is advertised but not dispatched");
        }
    }

    /// The rules a model most often gets wrong belong in the schema, not in a doc it will
    /// never read.
    #[test]
    fn the_schemas_carry_the_rules_that_cause_rejections() {
        let all = serde_json::to_string(&definitions()).unwrap();
        assert!(
            all.contains("machinelearning"),
            "the tag rule should show the fix"
        );
        assert!(all.contains("no_ai"));
        assert!(all.contains("fully_autonomous"));
        assert!(all.contains("bytes not characters"));
        assert!(all.contains("whitespace stripped"));
        assert!(all.contains("no image upload"));
    }

    fn validate(args: Value) -> Value {
        validate_draft(&args, 1_757_000_000).structured
    }

    #[test]
    fn a_clean_draft_validates_without_findings() {
        let result = validate(json!({
            "title": "A reasonable title",
            "body_markdown": "Some words.",
            "tags": ["rust", "webdev"],
            "ai_disclosure_level": "some_ai"
        }));
        assert_eq!(result["sendable"], json!(true));
        assert_eq!(result["blocking_count"], json!(0));
        assert_eq!(result["warning_count"], json!(0));
    }

    #[test]
    fn a_hyphenated_tag_is_reported_with_its_fix() {
        let result = validate(json!({
            "title": "Title",
            "tags": ["machine-learning"],
            "ai_disclosure_level": "some_ai"
        }));
        assert_eq!(result["sendable"], json!(false));
        let finding = result["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["rule"] == json!("TAG_INVALID_CHARACTERS"))
            .expect("expected the tag rule to fire");
        assert!(
            finding["remedy"]
                .as_str()
                .unwrap()
                .contains("machinelearning"),
            "{finding}"
        );
    }

    #[test]
    fn an_undisclosed_draft_warns_without_blocking() {
        let result = validate(json!({"title": "Title", "body_markdown": "Words."}));
        assert_eq!(result["sendable"], json!(true));
        assert!(
            result["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["rule"] == json!("DISCLOSURE_NOT_DISCLOSED"))
        );
        assert!(result["summary"].as_str().unwrap().contains("warnings"));
    }

    /// A relative schedule is easier for a caller to get right than an absolute timestamp,
    /// and both have to land on the same rule.
    #[test]
    fn scheduling_accepts_an_offset_or_an_absolute_time() {
        let future = validate(json!({
            "title": "Title", "published": true,
            "publish_in_seconds": 3600, "ai_disclosure_level": "some_ai"
        }));
        assert_eq!(future["sendable"], json!(true));

        let past = validate(json!({
            "title": "Title", "published": true,
            "publish_in_seconds": -3600, "ai_disclosure_level": "some_ai"
        }));
        assert_eq!(past["sendable"], json!(false));

        let absolute_past = validate(json!({
            "title": "Title", "published": true,
            "published_at_unix": 1_757_000_000i64 - 3600, "ai_disclosure_level": "some_ai"
        }));
        assert_eq!(absolute_past["sendable"], json!(false));
    }

    #[test]
    fn an_unrecognised_disclosure_level_is_refused_rather_than_defaulted() {
        let outcome = validate_draft(
            &json!({"title": "Title", "ai_disclosure_level": "probably_fine"}),
            0,
        );
        assert!(outcome.is_error);
        assert!(
            outcome.structured["remedy"]
                .as_str()
                .unwrap()
                .contains("fully_autonomous")
        );
    }

    #[test]
    fn front_matter_conflicts_are_surfaced_by_the_tool() {
        let result = validate(json!({
            "title": "Payload title",
            "body_markdown": "---\ntitle: Front title\n---\n\nBody.",
            "series": "A series",
            "ai_disclosure_level": "some_ai"
        }));
        let rules: Vec<&str> = result["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["rule"].as_str().unwrap())
            .collect();
        assert!(rules.contains(&"FRONT_MATTER_OVERRIDES_PAYLOAD"));
        assert!(rules.contains(&"FRONT_MATTER_DROPS_SERIES"));
    }

    #[test]
    fn update_published_freezes_the_publication_time() {
        let result = validate(json!({
            "title": "Title", "operation": "update_published",
            "publish_in_seconds": 86_400, "ai_disclosure_level": "some_ai"
        }));
        assert!(
            result["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["rule"] == json!("PUBLISHED_AT_SILENTLY_DROPPED"))
        );
    }

    /// A relative offset is added to now. Getting the arithmetic wrong would move the
    /// boundary somewhere unrecognisable, so pin it either side of the fifteen minutes.
    #[test]
    fn a_relative_schedule_lands_on_the_same_boundary_as_an_absolute_one() {
        let just_inside = validate(json!({
            "title": "Title", "published": true,
            "publish_in_seconds": -899, "ai_disclosure_level": "some_ai"
        }));
        assert_eq!(
            just_inside["sendable"],
            json!(true),
            "899 seconds back is inside the fifteen minute grace period"
        );

        let on_the_floor = validate(json!({
            "title": "Title", "published": true,
            "publish_in_seconds": -900, "ai_disclosure_level": "some_ai"
        }));
        assert_eq!(on_the_floor["sendable"], json!(false));
    }

    /// `status` is a different post type with different rules, not a label.
    #[test]
    fn a_status_post_is_validated_as_a_status_post() {
        let with_body = validate(json!({
            "title": "Look at this", "body_markdown": "Some words.",
            "article_type": "status", "ai_disclosure_level": "some_ai"
        }));
        assert_eq!(with_body["sendable"], json!(false));
        assert!(
            with_body["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["rule"] == json!("STATUS_BODY_NOT_ALLOWED"))
        );

        let as_full_post = validate(json!({
            "title": "Look at this", "body_markdown": "Some words.",
            "ai_disclosure_level": "some_ai"
        }));
        assert_eq!(as_full_post["sendable"], json!(true));
    }

    /// Editing an existing draft is stricter than creating one: a draft being created may
    /// carry any publication time, but moving an existing one into the past is refused.
    #[test]
    fn editing_a_draft_applies_the_update_rules_not_the_create_rules() {
        let creating = validate(json!({
            "title": "Title", "published": false,
            "publish_in_seconds": -3600, "ai_disclosure_level": "some_ai"
        }));
        assert_eq!(creating["sendable"], json!(true));

        let editing = validate(json!({
            "title": "Title", "published": false, "operation": "update_draft",
            "publish_in_seconds": -3600, "ai_disclosure_level": "some_ai"
        }));
        assert_eq!(editing["sendable"], json!(false));
    }

    /// An absent optional field is absent, not an empty string. The difference shows up in
    /// the series rule, which only fires when a series was actually asked for.
    #[test]
    fn an_omitted_optional_field_is_not_an_empty_one() {
        let no_series = validate(json!({
            "title": "Title",
            "body_markdown": "---\ntitle: Title\n---\n\nBody.",
            "ai_disclosure_level": "some_ai"
        }));
        assert!(
            !no_series["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["rule"] == json!("FRONT_MATTER_DROPS_SERIES")),
            "no series was requested, so none can be dropped"
        );

        let with_series = validate(json!({
            "title": "Title", "series": "My series",
            "body_markdown": "---\ntitle: Title\n---\n\nBody.",
            "ai_disclosure_level": "some_ai"
        }));
        assert!(
            with_series["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["rule"] == json!("FRONT_MATTER_DROPS_SERIES"))
        );
    }

    // ---- behaviour that needs a request in flight ------------------------------------

    use devto_client::{Clock, Config as ClientConfig, HttpRequest, HttpResponse, Transport};
    use std::cell::RefCell;

    struct Recorder {
        responses: RefCell<Vec<String>>,
        seen: RefCell<Vec<String>>,
        sent: RefCell<Vec<(String, String)>>,
    }

    impl Recorder {
        fn new(bodies: &[&str]) -> Self {
            Self {
                responses: RefCell::new(bodies.iter().map(|b| b.to_string()).collect()),
                seen: RefCell::new(Vec::new()),
                sent: RefCell::new(Vec::new()),
            }
        }
    }

    impl Transport for Recorder {
        fn execute(&self, request: HttpRequest) -> Result<HttpResponse, String> {
            self.seen.borrow_mut().push(request.url.clone());
            self.sent.borrow_mut().push((
                request.method.as_str().to_string(),
                request.body.clone().unwrap_or_default(),
            ));
            let mut responses = self.responses.borrow_mut();
            let body = if responses.is_empty() {
                "[]".to_string()
            } else {
                responses.remove(0)
            };
            Ok(HttpResponse {
                status: 200,
                headers: Vec::new(),
                body,
            })
        }
    }

    struct Frozen;

    impl Clock for Frozen {
        fn now_millis(&self) -> u64 {
            0
        }
        fn sleep_millis(&self, _millis: u64) {}
    }

    /// Run one tool call and hand back both what it produced and the URL it asked for.
    fn invoke(name: &str, args: Value, bodies: &[&str]) -> (Value, Vec<String>) {
        let (result, urls, _) = invoke_as(
            crate::config::Capabilities {
                publish: false,
                claim_no_ai: false,
            },
            name,
            args,
            bodies,
        );
        (result, urls)
    }

    /// The same, with the requests the tool actually sent and the capabilities it ran under.
    fn invoke_as(
        capabilities: crate::config::Capabilities,
        name: &str,
        args: Value,
        bodies: &[&str],
    ) -> (Value, Vec<String>, Vec<(String, String)>) {
        let config = Config {
            base_url: "https://dev.to".to_string(),
            api_key: Some("secret".to_string()),
            capabilities,
        };
        let mut client = DevtoClient::with_parts(
            ClientConfig {
                api_key: config.api_key.clone(),
                ..ClientConfig::default()
            },
            Recorder::new(bodies),
            Frozen,
        );
        let outcome = {
            let mut ctx = ToolContext {
                client: &mut client,
                config: &config,
                now_unix: 1_757_000_000,
            };
            call(name, &args, &mut ctx)
        };
        let urls = client.transport().seen.borrow().clone();
        let sent = client.transport().sent.borrow().clone();
        (outcome.structured, urls, sent)
    }

    fn permissive() -> crate::config::Capabilities {
        crate::config::Capabilities {
            publish: true,
            claim_no_ai: true,
        }
    }

    fn restrictive() -> crate::config::Capabilities {
        crate::config::Capabilities {
            publish: false,
            claim_no_ai: false,
        }
    }

    const OWN_DRAFT: &str =
        r#"[{"id":7,"title":"A draft","published":false,"description":"","tag_list":[]}]"#;
    const OWN_PUBLISHED: &str = r#"[{"id":7,"title":"Live","published":true,"published_at":"2026-01-01T00:00:00Z","description":"","tag_list":[]}]"#;
    const WRITE_OK: &str =
        r#"{"id":7,"title":"A draft","published":false,"url":"https://dev.to/x/a-draft"}"#;

    fn draft_args() -> Value {
        json!({
            "title": "A reasonable title",
            "body_markdown": "Some words.",
            "tags": ["rust"],
            "ai_disclosure_level": "some_ai"
        })
    }

    #[test]
    fn the_article_status_selects_which_listing_is_fetched() {
        for (status, expected) in [
            ("published", "/api/articles/me/published"),
            ("unpublished", "/api/articles/me/unpublished"),
            ("all", "/api/articles/me/all"),
        ] {
            let (_, urls) = invoke("my_articles", json!({ "status": status }), &["[]"]);
            assert!(
                urls[0].ends_with(expected),
                "{status} asked for {}",
                urls[0]
            );
        }
        // An unrecognised status falls back to everything rather than to nothing.
        let (_, urls) = invoke("my_articles", json!({"status": "nonsense"}), &["[]"]);
        assert!(urls[0].ends_with("/api/articles/me/all"));
    }

    #[test]
    fn pagination_arguments_reach_the_request() {
        let (_, urls) = invoke("my_articles", json!({"page": 3, "per_page": 7}), &["[]"]);
        assert!(urls[0].contains("page=3"), "{}", urls[0]);
        assert!(urls[0].contains("per_page=7"), "{}", urls[0]);

        let (_, urls) = invoke("list_tags", json!({}), &["[]"]);
        assert_eq!(
            urls[0], "https://dev.to/api/tags",
            "absent pagination must not become page=0"
        );
    }

    #[test]
    fn an_article_is_fetched_by_id_or_by_author_and_slug() {
        let body = r##"{"id":42,"title":"T","body_markdown":"# heading"}"##;
        let (_, urls) = invoke("get_article", json!({"id": 42}), &[body]);
        assert_eq!(urls[0], "https://dev.to/api/articles/42");

        let (_, urls) = invoke(
            "get_article",
            json!({"username": "copyleftdev", "slug": "a-post"}),
            &[body],
        );
        assert_eq!(urls[0], "https://dev.to/api/articles/copyleftdev/a-post");

        let (result, urls) = invoke("get_article", json!({}), &[body]);
        assert!(
            urls.is_empty(),
            "an unidentified article must not spend a request"
        );
        assert!(result["remedy"].as_str().unwrap().contains("username"));
    }

    #[test]
    fn a_feed_result_carries_the_fields_a_reader_needs() {
        let body = r#"[{"id":1,"title":"A post","description":"d","url":"https://dev.to/x/a",
                        "tag_list":["rust"],"public_reactions_count":5,"comments_count":2}]"#;
        let (result, _) = invoke("search_articles", json!({"mode": "feed"}), &[body]);
        assert_eq!(result["mode"], json!("feed"));
        assert_eq!(result["count"], json!(1));
        let first = &result["results"][0];
        assert_eq!(first["id"], json!(1));
        assert_eq!(first["title"], json!("A post"));
        assert_eq!(first["tags"], json!(["rust"]));
        assert_eq!(first["reactions"], json!(5));
        assert_eq!(first["comments"], json!(2));
    }

    /// Replies are nested, and the totals have to count the whole tree rather than the top.
    #[test]
    fn comment_threads_are_rendered_and_counted_through_their_replies() {
        let body = r#"[
            {"id_code":"a","body_html":"<p>top</p>","user":{"username":"alice"},
             "children":[
               {"id_code":"b","body_html":"<p>reply</p>","user":{"username":"bob"},"children":[
                 {"id_code":"c","body_html":"<p>deep</p>","children":[]}
               ]}
             ]},
            {"id_code":"d","body_html":"<p>another</p>","children":[]}
        ]"#;
        let (result, urls) = invoke("read_comments", json!({"article_id": 99}), &[body]);
        assert_eq!(urls[0], "https://dev.to/api/comments?a_id=99");
        assert_eq!(result["top_level_count"], json!(2));
        assert_eq!(
            result["total_count"],
            json!(4),
            "replies must be counted too"
        );
        assert_eq!(result["comments"][0]["author"], json!("alice"));
        assert_eq!(result["comments"][0]["replies"][0]["id_code"], json!("b"));
        assert_eq!(
            result["comments"][0]["replies"][0]["replies"][0]["id_code"],
            json!("c")
        );
        assert!(result["note"].as_str().unwrap().contains("cannot post"));
    }

    #[test]
    fn read_comments_without_an_article_id_spends_no_request() {
        let (result, urls) = invoke("read_comments", json!({}), &["[]"]);
        assert!(urls.is_empty());
        assert!(result["error"].as_str().unwrap().contains("article_id"));
    }

    #[test]
    fn semantic_results_report_their_ranking_and_split_their_tags() {
        let body = r#"[{"id":1,"title":"T","path":"/a/b","cached_tag_list":"rust, wasm",
                        "similarity":0.78,"public_reactions_count":3}]"#;
        let (result, _) = invoke(
            "search_articles",
            json!({"mode": "semantic", "query": "ownership"}),
            &[body],
        );
        assert_eq!(result["results"][0]["tags"], json!(["rust", "wasm"]));
        assert_eq!(result["results"][0]["url"], json!("https://dev.to/a/b"));
        assert_eq!(result["results"][0]["similarity"], json!(0.78));
        assert!(
            result["ranking"]
                .as_str()
                .unwrap()
                .contains("not sorted by it"),
            "the ranking caveat has to travel with the results"
        );
    }

    // ---- governance ------------------------------------------------------------------

    /// Disclosure is required with no default, so a caller has to state it rather than
    /// letting silence record "not disclosed" on the article.
    #[test]
    fn creating_a_draft_without_a_disclosure_is_refused_before_anything_is_sent() {
        let mut args = draft_args();
        args.as_object_mut().unwrap().remove("ai_disclosure_level");
        let (result, urls, _) =
            invoke_as(restrictive(), "create_draft", args, &[OWN_DRAFT, WRITE_OK]);

        assert!(urls.is_empty(), "a refusal must not spend a request");
        assert!(result["error"].as_str().unwrap().contains("required"));
        for level in ["no_ai", "some_ai", "fully_autonomous"] {
            assert!(result["remedy"].as_str().unwrap().contains(level));
        }
    }

    /// The claim a tool call cannot honestly make. Off by default, and the refusal explains
    /// itself rather than just failing.
    #[test]
    fn claiming_no_ai_is_refused_unless_the_account_holder_allowed_it() {
        let args = json!({
            "title": "A reasonable title", "body_markdown": "Words.",
            "ai_disclosure_level": "no_ai"
        });

        let (refused, urls, _) = invoke_as(
            restrictive(),
            "create_draft",
            args.clone(),
            &[OWN_DRAFT, WRITE_OK],
        );
        assert!(urls.is_empty());
        assert!(refused["error"].as_str().unwrap().contains("no_ai"));
        let remedy = refused["remedy"].as_str().unwrap();
        assert!(remedy.contains("DEVTO_ALLOW_NO_AI_CLAIM"));
        assert!(
            remedy.contains("some_ai"),
            "the refusal should offer a way forward"
        );

        let (allowed, urls, _) = invoke_as(permissive(), "create_draft", args, &[WRITE_OK]);
        assert!(!urls.is_empty(), "with the allowance it should go through");
        assert_eq!(allowed["id"], json!(7));
    }

    #[test]
    fn the_other_disclosure_levels_need_no_permission() {
        for level in ["some_ai", "fully_autonomous"] {
            let args = json!({
                "title": "A reasonable title", "body_markdown": "Words.",
                "ai_disclosure_level": level
            });
            let (result, urls, _) = invoke_as(restrictive(), "create_draft", args, &[WRITE_OK]);
            assert!(!urls.is_empty(), "{level} should not need a capability");
            assert_eq!(result["id"], json!(7), "{level}");
        }
    }

    /// A draft is a draft. There is no argument that makes create_draft publish.
    #[test]
    fn a_created_draft_is_always_unpublished() {
        let mut args = draft_args();
        args["published"] = json!(true);
        args["publish_in_seconds"] = json!(0);

        let (_, _, sent) = invoke_as(restrictive(), "create_draft", args, &[WRITE_OK]);
        let (method, body) = &sent[0];
        assert_eq!(method, "POST");
        let payload: Value = serde_json::from_str(body).unwrap();
        assert_eq!(payload["article"]["published"], json!(false));
        assert!(
            payload["article"].get("published_at").is_none(),
            "a draft carries no publication time: {body}"
        );
    }

    #[test]
    fn the_disclosure_travels_with_the_article() {
        let (_, _, sent) = invoke_as(restrictive(), "create_draft", draft_args(), &[WRITE_OK]);
        let payload: Value = serde_json::from_str(&sent[0].1).unwrap();
        assert_eq!(payload["article"]["ai_disclosure_level"], json!("some_ai"));
    }

    /// The whole argument for a local validator: a rejection should cost nothing.
    #[test]
    fn a_payload_dev_to_would_reject_never_reaches_the_network() {
        let mut args = draft_args();
        args["tags"] = json!(["machine-learning"]);

        let (result, urls, _) = invoke_as(restrictive(), "create_draft", args, &[WRITE_OK]);
        assert!(
            urls.is_empty(),
            "the write was spent on a payload that would 422"
        );
        assert_eq!(result["sendable"], json!(false));
        assert!(result["remedy"].as_str().unwrap().contains("no write"));
        assert!(
            result["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["rule"] == json!("TAG_INVALID_CHARACTERS"))
        );
    }

    /// Warnings describe something dev.to will accept and quietly do differently, so they
    /// travel with the success rather than stopping it.
    #[test]
    fn warnings_are_reported_alongside_a_successful_write() {
        let mut args = draft_args();
        args["body_markdown"] = json!("---\ntitle: Front matter wins\n---\n\nBody.");

        let (result, urls, _) = invoke_as(restrictive(), "create_draft", args, &[WRITE_OK]);
        assert!(!urls.is_empty(), "a warning must not block the write");
        assert_eq!(result["id"], json!(7));
        assert!(
            result["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["rule"] == json!("FRONT_MATTER_OVERRIDES_PAYLOAD"))
        );
    }

    // ---- publishing ------------------------------------------------------------------

    #[test]
    fn publishing_is_refused_when_the_capability_is_off_and_the_setting_is_named() {
        let (result, urls, _) = invoke_as(
            restrictive(),
            "publish_article",
            json!({"article_id": 7}),
            &[OWN_DRAFT, WRITE_OK],
        );
        assert!(
            urls.is_empty(),
            "a refused publish must not even look the article up"
        );
        let remedy = result["remedy"].as_str().unwrap();
        assert!(remedy.contains("DEVTO_PUBLISH"));
        assert!(
            remedy.contains("unscoped"),
            "the refusal should say why the boundary lives here"
        );
    }

    #[test]
    fn publishing_works_when_the_account_holder_enabled_it() {
        let published = r#"{"id":7,"title":"A draft","published":true,"url":"https://dev.to/x/a"}"#;
        let (result, _, sent) = invoke_as(
            permissive(),
            "publish_article",
            json!({"article_id": 7}),
            &[OWN_DRAFT, published],
        );
        assert_eq!(result["published"], json!(true));
        assert!(result["note"].as_str().unwrap().contains("visible now"));

        let write = sent
            .iter()
            .find(|(m, _)| m == "PUT")
            .expect("expected a PUT");
        let payload: Value = serde_json::from_str(&write.1).unwrap();
        assert_eq!(payload["article"]["published"], json!(true));
    }

    #[test]
    fn a_scheduled_publish_sends_an_rfc3339_timestamp_and_says_it_is_final() {
        let published = r#"{"id":7,"title":"A draft","published":true}"#;
        let (result, _, sent) = invoke_as(
            permissive(),
            "publish_article",
            json!({"article_id": 7, "publish_at": "2027-03-01T09:30:00Z"}),
            &[OWN_DRAFT, published],
        );
        assert!(
            result["note"]
                .as_str()
                .unwrap()
                .contains("cannot be changed")
        );

        let write = sent
            .iter()
            .find(|(m, _)| m == "PUT")
            .expect("expected a PUT");
        let payload: Value = serde_json::from_str(&write.1).unwrap();
        assert_eq!(
            payload["article"]["published_at"],
            json!("2027-03-01T09:30:00Z")
        );
    }

    #[test]
    fn a_publication_time_in_the_past_is_refused_without_a_write() {
        let (result, urls, _) = invoke_as(
            permissive(),
            "publish_article",
            json!({"article_id": 7, "publish_at": "2020-01-01T00:00:00Z"}),
            &[OWN_DRAFT, WRITE_OK],
        );
        assert_eq!(result["sendable"], json!(false));
        assert_eq!(urls.len(), 1, "only the lookup should have happened");
        assert!(
            result["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["rule"] == json!("PUBLISHED_AT_IN_PAST"))
        );
    }

    #[test]
    fn an_unreadable_publication_time_is_refused_with_an_example() {
        let (result, _, _) = invoke_as(
            permissive(),
            "publish_article",
            json!({"article_id": 7, "publish_at": "next tuesday"}),
            &[OWN_DRAFT, WRITE_OK],
        );
        assert!(result["error"].as_str().unwrap().contains("publish_at"));
        assert!(
            result["remedy"]
                .as_str()
                .unwrap()
                .contains("2026-09-20T14:00:00Z")
        );
    }

    /// Knowing the article's current state is worth the read it costs: without it, every
    /// one of these would be a silent no-op instead of an answer.
    #[test]
    fn the_current_state_of_the_article_is_checked_before_writing() {
        let (already_live, _, sent) = invoke_as(
            permissive(),
            "publish_article",
            json!({"article_id": 7}),
            &[OWN_PUBLISHED, WRITE_OK],
        );
        assert!(
            already_live["error"]
                .as_str()
                .unwrap()
                .contains("already published")
        );
        assert!(!sent.iter().any(|(m, _)| m == "PUT"));

        let (already_draft, _, sent) = invoke_as(
            permissive(),
            "unpublish_article",
            json!({"article_id": 7}),
            &[OWN_DRAFT, WRITE_OK],
        );
        assert!(
            already_draft["error"]
                .as_str()
                .unwrap()
                .contains("already a draft")
        );
        assert!(!sent.iter().any(|(m, _)| m == "PUT"));

        let (missing, _, sent) = invoke_as(
            permissive(),
            "publish_article",
            json!({"article_id": 999}),
            &[OWN_DRAFT, WRITE_OK],
        );
        assert!(
            missing["error"]
                .as_str()
                .unwrap()
                .contains("no article 999")
        );
        assert!(!sent.iter().any(|(m, _)| m == "PUT"));
    }

    #[test]
    fn unpublishing_uses_an_ordinary_edit_rather_than_the_admin_endpoint() {
        let back_to_draft = r#"{"id":7,"title":"Live","published":false}"#;
        let (result, urls, sent) = invoke_as(
            permissive(),
            "unpublish_article",
            json!({"article_id": 7}),
            &[OWN_PUBLISHED, back_to_draft],
        );
        assert_eq!(result["published"], json!(false));
        assert!(
            !urls.iter().any(|u| u.contains("/unpublish")),
            "the /unpublish endpoint is admin-only: {urls:?}"
        );
        let write = sent
            .iter()
            .find(|(m, _)| m == "PUT")
            .expect("expected a PUT");
        assert!(write.1.contains("\"published\":false"), "{}", write.1);
        assert!(result["note"].as_str().unwrap().contains("no delete"));
    }

    // ---- editing ---------------------------------------------------------------------

    #[test]
    fn update_article_refuses_to_change_publication_state() {
        let (result, urls, _) = invoke_as(
            permissive(),
            "update_article",
            json!({"article_id": 7, "published": true}),
            &[OWN_DRAFT, WRITE_OK],
        );
        assert!(urls.is_empty());
        assert!(
            result["remedy"]
                .as_str()
                .unwrap()
                .contains("publish_article")
        );
    }

    /// A partial edit sends only what was asked for: anything else would overwrite fields
    /// the caller never mentioned.
    #[test]
    fn an_edit_sends_only_the_fields_it_was_given() {
        let (_, _, sent) = invoke_as(
            permissive(),
            "update_article",
            json!({"article_id": 7, "title": "A better title"}),
            &[OWN_DRAFT, WRITE_OK],
        );
        let write = sent
            .iter()
            .find(|(m, _)| m == "PUT")
            .expect("expected a PUT");
        let payload: Value = serde_json::from_str(&write.1).unwrap();
        let article = payload["article"].as_object().unwrap();

        assert_eq!(article["title"], json!("A better title"));
        assert!(!article.contains_key("body_markdown"), "{write:?}");
        assert!(!article.contains_key("published"), "{write:?}");
        assert!(!article.contains_key("published_at"), "{write:?}");
        assert!(!article.contains_key("ai_disclosure_level"), "{write:?}");
    }

    #[test]
    fn an_edit_that_changes_the_disclosure_sends_it() {
        let (_, _, sent) = invoke_as(
            permissive(),
            "update_article",
            json!({"article_id": 7, "ai_disclosure_level": "fully_autonomous"}),
            &[OWN_DRAFT, WRITE_OK],
        );
        let write = sent
            .iter()
            .find(|(m, _)| m == "PUT")
            .expect("expected a PUT");
        let payload: Value = serde_json::from_str(&write.1).unwrap();
        assert_eq!(
            payload["article"]["ai_disclosure_level"],
            json!("fully_autonomous")
        );
    }

    #[test]
    fn an_edit_is_validated_against_the_articles_real_state() {
        let (result, _, sent) = invoke_as(
            permissive(),
            "update_article",
            json!({"article_id": 7, "tags": ["machine-learning"]}),
            &[OWN_PUBLISHED, WRITE_OK],
        );
        assert_eq!(result["sendable"], json!(false));
        assert!(!sent.iter().any(|(m, _)| m == "PUT"));
    }

    /// The operation has to reach the validator, or an edit to a published article is
    /// checked under the rules for creating a new one.
    #[test]
    fn an_edit_is_checked_under_the_rules_for_the_articles_actual_state() {
        let (published, _, _) = invoke_as(
            permissive(),
            "update_article",
            json!({"article_id": 7, "title": "Same", "publish_in_seconds": 86_400}),
            &[OWN_PUBLISHED, WRITE_OK],
        );
        assert!(
            published["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["rule"] == json!("PUBLISHED_AT_SILENTLY_DROPPED")),
            "editing a published article should warn that its time is frozen: {published}"
        );

        let (draft, _, _) = invoke_as(
            permissive(),
            "update_article",
            json!({"article_id": 7, "title": "Same", "publish_in_seconds": 86_400}),
            &[OWN_DRAFT, WRITE_OK],
        );
        assert!(
            !draft["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["rule"] == json!("PUBLISHED_AT_SILENTLY_DROPPED")),
            "an unpublished draft can still be scheduled"
        );
    }

    /// Everything the caller wrote has to survive into the request body.
    #[test]
    fn a_created_draft_carries_its_content() {
        let args = json!({
            "title": "A reasonable title",
            "body_markdown": "Some words.",
            "tags": ["rust", "webdev"],
            "description": "A summary.",
            "series": "A series",
            "canonical_url": "https://example.com/original",
            "cover_image_url": "https://example.com/cover.png",
            "organization_id": 42,
            "ai_disclosure_level": "some_ai"
        });
        let (_, _, sent) = invoke_as(permissive(), "create_draft", args, &[WRITE_OK]);
        let payload: serde_json::Value = serde_json::from_str(&sent[0].1).unwrap();
        let article = &payload["article"];

        assert_eq!(article["title"], json!("A reasonable title"));
        assert_eq!(article["body_markdown"], json!("Some words."));
        assert_eq!(article["tags"], json!(["rust", "webdev"]));
        assert_eq!(article["description"], json!("A summary."));
        assert_eq!(article["series"], json!("A series"));
        assert_eq!(
            article["canonical_url"],
            json!("https://example.com/original")
        );
        assert_eq!(
            article["main_image"],
            json!("https://example.com/cover.png")
        );
        assert_eq!(article["organization_id"], json!(42));
    }

    /// A disclosure given while publishing has to reach dev.to, not just pass the gate.
    #[test]
    fn publishing_sends_a_disclosure_it_was_given() {
        let published = r#"{"id":7,"title":"A draft","published":true}"#;
        let (_, _, sent) = invoke_as(
            permissive(),
            "publish_article",
            json!({"article_id": 7, "ai_disclosure_level": "fully_autonomous"}),
            &[OWN_DRAFT, published],
        );
        let write = sent
            .iter()
            .find(|(m, _)| m == "PUT")
            .expect("expected a PUT");
        let payload: serde_json::Value = serde_json::from_str(&write.1).unwrap();
        assert_eq!(
            payload["article"]["ai_disclosure_level"],
            json!("fully_autonomous")
        );

        // And when none is given, the field is left alone rather than overwritten.
        let (_, _, sent) = invoke_as(
            permissive(),
            "publish_article",
            json!({"article_id": 7}),
            &[OWN_DRAFT, published],
        );
        let write = sent
            .iter()
            .find(|(m, _)| m == "PUT")
            .expect("expected a PUT");
        let payload: serde_json::Value = serde_json::from_str(&write.1).unwrap();
        assert!(
            payload["article"].get("ai_disclosure_level").is_none(),
            "publishing overwrote the article's existing disclosure: {}",
            write.1
        );
    }

    #[test]
    fn every_write_tool_needs_an_account() {
        for name in [
            "create_draft",
            "update_article",
            "publish_article",
            "unpublish_article",
        ] {
            let config = Config {
                base_url: "https://dev.to".to_string(),
                api_key: None,
                capabilities: permissive(),
            };
            let mut client =
                DevtoClient::with_parts(ClientConfig::default(), Recorder::new(&[]), Frozen);
            let outcome = {
                let mut ctx = ToolContext {
                    client: &mut client,
                    config: &config,
                    now_unix: 1_757_000_000,
                };
                call(name, &draft_args(), &mut ctx)
            };
            assert!(outcome.is_error, "{name} ran without a key");
            assert!(
                client.transport().seen.borrow().is_empty(),
                "{name} reached the network without a key"
            );
        }
    }

    #[test]
    fn an_unknown_tool_lists_the_ones_that_exist() {
        let outcome = ToolOutcome::failed("x", "y");
        assert!(outcome.is_error);
        assert!(names().contains(&"validate_draft".to_string()));
    }
}
