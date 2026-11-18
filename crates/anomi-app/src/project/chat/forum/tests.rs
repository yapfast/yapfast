//! A forum post's card as Discord's client shows it (`post_row`): its first
//! three tags and "+2", the forum's own reaction for a post without any,
//! "OLDER POSTS" over the first older one, and how long ago it was active.

use yapfast_core::application::{Application, ForumInfo, ForumPost};
use yapfast_core::chat::{ForumTag, ListedThread, PostStarter, ShownEmoji, ThreadPerson};
use yapfast_lang::tr;

use anomi_vm::chat::forum::{Mark, Reaction};

use super::row;
use crate::project::chat::threads::ago;

const NOW: u64 = 1_790_000_000_000;

fn tag(name: &str) -> ForumTag {
    ForumTag {
        id: name.into(),
        name: name.into(),
        emoji: Some(ShownEmoji {
            text: "🚀".into(),
            picture: None,
        }),
        moderated: false,
    }
}

fn post(starter: Option<Option<PostStarter>>) -> ForumPost {
    ForumPost {
        thread: ListedThread {
            room: "post".into(),
            name: "Dark theme for the sidebar".into(),
            replies: 2,
            active_ms: NOW - 3 * 3600 * 1000,
            ..ListedThread::default()
        },
        tags: ["Feature", "Bug", "Question", "Done", "Idea"]
            .map(tag)
            .to_vec(),
        starter,
        new: true,
    }
}

#[test]
fn a_card_shows_three_tags_the_forums_reaction_and_its_age() {
    let core = Application::default();
    let info = ForumInfo {
        reaction: Some(ShownEmoji {
            text: "👍".into(),
            picture: None,
        }),
        ..ForumInfo::default()
    };
    let starter = PostStarter {
        author: ThreadPerson {
            name: "Carol".into(),
            color: Some(0x3498db),
            ..ThreadPerson::default()
        },
        text: "The sidebar could follow the system theme.".into(),
        ..PostStarter::default()
    };
    let card = row(&core, &info, &post(Some(Some(starter))), true, NOW);
    assert_eq!(card.heading, tr!("OLDER POSTS"));
    assert_eq!(card.tags.len(), 3);
    assert_eq!(card.tags[0].mark, Mark::Unicode("🚀".into()));
    assert_eq!(card.more_tags, "+2");
    assert_eq!(card.person, "Carol");
    assert!(card.color.is_some() && card.new);
    assert_eq!(
        card.reaction,
        Some(Reaction {
            mark: Mark::Unicode("👍".into()),
            count: String::new(),
        })
    );
    assert_eq!(
        (card.replies.as_str(), card.time.as_str()),
        ("2", ago(NOW - 3 * 3600 * 1000, NOW).as_str())
    );
    // A deleted first message says so and offers no reaction.
    let deleted = row(&core, &info, &post(Some(None)), false, NOW);
    assert_eq!(deleted.text, tr!("Original message was deleted"));
    assert!(deleted.reaction.is_none() && deleted.heading.is_empty());
}

#[test]
fn ages_read_as_discords_list_of_threads_says_them() {
    let second = 1000;
    assert_eq!(ago(NOW - 5 * second, NOW), tr!("{}s ago", 5));
    assert_eq!(ago(NOW - 300 * second, NOW), tr!("{}m ago", 5));
    assert_eq!(ago(NOW - 40 * 86400 * second, NOW), tr!(">30d ago"));
    assert_eq!(ago(NOW - 400 * 86400 * second, NOW), tr!(">1y ago"));
}
