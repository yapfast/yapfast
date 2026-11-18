//! What opens over the window: one [`Layer`] for each of the Slint window's
//! layers (`YapfastWindow`, `ui/yapfast.slint:4560` to the end) and its
//! popup windows (`:2068`-`:2730`), named after what each shows and drawn
//! in the order of that file, popups last. Each names the line it ports and
//! the task whose view it shows. Where one view shows several of Slint's
//! that draw together (the server and tag cards; the emoji and sticker
//! cards; leaving a server and a moved channel's permissions), they are one
//! layer.
//!
//! What Slint draws over the window for the pointer alone is not a layer
//! here but its area's own overlay, as `anomi_widgets::tip` draws one: the
//! tooltip and a badge's card (`:5633`, `:5648`), who reacted (`:5446`), a
//! voice channel's people (`:5647`), the autoscroll mark (`:5652`), and the
//! composer's suggestions above it (`:4649`-`:4688`). The window's bar and
//! its resize edges (`:5581`-`:5630`) are the chrome's.

use crate::menu::Spot;

/// A layer over the window. The order of the variants is the order they
/// draw in, the last on top: the Slint file's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Layer {
    /// The stream watched, in a corner of the conversation while its call
    /// is off screen: `StreamPip` (`:4566`), V06's `pip`.
    Pip,
    /// The call's large tile over the whole window, until Escape
    /// (`:4597`), V04's `stage`.
    FullStage,
    /// A call ringing, at the top of the window: `IncomingCall` (`:4628`),
    /// V08's `incoming`.
    IncomingCall,
    /// Switching voice channels, or joining one to watch a stream, asks
    /// first: `SwitchDialog` (`:4640`), V08's.
    SwitchCall,
    /// The person's settings: `SettingsScreen` (`:4691`), S01's `frame`.
    Settings,
    /// A server's settings: `ServerSettingsScreen` (`:4843`), X01.
    ServerSettings,
    /// The dialogs a server's settings draw over themselves
    /// (`ui/server_settings.slint:466`-`:505`), each over a backdrop at
    /// 0.667: Security Actions (`SecurityActionsDialog`), X05's
    /// `safety::actions`.
    SecurityActions,
    /// A ban open: `BanDialog`, X06.
    Ban,
    /// Delete Role or Remove member: `RoleConfirmDialog`
    /// (`ui/roles.slint:664`), X09.
    RoleConfirm,
    /// Add Members to a role: `AddMembersDialog` (`ui/roles.slint:713`),
    /// X09.
    RoleAdding,
    /// An emoji's upload: `EmojiUploadDialog`
    /// (`ui/server_expressions.slint:182`), X10.
    EmojiUpload,
    /// A sticker's form: `StickerDialog` (`:341`), X11.
    StickerForm,
    /// Delete Server or Transfer Ownership: `OwnershipFrame`
    /// (`ui/server_ownership.slint:36`), X12.
    Ownership,
    /// A channel's settings: `ChannelSettingsScreen` (`:4869`), X13.
    ChannelSettings,
    /// The Permissions page's question or notice: `OverwriteConfirmDialog`
    /// (`ui/channel_settings.slint:322`), X14.
    PermissionsConfirm,
    /// Add members or roles to the Permissions: `AddOverwriteDialog`
    /// (`:383`), X14.
    PermissionsAdd,
    /// A channel's topic in full, under the header's line of it:
    /// `TopicPopover` (`:4879`), N07's.
    Topic,
    /// The open conversation's pins, under their button: `PinsPopover`
    /// (`:4903`), C22.
    Pins,
    /// The open channel's threads, under their button: `ThreadsPopover`
    /// (`:4928`), C23.
    Threads,
    /// Friends to add to the open DM, under its button: `AddPeoplePopover`
    /// (`:4960`), P13.
    AddPeople,
    /// A role mentioned in a message: who has it, under the card of one
    /// of them clicked there (`:4977`): `RoleCard` (`:4980`), P12's
    /// `cards`.
    RoleCard,
    /// A person's card beside the member list or what was clicked:
    /// `ProfileCard` (`:5013`), P07.
    Profile,
    /// A person's full profile: `ProfileModal` (`:5053`), P09.
    FullProfile,
    /// A mutual server's or a server tag's card, over the person's card or
    /// full profile it opened from (`open-server`, `:5090`): `ServerCard`,
    /// `TagCardView` (`:5100`, `:5151`), P12's `cards`.
    Card,
    /// An emoji's or a sticker's card under it: `EmojiCardView`,
    /// `MessageStickerCardView` (`:5126`, `:5177`), C08's `cards`.
    MessageCard,
    /// A picture large over the window: `ImageViewer` (`:5201`), C10.
    Viewer,
    /// A video large over the window: `VideoViewer` (`:5218`), C09.
    Video,
    /// `AddFriendDialog` (`:5229`), P03.
    AddFriend,
    /// `InviteDialog` (`:5238`), N12.
    Invite,
    /// `JoinDialog` (`:5246`), N12.
    Join,
    /// `EventDialog` (`:5257`), N12.
    Event,
    /// `GiftDialog` (`:5264`), N12.
    Gift,
    /// `OAuthDialog` (`:5272`), N12.
    OAuth,
    /// Making a channel or a category: `ChannelDialog` (`:5281`), X15.
    CreateChannel,
    /// `ProfileEditorDialog` (`:5288`), P14, with P16's name style window
    /// over it.
    ProfileEditor,
    /// Leaving a server, or whether a moved channel follows its category's
    /// permissions: the `ChannelDialog`s of `:5315` and `:5323`, N13's
    /// `leave`.
    Leave,
    /// About a DM's other person: `PersonDialog` (`:5332`), P11.
    Person,
    /// The custom status: `StatusDialog` (`:5342`), P10.
    Status,
    /// Making a poll: `PollDialog` (`:5358`), C07.
    Poll,
    /// Starting a DM: `NewDmDialog` (`:5381`), N11.
    NewDm,
    /// A message's reactions and who reacted: `ReactionsDialog` (`:5391`),
    /// C03.
    Reactions,
    /// Forwarding a message: `ForwardDialog` (`:5404`), C28.
    Forward,
    /// A masked link, before it opens: `LeavingDialog` (`:5418`), N13.
    LeavingLink,
    /// Ctrl+K: `QuickSwitcher` (`:5539`), N10.
    Switcher,
    /// A bot's form: `BotModalDialog` (`:5548`), C20.
    BotModal,
    /// A bot's select menu open under the box that opened it, in a message
    /// or a form: `BotSelectDropdown` (`:5561`), C20's.
    BotSelect,
    /// Two-factor: `MfaDialog` (`:5570`), S11.
    Mfa,
    /// Mentions and unreads, under the bar's Inbox: `InboxPopout`
    /// (`:5595`), N08.
    Inbox,
    /// A microphone's or headphones' menu: the `input-menu` and
    /// `output-menu` popups (`:2068`, `:2096`), V09's `devices`.
    Devices,
    /// The `soundboard-menu` popup (`:2134`), V12.
    Soundboard,
    /// The `share-menu` popup (`:2147`), V10.
    Share,
    /// The account's popover above its card, with the status submenu: the
    /// `status-menu` popup (`:2412`), N06's `AccountPopover`. The window's
    /// context menu (`vm.menu`, `:2311`, with the composer's + and an
    /// activity's menu) draws just under it.
    Account,
    /// The GIFs, Emoji and Stickers above the composer: the `expressions`
    /// popup (`:2542`), C18's `pickers`.
    Expressions,
    /// The emoji picker for a reaction or a status: the `emoji-picker`
    /// popup (`:2648`), C17.
    EmojiPicker,
    /// The call's connection: the `call-link-menu` popup (`:2672`), V02.
    Connection,
    /// A participant's menu: the `voice-menu` popup (`:2686`), V11.
    ParticipantMenu,
    /// A message's menu: the `message-menu` popup (`:2730`), C12.
    MessageMenu,
}

impl Layer {
    /// Every layer, in drawing order.
    pub const ALL: &[Layer] = &[
        Layer::Pip,
        Layer::FullStage,
        Layer::IncomingCall,
        Layer::SwitchCall,
        Layer::Settings,
        Layer::ServerSettings,
        Layer::SecurityActions,
        Layer::Ban,
        Layer::RoleConfirm,
        Layer::RoleAdding,
        Layer::EmojiUpload,
        Layer::StickerForm,
        Layer::Ownership,
        Layer::ChannelSettings,
        Layer::PermissionsConfirm,
        Layer::PermissionsAdd,
        Layer::Topic,
        Layer::Pins,
        Layer::Threads,
        Layer::AddPeople,
        Layer::RoleCard,
        Layer::Profile,
        Layer::FullProfile,
        Layer::Card,
        Layer::MessageCard,
        Layer::Viewer,
        Layer::Video,
        Layer::AddFriend,
        Layer::Invite,
        Layer::Join,
        Layer::Event,
        Layer::Gift,
        Layer::OAuth,
        Layer::CreateChannel,
        Layer::ProfileEditor,
        Layer::Leave,
        Layer::Person,
        Layer::Status,
        Layer::Poll,
        Layer::NewDm,
        Layer::Reactions,
        Layer::Forward,
        Layer::LeavingLink,
        Layer::Switcher,
        Layer::BotModal,
        Layer::BotSelect,
        Layer::Mfa,
        Layer::Inbox,
        Layer::Devices,
        Layer::Soundboard,
        Layer::Share,
        Layer::Account,
        Layer::Expressions,
        Layer::EmojiPicker,
        Layer::Connection,
        Layer::ParticipantMenu,
        Layer::MessageMenu,
    ];

    /// One of the Slint window's popup windows, drawn over every other
    /// layer and the window's context menu among them.
    pub fn popup(self) -> bool {
        self >= Layer::Devices
    }
}

/// A layer that shows: open, or playing its way out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opened {
    pub layer: Layer,
    /// It shows; `false` while it plays its way out.
    pub open: bool,
    /// Where it opens from, for a popover, a card or a menu: the button
    /// under which it opens, the row beside which, the pointer. Without
    /// one it opens where its kind does (a dialog in the middle).
    pub at: Option<Spot>,
}
