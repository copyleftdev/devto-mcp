use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Forem will reject this payload. Sending it spends a write and returns 422.
    Blocking,
    /// Forem will accept this payload, but it will not do what the caller meant.
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Title,
    BodyMarkdown,
    Tags,
    CanonicalUrl,
    MainImage,
    VideoSourceUrl,
    PublishedAt,
    Series,
    AiDisclosureLevel,
    ArticleType,
    FrontMatter,
}

/// Stable identifiers. A caller may key off these; the prose may be reworded, these may not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuleId {
    TitleBlank,
    TitleTooLong,
    TitleDuplicateRecent,
    BodyTooLarge,
    StatusBodyNotAllowed,
    FullscreenEmbedRequiresAdmin,
    TooManyTags,
    TagInvalidCharacters,
    TagTooLong,
    TagListTooLong,
    TagSplitOnComma,
    TagNotLowercase,
    TagDroppedAsEmpty,
    CanonicalUrlWhitespace,
    CanonicalUrlScheme,
    CanonicalUrlLocal,
    CanonicalUrlCollision,
    MainImageScheme,
    MainImageIgnoredFrontMatterSticky,
    VideoSourceUrlNotAllowed,
    VideoSourceUrlNotHttps,
    PublishedAtInPast,
    PublishedAtSilentlyDropped,
    DisclosureNotDisclosed,
    FrontMatterOverridesPayload,
    FrontMatterDropsSeries,
}

impl RuleId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TitleBlank => "TITLE_BLANK",
            Self::TitleTooLong => "TITLE_TOO_LONG",
            Self::TitleDuplicateRecent => "TITLE_DUPLICATE_RECENT",
            Self::BodyTooLarge => "BODY_TOO_LARGE",
            Self::StatusBodyNotAllowed => "STATUS_BODY_NOT_ALLOWED",
            Self::FullscreenEmbedRequiresAdmin => "FULLSCREEN_EMBED_REQUIRES_ADMIN",
            Self::TooManyTags => "TOO_MANY_TAGS",
            Self::TagInvalidCharacters => "TAG_INVALID_CHARACTERS",
            Self::TagTooLong => "TAG_TOO_LONG",
            Self::TagListTooLong => "TAG_LIST_TOO_LONG",
            Self::TagSplitOnComma => "TAG_SPLIT_ON_COMMA",
            Self::TagNotLowercase => "TAG_NOT_LOWERCASE",
            Self::TagDroppedAsEmpty => "TAG_DROPPED_AS_EMPTY",
            Self::CanonicalUrlWhitespace => "CANONICAL_URL_WHITESPACE",
            Self::CanonicalUrlScheme => "CANONICAL_URL_SCHEME",
            Self::CanonicalUrlLocal => "CANONICAL_URL_LOCAL",
            Self::CanonicalUrlCollision => "CANONICAL_URL_COLLISION",
            Self::MainImageScheme => "MAIN_IMAGE_SCHEME",
            Self::MainImageIgnoredFrontMatterSticky => "MAIN_IMAGE_IGNORED_FRONT_MATTER_STICKY",
            Self::VideoSourceUrlNotAllowed => "VIDEO_SOURCE_URL_NOT_ALLOWED",
            Self::VideoSourceUrlNotHttps => "VIDEO_SOURCE_URL_NOT_HTTPS",
            Self::PublishedAtInPast => "PUBLISHED_AT_IN_PAST",
            Self::PublishedAtSilentlyDropped => "PUBLISHED_AT_SILENTLY_DROPPED",
            Self::DisclosureNotDisclosed => "DISCLOSURE_NOT_DISCLOSED",
            Self::FrontMatterOverridesPayload => "FRONT_MATTER_OVERRIDES_PAYLOAD",
            Self::FrontMatterDropsSeries => "FRONT_MATTER_DROPS_SERIES",
        }
    }
}

/// One thing wrong with a draft, and what to do about it.
///
/// `remedy` is not decoration: the suite asserts that applying a finding's remedy clears
/// that finding, which is what makes the report usable by a caller that iterates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub rule: RuleId,
    pub severity: Severity,
    pub field: Field,
    pub message: String,
    pub remedy: String,
}

impl Finding {
    pub fn blocking(
        rule: RuleId,
        field: Field,
        message: impl Into<String>,
        remedy: impl Into<String>,
    ) -> Self {
        Self {
            rule,
            severity: Severity::Blocking,
            field,
            message: message.into(),
            remedy: remedy.into(),
        }
    }

    pub fn warning(
        rule: RuleId,
        field: Field,
        message: impl Into<String>,
        remedy: impl Into<String>,
    ) -> Self {
        Self {
            rule,
            severity: Severity::Warning,
            field,
            message: message.into(),
            remedy: remedy.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn push(&mut self, finding: Finding) {
        self.findings.push(finding);
    }

    /// True when nothing here will make Forem reject the payload. Warnings do not block.
    pub fn is_sendable(&self) -> bool {
        !self
            .findings
            .iter()
            .any(|f| f.severity == Severity::Blocking)
    }

    pub fn blocking(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Blocking)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Finding> {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
    }

    pub fn has_rule(&self, rule: RuleId) -> bool {
        self.findings.iter().any(|f| f.rule == rule)
    }

    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn len(&self) -> usize {
        self.findings.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_RULES: [RuleId; 26] = [
        RuleId::TitleBlank,
        RuleId::TitleTooLong,
        RuleId::TitleDuplicateRecent,
        RuleId::BodyTooLarge,
        RuleId::StatusBodyNotAllowed,
        RuleId::FullscreenEmbedRequiresAdmin,
        RuleId::TooManyTags,
        RuleId::TagInvalidCharacters,
        RuleId::TagTooLong,
        RuleId::TagListTooLong,
        RuleId::TagSplitOnComma,
        RuleId::TagNotLowercase,
        RuleId::TagDroppedAsEmpty,
        RuleId::CanonicalUrlWhitespace,
        RuleId::CanonicalUrlScheme,
        RuleId::CanonicalUrlLocal,
        RuleId::CanonicalUrlCollision,
        RuleId::MainImageScheme,
        RuleId::MainImageIgnoredFrontMatterSticky,
        RuleId::VideoSourceUrlNotAllowed,
        RuleId::VideoSourceUrlNotHttps,
        RuleId::PublishedAtInPast,
        RuleId::PublishedAtSilentlyDropped,
        RuleId::DisclosureNotDisclosed,
        RuleId::FrontMatterOverridesPayload,
        RuleId::FrontMatterDropsSeries,
    ];

    /// Rule ids are the stable part of the contract: a caller may branch on them, so they
    /// have to be distinct, and the string form has to be the one that appears in JSON.
    #[test]
    fn rule_ids_are_distinct_and_agree_with_their_serialized_form() {
        let mut seen = std::collections::BTreeSet::new();
        for rule in ALL_RULES {
            let name = rule.as_str();
            assert!(!name.is_empty(), "{rule:?} has no name");
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
                "{name} is not SCREAMING_SNAKE_CASE"
            );
            assert_eq!(
                serde_json::to_string(&rule).unwrap(),
                format!("\"{name}\""),
                "as_str and serde disagree for {rule:?}"
            );
            assert!(seen.insert(name), "duplicate rule id {name}");
        }
    }

    fn warning(rule: RuleId) -> Finding {
        Finding::warning(rule, Field::Tags, "message", "remedy")
    }

    fn blocking(rule: RuleId) -> Finding {
        Finding::blocking(rule, Field::Title, "message", "remedy")
    }

    #[test]
    fn an_empty_report_is_sendable() {
        let report = Report::default();
        assert!(report.is_empty());
        assert_eq!(report.len(), 0);
        assert!(report.is_sendable());
        assert_eq!(report.blocking().count(), 0);
        assert_eq!(report.warnings().count(), 0);
    }

    /// The severity split is the whole point of the report: a warning describes something
    /// Forem will accept and quietly do differently, so it must never stop a send.
    #[test]
    fn warnings_are_counted_but_do_not_block() {
        let mut report = Report::default();
        report.push(warning(RuleId::DisclosureNotDisclosed));
        report.push(warning(RuleId::TagSplitOnComma));

        assert!(!report.is_empty());
        assert_eq!(report.len(), 2);
        assert!(report.is_sendable());
        assert_eq!(report.warnings().count(), 2);
        assert_eq!(report.blocking().count(), 0);

        report.push(blocking(RuleId::TitleBlank));
        assert_eq!(report.len(), 3);
        assert!(!report.is_sendable());
        assert_eq!(report.warnings().count(), 2);
        assert_eq!(report.blocking().count(), 1);
        assert_eq!(
            report.blocking().next().map(|f| f.rule),
            Some(RuleId::TitleBlank)
        );
    }

    #[test]
    fn has_rule_finds_only_what_was_pushed() {
        let mut report = Report::default();
        report.push(blocking(RuleId::TitleBlank));
        assert!(report.has_rule(RuleId::TitleBlank));
        assert!(!report.has_rule(RuleId::TitleTooLong));
    }

    #[test]
    fn the_constructors_set_the_severity_they_name() {
        assert_eq!(blocking(RuleId::TitleBlank).severity, Severity::Blocking);
        assert_eq!(warning(RuleId::TagSplitOnComma).severity, Severity::Warning);
        assert_eq!(blocking(RuleId::TitleBlank).field, Field::Title);
        assert_eq!(blocking(RuleId::TitleBlank).message, "message");
        assert_eq!(blocking(RuleId::TitleBlank).remedy, "remedy");
    }
}
