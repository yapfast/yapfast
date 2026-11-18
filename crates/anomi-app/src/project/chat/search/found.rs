//! A message found, as a search's results, the pins and the message a new
//! thread starts from show it (`found_row`, `src/ui/messages.rs:1261`): its
//! author, when, what it says, its first pictures, its other files named and
//! its poll. Each part is the timeline's own: the text through
//! `message::found_text`, the pictures as `attachments` sizes them, laid out
//! as the timeline's gallery and made smaller, the poll as the timeline's
//! card, read only.
//!
//! [`Built`] keeps a list of them: a row is built again only when what it
//! shows changed (its message, as [`print`] sums it, or a picture of its
//! author's), so a next page of results builds the new rows alone, and a
//! backend event while the list shows builds nothing.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use yapfast_core::application::AppState;
use yapfast_core::chat::{Attachment, ChatMessage, FOUND_PICTURES, Placement};
use yapfast_core::clock;

use anomi_vm::chat::attachments::{Cell as Tile, Gallery};
use anomi_vm::chat::search::{Cell, Found};
use anomi_vm::chat::timeline::group;

use crate::pictures::{Requests, Watch};
use crate::project::chat::{attachments, message, poll};
use crate::project::rows;

/// How wide a found message's pictures show at most: the 420 px card less
/// its padding and avatar (`FOUND_GALLERY_WIDTH`).
const GALLERY: u32 = 300;

/// A list of found messages as last built: what each row was built from,
/// and the pictures each shows.
#[derive(Default)]
pub(crate) struct Built {
    prints: Vec<u64>,
    pictures: Watch<usize>,
}

/// A found message and where it shows: its room, and the heading above it.
pub(crate) struct Entry<'a> {
    pub message: &'a ChatMessage,
    pub room: &'a str,
    pub heading: &'a str,
}

impl Built {
    /// Shows `entries` in `rows`, building only the rows whose message,
    /// heading or author's picture changed, and wanting their authors'
    /// pictures.
    pub(crate) fn show<'a>(
        &mut self,
        state: &AppState,
        twelve_hour: bool,
        entries: impl ExactSizeIterator<Item = Entry<'a>>,
        rows: &mut Vec<Found>,
        requests: &mut Requests,
    ) {
        let moved = self.pictures.moved(state);
        let count = entries.len();
        let now = clock::now_ms() / 1000;
        for (index, entry) in entries.enumerate() {
            let print = {
                let mut hasher = DefaultHasher::new();
                print(entry.message, &mut hasher);
                (entry.room, entry.heading, twelve_hour).hash(&mut hasher);
                hasher.finish()
            };
            if self.prints.get(index) == Some(&print) && !moved.contains(&index) {
                continue;
            }
            let avatar = entry.message.avatar.as_deref().unwrap_or_default();
            self.pictures.retain(|row| row != index);
            self.pictures.note(state, avatar, index);
            requests.want(state, [avatar]);
            let row = Found {
                room: entry.room.to_owned(),
                heading: entry.heading.to_owned(),
                ..found(state, entry.message, twelve_hour, now)
            };
            match rows.get_mut(index) {
                Some(shown) => *shown = row,
                None => rows.push(row),
            }
            match self.prints.get_mut(index) {
                Some(shown) => *shown = print,
                None => self.prints.push(print),
            }
        }
        rows.truncate(count);
        self.prints.truncate(count);
        self.pictures.retain(|row| row < count);
    }
}

/// `message` as a found message's card shows it, stamped on the chosen
/// clock, its spoilers covered (a click shows the message).
pub(crate) fn found(state: &AppState, message: &ChatMessage, twelve_hour: bool, now: u64) -> Found {
    let when = match message.timestamp {
        0 => String::new(),
        sent => clock::sent(sent, now, twelve_hour, false),
    };
    let pictures: Vec<(usize, &Attachment)> = message
        .attachments
        .iter()
        .enumerate()
        .filter(|(_, file)| file.placement == Placement::Message && (file.image || file.video))
        .take(FOUND_PICTURES)
        .filter(|(_, file)| file.preview.is_some() || !file.preview_failed)
        .collect();
    let (images, gallery) = gallery(&pictures);
    let files: Vec<&str> = message
        .attachments
        .iter()
        .enumerate()
        .filter(|&(index, file)| {
            file.placement == Placement::Message && !pictures.iter().any(|&(at, _)| at == index)
        })
        .map(|(_, file)| file.name.as_str())
        .collect();
    let key = group::key(&message.id);
    Found {
        item: message.item.clone(),
        author: message.author.clone(),
        avatar: rows::avatar(
            state,
            &message.author,
            &message.author,
            message.avatar.as_deref(),
        ),
        color: message.color.map(rows::rgb),
        body: message::found_text(state, message, &when),
        when,
        images,
        gallery,
        files: files.join(", "),
        poll: (message.poll.as_ref()).map(|asked| poll::found(state, message, asked, key, now)),
        ..Found::default()
    }
}

/// The pictures laid out as the timeline's gallery, made smaller to fit.
fn gallery(pictures: &[(usize, &Attachment)]) -> (Vec<Cell>, (f32, f32)) {
    let tiles = pictures
        .iter()
        .map(|&(index, file)| {
            let (width, height) = attachments::size(file);
            Tile {
                index,
                picture: attachments::picture(file),
                width,
                height,
                ..Tile::default()
            }
        })
        .collect();
    let laid = Gallery::mosaic(tiles);
    let (scale, width, height) = if laid.width > GALLERY {
        let scale =
            |side: u32| (u64::from(side) * u64::from(GALLERY) / u64::from(laid.width)) as u32;
        (Some(scale), GALLERY, scale(laid.height).max(1))
    } else {
        (None, laid.width, laid.height)
    };
    let fit = |side: u32| scale.map_or(side, |scale| scale(side));
    let cells = laid
        .cells
        .into_iter()
        .zip(pictures)
        .map(|(tile, (_, file))| Cell {
            x: fit(tile.x) as f32,
            y: fit(tile.y) as f32,
            width: fit(tile.width).max(1) as f32,
            height: fit(tile.height).max(1) as f32,
            picture: tile.picture,
            spoiler: file.spoiler,
            video: file.video,
        })
        .collect();
    (cells, (width as f32, height as f32))
}

/// Sums up what a found message's card shows, for its row to be built again
/// only once that changed: an edit, a picture that came, a vote.
pub(crate) fn print(message: &ChatMessage, hasher: &mut impl Hasher) {
    (
        &message.item,
        &message.body,
        message.edited,
        message.deleted,
    )
        .hash(hasher);
    (
        &message.author,
        &message.avatar,
        message.color,
        message.timestamp,
    )
        .hash(hasher);
    for file in &message.attachments {
        let preview = file
            .preview
            .as_ref()
            .map(|preview| Arc::as_ptr(preview).addr());
        (&file.name, file.spoiler, preview, file.preview_failed).hash(hasher);
    }
    if let Some(poll) = &message.poll {
        (poll.finalized, poll.expiry).hash(hasher);
        for answer in &poll.answers {
            (answer.id, answer.votes, answer.mine).hash(hasher);
        }
    }
}
