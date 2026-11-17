// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The flows of a network's optimum, read arc by arc through the dealing of the arcs to their slots.

use core::fmt;
use core::iter::FusedIterator;

use super::dealt_columns::{DealtColumns, DealtColumnsIter};

/// The flow of each arc of a network's optimum, in units of flow, its lower bound included, arc `a`'s at `a`: a view
/// that reads each flow where it is asked for, in constant time.
///
/// [`get`](Self::get) answers one arc's flow and [`iter`](Self::iter) every arc's in ascending order; collecting the
/// iterator, or [`Solution::from`](crate::Solution) the optimum, copies them out.
///
/// ```
/// use kahlnws::{Network, Problem, ProblemError};
///
/// fn main() -> Result<(), ProblemError> {
///     // Node 0 sends 2 units to node 1 over two parallel arcs; the cheaper one carries 1.
///     let problem = Problem {
///         tails: &[0, 0],
///         heads: &[1, 1],
///         lower_bounds: None,
///         capacities: &[2, 1],
///         costs: &[3, 1],
///         supplies: &[2, -2],
///     };
///
///     let mut network = Network::new();
///     network.load(&problem)?;
///     network.solve();
///     let flows = network.optimum().expect("two units should fit the two arcs").flows;
///     assert_eq!((flows.len(), flows.get(1), flows.get(2)), (2, Some(1), None));
///     assert_eq!(flows.iter().collect::<Vec<u64>>(), [1, 1]);
///
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy)]
pub struct Flows<'n> {
    /// Per slot: what the arc dealt to it carries above its lower bound, and its lower bound, in units of flow, read
    /// together through the dealing.
    flows: DealtColumns<'n, u64, 2>,
}

/// Every flow of a [`Flows`] view in ascending arc order, each with its arc's lower bound.
#[derive(Clone, Debug)]
pub struct FlowsIter<'n> {
    /// The flows and lower bounds of the arcs still to come, in ascending arc order.
    flows: DealtColumnsIter<'n, u64, 2>,
}

impl<'n> Flows<'n> {
    /// The flows of no arc.
    pub(super) const OF_NO_ARC: Self = Self { flows: DealtColumns::OF_NO_ARC };

    /// The view of the flows of the arcs whose flows above their lower bounds and lower bounds `flows` holds, in that
    /// order, by slot, through the dealing.
    pub(super) const fn new(flows: DealtColumns<'n, u64, 2>) -> Self {
        Self { flows }
    }

    /// The number of arcs, `m`.
    pub fn len(&self) -> usize {
        self.flows.len()
    }

    /// Whether there is no arc.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The flow of `arc`, in units of flow, its lower bound included; `None` where `arc` is not below
    /// [`len`](Self::len). It costs two divisions and two reads.
    ///
    /// # Panics
    /// Panics if the flow and the lower bound add up past `u64::MAX`, which no flow of a network's optimum does: it
    /// lies within its arc's capacity.
    #[doc(alias = "flow")]
    pub fn get(&self, arc: usize) -> Option<u64> {
        self.flows.get(arc).map(whole_flow)
    }

    /// The flow of `arc`, as [`get`](Self::get) answers it where it answers one, without the comparison with the arc
    /// count.
    ///
    /// # Panics
    /// As [`get`](Self::get).
    ///
    /// # Safety
    /// `arc` is below [`len`](Self::len): an arc for which `get` answers `Some`.
    pub unsafe fn get_unchecked(&self, arc: usize) -> u64 {
        debug_assert!(
            arc < self.len(),
            "called `Flows::get_unchecked` with arc {arc}, which should be below the arc count {}",
            self.len()
        );
        // SAFETY: `arc` is below the arc count (the caller's promise): the precondition of
        // `DealtColumns::get_unchecked`.
        whole_flow(unsafe { self.flows.get_unchecked(arc) })
    }

    /// Every arc's flow, in units of flow, its lower bound included, in ascending arc order. It costs O(m) in all.
    pub fn iter(&self) -> FlowsIter<'n> {
        FlowsIter { flows: self.flows.iter() }
    }
}

impl fmt::Debug for Flows<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Flows ")?;
        formatter.debug_list().entries(self).finish()
    }
}

impl PartialEq for Flows<'_> {
    /// Whether the two views answer the same flow for every arc, and have as many arcs.
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for Flows<'_> {}

impl Default for Flows<'_> {
    /// The flows of no arc.
    fn default() -> Self {
        Self::OF_NO_ARC
    }
}

impl<'n> IntoIterator for Flows<'n> {
    type Item = u64;
    type IntoIter = FlowsIter<'n>;

    fn into_iter(self) -> FlowsIter<'n> {
        self.iter()
    }
}

impl<'n> IntoIterator for &Flows<'n> {
    type Item = u64;
    type IntoIter = FlowsIter<'n>;

    fn into_iter(self) -> FlowsIter<'n> {
        self.iter()
    }
}

impl Iterator for FlowsIter<'_> {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        self.flows.next().map(whole_flow)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.flows.size_hint()
    }
}

impl DoubleEndedIterator for FlowsIter<'_> {
    fn next_back(&mut self) -> Option<u64> {
        self.flows.next_back().map(whole_flow)
    }
}

impl ExactSizeIterator for FlowsIter<'_> {}

impl FusedIterator for FlowsIter<'_> {}

/// The flow of an arc that carries `flow` above its lower bound `lower_bound`: the two added up.
///
/// # Panics
/// Panics if the two add up past `u64::MAX`, which no flow of a network's optimum does: it lies within its arc's
/// capacity.
fn whole_flow([flow, lower_bound]: [u64; 2]) -> u64 {
    flow.strict_add(lower_bound)
}

#[cfg(test)]
pub(super) mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::network::dealt_columns::tests::dealt;
    use crate::slot_order::SlotOrder;

    /// Ten arcs on four nodes, dealt to the slots 0, 3, 6, 9, 1, 4, 7, 2, 5, 8: slot `s` holds the arc
    /// `[0, 4, 7, 1, 5, 8, 2, 6, 9, 3][s]`. Arc `a` carries `a` above a lower bound of `10·a`, both by slot.
    const FLOWS: [u64; 10] = [0, 4, 7, 1, 5, 8, 2, 6, 9, 3];
    /// The lower bounds of the ten arcs, by slot.
    const LOWER_BOUNDS: [u64; 10] = [0, 40, 70, 10, 50, 80, 20, 60, 90, 30];

    /// The view of the ten arcs.
    pub(in crate::network) fn ten_arcs() -> Flows<'static> {
        Flows::new(dealt([&FLOWS[..], &LOWER_BOUNDS[..]], SlotOrder::new(10, 4)))
    }

    #[test]
    fn test_the_flows_of_ten_arcs_read_through_their_slots_by_hand() {
        let flows = ten_arcs();

        assert_eq!((flows.len(), flows.is_empty()), (10, false));
        assert_eq!(flows.iter().collect::<Vec<_>>(), [0, 11, 22, 33, 44, 55, 66, 77, 88, 99]);
        assert_eq!(
            (flows.get(3), flows.get(9), flows.get(10), flows.get(usize::MAX)),
            (Some(33), Some(99), None, None)
        );
        debug_assert!(7 < flows.len(), "the index should be below its count, as the comment below says");
        // SAFETY: arc 7 is below the arc count 10.
        assert_eq!(unsafe { flows.get_unchecked(7) }, 77);
    }

    #[test]
    fn test_the_walk_of_the_flows_from_both_ends_by_hand() {
        let mut walk = ten_arcs().iter();
        assert_eq!((walk.len(), walk.next(), walk.next_back()), (10, Some(0), Some(99)));
        assert_eq!((walk.next_back(), walk.next(), walk.len()), (Some(88), Some(11), 6));

        assert_eq!(walk.by_ref().rev().collect::<Vec<_>>(), [77, 66, 55, 44, 33, 22]);
        assert_eq!((walk.next(), walk.next_back(), walk.len()), (None, None, 0));

        let by_reference: Vec<u64> = (&ten_arcs()).into_iter().collect();
        let by_value: Vec<u64> = ten_arcs().into_iter().collect();
        assert_eq!(by_reference, by_value);
    }

    #[test]
    fn test_flows_are_equal_where_every_arc_carries_the_same_by_hand() {
        // The same flows of two arcs on one node, split otherwise between the lower bound and the flow above it.
        let order = SlotOrder::new(2, 1);
        let (first, second) = (
            Flows::new(dealt([&[1, 0][..], &[0, 5][..]], order)),
            Flows::new(dealt([&[0, 2][..], &[1, 3][..]], order)),
        );
        assert_eq!(first, second);
        assert_ne!(first, Flows::new(dealt([&[0, 2][..], &[1, 4][..]], order)));
        assert_ne!(first, Flows::new(dealt([&[1][..], &[0][..]], SlotOrder::new(1, 1))), "fewer arcs");
        assert_eq!(format!("{first:?}"), "Flows [1, 5]");
    }

    #[test]
    #[should_panic(expected = "attempt to add with overflow")]
    fn test_a_flow_past_u64_max_with_its_lower_bound_panics() {
        let flows = Flows::new(dealt([&[u64::MAX][..], &[1][..]], SlotOrder::new(1, 1)));
        let _ = flows.get(0);
    }

    proptest! {
        #[test]
        fn test_get_and_get_unchecked_answer_alike_and_get_refuses_past_the_arcs(
            (columns, node_count) in (proptest::collection::vec((0_u64..1000, 0_u64..1000), 0..60), 0_usize..12),
        ) {
            let (flows, lower_bounds): (Vec<u64>, Vec<u64>) = columns.into_iter().unzip();
            let order = SlotOrder::new(flows.len(), node_count);
            let view = Flows::new(dealt([&flows[..], &lower_bounds[..]], order));

            for arc in 0..view.len() {
                let slot = order.slot_of(arc).map(usize::from).expect("an arc below the count should have a slot");
                debug_assert!(arc < view.len(), "the index should be below its count, as the comment below says");
                // SAFETY: `arc` is below the arc count.
                let unchecked = unsafe { view.get_unchecked(arc) };
                prop_assert_eq!(view.get(arc), Some(unchecked));
                prop_assert_eq!(unchecked, flows[slot] + lower_bounds[slot]);
            }
            for arc in [view.len(), view.len() + 1, usize::MAX] {
                prop_assert_eq!(view.get(arc), None);
            }
            prop_assert_eq!(view.iter().len(), view.len());
            prop_assert!(view.iter().eq((0..view.len()).filter_map(|arc| view.get(arc))));
        }
    }
}
