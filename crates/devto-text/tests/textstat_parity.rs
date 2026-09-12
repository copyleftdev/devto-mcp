//! Parity with `textstat` 0.7.13 on the counts every readability formula is built from.
//!
//! 52 cases: 22 hand-chosen for the places implementations diverge — contractions,
//! hyphenation, accented words, abbreviations, one-word sentences, empty input — and 30
//! extracts of real prose from the author's published articles.
//!
//! A divergence here is a defect in ours until proven otherwise.

use devto_text::counts;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Case {
    text: String,
    words: usize,
    sentences: usize,
    syllables: usize,
    letters: usize,
    polysyllables: usize,
    monosyllables: usize,
}

fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("fixtures/textstat_counts.json")).expect("the fixture")
}

/// One report for all of them: chasing a count divergence one assertion at a time hides
/// whether the cause is general or particular.
#[test]
fn counts_match_textstat() {
    let mut divergences: Vec<String> = Vec::new();

    for (i, case) in cases().iter().enumerate() {
        let got = counts::Counts::of(&case.text);
        let mut note = |metric: &str, want: usize, got: usize| {
            if want != got {
                let preview: String = case.text.chars().take(60).collect();
                divergences.push(format!(
                    "case {i} {metric}: textstat {want}, ours {got}  — {preview:?}"
                ));
            }
        };
        note("words", case.words, got.words);
        note("sentences", case.sentences, got.sentences);
        note("syllables", case.syllables, got.syllables);
        note("letters", case.letters, got.letters);
        note("polysyllables", case.polysyllables, got.polysyllables);
        note("monosyllables", case.monosyllables, got.monosyllables);
    }

    assert!(
        divergences.is_empty(),
        "{} divergence(s) from textstat:\n  {}",
        divergences.len(),
        divergences.join("\n  ")
    );
}

#[test]
fn the_fixture_covers_what_it_claims_to() {
    let cases = cases();
    assert!(cases.len() >= 50, "the fixture shrank to {}", cases.len());
    assert!(
        cases.iter().any(|c| c.text.is_empty()),
        "empty input is the case most often got wrong"
    );
    assert!(
        cases.iter().any(|c| c.text.contains('\'')),
        "contractions decide the word count"
    );
    assert!(
        cases.iter().any(|c| !c.text.is_ascii()),
        "accented words decide whether \\w is Unicode-aware"
    );
    assert!(
        cases.iter().any(|c| c.words > 500),
        "real articles, not just fragments"
    );
}
