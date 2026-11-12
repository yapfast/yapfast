//! Plugins in the window: the changes their manifests make to Yapfast's own
//! interface, applied where Yapfast builds each part. With no plugin, the
//! adapter holds `None` and none of this runs. See `docs/plugins/interface.md`.

use std::{collections::BTreeMap, sync::Arc};

use yapfast_core::plugins::{
    self,
    slots::{Output, Slot},
    ui::{
        Button, Checked, Kind, Part, Place, PluginName, ROW_PREFIX, Size, Subject, Target, Tone,
        Tree, UiOverrides,
    },
};
use yapfast_ui::{
    PluginBadge, PluginButton, PluginCell, PluginKind, PluginPart, PluginRow, PluginSurfaces,
    PluginTone,
};

use super::*;

/// The enabled plugins as the window uses them.
pub(super) struct Plugins {
    pub(super) overrides: Option<Arc<UiOverrides>>,
    /// The setting points the plugins set, read where each applies; `None`
    /// while none sets one. Yapfast lists no point yet.
    #[allow(dead_code)]
    pub(super) tweaks: Option<Arc<yapfast_core::tweaks::Tweaks>>,
    /// Where what the person does with plugins' rows and widgets goes: the
    /// plugin runtime, once one is attached.
    pub(super) runtime: Option<Box<dyn plugins::Runtime>>,
    names: Vec<PluginName>,
    /// What the plugins' folders show in slots until the runtime can,
    /// handed to the application once it exists.
    previews: Vec<Output>,
    /// Each slot's count of changes when the window last showed it.
    shown: [u64; Slot::ALL.len()],
}

impl Plugins {
    /// The plugins in the folder `YAPFAST_PLUGINS_DIR` names, with their
    /// colours laid over the theme; `None` without any, or with
    /// `--no-plugins`.
    pub(super) fn load() -> Option<Box<Self>> {
        let found = plugins::load_from_environment();
        if found.is_empty() {
            return None;
        }
        let checked: Vec<Checked> = found
            .iter()
            .map(|found| plugins::ui::check(&found.manifest))
            .collect();
        for plugin in &checked {
            for refusal in &plugin.refused {
                tracing::warn!(plugin = %plugin.id, what = %refusal.what, why = %refusal.why, "a plugin's change was refused");
            }
        }
        // Until choices are kept with grants, the first plugin keeps a
        // change two make.
        let (overrides, conflicts) = plugins::ui::derive(&checked, &BTreeMap::new());
        for conflict in conflicts {
            tracing::warn!(kept = %conflict.kept, lost = %conflict.lost, key = ?conflict.key, "two plugins change the same thing");
        }
        let tweaks: Vec<_> = found
            .iter()
            .map(|found| {
                yapfast_core::tweaks::check::<yapfast_core::tweaks::Tweaks>(&found.manifest)
            })
            .collect();
        for plugin in &tweaks {
            for (point, why) in &plugin.refused {
                tracing::warn!(plugin = %plugin.id, point, why, "a plugin's setting point was refused");
            }
        }
        let (tweaks, conflicts) = yapfast_core::tweaks::derive(&tweaks, &BTreeMap::new());
        for conflict in conflicts {
            tracing::warn!(kept = %conflict.kept, lost = %conflict.lost, point = conflict.point, "two plugins set a point differently");
        }
        let colours = overrides.as_ref().map_or_else(Vec::new, |overrides| {
            overrides
                .colours
                .iter()
                .map(|&(token, colour, _)| (token, colour))
                .collect()
        });
        theme::set_overlay(colours);
        tracing::info!(plugins = checked.len(), "loaded plugins");
        let previews = found.iter().flat_map(plugins::preview).collect();
        let names = found
            .into_iter()
            .map(|found| PluginName {
                id: found.manifest.id,
                name: found.manifest.name,
            })
            .collect();
        Some(Box::new(Self {
            overrides,
            tweaks,
            runtime: None,
            names,
            previews,
            shown: [0; Slot::ALL.len()],
        }))
    }

    /// The plugins of these overrides alone, for tests that need a window with
    /// some and no folder to load.
    #[cfg(test)]
    pub(super) fn with(
        overrides: Option<Arc<UiOverrides>>,
        runtime: Option<Box<dyn plugins::Runtime>>,
    ) -> Self {
        let names = overrides
            .as_ref()
            .map_or_else(Vec::new, |overrides| overrides.plugins.clone());
        Self {
            overrides,
            tweaks: None,
            runtime,
            names,
            previews: Vec::new(),
            shown: [0; Slot::ALL.len()],
        }
    }

    /// Tells the application which plugins are enabled, and shows what
    /// their folders preview.
    pub(super) fn enable(&mut self, app: &mut Application) {
        app.enable_plugins(self.names.clone());
        for output in self.previews.drain(..) {
            app.apply(yapfast_core::chat::Event::PluginSlot(output));
        }
    }
}

/// What plugins draw under a message: an empty model unless one decorated
/// it.
pub(super) fn decorations(slots: &plugins::slots::Slots, message: &str) -> ModelRc<PluginPart> {
    let parts: Vec<PluginPart> = slots
        .decorations(message)
        .filter_map(|kept| {
            // A plugin that only filters a row answered a hide and no tree, so
            // there is nothing of its to draw.
            let tree = kept.tree.as_ref()?;
            Some(part(
                &slots.names()[kept.plugin],
                Slot::Message,
                tree,
            ))
        })
        .collect();
    if parts.is_empty() {
        ModelRc::default()
    } else {
        ModelRc::new(VecModel::from(parts))
    }
}

/// A widget tree as the window draws it, under its plugin's name.
fn part(name: &PluginName, slot: Slot, tree: &Tree) -> PluginPart {
    tree_part(&name.id, &name.name, slot.id(), tree)
}

/// A button a plugin's manifest puts where Yapfast draws rows of its own, as
/// the rail and the header draw it. Its id is what choosing it reports.
fn row_button(name: &PluginName, button: &Button, plugin: usize) -> PluginButton {
    PluginButton {
        plugin: name.id.as_str().into(),
        name: name.name.as_str().into(),
        id: UiOverrides::row_id(plugin, &button.action).into(),
        label: button.label.as_str().into(),
    }
}

fn buttons(overrides: Option<&UiOverrides>, rows: impl Fn(&UiOverrides) -> &[(Button, usize)]) -> ModelRc<PluginButton> {
    let Some(overrides) = overrides else {
        return ModelRc::default();
    };
    let drawn: Vec<PluginButton> = rows(overrides)
        .iter()
        .filter_map(|(button, plugin)| {
            Some(row_button(overrides.plugins.get(*plugin)?, button, *plugin))
        })
        .collect();
    if drawn.is_empty() {
        ModelRc::default()
    } else {
        ModelRc::new(VecModel::from(drawn))
    }
}

/// What plugins badge a person in the member list with: an empty model unless
/// one badged them, each with its plugin's name for the row's screen reader.
pub(super) fn badges(slots: &plugins::slots::Slots, member: &str) -> ModelRc<PluginBadge> {
    let drawn: Vec<PluginBadge> = slots
        .badges(member)
        .map(|(name, badge)| PluginBadge {
            text: badge.text.as_str().into(),
            tone: tone(badge.tone),
            name: name.name.as_str().into(),
        })
        .collect();
    if drawn.is_empty() {
        ModelRc::default()
    } else {
        ModelRc::new(VecModel::from(drawn))
    }
}

/// A tree's cell tone, as the theme maps it.
fn tone(tone: Tone) -> PluginTone {
    match tone {
        Tone::Normal => PluginTone::Normal,
        Tone::Muted => PluginTone::Muted,
        Tone::Accent => PluginTone::Accent,
        Tone::Success => PluginTone::Success,
        Tone::Warning => PluginTone::Warning,
        Tone::Danger => PluginTone::Danger,
    }
}

/// A widget tree, drawn in `slot`, or the Plugins page's `settings`.
pub(super) fn tree_part(plugin: &str, name: &str, slot: &str, tree: &Tree) -> PluginPart {
    let kind = |kind: Kind| match kind {
        Kind::Text => PluginKind::Text,
        Kind::Heading => PluginKind::Heading,
        Kind::Muted => PluginKind::Muted,
        Kind::Code => PluginKind::Code,
        Kind::Badge => PluginKind::Badge,
        Kind::Icon => PluginKind::Icon,
        Kind::Image => PluginKind::Image,
        Kind::Button => PluginKind::Button,
        Kind::Link => PluginKind::Link,
        Kind::Toggle => PluginKind::Toggle,
        Kind::Select => PluginKind::Select,
        Kind::Input => PluginKind::Input,
        Kind::Slider => PluginKind::Slider,
        Kind::Progress => PluginKind::Progress,
        Kind::Divider => PluginKind::Divider,
        Kind::Spacer => PluginKind::Spacer,
    };
    let tone = |tone: Tone| match tone {
        Tone::Normal => PluginTone::Normal,
        Tone::Muted => PluginTone::Muted,
        Tone::Accent => PluginTone::Accent,
        Tone::Success => PluginTone::Success,
        Tone::Warning => PluginTone::Warning,
        Tone::Danger => PluginTone::Danger,
    };
    let rows: Vec<PluginRow> = tree
        .rows
        .iter()
        .map(|row| {
            let cells: Vec<PluginCell> = row
                .cells
                .iter()
                .map(|cell| {
                    let number = cell.value.trim().parse::<f32>().ok();
                    let amount = number.map_or(0.0, |number| {
                        ((number - cell.min) / (cell.max - cell.min)).clamp(0.0, 1.0)
                    });
                    let markdown = matches!(cell.kind, Kind::Text | Kind::Muted);
                    PluginCell {
                        kind: kind(cell.kind),
                        id: cell.id.as_str().into(),
                        text: cell.text.as_str().into(),
                        styled: if markdown {
                            slint::StyledText::from_markdown(&cell.text)
                                .unwrap_or_else(|_| slint::StyledText::from_plain_text(&cell.text))
                        } else {
                            Default::default()
                        },
                        tone: tone(cell.tone),
                        value: cell.value.as_str().into(),
                        checked: cell.checked,
                        enabled: cell.enabled,
                        // Plugins' pictures come with the runtime's assets and
                        // network gate.
                        picture: slint::Image::default(),
                        options: ModelRc::new(VecModel::from(
                            cell.options
                                .iter()
                                .map(|option| SharedString::from(option.as_str()))
                                .collect::<Vec<_>>(),
                        )),
                        selected: cell
                            .options
                            .iter()
                            .position(|option| *option == cell.value)
                            .map_or(-1, |index| index as i32),
                        minimum: cell.min,
                        maximum: cell.max,
                        amount,
                    }
                })
                .collect();
            PluginRow {
                framed: row.framed,
                indent: i32::from(row.indent),
                cells: ModelRc::new(VecModel::from(cells)),
            }
        })
        .collect();
    PluginPart {
        plugin: plugin.into(),
        name: name.into(),
        slot: slot.into(),
        rows: ModelRc::new(VecModel::from(rows)),
    }
}

/// Shows or hides each part a plugin can hide, and sets the sizes plugins
/// chose; without plugins, every part shows at Yapfast's own sizes.
pub(super) fn show(window: &YapfastWindow, plugins: Option<&Plugins>) {
    let overrides = plugins.and_then(|plugins| plugins.overrides.as_deref());
    let hides = |part| overrides.is_some_and(|overrides| overrides.hides(part));
    let parts = window.global::<yapfast_ui::UiParts>();
    parts.set_sidebar_banner(!hides(Part::SidebarBanner));
    parts.set_timeline_typing(!hides(Part::TimelineTyping));
    parts.set_message_reply(!hides(Part::MessageReply));
    parts.set_message_react(!hides(Part::MessageReact));
    parts.set_message_more(!hides(Part::MessageMore));
    let style = window.global::<ClientStyle>();
    // A negative size leaves Yapfast's own.
    let size = |size| {
        overrides
            .and_then(|overrides| overrides.size(size))
            .unwrap_or(-1.0)
    };
    style.set_text_size_plugin(size(Size::TextSize));
    style.set_sidebar_width_plugin(size(Size::SidebarWidth));
    style.set_members_width_plugin(size(Size::MembersWidth));
    style.set_channel_row_height_plugin(size(Size::ChannelRowHeight));
    style.set_message_gap_plugin(size(Size::MessageGap));
    style.set_message_group_gap_plugin(size(Size::MessageGroupGap));
    // The rows plugins' manifests put where Yapfast draws its own: the foot of
    // the rail, and the conversation's header.
    let surfaces = window.global::<PluginSurfaces>();
    surfaces.set_rail_items(buttons(overrides, |overrides| &overrides.rail));
    surfaces.set_header_buttons(buttons(overrides, |overrides| &overrides.header));
}

impl Adapter {
    /// Shows each slot whose trees changed since the window last showed
    /// it, and nothing else.
    pub(super) fn show_plugin_slots(&mut self, window: &YapfastWindow) {
        let (Some(plugins), Some(slots)) = (&mut self.plugins, &mut self.app.state.plugin_slots)
        else {
            return;
        };
        // Only the rows of messages whose decorations changed.
        let decorated = slots.take_decorated();
        if !decorated.is_empty() {
            for (index, row) in self.rows.iter_mut().enumerate() {
                if decorated.iter().any(|id| *id == row.id.as_str()) {
                    row.plugin = decorations(slots, &row.id);
                    self.models.messages.set_row_data(index, row.clone());
                }
            }
        }
        // Only the people whose badges changed.
        let badged = slots.take_badged();
        if !badged.is_empty() {
            for index in 0..self.models.roster.row_count() {
                let Some(mut row) = self.models.roster.row_data(index) else {
                    continue;
                };
                if !badged.iter().any(|id| *id == row.id.as_str()) {
                    continue;
                }
                row.plugin = badges(slots, &row.id);
                self.models.roster.set_row_data(index, row);
            }
        }
        for slot in Slot::ALL {
            let changes = slots.changes(slot);
            if plugins.shown[slot as usize] == changes {
                continue;
            }
            plugins.shown[slot as usize] = changes;
            if slot == Slot::Message {
                continue;
            }
            let parts: Vec<PluginPart> = slots
                .drawn(slot)
                .map(|drawn| part(&slots.names()[drawn.plugin], slot, &drawn.tree))
                .collect();
            let model = ModelRc::new(VecModel::from(parts));
            match slot {
                Slot::Banner => window.set_plugin_banners(model),
                Slot::Toast => window.set_plugin_toasts(model),
                Slot::Composer => window.set_plugin_composer(model),
                Slot::HoverBar => window.set_plugin_hover(model),
                Slot::Dialog => window.set_plugin_dialogs(model),
                Slot::ProfileBadges => window.set_plugin_profile(model),
                // Rows of messages, above.
                Slot::Message => {}
            }
        }
    }

    /// What the person did with a plugin's widget, for the application to
    /// pass on: a composer button acts on the open conversation.
    pub(super) fn plugin_acted(
        &mut self,
        window: &YapfastWindow,
        plugin: &str,
        slot: &str,
        id: &str,
        value: &str,
        subject: &str,
    ) {
        let Some(slot) = Slot::from_id(slot) else {
            return;
        };
        let subject = match slot {
            Slot::Composer => self.app.state.selected.clone().unwrap_or_default(),
            _ => subject.to_owned(),
        };
        let input = plugins::Input::Widget {
            plugin: plugin.to_owned(),
            slot,
            id: id.to_owned(),
            value: value.to_owned(),
            subject,
        };
        self.action(window, Action::PluginInput(input));
    }

    /// The person closed a plugin's notice or toast.
    pub(super) fn plugin_dismissed(&mut self, window: &YapfastWindow, plugin: &str, slot: &str) {
        if let Some(slot) = Slot::from_id(slot) {
            let input = plugins::Input::Dismissed {
                plugin: plugin.to_owned(),
                slot,
            };
            self.action(window, Action::PluginInput(input));
        }
    }

    pub(super) fn ui_overrides(&self) -> Option<&UiOverrides> {
        self.plugins.as_ref()?.overrides.as_deref()
    }
    /// A plugin's own row from a manifest was chosen in the rail or the header:
    /// it reports as a menu row's choice, with the open room behind it.
    pub(super) fn plugin_row_acted(&mut self, window: &YapfastWindow, id: &str) {
        let Some((plugin, action)) = self
            .ui_overrides()
            .and_then(|overrides| overrides.chosen(id))
            .map(|(plugin, action)| (plugin.id.clone(), action.to_owned()))
        else {
            return;
        };
        let target = self.app.state.selected.clone().unwrap_or_default();
        self.action(
            window,
            Action::PluginInput(plugins::Input::Menu {
                plugin,
                action,
                target,
            }),
        );
    }

    /// Hands what the person did to the plugin runtime.
    pub(super) fn plugin_input(&self, input: plugins::Input) {
        match self
            .plugins
            .as_ref()
            .and_then(|plugins| plugins.runtime.as_deref())
        {
            Some(runtime) => runtime.input(input),
            None => tracing::info!(?input, "no plugin runtime is attached to take this"),
        }
    }

    /// Draws, or takes away, a plugin's question about reaching a site.
    ///
    /// The question is the application's own state, so it is asked wherever the
    /// person is and outlives Settings: the request behind it is waiting, and
    /// nothing else can end the wait.
    pub(super) fn plugin_consent(
        &self,
        ask: Option<yapfast_core::application::plugins::ConsentAsk>,
    ) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let open = ask.is_some();
        match ask {
            Some(ask) => {
                window.set_consent_plugin(ask.name.into());
                window.set_consent_host(ask.host.into());
            }
            None => {
                window.set_consent_plugin(SharedString::default());
                window.set_consent_host(SharedString::default());
            }
        }
        // A question about what a plugin may reach is drawn in Yapfast's own
        // colours: a plugin can't restyle the dialog that decides what it
        // reaches. Another dialog that asks for them may still be open.
        theme::protect(&window, open || !window.get_leaving_link().is_empty());
    }
}

/// Which of `menu`'s menus is open, beside the id it's about.
pub(super) struct Shape {
    pub(super) person: bool,
    /// Someone's menu in a voice channel.
    pub(super) voice: bool,
    /// A thread header's More menu.
    pub(super) thread: bool,
    /// A server header's dropdown.
    pub(super) header: bool,
}

/// The menu a target names, as plugins address it, and what it's about;
/// `None` for a menu plugins can't change.
pub(super) fn menu_target(adapter: &Adapter, id: &str, shape: Shape) -> Option<(Target, String)> {
    let state = &adapter.app.state;
    if shape.voice || shape.person {
        return Some((Target::User, id.to_owned()));
    }
    if shape.thread {
        return Some((Target::Thread, id.to_owned()));
    }
    if shape.header {
        return Some((Target::ServerHeader, id.to_owned()));
    }
    if state.servers.iter().any(|server| server.id == id) {
        return Some((Target::Server, id.to_owned()));
    }
    let room = state.rooms.iter().find(|room| room.id == id)?;
    let target = match (room.direct, &room.person) {
        // A one-to-one DM's menu is its person's, as in Discord.
        (true, Some(_)) => Target::User,
        (true, None) => Target::GroupDm,
        _ if room.parent.is_some() => Target::Thread,
        _ => Target::Channel,
    };
    Some((target, id.to_owned()))
}

/// Applies plugins' changes to a menu's rows: built-in rows hidden,
/// relabelled or moved within their group, then plugins' own rows, under a
/// line after the built-in ones and before the destructive group. No row in
/// the danger tone changes.
pub(super) fn edit_menu(
    adapter: &Adapter,
    overrides: &UiOverrides,
    (target, about): (Target, String),
    rows: &mut Vec<MenuRow>,
) {
    let state = &adapter.app.state;
    let room = state.rooms.iter().find(|room| room.id == about);
    let member = matches!(target, Target::User)
        .then(|| state.person(&about))
        .flatten();
    let roles: &[String] = member.as_ref().map_or(&[], |member| &member.roles[..]);
    let subject = Subject {
        network: yapfast_core::network::Network::of(&about),
        own: state.discord_user.as_deref() == Some(about.as_str()),
        dm: room.is_some_and(|room| room.direct && room.person.is_some())
            || (target == Target::User
                && state
                    .rooms
                    .iter()
                    .any(|room| room.direct && room.person.as_deref() == Some(&about))),
        has_attachment: false,
        is_bot: member.as_ref().is_some_and(|member| member.bot),
        roles,
    };
    // Each row's group, by the lines between them.
    let mut groups: Vec<usize> = rows
        .iter()
        .scan(0, |group, row| {
            *group += usize::from(row.separated);
            Some(*group)
        })
        .collect();
    for (change, _) in &overrides.rows {
        if change.target != target || !change.when.matches(&subject) {
            continue;
        }
        let Some(index) = rows
            .iter()
            .position(|row| row.id == change.row.as_str() && !row.danger)
        else {
            continue;
        };
        if change.hide {
            rows.remove(index);
            groups.remove(index);
            continue;
        }
        if let Some(label) = &change.label {
            rows[index].text = label.as_str().into();
        }
        let Some(place) = &change.place else {
            continue;
        };
        let (Place::Before(anchor) | Place::After(anchor)) = place;
        let Some(to) = rows.iter().position(|row| row.id == anchor.as_str()) else {
            continue;
        };
        if groups[to] != groups[index] || rows[to].danger || to == index {
            continue;
        }
        let row = rows.remove(index);
        let group = groups.remove(index);
        let to = if to > index { to - 1 } else { to };
        let to = match place {
            Place::Before(_) => to,
            Place::After(_) => to + 1,
        };
        rows.insert(to, row);
        groups.insert(to, group);
    }
    // Plugins' rows go before a last group with a danger row in it.
    let destructive = groups.last().copied().filter(|&last| {
        rows.iter()
            .zip(&groups)
            .any(|(row, &group)| group == last && row.danger)
    });
    let at = destructive.map_or(rows.len(), |last| {
        groups
            .iter()
            .position(|&group| group == last)
            .unwrap_or(rows.len())
    });
    let plain = rows.first().is_some_and(|row| row.plain);
    let added: Vec<MenuRow> = overrides
        .added
        .iter()
        .filter(|(row, _)| row.target == target && row.when.matches(&subject))
        .map(|(row, plugin)| MenuRow {
            id: UiOverrides::row_id(*plugin, &row.action).into(),
            text: row.label.as_str().into(),
            // Who added it, so no plugin's row passes for Yapfast's own.
            subtitle: overrides.plugins[*plugin].name.as_str().into(),
            plain,
            ..Default::default()
        })
        .collect();
    let count = added.len();
    let group = usize::MAX;
    rows.splice(at..at, added);
    groups.splice(at..at, std::iter::repeat_n(group, count));
    for index in 0..rows.len() {
        rows[index].separated = index > 0 && groups[index] != groups[index - 1];
    }
}

/// Carries out a plugin's menu row: tells the plugin which action, on
/// what. False for an id that isn't a plugin's.
pub(super) fn choose(
    adapter: &mut Adapter,
    window: &YapfastWindow,
    target: &str,
    id: &str,
) -> bool {
    if !id.starts_with(ROW_PREFIX) {
        return false;
    }
    let Some((plugin, action)) = adapter
        .ui_overrides()
        .and_then(|overrides| overrides.chosen(id))
        .map(|(plugin, action)| (plugin.id.clone(), action.to_owned()))
    else {
        return true;
    };
    adapter.action(
        window,
        Action::PluginInput(plugins::Input::Menu {
            plugin,
            action,
            target: target.to_owned(),
        }),
    );
    true
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use yapfast_core::plugins::{Input, Manifest};

    use super::*;

    /// The overrides of manifests with these `ui` sections and rows, each
    /// plugin declaring `ui.modify`.
    fn overrides(plugins: &[(&str, &str)]) -> Arc<UiOverrides> {
        let checked: Vec<Checked> = plugins
            .iter()
            .enumerate()
            .map(|(index, (ui, menus))| {
                let manifest = Manifest::parse(&format!(
                    r#"{{ "api": 1, "id": "com.example.p{index}", "name": "Plugin {index}",
                        "permissions": {{ "capabilities": ["ui.modify"] }},
                        "ui": {ui}, "menus": {menus} }}"#
                ))
                .unwrap();
                plugins::ui::check(&manifest)
            })
            .collect();
        plugins::ui::derive(&checked, &BTreeMap::new()).0.unwrap()
    }

    fn adapter(window: &YapfastWindow) -> Rc<RefCell<Adapter>> {
        Adapter::new(
            Arc::new(BackgroundRuntime::new().unwrap()),
            tempfile::tempdir().unwrap().keep(),
            window,
        )
        .unwrap()
    }

    fn row(id: &str, separated: bool, danger: bool) -> MenuRow {
        MenuRow {
            id: id.into(),
            text: id.into(),
            separated,
            danger,
            ..Default::default()
        }
    }

    /// A person's menu: what to do, notes, copying, then the danger group.
    fn person_rows() -> Vec<MenuRow> {
        vec![
            row("profile", false, false),
            row("mention", false, false),
            row("message", false, false),
            row("add-note", true, false),
            row("copy-id", true, false),
            row("ignore", true, false),
            row("block", false, true),
        ]
    }

    fn shown(rows: &[MenuRow]) -> Vec<String> {
        rows.iter()
            .map(|row| {
                let line = if row.separated { "| " } else { "" };
                format!("{line}{}", row.text)
            })
            .collect()
    }

    #[test]
    fn menus_hide_relabel_and_move_rows_within_their_group() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let adapter = adapter(&window);
        let overrides = overrides(&[(
            r#"{ "menu": [
                { "target": "user", "row": "add-note", "hide": true },
                { "target": "user", "row": "message", "before": "profile" },
                { "target": "user", "row": "copy-id", "label": "Copy user ID" },
                { "target": "user", "row": "mention", "after": "copy-id" },
                { "target": "server", "row": "profile", "hide": true },
                { "target": "user", "row": "ignore", "hide": true, "when": { "network": "discord" } }
            ] }"#,
            "[]",
        )]);
        let mut rows = person_rows();
        edit_menu(
            &adapter.borrow(),
            &overrides,
            (Target::User, "@ada:example.org".into()),
            &mut rows,
        );
        // Mention stays in its group, as Copy ID is in another; Ignore
        // stays, as a Matrix person's menu isn't Discord's.
        assert_eq!(
            shown(&rows),
            [
                "message",
                "profile",
                "mention",
                "| Copy user ID",
                "| ignore",
                "block"
            ]
        );
    }

    #[test]
    fn plugins_rows_come_before_the_destructive_group_and_name_their_plugin() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let adapter = adapter(&window);
        let overrides = overrides(&[
            (
                "{}",
                r#"[{ "target": "user", "label": "Wave", "action": "wave" }]"#,
            ),
            (
                "{}",
                r#"[{ "target": "user", "label": "Bots only", "action": "b", "when": { "is_bot": true } },
                    { "target": "server", "label": "Elsewhere", "action": "c" },
                    { "target": "user", "label": "Hug", "action": "hug" }]"#,
            ),
        ]);
        let mut rows = person_rows();
        // The danger group starts with a line of its own.
        rows[6].separated = true;
        edit_menu(
            &adapter.borrow(),
            &overrides,
            (Target::User, "someone".into()),
            &mut rows,
        );
        assert_eq!(shown(&rows)[5..], ["| ignore", "| Wave", "Hug", "| block"]);
        assert_eq!(rows[6].id, "plugin/0/wave");
        assert_eq!(rows[6].subtitle, "Plugin 0");
        assert_eq!(rows[7].id, "plugin/1/hug");
        // Without a destructive group, they go last.
        let mut rows = vec![row("profile", false, false)];
        edit_menu(
            &adapter.borrow(),
            &overrides,
            (Target::User, "someone".into()),
            &mut rows,
        );
        assert_eq!(shown(&rows), ["profile", "| Wave", "Hug"]);
    }

    #[test]
    fn a_danger_row_stays_even_if_the_overrides_name_it() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let adapter = adapter(&window);
        // `check` refuses this; the menu holds the line too.
        let mut overrides = (*overrides(&[(
            r#"{ "menu": [{ "target": "user", "row": "profile", "hide": true }] }"#,
            "[]",
        )]))
        .clone();
        overrides.rows[0].0.row = "block".into();
        let mut rows = person_rows();
        edit_menu(
            &adapter.borrow(),
            &overrides,
            (Target::User, "someone".into()),
            &mut rows,
        );
        assert_eq!(rows.len(), 7);
    }

    #[test]
    fn a_plugins_row_goes_to_the_runtime() {
        struct Recorder(Rc<RefCell<Vec<Input>>>);
        impl plugins::Runtime for Recorder {
            fn input(&self, input: Input) {
                self.0.borrow_mut().push(input);
            }
        }
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let adapter = adapter(&window);
        let received = Rc::new(RefCell::new(Vec::new()));
        adapter.borrow_mut().plugins = Some(Box::new(Plugins::with(
            Some(overrides(&[(
                "{}",
                r#"[{ "target": "user", "label": "Wave", "action": "wave" }]"#,
            )])),
            Some(Box::new(Recorder(received.clone()))),
        )));
        menus::choose(
            &mut adapter.borrow_mut(),
            &window,
            "someone".into(),
            true,
            "plugin/0/wave",
        );
        // An index no plugin has goes nowhere, and isn't a built-in row.
        menus::choose(
            &mut adapter.borrow_mut(),
            &window,
            "someone".into(),
            true,
            "plugin/7/wave",
        );
        assert_eq!(
            *received.borrow(),
            [Input::Menu {
                plugin: "com.example.p0".into(),
                action: "wave".into(),
                target: "someone".into(),
            }]
        );
    }

    #[test]
    fn parts_and_sizes_show_as_plugins_set_them_and_return_without() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let plugins = Plugins::with(
            Some(overrides(&[(
                r#"{ "parts": { "hide": ["timeline.typing", "message.actions.more"] },
                     "sizes": { "message-group-gap": 4, "text-size": 18 } }"#,
                "[]",
            )])),
            None,
        );
        show(&window, Some(&plugins));
        let parts = window.global::<yapfast_ui::UiParts>();
        let style = window.global::<ClientStyle>();
        assert!(!parts.get_timeline_typing());
        assert!(!parts.get_message_more());
        assert!(parts.get_message_reply() && parts.get_sidebar_banner());
        assert_eq!(style.get_message_group_gap(), 4.0);
        assert_eq!(style.get_text_size(), 18.0);
        assert_eq!(style.get_message_gap(), 0.0, "Yapfast's own");
        show(&window, None);
        assert!(parts.get_timeline_typing() && parts.get_message_more());
        assert_eq!(style.get_message_group_gap(), 16.0);
        assert_eq!(style.get_text_size(), 15.0);
    }

    #[test]
    fn a_slot_shows_what_the_runtime_sets_and_closing_it_tells_the_plugin() {
        struct Recorder(Rc<RefCell<Vec<Input>>>);
        impl plugins::Runtime for Recorder {
            fn input(&self, input: Input) {
                self.0.borrow_mut().push(input);
            }
        }
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let adapter = adapter(&window);
        let received = Rc::new(RefCell::new(Vec::new()));
        let mut plugins = Plugins::with(
            Some(overrides(&[(
                "{}",
                r#"[{ "target": "user", "label": "Wave", "action": "wave" }]"#,
            )])),
            Some(Box::new(Recorder(received.clone()))),
        );
        let mut adapter = adapter.borrow_mut();
        plugins.enable(&mut adapter.app);
        adapter.plugins = Some(Box::new(plugins));
        let event = yapfast_core::chat::Event::PluginSlot(Output::SetSlot {
            plugin: "com.example.p0".into(),
            slot: Slot::Banner,
            tree: Tree::parse(
                r#"{ "rows": [{ "cells": [
                    { "kind": "text", "text": "Hi **there**", "tone": "success" },
                    { "kind": "slider", "id": "v", "min": 0, "max": 10, "value": "5" },
                    { "kind": "select", "id": "s", "options": ["a", "b"], "value": "b" }
                ] }] }"#,
            )
            .unwrap(),
        });
        let reach = refresh::Reach::of(&event);
        adapter.app.apply(event);
        adapter.show(&window, reach);
        let banners = window.get_plugin_banners();
        assert_eq!(banners.row_count(), 1);
        let banner = banners.row_data(0).unwrap();
        assert_eq!(
            (banner.name.as_str(), banner.slot.as_str()),
            ("Plugin 0", "banner")
        );
        let cells = banner.rows.row_data(0).unwrap().cells;
        assert_eq!(cells.row_data(0).unwrap().tone, PluginTone::Success);
        assert_eq!(cells.row_data(1).unwrap().amount, 0.5);
        assert_eq!(cells.row_data(2).unwrap().selected, 1);
        assert_eq!(window.get_plugin_toasts().row_count(), 0, "untouched");
        // A composer button acts on the open conversation.
        adapter.app.state.selected = Some("general".into());
        adapter.plugin_acted(&window, "com.example.p0", "composer", "shrug", "", "");
        adapter.plugin_dismissed(&window, "com.example.p0", "banner");
        assert_eq!(window.get_plugin_banners().row_count(), 0);
        assert_eq!(
            *received.borrow(),
            [
                Input::Widget {
                    plugin: "com.example.p0".into(),
                    slot: Slot::Composer,
                    id: "shrug".into(),
                    value: String::new(),
                    subject: "general".into(),
                },
                Input::Dismissed {
                    plugin: "com.example.p0".into(),
                    slot: Slot::Banner,
                },
            ]
        );
    }

    #[test]
    fn the_colours_plugins_can_name_are_the_themes_own() {
        let names: Vec<String> = theme::Theme::NAMES
            .iter()
            .map(|name| name.replace('_', "-"))
            .collect();
        assert_eq!(names, plugins::ui::COLOURS);
    }

    #[test]
    fn colours_lay_over_the_theme_but_not_over_a_protected_dialog() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let blue = slint::Color::from_rgb_u8(0x7a, 0xa2, 0xff);
        // Black text on Yap's dark surfaces is refused.
        theme::set_overlay(vec![("accent", 0xff7a_a2ff), ("text", 0xff00_0000)]);
        theme::YAP.apply(&window);
        let shown = theme::Theme::shown(&window);
        assert_eq!(shown.accent, blue);
        assert_eq!(shown.text, theme::YAP.text);
        theme::protect(&window, true);
        assert_eq!(theme::Theme::shown(&window), theme::YAP);
        theme::protect(&window, false);
        assert_eq!(theme::Theme::shown(&window).accent, blue);
        theme::set_overlay(Vec::new());
        theme::YAP.apply(&window);
        assert_eq!(theme::Theme::shown(&window), theme::YAP);
    }
}
