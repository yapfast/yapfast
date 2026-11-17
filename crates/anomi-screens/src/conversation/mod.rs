//! The conversation column: the header, and below it what the open place
//! shows (the timeline and the composer, Friends, a forum, a call), which
//! come with their tasks (docs/iced/tasks/). The skeleton shows the header
//! and an empty body. The Slint side: the conversation column of
//! `ui/yapfast.slint`. Task N01's, with the shell.

use iced::Fill;
use iced::widget::{column, container, rule, space, text};

use anomi_theme::catalog;
use anomi_theme::prelude::*;
use anomi_vm::{Intent, Vm};
use yapfast_lang::tr;

use crate::header;

pub fn view(vm: &Vm, motion: Motion) -> impl View<Intent> + '_ {
    let note = match (&vm.shell.notice, vm.shell.connecting) {
        (Some(notice), _) => text(notice.as_str()).style(catalog::text::danger).boxed(),
        (None, true) => text(tr!("Connecting..."))
            .style(catalog::text::muted)
            .boxed(),
        (None, false) => space().boxed(),
    };
    let body = container(note).center(Fill);
    container(column![
        header::view(&vm.header, motion).map(Intent::from),
        rule::horizontal(1).style(catalog::rule::outline_chat),
        body
    ])
    .width(Fill)
    .height(Fill)
    .style(catalog::container::chat)
}
