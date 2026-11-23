//! Settings → Themes from the keyboard: Tab and Shift+Tab go from the tabs
//! through what each tab offers (Back, each theme's main button and "…" and
//! that menu's items, Try again, Find theme, New theme), a ring shows where
//! the keys are, and Enter or Space does it. iced moves no keys between
//! buttons by itself.

use iced::Task;

use super::{
    Settings, appearance,
    theme_marketplace::{self, Spot, Tab},
};
use crate::{app::Message, ctx::Cx};

/// What a key does here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKey {
    Next,
    Previous,
    Activate,
}

impl Settings {
    /// Where the keys can go on the page as it shows, in order.
    fn theme_spots(&self, cx: &Cx<'_>) -> Vec<Spot> {
        let mut spots: Vec<Spot> = [
            Tab::Installed,
            Tab::Marketplace,
            Tab::AddFromUrl,
            Tab::Create,
        ]
        .into_iter()
        .map(Spot::Tab)
        .collect();
        if self.creator.is_some() || self.editor.is_some() {
            return spots;
        }
        let market = &self.marketplace;
        match market.tab {
            Tab::Installed => {
                if cx.themes.previous().is_some() {
                    spots.push(Spot::Back);
                }
                for installed in &cx.themes.installed {
                    spots.push(Spot::Main(installed.id.clone()));
                    spots.push(Spot::More(installed.id.clone()));
                    if market.menu.as_deref() == Some(installed.id.as_str()) {
                        let package = installed
                            .id
                            .strip_prefix(crate::themes::INSTALLED)
                            .unwrap_or(&installed.id)
                            .to_owned();
                        if cx.themes.chosen_id() != installed.id {
                            spots.push(Spot::Try(installed.id.clone()));
                        }
                        if installed.previous.is_some() {
                            spots.push(Spot::RollBack(package.clone()));
                        }
                        spots.push(Spot::Remove(package));
                    }
                }
            }
            Tab::Marketplace => {
                if market.catalog_error.is_some() {
                    spots.push(Spot::Retry);
                }
            }
            Tab::AddFromUrl => spots.push(Spot::Find),
            Tab::Create => spots.push(Spot::NewTheme),
        }
        spots
    }

    /// Tab, Shift+Tab, Enter or Space on Settings → Themes.
    pub(super) fn theme_key(&mut self, key: ThemeKey, cx: &mut Cx<'_>) -> Task<Message> {
        let spots = self.theme_spots(cx);
        let at = self
            .marketplace
            .focus
            .as_ref()
            .and_then(|focus| spots.iter().position(|spot| spot == focus));
        match key {
            ThemeKey::Next | ThemeKey::Previous => {
                let count = spots.len();
                let next = match (at, key) {
                    (None, ThemeKey::Next) => 0,
                    (None, _) => count - 1,
                    (Some(at), ThemeKey::Next) => (at + 1) % count,
                    (Some(at), _) => (at + count - 1) % count,
                };
                self.marketplace.focus = spots.get(next).cloned();
                Task::none()
            }
            ThemeKey::Activate => {
                let Some(spot) = self.marketplace.focus.clone() else {
                    return Task::none();
                };
                let event = match &spot {
                    Spot::Tab(tab) => {
                        super::Event::Marketplace(theme_marketplace::Event::Tab(*tab))
                    }
                    Spot::Back => super::Event::Theme(appearance::Event::Revert),
                    Spot::Main(id) => {
                        if cx.themes.chosen_id() == id && cx.themes.trial().is_some() {
                            super::Event::Marketplace(theme_marketplace::Event::KeepTrial)
                        } else if cx.themes.chosen_id() == id {
                            super::Event::Theme(appearance::Event::TurnOff)
                        } else {
                            super::Event::Theme(appearance::Event::Choose(id.clone()))
                        }
                    }
                    Spot::More(id) => {
                        let open = self.marketplace.menu.as_deref() == Some(id.as_str());
                        super::Event::Marketplace(theme_marketplace::Event::Menu(
                            (!open).then(|| id.clone()),
                        ))
                    }
                    Spot::Try(id) => super::Event::Marketplace(
                        theme_marketplace::Event::TryInstalled(id.clone()),
                    ),
                    Spot::RollBack(id) => {
                        super::Event::Marketplace(theme_marketplace::Event::RollBack(id.clone()))
                    }
                    Spot::Remove(id) => {
                        let asked = self.marketplace.removing.as_deref() == Some(id.as_str());
                        super::Event::Marketplace(if asked {
                            theme_marketplace::Event::Uninstall(id.clone())
                        } else {
                            theme_marketplace::Event::AskUninstall(id.clone())
                        })
                    }
                    Spot::Retry => {
                        super::Event::Marketplace(theme_marketplace::Event::RetryCatalog)
                    }
                    Spot::Find => super::Event::Marketplace(theme_marketplace::Event::FindUrl),
                    Spot::NewTheme => super::Event::Theme(appearance::Event::Creator(
                        super::theme_creator::Event::Open,
                    )),
                };
                let task = self.update(event, cx);
                // The keys stay where they were while it's still there.
                if self.theme_spots(cx).contains(&spot) {
                    self.marketplace.focus = Some(spot);
                }
                task
            }
        }
    }
}
