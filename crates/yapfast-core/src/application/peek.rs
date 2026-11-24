//! Peeking at a room's newest messages from the channel list, without
//! opening it or marking it read.

use super::*;

impl Application {
    /// Peeks at a room's newest messages, or stops. Only a Discord room, or
    /// one of the preview's, has any to show, and not a forum, whose
    /// messages are its posts'. Nothing here opens the room
    /// or marks it read: it isn't selected, and only the selected room is
    /// read.
    pub(super) fn peek(&mut self, room: Option<String>) -> Vec<Effect> {
        let Some(room) = room else {
            self.state.peek = None;
            return vec![];
        };
        if self
            .state
            .peek
            .as_ref()
            .is_some_and(|peek| peek.room == room)
        {
            return vec![];
        }
        if self.state.preview {
            let messages = self.samples.get(&room).map(|samples| {
                let newest = &samples[samples.len().saturating_sub(crate::chat::PEEK_MESSAGES)..];
                Ok(newest.iter().map(|message| (**message).clone()).collect())
            });
            self.state.peek = messages.map(|messages| Peek {
                room,
                messages: Some(messages),
            });
            return vec![];
        }
        let forum = self
            .state
            .rooms
            .iter()
            .any(|listed| listed.id == room && listed.forum);
        if forum || discord::parse_room(&room).is_none() {
            self.state.peek = None;
            return vec![];
        }
        self.state.peek = Some(Peek {
            room: room.clone(),
            messages: None,
        });
        vec![Effect::Backend(Command::Discord(DiscordCommand::Peek {
            room,
        }))]
    }

    /// Messages peeked at came; those of a peek that closed meanwhile go.
    pub(super) fn peeked(&mut self, room: String, result: Result<Vec<ChatMessage>, String>) {
        if let Some(peek) = self.state.peek.as_mut().filter(|peek| peek.room == room) {
            peek.messages = Some(result);
        }
    }
}
