//! The layout and the core: what the core takes of it before anything
//! starts (`Adapter::new`, `src/ui.rs:2535`-`:2578`), the channels it takes
//! before each backend starts (`:2954`), and what the core changed that the
//! layout keeps (`src/ui/refresh.rs:599`-`:660`; pinned DMs as
//! `src/ui/menus.rs:1208` keeps them, after any action that pins).

use yapfast_core::application::{Action, Application, Cosmetics, StickerUse};
use yapfast_core::chat::Sticker;

use super::{KeptSticker, KeptStickerUse, Layout, Saved};

/// Hands the core what it keeps of the layout: the reactions and stickers
/// used, the switches it acts on and the pinned DMs.
pub fn give(saved: &Saved, core: &mut Application) {
    core.remember_reaction_uses(saved.reactions.clone());
    core.remember_sticker_favorites(
        saved
            .sticker_favorites
            .iter()
            .map(|kept| sticker(&kept.id, &kept.name, kept.format, &kept.picture))
            .collect(),
    );
    core.remember_sticker_uses(
        saved
            .sticker_uses
            .iter()
            .map(|used| StickerUse {
                sticker: sticker(&used.id, &used.name, used.format, &used.picture),
                uses: used.uses,
            })
            .collect(),
    );
    let switches = [
        Action::ShowHiddenChannels(saved.show_hidden_channels),
        Action::ShowProfilePanel(!saved.profile_panel_hidden),
        Action::SyncDiscordReads(saved.sync_discord_reads.unwrap_or(true)),
        Action::ReplyPing(saved.reply_ping.unwrap_or(true)),
        Action::HideBlocked(saved.hide_blocked),
        Action::SkipCallPrompt(saved.skip_call_prompt),
        Action::ActivityCardsHidden(saved.activity_cards_hidden),
        Action::SetCosmetics(cosmetics(saved)),
        Action::StripTracking(saved.strip_tracking.unwrap_or(true)),
        Action::HideNameplates(!saved.nameplates.unwrap_or(true)),
        Action::SetPinned(saved.pinned.clone()),
    ];
    // Nothing runs yet: what they'd ask of a backend, none asks.
    for switch in switches {
        drop(core.dispatch(switch));
    }
}

/// Before a backend starts: the channel each server showed last, so a
/// server opens on it.
pub fn channels(layout: &Layout, core: &mut Application) {
    core.remember_channels(layout.channels.clone());
}

/// Keeps what the core changed that the layout holds. Runs after every
/// change to the core's state, so it compares in place and copies only
/// what changed.
pub fn follow(layout: &mut Layout, core: &Application) {
    if core.reaction_uses() != &layout.reactions {
        layout.edit(|saved| saved.reactions = core.reaction_uses().clone());
    }
    let uses = core.sticker_uses();
    let same_uses = uses.len() == layout.sticker_uses.len()
        && uses.iter().zip(&layout.sticker_uses).all(|(used, kept)| {
            used.uses == kept.uses
                && kept.id == used.sticker.id
                && kept.name == used.sticker.name
                && kept.format == used.sticker.format
                && kept.picture == used.sticker.picture
        });
    if !same_uses {
        layout.edit(|saved| saved.sticker_uses = uses.iter().map(kept_use).collect());
    }
    let favorites = core.sticker_favorites();
    let same_favorites = favorites.len() == layout.sticker_favorites.len()
        && favorites
            .iter()
            .zip(&layout.sticker_favorites)
            .all(|(sticker, kept)| same(kept, sticker));
    if !same_favorites {
        layout.edit(|saved| saved.sticker_favorites = favorites.iter().map(kept).collect());
    }
    let state = &core.state;
    // The pins are compared only once the people lists changed.
    if std::mem::replace(&mut layout.people, state.people_changes) != state.people_changes
        && state.pinned != layout.pinned
    {
        layout.edit(|saved| saved.pinned.clone_from(&state.pinned));
    }
    if std::mem::replace(&mut layout.selection, state.selection_version) != state.selection_version
        && !state.preview
        && state.account.is_some()
    {
        let last = core.last_channels();
        if last
            .iter()
            .any(|(server, room)| layout.channels.get(server) != Some(room))
        {
            layout.edit(|saved| saved.channels.extend(last));
        }
    }
}

/// The profile switches, as saved.
pub fn cosmetics(saved: &Saved) -> Cosmetics {
    let default = Cosmetics::default();
    Cosmetics {
        effects: saved.profile_effects.unwrap_or(default.effects),
        decorations: saved.decorations.unwrap_or(default.decorations),
        moving: saved.moving_cosmetics.unwrap_or(default.moving),
        name_styles: saved.name_styles.unwrap_or(default.name_styles),
        server_tags: saved.server_tags.unwrap_or(default.server_tags),
        frames: saved.profile_frames.unwrap_or(default.frames),
    }
}

fn sticker(id: &str, name: &str, format: u64, picture: &str) -> Sticker {
    Sticker {
        id: id.to_owned(),
        name: name.to_owned(),
        format,
        picture: picture.to_owned(),
        usable: true,
    }
}

fn same(kept: &KeptSticker, sticker: &Sticker) -> bool {
    kept.id == sticker.id
        && kept.name == sticker.name
        && kept.format == sticker.format
        && kept.picture == sticker.picture
}

fn kept(sticker: &Sticker) -> KeptSticker {
    KeptSticker {
        id: sticker.id.clone(),
        name: sticker.name.clone(),
        format: sticker.format,
        picture: sticker.picture.clone(),
    }
}

fn kept_use(used: &StickerUse) -> KeptStickerUse {
    KeptStickerUse {
        id: used.sticker.id.clone(),
        name: used.sticker.name.clone(),
        format: used.sticker.format,
        picture: used.sticker.picture.clone(),
        uses: used.uses,
    }
}
