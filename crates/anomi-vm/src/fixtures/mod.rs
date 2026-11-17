//! Sample view-models after the fake Discord's account
//! (`testing/fake_discord.ts --dms 6 --servers 3`): Alice signed in, Bob and
//! a group among her DMs. Snapshot tests and `anomi-shots` draw these. Text
//! the projection or the core would translate goes through `tr!` here too,
//! so a shot shows the language `anomi-shots` chose; tests choose none and
//! read the code's English.
//!
//! One module per view-model module, at the same path and owned by the same
//! task: `fixtures::chat::timeline` holds the samples of
//! `crate::chat::timeline`. The whole window's samples are the shell's
//! ([`shell::dm_home`], [`shell::server_home`]); an area's shot starts from
//! one of them and fills its own field. F01 keeps this file: [`Shot`], the
//! module list and [`avatar`].

pub mod account;
pub mod browse;
pub mod chat;
pub mod chrome;
pub mod conversation;
pub mod header;
pub mod inbox;
pub mod kind;
pub mod menu;
pub mod new_dm;
pub mod people;
pub mod places;
pub mod rail;
pub mod settings;
pub mod shell;
pub mod sidebar;
pub mod sign_in;
pub mod switcher;
pub mod text;
pub mod voice;

use iced::Color;

use crate::Vm;
use crate::kind::Avatar;

/// One picture `anomi-shots` renders: the whole window in a theme at a
/// size, from a view-model. Each screen module's `shots()` lists its own.
#[derive(Clone, Copy)]
pub struct Shot {
    /// The area's folder, as `shell` or `dm`: the golden screenshots' folder
    /// where one matches.
    pub area: &'static str,
    /// The file name, without `.png`: the golden screenshot's where one
    /// matches.
    pub name: &'static str,
    /// The window's size in logical pixels.
    pub size: (f32, f32),
    /// A built-in theme's id.
    pub theme: &'static str,
    pub vm: fn() -> Vm,
}

/// The golden screenshots' window size, and their small one.
pub const GOLDEN: (f32, f32) = (1400.0, 860.0);
pub const SMALL: (f32, f32) = (1000.0, 640.0);

/// An avatar before its picture arrives: an initial on a tint (`0xRRGGBB`).
pub fn avatar(initial: &str, tint: u32) -> Avatar {
    Avatar {
        initial: initial.into(),
        tint: Color::from_rgb8((tint >> 16) as u8, (tint >> 8) as u8, tint as u8),
        picture: None,
    }
}
