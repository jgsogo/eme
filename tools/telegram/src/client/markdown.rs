//! Convert CommonMark into the restricted HTML subset supported by Telegram.

use pulldown_cmark::{Options, Parser, html};

pub(crate) fn markdown_to_telegram_html(markdown: &str) -> String {
    // Treat raw HTML as text: Telegram only accepts a restricted HTML subset,
    // and passing model-provided tags through could make the whole message fail.
    let safe_markdown = markdown.replace('<', "&lt;").replace('>', "&gt;");
    let mut rendered = String::new();
    html::push_html(
        &mut rendered,
        Parser::new_ext(&safe_markdown, Options::all()),
    );

    // Telegram supports a small HTML subset. Map CommonMark's equivalent
    // elements to those tags and remove structural HTML elements.
    rendered = rendered
        .replace("<strong>", "<b>")
        .replace("</strong>", "</b>")
        .replace("<em>", "<i>")
        .replace("</em>", "</i>")
        .replace("<del>", "<s>")
        .replace("</del>", "</s>")
        .replace("<h1>", "<b>").replace("</h1>", "</b>\\n\\n")
        .replace("<h2>", "<b>").replace("</h2>", "</b>\\n\\n")
        .replace("<h3>", "<b>").replace("</h3>", "</b>\\n\\n")
        .replace("<h4>", "<b>").replace("</h4>", "</b>\\n\\n")
        .replace("<h5>", "<b>").replace("</h5>", "</b>\\n\\n")
        .replace("<h6>", "<b>").replace("</h6>", "</b>\\n\\n")
        .replace("<p>", "")
        .replace("</p>", "\n\n")
        .replace("<ul>", "")
        .replace("</ul>", "")
        .replace("<ol>", "")
        .replace("</ol>", "")
        .replace("<li>", "• ")
        .replace("</li>", "\n")
        .replace("<blockquote>", "<blockquote>")
        .replace("</blockquote>", "</blockquote>\n")
        .replace("<hr />", "──────────\n")
        .replace("<br />", "\n");

    // Telegram rejects HTML entities other than the three XML entities and
    // numeric entities. pulldown-cmark emits valid escaped text for these.
    rendered.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_common_inline_formatting() {
        assert_eq!(
            markdown_to_telegram_html("**bold**, *italic*, ~~deleted~~ and `code`"),
            "<b>bold</b>, <i>italic</i>, <s>deleted</s> and <code>code</code>"
        );
    }

    #[test]
    fn renders_links_and_lists() {
        let rendered = markdown_to_telegram_html("- [Rust](https://www.rust-lang.org)\n- **Safe**");
        assert!(rendered.contains("<a href=\"https://www.rust-lang.org\">Rust</a>"));
        assert!(rendered.contains("• <b>Safe</b>"));
    }

    #[test]
    fn renders_fenced_code_blocks() {
        let rendered = markdown_to_telegram_html("```rust\nfn main() {}\n```");
        assert!(rendered.contains("<pre><code"));
        assert!(rendered.contains("fn main() {}"));
    }

    #[test]
    fn escapes_raw_html_in_user_text() {
        let rendered = markdown_to_telegram_html("<script>alert(1)</script>");
        assert!(!rendered.contains("<script>"));
        assert!(rendered.contains("&lt;script&gt;"));
    }
}
