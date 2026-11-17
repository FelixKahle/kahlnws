// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tails or the heads of the arcs of the problem a network holds, read arc by arc through the dealing of the arcs
//! to their slots.

use core::fmt;
use core::iter::FusedIterator;

use super::dealt_columns::{DealtColumns, DealtColumnsIter};
use crate::basis::Ends;

/// The tail of each arc of the problem a [`Network`](crate::Network) holds, the node it leaves, or the head of each,
/// the node it enters, arc `a`'s at `a`: a view, lent by [`Network::tails`](crate::Network::tails) or
/// [`Network::heads`](crate::Network::heads), that reads each end where it is asked for, in constant time. No change
/// moves an arc's ends: they are those the problem was loaded with.
///
/// [`get`](Self::get) answers one arc's end and [`iter`](Self::iter) every arc's in ascending order; collecting the
/// iterator copies them out.
///
/// ```
/// use kahlnws::{Network, Problem};
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let mut network = Network::new();
///     network.load(&Problem {
///         tails: &[0, 1],
///         heads: &[1, 2],
///         lower_bounds: None,
///         capacities: &[3, 3],
///         costs: &[1, 1],
///         supplies: &[3, 0, -3],
///     })?;
///
///     let (tails, heads) = (network.tails(), network.heads());
///     assert_eq!((tails.get(1), heads.get(1), heads.get(2)), (Some(1), Some(2), None));
///     assert_eq!(tails.iter().zip(heads).collect::<Vec<_>>(), [(0, 1), (1, 2)]);
///
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy)]
pub struct ArcEnds<'n> {
    /// Per slot: the tail and the head of the arc dealt to it, read through the dealing.
    ends: DealtColumns<'n, Ends, 1>,
    /// Which of the two ends the view reads.
    end: End,
}

/// One of the two ends of an arc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    /// The node the arc leaves.
    Tail,
    /// The node the arc enters.
    Head,
}

/// Every end of an [`ArcEnds`] view in ascending arc order.
#[derive(Clone, Debug)]
pub struct ArcEndsIter<'n> {
    /// The ends of the arcs still to come, in ascending arc order.
    ends: DealtColumnsIter<'n, Ends, 1>,
    /// Which of the two ends the walk reads.
    end: End,
}

impl End {
    /// This end of an arc whose ends are `ends`.
    fn of(self, ends: Ends) -> usize {
        match self {
            Self::Tail => ends.tail(),
            Self::Head => ends.head(),
        }
    }

    /// The word for the ends this end is one of, in a view's `Debug`.
    const fn plural(self) -> &'static str {
        match self {
            Self::Tail => "tails",
            Self::Head => "heads",
        }
    }
}

impl<'n> ArcEnds<'n> {
    /// The ends of no arc.
    const OF_NO_ARC: Self = Self { ends: DealtColumns::OF_NO_ARC, end: End::Tail };

    /// The view of the tails of the arcs whose ends `ends` holds, by slot, through the dealing.
    pub(super) const fn tails(ends: DealtColumns<'n, Ends, 1>) -> Self {
        Self { ends, end: End::Tail }
    }

    /// The view of the heads of the arcs whose ends `ends` holds, by slot, through the dealing.
    pub(super) const fn heads(ends: DealtColumns<'n, Ends, 1>) -> Self {
        Self { ends, end: End::Head }
    }

    /// The number of arcs, `m`.
    pub fn len(&self) -> usize {
        self.ends.len()
    }

    /// Whether there is no arc.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The end of `arc` the view reads, a node; `None` where `arc` is not below [`len`](Self::len). It costs two
    /// divisions and a read.
    pub fn get(&self, arc: usize) -> Option<usize> {
        self.ends.get(arc).map(|[ends]| self.end.of(ends))
    }

    /// The end of `arc`, as [`get`](Self::get) answers it where it answers one, without the comparison with the arc
    /// count.
    ///
    /// # Safety
    /// `arc` is below [`len`](Self::len): an arc for which `get` answers `Some`.
    pub unsafe fn get_unchecked(&self, arc: usize) -> usize {
        debug_assert!(
            arc < self.len(),
            "called `ArcEnds::get_unchecked` with arc {arc}, which should be below the arc count {}",
            self.len()
        );
        // SAFETY: `arc` is below the arc count (the caller's promise): the precondition of
        // `DealtColumns::get_unchecked`.
        let [ends] = unsafe { self.ends.get_unchecked(arc) };
        self.end.of(ends)
    }

    /// Every arc's end the view reads, in ascending arc order. It costs O(m) in all.
    pub fn iter(&self) -> ArcEndsIter<'n> {
        ArcEndsIter { ends: self.ends.iter(), end: self.end }
    }
}

impl fmt::Debug for ArcEnds<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ArcEnds {} ", self.end.plural())?;
        formatter.debug_list().entries(self).finish()
    }
}

impl PartialEq for ArcEnds<'_> {
    /// Whether the two views answer the same node for every arc, and have as many arcs.
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for ArcEnds<'_> {}

impl Default for ArcEnds<'_> {
    /// The ends of no arc.
    fn default() -> Self {
        Self::OF_NO_ARC
    }
}

impl<'n> IntoIterator for ArcEnds<'n> {
    type Item = usize;
    type IntoIter = ArcEndsIter<'n>;

    fn into_iter(self) -> ArcEndsIter<'n> {
        self.iter()
    }
}

impl<'n> IntoIterator for &ArcEnds<'n> {
    type Item = usize;
    type IntoIter = ArcEndsIter<'n>;

    fn into_iter(self) -> ArcEndsIter<'n> {
        self.iter()
    }
}

impl Iterator for ArcEndsIter<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        self.ends.next().map(|[ends]| self.end.of(ends))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ends.size_hint()
    }
}

impl DoubleEndedIterator for ArcEndsIter<'_> {
    fn next_back(&mut self) -> Option<usize> {
        self.ends.next_back().map(|[ends]| self.end.of(ends))
    }
}

impl ExactSizeIterator for ArcEndsIter<'_> {}

impl FusedIterator for ArcEndsIter<'_> {}

#[cfg(test)]
mod tests {
    use kahlnws_testing::MincostNetwork;
    use proptest::prelude::*;

    use super::*;
    use crate::basis::{KeptBasisRoom, kept_star};
    use crate::network::dealt_columns::tests::dealt;
    use crate::problem::tests::problem_of;
    use crate::slot_order::SlotOrder;

    /// Ten arcs on four nodes, arc `a` from node `a mod 4` to node `(a + 1) mod 4`, dealt to the slots 0, 3, 6, 9, 1,
    /// 4, 7, 2, 5, 8, loaded into `room`; answers the ends column, by slot, and the dealing.
    fn ten_arcs(room: &mut KeptBasisRoom) -> (&[Ends], SlotOrder) {
        let arcs: Vec<(usize, usize, u64, u64, i64)> = (0..10).map(|arc| (arc % 4, (arc + 1) % 4, 0, 1, 1)).collect();
        let network = MincostNetwork::from_arcs(&[0; 4], &arcs);
        kept_star(room, &problem_of(&network));
        let ends = room.kept_problem().expect("the room should hold the basis just kept").ends;

        (ends, SlotOrder::new(10, 4))
    }

    #[test]
    fn test_the_tails_and_heads_of_ten_arcs_read_through_their_slots_by_hand() {
        let mut room = KeptBasisRoom::new();
        let (ends, order) = ten_arcs(&mut room);
        let (tails, heads) = (ArcEnds::tails(dealt([ends], order)), ArcEnds::heads(dealt([ends], order)));

        assert_eq!((tails.len(), heads.is_empty()), (10, false));
        assert_eq!(tails.iter().collect::<Vec<_>>(), [0, 1, 2, 3, 0, 1, 2, 3, 0, 1]);
        assert_eq!(heads.iter().collect::<Vec<_>>(), [1, 2, 3, 0, 1, 2, 3, 0, 1, 2]);
        assert_eq!((tails.get(7), heads.get(7), tails.get(10), heads.get(usize::MAX)), (Some(3), Some(0), None, None));
        debug_assert!(
            6 < tails.len() && 6 < heads.len(),
            "the index should be below its count, as the comment below says"
        );
        // SAFETY: arc 6 is below the arc count 10.
        assert_eq!(unsafe { (tails.get_unchecked(6), heads.get_unchecked(6)) }, (2, 3));
    }

    #[test]
    fn test_the_walk_of_the_ends_from_both_ends_by_hand() {
        let mut room = KeptBasisRoom::new();
        let (ends, order) = ten_arcs(&mut room);
        let mut walk = ArcEnds::heads(dealt([ends], order)).iter();
        assert_eq!((walk.len(), walk.next(), walk.next_back()), (10, Some(1), Some(2)));
        assert_eq!((walk.next_back(), walk.next(), walk.len()), (Some(1), Some(2), 6));

        assert_eq!(walk.by_ref().rev().collect::<Vec<_>>(), [0, 3, 2, 1, 0, 3]);
        assert_eq!((walk.next(), walk.next_back(), walk.len()), (None, None, 0));

        let by_reference: Vec<usize> = (&ArcEnds::tails(dealt([ends], order))).into_iter().collect();
        let by_value: Vec<usize> = ArcEnds::tails(dealt([ends], order)).into_iter().collect();
        assert_eq!(by_reference, by_value);
    }

    #[test]
    fn test_the_ends_of_no_arc_and_their_equality_by_hand() {
        let none = ArcEnds::OF_NO_ARC;
        assert_eq!((none.len(), none.is_empty(), none.get(0), none.iter().next()), (0, true, None, None));
        assert_eq!((ArcEnds::default(), format!("{:?}", ArcEnds::default())), (none, "ArcEnds tails []".to_owned()));

        // Tails and heads of the same arcs differ; the tails of arcs whose heads are other alike.
        let mut room = KeptBasisRoom::new();
        let (ends, order) = ten_arcs(&mut room);
        let (tails, heads) = (ArcEnds::tails(dealt([ends], order)), ArcEnds::heads(dealt([ends], order)));
        assert_ne!(tails, heads);
        let mut other = KeptBasisRoom::new();
        let arcs: Vec<(usize, usize, u64, u64, i64)> = (0..10).map(|arc| (arc % 4, arc % 4, 0, 1, 1)).collect();
        kept_star(&mut other, &problem_of(&MincostNetwork::from_arcs(&[0; 4], &arcs)));
        let loops = other.kept_problem().expect("the room should hold the basis just kept").ends;
        assert_eq!(tails, ArcEnds::tails(dealt([loops], order)));
        assert_eq!(
            format!("{:?}", ArcEnds::heads(dealt([loops], order))),
            "ArcEnds heads [0, 1, 2, 3, 0, 1, 2, 3, 0, 1]"
        );
    }

    proptest! {
        #[test]
        fn test_get_and_get_unchecked_answer_alike_and_get_refuses_past_the_arcs(
            (node_count, arcs) in (1_usize..8).prop_flat_map(|node_count| {
                (Just(node_count), proptest::collection::vec((0..node_count, 0..node_count), 0..40))
            }),
        ) {
            let columns: Vec<(usize, usize, u64, u64, i64)> =
                arcs.iter().map(|&(tail, head)| (tail, head, 0, 1, 1)).collect();
            let network = MincostNetwork::from_arcs(&vec![0; node_count], &columns);
            let mut room = KeptBasisRoom::new();
            kept_star(&mut room, &problem_of(&network));
            let ends = room.kept_problem().expect("the room should hold the basis just kept").ends;
            let order = SlotOrder::new(arcs.len(), node_count);

            for view in [ArcEnds::tails(dealt([ends], order)), ArcEnds::heads(dealt([ends], order))] {
                for (arc, &(tail, head)) in arcs.iter().enumerate() {
                    debug_assert!(arc < view.len(), "the index should be below its count, as the comment below says");
                    // SAFETY: `arc` is below the arc count.
                    let unchecked = unsafe { view.get_unchecked(arc) };
                    prop_assert_eq!(view.get(arc), Some(unchecked));
                    prop_assert_eq!(unchecked, if view.end == End::Tail { tail } else { head });
                }
                for arc in [view.len(), view.len() + 1, usize::MAX] {
                    prop_assert_eq!(view.get(arc), None);
                }
                prop_assert!(view.iter().eq((0..view.len()).filter_map(|arc| view.get(arc))));
            }
        }
    }
}
