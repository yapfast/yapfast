//! What people wear: the member list's lines with their group headings,
//! display name styles' fonts and colours, and the animated pictures of
//! decorations and nameplates; the chips a name wears beside it (server
//! tag, APP) are at the end.

use std::sync::Arc;

use crate::{
    app::Message,
    fonts::BOLD,
    style::{BLURPLE, faded, hex, rounded},
    widgets::Pic,
};
use iced::{
    Alignment, Color, Element, Font,
    widget::{Space, container, row, text},
};
use yapfast_core::{
    application::AppState,
    chat::{ChatMember, NameStyle, Presence, Role},
};

/// How long a member list stays as built while its people change: the first
/// change shows at once, those within the pause after it together at its
/// end.
pub const ROSTER_PAUSE: std::time::Duration = std::time::Duration::from_millis(500);

/// One line of the member list: a group's heading or a member. Headings are
/// lines of their own.
#[derive(Clone, Debug, PartialEq)]
pub enum MemberLine {
    /// `Online — 12`, and the address of the group's role icon.
    Heading {
        text: String,
        icon: Option<Arc<str>>,
    },
    /// Shared, so a member who didn't change keeps the look made before.
    Member(Arc<MemberLook>),
}

/// A member's row in the member list.
#[derive(Clone, Debug, PartialEq)]
pub struct MemberLook {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    /// The still of the decoration around the avatar; [`animated_url`] gives
    /// what plays while the row is pointed at.
    pub decoration: Option<Arc<str>>,
    /// The nameplate's still and its palette's colour on the theme shown,
    /// dark or light.
    pub nameplate: Option<(Arc<str>, Option<u32>)>,
    /// The name's font, in a server, or its whole style elsewhere.
    pub style: Option<NameLook>,
    /// The name's colour, from their highest coloured role.
    pub color: Option<u32>,
    pub role_icon: Option<Arc<str>>,
    /// The server tag's letters and its badge's address.
    pub tag: Option<(Arc<str>, Option<Arc<str>>)>,
    pub bot: bool,
    pub owner: bool,
    /// Timed out in the server, marked with a clock; boosting it, a gem
    /// whose tooltip says since when.
    pub timed_out: bool,
    pub boosting: Option<String>,
    pub presence: Option<Presence>,
    /// The line under the name.
    pub status: Status,
}

/// What someone is up to, on one line under their name: its words, the
/// address of a custom emoji's picture before them, the activity's icon
/// (0 none, 1 playing, 2 listening, 3 watching, 4 streaming, 5 competing)
/// and whether a bullet follows the icon.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub words: String,
    pub emoji: Option<String>,
    pub icon: u8,
    pub bullet: bool,
}

impl<'a> From<&'a Status> for std::borrow::Cow<'a, Status> {
    fn from(status: &'a Status) -> Self {
        std::borrow::Cow::Borrowed(status)
    }
}

impl From<Status> for std::borrow::Cow<'_, Status> {
    fn from(status: Status) -> Self {
        std::borrow::Cow::Owned(status)
    }
}

impl Status {
    /// Words alone, such as a group DM's size.
    pub fn words(words: String) -> Self {
        Self {
            words,
            ..Self::default()
        }
    }

    /// A custom status or activity as the core keeps it, its emoji apart.
    pub fn of(activity: Option<&str>) -> Self {
        let (emoji, words) = activity.map_or((None, ""), yapfast_core::discord::status_parts);
        Self {
            words: one_line(words),
            emoji,
            ..Self::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.emoji.is_none() && self.icon == 0
    }
}

/// `text` on one line, its runs of space as one.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl MemberLook {
    /// Offline members are faded: the avatar to 0.4, the rest to 0.5.
    pub fn offline(&self) -> bool {
        self.presence == Some(Presence::Offline)
    }
}

/// What every member's look in a list is made with besides the member:
/// a list made with others makes them all again.
#[derive(Clone, Debug, PartialEq)]
pub struct LooksMadeWith {
    pub owner: Option<String>,
    pub dark: bool,
    pub cosmetics: yapfast_core::application::Cosmetics,
    pub hide_nameplates: bool,
    pub in_server: bool,
}

impl LooksMadeWith {
    pub fn of(state: &AppState, owner: Option<String>, dark: bool) -> Self {
        Self {
            owner,
            dark,
            cosmetics: state.cosmetics,
            hide_nameplates: state.hide_nameplates,
            in_server: in_server(state.selected.as_deref()),
        }
    }
}

/// The member list's lines for `members`, with each group's heading.
/// `owner` is the server owner's id (`Client::server_owner`). Each look of
/// `before` (the members and lines shown, made with the same
/// [`LooksMadeWith`]) whose member is as it was is kept: under presence
/// churn most of a list stays.
pub fn member_lines(
    state: &AppState,
    members: &[ChatMember],
    owner: Option<&str>,
    dark: bool,
    before: Option<(&[ChatMember], &[MemberLine])>,
) -> Vec<MemberLine> {
    let cosmetics = state.cosmetics;
    let in_server = in_server(state.selected.as_deref());
    let now = yapfast_core::clock::now_ms();
    // The looks before, in their members' order, and where each member was.
    let (old_members, old_looks): (&[ChatMember], Vec<&Arc<MemberLook>>) = match before {
        Some((members, lines)) => (
            members,
            lines
                .iter()
                .filter_map(|line| match line {
                    MemberLine::Member(look) => Some(look),
                    MemberLine::Heading { .. } => None,
                })
                .collect(),
        ),
        None => (&[], Vec::new()),
    };
    let old_members = if old_looks.len() == old_members.len() {
        old_members
    } else {
        &[]
    };
    // Found by id only when a member moved, made the first time one did.
    let old_at = std::cell::OnceCell::<std::collections::HashMap<&str, usize>>::new();
    let kept = |index: usize, member: &ChatMember| {
        // A timeout ends with the clock, not with a change.
        if member.timed_out_until.is_some() {
            return None;
        }
        let at = if old_members
            .get(index)
            .is_some_and(|old| old.id == member.id)
        {
            index
        } else {
            *old_at
                .get_or_init(|| {
                    old_members
                        .iter()
                        .enumerate()
                        .map(|(index, member)| (member.id.as_str(), index))
                        .collect()
                })
                .get(member.id.as_str())?
        };
        (old_members.get(at)? == member).then(|| old_looks[at].clone())
    };
    let mut lines = Vec::with_capacity(members.len() + 4);
    for (index, member) in members.iter().enumerate() {
        let heading = if let Some(group) = &member.group {
            (index == 0 || members[index - 1].group.as_ref() != Some(group)).then(|| group.clone())
        } else if index == 0 || members[index - 1].role != member.role {
            let count = members
                .iter()
                .filter(|other| other.role == member.role)
                .count();
            let role = match member.role {
                Role::Admin => "ADMINS",
                Role::Moderator => "MODERATORS",
                Role::Member => "MEMBERS",
            };
            Some(format!("{role} — {count}"))
        } else {
            None
        };
        if let Some(text) = heading {
            lines.push(MemberLine::Heading {
                text,
                icon: member.group_icon.clone(),
            });
        }
        if let Some(look) = kept(index, member) {
            lines.push(MemberLine::Member(look));
            continue;
        }
        let subline = yapfast_core::yapfast_discord::activity::subline(
            &member.activities,
            member.activity.as_deref(),
        );
        let line = subline.name.as_deref().or(member.activity.as_deref());
        let (emoji, words) = line.map_or((None, ""), yapfast_core::discord::status_parts);
        lines.push(MemberLine::Member(Arc::new(MemberLook {
            id: member.id.clone(),
            name: member.name.clone(),
            avatar: member.avatar.clone(),
            decoration: cosmetics
                .decoration(member.decoration.as_deref())
                .map(Arc::from),
            nameplate: member
                .nameplate
                .as_ref()
                .filter(|_| !state.hide_nameplates)
                .map(|plate| {
                    let palette = plate.colors.map(|colors| colors[usize::from(!dark)]);
                    (plate.picture.clone(), palette)
                }),
            style: name_look(
                cosmetics.style(member.style.as_deref()),
                &member.name,
                in_server,
                dark,
            ),
            color: member.color,
            role_icon: member.role_icon.clone(),
            tag: cosmetics
                .tag(member.tag.as_ref())
                .map(|tag| (tag.text.clone(), tag.badge.clone())),
            bot: member.bot,
            owner: owner == Some(member.id.as_str()),
            timed_out: member.timed_out_until.is_some_and(|until| until > now),
            boosting: member.boosting_since.map(|since| {
                format!(
                    "Server boosting since {}",
                    yapfast_core::clock::day(since / 1000)
                )
            }),
            presence: member.presence,
            status: Status {
                words: one_line(words),
                emoji,
                icon: subline.icon,
                bullet: subline.bullet,
            },
        })));
    }
    lines
}

/// Whether a room is in a Discord server rather than a DM: there names show
/// their style's font alone, in the row's own colour.
pub fn in_server(room: Option<&str>) -> bool {
    room.and_then(yapfast_core::discord::parse_room)
        .is_some_and(|(guild, _)| guild != yapfast_core::discord::DM_GUILD)
}

/// Who wrote the forum post the open thread is, whom Discord tags OP:
/// whoever wrote its first message, which has the thread's own id. None
/// outside a forum's post, or until that message is loaded.
pub fn post_author(state: &AppState) -> Option<&str> {
    let room = state.selected_room()?;
    let parent = room.parent.as_deref()?;
    if room.network != yapfast_core::network::Network::Discord
        || !state
            .rooms
            .iter()
            .any(|found| found.id == parent && found.forum)
    {
        return None;
    }
    let item = format!("d:{}", room.id.rsplit(':').next()?);
    state
        .messages
        .iter()
        .find(|message| message.item == item)
        .map(|message| message.sender.as_str())
}

/// The address of the animated picture of a decoration's or nameplate's
/// still, which plays while its row is pointed at (and on an open profile
/// card); other pictures, such as a banner, are their own.
pub fn animated_url(url: &str) -> String {
    match url.strip_suffix("/static") {
        Some(base) if url.contains("/collectibles-shop/") => format!("{base}/animated"),
        _ => url.to_owned(),
    }
}

/// A display name style, on a dark or a light theme.
#[derive(Clone, Debug, PartialEq)]
pub struct NameLook {
    /// 1 solid, 2 gradient, 3 neon, 4 toon, 5 pop, 6 glow, 7 prism, 8 gummy.
    /// In a server it's always 1 with no colours: the font alone, in the
    /// row's own colour.
    pub effect: u8,
    /// The font's family, `None` for the usual font, and its weight.
    pub family: Option<&'static str>,
    pub weight: u16,
    /// The colours picked, made readable; empty in a server.
    pub colors: Vec<u32>,
    /// Shades of the first colour, for solid, neon, toon and pop.
    pub main: u32,
    pub light1: u32,
    pub light2: u32,
    pub dark1: u32,
    pub dark2: u32,
    pub toon: u32,
    pub neon: u32,
    /// A gummy name's letters, each with its colour.
    pub letters: Vec<(char, u32)>,
}

impl NameLook {
    /// The name's own font, if its style sets one.
    pub fn font(&self) -> Option<crate::fonts::NameFont> {
        self.family.map(|family| crate::fonts::NameFont {
            family,
            weight: self.weight,
        })
    }
}

/// The style a name is drawn in, or none. In a server only its font counts;
/// profiles show it whole, its colours made readable on the theme's ground,
/// dark or light.
pub fn name_look(
    style: Option<&NameStyle>,
    name: &str,
    in_server: bool,
    dark: bool,
) -> Option<NameLook> {
    let style = style.filter(|style| !style.colors().is_empty())?;
    if in_server {
        return Some(NameLook {
            effect: 1,
            family: family(style.font),
            weight: weight(style.font),
            colors: Vec::new(),
            main: 0,
            light1: 0,
            light2: 0,
            dark1: 0,
            dark2: 0,
            toon: 0,
            neon: 0,
            letters: Vec::new(),
        });
    }
    let ground = if dark { DARK_GROUND } else { LIGHT_GROUND };
    Some(palette(style, name, ground))
}

/// The font family Discord's client draws a style's `font_id` in; none, the
/// usual font, for 11 and the retired ids.
pub fn family(font: u8) -> Option<&'static str> {
    Some(match font {
        3 => "Sakura",
        4 => "Jellybean",
        6 => "Modern",
        7 => "Néo-castel",
        8 => "8Bit",
        10 => "Sinistre",
        12 => "Tempo",
        13 => "Monkey Bars",
        14 => "Mainframe",
        15 => "Headbang",
        16 => "Journal",
        _ => return None,
    })
}

/// The weight of that font's only face.
pub fn weight(font: u8) -> u16 {
    match font {
        6 | 8 => 500,
        10 => 548,
        12 => 600,
        13 | 14 | 16 => 700,
        _ => 400,
    }
}

/// The grounds a style's colours are made readable on.
const DARK_GROUND: u32 = 0x121214;
const LIGHT_GROUND: u32 = 0xfbfbfb;
const TOON_GROUND: u32 = 0x333333;

fn contrast_needed(effect: u8) -> f64 {
    match effect {
        2 | 6 | 7 => 2.5,
        _ => 3.0,
    }
}

type Rgb = (f64, f64, f64);

fn unpack(rgb: u32) -> Rgb {
    (
        f64::from((rgb >> 16) as u8),
        f64::from((rgb >> 8) as u8),
        f64::from(rgb as u8),
    )
}

fn pack((r, g, b): Rgb) -> u32 {
    let byte = |value: f64| u32::from(value.round().clamp(0., 255.) as u8);
    byte(r) << 16 | byte(g) << 8 | byte(b)
}

fn to_hsl((r, g, b): Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (r / 255., g / 255., b / 255.);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (max + min) / 2.;
    let delta = max - min;
    if delta == 0. {
        return (0., 0., lightness);
    }
    let saturation = delta / (1. - (2. * lightness - 1.).abs());
    let hue = if max == r {
        ((g - b) / delta).rem_euclid(6.)
    } else if max == g {
        (b - r) / delta + 2.
    } else {
        (r - g) / delta + 4.
    };
    (hue * 60., saturation, lightness)
}

fn from_hsl(hue: f64, saturation: f64, lightness: f64) -> Rgb {
    let chroma = (1. - (2. * lightness - 1.).abs()) * saturation;
    let sector = hue.rem_euclid(360.) / 60.;
    let x = chroma * (1. - (sector % 2. - 1.).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.),
        1 => (x, chroma, 0.),
        2 => (0., chroma, x),
        3 => (0., x, chroma),
        4 => (x, 0., chroma),
        _ => (chroma, 0., x),
    };
    let m = lightness - chroma / 2.;
    ((r + m) * 255., (g + m) * 255., (b + m) * 255.)
}

fn luminance((r, g, b): Rgb) -> f64 {
    let linear = |value: f64| {
        let value = value / 255.;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn readable(color: u32, ground: u32, ratio: f64) -> u32 {
    let ground = unpack(ground);
    let lighter = luminance(ground) <= 0.5;
    let mut color = unpack(color);
    for _ in 0..10 {
        if contrast(color, ground) >= ratio {
            break;
        }
        let (hue, saturation, lightness) = to_hsl(color);
        if lighter && lightness >= 0.95 || !lighter && lightness <= 0.05 {
            break;
        }
        let step = if lighter { 0.05 } else { -0.05 };
        color = unpack(pack(from_hsl(hue, saturation, lightness + step)));
    }
    pack(color)
}

fn palette(style: &NameStyle, name: &str, ground: u32) -> NameLook {
    let ratio = contrast_needed(style.effect);
    let ground = if style.effect == 4 {
        TOON_GROUND
    } else {
        ground
    };
    let colors: Vec<u32> = style
        .colors()
        .iter()
        .map(|rgb| readable(*rgb, ground, ratio))
        .collect();
    let main = colors[0];
    let (hue, saturation, lightness) = to_hsl(unpack(main));
    let with_lightness = |lightness: f64| pack(from_hsl(hue, saturation, lightness.clamp(0., 1.)));
    let letters = if style.effect == 8 {
        let mut turn = 0;
        name.chars()
            .map(|letter| {
                let color = colors[turn % colors.len()];
                if !letter.is_whitespace() {
                    turn += 1;
                }
                (letter, color)
            })
            .collect()
    } else {
        Vec::new()
    };
    NameLook {
        effect: if (1..=8).contains(&style.effect) {
            style.effect
        } else {
            1
        },
        family: family(style.font),
        weight: weight(style.font),
        main,
        light1: with_lightness((1.2 * lightness).min(1.)),
        light2: with_lightness((1.6 * lightness).min(1.)),
        dark1: with_lightness((0.6 * lightness).max(0.)),
        dark2: with_lightness((0.2 * lightness).max(0.)),
        toon: with_lightness((0.4 * lightness).max(0.12)),
        neon: pack(from_hsl(
            hue,
            (1.2 * saturation).min(1.),
            (lightness + 0.1).min(0.6),
        )),
        colors,
        letters,
    }
}

// ---------------------------------------------------------------------------
// The chips beside a name

/// How a server tag's chip is drawn where it shows: the letters' size and
/// font, the chip's height, side padding and corners, its ground and ink.
pub struct Chip {
    pub size: f32,
    pub font: Font,
    pub height: f32,
    pub pad: f32,
    pub radius: f32,
    pub fill: Color,
    pub ink: Color,
}

impl<'a> crate::ctx::Ctx<'a> {
    /// A server tag as Discord's client draws it: a grey chip, the badge,
    /// then up to four letters, all faded by `opacity`.
    pub fn tag_chip(
        self,
        letters: &str,
        badge: Option<&str>,
        chip: Chip,
        opacity: f32,
    ) -> Element<'a, Message> {
        let side = (chip.size + 2.0).round();
        let mut line = row![].spacing(2).align_y(Alignment::Center);
        if let Some(badge) = badge {
            // Its room is kept while the picture is on its way.
            line = line.push(match self.picture(badge, side as u32, side as u32, false) {
                Some(picture) => Element::from(Pic::square(picture, side).opacity(opacity)),
                None => Space::new().width(side).height(side).into(),
            });
        }
        line = line.push(
            text(letters.to_owned())
                .size(chip.size)
                .font(chip.font)
                .color(faded(chip.ink, opacity)),
        );
        let fill = faded(chip.fill, opacity);
        container(line)
            .padding([0.0, chip.pad])
            .height(chip.height)
            .align_y(Alignment::Center)
            .style(move |_| rounded(fill, chip.radius))
            .into()
    }

    /// The APP tag a bot's name wears.
    pub fn bot_tag(self, opacity: f32) -> Element<'a, Message> {
        self.blurple_tag("APP", opacity)
    }

    /// Discord's white on blurple tag after a name: APP, or OP for the
    /// author of a forum post.
    pub fn blurple_tag(self, label: &'static str, opacity: f32) -> Element<'a, Message> {
        let fill = faded(hex(BLURPLE), opacity);
        container(
            text(label)
                .size(10)
                .font(BOLD)
                .color(faded(Color::WHITE, opacity)),
        )
        .padding([0, 4])
        .height(16)
        .align_y(Alignment::Center)
        .style(move |_| rounded(fill, 4))
        .into()
    }
}

#[cfg(test)]
mod tests {
    use yapfast_core::{
        application::AppState,
        chat::{ChatMember, Presence, Role},
    };

    use super::{MemberLine, member_lines};

    fn member(id: &str, role: Role, group: Option<&str>) -> ChatMember {
        ChatMember {
            id: id.into(),
            bot: false,
            name: id.into(),
            username: None,
            role,
            avatar: None,
            decoration: None,
            nameplate: None,
            style: None,
            group: group.map(Into::into),
            group_icon: None,
            role_icon: None,
            presence: Some(Presence::Online),
            color: None,
            activity: None,
            activities: Default::default(),
            roles: Default::default(),
            timed_out_until: None,
            tag: None,
            boosting_since: None,
        }
    }

    fn headings(lines: &[MemberLine]) -> Vec<&str> {
        lines
            .iter()
            .filter_map(|line| match line {
                MemberLine::Heading { text, .. } => Some(text.as_str()),
                MemberLine::Member(_) => None,
            })
            .collect()
    }

    #[test]
    fn members_sit_under_their_roles_counted_or_their_hoisted_groups() {
        let state = AppState::default();
        let by_role = [
            member("ann", Role::Admin, None),
            member("bob", Role::Member, None),
            member("cat", Role::Member, None),
        ];
        let lines = member_lines(&state, &by_role, Some("ann"), true, None);
        assert_eq!(headings(&lines), ["ADMINS — 1", "MEMBERS — 2"]);
        assert_eq!(lines.len(), 5, "two headings and three rows");
        let owner: Vec<bool> = lines
            .iter()
            .filter_map(|line| match line {
                MemberLine::Member(look) => Some(look.owner),
                MemberLine::Heading { .. } => None,
            })
            .collect();
        assert_eq!(owner, [true, false, false], "the owner's crown on theirs");
        // Discord's groups come as the core names them, each once.
        let grouped = [
            member("ann", Role::Member, Some("Online — 2")),
            member("bob", Role::Member, Some("Online — 2")),
            member("cat", Role::Member, Some("Offline — 1")),
        ];
        let lines = member_lines(&state, &grouped, None, true, None);
        assert_eq!(headings(&lines), ["Online — 2", "Offline — 1"]);
    }

    #[test]
    fn a_timeout_still_running_shows_its_clock_and_a_boost_says_since_when() {
        let state = AppState::default();
        let now = yapfast_core::clock::now_ms();
        let mut timed = member("ann", Role::Member, None);
        timed.timed_out_until = Some(now + 60_000);
        let mut over = member("bob", Role::Member, None);
        over.timed_out_until = Some(now - 60_000);
        over.boosting_since = Some(1_700_000_000_000);
        let lines = member_lines(&state, &[timed, over], None, true, None);
        let looks: Vec<_> = lines
            .iter()
            .filter_map(|line| match line {
                MemberLine::Member(look) => Some(look),
                MemberLine::Heading { .. } => None,
            })
            .collect();
        assert!(
            looks[0].timed_out && !looks[1].timed_out,
            "only while it runs"
        );
        assert!(
            looks[1]
                .boosting
                .as_deref()
                .is_some_and(|words| words.starts_with("Server boosting since ")),
            "{:?}",
            looks[1].boosting
        );
    }

    #[test]
    fn a_roster_change_keeps_the_looks_of_members_who_stayed_the_same() {
        let state = AppState::default();
        let before = [
            member("ann", Role::Member, None),
            member("bob", Role::Member, None),
            member("cat", Role::Member, None),
        ];
        let lines = member_lines(&state, &before, None, true, None);
        // Bob went idle, Dan came before Cat.
        let mut idle = member("bob", Role::Member, None);
        idle.presence = Some(Presence::Idle);
        let after = [
            member("ann", Role::Member, None),
            idle,
            member("dan", Role::Member, None),
            member("cat", Role::Member, None),
        ];
        let new = member_lines(&state, &after, None, true, Some((&before, &lines)));
        let look = |lines: &[MemberLine], id: &str| match lines
            .iter()
            .find(|line| matches!(line, MemberLine::Member(look) if look.id == id))
        {
            Some(MemberLine::Member(look)) => look.clone(),
            _ => panic!("{id} listed"),
        };
        assert!(std::sync::Arc::ptr_eq(
            &look(&lines, "ann"),
            &look(&new, "ann")
        ));
        assert!(
            std::sync::Arc::ptr_eq(&look(&lines, "cat"), &look(&new, "cat")),
            "moved down"
        );
        assert!(!std::sync::Arc::ptr_eq(
            &look(&lines, "bob"),
            &look(&new, "bob")
        ));
        assert_eq!(look(&new, "bob").presence, Some(Presence::Idle));
        assert_eq!(headings(&new), ["MEMBERS — 4"]);
        assert_eq!(
            new,
            member_lines(&state, &after, None, true, None),
            "as made anew"
        );
    }
}
