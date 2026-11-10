//! Where a dragged tab lands and where a detached window opens (spec 2026-11-10 §5.1, §6.2).
//! Screen coordinates are the windows' `bounds()` (gpui's global space).

use gpui::{Bounds, Pixels, Point, Size, point, px, size};

use crate::theme;

/// A window's tab strip as painted: the strip and its chips, window-local.
#[derive(Debug, Clone, PartialEq)]
pub struct Strip {
    pub bounds: Bounds<Pixels>,
    pub chips: Vec<Bounds<Pixels>>,
}

/// A window on screen and its strip.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed<H> {
    pub host: H,
    pub window: Bounds<Pixels>,
    pub strip: Option<Strip>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target<H> {
    /// Into that window's strip, before chip `index` (`index == chips` = the end).
    Strip { host: H, index: usize },
    /// Into that window, at the end of its strip.
    Window(H),
    /// A new window with its top left here.
    NewWindow(Point<Pixels>),
}

/// The target of a tab released at screen `point` from `source`. `windows` are ordered most
/// recently activated first (the frontmost of overlapping Mkrew windows wins). `None` while
/// the point is inside the source window: its own handlers (reorder, split) decide.
// ponytail: other apps' windows are ignored (no OS z-order query); add one if drops through
// other apps confuse users. Mixed-DPI displays can be off by the scale difference.
pub fn drop_target<H: Copy + PartialEq>(
    point: Point<Pixels>,
    source: H,
    windows: &[Placed<H>],
) -> Option<Target<H>> {
    let Some(over) = windows.iter().find(|placed| placed.window.contains(&point)) else {
        return Some(Target::NewWindow(point));
    };
    if over.host == source {
        return None;
    }
    let local = point - over.window.origin;
    match &over.strip {
        Some(strip) if strip.bounds.contains(&local) => {
            let index = (strip.chips.iter())
                .position(|chip| local.x < chip.origin.x + chip.size.width / 2.0)
                .unwrap_or(strip.chips.len());
            Some(Target::Strip {
                host: over.host,
                index,
            })
        }
        _ => Some(Target::Window(over.host)),
    }
}

/// A new detached window of `size` (at least the minimum): its top left at `at` (the release
/// point), else offset from `near` (the window it comes from); kept on the display it starts on.
pub fn new_window_bounds(
    at: Option<Point<Pixels>>,
    size_wanted: Size<Pixels>,
    near: Bounds<Pixels>,
    displays: &[Bounds<Pixels>],
) -> Bounds<Pixels> {
    let size = size(
        size_wanted.width.max(px(theme::DETACHED_MIN_WIDTH)),
        size_wanted.height.max(px(theme::DETACHED_MIN_HEIGHT)),
    );
    let offset = px(theme::DETACHED_OFFSET);
    let origin = at.unwrap_or(point(near.origin.x + offset, near.origin.y + offset));
    let display = (displays.iter())
        .find(|display| display.contains(&origin))
        .or_else(|| {
            displays
                .iter()
                .find(|display| display.contains(&near.origin))
        });
    let mut bounds = Bounds::new(origin, size);
    if let Some(display) = display {
        bounds.origin.x = bounds
            .origin
            .x
            .min(display.right() - size.width)
            .max(display.origin.x);
        bounds.origin.y = bounds
            .origin
            .y
            .min(display.bottom() - size.height)
            .max(display.origin.y);
    }
    bounds
}

/// Saved bounds that intersect no display (a monitor that is gone) move to the center of the
/// primary display; others stay.
pub fn fit_on_displays(
    bounds: Bounds<Pixels>,
    displays: &[Bounds<Pixels>],
    primary: Option<Bounds<Pixels>>,
) -> Bounds<Pixels> {
    if displays.iter().any(|display| display.intersects(&bounds)) {
        return bounds;
    }
    match primary {
        Some(primary) => Bounds::centered_at(primary.center(), bounds.size),
        None => bounds,
    }
}

#[cfg(test)]
mod tests;
