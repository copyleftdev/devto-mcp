//! Splitting a dev.to article into the parts that are prose and the parts that are not.
//!
//! This is the step that decides whether every number above it means anything. A readability
//! score computed over an article's raw markdown is measuring the fenced code, the liquid
//! tags, the headings and the URLs as though they were sentences.
//!
//! Measured across the author's 30 published articles, counting code as prose inflates
//! **Coleman–Liau by 2.26 grade levels on average and by 6.88 at worst**, ARI by 1.71 and
//! Flesch–Kincaid by 0.84. Nobody reading "grade 13" would know it was computed over a shell
//! transcript.
//!
//! Liquid tags are removed before the markdown parser runs, because `{% embed … %}` is not
//! markdown and pulldown-cmark would hand it back as prose. Which tags were used is kept,
//! since that is worth reporting on its own.
//!
//! Two judgement calls, both deliberate. **Block quotes count as prose**, because a reader
//! reads them and they take time and effort even though the author did not write them.
//! **Headings do not**, because they are structure rather than sentences, and counting a
//! three-word heading as a sentence drags the average sentence length down and makes an
//! article score as easier than it reads. Both are counted separately so the split is
//! visible rather than assumed.
//!
//! The parse is CommonMark, while Forem renders with Redcarpet. They agree on everything
//! structural that matters here — fences, headings, lists, quotes, links — and disagree only
//! in corners that do not change a word count.

use std::collections::BTreeMap;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Serialize;

use crate::forem;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Document {
    /// Everything that is prose, joined — the text every metric should be computed over.
    pub prose: String,
    pub headings: Vec<Heading>,
    /// One entry per paragraph, for a length distribution.
    pub paragraphs: Vec<String>,
    /// Fenced and indented code, with the language where one was declared.
    pub code_blocks: Vec<CodeBlock>,
    pub inline_code_spans: usize,
    pub block_quotes: usize,
    pub list_items: usize,
    pub links: Vec<String>,
    pub images: usize,
    /// Liquid tag name to how many times it appears.
    pub liquid_tags: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub lines: usize,
    pub bytes: usize,
}

impl Document {
    /// Parse an article body. Front matter is stripped first, as Forem does.
    pub fn parse(body_markdown: &str) -> Self {
        let body = forem::strip_front_matter(body_markdown);
        let (without_liquid, liquid_tags) = strip_liquid_tags(&body);

        let mut document = Document {
            liquid_tags,
            ..Default::default()
        };
        document.walk(&without_liquid);
        document.prose = document.paragraphs.join("\n\n");
        document
    }

    fn walk(&mut self, markdown: &str) {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_TASKLISTS);

        let mut buffer = String::new();
        let mut heading_level: Option<u8> = None;
        let mut in_code_block: Option<Option<String>> = None;
        let mut code_text = String::new();
        // Prose inside a heading or a code fence is not body prose.
        let mut collecting_prose = false;

        for event in Parser::new_ext(markdown, options) {
            match event {
                Event::Start(Tag::Paragraph) => {
                    buffer.clear();
                    collecting_prose = true;
                }
                Event::End(TagEnd::Paragraph) => {
                    let text = buffer.trim().to_string();
                    if !text.is_empty() {
                        self.paragraphs.push(text);
                    }
                    buffer.clear();
                    collecting_prose = false;
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    buffer.clear();
                    heading_level = Some(heading_number(level));
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some(level) = heading_level.take() {
                        self.headings.push(Heading {
                            level,
                            text: buffer.trim().to_string(),
                        });
                    }
                    buffer.clear();
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    let language = match kind {
                        CodeBlockKind::Fenced(info) => {
                            let name = info.split_whitespace().next().unwrap_or_default();
                            (!name.is_empty()).then(|| name.to_string())
                        }
                        CodeBlockKind::Indented => None,
                    };
                    in_code_block = Some(language);
                    code_text.clear();
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(language) = in_code_block.take() {
                        self.code_blocks.push(CodeBlock {
                            language,
                            lines: code_text.lines().count(),
                            bytes: code_text.len(),
                        });
                    }
                    code_text.clear();
                }
                Event::Start(Tag::BlockQuote(_)) => self.block_quotes += 1,
                Event::Start(Tag::Item) => self.list_items += 1,
                Event::Start(Tag::Link { dest_url, .. }) => self.links.push(dest_url.to_string()),
                Event::Start(Tag::Image { .. }) => self.images += 1,
                Event::Code(_) => self.inline_code_spans += 1,
                Event::Text(text) => {
                    if in_code_block.is_some() {
                        code_text.push_str(&text);
                    } else if collecting_prose || heading_level.is_some() {
                        buffer.push_str(&text);
                    }
                }
                // A line break is a word boundary, not a sentence one, wherever it falls.
                //
                // There is no guard on this arm, and the two that were here were both dead.
                // `in_code_block.is_none()` cannot be false: pulldown-cmark delivers a fenced
                // block's contents as `Event::Text` with the newlines inside them and never
                // emits a break event within one. `collecting_prose || heading_level.is_some()`
                // cannot be observed either: `buffer` is cleared when a paragraph or heading
                // *opens*, so a space pushed outside one is always discarded before anything
                // accumulates behind it. Both were verified against the 30 published articles
                // and against list, quote and table cases, which is why they are gone rather
                // than excluded from the mutation gate.
                //
                // That second argument is the one to re-check if `buffer` ever stops being
                // cleared on open.
                Event::SoftBreak | Event::HardBreak => {
                    buffer.push(' ');
                }
                _ => {}
            }
        }
    }

    pub fn code_lines(&self) -> usize {
        self.code_blocks.iter().map(|b| b.lines).sum()
    }

    /// The languages declared on fenced blocks, most used first.
    pub fn code_languages(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for block in &self.code_blocks {
            if let Some(language) = &block.language {
                *counts.entry(language.to_lowercase()).or_default() += 1;
            }
        }
        let mut ordered: Vec<(String, usize)> = counts.into_iter().collect();
        ordered.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        ordered
    }

    /// How much of the article a prose metric actually measured.
    ///
    /// Reported with every score, because a readability figure without a denominator is the
    /// thing this module exists to prevent.
    pub fn prose_share(&self, body_markdown: &str) -> ProseShare {
        let whole = forem::ruby_word_count(&forem::strip_front_matter(body_markdown));
        let prose = forem::ruby_word_count(&self.prose);
        ProseShare {
            total_words: whole,
            prose_words: prose,
            percent_prose: if whole == 0 {
                0.0
            } else {
                100.0 * prose as f64 / whole as f64
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ProseShare {
    /// Words in the whole body, by Forem's own definition.
    pub total_words: usize,
    /// Words in the prose that the metrics were computed over.
    pub prose_words: usize,
    pub percent_prose: f64,
}

fn heading_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Remove `{% … %}` before the markdown parser sees it, recording what was there.
///
/// Liquid is not markdown: left in place, `{% embed https://… %}` is handed back as a
/// paragraph of prose and counted as five words of writing.
pub fn strip_liquid_tags(markdown: &str) -> (String, BTreeMap<String, usize>) {
    let mut out = String::with_capacity(markdown.len());
    let mut tags: BTreeMap<String, usize> = BTreeMap::new();
    let mut rest = markdown;

    while let Some(open) = rest.find("{%") {
        // The closing `%}` is searched for strictly *after* the opening `{%`. Searching from
        // the `{` lets `{%}` match its own `%` as the close, which makes the body slice run
        // backwards — and that panicked rather than degrading. An article containing `{%}`
        // took the whole server down with it.
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find("%}") else {
            break; // An unterminated `{%` is literal text, not a tag.
        };

        out.push_str(&rest[..open]);
        let name = after_open[..close]
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if !name.is_empty() {
            *tags.entry(name).or_default() += 1;
        }
        rest = &after_open[close + 2..];
    }

    // Whatever is left carries no further tag, including an unterminated one.
    out.push_str(rest);
    (out, tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTICLE: &str = r#"---
title: A Post
---

# The Heading

An opening paragraph with a [link](https://example.com) in it.

## A Subheading

```rust
fn main() {
    println!("hello");
}
```

A closing paragraph with `inline code` in it.

> A quotation.

- one
- two

{% embed https://example.com/x %}
{% youtube abc123 %}
"#;

    #[test]
    fn an_article_splits_into_its_parts() {
        let doc = Document::parse(ARTICLE);

        assert_eq!(doc.headings.len(), 2);
        assert_eq!(
            doc.headings[0],
            Heading {
                level: 1,
                text: "The Heading".into()
            }
        );
        assert_eq!(doc.headings[1].level, 2);

        // Three, not two: the block quote holds a paragraph of its own.
        assert_eq!(doc.paragraphs.len(), 3);
        assert_eq!(doc.code_blocks.len(), 1);
        assert_eq!(doc.code_blocks[0].language.as_deref(), Some("rust"));
        assert_eq!(doc.inline_code_spans, 1);
        assert_eq!(doc.block_quotes, 1);
        assert_eq!(doc.list_items, 2);
        assert_eq!(doc.links, vec!["https://example.com"]);
    }

    /// The whole point: code must not reach the prose that gets measured.
    #[test]
    fn code_never_reaches_the_prose() {
        let doc = Document::parse(ARTICLE);
        assert!(!doc.prose.contains("println"));
        assert!(!doc.prose.contains("fn main"));
        assert!(
            !doc.prose.contains("inline code"),
            "an inline code span is not prose either"
        );
        assert!(doc.prose.contains("opening paragraph"));
        assert!(doc.prose.contains("closing paragraph"));
    }

    /// A quotation is prose a reader reads, so it belongs in the measurement even though the
    /// author did not write it. Headings are the opposite: they are structure, and counting
    /// them as sentences shortens the average sentence and flatters the score. Both calls are
    /// deliberate, and the counts are reported separately so a reader can see them.
    #[test]
    fn quotations_are_prose_even_though_headings_are_not() {
        let doc = Document::parse(ARTICLE);
        assert!(
            doc.prose.contains("A quotation"),
            "a reader reads the quote"
        );
        assert_eq!(doc.block_quotes, 1, "and it is still counted on its own");
    }

    /// A heading is structure, not a sentence. Counting headings as prose shortens the
    /// average sentence and makes an article look easier than it reads.
    #[test]
    fn headings_are_structure_rather_than_prose() {
        let doc = Document::parse(ARTICLE);
        assert!(!doc.prose.contains("The Heading"));
        assert!(!doc.prose.contains("A Subheading"));
    }

    /// Images, code lines and the prose percentage were all unasserted: a mutant could count
    /// images downwards, report no code lines at all, or divide where it multiplies, and every
    /// test here still passed.
    #[test]
    fn images_and_code_lines_are_counted() {
        const BODY: &str = r#"# Title

![alt one](a.png) and ![alt two](b.png)

```rust
let a = 1;
let b = 2;
let c = 3;
```

```
plain
```
"#;
        let doc = Document::parse(BODY);
        assert_eq!(doc.images, 2);
        assert_eq!(doc.code_blocks.len(), 2);
        assert_eq!(
            doc.code_lines(),
            4,
            "three lines in one block and one in the other"
        );
    }

    /// A line break inside a paragraph is a word boundary. Without the space the two words
    /// either side of it fuse into one, which silently changes every count downstream.
    #[test]
    fn a_soft_break_inside_a_paragraph_becomes_a_space() {
        let doc = Document::parse("A paragraph whose sentence\nis split across two source lines.");
        assert_eq!(
            doc.prose,
            "A paragraph whose sentence is split across two source lines."
        );
        assert!(
            !doc.prose.contains("sentenceis"),
            "the break fused two words"
        );
    }

    /// The same for a heading, which collects its text down the other branch of that guard.
    /// It has to be a setext heading: an ATX one cannot span lines to begin with.
    #[test]
    fn a_soft_break_inside_a_heading_becomes_a_space() {
        let doc = Document::parse("A heading\nsplit over two lines\n=====\n\nProse here.\n");
        assert_eq!(doc.headings.len(), 1);
        assert_eq!(doc.headings[0].text, "A heading split over two lines");
        assert_eq!(doc.prose, "Prose here.", "the heading is not prose");
    }

    /// The percentage is the denominator the whole crate exists to report, and nothing pinned
    /// it: `100 * prose / whole` and `100 / prose / whole` both type-check.
    #[test]
    fn the_prose_percentage_is_a_percentage() {
        const BODY: &str = r#"# Title

Alpha beta gamma delta.

```
one two three four five six
```
"#;
        let share = Document::parse(BODY).prose_share(BODY);
        assert_eq!(share.prose_words, 4);
        assert_eq!(share.total_words, 12);
        assert!(
            (share.percent_prose - 33.3333).abs() < 0.001,
            "4 of 12 words is 33.33%, got {}",
            share.percent_prose
        );
    }

    #[test]
    fn liquid_tags_are_removed_and_counted() {
        let doc = Document::parse(ARTICLE);
        assert_eq!(doc.liquid_tags.get("embed"), Some(&1));
        assert_eq!(doc.liquid_tags.get("youtube"), Some(&1));
        assert!(!doc.prose.contains("embed"));
        assert!(!doc.prose.contains("youtube"));
    }

    #[test]
    fn a_repeated_liquid_tag_is_counted_each_time() {
        let (text, tags) = strip_liquid_tags("{% tweet 1 %} and {% tweet 2 %} and {% gist x %}");
        assert_eq!(tags.get("tweet"), Some(&2));
        assert_eq!(tags.get("gist"), Some(&1));
        assert_eq!(text.trim(), "and  and");
    }

    /// `{%}` used to panic: the close was searched for from the `{`, so the `%` of the opening
    /// delimiter matched as the closing one and the body slice ran backwards. Any article body
    /// containing those three characters took the server down. The property test above did not
    /// find it — random text does not produce `{%}` — and neither did any example here.
    #[test]
    fn a_degenerate_liquid_tag_does_not_panic() {
        for input in ["{%}", "a{%}b", "{%", "{", "{%%}", "café {%} ☕"] {
            let (text, _) = strip_liquid_tags(input);
            assert!(
                text.is_char_boundary(0),
                "{input:?} produced something unusable"
            );
        }
        // None of them is a terminated tag, so each is literal text — except `{%%}`, which is
        // a terminated tag with no name.
        assert_eq!(strip_liquid_tags("{%}").0, "{%}");
        assert_eq!(strip_liquid_tags("a{%}b").0, "a{%}b");
        assert_eq!(strip_liquid_tags("café {%} ☕").0, "café {%} ☕");
        assert_eq!(strip_liquid_tags("{%%}"), (String::new(), BTreeMap::new()));
    }

    /// A `{` as the final byte is the other end of the same arithmetic: the opening check has
    /// to look one byte ahead without running off the end.
    #[test]
    fn a_trailing_brace_is_not_read_past() {
        assert_eq!(strip_liquid_tags("text{").0, "text{");
        assert_eq!(strip_liquid_tags("{").0, "{");
        assert_eq!(strip_liquid_tags("{% gist x %}{").0, "{");
    }

    #[test]
    fn an_unterminated_liquid_tag_is_left_alone() {
        let (text, tags) = strip_liquid_tags("{% embed never closed");
        assert_eq!(text, "{% embed never closed");
        assert!(tags.is_empty());
    }

    #[test]
    fn front_matter_does_not_become_prose() {
        let doc = Document::parse(ARTICLE);
        assert!(!doc.prose.contains("title:"));
    }

    #[test]
    fn the_prose_share_says_how_much_was_measured() {
        let doc = Document::parse(ARTICLE);
        let share = doc.prose_share(ARTICLE);
        assert!(share.prose_words > 0);
        assert!(share.prose_words < share.total_words, "code was excluded");
        assert!(
            share.percent_prose > 0.0 && share.percent_prose < 100.0,
            "{share:?}"
        );
    }

    #[test]
    fn an_empty_body_yields_an_empty_document() {
        let doc = Document::parse("");
        assert!(doc.prose.is_empty());
        assert!(doc.headings.is_empty());
        assert!(doc.paragraphs.is_empty());
        assert_eq!(doc.prose_share("").percent_prose, 0.0);
    }

    #[test]
    fn code_languages_are_ranked_by_use() {
        let markdown = "```rust\na\n```\n\n```rust\nb\n```\n\n```python\nc\n```\n\n```\nd\n```";
        let doc = Document::parse(markdown);
        assert_eq!(doc.code_blocks.len(), 4);
        assert_eq!(
            doc.code_languages(),
            vec![("rust".to_string(), 2), ("python".to_string(), 1)],
            "an unlabelled fence contributes no language"
        );
    }

    #[hegel::test]
    fn parsing_arbitrary_text_never_panics(tc: hegel::TestCase) {
        let body = tc.draw(hegel::generators::text().max_size(500));
        let doc = Document::parse(&body);
        let share = doc.prose_share(&body);
        assert!(share.prose_words <= share.total_words.max(share.prose_words));
        assert!(share.percent_prose >= 0.0);
        assert!(doc.headings.iter().all(|h| (1..=6).contains(&h.level)));
    }

    #[hegel::test]
    fn stripping_liquid_tags_preserves_everything_else(tc: hegel::TestCase) {
        let text = tc.draw(hegel::generators::text().max_size(200));
        let (stripped, _) = strip_liquid_tags(&text);
        if !text.contains("{%") {
            assert_eq!(stripped, text, "nothing to strip, nothing changed");
        }
        assert!(stripped.len() <= text.len());
    }

    /// Arbitrary text never produces `{%}`, so the property above ran for a long time without
    /// reaching the case that panicked. This one draws from the delimiters themselves, which
    /// is where the interesting inputs live.
    #[hegel::test]
    fn arbitrary_delimiter_soup_neither_panics_nor_grows(tc: hegel::TestCase) {
        let soup: String = tc
            .draw(
                hegel::generators::vecs(hegel::generators::sampled_from(
                    "{}%a ☕".chars().collect::<Vec<_>>(),
                ))
                .max_size(40),
            )
            .into_iter()
            .collect();
        let (stripped, tags) = strip_liquid_tags(&soup);
        assert!(stripped.len() <= soup.len(), "{soup:?}");
        if !soup.contains("{%") {
            assert_eq!(stripped, soup, "{soup:?}");
            assert!(tags.is_empty(), "{soup:?}");
        }
    }
}
