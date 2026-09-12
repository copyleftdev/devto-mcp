//! A rate-paced, version-correct HTTP client for the DEV (Forem) API.
//!
//! Three things this crate refuses to let a caller get wrong, because each one fails
//! quietly rather than loudly:
//!
//! 1. **The version header.** `Accept: application/vnd.forem.api-v1+json` selects API V1.
//!    Without it the identical URL serves the deprecated V0 controller — which does not
//!    route unpublish, semantic search, reactions or surveys — and returns 200. Every
//!    request sends the header, and every response is checked for the `Warning: 299`
//!    stamp Forem puts on V0 replies, so a lost header is an error rather than a silently
//!    wrong contract.
//!
//! 2. **The budget.** dev.to sends no `X-RateLimit-*` headers, so the ceilings have to be
//!    modelled locally: 3 reads/second, 30 reads/minute, 1 write/second, counted against
//!    the IP *and* the API key simultaneously. See [`limiter`].
//!
//! 3. **The account's email.** `/api/users/me` returns it. [`models::Me`] does not
//!    deserialize it, so it cannot reach a model, a log or a cache.
//!
//! The HTTP call itself sits behind the [`transport::Transport`] trait and the clock
//! behind [`transport::Clock`], so pacing, caching and error mapping are all tested
//! against scripted responses rather than against a live budget of 30 reads a minute.
//!
//! ```no_run
//! use devto_client::{Config, DevtoClient, MyArticleStatus};
//!
//! let mut client = DevtoClient::new(Config {
//!     api_key: std::env::var("DEVTO_API_KEY").ok(),
//!     ..Config::default()
//! });
//!
//! let me = client.me()?;
//! let drafts = client.my_articles(MyArticleStatus::Unpublished, None, None)?;
//! println!("{} has {} drafts", me.username, drafts.len());
//! # Ok::<(), devto_client::Error>(())
//! ```

#![forbid(unsafe_code)]

pub mod cache;
pub mod client;
pub mod error;
pub mod limiter;
pub mod models;
pub mod net;
pub mod transport;

pub use client::{ArticleQuery, Config, DEFAULT_BASE_URL, DevtoClient, MyArticleStatus, V1_ACCEPT};
pub use error::{Error, Result};
pub use limiter::{Budget, Decision, RateLimiter, RequestKind};
pub use models::*;
pub use net::{SystemClock, UreqTransport};
pub use transport::{Clock, HttpRequest, HttpResponse, Method, Transport};
