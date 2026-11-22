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
    Alignment, Element, Length, Padding,
    widget::{column, container, row, scrollable, text, tooltip},
};
use yapfast_core::plugins::slots::{Drawn, Slot, Slots};
use yapfast_core::plugins::ui::{self, Kind, PluginName, Source, Tree};

use crate::{
    app::{App, Message},
    controls::{self, Kind as ActionKind},
    ctx::Ctx,
    fonts::{BOLD, MEDIUM},
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
    /// Brings the pictures from the web the drawings show up to what the core
    /// holds of them, and answers the ones nobody has asked for yet.
    fn web_pictures(
        &mut self,
        slots: &Slots,
        plugins: Option<&yapfast_core::plugins::PluginState>,
    ) -> Vec<(String, String)> {
        use yapfast_core::plugins::state::WebPictureState;
        let Some(plugins) = plugins else {
            return Vec::new();
        };
        let names = slots.names();
        let trees = slots
            .all_drawn()
            .iter()
            .filter_map(|drawn| Some((names.get(drawn.plugin)?, drawn.tree.as_ref())))
            .chain(slots.all_decorations().filter_map(|decoration| {
                Some((names.get(decoration.plugin)?, decoration.tree.as_deref()?))
            }));
        let mut wanted = Vec::new();
        for (plugin, tree) in trees {
            for cell in tree.cells() {
                if !matches!(cell.kind, Kind::Image | Kind::Icon) {
                    continue;
                }
                let Some(Source::Web { url, .. }) = ui::source(&cell.src) else {
                    continue;
                };
                let at = plugin_tree::image_key(&plugin.id, &cell.src);
                match plugins.web_pictures.get(&plugin.id, url) {
                    None => wanted.push((plugin.id.clone(), url.to_owned())),
                    Some(WebPictureState::Loading) => {}
                    Some(WebPictureState::Failed) => {
                        self.images.entry(at).or_insert(None);
                    }
                    Some(WebPictureState::Ready(picture)) => {
                        if !matches!(self.images.get(&at), Some(Some(_))) {
                            // The decode is straight RGBA; an `Rgba` handle
                            // is the renderer's format, as every other
                            // picture's is: on the CPU premultiplied BGRA.
                            let mut pixels = picture.rgba.clone();
                            crate::paint::to_native(&mut pixels);
                            self.images.insert(
                                at,
                                Some(iced::widget::image::Handle::from_rgba(
                                    picture.width,
                                    picture.height,
                                    pixels,
                                )),
                            );
                        }
                    }
                }
            }
        }
        wanted.sort();
        wanted.dedup();
        wanted
    }

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
    fn follow(
        &mut self,
        slots: &Slots,
        picture: impl Fn(&str, &str) -> Option<iced::widget::image::Handle>,
    ) {
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
            read_pictures(&mut self.images, &plugin.id, &drawn.tree, &picture);
        }
        // A card under a message may show the plugin's pictures too.
        for decoration in slots.all_decorations() {
            if let (Some(plugin), Some(tree)) =
                (names.get(decoration.plugin), decoration.tree.as_deref())
            {
                read_pictures(&mut self.images, &plugin.id, tree, &picture);
            }
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

/// Reads the pictures `tree`'s image cells name from the plugin's own
/// assets, once each. A picture from the web is not fetched by the window:
/// its cell keeps its room and shows its `text`.
fn read_pictures(
    images: &mut Images,
    plugin: &str,
    tree: &Tree,
    picture: &impl Fn(&str, &str) -> Option<iced::widget::image::Handle>,
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
        let handle = picture(plugin, path);
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
    // An installed plugin's picture is one of its assets, read from the store
    // as `assets.get` reads it. A `YAPFAST_PLUGINS_DIR` preview has no store
    // entry, so its pictures are read from its folder.
    let folders = app.plugin_kept.folders.clone();
    let picture = |id: &str, name: &str| {
        plugins
            .and_then(|plugins| plugins.picture(id, name))
            .map(iced::widget::image::Handle::from_bytes)
            .or_else(|| folders.get(id).and_then(|root| read_picture(root, name)))
    };
    // The pictures overlays show, read once each from the plugin's assets.
    for (plugin, layer) in &app.client.app.state.plugin_overlays {
        for sprite in &layer.sprites {
            let at = plugin_tree::image_key(plugin, &sprite.image);
            app.plugin_kept
                .images
                .entry(at)
                .or_insert_with(|| picture(plugin, &sprite.image));
        }
    }
    app.plugin_kept.follow(slots, picture);
    // Pictures from the web: asked for once each, through the plugin's gate,
    // and shown as they arrive; a cell shows its placeholder meanwhile.
    let wanted = app
        .plugin_kept
        .web_pictures(slots, plugins.map(|plugins| &**plugins));
    if !wanted.is_empty() {
        app.client
            .dispatch(yapfast_core::application::Action::Plugin(
                yapfast_core::application::plugins::Action::WantPictures(wanted),
            ));
    }
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

/// The buttons plugins add to the voice panel, drawn as the panel's own
/// buttons are: an even row of 32 px tiles on the raised ground, each with
/// its icon (or its words, for a button that names none) and a tooltip that
/// says what it does and whose it is, "Back · Controls plugin".
pub fn dock_buttons<'a>(
    ctx: Ctx<'a>,
    slot: Slot,
    subject: &'a str,
) -> Option<Element<'a, Message>> {
    let theme = ctx.look.theme;
    let mut tiles = row![].spacing(8).align_y(Alignment::Center);
    let mut any = false;
    for (drawn, plugin) in shown(ctx, slot) {
        let message = acted(&plugin.id, slot, subject);
        for cell in drawn.tree.rows.iter().flat_map(|row| &row.cells) {
            if cell.kind != Kind::Button {
                continue;
            }
            let said = if cell.tooltip.is_empty() {
                &cell.text
            } else {
                &cell.tooltip
            };
            let label = format!("{said} \u{b7} {} plugin", plugin.name);
            let press = cell.enabled.then(|| {
                message(Act::Set {
                    id: cell.id.clone(),
                    value: String::new(),
                })
            });
            let tile = match plugin_tree::kit_icon(&cell.icon) {
                Some(icon) => controls::dock_button(
                    ctx.look.glyph(icon, 20, theme.secondary),
                    label,
                    false,
                    theme,
                    press,
                ),
                None => {
                    let (rest, lit) = (theme.raised, theme.hover);
                    let enabled = press.is_some();
                    tooltip(
                        iced::widget::button(
                            container(text(cell.text.clone()).size(14).color(theme.secondary))
                                .center(Length::Fill),
                        )
                        .padding(0)
                        .width(Length::Fill)
                        .height(32)
                        .on_press_maybe(press)
                        .style(move |_, status| {
                            iced::widget::button::Style {
                                background: Some(
                                    if enabled
                                        && matches!(
                                            status,
                                            iced::widget::button::Status::Hovered
                                                | iced::widget::button::Status::Pressed
                                        )
                                    {
                                        lit
                                    } else {
                                        rest
                                    }
                                    .into(),
                                ),
                                border: iced::Border::default().rounded(8),
                                ..Default::default()
                            }
                        }),
                        controls::tip(theme, label),
                        tooltip::Position::Top,
                    )
                    .gap(6)
                    .into()
                }
            };
            tiles = tiles.push(tile);
            any = true;
        }
    }
    any.then(|| tiles.into())
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

/// What plugins drew on one message, as one number that changes whenever one
/// of their decorations of it does, and the marks they laid over its words.
///
/// The slots keep a decoration that didn't change as the same `Arc`, so its
/// address is what the number is made of: a row is laid out again only when a
/// plugin's answer about it really moved.
pub fn decor(slots: &Slots, item: &str) -> (u64, Vec<crate::markdown::Mark>) {
    use std::hash::{Hash, Hasher};
    use yapfast_plugin_api::message::MarkStyle;
    let mut hasher = std::hash::DefaultHasher::new();
    let mut marks = Vec::new();
    let mut any = false;
    for decoration in slots.decorations(item) {
        any = true;
        decoration.plugin.hash(&mut hasher);
        decoration
            .tree
            .as_ref()
            .map(|tree| std::sync::Arc::as_ptr(tree) as usize)
            .hash(&mut hasher);
        (std::sync::Arc::as_ptr(&decoration.inline) as usize).hash(&mut hasher);
        (decoration.hide, decoration.hide_media).hash(&mut hasher);
        marks.extend(
            decoration
                .inline
                .marks
                .iter()
                .map(|mark| crate::markdown::Mark {
                    text: mark.text.clone(),
                    look: match mark.style {
                        MarkStyle::Bold => crate::markdown::MarkLook::Bold,
                        MarkStyle::Italic => crate::markdown::MarkLook::Italic,
                        MarkStyle::Strike => crate::markdown::MarkLook::Strike,
                        MarkStyle::Highlight => crate::markdown::MarkLook::Highlight,
                    },
                }),
        );
    }
    if !any {
        return (0, marks);
    }
    (hasher.finish() | 1, marks)
}

/// What plugins add under a message's text: each one's line, its card and,
/// when it styled the text, a word saying so. Each plugin's part is credited
/// once, by one small line over it naming the plugin, with its id in a
/// tooltip ([`credit`]). Nothing here reaches the author's line, the time or a
/// badge: those are drawn above the text, and this is drawn below it.
pub fn under_message<'a>(
    ctx: Ctx<'a>,
    slots: &'a Slots,
    item: &'a str,
) -> Option<Element<'a, Message>> {
    let theme = ctx.look.theme;
    let mut shown = column![].spacing(6).width(Length::Fill);
    let mut any = false;
    for decoration in slots.decorations(item) {
        let Some(plugin) = slots.names().get(decoration.plugin) else {
            continue;
        };
        let inline = &decoration.inline;
        if inline.annotation.is_none() && decoration.tree.is_none() {
            if !inline.marks.is_empty() {
                // Styled words alone: the credit is the whole of it.
                shown = shown.push(credited(
                    theme,
                    plugin,
                    format!("Styled by {}", plugin.name),
                ));
                any = true;
            }
            continue;
        }
        // The credit over the plugin's part, so a translation reads as the
        // plugin's and never as more of what the author wrote, and the card
        // under it carries no second name.
        let mut part = column![credit(theme, plugin)].spacing(3);
        if let Some(annotation) = &inline.annotation {
            let tone = plugin_tree::tone_colour(theme, annotation_tone(&annotation.tone));
            part = part.push(
                text(annotation.text.clone())
                    .size(13)
                    .color(tone)
                    .width(Length::Fill),
            );
        }
        if let Some(tree) = &decoration.tree {
            part = part.push(
                container(
                    container(plugin_tree::draw(
                        ctx.plugin_kept.paint(ctx, &plugin.id, Slot::Message),
                        tree,
                        acted(&plugin.id, Slot::Message, item),
                    ))
                    .width(Length::Fill)
                    .clip(true),
                )
                .padding(8)
                .max_width(520)
                .style(move |_| style::rounded(theme.raised, 8)),
            );
        }
        shown = shown.push(part);
        any = true;
    }
    any.then(|| container(shown).padding(Padding::ZERO.top(4)).into())
}

/// What the plugin that replaces a built-in part (`ui.parts.replace`) draws in
/// its place, credited with a small line over it, or, where there is no room
/// for a line, in a tooltip; `None` while it hasn't drawn, and Yapfast's own
/// part shows.
pub fn replaced<'a>(
    ctx: Ctx<'a>,
    slot: Slot,
    subject: &'a str,
    line: bool,
) -> Option<Element<'a, Message>> {
    let part = slot.replaces()?;
    let overrides = ctx.plugins_ui?;
    let (_, owner) = overrides.replaced.iter().find(|(each, _)| *each == part)?;
    let owner = overrides.plugins.get(*owner)?;
    let (drawn, plugin) = shown(ctx, slot)
        .into_iter()
        .find(|(_, plugin)| plugin.id == owner.id)?;
    let body = draw(ctx, drawn, plugin, subject);
    let theme = ctx.look.theme;
    Some(if line {
        column![credit(theme, plugin), body].spacing(4).into()
    } else {
        tooltip(
            body,
            controls::tip(theme, format!("Drawn by the {}", credit_line(&plugin.name))),
            tooltip::Position::Top,
        )
        .into()
    })
}

/// How a plugin's part is credited: one small line, `Translator plugin`, with
/// the plugin's id in a tooltip. The word "plugin" is what keeps it from
/// passing for Yapfast's own; the id tells two plugins of one name apart.
pub fn credit<'a>(theme: &'a Theme, plugin: &PluginName) -> Element<'a, Message> {
    credited(theme, plugin, credit_line(&plugin.name))
}

/// [`credit`]'s words: the name, and "plugin" after it unless the name says
/// so already.
fn credit_line(name: &str) -> String {
    if name.to_lowercase().ends_with("plugin") {
        name.to_owned()
    } else {
        format!("{name} plugin")
    }
}

/// `words` in the credit's look, with the plugin's id in a tooltip.
fn credited<'a>(theme: &'a Theme, plugin: &PluginName, words: String) -> Element<'a, Message> {
    tooltip(
        text(words).size(11).font(BOLD).color(theme.muted),
        controls::tip(theme, format!("Plugin {}", plugin.id)),
        tooltip::Position::Top,
    )
    .into()
}

/// An annotation's tone as the widget tree's tones name it; the API checked it
/// is one of the five, and an unknown one draws muted.
fn annotation_tone(tone: &str) -> yapfast_core::plugins::ui::Tone {
    use yapfast_core::plugins::ui::Tone;
    match tone {
        "normal" => Tone::Normal,
        "accent" => Tone::Accent,
        "success" => Tone::Success,
        "warning" => Tone::Warning,
        _ => Tone::Muted,
    }
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
            credit(theme, name),
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
            row![container(credit(theme, plugin)).width(Length::Fill), close]
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

/// The strip over the conversation while a plugin sends as an agent, without a
/// keystroke: its name, that it is sending as you, and Stop, which takes
/// `messages.send.agent` back. It shows for two minutes after the last send.
pub fn agent_strip<'a>(cx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let shown = cx.state.plugin_agent.as_ref()?;
    if !shown.recent(yapfast_core::clock::now_ms()) {
        return None;
    }
    let theme = cx.look.theme;
    let danger = theme.danger;
    let stop = Message::PluginStopAgent(shown.plugin.clone());
    Some(
        container(
            row![
                text(format!(
                    "{} is sending messages for you, without you pressing a key",
                    shown.name
                ))
                .size(13)
                .font(BOLD)
                .color(theme.text)
                .width(Length::Fill),
                controls::action(theme, "Stop", ActionKind::Danger, Some(stop)).small(),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding([6, 12])
        .width(Length::Fill)
        .style(move |_| {
            iced::widget::container::Style::default()
                .background(iced::Color { a: 0.16, ..danger })
                .border(iced::Border {
                    color: danger,
                    width: 1.0,
                    radius: 0.0.into(),
                })
        })
        .into(),
    )
}

/// The bar at the top of the window while a plugin records the microphone:
/// a red dot, whose recording it is, how long it has run, and Stop. It is
/// Yapfast's own, drawn over everything a plugin draws, and no part a plugin
/// can hide or replace.
pub fn recording_bar<'a>(cx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let shown = cx.state.plugin_recording.as_ref()?;
    let theme = cx.look.theme;
    let danger = theme.danger;
    let seconds = yapfast_core::clock::now_ms().saturating_sub(shown.since_ms) / 1000;
    let on = iced::Color::WHITE;
    let bar = container(
        row![
            text("●").size(13).color(on),
            text(format!("{} is recording your microphone", shown.by))
                .size(13)
                .font(BOLD)
                .color(on),
            text(format!("{}:{:02}", seconds / 60, seconds % 60))
                .size(13)
                .font(MEDIUM)
                .color(on),
            iced::widget::button(text("Stop").size(12).font(BOLD).color(danger))
                .on_press(Message::PluginStopRecording)
                .padding([3, 10])
                .style(move |_, _| iced::widget::button::Style {
                    background: Some(on.into()),
                    border: iced::Border {
                        radius: 10.0.into(),
                        ..iced::Border::default()
                    },
                    ..iced::widget::button::Style::default()
                }),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([5, 8])
    .style(move |_| {
        iced::widget::container::Style::default()
            .background(danger)
            .border(iced::Border {
                radius: 16.0.into(),
                ..iced::Border::default()
            })
    });
    Some(
        container(bar)
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .padding(iced::Padding {
                top: 10.0,
                ..iced::Padding::ZERO
            })
            .into(),
    )
}

/// The line over the conversation while a plugin's sound plays: whose it is,
/// so a sound from nowhere always has a name.
pub fn playing_strip<'a>(cx: crate::ctx::Ctx<'a>) -> Option<Element<'a, Message>> {
    let shown = cx.state.plugin_playing.as_ref()?;
    if !shown.recent(yapfast_core::clock::now_ms()) {
        return None;
    }
    let theme = cx.look.theme;
    let ground = theme.raised;
    Some(
        container(
            row![
                cx.look
                    .glyph(crate::pictures::Icon::Volume, 14, theme.secondary),
                text(format!("The {} plugin is playing a sound", shown.name))
                    .size(12)
                    .font(MEDIUM)
                    .color(theme.secondary),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .padding([4, 12])
        .width(Length::Fill)
        .style(move |_| iced::widget::container::Style::default().background(ground))
        .into(),
    )
}

/// The line under a message a plugin sent, which the person didn't type.
pub fn sent_by<'a>(theme: &'a Theme, name: &str) -> Element<'a, Message> {
    text(format!("Sent by {name}, a plugin, not typed by you"))
        .size(11)
        .font(BOLD)
        .color(plugin_tree::tone_colour(
            theme,
            yapfast_core::plugins::ui::Tone::Warning,
        ))
        .into()
}
