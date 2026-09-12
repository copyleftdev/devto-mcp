//! Exercise the client against the real dev.to API.
//!
//! Not part of the gate: it spends the account's live read budget and needs a key. Run it
//! when the transport changes, to confirm the shapes the unit tests assert are still the
//! shapes dev.to sends.
//!
//! ```sh
//! set -a && . ~/.creds/dev2.env && set +a
//! DEVTO_API_KEY="$API_KEY" cargo run -p devto-client --example live_check
//! ```
//!
//! Ten reads, which is a third of the per-minute budget. The client paces itself.

use devto_client::{Config, DevtoClient, Error, MyArticleStatus};

fn main() {
    let api_key = std::env::var("DEVTO_API_KEY").ok();
    if api_key.is_none() {
        eprintln!("DEVTO_API_KEY is not set — running the anonymous checks only.");
    }

    let mut client = DevtoClient::new(Config {
        api_key,
        ..Config::default()
    });

    let mut failures = 0;
    let mut check = |name: &str, result: Result<String, Error>| match result {
        Ok(summary) => println!("  ok    {name:<22} {summary}"),
        Err(error) => {
            failures += 1;
            println!("  FAIL  {name:<22} {error}");
        }
    };

    println!("base: {}", client.base_url());
    println!("authenticated: {}", client.is_authenticated());
    println!();

    check(
        "tags",
        client
            .tags(None, Some(5))
            .map(|tags| format!("{} tags, first = {}", tags.len(), tags[0].name)),
    );

    check(
        "articles (feed)",
        client
            .articles(devto_client::ArticleQuery {
                tag: Some("rust"),
                per_page: Some(3),
                ..Default::default()
            })
            .map(|articles| {
                format!(
                    "{} articles, first = {:?}",
                    articles.len(),
                    articles.first().map(|a| a.title.clone())
                )
            }),
    );

    if client.is_authenticated() {
        check(
            "users/me",
            client
                .me()
                .map(|me| format!("{} (id {}), joined {}", me.username, me.id, me.joined_at)),
        );

        check(
            "articles/me/all",
            client
                .my_articles(MyArticleStatus::All, None, Some(5))
                .map(|articles| {
                    let drafts = articles.iter().filter(|a| !a.published).count();
                    let views: i64 = articles.iter().map(|a| a.page_views_count).sum();
                    format!(
                        "{} articles ({drafts} unpublished), {views} views across them",
                        articles.len()
                    )
                }),
        );

        check(
            "follows/tags",
            client.followed_tags().map(|tags| {
                let weighted = tags.iter().filter(|t| t.points != 0.0).count();
                format!(
                    "{} followed, {weighted} with a non-default weight",
                    tags.len()
                )
            }),
        );

        check(
            "semantic_search",
            client
                .semantic_search("rust ownership", None, Some(3), None)
                .map(|hits| {
                    let ordered_by_similarity = hits
                        .windows(2)
                        .all(|w| w[0].similarity >= w[1].similarity);
                    format!(
                        "{} hits, tags on first = {:?}, sorted by similarity = {ordered_by_similarity}",
                        hits.len(),
                        hits.first().map(|h| h.tags()).unwrap_or_default()
                    )
                }),
        );

        check(
            "analytics/dashboard",
            client
                .analytics_dashboard(None, None, None, None)
                .map(|dashboard| {
                    format!(
                        "{} views, {} reactions, {} comments, since {}",
                        dashboard.totals.page_views.total,
                        dashboard.totals.reactions.total,
                        dashboard.totals.comments.total,
                        dashboard.start_date_floor.as_deref().unwrap_or("?")
                    )
                }),
        );
    }

    // The cache has to earn its place: a repeat of a read already made must not go out.
    let before = client.cache_stats();
    let _ = client.tags(None, Some(5));
    let after = client.cache_stats();
    check(
        "cache (repeat read)",
        if after.0 == before.0 + 1 {
            Ok("served from cache, no request spent".to_string())
        } else {
            Err(Error::Transport("the repeat read hit the network".into()))
        },
    );

    let budget = client.budget();
    println!();
    println!(
        "budget left: {} reads this second, {} this minute, {} writes this second",
        budget.reads_this_second, budget.reads_this_minute, budget.writes_this_second
    );
    println!(
        "cache: {} hits, {} misses",
        client.cache_stats().0,
        client.cache_stats().1
    );

    if failures > 0 {
        eprintln!("\n{failures} check(s) failed");
        std::process::exit(1);
    }
    println!("\nall live checks passed");
}
