//! Theme files (ADR 0101): `<config>/themes/*.toml`, a name plus the fields of
//! [`Palette`]. Pure: the catalog reads the folder, this module only turns text into a
//! palette and back.

use serde::Deserialize;

use crate::theme::Palette;

/// A parsed theme file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeFile {
    pub name: String,
    pub palette: Palette,
}

/// The keys of a theme file, all required; an unknown key is an error (a typo must not pass
/// silently).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    name: String,
    dark: bool,
    bg: String,
    chrome: String,
    surface: String,
    fg: String,
    muted: String,
    accent: String,
    red: String,
    green: String,
    yellow: String,
    blue: String,
    magenta: String,
    cyan: String,
    ansi: Vec<String>,
}

/// `#RRGGBB` (any case) as `0xRRGGBB`; the error names `key`.
fn color(key: &str, text: &str) -> Result<u32, String> {
    text.strip_prefix('#')
        .filter(|hex| hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .ok_or_else(|| format!("{key}: {text:?} is not a color like #RRGGBB"))
}

/// Reads a theme file. The error is one line a person can act on: the TOML line and message,
/// or the key that is missing, unknown or wrong.
pub fn parse(src: &str) -> Result<ThemeFile, String> {
    let raw: Raw = toml::from_str(src).map_err(|error| {
        let Some(span) = error.span() else {
            return error.message().trim_end().to_owned();
        };
        let before = src.get(..span.start).unwrap_or_default();
        let line = before.matches('\n').count() + 1;
        // A wrong-type value: name the key from the `key = ` in front of the value.
        let key = before
            .rsplit('\n')
            .next()
            .and_then(|head| head.trim_end().strip_suffix('='))
            .filter(|_| error.message().starts_with("invalid type"))
            .map_or(String::new(), |key| format!("{}: ", key.trim()));
        format!("line {line}: {key}{}", error.message().trim_end())
    })?;
    let name = raw.name.trim().to_owned();
    if name.is_empty() {
        return Err("name: must not be empty".to_owned());
    }
    if name.chars().any(char::is_control) {
        return Err("name: must not contain control characters".to_owned());
    }
    let ansi = raw
        .ansi
        .iter()
        .enumerate()
        .map(|(i, text)| color(&format!("ansi[{i}]"), text))
        .collect::<Result<Vec<_>, _>>()?;
    let ansi = <[u32; 16]>::try_from(ansi)
        .map_err(|colors| format!("ansi: needs 16 colors, found {}", colors.len()))?;
    let palette = Palette {
        dark: raw.dark,
        bg: color("bg", &raw.bg)?,
        chrome: color("chrome", &raw.chrome)?,
        surface: color("surface", &raw.surface)?,
        fg: color("fg", &raw.fg)?,
        muted: color("muted", &raw.muted)?,
        accent: color("accent", &raw.accent)?,
        red: color("red", &raw.red)?,
        green: color("green", &raw.green)?,
        yellow: color("yellow", &raw.yellow)?,
        blue: color("blue", &raw.blue)?,
        magenta: color("magenta", &raw.magenta)?,
        cyan: color("cyan", &raw.cyan)?,
        ansi,
    };
    Ok(ThemeFile { name, palette })
}

/// The text of a theme file for `palette` named `name` ("New theme from current").
pub fn render(name: &str, palette: &Palette) -> String {
    let color = |value: u32| format!("\"#{value:06x}\"");
    let row = |colors: &[u32]| {
        colors
            .iter()
            .map(|c| color(*c))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = String::from("# A Mkrew theme. Every key is required; colors are #RRGGBB.\n");
    let mut line = |key: &str, value: String| {
        text.push_str(key);
        text.push_str(" = ");
        text.push_str(&value);
        text.push('\n');
    };
    line("name", toml::Value::String(name.to_owned()).to_string());
    line("dark", palette.dark.to_string());
    for (key, value) in [
        ("bg", palette.bg),
        ("chrome", palette.chrome),
        ("surface", palette.surface),
        ("fg", palette.fg),
        ("muted", palette.muted),
        ("accent", palette.accent),
        ("red", palette.red),
        ("green", palette.green),
        ("yellow", palette.yellow),
        ("blue", palette.blue),
        ("magenta", palette.magenta),
        ("cyan", palette.cyan),
    ] {
        line(key, color(value));
    }
    line(
        "ansi",
        format!(
            "[\n  {},\n  {},\n]",
            row(&palette.ansi[..8]),
            row(&palette.ansi[8..])
        ),
    );
    text
}

/// `<name>.toml` with every character a file name cannot hold replaced by `_`.
pub fn file_name(theme_name: &str) -> String {
    let stem: String = theme_name
        .chars()
        .map(|c| {
            if c.is_control() || "\\/:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let stem = stem
        .char_indices()
        .nth(100)
        .map_or(&stem[..], |(end, _)| &stem[..end]);
    let stem = stem.trim_end_matches(['.', ' ']);
    let device = stem
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (device.len() == 4
            && (device.starts_with("COM") || device.starts_with("LPT"))
            && matches!(device.as_bytes()[3], b'1'..=b'9'));
    match (stem.is_empty(), reserved) {
        (true, _) => "theme.toml".to_owned(),
        (_, true) => format!("_{stem}.toml"),
        _ => format!("{stem}.toml"),
    }
}

/// The most numbers [`copy_name`] tries.
const MAX_COPIES: u32 = 1000;
/// The longest base [`copy_name`] keeps, in characters: with " copy 1000" (10) it fits the
/// 100 characters [`file_name`] keeps, so every number is a different file.
const MAX_COPY_BASE_CHARS: usize = 90;

/// The name of a copy of `name`: "<name> copy", then "<name> copy 2", "<name> copy 3", ...
/// the first one `taken` does not know; `None` when the first [`MAX_COPIES`] are all taken.
/// A name over [`MAX_COPY_BASE_CHARS`] characters is cut first.
pub fn copy_name(name: &str, taken: impl Fn(&str) -> bool) -> Option<String> {
    let base = name
        .char_indices()
        .nth(MAX_COPY_BASE_CHARS)
        .map_or(name, |(end, _)| &name[..end]);
    (1..=MAX_COPIES)
        .map(|number| match number {
            1 => format!("{base} copy"),
            _ => format!("{base} copy {number}"),
        })
        .find(|candidate| !taken(candidate))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
name = "My Theme"
dark = true
bg = "#1e2030"
chrome = "#191a2a"
surface = "#222436"
fg = "#c8d3f5"
muted = "#828bb8"
accent = "#82aaff"
red = "#ff757f"
green = "#c3e88d"
yellow = "#ffc777"
blue = "#82aaff"
magenta = "#c099ff"
cyan = "#86e1fc"
ansi = ["#1b1d2b", "#ff757f", "#c3e88d", "#ffc777", "#82aaff", "#c099ff", "#86e1fc", "#828bb8",
        "#444a73", "#ff8d94", "#c7fb6d", "#ffd8ab", "#9ab8ff", "#caabff", "#b2ebff", "#c8d3f5"]
"##;

    #[test]
    fn a_valid_file_gives_the_palette() {
        let file = parse(SAMPLE).unwrap();
        assert_eq!(file.name, "My Theme");
        assert!(file.palette.dark);
        assert_eq!(file.palette.bg, 0x1E2030);
        assert_eq!(file.palette.cyan, 0x86E1FC);
        assert_eq!(file.palette.ansi[0], 0x1B1D2B);
        assert_eq!(file.palette.ansi[15], 0xC8D3F5);
    }

    #[test]
    fn colors_are_case_insensitive() {
        let upper = SAMPLE.replace("#1e2030", "#1E2030");
        assert_eq!(parse(&upper).unwrap(), parse(SAMPLE).unwrap());
    }

    #[test]
    fn a_missing_key_is_an_error_naming_it() {
        let text = SAMPLE.replace("cyan = \"#86e1fc\"\n", "");
        let error = parse(&text).unwrap_err();
        assert!(error.contains("cyan"), "{error}");
    }

    #[test]
    fn an_unknown_key_is_an_error_naming_it() {
        let text = format!("{SAMPLE}bgg = \"#000000\"\n");
        let error = parse(&text).unwrap_err();
        assert!(error.contains("bgg"), "{error}");
    }

    #[test]
    fn a_bad_color_names_its_key_and_value() {
        for bad in ["#12345", "#gggggg", "123456", "#1234567"] {
            let text = SAMPLE.replace("\"#1e2030\"", &format!("\"{bad}\""));
            let error = parse(&text).unwrap_err();
            assert!(error.contains("bg:") && error.contains(bad), "{error}");
        }
        let text = SAMPLE.replace("\"#ff757f\", \"#c3e88d\"", "\"#ff757f\", \"nope\"");
        assert!(parse(&text).unwrap_err().contains("ansi[2]"));
    }

    #[test]
    fn a_short_ansi_list_is_an_error_naming_ansi() {
        let text = SAMPLE.replace(", \"#c8d3f5\"]", "]");
        let error = parse(&text).unwrap_err();
        assert!(error.contains("ansi") && error.contains("16"), "{error}");
    }

    #[test]
    fn a_syntax_error_names_its_line() {
        let error = parse("name = \"x\"\ndark = maybe\n").unwrap_err();
        assert!(error.contains("line 2"), "{error}");
    }

    #[test]
    fn a_name_must_be_text_a_person_can_read() {
        for bad in ["", "   ", "a\\nb"] {
            let text = SAMPLE.replace("My Theme", bad);
            let error = parse(&text).unwrap_err();
            assert!(error.contains("name"), "{bad:?}: {error}");
        }
        let padded = SAMPLE.replace("My Theme", "  Padded  ");
        assert_eq!(parse(&padded).unwrap().name, "Padded");
    }

    #[test]
    fn a_rendered_file_parses_back_to_the_same_palette() {
        let original = parse(SAMPLE).unwrap();
        for name in [
            "My Theme copy",
            "Rosé Pine",
            "He said \"hi\"",
            "back\\slash",
        ] {
            let text = render(name, &original.palette);
            let back = parse(&text).unwrap();
            assert_eq!(back.name, name);
            assert_eq!(back.palette, original.palette);
        }
    }

    #[test]
    fn file_names_never_hold_what_a_path_cannot() {
        assert_eq!(file_name("Nord copy"), "Nord copy.toml");
        assert_eq!(
            file_name("a/b\\c:d*e?f\"g<h>i|j"),
            "a_b_c_d_e_f_g_h_i_j.toml"
        );
        assert_eq!(file_name("Rosé Pine copy"), "Rosé Pine copy.toml");
        assert_eq!(file_name("trailing. "), "trailing.toml");
        assert_eq!(file_name("..."), "theme.toml");
    }

    #[test]
    fn reserved_device_names_and_long_names_are_made_safe() {
        assert_eq!(file_name("CON"), "_CON.toml");
        assert_eq!(file_name("nul.txt"), "_nul.txt.toml");
        assert_eq!(file_name("Lpt9"), "_Lpt9.toml");
        assert_eq!(file_name("COM0"), "COM0.toml");
        assert_eq!(file_name("Console"), "Console.toml");
        let long = file_name(&"é".repeat(300));
        assert_eq!(long, format!("{}.toml", "é".repeat(100)));
    }

    #[test]
    fn a_wrong_type_names_its_key_on_one_line() {
        let error = parse(&SAMPLE.replace("\"#1e2030\"", "5")).unwrap_err();
        assert!(error.contains("bg") && !error.contains('\n'), "{error}");
        let error = parse(&format!("{SAMPLE}\n").replacen("ansi = [", "ansi = \"x\"\nold = [", 1))
            .unwrap_err();
        assert!(error.contains("ansi") && !error.contains('\n'), "{error}");
        let error =
            parse(&SAMPLE.replace("[\"#1b1d2b\"", "[\"#1b1d2b\", \"#000000\"")).unwrap_err();
        assert!(error.contains("found 17"), "{error}");
        let error = parse(&SAMPLE.replace("\"#1e2030\"", "\"a\\nb\"")).unwrap_err();
        assert!(!error.contains('\n'), "{error}");
    }

    #[test]
    fn a_long_name_still_gets_a_new_distinct_file_name() {
        let name = "n".repeat(150);
        let mut used = vec![];
        for _ in 0..3 {
            let copy = copy_name(&name, |c| used.contains(&file_name(c))).unwrap();
            let file = file_name(&copy);
            assert!(!used.contains(&file));
            used.push(file);
        }
    }

    #[test]
    fn a_copy_gives_up_when_every_number_is_taken() {
        assert_eq!(copy_name("Nord", |_| true), None);
    }

    #[test]
    fn a_copy_takes_the_first_free_number() {
        let taken = |names: &'static [&'static str]| move |n: &str| names.contains(&n);
        let copy = |name, names| copy_name(name, taken(names)).unwrap();
        assert_eq!(copy("Nord", &[]), "Nord copy");
        assert_eq!(copy("Nord", &["Nord copy"]), "Nord copy 2");
        assert_eq!(copy("Nord", &["Nord copy", "Nord copy 2"]), "Nord copy 3");
        assert_eq!(copy("Nord copy", &["Nord copy copy"]), "Nord copy copy 2");
    }
}
