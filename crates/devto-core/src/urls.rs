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
    let without_port = match after_userinfo.rfind(':') {
        // Leave bracketed IPv6 literals alone.
        Some(i) if !after_userinfo.starts_with('[') => &after_userinfo[..i],
        _ => after_userinfo,
    };
    without_port.trim_matches(['[', ']']).to_lowercase()
}

/// Forem's `no_local` option: a bare hostname with no dot, a `.local` name, or a loopback address.
fn is_local_host(host: &str) -> bool {
    LOCAL_HOSTS.contains(&host) || host.ends_with(".local") || !host.contains('.')
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
