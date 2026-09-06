use crossterm::cursor::{MoveRight, SetCursorStyle::BlinkingBar};
use std::io::{Result, Stdout, Write, stdout};

pub struct Output {
    pub text: String,
    pub length: usize,
    pub pos: usize,
    pub start: usize,
    pub replace: bool,
    out: Stdout,
}

impl Write for Output {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.out.write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        self.out.flush()
    }
}

/// Constructor
impl Output {
    pub fn new() -> Self {
        let mut out = stdout();
        let _ = write!(out, "{BlinkingBar}");

        Self {
            text: String::new(),
            length: 0,
            pos: 0,
            start: 0,
            replace: false,
            out,
        }
    }
}
/// Utilities
impl Output {
    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }
    pub const fn is_at_start(&self) -> bool {
        self.pos == 0
    }
    pub const fn is_at_end(&self) -> bool {
        self.pos == self.length
    }
    /// Byte offset of the `pos`-th character, and how many bytes it takes.
    ///
    /// A position past the end answers the end of the text with nothing to
    /// skip, which is what every caller wants there: they use the pair as an
    /// insertion point, so `(len, 0)` appends. It used to panic instead, and
    /// an empty input line was enough to reach it -- `replace_push` asks for
    /// `pos.saturating_sub(1)`, which is `0` when the line is empty, and the
    /// zeroth character of an empty string does not exist.
    pub fn char_pos(&self, pos: usize) -> (usize, usize) {
        let Some((offset, char)) = self.text.char_indices().nth(pos) else {
            return (self.text.len(), 0);
        };
        (offset, char.len_utf8())
    }
    pub fn visible_input_width() -> usize {
        super::terminal_width().saturating_sub(4).max(1)
    }
    pub fn cursor_to(&mut self, to: usize) -> Result<()> {
        if to > 0 {
            write!(self.out, "\r{}", MoveRight(to as u16))
        } else {
            write!(self.out, "\r")
        }
    }
    pub fn cursor_to_relative(&mut self, to: usize) -> Result<()> {
        let visible_pos = to
            .saturating_sub(self.start)
            .min(Self::visible_input_width());
        write!(self.out, "\r{}", MoveRight((visible_pos + 2) as u16))
    }
}
