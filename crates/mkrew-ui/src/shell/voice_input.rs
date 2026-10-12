//! Voice input (§20G): record, transcribe, and type the text where the keyboard is.

use crate::*;

impl Shell {
    /// Voice input (§20G): start recording, or stop it; the text arrives in `voice_text`.
    pub(crate) fn toggle_voice(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        match self.voice {
            // Leaving `Recording` drops its `Stop`: the clip comes to the task started below.
            voice::State::Recording { .. } => {
                self.voice = voice::State::Transcribing;
                cx.notify();
                return;
            }
            voice::State::Transcribing => return,
            voice::State::Idle | voice::State::Message(_) => {}
        }
        // Read per start: a hand edit of `[voice]` applies to the next recording.
        let settings = match mkrew_core::settings::load(&self.settings_file) {
            Ok(settings) => settings.voice,
            // Never fall back to the default provider: audio goes only where the user said (§1).
            Err(error) => {
                self.voice = voice::State::Message(format!("✕ Voice: {error}"));
                cx.notify();
                return;
            }
        };
        let key = match stt::api_key(&settings, |name| std::env::var(name).ok()) {
            Ok(key) => key,
            Err(error) => {
                self.voice = voice::State::Message(format!("✕ Voice: {error:#}"));
                cx.notify();
                return;
            }
        };
        let (stop, level, clip) =
            voice::record(Duration::from_secs(u64::from(settings.max_seconds)));
        let since = Instant::now();
        self.voice = voice::State::Recording { since, stop, level };
        cx.notify();
        // The level meter moves with the voice: redraw often, only while this recording lasts.
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(theme::VOICE_METER_TICK)
                    .await;
                let recording = this.update_in(cx, |shell, window, cx| {
                    let on = matches!(
                        &shell.voice,
                        voice::State::Recording { since: started, .. } if *started == since
                    );
                    if on && window.is_visible() {
                        cx.notify();
                    }
                    on
                });
                if !matches!(recording, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            let wav = clip
                .recv()
                .await
                .unwrap_or_else(|_| Err(anyhow::anyhow!("the microphone thread stopped")))
                .and_then(voice::Clip::into_wav);
            let wav = match wav {
                Ok(wav) => wav,
                Err(error) => {
                    let _ = this.update(cx, |shell, cx| {
                        shell.voice = voice::State::Message(format!("✕ Voice: {error:#}"));
                        cx.notify();
                    });
                    return;
                }
            };
            // Also reached when `max_seconds` ended the recording.
            let _ = this.update(cx, |shell, cx| {
                shell.voice = voice::State::Transcribing;
                cx.notify();
            });
            let text = cx
                .background_executor()
                .spawn(async move { stt::transcribe(&settings, key.as_deref(), &wav) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| shell.voice_text(text, window, cx));
        })
        .detach();
    }

    /// Where a transcript goes: the field or terminal with the keyboard, else the clipboard.
    fn voice_text(
        &mut self,
        text: anyhow::Result<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.voice = match text {
            Err(error) => voice::State::Message(format!("✕ Voice: {error:#}")),
            Ok(text) if text.is_empty() => voice::State::Message("Voice: heard nothing".into()),
            Ok(text) => {
                if self.type_voice_text(&text, window, cx) {
                    voice::State::Idle
                } else {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    voice::State::Message(
                        "Voice: no text field has the keyboard; the text is on the clipboard"
                            .into(),
                    )
                }
            }
        };
        cx.notify();
    }

    /// Types `text` into the text field that has the keyboard (like an IME commit) or into the
    /// focused terminal pane (one paste, never Enter). False when neither has focus: nothing is
    /// typed, so bare-key shortcuts (Git list) never fire.
    fn type_voice_text(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let field = self
            .typing_target
            .borrow()
            .clone()
            .filter(|(focus, _)| focus.is_focused(window));
        if let Some((focus, on_key)) = field {
            // An open composition belongs to what the user typed, not to the voice text.
            self.ime_marked.marked_for(&focus).set(0);
            for keystroke in ime::keys(&Cell::new(0), text, false) {
                let event = KeyDownEvent {
                    keystroke,
                    is_held: false,
                    prefer_character_input: false,
                };
                on_key(self, &event, window, cx);
            }
            return true;
        }
        let pane = self
            .panes
            .values()
            .find(|pane| {
                let pane = pane.read(cx);
                !pane.is_exited() && pane.has_focus(window, cx)
            })
            .cloned();
        if let Some(pane) = pane {
            pane.update(cx, |pane, cx| pane.type_voice(text, window, cx));
            return true;
        }
        false
    }
}
