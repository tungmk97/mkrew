//! An expanded Mermaid diagram of a Markdown preview (Docs, Files): the "Expand" of a diagram
//! card shows it large in a dialog; Esc or Close closes it.

use crate::*;
use gpui::img;
use mkrew_ui::preview::ExpandDiagram;

impl Shell {
    pub(crate) fn expand_diagram(
        &mut self,
        action: &ExpandDiagram,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // The dialog is drawn in a Space window only.
        if self.host_of(window).space().is_none() {
            let action = action.clone();
            return self.in_window(self.primary_host(), cx, move |shell, window, cx| {
                window.activate_window();
                shell.expand_diagram(&action, window, cx);
            });
        }
        if self.modal_open() {
            return;
        }
        self.close_menus();
        self.diagram_dialog = Some(action.0.clone());
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    fn close_diagram_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.diagram_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// The diagram as wide as the dialog, up to `DIAGRAM_ZOOM_MAX` times its natural size;
    /// a tall one scrolls.
    pub(crate) fn render_diagram_dialog(
        &self,
        diagram: &Diagram,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let body = dialog_body().child(
            div()
                .id("diagram-dialog-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(
                    div().flex().justify_center().child(
                        img(diagram.image(theme))
                            .w(px(diagram.width * theme::DIAGRAM_ZOOM_MAX))
                            .max_w_full()
                            .when(diagram.height > 0.0, |img| {
                                img.aspect_ratio(diagram.width / diagram.height)
                            }),
                    ),
                ),
        );
        let panel = dialog_frame(
            floating_panel("diagram-dialog", theme)
                .w(relative(theme::HISTORY_DIALOG_FRACTION))
                .h(relative(theme::HISTORY_DIALOG_FRACTION))
                .dialog_root(
                    &self.dialog_focus,
                    false,
                    cx.listener(Self::dialog_guard),
                    cx.listener(|shell, event: &KeyDownEvent, window, cx| {
                        cx.stop_propagation();
                        if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified()
                        {
                            shell.close_diagram_dialog(window, cx);
                        }
                    }),
                ),
            DialogHead {
                icon: Icon::Share2,
                tone: DialogTone::Normal,
                title: "Diagram".into(),
                sub: None,
                wrap: false,
            },
            Some(dialog_close("diagram-close", theme).on_click(cx.listener(
                |shell, _, window, cx| {
                    shell.close_diagram_dialog(window, cx);
                },
            ))),
            Some(body),
            None,
            theme,
        );
        Self::modal_shade("diagram-dialog-shade", panel, cx)
    }
}
