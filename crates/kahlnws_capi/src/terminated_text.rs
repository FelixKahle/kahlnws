// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A text formatted into a C string the caller holds: cut where it is full, always ended by a NUL, and the length of
//! the whole answered.

use core::fmt;
use core::mem::MaybeUninit;

/// A text written into a C string as it is formatted: as much of it as fits before the string's last byte, cut only
/// at a character, and the length of the whole.
///
/// # Invariant
/// `written <= end`, and `end` is below the length of `buffer` where `buffer` is not empty and zero where it is:
/// `buffer[..written]` holds the text kept so far, and `end` is where the kept text must stop, which leaves the byte
/// for the NUL.
#[derive(Debug)]
pub(crate) struct TerminatedText<'b> {
    /// The C string's bytes, written but never read.
    buffer: &'b mut [MaybeUninit<u8>],
    /// The bytes of the text kept, from the start of `buffer`.
    written: usize,
    /// The most bytes the kept text may reach: one less than the buffer's, or where a piece was cut short.
    end: usize,
    /// The length of the whole text written so far, kept or not, in bytes.
    length: usize,
}

impl<'b> TerminatedText<'b> {
    /// An empty text in `buffer`.
    pub(crate) fn new(buffer: &'b mut [MaybeUninit<u8>]) -> Self {
        let end = buffer.len().saturating_sub(1);

        Self { buffer, written: 0, end, length: 0 }
    }

    /// Ends the text kept with a NUL, where the buffer has a byte, and answers the length of the whole text.
    pub(crate) fn terminate(self) -> usize {
        if let Some(terminator) = self.buffer.get_mut(self.written) {
            terminator.write(0);
        }

        self.length
    }
}

impl fmt::Write for TerminatedText<'_> {
    fn write_str(&mut self, piece: &str) -> fmt::Result {
        self.length = self.length.strict_add(piece.len());
        let kept = piece.floor_char_boundary(self.end - self.written);

        self.buffer[self.written..self.written + kept].write_copy_of_slice(&piece.as_bytes()[..kept]);
        self.written += kept;

        // A piece cut short ends the text kept: a later piece that fits would not follow on from it.
        if kept < piece.len() {
            self.end = self.written;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::fmt::Write as _;

    use super::*;
    use crate::boundary::written_entries;

    /// What a terminated text in a buffer of `capacity` bytes, each starting at 7, holds after `pieces` are written to
    /// it, and the length it answers.
    fn terminated(pieces: &[&str], capacity: usize) -> (Vec<u8>, usize) {
        let mut buffer = vec![MaybeUninit::new(7); capacity];
        let mut text = TerminatedText::new(&mut buffer);
        for piece in pieces {
            text.write_str(piece).expect("a terminated text should take every piece");
        }
        let length = text.terminate();

        // SAFETY: the test made every byte with `MaybeUninit::new`, and the text writes a byte only whole.
        (unsafe { written_entries(&buffer) }, length)
    }

    #[test]
    fn test_a_terminated_text_cuts_at_a_character_and_keeps_a_byte_for_the_nul_by_hand() {
        // "a±b": `±` is the two bytes 0xC2 0xB1, so a cut after one byte of it falls back to before it.
        let cases: [(usize, &[u8]); 7] = [
            (0, b""),
            (1, b"\0"),
            (2, b"a\0"),
            (3, b"a\0\x07"),
            (4, "a±\0".as_bytes()),
            (5, "a±b\0".as_bytes()),
            (6, "a±b\0\x07".as_bytes()),
        ];

        for (capacity, kept) in cases {
            assert_eq!(terminated(&["a±b"], capacity), (kept.to_vec(), 4), "capacity {capacity}");
        }
    }

    #[test]
    fn test_a_terminated_text_of_pieces_keeps_nothing_after_a_piece_cut_short_by_hand() {
        // "ab" fits a buffer of 4; "±" does not fit the one byte left before the NUL, and "c", which would, must not
        // follow it.
        assert_eq!(terminated(&["ab", "±", "c"], 4), (b"ab\0\x07".to_vec(), 5));
        assert_eq!(terminated(&["ab", "±", "c"], 5), ("ab±\0".as_bytes().to_vec(), 5));
        assert_eq!(terminated(&["ab", "±", "c"], 6), ("ab±c\0".as_bytes().to_vec(), 5));
        assert_eq!(terminated(&["ab", "", "c"], 3), (b"ab\0".to_vec(), 3), "an empty piece cuts nothing");
    }
}
