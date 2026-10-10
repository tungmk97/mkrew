//! A vertical scroll bar over the right edge of a scrolling view (SPEC §12A): the Files
//! editor (a `uniform_list`) and the Markdown preview (a `list`). gpui-pre 0.3.7 has no scroll
//! bar element. The thumb maths ([`thumb`], [`offset_for`]) are pure and tested; [`bar`] draws
//! the thumb and turns presses and drags into scroll offsets.

use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    Bounds, DispatchPhase, Div, Hitbox, HitboxBehavior, ListState, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ScrollHandle, canvas, div, fill, point, px, rgb, size,
};

use crate::theme::{self, Theme, radius};

/// How far a view scrolls, in px.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// Height of the visible part.
    pub viewport: f32,
    /// Height of all the content.
    pub content: f32,
    /// How far it is scrolled down (0 = top).
    pub offset: f32,
}

/// The thumb in a `track` px tall bar: its top and height. `None` when everything fits.
pub fn thumb(metrics: Metrics, track: f32) -> Option<(f32, f32)> {
    let max = metrics.content - metrics.viewport;
    if max <= 0.0 || track <= 0.0 {
        return None;
    }
    let height = (track * metrics.viewport / metrics.content)
        .max(theme::SCROLLBAR_MIN_THUMB)
        .min(track);
    let top = (track - height) * (metrics.offset / max).clamp(0.0, 1.0);
    Some((top, height))
}

/// The scroll offset that puts the thumb's top at `top`, clamped to both ends.
pub fn offset_for(top: f32, metrics: Metrics, track: f32) -> f32 {
    let Some((_, height)) = thumb(metrics, track) else {
        return 0.0;
    };
    let room = track - height;
    if room <= 0.0 {
        return 0.0;
    }
    (top / room).clamp(0.0, 1.0) * (metrics.content - metrics.viewport)
}

/// What the bar scrolls.
#[derive(Debug, Clone)]
pub enum Source {
    /// A `uniform_list`: `UniformListScrollHandle.0.borrow().base_handle`.
    Handle(ScrollHandle),
    /// A `list`.
    List(ListState),
}

impl Source {
    /// From the last layout; all zero before the first one.
    fn metrics(&self) -> Metrics {
        match self {
            Source::Handle(handle) => {
                let viewport = f32::from(handle.bounds().size.height);
                Metrics {
                    viewport,
                    content: viewport + f32::from(handle.max_offset().y),
                    offset: -f32::from(handle.offset().y),
                }
            }
            Source::List(list) => {
                let viewport = f32::from(list.viewport_bounds().size.height);
                Metrics {
                    viewport,
                    content: viewport + f32::from(list.max_offset_for_scrollbar().y),
                    offset: -f32::from(list.scroll_px_offset_for_scrollbar().y),
                }
            }
        }
    }

    fn scroll_to(&self, offset: f32) {
        match self {
            Source::Handle(handle) => handle.set_offset(point(handle.offset().x, px(-offset))),
            Source::List(list) => list.set_offset_from_scrollbar(point(px(0.), px(-offset))),
        }
    }

    /// A list keeps its content height still while the thumb is held (items measured
    /// meanwhile would move the thumb away from the pointer).
    fn hold(&self, held: bool) {
        if let Source::List(list) = self {
            if held {
                list.scrollbar_drag_started();
            } else {
                list.scrollbar_drag_ended();
            }
        }
    }
}

/// Where the thumb was grabbed (px below its top) while it is dragged. The view keeps one per
/// bar, because the element is rebuilt every frame.
pub type Drag = Rc<Cell<Option<f32>>>;

/// The bar over the right edge of its `relative` parent. It draws only when the content is
/// taller than the view: a thumb in `subtle`, or `text_muted` while held. Dragging the thumb
/// scrolls; pressing the track jumps the thumb's middle there. The wheel still scrolls the
/// view below, and presses on the bar never reach it (no caret move, no selection).
pub fn bar(source: Source, drag: Drag, theme: &Theme) -> Div {
    let (idle, held) = (theme.subtle, theme.text_muted);
    let measured = source.clone();
    div()
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .w(px(theme::SCROLLBAR_WIDTH))
        .child(
            canvas(
                move |bounds, window, _| {
                    let metrics = measured.metrics();
                    let thumb = thumb(metrics, f32::from(bounds.size.height));
                    let hitbox: Option<Hitbox> = thumb.map(|_| {
                        window.insert_hitbox(bounds, HitboxBehavior::BlockMouseExceptScroll)
                    });
                    (metrics, thumb, hitbox)
                },
                move |bounds, (metrics, thumb, hitbox), window, _| {
                    let (Some((top, height)), Some(hitbox)) = (thumb, hitbox) else {
                        return;
                    };
                    let track = f32::from(bounds.size.height);
                    let inset = (theme::SCROLLBAR_WIDTH - theme::SCROLLBAR_THUMB_WIDTH) / 2.0;
                    let shape = Bounds::new(
                        point(bounds.left() + px(inset), bounds.top() + px(top)),
                        size(px(theme::SCROLLBAR_THUMB_WIDTH), px(height)),
                    );
                    let color = if drag.get().is_some() { held } else { idle };
                    window.paint_quad(fill(shape, rgb(color)).corner_radii(px(radius::SMALL)));
                    let origin = bounds.top();
                    window.on_mouse_event({
                        let (source, drag) = (source.clone(), Rc::clone(&drag));
                        move |event: &MouseDownEvent, phase, window, cx| {
                            if phase != DispatchPhase::Bubble
                                || event.button != MouseButton::Left
                                || !hitbox.is_hovered(window)
                            {
                                return;
                            }
                            let y = f32::from(event.position.y - origin);
                            let grab = if (top..top + height).contains(&y) {
                                y - top
                            } else {
                                height / 2.0
                            };
                            source.hold(true);
                            source.scroll_to(offset_for(y - grab, metrics, track));
                            drag.set(Some(grab));
                            cx.stop_propagation();
                            window.refresh();
                        }
                    });
                    // While held, every move in the window scrolls, even off the bar, and
                    // nothing below sees it (the editor would drag-select).
                    window.on_mouse_event({
                        let (source, drag) = (source.clone(), Rc::clone(&drag));
                        move |event: &MouseMoveEvent, phase, window, cx| {
                            let Some(grab) = drag.get() else {
                                return;
                            };
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            // Released outside the window: no mouse-up came.
                            if event.pressed_button != Some(MouseButton::Left) {
                                drag.set(None);
                                source.hold(false);
                                window.refresh();
                                return;
                            }
                            let y = f32::from(event.position.y - origin);
                            source.scroll_to(offset_for(y - grab, source.metrics(), track));
                            cx.stop_propagation();
                            window.refresh();
                        }
                    });
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, _| {
                        if phase == DispatchPhase::Capture && drag.take().is_some() {
                            source.hold(false);
                            window.refresh();
                        }
                    });
                },
            )
            .size_full(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(offset: f32) -> Metrics {
        Metrics {
            viewport: 100.0,
            content: 400.0,
            offset,
        }
    }

    /// A quarter of the content is visible: the thumb is a quarter of the track and slides
    /// with the offset; past the ends it stays at the ends; content that fits draws no bar;
    /// a huge file still gets a thumb you can grab.
    #[test]
    fn the_thumb_is_the_visible_share_of_the_track_and_follows_the_offset() {
        assert_eq!(thumb(at(0.0), 200.0), Some((0.0, 50.0)));
        assert_eq!(thumb(at(150.0), 200.0), Some((75.0, 50.0)));
        assert_eq!(thumb(at(300.0), 200.0), Some((150.0, 50.0)));
        assert_eq!(thumb(at(900.0), 200.0), Some((150.0, 50.0)));
        assert_eq!(thumb(at(-10.0), 200.0), Some((0.0, 50.0)));
        let fits = Metrics {
            viewport: 100.0,
            content: 100.0,
            offset: 0.0,
        };
        assert_eq!(thumb(fits, 200.0), None);
        assert_eq!(thumb(at(0.0), 0.0), None, "no room for a bar");
        let huge = Metrics {
            viewport: 100.0,
            content: 1_000_000.0,
            offset: 0.0,
        };
        assert_eq!(
            thumb(huge, 200.0).map(|(_, height)| height),
            Some(theme::SCROLLBAR_MIN_THUMB)
        );
    }

    /// Dragging the thumb's top to `top` scrolls to the matching offset, clamped to both ends,
    /// and maps back exactly even with the minimum thumb.
    #[test]
    fn dragging_the_thumb_maps_back_to_the_offset_and_stops_at_the_ends() {
        assert_eq!(offset_for(75.0, at(0.0), 200.0), 150.0);
        assert_eq!(offset_for(-20.0, at(0.0), 200.0), 0.0);
        assert_eq!(offset_for(500.0, at(0.0), 200.0), 300.0);
        let huge = Metrics {
            viewport: 100.0,
            content: 1_000_000.0,
            offset: 500_000.0,
        };
        let (top, _) = thumb(huge, 200.0).unwrap();
        assert!((offset_for(top, huge, 200.0) - 500_000.0).abs() < 1.0);
        let fits = Metrics {
            viewport: 100.0,
            content: 100.0,
            offset: 0.0,
        };
        assert_eq!(offset_for(50.0, fits, 200.0), 0.0);
    }
}
