//! Discord's Filters dialog for a search, opened by the popout's More
//! filters or Add search filters, or the results' Filters button. It reads
//! the box's filters into fields: From, In (in a server), Has, Mentions,
//! dates (up to four of Before, After or During a day), Author Type and
//! Pinned, each but Pinned taking several. Apply Filters, lit once
//! something changed, writes them back as Discord's client does (from,
//! mentions, in, has, dates, author type, pinned, then the words) and
//! searches, or with nothing left closes the results; Clear Filters empties
//! the fields. A filter that would take the search past 512 characters
//! isn't added: "Unable to add filter".

use chrono::{Datelike, NaiveDate, TimeZone};
use iced::{
    Alignment, Element, Length, Padding, Task,
    widget::{Space, button, column, container, operation, row, stack, text},
};
use yapfast_core::{
    application::AppState,
    discord,
    yapfast_discord::search::{self, Filter},
};

use crate::{
    app::{App, Message},
    controls::{Kind, action},
    ctx::Ctx,
    dialog::{CONFIRM_WIDTH, dialog},
    fonts::{MEDIUM, SEMIBOLD},
    pictures::Icon,
    search::Event,
    search_popout::Face,
};

const WIDTH: f32 = 560.0;
/// The most date rows, and options a field lists under it.
const DATES: usize = 4;
const OPTIONS: usize = 8;
/// Each date row's trash button, as Discord labels it.
const REMOVE_DATE: [&str; DATES] = [
    "Remove date filter 1",
    "Remove date filter 2",
    "Remove date filter 3",
    "Remove date filter 4",
];
/// The open field's search.
const QUERY: &str = "search-filters-query";
/// The kinds of date row, as Discord's dropdown names them.
const DATE_KINDS: [(Filter, &str); 3] = [
    (Filter::Before, "Before"),
    (Filter::After, "After"),
    (Filter::During, "During"),
];

/// A value chosen in one of the fields: as the box writes it after the key,
/// as the field shows it, and its avatar or icon.
#[derive(Clone, Debug, PartialEq)]
pub struct Chosen {
    value: String,
    label: String,
    face: Option<Face>,
}

/// Which field's list is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    From,
    In,
    Has,
    Mentions,
    AuthorType,
    Pinned,
    /// A date row's kind, or its day.
    Kind(usize),
    Day(usize),
}

/// The dialog's fields, the box's words, and what's open in it.
#[derive(Clone)]
pub struct Filters {
    from: Vec<Chosen>,
    mentions: Vec<Chosen>,
    channels: Vec<Chosen>,
    has: Vec<String>,
    authors: Vec<String>,
    pinned: Option<bool>,
    dates: Vec<(Filter, NaiveDate)>,
    words: String,
    /// The filters as the dialog opened on them, for Apply.
    initial: String,
    open: Option<Field>,
    query: String,
    month: (i32, u32),
    too_long: bool,
}

#[derive(Clone, Debug)]
pub enum FiltersEvent {
    /// The dialog opens on what the box holds.
    Open,
    Close,
    /// A field's list opens, or closes again.
    Toggle(Field),
    Query(String),
    /// One of the open field's options taken, or a chosen one let go.
    Add(usize),
    Remove(Field, usize),
    Pinned(usize),
    DateKind(usize, usize),
    Day(usize, NaiveDate),
    Month(i32),
    AddDate,
    RemoveDate(usize),
    Clear,
    Apply,
    /// "Back to editing", after a filter too long for the search.
    Back,
}

fn message(event: FiltersEvent) -> Message {
    Message::Search(Event::Filters(event))
}

impl Filters {
    /// The fields `text` fills: each filter's answers, the dates as days,
    /// the rest as words. A key without an answer goes, as Discord's
    /// dialog leaves it out.
    fn of(text: &str, state: &AppState) -> Self {
        let query = crate::search::parse(text, state);
        let today = chrono::Local::now().date_naive();
        let mut filters = Self {
            from: Vec::new(),
            mentions: Vec::new(),
            channels: Vec::new(),
            has: Vec::new(),
            authors: Vec::new(),
            pinned: None,
            dates: Vec::new(),
            words: String::new(),
            initial: String::new(),
            open: None,
            query: String::new(),
            month: (today.year(), today.month()),
            too_long: false,
        };
        let mut words = Vec::new();
        for token in &query.tokens {
            let typed = text[token.start..token.end].trim();
            match &token.kind {
                search::Kind::Text => words.extend(typed.split_whitespace()),
                search::Kind::Key(_) => {}
                search::Kind::Answer(filter, answer) => match filter {
                    Filter::From => filters.from.push(person(state, typed)),
                    Filter::Mentions => filters.mentions.push(person(state, typed)),
                    Filter::In => filters.channels.push(channel(state, typed)),
                    Filter::Has => filters.has.push(typed.to_owned()),
                    Filter::AuthorType => filters.authors.push(typed.to_owned()),
                    Filter::Pinned => filters.pinned = Some(typed.eq_ignore_ascii_case("true")),
                    Filter::Before | Filter::After | Filter::During => {
                        if let Some(day) = day_of(*filter, typed, answer) {
                            filters.dates.push((*filter, day));
                        }
                    }
                    // Typed only, which the dialog doesn't show: kept
                    // with the words.
                    Filter::LinkFrom | Filter::FileType | Filter::FileName => {
                        words.push(&text[token.start - filter.key().len()..token.end]);
                    }
                },
            }
        }
        filters.words = words.join(" ");
        filters.initial = filters.filters_text(true);
        filters
    }

    /// The filters as the box writes them, in Discord's order.
    fn filters_text(&self, server: bool) -> String {
        fn chosen(values: &[Chosen]) -> impl Iterator<Item = &str> {
            values.iter().map(|chosen| chosen.value.as_str())
        }
        fn each<'v>(
            parts: &mut Vec<String>,
            filter: Filter,
            values: impl IntoIterator<Item = &'v str>,
        ) {
            for value in values {
                parts.push(format!("{} {value}", filter.key()));
            }
        }
        let mut parts = Vec::new();
        each(&mut parts, Filter::From, chosen(&self.from));
        each(&mut parts, Filter::Mentions, chosen(&self.mentions));
        if server {
            each(&mut parts, Filter::In, chosen(&self.channels));
        }
        each(&mut parts, Filter::Has, self.has.iter().map(String::as_str));
        for (filter, day) in &self.dates {
            parts.push(format!("{} {}", filter.key(), day.format("%Y-%m-%d")));
        }
        each(
            &mut parts,
            Filter::AuthorType,
            self.authors.iter().map(String::as_str),
        );
        if let Some(pinned) = self.pinned {
            parts.push(format!("pinned: {pinned}"));
        }
        parts.join(" ")
    }

    /// The search Apply writes: the filters, then the words.
    fn search_text(&self, server: bool) -> String {
        format!("{} {}", self.filters_text(server), self.words)
            .trim()
            .to_owned()
    }

    /// How many filters are set, as Clear Filters counts them.
    fn count(&self, server: bool) -> usize {
        self.from.len()
            + self.mentions.len()
            + if server { self.channels.len() } else { 0 }
            + self.has.len()
            + self.dates.len()
            + self.authors.len()
            + usize::from(self.pinned.is_some())
    }

    /// What the open field offers for what's typed in it: those not
    /// chosen yet.
    fn options(&self, state: &AppState, server: Option<&str>) -> Vec<Chosen> {
        let typed = self.query.trim();
        let listed = |names: &[&str], chosen: &[String]| {
            names
                .iter()
                .filter(|name| {
                    name.starts_with(&typed.to_lowercase())
                        && !chosen.iter().any(|old| old == *name)
                })
                .map(|name| Chosen {
                    value: (*name).to_owned(),
                    label: capitalized(name),
                    face: None,
                })
                .take(OPTIONS)
                .collect()
        };
        let not_chosen = |found: Vec<(String, Face)>, chosen: &[Chosen]| {
            found
                .into_iter()
                .filter(|(value, _)| !chosen.iter().any(|old| old.value == *value))
                .map(|(value, face)| Chosen {
                    label: face_label(&face, &value),
                    value,
                    face: Some(face),
                })
                .take(OPTIONS)
                .collect()
        };
        match self.open {
            Some(Field::From) => not_chosen(
                crate::search_popout::people(state, typed, OPTIONS * 2),
                &self.from,
            ),
            Some(Field::Mentions) => not_chosen(
                crate::search_popout::people(state, typed, OPTIONS * 2),
                &self.mentions,
            ),
            Some(Field::In) => server.map_or_else(Vec::new, |server| {
                not_chosen(
                    crate::search_popout::channels(state, server, typed, OPTIONS * 2),
                    &self.channels,
                )
            }),
            Some(Field::Has) => listed(&search::HAS.map(|(name, _)| name), &self.has),
            Some(Field::AuthorType) => {
                listed(&search::AUTHOR_TYPES.map(|(name, _)| name), &self.authors)
            }
            _ => Vec::new(),
        }
    }
}

/// `from:` or `mentions:` as typed: someone the place knows, by
/// username, or as written.
fn person(state: &AppState, typed: &str) -> Chosen {
    let known = crate::search_popout::people(state, typed, 10)
        .into_iter()
        .find(|(value, _)| value.eq_ignore_ascii_case(typed));
    match known {
        Some((value, face)) => Chosen {
            label: face_label(&face, &value),
            value,
            face: Some(face),
        },
        None => Chosen {
            value: typed.to_owned(),
            label: typed.to_owned(),
            face: None,
        },
    }
}

/// `in:` as typed, by the channel's name.
fn channel(state: &AppState, typed: &str) -> Chosen {
    let name = typed
        .trim_start_matches('#')
        .trim_matches('"')
        .replace("\\\"", "\"")
        .replace("\\\\", "\\");
    let icon = state
        .rooms
        .iter()
        .find(|room| room.name == name && !room.direct)
        .map(|room| crate::sidebar::room_icon(room));
    Chosen {
        value: typed.to_owned(),
        label: name.clone(),
        face: icon.map(|icon| Face::Channel { icon, name }),
    }
}

fn face_label(face: &Face, value: &str) -> String {
    match face {
        Face::Person { name, .. } | Face::Channel { name, .. } => name.clone(),
        Face::Word => value.to_owned(),
    }
}

fn capitalized(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// A date answer as a day: as typed when it's one, else the day its
/// snowflakes start (the day before an `after:`'s).
fn day_of(filter: Filter, typed: &str, answer: &search::Answer) -> Option<NaiveDate> {
    if let Ok(day) = NaiveDate::parse_from_str(typed, "%Y-%m-%d") {
        return Some(day);
    }
    let search::Answer::Dates { min, max } = answer else {
        return None;
    };
    let day = |snowflake: u64| {
        let ms = (snowflake >> 22) + 1_420_070_400_000;
        chrono::Local
            .timestamp_millis_opt(ms as i64)
            .single()
            .map(|at| at.date_naive())
    };
    match filter {
        Filter::Before => max.and_then(day),
        Filter::After => min.and_then(day).and_then(|day| day.pred_opt()),
        _ => min.and_then(day),
    }
}

/// Whether the box's search is in a server, where `in:` is a filter.
fn server_of(state: &AppState) -> Option<String> {
    state
        .search_place()
        .and_then(|place| discord::parse_server(&place).map(str::to_owned))
}

pub fn update(app: &mut App, event: FiltersEvent) -> Task<Message> {
    let state = app.client.state();
    let server = server_of(state);
    if let FiltersEvent::Open = event {
        app.searching.filters = Some(Filters::of(&app.searching.text, state));
        return operation::focus(crate::app::NOBODY);
    }
    let Some(filters) = app.searching.filters.as_mut() else {
        return Task::none();
    };
    let before = filters.clone();
    match event {
        FiltersEvent::Open => {}
        FiltersEvent::Close => app.searching.filters = None,
        FiltersEvent::Toggle(field) => {
            filters.open = (filters.open != Some(field)).then_some(field);
            filters.query.clear();
            if let Some(Field::Day(row)) = filters.open
                && let Some((_, day)) = filters.dates.get(row)
            {
                filters.month = (day.year(), day.month());
            }
            if matches!(
                filters.open,
                Some(Field::From | Field::In | Field::Has | Field::Mentions | Field::AuthorType)
            ) {
                return operation::focus(QUERY);
            }
        }
        FiltersEvent::Query(query) => filters.query = query,
        FiltersEvent::Add(index) => {
            let options = filters.options(state, server.as_deref());
            if let (Some(chosen), Some(field)) = (options.into_iter().nth(index), filters.open) {
                match field {
                    Field::From => filters.from.push(chosen),
                    Field::Mentions => filters.mentions.push(chosen),
                    Field::In => filters.channels.push(chosen),
                    Field::Has => filters.has.push(chosen.value),
                    Field::AuthorType => filters.authors.push(chosen.value),
                    _ => {}
                }
                filters.query.clear();
            }
        }
        FiltersEvent::Remove(field, index) => {
            fn take<T>(values: &mut Vec<T>, index: usize) {
                if index < values.len() {
                    values.remove(index);
                }
            }
            match field {
                Field::From => take(&mut filters.from, index),
                Field::Mentions => take(&mut filters.mentions, index),
                Field::In => take(&mut filters.channels, index),
                Field::Has => take(&mut filters.has, index),
                Field::AuthorType => take(&mut filters.authors, index),
                _ => {}
            }
        }
        FiltersEvent::Pinned(index) => {
            filters.pinned = Some(index == 0);
            filters.open = None;
        }
        FiltersEvent::DateKind(row, kind) => {
            if let (Some(date), Some((filter, _))) =
                (filters.dates.get_mut(row), DATE_KINDS.get(kind))
            {
                date.0 = *filter;
            }
            filters.open = None;
        }
        FiltersEvent::Day(row, day) => {
            if let Some(date) = filters.dates.get_mut(row) {
                date.1 = day;
            }
            filters.open = None;
        }
        FiltersEvent::Month(by) => {
            let (year, month) = filters.month;
            let months = year * 12 + month as i32 - 1 + by;
            filters.month = (months.div_euclid(12), months.rem_euclid(12) as u32 + 1);
        }
        FiltersEvent::AddDate => {
            if filters.dates.len() < DATES {
                filters
                    .dates
                    .push((Filter::Before, chrono::Local::now().date_naive()));
            }
        }
        FiltersEvent::RemoveDate(row) => {
            if row < filters.dates.len() {
                filters.dates.remove(row);
            }
            filters.open = None;
        }
        FiltersEvent::Clear => {
            let words = std::mem::take(&mut filters.words);
            let initial = std::mem::take(&mut filters.initial);
            *filters = Filters::of("", state);
            filters.words = words;
            filters.initial = initial;
        }
        FiltersEvent::Back => filters.too_long = false,
        FiltersEvent::Apply => {
            let text = filters.search_text(server.is_some());
            app.searching.filters = None;
            let caret = text.len();
            app.searching.put(text.clone(), caret);
            let mut cx = crate::ctx::cx!(app);
            // Nothing left closes the results, as Discord's dialog does.
            let event = if text.is_empty() {
                Event::Clear
            } else {
                Event::Search
            };
            return app.searching.update(event, &mut cx);
        }
    }
    // A filter that takes the search past Discord's longest goes again.
    if let Some(filters) = app.searching.filters.as_mut() {
        let longer = filters.search_text(server.is_some());
        if longer.chars().count() > search::MAX_LENGTH
            && longer.len() > before.search_text(server.is_some()).len()
        {
            *filters = Filters {
                too_long: true,
                open: filters.open,
                query: String::new(),
                ..before
            };
        }
    }
    Task::none()
}

/// Whether the dialog is open.
pub fn is_open(app: &App) -> bool {
    app.searching.filters.is_some()
}

/// The avatars its people show.
pub fn pictures(app: &App) -> Vec<String> {
    let Some(filters) = &app.searching.filters else {
        return Vec::new();
    };
    let state = app.client.state();
    let server = server_of(state);
    filters
        .from
        .iter()
        .chain(&filters.mentions)
        .cloned()
        .chain(filters.options(state, server.as_deref()))
        .filter_map(|chosen| match chosen.face {
            Some(Face::Person { avatar, .. }) => avatar,
            _ => None,
        })
        .collect()
}

pub fn view<'a>(ctx: Ctx<'a>) -> Option<Element<'a, Message>> {
    let filters = ctx.searching.filters.as_ref()?;
    let theme = ctx.look.theme;
    let server = server_of(ctx.state);
    let in_server = server.is_some();
    let own = ctx.account.username.clone();
    let here = ctx
        .state
        .selected_room()
        .filter(|room| !room.direct)
        .map(|room| room.name.clone())
        .unwrap_or_else(|| "general".to_owned());
    let mut fields = column![].spacing(20);
    let multi = |field: Field,
                 label: &'static str,
                 sublabel: &'static str,
                 placeholder: String,
                 chosen: Vec<Chosen>| {
        labelled(
            ctx,
            label,
            sublabel,
            chooser(ctx, filters, field, placeholder, chosen, server.as_deref()),
        )
    };
    let words = |values: &[String]| {
        values
            .iter()
            .map(|value| Chosen {
                value: value.clone(),
                label: capitalized(value),
                face: None,
            })
            .collect::<Vec<_>>()
    };
    fields = fields.push(multi(
        Field::From,
        "From",
        "Sent by any of the selected users",
        format!("ex. {own}"),
        filters.from.clone(),
    ));
    if in_server {
        fields = fields.push(multi(
            Field::In,
            "In",
            "Sent in any of the selected channels",
            format!("ex. {here}"),
            filters.channels.clone(),
        ));
    }
    fields = fields.push(multi(
        Field::Has,
        "Has",
        "Includes any of the selected types of data",
        "Any content".to_owned(),
        words(&filters.has),
    ));
    fields = fields.push(multi(
        Field::Mentions,
        "Mentions",
        "Mentions any of the selected users",
        format!("ex. {own}"),
        filters.mentions.clone(),
    ));
    fields = fields.push(labelled(
        ctx,
        "Date",
        "When the message was sent",
        dates(ctx, filters),
    ));
    fields = fields.push(multi(
        Field::AuthorType,
        "Author Type",
        "Sent by any of the selected types of author",
        "ex. Bot".to_owned(),
        words(&filters.authors),
    ));
    let pinned = crate::form::dropdown(
        vec!["True".to_owned(), "False".to_owned()],
        filters.pinned.map(|pinned| usize::from(!pinned)),
        filters.open == Some(Field::Pinned),
        message(FiltersEvent::Toggle(Field::Pinned)),
        |index| message(FiltersEvent::Pinned(index)),
    )
    .view(ctx.look);
    let pinned: Element<'a, Message> = if filters.pinned.is_none() {
        // Unset: Discord's placeholder in the box.
        stack![
            pinned,
            container(text("ex. True").size(15).color(theme.muted))
                .padding(Padding::ZERO.left(12))
                .height(crate::form::DROPDOWN)
                .align_y(Alignment::Center)
        ]
        .into()
    } else {
        pinned
    };
    fields = fields.push(labelled(
        ctx,
        "Pinned",
        "If the message is pinned or not",
        pinned,
    ));
    let count = filters.count(in_server);
    let clear_label = if count == 0 {
        "Clear Filters".to_owned()
    } else {
        format!("Clear Filters ({count})")
    };
    let changed = filters.filters_text(in_server) != filters.initial;
    let foot = row![
        action(
            theme,
            clear_label,
            Kind::Ghost,
            (count > 0).then(|| message(FiltersEvent::Clear))
        ),
        Space::new().width(Length::Fill),
        action(
            theme,
            "Cancel",
            Kind::Plain,
            Some(message(FiltersEvent::Close))
        ),
        action(
            theme,
            "Apply Filters",
            Kind::Primary,
            changed.then(|| message(FiltersEvent::Apply))
        ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    let head = row![
        text("Filters")
            .size(20)
            .font(SEMIBOLD)
            .color(theme.text)
            .width(Length::Fill),
        ctx.look
            .icon_button(Icon::X, "Close", message(FiltersEvent::Close))
            .size(32.0, 18),
    ]
    .align_y(Alignment::Center);
    let height = (ctx.look.window.height - 200.0).max(240.0);
    let body = column![
        head,
        container(crate::widgets::scroll_view(
            container(fields).padding(Padding::ZERO.right(12))
        ))
        .max_height(height),
        foot,
    ]
    .spacing(20)
    .padding(24);
    let card = dialog(
        theme,
        body.into(),
        WIDTH.min(ctx.look.window.width - 32.0),
        message(FiltersEvent::Close),
    );
    if !filters.too_long {
        return Some(card);
    }
    let note = column![
        text("Unable to add filter")
            .size(20)
            .font(SEMIBOLD)
            .color(theme.text),
        text("You have reached the maximum length for a search query.")
            .size(15)
            .color(theme.secondary),
        crate::dialog::buttons([action(
            theme,
            "Back to editing",
            Kind::Primary,
            Some(message(FiltersEvent::Back))
        )
        .into()]),
    ]
    .spacing(12)
    .padding(24);
    Some(
        stack![
            card,
            dialog(
                theme,
                note.into(),
                CONFIRM_WIDTH,
                message(FiltersEvent::Back)
            ),
        ]
        .into(),
    )
}

/// A field's label and what it's for, over its control.
fn labelled<'a>(
    ctx: Ctx<'a>,
    label: &'static str,
    sublabel: &'static str,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    column![
        text(label).size(16).font(SEMIBOLD).color(theme.text),
        text(sublabel).size(13).color(theme.secondary),
        Space::new().height(4),
        control,
    ]
    .spacing(2)
    .into()
}

/// A field that takes several: what's chosen as chips in a box, which a
/// click opens on a search and the options for it.
fn chooser<'a>(
    ctx: Ctx<'a>,
    filters: &'a Filters,
    field: Field,
    placeholder: String,
    chosen: Vec<Chosen>,
    server: Option<&str>,
) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let open = filters.open == Some(field);
    let mut chips = row![].spacing(6).align_y(Alignment::Center);
    for (index, value) in chosen.iter().enumerate() {
        chips = chips.push(chip(
            ctx,
            value,
            message(FiltersEvent::Remove(field, index)),
        ));
    }
    if chosen.is_empty() && !open {
        chips = chips.push(text(placeholder.clone()).size(15).color(theme.muted));
    }
    let chips = chips.wrap();
    let (sunken, line, accent) = (theme.sunken, theme.line, theme.accent);
    let boxed = button(
        row![
            container(chips).width(Length::Fill),
            ctx.look.glyph(
                if open {
                    Icon::ChevronUp
                } else {
                    Icon::ChevronDown
                },
                16,
                theme.secondary
            ),
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding::from([8, 12]))
    .width(Length::Fill)
    .on_press(message(FiltersEvent::Toggle(field)))
    .style(move |_, _| button::Style {
        background: Some(sunken.into()),
        border: crate::style::edge(if open { accent } else { line }, 8.0),
        ..Default::default()
    });
    if !open {
        return boxed.into();
    }
    let search = crate::controls::field(
        theme,
        iced::widget::Id::new(QUERY),
        &placeholder,
        &filters.query,
        false,
        true,
    )
    .on_input(|query| message(FiltersEvent::Query(query)))
    .on_submit(message(FiltersEvent::Add(0)))
    .size(15);
    let mut list = column![search].spacing(2);
    let options = filters.options(ctx.state, server);
    if options.is_empty() {
        list = list.push(
            container(text("No results").size(14).color(theme.muted))
                .padding(Padding::from([8, 12])),
        );
    }
    let (hover, raised, edge) = (theme.hover, theme.raised, theme.edge);
    for (index, option) in options.iter().enumerate() {
        let face: Element<'a, Message> = match &option.face {
            Some(face @ (Face::Person { .. } | Face::Channel { .. })) => option_face(ctx, face),
            _ => text(option.label.clone())
                .size(15)
                .font(MEDIUM)
                .color(theme.text)
                .into(),
        };
        list = list.push(
            button(container(face).height(32).align_y(Alignment::Center))
                .padding(Padding::from([0, 8]))
                .width(Length::Fill)
                .on_press(message(FiltersEvent::Add(index)))
                .style(crate::controls::lit_style(None, hover, 4.0)),
        );
    }
    column![
        boxed,
        container(list)
            .padding(6)
            .width(Length::Fill)
            .style(move |_| crate::style::panel(raised, edge, 8.0)),
    ]
    .spacing(4)
    .into()
}

/// A chosen value's chip: its avatar or icon, its name, and an x that
/// lets it go.
fn chip<'a>(ctx: Ctx<'a>, chosen: &Chosen, remove: Message) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut line = row![].spacing(4).align_y(Alignment::Center);
    match &chosen.face {
        Some(Face::Person { name, avatar, .. }) => {
            line = line.push(ctx.avatar(avatar.as_deref(), name, 16.0, 8.0, 1.0));
        }
        Some(Face::Channel { icon, .. }) => {
            line = line.push(ctx.look.glyph(*icon, 14, theme.secondary));
        }
        _ => {}
    }
    line = line.push(
        text(chosen.label.clone())
            .size(14)
            .font(MEDIUM)
            .color(theme.text)
            .wrapping(text::Wrapping::None),
    );
    line = line.push(
        button(ctx.look.glyph(Icon::X, 12, theme.secondary))
            .padding(2)
            .on_press(remove)
            .style(crate::controls::lit_style(None, theme.hover, 4.0)),
    );
    let ground = theme.active;
    container(line)
        .padding(Padding::from([2, 6]))
        .style(move |_| crate::style::rounded(ground, 4))
        .into()
}

fn option_face<'a>(ctx: Ctx<'a>, face: &Face) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    match face {
        Face::Person {
            name,
            username,
            avatar,
        } => row![
            ctx.avatar(avatar.as_deref(), name, 20.0, 10.0, 1.0),
            text(name.clone()).size(15).font(MEDIUM).color(theme.text),
            text(username.clone()).size(13).color(theme.muted),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        Face::Channel { icon, name } => row![
            ctx.look.glyph(*icon, 16, theme.secondary),
            text(name.clone()).size(15).font(MEDIUM).color(theme.text),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        Face::Word => Space::new().into(),
    }
}

/// The date rows: Before, After or During, a day from a calendar, and a
/// trash button each; Add date under them, four at most.
fn dates<'a>(ctx: Ctx<'a>, filters: &'a Filters) -> Element<'a, Message> {
    let theme = ctx.look.theme;
    let mut rows = column![].spacing(8);
    for (index, (filter, day)) in filters.dates.iter().enumerate() {
        let kind = DATE_KINDS
            .iter()
            .position(|(kind, _)| kind == filter)
            .unwrap_or(0);
        let kinds = crate::form::dropdown(
            DATE_KINDS
                .iter()
                .map(|(_, name)| (*name).to_owned())
                .collect(),
            Some(kind),
            filters.open == Some(Field::Kind(index)),
            message(FiltersEvent::Toggle(Field::Kind(index))),
            move |kind| message(FiltersEvent::DateKind(index, kind)),
        )
        .width(140)
        .view(ctx.look);
        let open = filters.open == Some(Field::Day(index));
        let (sunken, line, accent) = (theme.sunken, theme.line, theme.accent);
        let day_box = button(
            row![
                text(day.format("%Y-%m-%d").to_string())
                    .size(15)
                    .color(theme.text)
                    .width(Length::Fill),
                ctx.look.glyph(Icon::Calendar, 16, theme.secondary),
            ]
            .align_y(Alignment::Center),
        )
        .padding(Padding::from([8, 12]))
        .width(Length::Fill)
        .on_press(message(FiltersEvent::Toggle(Field::Day(index))))
        .style(move |_, _| button::Style {
            background: Some(sunken.into()),
            border: crate::style::edge(if open { accent } else { line }, 8.0),
            ..Default::default()
        });
        let label = REMOVE_DATE[index.min(DATES - 1)];
        let trash = ctx
            .look
            .icon_button(Icon::Trash, label, message(FiltersEvent::RemoveDate(index)))
            .size(32.0, 16);
        rows = rows.push(
            row![kinds, day_box, trash]
                .spacing(8)
                .align_y(Alignment::Center),
        );
        if open {
            let calendar = crate::search_popout::calendar(
                ctx,
                filters.month,
                move |day| message(FiltersEvent::Day(index, day)),
                |by| message(FiltersEvent::Month(by)),
            );
            let (raised, edge) = (theme.raised, theme.edge);
            rows = rows.push(
                container(calendar)
                    .padding(8)
                    .width(Length::Fill)
                    .style(move |_| crate::style::panel(raised, edge, 8.0)),
            );
        }
    }
    if filters.dates.len() < DATES {
        rows = rows.push(action(
            theme,
            "Add date",
            Kind::Plain,
            Some(message(FiltersEvent::AddDate)),
        ));
    }
    rows.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_reads_the_box_and_writes_it_back_in_discords_order() {
        let state = AppState::default();
        let text = "plan pinned: true has: link from: bob after: 2026-11-02 notes fileType: pdf has: -file";
        let filters = Filters::of(text, &state);
        assert_eq!(filters.has, ["link", "-file"]);
        assert_eq!(filters.pinned, Some(true));
        assert_eq!(filters.dates.len(), 1);
        assert_eq!(filters.words, "plan notes fileType: pdf");
        assert_eq!(filters.count(false), 5);
        assert_eq!(
            filters.search_text(false),
            "from: bob has: link has: -file after: 2026-11-02 pinned: true plan notes fileType: pdf"
        );
        // Unchanged, Apply stays off.
        assert_eq!(filters.filters_text(true), filters.initial);
    }
}
