//! Plan checklists (SPEC §10, ADR 0039): the `- [ ]` / `- [x]` lines of a Markdown file.

use mkrew_protocol::PlanItem;

/// Longest item text kept, in characters; a longer one is cut and ends with `…`.
pub const MAX_TEXT_CHARS: usize = 120;
/// Most items kept (`total` still counts every one): the event stays small whatever the file.
pub const MAX_ITEMS: usize = 200;

/// The task-list items of one file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Checklist {
    pub done: u32,
    pub total: u32,
    /// The first [`MAX_ITEMS`] items in file order.
    pub items: Vec<PlanItem>,
}

/// The `- [ ]` / `- [x]` / `* [X]` / `+ [ ]` lines of `src`, at any indent, outside fenced
/// code blocks (``` or ~~~; an unclosed fence runs to the end, as rendered). A box needs a
/// space after it and some text; ordered lists (`1. [ ]`) are not counted.
pub fn parse(src: &str) -> Checklist {
    let mut list = Checklist::default();
    let mut fence: Option<(char, usize)> = None;
    for line in src.lines() {
        let line = line.trim_start();
        if let Some((marker, len)) = fence {
            let run = line.chars().take_while(|&c| c == marker).count();
            if run >= len && line[run..].trim().is_empty() {
                fence = None;
            }
            continue;
        }
        if let Some(open) = fence_open(line) {
            fence = Some(open);
            continue;
        }
        let Some((done, text)) = task_item(line) else {
            continue;
        };
        list.total += 1;
        list.done += u32::from(done);
        if list.items.len() < MAX_ITEMS {
            list.items.push(PlanItem {
                done,
                text: cut(text),
            });
        }
    }
    list
}

/// The fence character and length of a line that opens a code fence.
fn fence_open(line: &str) -> Option<(char, usize)> {
    let marker = line.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let len = line.chars().take_while(|&c| c == marker).count();
    (len >= 3).then_some((marker, len))
}

/// `(done, text)` of a task-list line (already left-trimmed).
fn task_item(line: &str) -> Option<(bool, &str)> {
    let rest = line.strip_prefix(['-', '*', '+'])?;
    let rest = rest.strip_prefix([' ', '\t'])?.trim_start();
    let (done, rest) = match rest.strip_prefix("[ ]") {
        Some(rest) => (false, rest),
        None => (
            true,
            rest.strip_prefix("[x]")
                .or_else(|| rest.strip_prefix("[X]"))?,
        ),
    };
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let text = rest.trim();
    (!text.is_empty()).then_some((done, text))
}

fn cut(text: &str) -> String {
    match text.char_indices().nth(MAX_TEXT_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(list: &Checklist) -> Vec<(&str, bool)> {
        list.items
            .iter()
            .map(|item| (item.text.as_str(), item.done))
            .collect()
    }

    #[test]
    fn counts_every_bullet_style_and_box_case() {
        let list = parse("- [ ] a\n- [x] b\n* [X] c\n+ [ ] d\n");
        assert_eq!((list.done, list.total), (2, 4));
        assert_eq!(
            texts(&list),
            [("a", false), ("b", true), ("c", true), ("d", false)]
        );
        assert_eq!(parse(""), Checklist::default());
    }

    #[test]
    fn nested_items_and_crlf_count_and_text_is_trimmed() {
        let list = parse("# Plan\r\n- [x]   one  \r\n  - [ ] two\r\n\t- [ ] three\r\nprose\r\n");
        assert_eq!((list.done, list.total), (1, 3));
        assert_eq!(
            texts(&list),
            [("one", true), ("two", false), ("three", false)]
        );
    }

    #[test]
    fn fenced_code_is_not_a_checklist() {
        let src = "- [ ] a\n```md\n- [ ] hidden\n```\n~~~~\n- [x] hidden\n~~~\n- [x] still hidden\n~~~~\n- [x] b\n";
        assert_eq!(texts(&parse(src)), [("a", false), ("b", true)]);
        // An unclosed fence hides the rest of the file, as Markdown renders it.
        assert_eq!(parse("- [ ] a\n```\n- [ ] b\n").total, 1);
    }

    #[test]
    fn lines_that_are_not_task_items_are_ignored() {
        for line in [
            "- [] a",
            "- [y] a",
            "-[ ] a",
            "[ ] a",
            "- [x]a",
            "1. [ ] a",
            "- [ ]",
            "- [ ]   ",
            "text - [ ] a",
            "-- [ ] a",
        ] {
            assert_eq!(parse(line).total, 0, "{line:?}");
        }
    }

    #[test]
    fn long_text_is_cut_and_items_are_capped_but_counted() {
        let long = "x".repeat(MAX_TEXT_CHARS + 30);
        let list = parse(&format!("- [ ] {long}\n"));
        assert_eq!(list.items[0].text.chars().count(), MAX_TEXT_CHARS + 1);
        assert!(list.items[0].text.ends_with('…'));
        // Never cut inside a multi-byte character.
        let wide = "é".repeat(MAX_TEXT_CHARS + 1);
        assert!(parse(&format!("- [x] {wide}")).items[0].text.ends_with('…'));

        let many: String = (0..MAX_ITEMS + 50)
            .map(|i| format!("- [{}] item {i}\n", if i % 2 == 0 { 'x' } else { ' ' }))
            .collect();
        let list = parse(&many);
        assert_eq!(
            (list.total as usize, list.items.len()),
            (MAX_ITEMS + 50, MAX_ITEMS)
        );
        assert_eq!(list.done as usize, (MAX_ITEMS + 50) / 2);
    }
}
