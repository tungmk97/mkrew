//! Markdown render model for the Docs preview (SPEC §12): `pulldown-cmark` events → a tree of
//! blocks (headings, paragraphs, lists, task lists, tables, code blocks, quotes, rules) made of
//! styled inline runs. Every block and table row carries its source line range so preview
//! selections and comment anchors map to lines of the file. Pure and tested headlessly;
//! `main.rs` turns it into GPUI elements.

use std::iter::Peekable;
use std::ops::Range;

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
    Quote(Vec<Block>),
    /// Shown verbatim (no syntax highlighting in v1); `text` has no trailing newline.
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

/// Parses `source` (GitHub-flavoured tables, task lists and strikethrough enabled).
pub fn parse(source: &str) -> Vec<Block> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let mut builder = Builder {
        events: Parser::new_ext(source, options)
            .into_offset_iter()
            .peekable(),
        source,
        line_starts: std::iter::once(0)
            .chain(source.match_indices('\n').map(|(i, _)| i + 1))
            .collect(),
        checks: Vec::new(),
        inline: Inline::default(),
    };
    builder.blocks()
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
                Event::Start(Tag::BlockQuote(_)) => BlockKind::Quote(self.blocks()),
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
                BlockKind::Quote(inner) => {
                    out.push_str(&format!("{pad}{at} quote\n"));
                    dump_blocks(inner, depth + 1, out);
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
}
