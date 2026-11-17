//! The tab bar (`ProfileTabBar`, `ui/people.slint:1279`): 48 px of tabs in
//! 14 px semibold, each 24 px apart, the shown one underlined by a 2 px rule
//! as wide as its words, over a 1 px line. Words too long for the row
//! are shortened.

use iced::widget::{Column, Row, button, container, space, text};
use iced::{Background, Border, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, typography};
use anomi_vm::people::full_profile::{FullProfile, Intent, Page};
use yapfast_lang::tr;

use super::look::Look;

const HEIGHT: f32 = 48.0;
const GAP: f32 = 24.0;
const RULE: f32 = 2.0;

/// The tab's words: Discord's own for the mutual tabs, which the client
/// names with their counts.
fn label(full: &FullProfile, page: Page) -> &str {
    match page {
        Page::Board => tr!("Board"),
        Page::Activity => tr!("Activity"),
        Page::Wishlist => tr!("Wishlist"),
        Page::Friends => &full.friends_label,
        Page::Servers => &full.servers_label,
    }
}

/// The bar over the shown page.
pub fn bar<'a>(full: &'a FullProfile, look: Look) -> Element<'a, Intent> {
    let shown = full.page();
    let tabs = full
        .tabs()
        .into_iter()
        .filter(|(_, present)| *present)
        .map(|(page, _)| tab(label(full, page), page, page == shown, look));
    let rule = container(space().height(1))
        .width(Fill)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(look.line(theme.palette()))),
            ..container::Style::default()
        });
    Column::<Element<'a, Intent>>::new()
        .push(Row::with_children(tabs).height(HEIGHT).boxed())
        .push(rule.boxed())
        .boxed()
}

fn tab<'a>(words: &'a str, page: Page, selected: bool, look: Look) -> Element<'a, Intent> {
    let face = text(words)
        .size(typography::size::SMALL)
        .font(typography::SEMIBOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(catalog::text::inherit);
    let rule = container(space().height(RULE))
        .width(Fill)
        .style(move |theme: &Theme| container::Style {
            background: selected.then(|| Background::Color(look.ink(theme.palette()))),
            border: Border::default().rounded(1.0),
            ..container::Style::default()
        });
    let body = Column::<Element<'a, Intent>>::new()
        .width(Length::Shrink)
        .push(container(face).center_y(HEIGHT - RULE).boxed())
        .push(rule.boxed());
    button(body)
        .padding(Padding {
            right: GAP,
            ..Padding::ZERO
        })
        .on_press(Intent::Choose(page))
        .style(move |theme: &Theme, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            let palette = theme.palette();
            button::Style {
                text_color: if selected || hovered {
                    look.ink(palette)
                } else {
                    look.ink2(palette)
                },
                ..button::Style::default()
            }
        })
        .boxed()
}
