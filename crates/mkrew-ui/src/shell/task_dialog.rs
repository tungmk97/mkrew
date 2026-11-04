//! The task dialog: create, edit, delete.

use crate::*;

impl Shell {
    /// Shows the modal task dialog (replacing any open one) and lists the agent profiles for
    /// its picker.
    pub(crate) fn open_task_dialog(
        &mut self,
        dialog: TaskDialog,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.workspace_menu = None;
        self.undo_dialog = None;
        self.task_dialog = Some(dialog);
        self.dialog_serial += 1;
        let serial = self.dialog_serial;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        let listed = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let agents = match listed.await {
                Ok(listed) => Agents::Loaded(listed.profiles),
                Err(error) => Agents::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if let Some(dialog) = shell.open_dialog(serial) {
                    dialog.agents = agents;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The open task dialog, if it is still the one opened as `serial`.
    pub(crate) fn open_dialog(&mut self, serial: u64) -> Option<&mut TaskDialog> {
        self.task_dialog
            .as_mut()
            .filter(|_| self.dialog_serial == serial)
    }

    pub(crate) fn close_task_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.task_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Keys typed in the task dialog ([`Shell::prompt_key_down`]) edit the focused field;
    /// Tab / Shift+Tab move between the title and the prompt, then through the buttons; Enter
    /// (title) or Ctrl+Enter saves, Esc cancels; `@` in the prompt picks a file, as in the
    /// composer.
    pub(crate) fn task_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let edge = (self.task_dialog.as_ref())
            .is_some_and(|dialog| dialog.at_edge(!event.keystroke.modifiers.shift));
        if self.dialog_nav(event, edge, window, cx, |shell, forward| {
            if let Some(dialog) = shell.task_dialog.as_mut() {
                dialog.enter(forward);
            }
        }) {
            return;
        }
        cx.stop_propagation();
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        if self.task_dialog.is_none() {
            return;
        }
        // A control has the keyboard: the hidden field takes nothing but Esc / Ctrl+Enter.
        if self.dialog_control_focused(window)
            && !matches!(key, DialogKey::Cancel | DialogKey::Save)
        {
            return;
        }
        if self.prompt_key_down(composer::Target::TaskDialog, event, window, cx) {
            return;
        }
        match key {
            DialogKey::Cancel => self.close_task_dialog(window, cx),
            // Ctrl+Enter, or Enter in the title.
            _ => self.save_task(window, cx),
        }
    }

    /// `task.create` / `task.update` for the dialog's draft; the dialog closes once it is
    /// saved and shows the error otherwise.
    pub(crate) fn save_task(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connected_or_toast(cx) else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(dialog) = self.task_dialog.as_mut().filter(|dialog| !dialog.busy) else {
            return;
        };
        // Diff against the task as it is now (`mk` or a run may have changed it meanwhile).
        if let Some(task) = dialog.task_id().and_then(|id| self.board.task(id)) {
            dialog.mode = task_dialog::Mode::Edit(Box::new(task.clone()));
        }
        let request = match dialog.save_request() {
            Ok(Some(request)) => request,
            Ok(None) => return self.close_task_dialog(window, cx),
            Err(invalid) => {
                dialog.error = Some(invalid.to_owned());
                cx.notify();
                return;
            }
        };
        dialog.busy = true;
        dialog.error = None;
        cx.notify();
        let saved = connection.call::<Task>(request);
        cx.spawn_in(window, async move |this, cx| {
            let saved = saved.await;
            let _ = this.update_in(cx, |shell, window, cx| match saved {
                Ok(task) => {
                    // Shown at once, without waiting for the `task.updated` echo.
                    shell.board.apply(&Event::TaskUpdated(task));
                    if shell.open_dialog(serial).is_some() {
                        shell.close_task_dialog(window, cx);
                    }
                }
                Err(error) => {
                    tracing::warn!("saving a task failed: {error:#}");
                    if let Some(dialog) = shell.open_dialog(serial) {
                        dialog.busy = false;
                        dialog.error = Some(format!("Cannot save the task: {error:#}"));
                    } else {
                        shell.toast_error(format!("Cannot save the task: {error:#}"), cx);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Asks, then `task.delete`s the task being edited. The daemon refuses while its session
    /// still runs; the dialog shows why.
    pub(crate) fn delete_task(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connected_or_toast(cx) else {
            return;
        };
        let Some(task_id) = self
            .task_dialog
            .as_ref()
            .filter(|dialog| !dialog.busy)
            .and_then(TaskDialog::task_id)
        else {
            return;
        };
        let serial = self.dialog_serial;
        let answer = window.prompt(
            PromptLevel::Critical,
            "Delete this task?",
            Some("The card and its prompt are removed. Its past sessions stay in the sidebar."),
            &task_dialog::DELETE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if !task_dialog::confirms_delete(answer.await.ok()) {
                return;
            }
            let deleted = this.update(cx, |shell, cx| {
                let dialog = shell.open_dialog(serial).filter(|dialog| !dialog.busy)?;
                dialog.busy = true;
                dialog.error = None;
                cx.notify();
                Some(connection.request(Request::TaskDelete(TaskRef { task_id })))
            });
            let Ok(Some(deleted)) = deleted else {
                return;
            };
            let failure = match deleted.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| match failure {
                None => {
                    if shell.open_dialog(serial).is_some() {
                        shell.close_task_dialog(window, cx);
                    }
                }
                Some(error) => {
                    tracing::warn!(%task_id, "task.delete failed: {error}");
                    if let Some(dialog) = shell.open_dialog(serial) {
                        dialog.busy = false;
                        dialog.error = Some(format!("Cannot delete the task: {error}"));
                    } else {
                        shell.toast_error(format!("Cannot delete the task: {error}"), cx);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// "New task": the primary action of the empty board (with its shortcut beside it), else
    /// quiet with the shortcut in its tooltip only (it fits the Todo column header). On the All
    /// workspaces board the tooltip names the workspace the task goes to (the active one).
    pub(crate) fn new_task_button(
        &self,
        project_id: Ulid,
        primary: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let keys = self.keymap.label(Command::NewTask);
        let tip = (self.sidebar.projects().iter())
            .find(|project| self.board_all && project.id == project_id)
            .map_or_else(
                || "New task".to_owned(),
                |project| format!("New task in {}", project.name),
            );
        button(
            "new-task",
            icon_text(Icon::Plus, "New task", button_fg(theme, primary)),
            theme,
            primary,
        )
        .flex_none()
        .tooltip(tooltip(theme, &tip, keys.clone()))
        .when(primary, |el| {
            el.child(
                div()
                    .text_size(font_size::META)
                    .opacity(opacity::SECONDARY)
                    .child(keys),
            )
        })
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.open_task_dialog(TaskDialog::new(project_id), window, cx);
        }))
    }

    /// The modal new / edit task dialog (a stray click on its shade never throws a draft
    /// away; Esc or Cancel does).
    pub(crate) fn render_task_dialog(
        &self,
        dialog: &TaskDialog,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let editing = dialog.task_id().is_some();
        let default_agent = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == dialog.project_id())
            .and_then(|project| project.default_agent.as_deref());
        let label = |text: &'static str| {
            div()
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        // A button holding the keyboard means no field is: only one focus indicator at a time.
        let text_focused = self.dialog_focus.is_focused(window);
        let rewriting = dialog.prompt.rewriting();
        let field = |id: &'static str, which: Field, placeholder: &'static str| {
            let input = match which {
                Field::Title => &dialog.title,
                Field::Prompt => dialog.prompt.text.input(),
            };
            // The prompt shows no caret while ✦ rewrites it (keys wait, like the composer's).
            let caret = dialog.field == which && text_focused;
            let caret = caret && !(which == Field::Prompt && rewriting);
            // A click focuses it and puts the caret under the pointer (drag / Shift+click select).
            caret_field(
                id,
                input,
                placeholder,
                caret,
                theme,
                cx,
                move |shell, caret| {
                    if let Some(dialog) = shell.task_dialog.as_mut() {
                        dialog.click(which, caret.at, caret.extend, caret.clicks);
                    }
                },
            )
        };
        let agents = match &dialog.agents {
            Agents::Loading => vec![label("Loading agents…").into_any_element()],
            Agents::Failed(error) => vec![
                notice(theme.text, format!("✕ Cannot list the agents: {error}")).into_any_element(),
            ],
            Agents::Loaded(profiles) => {
                let choices = dialog.agent_choices(profiles, default_agent);
                let len = choices.len();
                choices
                    .into_iter()
                    .enumerate()
                    .map(|(index, choice)| {
                        let hover = theme.surface_raised;
                        let agent = choice.id;
                        div()
                            .id(SharedString::from(format!("task-agent-{agent}")))
                            .px(px(space::S))
                            .py(px(space::XS))
                            .rounded(px(radius::SMALL))
                            .border(px(theme::BORDER_WIDTH))
                            .cursor_pointer()
                            .text_size(font_size::BODY)
                            .when(choice.selected, |el| {
                                el.border_color(rgb(theme.accent))
                                    .text_color(rgb(theme.text))
                                    .font_weight(FontWeight(weight::MEDIUM))
                                    .child(format!("✓ {}", choice.label))
                            })
                            .when(!choice.selected, |el| {
                                el.border_color(rgb(theme.border))
                                    .text_color(rgb(theme.text_muted))
                                    .hover(move |style| style.bg(rgb(hover)))
                                    .active(|style| style.opacity(opacity::ACTIVE))
                                    .child(choice.label)
                            })
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                if let Some(dialog) = shell.task_dialog.as_mut() {
                                    dialog.choose_agent(&agent);
                                }
                                // An open `/` picker lists the new agent's commands (§20B.7).
                                shell.retarget_slash_picker(window, cx);
                                cx.notify();
                            }))
                            .focusable_bordered(theme)
                            .chip_arrows(index, len)
                            .into_any_element()
                    })
                    .collect()
            }
        };
        let hover = theme.surface_raised;
        let locked = dialog.worktree_locked();
        let worktree = div()
            .id("task-worktree")
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::XS))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .text_size(font_size::BODY)
            .tooltip(tooltip(
                theme,
                if locked && !dialog.busy {
                    "This task already has its worktree"
                } else {
                    "The agent works on its own branch; your checkout stays untouched"
                },
                "",
            ))
            .when(locked, |el| el.text_color(rgb(theme.text_muted)))
            .when(!locked, |el| {
                el.cursor_pointer()
                    .text_color(rgb(theme.text))
                    .hover(move |style| style.bg(rgb(hover)))
                    .active(|style| style.opacity(opacity::ACTIVE))
                    .on_click(cx.listener(|shell, _, _, cx| {
                        if let Some(dialog) = shell.task_dialog.as_mut() {
                            dialog.toggle_worktree();
                        }
                        cx.notify();
                    }))
                    .focus_stop(theme)
            })
            .child(
                div()
                    .size(px(space::M))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::SMALL))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.border))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.accent))
                    .child(if dialog.worktree { "✓" } else { "" }),
            )
            .child(div().flex_1().child("Run in a worktree"));
        let save_label = match (dialog.busy, editing) {
            (true, _) => "Saving…",
            (false, true) => "Save",
            (false, false) => "Create task",
        };
        // The accent ring while an OS file drag is over the Prompt box: state-driven
        // (`drop_hover`), because `drag_over` is hover-gated and gpui-pre 0.3.7 keeps keyboard
        // modality during OLE drags (see `Shell::drop_hover`). The tint keeps it visible on a
        // focused field, whose own border is already the accent.
        let ring = !dialog.busy
            && !self.prompt_box_covered()
            && matches!(self.drop_hover, Some((DropTarget::TaskPrompt, _)))
            && cx.has_active_drag();
        let ring_border: gpui::Hsla = if ring {
            rgb(theme.accent).into()
        } else {
            gpui::transparent_black()
        };
        let tint = theme.tint(theme.accent);
        let drop_ring = div()
            .absolute()
            .inset_0()
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(ring_border)
            .when(ring, move |el| el.bg(tint));
        let panel = div()
            .id("task-dialog")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            // A picker over it (library prompt, `@` file) has the keyboard meanwhile.
            .when(
                self.prompt_picker.is_none()
                    && self.quick_open.is_none()
                    && self.slash_picker.is_none(),
                |el| {
                    el.dialog_root(
                        &self.dialog_focus,
                        true,
                        cx.listener(Self::dialog_guard),
                        cx.listener(Self::task_dialog_key_down),
                    )
                    .relative()
                    .child(self.typing(
                        &self.dialog_focus,
                        Self::task_dialog_key_down,
                        cx,
                    ))
                },
            )
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(if editing { "Edit task" } else { "New task" }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(label("Title"))
                    .child(field("task-title", Field::Title, "What should be done?")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space::XS))
                            .child(div().flex_1().child(label("Prompt")))
                            // ✦ (§20B.1): the task's agent rewrites the prompt, as in the
                            // composer.
                            .when(!dialog.busy, |el| {
                                let sparkles =
                                    icon_text(Icon::Sparkles, "Improve", button_fg(theme, false));
                                let improve = if rewriting || dialog.prompt.text.is_blank() {
                                    disabled_button("task-enhance", sparkles, theme, false)
                                } else {
                                    button("task-enhance", sparkles, theme, false)
                                        .focus_stop(theme)
                                        .on_click(cx.listener(|shell, _, window, cx| {
                                            shell.enhance_prompt(
                                                composer::Target::TaskDialog,
                                                window,
                                                cx,
                                            );
                                        }))
                                };
                                el.child(improve.tooltip(tooltip(
                                    theme,
                                    "Improve this prompt with the agent (it may read the code)",
                                    "",
                                )))
                            })
                            .when(!dialog.busy, |el| {
                                el.child(
                                    button("task-from-prompt", "From prompt…", theme, false)
                                        .focus_stop(theme)
                                        .tooltip(tooltip(
                                            theme,
                                            "Add a prompt from the library",
                                            "",
                                        ))
                                        .on_click(cx.listener(|shell, _, window, cx| {
                                            shell.open_prompt_picker(
                                                composer::Target::TaskDialog,
                                                window,
                                                cx,
                                            );
                                        })),
                                )
                            }),
                    )
                    .child(
                        div()
                            .relative()
                            .on_drag_move::<ExternalPaths>(cx.listener(
                                |shell, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                                    let inside = !shell.prompt_box_covered()
                                        && event.bounds.contains(&event.event.position);
                                    let hovered = matches!(
                                        shell.drop_hover,
                                        Some((DropTarget::TaskPrompt, _))
                                    );
                                    if inside {
                                        shell.drop_hover =
                                            Some((DropTarget::TaskPrompt, event.drag(cx).clone()));
                                    } else if hovered {
                                        shell.drop_hover = None;
                                    }
                                    if inside != hovered {
                                        cx.notify();
                                    }
                                },
                            ))
                            .child(
                                field(
                                    "task-prompt",
                                    Field::Prompt,
                                    "Sent to the agent when the task runs.",
                                )
                                .h(px(theme::PROMPT_FIELD_HEIGHT))
                                .overflow_y_scroll()
                                .track_scroll(&self.prompt_scroll)
                                .when(rewriting, |el| el.opacity(opacity::DISABLED)),
                            )
                            .child(drop_catcher(DropTarget::TaskPrompt, cx))
                            .child(drop_ring),
                    )
                    // The composer's keys hold here too; the line stays so the dialog
                    // never jumps.
                    .child(
                        div()
                            .flex()
                            .gap(px(space::XS))
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .when(rewriting, |el| el.child("✦"))
                            .child(if rewriting {
                                "Improving the prompt…"
                            } else {
                                "@ for a file · paste an image or drop a file · Ctrl+Enter to save"
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(label("Agent"))
                    .child(div().flex().flex_wrap().gap(px(space::XS)).children(agents)),
            )
            .child(worktree)
            .children(dialog.error.clone().map(|error| {
                div()
                    .flex()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                    .child(div().text_color(rgb(theme.text)).child(error))
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .when(editing && dialog.busy, |el| {
                        el.child(disabled_button("task-delete", "Delete", theme, false))
                    })
                    .when(editing && !dialog.busy, |el| {
                        el.child(
                            button("task-delete", "Delete", theme, false)
                                .focus_stop(theme)
                                .tooltip(tooltip(theme, "Delete this task", ""))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.delete_task(window, cx);
                                })),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        button("task-cancel", "Cancel", theme, false)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Discard changes", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_task_dialog(window, cx);
                            })),
                    )
                    .child(if dialog.busy {
                        disabled_button("task-save", save_label, theme, true)
                    } else {
                        button("task-save", save_label, theme, true)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, save_label, "Ctrl+Enter"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.save_task(window, cx);
                            }))
                    }),
            );
        Self::modal_shade("task-dialog-shade", panel, cx)
    }
}
