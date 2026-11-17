// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bounds of an arc, set and answered as a pair.

/// The least and the most flow of an arc, in units of flow: what [`Network::set_bounds`](crate::Network::set_bounds)
/// sets at once and answers. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// The least flow of the arc, in units of flow.
    pub lower_bound: u64,
    /// The most flow of the arc, in units of flow.
    pub capacity: u64,
}

impl Bounds {
    /// The bounds of an arc whose least flow is `lower_bound` and which can carry `width` more: its capacity is the
    /// two added up. The one place an arc's capacity is made from its lower bound and its width.
    ///
    /// # Panics
    /// Panics if the two add up past `u64::MAX`, which no arc a network holds does: they add up to its capacity.
    #[track_caller]
    pub(super) const fn of_width(lower_bound: u64, width: u64) -> Self {
        Self { lower_bound, capacity: lower_bound.strict_add(width) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounds_read_as_their_two_numbers_by_name() {
        let bounds = Bounds { lower_bound: 1, capacity: 3 };
        assert_eq!(format!("{bounds:?}"), "Bounds { lower_bound: 1, capacity: 3 }");
        assert_ne!(bounds, Bounds { capacity: 1, lower_bound: 3 }, "the two numbers are not interchangeable");
    }

    #[test]
    fn test_of_width_adds_the_width_to_the_lower_bound_by_hand() {
        assert_eq!(Bounds::of_width(2, 5), Bounds { lower_bound: 2, capacity: 7 });
        assert_eq!(Bounds::of_width(u64::MAX, 0), Bounds { lower_bound: u64::MAX, capacity: u64::MAX });
    }

    #[test]
    #[should_panic(expected = "attempt to add with overflow")]
    fn test_of_width_past_u64_max_panics() {
        let _ = Bounds::of_width(u64::MAX, 1);
    }
}
