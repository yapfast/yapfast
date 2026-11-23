//! A theme's server tree (`layout.rail: "tree"`): the direct messages,
//! the rail's folders and servers, and the categories and channels of each
//! open place, as one list drawn as a file tree.
//!
//! Each place opens and closes. The place the window is in is open unless
//! it was closed, the others closed unless they were opened; where a place
//! differs from that is kept in `layout.json` (`tree_open`), and going to a
//! place opens it again. Categories fold as the stock sidebar folds them
//! (`collapsed`). A closed place or category still shows its mentions, or
//! that it has unread messages, on its own line.
//!
//! Only open places' rows are made, once each time the lists are, and the
//! list builds only the rows near its viewport, so a closed place costs
//! nothing but its line.
//!
//! A click on a row gives the tree the keys until a press elsewhere: Up and
//! Down move, Right opens or steps into the first child, Left closes or
//! steps out to the parent, Enter (or Space) does what a click on the row
//! does, Home and End go to the first and last row, and Escape gives the
//! keys back to the composer. The row the keys are on is outlined once a
//! key moves it.

use iced::{
    Alignment, Color, Element, Length, Padding, Point, Rectangle, Size, Task,
    advanced::{Layout, Widget, layout, mouse as advanced_mouse, renderer, widget::Tree},
    keyboard::{self, Key, key::Named},
    widget::{Space, column, container, mouse_area, operation, row, scrollable::AbsoluteOffset},
};
use yapfast_core::application::{Action, Navigation};

use crate::{
    app::{App, Message, key},
    rail::{Kind, RailItem},
    style::scrollbar,
    sync::MARGIN,
    widgets::{RightPress, Rows, scroll_view},
};

use super::{Event, GAP, LIST, SideRow};

/// `tree_open`'s key for the direct messages.
pub const DMS: &str = "dm";
/// A place's line, and a category's.
const NODE: f32 = 22.0;
const HEADING: f32 = 22.0;
/// The width each level of the tree takes.
const STEP: f32 = 14.0;
/// The open and closed mark's box.
const MARK: f32 = 10.0;

/// One row of the tree.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeRow {
    /// Its key, the same while it's listed: the keys stay on it by this.
    pub key: String,
    /// The row it hangs from, by its place in the tree.
    pub parent: Option<usize>,
    /// Each level above its own, and whether that level's line runs on
    /// past it.
    pub guides: Vec<bool>,
    /// Its own branch, `└──` when it's its parent's last (true), else
    /// `├──`; none for the root and for a thread, which its channel's row
    /// draws its own way.
    pub branch: Option<bool>,
    pub kind: TreeKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TreeKind {
    /// The tree's root line, `~/`.
    Root(String),
    /// The direct messages, a folder or a server.
    Node {
        node: Node,
        name: String,
        open: bool,
        selected: bool,
        unread: bool,
        badge: String,
    },
    /// A category of a listed place.
    Heading {
        server: String,
        fold: String,
        name: String,
        open: bool,
        unread: bool,
        badge: String,
    },
    /// A row of the place the window is in, by its place in the sidebar's
    /// rows, which it's drawn from (with what may be dragged there).
    Open(usize),
    /// A row of another place open in the tree.
    Other(Box<SideRow>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    Dms,
    Folder(String),
    Server(String),
}

impl Node {
    fn key(&self) -> String {
        match self {
            Node::Dms => DMS.to_owned(),
            Node::Folder(id) => format!("folder:{id}"),
            Node::Server(id) => format!("server:{id}"),
        }
    }

    fn navigation(&self) -> Option<Navigation> {
        match self {
            Node::Dms => Some(Navigation::DirectMessages),
            Node::Server(id) => Some(Navigation::Server(id.clone())),
            Node::Folder(_) => None,
        }
    }
}

impl TreeRow {
    /// Whether the keys may stop on it.
    fn steppable(&self, rows: &[SideRow]) -> bool {
        let side = match &self.kind {
            TreeKind::Root(_) => return false,
            TreeKind::Node { .. } | TreeKind::Heading { .. } => return true,
            TreeKind::Open(index) => rows.get(*index),
            TreeKind::Other(row) => Some(row.as_ref()),
        };
        matches!(side, Some(SideRow::Room(_) | SideRow::Browse { .. }))
    }

    fn open(&self) -> Option<bool> {
        match &self.kind {
            TreeKind::Node { open, .. } | TreeKind::Heading { open, .. } => Some(*open),
            _ => None,
        }
    }
}

/// Whether the theme shown lays the sidebar out as a tree.
pub fn on(app: &App) -> bool {
    app.themes
        .package_layout(&app.theme.id)
        .is_some_and(|layout| layout.rail == Some(yapfast_core::themes::layout::RailSide::Tree))
}

/// Whether the tree opens one folder at a time, keeping it in
/// `tree_folder` rather than the rail's open folders.
pub fn one_folder(app: &App) -> bool {
    on(app)
        && app
            .themes
            .package_layout(&app.theme.id)
            .is_some_and(|layout| !layout.tree.expand_many)
}

/// The folders open where they're shown: the rail's, or the one folder of
/// a tree that opens one at a time.
pub fn folders_shown(app: &App) -> std::borrow::Cow<'_, std::collections::BTreeSet<String>> {
    if one_folder(app) {
        std::borrow::Cow::Owned(app.layout.tree_folder.iter().cloned().collect())
    } else {
        std::borrow::Cow::Borrowed(&app.layout.open_folders)
    }
}

/// A place's key in `tree_open`.
fn place_key(navigation: &Navigation) -> &str {
    match navigation {
        Navigation::DirectMessages => DMS,
        Navigation::Server(id) => id,
    }
}

/// Whether a place is open: as `tree_open` keeps it, else while the window
/// is in it.
pub fn place_open(app: &App, navigation: &Navigation) -> bool {
    app.layout
        .tree_open
        .get(place_key(navigation))
        .copied()
        .unwrap_or(app.client.state().navigation == *navigation)
}

/// Opens or closes a place, keeping only what differs from its default.
fn set_place(app: &mut App, navigation: &Navigation, open: bool) {
    let default = app.client.state().navigation == *navigation;
    let key = place_key(navigation).to_owned();
    if open == default {
        app.layout.tree_open.remove(&key);
    } else {
        app.layout.tree_open.insert(key, open);
    }
    crate::layout::store(app.client.root(), "tree_open", &app.layout.tree_open);
    app.lists_shown = None;
}

/// As the window goes to a place, or a channel is opened (by a click, a
/// notification or a jump to a mention), the way to it opens: its folder,
/// its place and its category. The place the window starts in stays as it
/// was left.
pub fn follow_place(app: &mut App) {
    let state = app.client.state();
    let now = (state.navigation.clone(), state.selected.clone());
    let Some((navigation, selected)) = app.sidebar.tree_seen.replace(now.clone()) else {
        return;
    };
    if navigation != now.0 {
        let key = place_key(&now.0);
        if app.layout.tree_open.get(key) == Some(&false) {
            app.layout.tree_open.remove(key);
            crate::layout::store(app.client.root(), "tree_open", &app.layout.tree_open);
        }
        // A server in a closed folder: the folder opens.
        if let Navigation::Server(server) = &now.0
            && let Some(folder) = app
                .client
                .state()
                .servers
                .iter()
                .find(|known| &known.id == server)
                .and_then(|known| known.folder.clone())
            && !folders_shown(app).contains(&folder.id)
        {
            if one_folder(app) {
                app.layout.tree_folder = Some(folder.id);
                crate::layout::store(app.client.root(), "tree_folder", &app.layout.tree_folder);
            } else {
                app.layout.open_folders.insert(folder.id);
                crate::layout::store(app.client.root(), "open_folders", &app.layout.open_folders);
            }
        }
    }
    if selected != now.1
        && let Navigation::Server(server) = &now.0
        && let Some(fold) = now.1.as_deref().and_then(|room| fold_of(app, room))
        && let Some(folded) = app.layout.collapsed.get_mut(server)
        && folded.remove(&fold)
    {
        if folded.is_empty() {
            app.layout.collapsed.remove(server);
        }
        crate::layout::store(app.client.root(), "collapsed", &app.layout.collapsed);
    }
}

/// What the category a room is listed under folds by, as the sidebar's
/// rows have it; a thread's is its channel's.
fn fold_of(app: &App, room: &str) -> Option<String> {
    let state = app.client.state();
    let find = |id: &str| state.rooms.iter().find(|room| room.id == id);
    let mut room = find(room)?;
    if let Some(parent) = room.parent.as_deref().and_then(find) {
        room = parent;
    }
    Some(if room.capabilities.categories {
        room.category_id
            .as_deref()
            .unwrap_or(&room.category)
            .to_owned()
    } else if room.voice {
        "VOICE CHANNELS".to_owned()
    } else {
        "TEXT CHANNELS".to_owned()
    })
}

/// A part of the tree before it's laid out: a row and what hangs from it.
struct Entry {
    kind: TreeKind,
    key: String,
    /// A thread, which hangs from its channel without a branch.
    bare: bool,
    children: Vec<Entry>,
}

/// The tree's rows, from the rail's servers and folders, and the rows of
/// each open place: the window's own place's from `app.sidebar.rows`.
pub fn build(app: &App) -> Vec<TreeRow> {
    let layout = app.themes.package_layout(&app.theme.id);
    let tree = layout.map(|layout| &layout.tree);
    let mut tops = Vec::new();
    // The direct messages first.
    let dms = Node::Dms;
    let open = place_open(app, &Navigation::DirectMessages);
    tops.push(Entry {
        key: dms.key(),
        kind: TreeKind::Node {
            node: dms,
            name: tree.map_or("dm", |tree| tree.dms.as_str()).to_owned(),
            open,
            selected: app.rail.home.selected,
            unread: app.rail.home.unread,
            badge: app.rail.home.badge.clone(),
        },
        bare: false,
        children: if open {
            place_entries(app, &Navigation::DirectMessages)
        } else {
            Vec::new()
        },
    });
    let mut items = app.rail.items.iter().peekable();
    while let Some(item) = items.next() {
        match &item.kind {
            Kind::Folder { open, .. } => {
                let mut children = Vec::new();
                while let Some(nested) =
                    items.next_if(|next| matches!(next.kind, Kind::Nested { .. }))
                {
                    if *open {
                        children.push(server_entry(app, nested));
                    }
                }
                let node = Node::Folder(item.id.clone());
                tops.push(Entry {
                    key: node.key(),
                    kind: TreeKind::Node {
                        node,
                        name: item.name.clone(),
                        open: *open,
                        selected: item.selected && !*open,
                        unread: item.unread,
                        badge: if *open {
                            String::new()
                        } else {
                            item.badge.clone()
                        },
                    },
                    bare: false,
                    children,
                });
            }
            Kind::Server | Kind::Nested { .. } => tops.push(server_entry(app, item)),
            // The unread DMs the rail shows are the direct messages' own.
            Kind::Dm | Kind::Divider | Kind::Add => {}
        }
    }
    let mut rows = vec![TreeRow {
        key: "root".to_owned(),
        parent: None,
        guides: Vec::new(),
        branch: None,
        kind: TreeKind::Root(tree.map_or("~/", |tree| tree.root.as_str()).to_owned()),
    }];
    lay_out(tops, &[], None, &mut rows);
    rows
}

fn server_entry(app: &App, item: &RailItem) -> Entry {
    let navigation = Navigation::Server(item.id.clone());
    let open = place_open(app, &navigation);
    let node = Node::Server(item.id.clone());
    Entry {
        key: node.key(),
        kind: TreeKind::Node {
            node,
            name: item.name.clone(),
            open,
            selected: item.selected,
            unread: item.unread,
            badge: item.badge.clone(),
        },
        bare: false,
        children: if open {
            place_entries(app, &navigation)
        } else {
            Vec::new()
        },
    }
}

/// A place's rows as the tree hangs them: its categories, each with its
/// channels while open, the channels before them, and threads under their
/// channels. A folded category's channels the stock list keeps (unread,
/// open, busy voice) count on its line instead.
fn place_entries(app: &App, navigation: &Navigation) -> Vec<Entry> {
    let state = app.client.state();
    let open = state.navigation == *navigation;
    let other;
    let rows: &[SideRow] = if open {
        &app.sidebar.rows
    } else {
        let categories = match navigation {
            Navigation::Server(server) => app.client.app.categories(server),
            Navigation::DirectMessages => Vec::new(),
        };
        other = super::rows::sidebar_in(
            state,
            navigation,
            &categories,
            &app.layout.collapsed,
            "",
            app.theme.dark,
        );
        &other
    };
    let place = Node::navigation_key(navigation);
    let server = match navigation {
        Navigation::Server(id) => id.as_str(),
        Navigation::DirectMessages => "",
    };
    let side = |index: usize, row: &SideRow| Entry {
        key: format!("{place}>{}", row.key()),
        kind: if open {
            TreeKind::Open(index)
        } else {
            TreeKind::Other(Box::new(row.clone()))
        },
        bare: matches!(row, SideRow::Room(room) if room.thread),
        children: Vec::new(),
    };
    let mut entries: Vec<Entry> = Vec::new();
    // Whether rows now come under a category, and whether it's folded.
    let mut under: Option<bool> = None;
    for (index, row) in rows.iter().enumerate() {
        match row {
            SideRow::PinnedEnd => under = None,
            SideRow::Heading {
                name,
                fold,
                collapsed,
            } => {
                under = Some(*collapsed);
                entries.push(Entry {
                    key: format!("{place}>{}", row.key()),
                    kind: TreeKind::Heading {
                        server: server.to_owned(),
                        fold: fold.clone(),
                        name: name.clone(),
                        open: !collapsed,
                        unread: false,
                        badge: String::new(),
                    },
                    bare: false,
                    children: Vec::new(),
                });
            }
            SideRow::Room(room) => {
                let entry = side(index, row);
                let heading = if under.is_some() {
                    entries.last_mut()
                } else {
                    None
                };
                match heading {
                    Some(Entry {
                        kind:
                            TreeKind::Heading {
                                open: false,
                                unread,
                                badge,
                                ..
                            },
                        ..
                    }) => {
                        *unread |= room.unread;
                        *badge = add_badges(badge, &room.badge);
                    }
                    Some(heading) => match heading.children.last_mut() {
                        Some(channel) if room.thread => channel.children.push(entry),
                        _ => heading.children.push(entry),
                    },
                    None => match entries.last_mut() {
                        Some(channel) if room.thread => channel.children.push(entry),
                        _ => entries.push(entry),
                    },
                }
            }
            _ => {
                under = None;
                entries.push(side(index, row));
            }
        }
    }
    entries
}

impl Node {
    fn navigation_key(navigation: &Navigation) -> String {
        match navigation {
            Navigation::DirectMessages => Node::Dms.key(),
            Navigation::Server(id) => Node::Server(id.clone()).key(),
        }
    }
}

/// Two mention counts as one badge shows them ("99+" past 99).
fn add_badges(one: &str, other: &str) -> String {
    let count = |badge: &str| badge.trim_end_matches('+').parse::<u64>().unwrap_or(0);
    crate::controls::count_badge(count(one) + count(other))
}

/// Lays `entries` out after `guides`, under the row at `parent`.
fn lay_out(entries: Vec<Entry>, guides: &[bool], parent: Option<usize>, rows: &mut Vec<TreeRow>) {
    // A thread is no branch of its own: the last branch is the last row's
    // that isn't one.
    let last = entries.iter().rposition(|entry| !entry.bare);
    for (index, entry) in entries.into_iter().enumerate() {
        let ends = Some(index) >= last;
        let at = rows.len();
        rows.push(TreeRow {
            key: entry.key,
            parent,
            guides: if entry.bare {
                guides
                    .iter()
                    .copied()
                    .chain([last.is_some_and(|last| index < last)])
                    .collect()
            } else {
                guides.to_vec()
            },
            branch: (!entry.bare).then_some(ends),
            kind: entry.kind,
        });
        if !entry.children.is_empty() {
            let mut below = guides.to_vec();
            below.push(!ends);
            lay_out(entry.children, &below, Some(at), rows);
        }
    }
}

/// A row's height in the list, its gap after included.
pub fn height(rows: &[SideRow], row: &TreeRow, theme: &crate::theme::Theme, gap: f32) -> f32 {
    let side = match &row.kind {
        TreeKind::Root(_) | TreeKind::Node { .. } => return NODE,
        TreeKind::Heading { .. } => return HEADING,
        TreeKind::Open(index) => match rows.get(*index) {
            Some(side) => side,
            None => return 0.0,
        },
        TreeKind::Other(row) => row.as_ref(),
    };
    // Links keep the stock rows whatever a theme's channels take, as the
    // list measures them.
    side.height(theme, gap)
}

fn row_gap(app: &App) -> f32 {
    app.themes
        .channel_rows(&app.theme.id)
        .and_then(|rows| rows.gap)
        .unwrap_or(GAP)
}

/// A press anywhere but on the row the keys went to takes them from the
/// tree.
pub fn follow_press(app: &mut App) {
    if app.sidebar.tree_cursor.is_some() && Some(app.pressed.get()) != app.sidebar.tree_press {
        leave(app);
    }
}

fn leave(app: &mut App) {
    app.sidebar.tree_cursor = None;
    app.sidebar.tree_ring = false;
    app.sidebar.tree_press = None;
}

/// The keys go to the tree, on the row at `key`, as it's clicked.
fn take_keys(app: &mut App, key: &str) -> Task<Message> {
    let first = app.sidebar.tree_cursor.is_none();
    app.sidebar.tree_cursor = Some(key.to_owned());
    app.sidebar.tree_press = Some(app.pressed.get());
    if first {
        operation::focus(crate::app::NOBODY)
    } else {
        Task::none()
    }
}

/// A click on a row's open and closed mark, or a category's line.
pub fn toggle(app: &mut App, key: &str) -> Task<Message> {
    let keys = take_keys(app, key);
    Task::batch([keys, toggle_row(app, key)])
}

/// A click on a place's name: the window goes there, or, there already,
/// it opens or closes.
pub fn activate(app: &mut App, key: &str) -> Task<Message> {
    let keys = take_keys(app, key);
    Task::batch([keys, activate_row(app, key)])
}

fn toggle_row(app: &mut App, key: &str) -> Task<Message> {
    let Some(row) = app.sidebar.tree.iter().find(|row| row.key == key) else {
        return Task::none();
    };
    match row.kind.clone() {
        TreeKind::Node { node, open, .. } => match node {
            Node::Folder(id) => {
                return crate::rail::update(app, crate::rail::Event::Folder(id));
            }
            node => {
                if let Some(navigation) = node.navigation() {
                    set_place(app, &navigation, !open);
                }
            }
        },
        TreeKind::Heading { server, fold, .. } => {
            app.lists_shown = None;
            let collapsed = app.layout.collapsed.entry(server.clone()).or_default();
            if !collapsed.remove(&fold) {
                collapsed.insert(fold);
            }
            if collapsed.is_empty() {
                app.layout.collapsed.remove(&server);
            }
            crate::layout::store(app.client.root(), "collapsed", &app.layout.collapsed);
        }
        _ => return Task::none(),
    }
    app.sync()
}

fn activate_row(app: &mut App, key: &str) -> Task<Message> {
    let Some(row) = app.sidebar.tree.iter().find(|row| row.key == key) else {
        return Task::none();
    };
    let side = match &row.kind {
        TreeKind::Node { node, .. } => {
            let Some(navigation) = node.navigation() else {
                return toggle_row(app, key);
            };
            if app.client.state().navigation == navigation {
                return toggle_row(app, key);
            }
            app.client.dispatch(Action::Navigate(navigation));
            return app.sync();
        }
        TreeKind::Heading { .. } => return toggle_row(app, key),
        TreeKind::Root(_) => return Task::none(),
        TreeKind::Open(index) => app.sidebar.rows.get(*index).cloned(),
        TreeKind::Other(row) => Some(row.as_ref().clone()),
    };
    match side {
        Some(SideRow::Room(room)) if room.voice => {
            super::update(app, Event::Voice(room.id.clone()))
        }
        Some(SideRow::Room(room)) => super::update(app, Event::Select(room.id.clone())),
        Some(SideRow::Browse { .. }) => super::update(app, Event::Browse),
        _ => Task::none(),
    }
}

/// A key while the tree has them; none for one it leaves to the window,
/// which takes them from the tree unless it's a modifier's.
pub fn pressed(app: &mut App, key: &Key, modifiers: keyboard::Modifiers) -> Option<Task<Message>> {
    let cursor = app.sidebar.tree_cursor.clone()?;
    if !on(app) {
        leave(app);
        return None;
    }
    let named = match key {
        Key::Named(named) => Some(*named),
        _ => None,
    };
    if modifiers.command() || modifiers.alt() {
        return None;
    }
    if matches!(
        named,
        Some(Named::Shift | Named::Control | Named::Alt | Named::Super)
    ) {
        return None;
    }
    let rows = &app.sidebar.tree;
    let side = &app.sidebar.rows;
    let steps: Vec<usize> = (0..rows.len())
        .filter(|at| rows[*at].steppable(side))
        .collect();
    let at = rows.iter().position(|row| row.key == cursor);
    let here = at.and_then(|at| steps.iter().position(|step| *step == at));
    let go = |to: Option<usize>| to.map(|to| rows[to].key.clone());
    let to = match named {
        Some(Named::ArrowDown) => go(match here {
            Some(here) => steps.get(here + 1).or(steps.last()).copied(),
            None => steps.first().copied(),
        }),
        Some(Named::ArrowUp) => go(match here {
            Some(here) => steps.get(here.saturating_sub(1)).copied(),
            None => steps.first().copied(),
        }),
        Some(Named::Home) => go(steps.first().copied()),
        Some(Named::End) => go(steps.last().copied()),
        Some(Named::ArrowRight) => {
            let at = at?;
            match rows[at].open() {
                Some(false) => {
                    app.sidebar.tree_ring = true;
                    return Some(toggle_row(app, &cursor));
                }
                Some(true) => go(rows
                    .get(at + 1)
                    .filter(|child| child.parent == Some(at))
                    .map(|_| at + 1)),
                None => None,
            }
        }
        Some(Named::ArrowLeft) => {
            let at = at?;
            match rows[at].open() {
                Some(true) => {
                    app.sidebar.tree_ring = true;
                    return Some(toggle_row(app, &cursor));
                }
                _ => go(rows[at]
                    .parent
                    .filter(|parent| rows[*parent].steppable(side))),
            }
        }
        Some(Named::Enter | Named::Space) => {
            app.sidebar.tree_ring = true;
            return Some(activate_row(app, &cursor));
        }
        Some(Named::Escape) => {
            leave(app);
            return Some(crate::composer::refocus(app));
        }
        _ => {
            // Typing goes to the composer, as it does anywhere.
            leave(app);
            return None;
        }
    };
    app.sidebar.tree_ring = true;
    let Some(to) = to else {
        return Some(Task::none());
    };
    app.sidebar.tree_cursor = Some(to.clone());
    Some(reveal(app, &to))
}

/// Scrolls the tree just enough to show the row at `key`.
fn reveal(app: &App, key: &str) -> Task<Message> {
    let gap = row_gap(app);
    let mut top = 0.0;
    for row in &app.sidebar.tree {
        let height = height(&app.sidebar.rows, row, &app.theme, gap);
        if row.key == key {
            let scroll = app.sidebar.scroll;
            let to = if top < scroll.offset {
                top
            } else if top + height > scroll.offset + scroll.height {
                top + height - scroll.height
            } else {
                return Task::none();
            };
            return operation::scroll_to(
                LIST,
                AbsoluteOffset {
                    x: None,
                    y: Some(to.max(0.0)),
                },
            );
        }
        top += height;
    }
    Task::none()
}

/// The sidebar rows the tree has in view, the window's own place's and
/// those of other places open in it, for the pictures asked for.
pub fn rows_in_view(app: &App, from: f32, to: f32) -> Vec<&SideRow> {
    let gap = row_gap(app);
    let mut top = 0.0;
    let mut shown = Vec::new();
    for row in &app.sidebar.tree {
        let height = height(&app.sidebar.rows, row, &app.theme, gap);
        if top + height >= from && top <= to {
            match &row.kind {
                TreeKind::Open(index) => shown.extend(app.sidebar.rows.get(*index)),
                TreeKind::Other(side) => shown.push(side.as_ref()),
                _ => {}
            }
        }
        top += height;
    }
    shown
}

impl<'a> super::View<'a> {
    /// The tree, under the sidebar's header: the open server's boost goal
    /// and events over it, as over its channels.
    pub fn tree_view(self) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let gap = self
            .cx
            .themes
            .channel_rows(&theme.id)
            .and_then(|rows| rows.gap)
            .unwrap_or(GAP);
        let total: f32 = self
            .s
            .tree
            .iter()
            .map(|row| height(&self.s.rows, row, theme, gap))
            .sum::<f32>()
            + super::END;
        let viewport = self.s.scroll;
        let mut list = Rows::new(total);
        let mut top = 0.0;
        for row in &self.s.tree {
            let height = height(&self.s.rows, row, theme, gap);
            let at = top;
            top += height;
            if at + height < viewport.offset - MARGIN
                || at > viewport.offset + viewport.height + MARGIN
            {
                continue;
            }
            list.push(key(&row.key), at, height, self.tree_row(row, height));
        }
        let list = scroll_view(list)
            .id(LIST)
            .on_scroll(|viewport| Message::Side(Event::Scrolled(viewport)))
            .height(Length::Fill)
            .style(scrollbar(theme));
        let mut top = column![];
        if let Some(boosts) = self
            .s
            .header
            .boosts
            .filter(|_| self.s.header.server.is_some())
        {
            top = top.push(self.boost_goal(boosts));
        }
        if let Some(events) = self
            .s
            .header
            .events
            .filter(|_| self.s.header.server.is_some())
        {
            top = top.push(self.events_row(events));
        }
        container(column![top, list])
            .width(super::width(self.cx.layout, theme))
            .height(Length::Fill)
            .clip(true)
            .into()
    }

    fn tree_row(self, tree_row: &'a TreeRow, height: f32) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let ink = crate::theme_chrome::tree_ink(self.cx);
        let head = match &tree_row.kind {
            TreeKind::Open(at) => match self.s.rows.get(*at) {
                Some(SideRow::Room(room)) => room.row_height(theme),
                _ => height,
            },
            TreeKind::Other(row) => match row.as_ref() {
                SideRow::Room(room) => room.row_height(theme),
                _ => height,
            },
            _ => height,
        };
        let levels = tree_row.guides.len() + usize::from(tree_row.branch.is_some());
        let guides = Element::new(Guides {
            levels: tree_row.guides.clone(),
            branch: tree_row.branch,
            head,
            ink,
        });
        let content: Element<'a, Message> = match &tree_row.kind {
            TreeKind::Root(root) => container(
                iced::widget::text(root.clone())
                    .font(crate::theme_chrome::mono(self.cx))
                    .size(14)
                    .color(ink),
            )
            .height(NODE)
            .align_y(Alignment::Center)
            .into(),
            TreeKind::Node {
                node,
                name,
                open,
                selected,
                unread,
                badge,
            } => self.node_line(&tree_row.key, node, name, *open, *selected, *unread, badge),
            TreeKind::Heading {
                fold,
                name,
                open,
                unread,
                badge,
                ..
            } => self.heading_line(&tree_row.key, fold, name, *open, *unread, badge),
            TreeKind::Open(at) => match self.s.rows.get(*at) {
                Some(side) => {
                    let place = self.s.places.of_row.get(*at).copied().flatten();
                    let after_room =
                        *at > 0 && matches!(self.s.rows.get(*at - 1), Some(SideRow::Room(_)));
                    self.side_row(side, place, after_room)
                }
                None => Space::new().into(),
            },
            TreeKind::Other(side) => self.side_row(side, None, false),
        };
        let line = row![
            container(guides)
                .width(STEP * levels as f32)
                .height(Length::Fill),
            container(content).width(Length::Fill)
        ]
        .height(height)
        .padding(Padding::ZERO.left(8));
        if self.s.tree_ring && self.s.tree_cursor.as_deref() == Some(tree_row.key.as_str()) {
            let accent = theme.accent;
            let ring = container(Space::new())
                .width(Length::Fill)
                .height(head)
                .style(move |_| iced::widget::container::Style {
                    border: iced::Border {
                        color: accent,
                        width: 1.0,
                        radius: 0.0.into(),
                    },
                    ..Default::default()
                });
            iced::widget::stack![
                line,
                container(ring).padding(Padding {
                    left: 8.0 + STEP * levels as f32,
                    right: 4.0,
                    ..Padding::ZERO
                })
            ]
            .into()
        } else {
            line.into()
        }
    }

    /// The open and closed mark: the theme's folder glyphs, drawn as
    /// little triangles when they're the usual ones (which the terminal
    /// faces lack).
    fn mark(self, key: &str, open: bool, ink: Color) -> Element<'a, Message> {
        let folders = self
            .cx
            .themes
            .chrome(&self.cx.look.theme.id)
            .and_then(|chrome| chrome.folders.as_ref());
        let glyph = folders.map(|folders| if open { &folders.open } else { &folders.closed });
        let drawn =
            glyph.is_none_or(|glyph| ["▸", "▾", "▶", "▼", "►", "▹", "▿"].contains(&glyph.as_str()));
        let face: Element<'a, Message> = if drawn {
            Element::new(Triangle { open, ink })
        } else {
            iced::widget::text(glyph.cloned().unwrap_or_default())
                .font(crate::theme_chrome::mono(self.cx))
                .size(14)
                .color(ink)
                .into()
        };
        mouse_area(
            container(face)
                .width(if drawn {
                    Length::Fixed(MARK)
                } else {
                    Length::Shrink
                })
                .height(NODE)
                .align_y(Alignment::Center),
        )
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(Message::Side(Event::TreeToggle(key.to_owned())))
        .into()
    }

    /// What a closed line counts: its mentions, else that it has unread
    /// messages, in the theme's words for them.
    fn counted(self, unread: bool, badge: &str, selected: bool) -> Option<Element<'a, Message>> {
        let theme = self.cx.look.theme;
        let counts = self
            .cx
            .themes
            .chrome(&theme.id)
            .and_then(|chrome| chrome.counts.as_ref());
        let ink = if selected {
            theme.on_accent
        } else {
            use yapfast_core::themes::chrome::CountInk;
            match counts.and_then(|counts| counts.ink) {
                Some(CountInk::Loud) => theme.loud,
                Some(CountInk::Accent) => theme.accent,
                Some(CountInk::Text) => theme.text,
                Some(CountInk::Secondary) => theme.secondary,
                Some(CountInk::Muted) => theme.muted,
                Some(CountInk::Danger) | None => theme.danger,
            }
        };
        let words = if !badge.is_empty() {
            match counts.filter(|counts| !counts.mentions.is_empty()) {
                Some(counts) => {
                    yapfast_core::themes::chrome::fill(&counts.mentions, &[("n", badge)])
                }
                None => format!("({badge})"),
            }
        } else if unread {
            match counts.filter(|counts| !counts.unread.is_empty()) {
                Some(counts) => counts.unread.clone(),
                None => "•".to_owned(),
            }
        } else {
            return None;
        };
        let ink = if badge.is_empty() && !selected {
            theme.text
        } else {
            ink
        };
        Some(
            iced::widget::text(words)
                .size(14)
                .font(crate::fonts::BOLD)
                .color(ink)
                .wrapping(iced::widget::text::Wrapping::None)
                .into(),
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "each part it draws is its own input"
    )]
    fn node_line(
        self,
        key: &str,
        node: &Node,
        name: &str,
        open: bool,
        selected: bool,
        unread: bool,
        badge: &str,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let chrome = self.cx.themes.chrome(&theme.id);
        let role = chrome
            .and_then(|chrome| chrome.font)
            .map_or("body", yapfast_core::themes::package::Role::name);
        let pointed = self.cx.pointed.side.as_deref() == Some(key);
        let ink = if selected {
            theme.on_accent
        } else if unread || pointed {
            theme.text
        } else {
            theme.secondary
        };
        let suffix = chrome
            .and_then(|chrome| chrome.folders.as_ref())
            .map_or("/", |folders| folders.suffix.as_str());
        let before = match node {
            Node::Server(_) => chrome
                .and_then(|chrome| {
                    chrome
                        .prefix
                        .get(&yapfast_core::themes::chrome::Place::Server)
                })
                .map_or("", String::as_str),
            _ => "",
        };
        let mark_ink = if selected {
            theme.on_accent
        } else {
            crate::theme_chrome::tree_ink(self.cx)
        };
        let mut line = row![self.mark(key, open, mark_ink)]
            .align_y(Alignment::Center)
            .spacing(4);
        line = line.push(
            crate::elide::elide(format!("{before}{name}{suffix}"))
                .size(theme.type_size("small", 14.0))
                .font(if unread && !selected {
                    crate::fonts::BOLD
                } else {
                    theme.font(role, crate::fonts::REGULAR)
                })
                .color(ink),
        );
        // A place's mentions show open or closed, as the rail's; that it's
        // unread only while it's closed (its channels show it open).
        if let Some(counted) = self.counted(unread && !open, badge, selected) {
            line = line.push(counted);
        }
        let ground = if selected {
            theme.accent
        } else if pointed {
            crate::style::faded(theme.text, 0.08)
        } else {
            Color::TRANSPARENT
        };
        let face = container(line)
            .height(NODE)
            .width(Length::Fill)
            .padding([0.0, 4.0])
            .style(move |_| crate::style::fill(ground));
        let hit = mouse_area(face)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_enter(Message::Side(Event::Point(key.to_owned(), true)))
            .on_exit(Message::Side(Event::Point(key.to_owned(), false)))
            .on_press(Message::Side(Event::TreeActivate(key.to_owned())));
        match node {
            Node::Dms => hit.into(),
            Node::Folder(id) | Node::Server(id) => {
                let (id, spot) = (id.clone(), self.cx.pressed);
                RightPress::new(hit, move |_| {
                    Message::Rail(crate::rail::Event::Menu(id.clone(), spot.get()))
                })
                .into()
            }
        }
    }

    fn heading_line(
        self,
        key: &str,
        fold: &str,
        name: &str,
        open: bool,
        unread: bool,
        badge: &str,
    ) -> Element<'a, Message> {
        let theme = self.cx.look.theme;
        let pointed = self.cx.pointed.side.as_deref() == Some(key);
        let suffix = self
            .cx
            .themes
            .chrome(&theme.id)
            .and_then(|chrome| chrome.folders.as_ref())
            .map_or("/", |folders| folders.suffix.as_str());
        let ink = if pointed || (!open && unread) {
            theme.text
        } else {
            theme.muted
        };
        let mut line = row![self.mark(key, open, crate::theme_chrome::tree_ink(self.cx))]
            .align_y(Alignment::Center)
            .spacing(4);
        line = line.push(
            crate::elide::elide(format!("{name}{suffix}"))
                .size(theme.type_size("labels", 13.0))
                .font(theme.font("labels", crate::fonts::SEMIBOLD))
                .color(ink),
        );
        if !open && let Some(counted) = self.counted(unread, badge, false) {
            line = line.push(counted);
        }
        let face = container(line)
            .height(HEADING)
            .width(Length::Fill)
            .padding([0.0, 4.0]);
        let hit = mouse_area(face)
            .interaction(iced::mouse::Interaction::Pointer)
            .on_enter(Message::Side(Event::Point(key.to_owned(), true)))
            .on_exit(Message::Side(Event::Point(key.to_owned(), false)))
            .on_press(Message::Side(Event::TreeToggle(key.to_owned())));
        if !self.cx.state.is_category(fold) {
            return hit.into();
        }
        let (room, spot) = (fold.to_owned(), self.cx.pressed);
        RightPress::new(hit, move |_| {
            Message::Side(Event::Menu(room.clone(), spot.get()))
        })
        .into()
    }
}

/// A tree row's lines: each level above it whose line runs on, and its own
/// branch, `├──` or `└──`, its stub at the middle of its first line `head`
/// tall. Drawn as lines rather than box characters, so they meet across
/// rows of any height.
struct Guides {
    levels: Vec<bool>,
    branch: Option<bool>,
    head: f32,
    ink: Color,
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for Guides {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: advanced_mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        let fill = |renderer: &mut iced::Renderer, x: f32, y: f32, w: f32, h: f32| {
            if w > 0.0 && h > 0.0 {
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle::new(Point::new(x, y), Size::new(w, h)),
                        ..Default::default()
                    },
                    self.ink,
                );
            }
        };
        let x_of = |level: usize| (bounds.x + level as f32 * STEP + 4.0).round();
        let top = bounds.y.round();
        let bottom = (bounds.y + bounds.height).round();
        for (level, runs) in self.levels.iter().enumerate() {
            if *runs {
                fill(renderer, x_of(level), top, 1.0, bottom - top);
            }
        }
        if let Some(last) = self.branch {
            let level = self.levels.len();
            let x = x_of(level);
            let middle = (bounds.y + self.head / 2.0).round();
            let end = if last { middle + 1.0 } else { bottom };
            fill(renderer, x, top, 1.0, end - top);
            fill(
                renderer,
                x,
                middle,
                (bounds.x + (level + 1) as f32 * STEP).round() - x,
                1.0,
            );
        }
    }
}

/// The usual open and closed mark, `▾` and `▸`, drawn in whole pixels.
struct Triangle {
    open: bool,
    ink: Color,
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for Triangle {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(MARK), Length::Fixed(MARK))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, MARK, MARK)
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: advanced_mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let bounds = layout.bounds();
        let (x, y) = (bounds.x.round(), bounds.y.round());
        // Seven pixels across its long side, four along the other.
        for step in 0..4 {
            let (at, length) = (step as f32, 7.0 - 2.0 * step as f32);
            let piece = if self.open {
                Rectangle::new(
                    Point::new(x + 1.0 + at, y + 3.0 + at),
                    Size::new(length, 1.0),
                )
            } else {
                Rectangle::new(
                    Point::new(x + 3.0 + at, y + 1.0 + at),
                    Size::new(1.0, length),
                )
            };
            renderer.fill_quad(
                renderer::Quad {
                    bounds: piece,
                    ..Default::default()
                },
                self.ink,
            );
        }
    }
}
