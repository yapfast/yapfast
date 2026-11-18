//! Linking Discord from the settings' Discord page: the panel of
//! [`super::discord`] past its first phase, in a dialog over the settings
//! (`Layer::Link`). The page itself says what linking does and risks and
//! offers the code, as `DiscordLinkSettings` (`ui/settings.slint:2110`)
//! starts; the code, who scanned it, the token, the captcha and why it
//! failed with "Try again" show here, as the same panel does in that page.
//! The X, Escape and a press beside it stop the linking.

use anomi_theme::prelude::*;
use anomi_vm::Intent;
use anomi_vm::shell::{self, Layer};
use anomi_vm::sign_in::SignIn;

use crate::places::leave::card;

/// The dialog's width: the code's 184 px box and the words beside it
/// (`ui/discord.slint:104`) with room for a line of them.
const WIDTH: f32 = 520.0;

pub fn dialog(sign_in: &SignIn, motion: Motion) -> Element<'_, Intent> {
    let close = Intent::Shell(shell::Intent::Close(Layer::Link));
    let parts = vec![
        // The page's own title, the brand's name in every language.
        card::title_row("Discord", close, motion),
        super::discord::panel(sign_in, motion)
            .map(Intent::from)
            .boxed(),
    ];
    card::card(WIDTH, parts).boxed()
}
