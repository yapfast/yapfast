//! Member searches for mentions, and requests for members' server
//! profiles: each waits for the chunk of members Discord sends back with
//! its nonce.

use std::collections::HashMap;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::oneshot;

use super::tracker::Tracker;
use crate::{
    event::Event,
    gateway::{Dispatch, Gateway},
};

/// The most members one search asks for.
const FOUND: u64 = 10;

/// Member searches waiting for their answer, by nonce.
#[derive(Default)]
pub(super) struct MemberSearches {
    waiting: HashMap<String, oneshot::Sender<Vec<String>>>,
    /// Presences asked for, by nonce: whose, and who waits for it.
    presences: HashMap<String, (String, oneshot::Sender<Option<Value>>)>,
    nonce: u64,
}

impl MemberSearches {
    /// Asks Discord for the members of `guild` whose names start with `query`.
    pub(super) async fn start(
        &mut self,
        gateway: &mut Gateway,
        guild: &str,
        query: &str,
        reply: oneshot::Sender<Vec<String>>,
    ) -> Result<()> {
        if gateway.members_held(guild) {
            let _ = reply.send(Vec::new());
            return Ok(());
        }
        let nonce = self.wait(reply);
        gateway.search_members(guild, query, FOUND, &nonce).await
    }

    /// Asks Discord for these members of `guild`, up to 100; `reply` hears
    /// which it found once their profiles are learned.
    pub(super) async fn members(
        &mut self,
        gateway: &mut Gateway,
        guild: &str,
        users: &[String],
        reply: oneshot::Sender<Vec<String>>,
    ) -> Result<()> {
        if gateway.members_held(guild) {
            let _ = reply.send(Vec::new());
            return Ok(());
        }
        let nonce = self.wait(reply);
        gateway
            .request_members(guild, users, Some(&nonce), false)
            .await
    }

    /// Asks Discord for `user`'s presence in `guild`, as its client does
    /// for a profile it opens there; `reply` hears it, or none.
    pub(super) async fn presence(
        &mut self,
        gateway: &mut Gateway,
        guild: &str,
        user: &str,
        reply: oneshot::Sender<Option<Value>>,
    ) -> Result<()> {
        if gateway.members_held(guild) {
            let _ = reply.send(None);
            return Ok(());
        }
        self.nonce += 1;
        let nonce = format!("yapfast-presence-{}", self.nonce);
        self.presences.retain(|_, (_, reply)| !reply.is_closed());
        self.presences
            .insert(nonce.clone(), (user.to_owned(), reply));
        gateway
            .request_members(guild, &[user.to_owned()], Some(&nonce), true)
            .await
    }

    /// A new nonce, whose chunk `reply` waits for.
    fn wait(&mut self, reply: oneshot::Sender<Vec<String>>) -> String {
        self.nonce += 1;
        let nonce = format!("yapfast-search-{}", self.nonce);
        // Unanswered searches are dropped, so their callers stop waiting.
        self.waiting.retain(|_, reply| !reply.is_closed());
        self.waiting.insert(nonce.clone(), reply);
        nonce
    }

    /// Answers the search a chunk of members is for, if any. Its members are
    /// learned first, so their names are known; then this returns whether
    /// that changed the tracker, and the chunk needn't be applied again.
    pub(super) fn answer(&mut self, tracker: &mut Tracker, event: &Dispatch) -> Option<bool> {
        if event.event == Event::RateLimited {
            // Discord dropped the request this nonce was for.
            let nonce = event.data["meta"]["nonce"].as_str()?;
            if let Some((_, reply)) = self.presences.remove(nonce) {
                let _ = reply.send(None);
                return Some(false);
            }
            let reply = self.waiting.remove(nonce)?;
            let _ = reply.send(Vec::new());
            return Some(false);
        }
        if let Some((user, reply)) = event.data["nonce"]
            .as_str()
            .and_then(|nonce| self.presences.remove(nonce))
        {
            let changed = tracker.apply(&event.event, &event.data);
            let presence = event.data["presences"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|presence| presence["user"]["id"] == user.as_str())
                .cloned();
            let _ = reply.send(presence);
            return Some(changed);
        }
        let reply = event.data["nonce"]
            .as_str()
            .and_then(|nonce| self.waiting.remove(nonce))?;
        let changed = tracker.apply(&event.event, &event.data);
        let found = event.data["members"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|member| member["user"]["id"].as_str())
            .map(str::to_owned)
            .collect();
        let _ = reply.send(found);
        Some(changed)
    }
}
