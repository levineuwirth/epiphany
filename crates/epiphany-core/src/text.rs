//! Score text (schema major 5): the text a lyric syllable, a staff or tempo
//! mark, a rehearsal mark, a written dynamic or a text line carries.
//!
//! Canonical text fields are UTF-8 in Unicode NFC
//! (`req:determinism:unicode-canonicalization`). The whole-score codec keeps a
//! plain `String` byte for byte, NFC or not, so a score's other strings can
//! hold text that is not NFC; [`Text`] cannot. Its constructor folds to NFC,
//! and its decoder refuses a string that is not already NFC, so an accepted
//! byte string is canonical. Its text projection parses by comparison, never
//! by folding (`req:textproj:strict-parse`). It is plain text: style follows
//! the kind of mark that carries it, and shaping is the layout's.

use unicode_normalization::UnicodeNormalization;

/// Plain score text, held in Unicode NFC.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Text(String);

impl Text {
    /// The text, folded to NFC.
    pub fn new(text: impl Into<String>) -> Self {
        let text: String = text.into();
        if unicode_normalization::is_nfc(&text) {
            Text(text)
        } else {
            Text(text.nfc().collect())
        }
    }

    /// The NFC text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `text` as held, if it is already NFC; `None` otherwise. Decoders call
    /// this, so a byte string that is not NFC is refused, never folded.
    pub fn from_nfc(text: String) -> Option<Self> {
        unicode_normalization::is_nfc(&text).then_some(Text(text))
    }
}

impl From<&str> for Text {
    fn from(text: &str) -> Self {
        Text::new(text)
    }
}

#[cfg(test)]
mod tests {
    use super::Text;

    #[test]
    fn text_is_held_in_nfc_and_a_decoder_refuses_any_other_form() {
        // "é" written as "e" and a combining acute.
        let decomposed = "e\u{301}";
        let composed = "\u{e9}";
        assert_eq!(Text::new(decomposed).as_str(), composed);
        assert_eq!(Text::new(decomposed), Text::new(composed));
        assert!(Text::from_nfc(decomposed.to_owned()).is_none());
        assert_eq!(
            Text::from_nfc(composed.to_owned()).expect("NFC").as_str(),
            composed
        );
    }
}
