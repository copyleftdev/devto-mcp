//! Forem's own reading time, reproduced exactly.
//!
//! ```ruby
//! # app/services/markdown_processor/parser.rb
//! WORDS_READ_PER_MINUTE = 275.0
//! @content = (content || "").gsub(/!\[Image Description\]/i, "![ ]")
//! word_count = @content.split(/\W+/).count
//! (word_count / WORDS_READ_PER_MINUTE).ceil
//! ```
//!
//! Verified against the 30 published articles in `tests/fixtures/forem_reading_time.json`.
//! Three details decide whether a reimplementation matches, and getting any one wrong scored
//! 25 of 30 rather than failing outright:
//!
//! - **Front matter is stripped first.** `ContentRenderer` parses it off before the parser
//!   ever sees the content. All five misses in the first attempt had front matter.
//! - **Ruby's `\w` is ASCII-only.** `naïve` counts as *two* words and `café` as one. This is
//!   the opposite of [`crate::counts`], where Python's Unicode `\w` keeps both whole.
//! - **Ruby's `String#split` keeps a leading empty field and drops trailing ones.** A body
//!   opening with `#` therefore counts one more word than it looks like it should.
//!
//! What the metric reveals is worth reporting on its own: it counts fenced code, liquid tags,
//! headings, list markers and URLs as prose. Measured with [`crate::segment`] across the 30
//! published articles, **23.8% of counted words are not prose**, **28 of 30** articles have an
//! overstated reading time, and the largest overstatement is **five minutes**.

use serde::Serialize;

/// `WORDS_READ_PER_MINUTE`.
pub const WORDS_PER_MINUTE: f64 = 275.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ReadingTime {
    /// What dev.to will display, in minutes.
    pub minutes: usize,
    /// The word count the figure is derived from, by Forem's own definition of a word.
    pub counted_words: usize,
}

/// Forem's reading time for an article body.
///
/// `body` is the raw `body_markdown`, front matter included — it is stripped here, because
/// that is where it happens upstream.
pub fn reading_time(body: &str) -> ReadingTime {
    let content = strip_front_matter(body);
    let content = replace_image_description(&content);
    let counted_words = ruby_word_count(&content);
    ReadingTime {
        minutes: (counted_words as f64 / WORDS_PER_MINUTE).ceil() as usize,
        counted_words,
    }
}

/// Remove a leading Jekyll front matter block, as `ContentRenderer` does before counting.
pub fn strip_front_matter(body: &str) -> String {
    let mut lines = body.split('\n');
    match lines.next() {
        Some(first) if first.trim_end() == "---" => {}
        _ => return body.to_string(),
    }
    let rest: Vec<&str> = lines.collect();
    for (i, line) in rest.iter().enumerate() {
        if line.trim_end() == "---" {
            return rest[i + 1..].join("\n");
        }
    }
    // An unterminated block is not front matter, so nothing is removed.
    body.to_string()
}

/// `gsub(/!\[Image Description\]/i, "![ ]")`. Present because Forem does it, and it changes
/// the count: three words become one.
///
/// The match positions are collected first and then applied, so the loop is a `for` over a
/// fixed list. A `while` advancing a cursor by `found + NEEDLE.len()` says the same thing
/// until a mutation turns the `+` into a `*`, and then it never advances at all.
fn replace_image_description(content: &str) -> String {
    const NEEDLE: &str = "![image description]";
    let needle = NEEDLE.as_bytes();

    // Searched over `content` itself rather than a lowercased copy. Lowercasing first and
    // slicing the original with the offsets it returns is wrong: `İ` is two bytes and
    // lowercases to three, so every offset past one is shifted and the output quietly gains
    // or loses a character. The needle is ASCII, so ASCII folding is the only folding that
    // can match, and a match can only begin on a char boundary.
    let starts: Vec<usize> = content
        .as_bytes()
        .windows(needle.len())
        .enumerate()
        .filter(|(_, window)| window.eq_ignore_ascii_case(needle))
        .map(|(at, _)| at)
        .collect();

    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    for start in starts {
        if start < cursor {
            continue; // An overlapping match, already inside one that was replaced.
        }
        out.push_str(&content[cursor..start]);
        out.push_str("![ ]");
        cursor = start + NEEDLE.len();
    }
    out.push_str(&content[cursor..]);
    out
}

/// `content.split(/\W+/).count` with Ruby's semantics.
///
/// Ruby's `\w` is `[A-Za-z0-9_]` — ASCII only, regardless of the string's encoding. `split`
/// keeps a leading empty field when the separator matches at position zero, and drops
/// trailing empty fields.
pub fn ruby_word_count(content: &str) -> usize {
    let is_word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut fields = 0usize;
    let mut pending_empty = 0usize;
    let mut in_word = false;
    let mut started = false;

    for c in content.chars() {
        if is_word(c) {
            if !in_word {
                // A separator run before the first word yields one empty leading field.
                fields += pending_empty;
                pending_empty = 0;
                fields += 1;
                in_word = true;
            }
            started = true;
        } else if in_word || !started {
            if in_word {
                in_word = false;
            } else if pending_empty == 0 {
                // Only the very first separator run produces a leading empty field.
                pending_empty = 1;
            }
        }
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leading_separator_produces_one_extra_field() {
        // Ruby: "# Hello".split(/\W+/) => ["", "Hello"]
        assert_eq!(ruby_word_count("# Hello"), 2);
        assert_eq!(ruby_word_count("Hello"), 1);
        assert_eq!(ruby_word_count("   Hello"), 2);
        assert_eq!(
            ruby_word_count("## ## Hello"),
            2,
            "one leading field, not three"
        );
    }

    #[test]
    fn trailing_separators_produce_nothing() {
        // Ruby drops trailing empty fields.
        assert_eq!(ruby_word_count("Hello."), 1);
        assert_eq!(ruby_word_count("Hello world!!!"), 2);
        assert_eq!(ruby_word_count("Hello   "), 1);
    }

    #[test]
    fn separators_alone_count_as_nothing() {
        assert_eq!(ruby_word_count(""), 0);
        assert_eq!(ruby_word_count("   "), 0);
        assert_eq!(ruby_word_count("---"), 0);
    }

    /// The detail that makes this metric different from every other count in this crate.
    #[test]
    fn ruby_treats_accented_letters_as_separators() {
        assert_eq!(ruby_word_count("naïve"), 2, "na + ve");
        assert_eq!(
            ruby_word_count("café"),
            1,
            "caf, with a trailing field dropped"
        );
        assert_eq!(ruby_word_count("日本語"), 0);
    }

    #[test]
    fn underscores_are_word_characters() {
        assert_eq!(ruby_word_count("snake_case_name"), 1);
        assert_eq!(ruby_word_count("kebab-case-name"), 3);
    }

    #[test]
    fn front_matter_is_removed_before_counting() {
        let body = "---\ntitle: A Post\npublished: true\n---\n\nJust four words here.";
        assert_eq!(strip_front_matter(body), "\nJust four words here.");
        assert_eq!(
            reading_time(body).counted_words,
            5,
            "one leading empty field"
        );
    }

    #[test]
    fn a_body_without_front_matter_is_untouched() {
        let body = "Just some words.";
        assert_eq!(strip_front_matter(body), body);
    }

    #[test]
    fn an_unterminated_block_is_not_front_matter() {
        let body = "---\ntitle: A Post\n\nNo closing fence here.";
        assert_eq!(strip_front_matter(body), body);
    }

    #[test]
    fn a_horizontal_rule_further_down_is_not_front_matter() {
        let body = "Intro.\n\n---\n\nMore.";
        assert_eq!(strip_front_matter(body), body);
    }

    #[test]
    fn the_image_description_substitution_changes_the_count() {
        // Ruby: ["", "Image", "Description", "x", "png"]
        assert_eq!(ruby_word_count("![Image Description](x.png)"), 5);
        assert_eq!(
            reading_time("![Image Description](x.png)").counted_words,
            3,
            "the three-word alt text becomes one space"
        );
        // The substitution is case-insensitive, as Forem's /i flag makes it.
        assert_eq!(
            reading_time("![IMAGE DESCRIPTION](x.png)").counted_words,
            reading_time("![image description](x.png)").counted_words
        );
    }

    /// Every earlier case put the substitution at position zero, where `cursor + found` is
    /// indistinguishable from a subtraction or a multiplication — the offset arithmetic was
    /// never actually exercised. Here the first match starts partway in and there are two.
    #[test]
    fn the_substitution_finds_matches_past_the_start_and_more_than_once() {
        let body = "Some intro prose ![Image Description](a.png) and more \
                    prose ![IMAGE description](b.png) and a close.";
        // Both alt texts collapse to one space each; nothing around them is disturbed.
        let expected =
            ruby_word_count("Some intro prose ![ ](a.png) and more prose ![ ](b.png) and a close.");
        assert_eq!(reading_time(body).counted_words, expected);
    }

    /// The haystack used to be a lowercased copy of the body, and the offsets it returned were
    /// used to slice the original. `İ` is two bytes and lowercases to three, so a body
    /// containing one shifted every offset after it and the substitution landed in the wrong
    /// place. Forem's own `gsub` has no such seam.
    #[test]
    fn a_case_changing_character_does_not_shift_the_substitution() {
        let dotted = "İ ![Image Description](alpha.png) beta gamma delta";
        assert_eq!(
            reading_time(dotted).counted_words,
            ruby_word_count("İ ![ ](alpha.png) beta gamma delta")
        );
        // The same body with an ASCII letter in place of the dotted capital, which is the
        // case that always worked.
        let plain = "A ![Image Description](alpha.png) beta gamma delta";
        assert_eq!(
            reading_time(plain).counted_words,
            ruby_word_count("A ![ ](alpha.png) beta gamma delta")
        );
    }

    #[test]
    fn minutes_round_up() {
        let words = |n: usize| "word ".repeat(n);
        assert_eq!(reading_time(&words(1)).minutes, 1);
        assert_eq!(reading_time(&words(275)).minutes, 1);
        assert_eq!(reading_time(&words(276)).minutes, 2);
        assert_eq!(reading_time(&words(550)).minutes, 2);
        assert_eq!(reading_time(&words(551)).minutes, 3);
        assert_eq!(reading_time("").minutes, 0);
    }

    #[hegel::test]
    fn counting_arbitrary_text_never_panics(tc: hegel::TestCase) {
        let body = tc.draw(hegel::generators::text().max_size(400));
        let result = reading_time(&body);
        let expected = (result.counted_words as f64 / WORDS_PER_MINUTE).ceil() as usize;
        assert_eq!(result.minutes, expected);
    }

    /// Ruby's count never exceeds the number of separator runs plus one, and a text made only
    /// of word characters is exactly one field.
    #[hegel::test]
    fn a_single_run_of_word_characters_is_one_field(tc: hegel::TestCase) {
        let word: String = tc
            .draw(
                hegel::generators::vecs(hegel::generators::sampled_from(
                    "abcXYZ019_".chars().collect::<Vec<_>>(),
                ))
                .min_size(1)
                .max_size(40),
            )
            .into_iter()
            .collect();
        assert_eq!(ruby_word_count(&word), 1, "{word:?}");
    }
}
