// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A primal network simplex for the min-cost flow problem on exact integers.
//!
//! A [`Problem`] borrows six tables: for each arc its tail, its head, its lower bound (or none, for zero everywhere),
//! its capacity and its cost per unit of flow, and for each node its supply, positive where the node sends flow and
//! negative where it takes flow. Arc `a` is position `a` of every per-arc table and node `v` position `v` of the
//! supplies. A [`NetworkSimplex`] solves it: it finds a flow of least cost that meets every supply within the bounds,
//! or that no such flow exists.
//!
//! # What a solve answers
//!
//! A solve answers `Result<Answer<T>, ProblemError>`, one `?` away from what it found:
//!
//! - [`Outcome::Optimal`]: a flow of least cost, its cost as an `i128`, and a potential per node. With the
//!   potentials, `cost + potential[tail] − potential[head]` is at least zero on every arc whose flow is below its
//!   capacity and at most zero on every arc whose flow is above its lower bound, which proves the flow optimal.
//! - [`Outcome::Infeasible`]: no flow within the bounds meets the supplies.
//! - [`ProblemError`]: the problem lies outside the ranges a solve admits, and nothing ran. Its variant names the
//!   breach and holds the values that make it one.
//!
//! Beside the outcome, the [`Answer`] holds the [`Work`] of the solve: its pivots, those that sent no flow, and those
//! made by the arcs it tries before block search, counted exactly.
//!
//! Every number is checked before the solve starts: the ranges are those [`ProblemError`] names, among them a cost
//! within plus or minus [`cost_bound`] of the node count. Within them every sum the solve forms fits the integer type
//! it is held in. The answer, the work included, is a function of the problem as given, its arc order included: the
//! same problem gives the same answer from a fresh solver and from one that solved others before.
//!
//! # Two levels
//!
//! - [`NetworkSimplex::solve`] answers a [`Solution`] that owns the flows and the potentials.
//! - [`NetworkSimplex::solve_into`] writes the flows into a buffer the caller holds and answers an [`Optimum`] that
//!   lends the potentials from the solver. After [`NetworkSimplex::try_reserve`], a solve of a problem within the
//!   reserved counts allocates nothing.
//!
//! # Example
//!
//! The same three tables of arcs answer each of the three ways a solve can go, as their supplies change:
//!
//! ```
//! use kahlnws::{NetworkSimplex, Outcome, Problem, ProblemError};
//!
//! /// The cost of the cheapest flow, `None` where no flow exists.
//! fn cheapest(solver: &mut NetworkSimplex, supplies: &[i64]) -> Result<Option<i128>, ProblemError> {
//!     // Node 0 sends to node 2, directly at 5 a unit for up to 3 units, or through node 1 at 2 a unit for up to 2.
//!     let problem = Problem {
//!         tails: &[0, 0, 1],
//!         heads: &[2, 1, 2],
//!         lower_bounds: None,
//!         capacities: &[3, 4, 2],
//!         costs: &[5, 1, 1],
//!         supplies,
//!     };
//!
//!     Ok(match solver.solve(&problem)?.outcome {
//!         Outcome::Optimal(solution) => Some(solution.total_cost),
//!         Outcome::Infeasible => None,
//!     })
//! }
//!
//! let mut solver = NetworkSimplex::new();
//! assert_eq!(cheapest(&mut solver, &[4, 0, -4]), Ok(Some(14)));
//! assert_eq!(cheapest(&mut solver, &[6, 0, -6]), Ok(None), "the two routes carry 5 units at most");
//! assert_eq!(cheapest(&mut solver, &[4, 0, -3]), Err(ProblemError::Unbalanced { total: 1 }));
//! ```
//!
//! The repository's `README.md` and the pages of its `docs/` folder describe the problem, the guarantees and the
//! algorithm at length, and the C, C++ and Python interfaces built on this crate.

mod basis;
mod first_pivots;
mod outcome;
mod pricing;
mod problem;
mod ranges;
mod room;
mod slot_order;
mod solver;
mod starting_basis;
mod tree;

pub use outcome::{Answer, Optimum, Outcome, Solution, Work};
pub use problem::{Problem, ProblemError, Table};
pub use ranges::cost_bound;
pub use solver::NetworkSimplex;

/// The Rust examples of the repository's `README.md`, run as doctests so that they stay true.
#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
struct ReadmeExamples;

/// The Rust examples of the repository's guide to the Rust interface, run as doctests so that they stay true.
#[cfg(doctest)]
#[doc = include_str!("../../../docs/rust-api.md")]
struct RustGuideExamples;
