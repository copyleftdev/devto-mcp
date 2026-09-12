//! Offline validation for the DEV (Forem) authorship API.
//!
//! Forem's real rules live in Rails validators, Pundit policies and a Rack::Attack
//! initializer. None of them appear in the OpenAPI description a code generator would
//! consume, so a client built from the spec discovers them one 422 at a time — at a
//! budget of one write per second.
//!
//! This crate mirrors those rules as a pure function. No I/O, no async, and no ambient
//! clock: `validate` takes the current time as an argument so the time-dependent rules
//! (`published_at` must be future or within the last fifteen minutes) are testable at
//! their boundaries instead of flaky.
//!
//! ```
//! use devto_core::{validate, Context, Draft, RuleId};
//!
//! let mut draft = Draft::new("Writing a Forem client", "# Notes\n");
//! draft.tags = vec!["machine-learning".into()];
//!
//! let report = validate(&draft, 1_757_000_000, &Context::create());
//! assert!(!report.is_sendable());
//! assert!(report.has_rule(RuleId::TagInvalidCharacters));
//! ```
//!
//! Rules are derived from `forem/forem` at commit `ac54b3b` (2026-09-10); each one cites
//! the method that sets it. See `docs/FINDINGS.md` for the research behind them.

#![forbid(unsafe_code)]

pub mod draft;
pub mod finding;
pub mod frontmatter;
pub mod limits;
pub mod tags;
pub mod urls;
pub mod validate;

pub use draft::{AiDisclosure, ArticleType, Context, Draft, Operation, RecentTitle, UnixSeconds};
pub use finding::{Field, Finding, Report, RuleId, Severity};
pub use frontmatter::FrontMatter;
pub use validate::validate;
