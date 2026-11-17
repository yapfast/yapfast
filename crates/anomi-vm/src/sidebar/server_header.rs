//! The server's header at the top of its channel list: its name, banner,
//! badge and invite button, and under them the boost goal and the events
//! row. The Slint side: `ServerHeader` (`ui/rail.slint:946`) and the header
//! box in `ui/yapfast.slint:2817`, fed by `show_sidebar` in
//! `src/ui/refresh.rs:1059`. Task N05 (docs/iced/tasks/shell.md). The menu
//! it opens is the window's (`vm.menu`, F06); this only asks for it.

use iced::widget::image;

use crate::menu::Spot;

/// Boosts Discord's first goal wants, which the boost bar counts toward.
pub const BOOST_GOAL: u32 = 33;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerHeader {
    pub name: String,
    /// The server's menu is open: the chevron turns over and the name stays
    /// lit.
    pub open: bool,
    /// A Community server, which starts its name with Discord's badge.
    pub community: bool,
    /// Verified or partnered: Discord's green badge takes the Community
    /// badge's place.
    pub verified: bool,
    /// The server's banner, behind the name, which turns white. Where it is
    /// still on its way the header is as tall as with it, and bare.
    pub banner: crate::kind::Expected,
    /// The invite button beside the name.
    pub invite: bool,
    /// The boosts, for the goal bar under the name; `None` draws no bar.
    pub boosts: Option<u32>,
    /// How many events the server has, for the row under the name; `None`
    /// draws no row (a Community server always has one, with zero events).
    pub events: Option<u32>,
}

impl ServerHeader {
    /// Whether the banner's room is kept, which makes the header taller.
    pub fn has_banner(&self) -> bool {
        !matches!(self.banner, crate::kind::Expected::None)
    }

    /// The banner's picture, once it is here.
    pub fn banner_picture(&self) -> Option<&image::Handle> {
        self.banner.ready()
    }

    /// How full the goal bar is, 0 to 1.
    pub fn boost_share(&self) -> f32 {
        self.boosts
            .map_or(0.0, |boosts| (boosts as f32 / BOOST_GOAL as f32).min(1.0))
    }
}

/// What the server's header asks for. Keep `#[non_exhaustive]`: the app
/// matches it with a wildcard until its I task handles every variant.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Intent {
    /// The name was pressed (or Enter, Space): open the server's menu
    /// there.
    Menu(Spot),
    /// The invite button: invite people to this server.
    Invite,
    /// The events row: its menu opens there.
    Events(Spot),
}
