//! The clipboard of the compositor (F030).
//!
//! A Wayland client sets the clipboard with a data source that offers mime
//! types. The compositor reads the text or the PNG image of a new selection
//! and gives it to the shell as [`crate::Event::Clipboard`]. The shell gives
//! the clipboard of the client computer as [`crate::Command::SetClipboard`],
//! and the compositor then owns the selection: it writes the content to each
//! client that pastes.

/// The largest clipboard content in bytes. The compositor does not read a larger selection to its end.
pub const MAX_CLIP: usize = 16 << 20;

/// The mime types of text, in the order of preference. `STRING` and `TEXT` are the names of X11 apps.
pub(crate) const TEXT_TYPES: [&str; 5] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
    "TEXT",
];

/// The mime type of an image.
pub(crate) const PNG_TYPE: &str = "image/png";

/// The content of the clipboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Clip {
    /// Text.
    Text(String),
    /// A PNG image.
    Png(Vec<u8>),
}

impl Clip {
    /// Returns the mime types that the compositor offers to the clients for this content.
    #[must_use]
    pub fn mime_types(&self) -> Vec<String> {
        match self {
            Self::Text(_) => TEXT_TYPES.iter().map(|&t| t.to_owned()).collect(),
            Self::Png(_) => vec![PNG_TYPE.to_owned()],
        }
    }

    /// Returns the bytes of the content: UTF-8 for text.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Png(png) => png,
        }
    }
}

/// The kind of content that the compositor reads of a selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClipKind {
    Text,
    Png,
}

/// Selects the mime type to read of a selection: text first, then a PNG image.
/// `None` if the selection has neither, for example a list of files only.
pub(crate) fn pick(offered: &[String]) -> Option<(String, ClipKind)> {
    // The request must name the type as the source spells it.
    TEXT_TYPES
        .iter()
        .find_map(|t| offered.iter().find(|o| o.eq_ignore_ascii_case(t)))
        .map(|o| (o.clone(), ClipKind::Text))
        .or_else(|| {
            offered
                .iter()
                .any(|o| o == PNG_TYPE)
                .then(|| (PNG_TYPE.to_owned(), ClipKind::Png))
        })
}

/// Makes the content from the bytes that a client wrote. Text that is not valid UTF-8 gets replacement characters.
pub(crate) fn clip(kind: ClipKind, bytes: Vec<u8>) -> Clip {
    match kind {
        ClipKind::Text => Clip::Text(
            String::from_utf8(bytes)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()),
        ),
        ClipKind::Png => Clip::Png(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(list: &[&str]) -> Vec<String> {
        list.iter().map(|&t| t.to_owned()).collect()
    }

    #[test]
    fn text_goes_before_an_image() {
        assert_eq!(
            pick(&types(&["image/png", "text/plain", "UTF8_STRING"])),
            Some(("UTF8_STRING".to_owned(), ClipKind::Text))
        );
        assert_eq!(
            pick(&types(&["image/png", "image/jpeg"])),
            Some(("image/png".to_owned(), ClipKind::Png))
        );
        assert_eq!(pick(&types(&["text/uri-list"])), None);
    }

    #[test]
    fn the_utf8_type_matches_with_any_case() {
        assert_eq!(
            pick(&types(&["text/plain;charset=UTF-8"])),
            Some(("text/plain;charset=UTF-8".to_owned(), ClipKind::Text))
        );
    }

    #[test]
    fn text_that_is_not_utf8_keeps_its_valid_part() {
        assert_eq!(
            clip(ClipKind::Text, b"ab\xffc".to_vec()),
            Clip::Text("ab\u{fffd}c".to_owned())
        );
    }
}
