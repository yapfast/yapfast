//! The card both dialogs sit on: raised, edged, 12 px corners and the
//! shadow under it (`background: Theme.raised; border-radius: 12px;
//! drop-shadow-blur: 32px; drop-shadow-offset-y: 12px; #00000066`,
//! `ui/voice.slint:1222`, `:1288`).

use iced::widget::container;

use anomi_theme::{Theme, catalog, metrics};

pub fn card(theme: &Theme) -> container::Style {
    let raised = catalog::container::raised(theme);
    container::Style {
        border: raised.border.rounded(metrics::RADIUS_DIALOG),
        shadow: catalog::float::dialog(theme).shadow,
        ..raised
    }
}
