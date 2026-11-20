//! The places a plugin puts something in the window: a notice over the
//! conversation, a toast in a corner, a dialog over everything, buttons beside
//! the composer, on a message's hover bar, in the voice panel and in a call,
//! rows in a message's and a person's menu, a section on a person's profile,
//! a tab in the emoji picker, and badges on a person.
//!
//! What is drawn comes from `yapfast_core::plugins::slots` (`AppState::
//! plugin_slots`), and what the person does with it goes back as
//! [`Message::PluginAct`], which keeps what only the window keeps (a draft, a
//! tab) in [`Kept`] and hands the rest to the runtime that owns the plugin.
//!
//! Every surface names the plugin whose drawing it is, and each slot shows so
//! many plugins and so many of their buttons (`Slot::most_plugins`,
//! `Slot::buttons_each`). A window with no plugin pays one `Option` test per
//! surface: `plugin_slots` is `None` until one is enabled.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::{
    Alignment, Element, Length,
    widget::{column, container, row, scrollable, text, tooltip},
};
use yapfast_core::plugins::slots::{Drawn, Slot, Slots};
use yapfast_core::plugins::ui::{self, Kind, PluginName, Source, Tree};

use crate::{
    app::{App, Message},
    controls::{self, Kind as ActionKind},
    ctx::Ctx,
    fonts::BOLD,
    plugin_tree::{self, Act, Images, Local, Paint},
    style,
    theme::Theme,
};

/// Whose drawing an act was in, and about what: the plugin, its slot and
/// the subject (the message a button is on, the open conversation, the
/// person a profile is).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub plugin: String,
    pub slot: Slot,
    pub subject: String,
}

impl Scope {
    fn key(&self) -> String {
        key(&self.plugin, self.slot)
    }
}

/// What the window keeps of a slot's drawing, by plugin and slot.
fn key(plugin: &str, slot: Slot) -> String {
    format!("{plugin}\u{1f}{}", slot.id())
}

/// What the window keeps of what plugins draw, between their draws: each
/// drawing's [`Local`] and the pictures their image cells show.
#[derive(Debug, Default)]
pub struct Kept {
    locals: BTreeMap<String, Local>,
    /// What a drawing nothing is kept for borrows.
    empty: Local,
    pub images: Images,
    /// The tree each [`Local`] was last made for, to tell a plugin drawing
    /// again from the same drawing.
    seen: BTreeMap<String, Arc<Tree>>,
    /// The slots' counts of changes when they were last looked at.
    stamp: u64,
    /// The folders `YAPFAST_PLUGINS_DIR`'s plugins are read from, by id.
    pub folders: BTreeMap<String, PathBuf>,
}

impl Kept {
    pub fn local(&self, plugin: &str, slot: Slot) -> &Local {
        self.locals.get(&key(plugin, slot)).unwrap_or(&self.empty)
    }

    /// What a drawing of `plugin`'s reads besides its tree.
    pub fn paint<'a>(&'a self, ctx: Ctx<'a>, plugin: &'a str, slot: Slot) -> Paint<'a> {
        Paint {
            look: ctx.look,
            local: self.local(plugin, slot),
            images: &self.images,
            plugin,
        }
    }

    /// Keeps what `act` changes in `scope`'s drawing; true when the plugin
    /// is to hear of it.
    pub fn apply(&mut self, scope: &Scope, act: &Act) -> bool {
        self.locals.entry(scope.key()).or_default().apply(act)
    }

    /// Follows what the plugins drew since the last look: a drawing a plugin
    /// made again lets go of the drafts it now answers, its text areas get
    /// their editors, its pictures are read, and a drawing that went takes
    /// what was kept for it.
    fn follow(&mut self, slots: &Slots, folder: impl Fn(&str) -> Option<PathBuf>) {
        let stamp = Slot::ALL
            .into_iter()
            .fold(0u64, |sum, slot| sum.wrapping_add(slots.changes(slot)));
        if stamp == self.stamp {
            return;
        }
        self.stamp = stamp;
        let names = slots.names();
        let mut live = Vec::new();
        for drawn in slots.all_drawn() {
            let Some(plugin) = names.get(drawn.plugin) else {
                continue;
            };
            let at = key(&plugin.id, drawn.slot);
            live.push(at.clone());
            let before = self.seen.get(&at);
            if before.is_some_and(|before| Arc::ptr_eq(before, &drawn.tree)) {
                continue;
            }
            let local = self.locals.entry(at.clone()).or_default();
            redrawn(local, before.map(Arc::as_ref), &drawn.tree);
            local.prepare(&drawn.tree);
            self.seen.insert(at, Arc::clone(&drawn.tree));
            read_pictures(&mut self.images, &plugin.id, &drawn.tree, &folder);
        }
        self.locals.retain(|at, _| live.contains(at));
        self.seen.retain(|at, _| live.contains(at));
    }
}

/// A plugin drew `tree` again over `before`: a field it now gives the text
/// typed into it, or gives a text of its own, shows the plugin's from now on.
fn redrawn(local: &mut Local, before: Option<&Tree>, tree: &Tree) {
    let value = |tree: &Tree, id: &str| {
        tree.cells()
            .into_iter()
            .find(|cell| cell.id == id)
            .map(|cell| cell.value.clone())
    };
    local.drafts.retain(|id, draft| {
        let now = value(tree, id);
        let then = before.and_then(|before| value(before, id));
        now.is_some() && now.as_deref() != Some(draft.as_str()) && now == then
    });
    // A text area the plugin gave other text starts over from it.
    for cell in tree.cells() {
        if cell.kind == Kind::Textarea
            && before.and_then(|before| value(before, &cell.id)) != Some(cell.value.clone())
        {
            local.areas.remove(&cell.id);
        }
    }
}

/// The most a plugin's picture file may hold.
const PICTURE_BYTES: u64 = 1 << 20;

/// Reads the pictures `tree`'s image cells name from the plugin's own files,
/// once each. A picture from the web is drawn once the runtime fetches it,
/// which the assets API will do; until then its cell keeps its room.
fn read_pictures(
    images: &mut Images,
    plugin: &str,
    tree: &Tree,
    folder: &impl Fn(&str) -> Option<PathBuf>,
) {
    for cell in tree.cells() {
        if !matches!(cell.kind, Kind::Image | Kind::Icon) {
            continue;
        }
        let Some(Source::Asset(path)) = ui::source(&cell.src) else {
            continue;
        };
        let at = plugin_tree::image_key(plugin, &cell.src);
        if images.contains_key(&at) {
            continue;
        }
        let handle = folder(plugin).and_then(|root| read_picture(&root, path));
        images.insert(at, handle);
    }
}

/// A picture from inside `root`, by its relative path: none past the size
/// cap, and none a link leads out of the folder to.
fn read_picture(root: &Path, path: &str) -> Option<iced::widget::image::Handle> {
    let root = root.canonicalize().ok()?;
    let file = root.join(path).canonicalize().ok()?;
    if !file.starts_with(&root) || std::fs::metadata(&file).ok()?.len() > PICTURE_BYTES {
        return None;
    }
    Some(iced::widget::image::Handle::from_bytes(
        std::fs::read(file).ok()?,
    ))
}

/// Brings what the window keeps up to what the plugins drew; called as the
/// window syncs, and nothing while no plugin draws.
pub fn follow(app: &mut App) {
    let Some(slots) = app.client.app.state.plugin_slots.as_deref() else {
        return;
    };
    let plugins = app.client.app.state.plugins.as_ref();
    let folders = &app.plugin_kept.folders;
    let folder = |id: &str| {
        folders
            .get(id)
            .cloned()
            .or_else(|| plugins.and_then(|plugins| plugins.package_folder(id).map(Path::to_owned)))
    };
    // The borrow of the folders ends before the kept state is changed.
    let found: BTreeMap<String, PathBuf> = slots
        .names()
        .iter()
        .filter_map(|name| folder(&name.id).map(|path| (name.id.clone(), path)))
        .collect();
    app.plugin_kept.follow(slots, |id| found.get(id).cloned());
    // A plugin's own settings page, while it shows, gets its text areas'
    // editors as a slot's drawing does.
    let Some(slots) = app.client.app.state.plugin_slots.as_deref() else {
        return;
    };
    if let Some(settings) = app.win.settings.get_mut()
        && settings.plugins.custom
        && let Some(open) = settings.plugins.open.as_deref()
        && let Some(drawn) = slots.drawn_by(open, Slot::SettingsPage)
    {
        settings.plugins.local.prepare(&drawn.tree);
    }
}

/// What the person did with a plugin's drawing: kept here, and handed to the
/// plugin when it is the plugin's to hear of.
pub fn act(app: &mut App, scope: Scope, act: Act) {
    if !app.plugin_kept.apply(&scope, &act) {
        return;
    }
    let (Act::Set { id, value } | Act::Edit { id, value }) = act else {
        return;
    };
    app.client
        .dispatch(yapfast_core::application::Action::PluginInput(
            yapfast_core::plugins::Input::Widget {
                plugin: scope.plugin,
                slot: scope.slot,
                id,
                value,
                subject: scope.subject,
            },
        ));
}

/// The message a cell of `plugin`'s drawing in `slot` sends.
fn acted(plugin: &str, slot: Slot, subject: &str) -> impl Fn(Act) -> Message + use<> {
    let scope = Scope {
        plugin: plugin.to_owned(),
        slot,
        subject: subject.to_owned(),
    };
    move |act| Message::PluginAct(scope.clone(), act)
}

/// One plugin's drawing in `slot`, about `subject`.
fn draw<'a>(
    ctx: Ctx<'a>,
    drawn: &'a Drawn,
    plugin: &'a PluginName,
    subject: &str,
) -> Element<'a, Message> {
    plugin_tree::draw(
        ctx.plugin_kept.paint(ctx, &plugin.id, drawn.slot),
        &drawn.tree,
        acted(&plugin.id, drawn.slot, subject),
    )
}

/// One plugin's buttons in `slot`, on one line.
fn draw_inline<'a>(
    ctx: Ctx<'a>,
    drawn: &'a Drawn,
    plugin: &'a PluginName,
    subject: &str,
) -> Element<'a, Message> {
    plugin_tree::draw_inline(
        ctx.plugin_kept.paint(ctx, &plugin.id, drawn.slot),
        &drawn.tree,
        acted(&plugin.id, drawn.slot, subject),
    )
}

/// The plugins drawing in `slot`, with their names; empty without any.
fn shown<'a>(ctx: Ctx<'a>, slot: Slot) -> Vec<(&'a Drawn, &'a PluginName)> {
    let Some(slots) = ctx.state.plugin_slots.as_deref() else {
        return Vec::new();
    };
    slots
        .drawn(slot)
        .filter_map(|drawn| Some((drawn, slots.names().get(drawn.plugin)?)))
        .collect()
}

/// A notice a plugin puts over the conversation, with the button that closes
/// it: the chat's top banner area.
pub fn banner<'a>(ctx: Ctx<'a>, subject: &'a str) -> Option<Element<'a, Message>> {
    let theme = ctx.look.theme;
    let mut shown_ = column![].spacing(8).width(Length::Fill);
    let mut any = false;
    for (drawn, plugin) in shown(ctx, Slot::Banner) {
        let body = draw(ctx, drawn, plugin, subject);
        shown_ = shown_.push(notice_card(theme, plugin, Slot::Banner, body));
        any = true;
    }
    any.then(|| shown_.into())
}

/// The dialogs plugins ask, one card each, each naming its plugin, in the
/// kit's dialog frame. They scroll past a height, so no tree can push
/// Dismiss off the window, and a click outside or Escape dismisses the first.
pub fn dialog<'a>(ctx: Ctx<'a>, subject: &'a str) -> Option<Element<'a, Message>> {
    let theme = ctx.look.theme;
    let mut cards = column![].spacing(12).width(Length::Fill);
    let mut first = None;
    for (drawn, plugin) in shown(ctx, Slot::Dialog) {
        first.get_or_insert(plugin.id.as_str());
        let body = draw(ctx, drawn, plugin, subject);
        cards = cards.push(notice_card(theme, plugin, Slot::Dialog, body));
    }
    let first = first?;
    let body = container(scrollable(cards).height(Length::Shrink))
        .padding(16)
        .max_height(DIALOG_HEIGHT);
    Some(crate::dialog::dialog(
        theme,
        body.into(),
        crate::dialog::CONFIRM_WIDTH,
        dismissed(first, Slot::Dialog),
    ))
}

/// The tallest a plugin's dialog draws before it scrolls.
const DIALOG_HEIGHT: f32 = 520.0;

/// What closing a plugin's notice or dialog tells it.
pub fn dismissed(plugin: &str, slot: Slot) -> Message {
    Message::Plugin(yapfast_core::plugins::Input::Dismissed {
        plugin: plugin.to_owned(),
        slot,
    })
}

/// The toasts in the window's corner, each gone when it says so.
pub fn toasts<'a>(ctx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let theme = ctx.look.theme;
    let mut cards = column![].spacing(8).width(Length::Fixed(320.0));
    let mut any = false;
    for (drawn, plugin) in shown(ctx, Slot::Toast) {
        let body = draw(ctx, drawn, plugin, "");
        cards = cards.push(notice_card(theme, plugin, Slot::Toast, body));
        any = true;
    }
    any.then(|| cards.into())
}

/// The buttons plugins add to a row of Yapfast's own (`slot` is one of
/// buttons), each plugin's under a tooltip naming it.
pub fn buttons<'a>(ctx: Ctx<'a>, slot: Slot, subject: &'a str) -> Option<Element<'a, Message>> {
    let mut buttons = row![].spacing(4).align_y(Alignment::Center);
    let mut any = false;
    for (drawn, plugin) in shown(ctx, slot) {
        buttons = buttons.push(named(
            ctx.look.theme,
            plugin,
            draw_inline(ctx, drawn, plugin, subject),
        ));
        any = true;
    }
    any.then(|| buttons.into())
}

/// The buttons a plugin adds beside the composer's, for the open
/// conversation.
pub fn composer_buttons<'a>(ctx: Ctx<'a>, subject: &'a str) -> Option<Element<'a, Message>> {
    buttons(ctx, Slot::Composer, subject)
}

/// The buttons a plugin adds to a message's hover bar, before More, for the
/// message `item` of the open conversation.
pub fn hover_bar<'a>(ctx: Ctx<'a>, item: &'a str) -> Option<Element<'a, Message>> {
    buttons(ctx, Slot::HoverBar, item)
}

/// The rows plugins add to a menu from their `menu.message` or `menu.user`
/// drawing: each button a row under its plugin's name, choosing it what
/// pressing the button would be, about `subject`.
pub fn menu_rows(
    state: &yapfast_core::application::AppState,
    slot: Slot,
    subject: &str,
) -> Vec<crate::menus::MenuRow> {
    let Some(slots) = state.plugin_slots.as_deref() else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for drawn in slots.drawn(slot) {
        let Some(plugin) = slots.names().get(drawn.plugin) else {
            continue;
        };
        for cell in drawn.tree.rows.iter().flat_map(|row| &row.cells) {
            if cell.kind != Kind::Button || !cell.enabled {
                continue;
            }
            rows.push(crate::menus::MenuRow {
                act: crate::menus::MenuAct::PluginAct(
                    Scope {
                        plugin: plugin.id.clone(),
                        slot,
                        subject: subject.to_owned(),
                    },
                    Act::Set {
                        id: cell.id.clone(),
                        value: String::new(),
                    },
                ),
                label: cell
                    .text
                    .chars()
                    .take(ui::LABEL_CHARS)
                    .collect::<String>()
                    .into(),
                icon: None,
                danger: false,
                // The first of them under a line, after Yapfast's own.
                separated: rows.is_empty(),
                disabled: false,
                // Whose row it is, so no plugin's row passes for Yapfast's own.
                subtitle: Some(plugin.name.clone().into()),
                check: Default::default(),
                key: String::new(),
                submenu: Vec::new(),
                click: None,
            });
        }
    }
    rows
}

/// What plugins add to a person's card, drawn under it.
pub fn badges<'a>(ctx: Ctx<'a>, person: &'a str) -> Option<Element<'a, Message>> {
    let theme = ctx.look.theme;
    let mut cards = column![].spacing(4).width(Length::Fill);
    let mut any = false;
    for (drawn, plugin) in shown(ctx, Slot::ProfileBadges) {
        // Each plugin's badges sit under its name, so a badge that says
        // "Staff" reads as that plugin's word and not Yapfast's or Discord's.
        cards = cards.push(
            column![
                text(format!("From {}", plugin.name))
                    .size(11)
                    .font(BOLD)
                    .color(theme.muted),
                draw(ctx, drawn, plugin, person),
            ]
            .spacing(2),
        );
        any = true;
    }
    any.then(|| cards.into())
}

/// The sections plugins add to a person's card and full profile, each
/// headed by its plugin's name.
pub fn profile_sections<'a>(ctx: Ctx<'a>, person: &'a str) -> Option<Element<'a, Message>> {
    let theme = ctx.look.theme;
    let mut sections = column![].spacing(12).width(Length::Fill);
    let mut any = false;
    for (drawn, plugin) in shown(ctx, Slot::ProfileSection) {
        sections = sections.push(
            column![
                text(plugin.name.to_uppercase())
                    .size(12)
                    .font(BOLD)
                    .color(theme.muted),
                container(draw(ctx, drawn, plugin, person))
                    .width(Length::Fill)
                    .clip(true),
            ]
            .spacing(6),
        );
        any = true;
    }
    any.then(|| sections.into())
}

/// The tabs plugins add to the emoji and sticker picker: each plugin's name
/// as a tab, and the chosen one's drawing.
pub fn picker_tabs(ctx: Ctx<'_>) -> Vec<&PluginName> {
    shown(ctx, Slot::PickerTab)
        .into_iter()
        .map(|(_, plugin)| plugin)
        .collect()
}

/// The chosen plugin's tab in the picker, about the open conversation.
pub fn picker_tab<'a>(
    ctx: Ctx<'a>,
    plugin: &str,
    subject: &'a str,
) -> Option<Element<'a, Message>> {
    let (drawn, name) = shown(ctx, Slot::PickerTab)
        .into_iter()
        .find(|(_, name)| name.id == plugin)?;
    let theme = ctx.look.theme;
    Some(
        column![
            text(from(name)).size(11).font(BOLD).color(theme.muted),
            scrollable(container(draw(ctx, drawn, name, subject)).padding([0, 8]))
                .height(Length::Fill),
        ]
        .spacing(8)
        .padding(12)
        .height(Length::Fill)
        .into(),
    )
}

/// A plugin's buttons, with a tooltip that names the plugin, so a button that
/// says "Report" can't pass for Yapfast's own.
fn named<'a>(
    theme: &'a Theme,
    plugin: &PluginName,
    drawn: Element<'a, Message>,
) -> Element<'a, Message> {
    tooltip(
        drawn,
        controls::tip(theme, from(plugin)),
        tooltip::Position::Top,
    )
    .into()
}

/// How a plugin's drawing is labelled: its name and id.
pub fn from(plugin: &PluginName) -> String {
    format!("{} \u{b7} plugin {}", plugin.name, plugin.id)
}

/// A notice, its plugin named so it can't pass for Yapfast's own, and the
/// button that closes it.
fn notice_card<'a>(
    theme: &'a Theme,
    plugin: &PluginName,
    slot: Slot,
    body: Element<'a, Message>,
) -> Element<'a, Message> {
    let ground = theme.raised;
    let name = theme.muted;
    let close: Element<'a, Message> = controls::action(
        theme,
        "Dismiss",
        ActionKind::Ghost,
        Some(dismissed(&plugin.id, slot)),
    )
    .small()
    .into();
    container(
        column![
            // The name takes what Dismiss leaves and wraps, so a narrow
            // conversation never pushes Dismiss out of the card.
            row![
                text(from(plugin))
                    .size(11)
                    .font(BOLD)
                    .color(name)
                    .width(Length::Fill),
                close
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            // A drawing wider than the card is cut at its edge rather than
            // spilling over what is beside it.
            container(body).width(Length::Fill).clip(true),
        ]
        .spacing(6),
    )
    .padding(10)
    .width(Length::Fill)
    .style(move |_| style::rounded(ground, 8))
    .into()
}
