//! Python's `xml.sax.saxutils.quoteattr`.

/// An XML attribute value with its quotes, like `quoteattr`.
pub fn quoteattr(s: &str) -> String {
    let mut d = s
        .replace('&', "&amp;")
        .replace('>', "&gt;")
        .replace('<', "&lt;");
    d = d
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
        .replace('\t', "&#9;");
    if d.contains('"') {
        if d.contains('\'') {
            format!("\"{}\"", d.replace('"', "&quot;"))
        } else {
            format!("'{d}'")
        }
    } else {
        format!("\"{d}\"")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn like_python() {
        assert_eq!(super::quoteattr("a&b<c>"), "\"a&amp;b&lt;c&gt;\"");
        assert_eq!(super::quoteattr("say \"hi\""), "'say \"hi\"'");
        assert_eq!(super::quoteattr("it's \"x\""), "\"it's &quot;x&quot;\"");
    }
}
