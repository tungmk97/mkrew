//! Syntax highlighting for the Files viewer (SPEC §12A): `syntect` with its bundled syntaxes
//! (pure-Rust `fancy-regex`, loaded once on first use) turns a file into lines of [`Token`]
//! spans. Colors are not decided here: the renderer maps tokens to theme tokens (SPEC §14).
//! Slow on big files, so the shell runs [`Code::new`] off the UI thread.

use std::ops::Range;
use std::sync::OnceLock;

use mkrew_protocol::FsContent;
use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Color, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::{SyntaxReference, SyntaxSet};

/// Files bigger than this are shown without highlighting (it would take seconds). Below the
/// daemon's 2 MB viewer limit (SPEC §12A), so 512 KB – 2 MB files show as plain text.
pub const HIGHLIGHT_MAX_BYTES: usize = 512 * 1024;

/// Lines longer than this (minified code) turn highlighting off for the whole file.
pub const HIGHLIGHT_MAX_LINE: usize = 4096;

/// A tab shows as this many spaces.
const TAB: &str = "    ";

/// What a highlighted span is; plain text has no span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Comment,
    String,
    Number,
    Keyword,
    Type,
    Function,
}

impl Token {
    const ALL: [Token; 6] = [
        Token::Comment,
        Token::String,
        Token::Number,
        Token::Keyword,
        Token::Type,
        Token::Function,
    ];

    /// TextMate scope selectors of each token (the most specific match wins).
    fn scopes(self) -> &'static str {
        match self {
            Token::Comment => "comment, punctuation.definition.comment",
            Token::String => "string, markup.raw, markup.inline.raw",
            Token::Number => "constant.numeric, constant.language, constant.character",
            Token::Keyword => {
                "keyword, storage, entity.name.tag, markup.heading, punctuation.definition.heading"
            }
            Token::Type => "entity.name.type, entity.name.class, support.type, support.class",
            Token::Function => {
                "entity.name.function, support.function, variable.function, \
                 entity.other.attribute-name, meta.attribute"
            }
        }
    }
}

/// A file as the viewer shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    /// Lines without their line ending, tabs expanded; line `n` is `lines[n - 1]`.
    pub lines: Vec<String>,
    /// Per line, its highlighted byte ranges (empty when not highlighted).
    pub spans: Vec<Vec<(Range<usize>, Token)>>,
    /// Bytes on disk.
    pub size: u64,
    /// Not shown: binary, or over the daemon's viewer limit.
    pub binary: bool,
    pub too_large: bool,
    /// The syntax used, `None` = plain text.
    pub syntax: Option<String>,
}

impl Code {
    /// Splits and highlights an `fs.read` answer (by the file's extension, then the daemon's
    /// language guess).
    pub fn new(content: &FsContent) -> Code {
        let text = content.content.as_deref().unwrap_or("");
        let mut code = Code::plain(text);
        code.size = content.size;
        code.binary = content.binary;
        code.too_large = content.too_large;
        let long = code
            .lines
            .iter()
            .any(|line| line.len() > HIGHLIGHT_MAX_LINE);
        if text.len() > HIGHLIGHT_MAX_BYTES || long {
            return code;
        }
        let Some(syntax) = find_syntax(&content.path, content.language.as_deref()) else {
            return code;
        };
        code.syntax = Some(syntax.name.clone());
        code.spans = highlight(&code.lines, syntax);
        code
    }

    /// `text` unhighlighted.
    pub fn plain(text: &str) -> Code {
        let lines: Vec<String> = text
            .lines()
            .map(|line| line.strip_suffix('\r').unwrap_or(line).replace('\t', TAB))
            .collect();
        Code {
            spans: vec![Vec::new(); lines.len()],
            lines,
            size: u64::try_from(text.len()).unwrap_or(u64::MAX),
            binary: false,
            too_large: false,
            syntax: None,
        }
    }
}

fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}

/// The theme every highlight runs with: each token's foreground carries its index + 1 in the
/// red channel (0 = plain), which [`highlight`] reads back.
fn token_theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let color = |r| Color {
            r,
            g: 0,
            b: 0,
            a: 0xFF,
        };
        let scopes = (1..)
            .zip(Token::ALL)
            .filter_map(|(r, token)| {
                let scope = token.scopes().parse::<ScopeSelectors>().ok()?;
                Some(ThemeItem {
                    scope,
                    style: StyleModifier {
                        foreground: Some(color(r)),
                        ..StyleModifier::default()
                    },
                })
            })
            .collect();
        Theme {
            settings: ThemeSettings {
                foreground: Some(color(0)),
                ..ThemeSettings::default()
            },
            scopes,
            ..Theme::default()
        }
    })
}

/// The syntax for `path`: by extension, else by the daemon's language name (TypeScript and
/// JSX read as JavaScript, which the bundled set lacks); `None` = plain text.
fn find_syntax(path: &str, language: Option<&str>) -> Option<&'static SyntaxReference> {
    let set = syntaxes();
    let name = path.rsplit('/').next().unwrap_or(path);
    let by_extension = name
        .rsplit_once('.')
        .and_then(|(_, ext)| set.find_syntax_by_extension(&ext.to_ascii_lowercase()));
    let by_language = || {
        let token = match language? {
            "typescript" | "tsx" | "jsx" => "js",
            "shell" => "sh",
            "csharp" => "cs",
            "batch" => "bat",
            other => other,
        };
        set.find_syntax_by_token(token)
    };
    by_extension
        .or_else(by_language)
        .or_else(|| set.find_syntax_by_extension(name))
        .filter(|syntax| syntax.name != set.find_syntax_plain_text().name)
}

/// Token spans of each line; a syntect error leaves the remaining lines plain.
fn highlight(lines: &[String], syntax: &SyntaxReference) -> Vec<Vec<(Range<usize>, Token)>> {
    let set = syntaxes();
    let mut highlighter = HighlightLines::new(syntax, token_theme());
    let mut failed = false;
    lines
        .iter()
        .map(|line| {
            if failed {
                return Vec::new();
            }
            // The "newlines" syntaxes expect each line to end in `\n`.
            let with_ending = format!("{line}\n");
            let Ok(regions) = highlighter.highlight_line(&with_ending, set) else {
                failed = true;
                return Vec::new();
            };
            let mut spans: Vec<(Range<usize>, Token)> = Vec::new();
            let mut start = 0;
            for (style, text) in regions {
                let end = (start + text.len()).min(line.len());
                let token = usize::from(style.foreground.r)
                    .checked_sub(1)
                    .and_then(|index| Token::ALL.get(index).copied());
                if let Some(token) = token.filter(|_| end > start) {
                    match spans.last_mut() {
                        Some((range, last)) if *last == token && range.end == start => {
                            range.end = end;
                        }
                        _ => spans.push((start..end, token)),
                    }
                }
                start = end;
            }
            spans
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Counts the lines syntect's `LinesWithEndings` yields, to check [`Code::plain`] splits
    /// alike.
    fn syntect_line_count(text: &str) -> usize {
        syntect::util::LinesWithEndings::from(text).count()
    }

    fn read(path: &str, text: &str, language: Option<&str>) -> FsContent {
        FsContent {
            path: path.into(),
            content: Some(text.into()),
            size: text.len() as u64,
            binary: false,
            too_large: false,
            language: language.map(Into::into),
        }
    }

    fn tokens(code: &Code, line: usize) -> Vec<(&str, Token)> {
        code.spans[line]
            .iter()
            .map(|(range, token)| (&code.lines[line][range.clone()], *token))
            .collect()
    }

    #[test]
    fn rust_gets_keywords_strings_comments_and_numbers() {
        let code = Code::new(&read(
            "src/main.rs",
            "// hi\r\nfn main() {\n\tlet s = \"x\";\n\tlet n = 42;\n}\n",
            Some("rust"),
        ));
        assert_eq!(code.syntax.as_deref(), Some("Rust"));
        assert_eq!(code.lines.len(), 5);
        assert_eq!(code.lines[2], "    let s = \"x\";");
        assert_eq!(tokens(&code, 0), [("// hi", Token::Comment)]);
        let main = tokens(&code, 1);
        assert!(main.contains(&("fn", Token::Keyword)), "{main:?}");
        assert!(main.contains(&("main", Token::Function)), "{main:?}");
        assert!(tokens(&code, 2).contains(&("\"x\"", Token::String)));
        assert!(tokens(&code, 3).contains(&("42", Token::Number)));
        // Every span is a valid range of its line.
        for (line, spans) in code.lines.iter().zip(&code.spans) {
            assert!(
                spans
                    .iter()
                    .all(|(range, _)| line.get(range.clone()).is_some())
            );
        }
    }

    #[test]
    fn spans_stay_on_char_boundaries_with_tabs_and_multibyte_text() {
        let code = Code::new(&read("a.rs", "\tlet s = \"é\"; // ü\n", Some("rust")));
        assert_eq!(code.lines[0], "    let s = \"é\"; // ü");
        let spans = tokens(&code, 0);
        assert!(spans.contains(&("\"é\"", Token::String)), "{spans:?}");
        assert!(spans.contains(&("// ü", Token::Comment)), "{spans:?}");
    }

    #[test]
    fn the_language_name_is_the_fallback_and_unknown_files_stay_plain() {
        let ts = Code::new(&read("a/b.ts", "const x = 1;\n", Some("typescript")));
        assert_eq!(ts.syntax.as_deref(), Some("JavaScript"));
        let make = Code::new(&read("Makefile", "all:\n\techo hi\n", Some("makefile")));
        assert_eq!(make.syntax.as_deref(), Some("Makefile"));
        let plain = Code::new(&read("LICENSE", "MIT\n", None));
        assert_eq!(plain.syntax, None);
        assert_eq!(plain.spans, [Vec::new()]);
    }

    #[test]
    fn big_files_and_long_lines_are_not_highlighted() {
        let long = format!("let x = \"{}\";\n", "a".repeat(HIGHLIGHT_MAX_LINE));
        assert_eq!(Code::new(&read("a.rs", &long, Some("rust"))).syntax, None);
        let big = "let x = 1;\n".repeat(HIGHLIGHT_MAX_BYTES / 10);
        let code = Code::new(&read("a.rs", &big, Some("rust")));
        assert_eq!(code.syntax, None);
        assert_eq!(code.lines.len(), HIGHLIGHT_MAX_BYTES / 10);
    }

    #[test]
    fn binary_and_too_large_files_keep_their_flags_and_no_lines() {
        let content = FsContent {
            path: "a.png".into(),
            content: None,
            size: 3_000_000,
            binary: true,
            too_large: true,
            language: None,
        };
        let code = Code::new(&content);
        assert!(code.binary && code.too_large);
        assert_eq!(code.size, 3_000_000);
        assert!(code.lines.is_empty());
    }

    #[test]
    fn plain_splits_lines_like_syntect() {
        for text in ["", "a", "a\n", "a\r\nb", "a\n\nb\n"] {
            assert_eq!(
                Code::plain(text).lines.len(),
                syntect_line_count(text),
                "{text:?}"
            );
        }
    }
}
