//! The note as bytes. Everything saved comes from this rope; nothing here
//! normalizes line endings, spaces or anything else, so an untouched file
//! writes back byte for byte.
//!
//! `ropey` is built without `unicode_lines`/`cr_lines`, so only `\n` starts a
//! new line. A `\r` before it stays in the text and `line_text` hides it from
//! display, which keeps CRLF files exact.

use ropey::Rope;
use std::ops::Range;

pub struct Doc {
    pub rope: Rope,
}

impl Doc {
    /// Fails on bytes that are not UTF-8: refusing is the only exact option
    /// for a text rope, so a caller shows an error rather than guess.
    pub fn from_bytes(bytes: &[u8]) -> Result<Doc, std::str::Utf8Error> {
        Ok(Doc {
            rope: Rope::from_str(std::str::from_utf8(bytes)?),
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.rope.len_bytes());
        for chunk in self.rope.chunks() {
            out.extend_from_slice(chunk.as_bytes());
        }
        out
    }

    pub fn len(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line_of(&self, byte: usize) -> usize {
        self.rope.byte_to_line(byte.min(self.len()))
    }

    pub fn line_start(&self, line: usize) -> usize {
        self.rope.line_to_byte(line)
    }

    /// The line without its `\n` or `\r\n`.
    pub fn line_text(&self, line: usize) -> String {
        let mut text: String = self.rope.line(line).into();
        if text.ends_with('\n') {
            text.pop();
            if text.ends_with('\r') {
                text.pop();
            }
        }
        text
    }

    pub fn line_end(&self, line: usize) -> usize {
        self.line_start(line) + self.line_text(line).len()
    }

    pub fn slice(&self, range: Range<usize>) -> String {
        self.rope.byte_slice(range).to_string()
    }

    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        let start = self.rope.byte_to_char(range.start);
        let end = self.rope.byte_to_char(range.end);
        if start != end {
            self.rope.remove(start..end);
        }
        if !text.is_empty() {
            self.rope.insert(start, text);
        }
    }

    pub fn utf16_of(&self, byte: usize) -> usize {
        self.rope
            .char_to_utf16_cu(self.rope.byte_to_char(byte.min(self.len())))
    }

    pub fn byte_of_utf16(&self, utf16: usize) -> usize {
        let max = self.rope.len_utf16_cu();
        self.rope
            .char_to_byte(self.rope.utf16_cu_to_char(utf16.min(max)))
    }

    pub fn len_utf16(&self) -> usize {
        self.rope.len_utf16_cu()
    }

    /// The nearest char boundary at or before `byte`.
    pub fn floor_boundary(&self, byte: usize) -> usize {
        let byte = byte.min(self.len());
        self.rope.char_to_byte(self.rope.byte_to_char(byte))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRICKY: &[&[u8]] = &[
        b"",
        b"no newline at end",
        b"crlf\r\nlines\r\nmixed\nlone\rcr\n",
        "\u{feff}bom first\n".as_bytes(),
        "nbsp\u{a0} ls\u{2028} ps\u{2029} nel\u{85} vt\u{b} ff\u{c}\n".as_bytes(),
        "---\ntitle:   spaced  \ntags: [a,  b]\n---\n\n[[wiki|alias]] > [!NOTE]-\n\t\ttabs   \n\n\n".as_bytes(),
        "e\u{301} \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} 中文\u{3000}全角\n".as_bytes(),
    ];

    #[test]
    fn untouched_is_byte_exact() {
        for bytes in TRICKY {
            let doc = Doc::from_bytes(bytes).unwrap();
            assert_eq!(doc.to_bytes(), *bytes);
        }
    }

    #[test]
    fn fixtures_are_byte_exact() {
        for name in ["10k", "50k", "200k", "300k", "500k", "700k", "1m", "visual"] {
            let path = format!("{}/fixtures/{name}.md", env!("CARGO_MANIFEST_DIR"));
            let bytes = std::fs::read(path).unwrap();
            assert_eq!(Doc::from_bytes(&bytes).unwrap().to_bytes(), bytes);
        }
    }

    #[test]
    fn edit_and_inverse_is_byte_exact() {
        for bytes in TRICKY {
            let mut doc = Doc::from_bytes(bytes).unwrap();
            let at = doc.floor_boundary(bytes.len() / 2);
            doc.replace(at..at, "插入\n");
            doc.replace(at..at + "插入\n".len(), "");
            assert_eq!(doc.to_bytes(), *bytes);
        }
    }

    #[test]
    fn invalid_utf8_is_refused() {
        assert!(Doc::from_bytes(&[0x66, 0xff, 0x66]).is_err());
    }

    #[test]
    fn crlf_hides_only_from_display() {
        let doc = Doc::from_bytes(b"a\r\nb").unwrap();
        assert_eq!(doc.line_count(), 2);
        assert_eq!(doc.line_text(0), "a");
        assert_eq!(doc.line_start(1), 3);
    }
}
