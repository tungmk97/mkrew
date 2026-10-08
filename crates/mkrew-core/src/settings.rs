//! User settings (SPEC §20.0): `<config>/mkrew/settings.toml`.
//!
//! A missing file means all defaults; a missing key means its default. Unknown keys are
//! errors, like in profiles. Callers keep their last good [`Settings`] when [`load`] fails.

use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::profile::line_col;

/// Contents of `settings.toml` (SPEC §20.0).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub notify: NotifySettings,
    pub awake: AwakeSettings,
    pub composer: ComposerSettings,
    pub zed: ZedSettings,
    /// `[ui]`: the app's font (SPEC §14, §20.0).
    pub ui: FontSettings,
    /// `[terminal]`: the terminal font; also used by the Files and Git code views.
    pub terminal: FontSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NotifySettings {
    /// OS notification on `needs_input`.
    pub needs_input: bool,
    /// OS notification on `done`.
    pub done: bool,
    /// The notification plays the OS default sound.
    pub sound: bool,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            needs_input: true,
            done: true,
            sound: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AwakeSettings {
    /// §20A.5.
    pub keep_awake: bool,
}

impl Default for AwakeSettings {
    fn default() -> Self {
        Self { keep_awake: true }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ComposerSettings {
    /// §20B.1: how a picked file is typed; `{path}` is replaced by the path.
    pub file_ref: String,
}

impl Default for ComposerSettings {
    fn default() -> Self {
        Self {
            file_ref: "@{path}".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ZedSettings {
    /// §20C.4.
    pub follow_debounce_ms: u64,
}

impl Default for ZedSettings {
    fn default() -> Self {
        Self {
            follow_debounce_ms: 1500,
        }
    }
}

/// `[ui]` / `[terminal]` font. An empty family is the built-in default (SPEC §14).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FontSettings {
    pub font_family: String,
    pub font_size: FontSize,
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            font_family: String::new(),
            font_size: FontSize(13),
        }
    }
}

/// A font size in px, 8–32 (checked when parsed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "u16")]
pub struct FontSize(pub u16);

impl TryFrom<u16> for FontSize {
    type Error = String;

    fn try_from(px: u16) -> Result<Self, String> {
        if (8..=32).contains(&px) {
            Ok(Self(px))
        } else {
            Err(format!("font_size must be 8–32 px, got {px}"))
        }
    }
}

/// The commented `settings.toml` written when the user opens settings and none exists:
/// every key at its default (SPEC §20.0).
pub fn template() -> &'static str {
    "# Mkrew settings (SPEC §20.0). Uncomment a line to change it; changes apply on save.\n\
     # A mistake is reported in the command palette and the last good settings stay.\n\
     \n\
     [notify]\n\
     # needs_input = true\n\
     # done = true\n\
     # sound = true\n\
     \n\
     [awake]\n\
     # keep_awake = true\n\
     \n\
     [composer]\n\
     # file_ref = \"@{path}\"\n\
     \n\
     [zed]\n\
     # follow_debounce_ms = 1500\n\
     \n\
     # App font: an installed family name (empty: the system UI font); size 8–32 px.\n\
     [ui]\n\
     # font_family = \"\"\n\
     # font_size = 13\n\
     \n\
     # Terminal, Files and Git code font: pick a monospace family (empty: Cascadia Code /\n\
     # SF Mono / JetBrains Mono, the first installed); size 8–32 px.\n\
     [terminal]\n\
     # font_family = \"\"\n\
     # font_size = 13\n"
}

/// A settings file that could not be loaded. Displays as `file[:line:column]: message`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}{}: {message}", file.display(), position(*line_col))]
pub struct SettingsError {
    pub file: PathBuf,
    /// 1-based line and character column; `None` when not tied to a position.
    pub line_col: Option<(usize, usize)>,
    pub message: String,
}

fn position(line_col: Option<(usize, usize)>) -> String {
    line_col.map_or_else(String::new, |(line, column)| format!(":{line}:{column}"))
}

/// Reads `path`. A missing file yields the defaults.
pub fn load(path: &Path) -> Result<Settings, SettingsError> {
    match std::fs::read_to_string(path) {
        Ok(src) => parse(path, &src),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(SettingsError {
            file: path.to_path_buf(),
            line_col: None,
            message: e.to_string(),
        }),
    }
}

/// Parses settings source. `file` is only used in error messages.
pub fn parse(file: &Path, src: &str) -> Result<Settings, SettingsError> {
    toml::from_str(src).map_err(|e: toml::de::Error| SettingsError {
        file: file.to_path_buf(),
        line_col: e.span().map(|span| line_col(src, span.start)),
        message: e.message().trim_end().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_err(src: &str) -> String {
        parse(Path::new("settings.toml"), src)
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn missing_file_is_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let settings = load(&dir.path().join("settings.toml")).unwrap();
        assert_eq!(settings, Settings::default());
        assert!(settings.notify.needs_input && settings.notify.done && settings.notify.sound);
        assert!(settings.awake.keep_awake);
        assert_eq!(settings.composer.file_ref, "@{path}");
        assert_eq!(settings.zed.follow_debounce_ms, 1500);
        assert_eq!(
            settings.ui,
            FontSettings {
                font_family: String::new(),
                font_size: FontSize(13)
            }
        );
        assert_eq!(settings.terminal, settings.ui);
    }

    #[test]
    fn empty_file_is_defaults() {
        let settings = parse(Path::new("settings.toml"), "").unwrap();
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn partial_file_keeps_other_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(
            &path,
            "[notify]\nsound = false\n\n[zed]\nfollow_debounce_ms = 3\n",
        )
        .unwrap();
        let settings = load(&path).unwrap();
        let mut expected = Settings::default();
        expected.notify.sound = false;
        expected.zed.follow_debounce_ms = 3;
        assert_eq!(settings, expected);
    }

    #[test]
    fn full_file_parses() {
        let src = r#"
[notify]
needs_input = false
done = false
sound = false

[awake]
keep_awake = false

[composer]
file_ref = "{path}"

[zed]
follow_debounce_ms = 500

[ui]
font_family = "Inter"
font_size = 14

[terminal]
font_family = "JetBrains Mono"
font_size = 15
"#;
        let settings = parse(Path::new("settings.toml"), src).unwrap();
        assert_eq!(
            settings,
            Settings {
                notify: NotifySettings {
                    needs_input: false,
                    done: false,
                    sound: false,
                },
                awake: AwakeSettings { keep_awake: false },
                composer: ComposerSettings {
                    file_ref: "{path}".into()
                },
                zed: ZedSettings {
                    follow_debounce_ms: 500
                },
                ui: FontSettings {
                    font_family: "Inter".into(),
                    font_size: FontSize(14),
                },
                terminal: FontSettings {
                    font_family: "JetBrains Mono".into(),
                    font_size: FontSize(15),
                },
            }
        );
    }

    #[test]
    fn unknown_key_is_an_error() {
        let message = parse_err("[notify]\nsond = false\n");
        assert!(
            message.starts_with("settings.toml:2:1: unknown field `sond`"),
            "{message}"
        );
        let message = parse_err("[awak]\nkeep_awake = true\n");
        assert!(message.starts_with("settings.toml:1:"), "{message}");
        assert!(message.contains("unknown field `awak`"), "{message}");
    }

    #[test]
    fn bad_type_is_an_error() {
        let message = parse_err("[zed]\nfollow_debounce_ms = \"fast\"\n");
        assert!(message.starts_with("settings.toml:2:"), "{message}");
        assert!(message.contains("invalid type"), "{message}");
    }

    #[test]
    fn syntax_error_has_position() {
        let message = parse_err("[notify]\nsound = \n");
        assert!(message.starts_with("settings.toml:2:"), "{message}");
    }

    #[test]
    fn font_size_outside_8_to_32_is_an_error() {
        let message = parse_err("[terminal]\nfont_size = 4\n");
        assert!(message.starts_with("settings.toml:2:"), "{message}");
        assert!(message.contains("8–32"), "{message}");
        let message = parse_err("[ui]\nfont_size = 33\n");
        assert!(message.contains("8–32"), "{message}");
        let ok = parse(
            Path::new("settings.toml"),
            "[ui]\nfont_size = 8\n[terminal]\nfont_size = 32\nfont_family = \"Consolas\"\n",
        )
        .unwrap();
        assert_eq!(ok.ui.font_size, FontSize(8));
        assert_eq!(ok.terminal.font_size, FontSize(32));
        assert_eq!(ok.terminal.font_family, "Consolas");
        assert_eq!(
            ok.ui.font_family, "",
            "a missing family is the built-in default"
        );
    }

    #[test]
    fn the_template_parses_to_the_defaults_commented_or_not() {
        assert_eq!(
            parse(Path::new("settings.toml"), template()).unwrap(),
            Settings::default()
        );
        let uncommented: String = template()
            .lines()
            .map(|line| {
                line.strip_prefix("# ")
                    .filter(|l| l.contains(" = "))
                    .unwrap_or(line)
            })
            .map(|line| format!("{line}\n"))
            .collect();
        assert_eq!(
            parse(Path::new("settings.toml"), &uncommented).unwrap(),
            Settings::default(),
            "{uncommented}"
        );
    }

    #[test]
    fn unreadable_path_is_an_error() {
        // A directory cannot be read as a file.
        let dir = tempfile::tempdir().unwrap();
        let err = load(dir.path()).unwrap_err();
        assert_eq!(err.line_col, None);
        assert_eq!(err.file, dir.path());
    }
}
