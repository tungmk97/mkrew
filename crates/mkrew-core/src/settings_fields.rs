//! The settings dialog's table (SPEC §20.0, ADR 0073): one [`Setting`] per `settings.toml`
//! key, so the UI builds its rows from data and a test keeps the table and the template in step.

use std::path::Path;

use crate::settings::{Settings, SettingsError, THEME_NAMES, write_value};

/// A page of the dialog (Keyboard is the UI's own).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    General,
    Appearance,
    Notifications,
    Voice,
    Jira,
    Phone,
    Features,
}

impl Page {
    pub const ALL: [Page; 7] = [
        Page::General,
        Page::Appearance,
        Page::Notifications,
        Page::Voice,
        Page::Jira,
        Page::Phone,
        Page::Features,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Appearance => "Appearance",
            Page::Notifications => "Notifications",
            Page::Voice => "Voice",
            Page::Jira => "Jira",
            Page::Phone => "Phone",
            Page::Features => "Features",
        }
    }
}

/// How a value is edited. Ranges are checked by serde when the file is parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    /// One of these TOML strings.
    Choice(&'static [&'static str]),
    /// A whole number.
    Number,
    Text,
    /// An installed font family; empty = the built-in default.
    Font,
    /// One of these names, picked from a searchable list (too many for `Choice`).
    List(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setting {
    pub page: Page,
    pub section: &'static str,
    pub key: &'static str,
    pub kind: Kind,
    pub label: &'static str,
    pub help: &'static str,
    /// Set by another control (dragging a panel edge); listed for the template check only.
    pub hidden: bool,
}

const fn row(
    page: Page,
    section: &'static str,
    key: &'static str,
    kind: Kind,
    label: &'static str,
    help: &'static str,
) -> Setting {
    Setting {
        page,
        section,
        key,
        kind,
        label,
        help,
        hidden: false,
    }
}

const fn hidden(section: &'static str, key: &'static str) -> Setting {
    Setting {
        page: Page::Appearance,
        section,
        key,
        kind: Kind::Number,
        label: "",
        help: "",
        hidden: true,
    }
}

use Kind::{Bool, Choice, Font, List, Number, Text};
use Page::{Appearance, Features, General, Jira, Notifications, Phone, Voice};

/// Every `settings.toml` key, in page order.
pub const SETTINGS: &[Setting] = &[
    row(
        General,
        "awake",
        "keep_awake",
        Bool,
        "Keep the PC awake",
        "While an agent works, the PC does not sleep.",
    ),
    row(
        General,
        "composer",
        "file_ref",
        Text,
        "File reference",
        "How a picked file is typed in the composer; {path} is the path.",
    ),
    row(
        General,
        "zed",
        "follow_debounce_ms",
        Number,
        "Follow in Zed wait",
        "Milliseconds after the agent's last edit before Zed opens the file.",
    ),
    row(
        General,
        "ui",
        "reading_width",
        Bool,
        "Reading width",
        "Agent panes start narrow (about 120–140 columns, centered).",
    ),
    row(
        General,
        "ui",
        "close_to_tray",
        Bool,
        "Close to tray",
        "Windows: the window's ✕ hides Mkrew to the tray icon.",
    ),
    row(
        Appearance,
        "ui",
        "theme",
        Choice(&["system", "light", "dark"]),
        "Theme",
        "System follows the OS.",
    ),
    row(
        Appearance,
        "ui",
        "light_theme",
        List(THEME_NAMES),
        "Light mode theme",
        "Used in Light mode and when the OS is light.",
    ),
    row(
        Appearance,
        "ui",
        "dark_theme",
        List(THEME_NAMES),
        "Dark mode theme",
        "Used in Dark mode and when the OS is dark.",
    ),
    row(
        Appearance,
        "ui",
        "font_family",
        Font,
        "App font",
        "An installed font; Default is the system UI font.",
    ),
    row(
        Appearance,
        "ui",
        "font_size",
        Number,
        "App font size",
        "8–32 px.",
    ),
    row(
        Appearance,
        "terminal",
        "font_family",
        Font,
        "Code font",
        "Terminal, Files and Git; pick a monospace font.",
    ),
    row(
        Appearance,
        "terminal",
        "font_size",
        Number,
        "Code font size",
        "8–32 px.",
    ),
    row(
        Appearance,
        "ui",
        "diff_mode",
        Choice(&["unified", "split"]),
        "Diff layout",
        "Unified: one column. Split: old | new.",
    ),
    row(
        Appearance,
        "ui",
        "diff_wrap",
        Bool,
        "Wrap long diff lines",
        "Off: lines are cut at the pane edge.",
    ),
    hidden("ui", "inspector_width"),
    hidden("ui", "sidebar_width"),
    row(
        Notifications,
        "notify",
        "needs_input",
        Bool,
        "When an agent needs you",
        "An OS notification.",
    ),
    row(
        Notifications,
        "notify",
        "done",
        Bool,
        "When an agent is done",
        "An OS notification.",
    ),
    row(
        Notifications,
        "notify",
        "sound",
        Bool,
        "Sound",
        "Notifications play the OS sound.",
    ),
    row(
        Notifications,
        "notify",
        "flash_taskbar",
        Bool,
        "Flash the taskbar",
        "Windows: while an agent needs you and Mkrew is not focused.",
    ),
    row(
        Voice,
        "voice",
        "base_url",
        Text,
        "API URL",
        "An OpenAI-compatible speech-to-text API.",
    ),
    row(Voice, "voice", "model", Text, "Model", ""),
    row(
        Voice,
        "voice",
        "api_key_env",
        Text,
        "API key variable",
        "The environment variable holding the key; empty: no key. Restart Mkrew after setting it.",
    ),
    row(
        Voice,
        "voice",
        "language",
        Text,
        "Language",
        "Empty detects it per recording; vi or en forces one.",
    ),
    row(
        Voice,
        "voice",
        "prompt",
        Text,
        "Spelling hint",
        "Words the model should spell as written.",
    ),
    row(
        Voice,
        "voice",
        "max_seconds",
        Number,
        "Longest recording",
        "1–600 seconds.",
    ),
    row(
        Jira,
        "jira",
        "site",
        Text,
        "Site",
        "Like acme.atlassian.net; empty turns Jira off.",
    ),
    row(
        Jira,
        "jira",
        "email",
        Text,
        "Email",
        "Your Atlassian account email.",
    ),
    row(
        Jira,
        "jira",
        "api_token_env",
        Text,
        "API token variable",
        "The environment variable holding the token.",
    ),
    row(
        Jira,
        "jira",
        "mine",
        Bool,
        "Only my issues",
        "The Board lists only issues assigned to you (its All | Mine filter writes it).",
    ),
    row(
        Phone,
        "web",
        "enabled",
        Bool,
        "Phone access",
        "The daemon runs mk web for phones on your tailnet.",
    ),
    row(
        Phone,
        "web",
        "push",
        Bool,
        "Phone notifications",
        "mk web notifies phones that opted in.",
    ),
    row(
        Features,
        "features",
        "docs",
        Bool,
        "Docs view",
        "Markdown docs with comments.",
    ),
    row(
        Features,
        "features",
        "turns",
        Bool,
        "Turn snapshots",
        "The agent's Turns… dialog; applies to sessions started after the change.",
    ),
];

impl Setting {
    /// `section.key`, e.g. `ui.theme`.
    pub fn id(&self) -> String {
        format!("{}.{}", self.section, self.key)
    }

    /// The value in `settings` as the dialog shows it: `true`, `dark`, `13`, or the raw text.
    pub fn current(&self, settings: &Settings) -> String {
        // Never fails in practice: every settings type is plain data with string keys.
        let Ok(toml::Value::Table(table)) = toml::Value::try_from(settings) else {
            return String::new();
        };
        match table
            .get(self.section)
            .and_then(|section| section.get(self.key))
        {
            Some(toml::Value::String(text)) => text.clone(),
            Some(value) => value.to_string(),
            None => String::new(),
        }
    }

    /// `text` as the TOML literal written for this setting. A number with other characters is
    /// written as a string, so the parse refuses it with serde's own message.
    pub fn literal(&self, text: &str) -> String {
        let text = if self.kind == Kind::Text {
            text
        } else {
            text.trim()
        };
        match self.kind {
            Kind::Bool => text.to_owned(),
            Kind::Number if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
                text.to_owned()
            }
            _ => toml::Value::String(text.to_owned()).to_string(),
        }
    }

    /// Saves `text` (see [`Self::literal`]); nothing is written when the result does not parse.
    pub fn write(&self, path: &Path, text: &str) -> Result<(), SettingsError> {
        write_value(path, self.section, self.key, &self.literal(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Settings, load, template};
    use std::collections::BTreeSet;

    /// `section.key` of every key the template lists (commented or not).
    fn template_keys() -> BTreeSet<String> {
        let mut section = String::new();
        let mut keys = BTreeSet::new();
        for line in template().lines() {
            let line = line.trim_start_matches('#').trim();
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.to_owned();
            } else if let Some((key, _)) = line.split_once(" = ")
                && !key.contains(' ')
                && !section.is_empty()
            {
                keys.insert(format!("{section}.{key}"));
            }
        }
        keys
    }

    fn setting(id: &str) -> &'static Setting {
        SETTINGS.iter().find(|s| s.id() == id).unwrap()
    }

    #[test]
    fn every_template_key_has_exactly_one_setting() {
        let table: Vec<String> = SETTINGS.iter().map(Setting::id).collect();
        let unique: BTreeSet<String> = table.iter().cloned().collect();
        assert_eq!(unique.len(), table.len(), "a key is listed twice");
        assert_eq!(unique, template_keys());
    }

    #[test]
    fn writing_each_default_reads_it_back_and_keeps_comments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let defaults = Settings::default();
        for setting in SETTINGS {
            let text = setting.current(&defaults);
            setting
                .write(&path, &text)
                .unwrap_or_else(|e| panic!("{}: {e}", setting.id()));
            let loaded = load(&path).unwrap();
            assert_eq!(setting.current(&loaded), text, "{}", setting.id());
        }
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.starts_with("# Mkrew settings"),
            "the template comments stay"
        );
        assert_eq!(load(&path).unwrap(), defaults);
    }

    #[test]
    fn out_of_range_values_are_refused_and_the_file_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "[ui]\nfont_size = 14\n").unwrap();
        for (id, text, message) in [
            ("ui.font_size", "40", "font_size must be 8–32 px, got 40"),
            (
                "voice.max_seconds",
                "0",
                "max_seconds must be 1–600 seconds, got 0",
            ),
        ] {
            let error = setting(id).write(&path, text).unwrap_err();
            assert!(error.to_string().contains(message), "{id}: {error}");
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[ui]\nfont_size = 14\n"
        );
    }

    #[test]
    fn a_number_with_letters_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        assert!(setting("ui.font_size").write(&path, "1a").is_err());
        assert!(!path.exists(), "nothing written");
    }

    #[test]
    fn text_is_written_as_an_escaped_toml_string() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let text = r#"Sửa "lỗi" C:\repo"#;
        setting("voice.prompt").write(&path, text).unwrap();
        assert_eq!(load(&path).unwrap().voice.prompt, text);
    }

    #[test]
    fn values_show_as_their_toml_names() {
        let defaults = Settings::default();
        assert_eq!(setting("ui.theme").current(&defaults), "system");
        assert_eq!(setting("ui.diff_wrap").current(&defaults), "true");
        assert_eq!(setting("ui.font_size").current(&defaults), "13");
        assert_eq!(setting("composer.file_ref").current(&defaults), "@{path}");
    }
}
