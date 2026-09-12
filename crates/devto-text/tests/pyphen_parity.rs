//! Parity with `pyphen`, the hyphenator `textstat` uses.
//!
//! Six hand-picked words prove nothing about an algorithm with 11,015 patterns. The fixture
//! is 20,854 words — every word of more than one letter from the author's own 30 published
//! articles, plus a seeded sample of CMUdict for the long and unusual ones — with the
//! positions `pyphen.Pyphen(lang="en_US")` reports for each.
//!
//! Regenerate with `scripts/refresh-parity.sh`. A divergence here is a defect in ours until
//! proven otherwise.

use std::collections::BTreeMap;

use devto_text::hyphen;

#[test]
fn hyphenation_matches_pyphen_across_the_vocabulary() {
    let raw = include_str!("fixtures/pyphen_positions.json");
    let expected: BTreeMap<String, Vec<usize>> =
        serde_json::from_str(raw).expect("the parity fixture");

    assert!(
        expected.len() > 20_000,
        "the fixture shrank to {} words",
        expected.len()
    );

    let hyphenator = hyphen::en_us();
    let mut divergences = Vec::new();

    for (word, want) in &expected {
        let got = hyphenator.positions(word);
        if &got != want {
            divergences.push(format!("{word:?}: pyphen {want:?}, ours {got:?}"));
        }
    }

    assert!(
        divergences.is_empty(),
        "{} of {} words hyphenate differently from pyphen:\n  {}",
        divergences.len(),
        expected.len(),
        divergences
            .iter()
            .take(15)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

/// The count is what `textstat` actually consumes, so check it directly rather than
/// inferring it from the positions.
#[test]
fn syllable_counts_follow_from_the_positions() {
    let raw = include_str!("fixtures/pyphen_positions.json");
    let expected: BTreeMap<String, Vec<usize>> =
        serde_json::from_str(raw).expect("the parity fixture");

    let hyphenator = hyphen::en_us();
    for (word, want) in expected.iter().take(5_000) {
        assert_eq!(
            hyphenator.syllables(word),
            want.len() + 1,
            "syllable count for {word:?}"
        );
    }
}
