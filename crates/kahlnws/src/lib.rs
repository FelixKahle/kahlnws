// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A primal network simplex for the min-cost flow problem on exact integers.
//!
//! A [`Problem`] borrows six tables: for each arc its tail, its head, its lower bound (or none, for zero everywhere),
//! its capacity and its cost per unit of flow, and for each node its supply, positive where the node sends flow and
//! negative where it takes flow. Arc `a` is position `a` of every per-arc table and node `v` position `v` of the
//! supplies. A solve finds a flow of least cost that meets every supply within the bounds, or that no such flow exists.
//!
//! Two types solve it, each with one contract:
//!
//! - **The solver**, [`NetworkSimplex`], is a function: a problem in, an answer out. It holds room and nothing else,
//!   so its answer, the work included, is a function of the problem alone. Reach for it to solve problems that each
//!   stand on their own.
//! - **A network**, [`Network`], is a state: it holds one problem and a strongly feasible basis of it. Its setters
//!   change a few numbers of the problem held, a load hands it a whole problem, and its solve starts from the basis it
//!   holds. Reach for it where one problem is changed and solved again and again, as a search does.
//!
//! # What a solve answers
//!
//! A solver's solve answers `Result<Answer<T>, ProblemError>`, one `?` away from what it found; a network's solve,
//! which takes no problem, answers the [`Answer`] alone:
//!
//! - [`Outcome::Optimal`]: a flow of least cost, its cost as an `i128`, and a potential per node; a network's holds the
//!   cost, and the network lends the flow and the potentials through [`Network::optimum`]. With the potentials,
//!   `cost + potential[tail] − potential[head]` is at least zero on every arc whose flow is below its capacity and at
//!   most zero on every arc whose flow is above its lower bound, which proves the flow optimal.
//! - [`Outcome::Infeasible`]: no flow within the bounds meets the supplies.
//! - [`ProblemError`]: the problem lies outside the ranges a solve admits, and nothing ran. Its variant names the
//!   breach and holds the values that make it one.
//!
//! Beside the outcome, the [`Answer`] holds the [`Work`] of the solve: its pivots, those that sent no flow, and those
//! made by the arcs it tries before block search, counted exactly.
//!
//! Every number is checked before the solve starts: the ranges are those [`ProblemError`] names, among them a cost
//! within plus or minus [`cost_bound`] of the node count. Within them every sum the solve forms fits the integer type
//! it is held in.
//!
//! # The solver
//!
//! - [`NetworkSimplex::solve`] answers a [`Solution`] that owns the flows and the potentials.
//! - [`NetworkSimplex::solve_into`] writes the flows into a buffer the caller holds and answers an [`Optimum`] that
//!   lends the potentials from the solver. After [`NetworkSimplex::try_reserve`], a solve of a problem within the
//!   reserved counts allocates nothing.
//!
//! The answer of a solve, the work included, is a function of the problem as given, its arc order included: the same
//! problem gives the same answer from a fresh solver and from one that solved others before.
//!
//! # A network
//!
//! A [`Network`] holds one problem and a basis of it. [`Network::load`] copies a problem into the layout it is solved
//! in. Where the network holds a basis of a problem with the same arcs, and that basis serves the new numbers, the load
//! carries it over to them; otherwise it starts from the star. The solve after a load answers the same outcome and cost
//! from either start, and the same problem loaded again onto the optimum the network found for it, where that basis
//! serves it, makes no pivot. A load refuses a problem as a solve does, and a refused load keeps what the network held,
//! its basis included.
//!
//! Its setters change one number of the problem held at once and keep the basis a basis of the problem as it then
//! stands. Each answers the number it replaced, so setting that number back undoes it, and a move of supply is undone
//! by the move back. A capacity of zero switches an arc off. A number out of range is refused with a [`ChangeError`],
//! which changes nothing. Its reads answer the problem as it then stands, each number where it is asked for, so a
//! caller keeps no copy of it. [`Network`]'s doc lists the setters and the reads.
//!
//! Its [`solve`](Network::solve) takes no problem and answers no error: the cost of the optimum, or that no flow
//! exists, with its work. [`optimum`](Network::optimum) then lends the optimum as a [`NetworkOptimum`] that borrows the
//! network, its cost and potentials beside a [`Flows`] view that reads each arc's flow where it is asked for. A network
//! solved again with nothing changed prices no arc and makes no pivot; after a few changes it makes the pivots they
//! call for and prices the arcs they may have made worth using, and one whole pass more where it pivots.
//!
//! Its outcome and cost are those of the problem as it stands, a solver's too wherever a solver admits it, and its
//! potentials prove its flow optimal. The flow among tied optima, the potentials and the work are the network's own, a
//! deterministic function of the calls made on it. Moves of supply and lower bounds keep every supply within `i64` and
//! the flow still to be sent within the limit, but can take the net supplies past the limit
//! [`ProblemError::NetSupplyBeyondRange`] names, which a solver and a load refuse and the network still solves exactly.
//!
//! ```
//! use kahlnws::{Network, Problem, ProblemError};
//!
//! fn main() -> Result<(), ProblemError> {
//!     // Node 0 sends 4 units to node 2, directly at 5 a unit for up to 3 units, or through node 1 at 2 a unit for
//!     // up to 2.
//!     let mut network = Network::new();
//!     network.load(&Problem {
//!         tails: &[0, 0, 1],
//!         heads: &[2, 1, 2],
//!         lower_bounds: None,
//!         capacities: &[3, 4, 2],
//!         costs: &[5, 1, 1],
//!         supplies: &[4, 0, -4],
//!     })?;
//!
//!     network.solve();
//!     let optimum = network.optimum().expect("the routes should carry the supply");
//!     assert_eq!((optimum.total_cost, optimum.flows.get(2)), (14, Some(2)));
//!     assert_eq!(network.solve().work.pivots, 0, "nothing changed");
//!
//!     // The direct route at 2 a unit: it fills, and one unit still goes through node 1.
//!     assert_eq!(network.set_cost(0, 2), Ok(5));
//!     network.solve();
//!     let optimum = network.optimum().expect("the routes should carry the supply");
//!     assert_eq!(optimum.total_cost, 3 * 2 + 2);
//!
//!     // Node 2 needs one more unit, which node 0 sends: through node 1, at 2.
//!     assert_eq!(network.move_supply(2, 0, 1), Ok(()));
//!     network.solve();
//!     let optimum = network.optimum().expect("the routes should carry the supply");
//!     assert_eq!(optimum.total_cost, 3 * 2 + 2 * 2);
//!
//!     Ok(())
//! }
//! ```
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

// The map of the root. Two front ends, each a folder with what only it uses, rest on the shared ground beside them;
// neither uses the other, but for the tests of the network, which check its answers against the solver.
//
// - `network_simplex`: the front end handed a problem at each solve, `NetworkSimplex`; its own: its run, the arcs
//   it tries first, its block search, and the optimum it lends.
// - `network`: the front end that holds one problem through its changes, `Network`; its own: its run, its changes,
//   its marked blocks and their search, the sums it keeps, its views and the optimum it lends.
//
// The shared ground, what both front ends rest on, depends on itself in this order and no other: `ranges`, `room` and
// `outcome` use nothing of the crate, their tests included; `problem` and `slot_order` use `ranges`; `block_scan` uses
// `slot_order`; `tree` uses `ranges` and `room`; `starting_basis` uses `problem`, `ranges`, `room` and `tree`; and
// `basis` uses all of those but `block_scan` and `outcome`.
//
// - `ranges`: the bounds that keep the solve's arithmetic exact, and the width an index is stored at.
// - `room`: columns kept from one solve to the next, reserved and grown up to a total.
// - `outcome`: what a solve of either front end answers, its outcome, its owned optimum and its work.
// - `problem`: the problem as the caller gives it, why it is refused, and the checked problem.
// - `slot_order`: the dealing of the arcs to the slots they are priced in.
// - `block_scan`: the size of a block and the scan of a block, by which both searches for the entering arc price.
// - `tree`: the threaded spanning tree with its potentials, and its walks.
// - `starting_basis`: what a start is made from: the net supplies of the star, which both front ends start from, and
//   the excesses over a kept basis, which only the network's load starts from but the basis re-flows.
// - `basis`: the strongly feasible basis, its pivot, and every proof that rests on its columns; the room both front
//   ends make a basis in, and the kept room only a network holds, through which alone a basis is kept between calls,
//   started from, re-flowed, reopened, copied and changed: the solver holds no value of that type.

mod basis;
mod block_scan;
mod network;
mod network_simplex;
mod outcome;
mod problem;
mod ranges;
mod room;
mod slot_order;
mod starting_basis;
mod tree;

pub use network::{
    ArcEnds, ArcEndsIter, Bounds, ChangeError, Costs, CostsIter, FlowBounds, FlowBoundsIter, Flows, FlowsIter, Network,
    NetworkOptimum,
};
pub use network_simplex::{NetworkSimplex, Optimum};
pub use outcome::{Answer, Outcome, Solution, Work};
pub use problem::{Problem, ProblemError, Table};
pub use ranges::cost_bound;

/// The Rust examples of the repository's `README.md`, run as doctests so that they stay true.
#[cfg(doctest)]
#[doc = include_str!("../../../README.md")]
struct ReadmeExamples;

/// The Rust example of the crate's own `README.md`, the page of the crate on the registry, run as a doctest so that
/// it stays true.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct CrateReadmeExamples;

/// The Rust examples of the repository's guide to the Rust interface, run as doctests so that they stay true.
#[cfg(doctest)]
#[doc = include_str!("../../../docs/rust-api.md")]
struct RustGuideExamples;
