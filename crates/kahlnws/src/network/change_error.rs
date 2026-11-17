// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Why a change of a network is refused.

use crate::ranges::{NET_SUPPLY_LIMIT, cost_bound};

/// Why a change of a [`Network`](crate::Network) is refused: the number it would set lies outside the range a solve
/// admits. Plain data.
///
/// A refused change changes nothing: the network holds the problem it held, and an optimum it held. A variant refused
/// for the reason a [`ProblemError`](crate::ProblemError) would refuse a problem has that variant's name and fields;
/// the two types stay apart, since a problem can be refused for what no change can make, and a change for what no
/// problem meets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum ChangeError {
    /// The lower bound set on an arc passes its capacity, either set with it or the one it holds.
    #[error("arc {arc} has lower bound {lower_bound} above its capacity {capacity}")]
    LowerAboveCapacity {
        /// The arc.
        arc: usize,
        /// Its lower bound, in units of flow.
        lower_bound: u64,
        /// Its capacity, in units of flow.
        capacity: u64,
    },
    /// The cost set on an arc lies beyond plus or minus [`cost_bound`] of the node count.
    #[error(
        "arc {arc} has cost {cost} beyond the bound ±{bound} for the node count {node_count}",
        bound = cost_bound(*node_count)
    )]
    CostBeyondBound {
        /// The arc.
        arc: usize,
        /// The cost set on it, in units of cost.
        cost: i64,
        /// The number of nodes, which sets the bound.
        node_count: usize,
    },
    /// With the cost or the capacity set on an arc, the magnitudes of the costs times the capacities would add up past
    /// `i128::MAX`. Its `arc` is the arc changed, where a problem's refusal of the same name names the arc at which the
    /// running sum passes.
    #[error("the costs times the capacities, in magnitude, sum past {max} at arc {arc}", max = i128::MAX)]
    CostTotalBeyondRange {
        /// The arc whose change would take the sum past `i128::MAX`.
        arc: usize,
    },
    /// A move of supply would take the supply of a node past the range of an `i64`: below `i64::MIN` for the node it
    /// is taken from, above `i64::MAX` for the node it is added to.
    #[error("the supply of node {node} would be {supply}, outside the range of a signed 64-bit integer")]
    SupplyBeyondRange {
        /// The node, the one the amount is taken from where both would leave the range.
        node: usize,
        /// The supply it would have, in units of flow.
        supply: i128,
    },
    /// A change would leave more flow still to be sent than a network holds between solves: what the last
    /// [`load`](crate::Network::load) or a solve that found no flow left to be sent, with what changes since moved that
    /// the arcs could not carry at once, plus this change's amount, adds up to `total`, past `2^64 − 2`. A load leaves
    /// to be sent what its start does not route, up to the whole of the positive supplies net of the lower bounds where
    /// it starts from no basis, so a change right after a load of supplies near that limit can be refused. A solve that
    /// finds a flow leaves none. No solve of a [`NetworkSimplex`](crate::NetworkSimplex) returns it.
    #[error(
        "this change would leave {total} units of flow still to be sent, past {NET_SUPPLY_LIMIT}; a solve that finds a \
         flow leaves none"
    )]
    ExcessBeyondRange {
        /// That sum, in units of flow.
        total: u128,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_each_refusal_says_what_went_wrong_with_its_values() {
        let lower = ChangeError::LowerAboveCapacity { arc: 1, lower_bound: 3, capacity: 2 };
        assert_eq!(lower.to_string(), "arc 1 has lower bound 3 above its capacity 2");

        let bound = ChangeError::CostBeyondBound { arc: 2, cost: -1_537_228_672_809_129_302, node_count: 3 };
        assert_eq!(
            bound.to_string(),
            "arc 2 has cost -1537228672809129302 beyond the bound ±1537228672809129301 for the node count 3"
        );

        let total = ChangeError::CostTotalBeyondRange { arc: 4 };
        assert_eq!(
            total.to_string(),
            "the costs times the capacities, in magnitude, sum past 170141183460469231731687303715884105727 at arc 4"
        );

        let supply = ChangeError::SupplyBeyondRange { node: 1, supply: -9_223_372_036_854_775_809 };
        assert_eq!(
            supply.to_string(),
            "the supply of node 1 would be -9223372036854775809, outside the range of a signed 64-bit integer"
        );

        let excess = ChangeError::ExcessBeyondRange { total: 18_446_744_073_709_551_615 };
        assert_eq!(
            excess.to_string(),
            "this change would leave 18446744073709551615 units of flow still to be sent, past \
             18446744073709551614; a solve that finds a flow leaves none"
        );
    }
}
