//! Numeric limits mirrored from Forem. Each carries the source that sets it, because
//! the published API description states none of them.

/// `validates :body_markdown, bytesize: { maximum: 800.kilobytes }` — bytes, not characters.
pub const BODY_MAX_BYTES: usize = 800 * 1024;

/// `Article#title_length_based_on_type_of`, measured after every whitespace character is removed.
pub const TITLE_MAX_CHARS_FULL_POST: usize = 128;
pub const TITLE_MAX_CHARS_STATUS: usize = 256;

/// `Article::MAX_TAG_LIST_SIZE`.
pub const MAX_TAGS: usize = 4;

/// `Tag#validate_name`.
pub const TAG_MAX_CHARS: usize = 30;

/// `validates :cached_tag_list, length: { maximum: 126 }` — the comma-joined list.
pub const TAG_LIST_MAX_CHARS: usize = 126;

/// `Article#future_or_current_published_at` allows a published_at up to 15 minutes in the past.
pub const PUBLISHED_AT_PAST_GRACE_SECS: i64 = 15 * 60;

/// `Article#title_unique_for_user_past_five_minutes`.
pub const DUPLICATE_TITLE_WINDOW_SECS: i64 = 5 * 60;

/// Hosts Forem treats as local and rejects for canonical and feed URLs.
pub const LOCAL_HOSTS: [&str; 4] = ["localhost", "127.0.0.1", "0.0.0.0", "::1"];

#[cfg(test)]
mod tests {
    use super::*;

    /// These are facts about Forem, not arithmetic. Pin every value to its literal so a
    /// slip in the expression shows up as a failing test rather than a silently wrong limit
    /// that only surfaces as a 422 against the live API.
    #[test]
    fn every_limit_matches_forem() {
        assert_eq!(BODY_MAX_BYTES, 819_200, "800.kilobytes");
        assert_eq!(TITLE_MAX_CHARS_FULL_POST, 128);
        assert_eq!(TITLE_MAX_CHARS_STATUS, 256);
        assert_eq!(MAX_TAGS, 4);
        assert_eq!(TAG_MAX_CHARS, 30);
        assert_eq!(TAG_LIST_MAX_CHARS, 126);
        assert_eq!(PUBLISHED_AT_PAST_GRACE_SECS, 900, "15.minutes");
        assert_eq!(DUPLICATE_TITLE_WINDOW_SECS, 300, "5.minutes");
        assert_eq!(LOCAL_HOSTS.len(), 4);
    }

    /// Four tags at the maximum length land exactly on the stored-list limit. That is not a
    /// coincidence worth relying on, but it is worth knowing: the list cap can only bite
    /// when tags are long, never when there are merely four of them.
    #[test]
    fn four_maximum_length_tags_sit_exactly_on_the_list_limit() {
        let joined = MAX_TAGS * TAG_MAX_CHARS + (MAX_TAGS - 1) * ", ".len();
        assert_eq!(joined, TAG_LIST_MAX_CHARS);
    }
}
