//! A theme's lists on the window: the rail's tiles, the channel list's rows
//! and the member list's rows (`layout.rail_tiles`, `layout.channel_rows`,
//! `layout.member_rows`), through the Slint globals `Rails`, `ChannelLists`
//! and `MemberLists` (`ui/theme_lists.slint`). See `docs/themes.md`.
//!
//! Every picture is decoded once per theme, no larger than twice the size
//! it's drawn at, and shared by every row as one refcounted `slint::Image`:
//! a row only picks among a slot's pictures by its seed. Nothing is decoded
//! per row, per scroll or per hover; [`decodes`] counts what was, for the
//! test that says so. [`reset`] puts every value back to stock, which leaves
//! the stock rows drawing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use slint::{Color, ComponentHandle, Image, ModelRc, SharedString, VecModel};
use yapfast_core::themes::Performance;
use yapfast_core::themes::layout::RailSide;
use yapfast_core::themes::lists::{
    Case, FontRole, Glyph, HeaderStyle, HoverGround, HoverKind, LabelPlace, PillMode, Separator, StatusStyle,
    SubLine, TileArt, UnreadMark,
};
use yapfast_core::themes::package::art::Fit;
use yapfast_core::themes::package::{Resolved, Role};
use yapfast_ui::{
    ArtImage, ChannelLists, Chrome, ListGlyph, MemberLists, Picker, PickerRow, Rails, TileLayer, YapfastWindow,
};

/// Pictures decoded since the program started: the decode-once test reads it.
static DECODES: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub fn decodes() -> usize {
    DECODES.load(Ordering::Relaxed)
}

#[derive(Default)]
struct State {
    folder: PathBuf,
    /// Decoded pictures by the layer that made them, while the theme shows.
    cache: HashMap<String, Image>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Answers the globals' callbacks. Called once per window.
pub fn init(window: &YapfastWindow) {
    window
        .global::<ChannelLists>()
        .on_fill(|template, name, n| {
            fill(&template, &[("name", name.as_str()), ("n", n.to_string().as_str())]).into()
        });
    window.global::<MemberLists>().on_fill(|template, role, n, name, detail, activity| {
        // A heading passes its words whole, "Leads — 2", as `role`.
        let (role, n) = if n.is_empty() {
            match role.rsplit_once(" — ") {
                Some((role, count)) => (role.to_owned(), count.to_owned()),
                None => (role.to_string(), String::new()),
            }
        } else {
            (role.to_string(), n.to_string())
        };
        let either = if detail.is_empty() { name.to_string() } else { detail.to_string() };
        fill(
            &template,
            &[
                ("role", role.as_str()),
                ("n", n.as_str()),
                ("name", name.as_str()),
                ("detail", detail.as_str()),
                ("detail|name", either.as_str()),
                ("activity", activity.as_str()),
            ],
        )
        .into()
    });
    let picker = window.global::<Picker>();
    picker.set_rows(ModelRc::from(PICKER.with_borrow(|state| state.model.clone())));
    picker.on_toggle(|folder| {
        PICKER.with_borrow_mut(|picker| {
            if !picker.expanded.remove(folder.as_str()) {
                picker.expanded.insert(folder.to_string());
            }
        });
        show_picker();
    });
    picker.on_filter(|folder| {
        PICKER.with_borrow_mut(|picker| picker.folder = folder.to_string());
        show_picker();
    });
    picker.on_count_of(|folder| picker_count(&folder));
    window
        .global::<MemberLists>()
        .on_numbered(|template, n| {
            let n = n.to_string();
            fill(&template, &[("online", n.as_str()), ("n", n.as_str())]).into()
        });
}

/// A template with its placeholders filled. A part between " · " whose
/// placeholders all came out empty is left out, with its separator, so
/// "{role} · {activity}" reads "Crew" for someone doing nothing.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let one = |part: &str| -> (String, bool, bool) {
        let mut out = part.to_owned();
        let (mut any, mut filled) = (false, false);
        for (key, value) in values {
            let mark = format!("{{{key}}}");
            if out.contains(&mark) {
                any = true;
                filled |= !value.is_empty();
                out = out.replace(&mark, value);
            }
        }
        (out, any, filled)
    };
    let parts: Vec<String> = template
        .split(" · ")
        .filter_map(|part| {
            let (out, any, filled) = one(part);
            (!any || filled).then_some(out)
        })
        .collect();
    parts.join(" · ")
}

/// The server picker a hidden rail's top bar opens: every row, which folders
/// it shows open, which one alone (empty for all), and the model it shows.
#[derive(Default)]
struct PickerState {
    hidden: bool,
    all: Vec<PickerRow>,
    expanded: std::collections::BTreeSet<String>,
    folder: String,
    model: std::rc::Rc<VecModel<PickerRow>>,
}

thread_local! {
    static PICKER: RefCell<PickerState> = RefCell::new(PickerState::default());
}

/// Whether the shown theme hides the rail, which its top bar stands in for.
pub fn rail_hidden() -> bool {
    PICKER.with_borrow(|picker| picker.hidden)
}

/// Rebuilds the picker's rows from the servers, folders and unread DMs, in
/// the rail's order, when the rail is hidden; `numbers` is how many
/// workspaces the bar shows, which number the first top-level rows.
pub fn picker(state: &yapfast_core::application::AppState, numbers: usize) {
    if !rail_hidden() {
        return;
    }
    let unread_rooms: HashMap<&str, (bool, u64)> = state
        .rooms
        .iter()
        .map(|room| (room.id.as_str(), (room.mentions > 0 || room.unread > 0, room.mentions)))
        .collect();
    let marks = |server: &yapfast_core::chat::ChatServer| {
        let mut unread = false;
        let mut mentions = 0;
        for channel in &server.channels {
            if let Some((one, more)) = unread_rooms.get(channel.as_str()) {
                unread |= *one && !server.muted;
                mentions += more;
            }
        }
        (unread, mentions)
    };
    let selected = |server: &yapfast_core::chat::ChatServer| {
        state.navigation == yapfast_core::application::Navigation::Server(server.id.clone())
    };
    let badge = |count: u64| -> SharedString {
        match count {
            0 => SharedString::new(),
            1..=99 => count.to_string().into(),
            _ => "99+".into(),
        }
    };
    let row = |server: &yapfast_core::chat::ChatServer, number: String, nested: bool| {
        let (unread, mentions) = marks(server);
        PickerRow {
            id: server.id.as_str().into(),
            name: server.name.as_str().into(),
            initial: server.name.chars().next().map(|first| first.to_uppercase().to_string()).unwrap_or_default().into(),
            picture_url: server.icon.as_deref().unwrap_or_default().into(),
            number: number.into(),
            selected: selected(server),
            unread,
            badge: badge(mentions),
            nested,
            ..PickerRow::default()
        }
    };
    let mut rows: Vec<PickerRow> = super::refresh::unread_dm_rows(state)
        .into_iter()
        .map(|dm| PickerRow {
            id: dm.id,
            name: dm.name,
            initial: dm.initial,
            picture_url: dm.picture_url,
            unread: true,
            badge: dm.badge,
            dm: true,
            ..PickerRow::default()
        })
        .collect();
    let mut place = 0;
    let mut servers = state.servers.iter().peekable();
    while let Some(server) = servers.next() {
        place += 1;
        let number = if place <= numbers { place.to_string() } else { String::new() };
        let Some(folder) = &server.folder else {
            rows.push(row(server, number, false));
            continue;
        };
        let mut inside = vec![server];
        while let Some(next) = servers.next_if(|next| next.folder.as_ref() == Some(folder)) {
            inside.push(next);
        }
        let (unread, mentions) = inside
            .iter()
            .map(|server| marks(server))
            .fold((false, 0), |(unread, sum), (one, more)| (unread || one, sum + more));
        rows.push(PickerRow {
            id: folder.id.as_str().into(),
            name: folder
                .name
                .clone()
                .unwrap_or_else(|| inside.iter().map(|server| server.name.as_str()).take(3).collect::<Vec<_>>().join(", "))
                .into(),
            number: number.into(),
            selected: inside.iter().any(|server| selected(server)),
            unread,
            badge: badge(mentions),
            folder: true,
            preview: ModelRc::new(VecModel::from(
                inside
                    .iter()
                    .take(4)
                    .map(|server| yapfast_ui::FolderServer {
                        initial: server.name.chars().next().map(|first| first.to_uppercase().to_string()).unwrap_or_default().into(),
                        picture_url: server.icon.as_deref().unwrap_or_default().into(),
                    })
                    .collect::<Vec<_>>(),
            )),
            ..PickerRow::default()
        });
        for server in inside {
            rows.push(row(server, String::new(), true));
        }
    }
    rows.push(PickerRow {
        id: "@join".into(),
        name: "Join a server".into(),
        join: true,
        ..PickerRow::default()
    });
    PICKER.with_borrow_mut(|picker| picker.all = rows);
    show_picker();
}

/// The rows the picker shows: one folder's servers, or every row with the
/// open folders' servers under them.
fn show_picker() {
    PICKER.with_borrow(|picker| {
        let mut shown = Vec::new();
        let mut inside: Option<&str> = None;
        for row in &picker.all {
            if row.folder {
                inside = Some(row.id.as_str());
            } else if !row.nested {
                inside = None;
            }
            if !picker.folder.is_empty() {
                if row.nested && inside == Some(picker.folder.as_str()) {
                    shown.push(PickerRow { nested: false, ..row.clone() });
                }
                continue;
            }
            if row.nested && !inside.is_some_and(|folder| picker.expanded.contains(folder)) {
                continue;
            }
            let open = row.folder && picker.expanded.contains(row.id.as_str());
            shown.push(PickerRow { open, ..row.clone() });
        }
        picker.model.set_vec(shown);
    });
}

/// How many servers the picker counts: all of them, or a folder's.
fn picker_count(folder: &str) -> i32 {
    PICKER.with_borrow(|picker| {
        let mut inside: Option<&str> = None;
        let mut count = 0;
        for row in &picker.all {
            if row.folder {
                inside = Some(row.id.as_str());
            } else if !row.nested {
                inside = None;
            }
            let server = !row.folder && !row.join && !row.dm;
            if server && (folder.is_empty() || inside == Some(folder)) {
                count += 1;
            }
        }
        count
    })
}

/// Every value back to stock.
pub fn reset(window: &YapfastWindow) {
    let rails = window.global::<Rails>();
    rails.set_on(false);
    rails.set_frames(ModelRc::default());
    rails.set_overs(ModelRc::default());
    for set in [Rails::set_below, Rails::set_between, Rails::set_sel_frame, Rails::set_sel_over, Rails::set_folder_frame, Rails::set_folder_open_frame] {
        set(&rails, TileLayer::default());
    }
    rails.set_reflection(false);
    rails.set_hover(0);
    rails.set_label(0);
    rails.set_tilt(0.);
    rails.set_sel_dx(0.);
    rails.set_sel_dy(0.);
    rails.set_sel_scale(1.);
    rails.set_stamp_time(0);
    rails.set_pill(0);
    rails.set_focus_window(false);
    rails.set_right(false);
    rails.set_divider_on(false);
    window.global::<Chrome>().set_rail_tiles_px(0.);
    window.global::<Picker>().set_on(false);
    PICKER.with_borrow_mut(|picker| picker.hidden = false);
    let channels = window.global::<ChannelLists>();
    channels.set_on(false);
    channels.set_frames(ModelRc::default());
    channels.set_head_on(false);
    channels.set_sep_on(false);
    channels.set_sel_frame(TileLayer::default());
    channels.set_after(TileLayer::default());
    channels.set_sep_art(TileLayer::default());
    channels.set_head_frame(TileLayer::default());
    channels.set_underline(false);
    channels.set_prefix(SharedString::new());
    channels.set_blink(0);
    let members = window.global::<MemberLists>();
    members.set_on(false);
    members.set_frames(ModelRc::default());
    members.set_avatar_frames(ModelRc::default());
    members.set_glyphs(ModelRc::default());
    members.set_sel_frame(TileLayer::default());
    members.set_sel_over(TileLayer::default());
    members.set_head_frame(TileLayer::default());
    members.set_head_on(false);
    members.set_ring_on(false);
    members.set_meta_on(false);
    members.set_badge_on(false);
    members.set_sep_on(false);
    members.set_grid_columns(0);
}

/// Forgets the decoded pictures.
pub fn forget() {
    STATE.with_borrow_mut(|state| state.cache.clear());
}

/// The face, size and weight a list's text takes from a type role: the
/// theme's registered family for it, else the body's, else the stock one.
struct Face {
    family: SharedString,
    size: Option<f32>,
    weight: Option<i32>,
}

fn face(resolved: &Resolved, fonts: &[(Role, String)], role: FontRole) -> Face {
    let role = match role {
        FontRole::Body | FontRole::Small => Role::Body,
        FontRole::Names => Role::Names,
        FontRole::Labels => Role::Labels,
        FontRole::Headings => Role::Headings,
        FontRole::Code => Role::Code,
    };
    let family = fonts
        .iter()
        .find(|(known, _)| *known == role)
        .or_else(|| fonts.iter().find(|(known, _)| *known == Role::Body))
        .map(|(_, family)| family.as_str().into())
        .unwrap_or_default();
    let kind = resolved.types.get(&role);
    Face {
        family,
        size: kind.and_then(|kind| kind.size),
        weight: kind.and_then(|kind| kind.weight),
    }
}

fn case(case: Option<Case>) -> i32 {
    match case {
        None | Some(Case::AsIs) => 0,
        Some(Case::Upper) => 1,
        Some(Case::Lower) => 2,
    }
}

fn colour(argb: Option<u32>) -> Color {
    argb.map_or(Color::default(), Color::from_argb_encoded)
}

/// A picture decoded once per theme: no larger than twice its drawn size,
/// a nine-sliced one at its own pixels, since its edges count them.
fn picture(folder: &Path, art: &TileArt) -> Option<Image> {
    // A picture drawn at a set size needs no more than twice it; one that
    // fills its row or tile, or is nine-sliced (its edges count its own
    // pixels), keeps its own pixels, as theme art is made at twice its
    // drawn size anyway.
    let cap = match (art.size, art.slice) {
        (Some((width, height)), None) => (
            ((width * 2.0).ceil() as u32).clamp(8, 1024),
            ((height * 2.0).ceil() as u32).clamp(8, 1024),
        ),
        _ => (1024, 1024),
    };
    let mut layer = art.layer.clone();
    // Opacity stays apart, for a hover to raise.
    layer.opacity = 1.0;
    let key = format!("{layer:?}:{cap:?}");
    if let Some(image) = STATE.with_borrow(|state| state.cache.get(&key).cloned()) {
        return Some(image);
    }
    // A sprite sheet keeps all its frames: its cap is per frame.
    let cap = match layer.frames {
        Some(frames) if frames.count > 1 => (
            cap.0 * frames.columns.max(1),
            cap.1 * frames.count.div_ceil(frames.columns.max(1)),
        ),
        _ => cap,
    };
    let pixels = super::theme_art::baked_at(folder, &layer, 0xff00_0000, cap)?;
    DECODES.fetch_add(1, Ordering::Relaxed);
    let image = super::theme_art::slint_image(pixels);
    STATE.with_borrow_mut(|state| state.cache.insert(key, image.clone()));
    Some(image)
}

/// A slot's picture as the interface takes it; off when it can't be read.
fn layer(folder: &Path, art: &TileArt, moves: bool) -> TileLayer {
    let Some(image) = picture(folder, art) else {
        return TileLayer::default();
    };
    let decoded = image.size();
    let frames = art.layer.frames.map_or((1, 1, 1000.0), |frames| {
        (frames.count as i32, frames.columns.max(1) as i32, frames.period)
    });
    let (x, y) = art.layer.position;
    // A nine-slice picture that was shrunk to fit its cap counts its edges
    // in its own pixels still: it never is, as its cap is generous.
    let [top, right, bottom, left] = art.slice.unwrap_or([0.0; 4]);
    let window = art.window.unwrap_or([0.0; 4]);
    TileLayer {
        on: true,
        picture: ArtImage {
            image,
            fit: match art.layer.fit {
                Fit::Cover => 0,
                Fit::Contain => 1,
                Fit::Tile => 2,
                Fit::Stretch => 3,
            },
            halign: 1,
            valign: 1,
            drift_x: 0.,
            drift_y: 0.,
            frames: frames.0,
            columns: frames.1,
            period: frames.2,
            jitter: 0.,
            moves: moves && frames.0 > 1,
        },
        opacity: art.layer.opacity,
        sized: art.size.is_some(),
        w: art.size.map_or(0., |size| size.0),
        h: art.size.map_or(0., |size| size.1),
        ax: x,
        ay: y,
        dx: art.offset.0,
        dy: art.offset.1,
        rotation: art.rotation,
        sliced: art.slice.is_some() && decoded.width > 0,
        st: top as i32,
        sr: right as i32,
        sb: bottom as i32,
        sl: left as i32,
        density: art.density,
        pixelated: art.pixelated,
        window: art.window.is_some(),
        wx: window[0],
        wy: window[1],
        ww: window[2],
        wh: window[3],
        wr: art.window_radius,
    }
}

fn layers(folder: &Path, arts: &[TileArt], moves: bool) -> ModelRc<TileLayer> {
    ModelRc::new(VecModel::from(
        arts.iter().map(|art| layer(folder, art, moves)).collect::<Vec<_>>(),
    ))
}

fn one(folder: &Path, art: Option<&TileArt>, moves: bool) -> TileLayer {
    art.map_or_else(TileLayer::default, |art| layer(folder, art, moves))
}

fn glyph(folder: &Path, glyph: Option<&Glyph>, moves: bool) -> ListGlyph {
    match glyph {
        None => ListGlyph::default(),
        Some(Glyph::Text(text)) => ListGlyph {
            text: text.as_str().into(),
            art: TileLayer::default(),
        },
        Some(Glyph::Art(art)) => ListGlyph {
            text: SharedString::new(),
            art: layer(folder, art, moves),
        },
    }
}

fn hover_kind(kind: Option<HoverKind>) -> i32 {
    match kind {
        None | Some(HoverKind::None) => 0,
        Some(HoverKind::Magnify) => 1,
        Some(HoverKind::Lift) => 2,
        Some(HoverKind::Tilt) => 3,
    }
}

/// Sets a theme's lists, as Performance and reduced motion allow: under
/// either, hovers and stamps reach their end at once and nothing blinks.
/// Called by `theme_look::paint` after `reset`; `fonts` are the families
/// registered for each type role.
pub fn apply(
    window: &YapfastWindow,
    resolved: &Resolved,
    folder: &Path,
    fonts: &[(Role, String)],
    performance: Performance,
    reduced: bool,
) {
    STATE.with_borrow_mut(|state| {
        if state.folder != folder {
            state.cache.clear();
            state.folder = folder.to_owned();
        }
    });
    let still = performance == Performance::Light || reduced;
    let time = |ms: f32| if still { 0 } else { ms.round() as i64 };
    let layout = &resolved.layout;
    let hidden = layout.rail == Some(RailSide::Hidden);
    PICKER.with_borrow_mut(|picker| picker.hidden = hidden);
    window.global::<Picker>().set_on(hidden);
    if let Some(tiles) = layout.rail_tiles.as_ref().filter(|_| !hidden) {
        let rails = window.global::<Rails>();
        rails.set_on(true);
        rails.set_shape(tiles.shape.index());
        rails.set_tile_w(tiles.width);
        rails.set_tile_h(tiles.height);
        rails.set_radius(tiles.radius);
        rails.set_gap(tiles.gap);
        rails.set_pad(tiles.pad);
        // A lean is static: it stays under reduced motion, and Light
        // leaves the tiles straight, as it does cards.
        rails.set_tilt(if performance == Performance::Light { 0. } else { tiles.tilt });
        rails.set_label(match tiles.label {
            LabelPlace::None => 0,
            LabelPlace::Below => 1,
            LabelPlace::Inside => 2,
        });
        let label = face(resolved, fonts, tiles.label_font);
        rails.set_label_family(label.family);
        rails.set_label_size(tiles.label_size);
        rails.set_label_weight(label.weight.unwrap_or(400));
        rails.set_label_colour(colour(tiles.label_colour));
        rails.set_label_ground(colour(tiles.label_ground));
        rails.set_label_case(case(Some(tiles.label_case)));
        if let Some([x, y, width, height]) = tiles.label_box {
            rails.set_label_boxed(true);
            rails.set_label_x(x);
            rails.set_label_y(y);
            rails.set_label_w(width);
            rails.set_label_h(height);
        } else {
            rails.set_label_boxed(false);
        }
        rails.set_frames(layers(folder, &tiles.frame, !still));
        rails.set_overs(layers(folder, &tiles.over, !still));
        rails.set_below(one(folder, tiles.below.as_ref(), !still));
        rails.set_between(one(folder, tiles.between.as_ref(), !still));
        rails.set_between_height(tiles.between_height);
        rails.set_reflection(tiles.reflection);
        rails.set_hover(hover_kind(Some(tiles.hover)));
        rails.set_hover_amount(tiles.hover_amount);
        rails.set_hover_time(time(tiles.hover_ms));
        rails.set_sel_frame(one(folder, tiles.selected.frame.as_ref(), !still));
        rails.set_sel_over(one(folder, tiles.selected.over.as_ref(), !still));
        rails.set_sel_dx(tiles.selected.offset.0);
        rails.set_sel_dy(tiles.selected.offset.1);
        rails.set_sel_scale(tiles.selected.scale.unwrap_or(1.));
        rails.set_stamp_time(time(tiles.selected.stamp_ms));
        rails.set_pill(match tiles.pill {
            PillMode::On => 0,
            PillMode::Off => 1,
            PillMode::Unread => 2,
        });
        rails.set_focus_window(tiles.focus == yapfast_core::themes::lists::FocusPlace::Window);
        rails.set_folder_frame(one(folder, tiles.folder_frame.as_ref(), !still));
        rails.set_folder_open_frame(one(folder, tiles.folder_open_frame.as_ref(), !still));
        rails.set_right(layout.rail == Some(RailSide::Right));
        if let Some(divider) = &tiles.divider {
            rails.set_divider_on(true);
            rails.set_divider_w(divider.size.0);
            rails.set_divider_h(divider.size.1);
            rails.set_divider_colour(Color::from_argb_encoded(divider.colour));
            rails.set_divider_dotted(divider.dotted);
        }
        rails.set_focus_radius(tiles.focus_radius.unwrap_or(-1.));
        rails.set_badge_ground(colour(tiles.badge.ground));
        rails.set_badge_ink(colour(tiles.badge.ink));
        window.global::<Chrome>().set_rail_tiles_px(tiles.rail_width());
    }
    if let Some(rows) = &layout.channel_rows {
        let lists = window.global::<ChannelLists>();
        lists.set_on(true);
        lists.set_height(rows.height.unwrap_or(-1.));
        lists.set_gap(rows.gap.unwrap_or(2.));
        let text = face(resolved, fonts, rows.font.unwrap_or(FontRole::Body));
        lists.set_family(if rows.font.is_some() { text.family } else { SharedString::new() });
        lists.set_size(rows.size.or(rows.font.and(text.size)).unwrap_or(-1.));
        lists.set_weight(rows.weight.map_or(0, |weight| weight as i32));
        lists.set_case(case(rows.case));
        lists.set_inset(rows.inset.unwrap_or(8.));
        lists.set_icon_size(rows.icon.size.unwrap_or(20.));
        lists.set_icon_gap(rows.icon.gap.unwrap_or(8.));
        lists.set_icon_colour(colour(rows.icon.colour));
        lists.set_icon_hover(colour(rows.icon.hover));
        lists.set_icon_selected(colour(rows.icon.selected));
        lists.set_icon_follows_ink(rows.icon.follow_ink);
        lists.set_frames(layers(folder, &rows.frame, !still));
        lists.set_fit_text(rows.fit_text);
        lists.set_tilt(if performance == Performance::Light { 0. } else { rows.tilt });
        match &rows.separator {
            Some(Separator::Line { colour, width, inset }) => {
                lists.set_sep_on(true);
                lists.set_sep_colour(Color::from_argb_encoded(*colour));
                lists.set_sep_width(*width);
                lists.set_sep_left(inset.0);
                lists.set_sep_right(inset.1);
            }
            Some(Separator::Art { art, height }) => {
                lists.set_sep_art(layer(folder, art, !still));
                lists.set_sep_height(*height);
            }
            None => {}
        }
        let selected = &rows.selected;
        lists.set_sel_frame(one(folder, selected.frame.as_ref(), !still));
        lists.set_prefix(selected.prefix.as_str().into());
        lists.set_prefix_colour(colour(selected.prefix_colour));
        lists.set_blink(time(selected.blink_ms));
        lists.set_after(one(folder, selected.after.as_ref(), !still));
        if let Some(underline) = selected.underline {
            lists.set_underline(true);
            lists.set_underline_colour(Color::from_argb_encoded(underline.colour));
            lists.set_underline_width(underline.width);
            lists.set_underline_offset(underline.offset);
        }
        // With art under the text, a hover lifts the row a little rather
        // than lighting a ground the art would cover.
        let framed = !rows.frame.is_empty();
        let hover = rows.hover.or(framed.then_some(HoverKind::Lift));
        lists.set_hover(hover_kind(hover));
        lists.set_hover_amount(rows.hover_amount.unwrap_or(match hover {
            Some(HoverKind::Lift) if rows.hover.is_none() => 2.,
            Some(kind) => kind.amount().0,
            None => 0.,
        }));
        lists.set_hover_time(time(rows.hover_ms));
        lists.set_hover_ground(match rows.hover_ground {
            Some(HoverGround::None) => 0,
            Some(HoverGround::Hover) => 1,
            Some(HoverGround::RowHover) => 2,
            None if framed => 0,
            None => 1,
        });
        lists.set_hover_frame_opacity(rows.hover_frame_opacity.unwrap_or(-1.));
        let meta = &rows.meta;
        lists.set_unread_mark(match meta.unread {
            None | Some(UnreadMark::Badge) => 0,
            Some(UnreadMark::Glyph) => 1,
            Some(UnreadMark::None) => 2,
        });
        lists.set_unread_glyph(meta.unread_glyph.is_some());
        lists.set_unread(glyph(folder, meta.unread_glyph.as_ref(), !still));
        lists.set_voice(meta.voice.as_str().into());
        let meta_face = face(resolved, fonts, meta.font.unwrap_or(FontRole::Body));
        lists.set_meta_family(if meta.font.is_some() { meta_face.family } else { SharedString::new() });
        lists.set_meta_size(meta.size.unwrap_or(12.));
        lists.set_meta_colour(colour(meta.colour));
        lists.set_meta_inset(meta.inset.unwrap_or(8.));
        lists.set_badge_ground(colour(meta.badge.ground));
        lists.set_badge_ink(colour(meta.badge.ink));
        lists.set_badge_radius(meta.badge.radius.unwrap_or(-1.));
        lists.set_ink_read(colour(rows.ink.read));
        lists.set_ink_unread(colour(rows.ink.unread));
        lists.set_ink_selected(colour(rows.ink.selected));
        lists.set_ink_muted(colour(rows.ink.muted));
        lists.set_ink_shadow(colour(rows.ink.selected_shadow));
        lists.set_ink_hover(colour(rows.ink.hover));
        lists.set_emboss(rows.emboss);
        lists.set_focus_outset(rows.focus_outset);
        lists.set_focus_radius(rows.focus_radius.unwrap_or(-1.));
        let headers = &rows.headers;
        let head = face(resolved, fonts, headers.font.unwrap_or(FontRole::Body));
        lists.set_head_on(*headers != Default::default());
        lists.set_head_family(if headers.font.is_some() { head.family } else { SharedString::new() });
        lists.set_head_size(headers.size.or(headers.font.and(head.size)).unwrap_or(-1.));
        lists.set_head_weight(headers.weight.map_or(head.weight.unwrap_or(0), |weight| weight as i32));
        lists.set_head_case(case(headers.case));
        lists.set_head_text(headers.text.as_str().into());
        lists.set_head_ransom(headers.style == HeaderStyle::Ransom && performance != Performance::Light);
        lists.set_head_height(headers.height.unwrap_or(-1.));
        lists.set_head_colour(colour(headers.colour));
        lists.set_head_frame(one(folder, headers.frame.as_ref(), !still));
    }
    if let Some(rows) = &layout.member_rows {
        let lists = window.global::<MemberLists>();
        lists.set_on(true);
        lists.set_height(rows.height.unwrap_or(-1.));
        lists.set_gap(rows.gap.unwrap_or(2.));
        let names = face(resolved, fonts, rows.font.unwrap_or(FontRole::Names));
        lists.set_family(names.family);
        lists.set_size(rows.size.or(names.size).unwrap_or(-1.));
        lists.set_weight(rows.weight.map_or(names.weight.unwrap_or(0), |weight| weight as i32));
        lists.set_case(case(rows.case));
        lists.set_avatar(rows.avatar.size.unwrap_or(-1.));
        lists.set_avatar_frames(layers(folder, &rows.avatar.frame, !still));
        lists.set_avatar_tilt(if performance == Performance::Light { 0. } else { rows.avatar.tilt });
        lists.set_dot(rows.avatar.dot.unwrap_or(16.));
        lists.set_dot_out(rows.avatar.dot_out.unwrap_or(4.));
        lists.set_avatar_gap(rows.avatar.gap.unwrap_or(10.));
        let (left, right) = rows.margin.unwrap_or((8., 8.));
        lists.set_margin_left(left);
        lists.set_margin_right(right);
        let (left, right) = rows.inset.unwrap_or((8., 8.));
        lists.set_inset_left(left);
        lists.set_inset_right(right);
        if let Some(ring) = &rows.avatar.ring {
            lists.set_ring_on(true);
            lists.set_ring_colour(Color::from_argb_encoded(ring.colour));
            lists.set_ring_width(ring.width);
            lists.set_ring_glow(ring.glow);
            let mut presences: Vec<SharedString> =
                ring.presences.iter().map(|presence| presence.as_str().into()).collect();
            presences.resize(4, SharedString::new());
            lists.set_ring_presences(ModelRc::new(VecModel::from(presences)));
        }
        lists.set_frames(layers(folder, &rows.frame, !still));
        if let Some(Separator::Line { colour, width, inset }) = &rows.separator {
            lists.set_sep_on(true);
            lists.set_sep_colour(Color::from_argb_encoded(*colour));
            lists.set_sep_width(*width);
            lists.set_sep_left(inset.0);
            lists.set_sep_right(inset.1);
        }
        lists.set_status(match rows.status {
            None | Some(StatusStyle::Dot) => 0,
            Some(StatusStyle::Glyph) => 1,
            Some(StatusStyle::Text) => 2,
            Some(StatusStyle::None) => 3,
        });
        lists.set_glyphs(ModelRc::new(VecModel::from(
            rows.status_glyphs
                .iter()
                .map(|known| glyph(folder, known.as_ref(), !still))
                .collect::<Vec<_>>(),
        )));
        let meta = &rows.meta;
        lists.set_meta_on(meta.is_set());
        lists.set_meta_presence(ModelRc::new(VecModel::from(
            [&meta.online, &meta.idle, &meta.dnd, &meta.offline, &meta.streaming]
                .iter()
                .map(|words| words.as_deref().unwrap_or("").into())
                .collect::<Vec<SharedString>>(),
        )));
        lists.set_meta_playing(meta.playing.as_deref().unwrap_or("").into());
        lists.set_meta_listening(meta.listening.as_deref().unwrap_or("").into());
        lists.set_meta_watching(meta.watching.as_deref().unwrap_or("").into());
        lists.set_meta_streaming(meta.streaming.as_deref().unwrap_or("").into());
        let meta_face = face(resolved, fonts, meta.font.unwrap_or(FontRole::Body));
        lists.set_meta_family(if meta.font.is_some() { meta_face.family } else { SharedString::new() });
        lists.set_meta_size(meta.size.unwrap_or(12.));
        lists.set_meta_colour(colour(meta.colour));
        lists.set_meta_max(meta.max_width);
        lists.set_sub(match rows.sub {
            None | Some(SubLine::Activity) => 0,
            Some(SubLine::Role) => 1,
            Some(SubLine::None) => 2,
        });
        lists.set_sub_text(rows.sub_text.as_str().into());
        if let Some(badge) = &rows.badge {
            lists.set_badge_on(true);
            lists.set_badge_text(badge.text.as_str().into());
            lists.set_badge_ground(Color::from_argb_encoded(badge.ground));
            lists.set_badge_ink(Color::from_argb_encoded(badge.ink));
            lists.set_badge_family(face(resolved, fonts, badge.font).family);
            lists.set_badge_online_only(badge.online_only);
        }
        lists.set_sel_frame(one(folder, rows.selected_frame.as_ref(), !still));
        lists.set_sel_over(one(folder, rows.selected_over.as_ref(), !still));
        lists.set_hover_frame_opacity(rows.hover_frame_opacity.unwrap_or(-1.));
        let headers = &rows.headers;
        let head = face(resolved, fonts, headers.font.unwrap_or(FontRole::Body));
        lists.set_head_on(*headers != Default::default());
        lists.set_head_family(if headers.font.is_some() { head.family } else { SharedString::new() });
        lists.set_head_size(headers.size.or(headers.font.and(head.size)).unwrap_or(-1.));
        lists.set_head_weight(headers.weight.map_or(head.weight.unwrap_or(0), |weight| weight as i32));
        lists.set_head_case(case(headers.case));
        lists.set_head_text(headers.text.as_str().into());
        lists.set_head_height(headers.height.unwrap_or(-1.));
        lists.set_head_colour(colour(headers.colour));
        lists.set_head_frame(one(folder, headers.frame.as_ref(), !still));
        let (columns, size) = rows.grid.unwrap_or((0, 48.));
        lists.set_grid_columns(columns as i32);
        lists.set_grid_size(size);
        lists.set_tile_outline(rows.tile_outline);
        let title = &rows.title;
        let title_face = face(resolved, fonts, title.font.unwrap_or(FontRole::Body));
        lists.set_title_family(if title.font.is_some() { title_face.family } else { SharedString::new() });
        lists.set_title_size(title.size.or(title.font.and(title_face.size)).unwrap_or(-1.));
        lists.set_title_colour(colour(title.colour));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model as _;
    use yapfast_core::themes::package::{Mode, Package};

    /// Decoding happens here, once per picture per theme: three frames, two
    /// tapes and a sticker are six decodes whether one row shows or a
    /// thousand, and painting the theme again decodes nothing.
    #[test]
    fn each_picture_is_decoded_once_per_theme() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        init(&window);
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir(folder.path().join("art")).unwrap();
        for (index, name) in ["a", "b", "c", "tape", "star", "pill"].iter().enumerate() {
            let picture = image::RgbaImage::from_pixel(24, 24, image::Rgba([index as u8 * 40, 0, 0, 255]));
            picture.save(folder.path().join(format!("art/{name}.png"))).unwrap();
        }
        let package = Package::parse(
            r##"{ "theme_api": 1, "id": "dev.example.lists", "name": "Lists", "version": "1.0.0",
                "layout": {
                    "rail_tiles": { "shape": "card", "tilt": 6,
                        "frame": [{ "image": "art/a.png", "window": [4, 4, 36, 36] }, { "image": "art/b.png" }, { "image": "art/c.png" }],
                        "over": { "image": "art/tape.png", "size": [30, 10], "position": "top" },
                        "selected": { "over": { "image": "art/star.png", "size": [14, 14] } } },
                    "channel_rows": { "frame": [{ "image": "art/a.png", "slice": 4 }, { "image": "art/b.png", "slice": 4 }],
                        "selected": { "frame": { "image": "art/pill.png", "slice": 4 } } },
                    "member_rows": { "frame": { "image": "art/c.png", "slice": 4 } } },
                "variants": { "dark": { "base": "yap" } } }"##,
        )
        .unwrap();
        let resolved = package.resolve(Mode::Dark).unwrap();
        let before = decodes();
        apply(&window, &resolved, folder.path(), &[], Performance::Full, false);
        // a, b, c, the tape, the star and the pill: the channels' and
        // members' a, b and c are the rail's.
        let first = decodes() - before;
        assert_eq!(first, 6);
        let rails = window.global::<Rails>();
        assert!(rails.get_on());
        assert_eq!(rails.get_frames().row_count(), 3);
        // A thousand rows pick among the same images: Slint holds handles
        // to them, and nothing decodes as rows come and go.
        let frames: Vec<TileLayer> = rails.get_frames().iter().collect();
        for seed in 0..1000 {
            let picked = &frames[seed % 3];
            assert!(picked.on);
        }
        // Painting again (Performance changing, a dialog closing) decodes
        // nothing more.
        reset(&window);
        apply(&window, &resolved, folder.path(), &[], Performance::Light, true);
        assert_eq!(decodes() - before, first);
        assert_eq!(window.global::<Rails>().get_hover_time(), 0, "reduced motion: no tween");
        assert_eq!(window.global::<Rails>().get_tilt(), 0., "Light leaves the tiles straight");
        // Cleared, it's stock again.
        reset(&window);
        assert!(!window.global::<Rails>().get_on());
        assert!(!window.global::<ChannelLists>().get_on());
        assert!(!window.global::<MemberLists>().get_on());
    }

    /// A theme without the three blocks leaves every list stock.
    #[test]
    fn a_theme_without_lists_leaves_them_stock() {
        i_slint_backend_testing::init_no_event_loop();
        let window = YapfastWindow::new().unwrap();
        let package = Package::parse(
            r##"{ "theme_api": 1, "id": "dev.example.plain", "name": "Plain", "version": "1.0.0",
                "variants": { "dark": { "base": "yap" } } }"##,
        )
        .unwrap();
        let resolved = package.resolve(Mode::Dark).unwrap();
        let folder = tempfile::tempdir().unwrap();
        apply(&window, &resolved, folder.path(), &[], Performance::Full, false);
        assert!(!window.global::<Rails>().get_on());
        assert!(!window.global::<ChannelLists>().get_on());
        assert!(!window.global::<MemberLists>().get_on());
        assert_eq!(window.global::<Chrome>().get_rail_tiles_px(), 0.);
    }

    #[test]
    fn fills_templates_and_drops_empty_parts() {
        let values = [("role", "Crew"), ("activity", "")];
        assert_eq!(fill("{role} · {activity}", &values), "Crew");
        let values = [("role", "Crew"), ("activity", "Lumen Drift")];
        assert_eq!(fill("{role} · {activity}", &values), "Crew · Lumen Drift");
        assert_eq!(fill("WORLD {n} · {name}", &[("n", "2"), ("name", "OFF HOURS")]), "WORLD 2 · OFF HOURS");
        assert_eq!(fill("voice · {n}", &[("n", "0")]), "voice · 0");
        assert_eq!(fill("P{online}", &[("online", "5")]), "P5");
        assert_eq!(fill("// {name}", &[("name", "studio")]), "// studio");
    }
}
