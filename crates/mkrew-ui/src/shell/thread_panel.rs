//! The session panel's Thread section (ADR 0057): the shown session's transcript read like a
//! chat. A 1 s poll runs only while the panel shows an agent session with a native id.

use mkrew_ui::thread_view::{self, PageItem, ThreadFile};

use crate::*;

/// The reading of one shown session.
pub(crate) struct ThreadWatch {
    /// The session and native id being read; another pair starts a new watch.
    pub(crate) key: (Ulid, String),
    /// The 1 s poll; dropping it stops reading. It ends by itself after the session exited (one
    /// last read) or when the profile has no `[usage]`.
    _poll: gpui::Task<()>,
    /// Built off the UI thread on each change; `None` until a read found turns.
    pub(crate) page: Option<Rc<thread_view::Page>>,
    /// Expanded tool groups: (turn key = `page.first_turn` + turn index, item index).
    pub(crate) expanded: HashSet<(u64, usize)>,
    pub(crate) scroll: ScrollHandle,
}

impl ThreadWatch {
    /// The list shows its end (or nothing yet): new turns keep it there.
    fn at_bottom(&self) -> bool {
        self.page.is_none()
            || -f32::from(self.scroll.offset().y) >= f32::from(self.scroll.max_offset().y) - 1.0
    }
}

impl Shell {
    /// Starts, keeps or stops the Thread reading to match the panel (status tick). No work
    /// unless the panel is open on an agent with a native id.
    pub(crate) fn sync_thread(&mut self, cx: &mut Context<'_, Self>) {
        let session = self
            .panel_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let profile = session.map(|s| s.agent_profile.clone());
        let wanted = self.panel_shown() && self.panel_tab() == PanelTab::Session;
        match inspector::wanted_thread(wanted, session) {
            None => {
                if self.thread.take().is_some() {
                    cx.notify();
                }
            }
            Some(key) if self.thread.as_ref().is_some_and(|w| w.key == key) => {}
            Some(key) => {
                let poll = self.poll_thread(key.clone(), profile.unwrap_or_default(), cx);
                self.thread = Some(ThreadWatch {
                    key,
                    _poll: poll,
                    page: None,
                    expanded: HashSet::new(),
                    scroll: ScrollHandle::new(),
                });
                cx.notify();
            }
        }
    }

    fn poll_thread(
        &self,
        key: (Ulid, String),
        profile: String,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Task<()> {
        let agents_dir = self.config_dir.join("agents");
        cx.spawn(async move |this, cx| {
            let native = key.1.clone();
            let opened = cx
                .background_executor()
                .spawn(async move {
                    let format = mkrew_core::load_dir(&agents_dir)
                        .get(&profile)?
                        .usage
                        .as_ref()?
                        .format
                        .clone();
                    let locate = format.clone();
                    let locate = Box::new(move || mkrew_daemon::transcript_path(&locate, &native));
                    ThreadFile::new(&format, locate).ok()
                })
                .await;
            // No profile, no `[usage]` or an unusable format: the watch shows no section.
            let Some(mut file) = opened else { return };
            // ponytail: locating a transcript that does not exist yet walks the agent's
            // sessions folder once a second while shown; cache the miss if it shows in a profile.
            loop {
                let (back, page) = cx
                    .background_executor()
                    .spawn(async move {
                        let page = file.read().then(|| file.page());
                        (file, page)
                    })
                    .await;
                file = back;
                let keep_going = this.update(cx, |shell, cx| {
                    if let Some(watch) = shell.thread.as_mut().filter(|w| w.key == key)
                        && let Some(page) = page
                    {
                        let follow = watch.at_bottom();
                        watch.page = Some(Rc::new(page));
                        if follow {
                            watch.scroll.scroll_to_bottom();
                        }
                        cx.notify();
                    }
                    // The read above comes first: one last read after the exit.
                    shell.thread.as_ref().is_some_and(|w| w.key == key) && !shell.is_exited(key.0)
                });
                if !keep_going.unwrap_or(false) {
                    return;
                }
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        })
    }

    /// The "Thread" title and the turns, filling the rest of the panel; `None` (no section)
    /// when nothing was read yet or the session has no readable thread.
    pub(crate) fn render_thread_section(
        &self,
        session: &Session,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let watch = self.thread.as_ref().filter(|w| w.key.0 == session.id)?;
        let page = watch.page.clone().filter(|p| !p.turns.is_empty())?;
        let id = session.id;
        let earlier = page.truncated.then(|| {
            meta_line(theme, "Earlier turns: Open transcript".into())
                .id("thread-earlier")
                .cursor_pointer()
                .on_click(cx.listener(move |shell, _, _, cx| shell.open_transcript(id, cx)))
        });
        let turns = page.turns.iter().enumerate().map(|(turn_index, turn)| {
            let turn_key = page.first_turn + turn_index as u64;
            div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .child(
                    div()
                        .p(px(space::S))
                        .rounded(px(radius::MEDIUM))
                        .bg(rgb(theme.card))
                        .child(turn.prompt.clone()),
                )
                .children(turn.items.iter().enumerate().map(|(item_index, item)| {
                    match item {
                        PageItem::Text(shown) => div()
                            .flex()
                            .flex_col()
                            .gap(px(space::XS))
                            .text_size(font_size::BODY)
                            .children(preview::blocks(&shown.blocks, shown, theme))
                            .into_any_element(),
                        PageItem::Tools(calls) => {
                            let open = watch.expanded.contains(&(turn_key, item_index));
                            let head = div()
                                .id(SharedString::from(format!(
                                    "thread-tools-{turn_key}-{item_index}"
                                )))
                                .cursor_pointer()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(format!(
                                    "{} +{} tool call{}",
                                    if open { "▾" } else { "▸" },
                                    calls.len(),
                                    if calls.len() == 1 { "" } else { "s" }
                                ))
                                .on_click(cx.listener(move |shell, _, _, cx| {
                                    if let Some(watch) = shell.thread.as_mut()
                                        && !watch.expanded.remove(&(turn_key, item_index))
                                    {
                                        watch.expanded.insert((turn_key, item_index));
                                    }
                                    cx.notify();
                                }));
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(space::XS))
                                .child(head)
                                .when(open, |el| {
                                    el.children(calls.iter().map(|call| {
                                        div()
                                            .flex()
                                            .gap(px(space::S))
                                            .text_size(font_size::META)
                                            .text_color(rgb(theme.text_muted))
                                            .child(
                                                div()
                                                    .flex_none()
                                                    .font_weight(FontWeight(weight::MEDIUM))
                                                    .child(call.name.clone()),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .font_family(theme.terminal_font_family.clone())
                                                    .child(call.target.clone()),
                                            )
                                    }))
                                })
                                .into_any_element()
                        }
                    }
                }))
        });
        let list = div()
            .id("inspector-thread")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&watch.scroll)
            .flex()
            .flex_col()
            .gap(px(space::M))
            .children(earlier)
            .children(turns);
        Some(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .px(px(space::M))
                .pb(px(space::M))
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child("Thread"),
                )
                .child(list)
                // Outside the scrolled list and always present (blank when idle): a row that
                // came and went would change the viewport height, hence `max_offset`, and
                // break following the bottom.
                .child(
                    meta_line(
                        theme,
                        if session.state == SessionState::Working {
                            "Working…"
                        } else {
                            " "
                        }
                        .into(),
                    )
                    .flex_none(),
                )
                .into_any_element(),
        )
    }
}
