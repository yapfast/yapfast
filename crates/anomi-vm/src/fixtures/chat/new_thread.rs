//! Samples of a new thread for tests and shots, after the fake Discord's
//! data: task C25's. The form is the channel's, #general, with its member
//! list replaced.

use crate::Vm;
use crate::chat::new_thread::NewThread;
use crate::conversation::{Aside, Beside, Conversation};
use crate::fixtures::chat::search;
use crate::fixtures::{conversation, shell};

/// A form with nothing written, in a channel where a thread may be public
/// or private.
pub fn blank() -> NewThread {
    NewThread {
        placeholder: "New Thread".into(),
        can_private: true,
        ..NewThread::default()
    }
}

/// A form where a channel's thread is named by hand and its first message
/// is written.
pub fn written() -> NewThread {
    NewThread {
        name: "Weekend plans".into(),
        body: "Who is in for Saturday?".into(),
        focused: true,
        ..blank()
    }
}

/// Starting from a message: it shows over the name, which is optional, and
/// the placeholder is the name made from it.
pub fn from_message() -> NewThread {
    NewThread {
        starter: Some(search::found(3)),
        placeholder: "Message 3: 👍 sounds good".into(),
        ..blank()
    }
}

/// A private thread: the switch is on and says how to invite.
pub fn private() -> NewThread {
    NewThread {
        private: true,
        ..written()
    }
}

/// A channel that offers only private threads: the switch is on and can't
/// change.
pub fn only_private() -> NewThread {
    NewThread {
        private: true,
        only_private: true,
        ..blank()
    }
}

/// A channel that offers no private thread: no switch.
pub fn public_only() -> NewThread {
    NewThread {
        can_private: false,
        ..written()
    }
}

/// Being made: every field and button waits.
pub fn starting() -> NewThread {
    NewThread {
        starting: true,
        ..written()
    }
}

/// Refused, with Discord's reason under the switch.
pub fn refused() -> NewThread {
    NewThread {
        error: "Thread name must be between 1 and 100 characters".into(),
        ..written()
    }
}

/// Closing it asks first.
pub fn leaving() -> NewThread {
    NewThread {
        leaving: true,
        ..written()
    }
}

/// #general with the form beside it, in place of its member list.
pub fn beside() -> Conversation {
    Conversation {
        beside: Beside {
            thread: Some(Aside::NewThread),
            ..conversation::channel().beside
        },
        ..conversation::channel()
    }
}

/// The window with `form` beside the channel.
pub fn window(form: NewThread) -> Vm {
    let mut vm = shell::channel_with(beside);
    vm.chat.new_thread = form;
    vm
}
