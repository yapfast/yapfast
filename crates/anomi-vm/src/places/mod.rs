//! The dialogs about servers and links: invite, join, event, gift, authorize,
//! leave: one module each, the view-model and `Intent` of one task
//! (docs/iced/tasks.md). F01 keeps this file: its fields and variants are
//! wired for every module below, so a task fills only its own module.

pub mod event;
pub mod gift;
pub mod invite;
pub mod join;
pub mod leave;
pub mod leaving_link;
pub mod oauth;

/// What the dialogs about servers and links show, by area.
#[derive(Clone, Debug, Default)]
pub struct Places {
    pub invite: invite::Invite,
    pub join: join::Join,
    pub event: event::Event,
    pub gift: gift::Gift,
    pub oauth: oauth::OAuth,
    pub leave: leave::Leave,
    pub leaving_link: leaving_link::LeavingLink,
}

/// What the dialogs about servers and links ask for, by area.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    Invite(invite::Intent),
    Join(join::Intent),
    Event(event::Intent),
    Gift(gift::Intent),
    OAuth(oauth::Intent),
    Leave(leave::Intent),
    LeavingLink(leaving_link::Intent),
}

crate::parts!(Intent in Places:
    Invite(invite::Intent),
    Join(join::Intent),
    Event(event::Intent),
    Gift(gift::Intent),
    OAuth(oauth::Intent),
    Leave(leave::Intent),
    LeavingLink(leaving_link::Intent),
);
