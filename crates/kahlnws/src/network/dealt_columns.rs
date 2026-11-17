// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The columns of the problem a network holds by slot, read arc by arc through the dealing of the arcs to their slots:
//! the one walk and the one set of proofs the four public views of a network rest on.

use core::iter::FusedIterator;
use core::ops::Range;

use crate::slot_order::SlotOrder;

/// `COLUMNS` columns of a network's problem by slot, with the dealing of its arcs to the slots: each arc's row, its
/// entry in every column, read where it is asked for, at the cost of two divisions, and every arc's in ascending order.
///
/// The slot an arc is dealt to is found once for the row, and every column is read at it.
///
/// # Invariant
/// Only [`new_unchecked`](Self::new_unchecked), its checked twin `new` and [`OF_NO_ARC`](Self::OF_NO_ARC) make one:
/// every column holds `order.arc_count()` entries, so the slot `order` deals an arc below that count indexes every
/// column.
#[derive(Clone, Copy, Debug)]
pub(super) struct DealtColumns<'n, T, const COLUMNS: usize> {
    /// The columns, by slot.
    columns: [&'n [T]; COLUMNS],
    /// The dealing of the arcs to the slots.
    order: SlotOrder,
}

/// Every row of a [`DealtColumns`] in ascending arc order.
///
/// # Invariant
/// Only [`DealtColumns::iter`] makes one: `arcs` lies within `0..columns.len()`.
#[derive(Clone, Debug)]
pub(super) struct DealtColumnsIter<'n, T, const COLUMNS: usize> {
    /// The columns read.
    columns: DealtColumns<'n, T, COLUMNS>,
    /// The arcs whose rows are still to come.
    arcs: Range<usize>,
}

impl<'n, T: Copy, const COLUMNS: usize> DealtColumns<'n, T, COLUMNS> {
    /// The columns of no arc.
    // No column entry and no arc: `DealtColumns`' invariant.
    pub(super) const OF_NO_ARC: Self = Self { columns: [&[]; COLUMNS], order: SlotOrder::new(0, 0) };

    /// The columns `columns`, by slot, of the arcs `order` deals; `None` where a column holds another number of
    /// entries than `order` has arcs. The twin of [`new_unchecked`](Self::new_unchecked) its tests check it against.
    #[cfg(test)]
    pub(super) fn new(columns: [&'n [T]; COLUMNS], order: SlotOrder) -> Option<Self> {
        columns.iter().all(|column| column.len() == order.arc_count()).then_some(Self { columns, order })
    }

    /// The columns `columns`, by slot, of the arcs `order` deals, as `new` makes them where it makes them, without
    /// comparing their lengths with the arc count.
    ///
    /// # Safety
    /// Every column holds exactly `order.arc_count()` entries: the columns `new` admits.
    pub(super) unsafe fn new_unchecked(columns: [&'n [T]; COLUMNS], order: SlotOrder) -> Self {
        debug_assert!(
            columns.iter().all(|column| column.len() == order.arc_count()),
            "called `DealtColumns::new_unchecked` with a column of another length than the arc count {}",
            order.arc_count()
        );

        Self { columns, order }
    }

    /// The number of arcs, `m`.
    pub(super) fn len(&self) -> usize {
        self.order.arc_count()
    }

    /// The row of `arc`, its entry in each column in the order the columns were given; `None` where `arc` is not
    /// below [`len`](Self::len).
    pub(super) fn get(&self, arc: usize) -> Option<[T; COLUMNS]> {
        (arc < self.len()).then(|| {
            debug_assert!(arc < self.len(), "the arc should be below the arc count");
            // SAFETY: `arc` is below the arc count, as just compared: the precondition of `get_unchecked`.
            unsafe { self.get_unchecked(arc) }
        })
    }

    /// The row of `arc`, as [`get`](Self::get) answers it where it answers one, without the comparison with the arc
    /// count.
    ///
    /// # Safety
    /// `arc` is below [`len`](Self::len).
    pub(super) unsafe fn get_unchecked(&self, arc: usize) -> [T; COLUMNS] {
        debug_assert!(arc < self.len(), "called `DealtColumns::get_unchecked` with arc {arc} past {}", self.len());
        // SAFETY: `arc` is below the arc count (the caller's promise): the precondition of `slot_of_unchecked`.
        let slot = usize::from(unsafe { self.order.slot_of_unchecked(arc) });

        self.columns.map(|column| {
            debug_assert!(slot < column.len(), "a dealt slot should be below the arc count");
            // SAFETY: a slot `slot_of_unchecked` answers for an arc below the arc count is below it (`SlotOrder`), and
            // every column holds that many entries (`DealtColumns`).
            unsafe { *column.get_unchecked(slot) }
        })
    }

    /// Every arc's row, in ascending arc order.
    pub(super) fn iter(&self) -> DealtColumnsIter<'n, T, COLUMNS> {
        DealtColumnsIter { columns: *self, arcs: 0..self.len() }
    }
}

impl<T: Copy, const COLUMNS: usize> Iterator for DealtColumnsIter<'_, T, COLUMNS> {
    type Item = [T; COLUMNS];

    fn next(&mut self) -> Option<[T; COLUMNS]> {
        let arc = self.arcs.next()?;

        debug_assert!(arc < self.columns.len(), "an arc of the walk should be below the arc count");
        // SAFETY: the arcs still to come lie within `0..columns.len()` (`DealtColumnsIter`).
        Some(unsafe { self.columns.get_unchecked(arc) })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.arcs.size_hint()
    }
}

impl<T: Copy, const COLUMNS: usize> DoubleEndedIterator for DealtColumnsIter<'_, T, COLUMNS> {
    fn next_back(&mut self) -> Option<[T; COLUMNS]> {
        let arc = self.arcs.next_back()?;

        debug_assert!(arc < self.columns.len(), "an arc of the walk should be below the arc count");
        // SAFETY: the arcs still to come lie within `0..columns.len()` (`DealtColumnsIter`).
        Some(unsafe { self.columns.get_unchecked(arc) })
    }
}

impl<T: Copy, const COLUMNS: usize> ExactSizeIterator for DealtColumnsIter<'_, T, COLUMNS> {}

impl<T: Copy, const COLUMNS: usize> FusedIterator for DealtColumnsIter<'_, T, COLUMNS> {}

#[cfg(test)]
pub(super) mod tests {
    use proptest::prelude::*;

    use super::*;

    /// The columns `columns`, by slot, of the arcs `order` deals, for a view's tests.
    ///
    /// # Panics
    /// Panics if a column holds another number of entries than `order` has arcs.
    pub(in crate::network) fn dealt<T: Copy, const COLUMNS: usize>(
        columns: [&[T]; COLUMNS],
        order: SlotOrder,
    ) -> DealtColumns<'_, T, COLUMNS> {
        DealtColumns::new(columns, order).expect("the test's columns should hold one entry an arc")
    }

    /// Ten arcs on four nodes, dealt to the slots 0, 3, 6, 9, 1, 4, 7, 2, 5, 8: slot `s` holds the arc
    /// `[0, 4, 7, 1, 5, 8, 2, 6, 9, 3][s]`, whose entry is `10·a` in the first column and `a` in the second.
    const TENS: [u64; 10] = [0, 40, 70, 10, 50, 80, 20, 60, 90, 30];
    /// The second column of the ten arcs, by slot.
    const ONES: [u64; 10] = [0, 4, 7, 1, 5, 8, 2, 6, 9, 3];

    #[test]
    fn test_one_column_of_ten_arcs_read_through_its_slots_by_hand() {
        let column = DealtColumns::new([&TENS[..]], SlotOrder::new(10, 4))
            .expect("the column should hold ten entries for ten arcs");

        assert_eq!(column.len(), 10);
        assert_eq!(column.iter().collect::<Vec<_>>(), [0, 10, 20, 30, 40, 50, 60, 70, 80, 90].map(|entry| [entry]));
        assert_eq!(
            (column.get(3), column.get(9), column.get(10), column.get(usize::MAX)),
            (Some([30]), Some([90]), None, None)
        );
        debug_assert!(7 < column.len(), "the index should be below its count, as the comment below says");
        // SAFETY: arc 7 is below the arc count 10.
        assert_eq!(unsafe { column.get_unchecked(7) }, [70]);

        let mut walk = column.iter();
        assert_eq!((walk.len(), walk.next(), walk.next_back()), (10, Some([0]), Some([90])));
        assert_eq!(walk.by_ref().rev().collect::<Vec<_>>(), [80, 70, 60, 50, 40, 30, 20, 10].map(|entry| [entry]));
        assert_eq!((walk.next(), walk.next_back(), walk.len()), (None, None, 0));
    }

    #[test]
    fn test_two_columns_of_ten_arcs_read_at_one_slot_by_hand() {
        let columns = DealtColumns::new([&TENS[..], &ONES[..]], SlotOrder::new(10, 4))
            .expect("each column should hold ten entries");

        assert_eq!((columns.get(4), columns.get(10)), (Some([40, 4]), None));
        assert_eq!(columns.iter().next_back(), Some([90, 9]));
    }

    #[test]
    fn test_columns_of_another_length_than_the_arcs_and_of_no_arc_by_hand() {
        let order = SlotOrder::new(2, 1);
        assert!(DealtColumns::new([&[1_u64, 2, 3][..]], order).is_none());
        assert!(DealtColumns::new([&[1_u64, 2][..], &[0_u64][..]], order).is_none());
        assert!(DealtColumns::new([&[1_u64][..], &[0_u64, 0][..]], order).is_none());

        let none = DealtColumns::<i64, 1>::OF_NO_ARC;
        assert_eq!((none.len(), none.get(0), none.iter().next()), (0, None, None));
        assert_eq!(DealtColumns::<u64, 2>::OF_NO_ARC.len(), 0);
    }

    proptest! {
        #[test]
        fn test_get_and_get_unchecked_answer_alike_and_get_refuses_past_the_arcs(
            (column, node_count) in (proptest::collection::vec(any::<i64>(), 0..60), 0_usize..12),
        ) {
            let order = SlotOrder::new(column.len(), node_count);
            let view = DealtColumns::new([&column[..]], order).expect("the column should hold one entry an arc");

            for arc in 0..view.len() {
                let slot = order.slot_of(arc).map(usize::from).expect("an arc below the count should have a slot");
                debug_assert!(arc < view.len(), "the index should be below its count, as the comment below says");
                // SAFETY: `arc` is below the arc count.
                let unchecked = unsafe { view.get_unchecked(arc) };
                prop_assert_eq!(view.get(arc), Some(unchecked));
                prop_assert_eq!(unchecked, [column[slot]]);
            }
            for arc in [view.len(), view.len() + 1, usize::MAX] {
                prop_assert_eq!(view.get(arc), None);
            }
            prop_assert!(view.iter().eq((0..view.len()).filter_map(|arc| view.get(arc))));
            prop_assert!(view.iter().rev().eq((0..view.len()).rev().filter_map(|arc| view.get(arc))));
            debug_assert!(column.len() == order.arc_count(), "the column should hold one entry an arc");
            // SAFETY: the column holds one entry an arc, as `new` just admitted.
            let unchecked = unsafe { DealtColumns::new_unchecked([&column[..]], order) };
            prop_assert!(unchecked.iter().eq(view.iter()));
            let longer = [&column[..], &[0][..]].concat();
            prop_assert!(DealtColumns::new([&longer[..]], order).is_none(), "one entry more than the arcs");
        }
    }
}
