//! The shell's layers: kept in drawing order, closing plays out before they
//! go, opening again keeps their place.

use iced::{Point, Rectangle, Size};

use super::*;

fn layers(shell: &Shell) -> Vec<(Layer, bool)> {
    shell.layers.iter().map(|o| (o.layer, o.open)).collect()
}

#[test]
fn layers_draw_in_the_slint_order_whatever_order_they_opened_in() {
    let mut shell = Shell::default();
    shell.open(Layer::Switcher);
    shell.open(Layer::Settings);
    shell.open(Layer::MessageMenu);
    shell.open(Layer::Pins);
    assert_eq!(
        layers(&shell),
        [
            (Layer::Settings, true),
            (Layer::Pins, true),
            (Layer::Switcher, true),
            (Layer::MessageMenu, true),
        ]
    );
}

#[test]
fn a_closed_layer_stays_until_its_way_out_ends() {
    let mut shell = Shell::default();
    shell.open(Layer::NewDm);
    shell.close(Layer::NewDm);
    assert_eq!(layers(&shell), [(Layer::NewDm, false)]);
    assert!(!shell.is_open(Layer::NewDm));
    shell.gone(Layer::NewDm);
    assert!(shell.layers.is_empty());
}

#[test]
fn opening_again_on_the_way_out_keeps_it() {
    let mut shell = Shell::default();
    shell.open(Layer::Invite);
    shell.close(Layer::Invite);
    shell.open(Layer::Invite);
    shell.gone(Layer::Invite);
    assert_eq!(layers(&shell), [(Layer::Invite, true)]);
}

#[test]
fn opening_from_another_spot_moves_it() {
    let mut shell = Shell::default();
    let first = Spot::below(Rectangle::new(
        Point::new(10.0, 10.0),
        Size::new(32.0, 32.0),
    ));
    let second = Spot::below(Rectangle::new(
        Point::new(90.0, 10.0),
        Size::new(32.0, 32.0),
    ));
    shell.open_at(Layer::Pins, first);
    shell.open_at(Layer::Pins, second);
    assert_eq!(shell.layers.len(), 1);
    assert_eq!(shell.opened(Layer::Pins).and_then(|o| o.at), Some(second));
}

#[test]
fn removing_takes_it_at_once() {
    let mut shell = Shell::default();
    shell.open(Layer::Pip);
    shell.remove(Layer::Pip);
    assert!(shell.opened(Layer::Pip).is_none());
}

#[test]
fn a_card_a_profile_opens_draws_over_it_and_a_role_card_under_it() {
    // A mutual server's or a tag's card opens from a person's card or full
    // profile (`ui/yapfast.slint:5090`); a member of a role, over its card
    // (`:4977`).
    assert!(Layer::FullProfile < Layer::Card && Layer::Profile < Layer::Card);
    assert!(Layer::RoleCard < Layer::Profile);
    let mut shell = Shell::default();
    shell.open(Layer::Card);
    shell.open(Layer::FullProfile);
    assert_eq!(
        layers(&shell),
        [(Layer::FullProfile, true), (Layer::Card, true)]
    );
}

#[test]
fn the_list_of_layers_is_in_drawing_order() {
    assert!(Layer::ALL.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(Layer::MessageMenu.popup() && Layer::Devices.popup());
    assert!(!Layer::Inbox.popup());
}

#[test]
fn what_hangs_off_the_window_is_transient_and_what_stands_on_its_own_is_not() {
    let transient = [
        Layer::Devices,
        Layer::Share,
        Layer::Account,
        Layer::Expressions,
        Layer::EmojiPicker,
        Layer::Connection,
        Layer::ParticipantMenu,
        Layer::MessageMenu,
        Layer::Pins,
        Layer::Topic,
        Layer::Inbox,
        Layer::Profile,
        Layer::MessageCard,
        Layer::FolderColour,
    ];
    assert!(transient.iter().all(|layer| layer.transient()));
    let stays = [
        Layer::FolderSettings,
        Layer::NewDm,
        Layer::Invite,
        Layer::Status,
        Layer::BotModal,
        Layer::BotSelect,
        Layer::Switcher,
        Layer::Settings,
        Layer::Ban,
        Layer::ProfileEditor,
        Layer::FullProfile,
        Layer::Viewer,
        Layer::IncomingCall,
        Layer::SwitchCall,
        Layer::CameraUnavailable,
        Layer::FullStage,
        Layer::Pip,
    ];
    assert!(stays.iter().all(|layer| !layer.transient()));
    // Every popup is.
    assert!(
        Layer::ALL
            .iter()
            .filter(|l| l.popup())
            .all(|l| l.transient())
    );
}
