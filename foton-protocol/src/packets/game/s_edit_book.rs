//! Serverbound packet sent when a player writes in, or signs, a book and quill.

use std::io::{Cursor, Error, Result};

use foton_macros::ServerPacket;
use foton_utils::codec::VarInt;
use foton_utils::serial::{ReadFrom, prefixed_read::read_utf};

/// Vanilla parity: the `stringUtf8(1024)` bound of `ServerboundEditBookPacket`.
pub const MAX_PAGE_LENGTH: usize = 1024;
/// Vanilla parity: the `list(100)` bound of `ServerboundEditBookPacket`.
pub const MAX_PAGES: usize = 100;
/// Vanilla parity: the `stringUtf8(32)` bound of `ServerboundEditBookPacket`.
pub const MAX_TITLE_LENGTH: usize = 32;

/// Sent by the client when a book and quill is written to or signed.
///
/// A present `title` means the player pressed "Sign"; an absent one means they
/// only saved the pages.
#[derive(ServerPacket, Clone, Debug)]
pub struct SEditBook {
    /// The player-inventory slot holding the book.
    pub slot: i32,
    /// The page text, in order.
    pub pages: Vec<String>,
    /// The title, present only when signing.
    pub title: Option<String>,
}

impl ReadFrom for SEditBook {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let slot = VarInt::read(data)?.0;

        let page_count = VarInt::read(data)?.0;
        let page_count =
            usize::try_from(page_count).map_err(|_| Error::other("Negative book page count"))?;
        if page_count > MAX_PAGES {
            return Err(Error::other(format!(
                "{page_count} pages exceeded max size of {MAX_PAGES}"
            )));
        }
        let mut pages = Vec::with_capacity(page_count);
        for _ in 0..page_count {
            pages.push(read_utf(data, MAX_PAGE_LENGTH)?);
        }

        let title = if bool::read(data)? {
            Some(read_utf(data, MAX_TITLE_LENGTH)?)
        } else {
            None
        };

        Ok(Self { slot, pages, title })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use foton_utils::codec::VarInt;
    use foton_utils::serial::{ReadFrom as _, WriteTo as _};

    use super::{MAX_PAGES, SEditBook};

    fn encoded_string(text: &str) -> Vec<u8> {
        let mut encoded = Vec::new();
        VarInt(text.len() as i32)
            .write(&mut encoded)
            .expect("test string length should encode");
        encoded.extend_from_slice(text.as_bytes());
        encoded
    }

    fn packet(slot: i32, pages: &[&str], title: Option<&str>) -> Vec<u8> {
        let mut encoded = Vec::new();
        VarInt(slot).write(&mut encoded).expect("slot encodes");
        VarInt(pages.len() as i32)
            .write(&mut encoded)
            .expect("page count encodes");
        for page in pages {
            encoded.extend_from_slice(&encoded_string(page));
        }
        match title {
            Some(title) => {
                encoded.push(1);
                encoded.extend_from_slice(&encoded_string(title));
            }
            None => encoded.push(0),
        }
        encoded
    }

    #[test]
    fn an_unsigned_edit_carries_pages_and_no_title() {
        let bytes = packet(3, &["first", "second"], None);
        let decoded = SEditBook::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");

        assert_eq!(decoded.slot, 3);
        assert_eq!(decoded.pages, vec!["first".to_owned(), "second".to_owned()]);
        assert_eq!(decoded.title, None);
    }

    #[test]
    fn signing_carries_the_title() {
        let bytes = packet(0, &["only page"], Some("My Book"));
        let decoded = SEditBook::read(&mut Cursor::new(bytes.as_slice())).expect("packet parses");

        assert_eq!(decoded.title.as_deref(), Some("My Book"));
    }

    #[test]
    fn more_pages_than_vanilla_allows_is_rejected() {
        let pages = vec!["a"; MAX_PAGES + 1];
        let bytes = packet(0, &pages, None);

        assert!(SEditBook::read(&mut Cursor::new(bytes.as_slice())).is_err());
    }

    #[test]
    fn a_page_longer_than_vanilla_allows_is_rejected() {
        let long = "a".repeat(1025);
        let bytes = packet(0, &[long.as_str()], None);

        assert!(SEditBook::read(&mut Cursor::new(bytes.as_slice())).is_err());
    }
}
