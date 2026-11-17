// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The blocks of slots that may hold an arc that violates its bound, with the blocks of the arcs at each node, and the
//! search over the marked blocks alone.

use core::{iter, mem};

use std::collections::TryReserveError;

use crate::basis::PricedRows;
use crate::block_scan::{BlockScan, block_of};
use crate::ranges::widen;
use crate::room::{grow_total, refill, reserve_total};
use crate::slot_order::Slot;

/// The bits of one word of the marks: those of a `u64`.
const WORD_BITS: usize = 64;

/// Which blocks of slots may hold an arc that violates its bound, a bit a block, with the blocks of the arcs at each
/// node: what a solve that prices only what changed searches, and what a change marks.
///
/// The slots are cut into blocks of `⌊√m⌋` consecutive slots, at least one, as block search prices them, the last block
/// shorter where `m` is no multiple of it. A block is marked or clear. Its keeper marks the block of every arc whose
/// violation a change may have made other than it was when the block was last priced clean, and every block at a pivot;
/// [`MarkedSearch`] clears a block only once it prices every row of it clean: so where every arc that violated lay in a
/// marked block before, it does after, and where no block is marked no arc violates. Marking the arcs at a node marks
/// the block of each arc with an end at that node.
///
/// It holds two entries an arc and one a node, at the native width, and a bit a block.
///
/// # Invariant
/// - `block` is `max(⌊√m⌋, 1)` for the `slot_count` slots, `m`: there are `⌈m / block⌉` blocks.
/// - `words` holds `⌈blocks / 64⌉` words, block `b` at bit `b % 64` of word `b / 64`, and no bit for a block at or past
///   the block count is set.
/// - `row_starts` holds `n + 2` entries for the `n` nodes the blocks were laid for, rising from zero to the length of
///   `rows`, or none, before the first lay and after a lay that unwound; row `v` of them,
///   `rows[row_starts[v]..row_starts[v + 1]]`, holds the block of every end at `v` of every arc, each below the block
///   count, a loop's twice; the last row, the root's, is empty.
///
/// Only [`lay`](Self::lay) and [`copy_from`](Self::copy_from), which copies marks that keep it, write `block`,
/// `slot_count`, `row_starts` and `rows`; every other method writes `words` alone, and only at a block below the block
/// count.
#[derive(Clone, Debug)]
pub(super) struct MarkedBlocks {
    /// The rows of a block, at least one.
    block: usize,
    /// The number of slots, `m`.
    slot_count: usize,
    /// Per block, one bit: whether it is marked.
    words: Vec<u64>,
    /// Per node, the root last, and one past it: where its row of blocks starts in `rows`.
    row_starts: Vec<usize>,
    /// Per node, its row: the block of each arc at it, at each of its ends.
    rows: Vec<usize>,
    /// The rows the searches priced, added up: what a test reads to tell how much a solve priced.
    #[cfg(test)]
    priced_rows: usize,
}

/// The counts a lay lays the marks for, which it checks every arc against. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    /// The rows of a block, at least one.
    block: usize,
    /// The number of slots, `m`.
    slot_count: usize,
    /// The number of nodes, `n`, the root not counted.
    node_count: usize,
}

/// The search of one solve over the blocks a [`MarkedBlocks`] marks, from where its last answer was found.
///
/// A search prices the marked blocks in ascending order, from the block after the one that held its last answer and
/// around, the first search from the first block. A block that holds a violation answers the most violating arc of
/// it, the first among equals, and stays marked; a block that holds none is cleared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MarkedSearch {
    /// The block the next search starts at: the one after the block of its last answer, or zero.
    next_block: usize,
}

impl MarkedBlocks {
    /// The blocks of no slot and no node, laid for nothing: no block, no row. It allocates nothing.
    pub(super) const fn new() -> Self {
        Self {
            block: 1,
            slot_count: 0,
            words: Vec::new(),
            row_starts: Vec::new(),
            rows: Vec::new(),
            #[cfg(test)]
            priced_rows: 0,
        }
    }

    /// Makes room for the blocks of up to `arc_count` slots and the rows of up to `node_count` nodes: a
    /// [`lay`](Self::lay) of such counts allocates nothing afterwards. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    pub(super) fn try_reserve(&mut self, node_count: usize, arc_count: usize) -> Result<(), TryReserveError> {
        let blocks = arc_count.div_ceil(block_of(arc_count));

        reserve_total(&mut self.words, blocks.div_ceil(WORD_BITS))?;
        reserve_total(&mut self.row_starts, node_count.saturating_add(2))?;
        reserve_total(&mut self.rows, arc_count.saturating_mul(2))
    }

    /// Makes room as [`try_reserve`](Self::try_reserve) does, growing as a `Vec` does where the memory runs out.
    ///
    /// # Panics
    /// Panics if a column's room passes `isize::MAX` bytes, as a growing `Vec` does.
    pub(super) fn grow(&mut self, node_count: usize, arc_count: usize) {
        let blocks = arc_count.div_ceil(block_of(arc_count));

        grow_total(&mut self.words, blocks.div_ceil(WORD_BITS));
        grow_total(&mut self.row_starts, node_count.saturating_add(2));
        grow_total(&mut self.rows, arc_count.saturating_mul(2));
    }

    /// Makes these marks a copy of `source`: laid for the same slots and nodes, with the same rows, and the same
    /// blocks marked. `source` is unchanged. It costs a copy of `source`'s words and rows, and allocates nothing where
    /// the room holds room for them: where `source` was laid from one arc a slot, as [`try_reserve`](Self::try_reserve)
    /// of its node and slot counts makes it.
    pub(super) fn copy_from(&mut self, source: &Self) {
        (self.block, self.slot_count) = (source.block, source.slot_count);
        self.words.clone_from(&source.words);
        self.row_starts.clone_from(&source.row_starts);
        self.rows.clone_from(&source.rows);
        #[cfg(test)]
        {
            self.priced_rows = source.priced_rows;
        }
    }

    /// Lays the blocks of `slot_count` slots and the rows of `node_count` nodes, the root after them, from `arcs`, each
    /// arc's slot, tail and head, and marks every block. `arcs` is walked twice, and must yield the same arcs each
    /// time. It costs O(m + n), and allocates only where the room is smaller than the counts.
    ///
    /// # Panics
    /// Panics if an arc's slot is not below `slot_count`, or an end not below `node_count`; or if the two walks of
    /// `arcs` yield ends of another number at some node. The marks are then of no slot and no node: no row, no block.
    #[track_caller]
    pub(super) fn lay(
        &mut self,
        slot_count: usize,
        node_count: usize,
        arcs: impl Iterator<Item = (Slot, usize, usize)> + Clone,
    ) {
        // The columns are taken out until the last check has passed: a lay that unwinds leaves marks of no slot and no
        // node, which hold no row and no block, and the room is dropped with the locals.
        let (mut row_starts, mut rows, mut words) =
            (mem::take(&mut self.row_starts), mem::take(&mut self.rows), mem::take(&mut self.words));
        (self.block, self.slot_count) = (1, 0);
        let layout = Layout { block: block_of(slot_count), slot_count, node_count };

        // A counting sort by node: each end counted two places on, the counts summed into the end of each row two
        // places on, which is the start of the row one place on; each end then placed at that start, which it moves
        // on, so that every start ends one place back, where it belongs.
        refill(&mut row_starts, iter::repeat_n(0, node_count + 2));
        for (slot, tail, head) in arcs.clone() {
            let _ = layout.block_at(slot, [tail, head]);
            row_starts[tail + 2] += 1;
            row_starts[head + 2] += 1;
        }
        for at in 1..row_starts.len() {
            row_starts[at] += row_starts[at - 1];
        }

        refill(&mut rows, iter::repeat_n(0, row_starts[node_count + 1]));
        for (slot, tail, head) in arcs {
            let block = layout.block_at(slot, [tail, head]);
            for end in [tail, head] {
                let start = &mut row_starts[end + 1];
                rows[*start] = block;
                *start += 1;
            }
        }
        // The rows are read without a check where these starts say: they must rise to the rows' length.
        let rising = row_starts.is_sorted() && row_starts.last() == Some(&rows.len());
        assert!(rising, "called `MarkedBlocks::lay` with arcs whose two walks yield other ends");

        refill_marked(&mut words, slot_count.div_ceil(layout.block));
        (self.block, self.slot_count, self.row_starts, self.rows, self.words) =
            (layout.block, slot_count, row_starts, rows, words);
    }

    /// Marks every block: the mark of a pivot. A pivot moves the potentials of a subtree, and the arcs at the nodes of
    /// either side of it lie, under the dealing of the arcs to their slots, in nearly every block once the side holds
    /// about as many arc ends as there are blocks; marking every block costs a word for 64 of them, keeps every arc
    /// that violates in a marked block whatever moved, and leaves the next search block search from where the last
    /// stopped. It allocates nothing.
    pub(super) fn mark_every_block(&mut self) {
        refill_marked(&mut self.words, self.slot_count.div_ceil(self.block));
    }

    /// Marks the block of `slot`. It costs one bit.
    ///
    /// # Panics
    /// Panics if `slot` is not below the slot count the blocks were laid for.
    #[track_caller]
    pub(super) fn mark_slot(&mut self, slot: Slot) {
        let (at, slot_count) = (usize::from(slot), self.slot_count);
        assert!(
            at < slot_count,
            "called `MarkedBlocks::mark_slot` with {slot:?}, which should be below {slot_count} slots"
        );

        // A slot below the slot count lies in a block below the block count, which the words hold a bit for.
        let block = at / self.block;
        self.words[block / WORD_BITS] |= 1 << (block % WORD_BITS);
    }

    /// Marks the block of every arc with an end at each node of `nodes`, the root being the node after the last.
    /// It costs one read a node and one bit an end of an arc there.
    ///
    /// # Panics
    /// Panics if a node is past the root of the nodes the blocks were laid for.
    #[track_caller]
    pub(super) fn mark_nodes(&mut self, nodes: impl IntoIterator<Item = usize>) {
        for node in nodes {
            self.mark_node(node);
        }
    }

    /// Marks the block of every arc with an end at `node`.
    ///
    /// # Panics
    /// Panics if `node` is past the root of the nodes the blocks were laid for.
    #[track_caller]
    fn mark_node(&mut self, node: usize) {
        let row_count = self.row_starts.len().saturating_sub(1);
        if node >= row_count {
            mark_nodes_outside_rows(node, row_count);
        }

        debug_assert!(node + 1 < self.row_starts.len(), "a node below the row count should have its row's two ends");
        // SAFETY: the starts hold one more entry than there are rows (`MarkedBlocks`), and `node` is below the row
        // count, as just compared.
        let (start, end) = unsafe { (*self.row_starts.get_unchecked(node), *self.row_starts.get_unchecked(node + 1)) };
        debug_assert!(start <= end && end <= self.rows.len(), "a row should lie within the rows");
        // SAFETY: the starts rise to the length of the rows (`MarkedBlocks`), so each row lies within them.
        let row = unsafe { self.rows.get_unchecked(start..end) };

        for &block in row {
            let word = block / WORD_BITS;
            debug_assert!(word < self.words.len(), "a block of a row should be below the block count");
            // SAFETY: every block a row holds is below the block count, and the words hold a bit for each block
            // (`MarkedBlocks`).
            unsafe { *self.words.get_unchecked_mut(word) |= 1 << (block % WORD_BITS) };
        }
    }

    /// The first marked block at or after `from` and then from the first block, around; `None` where none is marked.
    /// It reads at most one word more than the words.
    fn next_marked(&self, from: usize) -> Option<usize> {
        self.first_marked_from(from).or_else(|| self.first_marked_from(0))
    }

    /// The first marked block at or after `from`; `None` where none is.
    fn first_marked_from(&self, from: usize) -> Option<usize> {
        let first = from / WORD_BITS;
        let words = self.words.get(first..)?;
        let below = u64::MAX << (from % WORD_BITS);

        // No bit at or past the block count is set (`MarkedBlocks`): a bit found is a block's.
        words.iter().enumerate().find_map(|(offset, &word)| {
            let word = if offset == 0 { word & below } else { word };
            (word != 0).then(|| (first + offset) * WORD_BITS + widen(word.trailing_zeros()))
        })
    }

    /// Clears `block`; a block past the words, which no bit marks, stays clear.
    fn clear(&mut self, block: usize) {
        if let Some(word) = self.words.get_mut(block / WORD_BITS) {
            *word &= !(1 << (block % WORD_BITS));
        }
    }

    /// The slots of `block`, a block below the block count: its `block` rows, fewer for the last.
    fn slots_of(&self, block: usize) -> core::ops::Range<usize> {
        let start = block * self.block;

        start..(start + self.block).min(self.slot_count)
    }
}

impl Layout {
    /// The block of `slot`, checked below the slot count, with `ends` checked below the node count.
    ///
    /// # Panics
    /// Panics if `slot` is not below the slot count, or an end not below the node count.
    #[track_caller]
    fn block_at(&self, slot: Slot, ends: [usize; 2]) -> usize {
        let Self { block, slot_count, node_count } = *self;
        let at = usize::from(slot);
        assert!(
            at < slot_count && ends.iter().all(|&end| end < node_count),
            "called `MarkedBlocks::lay` with the arc at {slot:?} joining {ends:?}, which should lie within \
             {slot_count} slots and {node_count} nodes"
        );

        at / block
    }
}

impl MarkedSearch {
    /// The search of a solve that has priced nothing yet: from the first block.
    pub(super) const fn new() -> Self {
        Self { next_block: 0 }
    }

    /// The slot of the most violating arc of the first marked block of `rows` that holds a violation, the first priced
    /// among equals, from the block after the one of this search's last answer and around; `None` exactly when no arc
    /// of a marked block violates its bound. Each marked block priced on the way that holds no violation is cleared in
    /// `marks`; the block of the answer stays marked.
    ///
    /// It prices each block it passes, `⌊√m⌋` rows in sequence, and reads at most one word more than the words of the
    /// marks to find them; it allocates nothing.
    ///
    /// # Panics
    /// Panics if `rows` holds another number of rows than the slots `marks` were laid for.
    #[track_caller]
    pub(super) fn next_entering(&mut self, rows: &PricedRows<'_>, marks: &mut MarkedBlocks) -> Option<Slot> {
        let (row_count, slot_count) = (rows.len(), marks.slot_count);
        if row_count != slot_count {
            next_entering_of_other_rows(row_count, slot_count);
        }

        // Each pass of the loop clears a marked block or answers: at most the block count of passes.
        loop {
            let block = marks.next_marked(self.next_block)?;
            // A marked block is below the block count (`MarkedBlocks`): its slots lie within the rows, and it holds
            // one row at least.
            let slots = marks.slots_of(block);
            let mut scan = BlockScan::new(slots.len());
            #[cfg(test)]
            {
                marks.priced_rows += slots.len();
            }
            let _ = scan.price(rows.violations(slots));

            match scan.entering() {
                Some(slot) => {
                    self.next_block = block + 1;
                    return Some(slot);
                }
                None => marks.clear(block),
            }
        }
    }
}

/// Rewrites `words` as the marks of `blocks` blocks, every one marked: every word but the last holds 64 blocks, the
/// last the rest, from its first bit up.
fn refill_marked(words: &mut Vec<u64>, blocks: usize) {
    let word = |at: usize| match blocks - at * WORD_BITS {
        whole if whole >= WORD_BITS => u64::MAX,
        rest => (1 << rest) - 1,
    };

    refill(words, (0..blocks.div_ceil(WORD_BITS)).map(word));
}

/// Panics for a `node` past the `row_count` rows, as [`MarkedBlocks::mark_nodes`] documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: the nodes a network marks are its own, so the check keeps only the comparison. Its caller
// tracks its own caller too, so the panic names the line the node came from.
#[cold]
#[inline(never)]
#[track_caller]
fn mark_nodes_outside_rows(node: usize, row_count: usize) -> ! {
    panic!("called `MarkedBlocks::mark_nodes` with node {node}, which should be below the {row_count} rows laid")
}

/// Panics for `row_count` rows where the marks were laid for `slot_count` slots, as [`MarkedSearch::next_entering`]
/// documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: a search is made for the rows of the network that laid the marks, so the check keeps only
// the comparison. Its caller tracks its own caller too, so the panic names the line of the search.
#[cold]
#[inline(never)]
#[track_caller]
fn next_entering_of_other_rows(row_count: usize, slot_count: usize) -> ! {
    panic!(
        "called `MarkedSearch::next_entering` with {row_count} rows, which should be the {slot_count} slots the marks \
         were laid for"
    )
}

#[cfg(test)]
pub(super) mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use kahlnws_testing::MincostNetwork;
    use proptest::prelude::*;

    use super::*;
    use crate::basis::{BasisRoom, loaded};
    use crate::problem::tests::problem_of;
    use crate::slot_order::tests::slot;

    impl MarkedBlocks {
        /// The room of each of its columns, in entries: the words, the row starts and the rows.
        pub(in crate::network) fn capacities(&self) -> [usize; 3] {
            [self.words.capacity(), self.row_starts.capacity(), self.rows.capacity()]
        }
    }

    #[test]
    fn test_grow_makes_the_room_try_reserve_makes() {
        // 100 nodes and 90 000 arcs: every column's total lies above the least room an empty `Vec` makes, 4 entries,
        // so each total is the room made, and a total that drifts by one shows: the words those of
        // `⌈90 000 / 300⌉ = 300` blocks, `⌈300 / 64⌉ = 5`, the row starts 102 and the rows 180 000.
        let (mut grown, mut reserved) = (MarkedBlocks::new(), MarkedBlocks::new());
        grown.grow(100, 90_000);
        reserved.try_reserve(100, 90_000).expect("100 nodes and 90 000 arcs should fit");
        assert_eq!(grown.capacities(), reserved.capacities());
    }

    /// Whether the block of `slot` is marked in `marks`.
    pub(in crate::network) fn is_marked(marks: &MarkedBlocks, slot: Slot) -> bool {
        let block = usize::from(slot) / marks.block;

        marks.words.get(block / WORD_BITS).is_some_and(|word| word & (1 << (block % WORD_BITS)) != 0)
    }

    /// Clears every block of `marks`.
    pub(in crate::network) fn clear_every_block(marks: &mut MarkedBlocks) {
        marks.words.iter_mut().for_each(|word| *word = 0);
    }

    /// The rows `marks`'s searches priced, added up.
    pub(in crate::network) fn priced_rows(marks: &MarkedBlocks) -> usize {
        marks.priced_rows
    }

    /// The number of blocks of `marks`, `⌈m / block⌉`.
    fn block_count(marks: &MarkedBlocks) -> usize {
        marks.slot_count.div_ceil(marks.block)
    }

    /// The marked blocks of `marks`, ascending.
    fn marked(marks: &MarkedBlocks) -> Vec<usize> {
        (0..block_count(marks)).filter(|&block| is_marked(marks, slot(block * marks.block))).collect()
    }

    /// One node of no supply with a loop of capacity one for each of `costs`. A single node deals arc `a` to slot `a`,
    /// and in the star every potential is zero and every loop stands at its lower bound: the arc at slot `a` violates
    /// by `costs[a]`.
    fn loops(costs: &[i64]) -> MincostNetwork {
        let arcs: Vec<(usize, usize, u64, u64, i64)> = costs.iter().map(|&cost| (0, 0, 0, 1, cost)).collect();

        MincostNetwork::from_arcs(&[0], &arcs)
    }

    /// The marks of `slot_count` loops at one node, every block marked.
    fn laid_loops(slot_count: usize) -> MarkedBlocks {
        let mut marks = MarkedBlocks::new();
        marks.lay(slot_count, 1, (0..slot_count).map(|at| (slot(at), 0, 0)));

        marks
    }

    /// Four arcs on three nodes, laid in ten slots in blocks of three: from 0 to 1 at slot 0, from 1 to 2 at slot 4, a
    /// loop at node 1 at slot 5, and from 2 to 0 at slot 9.
    fn four_arcs() -> MarkedBlocks {
        let mut marks = MarkedBlocks::new();
        let arcs = [(0, 0, 1), (4, 1, 2), (9, 2, 0), (5, 1, 1)];
        marks.lay(10, 3, arcs.into_iter().map(|(at, tail, head)| (slot(at), tail, head)));

        marks
    }

    #[test]
    fn test_a_lay_writes_each_nodes_blocks_and_marks_every_block_by_hand() {
        // Blocks of three over ten slots: 0 to 2, 3 to 5, 6 to 8 and 9. Node 0 holds the ends of the arcs at slots 0
        // and 9, blocks 0 and 3; node 1 those at slots 0, 4 and 5, the loop twice: blocks 0, 1, 1, 1; node 2 those at
        // slots 4 and 9, blocks 1 and 3; the root none.
        let marks = four_arcs();

        assert_eq!((marks.block, marks.slot_count, block_count(&marks)), (3, 10, 4));
        assert_eq!(marks.row_starts, [0, 2, 6, 8, 8]);
        assert_eq!(marks.rows, [0, 3, 0, 1, 1, 1, 1, 3]);
        assert_eq!(marks.words, [0b1111]);
        assert_eq!(marked(&marks), [0, 1, 2, 3]);
    }

    #[test]
    fn test_marking_every_block_sets_no_bit_past_the_blocks_by_hand() {
        // 4,225 slots in blocks of 65: exactly 65 blocks, one whole word and one bit of the next; 4,096 slots in blocks
        // of 64: 64 blocks, one whole word; no slot: no block, no word.
        assert_eq!(laid_loops(4225).words, [u64::MAX, 1]);
        assert_eq!(laid_loops(4096).words, [u64::MAX]);
        assert_eq!(laid_loops(0).words, Vec::<u64>::new());
        assert_eq!(laid_loops(3).words, [0b111], "blocks of one row");
    }

    #[test]
    fn test_marking_a_node_marks_the_block_of_each_of_its_arcs_by_hand() {
        let mut marks = four_arcs();
        clear_every_block(&mut marks);

        marks.mark_nodes([2]);
        assert_eq!(marked(&marks), [1, 3], "node 2: the arcs at slots 4 and 9");
        marks.mark_nodes([3]);
        assert_eq!(marked(&marks), [1, 3], "the root has no arc");
        marks.mark_nodes([1, 0]);
        assert_eq!(marked(&marks), [0, 1, 3], "block 2 holds no arc");
    }

    #[test]
    fn test_marking_a_slot_marks_its_block_by_hand() {
        let mut marks = four_arcs();
        clear_every_block(&mut marks);

        marks.mark_slot(slot(7));
        assert_eq!(marked(&marks), [2], "slots 6 to 8 make block 2");
        marks.mark_slot(slot(9));
        marks.mark_slot(slot(7));
        assert_eq!(marked(&marks), [2, 3], "the last block holds slot 9 alone; a mark twice is one");
    }

    #[test]
    #[should_panic(expected = "called `MarkedBlocks::mark_slot` with Slot 10, which should be below 10 slots")]
    fn test_marking_a_slot_past_the_slots_panics() {
        four_arcs().mark_slot(slot(10));
    }

    #[test]
    #[should_panic(expected = "called `MarkedBlocks::mark_nodes` with node 4, which should be below the 4 rows laid")]
    fn test_marking_a_node_past_the_root_panics() {
        four_arcs().mark_nodes([0, 4]);
    }

    #[test]
    #[should_panic(expected = "called `MarkedBlocks::mark_nodes` with node 0, which should be below the 0 rows laid")]
    fn test_marks_never_laid_have_no_node() {
        MarkedBlocks::new().mark_nodes([0]);
    }

    #[test]
    #[should_panic(expected = "called `MarkedBlocks::lay` with the arc at Slot 3 joining [0, 1], which should lie \
                               within 3 slots and 2 nodes")]
    fn test_a_lay_refuses_a_slot_past_the_slots() {
        MarkedBlocks::new().lay(3, 2, [(slot(3), 0, 1)].into_iter());
    }

    #[test]
    #[should_panic(expected = "called `MarkedBlocks::lay` with the arc at Slot 0 joining [0, 2], which should lie \
                               within 3 slots and 2 nodes")]
    fn test_a_lay_refuses_an_end_past_the_nodes() {
        MarkedBlocks::new().lay(3, 2, [(slot(0), 0, 2)].into_iter());
    }

    /// Two walks of arcs that disagree: the clone yields arcs from 0 to 1 and from 1 to 2, the original two loops at 0.
    #[derive(Debug)]
    struct TwoWalksIter {
        /// Whether this is the clone.
        cloned: bool,
        /// The arcs yielded so far.
        at: usize,
    }

    impl Clone for TwoWalksIter {
        fn clone(&self) -> Self {
            Self { cloned: true, at: self.at }
        }
    }

    impl Iterator for TwoWalksIter {
        type Item = (Slot, usize, usize);

        fn next(&mut self) -> Option<Self::Item> {
            let arcs = if self.cloned { [(0, 0, 1), (1, 1, 2)] } else { [(0, 0, 0), (1, 0, 0)] };
            let (at, tail, head) = arcs.get(self.at).copied()?;
            self.at += 1;

            Some((slot(at), tail, head))
        }
    }

    #[test]
    fn test_a_lay_that_unwinds_leaves_marks_of_no_row_and_no_block() {
        // Laid first for ten slots, then a lay whose two walks disagree unwinds at its last check: the marks then hold
        // no row, so a node is refused, and no block, so a search finds none.
        let mut marks = four_arcs();
        let laid = catch_unwind(AssertUnwindSafe(|| marks.lay(4, 3, TwoWalksIter { cloned: false, at: 0 })));
        assert!(laid.is_err(), "the lay should refuse walks that disagree");

        let marked = catch_unwind(AssertUnwindSafe(|| marks.mark_nodes([1]))).expect_err("no row should be laid");
        assert_eq!(
            marked.downcast_ref::<String>().map(String::as_str),
            Some("called `MarkedBlocks::mark_nodes` with node 1, which should be below the 0 rows laid")
        );
        let network = loops(&[]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        assert_eq!(MarkedSearch::new().next_entering(&basis.rows(), &mut marks), None);
    }

    #[test]
    fn test_a_search_answers_from_the_first_marked_block_that_violates_and_clears_the_clean_by_hand() {
        // Nine slots in blocks of three, violations −1 at slot 4 and −5 at slot 6.
        let network = loops(&[0, 0, 0, 0, -1, 0, -5, 0, 0]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let rows = basis.rows();
        let mut marks = laid_loops(9);
        let mut search = MarkedSearch::new();

        // Block 0 prices clean and is cleared; block 1 holds −1 at slot 4, and stays.
        assert_eq!(search.next_entering(&rows, &mut marks), Some(slot(4)));
        assert_eq!((marked(&marks), search.next_block, priced_rows(&marks)), (vec![1, 2], 2, 6));

        // From block 2: −5 at slot 6.
        assert_eq!(search.next_entering(&rows, &mut marks), Some(slot(6)));
        assert_eq!((marked(&marks), search.next_block, priced_rows(&marks)), (vec![1, 2], 3, 9));

        // From past the last block, around: block 1 again, block 0 being clear.
        assert_eq!(search.next_entering(&rows, &mut marks), Some(slot(4)));
        assert_eq!(priced_rows(&marks), 12, "the clear block 0 is not priced");
    }

    #[test]
    fn test_a_search_where_nothing_violates_clears_every_block_and_then_prices_no_row_by_hand() {
        let network = loops(&[0, 1, 2, 0, 5, 3, 1]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let mut marks = laid_loops(7);
        let mut search = MarkedSearch { next_block: 2 };

        assert_eq!(search.next_entering(&basis.rows(), &mut marks), None);
        assert_eq!((marked(&marks), priced_rows(&marks), search.next_block), (vec![], 7, 2));

        assert_eq!(search.next_entering(&basis.rows(), &mut marks), None);
        assert_eq!(priced_rows(&marks), 7, "nothing marked: no row priced");
    }

    #[test]
    fn test_a_search_prices_only_the_marked_blocks_by_hand() {
        // Ten slots in blocks of three; only block 3, slot 9 alone, is marked, and it holds −2. The −7 at slot 1 lies
        // in a clear block and is not seen.
        let network = loops(&[0, -7, 0, 0, 0, 0, 0, 0, 0, -2]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let mut marks = laid_loops(10);
        marks.words = vec![0b1000];

        assert_eq!(MarkedSearch::new().next_entering(&basis.rows(), &mut marks), Some(slot(9)));
        assert_eq!((marked(&marks), priced_rows(&marks)), (vec![3], 1));
    }

    #[test]
    fn test_the_first_of_equal_violations_in_a_block_enters_and_blocks_of_one_row_answer_the_first_by_hand() {
        let network = loops(&[-3, -3, 0, 0]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        assert_eq!(MarkedSearch::new().next_entering(&basis.rows(), &mut laid_loops(4)), Some(slot(0)));

        // Three slots, blocks of one: the first violation, not the most violating.
        let network = loops(&[0, -1, -5]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let mut marks = laid_loops(3);
        assert_eq!(MarkedSearch::new().next_entering(&basis.rows(), &mut marks), Some(slot(1)));
        assert_eq!(marked(&marks), [1, 2]);
    }

    #[test]
    #[should_panic(
        expected = "called `MarkedSearch::next_entering` with 3 rows, which should be the 4 slots the marks \
                               were laid for"
    )]
    fn test_a_search_refuses_rows_of_another_count() {
        let network = loops(&[0, 0, 0]);
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));
        let _ = MarkedSearch::new().next_entering(&basis.rows(), &mut laid_loops(4));
    }

    #[test]
    fn test_a_lay_within_the_room_reserved_allocates_nothing() {
        let mut marks = MarkedBlocks::new();
        marks.try_reserve(3, 4225).expect("the room should be reserved");
        let room = (marks.words.as_ptr(), marks.row_starts.as_ptr(), marks.rows.as_ptr());

        marks.lay(4225, 3, (0..4225).map(|at| (slot(at), at % 3, (at + 1) % 3)));
        assert_eq!((marks.words.len(), marks.row_starts.len(), marks.rows.len()), (2, 5, 8450));
        assert_eq!((marks.words.as_ptr(), marks.row_starts.as_ptr(), marks.rows.as_ptr()), room);
    }

    #[test]
    fn test_a_copy_holds_the_same_rows_and_marks_and_a_copy_of_none_is_laid_for_nothing_by_hand() {
        // The four arcs with blocks 1 and 3 marked alone, copied over the marks of 4,225 loops at one node.
        let mut source = four_arcs();
        clear_every_block(&mut source);
        source.mark_nodes([2]);
        let mut copy = laid_loops(4225);

        copy.copy_from(&source);
        assert_eq!(
            (copy.block, copy.slot_count, &copy.row_starts, &copy.rows),
            (3, 10, &source.row_starts, &source.rows)
        );
        assert_eq!((marked(&copy), copy.words.as_slice()), (vec![1, 3], [0b1010].as_slice()));
        assert_eq!(marked(&source), [1, 3], "the source is unchanged");

        // A copy of marks never laid: no row, no block.
        copy.copy_from(&MarkedBlocks::new());
        assert_eq!(
            (copy.block, copy.slot_count, copy.words.len(), copy.row_starts.len(), copy.rows.len()),
            (1, 0, 0, 0, 0)
        );
    }

    #[test]
    fn test_a_copy_into_the_room_reserved_for_its_counts_allocates_nothing() {
        let source = laid_loops(4225);
        let mut copy = MarkedBlocks::new();
        copy.try_reserve(1, 4225).expect("the room should be reserved");
        let room = (copy.words.as_ptr(), copy.row_starts.as_ptr(), copy.rows.as_ptr());

        copy.copy_from(&source);
        assert_eq!((copy.words.len(), copy.row_starts.len(), copy.rows.len()), (2, 3, 8450));
        assert_eq!((copy.words.as_ptr(), copy.row_starts.as_ptr(), copy.rows.as_ptr()), room);
    }

    proptest! {
        #[test]
        fn test_a_search_answers_as_a_plain_scan_of_the_marked_blocks(
            costs in proptest::collection::vec(-3_i64..=3, 0..=40),
            marks_drawn in proptest::collection::vec(any::<bool>(), 7),
            from in 0_usize..=8,
        ) {
            // The model: the blocks in the order the search visits them, from `from` around; the first marked one
            // that holds a violation answers its least, the first among equals; every marked one before it is
            // cleared, and nothing else changes.
            let network = loops(&costs);
            let mut room = BasisRoom::new();
            let basis = loaded(&mut room, &problem_of(&network));
            let mut marks = laid_loops(costs.len());
            let block_count = block_count(&marks);
            let drawn: Vec<usize> = (0..block_count).filter(|&block| marks_drawn[block % 7]).collect();
            clear_every_block(&mut marks);
            for &block in &drawn {
                marks.words[block / WORD_BITS] |= 1 << (block % WORD_BITS);
            }

            let mut expected_marked = drawn.clone();
            let mut expected = None;
            let order = (from..block_count).chain(0..from.min(block_count));
            for block in order.filter(|block| drawn.contains(block)) {
                let slots = marks.slots_of(block);
                let least = slots.clone().map(|at| costs[at]).min().filter(|&least| least < 0);
                match least {
                    Some(least) => {
                        expected = slots.clone().find(|&at| costs[at] == least).map(slot);
                        break;
                    }
                    None => expected_marked.retain(|&marked| marked != block),
                }
            }

            // The rows priced are those of the blocks visited: each cleared one and the answer's.
            let answer_rows = expected.map_or(0, |at| marks.slots_of(usize::from(at) / marks.block).len());
            let cleared = drawn.iter().filter(|block| !expected_marked.contains(block));
            let expected_rows: usize = cleared.map(|&block| marks.slots_of(block).len()).sum::<usize>() + answer_rows;

            let mut search = MarkedSearch { next_block: from };
            prop_assert_eq!(search.next_entering(&basis.rows(), &mut marks), expected);
            prop_assert_eq!(marked(&marks), expected_marked);
            prop_assert_eq!(priced_rows(&marks), expected_rows);
        }
    }
}
