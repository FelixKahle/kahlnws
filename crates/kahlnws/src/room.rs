// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The room a front end keeps from one solve to the next: columns that are rewritten in the room they hold, never
//! shrunk, and reserved or grown up to a total.

use std::collections::TryReserveError;

/// Rewrites `column` as the values of `values`, in the room it holds: it allocates only where `values` yields more
/// entries than that room has, and it never gives room back.
pub(crate) fn refill<T>(column: &mut Vec<T>, values: impl Iterator<Item = T>) {
    column.clear();
    column.extend(values);
}

/// Makes room in `column` for `total` entries in all, whatever it holds now: a refill of up to `total` entries
/// allocates nothing afterwards. Room already held is kept, and a total at or below the room held asks for nothing.
///
/// # Errors
/// Returns the `TryReserveError` of a refused reservation.
pub(crate) fn reserve_total<T>(column: &mut Vec<T>, total: usize) -> Result<(), TryReserveError> {
    // `try_reserve` counts from the entries held, not from the room, so the room for `total` asks for `total` less
    // those entries.
    column.try_reserve(total.saturating_sub(column.len()))
}

/// Makes room in `column` for `total` entries in all, whatever it holds now, as [`reserve_total`] does, growing as a
/// `Vec` does where the memory runs out: the same failure as a refill that grows the column.
///
/// # Panics
/// Panics if the room passes `isize::MAX` bytes, as a growing `Vec` does.
pub(crate) fn grow_total<T>(column: &mut Vec<T>, total: usize) {
    column.reserve(total.saturating_sub(column.len()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_refill_rewrites_every_entry_in_the_room_held_by_hand() {
        let mut column = Vec::with_capacity(8);
        column.extend([9, 9, 9, 9, 9]);
        let (start, room) = (column.as_ptr(), column.capacity());

        refill(&mut column, [1, 2].into_iter());
        assert_eq!(column, [1, 2], "nothing of the earlier entries is kept");
        refill(&mut column, 0..8);
        assert_eq!(column, [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!((column.as_ptr(), column.capacity()), (start, room), "the room held is used, not given back");

        refill(&mut column, 0..9);
        assert_eq!(column.len(), 9, "more entries than the room grow it");
    }

    #[test]
    fn test_a_reservation_counts_to_a_total_by_hand() {
        let mut column: Vec<u64> = Vec::new();
        reserve_total(&mut column, 6).expect("room for six entries should be granted");
        assert!(column.capacity() >= 6);

        // Five entries held: a total of six asks for room for one more, not six more.
        refill(&mut column, 0..5);
        let room = column.capacity();
        reserve_total(&mut column, 6).expect("room for six entries should be granted");
        assert_eq!(column.capacity(), room, "the room already held covers a total of six");

        // A total below the entries held asks for nothing and gives nothing back.
        reserve_total(&mut column, 2).expect("a smaller total should be granted");
        assert_eq!((column.len(), column.capacity()), (5, room));
    }

    #[test]
    fn test_growing_counts_to_a_total_by_hand() {
        let mut column: Vec<u64> = vec![1, 2];
        grow_total(&mut column, 7);
        assert!(column.capacity() >= 7);
        assert_eq!(column, [1, 2], "growing changes no entry");

        let (start, room) = (column.as_ptr(), column.capacity());
        grow_total(&mut column, 3);
        assert_eq!((column.as_ptr(), column.capacity()), (start, room), "a total within the room asks for nothing");
    }

    #[test]
    fn test_a_reservation_past_the_address_space_is_refused() {
        let mut column: Vec<u64> = vec![1, 2];

        assert!(reserve_total(&mut column, usize::MAX).is_err());
        assert_eq!(column, [1, 2], "a refused reservation changes nothing");
    }
}
