//! Tag handling, mirroring what happens between the API payload and the database.
//!
//! `Articles::Creator` joins the `tags` array with `", "` into a single `tag_list` string,
//! which `ActsAsTaggableOn` then re-splits on commas, strips, unquotes, and — because
//! `ActsAsTaggableOn.force_lowercase = true` in Forem — downcases.
//!
//! The round trip is not identity. `["rust,zig"]` arrives as one array entry and lands as
//! two tags, which can silently push a payload past the four-tag limit.

use serde::{Deserialize, Serialize};

use crate::limits::{TAG_LIST_MAX_CHARS, TAG_MAX_CHARS};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedTag {
    /// Index into the caller's `tags` array that produced this tag.
    pub source_index: usize,
    /// The text as it appeared inside that array entry, before stripping and downcasing.
    pub raw: String,
    /// The tag as Forem will store it.
    pub value: String,
}

/// Apply Forem's parse-and-normalize pipeline to a `tags` array.
pub fn normalize(tags: &[String]) -> Vec<NormalizedTag> {
    let mut out = Vec::new();
    for (source_index, entry) in tags.iter().enumerate() {
        for piece in entry.split(',') {
            let raw = piece.trim();
            if raw.is_empty() {
                continue;
            }
            let unquoted = strip_matching_quotes(raw);
            if unquoted.is_empty() {
                continue;
            }
            out.push(NormalizedTag {
                source_index,
                raw: raw.to_string(),
                value: unquoted.to_lowercase(),
            });
        }
    }
    out
}

/// `ActsAsTaggableOn::DefaultParser` removes one matched pair of surrounding quotes.
fn strip_matching_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// `Tag#validate_name`: 1–30 characters, every one of them alphanumeric.
///
/// Forem's regex is Ruby's `/\A[[:alnum:]]+\z/i`, which is Unicode-aware — `español` is a
/// live tag on dev.to. Hyphens, underscores, dots and spaces are all rejected, which is
/// why `machine-learning` is not a valid tag and `machinelearning` is.
pub fn invalid_characters(tag: &str) -> Vec<char> {
    tag.chars().filter(|c| !c.is_alphanumeric()).collect()
}

pub fn is_too_long(tag: &str) -> bool {
    tag.chars().count() > TAG_MAX_CHARS
}

/// The `cached_tag_list` column Forem length-validates: the normalized tags, comma-joined.
pub fn cached_tag_list(tags: &[NormalizedTag]) -> String {
    tags.iter()
        .map(|t| t.value.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn tag_list_too_long(tags: &[NormalizedTag]) -> bool {
    cached_tag_list(tags).chars().count() > TAG_LIST_MAX_CHARS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn splits_a_comma_bearing_entry_into_several_tags() {
        let tags = normalize(&v(&["rust,zig"]));
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].value, "rust");
        assert_eq!(tags[1].value, "zig");
        assert_eq!(tags[1].source_index, 0);
    }

    #[test]
    fn downcases_and_trims() {
        let tags = normalize(&v(&["  RuSt  "]));
        assert_eq!(tags[0].value, "rust");
        assert_eq!(tags[0].raw, "RuSt");
    }

    #[test]
    fn strips_one_matched_pair_of_quotes() {
        assert_eq!(normalize(&v(&["\"rust\""]))[0].value, "rust");
        assert_eq!(normalize(&v(&["'rust'"]))[0].value, "rust");
        assert_eq!(normalize(&v(&["\"rust"]))[0].value, "\"rust");
    }

    #[test]
    fn drops_empty_pieces() {
        assert!(normalize(&v(&["", "  ", ","])).is_empty());
    }

    #[test]
    fn hyphens_are_invalid_but_diacritics_are_not() {
        assert_eq!(invalid_characters("machine-learning"), vec!['-']);
        assert!(invalid_characters("español").is_empty());
        assert!(invalid_characters("rust2024").is_empty());
    }

    #[test]
    fn cached_list_joins_with_comma_space() {
        let tags = normalize(&v(&["rust", "zig"]));
        assert_eq!(cached_tag_list(&tags), "rust, zig");
    }

    fn any_tag_input(tc: &hegel::TestCase) -> Vec<String> {
        tc.draw(hegel::generators::vecs(hegel::generators::text().max_size(40)).max_size(8))
    }

    /// Normalizing is trim, unquote and downcase. Running it on its own output must be a
    /// no-op, or a caller that echoes stored tags back to us gets a different result.
    #[hegel::test]
    fn normalize_is_idempotent(tc: hegel::TestCase) {
        let once: Vec<String> = normalize(&any_tag_input(&tc))
            .into_iter()
            .map(|t| t.value)
            .collect();
        let twice: Vec<String> = normalize(&once).into_iter().map(|t| t.value).collect();
        assert_eq!(once, twice);
    }

    /// Whatever went in, what comes out is a storable tag: non-empty, trimmed, and free of
    /// the delimiter Forem splits on.
    #[hegel::test]
    fn every_normalized_tag_is_storable(tc: hegel::TestCase) {
        for tag in normalize(&any_tag_input(&tc)) {
            assert!(!tag.value.is_empty());
            assert!(!tag.value.contains(','));
            assert_eq!(tag.value.trim(), tag.value);
            assert_eq!(tag.value.to_lowercase(), tag.value);
        }
    }

    /// `source_index` is how the validator points at the offending array entry, so it has
    /// to stay a valid index into what the caller sent.
    #[hegel::test]
    fn source_index_always_addresses_the_input(tc: hegel::TestCase) {
        let input = any_tag_input(&tc);
        for tag in normalize(&input) {
            assert!(tag.source_index < input.len());
            assert!(input[tag.source_index].to_lowercase().contains(&tag.value));
        }
    }

    /// `cached_tag_list` is what Forem length-validates, and the column is the tags joined
    /// with ", ". Splitting it back apart must recover exactly the tags that went in —
    /// which holds precisely because no normalized tag can contain a comma.
    #[hegel::test]
    fn cached_tag_list_splits_back_into_its_tags(tc: hegel::TestCase) {
        let tags = normalize(&any_tag_input(&tc));
        if tags.is_empty() {
            assert_eq!(cached_tag_list(&tags), "");
            return;
        }
        let joined = cached_tag_list(&tags);
        let recovered: Vec<&str> = joined.split(", ").collect();
        let expected: Vec<&str> = tags.iter().map(|t| t.value.as_str()).collect();
        assert_eq!(recovered, expected);
    }

    /// Nothing in the pipeline slices on a byte index that could land mid-character.
    #[hegel::test]
    fn survives_arbitrary_text(tc: hegel::TestCase) {
        for tag in normalize(&any_tag_input(&tc)) {
            let _ = invalid_characters(&tag.value);
            let _ = is_too_long(&tag.value);
        }
    }
}
