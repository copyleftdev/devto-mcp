//! Syllable counts from CMUdict, with hyphenation as the fallback.
//!
//! `textstat` looks a word up in the CMU Pronouncing Dictionary and counts the phonemes in
//! its *first* pronunciation whose last character is a digit — CMUdict's stress marker, and
//! therefore its vowel marker. Only when the word is unknown does it fall back to
//! hyphenating. Both halves are needed for parity: a hyphenation-only implementation
//! diverges on every one of the 123,455 words CMUdict knows.
//!
//! **The edition matters.** `textstat` reads CMUdict through NLTK, which ships an older
//! release than the current `cmusphinx/cmudict` master. They disagree on which pronunciation
//! comes first, and only the first one is read: today's master gives `extraordinary` six
//! stressed vowels where NLTK's gives five. Building from master looked correct and was
//! wrong. This table is extracted from NLTK's copy, which is the one textstat consults.

use std::sync::OnceLock;

use fst::Map;

/// Word -> syllable count, built from CMUdict by `scripts/build_syllable_fst.rs`.
///
/// An FST rather than a `HashMap`: it is consulted once per word over whole articles, and
/// this form needs no parse at startup and shares prefixes across 126k entries in 673 KB.
const CMUDICT_FST: &[u8] = include_bytes!("../data/cmudict_syllables.fst");

pub struct Cmudict {
    map: Map<&'static [u8]>,
}

impl Cmudict {
    /// The syllable count CMUdict gives this word, or `None` if it does not know it.
    pub fn syllables(&self, word: &str) -> Option<usize> {
        self.map.get(word.as_bytes()).map(|n| n as usize)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

pub fn cmudict() -> &'static Cmudict {
    static INSTANCE: OnceLock<Cmudict> = OnceLock::new();
    INSTANCE.get_or_init(|| Cmudict {
        map: Map::new(CMUDICT_FST).expect("the bundled CMUdict FST"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hyphen;

    /// `len` and `is_empty` were reachable public API that nothing called and nothing
    /// checked. Pinning the size also pins the dataset: this is NLTK's edition of CMUdict,
    /// which is the one `textstat` reads, and it is not the same as cmusphinx master.
    #[test]
    fn the_bundled_dictionary_is_nltks_cmudict() {
        let dict = cmudict();
        assert!(!dict.is_empty());
        // `!is_empty()` alone does not pin `is_empty`: a body returning a constant `false`
        // satisfies it. An actually-empty dictionary is the other half of the statement.
        let bytes: &'static [u8] =
            Box::leak(Map::default().into_fst().into_inner().into_boxed_slice());
        let empty = Cmudict {
            map: Map::new(bytes).expect("an empty fst"),
        };
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert_eq!(dict.len(), 123_455, "NLTK's CMUdict, not cmusphinx master");
        // The word that told the two editions apart: master gives it six.
        assert_eq!(dict.syllables("extraordinary"), Some(5));
    }

    #[test]
    fn the_dictionary_loads_with_every_entry() {
        assert_eq!(cmudict().len(), 123_455);
    }

    /// Counts taken from CMUdict's own first pronunciation for each word.
    #[test]
    fn known_words_come_from_the_dictionary() {
        let d = cmudict();
        assert_eq!(d.syllables("hello"), Some(2));
        assert_eq!(d.syllables("computer"), Some(3));
        assert_eq!(d.syllables("a"), Some(1));
        assert_eq!(d.syllables("extraordinary"), Some(5));
    }

    /// This is why both halves exist: CMUdict is a 1990s dictionary and knows nothing about
    /// the vocabulary a technical article is written in.
    #[test]
    fn modern_words_fall_through_to_hyphenation() {
        let d = cmudict();
        let h = hyphen::en_us();
        for word in ["kubernetes", "webassembly", "devto"] {
            assert!(d.syllables(word).is_none(), "{word} was in CMUdict");
            assert!(h.syllables(word) >= 1);
        }
    }

    #[test]
    fn an_unknown_word_is_none_rather_than_zero() {
        assert_eq!(cmudict().syllables("zzzzqqqq"), None);
        assert_eq!(cmudict().syllables(""), None);
    }

    /// The FST is keyed on lowercase, which is what `list_words(lowercase=true)` produces.
    #[test]
    fn lookups_are_lowercase() {
        assert!(cmudict().syllables("Hello").is_none());
        assert!(cmudict().syllables("hello").is_some());
    }
}
