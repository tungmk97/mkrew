//! Link hints (SPEC §10): short labels over the links on screen; typing a label opens its
//! link. Pure and tested headlessly; the pane scans the rows, draws the labels and opens the
//! link (like Ctrl+Click). The letters are not in `keymap.toml`.

/// The home row first.
const ALPHABET: &[u8] = b"asdfghjklqwertyuiopzxcvbnm";

/// `count` labels in link order: single letters first, then two letters. No label starts
/// another, so a label is complete the moment it is typed: the last letters of the alphabet
/// stop being labels and become the first letter of the two-letter ones.
// ponytail: at most 26 * 26 labels; links past that get none. A screen holds far fewer.
pub fn labels(count: usize) -> Vec<String> {
    let n = ALPHABET.len();
    // `prefixes` letters give up being labels: capacity (n - prefixes) + prefixes * n.
    let prefixes = if count <= n {
        0
    } else {
        (count - n).div_ceil(n - 1).min(n)
    };
    let letter = |i: usize| char::from(ALPHABET[i]);
    let mut out: Vec<String> = (0..n - prefixes).map(|i| letter(i).to_string()).collect();
    for first in n - prefixes..n {
        for second in 0..n {
            out.push(format!("{}{}", letter(first), letter(second)));
        }
    }
    out.truncate(count);
    out
}

/// What the letters typed so far mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typed {
    /// A whole label: open the link at this index.
    Pick(usize),
    /// Some labels still start with it: wait for the next letter.
    Narrow,
    /// None does: cancel.
    Nothing,
}

/// `typed` against `labels` (prefix-free, see [`labels`]).
pub fn typed(labels: &[String], typed: &str) -> Typed {
    if let Some(at) = labels.iter().position(|label| label == typed) {
        return Typed::Pick(at);
    }
    if !typed.is_empty() && labels.iter().any(|label| label.starts_with(typed)) {
        Typed::Narrow
    } else {
        Typed::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_few_links_get_the_home_row() {
        assert_eq!(labels(3), ["a", "s", "d"]);
        assert!(labels(0).is_empty());
        assert_eq!(labels(26).len(), 26);
    }

    #[test]
    fn more_links_than_letters_get_two_letters_after_the_single_ones() {
        let all = labels(27);
        assert_eq!(all.len(), 27);
        assert_eq!(all[0], "a");
        assert_eq!(all[24], "n", "the last single letter");
        assert_eq!(
            &all[25..],
            ["ma", "ms"],
            "two letters start with the unused letter"
        );
        assert!(!all.contains(&"m".to_owned()), "m is a prefix now");
    }

    #[test]
    fn labels_never_start_with_each_other() {
        for count in [1, 5, 26, 27, 60, 200, 676, 700] {
            let all = labels(count);
            assert_eq!(all.len(), count.min(676), "{count}");
            for (i, a) in all.iter().enumerate() {
                assert!(a.chars().all(|c| c.is_ascii_lowercase()));
                for (j, b) in all.iter().enumerate() {
                    assert!(i == j || !b.starts_with(a.as_str()), "{count}: {a} / {b}");
                }
            }
        }
    }

    #[test]
    fn typing_picks_narrows_or_cancels() {
        let all = labels(30);
        assert_eq!(typed(&all, "a"), Typed::Pick(0));
        assert_eq!(typed(&all, "m"), Typed::Narrow);
        assert_eq!(typed(&all, "ms"), Typed::Pick(26));
        assert_eq!(typed(&all, "mq"), Typed::Nothing);
        assert_eq!(typed(&all, "1"), Typed::Nothing);
        assert_eq!(typed(&[], "a"), Typed::Nothing);
    }
}
