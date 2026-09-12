//! Forem's reading time against ground truth from the live API.
//!
//! Not a reimplementation checked against my reading of the Ruby: 30 published articles with
//! the `reading_time_minutes` dev.to itself reports for each. That is the only oracle that
//! settles it, and it is the one that caught the front-matter detail — a first attempt over
//! the raw body scored 25 of 30, and every miss was `+1` with a front matter block.

use devto_text::forem;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Article {
    id: i64,
    title: String,
    reading_time_minutes: usize,
    body_markdown: String,
}

#[test]
fn reading_time_matches_what_dev_to_reports() {
    let articles: Vec<Article> =
        serde_json::from_str(include_str!("fixtures/forem_reading_time.json"))
            .expect("the ground truth fixture");
    assert_eq!(articles.len(), 30);

    let mut divergences = Vec::new();
    for article in &articles {
        let got = forem::reading_time(&article.body_markdown);
        if got.minutes != article.reading_time_minutes {
            divergences.push(format!(
                "id {} — dev.to {} min, ours {} min ({} words): {:?}",
                article.id,
                article.reading_time_minutes,
                got.minutes,
                got.counted_words,
                article.title.chars().take(45).collect::<String>()
            ));
        }
    }

    assert!(
        divergences.is_empty(),
        "{} of {} articles disagree with dev.to:\n  {}",
        divergences.len(),
        articles.len(),
        divergences.join("\n  ")
    );
}

/// Stripping front matter is the step that took this from 25/30 to 30/30, so prove it is
/// still load-bearing rather than trusting that it is.
#[test]
fn skipping_the_front_matter_strip_would_regress() {
    let articles: Vec<Article> =
        serde_json::from_str(include_str!("fixtures/forem_reading_time.json"))
            .expect("the ground truth fixture");

    let without_strip = articles
        .iter()
        .filter(|a| {
            let words = forem::ruby_word_count(&a.body_markdown);
            let minutes = (words as f64 / forem::WORDS_PER_MINUTE).ceil() as usize;
            minutes == a.reading_time_minutes
        })
        .count();

    assert!(
        without_strip < articles.len(),
        "front matter no longer affects any article, so this test proves nothing"
    );
    assert_eq!(
        without_strip, 25,
        "counting the raw body should still match exactly 25 of 30"
    );
}
