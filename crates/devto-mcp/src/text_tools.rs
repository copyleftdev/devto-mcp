//! The three text-analysis tools.
//!
//! Each reports the denominator it used. A readability score without one is measuring an
//! article's code as though it were writing, and across this author's own published work
//! that inflates Coleman–Liau by 2.26 grade levels on average and 6.88 at worst.
//!
//! All three are free: no network request, no rate-limit budget. Only the optional
//! `article_id` argument costs a read, and only because the body has to come from somewhere.

use devto_text::{Document, Readability, forem};
use serde_json::{Value, json};

use crate::tools::ToolOutcome;

pub fn definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "analyze_readability",
            "title": "How hard is this article to read",
            "description":
                "Readability over an article's prose, with the share of the document that \
                 was measured stated alongside it.\n\n\
                 dev.to articles are part writing and part code, and a score computed over \
                 the whole thing measures the shell transcripts as sentences. This excludes \
                 fenced and inline code, liquid tags, headings and link targets first, and \
                 reports both figures so the difference is visible — it is routinely two or \
                 three grade levels.\n\n\
                 Six metrics, reproducing Python's textstat exactly: Flesch Reading Ease, \
                 Flesch-Kincaid Grade, SMOG, Coleman-Liau, Automated Readability Index and \
                 McAlpine EFLAW. Costs no request unless you pass article_id.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "body_markdown": {"type": "string", "description": "The article source. Front matter and liquid tags are handled."},
                    "article_id": {"type": "integer", "description": "Alternative to body_markdown: fetch one of your own articles first. Costs one read."}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "analyze_structure",
            "title": "The shape of an article",
            "description":
                "Heading hierarchy, paragraph lengths, how much of the article is code, link \
                 density, and which liquid tags it uses.\n\n\
                 Answers the questions a reader feels before they can name them: does the \
                 article have a spine, are the paragraphs walls, is it a tutorial with prose \
                 attached or an essay with examples. Reports skipped heading levels, which \
                 break both the outline and screen-reader navigation. Costs no request unless \
                 you pass article_id.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "body_markdown": {"type": "string"},
                    "article_id": {"type": "integer", "description": "Costs one read."}
                },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "forem_reading_time",
            "title": "What dev.to will say, and what it should say",
            "description":
                "The reading time dev.to will display, reproduced exactly from Forem's own \
                 formula, alongside the figure the prose alone would give.\n\n\
                 dev.to counts every word-like token as reading: fenced code, liquid tags, \
                 URLs, headings. On a code-heavy post the published estimate can be minutes \
                 too long, which sets a reader's expectation before they start. Verified \
                 against the reading time dev.to reports for 30 real articles. Costs no \
                 request unless you pass article_id.",
            "inputSchema": {
                "$schema": "https://json-schema.org/draft/2020-12/schema",
                "type": "object",
                "properties": {
                    "body_markdown": {"type": "string"},
                    "article_id": {"type": "integer", "description": "Costs one read."}
                },
                "additionalProperties": false
            }
        }),
    ]
}

pub fn names() -> Vec<&'static str> {
    vec![
        "analyze_readability",
        "analyze_structure",
        "forem_reading_time",
    ]
}

pub fn is_text_tool(name: &str) -> bool {
    names().contains(&name)
}

/// Run one of the text tools over a body that the caller has already resolved.
pub fn call(name: &str, body: &str) -> ToolOutcome {
    match name {
        "analyze_readability" => analyze_readability(body),
        "analyze_structure" => analyze_structure(body),
        "forem_reading_time" => reading_time(body),
        other => ToolOutcome::failed(
            format!("unknown text tool: {other}"),
            format!("Call one of: {}.", names().join(", ")),
        ),
    }
}

fn metrics(readability: &Readability) -> Value {
    json!({
        "flesch_reading_ease": round2(readability.flesch_reading_ease),
        "flesch_kincaid_grade": round2(readability.flesch_kincaid_grade),
        "smog_index": round2(readability.smog_index),
        "coleman_liau_index": round2(readability.coleman_liau_index),
        "automated_readability_index": round2(readability.automated_readability_index),
        "mcalpine_eflaw": round2(readability.mcalpine_eflaw),
    })
}

/// Two decimals. The formulas carry more precision than the inputs justify, and a grade
/// level quoted to six places invites more confidence than a readability score deserves.
pub(crate) fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn analyze_readability(body: &str) -> ToolOutcome {
    let document = Document::parse(body);
    let share = document.prose_share(body);

    if share.prose_words == 0 {
        return ToolOutcome::ok(json!({
            "measured": false,
            "reason": "no prose to measure — the article is all code, embeds or headings",
            "prose": { "words": 0, "share_percent": 0.0 },
        }));
    }

    let prose = Readability::of(&document.prose);
    let whole = Readability::of(body);

    ToolOutcome::ok(json!({
        "measured": true,
        "prose": {
            "words": share.prose_words,
            "total_words": share.total_words,
            "share_percent": round2(share.percent_prose),
            "excluded": {
                "code_blocks": document.code_blocks.len(),
                "code_lines": document.code_lines(),
                "inline_code_spans": document.inline_code_spans,
                "headings": document.headings.len(),
                "liquid_tags": document.liquid_tags.values().sum::<usize>(),
            }
        },
        "readability": metrics(&prose),
        "reading_ease_band": prose.reading_ease_band(),
        "sentence_length_words": round2(prose.words_per_sentence),
        "syllables_per_word": round2(prose.syllables_per_word),
        "if_code_were_counted_as_prose": {
            "note": "What the same metrics give over the raw markdown. The gap is how much \
                     the code inflates the apparent difficulty.",
            "readability": metrics(&whole),
            "coleman_liau_inflation": round2(
                whole.coleman_liau_index - prose.coleman_liau_index
            ),
            "flesch_kincaid_inflation": round2(
                whole.flesch_kincaid_grade - prose.flesch_kincaid_grade
            ),
        },
        "reference": "Metrics reproduce Python textstat 0.7.13 exactly, verified against it \
                      over 58 documents.",
    }))
}

fn analyze_structure(body: &str) -> ToolOutcome {
    let document = Document::parse(body);
    let share = document.prose_share(body);

    let mut paragraph_words: Vec<usize> = document
        .paragraphs
        .iter()
        .map(|p| forem::ruby_word_count(p))
        .collect();
    paragraph_words.sort_unstable();

    let median = if paragraph_words.is_empty() {
        0
    } else {
        paragraph_words[paragraph_words.len() / 2]
    };
    let longest = paragraph_words.last().copied().unwrap_or(0);

    let headings: Vec<Value> = document
        .headings
        .iter()
        .map(|h| json!({ "level": h.level, "text": h.text }))
        .collect();

    ToolOutcome::ok(json!({
        "headings": {
            "count": document.headings.len(),
            "outline": headings,
            "skipped_levels": skipped_heading_levels(&document),
        },
        "paragraphs": {
            "count": document.paragraphs.len(),
            "median_words": median,
            "longest_words": longest,
            "over_150_words": paragraph_words.iter().filter(|&&w| w > 150).count(),
        },
        "code": {
            "blocks": document.code_blocks.len(),
            "lines": document.code_lines(),
            "inline_spans": document.inline_code_spans,
            "languages": document.code_languages()
                .into_iter()
                .map(|(name, count)| json!({ "language": name, "blocks": count }))
                .collect::<Vec<_>>(),
            "unlabelled_blocks": document.code_blocks.iter()
                .filter(|b| b.language.is_none()).count(),
        },
        "links": { "count": document.links.len(), "urls": document.links },
        "images": document.images,
        "list_items": document.list_items,
        "block_quotes": document.block_quotes,
        "liquid_tags": document.liquid_tags,
        "prose_share_percent": round2(share.percent_prose),
    }))
}

/// Heading levels that jump by more than one — `##` straight to `####`. It breaks the
/// document outline and screen-reader navigation, and nothing in dev.to's editor warns.
fn skipped_heading_levels(document: &Document) -> Vec<Value> {
    let mut skips = Vec::new();
    let mut previous: Option<u8> = None;
    for heading in &document.headings {
        if let Some(previous) = previous
            && heading.level > previous + 1
        {
            skips.push(json!({
                "from_level": previous,
                "to_level": heading.level,
                "at_heading": heading.text,
            }));
        }
        previous = Some(heading.level);
    }
    skips
}

fn reading_time(body: &str) -> ToolOutcome {
    let published = forem::reading_time(body);
    let document = Document::parse(body);
    let prose = forem::reading_time(&document.prose);
    let share = document.prose_share(body);

    let overstated = published.minutes.saturating_sub(prose.minutes);

    ToolOutcome::ok(json!({
        "dev_to_will_display": published.minutes,
        "counted_words": published.counted_words,
        "prose_only": {
            "minutes": prose.minutes,
            "words": prose.counted_words,
        },
        "overstated_by_minutes": overstated,
        "prose_share_percent": round2(share.percent_prose),
        "how_dev_to_counts": "Forem splits the body on non-word characters and divides by \
                              275 words per minute, rounding up. Fenced code, liquid tags, \
                              URLs and headings all count as words.",
        "note": if overstated > 0 {
            format!(
                "dev.to will tell readers {} minutes; the prose alone is {}. A reader who \
                 skims the code finishes sooner than the label promises.",
                published.minutes, prose.minutes
            )
        } else {
            "The published estimate and the prose agree.".to_string()
        },
        "reference": "Verified against the reading time dev.to reports for 30 real articles.",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTICLE: &str = r#"---
title: A Post
---

# Heading One

An opening paragraph that runs on for a little while so that there is something to measure
here, with enough words in it to make a sentence worth counting.

```rust
fn main() {
    let x = compute_something_complicated();
    println!("{x}");
}
```

#### A Skipped Level

Another paragraph of ordinary prose, written plainly.

{% embed https://example.com %}
"#;

    fn structured(name: &str, body: &str) -> Value {
        let outcome = call(name, body);
        assert!(!outcome.is_error, "{name} failed: {}", outcome.structured);
        outcome.structured
    }

    #[test]
    fn every_text_tool_is_defined_and_dispatches() {
        let defined: Vec<String> = definitions()
            .iter()
            .map(|d| d["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(defined.len(), 3);
        for name in names() {
            assert!(defined.contains(&name.to_string()), "{name} not defined");
            assert!(is_text_tool(name));
            assert!(!call(name, ARTICLE).is_error, "{name} did not dispatch");
        }
        assert!(!is_text_tool("validate_draft"));
    }

    /// The denominator is the point. A score without it is measuring code as writing.
    #[test]
    fn readability_states_what_share_it_measured() {
        let result = structured("analyze_readability", ARTICLE);
        assert_eq!(result["measured"], json!(true));

        let prose = &result["prose"];
        assert!(prose["words"].as_u64().unwrap() > 0);
        assert!(prose["words"].as_u64() < prose["total_words"].as_u64());
        let share = prose["share_percent"].as_f64().unwrap();
        assert!(share > 0.0 && share < 100.0, "share was {share}");
        assert_eq!(prose["excluded"]["code_blocks"], json!(1));
        assert_eq!(prose["excluded"]["liquid_tags"], json!(1));
    }

    #[test]
    fn readability_shows_what_counting_code_would_have_cost() {
        let result = structured("analyze_readability", ARTICLE);
        let inflation = result["if_code_were_counted_as_prose"]["coleman_liau_inflation"]
            .as_f64()
            .unwrap();
        assert!(
            inflation > 0.0,
            "code should make the article look harder, got {inflation}"
        );
    }

    /// An article with no prose at all must say so rather than return a score over nothing.
    #[test]
    fn an_article_with_no_prose_reports_that_rather_than_a_number() {
        let result = structured("analyze_readability", "```rust\nfn main() {}\n```");
        assert_eq!(result["measured"], json!(false));
        assert!(result["reason"].as_str().unwrap().contains("all code"));
        assert!(result.get("readability").is_none());
    }

    #[test]
    fn structure_reports_the_outline_and_its_gaps() {
        let result = structured("analyze_structure", ARTICLE);
        assert_eq!(result["headings"]["count"], json!(2));

        let skips = result["headings"]["skipped_levels"].as_array().unwrap();
        assert_eq!(skips.len(), 1, "h1 to h4 is a skip");
        assert_eq!(skips[0]["from_level"], json!(1));
        assert_eq!(skips[0]["to_level"], json!(4));
    }

    #[test]
    fn a_clean_outline_reports_no_skips() {
        let result = structured(
            "analyze_structure",
            "# One\n\ntext\n\n## Two\n\ntext\n\n### Three\n\ntext",
        );
        assert!(
            result["headings"]["skipped_levels"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn structure_reports_the_code_it_found() {
        let result = structured("analyze_structure", ARTICLE);
        assert_eq!(result["code"]["blocks"], json!(1));
        assert_eq!(result["code"]["languages"][0]["language"], json!("rust"));
        assert_eq!(result["liquid_tags"]["embed"], json!(1));
    }

    #[test]
    fn reading_time_reports_both_figures_and_the_gap() {
        let result = structured("forem_reading_time", ARTICLE);
        let published = result["dev_to_will_display"].as_u64().unwrap();
        let prose = result["prose_only"]["minutes"].as_u64().unwrap();
        assert!(published >= prose);
        assert_eq!(
            result["overstated_by_minutes"].as_u64().unwrap(),
            published - prose
        );
        assert!(
            result["how_dev_to_counts"]
                .as_str()
                .unwrap()
                .contains("275")
        );
    }

    #[test]
    fn an_empty_body_does_not_panic_or_invent_a_score() {
        for name in names() {
            let outcome = call(name, "");
            assert!(!outcome.is_error, "{name} errored on empty input");
        }
        assert_eq!(
            structured("forem_reading_time", "")["dev_to_will_display"],
            json!(0)
        );
    }

    /// The six scores are the entire point of the tool, and until this test existed nothing
    /// asserted their values: a mutant replacing the whole payload with `null` passed every
    /// other assertion in this module, because they all check the *denominator* instead.
    #[test]
    fn every_readability_score_is_reported_as_a_rounded_number() {
        let result = structured("analyze_readability", ARTICLE);

        for section in ["readability", "if_code_were_counted_as_prose"] {
            let scores = if section == "readability" {
                &result["readability"]
            } else {
                &result[section]["readability"]
            };
            for key in [
                "flesch_reading_ease",
                "flesch_kincaid_grade",
                "smog_index",
                "coleman_liau_index",
                "automated_readability_index",
                "mcalpine_eflaw",
            ] {
                let value = scores[key]
                    .as_f64()
                    .unwrap_or_else(|| panic!("{section}.{key} absent or not a number"));
                assert!(value.is_finite(), "{section}.{key} was {value}");
                assert_eq!(value, round2(value), "{section}.{key} is not rounded");
            }
        }

        // Ordinary English prose, not a placeholder that happens to be finite.
        let grade = result["readability"]["flesch_kincaid_grade"]
            .as_f64()
            .unwrap();
        assert!((1.0..20.0).contains(&grade), "grade level was {grade}");
    }

    /// Four paragraphs, not three: with three, the middle index is 1 whether the length is
    /// halved or taken modulo two, and the assertion would hold for the wrong reason.
    #[test]
    fn the_median_paragraph_is_the_middle_one_by_length() {
        let paragraphs = [
            "word ".repeat(16),
            "word ".repeat(2),
            "word ".repeat(8),
            "word ".repeat(4),
        ];
        let body = paragraphs.join("\n\n");
        let result = structured("analyze_structure", &body);

        let found = &result["paragraphs"];
        assert_eq!(found["count"], json!(4));
        assert_eq!(
            found["median_words"],
            json!(8),
            "sorted lengths are [2, 4, 8, 16] and the median index is 2"
        );
        assert_eq!(found["longest_words"], json!(16));
        assert_eq!(found["over_150_words"], json!(0));
    }

    #[test]
    fn a_single_paragraph_is_its_own_median() {
        let result = structured("analyze_structure", &"word ".repeat(5));
        assert_eq!(result["paragraphs"]["count"], json!(1));
        assert_eq!(result["paragraphs"]["median_words"], json!(5));
    }

    #[test]
    fn scores_are_rounded_to_two_places() {
        assert_eq!(round2(10.336_666), 10.34);
        assert_eq!(round2(10.334_444), 10.33);
        assert_eq!(round2(-6.806), -6.81, "negatives round away from zero");
        assert_eq!(round2(-6.801), -6.8);
        assert_eq!(round2(0.0), 0.0);
        assert_eq!(round2(12.0), 12.0, "a whole number is left alone");
    }
}
