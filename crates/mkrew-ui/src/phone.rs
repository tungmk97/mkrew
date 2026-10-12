//! Phone access, the pure pieces (spec 2026-10-12-phone-access-ui-design.md): the link the
//! phone opens, the access token `mk web` stored, and the QR code of the link as an SVG.
//! No GPUI here; the Phone access dialog shows the SVG with `gpui::img`. The link carries the
//! token: never log it.

use std::fmt::Write as _;
use std::path::Path;

use qrcodegen::{QrCode, QrCodeEcc};

/// Light border around the symbol, in modules (ISO/IEC 18004 asks for 4).
const QUIET_ZONE: i32 = 4;

/// Pixels per module in the SVG's own size. GPUI rasterizes an SVG at that size × 2 and then
/// scales the bitmap, so 8 px per module gives 16 bitmap px per module, crisp when shown smaller.
const MODULE_PX: i32 = 8;

/// What the phone opens: the token travels in the fragment, which the browser never sends.
pub fn link(host: &str, token: &str) -> String {
    format!("https://{host}/#t={token}")
}

/// The token `mk web` keeps in `<config_dir>/web_token`; `None` before its first start or when
/// the file does not hold a token (exactly 64 lowercase hex characters, like `mk web`'s rule).
pub fn read_token(config_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(config_dir.join("web_token")).ok()?;
    let token = text.trim();
    let valid = token.len() == 64
        && token
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    valid.then(|| token.to_owned())
}

/// `text` as a QR code (ECC Medium) drawn as an SVG: `dark` modules on a `light` panel, both
/// `#rrggbb` ([`crate::theme::hex`] of `qr_dark` / `qr_light`). `None` when the text is too
/// long for the largest QR code.
pub fn qr_svg(text: &str, dark: &str, light: &str) -> Option<String> {
    let qr = QrCode::encode_text(text, QrCodeEcc::Medium).ok()?;
    let size = qr.size();
    let side = size + 2 * QUIET_ZONE;
    // One rectangle per run of dark modules in a row: far fewer elements than one per module.
    let mut path = String::new();
    for y in 0..size {
        let mut x = 0;
        while x < size {
            if !qr.get_module(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < size && qr.get_module(x, y) {
                x += 1;
            }
            let width = x - start;
            // Writing to a String cannot fail.
            let _ = write!(
                path,
                "M{},{}h{width}v1h-{width}z",
                start + QUIET_ZONE,
                y + QUIET_ZONE
            );
        }
    }
    let pixels = side * MODULE_PX;
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{pixels}\" height=\"{pixels}\" \
         viewBox=\"0 0 {side} {side}\" shape-rendering=\"crispEdges\">\
         <rect width=\"{side}\" height=\"{side}\" fill=\"{light}\"/>\
         <path d=\"{path}\" fill=\"{dark}\"/></svg>"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// A config folder, with a `web_token` file holding `token_file` when given.
    fn config_with(token_file: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        if let Some(text) = token_file {
            std::fs::write(dir.path().join("web_token"), text).unwrap();
        }
        dir
    }

    /// ~110 chars, like the real link of a Tailscale name.
    fn typical_link() -> String {
        link("desktop-nguyentung.tail1234.ts.net", TOKEN)
    }

    /// The dark cells the SVG's path draws, as a `side` × `side` grid (quiet zone included).
    fn drawn_cells(svg: &str, side: usize) -> Vec<Vec<bool>> {
        let path = svg
            .split("<path d=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("a path");
        let mut grid = vec![vec![false; side]; side];
        for run in path.split('M').filter(|run| !run.is_empty()) {
            // One run of dark cells in a row: "x,yhWv1h-Wz".
            let (x, rest) = run.split_once(',').unwrap();
            let (y, rest) = rest.split_once('h').unwrap();
            let (width, rest) = rest.split_once('v').unwrap();
            assert_eq!(rest, format!("1h-{width}z"), "run {run}");
            let (x, y, width): (usize, usize, usize) = (
                x.parse().unwrap(),
                y.parse().unwrap(),
                width.parse().unwrap(),
            );
            for cell in &mut grid[y][x..x + width] {
                assert!(!*cell, "cell drawn twice in run {run}");
                *cell = true;
            }
        }
        grid
    }

    #[test]
    fn the_link_carries_the_token_in_the_fragment() {
        assert_eq!(
            link("pc.tail1234.ts.net", TOKEN),
            format!("https://pc.tail1234.ts.net/#t={TOKEN}")
        );
    }

    #[test]
    fn read_token_trims_and_accepts_only_64_lowercase_hex() {
        let read = |text: &str| read_token(config_with(Some(text)).path());
        assert_eq!(read(TOKEN).as_deref(), Some(TOKEN));
        assert_eq!(
            read(&format!("{TOKEN}\r\n")).as_deref(),
            Some(TOKEN),
            "an editor or `echo` adds a newline"
        );
        assert_eq!(read(&format!("  {TOKEN}\n")).as_deref(), Some(TOKEN));
        assert_eq!(read(&TOKEN.to_uppercase()), None, "mk web writes lowercase");
        assert_eq!(read(&TOKEN[..63]), None, "too short");
        assert_eq!(read(&format!("{TOKEN}0")), None, "too long");
        assert_eq!(read(&format!("g{}", &TOKEN[1..])), None, "not hex");
        assert_eq!(read(""), None);
        assert_eq!(
            read(&format!("{TOKEN}\n{TOKEN}\n")),
            None,
            "two tokens are not one"
        );
    }

    #[test]
    fn read_token_is_none_before_the_first_start() {
        let dir = config_with(None);
        assert_eq!(read_token(dir.path()), None, "no web_token file yet");
        assert_eq!(
            read_token(&dir.path().join("no-such-folder")),
            None,
            "no config folder yet"
        );
    }

    #[test]
    fn the_svg_draws_the_symbol_inside_a_four_module_quiet_zone() {
        let text = typical_link();
        let svg = qr_svg(&text, "#101114", "#ffffff").expect("a link fits a QR code");
        let qr = QrCode::encode_text(&text, QrCodeEcc::Medium).unwrap();
        let side = qr.size() + 8;
        assert!(svg.starts_with("<svg "), "{svg}");
        assert!(
            svg.contains(&format!("viewBox=\"0 0 {side} {side}\"")),
            "symbol + 4 modules each side: {side}"
        );
        assert!(
            svg.contains(&format!("width=\"{0}\" height=\"{0}\"", side * 8)),
            "8 px per module: GPUI rasterizes at twice the intrinsic size"
        );
        let cells = drawn_cells(&svg, side as usize);
        for y in 0..side {
            for x in 0..side {
                assert_eq!(
                    cells[y as usize][x as usize],
                    qr.get_module(x - 4, y - 4),
                    "module {x},{y} (quiet zone is light, finder patterns are where the spec puts them)"
                );
            }
        }
    }

    #[test]
    fn the_svg_has_the_top_left_finder_pattern_at_the_quiet_zone_offset() {
        // Fixed by ISO/IEC 18004, not read back from qrcodegen: a 7 x 7 finder (dark ring,
        // light ring, dark 3 x 3 core) at the symbol's top-left, then a light separator.
        let text = typical_link();
        let svg = qr_svg(&text, "#101114", "#ffffff").unwrap();
        let side = QrCode::encode_text(&text, QrCodeEcc::Medium)
            .unwrap()
            .size() as usize
            + 8;
        let cells = drawn_cells(&svg, side);
        for y in 0..7_usize {
            for x in 0..7_usize {
                let ring = (x.abs_diff(3)).max(y.abs_diff(3));
                assert_eq!(
                    cells[4 + y][4 + x],
                    ring != 2,
                    "finder cell {x},{y}: dark outer ring (3), light ring (2), dark core (<= 1)"
                );
            }
        }
        for i in 0..8 {
            assert!(!cells[4 + 7][4 + i], "separator row, cell {i}");
            assert!(!cells[4 + i][4 + 7], "separator column, cell {i}");
        }
        assert!(
            cells[..4].iter().flatten().all(|cell| !cell),
            "quiet zone above the symbol is light"
        );
    }

    #[test]
    fn the_svg_is_dark_modules_on_a_light_background() {
        let svg = qr_svg(&typical_link(), "#101114", "#ffffff").unwrap();
        let element = |tag: &str| {
            svg.split(tag)
                .nth(1)
                .and_then(|rest| rest.split("/>").next())
                .unwrap_or_default()
                .to_owned()
        };
        let (background, modules) = (element("<rect "), element("<path "));
        assert!(background.contains("#ffffff"), "{background}");
        assert!(!background.contains("#101114"), "{background}");
        assert!(modules.contains("#101114"), "{modules}");
        assert!(!modules.contains("#ffffff"), "{modules}");
        assert!(svg.contains("crispEdges"), "no blurred module edges");
    }

    #[test]
    fn the_svg_is_none_when_the_text_does_not_fit_any_version() {
        // Version 40 at ECC Medium holds 2331 bytes; lowercase ASCII is encoded as bytes.
        assert!(qr_svg(&"a".repeat(2331), "#000000", "#ffffff").is_some());
        assert_eq!(qr_svg(&"a".repeat(2332), "#000000", "#ffffff"), None);
    }
}
