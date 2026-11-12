// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The rooted spanning tree a solve works on: the root after the nodes of the problem, each node joined to its parent
//! by a link, threaded in a preorder, with each node's potential; the cycle an arc closes with it; and the exchange of
//! a leaving link for an entering one.
//!
//! The modules depend on one another in this order and no other: `cycle` uses `spanning_tree`, and `rehang` uses
//! both.
//!
//! - `spanning_tree`: the [`TreeRoom`](spanning_tree::TreeRoom) a tree is kept in, and the
//!   [`SpanningTree`](spanning_tree::SpanningTree) a star loads into it, with its invariant; the indices only this file
//!   makes, of which a [`Link`](spanning_tree::Link) alone leaves the folder, tied to the tree's borrow; the climb of a
//!   cycle's two ends to their apex, which holds the tree it climbs, the ends it finds with the tree, and the walk of a
//!   side; and the view through which the walks read and write the columns. [`Direction`] is re-exported for the
//!   starting basis, which gives each root link one; the room, the tree, a link, a direction and a side for the basis,
//!   which loads the tree and pivots on it. For the basis's tests alone, each hung node's hanging read out as plain
//!   numbers, and the check of the threaded columns against the model.
//! - `cycle`: the [`CycleWalk`](cycle::CycleWalk) from the two ends of the cycle an arc closes with the tree up to
//!   their apex, which holds the climb and whose steps keep the node each side would have leave, and the
//!   [`Cycle`](cycle::Cycle) it becomes, with the hangings of its two sides and the kept node an exchange is made of.
//!   For the tests of the folder and of the basis, the one fixture that makes a walk from raw ends.
//! - `rehang`: the [`Exchange`](rehang::Exchange) made of a cycle with a kept node, which moves the subtree the
//!   leaving link cuts off under the entering arc and shifts its potentials.
//! - `plain_tree` (tests only): the model the tests check against, a tree as parent links with every derived column
//!   recomputed from them, and the trees the tests draw.

mod cycle;
#[cfg(test)]
mod plain_tree;
mod rehang;
mod spanning_tree;

#[cfg(test)]
pub(crate) use cycle::tests::cycle_walk;
pub(crate) use cycle::{Cycle, CycleWalk};
pub(crate) use rehang::Exchange;
#[cfg(test)]
pub(crate) use spanning_tree::tests::{PlainHanging, check_threading, hangings_of};
pub(crate) use spanning_tree::{Direction, Link, Side, SpanningTree, TreeRoom};
