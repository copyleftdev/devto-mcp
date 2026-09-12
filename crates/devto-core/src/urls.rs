//! URL rules. Forem validates these with its own `UrlValidator`, which is stricter than
//! "parses as a URL": it pins the scheme set per field and rejects local hosts.

use crate::limits::LOCAL_HOSTS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlIssue {
    Whitespace,
    BadScheme,
    NoHost,
    LocalHost,
}

/// Check an absolute URL the way Forem's validator does.
///
/// `schemes` is the allowlist for this field: canonical URLs and cover images take
/// `["https", "http"]`, `video_source_url` takes `["https"]` alone.
pub fn check(url: &str, schemes: &[&str], reject_local: bool) -> Vec<UrlIssue> {
    let mut issues = Vec::new();

    if url.chars().any(char::is_whitespace) {
        issues.push(UrlIssue::Whitespace);
    }

    let Some((scheme, rest)) = split_scheme(url) else {
        issues.push(UrlIssue::BadScheme);
        return issues;
    };

    if !schemes.iter().any(|s| s.eq_ignore_ascii_case(&scheme)) {
        issues.push(UrlIssue::BadScheme);
    }

    let host = host_of(rest);
    if host.is_empty() {
        issues.push(UrlIssue::NoHost);
    } else if reject_local && is_local_host(&host) {
        issues.push(UrlIssue::LocalHost);
    }

    issues
}

fn split_scheme(url: &str) -> Option<(String, &str)> {
    let idx = url.find("://")?;
    let scheme = &url[..idx];
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
    {
        return None;
    }
    Some((scheme.to_lowercase(), &url[idx + 3..]))
}

fn host_of(rest: &str) -> String {
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let after_userinfo = authority.rsplit('@').next().unwrap_or_default();

    // A bracketed IPv6 literal is full of colons, so the port has to be found after the
    // closing bracket rather than at the last colon.
    if let Some(inner) = after_userinfo.strip_prefix('[')
        && let Some(end) = inner.find(']')
    {
        return inner[..end].to_lowercase();
    }

    match after_userinfo.find(':') {
        Some(i) => after_userinfo[..i].to_lowercase(),
        None => after_userinfo.to_lowercase(),
    }
}

/// Forem's `no_local` option: a bare hostname with no dot, a `.local` name, or a loopback address.
fn is_local_host(host: &str) -> bool {
    if LOCAL_HOSTS.contains(&host) || host.ends_with(".local") {
        return true;
    }
    // An IPv6 literal has no dots but is not a bare intranet hostname either.
    if host.contains(':') {
        return false;
    }
    !host.contains('.')
}

/// The three video hosts `Api::ArticlesController#article_params` will permit, as the
/// controller's own regexes. A URL outside these is dropped from the payload entirely
/// rather than rejected, so the article saves without the video.
pub fn is_permitted_video_source(url: &str) -> bool {
    let lower = url.to_lowercase();
    for prefix in ["https://", "http://"] {
        let Some(rest) = lower.strip_prefix(prefix) else {
            continue;
        };
        let rest = rest.strip_prefix("www.").unwrap_or(rest);
        if rest.starts_with("youtube.com/watch?v=")
            || rest.starts_with("youtu.be/")
            || rest.starts_with("twitch.tv/videos/")
        {
            return true;
        }
        // `player.mux.com` is matched without the optional www. prefix upstream.
        if lower[prefix.len()..].starts_with("player.mux.com/") {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEB: [&str; 2] = ["https", "http"];

    #[test]
    fn accepts_an_ordinary_https_url() {
        assert!(check("https://example.com/post", &WEB, true).is_empty());
    }

    #[test]
    fn rejects_whitespace_anywhere() {
        assert!(check("https://example.com/a b", &WEB, true).contains(&UrlIssue::Whitespace));
    }

    #[test]
    fn rejects_a_scheme_outside_the_allowlist() {
        assert!(check("ftp://example.com", &WEB, true).contains(&UrlIssue::BadScheme));
        assert!(check("http://youtu.be/x", &["https"], true).contains(&UrlIssue::BadScheme));
    }

    #[test]
    fn rejects_local_hosts() {
        for url in [
            "http://localhost/x",
            "http://127.0.0.1:3000/x",
            "http://box.local/x",
            "http://intranet/x",
        ] {
            assert!(
                check(url, &WEB, true).contains(&UrlIssue::LocalHost),
                "expected {url} to be local"
            );
        }
    }

    #[test]
    fn strips_userinfo_and_port_before_judging_the_host() {
        assert!(check("https://user:pw@example.com:8443/x", &WEB, true).is_empty());
    }

    #[test]
    fn permits_only_the_three_video_hosts() {
        assert!(is_permitted_video_source(
            "https://www.youtube.com/watch?v=abc"
        ));
        assert!(is_permitted_video_source("https://youtu.be/abc"));
        assert!(is_permitted_video_source("https://player.mux.com/abc"));
        assert!(is_permitted_video_source(
            "https://www.twitch.tv/videos/123"
        ));
        assert!(!is_permitted_video_source("https://vimeo.com/123"));
        assert!(!is_permitted_video_source("https://youtube.com/shorts/abc"));
    }

    /// A scheme is letters, digits, `+`, `-` and `.`. Anything else in that position means
    /// the string is not an absolute URL at all, whatever follows the `://`.
    #[test]
    fn the_scheme_charset_decides_whether_this_is_a_url() {
        assert!(check("svn+ssh://example.com", &["svn+ssh"], false).is_empty());
        assert!(check("view-source://example.com", &["view-source"], false).is_empty());
        assert!(check("x.y://example.com", &["x.y"], false).is_empty());

        for not_a_url in [
            "ht tp://example.com",
            "://example.com",
            "ht_tp://example.com",
            "https:/example.com",
            "example.com",
        ] {
            assert!(
                check(not_a_url, &WEB, false).contains(&UrlIssue::BadScheme),
                "{not_a_url} should not parse as a web URL"
            );
        }
    }

    /// A string with no usable scheme is not a URL, so the check stops there rather than
    /// also passing judgement on a host it never parsed. One problem, not two.
    #[test]
    fn a_bad_scheme_stops_the_check_before_the_host_is_judged() {
        assert_eq!(
            check("://localhost/x", &WEB, true),
            vec![UrlIssue::BadScheme]
        );
        assert_eq!(
            check("ht_tp://localhost/x", &WEB, true),
            vec![UrlIssue::BadScheme]
        );
    }

    #[test]
    fn the_scheme_comparison_ignores_case() {
        assert!(check("HTTPS://example.com", &WEB, false).is_empty());
    }

    #[test]
    fn a_url_with_no_host_is_reported_as_such() {
        assert!(check("https:///path", &WEB, false).contains(&UrlIssue::NoHost));
    }

    /// A bracketed IPv6 literal is all colons, so the port cannot be found at the last one.
    #[test]
    fn an_ipv6_literal_keeps_its_address_and_is_not_local() {
        assert!(check("https://[2001:db8::1]/x", &WEB, true).is_empty());
        assert!(check("https://[2001:db8::1]:8443/x", &WEB, true).is_empty());
        assert!(check("https://[::1]/x", &WEB, true).contains(&UrlIssue::LocalHost));
    }

    fn any_url_ish(tc: &hegel::TestCase) -> String {
        tc.draw(hegel::one_of!(
            hegel::generators::text().max_size(60),
            hegel::generators::urls(),
        ))
    }

    /// These run over caller-supplied strings that were never parsed, so the only thing
    /// that must never happen is a panic — `check` slices on byte indices it computed itself.
    #[hegel::test]
    fn check_never_panics(tc: hegel::TestCase) {
        let url = any_url_ish(&tc);
        let reject_local = tc.draw(hegel::generators::booleans());
        let _ = check(&url, &WEB, reject_local);
        let _ = check(&url, &["https"], reject_local);
        let _ = check(&url, &[], reject_local);
    }

    #[hegel::test]
    fn video_source_check_never_panics(tc: hegel::TestCase) {
        let _ = is_permitted_video_source(&any_url_ish(&tc));
    }

    /// The two functions have to agree: the controller's video allowlist only ever matches
    /// http or https URLs, so anything it permits must clear the web scheme check too.
    #[hegel::test]
    fn a_permitted_video_source_is_always_a_web_url(tc: hegel::TestCase) {
        let url = any_url_ish(&tc);
        if is_permitted_video_source(&url) {
            let issues = check(&url, &WEB, false);
            assert!(
                !issues.contains(&UrlIssue::BadScheme),
                "{url} was permitted as a video source but has a non-web scheme"
            );
        }
    }

    /// Narrowing the allowlist can only ever add a scheme complaint, never remove one.
    #[hegel::test]
    fn a_narrower_scheme_list_is_never_more_permissive(tc: hegel::TestCase) {
        let url = any_url_ish(&tc);
        if !check(&url, &WEB, false).contains(&UrlIssue::BadScheme) {
            return;
        }
        assert!(check(&url, &["https"], false).contains(&UrlIssue::BadScheme));
    }
}
