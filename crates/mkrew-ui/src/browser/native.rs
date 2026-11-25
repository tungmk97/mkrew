//! The page of a browser pane: a `wry` web view (WebView2 / WKWebView) made a child of the
//! GPUI window, kept over the pane by [`WebSurface`] (modeled on longbridge `gpui-webview`,
//! which pins another gpui). A native view paints above everything GPUI draws and stays on
//! screen until hidden: the Shell hides it (spec 7.2).

use super::{PageEvent, keys, parse_ipc};
use gpui::{
    App, Bounds, ElementId, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId,
    IntoElement, LayoutId, MouseDownEvent, Pixels, Size, Style, Window,
};
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use ulid::Ulid;
use wry::dpi::{LogicalPosition, LogicalSize};
use wry::raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
use wry::{NewWindowResponse, PageLoadEvent, Rect, WebContext, WebViewBuilder};

/// The window a page is built in, by its raw handle. Building a page runs the message loop
/// (WebView2 waits for its environment), and that loop runs GPUI's other tasks and paints: a
/// task that then updates a window the build was started from fails. So the handle is copied
/// while the window is at hand and the page is built later, from a task that borrows nothing
/// (`Shell::prepare_browsers`, ADR 0114).
#[derive(Clone, Copy)]
pub struct Parent(RawWindowHandle);

impl Parent {
    pub fn of(window: &Window) -> Result<Self, String> {
        let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
        Ok(Self(handle.as_raw()))
    }
}

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: the handle must be valid while `build_as_child` runs. It was read from a
        // live GPUI window, and `Shell::prepare_browsers` checks that window is still open in
        // the same synchronous step, right before the build. What wry keeps of the parent
        // afterwards (focus, bounds) is what it kept when it was given the window itself.
        #[allow(unsafe_code)]
        Ok(unsafe { WindowHandle::borrow_raw(self.0) })
    }
}

pub struct NativeView {
    view: Rc<wry::WebView>,
    shown: Rc<Cell<bool>>,
    dark: Cell<bool>,
}

impl NativeView {
    /// Builds the page of browser pane `id` on `url` as a child of `parent` (hidden until its
    /// surface is drawn). Events go to `events` with the pane's id. Runs the message loop: call
    /// it with nothing of GPUI borrowed (see [`Parent`]).
    pub fn build(
        parent: &Parent,
        id: Ulid,
        url: &str,
        keys: &[String],
        dark: bool,
        data_dir: Option<&Path>,
        events: async_channel::Sender<(Ulid, PageEvent)>,
    ) -> Result<NativeView, String> {
        let mut context = WebContext::new(data_dir.map(Path::to_path_buf));
        let send = move |event: PageEvent| {
            let _ = events.try_send((id, event));
        };
        let (on_load, on_title, on_open, on_ipc) = (send.clone(), send.clone(), send.clone(), send);
        let builder = WebViewBuilder::new_with_web_context(&mut context)
            .with_url(url)
            .with_visible(false)
            .with_devtools(true)
            .with_initialization_script(keys::init_script(keys))
            .with_on_page_load_handler(move |event, url| {
                on_load(match event {
                    PageLoadEvent::Started => PageEvent::Started,
                    PageLoadEvent::Finished => PageEvent::Finished(url),
                })
            })
            .with_document_title_changed_handler(move |title| on_title(PageEvent::Title(title)))
            .with_new_window_req_handler(move |url, _| {
                on_open(PageEvent::Open(url));
                NewWindowResponse::Deny
            })
            .with_ipc_handler(move |request| {
                if let Some(event) = parse_ipc(request.body()) {
                    on_ipc(event);
                }
            });
        #[cfg(windows)]
        let builder = {
            use wry::WebViewBuilderExtWindows as _;
            builder.with_theme(theme(dark))
        };
        let view = builder
            .build_as_child(parent)
            .map_err(|error| error.to_string())?;
        Ok(NativeView {
            view: Rc::new(view),
            shown: Rc::new(Cell::new(false)),
            dark: Cell::new(dark),
        })
    }

    pub fn load(&self, url: &str) {
        if let Err(error) = self.view.load_url(url) {
            tracing::warn!("browser: load {url}: {error}");
        }
    }

    pub fn script(&self, js: &str) {
        if let Err(error) = self.view.evaluate_script(js) {
            tracing::warn!("browser: script: {error}");
        }
    }

    /// Shows or hides the page. Hiding never moves the keyboard (SetFocus on Windows can
    /// activate a background window): the Shell gives it back while this window is active.
    pub fn set_shown(&self, shown: bool) {
        if self.shown.get() == shown {
            return;
        }
        if let Err(error) = self.view.set_visible(shown) {
            tracing::warn!("browser: set_visible({shown}): {error}");
        }
        self.shown.set(shown);
    }

    /// Follows Mkrew's theme (`prefers-color-scheme`); macOS follows the app appearance itself.
    pub fn set_dark(&self, dark: bool) {
        if self.dark.replace(dark) != dark {
            #[cfg(windows)]
            {
                use wry::WebViewExtWindows as _;
                let _ = self.view.set_theme(theme(dark));
            }
        }
    }

    /// Whether the page is on screen (the Shell shows pages it draws and hides the rest).
    pub fn is_shown(&self) -> bool {
        self.shown.get()
    }

    pub fn focus(&self) {
        let _ = self.view.focus();
    }

    pub fn focus_parent(&self) {
        let _ = self.view.focus_parent();
    }

    /// The element that keeps the page over its bounds; `show` false while an overlay is open.
    pub fn surface(&self, show: bool) -> WebSurface {
        WebSurface {
            view: Rc::clone(&self.view),
            shown: Rc::clone(&self.shown),
            show,
        }
    }
}

impl Drop for NativeView {
    fn drop(&mut self) {
        self.set_shown(false);
    }
}

#[cfg(windows)]
fn theme(dark: bool) -> wry::Theme {
    if dark {
        wry::Theme::Dark
    } else {
        wry::Theme::Light
    }
}

/// Fills its parent; keeps the native page over its bounds (clipped to the content mask),
/// blocks GPUI's mouse under it, and gives the keyboard back to the window on a click outside.
pub struct WebSurface {
    view: Rc<wry::WebView>,
    shown: Rc<Cell<bool>>,
    show: bool,
}

impl IntoElement for WebSurface {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::Element for WebSurface {
    type RequestLayoutState = ();
    type PrepaintState = Option<Hitbox>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: Size::full(),
            flex_grow: 1.,
            flex_shrink: 1.,
            ..Style::default()
        };
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) -> Option<Hitbox> {
        let visible = bounds.intersect(&window.content_mask().bounds);
        if !self.show || visible.is_empty() {
            hide(&self.view, &self.shown);
            return None;
        }
        let px = |value: Pixels| f64::from(f32::from(value));
        let rect = Rect {
            position: LogicalPosition::new(px(visible.origin.x), px(visible.origin.y)).into(),
            size: LogicalSize::new(px(visible.size.width), px(visible.size.height)).into(),
        };
        if let Err(error) = self.view.set_bounds(rect) {
            tracing::warn!("browser: set_bounds: {error}");
            hide(&self.view, &self.shown);
            return None;
        }
        if !self.shown.get() {
            let _ = self.view.set_visible(true);
            self.shown.set(true);
        }
        Some(window.insert_hitbox(visible, HitboxBehavior::BlockMouse))
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        hitbox: &mut Option<Hitbox>,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let Some(bounds) = hitbox.as_ref().map(|hitbox| hitbox.bounds) else {
            return;
        };
        let view = Rc::clone(&self.view);
        window.on_mouse_event(move |event: &MouseDownEvent, _, _, _| {
            if !bounds.contains(&event.position) {
                let _ = view.focus_parent();
            }
        });
    }
}

fn hide(view: &wry::WebView, shown: &Cell<bool>) {
    if shown.replace(false) {
        let _ = view.set_visible(false);
    }
}
