//! Sizes, spacing and radii, in logical pixels: `ClientStyle` in
//! `ui/yapfast/style.slint` with its Discord style on, and the sizes the
//! shell's components hold. Views use these names rather than numbers, so a
//! size that changes changes in one place.

/// The server rail's width.
pub const RAIL_WIDTH: f32 = 72.0;
/// A server's tile on the rail.
pub const RAIL_TILE: f32 = 40.0;
/// A folder's tile on the rail.
pub const FOLDER_TILE: f32 = 48.0;
/// The gap between rail tiles.
pub const RAIL_GAP: f32 = 8.0;
/// The pill at the rail's edge: its width, and its height when the item is
/// selected, hovered and unread.
pub const RAIL_PILL_WIDTH: f32 = 4.0;
pub const RAIL_PILL_SELECTED: f32 = 40.0;
pub const RAIL_PILL_HOVERED: f32 = 20.0;
pub const RAIL_PILL_UNREAD: f32 = 8.0;

/// The channel and DM list's width, before someone drags its edge, and the
/// range dragging keeps it in.
pub const SIDEBAR_WIDTH: f32 = 304.0;
pub const SIDEBAR_MIN: f32 = 180.0;
pub const SIDEBAR_MAX: f32 = 380.0;
/// The conversation keeps at least this much beside the sidebar.
pub const CONVERSATION_MIN: f32 = 420.0;

/// Discord's header rows: 48 between its outline and the line under them,
/// 50 with both.
pub const HEADER_HEIGHT: f32 = 50.0;

/// The account card at the bottom of the sidebar, and its inset.
pub const ACCOUNT_HEIGHT: f32 = 57.0;
pub const ACCOUNT_INSET: f32 = 8.0;

/// A DM's row and a channel's, and Friends' and Requests' above the DMs.
pub const DM_ROW_HEIGHT: f32 = 42.0;
pub const NAV_ROW_HEIGHT: f32 = 34.0;
pub const CHANNEL_ROW_HEIGHT: f32 = 32.0;
/// How far a sidebar row sits from the sidebar's sides.
pub const ROW_INSET: f32 = 8.0;

/// The member list's and a DM profile's width.
pub const MEMBERS_WIDTH: f32 = 265.0;
pub const PROFILE_WIDTH: f32 = 320.0;

/// The message box.
pub const COMPOSER_HEIGHT: f32 = 58.0;
pub const COMPOSER_PADDING: f32 = 18.0;

/// The window's smallest size.
pub const WINDOW_MIN_WIDTH: f32 = 640.0;
pub const WINDOW_MIN_HEIGHT: f32 = 560.0;
/// Its size on a first start.
pub const WINDOW_WIDTH: f32 = 1280.0;
pub const WINDOW_HEIGHT: f32 = 800.0;

/// Avatars: in a DM row, on the account card, beside a message.
pub const AVATAR_ROW: f32 = 32.0;
pub const AVATAR_ACCOUNT: f32 = 40.0;
pub const AVATAR_MESSAGE: f32 = 40.0;
/// The presence dot at an avatar's corner, and the ring around it.
pub const PRESENCE_DOT: f32 = 10.0;
pub const PRESENCE_RING: f32 = 3.0;

/// Radii: a row, a field, a card, a dialog.
pub const RADIUS_ROW: f32 = 6.0;
pub const RADIUS_FIELD: f32 = 4.0;
pub const RADIUS_CARD: f32 = 8.0;
pub const RADIUS_DIALOG: f32 = 12.0;
/// A rail tile's corner, in the Discord style the same at rest and lit.
pub const RADIUS_TILE: f32 = 12.0;

/// The spacing scale.
pub const SPACE_XS: f32 = 4.0;
pub const SPACE_S: f32 = 8.0;
pub const SPACE_M: f32 = 12.0;
pub const SPACE_L: f32 = 16.0;
pub const SPACE_XL: f32 = 24.0;

/// Icons: in a row, in a header, in the composer.
pub const ICON_ROW: f32 = 20.0;
pub const ICON_HEADER: f32 = 24.0;
