//! The rail's shots: the whole window with the rail as the golden
//! screenshots have it, and the states they don't show (folders).

#[cfg(feature = "shots")]
pub fn shots() -> Vec<crate::shots::Shot> {
    use crate::shots::{GOLDEN, Shot};
    use anomi_vm::fixtures::{rail, shell};

    let shot = |area, name, vm| Shot {
        area,
        name,
        size: GOLDEN,
        theme: "bonfire",
        vm,
    };
    vec![
        // `server/general_unread_divider.png`: the rail without DMs.
        shot("rail", "general_unread_divider", shell::server_home),
        shot("rail", "unread_dms", rail::unread_dms_window),
        // `voice/joined_sidebar_dock.png`'s speaker on the server's icon.
        shot("rail", "joined_sidebar_dock", rail::call_window),
        shot("rail", "folders", rail::folders_window),
        // `menu/server_icon_context.png`.
        shot("menu", "server_icon_context", rail::server_menu),
    ]
}
