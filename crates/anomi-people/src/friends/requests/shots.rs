//! The Requests screen's shots: the whole window with the screen open,
//! beside the golden screenshot it matches. Task P02's.

#[cfg(feature = "shots")]
pub use shown::shots;

#[cfg(feature = "shots")]
mod shown {
    use anomi_vm::Vm;
    use anomi_vm::conversation::{Body, Conversation};
    use anomi_vm::fixtures::people::requests;
    use anomi_vm::fixtures::{header, shell};
    use anomi_vm::people::requests::Requests;

    use crate::shots::{GOLDEN, Shot};

    /// The window on Requests, with `screen`'s rows.
    fn open(screen: Requests) -> Vm {
        let mut vm = shell::dm_with(|| Conversation {
            body: Body::Requests,
            ..Conversation::default()
        });
        vm.header = header::requests();
        if let anomi_vm::sidebar::Place::Dms(dms) = &mut vm.sidebar.place {
            dms.requests_selected = true;
            for row in &mut dms.rows {
                row.selected = false;
            }
        }
        vm.people.requests = screen;
        vm
    }

    fn waiting() -> Vm {
        open(requests::waiting())
    }

    fn spam_open() -> Vm {
        open(requests::spam_open())
    }

    fn everything() -> Vm {
        open(requests::everything())
    }

    fn none() -> Vm {
        open(requests::none())
    }

    pub fn shots() -> Vec<Shot> {
        let shot = |name, vm: fn() -> Vm| Shot {
            area: "friends",
            name,
            size: GOLDEN,
            theme: "bonfire",
            vm,
        };
        vec![
            shot("requests", waiting),
            shot("requests_spam_open", spam_open),
            shot("requests_every_row", everything),
            shot("requests_none", none),
        ]
    }
}
