//! A small markdown-to-HTML converter for what this app writes.
//!
//! Only the subset the brief and the drafts use: `#` headings, `-` lists,
//! `**bold**`, `` `code` ``, paragraphs. A full markdown crate would handle
//! more and cost ~300 KB of WASM to do it; this is a few dozen lines and runs
//! in the browser, which is where the clipboard is.
//!
//! The output is meant for pasting into Google Docs or Notion, both of which
//! read `text/html` from the clipboard and keep headings and lists. Plain
//! markdown pasted there arrives as literal `#` and `-` characters.

/// Escapes the three characters that would otherwise be read as markup.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Inline markup: bold and code. Anything else passes through escaped.
fn inline(s: &str) -> String {
    let mut out = String::new();
    let mut rest = esc(s);
    // **bold**
    loop {
        let Some(a) = rest.find("**") else { break };
        let Some(b) = rest[a + 2..].find("**") else {
            break;
        };
        out.push_str(&rest[..a]);
        out.push_str("<strong>");
        out.push_str(&rest[a + 2..a + 2 + b]);
        out.push_str("</strong>");
        rest = rest[a + 4 + b..].to_string();
    }
    out.push_str(&rest);
    // `code`
    let mut coded = String::new();
    let mut rest = out;
    loop {
        let Some(a) = rest.find('`') else { break };
        let Some(b) = rest[a + 1..].find('`') else {
            break;
        };
        coded.push_str(&rest[..a]);
        coded.push_str("<code>");
        coded.push_str(&rest[a + 1..a + 1 + b]);
        coded.push_str("</code>");
        rest = rest[a + 2 + b..].to_string();
    }
    coded.push_str(&rest);
    coded
}

/// Converts markdown to HTML.
pub fn to_html(markdown: &str) -> String {
    let mut html = String::new();
    let mut in_list = false;
    let mut para: Vec<String> = Vec::new();

    let flush_para = |para: &mut Vec<String>, html: &mut String| {
        if !para.is_empty() {
            html.push_str("<p>");
            html.push_str(&inline(&para.join(" ")));
            html.push_str("</p>\n");
            para.clear();
        }
    };

    for line in markdown.lines() {
        let t = line.trim_end();
        let trimmed = t.trim_start();

        if trimmed.is_empty() {
            flush_para(&mut para, &mut html);
            if in_list {
                html.push_str("</ul>\n");
                in_list = false;
            }
            continue;
        }

        // Headings: count the hashes.
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if hashes > 0 && hashes <= 6 && trimmed[hashes..].starts_with(' ') {
            flush_para(&mut para, &mut html);
            if in_list {
                html.push_str("</ul>\n");
                in_list = false;
            }
            let text = trimmed[hashes + 1..].trim();
            html.push_str(&format!("<h{hashes}>{}</h{hashes}>\n", inline(text)));
            continue;
        }

        // List items.
        if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            flush_para(&mut para, &mut html);
            if !in_list {
                html.push_str("<ul>\n");
                in_list = true;
            }
            html.push_str(&format!("<li>{}</li>\n", inline(item.trim())));
            continue;
        }

        // Fenced code and horizontal rules are dropped: the brief does not
        // produce them, and a stray ``` would otherwise become a paragraph.
        if trimmed.starts_with("```") || trimmed == "---" {
            continue;
        }

        if in_list {
            html.push_str("</ul>\n");
            in_list = false;
        }
        para.push(trimmed.to_string());
    }
    flush_para(&mut para, &mut html);
    if in_list {
        html.push_str("</ul>\n");
    }
    html
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_lists_and_paragraphs() {
        let md = "# Brief: kawa\n\n## Questions\n\n- Czy kawa jest zdrowa?\n- Ile kofeiny?\n\nAkapit pierwszy.\nCiąg dalszy.\n\nDrugi akapit.";
        let h = to_html(md);
        assert!(h.contains("<h1>Brief: kawa</h1>"));
        assert!(h.contains("<h2>Questions</h2>"));
        assert!(h.contains("<ul>\n<li>Czy kawa jest zdrowa?</li>\n<li>Ile kofeiny?</li>\n</ul>"));
        // Soft line breaks join into one paragraph, blank lines split them.
        assert!(h.contains("<p>Akapit pierwszy. Ciąg dalszy.</p>"));
        assert!(h.contains("<p>Drugi akapit.</p>"));
    }

    #[test]
    fn inline_bold_and_code() {
        assert_eq!(
            inline("to **ważne** i `kod`"),
            "to <strong>ważne</strong> i <code>kod</code>"
        );
    }

    #[test]
    fn html_in_the_source_is_escaped_not_executed() {
        let h = to_html("Uwaga: <script>alert(1)</script> & co");
        assert!(h.contains("&lt;script&gt;"));
        assert!(!h.contains("<script>"));
        assert!(h.contains("&amp; co"));
    }

    #[test]
    fn a_list_directly_after_a_heading_closes_cleanly() {
        let h = to_html("## A\n- x\n## B\n- y");
        assert_eq!(h.matches("<ul>").count(), 2);
        assert_eq!(h.matches("</ul>").count(), 2);
    }
}
