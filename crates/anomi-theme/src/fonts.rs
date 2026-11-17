//! The fonts the interface draws with, and how the text engine takes them.
//!
//! - DM Sans is the interface's face ([`typography::UI`](crate::typography::UI)),
//!   upright and italic, variable in weight; a font the person loads replaces
//!   it under the same family ([`custom`]), so every `Font` naming DM Sans draws
//!   theirs.
//! - [`EMOJI`] is Twemoji's colour font, which Discord's client draws emoji
//!   from: any text whose own font lacks an emoji draws Twemoji's picture, a
//!   flag or a skin tone joined into its one picture, and never a system
//!   emoji font's ([`twemoji`] for the glyphs).
//! - [`name`] gives a display name style's face, by family, at any weight.
//!
//! [`install`] puts all of it in iced's font system. Whatever draws the
//! interface must call it once before its first frame: the program before
//! `iced::daemon(..)`, `anomi-shots` before it renders, each test binary
//! before it builds a `Simulator`. Loading only [`ALL`] through iced
//! (`Settings::fonts`, `.fonts(..)`, `load_font`) draws, but with what
//! iced's defaults give: each font copied onto the heap, Twemoji and the name
//! faces at their own weight only (a face whose weight iced can't name draws
//! in a fallback font), and the system's emoji font before Twemoji where one
//! is installed, so emoji draw in its pictures, some monochrome, and skin
//! tones and flags apart.
//!
//! When the person's font changes, the program calls
//! [`custom::Loaded::load`], which makes every paragraph shape again, and
//! runs the task [`redrawn`] gives, which makes each window's renderer anew,
//! so the text it drew outside paragraphs shapes again too, then moves its
//! [`Repaint`], so the next frame draws every pixel again.
//!
//! Ports the `Twemoji` global's font (`ui/theme.slint`), `src/emoji_font.rs`
//! and `src/han_fallback.rs` (the fallback, `chain.rs`), `ui/fonts/README.md`
//! (the faces) and `src/custom_font.rs` ([`custom`]). Task F03.

mod chain;
pub mod custom;
mod faces;
mod repaint;
#[cfg(test)]
mod tests;
mod weights;

use std::borrow::Cow;
use std::sync::Once;

use iced::Font;
use iced::advanced::graphics::text::font_system;
use iced::font::Weight;

pub use anomi_fonts::names::Name;
pub use anomi_fonts::twemoji;
pub use repaint::Repaint;

/// The family of DM Sans, which [`typography::UI`](crate::typography::UI)
/// names, and which the person's font takes when they load one.
pub const SANS: &str = "DM Sans";

/// DM Sans, upright and italic, variable in weight and optical size.
pub static DM_SANS: &[u8] = include_bytes!("../../../ui/fonts/DMSans[opsz,wght].ttf");
pub static DM_SANS_ITALIC: &[u8] = include_bytes!("../../../ui/fonts/DMSans-Italic[opsz,wght].ttf");

/// Every font the interface draws with: DM Sans, Twemoji, and the eleven
/// display name faces.
pub static ALL: [&[u8]; 14] = [
    DM_SANS,
    DM_SANS_ITALIC,
    anomi_fonts::twemoji::FONT,
    anomi_fonts::names::SAKURA,
    anomi_fonts::names::JELLYBEAN,
    anomi_fonts::names::MODERN,
    anomi_fonts::names::MEDIEVAL,
    anomi_fonts::names::EIGHT_BIT,
    anomi_fonts::names::VAMPYRE,
    anomi_fonts::names::TEMPO,
    anomi_fonts::names::MONKEY_BARS,
    anomi_fonts::names::MAINFRAME,
    anomi_fonts::names::HEADBANG,
    anomi_fonts::names::JOURNAL,
];

/// Twemoji's family: text in it draws each emoji as Twemoji's picture, and
/// a glyph of [`twemoji::text`] as the picture it stands for. Text in any
/// other font falls back to it for an emoji it lacks.
pub const EMOJI: Font = Font::new(twemoji::FAMILY);

/// The face a display name style draws in. Each has one weight, which
/// [`install`] registers at every weight, so the name takes the weight its
/// row asks for and draws in the face's own; the weight given here is the
/// nearest iced names to the face's. Without [`install`] a face matches only
/// at its own weight, so one whose weight iced can't name (Vampyre's
/// Sinistre, 548, asked for at Medium) draws in a fallback font instead.
pub const fn name(face: Name) -> Font {
    let weight = match face.weight() {
        0..=450 => Weight::Normal,
        451..=550 => Weight::Medium,
        551..=650 => Weight::Semibold,
        _ => Weight::Bold,
    };
    Font::new(face.family()).weight(weight)
}

/// What the program runs after the person's font changes
/// ([`custom::Loaded::load`]), for the text each renderer drew outside
/// paragraphs (`fill_text`: the rail's initials and badges, a button's
/// label, a placeholder, a pick list's choice): the renderer keeps that as it
/// shaped it, and its glyphs, in caches loading a font doesn't empty
/// (tiny-skia's compositor leaves them alone). So this sets the program's
/// defaults again, the interface's font and iced's text size (`run.rs` sets
/// none; the two must stay the same), which makes every window's renderer
/// anew, its caches empty, and lays every window out again. The images the
/// old renderers held are made again from their handles as they're drawn.
/// Then it sends `repainted`.
///
/// New glyphs aren't yet new pixels: tiny-skia repaints only what differs
/// from the frame last drawn into the same buffer, and the text compares
/// equal, its `Font` still naming DM Sans. So the program does three things:
///
/// - returns this task after [`custom::Loaded::load`];
/// - on `repainted`, takes the next [`Repaint`]
///   (`self.repaint = self.repaint.next()`), and nowhere else;
/// - gives iced its background through it: `.style(|app, theme|
///   app.repaint.style(theme))` on the daemon in `run.rs`.
///
/// `repainted` comes after iced has made the renderers anew, as the action
/// and the message go through one queue in order (or are both done before
/// the next frame, when the update finds them ready). So the first frame
/// with the new background draws with the new renderers, and repaints every
/// pixel. A background moved before the renderers would repaint once in the
/// old glyphs and leave them there.
pub fn redrawn<Message: Send + 'static>(repainted: Message) -> iced::Task<Message> {
    iced::font::set_defaults(
        crate::typography::UI,
        iced::advanced::renderer::Settings::default().text_size,
    )
    .chain(iced::Task::done(repainted))
}

/// Puts the interface's fonts in iced's font system, as they are in the
/// program (nothing copies them), with the fallback the interface wants:
/// Twemoji first for what a font lacks, then DM Sans, then the system's
/// fonts, Chinese, Japanese and Korean in the order of the system's language,
/// and no system emoji font at all. Single-weight faces (Twemoji, the name
/// faces) answer every weight, as cosmic-text takes a face only at its own
/// weight or a variable one's; so does each system font the fallback names,
/// with its nearest face at a weight it lacks ([`weights`]), so Russian at
/// Semibold draws in Noto Sans Bold rather than any font that has Semibold.
///
/// Once per process; later calls do nothing. Fonts of [`ALL`] that iced
/// loaded before are taken out, and iced loads none of them after, as it
/// loads a borrowed font once.
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let mut system = font_system().write().expect("the font system");
        // iced keeps the address of each borrowed font it loaded and loads it
        // no more: loading ALL here makes a later load of it do nothing. The
        // faces it made, heap copies, go with those loaded before.
        for font in ALL {
            system.load_font(Cow::Borrowed(font));
        }
        let raw = system.raw();
        faces::forget(raw.db_mut());
        faces::register(raw.db_mut());
        weights::system(raw);
        chain::rebuild(raw, 0);
        faces::refresh(&mut system);
    });
}
