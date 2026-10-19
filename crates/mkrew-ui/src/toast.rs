//! In-app toasts (SPEC §14): a short queue of messages drawn bottom-right over the content.
//! Pure: the time is passed in, so the tests need no clock. `shell/toasts.rs` draws it and
//! the status tick expires it.

use std::time::Instant;

use crate::theme::TOAST_DURATION;

/// Most toasts shown at once; a new one past this drops the oldest.
pub const MAX_VISIBLE: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Never reused, so a late click on ✕ cannot close a newer toast.
    pub id: u64,
    pub kind: Kind,
    pub text: String,
    /// It is dropped by the first [`Toasts::expire`] at or after this time.
    pub expires: Instant,
}

/// The shown toasts, oldest first.
#[derive(Debug, Default)]
pub struct Toasts {
    next_id: u64,
    items: Vec<Toast>,
}

impl Toasts {
    /// Shows `text`. The same kind and text as a toast already shown restarts that one instead
    /// of stacking a copy (a failure that repeats); past [`MAX_VISIBLE`] the oldest goes.
    pub fn push(&mut self, kind: Kind, text: impl Into<String>, now: Instant) {
        let text = text.into();
        // Overflow of an `Instant` is not reachable in practice; if it ever were, the toast
        // would just hide at the next tick.
        let expires = now.checked_add(TOAST_DURATION).unwrap_or(now);
        if let Some(shown) = self
            .items
            .iter_mut()
            .find(|toast| toast.kind == kind && toast.text == text)
        {
            shown.expires = expires;
            return;
        }
        self.next_id += 1;
        self.items.push(Toast {
            id: self.next_id,
            kind,
            text,
            expires,
        });
        if self.items.len() > MAX_VISIBLE {
            self.items.remove(0);
        }
    }

    /// The ✕ of toast `id`. False when it is already gone.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|toast| toast.id != id);
        self.items.len() != before
    }

    /// Drops what has expired by `now`. True when something went (the window needs a redraw).
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.items.len();
        self.items.retain(|toast| toast.expires > now);
        self.items.len() != before
    }

    pub fn items(&self) -> &[Toast] {
        &self.items
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::theme::TOAST_DURATION;

    fn later(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn texts(toasts: &Toasts) -> Vec<&str> {
        toasts
            .items()
            .iter()
            .map(|toast| toast.text.as_str())
            .collect()
    }

    #[test]
    fn a_toast_hides_exactly_when_toast_duration_has_passed() {
        let base = Instant::now();
        let mut toasts = Toasts::default();
        toasts.push(Kind::Error, "Cannot save the theme", base);
        assert!(!toasts.expire(base + TOAST_DURATION - Duration::from_millis(1)));
        assert_eq!(texts(&toasts), ["Cannot save the theme"]);
        assert!(toasts.expire(base + TOAST_DURATION));
        assert!(toasts.items().is_empty());
        assert!(!toasts.expire(base + TOAST_DURATION * 2), "nothing left to drop");
    }

    #[test]
    fn the_oldest_toast_expires_first_and_the_newer_one_keeps_its_own_time() {
        let base = Instant::now();
        let half = TOAST_DURATION / 2;
        let mut toasts = Toasts::default();
        toasts.push(Kind::Info, "a", base);
        toasts.push(Kind::Info, "b", base + half);
        assert!(toasts.expire(base + TOAST_DURATION));
        assert_eq!(texts(&toasts), ["b"]);
        assert!(toasts.expire(base + half + TOAST_DURATION));
        assert!(toasts.items().is_empty());
    }

    #[test]
    fn a_full_stack_drops_the_oldest_and_a_repeat_drops_nothing() {
        let base = Instant::now();
        let mut toasts = Toasts::default();
        for text in ["a", "b", "c", "d"] {
            toasts.push(Kind::Error, text, base);
        }
        assert_eq!(toasts.items().len(), MAX_VISIBLE);
        assert_eq!(texts(&toasts), ["b", "c", "d"], "the oldest went");
        let ids: Vec<u64> = toasts.items().iter().map(|toast| toast.id).collect();
        toasts.push(Kind::Error, "b", later(base, 1));
        let again: Vec<u64> = toasts.items().iter().map(|toast| toast.id).collect();
        assert_eq!(again, ids, "a repeat of a shown message drops nobody");
    }

    #[test]
    fn the_same_message_restarts_its_toast_instead_of_stacking() {
        let base = Instant::now();
        let mut toasts = Toasts::default();
        toasts.push(Kind::Error, "Cannot follow in Zed: not found", base);
        let first = toasts.items()[0].id;
        let again = later(base, 3_000);
        toasts.push(Kind::Error, "Cannot follow in Zed: not found", again);
        assert_eq!(toasts.items().len(), 1);
        assert_eq!(toasts.items()[0].id, first);
        assert!(
            !toasts.expire(base + TOAST_DURATION),
            "restarted: the first start no longer counts"
        );
        assert!(toasts.expire(again + TOAST_DURATION));
        // The same words with another kind are another message.
        toasts.push(Kind::Info, "x", base);
        toasts.push(Kind::Error, "x", base);
        assert_eq!(toasts.items().len(), 2);
    }

    #[test]
    fn dismiss_removes_only_its_toast_and_a_late_click_is_harmless() {
        let base = Instant::now();
        let mut toasts = Toasts::default();
        toasts.push(Kind::Info, "a", base);
        toasts.push(Kind::Info, "b", base);
        let a = toasts.items()[0].id;
        assert!(toasts.dismiss(a));
        assert_eq!(texts(&toasts), ["b"]);
        assert!(!toasts.dismiss(a), "already gone: expired or clicked twice");
        assert_eq!(texts(&toasts), ["b"]);
        toasts.push(Kind::Info, "c", base);
        assert_ne!(toasts.items()[1].id, a, "ids are never reused");
    }
}
