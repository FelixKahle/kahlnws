// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Amounts of supply a property test moves from one node of a network it holds to another: mostly small, sometimes
//! one that takes a supply to an end of `i64` or one past it, sometimes the largest amount, so that moves at the
//! extremes, moves the arcs cannot carry, and moves refused all occur.

use core::ops::RangeInclusive;

use proptest::prelude::*;

/// How a test draws an amount of supply to move from one node to another. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AmountDraw {
    /// This amount, in units of flow.
    Small(u64),
    /// The amount that takes the supply of the node it is taken from to `i64::MIN` exactly.
    ToLeastSupply,
    /// One more than [`ToLeastSupply`](Self::ToLeastSupply).
    PastLeastSupply,
    /// The amount that takes the supply of the node it is added to to `i64::MAX` exactly.
    ToMostSupply,
    /// One more than [`ToMostSupply`](Self::ToMostSupply).
    PastMostSupply,
    /// `u64::MAX`, the largest amount.
    Widest,
}

impl AmountDraw {
    /// The amount drawn for a move from a node of supply `from_supply` to a node of supply `to_supply`, in units of
    /// flow.
    ///
    /// # Panics
    /// Panics if one past an end passes `u64::MAX`: one past the least supply of a node at `i64::MAX`, or one past the
    /// most supply of a node at `i64::MIN`.
    #[track_caller]
    pub fn amount(self, from_supply: i64, to_supply: i64) -> u64 {
        match self {
            Self::Small(amount) => amount,
            Self::ToLeastSupply => from_supply.abs_diff(i64::MIN),
            Self::PastLeastSupply => from_supply.abs_diff(i64::MIN).strict_add(1),
            Self::ToMostSupply => i64::MAX.abs_diff(to_supply),
            Self::PastMostSupply => i64::MAX.abs_diff(to_supply).strict_add(1),
            Self::Widest => u64::MAX,
        }
    }
}

/// The strategy of an amount to move: four draws in five evenly from `small`, and one in five at or past an end of
/// `i64` for one of the two supplies, or the largest amount, each of the five alike.
pub fn amount_draws(small: RangeInclusive<u64>) -> impl Strategy<Value = AmountDraw> {
    prop_oneof![
        20 => small.prop_map(AmountDraw::Small),
        1 => Just(AmountDraw::ToLeastSupply),
        1 => Just(AmountDraw::PastLeastSupply),
        1 => Just(AmountDraw::ToMostSupply),
        1 => Just(AmountDraw::PastMostSupply),
        1 => Just(AmountDraw::Widest),
    ]
}

#[cfg(test)]
mod tests {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    use super::*;

    #[test]
    fn test_each_draw_names_its_amount_by_hand() {
        // From a node of supply 5 to one of supply −3: to `i64::MIN` is `5 + 2^63`, to `i64::MAX` is `2^63 − 1 + 3`.
        let draws = [
            AmountDraw::Small(7),
            AmountDraw::ToLeastSupply,
            AmountDraw::PastLeastSupply,
            AmountDraw::ToMostSupply,
            AmountDraw::PastMostSupply,
            AmountDraw::Widest,
        ];
        let half = 1_u64 << 63;
        let expected = [7, half + 5, half + 6, half + 2, half + 3, u64::MAX];
        assert_eq!(draws.map(|draw| draw.amount(5, -3)), expected);

        // The ends themselves: a node at `i64::MIN` has nothing more to give, one at `i64::MAX` no room to take.
        assert_eq!(AmountDraw::ToLeastSupply.amount(i64::MIN, 0), 0);
        assert_eq!(AmountDraw::ToMostSupply.amount(0, i64::MAX), 0);
        assert_eq!(AmountDraw::ToLeastSupply.amount(i64::MAX, 0), u64::MAX);
    }

    #[test]
    #[should_panic(expected = "attempt to add with overflow")]
    fn test_a_draw_past_u64_max_is_refused() {
        let _ = AmountDraw::PastLeastSupply.amount(i64::MAX, 0);
    }

    #[test]
    fn test_the_draws_reach_every_kind_and_keep_small_amounts_in_their_range() {
        let mut runner = TestRunner::deterministic();
        let strategy = amount_draws(1..=3);
        let mut kinds = [0_usize; 6];
        for _ in 0..600 {
            let draw = strategy.new_tree(&mut runner).expect("an amount should be drawn").current();
            let kind = match draw {
                AmountDraw::Small(amount) => {
                    assert!((1..=3).contains(&amount), "{amount}");
                    0
                }
                AmountDraw::ToLeastSupply => 1,
                AmountDraw::PastLeastSupply => 2,
                AmountDraw::ToMostSupply => 3,
                AmountDraw::PastMostSupply => 4,
                AmountDraw::Widest => 5,
            };
            kinds[kind] += 1;
        }

        assert!(kinds.iter().all(|&count| count > 0), "{kinds:?}");
    }
}
