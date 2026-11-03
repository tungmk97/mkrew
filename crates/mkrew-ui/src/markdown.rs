//! Markdown render model for the Docs preview (SPEC §12): `pulldown-cmark` events → a tree of
//! blocks (headings, paragraphs, lists, task lists, tables, code blocks, quotes and GitHub
//! alerts, rules, YAML front matter) made of styled inline runs. Every block and table row
//! carries its source line range so preview selections and comment anchors map to lines of the
//! file. Also the pure helpers the preview's clicks need: task check boxes, heading anchors,
//! link targets, the block at a source line. Pure and tested headlessly; `preview.rs` turns it
//! into GPUI elements.

use std::iter::Peekable;
use std::ops::Range;

pub use pulldown_cmark::BlockQuoteKind as Alert;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// 1-based, inclusive source line range (the numbering of comment anchors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lines {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub lines: Lines,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockKind {
    /// `level` is 1–6.
    Heading {
        level: u8,
        runs: Vec<Run>,
    },
    Paragraph(Vec<Run>),
    /// `start` is the first number of an ordered list, `None` for bullets.
    List {
        start: Option<u64>,
        items: Vec<Item>,
    },
    /// A block quote; `alert` is set for GitHub's `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`,
    /// `[!WARNING]` and `[!CAUTION]` (the marker line is not part of `blocks`).
    Quote {
        alert: Option<Alert>,
        blocks: Vec<Block>,
    },
    /// Highlighted by the preview (`highlight::fenced`); `text` has no trailing newline.
    Code {
        lang: Option<String>,
        text: String,
    },
    Table {
        /// One per column.
        aligns: Vec<Align>,
        head: Row,
        rows: Vec<Row>,
    },
    Rule,
    /// Raw HTML block, shown as source text.
    Html(String),
    /// The YAML front matter (`---` … `---` at the very top), without its fences.
    FrontMatter(String),
}

/// Column alignment of a table (`:--`, `:-:`, `--:`; `Default` = no colon).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Default,
    Left,
    Center,
    Right,
}

impl From<Alignment> for Align {
    fn from(alignment: Alignment) -> Self {
        match alignment {
            Alignment::None => Align::Default,
            Alignment::Left => Align::Left,
            Alignment::Center => Align::Center,
            Alignment::Right => Align::Right,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// `Some` for task list items (`- [x]` / `- [ ]`).
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
    pub lines: Lines,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub cells: Vec<Vec<Run>>,
    pub lines: Lines,
}

/// A piece of inline text with one style; adjacent pieces with the same style are merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub style: Style,
    /// Destination of the enclosing link (or image, whose alt text is the run).
    pub link: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
}

/// Parses `source` (GitHub-flavoured tables, task lists, strikethrough and alerts, and YAML
/// front matter at the top).
pub fn parse(source: &str) -> Vec<Block> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_GFM;
    // Not pulldown-cmark's metadata blocks: it takes `---` … `---` anywhere a block starts,
    // which swallows a document's `---` rules and everything between them.
    let front = front_matter(source);
    let body = front.as_ref().map_or(0, |(_, _, end)| *end);
    let mut builder = Builder {
        events: Parser::new_ext(&source[body..], options)
            .into_offset_iter()
            .map(move |(event, range)| (event, range.start + body..range.end + body))
            .peekable(),
        source,
        line_starts: std::iter::once(0)
            .chain(source.match_indices('\n').map(|(i, _)| i + 1))
            .collect(),
        checks: Vec::new(),
        inline: Inline::default(),
    };
    let front = front.filter(|(text, _, _)| !text.is_empty());
    (front.map(|(text, lines, _)| Block {
        kind: BlockKind::FrontMatter(text),
        lines,
    }))
    .into_iter()
    .chain(builder.blocks())
    .collect()
}

/// The YAML front matter: `---` as the first line, closed by a `---` or `...` line. Its text
/// without the fences (LF line ends), its lines, and the byte where the rest starts.
fn front_matter(source: &str) -> Option<(String, Lines, usize)> {
    let mut lines = source.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    let mut at = first.len();
    let mut text: Vec<&str> = Vec::new();
    for line in lines {
        at += line.len();
        let line = line.trim_end_matches(['\r', '\n']);
        if line == "---" || line == "..." {
            let end = u32::try_from(text.len() + 2).unwrap_or(u32::MAX);
            return Some((text.join("\n"), Lines { start: 1, end }, at));
        }
        text.push(line);
    }
    None
}

/// Calls `f` on every block, nested ones (list items, quotes) included, in document order.
pub fn walk<'a>(blocks: &'a [Block], f: &mut impl FnMut(&'a Block)) {
    for block in blocks {
        f(block);
        match &block.kind {
            BlockKind::List { items, .. } => {
                for item in items {
                    walk(&item.blocks, f);
                }
            }
            BlockKind::Quote { blocks, .. } => walk(blocks, f),
            _ => {}
        }
    }
}

/// Inline formatting state while collecting runs.
#[derive(Default)]
struct Inline {
    bold: u32,
    italic: u32,
    strike: u32,
    links: Vec<String>,
}

impl Inline {
    fn style(&self, code: bool) -> Style {
        Style {
            bold: self.bold > 0,
            italic: self.italic > 0,
            strike: self.strike > 0,
            code,
        }
    }
}

struct Builder<'a, I: Iterator<Item = (Event<'a>, Range<usize>)>> {
    events: Peekable<I>,
    source: &'a str,
    /// Byte offset of the start of every line.
    line_starts: Vec<usize>,
    /// Task marker of every open list item, innermost last.
    checks: Vec<Option<bool>>,
    inline: Inline,
}

impl<'a, I: Iterator<Item = (Event<'a>, Range<usize>)>> Builder<'a, I> {
    /// Blocks up to (and including) the end of the enclosing container, or the end of input.
    fn blocks(&mut self) -> Vec<Block> {
        let mut blocks = Vec::new();
        while let Some((event, range)) = self.events.peek() {
            if is_inline(event) {
                // Tight list items hold their text without a paragraph.
                let range = range.clone();
                let (runs, range) = self.runs(range);
                blocks.push(Block {
                    kind: BlockKind::Paragraph(runs),
                    lines: self.lines(range),
                });
                continue;
            }
            let Some((event, range)) = self.events.next() else {
                break;
            };
            let kind = match event {
                Event::End(_) => break,
                Event::Rule => BlockKind::Rule,
                Event::Start(Tag::Paragraph) => {
                    let (runs, _) = self.runs(range.clone());
                    self.events.next();
                    BlockKind::Paragraph(runs)
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    let (runs, _) = self.runs(range.clone());
                    self.events.next();
                    BlockKind::Heading {
                        level: level as u8,
                        runs,
                    }
                }
                Event::Start(Tag::BlockQuote(alert)) => BlockKind::Quote {
                    alert,
                    blocks: self.blocks(),
                },
                Event::Start(Tag::CodeBlock(kind)) => BlockKind::Code {
                    lang: match kind {
                        CodeBlockKind::Fenced(info) => {
                            info.split_whitespace().next().map(str::to_owned)
                        }
                        CodeBlockKind::Indented => None,
                    },
                    text: self.text(),
                },
                Event::Start(Tag::HtmlBlock) => BlockKind::Html(self.text()),
                Event::Start(Tag::List(start)) => BlockKind::List {
                    start,
                    items: self.items(),
                },
                Event::Start(Tag::Table(alignments)) => {
                    let mut rows = self.rows();
                    let head = if rows.is_empty() {
                        Row {
                            cells: Vec::new(),
                            lines: self.lines(range.clone()),
                        }
                    } else {
                        rows.remove(0)
                    };
                    BlockKind::Table {
                        aligns: alignments.into_iter().map(Align::from).collect(),
                        head,
                        rows,
                    }
                }
                // Not enabled by `parse` (footnotes, metadata, …): keep their content.
                Event::Start(_) => {
                    blocks.extend(self.blocks());
                    continue;
                }
                // Stray `Html`/`TaskListMarker` outside a block: shown as text / ignored.
                Event::Html(html) => BlockKind::Html(html.trim_end().to_owned()),
                _ => continue,
            };
            blocks.push(Block {
                kind,
                lines: self.lines(range),
            });
        }
        blocks
    }

    /// List items up to (and including) the end of the list.
    fn items(&mut self) -> Vec<Item> {
        let mut items = Vec::new();
        while let Some((event, range)) = self.events.next() {
            if !matches!(event, Event::Start(Tag::Item)) {
                break;
            }
            self.checks.push(None);
            let blocks = self.blocks();
            items.push(Item {
                checked: self.checks.pop().flatten(),
                blocks,
                lines: self.lines(range),
            });
        }
        items
    }

    /// Table head and body rows up to (and including) the end of the table; the head comes first.
    fn rows(&mut self) -> Vec<Row> {
        let mut rows = Vec::new();
        while let Some((event, range)) = self.events.next() {
            if !matches!(event, Event::Start(Tag::TableHead | Tag::TableRow)) {
                break;
            }
            let mut cells = Vec::new();
            while let Some((Event::Start(Tag::TableCell), cell)) = self.events.next() {
                cells.push(self.runs(cell).0);
                self.events.next();
            }
            rows.push(Row {
                cells,
                lines: self.lines(range),
            });
        }
        rows
    }

    /// Text of a code or HTML block up to (and including) its end, without the final newline.
    fn text(&mut self) -> String {
        let mut text = String::new();
        for (event, _) in self.events.by_ref() {
            match event {
                Event::Text(t) | Event::Html(t) => text.push_str(&t),
                _ => break,
            }
        }
        // pulldown-cmark already turns CRLF into LF here.
        if text.ends_with('\n') {
            text.pop();
        }
        text
    }

    /// Consumes the inline events that follow, returning their runs and the byte range covering
    /// `range` and every consumed event. The block's `End` is left in place.
    fn runs(&mut self, mut range: Range<usize>) -> (Vec<Run>, Range<usize>) {
        let mut runs: Vec<Run> = Vec::new();
        while let Some((event, _)) = self.events.peek() {
            if !is_inline(event) {
                break;
            }
            let Some((event, at)) = self.events.next() else {
                break;
            };
            range.start = range.start.min(at.start);
            range.end = range.end.max(at.end);
            let (text, code) = match event {
                Event::Text(t)
                | Event::InlineHtml(t)
                | Event::InlineMath(t)
                | Event::DisplayMath(t) => (t, false),
                Event::Code(t) => (t, true),
                Event::SoftBreak => (" ".into(), false),
                Event::HardBreak => ("\n".into(), false),
                Event::FootnoteReference(name) => (format!("[^{name}]").into(), false),
                Event::TaskListMarker(checked) => {
                    if let Some(check) = self.checks.last_mut() {
                        *check = Some(checked);
                    }
                    continue;
                }
                Event::Start(tag) => {
                    match tag {
                        Tag::Strong => self.inline.bold += 1,
                        Tag::Emphasis => self.inline.italic += 1,
                        Tag::Strikethrough => self.inline.strike += 1,
                        Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. } => {
                            self.inline.links.push(dest_url.into_string())
                        }
                        _ => {}
                    }
                    continue;
                }
                Event::End(tag) => {
                    match tag {
                        // pulldown-cmark balances inline tags; saturate anyway.
                        TagEnd::Strong => self.inline.bold = self.inline.bold.saturating_sub(1),
                        TagEnd::Emphasis => {
                            self.inline.italic = self.inline.italic.saturating_sub(1)
                        }
                        TagEnd::Strikethrough => {
                            self.inline.strike = self.inline.strike.saturating_sub(1)
                        }
                        TagEnd::Link | TagEnd::Image => {
                            self.inline.links.pop();
                        }
                        _ => {}
                    }
                    continue;
                }
                _ => continue,
            };
            let style = self.inline.style(code);
            let link = self.inline.links.last().cloned();
            match runs.last_mut() {
                Some(last) if last.style == style && last.link == link => last.text.push_str(&text),
                _ => runs.push(Run {
                    text: text.into_string(),
                    style,
                    link,
                }),
            }
        }
        (runs, range)
    }

    /// Lines of a byte range, ignoring trailing whitespace (blocks may own trailing blank lines).
    fn lines(&self, range: Range<usize>) -> Lines {
        let trimmed = self.source[range.clone()].trim_end().len();
        let last = range.start + trimmed.saturating_sub(1);
        Lines {
            start: self.line_of(range.start),
            end: self.line_of(last),
        }
    }

    fn line_of(&self, byte: usize) -> u32 {
        let line = self.line_starts.partition_point(|&start| start <= byte);
        u32::try_from(line).unwrap_or(u32::MAX)
    }
}

/// Events that belong inside a paragraph, heading or table cell.
fn is_inline(event: &Event<'_>) -> bool {
    match event {
        Event::Start(tag) => matches!(
            tag,
            Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Superscript
                | Tag::Subscript
                | Tag::Link { .. }
                | Tag::Image { .. }
        ),
        Event::End(tag) => matches!(
            tag,
            TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Superscript
                | TagEnd::Subscript
                | TagEnd::Link
                | TagEnd::Image
        ),
        Event::Text(_)
        | Event::Code(_)
        | Event::InlineHtml(_)
        | Event::InlineMath(_)
        | Event::DisplayMath(_)
        | Event::FootnoteReference(_)
        | Event::SoftBreak
        | Event::HardBreak
        | Event::TaskListMarker(_) => true,
        Event::Html(_) | Event::Rule => false,
    }
}

/// Where a clicked link goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// `http`, `https` or `mailto`: the system opens it.
    Web(String),
    /// `#anchor`: a heading of the shown file.
    Anchor(String),
    /// A file of the shown folder (relative, `/`-separated), maybe at a line (`#L12`).
    File { path: String, line: Option<u32> },
}

/// Where link `dest` of file `from` (relative, `/`-separated) goes: `#x` is a heading of the
/// file, a relative path is resolved against `from`'s folder and `/path` against the root.
/// `None` for what nothing opens: other schemes (`file:`, `javascript:` …), a path leaving the
/// folder, an empty link.
pub fn link_target(from: &str, dest: &str) -> Option<LinkTarget> {
    let dest = dest.trim();
    if let Some(anchor) = dest.strip_prefix('#') {
        return (!anchor.is_empty()).then(|| LinkTarget::Anchor(percent_decode(anchor)));
    }
    // A one-letter "scheme" is a Windows drive (`C:\…`), not a URL.
    if let Some((scheme, _)) = dest.split_once(':')
        && scheme.len() > 1
        && (scheme.chars()).all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    {
        let web = ["http", "https", "mailto"].contains(&scheme.to_ascii_lowercase().as_str());
        return web.then(|| LinkTarget::Web(dest.to_owned()));
    }
    let (path, fragment) = match dest.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (dest, None),
    };
    let path = percent_decode(path.split('?').next().unwrap_or_default());
    if path.is_empty() {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    if !path.starts_with('/') {
        parts.extend(from.split('/'));
        parts.pop();
    }
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    let path = parts.join("/");
    if path.is_empty() {
        return None;
    }
    let line = (fragment.and_then(|f| f.strip_prefix('L')))
        .and_then(|f| f.split('-').next()?.parse().ok());
    match fragment {
        Some(anchor) if path == from && line.is_none() && !anchor.is_empty() => {
            Some(LinkTarget::Anchor(percent_decode(anchor)))
        }
        _ => Some(LinkTarget::File { path, line }),
    }
}

/// `%XX` escapes decoded (links to `My%20File.md`, encoded anchors); invalid UTF-8 is replaced.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let hex = |at: usize| (bytes.get(at).map(|b| *b as char)).and_then(|c| c.to_digit(16));
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        if byte == b'%'
            && let (Some(high), Some(low)) = (hex(at + 1), hex(at + 2))
        {
            // Two hex digits are at most 0xFF.
            out.push((high * 16 + low) as u8);
            at += 3;
        } else {
            out.push(byte);
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// GitHub's anchor of a heading: lower case, letters, digits, `-` and `_` kept, spaces become
/// `-`, anything else dropped.
pub fn slug(text: &str) -> String {
    (text.trim().chars())
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// The top-level heading `anchor` names (an index into `blocks`): anchors are given in document
/// order, a repeated one gets `-1`, `-2`, … like on GitHub.
pub fn heading_for(blocks: &[Block], anchor: &str) -> Option<usize> {
    let anchor = anchor.to_lowercase();
    let mut seen = std::collections::HashMap::<String, usize>::new();
    blocks.iter().position(|block| {
        let BlockKind::Heading { runs, .. } = &block.kind else {
            return false;
        };
        let base = slug(&runs.iter().map(|run| run.text.as_str()).collect::<String>());
        let count = seen.entry(base.clone()).or_default();
        let id = match *count {
            0 => base,
            n => format!("{base}-{n}"),
        };
        *count += 1;
        id == anchor
    })
}

/// The edit that ticks or unticks the task item whose first source line is `line`: the byte
/// inside its `[ ]` / `[x]` and what goes there. `None` when the line has no task marker right
/// after its list marker.
pub fn task_toggle(line: &str) -> Option<(usize, &'static str)> {
    let at = line.find('[')?;
    let marker = (line[..at].chars()).all(|c| c.is_whitespace() || "-*+>.)0123456789".contains(c));
    let new = match line.get(at + 1..at + 3)? {
        " ]" => "x",
        "x]" | "X]" => " ",
        _ => return None,
    };
    (marker && !line[..at].trim().is_empty()).then_some((at + 1, new))
}

/// The top-level block shown for 1-based source `line`, and how far into it (0 at its first
/// line, towards 1 at its last): the last block starting at or before `line`; a line past its
/// end (a blank line between blocks) counts as its end. `None` before the first block.
pub fn block_at_line(blocks: &[Block], line: u32) -> Option<(usize, f32)> {
    let index = (blocks.partition_point(|block| block.lines.start <= line)).checked_sub(1)?;
    let lines = blocks[index].lines;
    let span = (lines.end - lines.start + 1) as f32;
    Some((index, ((line - lines.start) as f32 / span).min(1.0)))
}

/// The YAML front matter as rows: `key: value` lines split at the first `:`, other lines
/// (list items, nested values, comments) whole without a key. Blank lines are dropped.
pub fn front_matter_rows(text: &str) -> Vec<(Option<&str>, &str)> {
    (text.lines())
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let top = !line.starts_with([' ', '\t', '-', '#']);
            match line.split_once(':').filter(|_| top) {
                Some((key, value)) => (Some(key.trim()), value.trim()),
                None => (None, line.trim_end()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Indented outline of the model: `[lines] kind: runs`, runs written back as Markdown-ish
    /// markup (`**bold**`, `_italic_`, `~~strike~~`, `` `code` ``, `[text](url)`).
    fn dump(blocks: &[Block]) -> String {
        let mut out = String::new();
        dump_blocks(blocks, 0, &mut out);
        out
    }

    fn dump_blocks(blocks: &[Block], depth: usize, out: &mut String) {
        for block in blocks {
            let pad = "  ".repeat(depth);
            let at = span(block.lines);
            match &block.kind {
                BlockKind::Heading { level, runs } => {
                    out.push_str(&format!("{pad}{at} h{level}: {}\n", markup(runs)))
                }
                BlockKind::Paragraph(runs) => {
                    out.push_str(&format!("{pad}{at} p: {}\n", markup(runs)))
                }
                BlockKind::List { start, items } => {
                    out.push_str(&format!("{pad}{at} list start={start:?}\n"));
                    for item in items {
                        let check = match item.checked {
                            Some(true) => " [x]",
                            Some(false) => " [ ]",
                            None => "",
                        };
                        out.push_str(&format!("{pad}  {} item{check}\n", span(item.lines)));
                        dump_blocks(&item.blocks, depth + 2, out);
                    }
                }
                BlockKind::Quote { alert, blocks } => {
                    let alert = alert.map(|alert| format!(" {alert:?}")).unwrap_or_default();
                    out.push_str(&format!("{pad}{at} quote{alert}\n"));
                    dump_blocks(blocks, depth + 1, out);
                }
                BlockKind::Code { lang, text } => {
                    out.push_str(&format!("{pad}{at} code lang={lang:?}\n"));
                    for line in text.lines() {
                        out.push_str(&format!("{pad}  | {line}\n"));
                    }
                }
                BlockKind::Table { aligns, head, rows } => {
                    out.push_str(&format!("{pad}{at} table {aligns:?}\n"));
                    for row in std::iter::once(head).chain(rows) {
                        let cells: Vec<String> = row.cells.iter().map(|c| markup(c)).collect();
                        out.push_str(&format!(
                            "{pad}  {} | {} |\n",
                            span(row.lines),
                            cells.join(" | ")
                        ));
                    }
                }
                BlockKind::Rule => out.push_str(&format!("{pad}{at} rule\n")),
                BlockKind::Html(html) => out.push_str(&format!("{pad}{at} html: {html:?}\n")),
                BlockKind::FrontMatter(text) => {
                    out.push_str(&format!("{pad}{at} front matter: {text:?}\n"))
                }
            }
        }
    }

    fn span(lines: Lines) -> String {
        if lines.start == lines.end {
            format!("[{}]", lines.start)
        } else {
            format!("[{}-{}]", lines.start, lines.end)
        }
    }

    fn markup(runs: &[Run]) -> String {
        runs.iter()
            .map(|run| {
                let mut text = run.text.replace('\n', "⏎");
                if run.style.code {
                    text = format!("`{text}`");
                }
                if run.style.strike {
                    text = format!("~~{text}~~");
                }
                if run.style.italic {
                    text = format!("_{text}_");
                }
                if run.style.bold {
                    text = format!("**{text}**");
                }
                match &run.link {
                    Some(url) => format!("[{text}]({url})"),
                    None => text,
                }
            })
            .collect()
    }

    const PLAN: &str = "\
# Plan

Use **Redis** for _caching_, see [docs](https://redis.io) and `cfg.toml`.
Second line with ~~old~~ text.

## Steps

1. Design
2. Build
   - nested *one*
   - nested two
3. Ship

- [x] write spec
- [ ] review it

> Quoted **advice**
> on two lines.

```rust
fn main() {}
```

| Name | Size |
|:-----|-----:|
| a    | 1    |
| **b** | 22  |

---

<div>raw</div>
";

    #[test]
    fn plan_renders_every_block_kind_with_line_ranges() {
        insta::assert_snapshot!(dump(&parse(PLAN)));
    }

    #[test]
    fn loose_lists_keep_paragraphs_and_task_markers() {
        let source = "- [x] first\n\n  more text\n\n- [ ] second\n";
        insta::assert_snapshot!(dump(&parse(source)));
    }

    #[test]
    fn crlf_sources_use_the_same_lines() {
        let lf = parse(PLAN);
        let crlf = parse(&PLAN.replace('\n', "\r\n"));
        assert_eq!(dump(&lf), dump(&crlf));
    }

    #[test]
    fn code_block_lines_cover_the_fences() {
        let blocks = parse("text\n\n```\na\nb\n```\n\nafter\n");
        assert_eq!(blocks[1].lines, Lines { start: 3, end: 6 });
        assert_eq!(
            blocks[1].kind,
            BlockKind::Code {
                lang: None,
                text: "a\nb".into()
            }
        );
        assert_eq!(blocks[2].lines, Lines { start: 8, end: 8 });
    }

    #[test]
    fn hard_breaks_and_images_become_runs() {
        let blocks = parse("one  \ntwo ![logo](logo.png)\n");
        let BlockKind::Paragraph(runs) = &blocks[0].kind else {
            panic!("expected a paragraph: {blocks:?}");
        };
        assert_eq!(markup(runs), "one⏎two [logo](logo.png)");
    }

    #[test]
    fn empty_source_has_no_blocks() {
        assert!(parse("").is_empty());
        assert!(parse("\n\n").is_empty());
    }

    #[test]
    fn alerts_and_front_matter_are_their_own_blocks() {
        let source = "---\ntitle: Plan\n---\n\n> [!WARNING]\n> Mind **this**.\n\n> plain\n";
        insta::assert_snapshot!(dump(&parse(source)), @r#"
        [1-3] front matter: "title: Plan"
        [5-6] quote Warning
          [6] p: Mind **this**.
        [8] quote
          [8] p: plain
        "#);
        // A `---` further down is a rule, not front matter.
        assert_eq!(parse("text\n\n---\n")[1].kind, BlockKind::Rule);
        // Two rules around a section stay a section: nothing between them is swallowed.
        let blocks = parse("text\n\n---\n## Part 2\n\nbody\n\n---\n## Part 3\n");
        assert_eq!(blocks.len(), 6, "{blocks:?}");
        assert!(
            !blocks
                .iter()
                .any(|b| matches!(b.kind, BlockKind::FrontMatter(_)))
        );
        // An unclosed or CRLF front matter: unclosed is a rule and a heading, CRLF closes.
        assert!(matches!(parse("---\ntitle: x\n")[0].kind, BlockKind::Rule));
        let crlf = parse("---\r\ntitle: x\r\n---\r\n# T\r\n");
        assert_eq!(crlf[0].kind, BlockKind::FrontMatter("title: x".into()));
        assert_eq!(crlf[1].lines, Lines { start: 4, end: 4 });
    }

    #[test]
    fn front_matter_rows_split_top_level_keys_only() {
        let rows = front_matter_rows("title: A: B\ntags:\n  - x\n\n# note\nurl: https://x.io");
        assert_eq!(
            rows,
            [
                (Some("title"), "A: B"),
                (Some("tags"), ""),
                (None, "  - x"),
                (None, "# note"),
                (Some("url"), "https://x.io"),
            ]
        );
    }

    #[test]
    fn links_resolve_against_the_file_folder() {
        let file = |path: &str, line| {
            Some(LinkTarget::File {
                path: path.into(),
                line,
            })
        };
        let from = "docs/specs/plan.md";
        assert_eq!(link_target(from, "../SPEC.md"), file("docs/SPEC.md", None));
        assert_eq!(
            link_target(from, "./a b.md"),
            file("docs/specs/a b.md", None)
        );
        assert_eq!(
            link_target(from, "My%20File.md"),
            file("docs/specs/My File.md", None)
        );
        assert_eq!(
            link_target(from, "/src/main.rs#L12"),
            file("src/main.rs", Some(12))
        );
        assert_eq!(
            link_target(from, "x.rs#L3-L9"),
            file("docs/specs/x.rs", Some(3))
        );
        // A heading of another file opens that file; one of this file is an anchor.
        assert_eq!(
            link_target(from, "x.md#intro"),
            file("docs/specs/x.md", None)
        );
        assert_eq!(
            link_target(from, "plan.md#intro"),
            Some(LinkTarget::Anchor("intro".into()))
        );
        assert_eq!(
            link_target(from, "#c%C3%A1ch-d%C3%B9ng"),
            Some(LinkTarget::Anchor("cách-dùng".into()))
        );
        // Leaving the folder, empty links and unsafe schemes open nothing.
        assert_eq!(link_target(from, "../../../etc/passwd"), None);
        assert_eq!(link_target(from, ""), None);
        assert_eq!(link_target(from, "#"), None);
        assert_eq!(link_target(from, "javascript:alert(1)"), None);
        assert_eq!(link_target(from, "file:///C:/x"), None);
        assert_eq!(
            link_target(from, "HTTPS://redis.io"),
            Some(LinkTarget::Web("HTTPS://redis.io".into()))
        );
        assert_eq!(
            link_target(from, "mailto:a@b.c"),
            Some(LinkTarget::Web("mailto:a@b.c".into()))
        );
    }

    #[test]
    fn anchors_follow_github_slugs_with_numbered_repeats() {
        assert_eq!(slug(" Cách dùng: API v2.0 "), "cách-dùng-api-v20");
        let blocks = parse("# Intro\n\npara\n\n## Steps\n\n## Steps\n\n### Ship it!\n");
        assert_eq!(heading_for(&blocks, "intro"), Some(0));
        assert_eq!(heading_for(&blocks, "steps"), Some(2));
        assert_eq!(heading_for(&blocks, "steps-1"), Some(3));
        assert_eq!(heading_for(&blocks, "Ship-It"), Some(4));
        assert_eq!(heading_for(&blocks, "para"), None);
    }

    #[test]
    fn task_toggle_flips_the_marker_after_the_list_marker() {
        assert_eq!(task_toggle("- [ ] todo"), Some((3, "x")));
        assert_eq!(task_toggle("  * [x] done"), Some((5, " ")));
        assert_eq!(task_toggle("12. [X] done"), Some((5, " ")));
        assert_eq!(task_toggle("> - [ ] quoted"), Some((5, "x")));
        // Brackets that are not the item's marker are left alone.
        assert_eq!(task_toggle("- see [ ] later"), None);
        assert_eq!(task_toggle("[ ] no list marker"), None);
        assert_eq!(task_toggle("- [link](x)"), None);
    }

    #[test]
    fn block_at_line_finds_the_block_and_the_way_into_it() {
        let blocks = parse("\n# Title\n\n- a\n- b\n- c\n- d\n\nend\n");
        assert_eq!(block_at_line(&blocks, 1), None);
        assert_eq!(block_at_line(&blocks, 2), Some((0, 0.0)));
        // Line 3 is the blank after the heading: its end.
        assert_eq!(block_at_line(&blocks, 3), Some((0, 1.0)));
        assert_eq!(block_at_line(&blocks, 6), Some((1, 0.5)));
        assert_eq!(block_at_line(&blocks, 99), Some((2, 1.0)));
    }
}
