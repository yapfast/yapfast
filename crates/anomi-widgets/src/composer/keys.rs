//! What a key does in the field: `capture-key-pressed` of `Composer`
//! (`ui/composer-field.slint:108`), in its order, then iced's editing keys
//! with the field's changes to them.

use iced::advanced::text::editor::{Binding, KeyPress};
use iced::keyboard::Key;
use iced::keyboard::key::Named;

use super::{Command, CommandKey, Expressions, Suggestion};

/// What the field does with a key, past editing: each becomes one of the
/// caller's messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Act {
    Suggest(Suggestion),
    Command(CommandKey),
    Send,
    Up,
    Browse,
    Paste,
    Expressions(Expressions),
}

/// What the field knows as a key comes: what shows over it, its text, and
/// which messages the caller gave, as a key whose message is missing does
/// what it would without the rule.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Context {
    pub suggesting: bool,
    pub picking: bool,
    pub command: Option<Command>,
    pub empty: bool,
    /// An input method composes: its keys are its own.
    pub composing: bool,
    pub can: Can,
}

/// The messages the caller gave.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Can {
    pub suggest: bool,
    pub command: bool,
    pub send: bool,
    pub up: bool,
    pub browse: bool,
    pub paste: bool,
    pub expressions: bool,
}

/// The binding of a key pressed while the field has the keys, or `None` to
/// leave it to the window: Escape nothing here wants, Tab, keys that do
/// nothing.
pub(super) fn bind(press: KeyPress, context: &Context) -> Option<Binding<Act>> {
    if !press.is_focused {
        return None;
    }
    let mods = press.modifiers;
    let named = match press.key.as_ref() {
        Key::Named(named) => Some(named),
        _ => None,
    };
    if context.composing {
        return None;
    }
    let custom = |act| Some(Binding::Custom(act));
    if context.suggesting && context.can.suggest {
        match named {
            Some(Named::ArrowUp) => return custom(Act::Suggest(Suggestion::Move(-1))),
            Some(Named::ArrowDown) => return custom(Act::Suggest(Suggestion::Move(1))),
            Some(Named::Enter | Named::Tab) if !mods.shift() => {
                return custom(Act::Suggest(Suggestion::Choose));
            }
            Some(Named::Escape) => return custom(Act::Suggest(Suggestion::Dismiss)),
            _ => {}
        }
    }
    if context.can.command {
        let close = |back| custom(Act::Command(CommandKey::Close { back }));
        match context.command {
            None if context.picking && named == Some(Named::Escape) => return close(false),
            Some(command) => match named {
                Some(Named::Tab) if command.options > 0 => {
                    let step = if mods.shift() { -1 } else { 1 };
                    return custom(Act::Command(CommandKey::Tab(step)));
                }
                Some(Named::Escape) => return close(false),
                Some(Named::Backspace) if context.empty && command.focus == 0 => {
                    return close(true);
                }
                Some(Named::Enter)
                    if !mods.shift() && command.options > 0 && command.wants_file =>
                {
                    return custom(Act::Command(CommandKey::File(command.focus)));
                }
                _ => {}
            },
            None => {}
        }
    }
    match named {
        // Left for the window: the reply cancelled, the edit undone (`escaped`).
        Some(Named::Escape) => return None,
        Some(Named::ArrowUp) if mods.alt() && context.can.browse => return custom(Act::Browse),
        Some(Named::ArrowUp)
            if context.empty && !mods.shift() && !mods.control() && context.can.up =>
        {
            return custom(Act::Up);
        }
        Some(Named::Enter)
            if !mods.shift() && !mods.control() && !mods.alt() && context.can.send =>
        {
            return custom(Act::Send);
        }
        _ => {}
    }
    let letter = press
        .key
        .to_latin(press.physical_key)
        .map(|letter| letter.to_ascii_lowercase());
    if mods.control() && !mods.alt() {
        match letter {
            Some('v') if context.can.paste => return custom(Act::Paste),
            Some('g') if !mods.shift() && context.can.expressions => {
                return custom(Act::Expressions(Expressions::Gifs));
            }
            Some('e') if !mods.shift() && context.can.expressions => {
                return custom(Act::Expressions(Expressions::Emoji));
            }
            // Slint's text input redoes on Ctrl+Shift+Z as well as Ctrl+Y.
            Some('z') if mods.shift() => return Some(Binding::Redo),
            _ => {}
        }
    }
    Binding::from_key_press(press)
}

#[cfg(test)]
mod tests {
    use iced::keyboard::Modifiers;
    use iced::keyboard::key::{Code, Physical};

    use super::*;

    fn press(key: Key, modifiers: Modifiers, text: Option<&str>) -> KeyPress {
        KeyPress {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Code(Code::Fn),
            modifiers,
            text: text.map(Into::into),
            is_focused: true,
        }
    }

    fn named(named: Named, modifiers: Modifiers) -> KeyPress {
        press(Key::Named(named), modifiers, None)
    }

    fn all() -> Can {
        Can {
            suggest: true,
            command: true,
            send: true,
            up: true,
            browse: true,
            paste: true,
            expressions: true,
        }
    }

    fn context() -> Context {
        Context {
            can: all(),
            ..Context::default()
        }
    }

    fn act(binding: Option<Binding<Act>>) -> Option<Act> {
        match binding {
            Some(Binding::Custom(act)) => Some(act),
            _ => None,
        }
    }

    #[test]
    fn suggestions_take_the_arrows_tab_enter_and_escape() {
        let shown = Context {
            suggesting: true,
            ..context()
        };
        let none = Modifiers::empty();
        let suggest = |key, mods| act(bind(named(key, mods), &shown));
        assert_eq!(
            suggest(Named::ArrowUp, none),
            Some(Act::Suggest(Suggestion::Move(-1)))
        );
        assert_eq!(
            suggest(Named::ArrowDown, none),
            Some(Act::Suggest(Suggestion::Move(1)))
        );
        assert_eq!(
            suggest(Named::Tab, none),
            Some(Act::Suggest(Suggestion::Choose))
        );
        assert_eq!(
            suggest(Named::Enter, none),
            Some(Act::Suggest(Suggestion::Choose))
        );
        assert_eq!(
            suggest(Named::Escape, none),
            Some(Act::Suggest(Suggestion::Dismiss))
        );
        // Shift+Enter still breaks the line.
        assert_eq!(
            bind(named(Named::Enter, Modifiers::SHIFT), &shown),
            Some(Binding::Enter)
        );
    }

    #[test]
    fn enter_sends_and_shift_enter_breaks() {
        let plain = context();
        assert_eq!(
            act(bind(named(Named::Enter, Modifiers::empty()), &plain)),
            Some(Act::Send)
        );
        assert_eq!(
            bind(named(Named::Enter, Modifiers::SHIFT), &plain),
            Some(Binding::Enter)
        );
        let composing = Context {
            composing: true,
            ..context()
        };
        assert_eq!(
            bind(named(Named::Enter, Modifiers::empty()), &composing),
            None
        );
    }

    #[test]
    fn escape_and_tab_are_left_for_the_window() {
        let plain = context();
        assert_eq!(bind(named(Named::Escape, Modifiers::empty()), &plain), None);
        let tab = press(Key::Named(Named::Tab), Modifiers::empty(), Some("\t"));
        assert_eq!(bind(tab, &plain), None);
    }

    #[test]
    fn a_command_takes_tab_escape_backspace_and_enter_for_a_file() {
        let writing = |focus, wants_file, empty| Context {
            command: Some(Command {
                options: 2,
                focus,
                wants_file,
            }),
            empty,
            ..context()
        };
        let key = |key, mods, context: &Context| act(bind(named(key, mods), context));
        let none = Modifiers::empty();
        let open = writing(0, false, false);
        assert_eq!(
            key(Named::Tab, none, &open),
            Some(Act::Command(CommandKey::Tab(1)))
        );
        assert_eq!(
            key(Named::Tab, Modifiers::SHIFT, &open),
            Some(Act::Command(CommandKey::Tab(-1)))
        );
        assert_eq!(
            key(Named::Escape, none, &open),
            Some(Act::Command(CommandKey::Close { back: false }))
        );
        assert_eq!(key(Named::Backspace, none, &open), None, "text to delete");
        assert_eq!(
            key(Named::Backspace, none, &writing(0, false, true)),
            Some(Act::Command(CommandKey::Close { back: true }))
        );
        assert_eq!(key(Named::Backspace, none, &writing(1, false, true)), None);
        assert_eq!(
            key(Named::Enter, none, &writing(1, true, true)),
            Some(Act::Command(CommandKey::File(1)))
        );
        assert_eq!(key(Named::Enter, none, &open), Some(Act::Send));
        let picking = Context {
            picking: true,
            ..context()
        };
        assert_eq!(
            key(Named::Escape, none, &picking),
            Some(Act::Command(CommandKey::Close { back: false }))
        );
    }

    #[test]
    fn up_edits_the_last_message_only_from_an_empty_field() {
        let empty = Context {
            empty: true,
            ..context()
        };
        let up = |mods, context: &Context| act(bind(named(Named::ArrowUp, mods), context));
        assert_eq!(up(Modifiers::empty(), &empty), Some(Act::Up));
        assert_eq!(up(Modifiers::empty(), &context()), None);
        assert_eq!(up(Modifiers::SHIFT, &empty), None);
        assert_eq!(up(Modifiers::ALT, &context()), Some(Act::Browse));
    }

    #[test]
    fn ctrl_letters_go_by_their_latin_place() {
        let plain = context();
        let ctrl = |letter: &str, code, mods| {
            let mut key = press(Key::Character(letter.into()), mods, None);
            key.physical_key = Physical::Code(code);
            bind(key, &plain)
        };
        let control = Modifiers::CTRL;
        assert_eq!(act(ctrl("v", Code::KeyV, control)), Some(Act::Paste));
        assert_eq!(
            act(ctrl("м", Code::KeyV, control)),
            Some(Act::Paste),
            "Russian layout"
        );
        assert_eq!(
            act(ctrl("g", Code::KeyG, control)),
            Some(Act::Expressions(Expressions::Gifs))
        );
        assert_eq!(
            act(ctrl("e", Code::KeyE, control)),
            Some(Act::Expressions(Expressions::Emoji))
        );
        assert_eq!(
            ctrl("z", Code::KeyZ, control | Modifiers::SHIFT),
            Some(Binding::Redo)
        );
        assert_eq!(ctrl("z", Code::KeyZ, control), Some(Binding::Undo));
        let without = Context {
            can: Can::default(),
            ..context()
        };
        let mut paste = press(Key::Character("v".into()), control, None);
        paste.physical_key = Physical::Code(Code::KeyV);
        assert_eq!(bind(paste, &without), Some(Binding::Paste), "pastes itself");
    }
}
