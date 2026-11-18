//! The tab bar (`ProfileTabBar`, `ui/people.slint:1279`) as the preview sets
//! it: 48 px of tabs in 14 px semibold, each 24 px apart, the shown one
//! underlined by a 2 px rule, over a 1 px line; `END` px are left at its end
//! for the close button over it.

use iced::widget::{Column, Row, button, container, space, text};
use iced::{Background, Border, Fill, Length, Padding};

use anomi_theme::prelude::*;
use anomi_theme::typography;
use anomi_vm::people::editor_preview::{EditorPreview, Intent, Page};
use yapfast_lang::tr;

use super::look::Look;

const HEIGHT: f32 = 48.0;
const GAP: f32 = 24.0;
const RULE: f32 = 2.0;
/// The room at the row's end (`end-room: 40px`, `ui/server.slint:2282`).
const END: f32 = 40.0;

fn label(page: Page) -> &'static str {
    match page {
        Page::Board => tr!("Board"),
        Page::Activity => tr!("Activity"),
        Page::Wishlist => tr!("Wishlist"),
    }
}

pub fn bar<'a>(preview: &'a EditorPreview, look: Look) -> Element<'a, Intent> {
    let tabs = Page::ALL
        .into_iter()
        .map(|page| tab(page, page == preview.page, look));
    let rule = container(space().height(1))
        .width(Fill)
        .style(move |theme: &Theme| container::Style {
            background: Some(Background::Color(look.line(theme.palette()))),
            ..container::Style::default()
        });
    Column::<Element<'a, Intent>>::new()
        .push(
            container(Row::with_children(tabs).height(HEIGHT))
                .padding(Padding::ZERO.right(END))
                .boxed(),
        )
        .push(rule.boxed())
        .boxed()
}

fn tab<'a>(page: Page, selected: bool, look: Look) -> Element<'a, Intent> {
    let face = text(label(page))
        .size(typography::size::SMALL)
        .font(typography::SEMIBOLD)
        .wrapping(text::Wrapping::None)
        .ellipsis(text::Ellipsis::End)
        .style(anomi_theme::catalog::text::inherit);
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
        .padding(Padding::ZERO.right(GAP))
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
