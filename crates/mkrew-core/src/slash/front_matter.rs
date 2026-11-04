//! Front matter of skill, command and prompt files (spec 2026-11-04 §5).
//!
//! A file whose first line is `---` has YAML front matter up to the next `---` line.
//! Only the top-level keys Mkrew needs are read, and only as scalars: plain (also folded over
//! indented continuation lines), `'single'` and `"double"` quoted, and `|` / `>` blocks.
//! A block that is not valid YAML for these rules gives a [`FrontMatter`] with no fields set,
//! as Claude Code lists such a file; a block that is never closed is not front matter at all.

/// The keys read from a front matter block. A key that is missing, empty or not a scalar is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub name: Option<String>,
    pub description: Option<String>,
    /// `argument-hint`, or `argumentHint` (omp).
    pub argument_hint: Option<String>,
    /// `user-invocable`; only `true` and `false` count.
    pub user_invocable: Option<bool>,
    pub when_to_use: Option<String>,
}

/// Splits `text` into its front matter and the body after the closing `---` line.
///
/// `None` = the text has no front matter; the body is then the whole text.
pub fn parse(text: &str) -> (Option<FrontMatter>, &str) {
    let rest = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = rest.split_inclusive('\n');
    let Some(first) = lines.next().filter(|first| first.trim_end() == "---") else {
        return (None, text);
    };
    let mut offset = first.len();
    let mut block = Vec::new();
    for line in lines {
        offset += line.len();
        if line.trim_end() == "---" {
            let fields = parse_block(&block).unwrap_or_default();
            return (Some(fields), &rest[offset..]);
        }
        block.push(line.trim_end_matches(['\r', '\n']));
    }
    (None, text)
}

/// The first non-empty line of `body`, trimmed (the description when `description` is missing).
pub fn first_line(body: &str) -> Option<&str> {
    body.lines().map(str::trim).find(|line| !line.is_empty())
}

/// `None` = the block is not valid for the rules in the module docs.
fn parse_block(lines: &[&str]) -> Option<FrontMatter> {
    let mut fields = FrontMatter::default();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        let top_level = !line.starts_with([' ', '\t']);
        if line.trim().is_empty() || line.trim_start().starts_with('#') || !top_level {
            continue;
        }
        let Some((key, inline)) = split_key(line) else {
            if line == "-" || line.starts_with("- ") {
                continue; // an item of a sequence under the previous key
            }
            return None;
        };
        // Lines that belong to this key: the indented ones that follow (blank lines too).
        let start = i;
        while i < lines.len() && (lines[i].trim().is_empty() || lines[i].starts_with([' ', '\t'])) {
            i += 1;
        }
        let more = &lines[start..i];
        let value = scalar(inline, more)?;
        let value = (!value.is_empty()).then_some(value);
        match key {
            "name" => fields.name = value,
            "description" => fields.description = value,
            "argument-hint" | "argumentHint" => fields.argument_hint = value,
            "when_to_use" => fields.when_to_use = value,
            "user-invocable" => {
                fields.user_invocable = match value.as_deref().map(str::to_ascii_lowercase) {
                    Some(v) if v == "true" => Some(true),
                    Some(v) if v == "false" => Some(false),
                    _ => None,
                }
            }
            _ => {}
        }
    }
    Some(fields)
}

/// `key: value` or `key:` at the start of a line.
fn split_key(line: &str) -> Option<(&str, &str)> {
    let colon = line
        .find(": ")
        .or_else(|| line.strip_suffix(':').map(str::len))?;
    let key = line[..colon].trim_end();
    (!key.is_empty()).then(|| (key, line[colon + 1..].trim()))
}

/// The value after `key:` plus the indented lines that follow it.
fn scalar(inline: &str, more: &[&str]) -> Option<String> {
    let is_block = inline
        .strip_prefix(['|', '>'])
        .is_some_and(|marks| marks.chars().all(|c| matches!(c, '+' | '-' | '1'..='9')));
    if is_block {
        return Some(block(inline.starts_with('>'), more));
    }
    if let Some(rest) = inline.strip_prefix(['"', '\'']) {
        let quote = inline.chars().next()?;
        return quoted(&fold_lines(rest, more), quote);
    }
    // A nested mapping or sequence under the key is not a scalar.
    if inline.is_empty()
        && let Some(first) = more.iter().map(|l| l.trim()).find(|l| !l.is_empty())
        && (first == "-" || first.starts_with("- ") || split_key(first).is_some())
    {
        return Some(String::new());
    }
    // Plain: ` #` starts a comment; indented lines continue the value, folded with spaces.
    let mut words = Vec::new();
    for part in std::iter::once(inline).chain(more.iter().copied()) {
        let part = part.trim();
        let part = if part.starts_with('#') {
            ""
        } else {
            part.split(" #").next().unwrap_or(part).trim_end()
        };
        if !part.is_empty() {
            words.push(part);
        }
    }
    Some(words.join(" "))
}

/// `first` plus the continuation lines, line breaks folded into one space (a blank line is a
/// line break), as YAML does inside a quoted scalar.
fn fold_lines(first: &str, more: &[&str]) -> String {
    let mut text = first.to_owned();
    for line in more {
        let line = line.trim();
        if line.is_empty() {
            text.push('\n');
        } else {
            if !text.ends_with('\n') {
                text.push(' ');
            }
            text.push_str(line);
        }
    }
    text
}

/// A `|` (keep line breaks) or `>` (fold single breaks into spaces) block, trimmed.
fn block(fold: bool, lines: &[&str]) -> String {
    let indent = lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map_or(0, |l| l.len() - l.trim_start().len());
    let lines: Vec<&str> = lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or("").trim_end())
        .collect();
    let text = if fold {
        lines
            .split(|l| l.is_empty())
            .map(|paragraph| paragraph.join(" "))
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        lines.join("\n")
    };
    text.trim().to_owned()
}

/// A quoted scalar whose opening `quote` is already consumed; `None` = not closed on this line
/// or followed by anything but a comment.
fn quoted(rest: &str, quote: char) -> Option<String> {
    let mut value = String::new();
    let mut chars = rest.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if c == quote {
            if quote == '\'' && matches!(chars.peek(), Some((_, '\''))) {
                chars.next();
                value.push('\'');
                continue;
            }
            let after = rest[at + 1..].trim();
            return (after.is_empty() || after.starts_with('#')).then_some(value);
        }
        if c == '\\' && quote == '"' {
            match chars.next()?.1 {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                '/' => value.push('/'),
                other => {
                    value.push('\\');
                    value.push(other);
                }
            }
            continue;
        }
        value.push(c);
    }
    None
}

#[cfg(test)]
mod tests;
