// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The network simplex: a solver that is handed a problem at each solve and keeps the room it works in, and nothing
//! else, and answers the flows into a buffer or owned. It is a front end beside the network of
//! `network`, built on the same shared ground of the root, the basis, the tree, the start, the scan of blocks and the
//! checks; nothing in the crate uses it but the tests of the network, which check a network's answers against it.
//! Every module of this folder is the solver's own.
//!
//! The modules depend on one another in this order and no other: `solver` uses `first_pivots`, `optimum` and
//! `block_search`, and the tests of `first_pivots` use `block_search`.
//!
//! - `solver`: the [`NetworkSimplex`] with the room it keeps, its reservation, its two entries, `solve` and
//!   `solve_into`, and the run of one problem in that room, which enters the arcs it tries first and then those block
//!   search chooses.
//! - `first_pivots`: the arcs a run tries before block search, chosen from the problem's structure.
//! - `optimum`: the [`Optimum`] a solve into the caller's buffer answers, its cost and the potentials it lends.
//! - `block_search`: the block search over the slots, from where the last search stopped, entering the most violating
//!   arc of the first block that holds a violation, by the scan of `block_scan`.

mod block_search;
mod first_pivots;
mod optimum;
mod solver;

pub use optimum::Optimum;
pub use solver::NetworkSimplex;
