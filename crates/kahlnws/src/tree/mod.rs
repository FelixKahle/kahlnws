// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The rooted spanning tree a solve works on: the root after the nodes of the problem, each node joined to its parent
//! by a link, threaded in a preorder, with each node's potential; the cycle an arc closes with it; the exchange of a
//! leaving link for an entering one, and the cut of a node to the root; the two walks that make a kept tree a start of
//! a new problem; and the walk toward the root a network's change of supplies makes.
//!
//! It is shared ground of the two front ends, reached through the basis; the starting basis reads its directions, and
//! the network reads the smaller side of a cut to mark it. The two walks that make a kept tree a start serve the
//! network's load alone, the walk toward the root only the network's changes; the basis, below both
//! front ends, drives them.
//!
//! The modules depend on one another in this order and no other: `cycle`, `prune` and `tighten` use
//! `spanning_tree`, `rehang` uses `cycle` and `spanning_tree`, and `climb` uses `rehang` and `spanning_tree`.
//!
//! - `spanning_tree`: the [`TreeRoom`] a tree is kept in, which knows whether it keeps a whole tree between runs; the
//!   [`LentTree`] a star loads into it or a resume lends again, the one value that can keep it; the
//!   [`SpanningTree`] it holds, with its invariant; the indices only this file makes, of
//!   which a [`Link`] alone leaves the folder, tied to the tree's borrow; the climb of a cycle's two ends to their
//!   apex, which holds the tree it climbs, the ends it finds with the tree, and the walk of a side; what a root link
//!   costs ([`root_link_cost`]) and the potential that makes a link tight; whether a node hangs by a link, and the
//!   shift of the potentials of a node's subtree, by which a change of a link's cost keeps the link tight; the apex of
//!   two nodes, read alone; the ends of a cut; the nodes of the side of a cut with fewer nodes, and the tops, each
//!   walked in the thread's order; and the view through which the walks read and write the columns. [`Direction`] is
//!   re-exported for the starting basis, which gives each root link one; the room, the lent tree, a link, a direction,
//!   a side and the cost of a root link for the basis, which loads the tree, pivots on it, changes a cost on it, keeps
//!   it and resumes it; the tree itself for the basis, which lends it to be read. For the basis's tests alone, each
//!   hung node's hanging read out as plain numbers, and the check of the threaded columns against the model.
//! - `cycle`: the [`CycleWalk`] from the two ends of the cycle an arc closes with the tree up to their apex, which
//!   holds the climb and whose steps keep the node each side would have leave, and the [`Cycle`] it becomes, with the
//!   hangings of its two sides and the kept node an exchange is made of. For the tests of the folder and of the basis,
//!   the one fixture that makes a walk from raw ends.
//! - `rehang`: the move of a subtree under another node, which shifts its potentials, and the two moves made of it:
//!   the [`Exchange`] made of a cycle with a kept node, which hangs the subtree the leaving link cuts off under the
//!   entering arc; and the cut of a node from its parent, which hangs its subtree from the root by its root link.
//! - `prune`: the [`PruneWalk`] of the hung nodes from the leaves up, whose [`PruneStep`]s each keep their node on its
//!   link or hang it from the root by its root link, with its subtree, re-threaded at the thread's end. The walk and
//!   its step are re-exported for the basis, which decides each step.
//! - `tighten`: the [`TightenWalk`] of the hung nodes from the root down, whose steps each take their link's cost and
//!   make it tight. Each walk owns the tree it writes and hands it back only finished.
//! - `climb`: the [`RootwardWalk`] from one node up toward the root, whose [`RootwardStep`]s each keep their node on
//!   its link or hang it from the root by its root link, a cut with its subtree or a turn of a node there already,
//!   its subtree's potentials moved to make that link tight, the step answering the node where they moved. It owns
//!   the tree and hands it back at any step, every decision leaving the tree whole. The walk and its step are
//!   re-exported for the basis, which decides each step.
//! - `plain_tree` (tests only): the model the tests check against, a tree as parent links with every derived column
//!   recomputed from them, and the trees the tests draw.

mod climb;
mod cycle;
#[cfg(test)]
mod plain_tree;
mod prune;
mod rehang;
mod spanning_tree;
mod tighten;

pub(crate) use climb::{RootwardStep, RootwardWalk};
#[cfg(test)]
pub(crate) use cycle::tests::cycle_walk;
pub(crate) use cycle::{Cycle, CycleWalk};
pub(crate) use prune::{PruneStep, PruneWalk};
pub(crate) use rehang::Exchange;
#[cfg(test)]
pub(crate) use spanning_tree::tests::{DOWN, PlainHanging, UP, check_threading, hangings_of};
pub(crate) use spanning_tree::{Direction, LentTree, Link, Side, SpanningTree, TreeRoom, root_link_cost};
pub(crate) use tighten::TightenWalk;
