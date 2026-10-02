//! The network policy (SPEC §7.2): what a surface may fetch from the network.
//!
//! It has two halves, both in CSP's syntax: the host's, which its user
//! grants (`Host::set_network`), and the document's, its
//! `<meta name="hotty-network">`. A URL is fetched only when a source in each
//! half matches it, for the directive that covers its request. The same
//! rules as xterm-addon-hotty's `network.ts`.

use blitz_traits::net::Destination;
use std::collections::BTreeMap;
use url::Url;

/// What a directive covers (SPEC §7.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Directive {
    /// Images: `<img>`, and images in CSS.
    Img,
    /// `<audio>` and `<video>`, which this host does not play.
    Media,
    /// `@font-face`.
    Font,
    /// `<link rel=stylesheet>` and `@import`.
    Style,
}

impl Directive {
    pub const ALL: [Directive; 4] = [
        Directive::Img,
        Directive::Media,
        Directive::Font,
        Directive::Style,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Directive::Img => "img-src",
            Directive::Media => "media-src",
            Directive::Font => "font-src",
            Directive::Style => "style-src",
        }
    }

    fn parse(s: &str) -> Option<Directive> {
        Directive::ALL
            .into_iter()
            .find(|d| d.name().eq_ignore_ascii_case(s))
    }

    /// The directive that covers a request, by what it is for. None for a
    /// document (an `<iframe>`'s, a navigation's) or anything else: those
    /// are never fetched (SPEC §12).
    pub fn of(destination: Destination) -> Option<Directive> {
        match destination {
            Destination::Image => Some(Directive::Img),
            Destination::Style => Some(Directive::Style),
            Destination::Font => Some(Directive::Font),
            _ => None,
        }
    }
}

/// A source: an `http` or `https` origin, or `https:`, every HTTPS origin.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Source {
    AnyHttps,
    Origin(url::Origin),
}

impl Source {
    fn parse(s: &str) -> Option<Source> {
        let v = s.trim().to_ascii_lowercase();
        if v == "https:" {
            return Some(Source::AnyHttps);
        }
        // An origin: a scheme and an authority, and nothing after the host
        // and port but an optional slash.
        let rest = v
            .strip_prefix("https://")
            .or_else(|| v.strip_prefix("http://"))?;
        let host = rest.strip_suffix('/').unwrap_or(rest);
        if host.is_empty() || host.contains(['/', '?', '#', '@', '\\']) {
            return None;
        }
        let u = Url::parse(&v).ok()?;
        Some(Source::Origin(u.origin()))
    }

    fn matches(&self, url: &Url) -> bool {
        match self {
            Source::AnyHttps => url.scheme() == "https",
            Source::Origin(o) => url.origin() == *o,
        }
    }

    fn serialize(&self) -> String {
        match self {
            Source::AnyHttps => "https:".to_string(),
            Source::Origin(o) => o.ascii_serialization(),
        }
    }
}

/// One half of the policy: sources per directive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Policy(BTreeMap<Directive, Vec<Source>>);

impl Policy {
    /// CSP's syntax: directives separated by `;`, each followed by its
    /// sources separated by spaces. Unknown directives and what is not a
    /// source are left out.
    pub fn parse(csp: &str) -> Policy {
        let mut p = Policy::default();
        for part in csp.split(';') {
            let mut words = part.split_ascii_whitespace();
            let Some(d) = words.next().and_then(Directive::parse) else {
                continue;
            };
            for s in words.filter_map(Source::parse) {
                let srcs = p.0.entry(d).or_default();
                if !srcs.contains(&s) {
                    srcs.push(s);
                }
            }
        }
        p.0.retain(|_, s| !s.is_empty());
        p
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether a source of this half matches `url` for `directive`.
    pub fn allows(&self, directive: Directive, url: &Url) -> bool {
        self.0
            .get(&directive)
            .is_some_and(|srcs| srcs.iter().any(|s| s.matches(url)))
    }

    /// As the capabilities report it (`net`, SPEC §4): directive to sources.
    pub fn to_json(&self) -> serde_json::Value {
        let map: serde_json::Map<String, serde_json::Value> = self
            .0
            .iter()
            .map(|(d, srcs)| {
                let srcs = srcs
                    .iter()
                    .map(|s| serde_json::Value::String(s.serialize()));
                (d.name().to_string(), srcs.collect())
            })
            .collect();
        serde_json::Value::Object(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn sources_are_origins_or_https() {
        let p = Policy::parse(
            "img-src https://Example.com/ http://localhost:8080 https: \
             https://a.com/path http://b.com?q ftp://c.com file: * 'self' https://u@d.com; \
             font-src; nope-src https:; IMG-SRC https://e.com:443",
        );
        assert_eq!(
            p.to_json(),
            serde_json::json!({
                "img-src": ["https://example.com", "http://localhost:8080", "https:", "https://e.com"]
            })
        );
    }

    #[test]
    fn a_source_matches_its_origin_and_https_every_https_one() {
        let p = Policy::parse("img-src http://localhost:8080; style-src https:");
        assert!(p.allows(Directive::Img, &url("http://localhost:8080/a/b.png?x")));
        assert!(!p.allows(Directive::Img, &url("http://localhost:8081/b.png")));
        assert!(!p.allows(Directive::Img, &url("https://localhost:8080/b.png")));
        assert!(!p.allows(Directive::Font, &url("http://localhost:8080/f.woff2")));
        assert!(p.allows(Directive::Style, &url("https://any.example/x.css")));
        assert!(!p.allows(Directive::Style, &url("http://any.example/x.css")));
        assert!(!p.allows(Directive::Style, &url("file:///etc/passwd")));
        // A default port is the origin without one.
        let p = Policy::parse("img-src https://example.com:443");
        assert!(p.allows(Directive::Img, &url("https://example.com/x.png")));
    }

    #[test]
    fn an_empty_policy_allows_nothing() {
        for csp in ["", ";;", "img-src", "img-src 'none'"] {
            let p = Policy::parse(csp);
            assert!(p.is_empty(), "{csp}");
            assert_eq!(p.to_json(), serde_json::json!({}));
            assert!(!p.allows(Directive::Img, &url("https://example.com/x.png")));
        }
    }

    #[test]
    fn documents_and_frames_have_no_directive() {
        assert_eq!(Directive::of(Destination::Image), Some(Directive::Img));
        assert_eq!(Directive::of(Destination::Iframe), None);
        assert_eq!(Directive::of(Destination::Document), None);
        assert_eq!(Directive::of(Destination::Empty), None);
    }
}
