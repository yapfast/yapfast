//! How plugins say people look, put on everything that shows a person: a
//! message's author line and its mentions, the member list's rows, the voice
//! list and the typing strip, as well as the profile card.
//!
//! The answers are kept per person and server ([`super::identity`]); this
//! takes what the window would draw and gives back the same with a plugin's
//! name, colour, avatar and decoration in place of Yapfast's, copying only
//! the rows a plugin changed. The application keeps what it was given beside
//! what it shows, so an answer that goes away puts Yapfast's own back.

use std::sync::Arc;

use yapfast_plugin_api::identity::Identity;

use crate::chat::{ChatMember, ChatMessage};
use crate::mention::{Mentioned, Target};

/// How a person looks, by their id, when a plugin said.
pub type Lookup<'a> = dyn Fn(&str) -> Option<Arc<Identity>> + 'a;

/// A message as plugins say its author and the people it mentions look; none
/// when nothing about it changes.
pub fn message(message: &ChatMessage, look: &Lookup<'_>) -> Option<ChatMessage> {
    let author = crate::discord::sender_user(&message.sender)
        .filter(|user| !user.is_empty())
        .and_then(look);
    let mentions = mentions(&message.body, &message.mentions, look);
    if author.is_none() && mentions.is_none() {
        return None;
    }
    let mut dressed = message.clone();
    if let Some(identity) = author {
        if let Some(name) = &identity.display_name {
            dressed.author.clone_from(name);
        }
        if let Some(colour) = identity.name_colour {
            dressed.color = Some(colour);
            dressed.style = None;
        }
        if let Some(avatar) = &identity.avatar {
            dressed.avatar = Some(avatar.clone());
        }
        if let Some(decoration) = &identity.decoration {
            dressed.decoration = Some(Arc::from(decoration.as_str()));
        }
    }
    if let Some((body, mentions)) = mentions {
        dressed.body = body;
        dressed.mentions = mentions;
    }
    Some(dressed)
}

/// The body with each person it mentions named and coloured as a plugin says,
/// and the mentions' places moved to match; none when no mention changes.
pub fn mentions(
    body: &str,
    mentioned: &[Mentioned],
    look: &Lookup<'_>,
) -> Option<(String, Vec<Mentioned>)> {
    // Every place a mention is written, with what it becomes.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut out: Vec<Mentioned> = mentioned.to_vec();
    let mut any = false;
    for (index, mention) in mentioned.iter().enumerate() {
        let Target::User(person) = &mention.target else {
            continue;
        };
        let Some(identity) = look(person) else {
            continue;
        };
        if let Some(colour) = identity.name_colour {
            out[index].color = Some(colour);
            any = true;
        }
        let Some(name) = &identity.display_name else {
            continue;
        };
        let text = format!("@{name}");
        for &at in &mention.at {
            // Only where the text is what the mention says it is.
            if body.get(at..at + mention.text.len()) == Some(mention.text.as_str()) {
                edits.push((at, mention.text.len(), text.clone()));
            }
        }
        out[index].text = text;
        any = true;
    }
    if !any {
        return None;
    }
    edits.sort_by_key(|edit| edit.0);
    let mut written = String::with_capacity(body.len());
    let mut from = 0;
    // Two mentions never overlap; one that would is left as it was.
    edits.retain({
        let mut end = 0;
        move |(at, old, _)| {
            let keep = *at >= end;
            if keep {
                end = at + old;
            }
            keep
        }
    });
    for (at, old, new) in &edits {
        written.push_str(&body[from..*at]);
        written.push_str(new);
        from = at + old;
    }
    written.push_str(&body[from..]);
    let land = |at: usize| -> usize {
        // The shift of the edits before it, and its own when it is one.
        let before: isize = edits
            .iter()
            .filter(|(edit, _, _)| *edit < at)
            .map(|(_, old, new)| new.len() as isize - *old as isize)
            .sum();
        (at as isize + before).max(0) as usize
    };
    for mention in &mut out {
        mention.at = mention.at.iter().map(|&at| land(at)).collect();
    }
    Some((written, out))
}

/// A member as a plugin says they look; none when nothing changes.
pub fn member(member: &ChatMember, look: &Lookup<'_>) -> Option<ChatMember> {
    let identity = look(&member.id)?;
    let mut dressed = member.clone();
    if let Some(name) = &identity.display_name {
        dressed.name.clone_from(name);
    }
    if let Some(colour) = identity.name_colour {
        dressed.color = Some(colour);
        dressed.style = None;
    }
    if let Some(avatar) = &identity.avatar {
        dressed.avatar = Some(avatar.clone());
    }
    if let Some(decoration) = &identity.decoration {
        dressed.decoration = Some(Arc::from(decoration.as_str()));
    }
    (dressed != *member).then_some(dressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str, colour: Option<u32>) -> Arc<Identity> {
        Arc::new(Identity {
            display_name: Some(name.to_owned()),
            name_colour: colour,
            ..Identity::default()
        })
    }

    #[test]
    fn a_message_takes_its_authors_and_its_mentions_looks() {
        let message = ChatMessage {
            sender: "discord:7".into(),
            author: "ann".into(),
            body: "hi @bob and @bob, @cy".into(),
            mentions: vec![
                Mentioned {
                    text: "@bob".into(),
                    target: Target::User("8".into()),
                    color: None,
                    at: vec![3, 12],
                },
                Mentioned {
                    text: "@cy".into(),
                    target: Target::User("9".into()),
                    color: None,
                    at: vec![18],
                },
            ],
            ..ChatMessage::default()
        };
        let look = |person: &str| match person {
            "7" => Some(named("Annie", Some(0x00ff00))),
            "8" => Some(named("Robert", None)),
            _ => None,
        };
        let dressed = super::message(&message, &look).expect("changed");
        assert_eq!(dressed.author, "Annie");
        assert_eq!(dressed.color, Some(0x00ff00));
        assert_eq!(dressed.body, "hi @Robert and @Robert, @cy");
        assert_eq!(dressed.mentions[0].text, "@Robert");
        assert_eq!(dressed.mentions[0].at, vec![3, 15]);
        assert_eq!(dressed.mentions[1].at, vec![24], "a later mention moves");
        assert_eq!(&dressed.body[24..27], "@cy");
        assert!(super::message(&message, &|_| None).is_none());
    }

    #[test]
    fn a_member_takes_their_looks() {
        let member = ChatMember {
            bot: false,
            id: "8".into(),
            name: "bob".into(),
            username: None,
            role: crate::chat::Role::Member,
            avatar: None,
            decoration: None,
            nameplate: None,
            style: None,
            group: None,
            presence: None,
            color: None,
            activity: None,
            activities: Default::default(),
            roles: Default::default(),
            timed_out_until: None,
            tag: None,
            group_icon: None,
            role_icon: None,
            boosting_since: None,
        };
        let dressed = super::member(&member, &|_| Some(named("Robert", None))).expect("changed");
        assert_eq!(dressed.name, "Robert");
        assert!(super::member(&member, &|_| None).is_none());
    }
}
