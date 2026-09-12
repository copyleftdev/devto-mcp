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
