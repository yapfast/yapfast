//! What an empty field says while an app's command is written, in place of
//! the room's placeholder (`ui/composer-field.slint:99`-`:102`): that Enter
//! runs a command without options, that Enter picks a file for a file
//! option, or else what the option the text writes wants.

use std::borrow::Cow;

use yapfast_lang::tr;

/// The option of an app's command that the text writes, as the field
/// speaks of it (`focused-pill`, `:71`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Writing<'a> {
    /// The option's name.
    pub name: &'a str,
    /// What it wants, as its app describes it.
    pub description: &'a str,
    /// It takes a file.
    pub file: bool,
}

/// The placeholder while `/command` is written: `writing` is the option the
/// text writes, `None` when the command has no options. Give it to
/// [`placeholder`](super::Composer::placeholder).
pub fn command_placeholder<'a>(command: &str, writing: Option<Writing<'a>>) -> Cow<'a, str> {
    match writing {
        None => Cow::Owned(tr!("Press Enter to run /{}", command)),
        Some(Writing {
            name, file: true, ..
        }) => Cow::Owned(tr!("Press Enter to choose a file for {}", name)),
        Some(Writing { description, .. }) => Cow::Borrowed(description),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_says_what_enter_does_or_what_the_option_wants() {
        assert_eq!(
            command_placeholder("roll", None),
            "Press Enter to run /roll"
        );
        let file = Writing {
            name: "picture",
            description: "A picture to post",
            file: true,
        };
        assert_eq!(
            command_placeholder("post", Some(file)),
            "Press Enter to choose a file for picture"
        );
        let text = Writing {
            file: false,
            ..file
        };
        assert_eq!(command_placeholder("post", Some(text)), "A picture to post");
    }
}
