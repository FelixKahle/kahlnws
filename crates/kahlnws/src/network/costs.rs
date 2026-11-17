// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The costs of the arcs of the problem a network holds, read arc by arc through the dealing of the arcs to their
//! slots.

use core::fmt;
use core::iter::FusedIterator;

use super::dealt_columns::{DealtColumns, DealtColumnsIter};

/// The cost of a unit of flow on each arc of the problem a [`Network`](crate::Network) holds, in units of cost, arc
/// `a`'s at `a`, as the problem stands after the changes made so far: a view, lent by
/// [`Network::costs`](crate::Network::costs), that reads each cost where it is asked for, in constant time.
///
/// [`get`](Self::get) answers one arc's cost and [`iter`](Self::iter) every arc's in ascending order; collecting the
/// iterator copies them out.
///
/// ```
/// use kahlnws::{Network, Problem};
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let mut network = Network::new();
///     network.load(&Problem {
///         tails: &[0, 0],
///         heads: &[1, 1],
///         lower_bounds: None,
///         capacities: &[2, 1],
///         costs: &[3, 1],
///         supplies: &[2, -2],
///     })?;
///     network.set_cost(0, 4)?;
///
///     let costs = network.costs();
///     assert_eq!((costs.len(), costs.get(0), costs.get(2)), (2, Some(4), None));
///     assert_eq!(costs.iter().collect::<Vec<i64>>(), [4, 1]);
///
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy)]
pub struct Costs<'n> {
    /// Per slot: the cost of the arc dealt to it, in units of cost, read through the dealing.
    costs: DealtColumns<'n, i64, 1>,
}

/// Every cost of a [`Costs`] view in ascending arc order.
#[derive(Clone, Debug)]
pub struct CostsIter<'n> {
    /// The costs still to come, in ascending arc order.
    costs: DealtColumnsIter<'n, i64, 1>,
}

impl<'n> Costs<'n> {
    /// The costs of no arc.
    const OF_NO_ARC: Self = Self { costs: DealtColumns::OF_NO_ARC };

    /// The view of the costs `costs` holds, by slot, through the dealing.
    pub(super) const fn new(costs: DealtColumns<'n, i64, 1>) -> Self {
        Self { costs }
    }

    /// The number of arcs, `m`.
    pub fn len(&self) -> usize {
        self.costs.len()
    }

    /// Whether there is no arc.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The cost of `arc`, in units of cost; `None` where `arc` is not below [`len`](Self::len). It costs two
    /// divisions and a read.
    pub fn get(&self, arc: usize) -> Option<i64> {
        self.costs.get(arc).map(|[cost]| cost)
    }

    /// The cost of `arc`, as [`get`](Self::get) answers it where it answers one, without the comparison with the arc
    /// count.
    ///
    /// # Safety
    /// `arc` is below [`len`](Self::len): an arc for which `get` answers `Some`.
    pub unsafe fn get_unchecked(&self, arc: usize) -> i64 {
        debug_assert!(
            arc < self.len(),
            "called `Costs::get_unchecked` with arc {arc}, which should be below the arc count {}",
            self.len()
        );
        // SAFETY: `arc` is below the arc count (the caller's promise): the precondition of
        // `DealtColumns::get_unchecked`.
        let [cost] = unsafe { self.costs.get_unchecked(arc) };
        cost
    }

    /// Every arc's cost, in units of cost, in ascending arc order. It costs O(m) in all.
    pub fn iter(&self) -> CostsIter<'n> {
        CostsIter { costs: self.costs.iter() }
    }
}

impl fmt::Debug for Costs<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Costs ")?;
        formatter.debug_list().entries(self).finish()
    }
}

impl PartialEq for Costs<'_> {
    /// Whether the two views answer the same cost for every arc, and have as many arcs.
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for Costs<'_> {}

impl Default for Costs<'_> {
    /// The costs of no arc.
    fn default() -> Self {
        Self::OF_NO_ARC
    }
}

impl<'n> IntoIterator for Costs<'n> {
    type Item = i64;
    type IntoIter = CostsIter<'n>;

    fn into_iter(self) -> CostsIter<'n> {
        self.iter()
    }
}

impl<'n> IntoIterator for &Costs<'n> {
    type Item = i64;
    type IntoIter = CostsIter<'n>;

    fn into_iter(self) -> CostsIter<'n> {
        self.iter()
    }
}

impl Iterator for CostsIter<'_> {
    type Item = i64;

    fn next(&mut self) -> Option<i64> {
        self.costs.next().map(|[cost]| cost)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.costs.size_hint()
    }
}

impl DoubleEndedIterator for CostsIter<'_> {
    fn next_back(&mut self) -> Option<i64> {
        self.costs.next_back().map(|[cost]| cost)
    }
}

impl ExactSizeIterator for CostsIter<'_> {}

impl FusedIterator for CostsIter<'_> {}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::network::dealt_columns::tests::dealt;
    use crate::slot_order::SlotOrder;

    /// Ten arcs on four nodes, dealt to the slots 0, 3, 6, 9, 1, 4, 7, 2, 5, 8: slot `s` holds the arc
    /// `[0, 4, 7, 1, 5, 8, 2, 6, 9, 3][s]`, whose cost is `−10·a`, by slot.
    const COSTS: [i64; 10] = [0, -40, -70, -10, -50, -80, -20, -60, -90, -30];

    /// The view of the ten arcs.
    fn ten_arcs() -> Costs<'static> {
        Costs::new(dealt([&COSTS[..]], SlotOrder::new(10, 4)))
    }

    #[test]
    fn test_the_costs_of_ten_arcs_read_through_their_slots_by_hand() {
        let costs = ten_arcs();

        assert_eq!((costs.len(), costs.is_empty()), (10, false));
        assert_eq!(costs.iter().collect::<Vec<_>>(), [0, -10, -20, -30, -40, -50, -60, -70, -80, -90]);
        assert_eq!(
            (costs.get(3), costs.get(9), costs.get(10), costs.get(usize::MAX)),
            (Some(-30), Some(-90), None, None)
        );
        debug_assert!(7 < costs.len(), "the index should be below its count, as the comment below says");
        // SAFETY: arc 7 is below the arc count 10.
        assert_eq!(unsafe { costs.get_unchecked(7) }, -70);
    }

    #[test]
    fn test_the_walk_of_the_costs_from_both_ends_by_hand() {
        let mut walk = ten_arcs().iter();
        assert_eq!((walk.len(), walk.next(), walk.next_back()), (10, Some(0), Some(-90)));
        assert_eq!((walk.next_back(), walk.next(), walk.len()), (Some(-80), Some(-10), 6));

        assert_eq!(walk.by_ref().rev().collect::<Vec<_>>(), [-70, -60, -50, -40, -30, -20]);
        assert_eq!((walk.next(), walk.next_back(), walk.len()), (None, None, 0));

        let by_reference: Vec<i64> = (&ten_arcs()).into_iter().collect();
        let by_value: Vec<i64> = ten_arcs().into_iter().collect();
        assert_eq!(by_reference, by_value);
    }

    #[test]
    fn test_the_costs_of_no_arc_and_their_equality_by_hand() {
        let none = Costs::OF_NO_ARC;
        assert_eq!((none.len(), none.is_empty(), none.get(0), none.iter().next()), (0, true, None, None));
        assert_eq!((Costs::default(), format!("{:?}", Costs::default())), (none, "Costs []".to_owned()));

        // The same costs of two arcs on one node and on two, dealt alike; another cost; fewer arcs.
        let first = Costs::new(dealt([&[1, -2][..]], SlotOrder::new(2, 1)));
        assert_eq!(first, Costs::new(dealt([&[1, -2][..]], SlotOrder::new(2, 2))));
        assert_ne!(first, Costs::new(dealt([&[1, 2][..]], SlotOrder::new(2, 1))));
        assert_ne!(first, Costs::new(dealt([&[1][..]], SlotOrder::new(1, 1))), "fewer arcs");
        assert_eq!(format!("{first:?}"), "Costs [1, -2]");
    }

    proptest! {
        #[test]
        fn test_get_and_get_unchecked_answer_alike_and_get_refuses_past_the_arcs(
            (costs, node_count) in (proptest::collection::vec(-1000_i64..1000, 0..60), 0_usize..12),
        ) {
            let order = SlotOrder::new(costs.len(), node_count);
            let view = Costs::new(dealt([&costs[..]], order));

            for arc in 0..view.len() {
                let slot = order.slot_of(arc).map(usize::from).expect("an arc below the count should have a slot");
                debug_assert!(arc < view.len(), "the index should be below its count, as the comment below says");
                // SAFETY: `arc` is below the arc count.
                let unchecked = unsafe { view.get_unchecked(arc) };
                prop_assert_eq!(view.get(arc), Some(unchecked));
                prop_assert_eq!(unchecked, costs[slot]);
            }
            for arc in [view.len(), view.len() + 1, usize::MAX] {
                prop_assert_eq!(view.get(arc), None);
            }
            prop_assert_eq!(view.iter().len(), view.len());
            prop_assert!(view.iter().eq((0..view.len()).filter_map(|arc| view.get(arc))));
        }
    }
}
