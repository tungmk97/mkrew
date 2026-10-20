//! Raster icons for the OS shell (ADR 0064): the taskbar badge and the tray's state dot. Pure,
//! BGRA top-down, hard edges (alpha 0 or 255, so premultiplying changes nothing). Digits come
//! from a 3×5 pixel font scaled by whole pixels: no font or GDI text is needed.

/// Digits 0–9 and `+`, one byte per row, the low three bits from left to right.
const FONT: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b010, 0b010, 0b010], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
    [0b000, 0b010, 0b111, 0b010, 0b000], // +
];

/// A square picture, 4 bytes per pixel in BGRA order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub size: u32,
    pub bgra: Vec<u8>,
}

impl Bitmap {
    fn new(size: u32) -> Self {
        Self {
            size,
            bgra: vec![0; (size * size * 4) as usize],
        }
    }

    fn fill(&mut self, x: u32, y: u32, rgb: u32) {
        let at = ((y * self.size + x) * 4) as usize;
        self.bgra[at..at + 4].copy_from_slice(&[
            rgb as u8,
            (rgb >> 8) as u8,
            (rgb >> 16) as u8,
            255,
        ]);
    }

    /// The BGRA pixel at (`x`, `y`).
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * self.size + x) * 4) as usize;
        [
            self.bgra[at],
            self.bgra[at + 1],
            self.bgra[at + 2],
            self.bgra[at + 3],
        ]
    }
}

fn glyph(c: char) -> Option<&'static [u8; 5]> {
    match c {
        '0'..='9' => FONT.get(c as usize - '0' as usize),
        '+' => FONT.get(10),
        _ => None,
    }
}

/// The badge text: nothing for 0, `1`…`9`, then `9+`.
pub fn badge_text(count: usize) -> String {
    match count {
        0 => String::new(),
        1..=9 => count.to_string(),
        _ => "9+".to_owned(),
    }
}

/// The taskbar overlay: a rounded square (radius `size / 4`) in `bg` with the count in `ink`.
pub fn badge(count: usize, size: u32, bg: u32, ink: u32) -> Bitmap {
    let mut bitmap = Bitmap::new(size);
    let radius = size / 4;
    for y in 0..size {
        for x in 0..size {
            let cx = x.clamp(radius, size - 1 - radius);
            let cy = y.clamp(radius, size - 1 - radius);
            let (dx, dy) = (x.abs_diff(cx), y.abs_diff(cy));
            if dx * dx + dy * dy <= radius * radius {
                bitmap.fill(x, y, bg);
            }
        }
    }
    let text: Vec<&[u8; 5]> = badge_text(count).chars().filter_map(glyph).collect();
    if text.is_empty() {
        return bitmap;
    }
    let scale = (size / 8).max(1);
    let width = (text.len() as u32 * 4 - 1) * scale;
    let left = size.saturating_sub(width) / 2;
    let top = size.saturating_sub(5 * scale) / 2;
    for (index, rows) in text.iter().enumerate() {
        let origin = left + index as u32 * 4 * scale;
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..3u32 {
                if (*bits >> (2 - col)) & 1 == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        bitmap.fill(
                            origin + col * scale + dx,
                            top + row as u32 * scale + dy,
                            ink,
                        );
                    }
                }
            }
        }
    }
    bitmap
}

/// The tray's working / needs-you icon: a filled circle in `rgb`.
pub fn dot(size: u32, rgb: u32) -> Bitmap {
    let mut bitmap = Bitmap::new(size);
    let side = size as i32;
    for y in 0..size {
        for x in 0..size {
            // Pixel centres in half-pixel units from the middle.
            let (dx, dy) = (2 * x as i32 + 1 - side, 2 * y as i32 + 1 - side);
            if dx * dx + dy * dy <= side * side {
                bitmap.fill(x, y, rgb);
            }
        }
    }
    bitmap
}

#[cfg(test)]
mod tests {
    use super::*;

    const AMBER: u32 = 0xF5A623;
    const INK: u32 = 0x141519;

    fn bgra(rgb: u32) -> [u8; 4] {
        [rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8, 255]
    }

    fn count(bitmap: &Bitmap, rgb: u32) -> usize {
        let want = bgra(rgb);
        (0..bitmap.size)
            .flat_map(|y| (0..bitmap.size).map(move |x| (x, y)))
            .filter(|&(x, y)| bitmap.pixel(x, y) == want)
            .count()
    }

    #[test]
    fn the_label_caps_at_nine_plus() {
        assert_eq!(badge_text(0), "");
        assert_eq!(badge_text(1), "1");
        assert_eq!(badge_text(9), "9");
        assert_eq!(badge_text(10), "9+");
        assert_eq!(badge_text(250), "9+");
    }

    #[test]
    fn a_badge_is_a_rounded_amber_square_with_the_digit_in_ink() {
        let bitmap = badge(3, 16, AMBER, INK);
        assert_eq!((bitmap.size, bitmap.bgra.len()), (16, 16 * 16 * 4));
        assert_eq!(bitmap.pixel(0, 0)[3], 0, "the corner is cut");
        assert_eq!(bitmap.pixel(8, 1), bgra(AMBER));
        // A `3` has 11 lit cells of the 3×5 font, each drawn 2×2 at 16 px.
        assert_eq!(count(&bitmap, INK), 11 * 4);
    }

    #[test]
    fn nine_plus_fits_inside_the_square() {
        let bitmap = badge(12, 16, AMBER, INK);
        // `9` has 12 lit cells and `+` 5.
        assert_eq!(count(&bitmap, INK), (12 + 5) * 4);
        let columns: Vec<u32> = (0..16)
            .filter(|&x| (0..16).any(|y| bitmap.pixel(x, y) == bgra(INK)))
            .collect();
        assert_eq!((columns.first(), columns.last()), (Some(&1), Some(&14)));
    }

    #[test]
    fn a_zero_badge_is_a_plain_square() {
        assert_eq!(count(&badge(0, 16, AMBER, INK), INK), 0);
    }

    #[test]
    fn a_dot_is_round() {
        let bitmap = dot(16, AMBER);
        assert_eq!(bitmap.pixel(0, 0)[3], 0);
        assert_eq!(bitmap.pixel(8, 8), bgra(AMBER));
        assert_eq!(bitmap.pixel(15, 15)[3], 0);
        assert_eq!(dot(32, AMBER).bgra.len(), 32 * 32 * 4);
    }
}
