//! Secret input, and the one secret this crate hands back (docs/PLAN.md
//! section 4.2 and 4.3).
//!
//! Two types, and neither has `Clone`, `Display` or a `Debug` that prints
//! what it holds:
//!
//! - [`SecretText`]: what a person typed into a password-mode field (a
//!   password, a recovery phrase, the confirmation words). The interface
//!   moves the field's text into one with [`SecretText::take`], which leaves
//!   the field empty, and hands it to the worker in a command. The worker
//!   gives it to the library and drops it, and dropping it zeroes the bytes.
//! - [`PhraseForDisplay`]: the recovery phrase a new store was made from, for
//!   the one screen that shows it (section 4.3). It is the only secret an
//!   event carries, because showing the phrase once is the requirement.
//!
//! # What this does not reach
//!
//! The toolkit's own copies. A text field that keeps its value in a `String`
//! the interface owns is emptied by `take`, without a copy, but a widget may
//! keep glyph or undo buffers of its own, and an input method keeps what it
//! keeps. Phase 4's platform glue is where the input method is asked not to
//! learn a password field (`docs/spikes/P1-REPORT.md`, "No secure keyboard").

use core::fmt;

use zeroize::Zeroizing;

/// Text a person typed that must not outlive its use.
///
/// Built by [`SecretText::take`], which moves a `String`'s heap buffer in
/// without copying it, so the only copy is the one that zeroizes.
pub struct SecretText(Zeroizing<String>);

impl SecretText {
    /// Move `field`'s text into a secret and leave `field` empty.
    ///
    /// The interface calls this on the `String` behind a password-mode field
    /// when the person submits it. The buffer moves; nothing is copied, so no
    /// unzeroized copy is left behind in the field.
    pub fn take(field: &mut String) -> SecretText {
        SecretText(Zeroizing::new(core::mem::take(field)))
    }

    /// The text, for the library call it is meant for.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    /// Whether nothing was typed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many characters (Unicode scalar values) were typed: the unit the
    /// library's password floor counts in (`cli::create::MIN_PASSWORD_LEN`).
    #[must_use]
    pub fn chars(&self) -> usize {
        self.0.chars().count()
    }
}

impl From<Zeroizing<String>> for SecretText {
    fn from(text: Zeroizing<String>) -> SecretText {
        SecretText(text)
    }
}

impl fmt::Debug for SecretText {
    /// Says that it is a secret and nothing else: no text and no length.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretText(..)")
    }
}

/// A new store's recovery phrase, for the one screen that shows it.
///
/// Shown once, behind the confirmation (docs/PLAN.md section 4.3), and never
/// copied to the clipboard: there is no method that returns the whole
/// phrase as one string, only its words for laying out on the screen.
pub struct PhraseForDisplay {
    words: Zeroizing<String>,
}

impl PhraseForDisplay {
    pub(crate) fn new(words: &str) -> PhraseForDisplay {
        PhraseForDisplay {
            words: Zeroizing::new(words.to_owned()),
        }
    }

    /// The BIP39 all-`abandon` 24-word test vector, for drawing the phrase
    /// screen with sample data (the screenshots, docs/DECISIONS.md D27 item
    /// 1). It is a published test vector, so it is no one's phrase; a
    /// phrase a person is shown comes only from the worker.
    #[must_use]
    pub fn example() -> PhraseForDisplay {
        PhraseForDisplay::new(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon \
             abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon \
             abandon abandon abandon art",
        )
    }

    /// The words, in order, for laying out one per cell.
    pub fn words(&self) -> impl Iterator<Item = &str> {
        self.words.split_whitespace()
    }

    /// How many words the phrase has (24 for a phrase this wallet makes).
    #[must_use]
    pub fn len(&self) -> usize {
        self.words().count()
    }

    /// Whether the phrase has no words; never true for one the worker made.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl fmt::Debug for PhraseForDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PhraseForDisplay(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_moves_the_buffer_and_empties_the_field() {
        let mut field = String::with_capacity(64);
        field.push_str("correct horse battery staple");
        let before = field.as_ptr();
        let secret = SecretText::take(&mut field);
        assert!(field.is_empty());
        assert_eq!(field.capacity(), 0, "the field keeps no allocation");
        assert_eq!(secret.expose().as_ptr(), before, "moved, not copied");
        assert_eq!(secret.expose(), "correct horse battery staple");
    }

    #[test]
    fn debug_shows_neither_text_nor_length() {
        let mut field = String::from("hunter2hunter2");
        let secret = SecretText::take(&mut field);
        let shown = format!("{secret:?}");
        assert_eq!(shown, "SecretText(..)");
        let phrase = PhraseForDisplay::new("abandon ability able");
        assert_eq!(format!("{phrase:?}"), "PhraseForDisplay(..)");
    }

    #[test]
    fn chars_counts_scalar_values_not_bytes() {
        let mut field = String::from("ñüß");
        assert_eq!(SecretText::take(&mut field).chars(), 3);
    }

    #[test]
    fn phrase_words_in_order() {
        let phrase = PhraseForDisplay::new("  abandon  ability able ");
        assert_eq!(
            phrase.words().collect::<Vec<_>>(),
            ["abandon", "ability", "able"]
        );
        assert_eq!(phrase.len(), 3);
    }
}
