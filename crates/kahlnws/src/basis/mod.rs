// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The strongly feasible spanning-tree basis a run pivots on, from its start to its outcome, and the room it is kept
//! in from one run to the next.
//!
//! It is shared ground of the two front ends: both load it from the star, pivot on it, keep it and read its potentials.
//! What serves one of them alone stays here because it rests on the proofs of `columns`: the write-out of the flows
//! serves the solver; the kept room, with the start from a kept basis re-flowed, the changes, the reads in place and
//! the copy, serves the network, a type the solver holds no value of. Only a network uses the [`KeptBasisRoom`], the
//! [`LentBasis`], the [`KeptColumns`], the [`KeptProblem`], the [`KeptArc`], the [`Rerouted`] basis and the
//! [`Restated`] bounds: they are `pub(crate)`, the narrowest the module tree allows while their proofs stay here, and
//! the solver is kept from a kept start by type, holding none of them.
//!
//! The folder is split by proof, not by operation: every type that makes or relies on a proven index of the arc and
//! link columns lives in `columns`, so that each such proof and every access resting on it stay in one file; what
//! holds no such index lives in a file of its own. The modules depend on one another in this order and no other:
//! `cycle_flow` and `tree_arc_flow` use `arc_state`, `restatement` uses `arc_state` and `tree_arc_flow`, and `columns`
//! uses all four. Their tests follow the same edges, and the tests of `columns` use the model `plain_basis` besides.
//!
//! - `columns`: the [`BasisRoom`] both front ends make a basis in for a run, from the star, with the net supplies it
//!   starts from, which holds no basis between runs; the [`KeptBasisRoom`] only a network holds, which wraps it with
//!   the excesses over the basis it holds and the marker that says it holds one, and alone decides a start from that
//!   basis, carries it over, reopens it, copies it and reads it in place, through the [`KeptColumns`], the
//!   [`KeptProblem`] with each arc's [`Ends`], and the [`KeptArc`]; the [`Basis`] with its invariant, its pivot and the
//!   end of its run, the [`LoadedBasis`] that pairs it with the start it was made from, for the write-out and the cost
//!   of its arcs' flows, and the [`LentBasis`] that pairs it with the kept room's marker, which a network changes and
//!   keeps; the change of one arc's cost; the route of an amount from one node to another, and the [`Rerouted`] basis
//!   it answers with what its flow's cost and the flow up its root links gained, recording the nodes whose subtrees'
//!   potentials it moved, and that flow read alone; the restatement of one arc for new bounds by the same route, and
//!   the flow it moves, read from the room before it is made; the arc and link columns and the slot only they make; and
//!   the [`PricedRows`] the pricing reads. Re-exported: for the solver and its block search, the shared room, the
//!   loaded basis and the rows; for the network and its marked blocks, the kept room, the lent basis, the basis, the
//!   kept columns, the kept problem with its ends, the kept arc and the rerouted basis; for the tests of the solver,
//!   the first pivots, the searches and the network, the checked run of its tests, the entering rules a test drives it
//!   with, the invariant check, at a problem's own root cost or at another, the cost of a basis's flow, the arcs that
//!   violate as a model prices them, the load of a test's problem, a star kept in a kept room, and a room put out of
//!   step.
//! - `arc_state`: where an arc stands, out of the tree at one of its bounds or unpriced, and the bound an entering arc
//!   leaves.
//! - `cycle_flow`: what a pivot's cycle does to a link, its room and its flow once the step is sent, and the entering
//!   arc as the link of the end whose subtree an exchange moves.
//! - `tree_arc_flow`: how a tree arc stands once its node's excess is sent along it toward the root, the one rule of
//!   the re-flow, and how far a flow lies outside an arc's width.
//! - `restatement`: an arc's numbers restated for new bounds, the [`Restated`] a network hands the basis, and where the
//!   arc then stands, in the tree or out of it at a bound, with the flow that moves.
//! - `plain_basis` (tests only): the model the tests check against, a network extended by a root, recomputed from the
//!   network alone.

mod arc_state;
mod columns;
mod cycle_flow;
#[cfg(test)]
mod plain_basis;
mod restatement;
mod tree_arc_flow;

#[cfg(test)]
pub(crate) use columns::tests::{
    CheckedRun, EnteringRule, RunEnd, check, check_at, extended_cost_at, flow_up_the_root_links, kept_star, loaded,
    root_link_directions, tree_arcs, untie_the_kept_tree, violating_slots,
};
pub(crate) use columns::{
    Basis, BasisRoom, Ends, KeptArc, KeptBasisRoom, KeptColumns, KeptProblem, LentBasis, LoadedBasis, PricedRows,
    Rerouted,
};
pub(crate) use restatement::Restated;
