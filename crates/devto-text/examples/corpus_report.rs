//! What the analysis says about the author's own 30 published articles.
//!
//!   cargo run -p devto-text --example corpus_report

use devto_text::{Document, Readability, forem};
use serde::Deserialize;

#[derive(Deserialize)]
struct Article {
    title: String,
    reading_time_minutes: usize,
    body_markdown: String,
}

fn main() {
    let articles: Vec<Article> =
        serde_json::from_str(include_str!("../tests/fixtures/forem_reading_time.json")).unwrap();

    println!(
        "{:<44} {:>5} {:>5} {:>6} {:>6} {:>6}",
        "article", "devto", "prose", "prose%", "FK", "FK raw"
    );

    let (mut overstated, mut total_words, mut prose_words) = (0usize, 0usize, 0usize);
    let mut gaps = Vec::new();

    for a in &articles {
        let doc = Document::parse(&a.body_markdown);
        let share = doc.prose_share(&a.body_markdown);
        let prose_minutes = forem::reading_time(&doc.prose).minutes;
        let fk_prose = Readability::of(&doc.prose).flesch_kincaid_grade;
        let fk_raw = Readability::of(&a.body_markdown).flesch_kincaid_grade;

        total_words += share.total_words;
        prose_words += share.prose_words;
        let gap = a.reading_time_minutes as i64 - prose_minutes as i64;
        if gap > 0 {
            overstated += 1;
        }
        gaps.push(gap);

        println!(
            "{:<44} {:>5} {:>5} {:>5.0}% {:>6.1} {:>6.1}",
            a.title.chars().take(43).collect::<String>(),
            a.reading_time_minutes,
            prose_minutes,
            share.percent_prose,
            fk_prose,
            fk_raw
        );
    }

    println!();
    println!(
        "articles whose reading time dev.to overstates: {overstated}/{}",
        articles.len()
    );
    println!(
        "non-prose share of counted words: {:.1}%",
        100.0 - 100.0 * prose_words as f64 / total_words as f64
    );
    println!(
        "largest overstatement: {} minutes",
        gaps.iter().max().unwrap()
    );
}
