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
    pub tasks: TasksSettings,
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
    /// §20D.1; empty = off.
    pub webhook: String,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            needs_input: true,
            done: true,
            sound: true,
            webhook: String::new(),
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TasksSettings {
    /// §20D.3: per workspace, 0 = no limit.
    pub max_running: u32,
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
        assert_eq!(settings.notify.webhook, "");
        assert!(settings.awake.keep_awake);
        assert_eq!(settings.composer.file_ref, "@{path}");
        assert_eq!(settings.zed.follow_debounce_ms, 1500);
        assert_eq!(settings.tasks.max_running, 0);
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
            "[notify]\nsound = false\n\n[tasks]\nmax_running = 3\n",
        )
        .unwrap();
        let settings = load(&path).unwrap();
        let mut expected = Settings::default();
        expected.notify.sound = false;
        expected.tasks.max_running = 3;
        assert_eq!(settings, expected);
    }

    #[test]
    fn full_file_parses() {
        let src = r#"
[notify]
needs_input = false
done = false
sound = false
webhook = "https://example.invalid/hook"

[awake]
keep_awake = false

[composer]
file_ref = "{path}"

[zed]
follow_debounce_ms = 500

[tasks]
max_running = 2
"#;
        let settings = parse(Path::new("settings.toml"), src).unwrap();
        assert_eq!(
            settings,
            Settings {
                notify: NotifySettings {
                    needs_input: false,
                    done: false,
                    sound: false,
                    webhook: "https://example.invalid/hook".into(),
                },
                awake: AwakeSettings { keep_awake: false },
                composer: ComposerSettings {
                    file_ref: "{path}".into()
                },
                zed: ZedSettings {
                    follow_debounce_ms: 500
                },
                tasks: TasksSettings { max_running: 2 },
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
        let message = parse_err("[tasks]\nmax_running = -1\n");
        assert!(message.starts_with("settings.toml:2:"), "{message}");
        assert!(message.contains("invalid"), "{message}");
    }

    #[test]
    fn syntax_error_has_position() {
        let message = parse_err("[notify]\nsound = \n");
        assert!(message.starts_with("settings.toml:2:"), "{message}");
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
