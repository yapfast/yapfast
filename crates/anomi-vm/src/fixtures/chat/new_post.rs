//! Samples of a new forum post for tests and shots, after the fake Discord's
//! forum "ideas" (its tags Feature, Bug and Question; its topic "One idea per
//! post. Search before posting."): task C27's. `blank()` and the others are
//! the form alone; `window(..)` is the whole window with the form open in
//! the forum.

use yapfast_lang::tr;

use crate::Vm;
use crate::chat::composer::Staged;
use crate::chat::forum::Mark;
use crate::chat::new_post::{Choice, NewPost, Part};
use crate::fixtures::{chat::forum, pictures};
use crate::text::{Source, markdown};

fn choice(id: &str, name: &str, emoji: &str, chosen: bool) -> Choice {
    Choice {
        id: id.into(),
        name: name.into(),
        mark: Mark::Unicode(emoji.into()),
        chosen,
    }
}

/// The forum's tags, the given ones chosen.
fn tags(chosen: &[&str]) -> Vec<Choice> {
    [
        ("t1", "Feature", "\u{1F680}"),
        ("t2", "Bug", "\u{1F41B}"),
        ("t3", "Question", "\u{2753}"),
    ]
    .map(|(id, name, emoji)| choice(id, name, emoji, chosen.contains(&id)))
    .into()
}

/// The form as it opens: nothing written, no tag chosen, the guidelines open,
/// the title with the keys.
pub fn blank() -> NewPost {
    NewPost {
        key: 1,
        focus: Some(Part::Title),
        choices: tags(&[]),
        more_tags: true,
        guidelines: Some(markdown(Source::new(
            "One idea per post. Search before posting.",
        ))),
        guidelines_open: true,
        can_attach: true,
        can_spoil: true,
        ..NewPost::default()
    }
}

/// The form with the menu of tags open under "Add tags".
pub fn tags_menu() -> NewPost {
    NewPost {
        tags_open: true,
        ..blank()
    }
}

/// A post written: a title, two tags (Bug and Question, in that order),
/// a message and a picture to go with it.
pub fn written() -> NewPost {
    let chosen = tags(&["t2", "t3"]);
    NewPost {
        title: "Crash when pasting a GIF".into(),
        body: "Pasting a GIF from the browser closes the app.".into(),
        focus: Some(Part::Body),
        chosen: vec![chosen[1].clone(), chosen[2].clone()],
        choices: chosen,
        staged: vec![Staged {
            name: "crash.png".into(),
            size: "7 KB".into(),
            picture: Some(pictures::sunset(240, 180)),
            spoiler: false,
            remove_label: tr!("Remove {}", "crash.png"),
            spoil_label: tr!("Mark {} as a spoiler", "crash.png"),
        }],
        ..blank()
    }
}

/// Five tags chosen: "Add tags" is off and the menu's unticked rows too.
pub fn all_tags() -> NewPost {
    let mut choices = tags(&["t1", "t2", "t3"]);
    choices.push(choice("t4", "Done", "\u{2705}", true));
    choices.push(choice("t5", "Later", "\u{23F3}", true));
    choices.push(choice("t6", "Wontfix", "\u{1F6AB}", false));
    NewPost {
        chosen: choices[..5].to_vec(),
        choices,
        more_tags: false,
        ..blank()
    }
}

/// A forum that requires a tag, with the guidelines folded.
pub fn required() -> NewPost {
    NewPost {
        tag_required: true,
        guidelines_open: false,
        ..blank()
    }
}

/// A forum with no tags and no guidelines.
pub fn plain() -> NewPost {
    NewPost {
        choices: Vec::new(),
        more_tags: false,
        guidelines: None,
        ..blank()
    }
}

/// On its way: the fields refuse text and Publish reads "Posting...".
pub fn posting() -> NewPost {
    NewPost {
        posting: true,
        focus: None,
        ..written()
    }
}

/// A post Discord refused.
pub fn failed() -> NewPost {
    NewPost {
        error: "Missing Access".into(),
        ..written()
    }
}

/// Closing after writing: the form asks first.
pub fn leaving() -> NewPost {
    NewPost {
        leaving: true,
        ..written()
    }
}

/// The whole window with `post` open in the forum "ideas".
pub fn window(post: NewPost) -> Vm {
    let mut vm = forum::window(forum::forum());
    vm.chat.forum.composing = Some(post);
    vm
}
