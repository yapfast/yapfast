//! The dock, the stage and its tiles, call controls, devices, sharing and the
//! soundboard: one module each, the view-model and `Intent` of one task
//! (docs/iced/tasks.md). F01 keeps this file: its fields and variants are
//! wired for every module below, so a task fills only its own module.
//!
//! `tiles` has a variant but no field: the stage and picture in picture hold
//! their `tiles::Tile`s.

pub mod connection;
pub mod controls;
pub mod devices;
pub mod dock;
pub mod incoming;
pub mod join;
pub mod participant_menu;
pub mod pip;
pub mod share;
pub mod soundboard;
pub mod stage;
pub mod tiles;

/// What the dock, the stage and its tiles, call controls, devices, sharing
/// and the soundboard show, by area.
#[derive(Clone, Debug, Default)]
pub struct Voice {
    pub dock: dock::Dock,
    pub connection: connection::Connection,
    pub stage: stage::Stage,
    pub controls: controls::Controls,
    pub pip: pip::Pip,
    pub join: join::Join,
    pub incoming: incoming::Incoming,
    pub devices: devices::Devices,
    pub share: share::Share,
    pub participant_menu: participant_menu::ParticipantMenu,
    pub soundboard: soundboard::Soundboard,
}

/// What the dock, the stage and its tiles, call controls, devices, sharing
/// and the soundboard ask for, by area.
#[derive(Clone, Debug, PartialEq)]
pub enum Intent {
    Dock(dock::Intent),
    Connection(connection::Intent),
    Tiles(tiles::Intent),
    Stage(stage::Intent),
    Controls(controls::Intent),
    Pip(pip::Intent),
    Join(join::Intent),
    Incoming(incoming::Intent),
    Devices(devices::Intent),
    Share(share::Intent),
    ParticipantMenu(participant_menu::Intent),
    Soundboard(soundboard::Intent),
}

crate::parts!(Intent in Voice:
    Dock(dock::Intent),
    Connection(connection::Intent),
    Tiles(tiles::Intent),
    Stage(stage::Intent),
    Controls(controls::Intent),
    Pip(pip::Intent),
    Join(join::Intent),
    Incoming(incoming::Intent),
    Devices(devices::Intent),
    Share(share::Intent),
    ParticipantMenu(participant_menu::Intent),
    Soundboard(soundboard::Intent),
);
