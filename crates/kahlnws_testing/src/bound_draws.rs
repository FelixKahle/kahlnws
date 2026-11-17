// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Bounds a property test sets on an arc of a network it holds, a lower bound or a capacity: mostly small, so that an
//! arc switched off, an arc of no width, bounds that cross and flows moved past a bound all occur, and sometimes the
//! largest, so that changes at the extreme and changes refused for the flow they move occur too.

use core::ops::RangeInclusive;

use proptest::prelude::*;

/// How a test draws a bound to set on an arc, a lower bound or a capacity. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoundDraw {
    /// This bound, in units of flow.
    Small(u64),
    /// `u64::MAX`, the largest bound.
    Widest,
}

impl BoundDraw {
    /// The bound drawn, in units of flow.
    pub fn bound(self) -> u64 {
        match self {
            Self::Small(bound) => bound,
            Self::Widest => u64::MAX,
        }
    }
}

/// The strategy of a bound to set: nine draws in ten evenly from `small`, and one in ten the largest bound.
pub fn bound_draws(small: RangeInclusive<u64>) -> impl Strategy<Value = BoundDraw> {
    prop_oneof![9 => small.prop_map(BoundDraw::Small), 1 => Just(BoundDraw::Widest)]
}

#[cfg(test)]
mod tests {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    use super::*;

    #[test]
    fn test_each_draw_names_its_bound_by_hand() {
        let draws = [BoundDraw::Small(0), BoundDraw::Small(3), BoundDraw::Widest];
        assert_eq!(draws.map(BoundDraw::bound), [0, 3, 18_446_744_073_709_551_615]);
    }

    #[test]
    fn test_the_draws_reach_every_kind_and_keep_small_bounds_in_their_range() {
        let mut runner = TestRunner::deterministic();
        let strategy = bound_draws(0..=4);
        let mut kinds = [0_usize; 2];
        let mut small = [0_usize; 5];
        for _ in 0..400 {
            match strategy.new_tree(&mut runner).expect("a bound should be drawn").current() {
                BoundDraw::Small(bound) => {
                    let at = usize::try_from(bound).expect("a small bound should fit a usize");
                    assert!(at < small.len(), "{bound}");
                    small[at] += 1;
                    kinds[0] += 1;
                }
                BoundDraw::Widest => kinds[1] += 1,
            }
        }

        assert!(kinds.iter().chain(&small).all(|&count| count > 0), "{kinds:?} {small:?}");
    }
}
