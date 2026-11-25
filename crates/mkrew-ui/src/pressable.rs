//! One hover / press / disabled rule for every clickable element (ADR 0097). `press_style` is
//! pure, so it is tested without a window; `Pressable` applies it to a gpui element.

use gpui::prelude::*;
use gpui::{App, ClickEvent, Div, Rgba, Stateful, Window, rgb};

use crate::theme::{self, Theme, opacity};

/// How a pressable element looks at rest, hovered and pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// A list or menu row: hover fill `layer[0]`, label to `text`.
    Row,
    /// A chip or pill: hover fill `layer[0]`, label to `text`.
    Chip,
    /// An icon-only control: hover fill `layer[0]`, icon to `text`.
    Icon,
    /// An element on a raised surface or inside a hovered row (menu rows, toasts, row actions, the
    /// find bar): hover fill `layer[0]` stacked over what is behind it, the label unchanged.
    Raised,
    /// A text link: no fill, hover underline and full text color.
    Text,
}

/// The resolved style of a pressable element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PressStyle {
    /// Fill while hovered (translucent `layer[0]`, or opaque when the element has its own fill).
    pub hover_bg: Option<Rgba>,
    /// Fill while pressed (`layer[1]` stacked on the hover fill).
    pub pressed_bg: Option<Rgba>,
    /// Border color while hovered (a secondary button's ring); `None` keeps the rest color.
    pub hover_border: Option<Rgba>,
    pub hover_fg: Option<u32>,
    pub underline_on_hover: bool,
    /// Opacity while pressed; 1.0 = unchanged (a fill look presses with `pressed_bg`, a link and a
    /// filled control dim).
    pub active_opacity: f32,
    /// Opacity at rest: 1.0 enabled, `opacity::DISABLED` disabled.
    pub opacity: f32,
    /// The pointing-hand cursor (enabled elements only).
    pub pointer: bool,
}

/// `layer` (`0xRRGGBBAA`) drawn over the opaque `base`, as `0xRRGGBB`.
pub fn flatten(layer: u32, base: u32) -> u32 {
    theme::mix(base, layer >> 8, (layer & 0xFF) as f32 / 255.0)
}

/// `top` drawn over `below` (both `0xRRGGBBAA`): what two translucent layers show together over
/// any ground.
pub fn stack(top: u32, below: u32) -> u32 {
    let alpha = |color: u32| (color & 0xFF) as f32 / 255.0;
    let (at, ab) = (alpha(top), alpha(below));
    let out = at + ab * (1.0 - at);
    if out <= 0.0 {
        return 0;
    }
    let rgb = [24, 16, 8].into_iter().fold(0u32, |acc, shift| {
        let (ct, cb) = (
            ((top >> shift) & 0xFF) as f32,
            ((below >> shift) & 0xFF) as f32,
        );
        let channel = (ct * at + cb * ab * (1.0 - at)) / out;
        acc | ((channel.round() as u32) << shift)
    });
    rgb | (out * 255.0).round() as u32
}

/// What `look` gives an element in this state, resolved from `theme`: hover and pressed fills
/// (translucent layers), the label color, the underline, the opacities and the pointer. Pure, so
/// it is tested without a window.
pub fn press_style(look: Look, enabled: bool, theme: &Theme) -> PressStyle {
    press_style_on(look, enabled, theme, None)
}

/// [`press_style`] for an element that paints its own fill `rest` (a raised pill, a field): gpui's
/// hover style replaces the element's `bg`, so the layers are flattened over `rest` and the fill
/// keeps showing, one step lighter.
pub fn press_style_on(look: Look, enabled: bool, theme: &Theme, rest: Option<u32>) -> PressStyle {
    if !enabled {
        return PressStyle {
            hover_bg: None,
            pressed_bg: None,
            hover_border: None,
            hover_fg: None,
            underline_on_hover: false,
            active_opacity: 1.0,
            opacity: opacity::DISABLED,
            pointer: false,
        };
    }
    let [l1, l2, ..] = theme.layer;
    let (hover_bg, pressed_bg) = match (look, rest) {
        (Look::Text, _) => (None, None),
        (_, None) => (Some(gpui::rgba(l1)), Some(gpui::rgba(stack(l2, l1)))),
        (_, Some(rest)) => {
            let hover = flatten(l1, rest);
            (Some(rgb(hover)), Some(rgb(flatten(l2, hover))))
        }
    };
    let hover_fg = match look {
        Look::Raised => None,
        Look::Row | Look::Chip | Look::Icon | Look::Text => Some(theme.text),
    };
    PressStyle {
        hover_bg,
        pressed_bg,
        hover_border: None,
        hover_fg,
        underline_on_hover: look == Look::Text,
        active_opacity: if look == Look::Text {
            opacity::ACTIVE
        } else {
            1.0
        },
        opacity: 1.0,
        pointer: true,
    }
}

/// Applies a resolved style: cursor, rest opacity, hover and active. A widget that needs its
/// own hover fill (a round button, a danger button) overrides `hover_bg` first.
pub fn apply_press(el: Stateful<Div>, style: PressStyle) -> Stateful<Div> {
    let PressStyle {
        hover_bg,
        pressed_bg,
        hover_border,
        hover_fg,
        underline_on_hover,
        active_opacity,
        opacity,
        pointer,
    } = style;
    el.when(pointer, |el| el.cursor_pointer())
        .when(opacity < 1.0, |el| el.opacity(opacity))
        .when(
            hover_bg.is_some()
                || hover_border.is_some()
                || hover_fg.is_some()
                || underline_on_hover,
            |el| {
                el.hover(move |mut style| {
                    if let Some(bg) = hover_bg {
                        style = style.bg(bg);
                    }
                    if let Some(border) = hover_border {
                        style = style.border_color(border);
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
        .when(
            pointer && (pressed_bg.is_some() || active_opacity < 1.0),
            |el| {
                el.active(move |mut style| {
                    if let Some(bg) = pressed_bg {
                        style = style.bg(bg);
                    }
                    if active_opacity < 1.0 {
                        style = style.opacity(active_opacity);
                    }
                    style
                })
            },
        )
}

pub trait Pressable: Sized {
    /// Applies `press_style(look, enabled, theme)`: cursor, hover, active, disabled opacity.
    fn pressable(self, look: Look, enabled: bool, theme: &Theme) -> Self;
    /// [`Pressable::pressable`] for an enabled element that paints its own fill `rest` (the
    /// layers are flattened over it, see [`press_style_on`]).
    fn pressable_on(self, look: Look, rest: u32, theme: &Theme) -> Self;
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

    fn pressable_on(self, look: Look, rest: u32, theme: &Theme) -> Self {
        apply_press(self, press_style_on(look, true, theme, Some(rest)))
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

    fn alpha(color: u32) -> f32 {
        (color & 0xFF) as f32 / 255.0
    }

    #[test]
    fn fill_looks_hover_to_layer_one_and_press_stacks_layer_two() {
        for t in themes() {
            for look in [Look::Row, Look::Chip, Look::Icon, Look::Raised] {
                let s = press_style(look, true, &t);
                assert_eq!(s.hover_bg, Some(gpui::rgba(t.layer[0])), "{look:?}");
                assert_eq!(
                    s.pressed_bg,
                    Some(gpui::rgba(stack(t.layer[1], t.layer[0]))),
                    "{look:?}"
                );
                assert_eq!(s.hover_border, None, "{look:?}");
                assert_eq!(s.active_opacity, 1.0, "a fill look presses with a fill");
                assert_eq!(s.opacity, 1.0);
                assert!(s.pointer);
            }
            assert_eq!(press_style(Look::Row, true, &t).hover_fg, Some(t.text));
            assert_eq!(press_style(Look::Chip, true, &t).hover_fg, Some(t.text));
            assert_eq!(press_style(Look::Icon, true, &t).hover_fg, Some(t.text));
            assert_eq!(press_style(Look::Raised, true, &t).hover_fg, None);
            let text = press_style(Look::Text, true, &t);
            assert_eq!(
                (text.hover_bg, text.pressed_bg, text.hover_fg),
                (None, None, Some(t.text))
            );
            assert!(text.underline_on_hover && text.pointer);
            assert_eq!(
                text.active_opacity,
                opacity::ACTIVE,
                "a link has no fill to press"
            );
        }
    }

    #[test]
    fn a_disabled_press_style_has_no_pointer_hover_or_active() {
        for t in themes() {
            for look in [Look::Row, Look::Chip, Look::Icon, Look::Raised, Look::Text] {
                for rest in [None, Some(t.surface_raised)] {
                    let s = press_style_on(look, false, &t, rest);
                    assert_eq!(s.opacity, opacity::DISABLED);
                    assert!(!s.pointer);
                    assert_eq!(
                        (s.hover_bg, s.pressed_bg, s.hover_border),
                        (None, None, None)
                    );
                    assert_eq!(s.hover_fg, None);
                    assert!(!s.underline_on_hover);
                    assert_eq!(s.active_opacity, 1.0);
                }
            }
        }
    }

    #[test]
    fn on_a_fill_the_layers_are_flattened_so_hover_keeps_the_fill() {
        for t in themes() {
            let s = press_style_on(Look::Row, true, &t, Some(t.surface_raised));
            let hover = flatten(t.layer[0], t.surface_raised);
            assert_eq!(s.hover_bg, Some(gpui::rgb(hover)));
            assert_eq!(s.pressed_bg, Some(gpui::rgb(flatten(t.layer[1], hover))));
            assert_ne!(hover, t.surface_raised, "the step shows on the fill");
        }
    }

    #[test]
    fn stacked_layers_are_denser_than_either_and_keep_their_hue() {
        let white = |a: u8| 0xFFFF_FF00 | u32::from(a);
        let both = stack(white(26), white(18));
        assert_eq!(both >> 8, 0xFFFFFF);
        assert!(alpha(both) > alpha(white(26)) && alpha(both) > alpha(white(18)));
        assert_eq!(
            stack(white(26), 0),
            white(26),
            "over nothing it is unchanged"
        );
        assert_eq!(flatten(white(255), 0x101010), 0xFFFFFF);
        assert_eq!(flatten(white(0), 0x101010), 0x101010);
    }

    /// A hover or press step that equals the ground would be invisible: checked on every ground a
    /// pressable sits on, in every built-in theme.
    #[test]
    fn every_hover_and_press_step_shows_on_every_ground_of_every_theme() {
        for entry in crate::theme::THEMES {
            let t = entry.build();
            for (ground, name) in [
                (t.bg, "bg"),
                (t.surface, "surface"),
                (t.chrome, "chrome"),
                (t.pane_gap, "pane_gap"),
                (t.surface_raised, "surface_raised"),
                (t.well, "well"),
            ] {
                let hover = flatten(t.layer[0], ground);
                let pressed = flatten(t.layer[1], hover);
                assert_ne!(hover, ground, "{} hover is invisible on {name}", t.name);
                assert_ne!(hover, t.accent_bg, "{} hover is the picked fill", t.name);
                assert_ne!(pressed, hover, "{} press is invisible on {name}", t.name);
            }
        }
    }
}
