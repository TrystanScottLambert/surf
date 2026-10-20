//! Clean-up of text documents before display.

/// Remove a leading UTF-8 byte-order mark, which some editors add.
pub fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// Turn bare `http(s)://` URLs into markdown autolinks (`<url>`), as GitHub
/// does. URLs inside code (fenced or inline) and in existing links are kept.
pub fn linkify(markdown: &str) -> String {
    let mut out = String::with_capacity(markdown.len() + 64);
    let mut in_fence = false;
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            out.push_str(line);
        } else if in_fence || line.starts_with("    ") || line.starts_with('\t') {
            out.push_str(line);
        } else {
            linkify_line(line, &mut out);
        }
    }
    out
}

fn linkify_line(line: &str, out: &mut String) {
    let mut in_code = false;
    let mut rest = line;
    while !rest.is_empty() {
        if rest.starts_with('`') {
            in_code = !in_code;
            out.push('`');
            rest = &rest[1..];
            continue;
        }
        let at_url = rest.starts_with("https://") || rest.starts_with("http://");
        let prev = out.chars().last();
        // `(` covers `[text](url)`, `<` covers `<url>`, `"`/`'`/`=` cover HTML attributes.
        let already_linked = matches!(prev, Some('(' | '<' | '"' | '\'' | '='));
        if at_url && !in_code && !already_linked {
            let end = rest
                .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`'))
                .unwrap_or(rest.len());
            let mut url = &rest[..end];
            // Trailing punctuation usually ends the sentence, not the URL.
            while let Some(last) = url.chars().last() {
                let unbalanced_paren =
                    last == ')' && url.matches('(').count() < url.matches(')').count();
                if matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | '\'') || unbalanced_paren {
                    url = &url[..url.len() - last.len_utf8()];
                } else {
                    break;
                }
            }
            out.push('<');
            out.push_str(url);
            out.push('>');
            rest = &rest[url.len()..];
            continue;
        }
        let ch = rest.chars().next().expect("non-empty");
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_bom() {
        assert_eq!(strip_bom("\u{feff}## RIP"), "## RIP");
        assert_eq!(strip_bom("## RIP"), "## RIP");
    }

    #[test]
    fn links_bare_urls() {
        assert_eq!(
            linkify("see https://doi.org/10.1093/mnras/sty440). Next"),
            "see <https://doi.org/10.1093/mnras/sty440>). Next"
        );
        assert_eq!(
            linkify("(more details in https://doi.org/x/y)."),
            "(more details in <https://doi.org/x/y>)."
        );
        assert_eq!(
            linkify("from https://github.com/asgr/ProFound."),
            "from <https://github.com/asgr/ProFound>."
        );
    }

    #[test]
    fn leaves_existing_links_and_code() {
        let md =
            "[site](https://a.org) and <https://b.org>\n`https://c.org`\n```\nhttps://d.org\n```\n";
        assert_eq!(linkify(md), md);
        assert_eq!(
            linkify("    https://indented.org\n"),
            "    https://indented.org\n"
        );
    }

    #[test]
    fn keeps_balanced_parens_and_unicode() {
        assert_eq!(linkify("https://w.org/A_(b) é"), "<https://w.org/A_(b)> é");
    }
}
