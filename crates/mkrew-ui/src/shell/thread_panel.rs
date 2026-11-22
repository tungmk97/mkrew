//! The transcripts the window reads (ADR 0057, ADR 0104) and the session panel's Thread
//! section. A 1 s poll per session runs only while the panel shows that agent session with a
//! native id, or a pane on screen is in Chat mode.

use mkrew_core::thread::{CHAT_TURNS, THREAD_TURNS};
use mkrew_ui::chat;
use mkrew_ui::thread_view::{self, PageItem, ThreadFile};

use crate::*;

/// One session's transcript being read; shared by every consumer of that session.
pub(crate) struct ThreadRead {
    /// The session and native id being read; another pair starts a new read.
    pub(crate) key: (Ulid, String),
    /// The 1 s poll; dropping it stops reading. It ends by itself after the session exited
    /// (one last read).
    _poll: gpui::Task<()>,
    /// Built off the UI thread on each change; `None` until a read found turns.
    pub(crate) page: Option<Rc<thread_view::Page>>,
}

/// What the Thread section remembers about the shown session.
pub(crate) struct ThreadWatch {
    pub(crate) key: (Ulid, String),
    /// Expanded tool groups: (turn key = `page.first_turn` + turn index, item index).
    pub(crate) expanded: HashSet<(u64, usize)>,
    pub(crate) scroll: ScrollHandle,
}

/// The list shows its end: new turns keep it there.
pub(crate) fn at_bottom(scroll: &ScrollHandle) -> bool {
    -f32::from(scroll.offset().y) >= f32::from(scroll.max_offset().y) - 1.0
}

impl Shell {
    /// Starts, keeps or stops the readings to match what is shown (status tick): the panel's
    /// session, and the Chat panes. No work unless something shows a session with a native id.
    pub(crate) fn sync_thread(&mut self, cx: &mut Context<'_, Self>) {
        let session = self
            .panel_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let wanted = self.panel_shown() && self.panel_tab() == PanelTab::Session;
        let panel = inspector::wanted_thread(wanted, session);
        let panes = self.chat_panes();
        let chats = panes
            .iter()
            .filter_map(|(id, native)| Some((*id, native.clone()?)))
            .collect();
        let keys = chat::wanted_reads(panel.clone(), chats);
        let before = self.reads.len();
        self.reads.retain(|_, read| keys.contains(&read.key));
        let mut changed = self.reads.len() != before;
        for key in keys {
            if !self.reads.contains_key(&key.0) {
                let poll = self.poll_thread(key.clone(), cx);
                self.reads.insert(
                    key.0,
                    ThreadRead {
                        key,
                        _poll: poll,
                        page: None,
                    },
                );
                changed = true;
            }
        }
        match panel {
            None => changed |= self.thread.take().is_some(),
            Some(key) if self.thread.as_ref().is_some_and(|w| w.key == key) => {}
            Some(key) => {
                let scroll = ScrollHandle::new();
                // The read is shared: it may have a page already, so no first page will come.
                if self
                    .reads
                    .get(&key.0)
                    .is_some_and(|read| read.page.is_some())
                {
                    scroll.scroll_to_bottom();
                }
                self.thread = Some(ThreadWatch {
                    key,
                    expanded: HashSet::new(),
                    scroll,
                });
                changed = true;
            }
        }
        changed |= self.sync_chats(&panes);
        if changed {
            cx.notify();
        }
    }

    /// A new page of `session` was stored: a list that showed its end (or had nothing yet,
    /// `first`) keeps following it.
    fn page_arrived(&mut self, session: Ulid, first: bool) {
        if let Some(watch) = self.thread.as_mut().filter(|w| w.key.0 == session)
            && (first || at_bottom(&watch.scroll))
        {
            watch.scroll.scroll_to_bottom();
        }
        if let Some(ui) = self.chats.get_mut(&session)
            && (first || at_bottom(&ui.scroll))
        {
            ui.scroll.scroll_to_bottom();
        }
    }

    fn poll_thread(&self, key: (Ulid, String), cx: &mut Context<'_, Self>) -> gpui::Task<()> {
        cx.spawn(async move |this, cx| {
            // One `session.transcript` read a second while shown; the daemon finds and reads the
            // file, so this works the same for a window on another computer (spec 2026-10-29 §5).
            // The connection is read each round: after a reconnect the watch carries on from
            // its offset, and while there is none it just waits.
            let mut file: Option<ThreadFile> = None;
            loop {
                let offset = file.as_ref().map_or(0, ThreadFile::offset);
                let connection = this.update(cx, |shell, _| shell.connection.clone());
                let Ok(connection) = connection else { return };
                let read = match connection {
                    Some(connection) => Some(
                        connection
                            .call::<SessionTranscript>(Request::SessionTranscript(
                                SessionTranscriptParams {
                                    session_id: key.0,
                                    offset,
                                },
                            ))
                            .await,
                    ),
                    None => None,
                };
                let mut more = false;
                let page = match read {
                    Some(Ok(read)) if read.found => {
                        more = !read.data.is_empty()
                            && read.start + (read.data.len() as u64) < read.len;
                        if file.is_none() {
                            file =
                                ThreadFile::new(&UsageFormat::parse(&read.format), CHAT_TURNS).ok();
                        }
                        match file.take() {
                            Some(mut f) => {
                                let (back, page) = cx
                                    .background_executor()
                                    .spawn(async move {
                                        let changed = f.apply(read.start, &read.data);
                                        let page = changed.then(|| f.page());
                                        (f, page)
                                    })
                                    .await;
                                file = Some(back);
                                page
                            }
                            None => None,
                        }
                    }
                    // Not found yet (no usage profile, or the file is not written), or not
                    // connected: keep polling.
                    Some(Ok(_)) | None => None,
                    Some(Err(error)) => {
                        tracing::debug!("session.transcript failed: {error:#}");
                        None
                    }
                };
                let keep_going = this.update(cx, |shell, cx| {
                    let arrived = match (page, shell.reads.get_mut(&key.0).filter(|r| r.key == key))
                    {
                        (Some(page), Some(read)) => {
                            let first = read.page.is_none();
                            read.page = Some(Rc::new(page));
                            Some(first)
                        }
                        _ => None,
                    };
                    if let Some(first) = arrived {
                        shell.page_arrived(key.0, first);
                        cx.notify();
                    }
                    // The read above comes first: one last read after the exit.
                    shell.reads.get(&key.0).is_some_and(|r| r.key == key) && !shell.is_exited(key.0)
                });
                if !keep_going.unwrap_or(false) {
                    return;
                }
                // A reply that did not reach the end of the file: read on at once.
                if !more {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                }
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
        let read = self.reads.get(&session.id)?;
        let page = read.page.clone().filter(|p| !p.turns.is_empty())?;
        let id = session.id;
        let (skipped, shown_turns) = page.newest(THREAD_TURNS);
        let earlier = (page.truncated || skipped > 0).then(|| {
            meta_line(theme, "Earlier turns: Open transcript".into())
                .id("thread-earlier")
                .pressable(Look::Text, true, theme)
                .on_press(
                    true,
                    cx.listener(move |shell, _, _, cx| shell.open_transcript(id, cx)),
                )
        });
        let turns = shown_turns.iter().enumerate().map(|(turn_index, turn)| {
            let turn_key = page.first_turn + (skipped + turn_index) as u64;
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
                .children(
                    turn.items
                        .iter()
                        .enumerate()
                        .filter_map(|(item_index, item)| match item {
                            PageItem::Text(shown) => Some(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(space::XS))
                                    .text_size(font_size::BODY)
                                    .children(preview::blocks(&shown.blocks, shown, theme))
                                    .into_any_element(),
                            ),
                            PageItem::Tools(calls) => {
                                let open = watch.expanded.contains(&(turn_key, item_index));
                                let head = div()
                                    .id(SharedString::from(format!(
                                        "thread-tools-{turn_key}-{item_index}"
                                    )))
                                    .text_size(font_size::META)
                                    .text_color(rgb(theme.text_muted))
                                    .pressable(Look::Text, true, theme)
                                    .child(format!(
                                        "{} +{} tool call{}",
                                        if open { "▾" } else { "▸" },
                                        calls.len(),
                                        if calls.len() == 1 { "" } else { "s" }
                                    ))
                                    .on_press(
                                        true,
                                        cx.listener(move |shell, _, _, cx| {
                                            if let Some(watch) = shell.thread.as_mut()
                                                && !watch.expanded.remove(&(turn_key, item_index))
                                            {
                                                watch.expanded.insert((turn_key, item_index));
                                            }
                                            cx.notify();
                                        }),
                                    );
                                Some(
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
                                                            .font_family(
                                                                theme.terminal_font_family.clone(),
                                                            )
                                                            .child(call.target.clone()),
                                                    )
                                            }))
                                        })
                                        .into_any_element(),
                                )
                            }
                            PageItem::Thinking { .. } => None,
                        }),
                )
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
