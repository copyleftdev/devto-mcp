//! Detection of Jekyll front matter inside `body_markdown`.
//!
//! This is not a YAML parser and does not try to be. It exists for one reason: front
//! matter in the body **overrides the API payload**. `Article#evaluate_front_matter` runs
//! in `before_validation`, after the controller has assigned the params, and reassigns
//! `title`, `tags`, `published`, `published_at`, `main_image`, `canonical_url`,
//! `description` and the series from whatever the front matter says.
//!
//! So a caller that sends `tags: ["rust"]` alongside a body whose front matter says
//! `tags: webdev, beginners` gets the front matter's tags and no error. Detecting the
//! keys is enough to warn; parsing their values precisely is not required.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Front matter keys that displace an API field, and the field each one displaces.
pub const OVERRIDING_KEYS: [(&str, &str); 9] = [
    ("title", "title"),
    ("tags", "tags"),
    ("published", "published"),
    ("published_at", "published_at"),
    ("date", "published_at"),
    ("cover_image", "main_image"),
    ("canonical_url", "canonical_url"),
    ("description", "description"),
    ("series", "series"),
];

/// Keys that set the disclosure level when it is absent from the payload.
pub const DISCLOSURE_KEYS: [&str; 4] = [
    "ai_disclosure_level",
    "ai_disclosure",
    "ai_generated",
    "ai_assisted",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FrontMatter {
    pub present: bool,
    pub keys: BTreeMap<String, String>,
}

impl FrontMatter {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.keys.get(key).map(String::as_str)
    }

    pub fn has(&self, key: &str) -> bool {
        self.keys.contains_key(key)
    }
}

/// Extract the leading front matter block, if there is one.
///
/// A block must open with a line of exactly `---` at the very start of the body and close
/// with another such line. Anything else is ordinary markdown — a horizontal rule mid-post
/// is not front matter.
pub fn parse(body_markdown: &str) -> FrontMatter {
    let body = body_markdown.trim_start_matches('\u{feff}');
    let mut lines = body.lines();

    match lines.next() {
        Some(first) if first.trim_end() == "---" => {}
        _ => return FrontMatter::default(),
    }

    let mut keys = BTreeMap::new();
    for line in lines {
        if line.trim_end() == "---" {
            return FrontMatter {
                present: true,
                keys,
            };
        }
        if let Some((key, value)) = split_key_value(line) {
            keys.insert(key, value);
        }
    }

    // An unterminated block is not front matter to Forem's parser either.
    FrontMatter::default()
}

fn split_key_value(line: &str) -> Option<(String, String)> {
    if line.starts_with([' ', '\t', '-', '#']) {
        return None;
    }
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((key.to_lowercase(), unquote(value.trim()).to_string()))
}

fn unquote(s: &str) -> &str {
    let b = s.as_bytes();
    if b.len() >= 2 {
        let (first, last) = (b[0], b[b.len() - 1]);
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use hegel::Generator;

    #[test]
    fn reads_a_well_formed_block() {
        let fm = parse("---\ntitle: Hello\ntags: rust, zig\npublished: false\n---\n\nBody.");
        assert!(fm.present);
        assert_eq!(fm.get("title"), Some("Hello"));
        assert_eq!(fm.get("tags"), Some("rust, zig"));
        assert_eq!(fm.get("published"), Some("false"));
    }

    #[test]
    fn a_horizontal_rule_mid_body_is_not_front_matter() {
        assert!(!parse("Intro text\n\n---\n\nMore text").present);
    }

    #[test]
    fn an_unterminated_block_is_not_front_matter() {
        assert!(!parse("---\ntitle: Hello\n\nBody with no closing fence.").present);
    }

    #[test]
    fn strips_surrounding_quotes_from_values() {
        let fm = parse("---\ntitle: \"Quoted: with colon\"\n---\n");
        assert_eq!(fm.get("title"), Some("Quoted: with colon"));
    }

    #[test]
    fn ignores_list_items_and_comments() {
        let fm = parse("---\ntags:\n  - rust\n# a comment: here\ntitle: X\n---\n");
        assert_eq!(fm.get("title"), Some("X"));
        assert_eq!(fm.get("tags"), Some(""));
        assert!(!fm.has("# a comment"));
    }

    #[test]
    fn tolerates_a_byte_order_mark() {
        assert!(parse("\u{feff}---\ntitle: X\n---\n").present);
    }

    /// A line with nothing before the colon is not a key. Recording it would put an empty
    /// string in the key set and make every `has()` check unreliable.
    #[test]
    fn a_line_with_no_key_is_not_a_key() {
        let fm = parse("---\n: orphaned\ntitle: X\n---\n");
        assert_eq!(fm.keys.len(), 1);
        assert_eq!(fm.get("title"), Some("X"));
        assert!(!fm.has(""));
    }

    /// Keys are letters, digits and underscores. A line that merely contains a colon —
    /// prose, a URL, a time — is not a key/value pair.
    #[test]
    fn a_key_with_punctuation_or_spaces_is_not_a_key() {
        let fm = parse("---\nsome key: v\nsee https://example.com\ntitle: X\n---\n");
        assert_eq!(fm.get("title"), Some("X"));
        assert!(!fm.has("some key"));
        assert!(!fm.has("see https"));
        assert_eq!(fm.keys.len(), 1);
    }

    /// Only a matched pair comes off. One stray quote is part of the value.
    #[test]
    fn an_unmatched_quote_is_left_alone() {
        assert_eq!(
            parse("---\ntitle: \"half quoted\n---\n").get("title"),
            Some("\"half quoted")
        );
        assert_eq!(
            parse("---\ntitle: half quoted\"\n---\n").get("title"),
            Some("half quoted\"")
        );
        assert_eq!(
            parse("---\ntitle: 'mismatched\"\n---\n").get("title"),
            Some("'mismatched\"")
        );
        assert_eq!(
            parse("---\ntitle: half quoted'\n---\n").get("title"),
            Some("half quoted'")
        );
        assert_eq!(
            parse("---\ntitle: \"mismatched'\n---\n").get("title"),
            Some("\"mismatched'")
        );
    }

    #[test]
    fn a_later_line_wins_when_a_key_repeats() {
        assert_eq!(
            parse("---\ntitle: A\ntitle: B\n---\n").get("title"),
            Some("B")
        );
    }

    /// The body is whatever the caller wrote. Front matter detection must never panic on it.
    #[hegel::test]
    fn parse_never_panics(tc: hegel::TestCase) {
        let body = tc.draw(hegel::generators::text().max_size(300));
        let fm = parse(&body);
        for key in fm.keys.keys() {
            let _ = fm.get(key);
        }
    }

    /// Forem's parser only recognises a block that opens the document. A body that does not
    /// start with a `---` line has no front matter, whatever else it contains.
    #[hegel::test]
    fn a_body_not_opening_with_a_fence_has_no_front_matter(tc: hegel::TestCase) {
        let body = tc.draw(hegel::generators::text().max_size(300));
        let trimmed = body.trim_start_matches('\u{feff}');
        let opens_with_fence = trimmed
            .lines()
            .next()
            .is_some_and(|l| l.trim_end() == "---");
        if !opens_with_fence {
            assert!(
                !parse(&body).present,
                "wrongly detected front matter in {body:?}"
            );
        }
    }

    /// Build a block out of known keys and values, then read it back. This is the direction
    /// that matters: the validator warns based on which keys it found, so a missed key is a
    /// missed warning about front matter silently overriding the payload.
    #[hegel::test]
    fn recovers_the_keys_it_was_given(tc: hegel::TestCase) {
        let names = ["title", "tags", "published", "cover_image", "series"];
        let chosen: Vec<&str> = tc.draw(
            hegel::generators::vecs(hegel::generators::sampled_from(names.to_vec()))
                .unique(true)
                .max_size(names.len()),
        );
        let mut block = String::from("---\n");
        let mut expected: Vec<(String, String)> = Vec::new();
        for name in &chosen {
            let value = tc.draw(
                hegel::generators::text()
                    .max_size(20)
                    .map(|s: String| s.replace(['\n', '\r', ':'], "")),
            );
            let value = value.trim().to_string();
            block.push_str(&format!("{name}: {value}\n"));
            expected.push(((*name).to_string(), value));
        }
        block.push_str("---\n\nBody.\n");

        let fm = parse(&block);
        assert!(fm.present);
        for (name, value) in expected {
            assert_eq!(fm.get(&name), Some(unquote(&value)), "key {name}");
        }
    }
}
