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
        let panel = div()
            .id("diagram-dialog")
            .w(relative(theme::HISTORY_DIALOG_FRACTION))
            .h(relative(theme::HISTORY_DIALOG_FRACTION))
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
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(|shell, event: &KeyDownEvent, window, cx| {
                    cx.stop_propagation();
                    if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
                        shell.close_diagram_dialog(window, cx);
                    }
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(font_size::CARD_TITLE)
                            .font_weight(FontWeight(weight::SEMIBOLD))
                            .text_color(rgb(theme.text))
                            .child("Diagram"),
                    )
                    .child(
                        button("diagram-close", "Close", theme, false)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Close", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_diagram_dialog(window, cx);
                            })),
                    ),
            )
            .child(
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
        Self::modal_shade("diagram-dialog-shade", panel, cx)
    }
}
