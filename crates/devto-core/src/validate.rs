//! The validation engine.
//!
//! Pure: no I/O, no ambient clock, no network. Every rule mirrors a specific line of
//! Forem, cited at the rule. The point is that a caller can iterate a draft to clean for
//! free, and only then spend one of its writes — of which it gets one per second.

use crate::draft::{AiDisclosure, ArticleType, Context, Draft, Operation, UnixSeconds};
use crate::finding::{Field, Finding, Report, RuleId};
use crate::frontmatter::{self, DISCLOSURE_KEYS, FrontMatter};
use crate::limits::{
    BODY_MAX_BYTES, DUPLICATE_TITLE_WINDOW_SECS, MAX_TAGS, PUBLISHED_AT_PAST_GRACE_SECS,
    TAG_LIST_MAX_CHARS, TAG_MAX_CHARS,
};
use crate::tags;
use crate::urls::{self, UrlIssue};

const WEB_SCHEMES: [&str; 2] = ["https", "http"];
const HTTPS_ONLY: [&str; 1] = ["https"];

/// Check a draft against everything Forem will check, plus everything Forem will silently
/// do to it. `now` is the caller's clock; nothing here reads one.
pub fn validate(draft: &Draft, now: UnixSeconds, ctx: &Context) -> Report {
    let mut report = Report::default();
    let front_matter = frontmatter::parse(&draft.body_markdown);

    check_title(draft, now, ctx, &mut report);
    check_body(draft, &mut report);
    check_tags(draft, &mut report);
    check_canonical_url(draft, ctx, &mut report);
    check_main_image(draft, ctx, &front_matter, &mut report);
    check_video_source_url(draft, &mut report);
    check_published_at(draft, now, ctx, &mut report);
    check_article_type(draft, ctx, &mut report);
    check_disclosure(draft, &front_matter, &mut report);
    check_front_matter(draft, &front_matter, &mut report);

    report
}

/// `Article#title_length_based_on_type_of` measures the title with every whitespace
/// character removed, so a padded title can pass a naive character count and still fail.
fn visible_title_len(title: &str) -> usize {
    title.chars().filter(|c| !c.is_whitespace()).count()
}

fn check_title(draft: &Draft, now: UnixSeconds, ctx: &Context, report: &mut Report) {
    if draft.title.trim().is_empty() {
        report.push(Finding::blocking(
            RuleId::TitleBlank,
            Field::Title,
            "The title is empty.",
            "Give the article a title.",
        ));
        return;
    }

    let max = draft.title_max_chars();
    let len = visible_title_len(&draft.title);
    if len > max {
        report.push(Finding::blocking(
            RuleId::TitleTooLong,
            Field::Title,
            format!(
                "The title is {len} characters once whitespace is stripped; the limit for this \
                 post type is {max}."
            ),
            format!("Shorten the title by at least {} characters.", len - max),
        ));
    }

    if ctx.operation == Operation::Create {
        let cutoff = now - DUPLICATE_TITLE_WINDOW_SECS;
        if ctx
            .recent_titles
            .iter()
            .any(|r| r.created_at > cutoff && r.title == draft.title)
        {
            report.push(Finding::blocking(
                RuleId::TitleDuplicateRecent,
                Field::Title,
                "You posted an article with this exact title in the last five minutes; Forem \
                 rejects it as a duplicate.",
                "Wait five minutes, change the title, or update the existing article instead of \
                 creating a second one.",
            ));
        }
    }
}

fn check_body(draft: &Draft, report: &mut Report) {
    let bytes = draft.body_markdown.len();
    if bytes > BODY_MAX_BYTES {
        report.push(Finding::blocking(
            RuleId::BodyTooLarge,
            Field::BodyMarkdown,
            format!(
                "The body is {bytes} bytes; the limit is {BODY_MAX_BYTES}. Forem counts bytes, \
                 not characters, so non-ASCII content costs more than it looks."
            ),
            format!("Remove at least {} bytes.", bytes - BODY_MAX_BYTES),
        ));
    }

    if draft.article_type == ArticleType::Status && !body_is_status_safe(&draft.body_markdown) {
        report.push(Finding::blocking(
            RuleId::StatusBodyNotAllowed,
            Field::BodyMarkdown,
            "Status posts cannot carry a body; Forem accepts one only when it is nothing but \
             embed tags derived from URLs in the title.",
            "Clear body_markdown, or set article_type to full_post.",
        ));
    }
}

/// A status post may carry a body only when it is purely liquid embed tags.
fn body_is_status_safe(body: &str) -> bool {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return true;
    }
    let mut rest = trimmed;
    while let Some(open) = rest.find("{%") {
        if !rest[..open].trim().is_empty() {
            return false;
        }
        let Some(close) = rest[open..].find("%}") else {
            return false;
        };
        rest = &rest[open + close + 2..];
    }
    rest.trim().is_empty()
}

fn check_tags(draft: &Draft, report: &mut Report) {
    let normalized = tags::normalize(&draft.tags);

    for tag in &normalized {
        if tag.source_index < draft.tags.len() && draft.tags[tag.source_index].contains(',') {
            report.push(Finding::warning(
                RuleId::TagSplitOnComma,
                Field::Tags,
                format!(
                    "Entry {} of the tags array contains a comma, so Forem splits it into \
                     separate tags. This can push the post over the four-tag limit.",
                    tag.source_index
                ),
                "Send each tag as its own array entry instead of one comma-separated string.",
            ));
            break;
        }
    }

    if normalized.len() > MAX_TAGS {
        report.push(Finding::blocking(
            RuleId::TooManyTags,
            Field::Tags,
            format!(
                "{} tags after Forem's parsing; the limit is {MAX_TAGS}.",
                normalized.len()
            ),
            format!("Remove {} tag(s).", normalized.len() - MAX_TAGS),
        ));
    }

    for tag in &normalized {
        let bad = tags::invalid_characters(&tag.value);
        if !bad.is_empty() {
            let stripped: String = tag.value.chars().filter(|c| c.is_alphanumeric()).collect();
            let suggestion = if stripped.is_empty() {
                "Choose a tag made only of letters and digits.".to_string()
            } else {
                format!("Use \"{stripped}\" instead.")
            };
            report.push(Finding::blocking(
                RuleId::TagInvalidCharacters,
                Field::Tags,
                format!(
                    "Tag \"{}\" contains {}. Forem tags are letters and digits only — no hyphens, \
                     underscores, dots or spaces.",
                    tag.value,
                    describe_chars(&bad)
                ),
                suggestion,
            ));
        }

        if tags::is_too_long(&tag.value) {
            report.push(Finding::blocking(
                RuleId::TagTooLong,
                Field::Tags,
                format!(
                    "Tag \"{}\" is {} characters; the limit is {TAG_MAX_CHARS}.",
                    tag.value,
                    tag.value.chars().count()
                ),
                "Shorten the tag.",
            ));
        }

        if tag.raw.to_lowercase() != tag.raw {
            report.push(Finding::warning(
                RuleId::TagNotLowercase,
                Field::Tags,
                format!(
                    "Tag \"{}\" will be stored as \"{}\" — Forem downcases every tag.",
                    tag.raw, tag.value
                ),
                format!(
                    "Send \"{}\" if you want the payload to match what is stored.",
                    tag.value
                ),
            ));
        }
    }

    if tags::tag_list_too_long(&normalized) {
        report.push(Finding::blocking(
            RuleId::TagListTooLong,
            Field::Tags,
            format!(
                "The comma-joined tag list is {} characters; Forem caps the stored list at \
                 {TAG_LIST_MAX_CHARS}.",
                tags::cached_tag_list(&normalized).chars().count()
            ),
            "Use fewer or shorter tags.",
        ));
    }
}

fn describe_chars(chars: &[char]) -> String {
    let mut seen: Vec<char> = Vec::new();
    for c in chars {
        if !seen.contains(c) {
            seen.push(*c);
        }
    }
    let rendered: Vec<String> = seen.iter().map(|c| format!("'{c}'")).collect();
    rendered.join(", ")
}

fn check_canonical_url(draft: &Draft, ctx: &Context, report: &mut Report) {
    let Some(url) = draft
        .canonical_url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
    else {
        return;
    };

    for issue in urls::check(url, &WEB_SCHEMES, true) {
        let finding = match issue {
            UrlIssue::Whitespace => Finding::blocking(
                RuleId::CanonicalUrlWhitespace,
                Field::CanonicalUrl,
                "The canonical URL contains whitespace.",
                "Remove the whitespace, or percent-encode it.",
            ),
            UrlIssue::BadScheme | UrlIssue::NoHost => Finding::blocking(
                RuleId::CanonicalUrlScheme,
                Field::CanonicalUrl,
                "The canonical URL must be an absolute http or https URL.",
                "Supply the full URL including the scheme, for example https://example.com/post.",
            ),
            UrlIssue::LocalHost => Finding::blocking(
                RuleId::CanonicalUrlLocal,
                Field::CanonicalUrl,
                "Forem rejects local hosts in a canonical URL.",
                "Point the canonical URL at the publicly reachable original.",
            ),
        };
        report.push(finding);
    }

    if ctx.known_canonical_urls.iter().any(|known| known == url) {
        report.push(Finding::blocking(
            RuleId::CanonicalUrlCollision,
            Field::CanonicalUrl,
            "Another published article already claims this canonical URL. Forem enforces \
             uniqueness across published articles.",
            "Use the canonical URL of this specific piece, or leave it unset.",
        ));
    }
}

fn check_main_image(draft: &Draft, ctx: &Context, fm: &FrontMatter, report: &mut Report) {
    if let Some(url) = draft.main_image.as_deref().filter(|u| !u.trim().is_empty())
        && !urls::check(url, &WEB_SCHEMES, false).is_empty()
    {
        report.push(Finding::blocking(
            RuleId::MainImageScheme,
            Field::MainImage,
            "The cover image must be an absolute http or https URL. There is no image upload \
             on the API — the file has to be hosted somewhere public already.",
            "Host the image and pass its URL, for example a raw.githubusercontent.com link.",
        ));
    }

    let sticky = matches!(
        ctx.operation,
        Operation::Update {
            main_image_from_frontmatter: true,
            ..
        }
    );
    if sticky && fm.present && !fm.has("cover_image") {
        report.push(Finding::warning(
            RuleId::MainImageIgnoredFrontMatterSticky,
            Field::MainImage,
            "This article's cover image was originally set from front matter, which makes it \
             permanently front-matter-driven. Because the body has front matter without a \
             cover_image key, Forem will clear the cover image.",
            "Add a cover_image key to the front matter, or remove the front matter block entirely.",
        ));
    }
}

fn check_video_source_url(draft: &Draft, report: &mut Report) {
    let Some(url) = draft
        .video_source_url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
    else {
        return;
    };

    if !urls::is_permitted_video_source(url) {
        report.push(Finding::blocking(
            RuleId::VideoSourceUrlNotAllowed,
            Field::VideoSourceUrl,
            "Only YouTube, player.mux.com and twitch.tv/videos URLs are accepted. The controller \
             drops anything else from the payload silently, so the article saves without a video.",
            "Use a youtube.com/watch?v=, youtu.be/, player.mux.com/ or twitch.tv/videos/ URL, or \
             embed the video with a liquid tag in the body instead.",
        ));
        return;
    }

    if !urls::check(url, &HTTPS_ONLY, false).is_empty() {
        report.push(Finding::blocking(
            RuleId::VideoSourceUrlNotHttps,
            Field::VideoSourceUrl,
            "The controller accepts http for this field but the model validates https only, so \
             an http URL passes the first check and fails the second.",
            "Use the https form of the same URL.",
        ));
    }
}

fn check_published_at(draft: &Draft, now: UnixSeconds, ctx: &Context, report: &mut Report) {
    let Some(at) = draft.published_at else {
        return;
    };
    let floor = now - PUBLISHED_AT_PAST_GRACE_SECS;

    match ctx.operation {
        // `Article#future_or_current_published_at` is scoped to `on: :create` and only fires
        // when the article is being published — a draft may carry any published_at.
        Operation::Create => {
            if draft.published && at <= floor {
                report.push(Finding::blocking(
                    RuleId::PublishedAtInPast,
                    Field::PublishedAt,
                    "published_at is more than 15 minutes in the past. Forem will not backdate a \
                     post being published.",
                    "Use a future time to schedule, or omit published_at to publish now.",
                ));
            }
        }
        // `Articles::Updater` deletes the param before the model ever sees it when the article
        // has a published_at and is not scheduled. No error, no change.
        Operation::Update {
            already_published: true,
            scheduled: false,
            ..
        } => {
            report.push(Finding::warning(
                RuleId::PublishedAtSilentlyDropped,
                Field::PublishedAt,
                "This article is already published, so Forem discards published_at without \
                 reporting anything. The publication time will not move.",
                "Drop published_at from the payload; it cannot be changed after publication.",
            ));
        }
        // A draft or a scheduled post can still be moved, but not into the past.
        Operation::Update { .. } => {
            if at <= floor {
                report.push(Finding::blocking(
                    RuleId::PublishedAtInPast,
                    Field::PublishedAt,
                    "published_at is more than 15 minutes in the past.",
                    "Use a future time, or omit published_at.",
                ));
            }
        }
    }
}

fn check_article_type(draft: &Draft, ctx: &Context, report: &mut Report) {
    if draft.article_type == ArticleType::FullscreenEmbed && !ctx.author_is_admin {
        report.push(Finding::blocking(
            RuleId::FullscreenEmbedRequiresAdmin,
            Field::ArticleType,
            "fullscreen_embed is restricted to admins.",
            "Use full_post or status.",
        ));
    }
}

fn check_disclosure(draft: &Draft, fm: &FrontMatter, report: &mut Report) {
    let declared_in_front_matter = DISCLOSURE_KEYS.iter().any(|k| fm.has(k));
    if draft.ai_disclosure_level == AiDisclosure::NotDisclosed && !declared_in_front_matter {
        report.push(Finding::warning(
            RuleId::DisclosureNotDisclosed,
            Field::AiDisclosureLevel,
            "No AI disclosure. dev.to records this as not_disclosed and asks automated clients \
             to state a level accurately.",
            "Set ai_disclosure_level to no_ai, some_ai or fully_autonomous. \
             See https://dev.to/llms.txt for the definitions.",
        ));
    }
}

fn check_front_matter(draft: &Draft, fm: &FrontMatter, report: &mut Report) {
    if !fm.present {
        return;
    }

    let conflicts = conflicting_keys(draft, fm);
    if !conflicts.is_empty() {
        report.push(Finding::warning(
            RuleId::FrontMatterOverridesPayload,
            Field::FrontMatter,
            format!(
                "The body's front matter sets {}, which Forem applies after the API payload. \
                 The front matter wins and no error is reported.",
                conflicts.join(", ")
            ),
            "Remove these keys from the front matter, or stop setting the matching API fields — \
             pick one source of truth per field.",
        ));
    }

    // `evaluate_front_matter` nils collection_id whenever the front matter carries a title,
    // and only restores it if the front matter also names a series.
    if fm.has("title") && !fm.has("series") && draft.series.is_some() {
        report.push(Finding::warning(
            RuleId::FrontMatterDropsSeries,
            Field::Series,
            "The front matter has a title but no series, which makes Forem remove this article \
             from its series.",
            "Add the series to the front matter as well, or remove the title key from it.",
        ));
    }
}

fn conflicting_keys(draft: &Draft, fm: &FrontMatter) -> Vec<String> {
    let mut out = Vec::new();
    let mut note = |key: &str| out.push(key.to_string());

    if fm.has("title") && !draft.title.is_empty() && fm.get("title") != Some(draft.title.as_str()) {
        note("title");
    }
    if fm.has("tags") && !draft.tags.is_empty() {
        note("tags");
    }
    if fm.has("published")
        && matches!(fm.get("published"), Some("true") | Some("false"))
        && fm.get("published") != Some(if draft.published { "true" } else { "false" })
    {
        note("published");
    }
    if (fm.has("published_at") || fm.has("date")) && draft.published_at.is_some() {
        note("published_at");
    }
    if fm.has("cover_image")
        && draft.main_image.is_some()
        && fm.get("cover_image") != draft.main_image.as_deref()
    {
        note("cover_image (the front matter spelling of main_image)");
    }
    if fm.has("canonical_url")
        && draft.canonical_url.is_some()
        && fm.get("canonical_url") != draft.canonical_url.as_deref()
    {
        note("canonical_url");
    }
    if fm.has("description")
        && draft.description.is_some()
        && fm.get("description") != draft.description.as_deref()
    {
        note("description");
    }
    if fm.has("series") && draft.series.is_some() && fm.get("series") != draft.series.as_deref() {
        note("series");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draft::RecentTitle;
    use crate::tags;
    use hegel::Generator;
    use hegel::generators as g;

    /// A fixed instant, so every time-dependent assertion states its own boundary.
    const NOW: UnixSeconds = 1_757_000_000;

    fn draft(title: &str) -> Draft {
        Draft::new(title, "Body.")
    }

    // ---- worked examples -------------------------------------------------------------

    #[test]
    fn a_clean_draft_is_sendable() {
        let mut d = draft("A reasonable title");
        d.tags = vec!["rust".into(), "webdev".into()];
        d.ai_disclosure_level = AiDisclosure::SomeAi;
        let report = validate(&d, NOW, &Context::create());
        assert!(report.is_sendable(), "{:#?}", report.findings);
        assert!(report.is_empty(), "{:#?}", report.findings);
    }

    #[test]
    fn a_hyphenated_tag_is_rejected_with_the_joined_form_as_the_remedy() {
        let mut d = draft("Title");
        d.tags = vec!["machine-learning".into()];
        d.ai_disclosure_level = AiDisclosure::SomeAi;
        let report = validate(&d, NOW, &Context::create());
        let finding = report
            .findings
            .iter()
            .find(|f| f.rule == RuleId::TagInvalidCharacters)
            .expect("expected a tag character finding");
        assert!(
            finding.remedy.contains("machinelearning"),
            "{}",
            finding.remedy
        );
    }

    #[test]
    fn one_comma_bearing_entry_can_break_the_four_tag_limit() {
        let mut d = draft("Title");
        d.tags = vec!["rust,zig,go".into(), "c".into(), "ruby".into()];
        d.ai_disclosure_level = AiDisclosure::NoAi;
        let report = validate(&d, NOW, &Context::create());
        assert!(report.has_rule(RuleId::TagSplitOnComma));
        assert!(report.has_rule(RuleId::TooManyTags));
    }

    #[test]
    fn front_matter_overriding_the_payload_is_reported() {
        let mut d = draft("Payload title");
        d.body_markdown = "---\ntitle: Front matter title\ntags: webdev\n---\n\nBody.".into();
        d.tags = vec!["rust".into()];
        d.series = Some("A series".into());
        d.ai_disclosure_level = AiDisclosure::SomeAi;
        let report = validate(&d, NOW, &Context::create());
        assert!(report.has_rule(RuleId::FrontMatterOverridesPayload));
        assert!(report.has_rule(RuleId::FrontMatterDropsSeries));
        assert!(
            report.is_sendable(),
            "front matter conflicts are warnings, not rejections"
        );
    }

    #[test]
    fn republishing_an_article_cannot_move_its_publication_time() {
        let mut d = draft("Title");
        d.published = true;
        d.published_at = Some(NOW + 86_400);
        d.ai_disclosure_level = AiDisclosure::SomeAi;
        let report = validate(&d, NOW, &Context::update(true));
        assert!(report.has_rule(RuleId::PublishedAtSilentlyDropped));
        assert!(report.is_sendable());
    }

    #[test]
    fn a_recent_identical_title_blocks_a_create_but_not_an_update() {
        let mut d = draft("Exactly this title");
        d.ai_disclosure_level = AiDisclosure::NoAi;
        let mut ctx = Context::create();
        ctx.recent_titles = vec![RecentTitle {
            title: "Exactly this title".into(),
            created_at: NOW - 60,
        }];
        assert!(validate(&d, NOW, &ctx).has_rule(RuleId::TitleDuplicateRecent));

        ctx.recent_titles[0].created_at = NOW - DUPLICATE_TITLE_WINDOW_SECS - 1;
        assert!(!validate(&d, NOW, &ctx).has_rule(RuleId::TitleDuplicateRecent));

        d.title = "Exactly this title".into();
        let mut update_ctx = Context::update(false);
        update_ctx.recent_titles = vec![RecentTitle {
            title: "Exactly this title".into(),
            created_at: NOW - 60,
        }];
        assert!(!validate(&d, NOW, &update_ctx).has_rule(RuleId::TitleDuplicateRecent));
    }

    /// The limit is on bytes, so a body well under any character count can still be too big.
    #[test]
    fn the_body_limit_counts_bytes_not_characters() {
        for unit in ["é", "漢", "🌱"] {
            let width = unit.len();
            let at_limit = unit.repeat(BODY_MAX_BYTES / width);
            assert_eq!(at_limit.len(), BODY_MAX_BYTES - BODY_MAX_BYTES % width);
            assert!(
                at_limit.chars().count() < BODY_MAX_BYTES,
                "the point of this test is that the character count is well under the limit"
            );

            let mut d = draft("Title");
            d.body_markdown = at_limit;
            d.ai_disclosure_level = AiDisclosure::NoAi;
            assert!(!validate(&d, NOW, &Context::create()).has_rule(RuleId::BodyTooLarge));

            d.body_markdown
                .push_str(&"a".repeat(BODY_MAX_BYTES % width + 1));
            assert!(
                validate(&d, NOW, &Context::create()).has_rule(RuleId::BodyTooLarge),
                "{unit} at {} bytes should be over the limit",
                d.body_markdown.len()
            );
        }
    }

    // ---- properties ------------------------------------------------------------------

    fn any_url_ish(tc: &hegel::TestCase) -> String {
        tc.draw(hegel::one_of!(g::text().max_size(60), g::urls()))
    }

    fn any_draft(tc: &hegel::TestCase) -> Draft {
        Draft {
            title: tc.draw(g::text().max_size(200)),
            body_markdown: tc.draw(hegel::one_of!(
                g::text().max_size(200),
                g::text()
                    .max_size(120)
                    .map(|s: String| format!("---\n{s}\n---\n\nBody.")),
            )),
            tags: tc.draw(g::vecs(g::text().max_size(40)).max_size(8)),
            description: tc.draw(g::optional(g::text().max_size(40))),
            series: tc.draw(g::optional(g::text().max_size(30))),
            canonical_url: tc.draw(g::optional(hegel::one_of!(
                g::text().max_size(60),
                g::urls()
            ))),
            main_image: tc.draw(g::optional(hegel::one_of!(
                g::text().max_size(60),
                g::urls()
            ))),
            video_source_url: tc.draw(g::optional(hegel::one_of!(
                g::text().max_size(60),
                g::urls()
            ))),
            organization_id: tc.draw(g::optional(g::integers::<i64>())),
            subforem_id: None,
            language: None,
            published: tc.draw(g::booleans()),
            published_at: tc.draw(g::optional(
                g::integers::<i64>()
                    .min_value(NOW - 100_000)
                    .max_value(NOW + 100_000),
            )),
            ai_disclosure_level: tc.draw(g::sampled_from(vec![
                AiDisclosure::NotDisclosed,
                AiDisclosure::NoAi,
                AiDisclosure::SomeAi,
                AiDisclosure::FullyAutonomous,
            ])),
            article_type: tc.draw(g::sampled_from(vec![
                ArticleType::FullPost,
                ArticleType::Status,
                ArticleType::FullscreenEmbed,
            ])),
        }
    }

    fn any_context(tc: &hegel::TestCase) -> Context {
        let operation = if tc.draw(g::booleans()) {
            Operation::Create
        } else {
            Operation::Update {
                already_published: tc.draw(g::booleans()),
                scheduled: tc.draw(g::booleans()),
                main_image_from_frontmatter: tc.draw(g::booleans()),
            }
        };
        Context {
            operation,
            known_canonical_urls: tc.draw(g::vecs(g::urls()).max_size(3)),
            recent_titles: Vec::new(),
            author_is_admin: tc.draw(g::booleans()),
        }
    }

    /// Callers hand us arbitrary strings they have never parsed. Nothing here may panic.
    #[hegel::test]
    fn validate_never_panics(tc: hegel::TestCase) {
        let draft = any_draft(&tc);
        let ctx = any_context(&tc);
        let report = validate(&draft, NOW, &ctx);
        for finding in &report.findings {
            assert!(!finding.message.is_empty());
            assert!(!finding.remedy.is_empty());
            assert!(!finding.rule.as_str().is_empty());
        }
    }

    /// Apply what each blocking finding says to do. This is the whole promise of the report:
    /// a caller that follows it converges on a payload Forem will accept, rather than
    /// discovering the next rule one wasted write at a time.
    fn apply_remedies(draft: &Draft, report: &Report) -> Draft {
        let mut d = draft.clone();
        for finding in report.blocking() {
            match finding.rule {
                RuleId::TitleBlank => d.title = "Untitled".into(),
                RuleId::TitleTooLong => {
                    let max = d.title_max_chars();
                    d.title = d
                        .title
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .take(max)
                        .collect();
                    if d.title.is_empty() {
                        d.title = "Untitled".into();
                    }
                }
                RuleId::BodyTooLarge => {
                    let mut cut = BODY_MAX_BYTES;
                    while !d.body_markdown.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    d.body_markdown.truncate(cut);
                }
                RuleId::StatusBodyNotAllowed => d.body_markdown.clear(),
                RuleId::FullscreenEmbedRequiresAdmin => d.article_type = ArticleType::FullPost,
                RuleId::TooManyTags | RuleId::TagListTooLong => {
                    let normalized = tags::normalize(&d.tags);
                    let keep = normalized.len().saturating_sub(1).min(MAX_TAGS);
                    d.tags = normalized.into_iter().take(keep).map(|t| t.value).collect();
                }
                RuleId::TagInvalidCharacters => {
                    d.tags = d
                        .tags
                        .iter()
                        .map(|t| {
                            t.chars()
                                .filter(|c| c.is_alphanumeric())
                                .collect::<String>()
                        })
                        .filter(|t| !t.is_empty())
                        .collect();
                }
                RuleId::TagTooLong => {
                    d.tags = d
                        .tags
                        .iter()
                        .map(|t| t.chars().take(TAG_MAX_CHARS).collect())
                        .collect();
                }
                RuleId::CanonicalUrlWhitespace
                | RuleId::CanonicalUrlScheme
                | RuleId::CanonicalUrlLocal
                | RuleId::CanonicalUrlCollision => d.canonical_url = None,
                RuleId::MainImageScheme => d.main_image = None,
                RuleId::VideoSourceUrlNotAllowed | RuleId::VideoSourceUrlNotHttps => {
                    d.video_source_url = None
                }
                RuleId::PublishedAtInPast => d.published_at = None,
                RuleId::TitleDuplicateRecent => d.title.push('!'),
                _ => {}
            }
        }
        d
    }

    #[hegel::test]
    fn following_the_remedies_converges_on_a_sendable_draft(tc: hegel::TestCase) {
        let mut draft = any_draft(&tc);
        let ctx = any_context(&tc);

        for round in 0..8 {
            let report = validate(&draft, NOW, &ctx);
            if report.is_sendable() {
                return;
            }
            let next = apply_remedies(&draft, &report);
            assert_ne!(
                next,
                draft,
                "round {round}: remedies changed nothing but {:#?} still blocks",
                report.blocking().map(|f| f.rule).collect::<Vec<_>>()
            );
            draft = next;
        }

        let report = validate(&draft, NOW, &ctx);
        assert!(
            report.is_sendable(),
            "still blocked after eight rounds: {:#?}",
            report.blocking().collect::<Vec<_>>()
        );
    }

    /// The title limit is measured with whitespace stripped, so padding is free and the
    /// boundary sits exactly on the count of visible characters.
    #[hegel::test]
    fn the_title_limit_ignores_whitespace(tc: hegel::TestCase) {
        let article_type = tc.draw(g::sampled_from(vec![
            ArticleType::FullPost,
            ArticleType::Status,
        ]));
        let pad: String = tc
            .draw(g::vecs(g::sampled_from(vec![' ', '\t', '\n', '\u{3000}'])).max_size(12))
            .into_iter()
            .collect();

        let mut d = draft("");
        d.article_type = article_type;
        d.ai_disclosure_level = AiDisclosure::NoAi;
        d.body_markdown = String::new();
        let max = d.title_max_chars();

        d.title = format!("{pad}{}{pad}", "a".repeat(max));
        assert!(
            !validate(&d, NOW, &Context::create()).has_rule(RuleId::TitleTooLong),
            "{max} visible characters plus padding should fit"
        );

        d.title = format!("{pad}{}{pad}", "a".repeat(max + 1));
        assert!(
            validate(&d, NOW, &Context::create()).has_rule(RuleId::TitleTooLong),
            "{} visible characters should not fit",
            max + 1
        );
    }

    /// `future_or_current_published_at` rejects a published_at at or before the fifteen-minute
    /// floor, and only when the article is actually being published.
    #[hegel::test]
    fn published_at_blocks_only_at_or_before_the_floor(tc: hegel::TestCase) {
        let offset = tc.draw(g::integers::<i64>().min_value(-3600).max_value(3600));
        let mut d = draft("Title");
        d.ai_disclosure_level = AiDisclosure::NoAi;
        d.published_at = Some(NOW + offset);

        d.published = true;
        assert_eq!(
            validate(&d, NOW, &Context::create()).has_rule(RuleId::PublishedAtInPast),
            offset <= -PUBLISHED_AT_PAST_GRACE_SECS,
        );

        d.published = false;
        assert!(
            !validate(&d, NOW, &Context::create()).has_rule(RuleId::PublishedAtInPast),
            "an unpublished draft may carry any published_at",
        );
    }

    /// An already-published article never gets a hard error about published_at — the param is
    /// discarded upstream, so the only honest report is a warning.
    #[hegel::test]
    fn moving_a_published_articles_time_warns_but_never_blocks(tc: hegel::TestCase) {
        let offset = tc.draw(g::integers::<i64>().min_value(-100_000).max_value(100_000));
        let mut d = draft("Title");
        d.ai_disclosure_level = AiDisclosure::NoAi;
        d.published_at = Some(NOW + offset);

        let ctx = Context {
            operation: Operation::Update {
                already_published: true,
                scheduled: false,
                main_image_from_frontmatter: false,
            },
            ..Context::create()
        };
        let report = validate(&d, NOW, &ctx);
        assert!(report.has_rule(RuleId::PublishedAtSilentlyDropped));
        assert!(!report.has_rule(RuleId::PublishedAtInPast));
    }

    /// A canonical URL already used by one of the author's published posts is a guaranteed
    /// 422, so the collision has to be caught whatever else is wrong with the URL.
    #[hegel::test]
    fn a_known_canonical_url_always_collides(tc: hegel::TestCase) {
        let url = any_url_ish(&tc);
        let mut d = draft("Title");
        d.ai_disclosure_level = AiDisclosure::NoAi;
        d.canonical_url = Some(url.clone());

        let mut ctx = Context::create();
        ctx.known_canonical_urls = vec![url.clone()];

        if url.trim().is_empty() {
            assert!(!validate(&d, NOW, &ctx).has_rule(RuleId::CanonicalUrlCollision));
        } else {
            assert!(validate(&d, NOW, &ctx).has_rule(RuleId::CanonicalUrlCollision));
        }
    }

    /// Every level round-trips through its own wire value, and Forem's front-matter aliases
    /// are case- and hyphen-insensitive.
    #[hegel::test]
    fn disclosure_levels_round_trip_through_their_wire_values(tc: hegel::TestCase) {
        let level = tc.draw(g::sampled_from(vec![
            AiDisclosure::NotDisclosed,
            AiDisclosure::NoAi,
            AiDisclosure::SomeAi,
            AiDisclosure::FullyAutonomous,
        ]));
        assert_eq!(
            AiDisclosure::from_front_matter_value(level.wire_value()),
            Some(level)
        );
        assert_eq!(
            AiDisclosure::from_front_matter_value(
                &level.wire_value().replace('_', "-").to_uppercase()
            ),
            Some(level)
        );
        assert_eq!(
            AiDisclosure::from_front_matter_value(&level.stored_enum().to_string()),
            Some(level)
        );
    }
}
