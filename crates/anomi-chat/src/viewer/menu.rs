//! The menu of a right click on the picture: `ViewerMenu`
//! (`ui/messages.slint:2563`). It is the window's context menu (`vm.menu`),
//! which the app fills on `Intent::Menu` with these rows; the shell places it
//! in window coordinates over the viewer.

use yapfast_lang::tr;

use anomi_vm::Intent;
use anomi_vm::chat::viewer::{self, Viewer};
use anomi_vm::icon::Icon;
use anomi_vm::menu::Row;

/// The rows of `viewer`'s menu; `video` for the video viewer's, which has no
/// "Copy Image". Without an address only "Save"; a link's preview has no
/// file to save or copy, so only its address's two.
pub fn rows(viewer: &Viewer, video: bool) -> Vec<Row> {
    let ask = |intent: viewer::Intent| Intent::from(intent);
    let save = Row::new(
        if video {
            tr!("Save Video")
        } else {
            tr!("Save Image")
        },
        ask(viewer::Intent::Save {
            item: viewer.item.clone(),
            index: viewer.index,
        }),
    )
    .icon(Icon::Download);
    let Some(link) = &viewer.link else {
        return vec![save];
    };
    let copy_link = Row::new(
        tr!("Copy Link"),
        ask(viewer::Intent::CopyLink(link.clone())),
    )
    .icon(Icon::Link);
    let open_link = Row::new(
        tr!("Open Link"),
        ask(viewer::Intent::OpenLink(link.clone())),
    )
    .icon(Icon::ExternalLink);
    if viewer.preview {
        vec![copy_link, open_link]
    } else if video {
        vec![save, copy_link, open_link]
    } else {
        let copy_image = Row::new(
            tr!("Copy Image"),
            ask(viewer::Intent::CopyImage {
                item: viewer.item.clone(),
                index: viewer.index,
            }),
        )
        .icon(Icon::Image);
        vec![copy_image, save, copy_link, open_link]
    }
}
