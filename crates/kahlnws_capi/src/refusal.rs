// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Why a solve refused a problem, as plain data C reads without parsing text, and its message as a C string.

use core::fmt::{self, Write as _};
use core::mem::MaybeUninit;

use kahlnws::ProblemError;

use crate::halves::{Int128, UInt128};

/// Which breach a refusal names, by a number fixed for good: one of the `KAHLNWS_REFUSAL_KIND_` constants.
///
/// The numbers are part of the binary interface: none is ever changed or given to another breach. A number no
/// constant has, zero among them, names no breach; a zeroed `kahlnws_refusal` names none. Plain data: a caller may
/// hold any number.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RefusalKind(pub u32);

impl RefusalKind {
    /// The problem has more nodes or arcs than a solve counts: `node_count + 1` or `arc_count + node_count` passes
    /// `2^32 - 1`. Sets `node_count` and `arc_count`.
    pub const TOO_LARGE: Self = Self(1);
    /// An arc leaves a node the problem does not have. Sets `arc`, `node` (its tail) and `node_count`.
    pub const TAIL_OUTSIDE_NODES: Self = Self(2);
    /// An arc enters a node the problem does not have. Sets `arc`, `node` (its head) and `node_count`.
    pub const HEAD_OUTSIDE_NODES: Self = Self(3);
    /// An arc's lower bound passes its capacity. Sets `arc`, `lower_bound` and `capacity`.
    pub const LOWER_ABOVE_CAPACITY: Self = Self(4);
    /// An arc's cost lies beyond plus or minus `kahlnws_cost_bound(node_count)`. Sets `arc`, `cost` and
    /// `node_count`.
    pub const COST_BEYOND_BOUND: Self = Self(5);
    /// The magnitudes of the costs times the capacities, added up in arc order, pass `2^127 - 1`. Sets `arc`, the
    /// arc at which the sum passes.
    pub const COST_TOTAL_BEYOND_RANGE: Self = Self(6);
    /// The supplies do not add up to zero. Sets `supply_total`.
    pub const UNBALANCED: Self = Self(7);
    /// The supplies net of the lower bounds add up past `2^64 - 2` where they are positive; a node's net supply is
    /// its supply, less the lower bounds of the arcs that leave it, plus those of the arcs that enter it. Sets
    /// `net_supply_total`.
    pub const NET_SUPPLY_BEYOND_RANGE: Self = Self(8);
}

/// Why `kahlnws_solver_solve` refused a problem: the breach, by its kind, and the values that make it one.
///
/// A problem with several breaches is refused with the first in the order of the kinds' numbers, where the kinds
/// about one arc, from `KAHLNWS_REFUSAL_KIND_TAIL_OUTSIDE_NODES` to `KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE`,
/// are taken arc by arc: the lowest arc with any of them, and its first.
///
/// Plain data. Each kind sets the fields its constant names; every other field is zero. `kahlnws_refusal_message`
/// writes the same breach as text.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Refusal {
    /// Which breach: one of the `KAHLNWS_REFUSAL_KIND_` constants.
    pub kind: RefusalKind,
    /// The arc the breach is at; zero where the kind sets no arc.
    pub arc: usize,
    /// The node outside the problem's nodes, the arc's tail or its head; zero where the kind sets no node.
    pub node: usize,
    /// The problem's number of nodes; zero where the kind does not set it.
    pub node_count: usize,
    /// The problem's number of arcs; zero where the kind does not set it.
    pub arc_count: usize,
    /// The arc's lower bound, in units of flow; zero where the kind does not set it.
    pub lower_bound: u64,
    /// The arc's capacity, in units of flow; zero where the kind does not set it.
    pub capacity: u64,
    /// The arc's cost, in units of cost; zero where the kind does not set it.
    pub cost: i64,
    /// The sum of the supplies, in units of flow; zero where the kind does not set it.
    pub supply_total: Int128,
    /// The sum of the positive supplies net of the lower bounds, in units of flow; zero where the kind does not set
    /// it.
    pub net_supply_total: UInt128,
}

impl Refusal {
    /// The refusal that names `breach` with its values.
    ///
    /// # Panics
    /// Panics if `breach` is a table of another length than the tails, which no problem read from a `kahlnws_problem`
    /// has: one arc count gives every per-arc table its length.
    #[track_caller]
    pub(crate) fn of(breach: ProblemError) -> Self {
        let zero = Self::default();

        match breach {
            ProblemError::Length { .. } => {
                panic!(
                    "called `Refusal::of` with the breach \"{breach}\", which a problem of one arc count cannot have"
                )
            }
            ProblemError::TooLarge { node_count, arc_count } => {
                Self { kind: RefusalKind::TOO_LARGE, node_count, arc_count, ..zero }
            }
            ProblemError::TailOutsideNodes { arc, tail, node_count } => {
                Self { kind: RefusalKind::TAIL_OUTSIDE_NODES, arc, node: tail, node_count, ..zero }
            }
            ProblemError::HeadOutsideNodes { arc, head, node_count } => {
                Self { kind: RefusalKind::HEAD_OUTSIDE_NODES, arc, node: head, node_count, ..zero }
            }
            ProblemError::LowerAboveCapacity { arc, lower_bound, capacity } => {
                Self { kind: RefusalKind::LOWER_ABOVE_CAPACITY, arc, lower_bound, capacity, ..zero }
            }
            ProblemError::CostBeyondBound { arc, cost, node_count } => {
                Self { kind: RefusalKind::COST_BEYOND_BOUND, arc, cost, node_count, ..zero }
            }
            ProblemError::CostTotalBeyondRange { arc } => {
                Self { kind: RefusalKind::COST_TOTAL_BEYOND_RANGE, arc, ..zero }
            }
            ProblemError::Unbalanced { total } => {
                Self { kind: RefusalKind::UNBALANCED, supply_total: Int128::from(total), ..zero }
            }
            ProblemError::NetSupplyBeyondRange { total } => {
                Self { kind: RefusalKind::NET_SUPPLY_BEYOND_RANGE, net_supply_total: UInt128::from(total), ..zero }
            }
        }
    }

    /// The breach the refusal names, read from the fields its kind sets; `None` where its kind names no breach.
    pub(crate) fn breach(&self) -> Option<ProblemError> {
        let Self { kind, arc, node, node_count, arc_count, lower_bound, capacity, cost, .. } = *self;

        Some(match kind {
            RefusalKind::TOO_LARGE => ProblemError::TooLarge { node_count, arc_count },
            RefusalKind::TAIL_OUTSIDE_NODES => ProblemError::TailOutsideNodes { arc, tail: node, node_count },
            RefusalKind::HEAD_OUTSIDE_NODES => ProblemError::HeadOutsideNodes { arc, head: node, node_count },
            RefusalKind::LOWER_ABOVE_CAPACITY => ProblemError::LowerAboveCapacity { arc, lower_bound, capacity },
            RefusalKind::COST_BEYOND_BOUND => ProblemError::CostBeyondBound { arc, cost, node_count },
            RefusalKind::COST_TOTAL_BEYOND_RANGE => ProblemError::CostTotalBeyondRange { arc },
            RefusalKind::UNBALANCED => ProblemError::Unbalanced { total: i128::from(self.supply_total) },
            RefusalKind::NET_SUPPLY_BEYOND_RANGE => {
                ProblemError::NetSupplyBeyondRange { total: u128::from(self.net_supply_total) }
            }
            _ => return None,
        })
    }

    /// Writes the message of the breach to `buffer` as a C string, and answers its length in bytes, the terminating
    /// NUL not counted; where the refusal names no breach, the message is empty and the length 0.
    ///
    /// The message is the one the library gives the breach, in UTF-8. Where `buffer` holds more bytes than the
    /// message, it is written whole and NUL-terminated; otherwise as much of it as ends on a character and leaves a
    /// byte for the NUL. An empty `buffer` receives nothing; any other ends in a NUL. What `buffer` held before is
    /// never read. Nothing is allocated, so no refused memory can end the process.
    pub(crate) fn write_message(&self, buffer: &mut [MaybeUninit<u8>]) -> usize {
        let mut text = TerminatedText::new(buffer);
        if let Some(breach) = self.breach() {
            write!(text, "{breach}").expect("a terminated text should take every piece written to it");
        }

        text.terminate()
    }
}

/// A text written into a C string as it is formatted: as much of it as fits before the string's last byte, cut only
/// at a character, and the length of the whole.
///
/// # Invariant
/// `written <= end`, and `end` is below the length of `buffer` where `buffer` is not empty and zero where it is:
/// `buffer[..written]` holds the text kept so far, and `end` is where the kept text must stop, which leaves the byte
/// for the NUL.
#[derive(Debug)]
struct TerminatedText<'b> {
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
    fn new(buffer: &'b mut [MaybeUninit<u8>]) -> Self {
        let end = buffer.len().saturating_sub(1);

        Self { buffer, written: 0, end, length: 0 }
    }

    /// Ends the text kept with a NUL, where the buffer has a byte, and answers the length of the whole text.
    fn terminate(self) -> usize {
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
    use core::mem::{offset_of, size_of};

    use kahlnws::{Table, cost_bound};

    use super::*;

    /// Every breach a problem read from C can have, with values that differ from field to field, and its message.
    fn breaches() -> [(ProblemError, String); 8] {
        let bound = cost_bound(4);

        [
            (
                ProblemError::TooLarge { node_count: 4_294_967_295, arc_count: 7 },
                "the node count 4294967295 and the arc count 7 are too large: nodes + 1 and arcs + nodes are each \
                 limited to 4294967295"
                    .to_owned(),
            ),
            (
                ProblemError::TailOutsideNodes { arc: 2, tail: 9, node_count: 4 },
                "arc 2 leaves node 9, not below the node count 4".to_owned(),
            ),
            (
                ProblemError::HeadOutsideNodes { arc: 3, head: 8, node_count: 5 },
                "arc 3 enters node 8, not below the node count 5".to_owned(),
            ),
            (
                ProblemError::LowerAboveCapacity { arc: 1, lower_bound: 6, capacity: 5 },
                "arc 1 has lower bound 6 above its capacity 5".to_owned(),
            ),
            (
                ProblemError::CostBeyondBound { arc: 1, cost: -bound - 1, node_count: 4 },
                format!("arc 1 has cost {} beyond the bound ±{bound} for the node count 4", -bound - 1),
            ),
            (
                ProblemError::CostTotalBeyondRange { arc: 2 },
                format!("the costs times the capacities, in magnitude, sum past {} at arc 2", i128::MAX),
            ),
            (ProblemError::Unbalanced { total: i128::MIN }, format!("the supplies add up to {}, not zero", i128::MIN)),
            (
                ProblemError::NetSupplyBeyondRange { total: u128::MAX },
                format!(
                    "the positive supplies net of the lower bounds add up to {}, past 18446744073709551614",
                    u128::MAX
                ),
            ),
        ]
    }

    #[test]
    fn test_each_breach_by_hand() {
        let zero = Refusal::default();
        let bound = cost_bound(4);
        let expected = [
            Refusal { kind: RefusalKind::TOO_LARGE, node_count: 4_294_967_295, arc_count: 7, ..zero },
            Refusal { kind: RefusalKind::TAIL_OUTSIDE_NODES, arc: 2, node: 9, node_count: 4, ..zero },
            Refusal { kind: RefusalKind::HEAD_OUTSIDE_NODES, arc: 3, node: 8, node_count: 5, ..zero },
            Refusal { kind: RefusalKind::LOWER_ABOVE_CAPACITY, arc: 1, lower_bound: 6, capacity: 5, ..zero },
            Refusal { kind: RefusalKind::COST_BEYOND_BOUND, arc: 1, cost: -bound - 1, node_count: 4, ..zero },
            Refusal { kind: RefusalKind::COST_TOTAL_BEYOND_RANGE, arc: 2, ..zero },
            Refusal { kind: RefusalKind::UNBALANCED, supply_total: Int128 { low: 0, high: i64::MIN }, ..zero },
            Refusal {
                kind: RefusalKind::NET_SUPPLY_BEYOND_RANGE,
                net_supply_total: UInt128 { low: u64::MAX, high: u64::MAX },
                ..zero
            },
        ];

        for ((breach, _), refusal) in breaches().into_iter().zip(expected) {
            assert_eq!(Refusal::of(breach), refusal, "{breach:?}");
        }
    }

    #[test]
    fn test_each_breach_is_read_back_as_itself() {
        for (breach, _) in breaches() {
            assert_eq!(Refusal::of(breach).breach(), Some(breach));
        }
    }

    #[test]
    fn test_the_kinds_have_the_numbers_of_the_binary_interface() {
        let kinds = breaches().map(|(breach, _)| Refusal::of(breach).kind);

        assert_eq!(kinds, [1, 2, 3, 4, 5, 6, 7, 8].map(RefusalKind));
    }

    /// A buffer of `capacity` bytes, every one written as 7, so that a byte a writer leaves is seen.
    fn marked(capacity: usize) -> Vec<MaybeUninit<u8>> {
        vec![MaybeUninit::new(7); capacity]
    }

    /// The bytes of `buffer`, every one of which was written.
    fn bytes_of(buffer: &[MaybeUninit<u8>]) -> Vec<u8> {
        buffer
            .iter()
            .map(|byte| {
                // SAFETY: every buffer of these tests starts written by `marked`, and a writer only writes bytes.
                unsafe { byte.assume_init() }
            })
            .collect()
    }

    /// The bytes `refusal` writes as its message into a buffer of `capacity` bytes that start at 7, and the length
    /// it answers.
    fn message_of(refusal: &Refusal, capacity: usize) -> (Vec<u8>, usize) {
        let mut buffer = marked(capacity);
        let length = refusal.write_message(&mut buffer);

        (bytes_of(&buffer), length)
    }

    #[test]
    fn test_a_kind_no_constant_has_names_no_breach_and_writes_an_empty_message() {
        for number in [0, 9, 1000, u32::MAX] {
            let refusal = Refusal { kind: RefusalKind(number), arc: 3, ..Refusal::default() };
            let mut terminated = vec![0];
            terminated.extend([7; 15]);

            assert_eq!(refusal.breach(), None, "{number}");
            assert_eq!(message_of(&refusal, 16), (terminated, 0), "{number}: only the NUL is written");
            assert_eq!(message_of(&refusal, 0), (vec![], 0), "{number}: nothing to terminate");
        }
    }

    #[test]
    #[should_panic(
        expected = "called `Refusal::of` with the breach \"the heads have length 2 where the tails have length 3\", \
                    which a problem of one arc count cannot have"
    )]
    fn test_a_table_of_another_length_has_no_refusal() {
        let _ = Refusal::of(ProblemError::Length { table: Table::Heads, length: 2, expected: 3 });
    }

    #[test]
    fn test_the_message_is_the_breach_text_whole_where_it_fits() {
        for (breach, message) in breaches() {
            let (buffer, length) = message_of(&Refusal::of(breach), 200);

            assert_eq!(length, message.len(), "{message}");
            assert_eq!(&buffer[..length], message.as_bytes());
            assert_eq!(buffer[length], 0, "{message}");
        }
    }

    #[test]
    fn test_a_message_just_fits_in_its_length_and_one() {
        let breach = ProblemError::CostTotalBeyondRange { arc: 2 };
        let message = breach.to_string();
        let (buffer, length) = message_of(&Refusal::of(breach), message.len() + 1);

        assert_eq!(length, message.len());
        assert_eq!(buffer[..message.len()], *message.as_bytes());
        assert_eq!(buffer.last(), Some(&0));
    }

    /// What a terminated text in a buffer of `capacity` bytes, each starting at 7, holds after `pieces` are written to
    /// it, and the length it answers.
    fn terminated(pieces: &[&str], capacity: usize) -> (Vec<u8>, usize) {
        let mut buffer = marked(capacity);
        let mut text = TerminatedText::new(&mut buffer);
        for piece in pieces {
            text.write_str(piece).expect("a terminated text should take every piece");
        }
        let length = text.terminate();

        (bytes_of(&buffer), length)
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

        for (capacity, written) in cases {
            assert_eq!(terminated(&["a±b"], capacity), (written.to_vec(), 4), "capacity {capacity}");
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

    #[test]
    fn test_a_message_too_long_for_its_buffer_is_cut_and_still_answers_its_whole_length() {
        let breach = ProblemError::CostBeyondBound { arc: 1, cost: -cost_bound(4) - 1, node_count: 4 };
        let message = breach.to_string();
        let (buffer, length) = message_of(&Refusal::of(breach), 12);

        assert_eq!(length, message.len());
        assert_eq!(&buffer[..11], &message.as_bytes()[..11]);
        assert_eq!(buffer[11], 0);
        assert_eq!(message_of(&Refusal::of(breach), 0), (vec![], message.len()), "no buffer: only the length");
    }

    // The offsets of a 64-bit target, where every field but the kind is 8 or 16 bytes and aligned to 8.
    #[cfg(target_pointer_width = "64")]
    #[test]
    fn test_the_layout_c_reads_by_hand() {
        let offsets = [
            offset_of!(Refusal, kind),
            offset_of!(Refusal, arc),
            offset_of!(Refusal, node),
            offset_of!(Refusal, node_count),
            offset_of!(Refusal, arc_count),
            offset_of!(Refusal, lower_bound),
            offset_of!(Refusal, capacity),
            offset_of!(Refusal, cost),
            offset_of!(Refusal, supply_total),
            offset_of!(Refusal, net_supply_total),
        ];

        assert_eq!(offsets, [0, 8, 16, 24, 32, 40, 48, 56, 64, 80]);
        assert_eq!(size_of::<Refusal>(), 96);
    }
}
