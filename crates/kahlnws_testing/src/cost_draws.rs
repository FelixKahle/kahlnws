// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Costs a property test sets on an arc of a network it holds: mostly small, sometimes at an end of the cost bound of
//! the network's node count, sometimes one past an end, so that a change at the extremes and one refused both occur.

use core::ops::RangeInclusive;

use proptest::prelude::*;

/// How a test draws a cost to set on an arc. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CostDraw {
    /// This cost, in units of cost.
    Small(i64),
    /// The cost bound itself.
    AtBound,
    /// The cost bound, negated.
    AtNegativeBound,
    /// One above the cost bound.
    PastBound,
    /// One below the cost bound, negated.
    PastNegativeBound,
}

impl CostDraw {
    /// The cost drawn for a network whose costs lie within plus or minus `bound`, in units of cost.
    ///
    /// # Panics
    /// Panics if one past an end of `±bound` passes the range of an `i64`.
    #[track_caller]
    pub fn cost(self, bound: i64) -> i64 {
        match self {
            Self::Small(cost) => cost,
            Self::AtBound => bound,
            Self::AtNegativeBound => bound.strict_neg(),
            Self::PastBound => bound.strict_add(1),
            Self::PastNegativeBound => bound.strict_neg().strict_sub(1),
        }
    }
}

/// The strategy of a cost to set: five draws in six evenly from `small`, and one in six at or past an end of the cost
/// bound, each of the four alike.
pub fn cost_draws(small: RangeInclusive<i64>) -> impl Strategy<Value = CostDraw> {
    prop_oneof![
        20 => small.prop_map(CostDraw::Small),
        1 => Just(CostDraw::AtBound),
        1 => Just(CostDraw::AtNegativeBound),
        1 => Just(CostDraw::PastBound),
        1 => Just(CostDraw::PastNegativeBound),
    ]
}

#[cfg(test)]
mod tests {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    use super::*;

    #[test]
    fn test_each_draw_names_its_cost_by_hand() {
        let draws = [
            CostDraw::Small(-4),
            CostDraw::AtBound,
            CostDraw::AtNegativeBound,
            CostDraw::PastBound,
            CostDraw::PastNegativeBound,
        ];
        assert_eq!(draws.map(|draw| draw.cost(10)), [-4, 10, -10, 11, -11]);
    }

    #[test]
    #[should_panic(expected = "attempt to add with overflow")]
    fn test_a_draw_past_i64_max_is_refused() {
        let _ = CostDraw::PastBound.cost(i64::MAX);
    }

    #[test]
    fn test_the_draws_reach_every_kind_and_keep_small_costs_in_their_range() {
        let mut runner = TestRunner::deterministic();
        let strategy = cost_draws(-3..=3);
        let mut kinds = [0_usize; 5];
        for _ in 0..600 {
            let draw = strategy.new_tree(&mut runner).expect("a cost should be drawn").current();
            let kind = match draw {
                CostDraw::Small(cost) => {
                    assert!((-3..=3).contains(&cost), "{cost}");
                    0
                }
                CostDraw::AtBound => 1,
                CostDraw::AtNegativeBound => 2,
                CostDraw::PastBound => 3,
                CostDraw::PastNegativeBound => 4,
            };
            kinds[kind] += 1;
        }

        assert!(kinds.iter().all(|&count| count > 0), "{kinds:?}");
    }
}
