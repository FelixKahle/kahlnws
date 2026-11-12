// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The choice of the entering arc: block search over the slots, from where the last search stopped, entering the most
//! violating arc of the first block that holds a violation.

use crate::basis::PricedRows;
use crate::slot_order::Slot;

/// Block search over the slots of one basis, with where the next search starts.
///
/// A search prices the rows in blocks of `⌊√m⌋` consecutive slots, at least one, from the slot where the last search
/// stopped on to the last slot and then from the first, the count of a block running on across that wrap. It stops at
/// the end of the first block in which some arc violated its bound and enters the most violating arc it priced, the
/// first met among equals; the next search starts at the slot that ended that block. Where no block ends with a
/// violation behind it, the search prices every row once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BlockSearch {
    /// The rows a block prices: `⌊√m⌋` of the `m` slots the search was made for, at least one.
    block: usize,
    /// The slot the next search starts at: the one that ended the last search's block, or zero.
    next: usize,
}

/// The state of one search, shared by its scan of the two stretches of slots it wraps around.
///
/// # Invariant
/// `entering` is the slot of the least violation priced so far, the first priced among equals, and `least` that
/// violation, where it is below zero; while no violation below zero was priced, `entering` is `None` and `least` zero.
/// `left_in_block` is at least one, and at most `block`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BlockScan {
    /// The least violation priced, in units of cost; zero while none below zero was.
    least: i64,
    /// The slot of the arc of that violation; `None` while no arc violated.
    entering: Option<Slot>,
    /// The rows left to price in the current block.
    left_in_block: usize,
    /// The rows a block prices.
    block: usize,
}

impl BlockSearch {
    /// The search over `slot_count` slots, starting at the first.
    pub(crate) fn new(slot_count: usize) -> Self {
        // The block of `⌊√m⌋` rows is the established rule of block search, not a number fitted here. What it trades
        // is the rows a search prices against the choice it makes: a block of `b` rows costs a search at least `b`
        // rows and enters the best of the at least `b` candidates it saw; a block of one is first eligible, a block
        // of `m` best eligible, and `√m` lies between the two at an equal ratio from each. The floor of one gives a
        // problem of fewer than four arcs blocks of one row, and a problem of no arc a block that is never entered.
        Self { block: slot_count.isqrt().max(1), next: 0 }
    }

    /// The slot of the most violating arc of the first block of `rows` that holds a violation, the first met among
    /// equals, searching from where the last search stopped and wrapping once; `None` exactly when no arc violates its
    /// bound. The next search starts at the slot that ended the block the arc was found in.
    ///
    /// It is total: rows of another count than the search was made for are priced in its blocks too, from the slot
    /// the last search stopped at, or from the end of the rows where that lies past them, and wrapping once. It prices
    /// at least one block where a violation lies in the first, and at most every row once; it allocates nothing.
    pub(crate) fn next_entering(&mut self, rows: &PricedRows<'_>) -> Option<Slot> {
        let slot_count = rows.len();
        let start = self.next.min(slot_count);

        // Both stretches lie within the rows, since `start` is at most their count. The first search starts at slot
        // zero, so its second stretch is empty.
        let mut scan = BlockScan::new(self.block);
        let ended = scan.price(rows.violations(start..slot_count)).or_else(|| scan.price(rows.violations(0..start)));

        if let Some(ended) = ended {
            self.next = usize::from(ended);
        }

        scan.entering
    }
}

impl BlockScan {
    /// A scan of blocks of `block` rows, at least one, at the start of its first block, with nothing priced yet.
    fn new(block: usize) -> Self {
        Self { least: 0, entering: None, left_in_block: block, block }
    }

    /// Prices every row of `violations`, each a slot with its arc's violation, in turn until one ends a block in which
    /// some arc violated; answers that row's slot, where the search stops, or `None` where the rows ran out first. The
    /// least violation priced is kept, the first priced among equals. The count of a block runs on from one stretch to
    /// the next.
    fn price(&mut self, violations: impl Iterator<Item = (Slot, i64)>) -> Option<Slot> {
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
}

#[cfg(test)]
mod tests {
    use kahlnws_testing::{MincostNetwork, TinyBounds, TinyCosts, check_against_brute_force, tiny_networks};
    use proptest::prelude::*;

    use super::*;
    use crate::basis::BasisRoom;
    use crate::basis::tests::{CheckedRun, loaded};
    use crate::problem::tests::problem_of;
    use crate::slot_order::tests::slot;

    /// One node of no supply with a loop of capacity one for each of `costs`. A single node deals arc `a` to slot
    /// `a`, and in the star every potential is zero and every loop stands at its lower bound: the arc at slot `a`
    /// violates by `costs[a]`.
    fn loops(costs: &[i64]) -> MincostNetwork {
        let arcs: Vec<(usize, usize, u64, u64, i64)> = costs.iter().map(|&cost| (0, 0, 0, 1, cost)).collect();

        MincostNetwork::from_arcs(&[0], &arcs)
    }

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

    #[test]
    fn test_the_block_is_the_floor_of_the_square_root_and_at_least_one_by_hand() {
        let blocks = [0, 1, 3, 4, 8, 9, 15, 16, 1_000_000].map(|slot_count| BlockSearch::new(slot_count).block);

        assert_eq!(blocks, [1, 1, 1, 2, 2, 3, 3, 4, 1000]);
        assert_eq!(BlockSearch::new(9).next, 0, "the first search starts at the first slot");
    }

    #[test]
    fn test_a_search_restarts_at_the_slot_that_ended_its_block_and_wraps_by_hand() {
        // Nine slots in blocks of three, violations −1 at slot 4 and −5 at slot 6.
        let network = loops(&[0, 0, 0, 0, -1, 0, -5, 0, 0]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let rows = basis.rows();
        let mut search = BlockSearch::new(rows.len());

        // Slots 0 to 2 hold nothing; slots 3 to 5 hold −1 at slot 4, and slot 5 ends the block.
        assert_eq!(search.next_entering(&rows), Some(slot(4)));
        assert_eq!(search, BlockSearch { block: 3, next: 5 });

        // From slot 5 the block is 5 to 7, which holds −5 at slot 6.
        assert_eq!(search.next_entering(&rows), Some(slot(6)));
        assert_eq!(search.next, 7);

        // From slot 7 the block 7, 8, 0 wraps and holds nothing, as does 1 to 3; the block 4 to 6 holds both
        // violations, and the least of them enters.
        assert_eq!(search.next_entering(&rows), Some(slot(6)));
        assert_eq!(search.next, 6);
    }

    #[test]
    fn test_the_first_of_equal_violations_enters_by_hand() {
        let network = loops(&[-3, -3, 0, 0]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));

        assert_eq!(BlockSearch::new(4).next_entering(&basis.rows()), Some(slot(0)));
    }

    #[test]
    fn test_a_block_of_one_enters_the_first_violation_not_the_most_violating_by_hand() {
        let network = loops(&[0, -1, -5]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let mut search = BlockSearch::new(3);

        assert_eq!(search.next_entering(&basis.rows()), Some(slot(1)));
        assert_eq!(search.next, 1);
    }

    #[test]
    fn test_a_violation_in_the_last_partial_block_enters_without_moving_the_start_by_hand() {
        // Ten slots in blocks of three: the rows run out in the block that starts at slot 9 before it ends.
        let network = loops(&[0, 0, 0, 0, 0, 0, 0, 0, 0, -2]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let mut search = BlockSearch::new(10);

        assert_eq!(search.next_entering(&basis.rows()), Some(slot(9)));
        assert_eq!(search, BlockSearch { block: 3, next: 0 });
    }

    #[test]
    fn test_a_block_that_wraps_ends_in_the_first_stretch_by_hand() {
        // From slot 8, the block 8, 9, 0 wraps and ends at slot 0, which violates.
        let network = loops(&[-4, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let mut search = BlockSearch { block: 3, next: 8 };

        assert_eq!(search.next_entering(&basis.rows()), Some(slot(0)));
        assert_eq!(search.next, 0);
    }

    #[test]
    fn test_no_arc_entering_exactly_when_none_violates_by_hand() {
        let network = loops(&[0, 1, 2, 0, 5]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let mut search = BlockSearch { block: 2, next: 3 };

        assert_eq!(search.next_entering(&basis.rows()), None);
        assert_eq!(search.next, 3, "a search that finds nothing leaves the start where it was");

        let network = loops(&[0, 1, 2, 0, -1]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        assert_eq!(search.next_entering(&basis.rows()), Some(slot(4)), "one violating arc is found from anywhere");
    }

    #[test]
    fn test_a_problem_of_no_arc_has_nothing_to_enter() {
        let network = loops(&[]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let mut search = BlockSearch::new(0);

        assert_eq!((search.next_entering(&basis.rows()), search.next), (None, 0));
    }

    #[test]
    fn test_rows_short_of_where_the_last_search_stopped_are_searched_from_their_end_by_hand() {
        // A search left at slot 5 over three rows starts at their end, so its one stretch is all three.
        let network = loops(&[0, -1, -2]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let mut search = BlockSearch { block: 1, next: 5 };

        assert_eq!(search.next_entering(&basis.rows()), Some(slot(1)));
        assert_eq!(search.next, 1);
    }

    /// Runs `network` from the star, its lower bounds given, with the arcs block search chooses, to its end with the
    /// invariant checked after every pivot and the pivots bounded, and checks the end against the brute force.
    fn check_block_search(network: &MincostNetwork) -> Result<(), TestCaseError> {
        let mut room = BasisRoom::new();
        let mut search = BlockSearch::new(network.arc_count());
        let end = CheckedRun::start(&mut room, network, &problem_of(network))
            .and_then(|run| run.run_to_end(|rows| search.next_entering(rows)))
            .map_err(TestCaseError::fail)?;

        check_against_brute_force(network, end.answer.as_ref())
            .map_err(|breach| TestCaseError::fail(breach.to_string()))
    }

    proptest! {
        #[test]
        fn test_tiny_runs_by_block_search_keep_the_invariant_and_end_optimal(
            network in tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered),
        ) {
            check_block_search(&network)?;
        }

        #[test]
        fn test_tiny_runs_by_block_search_with_tied_costs_keep_the_invariant_and_end_optimal(
            network in tiny_networks(TinyCosts::ZeroOrUniform(-9..=9), TinyBounds::Ordered),
        ) {
            check_block_search(&network)?;
        }
    }
}
