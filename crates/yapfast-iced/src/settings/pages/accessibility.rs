//! Accessibility, laid out as Discord's page: a preview, the text size in
//! chat, and reduced motion with when GIFs, emoji and stickers play; and
//! what reduced motion changes as it comes and goes.

use iced::{
    Element,
    widget::{column, row, text},
};

use super::{ANIMATES, Pref, pref};
use crate::settings::Page;
use crate::{
    app::Message,
    controls::{choices, slider},
    ctx::{Cx, Pref as Shown, Request},
    fonts::MEDIUM,
    form::{self, switch},
    layout::{SCROLL_SPEEDS, TEXT_SIZES},
};

impl<'a> Page<'a> {
    pub(in crate::settings) fn accessibility_page(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let layout = self.cx.layout;
        let size = super::message_size(&layout.prefs);
        let preview = column![
            text("Preview").size(14).font(MEDIUM).color(theme.muted),
            self.sample(vec![
                text("what happened to all the beans")
                    .size(size)
                    .color(theme.text)
                    .into(),
                row![
                    text("here's a link ").size(size).color(theme.text),
                    text("https://discord.com/accessibility")
                        .size(size)
                        .color(theme.link),
                ]
                .wrap()
                .into(),
            ]),
        ]
        .spacing(8);
        // Discord's markers, from 12 px to 24 px; the slider's values start
        // at the smallest.
        let px = layout.prefs.text_px();
        let (least, most) = (TEXT_SIZES[0], TEXT_SIZES[TEXT_SIZES.len() - 1]);
        let marks = TEXT_SIZES
            .iter()
            .map(|&size| (u32::from(size - least), format!("{size}px")))
            .collect();
        let text_size = slider(
            "Text size in chat",
            u32::from(px - least),
            u32::from(most - least),
            move |value| pref(Pref::TextPx(least.saturating_add(value as u8))),
        )
        .step(1)
        .marks(marks)
        .shown(format!("{px}px"))
        .on_release(pref(Pref::KeepTextPx))
        .view(theme);
        let scroll_speed = SCROLL_SPEEDS
            .iter()
            .position(|&speed| speed == layout.prefs.scroll_speed())
            .unwrap_or(1);
        let reduced = layout.reduced_motion();
        let sync = layout.sync_reduced_motion;
        let animate = ANIMATES
            .iter()
            .position(|kind| *kind == layout.animate())
            .unwrap_or(0);
        let mut plays = vec![
            form::heading(theme, "PLAY GIFS, EMOJI AND STICKERS"),
            choices(
                theme,
                &["While Yapfast is focused", "Always", "Never"],
                animate,
                |index| pref(Pref::Animate(index)),
            ),
        ];
        if layout.animate_stilled && reduced {
            plays.push(form::note(
                theme,
                "This setting has been automatically disabled because of your Reduced Motion setting.",
            ));
        }
        form::page([
            form::title(theme, "Accessibility"),
            preview.into(),
            form::group([
                form::heading(theme, "TEXT READABILITY"),
                text_size,
                form::note(theme, "Adjust the size of the chat font."),
            ]),
            form::rule(theme),
            form::group([
                form::heading(theme, "SCROLL SPEED"),
                choices(
                    theme,
                    &["50%", "100%", "150%", "200%", "300%"],
                    scroll_speed,
                    |index| pref(Pref::ScrollSpeed(index)),
                ),
            ]),
            form::rule(theme),
            form::group([
                form::heading(theme, "REDUCED MOTION"),
                switch(
                    "Enable Reduced Motion",
                    reduced,
                    Some(pref(Pref::ReducedMotion(!reduced))),
                )
                .description(
                    "Reduce the amount and intensity of animations, hover effects, and other moving effects: themes' animations stay still, and GIFs, emoji and stickers stop playing.",
                )
                .view(self.cx.look),
                switch(
                    "Sync with computer setting",
                    sync,
                    Some(pref(Pref::SyncReducedMotion(!sync))),
                )
                .description("Reduced motion follows whether your desktop turns its animations off.")
                .view(self.cx.look),
            ]),
            form::group(plays),
        ])
    }
}

/// Reduced motion turned on or off here or on Themes: the person's own
/// choice, which stops following the desktop, as Discord's switch does.
pub(in crate::settings) fn choose_reduced_motion(cx: &mut Cx<'_>, on: bool) {
    let before = cx.layout.reduced_motion();
    cx.layout.reduce_motion = on;
    cx.keep("reduce_motion", on);
    if cx.layout.sync_reduced_motion {
        cx.layout.sync_reduced_motion = false;
        cx.keep("sync_reduced_motion", false);
    }
    motion_changed(cx, before);
}

/// Following the desktop's reduced motion, or no longer: turned off, the
/// person's choice becomes what the desktop last said, so nothing moves
/// differently, as Discord's client does.
pub(super) fn sync_reduced_motion(cx: &mut Cx<'_>, on: bool) {
    let before = cx.layout.reduced_motion();
    cx.layout.sync_reduced_motion = on;
    cx.keep("sync_reduced_motion", on);
    if on {
        cx.request(Request::ReadSystemMotion);
    } else {
        let system = cx.layout.system_reduced_motion;
        cx.layout.reduce_motion = system;
        cx.keep("reduce_motion", system);
    }
    motion_changed(cx, before);
}

/// The desktop said whether it reduces motion: while synced to it, the
/// window follows.
pub fn system_motion(cx: &mut Cx<'_>, reduce: bool) {
    let before = cx.layout.reduced_motion();
    cx.layout.system_reduced_motion = reduce;
    motion_changed(cx, before);
}

/// What follows reduced motion, once it came or went: the themes' motion
/// and, as Discord's client overrides its animation settings, GIFs, emoji
/// and stickers, stilled as it comes and playing as chosen again as it
/// goes.
fn motion_changed(cx: &mut Cx<'_>, before: bool) {
    let now = cx.layout.reduced_motion();
    if now == before {
        return;
    }
    cx.themes.motion_changed(cx.layout.theme_performance, now);
    if cx.layout.animate_stilled != now {
        cx.layout.animate_stilled = now;
        cx.keep("animate_stilled", now);
    }
    cx.request(Request::Changed(Shown::Theme));
    cx.touch();
}
