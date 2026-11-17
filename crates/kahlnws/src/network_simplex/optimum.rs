// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The optimum a solver lends beside the flows it wrote to the caller's buffer: its cost and its potentials.

/// What an optimal solve found beside the flows it wrote to the caller's buffer, the potentials lent by the solver: the
/// payload of [`Outcome::Optimal`](crate::Outcome::Optimal) that
/// [`NetworkSimplex::solve_into`](crate::NetworkSimplex::solve_into) answers, where
/// [`NetworkSimplex::solve`](crate::NetworkSimplex::solve) answers a [`Solution`](crate::Solution). Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Optimum<'s> {
    /// The cost of the flow, in units of cost times units of flow.
    #[doc(alias = "objective")]
    pub total_cost: i128,
    /// Each node's potential, in units of cost, one per node: with them, `cost + potential[tail] − potential[head]`
    /// is at least zero on every arc whose flow is below its capacity and at most zero on every arc whose flow is
    /// above its lower bound, which proves the flow optimal.
    pub potentials: &'s [i64],
}
