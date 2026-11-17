// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The order the pricing scans the arcs in: each arc dealt to its slot with a stride.

use core::iter::FusedIterator;

use crate::ranges::{INDEX_LIMIT, widen};

/// The place of an arc in the order the pricing scans the arcs. It names a place and proves nothing; the basis
/// checks one where it takes it.
///
/// Stored at 32 bits, as every index a solve keeps in a column, not at the native width: the walks up the tree read
/// one stored index a node, each a dependent load, so their speed is that of the cache the columns fit in, and a
/// narrow index takes half the bytes. In a network simplex with the same column layout, narrowing its stored links
/// from 64 to 32 bits, at an unchanged pivot count, took a solve of the assignment shape of 64,000 arcs from 71.5 ms
/// to 37.4 ms on an Apple M3 Max, of 32,000 arcs from 11.7 to 9.9 ms, and below 16,000 arcs it stayed within noise.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, derive_more::Debug)]
#[debug("Slot {_0}")]
pub(crate) struct Slot(u32);

impl From<usize> for Slot {
    /// The slot at `index`.
    ///
    /// # Panics
    /// Panics if `index` passes `u32::MAX`.
    #[track_caller]
    fn from(index: usize) -> Self {
        let Ok(raw) = u32::try_from(index) else { panic!("called `Slot::from` with {index}, past u32::MAX") };

        Self(raw)
    }
}

impl From<u32> for Slot {
    /// The slot at the stored index `raw`.
    fn from(raw: u32) -> Self {
        Self(raw)
    }
}

impl From<Slot> for usize {
    fn from(slot: Slot) -> Self {
        widen(slot.0)
    }
}

/// The dealing of the arcs of a problem of `m` arcs and `n` nodes to their slots, in closed form: the slot of any one
/// arc, read at once.
///
/// It is the dealing [`DealtSlotsIter`] walks, the stride `s = ⌈m / n⌉`, at least one: with `m = q·s + r`, the
/// first `r` runs of slots, `0, s, 2s, …` from each residue, hold `q + 1` slots and the others `q`, and the arcs fill
/// the runs in turn.
///
/// # Invariant
/// `stride` is the stride of `arc_count` arcs and `node_count` nodes, at least one; `quotient` is `arc_count / stride`
/// and `remainder` is `arc_count % stride`; `arc_count` is at most [`INDEX_LIMIT`]. So [`slot_of`](Self::slot_of)
/// answers a slot for every arc below `arc_count` and for no other, and every slot it answers is below `arc_count`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SlotOrder {
    /// The number of arcs, `m`, the number of slots.
    arc_count: usize,
    /// The number of nodes, `n`.
    node_count: usize,
    /// The stride, `s`, at least one.
    stride: usize,
    /// The slots of a short run, `q = ⌊m / s⌋`.
    quotient: usize,
    /// The runs of `q + 1` slots, `r = m mod s`.
    remainder: usize,
}

impl SlotOrder {
    /// The dealing of the arcs of a problem of `arc_count` arcs and `node_count` nodes.
    ///
    /// # Panics
    /// Panics if `arc_count` passes [`INDEX_LIMIT`].
    pub(crate) const fn new(arc_count: usize, node_count: usize) -> Self {
        assert!(arc_count <= INDEX_LIMIT, "called `SlotOrder::new` with an arc count past the index limit");

        let stride = stride(arc_count, node_count);

        Self { arc_count, node_count, stride, quotient: arc_count / stride, remainder: arc_count % stride }
    }

    /// The number of arcs, `m`, the number of slots.
    pub(crate) const fn arc_count(&self) -> usize {
        self.arc_count
    }

    /// The number of nodes, `n`.
    pub(crate) const fn node_count(&self) -> usize {
        self.node_count
    }

    /// The slot `arc` is dealt to, below the arc count, where `arc` is below it; `None` past it. It is the slot
    /// [`DealtSlotsIter`] yields for the arc, at the cost of two divisions.
    pub(crate) fn slot_of(&self, arc: usize) -> Option<Slot> {
        (arc < self.arc_count).then(|| {
            debug_assert!(arc < self.arc_count, "the arc should be below the arc count");
            // SAFETY: `arc` is below the arc count, as just compared: the precondition of `slot_of_unchecked`.
            unsafe { self.slot_of_unchecked(arc) }
        })
    }

    /// The slot `arc` is dealt to, as [`slot_of`](Self::slot_of) answers it where it answers one, without the
    /// comparison with the arc count.
    ///
    /// # Safety
    /// `arc` is below the arc count: an arc for which `slot_of` answers `Some`. Past it, the slot answered may lie
    /// past the arc count, which every caller indexes a column of `m` slots with.
    pub(crate) unsafe fn slot_of_unchecked(&self, arc: usize) -> Slot {
        debug_assert!(
            arc < self.arc_count,
            "called `SlotOrder::slot_of_unchecked` with arc {arc}, which should be below the arc count {}",
            self.arc_count
        );

        // The first `r` runs hold `q + 1` slots each, `r·(q + 1)` arcs in all; the arcs past them fill runs of `q`.
        // Where `q` is zero every arc lies in the long runs, since then `r = m`, so the second branch never divides by
        // zero. Run `j` at place `k` is the slot `j + k·s`: below `m`, since the run holds exactly the slots of its
        // residue below `m`.
        let long = self.quotient + 1;
        let (run, place) = match arc.checked_sub(self.remainder * long) {
            None => (arc / long, arc % long),
            Some(past) => (self.remainder + past / self.quotient, past % self.quotient),
        };
        let slot = run + place * self.stride;

        // `slot < m ≤ INDEX_LIMIT = u32::MAX` (`SlotOrder`), so the narrowing is exact.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the slot is below the arc count, at most u32::MAX by the invariant `new` establishes"
        )]
        let raw = slot as u32;

        Slot(raw)
    }
}

/// For each arc `0..m` in ascending order, the slot it is dealt to.
///
/// The arcs are dealt with the stride `s = ⌈m / n⌉`, at least one: arc 0 goes to slot 0, the next arcs to
/// `s, 2s, …` below `m`, then on to `1, 1 + s, …`, and so on. Consecutive slots so hold arcs at least a node's mean
/// number of arcs apart in the arc list, and a stride of one keeps the arcs' order.
///
/// # Invariant
/// It yields every slot of `0..m` exactly once, so every slot it yields is below the arc count it was made for, and
/// `m` is at most [`INDEX_LIMIT`].
#[derive(Clone, Debug)]
pub(crate) struct DealtSlotsIter {
    /// The number of arcs whose slot is still to come.
    remaining: usize,
    /// The number of arcs, `m`, the number of slots.
    arc_count: usize,
    /// The stride, `s`, at least one.
    stride: usize,
    /// The first slot of the run of slots the dealing is in, `0` to `s − 1`.
    residue: usize,
    /// The slot the next arc goes to.
    next_slot: usize,
}

impl DealtSlotsIter {
    /// The slots of the arcs of a problem of `arc_count` arcs and `node_count` nodes.
    ///
    /// # Panics
    /// Panics if `arc_count` passes [`INDEX_LIMIT`].
    #[track_caller]
    pub(crate) fn new(arc_count: usize, node_count: usize) -> Self {
        assert!(arc_count <= INDEX_LIMIT, "called `DealtSlotsIter::new` with {arc_count} arcs, past {INDEX_LIMIT}");

        Self { remaining: arc_count, arc_count, stride: stride(arc_count, node_count), residue: 0, next_slot: 0 }
    }
}

impl Iterator for DealtSlotsIter {
    type Item = Slot;

    fn next(&mut self) -> Option<Slot> {
        self.remaining = self.remaining.checked_sub(1)?;
        let slot = self.next_slot;

        // While an arc remains, `slot < m`, so the difference is at least one and a step is taken only below `m`.
        // A run ends at the last slot of its residue; the next starts at the next residue, which is below `s ≤ m`
        // wherever an arc remains.
        if self.arc_count - slot > self.stride {
            self.next_slot = slot + self.stride;
        } else {
            self.residue += 1;
            self.next_slot = self.residue;
        }

        // `slot < m ≤ INDEX_LIMIT = u32::MAX` (`DealtSlotsIter`), so the narrowing is exact.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the slot is below the arc count, at most u32::MAX by the invariant `new` establishes"
        )]
        let raw = slot as u32;

        Some(Slot(raw))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for DealtSlotsIter {}

impl FusedIterator for DealtSlotsIter {}

/// The stride the arcs of a problem of `arc_count` arcs and `node_count` nodes are dealt with: `⌈arc_count /
/// node_count⌉`, at least one, a problem of no node taken as one of a single node.
const fn stride(arc_count: usize, node_count: usize) -> usize {
    // The block search prices blocks of consecutive slots and enters the best arc of the first block that holds a
    // violation, so a block should sample the whole network, not one corner of it. Problems come with their arcs
    // grouped by node, `m / n` of them a node on average. With a stride of at least that mean, rounded up,
    // consecutive slots hold arcs at least one node's worth of arcs apart in the arc list, so a block takes its arcs
    // from as many stretches of the list as it has rows. A larger stride buys a block nothing more and scatters the
    // potentials it reads over more of the tree. The rule takes the problem's own counts and no other constant. No
    // more arcs than nodes gives the stride one, and a single node the stride `m`, one slot to each residue: both keep
    // the arcs' order.
    //
    // Written with `if` for the two floors of one, since `Ord::max` is no `const fn`.
    let nodes = if node_count == 0 { 1 } else { node_count };
    let stride = arc_count.div_ceil(nodes);

    if stride == 0 { 1 } else { stride }
}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;

    /// The slot of each arc `0..arc_count` dealt with `stride`, written from the rule: the slots `0, s, 2s, …` below
    /// the arc count, then `1, 1 + s, …`, and so on, handed to the arcs in turn.
    pub(crate) fn dealt_by_rule(arc_count: usize, stride: usize) -> Vec<usize> {
        let mut slots = Vec::new();
        for residue in 0..stride {
            let mut slot = residue;
            while slot < arc_count {
                slots.push(slot);
                slot += stride;
            }
        }

        slots
    }

    /// The arc dealt to each slot of a problem of `arc_count` arcs and `node_count` nodes, slot `s` at `s`: the
    /// dealing inverted, as [`dealt_by_rule`] writes it with the stride `⌈m / n⌉`, at least one.
    pub(crate) fn arcs_of_slots(arc_count: usize, node_count: usize) -> Vec<usize> {
        let stride = arc_count.div_ceil(node_count.max(1)).max(1);
        let mut arcs = vec![0; arc_count];
        for (arc, slot) in dealt_by_rule(arc_count, stride).into_iter().enumerate() {
            arcs[slot] = arc;
        }

        arcs
    }

    /// The slot at `index`, for a test to write out.
    pub(crate) fn slot(index: usize) -> Slot {
        Slot::from(index)
    }

    /// The slots `DealtSlotsIter` deals the arcs of a problem of `arc_count` arcs and `node_count` nodes to.
    fn dealt(arc_count: usize, node_count: usize) -> Vec<usize> {
        DealtSlotsIter::new(arc_count, node_count).map(usize::from).collect()
    }

    #[test]
    fn test_the_dealing_inverted_names_the_arc_of_each_slot_by_hand() {
        // Ten arcs on four nodes go to the slots 0, 3, 6, 9, 1, 4, 7, 2, 5, 8.
        assert_eq!(arcs_of_slots(10, 4), [0, 4, 7, 1, 5, 8, 2, 6, 9, 3]);
        assert_eq!(arcs_of_slots(3, 5), [0, 1, 2], "a stride of one");
        assert_eq!(arcs_of_slots(0, 2), Vec::<usize>::new());
    }

    #[test]
    fn test_ten_arcs_on_four_nodes_are_dealt_three_apart_by_hand() {
        // The stride is ⌈10 / 4⌉ = 3: arcs 0 to 3 go to slots 0, 3, 6, 9; arcs 4 to 6 to 1, 4, 7; arcs 7 to 9 to
        // 2, 5, 8.
        assert_eq!(dealt(10, 4), [0, 3, 6, 9, 1, 4, 7, 2, 5, 8]);
    }

    #[test]
    fn test_a_stride_of_one_keeps_the_arcs_order_by_hand() {
        assert_eq!(dealt(4, 4), [0, 1, 2, 3]);
        assert_eq!(dealt(3, 5), [0, 1, 2], "fewer arcs than nodes");
        assert_eq!(dealt(2, 0), [0, 1], "no node");
    }

    #[test]
    fn test_no_arc_deals_no_slot() {
        let mut slots = DealtSlotsIter::new(0, 3);

        assert_eq!(slots.len(), 0);
        assert_eq!(slots.next(), None);
    }

    #[test]
    fn test_the_dealing_counts_down_and_stays_ended() {
        // Seven arcs on two nodes: the stride is 4, so arcs 0 and 1 go to slots 0 and 4, arcs 2 and 3 to 1 and 5.
        let mut slots = DealtSlotsIter::new(7, 2);
        assert_eq!(slots.len(), 7);

        assert_eq!(slots.next().map(usize::from), Some(0));
        assert_eq!(slots.next().map(usize::from), Some(4));
        assert_eq!(slots.next().map(usize::from), Some(1));
        assert_eq!(slots.len(), 4);

        assert_eq!(slots.by_ref().map(usize::from).collect::<Vec<_>>(), [5, 2, 6, 3]);
        assert_eq!((slots.next(), slots.next(), slots.len()), (None, None, 0));
    }

    #[test]
    fn test_the_dealing_accepts_the_index_limit() {
        // One node: the stride is the arc count, so every arc keeps its index as its slot.
        let mut slots = DealtSlotsIter::new(INDEX_LIMIT, 1);

        assert_eq!(slots.len(), INDEX_LIMIT);
        assert_eq!(slots.by_ref().take(3).map(usize::from).collect::<Vec<_>>(), [0, 1, 2]);
    }

    // An arc count past `u32::MAX` exists only where a `usize` has 64 bits.
    #[cfg(target_pointer_width = "64")]
    #[test]
    #[should_panic(expected = "called `DealtSlotsIter::new` with 4294967296 arcs, past 4294967295")]
    fn test_the_dealing_refuses_arcs_past_the_index_limit() {
        let _ = DealtSlotsIter::new(INDEX_LIMIT + 1, 1);
    }

    #[test]
    fn test_the_stride_is_the_mean_arcs_a_node_rounded_up_and_at_least_one_by_hand() {
        let cases =
            [((0, 0), 1), ((5, 0), 5), ((5, 1), 5), ((3, 3), 1), ((2, 5), 1), ((10, 4), 3), ((12, 4), 3), ((13, 4), 4)];

        for ((arc_count, node_count), expected) in cases {
            assert_eq!(stride(arc_count, node_count), expected, "{arc_count} arcs, {node_count} nodes");
        }
        assert_eq!(stride(usize::MAX, 1), usize::MAX);
        assert_eq!(stride(usize::MAX, usize::MAX), 1);
    }

    #[test]
    fn test_a_slot_converts_both_ways_and_names_itself() {
        assert_eq!(usize::from(Slot::from(7_usize)), 7);
        assert_eq!(usize::from(Slot::from(INDEX_LIMIT)), INDEX_LIMIT);
        assert_eq!((usize::from(Slot::from(7_u32)), usize::from(Slot::from(u32::MAX))), (7, INDEX_LIMIT));
        assert_eq!(Slot::from(9_u32), Slot::from(9_usize));
        assert_eq!(format!("{:?}", Slot::from(7_usize)), "Slot 7");
        assert!(Slot::from(3_usize) < Slot::from(4_usize));
    }

    // An index past `u32::MAX` exists only where a `usize` has 64 bits.
    #[cfg(target_pointer_width = "64")]
    #[test]
    #[should_panic(expected = "called `Slot::from` with 4294967296, past u32::MAX")]
    fn test_a_slot_past_u32_max_panics() {
        let _ = Slot::from(INDEX_LIMIT + 1);
    }

    /// The slot `order` deals each of its arcs to, in arc order, as numbers.
    fn slots_of(order: &SlotOrder) -> Vec<usize> {
        (0..order.arc_count()).map(|arc| order.slot_of(arc).map_or(usize::MAX, usize::from)).collect()
    }

    #[test]
    fn test_the_closed_form_deals_ten_arcs_on_four_nodes_three_apart_by_hand() {
        // The stride is ⌈10 / 4⌉ = 3 and `10 = 3·3 + 1`: one run of four slots, 0, 3, 6, 9, then two of three.
        let order = SlotOrder::new(10, 4);
        assert_eq!(order, SlotOrder { arc_count: 10, node_count: 4, stride: 3, quotient: 3, remainder: 1 });
        assert_eq!(slots_of(&order), [0, 3, 6, 9, 1, 4, 7, 2, 5, 8]);
        assert_eq!((order.arc_count(), order.node_count()), (10, 4));
    }

    #[test]
    fn test_the_closed_form_past_the_long_runs_by_hand() {
        // Seven arcs on two nodes: the stride is 4 and `7 = 1·4 + 3`, three runs of two slots and one of one. Arc 6
        // lies past the long runs' six arcs, in run `3 + 0 / 1` at place `0 mod 1`: slot 3.
        let order = SlotOrder::new(7, 2);
        assert_eq!((order.quotient, order.remainder), (1, 3));
        assert_eq!(slots_of(&order), [0, 4, 1, 5, 2, 6, 3]);

        // Four arcs on three nodes: the stride is 2 and `4 = 2·2 + 0`, no long run at all.
        assert_eq!(slots_of(&SlotOrder::new(4, 3)), [0, 2, 1, 3]);
        // One node: the stride is the arc count, five runs of one slot, every arc at its own index.
        assert_eq!(slots_of(&SlotOrder::new(5, 1)), [0, 1, 2, 3, 4]);
        // Fewer arcs than nodes: the stride is one, a single run in the arcs' order.
        assert_eq!(slots_of(&SlotOrder::new(3, 5)), [0, 1, 2]);
    }

    #[test]
    fn test_the_closed_form_answers_no_slot_past_the_arcs_by_hand() {
        const EMPTY: SlotOrder = SlotOrder::new(0, 0);
        assert_eq!(EMPTY, SlotOrder { arc_count: 0, node_count: 0, stride: 1, quotient: 0, remainder: 0 });
        assert_eq!(EMPTY.slot_of(0), None);

        let order = SlotOrder::new(10, 4);
        assert_eq!((order.slot_of(10), order.slot_of(usize::MAX)), (None, None));
        assert_eq!(order.slot_of(9).map(usize::from), Some(8));
    }

    #[test]
    fn test_the_closed_form_accepts_the_index_limit() {
        // One node: the stride is the arc count, so every arc keeps its index as its slot, the last one included.
        let order = SlotOrder::new(INDEX_LIMIT, 1);

        assert_eq!(order.slot_of(INDEX_LIMIT - 1).map(usize::from), Some(INDEX_LIMIT - 1));
        assert_eq!(order.slot_of(INDEX_LIMIT), None);
    }

    // An arc count past `u32::MAX` exists only where a `usize` has 64 bits.
    #[cfg(target_pointer_width = "64")]
    #[test]
    #[should_panic(expected = "called `SlotOrder::new` with an arc count past the index limit")]
    fn test_the_closed_form_refuses_arcs_past_the_index_limit() {
        let _ = SlotOrder::new(INDEX_LIMIT + 1, 1);
    }

    #[test]
    fn test_the_closed_form_is_the_dealing_walked_for_every_small_count() {
        // Every arc of every pair of counts below 61 arcs and 41 nodes: 75,030 arcs.
        let mut arcs = 0;
        for arc_count in 0..61 {
            for node_count in 0..41 {
                let order = SlotOrder::new(arc_count, node_count);
                assert_eq!(slots_of(&order), dealt(arc_count, node_count), "{arc_count} arcs, {node_count} nodes");
                assert_eq!(order.slot_of(arc_count), None);
                arcs += arc_count;
            }
        }

        assert_eq!(arcs, 75_030);
    }

    proptest! {
        #[test]
        fn test_slot_of_and_its_unchecked_twin_agree_below_the_arcs_and_slot_of_refuses_past_them(
            arc_count in 0_usize..3000,
            node_count in 0_usize..400,
            past in 0_usize..4,
        ) {
            let order = SlotOrder::new(arc_count, node_count);
            for arc in 0..arc_count {
                debug_assert!(arc < order.arc_count(), "the arc should be below the arc count");
                // SAFETY: `arc` is below the arc count.
                let unchecked = unsafe { order.slot_of_unchecked(arc) };
                prop_assert_eq!(order.slot_of(arc), Some(unchecked));
            }

            for arc in [arc_count + past, usize::MAX - past] {
                prop_assert_eq!(order.slot_of(arc), None);
            }
        }

        #[test]
        fn test_the_closed_form_is_the_dealing_walked(arc_count in 0_usize..3000, node_count in 0_usize..400) {
            let order = SlotOrder::new(arc_count, node_count);

            prop_assert_eq!(slots_of(&order), dealt(arc_count, node_count));
            prop_assert_eq!(order.slot_of(arc_count), None);
        }

        #[test]
        fn test_the_dealing_is_the_bijection_the_rule_writes(arc_count in 0_usize..300, node_count in 0_usize..80) {
            let slots = dealt(arc_count, node_count);
            prop_assert_eq!(&slots, &dealt_by_rule(arc_count, stride(arc_count, node_count)));

            let mut sorted = slots;
            sorted.sort_unstable();
            prop_assert!(sorted.into_iter().eq(0..arc_count), "every slot exactly once");
        }
    }
}
