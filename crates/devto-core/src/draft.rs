use serde::{Deserialize, Serialize};

/// Seconds since the Unix epoch. The core never reads a clock; callers supply the time.
pub type UnixSeconds = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArticleType {
    #[default]
    FullPost,
    Status,
    FullscreenEmbed,
}

/// The disclosure ladder, with the enum values Forem stores.
///
/// Definitions are dev.to's own, from <https://dev.to/llms.txt>; they are reproduced on
/// [`AiDisclosure::definition`] so a tool surface can quote them rather than paraphrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AiDisclosure {
    #[default]
    NotDisclosed,
    NoAi,
    SomeAi,
    FullyAutonomous,
}

impl AiDisclosure {
    pub fn wire_value(self) -> &'static str {
        match self {
            Self::NotDisclosed => "not_disclosed",
            Self::NoAi => "no_ai",
            Self::SomeAi => "some_ai",
            Self::FullyAutonomous => "fully_autonomous",
        }
    }

    pub fn stored_enum(self) -> u8 {
        match self {
            Self::NotDisclosed => 0,
            Self::NoAi => 1,
            Self::SomeAi => 3,
            Self::FullyAutonomous => 5,
        }
    }

    pub fn definition(self) -> &'static str {
        match self {
            Self::NotDisclosed => {
                "No disclosure was provided. This is what omitting the field records."
            }
            Self::NoAi => {
                "Written by a human without meaningful assistance from AI generation tools."
            }
            Self::SomeAi => {
                "Human-authored with meaningful AI assistance, including drafting, code generation, \
                 major editing, or translation."
            }
            Self::FullyAutonomous => {
                "Produced primarily or entirely by an agent or language model, even when a human \
                 requested or approved it."
            }
        }
    }

    /// Forem accepts a wide set of aliases when the level arrives through front matter.
    /// Mirrors `Article#set_ai_disclosure_from_front_matter`.
    pub fn from_front_matter_value(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_lowercase().replace('-', "_");
        match normalized.as_str() {
            "0" | "not_disclosed" | "unstated" | "unknown" | "false" => Some(Self::NotDisclosed),
            "1" | "no_ai" | "no" | "none" | "human" | "hand_written" | "handwritten"
            | "100%_human" => Some(Self::NoAi),
            "3" | "some_ai" | "some" | "assisted" | "ai_assisted" | "ai_assist" => {
                Some(Self::SomeAi)
            }
            "5" | "fully_autonomous" | "autonomous" | "full" | "ai_generated" | "generated"
            | "fully_ai" => Some(Self::FullyAutonomous),
            _ => None,
        }
    }
}

/// An article payload as it would be sent to `POST /api/articles` or `PUT /api/articles/{id}`.
///
/// Field names match the API, not the front matter, which spells the cover image
/// `cover_image` rather than `main_image`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Draft {
    pub title: String,
    pub body_markdown: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub series: Option<String>,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub main_image: Option<String>,
    #[serde(default)]
    pub video_source_url: Option<String>,
    #[serde(default)]
    pub organization_id: Option<i64>,
    #[serde(default)]
    pub subforem_id: Option<i64>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub published: bool,
    #[serde(default)]
    pub published_at: Option<UnixSeconds>,
    #[serde(default)]
    pub ai_disclosure_level: AiDisclosure,
    #[serde(default)]
    pub article_type: ArticleType,
}

impl Draft {
    pub fn new(title: impl Into<String>, body_markdown: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body_markdown: body_markdown.into(),
            ..Self::default()
        }
    }

    pub fn title_max_chars(&self) -> usize {
        match self.article_type {
            ArticleType::Status => crate::limits::TITLE_MAX_CHARS_STATUS,
            ArticleType::FullPost | ArticleType::FullscreenEmbed => {
                crate::limits::TITLE_MAX_CHARS_FULL_POST
            }
        }
    }
}

/// Whether this payload creates a new article or edits an existing one. The rules differ
/// sharply: `published_at` is freely settable on a draft and frozen once published, and
/// `Articles::Updater` drops it silently rather than erroring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Operation {
    Create,
    Update {
        /// The article has a `published_at` in the past — publication already happened.
        already_published: bool,
        /// The article is scheduled: it has a future `published_at` and can still be moved.
        scheduled: bool,
        /// The article's cover image was once set from front matter, which makes it
        /// permanently front-matter-driven (`Article#set_main_image`).
        main_image_from_frontmatter: bool,
    },
}

/// A title the author posted recently, used to catch the five-minute duplicate rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentTitle {
    pub title: String,
    pub created_at: UnixSeconds,
}

/// Everything the validator needs that isn't in the payload itself. All of it is
/// optional: an empty context still catches every rule that depends only on the draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub operation: Operation,
    /// Canonical URLs already used by the author's published articles. Forem enforces
    /// uniqueness across published articles, so a collision is a guaranteed 422.
    #[serde(default)]
    pub known_canonical_urls: Vec<String>,
    #[serde(default)]
    pub recent_titles: Vec<RecentTitle>,
    /// `fullscreen_embed` is admin-only, and admins may set scores and labels.
    #[serde(default)]
    pub author_is_admin: bool,
}

impl Context {
    pub fn create() -> Self {
        Self {
            operation: Operation::Create,
            known_canonical_urls: Vec::new(),
            recent_titles: Vec::new(),
            author_is_admin: false,
        }
    }

    pub fn update(already_published: bool) -> Self {
        Self {
            operation: Operation::Update {
                already_published,
                scheduled: false,
                main_image_from_frontmatter: false,
            },
            ..Self::create()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::{TITLE_MAX_CHARS_FULL_POST, TITLE_MAX_CHARS_STATUS};

    const ALL_LEVELS: [AiDisclosure; 4] = [
        AiDisclosure::NotDisclosed,
        AiDisclosure::NoAi,
        AiDisclosure::SomeAi,
        AiDisclosure::FullyAutonomous,
    ];

    /// The definitions are quoted so a tool surface can show a caller dev.to's own wording
    /// rather than a paraphrase of it. Pin the load-bearing phrases from
    /// <https://dev.to/llms.txt>; a reworded definition is a changed obligation.
    #[test]
    fn each_level_carries_dev_tos_own_definition() {
        assert!(
            AiDisclosure::NoAi
                .definition()
                .contains("without meaningful assistance")
        );
        assert!(
            AiDisclosure::SomeAi
                .definition()
                .contains("meaningful AI assistance")
        );
        assert!(
            AiDisclosure::FullyAutonomous
                .definition()
                .contains("even when a human requested or approved it")
        );
        assert!(
            AiDisclosure::NotDisclosed
                .definition()
                .contains("omitting the field")
        );

        let mut seen = std::collections::BTreeSet::new();
        for level in ALL_LEVELS {
            assert!(!level.definition().is_empty());
            assert!(
                seen.insert(level.definition()),
                "{level:?} shares a definition"
            );
        }
    }

    /// Forem stores the level as a sparse enum — 0, 1, 3, 5 — not a dense one.
    #[test]
    fn the_stored_enum_values_match_forem() {
        assert_eq!(AiDisclosure::NotDisclosed.stored_enum(), 0);
        assert_eq!(AiDisclosure::NoAi.stored_enum(), 1);
        assert_eq!(AiDisclosure::SomeAi.stored_enum(), 3);
        assert_eq!(AiDisclosure::FullyAutonomous.stored_enum(), 5);
    }

    #[test]
    fn wire_values_are_distinct_and_match_the_serialized_form() {
        let mut seen = std::collections::BTreeSet::new();
        for level in ALL_LEVELS {
            assert!(seen.insert(level.wire_value()));
            assert_eq!(
                serde_json::to_string(&level).unwrap(),
                format!("\"{}\"", level.wire_value())
            );
        }
    }

    /// Forem accepts a generous alias set in front matter, and rejects anything outside it
    /// rather than guessing.
    #[test]
    fn front_matter_aliases_resolve_the_way_forem_resolves_them() {
        for alias in ["no", "none", "human", "hand-written", "100%_human", "1"] {
            assert_eq!(
                AiDisclosure::from_front_matter_value(alias),
                Some(AiDisclosure::NoAi),
                "{alias}"
            );
        }
        for alias in ["some", "assisted", "ai_assist", "3"] {
            assert_eq!(
                AiDisclosure::from_front_matter_value(alias),
                Some(AiDisclosure::SomeAi),
                "{alias}"
            );
        }
        for alias in ["autonomous", "generated", "fully_ai", "5"] {
            assert_eq!(
                AiDisclosure::from_front_matter_value(alias),
                Some(AiDisclosure::FullyAutonomous),
                "{alias}"
            );
        }
        assert_eq!(
            AiDisclosure::from_front_matter_value("  UNSTATED  "),
            Some(AiDisclosure::NotDisclosed)
        );
        assert_eq!(AiDisclosure::from_front_matter_value("maybe"), None);
        assert_eq!(AiDisclosure::from_front_matter_value(""), None);
    }

    /// Status posts get a longer title because the title is the post.
    #[test]
    fn the_title_limit_depends_on_the_post_type() {
        let mut d = Draft::new("t", "b");
        assert_eq!(d.title_max_chars(), TITLE_MAX_CHARS_FULL_POST);
        d.article_type = ArticleType::Status;
        assert_eq!(d.title_max_chars(), TITLE_MAX_CHARS_STATUS);
        d.article_type = ArticleType::FullscreenEmbed;
        assert_eq!(d.title_max_chars(), TITLE_MAX_CHARS_FULL_POST);
    }

    #[test]
    fn a_new_draft_is_unpublished_and_undisclosed() {
        let d = Draft::new("Title", "Body");
        assert_eq!(d.title, "Title");
        assert_eq!(d.body_markdown, "Body");
        assert!(!d.published);
        assert_eq!(d.ai_disclosure_level, AiDisclosure::NotDisclosed);
        assert_eq!(d.article_type, ArticleType::FullPost);
        assert!(d.tags.is_empty());
    }

    #[test]
    fn the_context_constructors_set_the_operation_they_name() {
        assert_eq!(Context::create().operation, Operation::Create);
        assert!(matches!(
            Context::update(true).operation,
            Operation::Update {
                already_published: true,
                ..
            }
        ));
        assert!(matches!(
            Context::update(false).operation,
            Operation::Update {
                already_published: false,
                ..
            }
        ));
    }
}
