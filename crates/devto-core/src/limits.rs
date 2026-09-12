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
