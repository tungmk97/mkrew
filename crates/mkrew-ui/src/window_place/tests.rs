use gpui::{Bounds, point, px, size};

use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
}

fn strip(chips: &[(f32, f32)]) -> Strip {
    Strip {
        bounds: rect(0.0, 40.0, 800.0, 36.0),
        chips: chips.iter().map(|&(x, w)| rect(x, 44.0, w, 28.0)).collect(),
    }
}

#[test]
fn inside_the_source_window_is_not_a_window_drop() {
    let windows = [Placed {
        host: 1,
        window: rect(0.0, 0.0, 800.0, 600.0),
        strip: None,
    }];
    assert_eq!(drop_target(point(px(10.0), px(10.0)), 1, &windows), None);
}

#[test]
fn over_another_strip_picks_the_gap_under_the_pointer() {
    let windows = [
        Placed {
            host: 2,
            window: rect(1000.0, 0.0, 800.0, 600.0),
            strip: Some(strip(&[(8.0, 100.0), (112.0, 100.0)])),
        },
        Placed {
            host: 1,
            window: rect(0.0, 0.0, 800.0, 600.0),
            strip: None,
        },
    ];
    // Left half of the second chip: before it.
    let at = drop_target(point(px(1000.0 + 130.0), px(60.0)), 1, &windows);
    assert_eq!(at, Some(Target::Strip { host: 2, index: 1 }));
    // Right of every chip: the end.
    let at = drop_target(point(px(1000.0 + 600.0), px(60.0)), 1, &windows);
    assert_eq!(at, Some(Target::Strip { host: 2, index: 2 }));
    // Below the strip: the window, at the end.
    let at = drop_target(point(px(1000.0 + 300.0), px(300.0)), 1, &windows);
    assert_eq!(at, Some(Target::Window(2)));
}

#[test]
fn chips_not_drawn_are_no_gap() {
    // Slot 0 and 2 are not drawn (empty); the pointer is left of the only chip.
    let mut gapped = strip(&[(8.0, 100.0)]);
    gapped.chips.insert(0, Bounds::default());
    gapped.chips.push(Bounds::default());
    let windows = [Placed {
        host: 2,
        window: rect(1000.0, 0.0, 800.0, 600.0),
        strip: Some(gapped),
    }];
    let at = drop_target(point(px(1000.0 + 20.0), px(60.0)), 1, &windows);
    assert_eq!(at, Some(Target::Strip { host: 2, index: 1 }));
    let at = drop_target(point(px(1000.0 + 600.0), px(60.0)), 1, &windows);
    assert_eq!(at, Some(Target::Strip { host: 2, index: 3 }));
}

#[test]
fn a_drop_slot_stays_in_the_pin_group() {
    // 2 pinned of 5 chips.
    assert_eq!(clamp_to_group(4, 2, 5, true), 2);
    assert_eq!(clamp_to_group(1, 2, 5, true), 1);
    assert_eq!(clamp_to_group(0, 2, 5, false), 2);
    assert_eq!(clamp_to_group(3, 2, 5, false), 3);
    assert_eq!(clamp_to_group(9, 2, 5, false), 5);
}

#[test]
fn the_drop_line_sits_in_the_gap_before_a_drawn_chip() {
    let mut gapped = strip(&[(8.0, 100.0), (112.0, 100.0)]);
    // Before the first chip (gap 4, line 2: 3 px left of the edge); chips sit 4 px below the strip top.
    let line = drop_line(&gapped, 0, 4.0).unwrap();
    assert_eq!(line.origin, point(px(5.0), px(4.0)));
    assert_eq!(line.size.height, px(28.0));
    // At the end: right of the last chip, in its gap.
    let line = drop_line(&gapped, 2, 4.0).unwrap();
    assert_eq!(line.origin.x, px(213.0));
    // A slot not drawn leads to the next drawn chip.
    gapped.chips.insert(1, Bounds::default());
    assert_eq!(drop_line(&gapped, 1, 4.0).unwrap().origin.x, px(109.0));
    assert!(drop_line(&strip(&[]), 0, 4.0).is_none());
}

#[test]
fn overlapping_windows_go_to_the_most_recently_activated() {
    let windows = [
        Placed {
            host: 3,
            window: rect(100.0, 100.0, 400.0, 400.0),
            strip: None,
        },
        Placed {
            host: 2,
            window: rect(0.0, 0.0, 800.0, 600.0),
            strip: None,
        },
        Placed {
            host: 1,
            window: rect(2000.0, 0.0, 800.0, 600.0),
            strip: None,
        },
    ];
    assert_eq!(
        drop_target(point(px(200.0), px(200.0)), 1, &windows),
        Some(Target::Window(3))
    );
}

#[test]
fn outside_every_window_opens_a_new_one_at_the_pointer() {
    let windows = [Placed {
        host: 1,
        window: rect(0.0, 0.0, 800.0, 600.0),
        strip: None,
    }];
    let at = point(px(1200.0), px(300.0));
    assert_eq!(drop_target(at, 1, &windows), Some(Target::NewWindow(at)));
}

#[test]
fn a_new_window_starts_at_the_pointer_and_stays_on_its_display() {
    let displays = [
        rect(0.0, 0.0, 1920.0, 1080.0),
        rect(1920.0, 0.0, 1280.0, 1024.0),
    ];
    let near = rect(100.0, 100.0, 1200.0, 760.0);
    let placed = new_window_bounds(
        Some(point(px(2000.0), px(100.0))),
        size(px(900.0), px(500.0)),
        near,
        &displays,
    );
    assert_eq!(placed, rect(2000.0, 100.0, 900.0, 500.0));
    // Too far right: pushed back onto the second display.
    let placed = new_window_bounds(
        Some(point(px(3100.0), px(900.0))),
        size(px(900.0), px(500.0)),
        near,
        &displays,
    );
    assert_eq!(placed, rect(2300.0, 524.0, 900.0, 500.0));
    // Smaller than the minimum: grown.
    let placed = new_window_bounds(
        Some(point(px(10.0), px(10.0))),
        size(px(100.0), px(100.0)),
        near,
        &displays,
    );
    assert_eq!(
        placed.size,
        size(
            px(theme::DETACHED_MIN_WIDTH),
            px(theme::DETACHED_MIN_HEIGHT)
        )
    );
}

#[test]
fn a_new_window_without_pointer_opens_offset_from_the_source() {
    let displays = [rect(0.0, 0.0, 1920.0, 1080.0)];
    let near = rect(100.0, 100.0, 1200.0, 760.0);
    let placed = new_window_bounds(None, size(px(900.0), px(500.0)), near, &displays);
    let offset = theme::DETACHED_OFFSET;
    assert_eq!(placed, rect(100.0 + offset, 100.0 + offset, 900.0, 500.0));
}

#[test]
fn bounds_off_every_display_move_to_the_primary() {
    let primary = rect(0.0, 0.0, 1920.0, 1080.0);
    let displays = [primary];
    let saved = rect(2500.0, 100.0, 800.0, 600.0);
    let fitted = fit_on_displays(saved, &displays, Some(primary));
    assert_eq!(
        fitted,
        rect(560.0, 240.0, 800.0, 600.0),
        "centered on the primary"
    );
    let on_screen = rect(100.0, 100.0, 800.0, 600.0);
    assert_eq!(
        fit_on_displays(on_screen, &displays, Some(primary)),
        on_screen
    );
    assert_eq!(
        fit_on_displays(saved, &[], None),
        saved,
        "no display known: unchanged"
    );
}
