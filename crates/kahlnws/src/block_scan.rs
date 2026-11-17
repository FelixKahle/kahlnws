// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The scan of slots in blocks that both searches for the entering arc share: the size of a block, `⌊√m⌋` of the
//! `m` slots, and the scan that prices rows a block at a time, keeping the most violating arc it priced.

use crate::slot_order::Slot;

/// The state of one search, shared by its scan of the two stretches of slots it wraps around; a search over marked
/// blocks scans each block it prices by one of its own.
///
/// # Invariant
/// `entering` is the slot of the least violation priced so far, the first priced among equals, and `least` that
/// violation, where it is below zero; while no violation below zero was priced, `entering` is `None` and `least` zero.
/// `left_in_block` is at least one, and at most `block`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BlockScan {
    /// The least violation priced, in units of cost; zero while none below zero was.
    least: i64,
    /// The slot of the arc of that violation; `None` while no arc violated.
    entering: Option<Slot>,
    /// The rows left to price in the current block.
    left_in_block: usize,
    /// The rows a block prices.
    block: usize,
}

/// The rows of a block of slots among `slot_count`: `⌊√m⌋`, at least one, the size both searches price blocks in.
pub(crate) fn block_of(slot_count: usize) -> usize {
    // The block of `⌊√m⌋` rows is the established rule of block search, not a number fitted here. What it trades is the
    // rows a search prices against the choice it makes: a block of `b` rows costs a search at least `b` rows and enters
    // the best of the at least `b` candidates it saw; a block of one is first eligible, a block of `m` best eligible,
    // and `√m` lies between the two at an equal ratio from each. The floor of one gives a problem of fewer than four
    // arcs blocks of one row, and a problem of no arc a block that is never entered.
    slot_count.isqrt().max(1)
}

impl BlockScan {
    /// A scan of blocks of `block` rows, at least one, at the start of its first block, with nothing priced yet.
    pub(crate) fn new(block: usize) -> Self {
        Self { least: 0, entering: None, left_in_block: block, block }
    }

    /// Prices every row of `violations`, each a slot with its arc's violation, in turn until one ends a block in which
    /// some arc violated; answers that row's slot, where the search stops, or `None` where the rows ran out first. The
    /// least violation priced is kept, the first priced among equals. The count of a block runs on from one stretch to
    /// the next.
    pub(crate) fn price(&mut self, violations: impl Iterator<Item = (Slot, i64)>) -> Option<Slot> {
        // The rule written as its loop: the counter alone decides when a block ends, and only then is it asked whether
        // the block held a violation, so a row costs one compare with the least and one count.
        for (slot, violation) in violations {
            if violation < self.least {
                self.least = violation;
                self.entering = Some(slot);
            }

            self.left_in_block -= 1;
            if self.left_in_block == 0 {
                self.left_in_block = self.block;
                if self.entering.is_some() {
                    return Some(slot);
                }
            }
        }

        None
    }

    /// The slot of the most violating arc priced so far, the first priced among equals; `None` while no arc violated.
    pub(crate) fn entering(&self) -> Option<Slot> {
        self.entering
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slot_order::tests::slot;

    /// `rows`, each a slot and its violation, as the scan prices them.
    fn hand(rows: &[(usize, i64)]) -> impl Iterator<Item = (Slot, i64)> + '_ {
        rows.iter().map(|&(at, violation)| (slot(at), violation))
    }

    #[test]
    fn test_price_keeps_the_least_and_stops_at_the_end_of_a_block_that_saw_a_violation_by_hand() {
        // A block of three: −2 at slot 1 is kept, and the third row ends the block with a violation behind it.
        let mut scan = BlockScan::new(3);
        assert_eq!(scan.price(hand(&[(0, 0), (1, -2), (2, 5), (3, -9)])), Some(slot(2)));
        assert_eq!(scan, BlockScan { least: -2, entering: Some(slot(1)), left_in_block: 3, block: 3 });

        // Blocks of two: the first ends with nothing violated and the count starts over; of the two rows of −1 in the
        // second the first stays, and the violation in the block's last row ends it.
        let mut scan = BlockScan::new(2);
        assert_eq!(scan.price(hand(&[(5, 1), (6, 0), (7, -1), (8, -1)])), Some(slot(8)));
        assert_eq!((scan.least, scan.entering), (-1, Some(slot(7))));

        // A lesser violation later in the block replaces the one kept; a row of violation zero never violates.
        let mut scan = BlockScan::new(4);
        assert_eq!(scan.price(hand(&[(0, -1), (1, 0), (2, -3), (3, -3)])), Some(slot(3)));
        assert_eq!((scan.least, scan.entering), (-3, Some(slot(2))));
    }

    #[test]
    fn test_price_in_blocks_of_one_stops_at_the_first_violation_by_hand() {
        let mut scan = BlockScan::new(1);

        assert_eq!(scan.price(hand(&[(0, 0), (1, 4), (2, -1), (3, -7)])), Some(slot(2)));
        assert_eq!(scan, BlockScan { least: -1, entering: Some(slot(2)), left_in_block: 1, block: 1 });
    }

    #[test]
    fn test_a_block_that_ends_with_the_stretch_stops_there_by_hand() {
        // Blocks of two over a stretch of four: the second block's last row is the stretch's last.
        let mut scan = BlockScan::new(2);

        assert_eq!(scan.price(hand(&[(0, 0), (1, 0), (2, -5), (3, 0)])), Some(slot(3)));
        assert_eq!(scan, BlockScan { least: -5, entering: Some(slot(2)), left_in_block: 2, block: 2 });
    }

    #[test]
    fn test_a_trailing_partial_block_runs_out_with_its_violation_kept_by_hand() {
        // Blocks of three over four rows: the violation lies in the second block, which the rows end short.
        let mut scan = BlockScan::new(3);

        assert_eq!(scan.price(hand(&[(0, 0), (1, 0), (2, 0), (3, -2)])), None);
        assert_eq!(scan, BlockScan { least: -2, entering: Some(slot(3)), left_in_block: 2, block: 3 });
    }

    #[test]
    fn test_the_count_of_a_block_runs_on_into_the_next_stretch_by_hand() {
        // A block of three that wraps: two rows at the end of the slots, one at their start.
        let mut scan = BlockScan::new(3);

        assert_eq!(scan.price(hand(&[(8, 0), (9, -1)])), None);
        assert_eq!(scan.left_in_block, 1);
        assert_eq!(scan.price(hand(&[(0, 0), (1, -6)])), Some(slot(0)));
        assert_eq!((scan.least, scan.entering), (-1, Some(slot(9))));
    }
}
