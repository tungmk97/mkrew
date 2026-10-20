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
    /// `[ui]`: the app's font and theme (SPEC §14, §20.0).
    pub ui: UiSettings,
    /// `[terminal]`: the terminal font; also used by the Files and Git code views.
    pub terminal: FontSettings,
    /// `[voice]`: speech to text for voice input (SPEC §20G, ADR 0045).
    pub voice: VoiceSettings,
    /// `[features]`: optional modules, all off by default (ADR 0046).
    pub features: FeatureSettings,
    /// `[web]`: phone access (ADR 0053).
    pub web: WebSettings,
    /// `[jira]`: Jira issues on the Board (SPEC §20I, ADR 0066).
    pub jira: JiraSettings,
}

/// `[features]` (ADR 0046): modules that are off unless switched on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FeatureSettings {
    /// The Docs view (SPEC §12) and "Review in Docs".
    pub docs: bool,
    /// Turn snapshots and the agent pane's "Turns…" dialog (SPEC §20C.1–§20C.2).
    pub turns: bool,
}

/// `[web]` (ADR 0053, ADR 0067): the daemon runs `mk web` while `enabled` is true; `mk web`
/// sends phone notifications while `push` is true. The Phone access dialog writes both; the
/// port is fixed (`mkrew_core::tailscale::WEB_PORT`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebSettings {
    pub enabled: bool,
    /// Notifications to subscribed phones through their browser's push service (ADR 0067).
    pub push: bool,
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
    /// Windows: the taskbar button flashes while a session needs input and the window is not
    /// focused (SPEC §20J). The daemon ignores it; the UI reads it.
    pub flash_taskbar: bool,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            needs_input: true,
            done: true,
            sound: true,
            flash_taskbar: true,
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

/// The default `[voice] prompt`: words Whisper should spell as written, in both languages, so
/// English coding terms in Vietnamese speech stay English.
pub const VOICE_PROMPT_DEFAULT: &str = "Mkrew, Claude Code, Codex, commit, branch, merge, refactor, test, bug. Sửa lỗi này rồi chạy test lại nhé.";

/// `[voice]` (SPEC §20G): an OpenAI-compatible speech-to-text endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VoiceSettings {
    /// API root; the request goes to `{base_url}/audio/transcriptions`.
    pub base_url: String,
    pub model: String,
    /// Name of the environment variable holding the API key; empty = send no key (a local
    /// server). The key itself is never read from this file.
    pub api_key_env: String,
    /// ISO-639-1 code (`vi`, `en`); empty = the model detects it per recording.
    pub language: String,
    /// Spelling and style hint (Whisper `prompt`, 224 tokens at most); empty = none.
    pub prompt: String,
    /// A recording stops by itself after this many seconds (1–600, checked when parsed).
    #[serde(deserialize_with = "voice_max_seconds")]
    pub max_seconds: u32,
}

fn voice_max_seconds<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let seconds = u32::deserialize(deserializer)?;
    if (1..=600).contains(&seconds) {
        Ok(seconds)
    } else {
        Err(serde::de::Error::custom(format!(
            "max_seconds must be 1–600 seconds, got {seconds}"
        )))
    }
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            base_url: "https://api.groq.com/openai/v1".into(),
            model: "whisper-large-v3-turbo".into(),
            api_key_env: "GROQ_API_KEY".into(),
            language: String::new(),
            prompt: VOICE_PROMPT_DEFAULT.into(),
            max_seconds: 120,
        }
    }
}

/// `[jira]` (SPEC §20I, ADR 0066): the Jira Cloud site the Board reads issues from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JiraSettings {
    /// `acme.atlassian.net` (with or without `https://`); empty = Jira off.
    pub site: String,
    /// The Atlassian account email used with the API token.
    pub email: String,
    /// Name of the environment variable holding the API token; the token is never read
    /// from this file.
    pub api_token_env: String,
}

impl Default for JiraSettings {
    fn default() -> Self {
        Self {
            site: String::new(),
            email: String::new(),
            api_token_env: "JIRA_API_TOKEN".into(),
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

/// `[ui] theme` (SPEC §14, ADR 0027): `system` follows the OS live.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn name(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
}

/// `[ui] diff_mode` (SPEC §12B): how the Git diff viewer lays out a file's changes. The
/// diff header toggle writes it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffMode {
    /// One column: removed lines, then the added ones.
    #[default]
    Unified,
    /// Two columns: the old file on the left, the new one on the right.
    Split,
}

impl DiffMode {
    pub fn name(self) -> &'static str {
        match self {
            DiffMode::Unified => "unified",
            DiffMode::Split => "split",
        }
    }

    /// The other mode (what the toggle switches to).
    pub fn other(self) -> DiffMode {
        match self {
            DiffMode::Unified => DiffMode::Split,
            DiffMode::Split => DiffMode::Unified,
        }
    }
}

/// `[ui] inspector_width` default, px (the UI clamps what it reads to its own limits).
pub const INSPECTOR_WIDTH_DEFAULT: u16 = 300;

/// `[ui] sidebar_width` default, px (the UI clamps what it reads to its own limits).
pub const SIDEBAR_WIDTH_DEFAULT: u16 = 280;

/// `[ui]`: the app font, theme, diff layout (and panel widths) (SPEC §14, §12B, §20.0).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiSettings {
    pub font_family: String,
    pub font_size: FontSize,
    pub theme: ThemeMode,
    pub diff_mode: DiffMode,
    /// `[ui] diff_wrap` (SPEC §12B): long diff lines wrap at the pane edge (else they are cut).
    /// The Git diff toolbar's Wrap toggle writes it.
    pub diff_wrap: bool,
    /// Width of the session panel right of Terminals, px (SPEC §10); dragging its edge saves it.
    pub inspector_width: u16,
    /// Width of the workspace sidebar, px (SPEC §10); dragging its edge saves it.
    pub sidebar_width: u16,
    /// `[ui] reading_width`: agent panes start at reading width (≈ 120–140 columns, centered);
    /// each pane's Reading width toggle overrides it until restart. Shell panes are full width.
    pub reading_width: bool,
    /// `[ui] close_to_tray` (SPEC §20J): the window's ✕ hides it to the tray icon instead of
    /// quitting the UI (Windows, while the tray icon exists). The agents run in the daemon either way.
    pub close_to_tray: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        let font = FontSettings::default();
        Self {
            font_family: font.font_family,
            font_size: font.font_size,
            theme: ThemeMode::System,
            diff_mode: DiffMode::Unified,
            diff_wrap: true,
            inspector_width: INSPECTOR_WIDTH_DEFAULT,
            sidebar_width: SIDEBAR_WIDTH_DEFAULT,
            reading_width: true,
            close_to_tray: true,
        }
    }
}

/// `src` with `[<section>] <key> = <value>` (`value` is a TOML literal such as `"dark"`, `320`
/// or `true`): replaces an uncommented `<key> =` line inside the section, else inserts one right
/// after the section header, else appends the section. Everything else (comments, order, other
/// keys, CRLF line ends) is kept byte for byte. Forms this does not recognise (`ui.theme = …`,
/// an inline `ui = {…}`) can yield invalid TOML: callers parse the result before writing it.
fn set_value(src: &str, section: &str, key: &str, value: &str) -> String {
    let eol = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let line = format!("{key} = {value}");
    let header_text = format!("[{section}]");
    // `[ui]`, `[ ui ]` and `[ui] # note` are the same table header.
    let is_header = |l: &&str| {
        let code = l.split('#').next().unwrap_or_default();
        code.split_whitespace().collect::<String>() == header_text
    };
    let mut lines: Vec<&str> = src.lines().collect();
    let Some(header) = lines.iter().position(is_header) else {
        let sep = if src.is_empty() || src.ends_with(&format!("{eol}{eol}")) {
            ""
        } else if src.ends_with('\n') {
            eol
        } else {
            "\n\n"
        };
        return format!("{src}{sep}{header_text}{eol}{line}{eol}");
    };
    let end = lines[header + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| header + 1 + i);
    // `key` followed by `=`: `theme_x = …` is another key.
    let is_key = |l: &&str| {
        l.trim_start()
            .strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    match lines[header + 1..end].iter().position(is_key) {
        Some(i) => lines[header + 1 + i] = &line,
        None => lines.insert(header + 1, &line),
    }
    let mut out = lines.join(eol);
    if src.ends_with('\n') {
        out.push_str(eol);
    }
    out
}

/// Saves `[<section>] <key> = <value>` in `path` (the template when it is missing) with
/// [`set_value`]. Writes nothing when the file does not parse or the edit would not parse
/// (an unusual section form, a value of the wrong type), so a good file never turns bad.
fn write_value(path: &Path, section: &str, key: &str, value: &str) -> Result<(), SettingsError> {
    let io_error = |e: io::Error| SettingsError {
        file: path.to_path_buf(),
        line_col: None,
        message: e.to_string(),
    };
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(e) if e.kind() == io::ErrorKind::NotFound => template().to_owned(),
        Err(e) => return Err(io_error(e)),
    };
    parse(path, &src)?;
    let out = set_value(&src, section, key, value);
    parse(path, &out)?;
    std::fs::write(path, out).map_err(io_error)
}

/// Saves `[ui] <key> = <value>` (`value` is a TOML literal: `"split"` with quotes, or `320`);
/// see [`write_value`].
pub fn write_ui_value(path: &Path, key: &str, value: &str) -> Result<(), SettingsError> {
    write_value(path, "ui", key, value)
}

/// Saves `[web] enabled` (the Phone access dialog's Start / Stop); see [`write_value`]. The
/// daemon applies it through its settings watcher.
pub fn write_web_enabled(path: &Path, enabled: bool) -> Result<(), SettingsError> {
    write_value(path, "web", "enabled", &enabled.to_string())
}

/// Saves `[web] push` (the Phone access dialog's notifications switch); see [`write_value`].
/// `mk web` reads it on every request and event, so no restart is needed.
pub fn write_web_push(path: &Path, enabled: bool) -> Result<(), SettingsError> {
    write_value(path, "web", "push", &enabled.to_string())
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
     # flash_taskbar = true\n\
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
     # Theme: system (follows the OS), light or dark; the title bar toggle writes it.\n\
     # Session panel width in px (dragging its edge writes it; Mkrew keeps it 240–560).\n\
     # Diff view: unified (one column) or split (old | new); the Git diff header toggle writes it.\n\
     # Diff wrap: long lines wrap (true) or are cut at the pane edge (false); the Wrap toggle writes it.\n\
     # Reading width: agent panes start narrow (about 120-140 columns, centered) or full width.\n\
     # Close to tray (Windows): the window's ✕ hides it to the tray icon; false quits the UI.\n\
     [ui]\n\
     # font_family = \"\"\n\
     # font_size = 13\n\
     # theme = \"system\"\n\
     # diff_mode = \"unified\"\n\
     # diff_wrap = true\n\
     # inspector_width = 300\n\
     # sidebar_width = 280\n\
     # reading_width = true\n\
     # close_to_tray = true\n\
     \n\
     # Terminal, Files and Git code font: pick a monospace family (empty: Cascadia Code /\n\
     # SF Mono / JetBrains Mono, the first installed); size 8–32 px.\n\
     [terminal]\n\
     # font_family = \"\"\n\
     # font_size = 13\n\
     \n\
     # Voice input (Ctrl+Shift+D): speech to text over an OpenAI-compatible API, Groq by\n\
     # default (OpenAI and local servers work too). The API key is read from the environment\n\
     # variable named by api_key_env (empty: no key); restart Mkrew after setting it.\n\
     # language: empty detects it per recording; \"vi\" or \"en\" forces one.\n\
     # prompt: words the model should spell as written.\n\
     # max_seconds: 1–600; a recording stops by itself after this.\n\
     [voice]\n\
     # base_url = \"https://api.groq.com/openai/v1\"\n\
     # model = \"whisper-large-v3-turbo\"\n\
     # api_key_env = \"GROQ_API_KEY\"\n\
     # language = \"\"\n\
     # prompt = \"Mkrew, Claude Code, Codex, commit, branch, merge, refactor, test, bug. Sửa lỗi này rồi chạy test lại nhé.\"\n\
     # max_seconds = 120\n\
     \n\
     # Optional modules, off by default. docs: the Docs view. turns: the agent's turn\n\
     # snapshots and its Turns… dialog (applies to sessions started after the change).\n\
     [features]\n\
     # docs = false\n\
     # turns = false\n\
     \n\
     # Phone access (ADR 0053): while enabled is true the daemon runs `mk web`, so a phone on your\n\
     # tailnet can reach Mkrew. The phone icon in the title bar writes it; the port is fixed (8787).\n\
     # Phone notifications (ADR 0067): when push is true, `mk web` notifies phones that opted in when an\n\
     # agent needs you or finishes. The message is encrypted for the phone and goes through its\n\
     # browser's push service (Apple, Google, Mozilla or Microsoft). [notify] needs_input / done apply.\n\
     [web]\n\
     # enabled = false\n\
     # push = false\n\
     \n\
     # Jira issues on the Board (Tasks | Jira): the Jira Cloud site to read from. The API token\n\
     # comes from the environment variable named by api_token_env, never from this file.\n\
     # site: like acme.atlassian.net; empty turns Jira off.\n\
     [jira]\n\
     # site = \"\"\n\
     # email = \"\"\n\
     # api_token_env = \"JIRA_API_TOKEN\"\n"
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

    fn set_theme(src: &str, mode: ThemeMode) -> String {
        set_ui_value(src, "theme", &format!("\"{}\"", mode.name()))
    }

    fn write_theme(path: &Path, mode: ThemeMode) -> Result<(), SettingsError> {
        write_ui_value(path, "theme", &format!("\"{}\"", mode.name()))
    }

    fn set_ui_value(src: &str, key: &str, value: &str) -> String {
        set_value(src, "ui", key, value)
    }

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
        assert!(
            !settings.web.enabled,
            "phone access is off until the user turns it on"
        );
        assert_eq!(settings.jira.site, "");
        assert_eq!(settings.jira.email, "");
        assert_eq!(settings.jira.api_token_env, "JIRA_API_TOKEN");
        assert_eq!(settings.composer.file_ref, "@{path}");
        assert_eq!(settings.zed.follow_debounce_ms, 1500);
        assert_eq!(settings.voice.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(settings.voice.model, "whisper-large-v3-turbo");
        assert_eq!(settings.voice.api_key_env, "GROQ_API_KEY");
        assert_eq!(settings.voice.language, "");
        assert_eq!(settings.voice.prompt, VOICE_PROMPT_DEFAULT);
        assert_eq!(settings.voice.max_seconds, 120);
        assert_eq!(settings.ui.font_family, "");
        assert_eq!(settings.ui.font_size, FontSize(13));
        assert_eq!(settings.ui.theme, ThemeMode::System);
        assert_eq!(
            settings.terminal,
            FontSettings {
                font_family: String::new(),
                font_size: FontSize(13)
            }
        );
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
flash_taskbar = false

[awake]
keep_awake = false

[composer]
file_ref = "{path}"

[zed]
follow_debounce_ms = 500

[ui]
font_family = "Inter"
font_size = 14
close_to_tray = false

[terminal]
font_family = "JetBrains Mono"
font_size = 15

[features]
docs = true

[web]
enabled = true
push = true
"#;
        let settings = parse(Path::new("settings.toml"), src).unwrap();
        assert_eq!(
            settings,
            Settings {
                notify: NotifySettings {
                    needs_input: false,
                    done: false,
                    sound: false,
                    flash_taskbar: false,
                },
                awake: AwakeSettings { keep_awake: false },
                composer: ComposerSettings {
                    file_ref: "{path}".into()
                },
                zed: ZedSettings {
                    follow_debounce_ms: 500
                },
                ui: UiSettings {
                    font_family: "Inter".into(),
                    font_size: FontSize(14),
                    theme: ThemeMode::System,
                    diff_mode: DiffMode::Unified,
                    diff_wrap: true,
                    inspector_width: INSPECTOR_WIDTH_DEFAULT,
                    sidebar_width: SIDEBAR_WIDTH_DEFAULT,
                    reading_width: true,
                    close_to_tray: false,
                },
                terminal: FontSettings {
                    font_family: "JetBrains Mono".into(),
                    font_size: FontSize(15),
                },
                voice: VoiceSettings::default(),
                features: FeatureSettings {
                    docs: true,
                    turns: false,
                },
                web: WebSettings {
                    enabled: true,
                    push: true,
                },
                jira: JiraSettings::default(),
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
    fn voice_section_parses_another_provider_and_rejects_a_key() {
        let voice = parse(
            Path::new("settings.toml"),
            "[voice]\nbase_url = \"https://api.openai.com/v1\"\nmodel = \"gpt-4o-mini-transcribe\"\n\
             api_key_env = \"OPENAI_API_KEY\"\nlanguage = \"vi\"\nmax_seconds = 30\n",
        )
        .unwrap()
        .voice;
        assert_eq!(
            voice,
            VoiceSettings {
                base_url: "https://api.openai.com/v1".into(),
                model: "gpt-4o-mini-transcribe".into(),
                api_key_env: "OPENAI_API_KEY".into(),
                language: "vi".into(),
                prompt: VOICE_PROMPT_DEFAULT.into(),
                max_seconds: 30,
            }
        );
        // The key itself never lives in the file (SPEC §20G).
        let message = parse_err("[voice]\napi_key = \"gsk_x\"\n");
        assert!(message.contains("unknown field `api_key`"), "{message}");
    }

    #[test]
    fn jira_section_parses_and_rejects_a_token() {
        let jira = parse(
            Path::new("settings.toml"),
            "[jira]\nsite = \"acme.atlassian.net\"\nemail = \"an@acme.com\"\napi_token_env = \"ACME_JIRA\"\n",
        )
        .unwrap()
        .jira;
        assert_eq!(
            jira,
            JiraSettings {
                site: "acme.atlassian.net".into(),
                email: "an@acme.com".into(),
                api_token_env: "ACME_JIRA".into(),
            }
        );
        // The token itself never lives in the file (SPEC §20I).
        let message = parse_err("[jira]\napi_token = \"x\"\n");
        assert!(message.contains("unknown field `api_token`"), "{message}");
    }

    #[test]
    fn voice_max_seconds_outside_1_to_600_is_an_error() {
        let message = parse_err("[voice]\nmax_seconds = 0\n");
        assert!(message.starts_with("settings.toml:2:"), "{message}");
        assert!(message.contains("1–600"), "{message}");
        assert!(parse_err("[voice]\nmax_seconds = 601\n").contains("1–600"));
        let ok = parse(Path::new("settings.toml"), "[voice]\nmax_seconds = 600\n").unwrap();
        assert_eq!(ok.voice.max_seconds, 600);
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
    fn ui_theme_parses_and_defaults_to_system() {
        let ui = |src: &str| parse(Path::new("s.toml"), src).unwrap().ui;
        assert_eq!(ui("").theme, ThemeMode::System);
        assert_eq!(ui("[ui]\ntheme = \"dark\"").theme, ThemeMode::Dark);
        assert_eq!(
            ui("[ui]\ntheme = \"light\"\nfont_size = 14").font_size,
            FontSize(14)
        );
        assert!(parse_err("[ui]\ntheme = \"blue\"").contains("unknown variant `blue`"));
        // [terminal] has no theme.
        assert!(parse_err("[terminal]\ntheme = \"dark\"").contains("unknown field `theme`"));
    }

    #[test]
    fn ui_diff_mode_parses_and_defaults_to_unified() {
        let ui = |src: &str| parse(Path::new("s.toml"), src).unwrap().ui;
        assert_eq!(ui("").diff_mode, DiffMode::Unified);
        assert_eq!(ui("[ui]\ndiff_mode = \"split\"").diff_mode, DiffMode::Split);
        // Another [ui] key does not change it.
        assert_eq!(ui("[ui]\ntheme = \"dark\"").diff_mode, DiffMode::Unified);
        assert!(parse_err("[ui]\ndiff_mode = \"side\"").contains("unknown variant `side`"));
        // [terminal] has no diff_mode.
        assert!(
            parse_err("[terminal]\ndiff_mode = \"split\"").contains("unknown field `diff_mode`")
        );
    }

    #[test]
    fn ui_diff_wrap_parses_and_defaults_to_on() {
        let ui = |src: &str| parse(Path::new("s.toml"), src).unwrap().ui;
        assert!(ui("").diff_wrap);
        assert!(!ui("[ui]\ndiff_wrap = false").diff_wrap);
        assert!(parse_err("[ui]\ndiff_wrap = \"no\"").contains("invalid type"));
    }

    #[test]
    fn flash_taskbar_and_close_to_tray_default_on_and_parse() {
        let settings = |src: &str| parse(Path::new("s.toml"), src).unwrap();
        assert!(settings("").notify.flash_taskbar);
        assert!(settings("").ui.close_to_tray);
        let off = settings("[notify]\nflash_taskbar = false\n");
        assert!(!off.notify.flash_taskbar);
        assert!(
            off.ui.close_to_tray,
            "another section's key does not change it"
        );
        assert!(!settings("[ui]\nclose_to_tray = false\n").ui.close_to_tray);
        assert!(parse_err("[notify]\nflash_taskbar = 1\n").contains("invalid type"));
        assert!(parse_err("[ui]\nclose_to_tra = false\n").contains("unknown field `close_to_tra`"));
    }

    #[test]
    fn writing_diff_mode_keeps_theme_and_comments() {
        let src = "[ui]\n# diff_mode = \"unified\"\ntheme = \"dark\"\n";
        let out = set_ui_value(src, "diff_mode", "\"split\"");
        let ui = parse(Path::new("s.toml"), &out).expect(&out).ui;
        assert_eq!((ui.diff_mode, ui.theme), (DiffMode::Split, ThemeMode::Dark));
        assert!(
            out.contains("# diff_mode = \"unified\""),
            "the user's comment stays: {out}"
        );
        // A second write replaces the line instead of adding another.
        let again = set_ui_value(&out, "diff_mode", "\"unified\"");
        assert_eq!(
            parse(Path::new("s.toml"), &again)
                .expect(&again)
                .ui
                .diff_mode,
            DiffMode::Unified
        );
        assert_eq!(
            again.matches("diff_mode =").count(),
            2,
            "comment + one key: {again}"
        );
    }

    #[test]
    fn write_ui_value_saves_diff_mode_into_the_template() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        write_ui_value(&path, "diff_mode", "\"split\"").unwrap();
        assert_eq!(load(&path).unwrap().ui.diff_mode, DiffMode::Split);
        // A file that does not parse is left alone.
        std::fs::write(&path, "[ui\n").unwrap();
        assert!(write_ui_value(&path, "diff_mode", "\"unified\"").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[ui\n");
    }

    #[test]
    fn set_ui_theme_keeps_comments_and_other_keys() {
        let src = "# mine\n[ui]\n# theme = \"system\"\nfont_size = 15\ntheme = \"light\"\n\n[terminal]\nfont_size = 12\n";
        let out = set_theme(src, ThemeMode::Dark);
        assert_eq!(out, src.replace("theme = \"light\"", "theme = \"dark\""));
        assert_eq!(
            parse(Path::new("s.toml"), &out).unwrap().ui.theme,
            ThemeMode::Dark
        );
    }

    #[test]
    fn set_ui_theme_inserts_ui_section() {
        let with_ui = set_theme("[ui]\nfont_size = 15\n", ThemeMode::Light);
        assert_eq!(with_ui, "[ui]\ntheme = \"light\"\nfont_size = 15\n");
        let without = set_theme("[zed]\nfollow_debounce_ms = 1\n", ThemeMode::Dark);
        assert_eq!(
            without,
            "[zed]\nfollow_debounce_ms = 1\n\n[ui]\ntheme = \"dark\"\n"
        );
        assert_eq!(
            set_theme("", ThemeMode::System),
            "[ui]\ntheme = \"system\"\n"
        );
    }

    #[test]
    fn set_ui_theme_finds_a_commented_or_spaced_ui_header() {
        for src in [
            "[ui] # app font\nfont_size = 15\n",
            "[ ui ]\nfont_size = 15\n",
            "[ui]#x\nfont_size = 15\n",
        ] {
            let out = set_theme(src, ThemeMode::Dark);
            let settings = parse(Path::new("s.toml"), &out).expect(&out);
            assert_eq!(settings.ui.theme, ThemeMode::Dark, "{out}");
            assert_eq!(settings.ui.font_size, FontSize(15), "{out}");
        }
    }

    #[test]
    fn set_ui_theme_keeps_crlf_line_endings() {
        let src = "[ui]\r\nfont_size = 15\r\n";
        assert_eq!(
            set_theme(src, ThemeMode::Light),
            "[ui]\r\ntheme = \"light\"\r\nfont_size = 15\r\n"
        );
        assert_eq!(
            set_theme("[zed]\r\n", ThemeMode::Dark),
            "[zed]\r\n\r\n[ui]\r\ntheme = \"dark\"\r\n"
        );
    }

    #[test]
    fn write_ui_theme_never_turns_a_good_file_bad() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        // Missing: the template plus the theme.
        write_theme(&path, ThemeMode::Dark).unwrap();
        assert_eq!(load(&path).unwrap().ui.theme, ThemeMode::Dark);
        // A dotted `ui.font_size` key: the edit would declare `ui` twice, so nothing is written.
        let dotted = "ui.font_size = 15\n";
        std::fs::write(&path, dotted).unwrap();
        assert!(write_theme(&path, ThemeMode::Light).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), dotted);
        // A file that does not parse is left alone.
        std::fs::write(&path, "[ui\n").unwrap();
        assert!(write_theme(&path, ThemeMode::Light).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[ui\n");
    }

    #[test]
    fn inspector_width_parses_and_defaults_to_300() {
        let ui = |src: &str| parse(Path::new("s.toml"), src).unwrap().ui;
        assert_eq!(ui("").inspector_width, INSPECTOR_WIDTH_DEFAULT);
        assert_eq!(INSPECTOR_WIDTH_DEFAULT, 300);
        assert_eq!(ui("[ui]\ninspector_width = 420").inspector_width, 420);
        assert_eq!(ui("").sidebar_width, SIDEBAR_WIDTH_DEFAULT);
        assert_eq!(ui("[ui]\nsidebar_width = 360").sidebar_width, 360);
        // A negative or fractional width is a typed error, not a silent default.
        assert!(parse_err("[ui]\ninspector_width = -1").contains("invalid value"));
        assert!(parse_err("[ui]\ninspector_width = \"wide\"").contains("invalid type"));
    }

    #[test]
    fn set_ui_value_replaces_only_its_own_key() {
        // `inspector_width_x` and the commented line are not `inspector_width`; the other keys stay.
        let src = "[ui]\n# inspector_width = 300\ninspector_width_x = 1\ninspector_width = 300 # px\nfont_size = 15\n";
        let out = set_ui_value(src, "inspector_width", "420");
        assert_eq!(
            out,
            "[ui]\n# inspector_width = 300\ninspector_width_x = 1\ninspector_width = 420\nfont_size = 15\n"
        );
    }

    #[test]
    fn set_ui_value_adds_the_key_or_the_section_and_keeps_crlf() {
        assert_eq!(
            set_ui_value("[ui]\r\nfont_size = 15\r\n", "inspector_width", "320"),
            "[ui]\r\ninspector_width = 320\r\nfont_size = 15\r\n"
        );
        assert_eq!(
            set_ui_value(
                "# mine\n[zed]\nfollow_debounce_ms = 1\n",
                "inspector_width",
                "320"
            ),
            "# mine\n[zed]\nfollow_debounce_ms = 1\n\n[ui]\ninspector_width = 320\n"
        );
        let out = set_ui_value("[ui]\nfont_size = 15\n", "inspector_width", "320");
        let settings = parse(Path::new("s.toml"), &out).unwrap();
        assert_eq!(
            (settings.ui.inspector_width, settings.ui.font_size),
            (320, FontSize(15))
        );
    }

    #[test]
    fn write_ui_value_saves_next_to_the_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        // Missing file: the template plus the value.
        write_ui_value(&path, "inspector_width", "400").unwrap();
        assert_eq!(load(&path).unwrap().ui.inspector_width, 400);
        write_theme(&path, ThemeMode::Dark).unwrap();
        write_ui_value(&path, "inspector_width", "350").unwrap();
        let ui = load(&path).unwrap().ui;
        assert_eq!((ui.inspector_width, ui.theme), (350, ThemeMode::Dark));
    }

    #[test]
    fn web_section_defaults_off_and_rejects_other_keys() {
        let web = |src: &str| parse(Path::new("s.toml"), src).unwrap().web;
        assert!(!web("").enabled);
        assert!(!web("[web]").enabled);
        assert!(web("[web]\nenabled = true").enabled);
        // Phone notifications (ADR 0067) are off until the user turns them on.
        assert!(!web("").push);
        assert!(!web("[web]\nenabled = true").push);
        assert!(web("[web]\npush = true").push);
        assert!(parse_err("[web]\npush = \"yes\"").contains("invalid type"));
        assert!(parse_err("[web]\nenabled = \"yes\"").contains("invalid type"));
        // The port is fixed at 8787 (ADR 0053), so it is not a setting.
        assert!(parse_err("[web]\nport = 8787").contains("unknown field `port`"));
    }

    #[test]
    fn write_web_push_keeps_enabled_and_the_rest_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "[web]\nenabled = true\n\n[features]\ndocs = true\n").unwrap();
        write_web_push(&path, true).unwrap();
        let settings = load(&path).unwrap();
        assert!(settings.web.enabled && settings.web.push && settings.features.docs);
        write_web_push(&path, false).unwrap();
        let settings = load(&path).unwrap();
        assert!(settings.web.enabled && !settings.web.push);
        // No file: the commented template plus the switch.
        let fresh = dir.path().join("fresh.toml");
        write_web_push(&fresh, true).unwrap();
        assert!(load(&fresh).unwrap().web.push);
    }

    #[test]
    fn set_value_stays_inside_its_own_section() {
        let src = "[ui]\nfont_size = 15\n\n[web]\nenabled = false\n";
        // Replaces the key in [web]; [ui] is untouched.
        assert_eq!(
            set_value(src, "web", "enabled", "true"),
            "[ui]\nfont_size = 15\n\n[web]\nenabled = true\n"
        );
        // A key of the same name in another section is not replaced: it is added to the
        // section asked for.
        assert_eq!(
            set_value(src, "ui", "enabled", "true"),
            "[ui]\nenabled = true\nfont_size = 15\n\n[web]\nenabled = false\n"
        );
        // No such section: appended, never put into the last one.
        assert_eq!(
            set_value("[ui]\nfont_size = 15\n", "web", "enabled", "true"),
            "[ui]\nfont_size = 15\n\n[web]\nenabled = true\n"
        );
    }

    #[test]
    fn write_web_enabled_keeps_the_rest_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let read = || std::fs::read_to_string(&path).unwrap();

        // No file: the commented template plus the switch; the second write flips it in place.
        write_web_enabled(&path, true).unwrap();
        assert!(load(&path).unwrap().web.enabled);
        write_web_enabled(&path, false).unwrap();
        assert!(!load(&path).unwrap().web.enabled);
        let text = read();
        assert_eq!(
            text.matches("\nenabled = ").count(),
            1,
            "one key, not two: {text}"
        );
        assert!(
            text.contains("# docs = false"),
            "the template stays: {text}"
        );

        // The user's file without [web]: everything stays, [web] is appended.
        let mine = "# mine\n[ui]\ntheme = \"dark\"\n\n[features]\ndocs = true\n";
        std::fs::write(&path, mine).unwrap();
        write_web_enabled(&path, true).unwrap();
        assert_eq!(read(), format!("{mine}\n[web]\nenabled = true\n"));

        // An existing [web] in the middle: only its line changes.
        let middle = "[web]\nenabled = false\n\n[features]\ndocs = true\n";
        std::fs::write(&path, middle).unwrap();
        write_web_enabled(&path, true).unwrap();
        assert_eq!(read(), "[web]\nenabled = true\n\n[features]\ndocs = true\n");
    }

    #[test]
    fn write_web_enabled_never_turns_a_good_file_bad() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        // Broken TOML; an unknown [web] key; a dotted `web.enabled` (appending `[web]` would
        // declare the table twice): each is refused and the file is left alone.
        for bad in ["[web\n", "[web]\nport = 1\n", "web.enabled = false\n"] {
            std::fs::write(&path, bad).unwrap();
            assert!(write_web_enabled(&path, true).is_err(), "{bad:?}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), bad);
        }
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
