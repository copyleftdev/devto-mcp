//! Word, sentence and syllable counts, reproducing `textstat` exactly.
//!
//! Every readability formula is arithmetic over these three numbers, so a divergence here is
//! a divergence in every metric. Three details decide whether an implementation matches:
//!
//! - **Python's `\w` is Unicode-aware.** `[^\w\s']` keeps `café` and `naïve` whole. This is
//!   the opposite of Forem's own reading-time count, where Ruby's `\W` is ASCII-only and
//!   splits `naïve` in two — see [`crate::forem`].
//! - **Apostrophes survive only inside contractions.** `don't` keeps its apostrophe;
//!   `'quoted'` loses both. Hyphens are always removed, so `well-known` becomes `wellknown`.
//! - **Sentences are found by regex, not by a trained tokenizer.** Fragments of two words or
//!   fewer are discarded, and the count never falls below one for non-empty input.

use crate::hyphen;
use crate::syllables;

/// The contraction endings whose apostrophe `textstat` keeps: `'t 's 'd 've 'll 're`.
fn is_contraction_ending(rest: &str) -> bool {
    const ENDINGS: [&str; 6] = ["t", "s", "d", "ve", "ll", "re"];
    ENDINGS.iter().any(|e| rest.starts_with(e))
}

/// Strip punctuation the way `remove_punctuation(text, rm_apostrophe=True)` does: nothing
/// but alphanumerics, underscores and whitespace survives.
pub fn remove_all_punctuation(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || c.is_whitespace())
        .collect()
}

/// Strip punctuation the way `remove_punctuation(text, rm_apostrophe=False)` does.
pub fn remove_punctuation(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());

    for (i, &c) in chars.iter().enumerate() {
        if c == '\'' {
            // An apostrophe survives only when a contraction ending follows it.
            let rest: String = chars[i + 1..].iter().take(2).collect();
            if is_contraction_ending(&rest) {
                out.push(c);
            }
            continue;
        }
        if c.is_alphanumeric() || c == '_' || c.is_whitespace() {
            out.push(c);
        }
    }
    out
}

/// `list_words(text, lowercase = ...)`: punctuation stripped, then split on whitespace.
pub fn list_words(text: &str, lowercase: bool) -> Vec<String> {
    let cleaned = remove_punctuation(text);
    let cleaned = if lowercase {
        cleaned.to_lowercase()
    } else {
        cleaned
    };
    cleaned.split_whitespace().map(str::to_string).collect()
}

pub fn count_words(text: &str) -> usize {
    list_words(text, false).len()
}

/// `letter_count` drops whitespace and then removes **all** punctuation, apostrophes
/// included — unlike the word count, which keeps a contraction's apostrophe. So `don't` is
/// five letters here and one word there.
pub fn count_letters(text: &str) -> usize {
    let without_space: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    remove_all_punctuation(&without_space).chars().count()
}

/// `count_sentences`: `re.findall(r"\b[^.!?]+[.!?]*", text)`, dropping fragments of two
/// words or fewer, floored at one for non-empty input.
///
/// The regex is reproduced by hand rather than with a regex engine, because `\b` is defined
/// against Python's Unicode `\w` and the interaction with the character class is the part
/// that has to match.
pub fn count_sentences(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }

    let chars: Vec<char> = text.chars().collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let is_terminator = |c: char| matches!(c, '.' | '!' | '?');
    let mut sentences = 0usize;
    let mut fragment_start: Option<usize> = None;

    // A `for` over the characters rather than a hand-advanced cursor. The two are the same
    // algorithm until a mutation turns an `i += 1` into `i *= 1`, at which point the cursor
    // version stops advancing and the suite hangs rather than failing — and a hung gate is
    // indistinguishable from a slow one.
    for (i, &c) in chars.iter().enumerate() {
        // `\b`, right-hand half only: a fragment opens on the first word character after the
        // previous one closed. The left half of `\b` is implied — there is no open fragment
        // here, so the preceding character was a terminator or a non-word character.
        if fragment_start.is_none() && is_word(c) {
            fragment_start = Some(i);
        }
        // `[^.!?]+` ends here, and the regex's trailing `[.!?]*` is deliberately not captured:
        // word counting strips punctuation, so those characters cannot change the outcome.
        if is_terminator(c)
            && let Some(start) = fragment_start.take()
        {
            let fragment: String = chars[start..i].iter().collect();
            if count_words(&fragment) > 2 {
                sentences += 1;
            }
        }
    }

    // A fragment with no terminator to close it runs to the end of the input.
    if let Some(start) = fragment_start {
        let fragment: String = chars[start..].iter().collect();
        if count_words(&fragment) > 2 {
            sentences += 1;
        }
    }

    // textstat floors the count at one for any non-empty input.
    sentences.max(1)
}

pub fn count_syllables(text: &str) -> usize {
    let dictionary = syllables::cmudict();
    let hyphenator = hyphen::en_us();
    list_words(text, true)
        .iter()
        .map(|word| {
            dictionary
                .syllables(word)
                .unwrap_or_else(|| hyphenator.syllables(word))
        })
        .sum()
}

/// Words of three or more syllables, as Gunning Fog and SMOG need them.
pub fn count_polysyllable_words(text: &str) -> usize {
    syllables_per_word(text).filter(|&n| n >= 3).count()
}

pub fn count_monosyllable_words(text: &str) -> usize {
    syllables_per_word(text).filter(|&n| n == 1).count()
}

fn syllables_per_word(text: &str) -> impl Iterator<Item = usize> + '_ {
    let dictionary = syllables::cmudict();
    let hyphenator = hyphen::en_us();
    list_words(text, true).into_iter().map(move |word| {
        dictionary
            .syllables(&word)
            .unwrap_or_else(|| hyphenator.syllables(&word))
    })
}

/// Cached because every formula asks for the same handful of numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Counts {
    pub words: usize,
    pub sentences: usize,
    pub syllables: usize,
    pub letters: usize,
    pub polysyllables: usize,
    pub monosyllables: usize,
}

impl Counts {
    pub fn of(text: &str) -> Self {
        let mut polysyllables = 0;
        let mut monosyllables = 0;
        let mut syllables = 0;
        for n in syllables_per_word(text) {
            syllables += n;
            if n >= 3 {
                polysyllables += 1;
            }
            if n == 1 {
                monosyllables += 1;
            }
        }
        Self {
            words: count_words(text),
            sentences: count_sentences(text),
            syllables,
            letters: count_letters(text),
            polysyllables,
            monosyllables,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `_` is a word character for Python's `\w`, so it can open a sentence fragment. Every
    /// other case here starts on a letter, where the distinction cannot show: the fragments
    /// have to be short enough that losing a leading `_` drops them under the two-word floor.
    #[test]
    fn an_underscore_is_a_word_character() {
        assert_eq!(count_sentences("_ _ _. _ _ _."), 2);
        assert_eq!(count_words("_ _ _"), 3);
    }

    #[test]
    fn an_apostrophe_survives_only_inside_a_contraction() {
        assert_eq!(remove_punctuation("don't"), "don't");
        assert_eq!(remove_punctuation("it's"), "it's");
        assert_eq!(remove_punctuation("we've"), "we've");
        assert_eq!(remove_punctuation("they'll"), "they'll");
        assert_eq!(remove_punctuation("you're"), "you're");
        assert_eq!(remove_punctuation("he'd"), "he'd");

        assert_eq!(remove_punctuation("'quoted'"), "quoted");
        assert_eq!(remove_punctuation("dogs'"), "dogs");
    }

    /// Hyphens are always removed, which joins the halves rather than splitting the word.
    #[test]
    fn a_hyphenated_word_becomes_one_word() {
        assert_eq!(remove_punctuation("well-known"), "wellknown");
        assert_eq!(count_words("a well-known case"), 3);
    }

    /// Unlike Forem's own word count, this one is Unicode-aware.
    #[test]
    fn accented_words_stay_whole() {
        assert_eq!(remove_punctuation("naïve café"), "naïve café");
        assert_eq!(count_words("naïve café"), 2);
    }

    /// A contraction is one word with an apostrophe, and five letters without one.
    #[test]
    fn letters_and_words_disagree_about_apostrophes_on_purpose() {
        assert_eq!(count_words("don't"), 1);
        assert_eq!(count_letters("don't"), 4);
        assert_eq!(count_letters("don't it's we've"), 11);
        assert_eq!(count_letters("Hello, world!"), 10);
    }

    #[test]
    fn punctuation_goes_but_whitespace_stays() {
        assert_eq!(remove_punctuation("Hello, world! (yes)"), "Hello world yes");
        assert_eq!(count_words("Hello, world! (yes)"), 3);
    }

    #[test]
    fn empty_text_counts_nothing() {
        assert_eq!(count_words(""), 0);
        assert_eq!(count_sentences(""), 0);
        assert_eq!(count_syllables(""), 0);
    }

    /// Fragments of two words or fewer are discarded, and the floor is one.
    #[test]
    fn short_fragments_do_not_count_as_sentences() {
        assert_eq!(count_sentences("This is a full sentence here."), 1);
        assert_eq!(
            count_sentences("This is a full sentence. And this is another one."),
            2
        );
        assert_eq!(
            count_sentences("Yes. No. This one is long enough to survive."),
            1,
            "two-word fragments are dropped"
        );
        assert_eq!(count_sentences("Hi."), 1, "the count never drops below one");
    }

    #[test]
    fn a_sentence_needs_no_terminator() {
        assert_eq!(count_sentences("This has no full stop at all"), 1);
    }

    #[hegel::test]
    fn counting_arbitrary_text_never_panics(tc: hegel::TestCase) {
        let text = tc.draw(hegel::generators::text().max_size(300));
        let counts = Counts::of(&text);
        assert!(counts.polysyllables <= counts.words);
        assert!(counts.monosyllables <= counts.words);
        assert!(
            counts.syllables >= counts.words,
            "every word has a syllable"
        );
        if !text.is_empty() {
            assert!(counts.sentences >= 1);
        }
    }

    #[hegel::test]
    fn stripping_punctuation_is_idempotent(tc: hegel::TestCase) {
        let text = tc.draw(hegel::generators::text().max_size(200));
        let once = remove_punctuation(&text);
        assert_eq!(remove_punctuation(&once), once);
    }
}
