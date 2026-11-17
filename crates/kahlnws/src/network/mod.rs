// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The network: one problem held, in the layout it is solved in, with a strongly feasible basis of it, which lends the
//! problem it holds and the optimum it found. It is a front end beside the solver of `network_simplex`,
//! built on the same shared ground of the root, the basis, the tree, the start, the scan of blocks and the checks, and
//! nothing in the crate uses it. Every module of this folder is the network's own.
//!
//! The modules depend on one another in this order and no other: the four views `arc_ends`, `costs`, `flow_bounds` and
//! `flows` use `dealt_columns`, `flow_bounds` uses `bounds` too, `optimum` uses `flows`, `held_problem` uses
//! `dealt_columns`, `unmet` and the views but `flows`, and `held` uses every other, its tests the test fixtures of
//! `marked_blocks` too; the tests of the views use the fixture of `dealt_columns`, and those of `optimum` the fixture
//! of `flows`.
//!
//! - `held`: the [`Network`] with its invariant, its load, its changes, each marking the blocks of arcs it may make
//!   violate, and its solve, which prices the marked blocks alone and marks every block at each of its pivots; its
//!   copy; its reads of the problem it holds, which its doc lists; and the cold panic of each entry.
//! - `held_problem`: the problem a network holds, read in place from columns already dealt, which lends the views of
//!   its tables: the one seam the reads of a network go through.
//! - `unmet`: the condition a call on a network's arc or node did not meet, with its values: the one home of the rule
//!   every entry of a network panics by, each under its own name.
//! - `bounds`: the [`Bounds`] of an arc, which a network sets and answers as a pair.
//! - `change_error`: the [`ChangeError`] a change of a network is refused with.
//! - `dealt_columns`: a fixed number of columns of the problem held by slot, read arc by arc as one row through the
//!   dealing of the arcs to their slots, and the walk over them in arc order: the one place the four views' reads are
//!   proved.
//! - `arc_ends`: the [`ArcEnds`] view of the tails or the heads of the arcs a network holds, and the [`ArcEndsIter`]
//!   over them.
//! - `costs`: the [`Costs`] view of the costs of the arcs a network holds, and the [`CostsIter`] over them.
//! - `flow_bounds`: the [`FlowBounds`] view of the lower bounds or the capacities of the arcs a network holds, and
//!   the [`FlowBoundsIter`] over them.
//! - `flows`: the [`Flows`] view of the flows of a network's optimum, which reads each arc's flow through the dealing
//!   of the arcs to their slots, and the [`FlowsIter`] over them.
//! - `optimum`: the [`NetworkOptimum`] a network lends, with its flows, and the owned copy of an optimum.
//! - `marked_blocks`: the blocks of slots that may hold an arc that violates, with the blocks of the arcs at each node,
//!   and the search over them, which prices only the marked blocks, by the scan of `block_scan`, and clears each it
//!   prices clean. A change marks what it may make violate, a pivot every block.
//! - `kept_sums`: the cost total of the problem, which a change is checked against, the cost of the basis's flow, kept
//!   as a change or a pivot moves it, so that an optimum answers its cost at once, and the flow up the basis's root
//!   links, which a move of supply and a change of bounds are checked against.

mod arc_ends;
mod bounds;
mod change_error;
mod costs;
mod dealt_columns;
mod flow_bounds;
mod flows;
mod held;
mod held_problem;
mod kept_sums;
mod marked_blocks;
mod optimum;
mod unmet;

pub use arc_ends::{ArcEnds, ArcEndsIter};
pub use bounds::Bounds;
pub use change_error::ChangeError;
pub use costs::{Costs, CostsIter};
pub use flow_bounds::{FlowBounds, FlowBoundsIter};
pub use flows::{Flows, FlowsIter};
pub use held::Network;
pub use optimum::NetworkOptimum;
