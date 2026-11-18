//! A message as a selection covering it copies it, built only while copying
//! the rows between a selection's ends (`message_text`, `src/ui.rs:924`):
//! reading as its row shows it, texts beside each other joined by a space,
//! others by line breaks, and each picture as `[Picture]`.

use anomi_vm::chat::timeline::Row;
use anomi_vm::text::Text;
use yapfast_lang::tr;

/// What a selection covering all of `row` copies.
pub fn copied(row: &Row) -> String {
    let message = &row.message;
    let mut lines = Vec::new();
    if message.headed() {
        lines.push(format!("{} {}", message.author.name, message.time));
    }
    if !message.body.is_empty() {
        lines.push(plain(&message.body));
    }
    let picture = || tr!("[Picture]").to_owned();
    let attachments = &message.attachments;
    lines.extend(attachments.gallery.cells.iter().map(|_| picture()));
    lines.extend(attachments.files.iter().map(|file| file.name.clone()));
    for card in &message.embeds.cards {
        let mut texts: Vec<String> = [&card.provider, &card.author, &card.title]
            .into_iter()
            .flatten()
            .map(plain)
            .collect();
        texts.extend(card.description.as_ref().map(plain));
        for fields in &card.rows {
            let names: Vec<String> = fields.iter().map(|field| plain(&field.name)).collect();
            let values: Vec<String> = fields.iter().map(|field| plain(&field.value)).collect();
            texts.push(names.join(" "));
            texts.push(values.join(" "));
        }
        if let Some(gallery) = &card.gallery {
            texts.extend(gallery.cells.iter().map(|_| picture()));
        }
        texts.extend(card.footer.as_ref().map(plain));
        // The thumbnail sits level with the card's first text, to its right.
        if card.thumbnail.is_some() {
            texts.insert(texts.len().min(1), picture());
        }
        lines.extend(texts);
    }
    lines.join("\n")
}

/// A text's words, its blocks on lines of their own.
fn plain(text: &Text) -> String {
    let blocks: Vec<String> = text
        .blocks()
        .map(|(_, spans)| spans.iter().map(|span| text.chars(span)).collect())
        .collect();
    blocks.join("\n")
}
