//! Readability parity with `textstat` 0.7.13, over the same 58 cases as the counts.
//!
//! Floats are compared with a tolerance of 1e-9 rather than for bit equality: both sides do
//! the same arithmetic in the same order, but Rust and CPython need not round an `f64`
//! division identically, and a tolerance this tight still catches a wrong constant, a
//! swapped term or a divergent count.

use devto_text::readability::Readability;
use serde::Deserialize;

const TOLERANCE: f64 = 1e-9;

#[derive(Debug, Deserialize)]
struct Case {
    text: String,
    flesch_reading_ease: f64,
    flesch_kincaid_grade: f64,
    smog_index: f64,
    coleman_liau_index: f64,
    automated_readability_index: f64,
    mcalpine_eflaw: f64,
}

#[test]
fn readability_matches_textstat() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("fixtures/textstat_readability.json"))
        .expect("the fixture");
    assert!(cases.len() >= 50, "the fixture shrank to {}", cases.len());

    let mut divergences: Vec<String> = Vec::new();

    for (i, case) in cases.iter().enumerate() {
        let got = Readability::of(&case.text);
        let mut note = |metric: &str, want: f64, got: f64| {
            if (want - got).abs() > TOLERANCE {
                let preview: String = case.text.chars().take(50).collect();
                divergences.push(format!(
                    "case {i} {metric}: textstat {want}, ours {got} \
                     (delta {:.3e}) — {preview:?}",
                    (want - got).abs()
                ));
            }
        };
        note(
            "flesch_reading_ease",
            case.flesch_reading_ease,
            got.flesch_reading_ease,
        );
        note(
            "flesch_kincaid_grade",
            case.flesch_kincaid_grade,
            got.flesch_kincaid_grade,
        );
        note("smog_index", case.smog_index, got.smog_index);
        note(
            "coleman_liau_index",
            case.coleman_liau_index,
            got.coleman_liau_index,
        );
        note(
            "automated_readability_index",
            case.automated_readability_index,
            got.automated_readability_index,
        );
        note("mcalpine_eflaw", case.mcalpine_eflaw, got.mcalpine_eflaw);
    }

    assert!(
        divergences.is_empty(),
        "{} divergence(s) from textstat:\n  {}",
        divergences.len(),
        divergences
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}
