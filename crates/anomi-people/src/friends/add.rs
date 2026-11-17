//! Adding a friend (`AddFriendDialog`, `ui/friends.slint:420`): a title with
//! its X, a line of explanation, a field for the username beside the Send
//! request button, and under them how the request went. The shell gives it
//! the dialog's backdrop, Escape and the way in; the card is drawn here.

use iced::widget::{column, container, row, text};
use iced::{Alignment, Background, Border, Fill, Shadow, Vector};

use anomi_theme::prelude::*;
use anomi_theme::{catalog, metrics, typography};
use anomi_vm::icon::Icon;
use anomi_vm::people::add_friend::{AddFriend, Intent, Note};
use anomi_widgets::button::{Kind, Side, action_button, icon_button};
use anomi_widgets::field::{Id, field};
use yapfast_lang::tr;

/// The card's width at most (`min(460px, root.width - 48px)`, `:437`).
const WIDTH: f32 = 460.0;
/// `padding: 20px; spacing: 12px`.
const PAD: f32 = 20.0;
const GAP: f32 = 12.0;
/// The title's size and line (`font-size: 18px`).
const TITLE_SIZE: f32 = 18.0;
const TITLE: f32 = 28.0;
/// The field's height, its text and padding (`:457`).
const FIELD: f32 = 36.0;
const FIELD_PAD: f32 = 10.0;
/// The space between the field and its button (`spacing: 8px`).
const BESIDE: f32 = 8.0;

/// The X's glyph (`IconButton` `size: 28px; glyph: 18px`, `:447`; the size
/// is `TITLE`).
const CLOSE_GLYPH: f32 = 18.0;
/// The dialog's shadow (`drop-shadow-blur: 32px; drop-shadow-color:
/// #00000066; drop-shadow-offset-y: 12px`, `:436`).
const SHADOW_BLUR: f32 = 32.0;
const SHADOW_DOWN: f32 = 12.0;
const SHADOW_ALPHA: u8 = 0x66;

/// The close X's id, to focus it from a test.
pub const CLOSE: &str = "add-friend-close";

/// The Send request button's id, to focus it from a test or a task.
pub const SEND: &str = "add-friend-send";

/// The field's id: the app focuses it as the dialog opens (`init`, `:456`)
/// with `anomi_widgets::field::focus`.
pub fn name_id() -> Id {
    Id::new("add-friend-name")
}

pub fn view(add_friend: &AddFriend, motion: Motion) -> impl View<Intent> + '_ {
    let title = row![
        text(tr!("Add a friend"))
            .size(TITLE_SIZE)
            .font(typography::BOLD)
            .wrapping(text::Wrapping::None)
            .ellipsis(text::Ellipsis::End)
            .width(Fill),
        icon_button(Icon::X, tr!("Close"), motion)
            .size(TITLE)
            .glyph(CLOSE_GLYPH)
            .on_press(Intent::Close)
            .face()
            .id(Id::new(CLOSE))
            .tipped(tr!("Close"), Side::Above),
    ]
    .align_y(Alignment::Center)
    .height(TITLE);
    let hint = typography::label(tr!(
        "Send someone a friend request on Discord with their username."
    ));
    let send = Intent::Send(add_friend.name.clone());
    let mut name = field(tr!("Username"), &add_friend.name)
        .id(name_id())
        .on_input(Intent::Name)
        .text_size(typography::size::BODY)
        .pad(FIELD_PAD)
        .height(FIELD)
        .motion(motion);
    let mut button = action_button(tr!("Send request"), Kind::Primary, motion)
        .id(Id::new(SEND))
        .enabled(add_friend.may_send());
    if add_friend.may_send() {
        name = name.on_submit(send.clone());
        button = button.on_press(send);
    }
    let mut card = column![
        title,
        hint,
        row![name.view(), button]
            .spacing(BESIDE)
            .align_y(Alignment::Center),
    ]
    .spacing(GAP)
    .padding(PAD);
    if let Some(line) = note(&add_friend.note) {
        card = card.push(line);
    }
    container(card).width(WIDTH).style(card_style)
}

/// The line under the field: green once it went, muted while it goes, red
/// when Discord refused (`:470`).
fn note(note: &Note) -> Option<Element<'_, Intent>> {
    let (words, style): (std::borrow::Cow<'_, str>, fn(&Theme) -> text::Style) = match note {
        Note::None => return None,
        Note::Sending => (tr!("Sending...").into(), catalog::text::muted),
        Note::Sent(name) => (tr!("Sent {} a friend request.", name).into(), sent),
        Note::Failed(error) => (error.as_str().into(), catalog::text::danger),
    };
    Some(
        text(words)
            .size(typography::size::SMALL)
            .width(Fill)
            .style(style)
            .boxed(),
    )
}

/// `Theme.live`.
fn sent(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().live),
        ..text::Style::default()
    }
}

/// `background: Theme.raised; border: edge 1px; radius 12px;` and the
/// dialog's shadow (32 px blur, 12 px down, `#00000066`).
fn card_style(theme: &Theme) -> container::Style {
    let palette = theme.palette();
    container::Style {
        background: Some(Background::Color(palette.raised)),
        border: Border::default()
            .rounded(metrics::RADIUS_DIALOG)
            .width(1)
            .color(palette.edge),
        shadow: Shadow {
            color: iced::Color::from_rgba8(0, 0, 0, SHADOW_ALPHA as f32 / 255.0),
            offset: Vector::new(0.0, SHADOW_DOWN),
            blur_radius: SHADOW_BLUR,
        },
        ..container::Style::default()
    }
}

/// Its shots for `anomi-shots`: the whole window over the sample dialog,
/// the first the golden screenshot's (`dialog/add_friend.png`).
#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use anomi_vm::Vm;
    use anomi_vm::fixtures::people::add_friend;
    use anomi_vm::fixtures::{self, conversation, shell};
    use anomi_vm::header::{Header, Place, Screen, Title, Tools};
    use anomi_vm::shell::Layer;

    use crate::shots::{GOLDEN, Shot};

    fn over(add_friend: AddFriend) -> Vm {
        let mut vm = shell::dm_with(conversation::friends);
        vm.header = Header::Open(Box::new(Place {
            title: Title::Screen(Screen::Friends { all: true }),
            tools: Tools::default(),
        }));
        if let anomi_vm::sidebar::Place::Dms(dms) = &mut vm.sidebar.place {
            dms.friends_selected = true;
            for row in &mut dms.rows {
                row.selected = false;
            }
        }
        vm.people.friends = fixtures::people::friends::all();
        vm.people.active_now = fixtures::people::active_now::around();
        let mut vm = shell::open(vm, Layer::AddFriend);
        vm.people.add_friend = add_friend;
        vm
    }
    let shot = |name, vm| Shot {
        area: "dialog",
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        shot("add_friend", || over(add_friend::empty())),
        shot("add_friend_typed", || over(add_friend::typed())),
        shot("add_friend_sending", || over(add_friend::sending())),
        shot("add_friend_sent", || over(add_friend::sent())),
        shot("add_friend_failed", || over(add_friend::failed())),
    ]
}
