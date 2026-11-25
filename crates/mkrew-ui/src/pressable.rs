//! One hover / press / disabled rule for every clickable element (ADR 0097). `press_style` is
//! pure, so it is tested without a window; `Pressable` applies it to a gpui element.

use gpui::prelude::*;
use gpui::{App, ClickEvent, Div, Stateful, Window, rgb};

use crate::theme::{Theme, opacity, raised_border};

/// How a pressable element looks at rest, hovered and pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// A list or menu row: hover fill `theme.hover`, no border change.
    Row,
    /// A chip or pill: hover fill `theme.hover`, label to full text.
    Chip,
    /// An icon-only control: hover fill `theme.hover`, icon to full text color.
    Icon,
    /// An element that sits on `surface_raised` (menu rows, toasts, cards, the find bar): hover
    /// fill `raised_border` (`layer[1]` over the raised fill), one step above its rest fill.
    Raised,
    /// A text link: no fill, hover underline and full text color.
    Text,
}

/// The resolved style of a pressable element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressStyle {
    pub hover_bg: Option<u32>,
    pub hover_fg: Option<u32>,
    pub underline_on_hover: bool,
    /// Opacity while pressed; 1.0 = unchanged (a disabled element does not react).
    pub active_opacity: f32,
    /// Opacity at rest: 1.0 enabled, `opacity::DISABLED` disabled.
    pub opacity: f32,
    /// The pointing-hand cursor (enabled elements only).
    pub pointer: bool,
}

/// What `look` gives an element in this state: the hover fill and label color, the underline, the
/// pressed and rest opacity and the pointer, resolved from `theme`. Pure, so it is tested
/// without a window.
pub fn press_style(look: Look, enabled: bool, theme: &Theme) -> PressStyle {
    if !enabled {
        return PressStyle {
            hover_bg: None,
            hover_fg: None,
            underline_on_hover: false,
            active_opacity: 1.0,
            opacity: opacity::DISABLED,
            pointer: false,
        };
    }
    let (hover_bg, hover_fg, underline_on_hover) = match look {
        Look::Row => (Some(theme.hover), None, false),
        Look::Chip | Look::Icon => (Some(theme.hover), Some(theme.text), false),
        Look::Raised => (Some(raised_border(theme)), None, false),
        Look::Text => (None, Some(theme.text), true),
    };
    PressStyle {
        hover_bg,
        hover_fg,
        underline_on_hover,
        active_opacity: opacity::ACTIVE,
        opacity: 1.0,
        pointer: true,
    }
}

/// Applies a resolved style: cursor, rest opacity, hover and active. A widget that needs its
/// own hover fill (a danger button, `quiet_button`'s `hover` argument) overrides `hover_bg` first.
pub fn apply_press(el: Stateful<Div>, style: PressStyle) -> Stateful<Div> {
    let PressStyle {
        hover_bg,
        hover_fg,
        underline_on_hover,
        active_opacity,
        opacity,
        pointer,
    } = style;
    el.when(pointer, |el| el.cursor_pointer())
        .when(opacity < 1.0, |el| el.opacity(opacity))
        .when(
            hover_bg.is_some() || hover_fg.is_some() || underline_on_hover,
            |el| {
                el.hover(move |mut style| {
                    if let Some(bg) = hover_bg {
                        style = style.bg(rgb(bg));
                    }
                    if let Some(fg) = hover_fg {
                        style = style.text_color(rgb(fg));
                    }
                    if underline_on_hover {
                        style = style.underline();
                    }
                    style
                })
            },
        )
        .when(pointer, |el| {
            el.active(move |style| style.opacity(active_opacity))
        })
}

pub trait Pressable: Sized {
    /// Applies `press_style(look, enabled, theme)`: cursor, hover, active, disabled opacity.
    fn pressable(self, look: Look, enabled: bool, theme: &Theme) -> Self;
    /// An element that may be picked (selected, active) but stays clickable: the pointer only
    /// while picked (no hover fill, no press dim), else `pressable(look, true, theme)`.
    /// `pressable(look, false, ..)` would fade it instead.
    fn pressable_unless(self, picked: bool, look: Look, theme: &Theme) -> Self;
    /// Attaches `handler` to clicks only when `enabled`.
    fn on_press(
        self,
        enabled: bool,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self;
}

impl Pressable for Stateful<Div> {
    fn pressable(self, look: Look, enabled: bool, theme: &Theme) -> Self {
        apply_press(self, press_style(look, enabled, theme))
    }

    fn pressable_unless(self, picked: bool, look: Look, theme: &Theme) -> Self {
        if picked {
            self.cursor_pointer()
        } else {
            self.pressable(look, true, theme)
        }
    }

    fn on_press(
        self,
        enabled: bool,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.when(enabled, |el| el.on_click(handler))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn themes() -> [Theme; 2] {
        [Theme::dark(), Theme::light()]
    }

    #[test]
    fn every_look_hovers_to_its_fill_and_dims_while_pressed() {
        for t in themes() {
            let row = press_style(Look::Row, true, &t);
            assert_eq!(row.hover_bg, Some(t.hover));
            assert_eq!(row.hover_fg, None);
            let chip = press_style(Look::Chip, true, &t);
            assert_eq!(
                (chip.hover_bg, chip.hover_fg),
                (Some(t.hover), Some(t.text))
            );
            let icon = press_style(Look::Icon, true, &t);
            assert_eq!(
                (icon.hover_bg, icon.hover_fg),
                (Some(t.hover), Some(t.text))
            );
            let raised = press_style(Look::Raised, true, &t);
            assert_eq!(
                (raised.hover_bg, raised.hover_fg),
                (Some(raised_border(&t)), None)
            );
            let text = press_style(Look::Text, true, &t);
            assert_eq!((text.hover_bg, text.hover_fg), (None, Some(t.text)));
            assert!(text.underline_on_hover);
            for style in [row, chip, icon, raised, text] {
                assert!(style.pointer);
                assert_eq!(style.active_opacity, opacity::ACTIVE);
                assert_eq!(style.opacity, 1.0, "no base opacity while enabled");
            }
        }
    }

    #[test]
    fn a_disabled_press_style_has_no_pointer_hover_or_active() {
        for t in themes() {
            for look in [Look::Row, Look::Chip, Look::Icon, Look::Raised, Look::Text] {
                let style = press_style(look, false, &t);
                assert_eq!(style.opacity, opacity::DISABLED);
                assert!(!style.pointer);
                assert_eq!((style.hover_bg, style.hover_fg), (None, None));
                assert!(!style.underline_on_hover);
                assert_eq!(style.active_opacity, 1.0);
            }
        }
    }

    /// Over every built-in theme: a hover fill is never the rest fill of the element and never
    /// `theme.selected` (a hovered row must not look selected).
    #[test]
    fn every_hover_fill_differs_from_the_rest_fill_and_from_selected_in_every_theme() {
        for entry in crate::theme::THEMES {
            let t = entry.build();
            // Row, Chip and Icon rest on the page, a panel or a bar.
            for look in [Look::Row, Look::Chip, Look::Icon] {
                let hover = press_style(look, true, &t).hover_bg.unwrap();
                for (rest, name) in [
                    (t.bg, "bg"),
                    (t.surface, "surface"),
                    (t.chrome, "chrome"),
                    (t.pane_gap, "pane_gap"),
                ] {
                    assert_ne!(hover, rest, "{} {look:?} hover equals {name}", t.name);
                }
                assert_ne!(
                    hover, t.selected,
                    "{} {look:?} hover equals selected",
                    t.name
                );
            }
            // Raised rests on `surface_raised`.
            let raised = press_style(Look::Raised, true, &t).hover_bg.unwrap();
            assert_ne!(
                raised, t.surface_raised,
                "{} Raised hover equals its rest fill",
                t.name
            );
            assert_ne!(
                raised, t.selected,
                "{} Raised hover equals selected",
                t.name
            );
        }
    }
}
