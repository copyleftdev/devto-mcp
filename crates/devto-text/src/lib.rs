//! Text analysis for DEV (Forem) articles.
//!
//! Two things this crate is careful about, because both are places a plausible-looking
//! implementation is quietly wrong.
//!
//! **It measures prose.** A readability score computed over a dev.to article is measuring
//! the fenced code, the liquid tags and the URLs as though they were sentences. On a real
//! article, stripping them moves Coleman–Liau by 1.45 grade levels. Every result here states
//! what share of the document it measured.
//!
//! **Its numbers are checked against a reference.** The counts and formulas reproduce
//! `textstat` 0.7.13, and the hyphenation reproduces `pyphen`, both verified against fixtures
//! generated from those tools rather than against our own expectations. Forem's own
//! `reading_time` is reproduced too, verified against 30 published articles.
#![forbid(unsafe_code)]

pub mod counts;
pub mod forem;
pub mod hyphen;
pub mod readability;
pub mod syllables;

pub use counts::Counts;
pub use forem::ReadingTime;
pub use readability::Readability;
