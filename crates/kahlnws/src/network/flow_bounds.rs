// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The lower bounds or the capacities of the arcs of the problem a network holds, read arc by arc through the dealing
//! of the arcs to their slots.

use core::fmt;
use core::iter::FusedIterator;

use super::bounds::Bounds;
use super::dealt_columns::{DealtColumns, DealtColumnsIter};

/// The lower bound of each arc of the problem a [`Network`](crate::Network) holds, the least flow it carries, or the
/// capacity of each, the most, in units of flow, arc `a`'s at `a`, as the problem stands after the changes made so
/// far: a view, lent by [`Network::lower_bounds`](crate::Network::lower_bounds) or
/// [`Network::capacities`](crate::Network::capacities), that reads each bound where it is asked for, in constant time.
///
/// [`get`](Self::get) answers one arc's bound and [`iter`](Self::iter) every arc's in ascending order; collecting the
/// iterator copies them out, as a [`Problem`](crate::Problem)'s column of the same name holds them.
///
/// ```
/// use kahlnws::{Network, Problem};
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let mut network = Network::new();
///     network.load(&Problem {
///         tails: &[0, 0],
///         heads: &[1, 1],
///         lower_bounds: Some(&[1, 0]),
///         capacities: &[2, 1],
///         costs: &[3, 1],
///         supplies: &[2, -2],
///     })?;
///     network.set_capacity(1, 4)?;
///
///     let (lower_bounds, capacities) = (network.lower_bounds(), network.capacities());
///     assert_eq!((lower_bounds.get(1), capacities.get(1), capacities.get(2)), (Some(0), Some(4), None));
///     assert_eq!(capacities.iter().collect::<Vec<u64>>(), [2, 4]);
///
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy)]
pub struct FlowBounds<'n> {
    /// Per slot: the lower bound and the width, its capacity less its lower bound, of the arc dealt to it, in units of
    /// flow, read together through the dealing.
    bounds: DealtColumns<'n, u64, 2>,
    /// Which of the two bounds the view reads.
    bound: Bound,
}

/// One of the two bounds of an arc's flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    /// The least flow of the arc.
    Lower,
    /// The most flow of the arc.
    Capacity,
}

/// Every bound of a [`FlowBounds`] view in ascending arc order.
#[derive(Clone, Debug)]
pub struct FlowBoundsIter<'n> {
    /// The lower bounds and widths of the arcs still to come, in ascending arc order.
    bounds: DealtColumnsIter<'n, u64, 2>,
    /// Which of the two bounds the walk reads.
    bound: Bound,
}

impl Bound {
    /// This bound of an arc whose lower bound and width are `lower_bound` and `width`, in units of flow.
    ///
    /// # Panics
    /// Panics if the two add up past `u64::MAX` and this is the capacity, which no arc of a network has: they add up
    /// to its capacity.
    fn of(self, [lower_bound, width]: [u64; 2]) -> u64 {
        match self {
            Self::Lower => lower_bound,
            Self::Capacity => Bounds::of_width(lower_bound, width).capacity,
        }
    }

    /// The name of the column this bound is read from, in a view's `Debug`: the field of a `Problem` it is.
    const fn column(self) -> &'static str {
        match self {
            Self::Lower => "lower_bounds",
            Self::Capacity => "capacities",
        }
    }
}

impl<'n> FlowBounds<'n> {
    /// The bounds of no arc.
    const OF_NO_ARC: Self = Self { bounds: DealtColumns::OF_NO_ARC, bound: Bound::Lower };

    /// The view of the lower bounds of the arcs whose lower bounds and widths `bounds` holds, in that order, by slot,
    /// through the dealing.
    pub(super) const fn lower_bounds(bounds: DealtColumns<'n, u64, 2>) -> Self {
        Self { bounds, bound: Bound::Lower }
    }

    /// The view of the capacities of the arcs whose lower bounds and widths `bounds` holds, in that order, by slot,
    /// through the dealing.
    pub(super) const fn capacities(bounds: DealtColumns<'n, u64, 2>) -> Self {
        Self { bounds, bound: Bound::Capacity }
    }

    /// The number of arcs, `m`.
    pub fn len(&self) -> usize {
        self.bounds.len()
    }

    /// Whether there is no arc.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The bound of `arc` the view reads, in units of flow; `None` where `arc` is not below [`len`](Self::len). It
    /// costs two divisions and two reads.
    ///
    /// # Panics
    /// Panics if the lower bound and the width add up past `u64::MAX` where the view reads capacities, which no arc of
    /// a network does: they add up to its capacity.
    pub fn get(&self, arc: usize) -> Option<u64> {
        self.bounds.get(arc).map(|row| self.bound.of(row))
    }

    /// The bound of `arc`, as [`get`](Self::get) answers it where it answers one, without the comparison with the arc
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
            "called `FlowBounds::get_unchecked` with arc {arc}, which should be below the arc count {}",
            self.len()
        );
        // SAFETY: `arc` is below the arc count (the caller's promise): the precondition of
        // `DealtColumns::get_unchecked`.
        let row = unsafe { self.bounds.get_unchecked(arc) };
        self.bound.of(row)
    }

    /// Every arc's bound the view reads, in units of flow, in ascending arc order. It costs O(m) in all.
    pub fn iter(&self) -> FlowBoundsIter<'n> {
        FlowBoundsIter { bounds: self.bounds.iter(), bound: self.bound }
    }
}

impl fmt::Debug for FlowBounds<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "FlowBounds {} ", self.bound.column())?;
        formatter.debug_list().entries(self).finish()
    }
}

impl PartialEq for FlowBounds<'_> {
    /// Whether the two views answer the same number for every arc, and have as many arcs.
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for FlowBounds<'_> {}

impl Default for FlowBounds<'_> {
    /// The bounds of no arc.
    fn default() -> Self {
        Self::OF_NO_ARC
    }
}

impl<'n> IntoIterator for FlowBounds<'n> {
    type Item = u64;
    type IntoIter = FlowBoundsIter<'n>;

    fn into_iter(self) -> FlowBoundsIter<'n> {
        self.iter()
    }
}

impl<'n> IntoIterator for &FlowBounds<'n> {
    type Item = u64;
    type IntoIter = FlowBoundsIter<'n>;

    fn into_iter(self) -> FlowBoundsIter<'n> {
        self.iter()
    }
}

impl Iterator for FlowBoundsIter<'_> {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        self.bounds.next().map(|row| self.bound.of(row))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.bounds.size_hint()
    }
}

impl DoubleEndedIterator for FlowBoundsIter<'_> {
    fn next_back(&mut self) -> Option<u64> {
        self.bounds.next_back().map(|row| self.bound.of(row))
    }
}

impl ExactSizeIterator for FlowBoundsIter<'_> {}

impl FusedIterator for FlowBoundsIter<'_> {}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::network::dealt_columns::tests::dealt;
    use crate::slot_order::SlotOrder;

    /// Ten arcs on four nodes, dealt to the slots 0, 3, 6, 9, 1, 4, 7, 2, 5, 8: slot `s` holds the arc
    /// `[0, 4, 7, 1, 5, 8, 2, 6, 9, 3][s]`. Arc `a` has a lower bound of `10·a` and a width of `a`, both by slot.
    const LOWER_BOUNDS: [u64; 10] = [0, 40, 70, 10, 50, 80, 20, 60, 90, 30];
    /// The widths of the ten arcs, by slot.
    const WIDTHS: [u64; 10] = [0, 4, 7, 1, 5, 8, 2, 6, 9, 3];

    #[test]
    fn test_the_bounds_of_ten_arcs_read_through_their_slots_by_hand() {
        let order = SlotOrder::new(10, 4);
        let lower_bounds = FlowBounds::lower_bounds(dealt([&LOWER_BOUNDS[..], &WIDTHS[..]], order));
        let capacities = FlowBounds::capacities(dealt([&LOWER_BOUNDS[..], &WIDTHS[..]], order));

        assert_eq!((lower_bounds.len(), capacities.is_empty()), (10, false));
        assert_eq!(lower_bounds.iter().collect::<Vec<_>>(), (0..10).map(|arc| 10 * arc).collect::<Vec<_>>());
        assert_eq!(capacities.iter().collect::<Vec<_>>(), (0..10).map(|arc| 11 * arc).collect::<Vec<_>>());
        assert_eq!((lower_bounds.get(7), capacities.get(7), capacities.get(10)), (Some(70), Some(77), None));
        debug_assert!(
            6 < lower_bounds.len() && 6 < capacities.len(),
            "the index should be below its count, as the comment below says"
        );
        // SAFETY: arc 6 is below the arc count 10.
        assert_eq!(unsafe { (lower_bounds.get_unchecked(6), capacities.get_unchecked(6)) }, (60, 66));

        let mut walk = capacities.iter();
        assert_eq!((walk.len(), walk.next(), walk.next_back()), (10, Some(0), Some(99)));
        assert_eq!(walk.by_ref().rev().collect::<Vec<_>>(), (1..9).rev().map(|arc| 11 * arc).collect::<Vec<_>>());
        assert_eq!((walk.next(), walk.next_back(), walk.len()), (None, None, 0));
        let by_reference: Vec<u64> = (&lower_bounds).into_iter().collect();
        assert_eq!(by_reference, lower_bounds.into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn test_the_bounds_of_no_arc_and_their_equality_by_hand() {
        let none = FlowBounds::OF_NO_ARC;
        assert_eq!((none.len(), none.is_empty(), none.get(0), none.iter().next()), (0, true, None, None));
        assert_eq!(format!("{:?}", FlowBounds::default()), "FlowBounds lower_bounds []");

        // Arc 0 of bounds `1..=1`, arc 1 of `0..=2`: the lower bounds and the capacities differ, and each view prints
        // the column it reads.
        let order = SlotOrder::new(2, 1);
        let (lower_bounds, capacities) = (
            FlowBounds::lower_bounds(dealt([&[1, 0][..], &[0, 2][..]], order)),
            FlowBounds::capacities(dealt([&[1, 0][..], &[0, 2][..]], order)),
        );
        assert_eq!(
            (format!("{lower_bounds:?}"), format!("{capacities:?}")),
            ("FlowBounds lower_bounds [1, 0]".to_owned(), "FlowBounds capacities [1, 2]".to_owned())
        );
        assert_ne!(lower_bounds, capacities);
        assert_eq!(
            capacities,
            FlowBounds::lower_bounds(dealt([&[1, 2][..], &[5, 5][..]], order)),
            "the same numbers read alike"
        );
    }

    #[test]
    #[should_panic(expected = "attempt to add with overflow")]
    fn test_a_capacity_past_u64_max_panics() {
        let _ = FlowBounds::capacities(dealt([&[1][..], &[u64::MAX][..]], SlotOrder::new(1, 1))).get(0);
    }

    proptest! {
        #[test]
        fn test_get_and_get_unchecked_answer_alike_and_get_refuses_past_the_arcs(
            (columns, node_count) in (proptest::collection::vec((0_u64..1000, 0_u64..1000), 0..60), 0_usize..12),
        ) {
            let (lower_bounds, widths): (Vec<u64>, Vec<u64>) = columns.into_iter().unzip();
            let order = SlotOrder::new(lower_bounds.len(), node_count);
            let views = [
                FlowBounds::lower_bounds(dealt([&lower_bounds[..], &widths[..]], order)),
                FlowBounds::capacities(dealt([&lower_bounds[..], &widths[..]], order)),
            ];

            for (view, widths_added) in views.iter().zip([0, 1]) {
                for arc in 0..view.len() {
                    let slot = order.slot_of(arc).map(usize::from).expect("an arc below the count should have a slot");
                    debug_assert!(arc < view.len(), "the index should be below its count, as the comment below says");
                    // SAFETY: `arc` is below the arc count.
                    let unchecked = unsafe { view.get_unchecked(arc) };
                    prop_assert_eq!(view.get(arc), Some(unchecked));
                    prop_assert_eq!(unchecked, lower_bounds[slot] + widths_added * widths[slot]);
                }
                for arc in [view.len(), view.len() + 1, usize::MAX] {
                    prop_assert_eq!(view.get(arc), None);
                }
                prop_assert!(view.iter().eq((0..view.len()).filter_map(|arc| view.get(arc))));
                prop_assert!(view.iter().rev().eq((0..view.len()).rev().filter_map(|arc| view.get(arc))));
            }
        }
    }
}
