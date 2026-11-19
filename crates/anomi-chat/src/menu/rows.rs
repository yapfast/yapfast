//! The message menu's rows, in Discord's order (`ui/messages.slint:1448`).

use yapfast_lang::tr;

use anomi_vm::chat::menu::{Intent, Menu};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Row;

/// Fills `menu.rows` from its facts and apps.
pub fn build(menu: &mut Menu) {
    menu.rows = make(menu);
}

/// The rows under the quick reactions: what the message allows, a line
/// starting each group of copies, the delete and the id.
fn make(menu: &Menu) -> Vec<Row<Intent>> {
    let facts = &menu.facts;
    let mut rows: Vec<Row<Intent>> = Vec::new();
    if facts.can_react {
        add(
            &mut rows,
            tr!("Add reaction"),
            Icon::SmilePlus,
            Intent::AddReaction,
        );
    }
    if facts.has_reactions {
        add(
            &mut rows,
            tr!("View Reactions"),
            Icon::Smile,
            Intent::ViewReactions,
        );
    }
    if facts.can_edit && facts.composer_enabled {
        add(&mut rows, tr!("Edit message"), Icon::Pencil, Intent::Edit);
    }
    if facts.can_reply && facts.composer_enabled {
        add(&mut rows, tr!("Reply"), Icon::Reply, Intent::Reply);
    }
    if facts.can_forward && facts.forward_allowed {
        add(
            &mut rows,
            tr!("MessageMenu" => "Forward"),
            Icon::Forward,
            Intent::Forward,
        );
    }
    if facts.can_start_thread && facts.can_reply && facts.delivered && facts.thread.is_none() {
        add(
            &mut rows,
            tr!("Create Thread"),
            Icon::Thread,
            Intent::StartThread,
        );
    }
    if let (true, Some(room)) = (facts.can_reply, &facts.thread) {
        add(
            &mut rows,
            tr!("View Thread"),
            Icon::Thread,
            Intent::ViewThread(room.clone()),
        );
    }
    if !facts.body.is_empty() {
        add(
            &mut rows,
            tr!("Copy text"),
            Icon::Copy,
            Intent::Copy(facts.body.clone()),
        );
    }
    if !facts.selection.is_empty() {
        add(
            &mut rows,
            tr!("Copy selection"),
            Icon::TextSelect,
            Intent::Copy(facts.selection.clone()),
        );
    }
    if facts.can_end_poll {
        add(
            &mut rows,
            tr!("End Poll Now"),
            Icon::CircleStop,
            Intent::EndPoll,
        );
    }
    if facts.can_pin && facts.delivered {
        let (text, icon) = if facts.pinned {
            (tr!("Unpin message"), Icon::PinOff)
        } else {
            (tr!("Pin message"), Icon::Pin)
        };
        add(&mut rows, text, icon, Intent::Pin);
    }
    if !menu.apps.is_empty() {
        let apps = menu
            .apps
            .iter()
            .map(|app| {
                Row::new(app.name.as_str(), Intent::RunApp(app.id.clone()))
                    .subtitle(app.app.as_str())
                    .plain()
            })
            .collect();
        // A click opens the submenu as the pointer arriving does.
        let mut row = Row::opens(tr!("Apps"), apps).icon(Icon::Slash);
        row.intent = Some(Intent::Apps(true));
        rows.push(row);
    }
    // After Apps, as in Discord's menu.
    if facts.can_mark_unread {
        add(
            &mut rows,
            tr!("Mark Unread"),
            Icon::MessageCircle,
            Intent::MarkUnread,
        );
    }
    if !facts.link.is_empty() {
        add(
            &mut rows,
            tr!("Copy Message Link"),
            Icon::Link,
            Intent::Copy(facts.link.clone()),
        );
    }
    // The picture's, the video's and the link's own, under a line.
    let from = rows.len();
    if let Some(media) = facts.media.filter(|_| !facts.preview) {
        if !facts.address.is_empty() && !media.video {
            add(
                &mut rows,
                tr!("Copy Image"),
                Icon::Image,
                Intent::CopyImage(media.index),
            );
        }
        let text = if media.video {
            tr!("Save Video")
        } else {
            tr!("Save Image")
        };
        add(&mut rows, text, Icon::Download, Intent::Save(media.index));
    }
    if !facts.address.is_empty() {
        add(
            &mut rows,
            tr!("Copy Link"),
            Icon::Link,
            Intent::Copy(facts.address.clone()),
        );
        if facts.media.is_some() {
            add(
                &mut rows,
                tr!("Open Link"),
                Icon::ExternalLink,
                Intent::OpenLink(facts.address.clone()),
            );
        }
    }
    separate(&mut rows, from);
    if facts.can_delete && facts.composer_enabled {
        let at = rows.len();
        rows.push(
            Row::new(tr!("Delete message"), Intent::Delete)
                .icon(Icon::Trash)
                .danger(),
        );
        separate(&mut rows, at);
    }
    // Last, as Discord's client puts it.
    if !facts.id.is_empty() {
        let at = rows.len();
        rows.push(
            Row::new(tr!("Copy Message ID"), Intent::Copy(facts.id.clone())).icon(Icon::Hash),
        );
        separate(&mut rows, at);
    }
    rows
}

/// A row with its icon.
fn add(rows: &mut Vec<Row<Intent>>, text: &str, icon: Icon, intent: Intent) {
    rows.push(Row::new(text, intent).icon(icon));
}

/// A line over the row at `at`, when there is one.
fn separate(rows: &mut [Row<Intent>], at: usize) {
    if let Some(row) = rows.get_mut(at) {
        row.separated = true;
    }
}
