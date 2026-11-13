//! The member lists Yapfast shows beside a channel: the one on screen, those
//! kept for channels shown lately, and the subscriptions that bring them.

use std::{
    collections::{HashMap, HashSet},
    pin::Pin,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use serde_json::Value;
use tokio::{
    sync::{broadcast, watch},
    time::{Duration, Instant, Sleep},
};

use super::{Live, Snapshot, tracker::Tracker};
use crate::{
    cache,
    event::Event,
    gateway::{Dispatch, Gateway},
    member_list::{ListedMember, MemberList, Presence},
    message::Names,
    ready::PRIVATE,
};

/// Member lists kept for channels shown again.
const LISTS: usize = 40;
/// How long a member list Discord sent stands without asking again. A list
/// Discord still sends stays current however old.
const LIST_FRESH: Duration = Duration::from_secs(10 * 60);

pub(super) struct MemberLists {
    live: broadcast::Sender<Live>,
    names: Arc<RwLock<Names>>,
    /// Servers with a channel on screen, subscribed again after reconnecting.
    viewing: HashSet<String>,
    /// The servers asked this session when their busy voice channels'
    /// calls started, with op 43, and the server on screen, a forum's
    /// included, which a new session asks again; none for a DM.
    asked: HashSet<String>,
    on_screen: Option<String>,
    /// The server and channel whose member list is on screen.
    listing: Option<(String, String)>,
    /// The group DM on screen, whose members Yapfast lists itself.
    group: Option<String>,
    /// Member lists of channels shown lately, newest last, to show again at
    /// once while Discord sends them afresh.
    kept: Vec<MemberList>,
    /// The member list of the channel on screen, while connected.
    shown: Option<MemberList>,
    /// The readers each server's subscription lists, while connected.
    subscribed: HashMap<String, String>,
    /// When the list on screen goes stale, unless Discord still sends it.
    stale: Pin<Box<Sleep>>,
    /// This account's presence as it last told Discord, which its row
    /// shows: Discord lists it online even while it's invisible.
    own: Presence,
}

impl MemberLists {
    pub(super) fn new(
        kept: Vec<MemberList>,
        live: broadcast::Sender<Live>,
        names: Arc<RwLock<Names>>,
        own: Presence,
    ) -> Self {
        Self {
            live,
            names,
            viewing: HashSet::new(),
            asked: HashSet::new(),
            on_screen: None,
            listing: None,
            group: None,
            kept,
            shown: None,
            subscribed: HashMap::new(),
            stale: Box::pin(tokio::time::sleep(LIST_FRESH)),
            own,
        }
    }

    /// Shows this account's row with the presence it just told Discord.
    pub(super) fn own_presence(&mut self, tracker: &Tracker, presence: Presence) {
        if self.own != presence {
            self.own = presence;
            self.publish(tracker);
            self.publish_group(tracker);
        }
    }

    /// The channel whose member list is on screen, and how many people can
    /// read it, as [`Snapshot::listed`].
    pub(super) fn listed(&self) -> Option<(String, u64)> {
        self.shown
            .as_ref()
            .and_then(|list| Some((list.channel().to_owned(), list.count()?)))
    }

    /// Picks up the list on screen in a new session, which has no
    /// subscriptions: those of servers on screen go again. The server whose
    /// channel is on screen is asked again when its voice channels' calls
    /// started; the others are as they're opened.
    pub(super) async fn connect(&mut self, tracker: &Tracker, gateway: &mut Gateway) -> Result<()> {
        self.shown = self.listing.clone().map(|(guild, channel)| {
            let readers = tracker.readers(&guild, &channel);
            self.take(&guild, &channel, &readers)
        });
        self.subscribed.clear();
        for guild in &self.viewing {
            let mut channel = None;
            if let Some(list) = self.shown.as_ref().filter(|list| list.guild() == guild) {
                self.subscribed
                    .insert(guild.clone(), list.readers().to_owned());
                channel = Some(list.channel().to_owned());
            }
            gateway.subscribe(guild, channel.as_deref()).await?;
        }
        if let Some(guild) = self.on_screen.clone() {
            self.ask(gateway, &guild).await?;
        }
        self.stale.as_mut().reset(Instant::now() + LIST_FRESH);
        Ok(())
    }

    /// Drops what only a session had. The list on screen is kept with the
    /// others, so the next session, or the saved account while offline,
    /// shows it again at once.
    pub(super) fn disconnected(&mut self) {
        if let Some(list) = self.shown.take() {
            self.keep(list);
        }
        self.subscribed.clear();
        self.asked.clear();
    }

    /// Asks when `guild`'s busy voice channels' calls started, once a
    /// session: Discord says when one starts or ends after that.
    async fn ask(&mut self, gateway: &mut Gateway, guild: &str) -> Result<()> {
        if self.asked.insert(guild.to_owned()) {
            gateway.request_channel_info(guild).await?;
        }
        Ok(())
    }

    /// Asks again when the calls of a server asked this session, or on
    /// screen, started, as it comes back from an outage or is joined again,
    /// having lost its times when it went.
    pub(super) async fn server_back(&mut self, gateway: &mut Gateway, guild: &str) -> Result<()> {
        if self.asked.contains(guild) || self.on_screen.as_deref() == Some(guild) {
            self.asked.remove(guild);
            self.ask(gateway, guild).await?;
        }
        Ok(())
    }

    /// Notes a channel on screen while Discord can't be told, and shows the
    /// member list kept for it. With [`PRIVATE`] as the server, notes the
    /// group DM on screen.
    pub(super) fn view_offline(
        &mut self,
        snapshots: &watch::Sender<Arc<Snapshot>>,
        guild: String,
        channel: Option<String>,
    ) {
        if guild == PRIVATE {
            self.group = channel;
            self.on_screen = None;
            return;
        }
        self.on_screen = Some(guild.clone());
        if let Some(channel) = channel {
            self.show_kept(snapshots, &guild, &channel);
            self.listing = Some((guild.clone(), channel));
        }
        self.group = None;
        self.viewing.insert(guild);
    }

    /// Tells Discord a server's channel is on screen, and shows its member
    /// list, at once when one is kept for it. With [`PRIVATE`] as the
    /// server, shows the group DM's members. Returns whether the list on
    /// screen changed, and with it the count [`Self::listed`] gives.
    pub(super) async fn view(
        &mut self,
        tracker: &Tracker,
        gateway: &mut Gateway,
        guild: String,
        channel: Option<String>,
    ) -> Result<bool> {
        if guild == PRIVATE {
            self.group = channel;
            self.on_screen = None;
            self.publish_group(tracker);
            return Ok(false);
        }
        self.on_screen = Some(guild.clone());
        self.group = None;
        let new = self.viewing.insert(guild.clone());
        // When its busy voice channels' calls started, which their rows show.
        self.ask(gateway, &guild).await?;
        let Some(channel) = channel else {
            if new {
                gateway.subscribe(&guild, None).await?;
            }
            return Ok(false);
        };
        self.listing = Some((guild.clone(), channel.clone()));
        let readers = tracker.readers(&guild, &channel);
        match self.shown.as_mut() {
            // Channels with the same readers share a list.
            Some(list) if list.guild() == guild && list.readers() == readers => {
                list.show_for(&channel);
            }
            _ => {
                let list = self.take(&guild, &channel, &readers);
                if let Some(previous) = self.shown.replace(list) {
                    self.keep(previous);
                }
            }
        }
        // Shown at once from the cache, until Discord's sync.
        if self.shown.as_ref().is_some_and(MemberList::synced) {
            self.publish(tracker);
        }
        let sent = self.refresh(gateway).await;
        if new && !self.subscribed.contains_key(&guild) {
            gateway.subscribe(&guild, None).await?;
        }
        sent.map(|()| true)
    }

    /// Whether the list on screen is one Discord doesn't send, so it goes
    /// stale.
    pub(super) fn unsent(&self) -> bool {
        self.shown.as_ref().is_some_and(|list| {
            self.subscribed
                .get(list.guild())
                .is_none_or(|readers| readers != list.readers())
        })
    }

    /// Waits until the list on screen went stale without Discord sending it.
    pub(super) async fn went_stale(&mut self) {
        self.stale.as_mut().await;
    }

    /// Asks Discord for the member list on screen, unless Discord already
    /// sends it or sent it within [`LIST_FRESH`]; then it's asked for when
    /// it's due.
    pub(super) async fn refresh(&mut self, gateway: &mut Gateway) -> Result<()> {
        let Some(list) = self.shown.as_mut() else {
            return Ok(());
        };
        if self
            .subscribed
            .get(list.guild())
            .is_some_and(|readers| readers == list.readers())
        {
            return Ok(());
        }
        if list.heard_within(LIST_FRESH)
            && let Some(at) = list.stale_at(LIST_FRESH)
        {
            self.stale.as_mut().reset(at.into());
            return Ok(());
        }
        self.subscribed
            .insert(list.guild().to_owned(), list.readers().to_owned());
        gateway.subscribe(list.guild(), Some(list.channel())).await
    }

    /// Takes in a member list update, for the list on screen or one kept;
    /// returns whether the count on screen changed.
    pub(super) fn update(&mut self, tracker: &Tracker, data: &Value) -> bool {
        let mut names = self
            .names
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (mut changed, mut counted) = (false, false);
        if let Some(list) = self.shown.as_mut() {
            let count = list.count();
            changed = list.apply(data, &mut names);
            counted = list.count() != count;
        }
        // Or a list kept from before, which Discord still sends.
        if !changed {
            for kept in &mut self.kept {
                if kept.apply(data, &mut names) {
                    break;
                }
            }
        }
        drop(names);
        // A list Discord just sent is saved for the next start.
        let sent = data["ops"]
            .as_array()
            .is_some_and(|ops| ops.iter().any(|op| op["op"] == "SYNC"));
        if changed && sent {
            cache::keep_lists(
                self.kept
                    .iter()
                    .chain(self.shown.as_ref())
                    .cloned()
                    .collect(),
            );
        }
        if changed {
            self.publish(tracker);
        }
        counted
    }

    /// Takes in a presence for the list on screen.
    pub(super) fn presence(&mut self, tracker: &Tracker, data: &Value) {
        if self.shown.as_mut().is_some_and(|list| list.presence(data)) {
            self.publish(tracker);
        }
    }

    /// Shows the list on screen again after a change to someone it lists
    /// that came without a list operation, as GUILD_MEMBER_UPDATE's roles,
    /// nickname or timeout may.
    pub(super) fn member_changed(&self, tracker: &Tracker, data: &Value) {
        let user = data["user"]["id"].as_str();
        let listed = self.shown.as_ref().is_some_and(|list| {
            data["guild_id"] == list.guild() && user.is_some_and(|user| list.lists(user))
        });
        if listed {
            self.publish(tracker);
        }
    }

    /// Shows the list on screen again after USER_UPDATE changed this
    /// account's display name or avatar, if it lists it.
    pub(super) fn user_changed(&self, tracker: &Tracker, data: &Value) {
        let user = data["id"].as_str();
        if self
            .shown
            .as_ref()
            .is_some_and(|list| user.is_some_and(|user| list.lists(user)))
        {
            self.publish(tracker);
        }
    }

    /// Shows the group DM's members again after an event that may change
    /// them. Server events, such as presences in a server, don't.
    pub(super) fn group_event(&self, tracker: &Tracker, event: &Dispatch) {
        if self.group.is_some()
            && event.data["guild_id"].is_null()
            && matches!(
                event.event,
                Event::ReadySupplemental
                    | Event::PresenceUpdate
                    | Event::UserUpdate
                    | Event::ChannelUpdate
                    | Event::ChannelRecipientAdd
                    | Event::ChannelRecipientRemove
            )
        {
            self.publish_group(tracker);
        }
    }

    /// The members of the group DM on screen.
    pub(super) fn publish_group(&self, tracker: &Tracker) {
        let Some(channel) = &self.group else {
            return;
        };
        if let Some(mut members) = tracker.group_members(channel) {
            self.show_own(&mut members, &tracker.me);
            let _ = self.live.send(Live::Members {
                channel: channel.clone(),
                members,
            });
        }
    }

    fn publish(&self, tracker: &Tracker) {
        let Some(list) = &self.shown else {
            return;
        };
        let server = tracker
            .servers
            .iter()
            .find(|server| server.id == list.guild())
            .map(Arc::as_ref);
        let names = self
            .names
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut members = list.members(server, &names);
        self.show_own(&mut members, &tracker.me);
        let _ = self.live.send(Live::Members {
            channel: list.channel().to_owned(),
            members,
        });
    }

    /// Gives this account's row its own presence, as Discord's apps do.
    fn show_own(&self, members: &mut [ListedMember], me: &str) {
        if let Some(member) = members.iter_mut().find(|member| member.user_id == me) {
            member.presence = self.own;
        }
    }

    /// Shows the member list kept for a channel while Discord can't send it,
    /// as when starting from the saved account.
    fn show_kept(&self, snapshots: &watch::Sender<Arc<Snapshot>>, guild: &str, channel: &str) {
        let snapshot = snapshots.borrow().clone();
        let Some(server) = snapshot.servers.iter().find(|server| server.id == guild) else {
            return;
        };
        let readers = server.readers(channel);
        let Some(list) = self
            .kept
            .iter()
            .find(|list| list.guild() == guild && list.readers() == readers && list.synced())
        else {
            return;
        };
        let mut members = {
            let names = self
                .names
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            list.members(Some(server), &names)
        };
        if let Some(me) = &snapshot.user_id {
            self.show_own(&mut members, me);
        }
        let _ = self.live.send(Live::Members {
            channel: channel.to_owned(),
            members,
        });
        if let Some(count) = list.count() {
            snapshots.send_modify(|snapshot| {
                Arc::make_mut(snapshot).listed = Some((channel.to_owned(), count))
            });
        }
    }

    /// Keeps a member list Discord sent, for when a channel with the same
    /// readers is shown again.
    fn keep(&mut self, list: MemberList) {
        if !list.synced() {
            return;
        }
        self.kept
            .retain(|kept| kept.guild() != list.guild() || kept.readers() != list.readers());
        self.kept.push(list);
        if self.kept.len() > LISTS {
            self.kept.remove(0);
        }
        cache::keep_lists(self.kept.clone());
    }

    /// The member list kept for channels with these readers, shown for
    /// `channel`, or an empty one.
    fn take(&mut self, guild: &str, channel: &str, readers: &str) -> MemberList {
        match self
            .kept
            .iter()
            .position(|list| list.guild() == guild && list.readers() == readers)
        {
            Some(index) => {
                let mut list = self.kept.remove(index);
                list.show_for(channel);
                list
            }
            None => MemberList::new(guild.to_owned(), channel.to_owned(), readers.to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Discord's sync of list `id` in server 10, listing Bob.
    fn sync(id: &str) -> Value {
        json!({
            "guild_id": "10", "id": id, "member_count": 1,
            "groups": [{"id": "online", "count": 1}],
            "ops": [{"op": "SYNC", "range": [0, 99], "items": [
                {"group": {"id": "online", "count": 1}},
                {"member": {"user": {"id": "2", "username": "bob"}, "roles": [], "nick": null}},
            ]}],
        })
    }

    #[tokio::test]
    async fn the_list_on_screen_is_kept_through_a_disconnect() {
        let (live, _) = broadcast::channel(8);
        let mut lists = MemberLists::new(Vec::new(), live, Arc::default(), Presence::Online);
        let mut list = MemberList::new("10".into(), "11".into(), "everyone".into());
        list.apply(&sync("everyone"), &mut Names::default());
        assert!(list.synced());
        lists.listing = Some(("10".into(), "11".into()));
        lists.shown = Some(list);
        lists.disconnected();
        assert!(lists.shown.is_none());
        // The next session takes it up again rather than starting empty.
        let taken = lists.take("10", "11", "everyone");
        assert!(taken.synced());
        assert_eq!(taken.count(), Some(1));
    }

    #[tokio::test]
    async fn a_channel_shown_before_keeps_its_list() {
        let (live, _) = broadcast::channel(8);
        let names = Arc::<RwLock<Names>>::default();
        let tracker = Tracker::from_ready("1".into(), &json!({}), names.clone());
        let mut lists = MemberLists::new(Vec::new(), live, names, Presence::Online);
        lists.shown = Some(MemberList::new("10".into(), "11".into(), "everyone".into()));
        lists.update(&tracker, &sync("everyone"));
        // Another channel, with other readers, is shown: Discord stops sending
        // the first list, and its sync comes once more, late.
        let second = MemberList::new("10".into(), "12".into(), "3007946959".into());
        let first = lists.shown.replace(second).unwrap();
        lists.keep(first);
        lists.update(
            &tracker,
            &json!({"guild_id": "10", "id": "everyone", "ops": [{"op": "INVALIDATE", "range": [0, 99]}]}),
        );
        lists.update(&tracker, &sync("everyone"));
        assert!(
            !lists.shown.as_ref().unwrap().synced(),
            "the first channel's list isn't the second's"
        );
        // Shown again, the first channel lists Bob still.
        let names = lists.names.clone();
        let taken = lists.take("10", "11", "everyone");
        let members = taken.members(None, &names.read().unwrap());
        assert_eq!(members.len(), 1);
    }

    #[tokio::test]
    async fn this_account_shows_the_presence_it_set() {
        let (live, mut shown) = broadcast::channel(8);
        let names = Arc::<RwLock<Names>>::default();
        let tracker = Tracker::from_ready("2".into(), &json!({}), names.clone());
        let mut lists = MemberLists::new(Vec::new(), live, names, Presence::Offline);
        lists.shown = Some(MemberList::new("10".into(), "11".into(), "everyone".into()));
        // Invisible, the account is listed online, as Discord sends it.
        let mut sync = sync("everyone");
        sync["ops"][0]["items"][1]["member"]["presence"] = json!({"status": "online"});
        lists.update(&tracker, &sync);
        let mut presence = || match shown.try_recv() {
            Ok(Live::Members { members, .. }) => members[0].presence,
            _ => panic!("no member list"),
        };
        assert_eq!(presence(), Presence::Offline);
        lists.own_presence(&tracker, Presence::Idle);
        assert_eq!(presence(), Presence::Idle);
    }
}
