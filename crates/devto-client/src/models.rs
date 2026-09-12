//! Response types, shaped from live dev.to responses captured 2026-09-12 rather than from
//! the OpenAPI description, which disagrees with the wire in two places.
//!
//! Every type ignores unknown fields, so Forem adding one does not break a caller.

use serde::{Deserialize, Serialize};

/// The authenticated account.
///
/// `/api/users/me` also returns the account's `email`. It is deliberately not deserialized:
/// nothing above this layer has a reason to see it, and a field that does not exist cannot
/// be handed to a model or written into a log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Me {
    pub id: i64,
    pub username: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub twitter_username: Option<String>,
    #[serde(default)]
    pub github_username: Option<String>,
    #[serde(default)]
    pub website_url: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub joined_at: String,
    #[serde(default)]
    pub profile_image: String,
    #[serde(default)]
    pub followers_count: Option<i64>,
}

/// An article owned by the authenticated user, from `/api/articles/me/*`.
///
/// The field names here are the jbuilder view's, not the database column names the source
/// lists in `ME_ATTRIBUTES_FOR_SERIALIZATION`: the wire says `cover_image`, `tag_list` and
/// `reading_time_minutes` where the columns are `main_image`, `cached_tag_list` and
/// `reading_time`. This is the only endpoint that returns unpublished work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyArticle {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub published: bool,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub cover_image: Option<String>,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub tag_list: Vec<String>,
    #[serde(default)]
    pub body_markdown: Option<String>,
    #[serde(default)]
    pub comments_count: i64,
    #[serde(default)]
    pub public_reactions_count: i64,
    #[serde(default)]
    pub page_views_count: i64,
    #[serde(default)]
    pub reading_time_minutes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRef {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub profile_image: Option<String>,
}

/// An article in a public listing or search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleSummary {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub cover_image: Option<String>,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub tag_list: Vec<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub comments_count: i64,
    #[serde(default)]
    pub public_reactions_count: i64,
    #[serde(default)]
    pub reading_time_minutes: Option<i64>,
    #[serde(default)]
    pub ai_disclosure_level: Option<String>,
    #[serde(default)]
    pub user: Option<UserRef>,
}

/// A full article, which adds the markdown source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleDetail {
    #[serde(flatten)]
    pub summary: ArticleSummary,
    #[serde(default)]
    pub body_markdown: Option<String>,
}

/// A semantic search hit.
///
/// This endpoint serializes with `article.as_json(only: ...)` straight from the model
/// rather than through a jbuilder view, so it returns **database column names** — no other
/// endpoint on the API does. `main_image` not `cover_image`, `cached_tag_list` (a comma
/// string, not an array) not `tag_list`, `reading_time` not `reading_time_minutes`.
///
/// `similarity` is `1.0 - cosine_distance`, and the results are **not sorted by it**: the
/// controller fuses keyword and vector rankings with Reciprocal Rank Fusion and then
/// applies recency and quality boosts. Re-sorting by similarity discards that ranking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticHit {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub main_image: Option<String>,
    #[serde(default)]
    pub cached_tag_list: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub reading_time: Option<i64>,
    #[serde(default)]
    pub public_reactions_count: i64,
    #[serde(default)]
    pub comments_count: i64,
    #[serde(default)]
    pub ai_disclosure_level: Option<String>,
    #[serde(default)]
    pub distance: Option<f64>,
    #[serde(default)]
    pub similarity: Option<f64>,
}

impl SemanticHit {
    /// The comma string this endpoint returns, split into the tag list every other
    /// endpoint would have given you.
    pub fn tags(&self) -> Vec<String> {
        self.cached_tag_list
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub bg_color_hex: Option<String>,
    #[serde(default)]
    pub text_color_hex: Option<String>,
}

/// A tag the authenticated user follows. `points` is the personal weighting Forem uses to
/// rank that tag in their feed — a negative value means the tag is down-weighted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowedTag {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub points: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id_code: String,
    #[serde(default)]
    pub body_html: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub user: Option<UserRef>,
    #[serde(default)]
    pub children: Vec<Comment>,
    #[serde(default)]
    pub ai_disclosure_level: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct CountTotal {
    #[serde(default)]
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PageViewTotals {
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub average_read_time_in_seconds: i64,
    #[serde(default)]
    pub total_read_time_in_seconds: i64,
}

/// Reaction counts. Forem returns one key per reaction category plus `total` and
/// `unique_reactors`, and the category set is instance-configurable, so the named fields
/// are the ones dev.to actually ships and the rest are kept as-is.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReactionTotals {
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub like: i64,
    #[serde(default)]
    pub unicorn: i64,
    #[serde(default)]
    pub readinglist: i64,
    #[serde(default)]
    pub unique_reactors: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalyticsTotals {
    #[serde(default)]
    pub comments: CountTotal,
    #[serde(default)]
    pub follows: CountTotal,
    #[serde(default)]
    pub reactions: ReactionTotals,
    #[serde(default)]
    pub page_views: PageViewTotals,
}

/// The bundled analytics payload.
///
/// Forem built this endpoint so a client could get five panels in one request instead of
/// tripping the 3-reads-per-second throttle. The nested panels keep their raw shape: they
/// are date-keyed maps and ranked lists whose structure varies, and reshaping them here
/// would only invent a contract Forem has not promised.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dashboard {
    #[serde(default)]
    pub totals: AnalyticsTotals,
    #[serde(default)]
    pub historical: serde_json::Value,
    #[serde(default)]
    pub referrers: serde_json::Value,
    #[serde(default)]
    pub top_contributors: serde_json::Value,
    #[serde(default)]
    pub follower_engagement: serde_json::Value,
    /// The earliest date with data — the account's registration, not an arbitrary epoch.
    #[serde(default)]
    pub start_date_floor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_fields_do_not_break_deserialization() {
        let json = r#"{"id":1,"username":"x","brand_new_field":true}"#;
        let me: Me = serde_json::from_str(json).unwrap();
        assert_eq!(me.id, 1);
        assert_eq!(me.username, "x");
    }

    /// The email is on the wire and must not survive the trip into this process.
    #[test]
    fn the_account_email_is_not_deserialized() {
        let json = r#"{"id":1,"username":"x","email":"someone@example.com"}"#;
        let me: Me = serde_json::from_str(json).unwrap();
        let round_tripped = serde_json::to_string(&me).unwrap();
        assert!(
            !round_tripped.contains("example.com"),
            "the email leaked back out: {round_tripped}"
        );
        assert!(!round_tripped.contains("email"));
    }

    #[test]
    fn a_semantic_hit_splits_its_comma_joined_tags() {
        let hit: SemanticHit = serde_json::from_str(
            r#"{"id":1,"title":"t","cached_tag_list":"rust, webdev , ","similarity":0.78}"#,
        )
        .unwrap();
        assert_eq!(hit.tags(), vec!["rust", "webdev"]);
        assert_eq!(hit.similarity, Some(0.78));
    }

    #[test]
    fn a_semantic_hit_with_no_tags_yields_an_empty_list() {
        let hit: SemanticHit = serde_json::from_str(r#"{"id":1,"title":"t"}"#).unwrap();
        assert!(hit.tags().is_empty());
        assert!(hit.similarity.is_none());
    }

    /// Verified live on 2026-09-12: the view rendered after a write returns `tag_list` as a
    /// comma-joined string, while every listing endpoint returns the same field as an
    /// array. A client that assumes one shape fails on the other.
    #[test]
    fn a_write_response_accepts_either_tag_list_shape() {
        let joined: WrittenArticle =
            serde_json::from_str(r#"{"id":1,"tag_list":"rust, webdev"}"#).unwrap();
        assert_eq!(joined.tag_list, vec!["rust", "webdev"]);

        let array: WrittenArticle =
            serde_json::from_str(r#"{"id":1,"tag_list":["rust","webdev"]}"#).unwrap();
        assert_eq!(array.tag_list, vec!["rust", "webdev"]);

        let single: WrittenArticle =
            serde_json::from_str(r#"{"id":1,"tag_list":"testing"}"#).unwrap();
        assert_eq!(single.tag_list, vec!["testing"]);

        for empty in [
            r#"{"id":1,"tag_list":""}"#,
            r#"{"id":1,"tag_list":[]}"#,
            r#"{"id":1,"tag_list":null}"#,
            r#"{"id":1}"#,
        ] {
            let parsed: WrittenArticle = serde_json::from_str(empty).unwrap();
            assert!(parsed.tag_list.is_empty(), "{empty}");
        }
    }

    /// The exact body dev.to replied with when the write path was first exercised for real.
    #[test]
    fn the_live_create_response_decodes() {
        let live = r#"{"type_of":"article","id":4640644,
            "title":"devto-mcp transport check (unpublished test artifact)",
            "description":"Transport verification for devto-mcp.","published":false,
            "published_at":null,"slug":"devto-mcp-transport-check-4a1b",
            "path":"/copyleftdev/devto-mcp-transport-check-4a1b",
            "url":"https://dev.to/copyleftdev/devto-mcp-transport-check-4a1b",
            "canonical_url":null,"tag_list":"testing",
            "ai_disclosure_level":"fully_autonomous"}"#;
        let parsed: WrittenArticle = serde_json::from_str(live).unwrap();
        assert_eq!(parsed.id, 4640644);
        assert!(!parsed.published);
        assert_eq!(parsed.tag_list, vec!["testing"]);
        assert_eq!(
            parsed.ai_disclosure_level.as_deref(),
            Some("fully_autonomous")
        );
    }

    /// Shaped from the live payload captured 2026-09-12.
    #[test]
    fn the_dashboard_totals_parse_as_forem_sends_them() {
        let json = r#"{
            "totals": {
                "comments": {"total": 165},
                "follows": {"total": 14227},
                "reactions": {"total": 670, "like": 359, "readinglist": 106,
                              "unicorn": 57, "exploding_head": 51, "unique_reactors": 281},
                "page_views": {"total": 29990, "average_read_time_in_seconds": 203,
                               "total_read_time_in_seconds": 6087970}
            },
            "historical": {},
            "referrers": {"domains": [{"domain": null, "count": 14473}]},
            "top_contributors": {},
            "follower_engagement": {},
            "start_date_floor": "2022-11-04"
        }"#;
        let dashboard: Dashboard = serde_json::from_str(json).unwrap();
        assert_eq!(dashboard.totals.page_views.total, 29990);
        assert_eq!(dashboard.totals.reactions.like, 359);
        assert_eq!(dashboard.totals.reactions.unique_reactors, 281);
        assert_eq!(dashboard.totals.comments.total, 165);
        assert_eq!(dashboard.start_date_floor.as_deref(), Some("2022-11-04"));
        assert!(dashboard.referrers.get("domains").is_some());
    }

    #[test]
    fn an_empty_dashboard_still_parses() {
        let dashboard: Dashboard = serde_json::from_str("{}").unwrap();
        assert_eq!(dashboard.totals.page_views.total, 0);
        assert!(dashboard.start_date_floor.is_none());
    }

    /// Shaped from the live payload: an unpublished draft has no published_at and no
    /// reading time, and every counter is zero.
    #[test]
    fn an_unpublished_draft_parses_with_its_nulls() {
        let json = r#"{"id":3245097,"title":"A draft","description":"","published":false,
                       "published_at":null,"page_views_count":0,"public_reactions_count":0,
                       "comments_count":0,"reading_time_minutes":null,"tag_list":[]}"#;
        let article: MyArticle = serde_json::from_str(json).unwrap();
        assert!(!article.published);
        assert!(article.published_at.is_none());
        assert!(article.reading_time_minutes.is_none());
        assert!(article.tag_list.is_empty());
    }
}

/// What to send when creating or updating an article.
///
/// Every field is optional because an update is a partial write: what is omitted is left
/// alone. `None` fields are not serialized at all — sending `null` would clear the value
/// rather than leave it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ArticlePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_markdown: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    /// The API field. Front matter spells the same concept `cover_image`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_disclosure_level: Option<String>,
    /// RFC 3339. Absent from the published OpenAPI description, but accepted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_source_url: Option<String>,
}

impl ArticlePayload {
    /// Forem expects the fields nested under an `article` key.
    pub fn to_request_body(&self) -> String {
        serde_json::json!({ "article": self }).to_string()
    }
}

/// Accept a tag list however the endpoint chose to send it.
///
/// The listing endpoints return `tag_list` as an array. The view rendered after a create
/// or update returns the same field as a **comma-joined string**. Verified live on
/// 2026-09-12: `POST /api/articles` replied with `"tag_list":"testing"` for an article
/// that `GET /api/articles/me/unpublished` then reported as `["testing"]`.
fn tag_list_either_shape<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        List(Vec<String>),
        Joined(String),
    }

    Ok(match Option::<Either>::deserialize(deserializer)? {
        Some(Either::List(tags)) => tags,
        Some(Either::Joined(text)) => text
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        None => Vec::new(),
    })
}

/// What comes back from a create or update.
///
/// A different view from the listing endpoints, so everything is defaulted: the useful
/// parts are the id and the URL, and a missing extra must not fail the whole write.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WrittenArticle {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub published: bool,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default, deserialize_with = "tag_list_either_shape")]
    pub tag_list: Vec<String>,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub ai_disclosure_level: Option<String>,
}
