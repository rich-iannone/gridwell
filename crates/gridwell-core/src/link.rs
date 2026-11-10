//! Links: the `href` of a text run. Only `http`, `https` and `mailto` URLs are
//! links. The IR may be untrusted, and a link of any other scheme (`javascript:`,
//! `data:`, `file:`, `vbscript:`) could run code or open a local file when someone
//! follows it in the output.

/// Why `href` isn't a link gridwell writes, or `None` if it is one: an absolute
/// `http` or `https` URL with a host, or a `mailto` URL with an address, without
/// whitespace or control characters (the scheme is matched ignoring case).
pub fn href_problem(href: &str) -> Option<&'static str> {
    if href.is_empty() {
        return Some("it is empty");
    }
    if href.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Some("it has whitespace or a control character");
    }
    let Some((scheme, rest)) = href.split_once(':') else {
        return Some("it has no scheme (expected http, https or mailto)");
    };
    if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
        match rest.strip_prefix("//") {
            Some(r) if !r.is_empty() && !r.starts_with(['/', '?', '#']) => None,
            _ => Some("it has no host"),
        }
    } else if scheme.eq_ignore_ascii_case("mailto") {
        (rest.is_empty()).then_some("it has no address")
    } else {
        Some("its scheme is not http, https or mailto")
    }
}

/// Whether `href` is a link gridwell writes (see [`href_problem`]).
pub fn is_link(href: &str) -> bool {
    href_problem(href).is_none()
}

/// The URL a format writes for `href`, or `None` if it isn't a link (see
/// [`href_problem`]). Its scheme is lowercased, and every character a URL can't
/// hold as it is (RFC 3986), a non-ASCII one included, is percent-encoded as
/// UTF-8, as is a `%` that doesn't start a `%XX` escape. What is left is ASCII
/// letters, digits and ``-._~:/?#[]@!$&'()*+,;=%``: no quote but `'`, and no `<`,
/// `>`, `\`, `^`, `` ` ``, `{`, `|` or `}`, so each format escapes only its own few
/// characters.
pub fn link_url(href: &str) -> Option<String> {
    if !is_link(href) {
        return None;
    }
    let (scheme, rest) = href.split_once(':')?;
    let b = rest.as_bytes();
    let mut out = String::with_capacity(href.len());
    out.push_str(&scheme.to_ascii_lowercase());
    out.push(':');
    for (i, &c) in b.iter().enumerate() {
        let hex = |j: usize| b.get(j).is_some_and(u8::is_ascii_hexdigit);
        let keep = c.is_ascii_alphanumeric()
            || b"-._~:/?#[]@!$&'()*+,;=".contains(&c)
            || (c == b'%' && hex(i + 1) && hex(i + 2));
        if keep {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{c:02X}"));
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_http_https_and_mailto_are_links() {
        for ok in [
            "http://example.com",
            "https://example.com/a?b=1#c",
            "HTTPS://EXAMPLE.COM",
            "mailto:someone@example.com",
            "MailTo:a@b.c?subject=hi",
            "https://例え.jp/パス",
        ] {
            assert_eq!(href_problem(ok), None, "{ok}");
        }
        for (bad, why) in [
            ("", "empty"),
            ("javascript:alert(1)", "scheme"),
            ("JavaScript:alert(1)", "scheme"),
            ("data:text/html,<script>", "scheme"),
            ("file:///etc/passwd", "scheme"),
            ("vbscript:x", "scheme"),
            ("/relative/path", "no scheme"),
            ("example.com", "no scheme"),
            ("https:example.com", "no host"),
            ("https://", "no host"),
            ("https:///path", "no host"),
            ("mailto:", "no address"),
            (" https://example.com", "whitespace"),
            ("java\tscript:alert(1)", "whitespace"),
            ("https://exa mple.com", "whitespace"),
            ("https://example.com/\u{7}", "control"),
            ("https://example.com/\u{85}", "whitespace"),
            ("\u{0}javascript:alert(1)", "control"),
        ] {
            let got = href_problem(bad);
            assert!(got.is_some_and(|g| g.contains(why)), "{bad:?}: {got:?}");
        }
    }

    #[test]
    fn urls_are_ascii_and_escape_what_formats_care_about() {
        assert_eq!(
            link_url("https://例え.jp/a").as_deref(),
            Some("https://%E4%BE%8B%E3%81%88.jp/a")
        );
        assert_eq!(
            link_url(r#"https://x.org/{a}|b\c^d`e"f<g>"#).as_deref(),
            Some("https://x.org/%7Ba%7D%7Cb%5Cc%5Ed%60e%22f%3Cg%3E")
        );
        // An escape stays; a lone `%` doesn't.
        assert_eq!(
            link_url("https://x.org/%41%zz%4").as_deref(),
            Some("https://x.org/%41%25zz%254")
        );
        assert_eq!(
            link_url("https://x.org/a-b_c.d~e:f?g=h&i#j[k]@l!$'()*+,;").as_deref(),
            Some("https://x.org/a-b_c.d~e:f?g=h&i#j[k]@l!$'()*+,;")
        );
        assert_eq!(
            link_url("HTTPS://Example.COM/A").as_deref(),
            Some("https://Example.COM/A")
        );
        assert_eq!(link_url("MailTo:a@b.c").as_deref(), Some("mailto:a@b.c"));
        assert_eq!(link_url("javascript:alert(1)"), None);
    }
}
