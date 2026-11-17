//! Discord's bar, once there is something to save, under the profile and its
//! tabs (`UnsavedBar`, `ui/settings.slint:259`): its words start where the
//! profile's do; Reset undoes the changes, Save takes them to Discord.

use iced::widget::{Space, container, row, text};
use iced::{Alignment, Background, Fill, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::people::editor::{Editor, Intent};
use anomi_widgets::button::{Kind, action_button};
use yapfast_lang::tr;

/// The bar's height while it shows.
pub const HEIGHT: f32 = 60.0;

/// The bar, or nothing while there is nothing to save.
pub fn view<'a>(editor: &'a Editor, pad: f32, motion: Motion) -> Option<Element<'a, Intent>> {
    if !editor.changed && !editor.saving {
        return None;
    }
    let ready = editor.ready();
    let reset = action_button(tr!("Reset"), Kind::Ghost, motion).enabled(ready);
    let save = action_button(
        if editor.saving {
            tr!("Saving...")
        } else {
            tr!("Save Changes")
        },
        Kind::Primary,
        motion,
    )
    .enabled(editor.may_save());
    Some(
        container(
            row![
                text(tr!("Careful \u{2014} you have unsaved changes!"))
                    .size(14)
                    .font(typography::SEMIBOLD)
                    .width(Fill),
                Space::new().width(8.0),
                if ready {
                    reset.on_press(Intent::Undo)
                } else {
                    reset
                },
                if editor.may_save() {
                    save.on_press(Intent::Save)
                } else {
                    save
                },
            ]
            .spacing(8.0)
            .align_y(Alignment::Center),
        )
        .height(HEIGHT)
        .width(Fill)
        .padding(Padding::from([0.0, 16.0]).left(pad))
        .align_y(Alignment::Center)
        .style(|theme: &Theme| container::Style {
            background: Some(Background::Color(theme.palette().raised)),
            ..container::Style::default()
        })
        .boxed(),
    )
}
