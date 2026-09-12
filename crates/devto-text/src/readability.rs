//! Readability formulas, reproducing `textstat` 0.7.13.
//!
//! Six metrics, chosen because none of them needs a word list. The four that do —
//! Gunning Fog, Dale–Chall in both revisions, and Spache — all depend on the Dale–Chall
//! familiar-words list, whose provenance traces to a 1948 publication. `textstat` ships it
//! under MIT; that is textstat's judgement to make rather than a licence grant, so these
//! metrics are left out instead of the question being argued. Adding Gunning Fog later costs
//! one formula and one data file.
//!
//! Every formula returns `0.0` for text it cannot measure, which is textstat's behaviour
//! rather than an error or a NaN. That is worth knowing when reading a result: a zero here
//! means "no answer", not "unreadable".

use serde::Serialize;

use crate::counts::{self, Counts};

/// English constants from textstat's language configuration.
const FRE_BASE: f64 = 206.835;
const FRE_SENTENCE_LENGTH: f64 = 1.015;
const FRE_SYLL_PER_WORD: f64 = 84.6;

/// Every metric over one text, with the ratios they are built from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Readability {
    pub flesch_reading_ease: f64,
    pub flesch_kincaid_grade: f64,
    pub smog_index: f64,
    pub coleman_liau_index: f64,
    pub automated_readability_index: f64,
    pub mcalpine_eflaw: f64,
    pub words_per_sentence: f64,
    pub syllables_per_word: f64,
}

impl Readability {
    pub fn of(text: &str) -> Self {
        let counts = Counts::of(text);
        Self {
            flesch_reading_ease: flesch_reading_ease(&counts),
            flesch_kincaid_grade: flesch_kincaid_grade(&counts),
            smog_index: smog_index(&counts),
            coleman_liau_index: coleman_liau_index(&counts),
            automated_readability_index: automated_readability_index(text, &counts),
            mcalpine_eflaw: mcalpine_eflaw(text, &counts),
            words_per_sentence: words_per_sentence(&counts),
            syllables_per_word: syllables_per_word(&counts),
        }
    }

    /// The plain-language band Flesch Reading Ease falls in. Reported alongside the number
    /// because "46.7" means nothing to most readers and "difficult" does.
    pub fn reading_ease_band(&self) -> &'static str {
        match self.flesch_reading_ease {
            s if s >= 90.0 => "very easy",
            s if s >= 80.0 => "easy",
            s if s >= 70.0 => "fairly easy",
            s if s >= 60.0 => "plain English",
            s if s >= 50.0 => "fairly difficult",
            s if s >= 30.0 => "difficult",
            _ => "very difficult",
        }
    }
}

pub fn words_per_sentence(counts: &Counts) -> f64 {
    ratio(counts.words, counts.sentences)
}

pub fn syllables_per_word(counts: &Counts) -> f64 {
    ratio(counts.syllables, counts.words)
}

pub fn letters_per_word(counts: &Counts) -> f64 {
    ratio(counts.letters, counts.words)
}

pub fn sentences_per_word(counts: &Counts) -> f64 {
    ratio(counts.sentences, counts.words)
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    numerator as f64 / denominator as f64
}

pub fn flesch_reading_ease(counts: &Counts) -> f64 {
    let sentence_length = words_per_sentence(counts);
    let syllables = syllables_per_word(counts);
    if sentence_length == 0.0 || syllables == 0.0 {
        return 0.0;
    }
    FRE_BASE - FRE_SENTENCE_LENGTH * sentence_length - FRE_SYLL_PER_WORD * syllables
}

pub fn flesch_kincaid_grade(counts: &Counts) -> f64 {
    let sentence_length = words_per_sentence(counts);
    let syllables = syllables_per_word(counts);
    if sentence_length == 0.0 || syllables == 0.0 {
        return 0.0;
    }
    (0.39 * sentence_length) + (11.8 * syllables) - 15.59
}

pub fn smog_index(counts: &Counts) -> f64 {
    if counts.sentences == 0 {
        return 0.0;
    }
    let poly = counts.polysyllables as f64 / counts.sentences as f64;
    (1.043 * (30.0 * poly).sqrt()) + 3.1291
}

pub fn coleman_liau_index(counts: &Counts) -> f64 {
    let letters = letters_per_word(counts) * 100.0;
    let sentences = sentences_per_word(counts) * 100.0;
    if letters == 0.0 || sentences == 0.0 {
        return 0.0;
    }
    (0.058 * letters) - (0.296 * sentences) - 15.8
}

/// ARI is the one metric that does not use the cleaned word count.
///
/// `chars_per_word` divides characters-without-whitespace by `count_words(rm_punctuation =
/// False)` — a plain whitespace split. So `(yes)` is one word here and one word there, but
/// `a -- b` is three tokens rather than two words.
pub fn automated_readability_index(text: &str, counts: &Counts) -> f64 {
    let chars = text.chars().filter(|c| !c.is_whitespace()).count();
    let raw_tokens = text.split_whitespace().count();
    let chars_per_word = ratio(chars, raw_tokens);
    let sentence_length = words_per_sentence(counts);
    if chars_per_word == 0.0 || sentence_length == 0.0 {
        return 0.0;
    }
    (4.71 * chars_per_word) + (0.5 * sentence_length) - 21.43
}

/// McAlpine EFLAW, for how hard a text is for a non-native reader. Short words help rather
/// than hurt here, which is why it counts them.
pub fn mcalpine_eflaw(text: &str, counts: &Counts) -> f64 {
    if counts.sentences == 0 {
        return 0.0;
    }
    let miniwords = count_miniwords(text, 3);
    (counts.words + miniwords) as f64 / counts.sentences as f64
}

/// Words of at most `max_size` characters, with apostrophes stripped — `count_miniwords`
/// uses `rm_apostrophe = True`, so `don't` is five characters here.
pub fn count_miniwords(text: &str, max_size: usize) -> usize {
    let cleaned = counts::remove_all_punctuation(text);
    cleaned
        .split_whitespace()
        .filter(|w| w.chars().count() <= max_size)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROSE: &str = "The quick brown fox jumps over the lazy dog. \
                         Readability formulas turn counts into a grade level. \
                         They disagree with each other more than anyone admits.";

    #[test]
    fn truly_empty_text_scores_zero_rather_than_failing() {
        let r = Readability::of("");
        assert_eq!(r.flesch_reading_ease, 0.0);
        assert_eq!(r.flesch_kincaid_grade, 0.0);
        assert_eq!(r.smog_index, 0.0);
        assert_eq!(r.coleman_liau_index, 0.0);
        assert_eq!(r.automated_readability_index, 0.0);
        assert_eq!(r.mcalpine_eflaw, 0.0);
    }

    /// SMOG is the odd one out, and this is textstat's behaviour rather than a bug in ours.
    /// Its `3.1291` is an additive constant, and whitespace-only text is not *empty*, so the
    /// sentence count floors at one and the constant survives. A SMOG score of 3.13 means
    /// "nothing measurable here", not "reads at third-grade level".
    #[test]
    fn whitespace_scores_smogs_floor_constant() {
        for text in [" ", "\n\n", "\t"] {
            let r = Readability::of(text);
            assert_eq!(r.smog_index, 3.1291, "{text:?}");
            assert_eq!(r.flesch_reading_ease, 0.0, "{text:?}");
            assert_eq!(r.mcalpine_eflaw, 0.0, "{text:?}");
        }
    }

    /// Flesch Reading Ease is nominally 0–100 and is not bounded by the formula. Very short
    /// sentences of short words run past 100.
    #[test]
    fn reading_ease_is_not_clamped_to_its_nominal_range() {
        let r = Readability::of("Hi.");
        assert!(r.flesch_reading_ease > 100.0, "{}", r.flesch_reading_ease);
        assert_eq!(r.reading_ease_band(), "very easy");
    }

    #[test]
    fn ordinary_prose_lands_in_a_plausible_range() {
        let r = Readability::of(PROSE);
        assert!(
            (0.0..=100.0).contains(&r.flesch_reading_ease),
            "{}",
            r.flesch_reading_ease
        );
        assert!(r.smog_index > 3.1291, "real prose has polysyllables");
        assert!(r.flesch_kincaid_grade > 0.0);
        assert!(r.words_per_sentence > 1.0);
        assert!(r.syllables_per_word >= 1.0);
    }

    /// The band is what a reader acts on, so the boundaries have to be the documented ones.
    #[test]
    fn the_reading_ease_bands_break_where_flesch_says() {
        let band = |score: f64| {
            let mut r = Readability::of(PROSE);
            r.flesch_reading_ease = score;
            r.reading_ease_band()
        };
        assert_eq!(band(95.0), "very easy");
        assert_eq!(band(90.0), "very easy");
        assert_eq!(band(89.9), "easy");
        assert_eq!(band(80.0), "easy");
        assert_eq!(band(70.0), "fairly easy");
        assert_eq!(band(60.0), "plain English");
        assert_eq!(band(50.0), "fairly difficult");
        assert_eq!(band(30.0), "difficult");
        assert_eq!(band(29.9), "very difficult");
        assert_eq!(band(0.0), "very difficult");
    }

    #[test]
    fn a_ratio_over_nothing_is_zero_not_a_nan() {
        assert_eq!(ratio(5, 0), 0.0);
        assert_eq!(ratio(0, 5), 0.0);
        assert_eq!(ratio(3, 2), 1.5);
    }

    #[test]
    fn miniwords_are_counted_without_their_apostrophes() {
        assert_eq!(count_miniwords("a an the", 3), 3);
        assert_eq!(count_miniwords("a an the four", 3), 3);
        assert_eq!(
            count_miniwords("don't", 3),
            0,
            "don't is five characters once the apostrophe goes"
        );
        assert_eq!(count_miniwords("it's", 3), 1, "its is three");
    }

    /// Longer sentences and longer words both make a text score as harder. A formula wired
    /// up backwards still produces plausible-looking numbers.
    #[test]
    fn harder_text_scores_as_harder() {
        let simple = "The cat sat on the mat. The dog ran to the log. A bird sat on a wall.";
        let hard = "Notwithstanding the aforementioned considerations regarding \
                    infrastructural interoperability, the organisation's representatives \
                    subsequently determined that comprehensive reconceptualisation remained \
                    fundamentally unavoidable.";

        let s = Readability::of(simple);
        let h = Readability::of(hard);

        assert!(
            s.flesch_reading_ease > h.flesch_reading_ease,
            "simple {} should read easier than hard {}",
            s.flesch_reading_ease,
            h.flesch_reading_ease
        );
        assert!(s.flesch_kincaid_grade < h.flesch_kincaid_grade);
        assert!(s.coleman_liau_index < h.coleman_liau_index);
        assert!(s.syllables_per_word < h.syllables_per_word);
    }

    #[hegel::test]
    fn no_text_produces_a_nan_or_an_infinity(tc: hegel::TestCase) {
        let text = tc.draw(hegel::generators::text().max_size(400));
        let r = Readability::of(&text);
        for (name, value) in [
            ("flesch_reading_ease", r.flesch_reading_ease),
            ("flesch_kincaid_grade", r.flesch_kincaid_grade),
            ("smog_index", r.smog_index),
            ("coleman_liau_index", r.coleman_liau_index),
            ("automated_readability_index", r.automated_readability_index),
            ("mcalpine_eflaw", r.mcalpine_eflaw),
        ] {
            assert!(value.is_finite(), "{name} was {value} for {text:?}");
        }
    }
}
