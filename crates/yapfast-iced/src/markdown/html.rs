//! Maps a small set of HTML elements to native message text.

use std::{borrow::Cow, cell::RefCell};

use html5ever::tokenizer::{
    BufferQueue, EndTag, StartTag, Tag, Token, TokenSink, TokenSinkResult, Tokenizer,
};
use yapfast_core::links;

use super::Run;

#[derive(Default)]
struct Tokens(RefCell<Vec<Token>>);

impl TokenSink for Tokens {
    type Handle = ();

    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        if matches!(token, Token::TagToken(_) | Token::CharacterTokens(_)) {
            self.0.borrow_mut().push(token);
        }
        TokenSinkResult::Continue
    }
}

pub(super) fn tokens(fragment: &str) -> Vec<Token> {
    let tokenizer = Tokenizer::new(Tokens::default(), Default::default());
    let input = BufferQueue::default();
    input.push_back(fragment.into());
    let result = tokenizer.feed(&input);
    debug_assert!(matches!(result, html5ever::TokenizerResult::Done));
    tokenizer.end();
    tokenizer.sink.0.into_inner()
}

#[derive(Default)]
pub(super) struct Formatting(Vec<(html5ever::LocalName, TagStyle)>);

#[derive(Default)]
struct TagStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    code: bool,
    suppress: bool,
    link: Option<String>,
    color: Option<u32>,
}

#[derive(Default)]
pub(super) struct Style<'a> {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
    pub code: bool,
    pub link: Option<&'a String>,
    pub color: Option<u32>,
}

impl Formatting {
    pub(super) fn style(&self) -> Style<'_> {
        let mut style = Style::default();
        for (_, tag) in &self.0 {
            style.bold |= tag.bold;
            style.italic |= tag.italic;
            style.strike |= tag.strike;
            style.underline |= tag.underline;
            style.code |= tag.code;
            style.link = tag.link.as_ref().or(style.link);
            style.color = tag.color.or(style.color);
        }
        style
    }

    pub(super) fn suppressed(&self) -> bool {
        self.0.iter().any(|(_, style)| style.suppress)
    }

    pub(super) fn tag(&mut self, tag: Tag, runs: &[Run]) -> Option<Cow<'static, str>> {
        let suppressed = self.suppressed();
        let name: &str = tag.name.as_ref();
        let block = matches!(
            name,
            "p" | "div"
                | "blockquote"
                | "li"
                | "ul"
                | "ol"
                | "pre"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
        );
        if tag.kind == EndTag {
            if let Some(at) = self.0.iter().rposition(|(open, _)| *open == tag.name) {
                self.0.truncate(at);
            }
        } else if tag.kind == StartTag && !suppressed {
            let attr = |key: &str| {
                tag.attrs
                    .iter()
                    .find(|attr| attr.name.local.as_ref() == key)
                    .map(|attr| attr.value.as_ref())
            };
            let mut style = TagStyle::default();
            match name {
                "br" => return Some("\n".into()),
                "hr" => return Some("\n────────\n".into()),
                "img" => return attr("alt").map(|alt| Cow::Owned(alt.to_owned())),
                "b" | "strong" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => style.bold = true,
                "i" | "em" => style.italic = true,
                "s" | "del" | "strike" => style.strike = true,
                "u" | "ins" => style.underline = true,
                "code" | "kbd" | "samp" | "pre" => style.code = true,
                "a" => {
                    style.link = attr("href")
                        .filter(|url| links::openable(url))
                        .map(|url| format!("{}{url}", links::LEAVE));
                }
                "font" => {
                    style.color = attr("color").and_then(|value| {
                        let hex = value.strip_prefix('#')?;
                        (hex.len() == 6)
                            .then(|| u32::from_str_radix(hex, 16).ok())
                            .flatten()
                    });
                }
                "script" | "style" | "iframe" | "object" | "template" => style.suppress = true,
                _ if !block => return None,
                _ => {}
            }
            if !tag.self_closing {
                self.0.push((tag.name.clone(), style));
            }
        }
        if block && !suppressed {
            let newline = runs.last().is_some_and(|run| !run.text.ends_with('\n'));
            if name == "li" && tag.kind == StartTag {
                return Some(if newline { "\n• " } else { "• " }.into());
            }
            return newline.then(|| "\n".into());
        }
        None
    }
}
