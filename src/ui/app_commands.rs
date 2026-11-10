//! Apps' commands in the window: the `/` picker over the composer, the
//! values an option may take, and the pills of the command being written.
//! What they show is the core's (`application/slash.rs`); this fills the
//! window's lists from it and binds their callbacks. The "Apps" submenus are
//! phase 2's.

use super::*;

/// Binds the command picker's and the "Apps" submenus' callbacks, and gives
/// the window the lists `show_app_commands` fills.
pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    window.set_command_choices(ModelRc::new(VecModel::<CommandChoice>::default()));
    window.set_command_options(ModelRc::new(VecModel::<CommandOptionPill>::default()));
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_slash_query(move |query| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::SlashQuery(query.to_string()));
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_pick_command(move |index| {
        let Some(window) = weak.upgrade() else { return };
        window.set_command_choice_index(0);
        state
            .borrow_mut()
            .action(&window, Action::PickCommand(index.max(0) as usize));
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_set_command_option(move |index, value| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(
                &window,
                Action::SetCommandOption {
                    index: index.max(0) as usize,
                    value: value.to_string(),
                },
            );
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_focus_command_option(move |index| {
        let Some(window) = weak.upgrade() else { return };
        window.set_command_choice_index(0);
        state
            .borrow_mut()
            .action(&window, Action::FocusCommandOption(index.max(0) as usize));
        // The text is the option's now; the cursor goes to its end.
        window.invoke_place_composer_caret(1_000_000);
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_close_command(move |back| {
        let Some(window) = weak.upgrade() else { return };
        state
            .borrow_mut()
            .action(&window, Action::CloseCommand(back));
        if back {
            window.invoke_place_composer_caret(1_000_000);
        }
    });
    // A file option's file, from the same dialog as the composer's files.
    let state = adapter.clone();
    window.on_command_file(move |index| {
        let index = index.max(0) as usize;
        let weak = Rc::downgrade(&state);
        state.borrow_mut().dialog(async move {
            let file = rfd::AsyncFileDialog::new()
                .set_title("Choose a file")
                .pick_file()
                .await;
            if let (Some(file), Some(adapter)) = (file, weak.upgrade()) {
                let window = adapter.borrow().window.upgrade();
                if let Some(window) = window {
                    adapter.borrow_mut().action(
                        &window,
                        Action::SetCommandOption {
                            index,
                            value: file.path().to_string_lossy().into_owned(),
                        },
                    );
                }
            }
        });
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_run_app_command(move |command, target| {
        if let Some(window) = weak.upgrade() {
            state.borrow_mut().action(
                &window,
                Action::RunAppCommand {
                    command: command.to_string(),
                    target: (!target.is_empty()).then(|| target.to_string()),
                },
            );
        }
    });
    let state = adapter.clone();
    let weak = window.as_weak();
    window.on_dismiss_ephemeral(move |item| {
        if let Some(window) = weak.upgrade() {
            state
                .borrow_mut()
                .action(&window, Action::DismissEphemeral(item.to_string()));
        }
    });
}

impl Adapter {
    /// Fills the list over the composer and the command's pills from the
    /// core's state, asking for the app icons and avatars it lacks.
    pub(super) fn show_app_commands(&mut self, window: &YapfastWindow) {
        let state = &self.app.state;
        let rows = state.command_rows();
        let heading = state.command_heading();
        let wanted: Vec<String> = rows
            .iter()
            .filter_map(|row| row.icon.clone())
            .filter(|icon| !state.pictures.contains_key(icon))
            .collect();
        let choices: Vec<CommandChoice> = rows
            .iter()
            .map(|row| CommandChoice {
                app: row.app.as_str().into(),
                icon: cached_picture(&mut self.pictures, state, row.icon.as_deref()),
                name: row.name.as_str().into(),
                description: row.description.as_str().into(),
            })
            .collect();
        let command = state.command_draft();
        let pills: Vec<CommandOptionPill> = command
            .map(|command| {
                command
                    .options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| CommandOptionPill {
                        name: option.name.as_str().into(),
                        description: option.description.as_str().into(),
                        value: option.shown().into(),
                        required: option.required,
                        error: state
                            .option_error(command, index)
                            .unwrap_or_default()
                            .into(),
                        file: option.is_attachment(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        window.set_command_name(command.map_or_else(SharedString::default, |command| {
            command.name.as_str().into()
        }));
        window.set_command_focus(command.map_or(0, |command| command.focused as i32));
        window.set_command_rows_are_commands(command.is_none());
        // The picker shows while `/` is typed, even with nothing to list
        // yet; an option's values only when there are some.
        window.set_command_picker_open(if command.is_some() {
            !choices.is_empty()
        } else {
            !heading.is_empty()
        });
        window.set_command_heading(heading.into());
        if window.get_command_choice_index() >= choices.len() as i32 {
            window.set_command_choice_index(0);
        }
        // The window holds the models `bind` gave it.
        let rows = window.get_command_choices();
        if let Some(model) = rows.as_any().downcast_ref::<VecModel<CommandChoice>>() {
            update_model(model, choices);
        }
        let rows = window.get_command_options();
        if let Some(model) = rows.as_any().downcast_ref::<VecModel<CommandOptionPill>>() {
            update_model(model, pills);
        }
        // Asked once each, and shown as they come; nothing to refresh now.
        if !wanted.is_empty() {
            let effects = self.app.dispatch(Action::LoadPictures(wanted));
            self.execute(effects);
        }
    }
}
