//! An empty GPUI window: `tests/measure.rs` samples its idle CPU as the framework's floor next to
//! Mkrew's (SPEC §15; GPUI on Windows wakes every window on each vblank even when nothing is
//! dirty, see ADR 0009). Logs `first frame` on stderr when it first renders.

use gpui::{App, AppContext, Context, IntoElement, Render, Window, WindowOptions, div};

struct Empty {
    logged: bool,
}

impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        if !self.logged {
            self.logged = true;
            eprintln!("first frame");
        }
        div()
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        if let Err(error) = cx.open_window(WindowOptions::default(), |_, cx| {
            cx.new(|_| Empty { logged: false })
        }) {
            eprintln!("cannot open the window: {error:#}");
            cx.quit();
        }
    });
}
