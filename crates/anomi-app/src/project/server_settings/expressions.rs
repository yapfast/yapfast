//! Emoji and Stickers from the core's pages (`emoji_prompt`,
//! `stickers_prompt`, `src/ui/server_settings.rs:1232`, `:1302`), with the
//! upload dialog and the sticker's form over them. The emoji whose name is
//! being written, and the row under the pointer, are the window's.

use yapfast_core::application::{AppState, Application, EmojiPage, LEVEL_BOOSTS, StickersPage};
use yapfast_lang::{tr, trn};

use anomi_vm::kind::Expected;
use anomi_vm::settings::Settings;
use anomi_vm::settings::emoji::{Row, Table, Upload};
use anomi_vm::settings::stickers::{Card, Form, Tile};
use anomi_vm::shell::{Layer, Shell};

use super::dialog;
use crate::project::rows;

/// A picture chosen and read: its preview, once decoded.
fn preview(picture: &std::sync::Arc<yapfast_core::chat::Picture>) -> Expected {
    crate::pictures::shown(picture).map_or(Expected::Waiting, Expected::Ready)
}

/// Emoji: both tables, newest first, each under its heading, and the upload
/// dialog while it's open.
pub(super) fn emoji(
    core: &Application,
    server: &str,
    page: &EmojiPage,
    all: &mut Settings,
    shell: &mut Shell,
    focus: &mut Option<&'static str>,
) {
    let state = &core.state;
    let rules = core.expression_rules(server);
    let mut tables = vec![];
    for (animated, heading) in [(false, tr!("Emoji")), (true, tr!("Animated Emoji"))] {
        let rows: Vec<Row> = page
            .emojis
            .iter()
            .rev()
            .filter(|emoji| emoji.animated == animated)
            .map(|emoji| {
                let uploader = emoji.user.as_ref();
                let name = uploader.map_or("", |user| user.name.as_str());
                Row {
                    id: emoji.id.clone(),
                    name: emoji.name.clone(),
                    picture: rows::expected(state, Some(&emoji.picture)),
                    uploader: name.to_owned(),
                    uploader_avatar: rows::avatar(
                        state,
                        name,
                        uploader.map_or("", |user| user.id.as_str()),
                        uploader.map(|user| user.avatar.as_str()),
                    ),
                    may_change: rules.may_change(uploader.map(|user| user.id.as_str())),
                }
            })
            .collect();
        // A table with no emoji isn't shown.
        if rows.is_empty() {
            continue;
        }
        tables.push(Table {
            heading: heading.to_owned(),
            slots: match page.free(animated) {
                0 => tr!("no slots available").to_owned(),
                free => trn!("{n} slot available", "{n} slots available", free),
            },
            rows,
        });
    }
    let vm = &mut all.emoji;
    if vm.tables != tables {
        vm.tables = tables;
    }
    vm.slots = u32::try_from(page.slots()).unwrap_or(u32::MAX);
    vm.may_upload = (rules.manage || rules.create) && (page.free(false) > 0 || page.free(true) > 0);
    let upload = page.upload.as_ref().map(|upload| Upload {
        preview: upload
            .picture
            .as_ref()
            .map_or(Expected::None, |picture| preview(&picture.preview)),
        name: upload.name.clone(),
        saving: upload.saving,
        may_finish: upload.picture.is_some() && upload.name.chars().count() >= 2 && !upload.saving,
        error: upload.error.clone().unwrap_or_default(),
    });
    if dialog(shell, Layer::EmojiUpload, &mut vm.upload, upload) {
        *focus = Some(anomi_settings::server::emoji::NAME);
    }
}

/// Stickers: Discord's four cards, and the form while it's open.
pub(super) fn stickers(
    core: &Application,
    server: &str,
    page: &StickersPage,
    all: &mut Settings,
    shell: &mut Shell,
    _focus: &mut Option<&'static str>,
) {
    let state = &core.state;
    let rules = core.expression_rules(server);
    let cards: Vec<Card> = page
        .cards()
        .iter()
        .map(|card| {
            let free = card.slots.saturating_sub(card.stickers.len());
            Card {
                title: match card.level {
                    0 => tr!("Free Slots").to_owned(),
                    level => tr!("Level {}", level),
                },
                status: if card.unlocked {
                    trn!(
                        "{} of {n} slot available",
                        "{} of {n} slots available",
                        card.slots,
                        free
                    )
                } else {
                    trn!("{n} Boost", "{n} Boosts", LEVEL_BOOSTS[card.level])
                },
                unlocked: card.unlocked,
                next: card.next,
                tiles: card
                    .stickers
                    .iter()
                    .map(|sticker| Tile {
                        id: sticker.id.clone(),
                        name: sticker.form.name.clone(),
                        picture: rows::expected(state, sticker.picture.as_deref()),
                        uploader: sticker
                            .user
                            .as_ref()
                            .map(|user| user.name.clone())
                            .unwrap_or_default(),
                        may_change: rules.may_change(sticker.user_id.as_deref()),
                    })
                    .collect(),
            }
        })
        .collect();
    let vm = &mut all.stickers;
    if vm.cards != cards {
        vm.cards = cards;
    }
    vm.may_upload = (rules.manage || rules.create) && !page.full();
    let form = page.draft.as_ref().map(|draft| Form {
        editing: draft.editing.is_some(),
        preview: match &draft.file {
            Some(file) => preview(&file.preview),
            None => sticker_picture(state, page, draft.editing.as_deref()),
        },
        has_file: draft.file.is_some(),
        file_name: draft.file_name.clone(),
        tags: draft.form.tags.clone(),
        name: draft.form.name.clone(),
        description: draft.form.description.clone(),
        saving: draft.saving,
        may_save: draft.ready(),
        error: draft.error.clone().unwrap_or_default(),
    });
    dialog(shell, Layer::StickerForm, &mut vm.form, form);
}

/// The picture of the sticker being edited, Discord's.
fn sticker_picture(state: &AppState, page: &StickersPage, editing: Option<&str>) -> Expected {
    let url = editing
        .and_then(|id| page.stickers.iter().find(|sticker| sticker.id == id))
        .and_then(|sticker| sticker.picture.as_deref());
    rows::expected(state, url)
}
