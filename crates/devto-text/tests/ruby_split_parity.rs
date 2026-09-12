//! `ruby_word_count` against Ruby itself.
//!
//! Forem's reading time is `content.split(/\W+/).count`, and the semantics that matter are
//! Ruby's, not a reasonable person's: `\w` is ASCII-only whatever the encoding, a separator
//! run at position zero yields a leading empty field, and trailing empty fields are dropped.
//!
//! The expectations here were produced by running that expression in Ruby. Regenerate with
//! `scripts/refresh-parity.sh`.

use devto_text::forem::ruby_word_count;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Case {
    text: String,
    count: usize,
}

#[test]
fn word_counting_matches_ruby() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("fixtures/ruby_word_counts.json"))
        .expect("the ruby fixture");
    assert!(cases.len() >= 25, "the fixture shrank to {}", cases.len());

    let divergences: Vec<String> = cases
        .iter()
        .filter_map(|case| {
            let got = ruby_word_count(&case.text);
            (got != case.count).then(|| {
                format!(
                    "{:?}: ruby {}, ours {}",
                    case.text.chars().take(40).collect::<String>(),
                    case.count,
                    got
                )
            })
        })
        .collect();

    assert!(
        divergences.is_empty(),
        "{} of {} disagree with Ruby:\n  {}",
        divergences.len(),
        cases.len(),
        divergences.join("\n  ")
    );
}
