//! The header's buttons at its right: befriending, a voice channel's chat,
//! the calls, threads, notifications, pins, adding people, the member list,
//! a thread's menu and a DM's profile; a place has some of them. Task N07's
//! (docs/iced/tasks/shell.md). The Slint side: `ui/yapfast.slint:3330`-`:3463`.

use iced::widget::{row, text};
use iced::{Alignment, Rectangle};

use anomi_theme::prelude::*;
use anomi_theme::{Palette, catalog, typography};
use anomi_vm::header::{Friend, Intent, Place, Title};
use anomi_vm::icon::Icon;
use anomi_vm::menu::{Align, Spot};
use anomi_widgets::button::{Kind, Side, action_button, header_toggle, icon_button};
use anomi_widgets::menu::opener;
use yapfast_lang::tr;

use super::tinted;

/// A button's picture (`glyph: 20px`).
const GLYPH: f32 = 20.0;

type Ink = fn(&Palette) -> iced::Color;

/// The ink of a toggle: bright while what it opens shows.
fn lit(open: bool) -> Ink {
    if open {
        |palette| palette.text
    } else {
        |palette| palette.secondary
    }
}

/// A button of the header's: 32 px with a 20 px picture and its tip below.
fn button<'a>(
    glyph: Icon,
    label: &'a str,
    motion: Motion,
) -> anomi_widgets::button::IconButton<'a, Intent> {
    icon_button(glyph, label, motion)
        .glyph(GLYPH)
        .tip_side(Side::Below)
}

/// A button that opens something under it, from where it is in the window;
/// `end` lines the popover's right edge up with the button's.
fn opens<'a>(
    face: impl View<Intent> + 'a,
    end: bool,
    ask: fn(Spot) -> Intent,
) -> Element<'a, Intent> {
    opener(face)
        .on_press(move |bounds: Rectangle| {
            let spot = Spot::below(bounds);
            ask(if end { spot.align(Align::End) } else { spot })
        })
        .boxed()
}

/// The buttons before the search field's place, and the count after it.
pub(super) fn view<'a>(
    place: &'a Place,
    motion: Motion,
) -> (Vec<Element<'a, Intent>>, Vec<Element<'a, Intent>>) {
    let tools = &place.tools;
    let direct = matches!(place.title, Title::Direct(_));
    let mut items: Vec<Element<'a, Intent>> = Vec::new();
    let mut push = |item: Element<'a, Intent>| items.push(item);
    if let Some(item) = friend(tools.friend, motion) {
        push(item);
    }
    if let Some(chat) = tools.voice_chat {
        let (glyph, label) = if chat {
            (Icon::Volume, tr!("Show voice channel"))
        } else {
            (Icon::MessageSquareText, tr!("Open chat"))
        };
        push(
            button(glyph, label, motion)
                .on_press(Intent::VoiceChat(!chat))
                .view()
                .boxed(),
        );
    }
    if let Some(call) = tools.call {
        let label = if call.going {
            tr!("Join Voice Call")
        } else {
            tr!("Start Voice Call")
        };
        push(
            button(Icon::Phone, label, motion)
                .enabled(call.can)
                .on_press(Intent::Call)
                .view()
                .boxed(),
        );
        if !call.going {
            push(
                button(Icon::Video, tr!("Start Video Call"), motion)
                    .enabled(call.can)
                    .on_press(Intent::VideoCall)
                    .view()
                    .boxed(),
            );
        }
    }
    if let Some(open) = tools.threads {
        let face = button(Icon::Thread, tr!("Threads"), motion).tint(lit(open));
        push(opens(face.view(), true, Intent::Threads));
    }
    if let Some(muted) = tools.notify {
        let (glyph, label) = if muted {
            (Icon::BellOff, tr!("Notification settings, muted"))
        } else {
            (Icon::Bell, tr!("Notification settings"))
        };
        push(opens(
            button(glyph, label, motion).view(),
            false,
            Intent::Notifications,
        ));
    }
    if let Some(open) = tools.pins {
        let face = button(Icon::Pin, tr!("Pinned messages"), motion).tint(lit(open));
        push(opens(face.view(), true, Intent::Pins));
    }
    if let Some(adding) = tools.add_people {
        let label = if adding.one_to_one {
            tr!("Add friends to DM")
        } else {
            tr!("Add friends to the group")
        };
        let face = button(Icon::UserPlus, label, motion).tint(lit(adding.open));
        push(opens(face.view(), true, Intent::AddPeople));
    }
    if let Some(members) = &tools.members {
        let label = if members.showing {
            tr!("Hide member list")
        } else {
            tr!("Show member list")
        };
        push(if direct {
            header_toggle(Icon::Users, &members.label, label, members.showing, motion)
                .on_press(Intent::Members)
                .view()
                .boxed()
        } else {
            button(Icon::Users, label, motion)
                .tint(lit(members.showing))
                .on_press(Intent::Members)
                .view()
                .boxed()
        });
    }
    if tools.more {
        push(opens(
            button(Icon::Ellipsis, tr!("More"), motion).view(),
            false,
            Intent::More,
        ));
    }
    if let Some(showing) = tools.profile {
        let label = if showing {
            tr!("Hide profile")
        } else {
            tr!("Show profile")
        };
        push(
            button(Icon::User, label, motion)
                .tint(lit(showing))
                .on_press(Intent::ProfilePanel)
                .view()
                .boxed(),
        );
    }
    let mut after = Vec::new();
    if tools.members.is_none() && tools.profile.is_none() && !tools.count.is_empty() {
        after.push(count(&tools.count));
    }
    (items, after)
}

/// Add Friend, or Accept Friend Request, as a small button.
fn friend<'a>(friend: Friend, motion: Motion) -> Option<Element<'a, Intent>> {
    let (words, kind) = match friend {
        Friend::None => return None,
        Friend::Add => (tr!("Add Friend"), Kind::Secondary),
        Friend::Accept => (tr!("Accept Friend Request"), Kind::Primary),
    };
    Some(
        action_button(words, kind, motion)
            .leading(Icon::UserPlus)
            .small()
            .on_press(Intent::Befriend)
            .boxed(),
    )
}

/// The members' count: its icon, and the 6 px between it and the words
/// (`ui/yapfast.slint:3453`).
const COUNT_ICON: f32 = 18.0;
const COUNT_GAP: f32 = 6.0;

/// How many are here, where there is no list to open (`:3453`).
fn count(words: &str) -> Element<'_, Intent> {
    row![
        tinted(Icon::Users, COUNT_ICON, |palette| palette.muted),
        text(words)
            .size(typography::size::SMALL)
            .font(typography::MEDIUM)
            .style(catalog::text::muted),
    ]
    .spacing(COUNT_GAP)
    .align_y(Alignment::Center)
    .boxed()
}
