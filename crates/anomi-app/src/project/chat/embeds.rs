//! A message's embeds as cards: `message_row`'s `EmbedCard`s
//! (`src/ui/messages.rs:576`-`:626`), with their fields in rows as
//! `field_rows` sets them (`:391`) and their titles as `embed_title`
//! (`:302`): the colour's bar, the provider, author, title linking to the
//! page, description, fields, footer, a thumbnail beside the text and the
//! pictures under it.

use yapfast_core::chat::{ChatMessage, Embed, EmbedField, Placement};

use anomi_vm::chat::embeds::{Card, Cell, Embeds, Field, Gallery, Thumbnail};
use anomi_vm::text::{self, Builder, Link, Source, Style, Text};

use super::attachments;
use super::message::{Context, body_sections};
use crate::project::rows;

pub(super) fn embeds(cx: &Context<'_>, message: &ChatMessage, key: u64) -> Embeds {
    let covered = |index: usize| !cx.revealed.contains(&format!("{}/a{index}", message.item));
    Embeds {
        item: key,
        cards: message
            .embeds
            .iter()
            .enumerate()
            .map(|(index, embed)| card(cx, message, key, index, embed, &covered))
            .collect(),
    }
}

fn card(
    cx: &Context<'_>,
    message: &ChatMessage,
    key: u64,
    index: usize,
    embed: &Embed,
    covered: &dyn Fn(usize) -> bool,
) -> Card {
    let plain = |words: &str| (!words.is_empty()).then(|| Text::styled(words, Style::default()));
    let thumbnail = attachments::shown(message, Placement::Thumbnail(index))
        .next()
        .map(|(index, attachment)| Thumbnail {
            picture: attachments::picture(attachment),
            index,
        });
    let pictures = attachments::gallery(cx, message, key, Placement::Embed(index), covered);
    Card {
        color: embed.color.map(rows::rgb),
        provider: plain(&embed.provider),
        author: (!embed.author.is_empty()).then(|| Text::bold(&embed.author)),
        title: (!embed.title.is_empty()).then(|| title(&embed.title, &embed.url)),
        url: embed.url.clone(),
        description: (!embed.description.is_empty()).then(|| markdown(&embed.description)),
        rows: rows(&embed.fields),
        footer: plain(&embed.footer),
        thumbnail,
        gallery: (!pictures.cells.is_empty()).then(|| Gallery {
            width: pictures.width as f32,
            height: pictures.height as f32,
            cells: pictures
                .cells
                .into_iter()
                .map(|cell| Cell {
                    x: cell.x as f32,
                    y: cell.y as f32,
                    width: cell.width as f32,
                    height: cell.height as f32,
                    index: cell.index,
                    picture: cell.picture,
                    spoiler: cell.spoiler,
                    name: cell.name,
                    playing: cell.video,
                })
                .collect(),
            video: embed.video,
        }),
    }
}

/// An embed's title in bold, linking to its page when it has one.
fn title(title: &str, url: &str) -> Text {
    if url.is_empty() {
        return Text::bold(title);
    }
    let mut builder = Builder::new();
    let bold = Style {
        bold: true,
        ..Style::default()
    };
    builder.link(title, bold, Link::Url(url.to_owned()), None);
    builder.finish()
}

/// An embed's Markdown, its blocks laid out, its emoji opening no cards.
fn markdown(markdown: &str) -> Text {
    let sections = body_sections(markdown);
    text::markdown(Source {
        sections: sections.as_deref(),
        ..Source::new(markdown)
    })
}

/// The fields in rows: up to three inline fields share one, and any other
/// field takes a row of its own, as in Discord.
fn rows(fields: &[EmbedField]) -> Vec<Vec<Field>> {
    let mut rows: Vec<Vec<Field>> = Vec::new();
    let mut open = false;
    for field in fields {
        let cell = Field {
            name: Text::bold(&field.name),
            value: markdown(&field.value),
        };
        match rows.last_mut() {
            Some(row) if open && field.inline && row.len() < 3 => row.push(cell),
            _ => rows.push(vec![cell]),
        }
        open = field.inline;
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, inline: bool) -> EmbedField {
        EmbedField {
            name: name.into(),
            value: "v".into(),
            inline,
        }
    }

    #[test]
    fn three_inline_fields_share_a_row() {
        let shape: Vec<usize> = rows(&[
            field("a", true),
            field("b", true),
            field("c", true),
            field("d", true),
            field("e", false),
            field("f", true),
        ])
        .iter()
        .map(Vec::len)
        .collect();
        assert_eq!(shape, [3, 1, 1, 1]);
    }
}
