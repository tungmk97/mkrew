//! This computer's window as the launcher (spec 2026-10-29 §4.5, §6.2): the machine windows it
//! starts, the states they report, and the Settings → Machines page that lists, opens, removes
//! and adds them. The tray and palette entries read the same list.

use crate::*;
use mkrew_ui::machines::{Field, Invalid};
use mkrew_ui::settings_dialog::{Effect, Focus, Tab};

/// Key of the Machines page's own error in the dialog's `errors` (a file that cannot be
/// written).
const PAGE_ERROR: &str = "machines";

/// Longest state text on a tray menu line, in characters.
const TRAY_LABEL_CHARS: usize = 60;

/// The dot before a state: green connected, amber connecting, red not reachable or another
/// version, muted for a closed window. The word always comes with it (§11).
pub(crate) fn state_color(theme: &Theme, state: Option<&MachineState>) -> u32 {
    match state {
        Some(MachineState::Connected) => theme.status_done,
        Some(MachineState::Unreachable(_) | MachineState::Version(_)) => theme.status_exited,
        Some(MachineState::Closed) => theme.text_muted,
        _ => theme.status_needs_input,
    }
}

impl Shell {
    /// Start of Mkrew: a window for every machine of the list (not for one that runs already).
    pub(crate) fn start_machines(&mut self) {
        let (Some((launcher, _)), Ok(machines)) = (self.launcher.as_mut(), &self.machines) else {
            return;
        };
        for machine in &machines.list {
            launcher.start(&machine.name);
        }
    }

    /// Ends every machine window this one started (Quit, update install).
    pub(crate) fn close_machines(&mut self) {
        if let Some((launcher, _)) = self.launcher.as_mut() {
            launcher.close_all();
        }
    }

    /// Takes the state lines the children wrote since the last tick.
    pub(crate) fn poll_machines(&mut self, cx: &mut Context<'_, Self>) {
        let Some((_, states)) = &self.launcher else {
            return;
        };
        let mut changed = false;
        while let Ok((name, state)) = states.try_recv() {
            changed |= self.machine_states.insert(name, state.clone()) != Some(state);
        }
        if changed {
            self.sync_tray(true, cx);
            cx.notify();
        }
    }

    /// What the page and the tray say about `name`: a window this launcher did not start (or
    /// never) counts as closed; Open raises or starts it.
    fn machine_state_label(&self, name: &str) -> String {
        (self.machine_states.get(name)).map_or_else(|| MachineState::Closed.label(), |s| s.label())
    }

    /// The tray menu's machine lines: name and state label (a long reason cut: a menu line is
    /// one line), in list order.
    pub(crate) fn tray_machines(&self) -> Vec<(String, String)> {
        let Ok(machines) = &self.machines else {
            return Vec::new();
        };
        (machines.list.iter())
            .map(|machine| {
                let label = self.machine_state_label(&machine.name);
                let label = match label.char_indices().nth(TRAY_LABEL_CHARS) {
                    Some((end, _)) => format!("{}…", &label[..end]),
                    None => label,
                };
                (machine.name.clone(), label)
            })
            .collect()
    }

    /// The palette's "Machine: Open <name>" entries.
    pub(crate) fn machine_names(&self) -> Vec<String> {
        (self.machines.iter())
            .flat_map(|machines| machines.list.iter().map(|machine| machine.name.clone()))
            .collect()
    }

    /// Opens or raises the window of the machine at `index` of the list (tray, palette).
    pub(crate) fn open_machine_at(&mut self, index: usize) {
        let name = (self.machines.as_ref().ok())
            .and_then(|machines| machines.list.get(index))
            .map(|machine| machine.name.clone());
        if let Some(name) = name {
            self.open_machine(&name);
        }
    }

    fn open_machine(&mut self, name: &str) {
        if let Some((launcher, _)) = self.launcher.as_mut() {
            launcher.open(name);
        }
    }

    /// The palette's "Machine: Add…": Settings on the Machines page with the form open (the
    /// error alone when the file is broken).
    pub(crate) fn open_machine_add(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.open_settings_dialog(window, cx);
        let editable = self.machines.is_ok();
        if let Some(dialog) = &mut self.settings_dialog {
            if editable {
                dialog.open_add();
            } else {
                let _ = dialog.set_tab(Tab::Machines);
            }
        }
    }

    /// The file as it is now: another editor may have changed it since the last read. A broken
    /// one is kept as the error and never written.
    fn reload_machines(&mut self) -> Option<Machines> {
        match Machines::load(&self.config_dir) {
            Ok(machines) => Some(machines),
            Err(text) => {
                self.machines = Err(text);
                None
            }
        }
    }

    /// The Add form's Save: validate, write the file, open the machine's window.
    pub(crate) fn add_machine(&mut self, machine: Machine, cx: &mut Context<'_, Self>) {
        let outcome = match self.reload_machines() {
            None => Err(None),
            Some(mut machines) => match machines.add(machine) {
                Err(invalid) => Err(Some(invalid)),
                Ok(()) => machines
                    .save(&self.config_dir)
                    .map(|()| machines)
                    .map_err(|error| {
                        Some(Invalid {
                            field: Field::Name,
                            message: format!("Cannot write machines.toml: {error}"),
                        })
                    }),
            },
        };
        match outcome {
            Ok(machines) => {
                let name = machines.list.last().map(|machine| machine.name.clone());
                self.machines = Ok(machines);
                if let Some(dialog) = &mut self.settings_dialog {
                    dialog.errors.remove(PAGE_ERROR);
                    dialog.close_add();
                }
                if let Some(name) = name {
                    self.open_machine(&name);
                }
                self.sync_tray(true, cx);
            }
            Err(Some(invalid)) => {
                if let Some(dialog) = &mut self.settings_dialog {
                    dialog.add_refused(invalid);
                }
            }
            // The file turned out broken: the page shows it now, without the form.
            Err(None) => {
                if let Some(dialog) = &mut self.settings_dialog {
                    dialog.close_add();
                }
            }
        }
        cx.notify();
    }

    /// Remove: asks, then ends that machine's window and deletes its line.
    fn ask_remove_machine(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let answer = window.prompt(
            PromptLevel::Critical,
            &format!("Remove “{name}”?"),
            Some(
                "Its window closes and its line is deleted from machines.toml. \
                 The other computer and its agents are not touched.",
            ),
            &workspace_menu::REMOVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if task_dialog::confirms_delete(answer.await.ok()) {
                let _ = this.update(cx, |shell, cx| shell.remove_machine(&name, cx));
            }
        })
        .detach();
    }

    fn remove_machine(&mut self, name: &str, cx: &mut Context<'_, Self>) {
        let Some(mut machines) = self.reload_machines() else {
            cx.notify();
            return;
        };
        machines.remove(name);
        match machines.save(&self.config_dir) {
            Ok(()) => {
                if let Some((launcher, _)) = self.launcher.as_mut() {
                    launcher.close(name);
                }
                self.machine_states.remove(name);
                self.machines = Ok(machines);
                if let Some(dialog) = &mut self.settings_dialog {
                    dialog.errors.remove(PAGE_ERROR);
                }
                self.sync_tray(true, cx);
            }
            Err(error) => {
                if let Some(dialog) = &mut self.settings_dialog {
                    dialog.errors.insert(
                        PAGE_ERROR.into(),
                        format!("Cannot write machines.toml: {error}"),
                    );
                }
            }
        }
        cx.notify();
    }

    /// The body of the Machines tab (spec §6.2).
    pub(crate) fn render_machines_page(
        &self,
        dialog: &SettingsDialog,
        typing: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        if self.target.is_machine() {
            return vec![
                meta_line(
                    theme,
                    "Machines are managed in This computer's window.".into(),
                )
                .into_any_element(),
            ];
        }
        let machines = match &self.machines {
            Ok(machines) => machines,
            Err(text) => return vec![self.machines_error(text.clone(), theme)],
        };
        let mut page: Vec<gpui::AnyElement> = Vec::new();
        if let Some(error) = dialog.errors.get(PAGE_ERROR) {
            page.push(self.machines_error(error.clone(), theme));
        }
        if machines.list.is_empty() && dialog.add.is_none() {
            page.push(
                meta_line(
                    theme,
                    "No machines yet. Add one to work on its agents from this computer.".into(),
                )
                .into_any_element(),
            );
        }
        page.extend((machines.list.iter()).map(|machine| self.machine_row(machine, theme, cx)));
        page.push(match &dialog.add {
            Some(form) => self.add_machine_form(dialog, form, typing, theme, cx),
            None => div()
                .child(
                    button(
                        "machine-add",
                        "Add machine…",
                        theme,
                        machines.list.is_empty(),
                    )
                    .focus_stop(theme)
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.settings_do(
                            |dialog, _| {
                                dialog.open_add();
                                Effect::None
                            },
                            window,
                            cx,
                        );
                    })),
                )
                .into_any_element(),
        });
        page.push(
            meta_line(
                theme,
                "The other computer needs Mkrew, an SSH server and this computer's SSH key.".into(),
            )
            .into_any_element(),
        );
        page
    }

    /// The broken-file line: its text names the file and the line; nothing can be changed.
    fn machines_error(&self, text: String, theme: &Theme) -> gpui::AnyElement {
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .p(px(space::S))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.surface_raised))
            .text_size(font_size::META)
            .child(div().text_color(rgb(theme.status_exited)).child("✕"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(rgb(theme.text))
                    .child(text),
            )
            .into_any_element()
    }

    /// One machine: name, host (and `mk`), state with its dot, Open and Remove.
    fn machine_row(
        &self,
        machine: &Machine,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let state = self.machine_states.get(&machine.name);
        let color = state_color(theme, state);
        let source = match &machine.mk {
            Some(mk) => format!("{} · {mk}", machine.host),
            None => machine.host.clone(),
        };
        let (open_name, remove_name) = (machine.name.clone(), machine.name.clone());
        div()
            .id(SharedString::from(format!("machine-row-{}", machine.name)))
            .flex()
            .items_center()
            .gap(px(space::M))
            .p(px(space::S))
            .rounded(px(radius::MEDIUM))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(
                        div()
                            .text_size(font_size::BODY)
                            .text_color(rgb(theme.text))
                            .child(machine.name.clone()),
                    )
                    .child(meta_line(theme, source.into()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::S))
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(status_dot(color, false))
                            .child(
                                div()
                                    .min_w_0()
                                    .child(self.machine_state_label(&machine.name)),
                            ),
                    ),
            )
            .child(
                secondary_button("machine-open", "Open", theme)
                    .focusable_bordered(theme)
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.open_machine(&open_name);
                        cx.notify();
                    })),
            )
            .child(
                danger_button("machine-remove", "Remove", theme)
                    .focus_stop(theme)
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.ask_remove_machine(remove_name.clone(), window, cx);
                    })),
            )
            .into_any_element()
    }

    /// Name, Host and mk path with the refusal under its field, Save and Cancel.
    fn add_machine_form(
        &self,
        dialog: &SettingsDialog,
        form: &mkrew_ui::settings_dialog::AddForm,
        typing: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let fields = [
            (Field::Name, "Name", "machine-name", "Home PC"),
            (
                Field::Host,
                "Host",
                "machine-host",
                "user@host or an SSH alias",
            ),
            (Field::Mk, "mk path (optional)", "machine-mk", "mk"),
        ];
        div()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::S))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.surface_raised))
            .children(fields.into_iter().map(|(field, label, id, hint)| {
                let focused = dialog.focus == Focus::Add(field) && typing;
                let refusal = form
                    .invalid
                    .as_ref()
                    .filter(|invalid| invalid.field == field);
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(meta_line(theme, label.into()))
                    .child(
                        caret_field(
                            id,
                            form.input(field),
                            hint,
                            focused,
                            theme,
                            cx,
                            move |shell, caret| {
                                let form =
                                    (shell.settings_dialog.as_mut()).and_then(|d| d.add.as_mut());
                                if let Some(form) = form {
                                    form.input_mut(field).click(
                                        caret.at,
                                        caret.extend,
                                        caret.clicks,
                                    );
                                }
                            },
                        )
                        .on_click(cx.listener(
                            move |shell, _, window, cx| {
                                shell.settings_do(
                                    |d, keymap| d.click(Focus::Add(field), keymap),
                                    window,
                                    cx,
                                );
                            },
                        )),
                    )
                    .children(refusal.map(|invalid| {
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.status_exited))
                            .child(invalid.message.clone())
                    }))
            }))
            .child(
                div()
                    .flex()
                    .gap(px(space::S))
                    .child(
                        button("machine-save", "Save", theme, true)
                            .focus_stop(theme)
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.settings_do(|d, _| d.submit_add(), window, cx);
                            })),
                    )
                    .child(
                        secondary_button("machine-cancel", "Cancel", theme)
                            .focusable_bordered(theme)
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.settings_do(
                                    |d, _| {
                                        d.close_add();
                                        Effect::None
                                    },
                                    window,
                                    cx,
                                );
                            })),
                    ),
            )
            .into_any_element()
    }
}
