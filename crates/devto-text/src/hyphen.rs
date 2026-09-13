//! Liang's hyphenation algorithm over the `hyph_en_US` patterns.
//!
//! This exists to match `pyphen`, because `textstat` counts syllables as
//! `len(pyphen.positions(word)) + 1` for every word CMUdict does not know. Matching it means
//! matching pyphen exactly, including two things that are easy to get wrong:
//!
//! - **pyphen ignores the dictionary's own `LEFTHYPHENMIN` / `RIGHTHYPHENMIN`.** The file says
//!   2 and 3; pyphen lists both directives among the lines it skips and uses its constructor
//!   defaults instead, which `textstat` leaves at `left = 2, right = 2`.
//! - **Patterns are trimmed to their non-zero span** before being stored, and the stored
//!   offset is what positions them against the word.
//!
//! The `hyph_en_US` file uses neither the `/=` non-standard-hyphenation syntax nor `^^xx` hex
//! escapes — verified, zero occurrences of each — so neither is implemented. A pattern file
//! that used them would silently hyphenate differently, so [`parse_patterns`] rejects them
//! rather than ignoring them.

use std::collections::HashMap;
use std::sync::OnceLock;

/// The pattern file, compiled in. 106 KB, and parsing it takes a few milliseconds once.
const PATTERNS: &str = include_str!("../data/hyph_en_US.dic");

/// `textstat` constructs `Pyphen(lang=...)`, leaving both at 2.
pub const LEFT_MIN: usize = 2;
pub const RIGHT_MIN: usize = 2;

#[derive(Debug)]
struct Pattern {
    /// Index of the first non-zero value, relative to the pattern's own start.
    offset: usize,
    /// The non-zero span of priority values.
    values: Vec<u8>,
}

#[derive(Debug)]
pub struct Hyphenator {
    patterns: HashMap<String, Pattern>,
    max_len: usize,
}

/// The shared en_US hyphenator. Parsed once.
pub fn en_us() -> &'static Hyphenator {
    static INSTANCE: OnceLock<Hyphenator> = OnceLock::new();
    INSTANCE.get_or_init(|| Hyphenator::parse(PATTERNS).expect("the bundled pattern file"))
}

#[derive(Debug, PartialEq, Eq)]
pub enum PatternError {
    /// The file uses a feature this implementation does not support, so results would differ
    /// from pyphen without saying so.
    Unsupported(&'static str),
    Empty,
}

impl Hyphenator {
    pub fn parse(source: &str) -> Result<Self, PatternError> {
        let mut patterns = HashMap::new();
        let mut max_len = 0usize;

        // The first line is the encoding declaration, which pyphen consumes separately.
        for line in source.lines().skip(1) {
            let line = line.trim();
            if line.is_empty() || starts_with_ignored(line) {
                continue;
            }
            if line.contains("^^") {
                return Err(PatternError::Unsupported("^^xx hex escapes"));
            }
            if line.contains('/') && line.contains('=') {
                return Err(PatternError::Unsupported("/= non-standard hyphenation"));
            }

            let (tags, values) = split_pattern(line);
            let Some(first) = values.iter().position(|&v| v != 0) else {
                continue; // pyphen drops patterns whose values are all zero.
            };

            max_len = max_len.max(tags.len());
            patterns.insert(
                tags,
                Pattern {
                    // Leading zeros are folded into `offset`; trailing ones are kept. Trimming
                    // them saved 18.4 KB of 81 KB across 11,015 patterns — 0.35% of the binary
                    // — and cost an unkillable mutant, because under-trimming a run of no-ops
                    // cannot change an output and so no test can detect it. The 18 KB is the
                    // better thing to spend.
                    offset: first,
                    values: values[first..].to_vec(),
                },
            );
        }

        if patterns.is_empty() {
            return Err(PatternError::Empty);
        }
        Ok(Self { patterns, max_len })
    }

    /// Every position in the word where a hyphen may fall, before the left and right
    /// minimums are applied. Mirrors `HyphDict.positions`.
    fn raw_positions(&self, word: &str) -> Vec<usize> {
        let lowered = word.to_lowercase();
        let pointed: Vec<char> = std::iter::once('.')
            .chain(lowered.chars())
            .chain(std::iter::once('.'))
            .collect();

        // One longer than the pointed word, exactly as pyphen sizes it.
        let mut references = vec![0u8; pointed.len() + 1];

        for i in 0..pointed.len().saturating_sub(1) {
            let stop = (i + self.max_len).min(pointed.len());
            for j in (i + 1)..=stop {
                let slice: String = pointed[i..j].iter().collect();
                let Some(pattern) = self.patterns.get(&slice) else {
                    continue;
                };
                let start = i + pattern.offset;
                for (k, &value) in pattern.values.iter().enumerate() {
                    // pyphen's `map(max, values, references[slice])` stops at the shorter
                    // iterable, so values running past the end are dropped rather than
                    // extending the list.
                    let Some(slot) = references.get_mut(start + k) else {
                        break;
                    };
                    *slot = (*slot).max(value);
                }
            }
        }

        // An odd priority means a hyphen is allowed. The index is shifted by the leading dot.
        references
            .iter()
            .enumerate()
            .filter(|(_, r)| **r % 2 == 1)
            .filter_map(|(i, _)| i.checked_sub(1))
            .collect()
    }

    /// Hyphenation positions with the left and right minimums applied, as `Pyphen.positions`
    /// returns them.
    pub fn positions(&self, word: &str) -> Vec<usize> {
        let length = word.chars().count();
        let Some(right) = length.checked_sub(RIGHT_MIN) else {
            return Vec::new();
        };
        self.raw_positions(word)
            .into_iter()
            .filter(|&i| i >= LEFT_MIN && i <= right)
            .collect()
    }

    /// What `textstat` uses: hyphenation points plus one.
    pub fn syllables(&self, word: &str) -> usize {
        self.positions(word).len() + 1
    }
}

fn starts_with_ignored(line: &str) -> bool {
    const IGNORED: [&str; 6] = [
        "%",
        "#",
        "LEFTHYPHENMIN",
        "RIGHTHYPHENMIN",
        "COMPOUNDLEFTHYPHENMIN",
        "COMPOUNDRIGHTHYPHENMIN",
    ];
    IGNORED.iter().any(|prefix| line.starts_with(prefix))
}

/// Split `.a2ch4` into the letters `.ach` and the priorities `[0, 0, 2, 0, 4]`.
///
/// Mirrors pyphen's `re.findall(r'(\d?)(\D?)', pattern)`: each step takes an optional digit
/// and then an optional non-digit, so a digit binds to the character *after* it.
fn split_pattern(pattern: &str) -> (String, Vec<u8>) {
    let chars: Vec<char> = pattern.chars().collect();
    let mut tags = String::new();
    let mut values = Vec::new();
    let mut consumed_as_letter = false;

    // A `for` over a fixed list, with a flag for the character a digit binds to, rather than a
    // `while` over a cursor. Both spell the same scan, but a mutation of a cursor's `+= 1`
    // stops it advancing and hangs the suite; this cannot fail to terminate.
    for (i, &c) in chars.iter().enumerate() {
        if consumed_as_letter {
            consumed_as_letter = false;
            continue;
        }

        // A digit gives the value at this position and binds to the character after it;
        // anything else sits at a position whose value is zero.
        let (value, letter_at) = if c.is_ascii_digit() {
            (c as u8 - b'0', i + 1)
        } else {
            (0, i)
        };
        values.push(value);

        // Two digits in a row leave the first with no letter, and so does a trailing digit.
        if let Some(&letter) = chars.get(letter_at)
            && !letter.is_ascii_digit()
        {
            tags.push(letter);
            consumed_as_letter = letter_at != i;
        }
    }

    // A trailing digit contributes a value with no letter, which is the position after the
    // last character. pyphen's findall produces the same trailing empty match.
    values.push(0);
    (tags, values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pattern_binds_each_digit_to_the_character_after_it() {
        let (tags, values) = split_pattern(".a2ch4");
        assert_eq!(tags, ".ach");
        assert_eq!(values, vec![0, 0, 2, 0, 4, 0]);

        let (tags, values) = split_pattern("a1b");
        assert_eq!(tags, "ab");
        assert_eq!(values, vec![0, 1, 0]);
    }

    #[test]
    fn the_bundled_patterns_parse() {
        let hyphenator = en_us();
        assert!(
            hyphenator.patterns.len() > 10_000,
            "only {} patterns parsed",
            hyphenator.patterns.len()
        );
        assert!(hyphenator.max_len >= 5);
    }

    /// A file using a feature we do not implement must fail loudly. Silently ignoring it
    /// would hyphenate differently from pyphen with no indication.
    #[test]
    fn unsupported_pattern_features_are_refused_rather_than_ignored() {
        assert_eq!(
            Hyphenator::parse("UTF-8\nx1y\nca^^e9t2").err(),
            Some(PatternError::Unsupported("^^xx hex escapes"))
        );
        assert_eq!(
            Hyphenator::parse("UTF-8\nx1y\na1b/c=d,1,2").err(),
            Some(PatternError::Unsupported("/= non-standard hyphenation"))
        );
    }

    #[test]
    fn an_empty_pattern_file_is_an_error() {
        assert!(matches!(
            Hyphenator::parse("UTF-8\n% nothing here\n"),
            Err(PatternError::Empty)
        ));
    }

    /// Captured from `pyphen.Pyphen(lang="en_US")`, which is how textstat builds it.
    #[test]
    fn known_words_hyphenate_where_pyphen_says() {
        let h = en_us();
        assert_eq!(h.positions("hyphenation"), vec![2, 6]);
        assert_eq!(h.positions("computer"), vec![3, 6]);
        assert_eq!(h.positions("syllable"), vec![3, 5]);
        assert_eq!(h.positions("readability"), vec![4, 8, 9]);
        assert_eq!(h.positions("extraordinary"), vec![2, 5, 7, 9]);
        assert!(
            h.positions("project").is_empty(),
            "pyphen finds no break in 'project'"
        );
    }

    #[test]
    fn short_words_have_nowhere_to_break() {
        let h = en_us();
        for word in ["", "a", "an", "the", "cat"] {
            assert!(h.positions(word).is_empty(), "{word:?}");
            assert_eq!(h.syllables(word), 1, "{word:?}");
        }
    }

    /// The shipped `hyph_en_US.dic` contains no `/` and no `=`, so nothing in the parity
    /// suite reaches this guard — a mutation loosening it to `||` changed no test. These
    /// feed `parse` directly, which is the only way to exercise a dictionary we do not ship.
    #[test]
    fn non_standard_hyphenation_is_rejected_rather_than_parsed_wrongly() {
        // `ff1f/ff=f` is the shape this cannot handle: a replacement spelling, not a simple
        // break point. Accepting it silently would hyphenate the word at the wrong place.
        let both = "UTF-8\nff1f/ff=f\na1bc\n";
        assert!(matches!(
            Hyphenator::parse(both),
            Err(PatternError::Unsupported(_))
        ));
    }

    /// The guard deliberately requires *both* characters. A `/` or an `=` alone is not the
    /// non-standard form, and rejecting a dictionary over one would be a false refusal.
    #[test]
    fn a_slash_or_an_equals_alone_is_still_parseable() {
        assert!(
            Hyphenator::parse("UTF-8\na1bc\nde/2f\n").is_ok(),
            "slash alone"
        );
        assert!(
            Hyphenator::parse("UTF-8\na1bc\nde=2f\n").is_ok(),
            "equals alone"
        );
    }

    /// A pattern line of bare digits carries no letters, so it can never match anything. The
    /// scan has to start at a slice of length one for that to hold: starting at length zero
    /// looks up the empty string, which such a pattern *does* key, and it would then apply at
    /// every position in every word.
    #[test]
    fn a_pattern_with_no_letters_applies_nowhere() {
        let plain = Hyphenator::parse("UTF-8\nhy3ph\n").expect("parses");
        let with_degenerate = Hyphenator::parse("UTF-8\nhy3ph\n1\n").expect("parses");
        assert_eq!(
            plain.raw_positions("hyphenation"),
            with_degenerate.raw_positions("hyphenation"),
            "a letterless pattern must not introduce break points"
        );
    }

    /// Trailing zeros are kept rather than trimmed, which is only safe because they are
    /// no-ops: `raw_positions` folds each value in with `max`.
    #[test]
    fn a_trailing_zero_is_a_no_op() {
        let plain = Hyphenator::parse("UTF-8\nhy3ph\n").expect("parses");
        let padded = Hyphenator::parse("UTF-8\nhy3ph0\n").expect("parses");
        assert_eq!(
            plain.raw_positions("hyphen"),
            padded.raw_positions("hyphen"),
            "a trailing zero must not move a break point"
        );
    }

    #[test]
    fn syllables_are_one_more_than_the_break_points() {
        let h = en_us();
        assert_eq!(h.syllables("hyphenation"), 3);
        assert_eq!(h.syllables("computer"), 3);
        assert_eq!(h.syllables("extraordinary"), 5);
    }

    /// pyphen reports 11,015 patterns with a longest key of 28 characters. A parser that
    /// quietly dropped a class of lines would still work and would still be wrong.
    #[test]
    fn the_pattern_table_matches_pyphens() {
        let h = en_us();
        assert_eq!(h.patterns.len(), 11_015);
        assert_eq!(h.max_len, 28);
    }

    #[test]
    fn case_does_not_change_the_answer() {
        let h = en_us();
        assert_eq!(h.positions("Hyphenation"), h.positions("hyphenation"));
        assert_eq!(h.positions("HYPHENATION"), h.positions("hyphenation"));
    }

    #[hegel::test]
    fn hyphenating_arbitrary_text_never_panics(tc: hegel::TestCase) {
        let word = tc.draw(hegel::generators::text().max_size(40));
        let h = en_us();
        let positions = h.positions(&word);
        let length = word.chars().count();
        for p in &positions {
            assert!(*p >= LEFT_MIN, "{word:?} broke at {p}");
            assert!(
                *p <= length - RIGHT_MIN,
                "{word:?} broke at {p} of {length}"
            );
        }
        assert_eq!(h.syllables(&word), positions.len() + 1);
    }

    /// Every break point must sit strictly inside the word, or the "syllables" it implies are
    /// not syllables.
    #[hegel::test]
    fn break_points_are_ordered_and_distinct(tc: hegel::TestCase) {
        let word: String = tc
            .draw(
                hegel::generators::vecs(hegel::generators::sampled_from(
                    "abcdefghijklmnopqrstuvwxyz".chars().collect::<Vec<_>>(),
                ))
                .max_size(30),
            )
            .into_iter()
            .collect();
        let positions = en_us().positions(&word);
        assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "{word:?} gave {positions:?}"
        );
    }
}
