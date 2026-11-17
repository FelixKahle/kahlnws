// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The optimum a network lends: its cost, the view of its flows, and its potentials; and the owned copy of it.

use super::flows::Flows;
use crate::outcome::Solution;

/// The optimum of a [`Network`](crate::Network), lent by it: a flow of least cost for the problem it holds, the
/// cost of that flow and the potentials that prove it optimal; what [`Network::optimum`](crate::Network::optimum) lends
/// after a solve that found a flow, whose answer carries the cost alone. Plain data but for the view of the flows.
///
/// It borrows the network, so no optimum can be read past a call that changes the network. Every number is that of the
/// problem as it stands, after the changes made since it was loaded, its lower bounds included: the flows each carry
/// their arc's lower bound, and the cost counts it. [`Solution::from`] copies it into an owned solution.
///
/// ```
/// use kahlnws::{Network, Problem, ProblemError, Solution};
///
/// fn main() -> Result<(), ProblemError> {
///     // Node 0 sends 3 units to node 1, which takes them on to node 2; the first arc carries at least 1.
///     let problem = Problem {
///         tails: &[0, 1],
///         heads: &[1, 2],
///         lower_bounds: Some(&[1, 0]),
///         capacities: &[5, 5],
///         costs: &[2, -1],
///         supplies: &[3, 0, -3],
///     };
///
///     let mut network = Network::new();
///     network.load(&problem)?;
///     network.solve();
///     let optimum = network.optimum().expect("the path should carry the supply");
///     assert_eq!((optimum.total_cost, optimum.flows.get(0), optimum.flows.get(2)), (3, Some(3), None));
///
///     let solution = Solution::from(optimum);
///     assert_eq!((solution.flows, solution.total_cost), (vec![3, 3], 3));
///
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetworkOptimum<'n> {
    /// The cost of the flow, in units of cost times units of flow.
    #[doc(alias = "objective")]
    pub total_cost: i128,
    /// The flow of each arc, in units of flow, its lower bound included, read where it is asked for.
    pub flows: Flows<'n>,
    /// Each node's potential, in units of cost, one per node, as [`Optimum::potentials`](crate::Optimum::potentials):
    /// with them, `cost + potential[tail] − potential[head]` is at least zero on every arc whose flow is below its
    /// capacity and at most zero on every arc whose flow is above its lower bound, which proves the flow optimal.
    pub potentials: &'n [i64],
}

impl NetworkOptimum<'static> {
    /// The optimum of the problem of no node: no flow, so no cost, and no potential.
    pub(super) const OF_NO_NODE: Self = Self { total_cost: 0, flows: Flows::OF_NO_ARC, potentials: &[] };
}

impl From<NetworkOptimum<'_>> for Solution {
    /// The solution of `optimum`, owned: its flows in arc order and its potentials copied, its cost. It allocates the
    /// two and costs O(m + n).
    fn from(optimum: NetworkOptimum<'_>) -> Self {
        let NetworkOptimum { total_cost, flows, potentials } = optimum;

        Self { flows: flows.iter().collect(), potentials: potentials.to_vec(), total_cost }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::flows::tests::ten_arcs;

    #[test]
    fn test_the_flows_of_no_arc_and_the_optimum_of_no_node_by_hand() {
        let none = Flows::OF_NO_ARC;
        assert_eq!((none.len(), none.is_empty(), none.get(0), none.iter().next()), (0, true, None, None));
        assert_eq!(Flows::default().len(), 0);
        assert_eq!(format!("{:?}", Flows::default()), "Flows []");

        let solution = Solution::from(NetworkOptimum::OF_NO_NODE);
        assert_eq!(solution, Solution { flows: vec![], potentials: vec![], total_cost: 0 });
    }

    #[test]
    fn test_an_optimum_converts_into_its_solution_by_hand() {
        let potentials = [0, -3, 4];
        let optimum = NetworkOptimum { total_cost: 17, flows: ten_arcs(), potentials: &potentials };
        let flows = vec![0, 11, 22, 33, 44, 55, 66, 77, 88, 99];

        assert_eq!(Solution::from(optimum), Solution { flows, potentials: vec![0, -3, 4], total_cost: 17 });
    }
}
