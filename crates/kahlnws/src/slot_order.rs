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
fn stride(arc_count: usize, node_count: usize) -> usize {
    // The block search prices blocks of consecutive slots and enters the best arc of the first block that holds a
    // violation, so a block should sample the whole network, not one corner of it. Problems come with their arcs
    // grouped by node, `m / n` of them a node on average. With a stride of at least that mean, rounded up,
    // consecutive slots hold arcs at least one node's worth of arcs apart in the arc list, so a block takes its arcs
    // from as many stretches of the list as it has rows. A larger stride buys a block nothing more and scatters the
    // potentials it reads over more of the tree. The rule takes the problem's own counts and no other constant. No
    // more arcs than nodes gives the stride one, and a single node the stride `m`, one slot to each residue: both keep
    // the arcs' order.
    arc_count.div_ceil(node_count.max(1)).max(1)
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

    proptest! {
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
