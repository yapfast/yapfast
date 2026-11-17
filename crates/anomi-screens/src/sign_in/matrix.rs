//! The way in of a build with Matrix: a homeserver, a Matrix ID and a
//! password (`ui/sign-in.slint:79`). Every field and button is off while
//! `busy`; Enter in the password signs in; the Matrix ID takes the keys when
//! the app asks (`sign-in-focus`, with `super::USERNAME`).

use iced::Fill;
use iced::widget::{Id, Space, column, text};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::sign_in::{Intent, SignIn};
use anomi_widgets::button::{Kind, action_button};
use anomi_widgets::field::field;
use yapfast_lang::tr;

/// The sign-in button's height (`min-height: 44px`).
const BUTTON: f32 = 44.0;

pub fn view(sign_in: &SignIn, motion: Motion) -> Element<'_, Intent> {
    let off = sign_in.busy;
    let mut column = column![
        super::heading(tr!("Use your existing Matrix account.")),
        Space::new().height(metrics::SPACE_M),
        super::caption(tr!("HOMESERVER")),
        edited(
            field("", &sign_in.homeserver).id(super::HOMESERVER),
            off,
            Intent::Homeserver,
            motion
        ),
        Space::new().height(metrics::SPACE_XS),
        super::caption(tr!("MATRIX ID")),
        edited(
            field("@you:matrix.org", &sign_in.username).id(super::USERNAME),
            off,
            Intent::Username,
            motion,
        ),
        Space::new().height(metrics::SPACE_XS),
        super::caption(tr!("PASSWORD")),
        edited(
            field("", &sign_in.password)
                .id(super::PASSWORD)
                .password(true)
                .on_submit(Intent::Login),
            off,
            Intent::Password,
            motion,
        ),
        Space::new().height(metrics::SPACE_M),
        action_button(
            if off {
                tr!("Connecting...")
            } else {
                tr!("Sign in")
            },
            Kind::Primary,
            motion,
        )
        .width(Fill)
        .height(BUTTON)
        .id(Id::new(super::SIGN_IN))
        .enabled(!off)
        .on_press(Intent::Login),
    ]
    .spacing(metrics::SPACE_S)
    .width(Fill);
    if !sign_in.notice.is_empty() {
        column = column.push(
            text(sign_in.notice.as_str())
                .size(typography::size::BODY)
                .style(catalog::text::danger)
                .boxed(),
        );
    }
    if sign_in.restore_available && !off {
        column = column.push(
            action_button(tr!("Retry saved session"), Kind::Secondary, motion)
                .width(Fill)
                .id(Id::new(super::RETRY))
                .on_press(Intent::Restore)
                .boxed(),
        );
    }
    column
        .push(
            typography::label(tr!(
                "Sign in with your Matrix password. Single sign-on isn't available yet."
            ))
            .boxed(),
        )
        .boxed()
}

/// A field that edits while the screen isn't busy, and is off while it is.
fn edited<'a>(
    well: anomi_widgets::field::Field<'a, Intent>,
    off: bool,
    edit: fn(String) -> Intent,
    motion: Motion,
) -> Element<'a, Intent> {
    let well = well.motion(motion);
    if off {
        well.view()
    } else {
        well.on_input(edit).view()
    }
}
