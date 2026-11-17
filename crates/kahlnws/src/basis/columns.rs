// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The columns of a run with the tree as one strongly feasible spanning-tree basis: the room both front ends make a
//! basis in from the star, and the room a network keeps its basis in between calls, which alone starts from the basis
//! it holds, re-flowed, reopens it and copies it; the rows the pricing reads, the pivot, the decision whether every
//! supply is routed, the write-out of the flows, the changes a network makes, and the end of a run.
//!
//! Every type here makes or relies on a proven index of the arc and link columns, so each such proof and every
//! access that rests on it stay in this one file.

use core::fmt;
use core::iter::{self, FusedIterator, Zip};
use core::mem;
use core::ops::{Deref, Range, RangeInclusive};
use core::slice;

use std::collections::TryReserveError;

use typed_index_collections::TiSlice;

use super::arc_state::{ArcState, Bound};
use super::cycle_flow::{EnteringLink, entering_link, room, sent};
use super::restatement::{Restated, Restatement};
use super::tree_arc_flow::{Standing, TreeArcFlow, resend};
use crate::problem::{CheckedArc, CheckedArcs, CheckedProblem, NodeId, ProblemError};
use crate::ranges::{NET_SUPPLY_LIMIT, cost_bound, narrow, root_cost, widen, widest_root_cost, within_cost_bound};
use crate::room::{grow_total, refill, reserve_total};
use crate::slot_order::{DealtSlotsIter, Slot};
use crate::starting_basis::{Excesses, KeptStart, NetSupplies, RootLink, Start, root_link};
use crate::tree::{
    Cycle, CycleWalk, Direction, Exchange, LentTree, Link, PruneStep, PruneWalk, RootwardStep, RootwardWalk, Side,
    SpanningTree, TightenWalk, TreeRoom, root_link_cost,
};

/// The room a basis is made in for a run, which both front ends share: the arc and link columns, the room of the tree
/// and the room of the net supplies, holding whatever the last basis left. It holds no whole basis between runs: it
/// knows nothing of what its columns held, and a load writes them anew.
///
/// It only grows. Nothing is read from it but through the [`LoadedBasis`] that [`load`](Self::load) writes into it and
/// lends.
///
/// Its tree room keeps a marker of its own, of whether its columns hold a whole tree: a run on this room writes it
/// twice, out when its star is written and in when its basis is kept, and nothing on the solver's path reads it.
#[derive(Clone, Debug, Default)]
pub(crate) struct BasisRoom {
    /// The arc and link columns and the room of the tree.
    columns: RoomColumns,
    /// The room of each node's supply net of the lower bounds, which a start reads.
    net_supplies: NetSupplies,
}

/// The room a network holds its basis in between its calls: the room every run is made in, with the room of the
/// excesses over the basis it holds, which only a start from that basis reads, and whether its columns hold a whole
/// basis. Only a network holds one, so every call that reads or reopens a kept basis, or starts from it, is written
/// on this type and on no other: the solver holds no value through which one can be made.
///
/// It only grows. Nothing is read from it but through the [`LentBasis`] that [`Resumption::begin`] makes in it, that
/// [`reopen`](Self::reopen) lends again, or that [`copy_from`](Self::copy_from) lends from the copy of the basis
/// another room holds; by the decision of [`resume`](Self::resume), which reads the kept ends and states where the room
/// holds a basis; and through what [`kept`](Self::kept), [`kept_problem`](Self::kept_problem) and
/// [`kept_arc`](Self::kept_arc) read where it holds one.
///
/// # Invariant
/// `kept` guards the arc and link columns of `room`, the ends, costs, states, capacities and flows: where it is
/// `Kept::Basis`, they are those of the basis a run ended on, as [`LentBasis::keep`] left them. That is one fact; the
/// tree room's own marker states the other, that the tree's columns, from the parents to the potentials, hold the whole
/// tree that keep left kept beside them. Every start, every reopen and every copy takes it out before its first write
/// to a column or its lending, and only [`LentBasis::keep`] puts it in. Where it is `Kept::Basis`, the columns' root
/// cost is the root cost of that basis.
#[derive(Default)]
pub(crate) struct KeptBasisRoom {
    /// The room every run is made in.
    room: BasisRoom,
    /// The room of each node's excess over the basis held, which a start from it reads.
    excesses: Excesses,
    /// Whether the arc and link columns hold the basis a run ended on.
    kept: Kept,
}

/// The arc and link columns of a basis room and the room of its tree, holding whatever the last basis left.
#[derive(Clone, Debug, Default)]
struct RoomColumns {
    /// Per slot: the arc's tail and head.
    ends: Vec<Ends>,
    /// Per slot: the arc's cost, in units of cost.
    costs: Vec<i64>,
    /// Per slot: where the arc stands.
    states: Vec<ArcState>,
    /// Per link, the arcs at their slots and then each node's root link: the most it carries, in units of flow.
    capacities: Vec<u64>,
    /// Per link, in the same order: what it carries, in units of flow above the arc's lower bound.
    flows: Vec<u64>,
    /// The room of the tree.
    tree: TreeRoom,
    /// The cost of a root link that points down in the basis last loaded or re-flowed here, in units of cost.
    root_cost: i64,
}

/// What a kept basis room's arc and link columns hold between a network's calls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Kept {
    /// No whole basis: before the first call's end, and from the first write of a start or a copy, or from a reopen,
    /// until the basis lent is kept.
    #[default]
    Nothing,
    /// The basis the last call ended on.
    Basis,
}

/// What the basis a room holds between runs carries on its arcs, and its nodes' potentials, read in place. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeptColumns<'r> {
    /// Per slot, `m` of them: what the arc dealt to it carries, in units of flow above its lower bound.
    pub(crate) arc_flows: &'r [u64],
    /// Per node, `n` of them: its potential, in units of cost, node `v` at `v`.
    pub(crate) potentials: &'r [i64],
}

/// The numbers of the problem the basis a room holds between runs is of, as it keeps them by slot, read in place. Plain
/// data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeptProblem<'r> {
    /// Per slot, `m` of them: the tail and the head of the arc dealt to it.
    pub(crate) ends: &'r [Ends],
    /// Per slot, `m` of them: the cost of the arc dealt to it, in units of cost.
    pub(crate) costs: &'r [i64],
    /// Per slot, `m` of them: what the arc dealt to it can carry above its lower bound, in units of flow.
    pub(crate) widths: &'r [u64],
}

/// One arc of the basis a room holds between runs, read in place. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeptArc {
    /// The cost of a unit of flow on it, in units of cost.
    pub(crate) cost: i64,
    /// What it can carry above its lower bound, in units of flow.
    pub(crate) width: u64,
    /// What it carries, in units of flow above its lower bound.
    pub(crate) flow: u64,
}

/// One row of the basis a room holds between runs, read in place. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KeptRow {
    /// The cost of a unit of flow on the arc, in units of cost.
    cost: i64,
    /// Where the arc stands.
    state: ArcState,
    /// What it can carry above its lower bound, in units of flow.
    width: u64,
    /// What it carries, in units of flow above its lower bound.
    flow: u64,
}

/// The start a kept basis room decided on for one problem, before writing anything of its columns: the basis it holds
/// re-flowed, where that basis serves the problem, or the star. It holds the room's columns and marker, so that
/// nothing writes them between the decision and [`begin`](Self::begin), which makes the basis it decided on.
///
/// # Invariant
/// Only [`KeptBasisRoom::resume`] makes one, from the room as it then stood, which nothing has written since: `start`
/// is `Kept` exactly where the room held a basis that serves the problem, with the proof of it ([`ServingStart`]), and
/// `Star` otherwise; either way with the start of the problem, its net supplies gathered in the room.
#[derive(Debug)]
pub(crate) struct Resumption<'r, 'p> {
    /// The columns the decision read.
    columns: &'r mut RoomColumns,
    /// The room's marker, which the basis begun puts back when it is kept.
    kept: &'r mut Kept,
    /// The start decided on, with what it is made from, borrowed from the room's net supplies and excesses.
    start: DecidedStart<'p, 'r>,
}

/// The start a [`Resumption`] decided on, with what it is made from.
#[derive(Debug)]
enum DecidedStart<'p, 'r> {
    /// The basis the room holds, which serves the problem of the start, with the excesses over it.
    Kept {
        /// The problem with its net supplies.
        start: Start<'p, 'r>,
        /// The proof that the basis held serves it, with the excesses over it.
        serving: ServingStart<'p, 'r>,
    },
    /// The star of the start.
    Star(Start<'p, 'r>),
}

/// A basis a route has moved, with what the cost of its flow and the flow up its root links gained: `B` the basis as
/// the route was handed it, a [`Basis`] or a [`LentBasis`]. Plain data.
#[derive(Debug)]
pub(crate) struct Rerouted<B> {
    /// The basis, a basis of the problem with the supplies the route moved.
    pub(crate) basis: B,
    /// What the cost of the basis's flow, root links counted at their costs, gained, in units of cost times units of
    /// flow, modulo `2^128`.
    pub(crate) cost_gained: i128,
    /// What the flow up the root links gained, in units of flow: below zero where it fell.
    pub(crate) up_gained: i128,
}

/// A strongly feasible spanning-tree basis of one problem, extended by a root and one root link per node, loaded into
/// the room it borrows for `'r`: what a run pivots on, from its start to its outcome.
///
/// The basis works on the problem with its lower bounds taken out: an arc of width `capacity − lower bound`, carrying
/// flow above its lower bound, and each node's supply net of the lower bounds. Each node also has a root link that
/// joins it to the root, of capacity `u64::MAX`, up from the node at cost zero or down to it at the root cost `M` the
/// basis holds, the one its start was made with: the problem's own for a star [`BasisRoom::load`] made, and the one
/// [`Resumption::begin`] was given for a start it made. A link is an arc at its slot, or node `v`'s root link at
/// `m + v`. Every potential is the tree node's, the root's last.
///
/// The problem a basis is of is the one it was made for, with every cost [`set_cost`](Self::set_cost) set since, every
/// supply [`route`](Self::route) moved since and every arc's bounds [`restate`](Self::restate) set since.
///
/// # Invariant
/// With `n` nodes and `m` arcs in the problem it is of, and `C` the largest magnitude of a cost there, it holds:
/// - *Shape.* The arc columns hold `m` slots, the arc dealt to slot `s` at `s`, with its ends, both below `n`, its
///   cost and its state; the link columns hold `m + n` links, an arc's capacity being its width; the tree has `n` hung
///   nodes, `n + 1` potentials, the root's last, and `m + n` links.
/// - *Tree.* An arc is in the tree exactly where it is some node's link; it then has the state `Unpriced`. Each hung
///   node's link joins it to its parent, the way its direction says: an arc's tail and head are the node and its
///   parent, so a node that hangs by an arc hangs from a node below `n`, never from the root; a root link joins its
///   own node and the root.
/// - *Duality.* The root's potential is zero and every tree link is tight: its cost plus its tail's potential less its
///   head's is zero. A root link costs zero where it points up and `M` where it points down, `M` the root cost the
///   basis holds, at least `(n − 1)·C + 1` and at most [`widest_root_cost`] of `n`. No pivot changes which way a root
///   link in the tree points; a start, a route or a restatement sets it.
/// - *Flow.* Every node conserves its net supply, root links counted; every flow lies within `0..=capacity`; an arc out
///   of the tree carries nothing (`AtLower`), its width (`AtUpper`), or has no width (`Unpriced`); a root link out of
///   the tree carries nothing. The flow up the root links, which equals the flow down them, is at most
///   [`NET_SUPPLY_LIMIT`].
/// - *Strong feasibility.* Every tree link can carry more flow toward the root: a link that runs up from its node
///   carries less than its capacity, one that runs down to it more than nothing.
///
/// [`BasisRoom::load`] and [`Resumption::begin`] establish it; [`pivot`](Self::pivot), [`set_cost`](Self::set_cost),
/// [`route`](Self::route) and [`restate`](Self::restate) keep it; [`keep`](Self::keep) ends the run on it. A network's
/// [`LentBasis::keep`] leaves it in the kept room, from which [`KeptBasisRoom::reopen`] lends it again as it was kept,
/// and [`KeptBasisRoom::copy_from`] lends a copy of it from another room.
#[derive(Debug)]
pub(crate) struct Basis<'r> {
    /// What the pricing reads of each arc, and its state.
    arcs: ArcColumns<'r>,
    /// What each link can carry and carries.
    links: LinkColumns<'r>,
    /// The tree, with each node's potential, lent by the tree's room.
    tree: LentTree<'r>,
    /// The cost of a root link that points down, `M`, in units of cost.
    root_cost: i64,
}

/// A basis a network holds, lent from its kept room with the room's marker, which [`keep`](LentBasis::keep) puts back:
/// `B` a [`Basis`] lent again by a reopen or a copy, or a [`LoadedBasis`] a start made with its problem. It reads as
/// the basis it holds, and changes it by its own calls alone, [`pivot`](LentBasis::pivot),
/// [`set_cost`](LentBasis::set_cost), [`route`](LentBasis::route) and [`restate`](LentBasis::restate): it lends no
/// `&mut` of its basis, so no code outside this file can put one room's basis beside another room's marker. What
/// remains discipline is this file's own: only the three makers its invariant names pair a basis with a marker, and
/// none of them swaps two bases' trees.
///
/// # Invariant
/// `kept` is the marker of the kept room `basis` is lent from, `Kept::Nothing` while it is lent: only
/// [`Resumption::begin`], [`KeptBasisRoom::reopen`] and [`KeptBasisRoom::copy_from`] make one, each after taking the
/// marker out.
#[derive(Debug)]
pub(crate) struct LentBasis<'r, B> {
    /// The basis.
    basis: B,
    /// The marker of the room it is lent from.
    kept: &'r mut Kept,
}

/// A basis with the start it was made from, the problem and its net supplies: what a solve pivots on and writes out,
/// so that no other problem can be written out with its flows.
///
/// It reads as the [`Basis`] it holds, and pivots it by [`pivot`](Self::pivot). It lends no `&mut` of its basis, so
/// the basis is never put in another's place beside this problem.
///
/// # Invariant
/// Only [`BasisRoom::load`] and [`Resumption::begin`] make one: `basis` was loaded or re-flowed from the problem of
/// `start`, so its *Shape* is that of the problem's `n` nodes and `m` arcs, the arc dealt to slot `s` at `s`, and no
/// pivot changes it.
#[derive(Debug)]
pub(crate) struct LoadedBasis<'r, 'p> {
    /// The basis.
    basis: Basis<'r>,
    /// The start the basis was made from, whose problem the write-out reads again.
    start: Start<'p, 'r>,
}

/// Each arc of a loaded problem in ascending order with its whole flow, its lower bound included, read from the basis
/// loaded from it.
///
/// # Invariant
/// Only [`LoadedBasis::whole_flows`] makes one: `arcs` are the arcs of the problem the basis was loaded from, `slots`
/// deals the slots of its `m` arcs, and `flows` is that basis's link column, `m + n` links, the arcs' first (`Basis`,
/// Shape). So every slot `slots` yields indexes `flows`.
#[derive(Debug)]
struct WholeFlowsIter<'b, A> {
    /// The arcs still to read, in ascending order.
    arcs: A,
    /// Their dealt slots, in the same order.
    slots: DealtSlotsIter,
    /// Per link: what it carries, in units of flow above the arc's lower bound.
    flows: &'b [u64],
}

/// What the pricing reads of each arc of a loaded basis, in slot order, with the arcs' states, which a pivot writes,
/// and their costs, which a change of a cost writes.
///
/// # Invariant
/// The three columns hold one entry per slot, `m` each, and every end is below the problem's node count `n`
/// ([`Ends`]).
#[derive(Debug)]
struct ArcColumns<'r> {
    /// Per slot: the arc's tail and head.
    ends: &'r [Ends],
    /// Per slot: the arc's cost, in units of cost.
    costs: &'r mut [i64],
    /// Per slot: where the arc stands.
    states: &'r mut [ArcState],
}

/// What each link of a loaded basis can carry and carries: the arcs at their slots, then each node's root link; the
/// arcs' widths, which a restatement writes, and every flow, which a pivot, a route and a restatement write.
///
/// # Invariant
/// Both columns hold exactly as many entries as the tree beside them in the [`Basis`] or the [`Reflow`], or held by a
/// walk of a route beside its [`RouteColumns`], has links, `m + n`, so every [`Link`] of that tree indexes them; and no
/// other tree's link ever does: this file reads links only from the tree held beside them, the one they were loaded or
/// re-flowed with, and a link does not outlive that tree's borrow. Since `m` is at most `m + n`, every [`ArcSlot`]
/// indexes them too.
#[derive(Debug)]
struct LinkColumns<'r> {
    /// Per link: the most it carries, in units of flow: an arc's width, `u64::MAX` for a root link.
    capacities: &'r mut [u64],
    /// Per link: what it carries, in units of flow above the arc's lower bound.
    flows: &'r mut [u64],
}

/// A slot of one loaded basis, below its arc count `m`: the index its arc columns, the rows lent from them, and the
/// arc part of its link columns are read and written at without a check.
///
/// What it proves: [`ArcColumns::slot`] made it from a [`Slot`], or [`ArcColumns::slot_of`] from a [`Link`], each
/// checked against the arc count, and nothing else in this file makes one, its tests included. What remains this
/// file's discipline: one is made within a call of the basis or the re-flow and used within that call on the same
/// columns, and never kept past it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArcSlot(usize);

/// The tail and head of one arc of a loaded basis.
///
/// Each is stored at 32 bits, not at the native width, so that the two ends of an arc are one 8-byte read beside its
/// cost and its state in every row the pricing reads. In a network simplex with this column layout, ends at 32 bits
/// against ends at 64 bits measured a few per cent faster on a supply-shaped network of about 4,000 arcs, and within
/// noise elsewhere.
///
/// # Invariant
/// Only [`RoomColumns::scatter`] writes one into a room's ends column, from an arc of the [`CheckedProblem`] a load or
/// a re-flow places: both ends lie below that problem's node count `n`, so below the `n + 1` potentials of the tree
/// beside them. Outside this file one is only read, by its [`tail`](Self::tail) and its [`head`](Self::head).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ends {
    /// The node the arc leaves.
    tail: u32,
    /// The node the arc enters.
    head: u32,
}

/// The arc a pivot brings in, read once. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EnteringArc {
    /// Its slot.
    slot: ArcSlot,
    /// The bound it leaves.
    bound: Bound,
    /// The end flow is sent along it from: its tail at the lower bound, its head at its capacity; one of its
    /// [`Ends`], so below the node count.
    first: usize,
    /// The end flow arrives at along it: its other end, below the node count too.
    second: usize,
    /// What it can carry above its lower bound, in units of flow; above zero, since it is priced.
    width: u64,
    /// Its reduced cost, in units of cost.
    reduced_cost: i64,
}

/// What the ratio test answers: the flow the cycle carries and the side whose kept node's link leaves. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RatioTest {
    /// The most flow the cycle carries, in units of flow: the least room of its links and the entering arc's width.
    step: u64,
    /// The side whose kept node's link leaves, named only where that side's walk kept a node; `None` exactly where no
    /// link leaves and the entering arc moves to its other bound.
    leaving: Option<Side>,
}

/// What one link of a loaded basis carries and the most it carries, read together. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LinkFill {
    /// What it carries, in units of flow above the arc's lower bound.
    flow: u64,
    /// The most it carries, in units of flow: an arc's width, `u64::MAX` for a root link.
    capacity: u64,
}

/// The ends and the states of a kept basis, of the counts of one problem: what is read of the kept basis to decide
/// whether it can serve that problem, and what its arcs out of the tree carry.
///
/// # Invariant
/// Only [`RoomColumns::kept_shape`] makes one, from the columns of a room that holds a whole basis: `ends` and `states`
/// hold one entry per arc of `problem`, `m` each, so every slot a [`DealtSlotsIter`] deals for `m` arcs indexes them.
#[derive(Clone, Copy, Debug)]
struct KeptShape<'r, 'p> {
    /// Per slot: the kept arc's tail and head.
    ends: &'r [Ends],
    /// Per slot: where the kept arc stood.
    states: &'r [ArcState],
    /// The problem whose counts the columns hold.
    problem: CheckedProblem<'p>,
}

/// The start of a problem over the basis a room holds, where that basis serves it: the proof the decision hands to
/// the re-flow.
///
/// What it proves: only [`RoomColumns::kept_start`] makes one, and only after it found, in the room it read, that the
/// tree room keeps a tree of the problem's `n` hung nodes and `m + n` links, that the arc columns hold `m` slots and
/// the link columns `m + n` links, and that every slot holds the ends of the problem's arc dealt to it; and `start`
/// holds the problem's excesses over that basis, within the limit, which [`KeptStart`] proves of itself.
///
/// What remains this file's discipline, since the value holds no borrow of the room: it is handed to the `reflow` of
/// the same room, with nothing written to that room in between. [`KeptBasisRoom::resume`], its one maker's one caller,
/// puts it in a [`Resumption`] beside the room it read, which holds that room until [`Resumption::begin`] hands it to
/// the room's `reflow`.
#[derive(Debug)]
struct ServingStart<'p, 'e> {
    /// The problem with its excesses.
    start: KeptStart<'p, 'e>,
}

/// The columns of a kept basis while the tree beside them is re-flowed for a new problem: the tree, and what the
/// re-flow writes besides it.
///
/// # Invariant
/// Only [`RoomColumns::reflow`] makes one, from a [`ServingStart`], with the problem's arcs just placed, which keeps
/// every column's length. The columns have the *Shape* of [`Basis`] for the problem, with its ends, costs and widths;
/// every arc stands where [`ArcState::at`] puts what its kept state gives it of its new width; the tree is the one the
/// last run ended on, of the problem's `n` hung nodes and `m + n` links; a root link out of that tree carries nothing;
/// and the excesses hold one entry per node ([`ReflowColumns`]). The tree keeps the *Tree* item of [`Basis`] all
/// through the re-flow: each hung node's link joins it to its parent, an arc's link a node below `n`, since every arc
/// has the ends it had in the basis the run ended on (`ServingStart`), and the prune only moves a node to its own root
/// link.
#[derive(Debug)]
struct Reflow<'r, 'e> {
    /// The tree the last run ended on, lent again.
    tree: LentTree<'r>,
    /// What the re-flow writes besides the tree.
    columns: ReflowColumns<'r, 'e>,
}

/// The arc and link columns of a re-flow, with each node's excess still to send toward the root.
///
/// # Invariant
/// The arc and link columns are those [`Reflow`] describes, and `excesses` holds one entry per node of the problem,
/// `n`, its excess over the arcs as they stand ([`KeptStart`]), each node's children's transfers added once they are
/// sent.
#[derive(Debug)]
struct ReflowColumns<'r, 'e> {
    /// What the pricing reads of each arc, and its state.
    arcs: ArcColumns<'r>,
    /// What each link can carry and carries.
    links: LinkColumns<'r>,
    /// Per node: its excess, in units of flow.
    excesses: &'e mut TiSlice<NodeId, i128>,
}

/// The arc and link columns of a basis while a route or a restatement walks its tree, with what their writes have
/// gained, and the nodes whose subtrees' potentials they moved.
///
/// # Invariant
/// The arc and link columns are those of a [`Basis`] whose tree a walk of the route holds, the one they were lent
/// with; `root_cost` is that basis's. `cost_gained` is what the route's writes so far added to the cost of the basis's
/// flow, root links counted at their costs, modulo `2^128`, and `up_gained` what they added to the flow up the root
/// links; only [`gain`](Self::gain) moves them, beside each flow written. `moved` ends with each node the route hung
/// from the root so far whose subtree's potentials moved, in the order they moved, after what it held before.
#[derive(Debug)]
struct RouteColumns<'r, 'm> {
    /// What the pricing reads of each arc, and its state.
    arcs: ArcColumns<'r>,
    /// What each link can carry and carries.
    links: LinkColumns<'r>,
    /// The cost of a root link that points down, `M`, in units of cost.
    root_cost: i64,
    /// What the cost of the flow gained, in units of cost times units of flow, modulo `2^128`.
    cost_gained: i128,
    /// What the flow up the root links gained, in units of flow.
    up_gained: i128,
    /// The nodes whose subtrees' potentials moved.
    moved: &'m mut Vec<usize>,
}

/// What a link a route writes is, which prices its flow: an arc of a cost, or a root link that points one way. Plain
/// data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkPrice {
    /// An arc, each unit of its flow at this cost, in units of cost.
    Arc(i64),
    /// A root link that points this way: each unit up at nothing, and counted in the flow up the root links; each
    /// unit down at the root cost.
    RootLink(Direction),
}

/// The rows the pricing reads, lent by a loaded basis: per slot the arc's ends, cost and state, with every tree node's
/// potential.
///
/// It is the one value that pairs a basis's ends with its potentials: every reduced cost the file forms is formed by
/// it.
///
/// # Invariant
/// Only [`Basis::rows`] makes one, from the columns of one basis: the three per-slot slices hold that basis's `m`
/// entries each, every end among them is below its node count `n` ([`Ends`]), and `potentials` holds its tree's
/// `n + 1` potentials (`Basis`, Shape).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PricedRows<'b> {
    /// Per slot: the arc's tail and head.
    ends: &'b [Ends],
    /// Per slot: the arc's cost, in units of cost.
    costs: &'b [i64],
    /// Per slot: where the arc stands.
    states: &'b [ArcState],
    /// Per tree node: its potential, in units of cost, the root's last.
    potentials: &'b [i64],
}

/// The columns a stretch of priced rows reads, zipped: each slot's number with its arc's ends, cost and state.
type RowsZip<'b> = Zip<Zip<Zip<Range<u32>, slice::Iter<'b, Ends>>, slice::Iter<'b, i64>>, slice::Iter<'b, ArcState>>;

/// Each slot of a stretch of priced rows in ascending order with its arc's violation: its pricing sign times its
/// reduced cost, in units of cost, negative exactly where the arc violates its bound.
///
/// # Invariant
/// Only [`PricedRows::violations`] makes one: `columns` walks slices of `rows`, so every end it yields is one of
/// those rows' ends.
#[derive(Clone, Debug)]
pub(crate) struct ViolationsIter<'b> {
    /// The slots still to read, with their arcs' ends, costs and states.
    columns: RowsZip<'b>,
    /// The rows the slots are of, which price them.
    rows: PricedRows<'b>,
}

impl BasisRoom {
    /// A room that holds nothing yet; it allocates nothing.
    pub(crate) const fn new() -> Self {
        Self { columns: RoomColumns::new(), net_supplies: NetSupplies::new() }
    }

    /// Makes room for the basis of a problem of up to `node_count` nodes and `arc_count` arcs: a load of such a
    /// problem, and every pivot of its basis, allocates nothing afterwards. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    pub(crate) fn try_reserve(&mut self, node_count: usize, arc_count: usize) -> Result<(), TryReserveError> {
        self.net_supplies.try_reserve(node_count)?;
        self.columns.try_reserve(node_count, arc_count)
    }

    /// Makes room for the basis of a problem of up to `node_count` nodes and `arc_count` arcs, as
    /// [`try_reserve`](Self::try_reserve) does, growing as a `Vec` does where the memory runs out.
    ///
    /// # Panics
    /// Panics if a column's room passes `isize::MAX` bytes, as a growing `Vec` does.
    pub(crate) fn grow(&mut self, node_count: usize, arc_count: usize) {
        self.net_supplies.grow(node_count);
        self.columns.grow(node_count, arc_count);
    }

    /// The star of `problem` at its own root cost, `M = (n − 1)·C + 1` for its `n` nodes and largest cost magnitude
    /// `C`, written into this room and lent from it as a basis with its start: each node's supply net of the lower
    /// bounds gathered; every arc at its dealt slot with its width, out of the tree and carrying nothing, `AtLower`, or
    /// `Unpriced` where it has no width; every node hanging from the root by its root link, carrying the magnitude of
    /// its net supply, at potential zero below a link that points up and `M` below one that points down. Nothing of an
    /// earlier basis is kept but the room.
    ///
    /// # Panics
    /// On a target whose `usize` has 32 bits it can panic while it refills a column, where the column's room passes
    /// `isize::MAX` bytes: the flows take `8·(m + n)`.
    ///
    /// # Errors
    /// Returns [`ProblemError::NetSupplyBeyondRange`] where the positive net supplies add up past their limit; the
    /// columns are then as they were.
    #[track_caller]
    pub(crate) fn load<'p>(&mut self, problem: CheckedProblem<'p>) -> Result<LoadedBasis<'_, 'p>, ProblemError> {
        let Self { columns, net_supplies } = self;
        let start = net_supplies.gather(problem)?;

        // The problem's own root cost lies within its root costs (`root_costs`).
        Ok(LoadedBasis { basis: columns.star(&start, problem.root_cost()), start })
    }
}

impl KeptBasisRoom {
    /// A room that holds nothing yet; it allocates nothing.
    pub(crate) const fn new() -> Self {
        Self { room: BasisRoom::new(), excesses: Excesses::new(), kept: Kept::Nothing }
    }

    /// Makes room for the basis of a problem of up to `node_count` nodes and `arc_count` arcs and for the excesses
    /// over it: a start of such a problem, from the star or from the basis held, and every pivot and change of its
    /// basis, allocates nothing afterwards. Room already held is kept, and nothing else changes: a basis held stays.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    pub(crate) fn try_reserve(&mut self, node_count: usize, arc_count: usize) -> Result<(), TryReserveError> {
        self.excesses.try_reserve(node_count)?;
        self.room.try_reserve(node_count, arc_count)
    }

    /// Makes room as [`try_reserve`](Self::try_reserve) does, growing as a `Vec` does where the memory runs out.
    /// Nothing else changes.
    ///
    /// # Panics
    /// Panics if a column's room passes `isize::MAX` bytes, as a growing `Vec` does.
    pub(crate) fn grow(&mut self, node_count: usize, arc_count: usize) {
        self.excesses.grow(node_count);
        self.room.grow(node_count, arc_count);
    }

    /// The start of `problem` decided, before anything of this room's columns is written: its net supplies gathered,
    /// and the basis the last call ended on where the room holds one of the problem's node and arc counts and of the
    /// same tail and head at every arc, and the excesses over it keep the limit of a start; otherwise the star. Its
    /// [`begin`](Resumption::begin) makes that basis, at a root cost it is given.
    ///
    /// The decision writes nothing of the columns or the marker: it gathers the net supplies, reads the counts, the
    /// ends and the states of the basis held, and writes the excesses over it. It costs a pass over the arcs and a few
    /// over the nodes, and allocates nothing where the room holds room for the problem.
    ///
    /// # Panics
    /// On a target whose `usize` has 32 bits it can panic where a scratch room grows past `isize::MAX` bytes.
    ///
    /// # Errors
    /// Returns [`ProblemError::NetSupplyBeyondRange`] where the positive net supplies add up past their limit; the
    /// room then holds what it held.
    pub(crate) fn resume<'p>(&mut self, problem: CheckedProblem<'p>) -> Result<Resumption<'_, 'p>, ProblemError> {
        let Self { room: BasisRoom { columns, net_supplies }, excesses, kept } = self;
        let start = net_supplies.gather(problem)?;
        let decided = match *kept {
            Kept::Nothing => DecidedStart::Star(start),
            Kept::Basis => match columns.kept_start(&start, excesses) {
                Some(serving) => DecidedStart::Kept { start, serving },
                None => DecidedStart::Star(start),
            },
        };

        // The room held a basis that serves the problem exactly where the start is `Kept`, as just decided, and the
        // columns and the marker are held from here on: `Resumption`'s invariant.
        Ok(Resumption { columns, kept, start: decided })
    }

    /// The basis this room holds, lent again as the call that kept it left it, every column and the tree as they
    /// stand; `None` where the room holds none, its own marker then saying so. The room holds no basis from then on
    /// until the basis lent is kept again. It costs nothing more.
    pub(crate) fn reopen(&mut self) -> Option<LentBasis<'_, Basis<'_>>> {
        if mem::take(&mut self.kept) == Kept::Nothing {
            return None;
        }
        self.room.columns.tree.kept()?;

        let RoomColumns { ends, costs, states, capacities, flows, tree, root_cost } = &mut self.room.columns;
        // The room held the basis `LentBasis::keep` left, beside the tree it kept, and nothing has written either since
        // (`KeptBasisRoom`): the columns and the tree keep `Basis`'s invariant, at the root cost the columns hold. The
        // tree room keeps a tree, as just read: the resume does not panic.
        let tree = tree.resume();
        let (arcs, links) = (ArcColumns { ends, costs, states }, LinkColumns { capacities, flows });
        let basis = Basis { arcs, links, tree, root_cost: *root_cost };

        Some(LentBasis { basis, kept: &mut self.kept })
    }

    /// Makes this room a copy of `source`, and lends the copy of the basis `source` holds, as
    /// [`reopen`](Self::reopen) lends a kept one: every column and the tree as `source`'s, at its root cost, the room
    /// holding no basis until the basis lent is kept. Where `source` holds no basis, the room is a copy of one that
    /// holds nothing, every column emptied, and nothing is lent; where its marker says it holds one beside a tree its
    /// tree room no longer keeps, which no call of the crate leaves, nothing is lent either. The scratch rooms of the
    /// net supplies and the excesses are not copied, but grown to hold `source`'s node count, so that a start of
    /// `source`'s counts in the copy allocates where one in `source` does. `source` is unchanged. It costs a copy of
    /// the columns of `source`'s basis, and allocates nothing where the room holds room for that basis, as
    /// [`try_reserve`](Self::try_reserve) of its node and arc counts makes it.
    pub(crate) fn copy_from(&mut self, source: &Self) -> Option<LentBasis<'_, Basis<'_>>> {
        // The room holds no basis from its first write on.
        self.kept = Kept::Nothing;
        let nothing = RoomColumns::new();
        let copied = match source.kept {
            Kept::Basis => &source.room.columns,
            Kept::Nothing => &nothing,
        };
        let node_count = source.room.columns.tree.kept().map_or(0, |kept| kept.node_count);
        self.room.net_supplies.grow(node_count);
        self.excesses.grow(node_count);

        let RoomColumns { ends, costs, states, capacities, flows, tree, root_cost } = &mut self.room.columns;
        ends.clone_from(&copied.ends);
        costs.clone_from(&copied.costs);
        states.clone_from(&copied.states);
        capacities.clone_from(&copied.capacities);
        flows.clone_from(&copied.flows);
        *root_cost = copied.root_cost;

        let tree = tree.copy_from(&copied.tree)?;
        // A tree was lent, so `source` held the basis `LentBasis::keep` left, beside the tree it kept, at the root
        // cost its columns hold, and nothing has written either since (`KeptBasisRoom`): copied whole, the columns and
        // the tree keep `Basis`'s invariant at that root cost.
        let (arcs, links) = (ArcColumns { ends, costs, states }, LinkColumns { capacities, flows });
        let basis = Basis { arcs, links, tree, root_cost: *root_cost };

        Some(LentBasis { basis, kept: &mut self.kept })
    }

    /// What the basis this room holds carries on its arcs, by slot, and each node's potential, read in place; `None`
    /// where the room holds none. It reads, and changes nothing.
    pub(crate) fn kept(&self) -> Option<KeptColumns<'_>> {
        // The basis holds `m + n` links, the arcs' first, and `n + 1` potentials, the root's last (`Basis`, Shape).
        let columns = self.whole_basis()?;
        let (_root, potentials) = columns.tree.kept_potentials()?.split_last()?;
        let arc_flows = columns.flows.get(..columns.ends.len())?;

        Some(KeptColumns { arc_flows, potentials })
    }

    /// The numbers of the problem the basis this room holds is of, by slot, read in place: each arc's ends, cost and
    /// width; `None` where the room holds none. It reads, and changes nothing.
    pub(crate) fn kept_problem(&self) -> Option<KeptProblem<'_>> {
        // The basis holds `m` slots in its arc columns and `m + n` links, the arcs' first (`Basis`, Shape).
        let columns = self.whole_basis()?;
        let widths = columns.capacities.get(..columns.ends.len())?;

        Some(KeptProblem { ends: &columns.ends, costs: &columns.costs, widths })
    }

    /// The arc at `slot` of the basis this room holds, read alone: its cost, its width and what it carries; `None`
    /// where the room holds none, or where `slot` is not below its arc count. It reads, and changes nothing.
    pub(crate) fn kept_arc(&self, slot: Slot) -> Option<KeptArc> {
        let KeptRow { cost, width, flow, .. } = self.kept_row(slot)?;

        Some(KeptArc { cost, width, flow })
    }

    /// The flow that [`LentBasis::restate`] of the arc at `slot` for `restated` would move in the basis this room
    /// holds, in units of flow: the most it can raise the flow up the root links by, below `2^66`. `None` where the
    /// room holds no basis, or where `slot` is not below its arc count. It reads, and changes nothing.
    pub(crate) fn amount_to_route(&self, slot: Slot, restated: Restated) -> Option<u128> {
        let KeptRow { state, width, flow, .. } = self.kept_row(slot)?;
        let arc = restated.of_flow(flow);

        Some(Restatement::of(state, width, arc).amount(arc))
    }

    /// The columns of this room, where they hold a whole basis, as its marker says; `None` where they hold none.
    fn whole_basis(&self) -> Option<&RoomColumns> {
        (self.kept == Kept::Basis).then_some(&self.room.columns)
    }

    /// The row at `slot` of the basis this room holds, read alone; `None` where the room holds none, or where `slot`
    /// is not below its arc count.
    fn kept_row(&self, slot: Slot) -> Option<KeptRow> {
        let columns = self.whole_basis()?;

        let at = usize::from(slot);
        if at >= columns.costs.len() {
            return None;
        }

        debug_assert!(
            at < columns.costs.len()
                && at < columns.states.len()
                && at < columns.capacities.len()
                && at < columns.flows.len(),
            "a slot below the arc count should be below the costs' and the states' m and the links' m + n"
        );
        // SAFETY: the room holds a basis (`KeptBasisRoom`: its marker says so, as `whole_basis` read), whose costs and
        // states hold `m` slots and whose link columns hold `m + n` links (`Basis`, Shape); `at` is below `m`, as
        // just compared.
        let (cost, state, width, flow) = unsafe {
            (
                *columns.costs.get_unchecked(at),
                *columns.states.get_unchecked(at),
                *columns.capacities.get_unchecked(at),
                *columns.flows.get_unchecked(at),
            )
        };

        Some(KeptRow { cost, state, width, flow })
    }
}

impl fmt::Debug for KeptBasisRoom {
    /// The columns and the marker; the scratch rooms of the net supplies and the excesses, which a start rewrites
    /// before it reads them and a copy does not copy, are left out, so that a copy prints as its source.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeptBasisRoom")
            .field("columns", &self.room.columns)
            .field("kept", &self.kept)
            .finish_non_exhaustive()
    }
}

impl RoomColumns {
    /// Columns that hold nothing yet; they allocate nothing.
    const fn new() -> Self {
        Self {
            ends: Vec::new(),
            costs: Vec::new(),
            states: Vec::new(),
            capacities: Vec::new(),
            flows: Vec::new(),
            tree: TreeRoom::new(),
            root_cost: 0,
        }
    }

    /// Makes room for the basis of a problem of up to `node_count` nodes and `arc_count` arcs. Room already held is
    /// kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    fn try_reserve(&mut self, node_count: usize, arc_count: usize) -> Result<(), TryReserveError> {
        let link_count = arc_count.saturating_add(node_count);

        reserve_total(&mut self.ends, arc_count)?;
        reserve_total(&mut self.costs, arc_count)?;
        reserve_total(&mut self.states, arc_count)?;
        reserve_total(&mut self.capacities, link_count)?;
        reserve_total(&mut self.flows, link_count)?;
        self.tree.try_reserve(node_count)
    }

    /// Makes room as [`try_reserve`](Self::try_reserve) does, growing as a `Vec` does where the memory runs out.
    ///
    /// # Panics
    /// Panics if a column's room passes `isize::MAX` bytes, as a growing `Vec` does.
    fn grow(&mut self, node_count: usize, arc_count: usize) {
        let link_count = arc_count.saturating_add(node_count);

        grow_total(&mut self.ends, arc_count);
        grow_total(&mut self.costs, arc_count);
        grow_total(&mut self.states, arc_count);
        grow_total(&mut self.capacities, link_count);
        grow_total(&mut self.flows, link_count);
        self.tree.grow(node_count);
    }

    /// The star of `start` at the root cost `root_cost`, which lies within the problem's [`root_costs`], written into
    /// these columns and lent from them, as [`BasisRoom::load`] says.
    fn star(&mut self, start: &Start<'_, '_>, root_cost: i64) -> Basis<'_> {
        let problem = start.problem();
        self.root_cost = root_cost;
        self.place_links(start);
        self.place_arcs(problem);

        let Self { ends, costs, states, capacities, flows, tree, .. } = self;
        let directions = start.root_links().map(|root_link| root_link.direction);
        // A checked problem keeps `n + 1` and `m + n` within the index limit, and the start gives one root link a
        // node: the star does not panic.
        let tree = tree.star(problem.node_count(), problem.arc_count(), directions, root_cost);

        let arcs = ArcColumns { ends, costs, states };
        Basis { arcs, links: LinkColumns { capacities, flows }, tree, root_cost }
    }

    /// The start of the problem of `start` over the basis these columns hold, a whole one as the kept room's marker
    /// says, written into `excesses`, where that basis is of the problem's counts and ends, and the excesses keep the
    /// limit; `None` otherwise. It writes nothing but `excesses`.
    fn kept_start<'p, 'e>(&self, start: &Start<'p, '_>, excesses: &'e mut Excesses) -> Option<ServingStart<'p, 'e>> {
        let shape = self.kept_shape(start.problem()).filter(KeptShape::are_of_problem)?;

        // Each arc out of the tree carries what its kept state gives it of its new width; a tree arc carries nothing
        // here, and the re-flow of the tree writes it.
        let start = match start.problem().arcs() {
            CheckedArcs::WithoutLowerBounds(arcs) => excesses.gather(start, shape.carried(arcs)),
            CheckedArcs::WithLowerBounds(arcs) => excesses.gather(start, shape.carried(arcs)),
        }?;

        // `kept_shape` compared the kept tree's counts and every column's length with the problem's counts, and
        // `are_of_problem` every slot's ends: `ServingStart`'s invariant.
        Some(ServingStart { start })
    }

    /// The ends and states of the basis these columns hold, where the tree its tree room keeps is of the counts of
    /// `problem`, `n` nodes and `m` arcs, and the arc and link columns hold them; `None` otherwise.
    fn kept_shape<'p>(&self, problem: CheckedProblem<'p>) -> Option<KeptShape<'_, 'p>> {
        let (node_count, arc_count) = (problem.node_count(), problem.arc_count());
        let kept = self.tree.kept()?;
        // A checked problem keeps `m + n` within the index limit: the sum does not overflow.
        let link_count = arc_count + node_count;
        let counts = [kept.node_count, kept.link_count, self.ends.len(), self.states.len()];
        let links = [self.capacities.len(), self.flows.len()];

        (counts == [node_count, link_count, arc_count, arc_count] && links == [link_count, link_count])
            .then_some(KeptShape { ends: &self.ends, states: &self.states, problem })
    }

    /// The basis `serving` begins from: the basis these columns hold, which serves its problem, re-flowed, at the root
    /// cost `root_cost`, which lies within the problem's [`root_costs`].
    fn reflow(&mut self, serving: ServingStart<'_, '_>, root_cost: i64) -> Basis<'_> {
        let ServingStart { start } = serving;
        let problem = start.problem();

        // The arcs' pass: every arc at its slot with its new ends, cost and width, carrying what its kept state gives
        // it. The ends are the kept ones, so the tree's links join the same nodes as before.
        self.place_arcs(problem);
        self.root_cost = root_cost;

        let Self { ends, costs, states, capacities, flows, tree, .. } = self;
        // The tree room keeps a tree (`ServingStart`): the tree's resume does not panic.
        let tree = tree.resume();
        let columns = ReflowColumns {
            arcs: ArcColumns { ends, costs, states },
            links: LinkColumns { capacities, flows },
            excesses: start.into_excesses(),
        };
        // The flows the pass from the leaves up writes read no cost, so they are the same at every root cost. The root
        // cost lies within the problem's `root_costs`: at least the problem's own, so that `M − (n − 1)·C ≥ 1`, all
        // that `NET_SUPPLY_LIMIT` and the decision whether every supply is routed ask of it; at most the widest of `n`
        // nodes, so that every potential the pass from the root down sets, for whatever tree the first pass left, and
        // every reduced cost and partial sum formed from them stays below `2^63` in magnitude (`widest_root_cost`).
        let Reflow { tree, columns: ReflowColumns { arcs, links, .. } } =
            Reflow { tree, columns }.flow_up().tighten(root_cost);

        Basis { arcs, links, tree, root_cost }
    }

    /// Rewrites the arc columns as the arcs of `problem` at their dealt slots, `m` entries each, and writes each arc's
    /// width as the capacity of its slot's link: each arc with its ends, cost and width, carrying what the state its
    /// slot held gives it ([`ArcState::carried`]) and standing where that flow puts it ([`ArcState::at`]).
    ///
    /// # Panics
    /// Panics if the link columns hold fewer than `m` links or the states fewer than `m` entries:
    /// [`place_links`](Self::place_links) places them first.
    fn place_arcs(&mut self, problem: CheckedProblem<'_>) {
        let (arc_count, node_count) = (problem.arc_count(), problem.node_count());
        match problem.arcs() {
            CheckedArcs::WithoutLowerBounds(arcs) => self.scatter(arcs, arc_count, node_count),
            CheckedArcs::WithLowerBounds(arcs) => self.scatter(arcs, arc_count, node_count),
        }
    }

    /// The arc columns as `arcs`, the arcs of a problem of `arc_count` arcs and `node_count` nodes in ascending arc
    /// order, each at the slot the dealing of those counts gives it, the states holding one state a slot: afterwards
    /// the three arc columns hold `arc_count` slots, each with its arc's ends and cost; the first `arc_count`
    /// capacities hold the arcs' widths and the first `arc_count` flows what each arc carries, its width where the
    /// state its slot held was `AtUpper` and nothing otherwise; and each arc stands where [`ArcState::at`] puts that
    /// flow. The capacities and flows past them are as they were. Every end of `arcs` is below `node_count`.
    ///
    /// # Panics
    /// Panics if the states, the capacities or the flows hold fewer than `arc_count` entries.
    fn scatter(&mut self, arcs: impl Iterator<Item = CheckedArc>, arc_count: usize, node_count: usize) {
        let state_count = self.states.len();

        // Every slot is written below, since the dealing is a bijection onto the slots; the refill only sets the
        // lengths.
        refill(&mut self.ends, iter::repeat_n(Ends { tail: 0, head: 0 }, arc_count));
        refill(&mut self.costs, iter::repeat_n(0, arc_count));
        let (ends, costs) = (&mut self.ends, &mut self.costs);
        let (widths, flows) = (&mut self.capacities[..arc_count], &mut self.flows[..arc_count]);
        let states = &mut self.states[..arc_count];
        debug_assert_eq!(state_count, arc_count, "the states should hold the state each slot held, one a slot");

        for (arc, slot) in arcs.zip(DealtSlotsIter::new(arc_count, node_count)) {
            let at = usize::from(slot);

            debug_assert!(
                at < ends.len() && at < costs.len() && at < states.len() && at < widths.len() && at < flows.len(),
                "a dealt slot should be below the arc count, the length of the arc columns, the widths and the flows"
            );
            // SAFETY: `DealtSlotsIter` yields only slots below the arc count it was made for (its invariant), which
            // is the length the ends and the costs were refilled to and the length of the states, the widths and the
            // flows sliced above.
            let (ends_at, cost_at, state_at, width_at, flow_at) = unsafe {
                (
                    ends.get_unchecked_mut(at),
                    costs.get_unchecked_mut(at),
                    states.get_unchecked_mut(at),
                    widths.get_unchecked_mut(at),
                    flows.get_unchecked_mut(at),
                )
            };

            *ends_at = Ends::of(arc);
            *cost_at = arc.cost();
            *width_at = arc.width();
            *flow_at = state_at.carried(*width_at);
            *state_at = ArcState::at(*flow_at, *width_at);
        }
    }

    /// Rewrites the link columns as the `m + n` links of `start`, each column whole, and the states as the arcs' in the
    /// star: the arcs at their slots, carrying nothing at their lower bounds (`AtLower`), of capacity zero until
    /// [`place_arcs`](Self::place_arcs) writes their widths; then each node's root link in ascending node order, of
    /// capacity `u64::MAX`, carrying the magnitude of the node's net supply.
    fn place_links(&mut self, start: &Start<'_, '_>) {
        let (arc_count, root_links) = (start.problem().arc_count(), start.root_links());

        refill(&mut self.states, iter::repeat_n(ArcState::AtLower, arc_count));
        refill(&mut self.capacities, iter::repeat_n(0, arc_count).chain(iter::repeat_n(u64::MAX, root_links.len())));
        refill(&mut self.flows, iter::repeat_n(0, arc_count).chain(root_links.map(|root_link| root_link.flow)));
    }
}

impl<'r, 'p> Resumption<'r, 'p> {
    /// The basis decided on, made at the root cost `root_cost` and lent from the room with its start and the room's
    /// marker: the basis the room held, re-flowed, or the star, as [`BasisRoom::load`] makes it at the problem's own
    /// root cost.
    ///
    /// The re-flow keeps the tree and where each arc out of it stands, and reads every number anew from the problem:
    /// an arc out of the tree carries its new width at its capacity and nothing at its lower bound; each node's excess
    /// is sent toward the root along its link, from the leaves up, and a link that cannot carry it, or could carry no
    /// more toward the root, leaves the tree for its node's root link; every potential is made tight from the root
    /// down, a root link that points down at `root_cost`. Its flows are the same at every root cost. It costs a few
    /// passes over the arcs and the nodes, O(m + n). Either way the room holds no basis until the basis lent is kept.
    ///
    /// # Panics
    /// Panics if `root_cost` lies outside the problem's root costs: below the problem's own, or above `(n − 1)·B + 1`
    /// for the cost bound `B` of `n` nodes ([`widest_root_cost`]). On a target whose `usize` has 32 bits it can also
    /// panic, as [`BasisRoom::load`] does, while it refills a column.
    #[track_caller]
    pub(crate) fn begin(self, root_cost: i64) -> LentBasis<'r, LoadedBasis<'r, 'p>> {
        let Self { columns, kept, start } = self;
        let serving = root_costs(start.problem());
        assert!(
            serving.contains(&root_cost),
            "called `Resumption::begin` with the root cost {root_cost}, which should lie within {serving:?}"
        );

        // The first write: the room holds no basis until the basis lent is kept. The columns were not written since the
        // decision (`Resumption`): a kept start still serves them.
        *kept = Kept::Nothing;
        let loaded = match start {
            DecidedStart::Kept { start, serving } => LoadedBasis { basis: columns.reflow(serving, root_cost), start },
            DecidedStart::Star(start) => LoadedBasis { basis: columns.star(&start, root_cost), start },
        };

        LentBasis { basis: loaded, kept }
    }

    /// Which start the decision took, for a test to tell.
    #[cfg(test)]
    pub(crate) fn kept(&self) -> bool {
        matches!(self.start, DecidedStart::Kept { .. })
    }
}

impl<'p> DecidedStart<'p, '_> {
    /// The problem the start is of.
    fn problem(&self) -> CheckedProblem<'p> {
        match self {
            Self::Kept { start, .. } | Self::Star(start) => start.problem(),
        }
    }
}

impl<'r> Basis<'r> {
    /// The rows the pricing reads: per slot the arc's ends, cost and state, with every tree node's potential.
    pub(crate) fn rows(&self) -> PricedRows<'_> {
        let ArcColumns { ends, costs, states } = &self.arcs;

        PricedRows { ends, costs, states, potentials: self.tree.potentials() }
    }

    /// Whether the arc at `slot` violates its bound: an arc at its lower bound of negative reduced cost, or at its
    /// capacity of positive reduced cost. An arc in the tree or of no width never does.
    ///
    /// # Panics
    /// Panics if `slot` is not below the arc count.
    #[track_caller]
    pub(crate) fn violates(&self, slot: Slot) -> bool {
        let Some(at) = self.arcs.slot(slot) else { violates_outside_slots(slot, self.arcs.len()) };

        self.rows().violation(at) < 0
    }

    /// The violation of the arc at `slot`, in units of cost: its pricing sign times its reduced cost, negative exactly
    /// where it violates its bound, as the rows price it. An arc in the tree or of no width has none, zero.
    ///
    /// # Panics
    /// Panics if `slot` is not below the arc count.
    #[track_caller]
    pub(crate) fn violation(&self, slot: Slot) -> i64 {
        let Some(at) = self.arcs.slot(slot) else { violation_outside_slots(slot, self.arcs.len()) };

        self.rows().violation(at)
    }

    /// Brings the arc at `entering` into the basis along the cycle it closes with the tree. It sends the most flow the
    /// cycle carries around it, the least room of the arc and the cycle's links. Where that is a link's room, the last
    /// link of least room met from the apex in the cycle's direction, the arc counted where it lies between the two
    /// sides, leaves the tree and the arc takes its place; where it is the arc's own width alone, the arc moves to its
    /// other bound.
    ///
    /// It answers the flow it sent around the cycle, in units of flow: zero exactly where the pivot is degenerate, a
    /// link of no room leaving; the arc's width, above zero, where the arc moved to its other bound.
    ///
    /// The basis keeps its invariant, and the cost of its flow, root links counted at their costs, falls by the flow
    /// sent times the arc's violation in magnitude. It walks the cycle from both ends up to their apex once, its two
    /// sides again where flow is sent, and costs an exchange the paths the exchange touches; it allocates nothing.
    ///
    /// # Panics
    /// Panics if `entering` is not below the arc count, or if its arc is not priced: in the tree, or of no width.
    #[track_caller]
    pub(crate) fn pivot(&mut self, entering: Slot) -> u64 {
        // Every pivot ends: an argument that uses no property of the entering rule. Costs and widths are fixed for the
        // run, so the potentials are a function of the tree (the root at zero, every tree link tight), and the states
        // are a function of the tree and the flow (`Basis`). The leaving rule keeps the basis strongly feasible
        // (`least_room`).
        //
        // - A pivot that sends flow lowers the cost of the flow, root links counted, by the step times the arc's
        //   violation in magnitude: the cycle's cost is the sum of its links' reduced costs, zero on the tree's. The
        //   step is a whole unit at least and so is the violation, so the cost falls by one at least; it is an integer
        //   bounded below, so there are finitely many such pivots, and no flow once left returns.
        // - A pivot that sends nothing has a leaving link of room zero. Strong feasibility gives every link on the
        //   second side, where the cycle's flow runs toward the root, room above zero, and the arc's width is above
        //   zero, since arcs of no width are not priced; so the link is on the first side, and the moved subtree holds
        //   the first end. `entering_link` shifts it by the violation in magnitude, above zero, and no other potential
        //   moves: the sum of the potentials rises strictly. So within a stretch of such pivots, at one flow, no tree
        //   returns, and there are finitely many trees.
        // - A pivot with no leaving link sends the arc's whole width, above zero: a pivot of the first kind.
        let Some(slot) = self.arcs.slot(entering) else { pivot_outside_slots(entering, self.arcs.len()) };
        let Some(arc) = self.rows().entering(slot, &self.links) else { pivot_unpriced(entering) };

        let link_count = self.tree.link_count();
        debug_assert!(
            arc.first < self.tree.node_count() && arc.second < self.tree.node_count(),
            "an arc's ends should be below the tree's hung nodes"
        );
        // SAFETY: both ends of an arc lie below the problem's node count `n` (`Ends`), and the tree has `n` hung nodes
        // (`Basis`, Shape).
        let mut walk = unsafe { CycleWalk::new_unchecked(&mut self.tree, arc.first, arc.second) };
        let RatioTest { step, leaving } = self.links.least_room(&mut walk, arc.width);
        let cycle = walk.into_cycle(leaving);
        self.links.send_around(&cycle, &arc, step);

        match Exchange::of(cycle) {
            None => self.arcs.flip(&arc),
            Some(exchange) => {
                self.arcs.exchange_states(&arc, exchange.leaving_link(), &self.links);
                let EnteringLink { direction, shift } = entering_link(arc.bound, exchange.side(), arc.reduced_cost);

                debug_assert!(arc.slot.0 < link_count, "an arc's slot should be below the tree's link count");
                // SAFETY: an `ArcSlot` is below the arc count `m` (`ArcSlot`), and the tree has `m + n` links (`Basis`,
                // Shape).
                unsafe { exchange.apply_unchecked(arc.slot.0, direction, shift) };
            }
        }

        step
    }

    /// Makes `cost` the cost of a unit of flow on the arc at `slot`, in units of cost: the basis is then a basis of the
    /// problem with that cost. No flow, state or link of the tree moves. Where the arc is the link of a node, every
    /// potential of that node's subtree moves alike so that the link stays tight; no other potential moves. So only the
    /// arc's own reduced cost changes and those of the arcs with exactly one end in that subtree. It answers that node
    /// where its subtree's potentials moved, and `None` where the arc is out of the tree or keeps its cost.
    ///
    /// It costs O(1) for an arc out of the tree, and for a tree arc the nodes of the subtree it hangs: for an arc near
    /// the root, most of the tree.
    ///
    /// # Panics
    /// Panics if `slot` is not below the arc count, if `cost` lies beyond plus or minus [`cost_bound`] of the node
    /// count, or if the root cost the basis holds is below `(n − 1)·|cost| + 1`, the root cost a problem with that cost
    /// asks for ([`root_cost`]).
    #[track_caller]
    pub(crate) fn set_cost(&mut self, slot: Slot, cost: i64) -> Option<usize> {
        let Some(at) = self.arcs.slot(slot) else { set_cost_outside_slots(slot, self.arcs.len()) };
        let node_count = self.tree.node_count();
        let bound = cost_bound(node_count);
        assert!(
            within_cost_bound(cost, bound),
            "called `Basis::set_cost` with the cost {cost}, which should lie within ±{bound} for {node_count} nodes"
        );
        // Within the cost bound the root cost a cost asks for is below `2^62`, so `root_cost` does not panic.
        let asked = root_cost(node_count, cost.unsigned_abs());
        assert!(
            asked <= self.root_cost,
            "called `Basis::set_cost` with the cost {cost}, which asks for the root cost {asked}, above the basis's {}",
            self.root_cost
        );

        // The basis is a basis of the problem with the new cost, item by item. *Shape*, *Tree*, *Flow* and *Strong
        // feasibility* read no cost but the one written, and no flow, state or link moves. *Duality*: the root lies in
        // no subtree, so it stays at zero; a tree link inside the shifted subtree has both ends shifted alike; the
        // arc's own link, where it is one, is tight again, which `LentTree::retighten` makes it; every other tree
        // link has neither end in the subtree. So the potentials are again the ones the tree implies, under costs
        // within the cost bound and a root cost within `(n − 1)·C + 1 ..= widest_root_cost(n)`, the lower end just
        // asserted for the new cost: the argument written in `cost_bound` and `widest_root_cost` bounds every
        // potential, reduced cost and partial sum below `2^63`, for any tree. Both costs lie within the cost bound,
        // `2^62` at most, so their difference fits an `i64`.
        let change = cost - self.arcs.cost(at);
        let hanging = self.hanging_node(at).filter(|_| change != 0);
        self.arcs.set_cost(at, cost);

        if let Some(node) = hanging {
            let retightened = self.tree.retighten(node, change);
            retightened.expect("a node that hangs by an arc should be below the node count");
        }

        hanging
    }

    /// Whether every root link carries nothing. Where it holds, the arcs' flows, their lower bounds added, meet every
    /// supply; where no arc violates its bound and it does not hold, no flow within the bounds meets the supplies. So a
    /// run at its end is optimal where it holds and has no flow otherwise: the one rule both front ends end a run by.
    /// It reads the root links' flows in order and stops at the first that carries flow: at most `n` reads.
    pub(crate) fn routes_every_supply(&self) -> bool {
        // Where no arc violates and a root link carries flow, no flow within the bounds meets the supplies. Suppose one
        // does, `x*`, and a root link that points down, into `d`, carries flow in the basis's `x`. The difference
        // `x* − x` is a circulation of the extended network that only lowers root-link flows, so it decomposes into
        // cycles conformal to it, one of which enters the root by lowering the flow down `d`'s link and leaves it by
        // lowering the flow up some `s`'s link, which points up; the rest of it is a simple path from `s` to `d` of at
        // most `n − 1` arcs, so its cost is at most `−M + (n − 1)·C ≤ −1`, the root cost being at least the problem's
        // own (`root_cost`). Every step of it can move in `x`: each arc out of the tree stands at the bound it moves
        // away from and does not violate, so its reduced cost in the step's direction is at least zero; each tree
        // link, the two root links among them, which carry flow, is tight. So the cycle's cost, the sum of its steps'
        // reduced costs, is at least zero: a contradiction. Every root link that points down carries nothing, and with
        // the root conserving, every one that points up.
        // No property of the start is used: which way each root link points is whatever the start set.
        //
        // Where every root link carries nothing, the arcs' flows meet the net supplies, and with the lower bounds
        // added the supplies. No arc of positive width violates, each tree arc is tight, and an arc of no width
        // carries its lower bound in every flow within the bounds: so the arcs meet the optimality conditions of the
        // problem itself under the potentials of the first `n` nodes, and the flow is of least cost. Nothing is claimed
        // of the root links out of the tree, which are never priced.
        //
        // The root links follow the `m` arcs in the link column (`Basis`, Shape).
        self.links.flows.iter().skip(self.arcs.len()).all(|&flow| flow == 0)
    }

    /// The flow up the root links, in units of flow, which equals the flow down them: zero exactly where
    /// [`routes_every_supply`](Self::routes_every_supply) holds. It reads the root link of each node that hangs from
    /// the root, once.
    ///
    /// # Panics
    /// Panics if it passes `u64::MAX`, which the basis's invariant rules out: it is at most [`NET_SUPPLY_LIMIT`].
    pub(crate) fn unrouted(&self) -> u64 {
        // Every root link carries nothing exactly where the flow up them is zero: one out of the tree carries nothing
        // (`Basis`, Flow), and the root conserves, so the flow down them equals the flow up.
        let unrouted = self.flow_up_root_links();

        u64::try_from(unrouted).expect("the flow up the root links should be at most NET_SUPPLY_LIMIT")
    }

    /// Makes the basis a basis of the problem in which `from` supplies `amount` more and `to` demands `amount` more:
    /// `amount` more flow leaves `from` and arrives at `to`. It answers the basis with what the cost of its flow, root
    /// links counted at their costs, gained, modulo `2^128`, and what the flow up the root links gained.
    ///
    /// The amount is sent from each of the two nodes along its path in the tree up to the deepest node the two paths
    /// share, and what the two paths hand that node, added up, on from it toward the root. Each node's link carries
    /// what it can of what arrives at it and hands its parent the change of what it carries. A link that cannot carry
    /// it, or could then carry no more toward the root, leaves the tree at its bound for its node's root link, which
    /// takes the rest: the node's subtree moves under the root with it. A node that hangs from the root takes on its
    /// root link what arrives, and turns it where its flow passes zero. Every potential that moves moves with a whole
    /// subtree, so that every tree link is tight. Where the flow up the root links before and `amount` add up to at
    /// most [`NET_SUPPLY_LIMIT`], the flow up the root links gains at most `amount`.
    ///
    /// Each node that hangs from the root once the route moved its subtree's potentials, by a cut or a turn, is pushed
    /// onto `moved`, once, after what it held; the route ends with each such node's subtree the one whose potentials
    /// moved with it. So the arcs whose reduced costs changed are among those with exactly one end in one of those
    /// subtrees, and the arcs whose states changed are those that left the tree, each of which joins a node pushed to
    /// the rest of the tree, or joins a node whose potentials did not move to its old parent and stands tight there.
    ///
    /// It costs the two paths up to where they meet, where the tree carries the amount; each link that cannot carry it,
    /// the nodes below that link, whose potentials move, and the path from it to the root; each root link that turns,
    /// every node under it, which can be most of the tree; and, where the flow up the root links rises, a read of the
    /// root link of each node that hangs from the root. It allocates nothing where `moved` has room for one more node
    /// for each node of the tree.
    ///
    /// # Panics
    /// Panics if `from` or `to` is not below the node count; or if the root links would carry more toward the root than
    /// [`NET_SUPPLY_LIMIT`], or one of them more than `u64::MAX`, which no amount does that adds up with the flow up
    /// the root links before to at most that limit.
    #[track_caller]
    fn route(self, from: usize, to: usize, amount: u64, moved: &mut Vec<usize>) -> Rerouted<Self> {
        let node_count = self.tree.node_count();
        assert!(
            from < node_count && to < node_count,
            "called `Basis::route` with the nodes {from} and {to}, which should be below the node count {node_count}"
        );

        // The basis is a basis of the problem with the supplies moved, item by item:
        //
        // - *Shape* and the arcs' costs and widths: nothing of them is written.
        // - *Flow*: `from` holds `amount` more and `to` `amount` less to send toward the root. Each step decides one
        //   node with what arrived at it: its link carries a flow within its width and hands its parent the change of
        //   what it carries, and the node's root link, where it takes one, keeps the rest, so every node conserves; a
        //   root link out of the tree carried nothing and is written only where it enters. What the two paths hand
        //   their apex goes on toward the root until a step hands on nothing, which a node that hangs from the root
        //   always does: the root conserves too, the two amounts adding up to zero. An arc that leaves stands at a
        //   bound, `resend`'s own argument.
        // - *Strong feasibility*: an arc stays only with room toward the root (`resend`); a root link points down only
        //   where it carries more than nothing (`root_link`), and one that points up carries at most the flow up the
        //   root links, below its capacity `u64::MAX` by the assertion below where that flow rose.
        // - *Tree*: every decision is the walk's, which keeps the tree's invariant; a node that leaves its arc hangs
        //   from the root by its own root link. A step changes nothing above its node, so each walk meets the node it
        //   was given to stop at, and the apex stays an ancestor of both nodes: a cut on one path moves only nodes
        //   strictly below the apex, none of them on the other path.
        // - *Duality*: every move of potentials is of a whole subtree, so that the link it hangs by is tight under its
        //   cost, `M` for a root link that points down: the potentials are again the ones the tree implies, within the
        //   bounds `widest_root_cost` argues for any tree.
        //
        // The nodes pushed. A node is pushed as its subtree's potentials move, which it then tops under the root, and
        // no later step of the route moves a node into or out of that subtree: a cut on a path moves the subtree of a
        // node strictly below the apex, and the walk goes on at its old parent, above it; the subtree of a node cut on
        // one path holds no node of the other, which meets it only at the apex, nor the apex, nor any node above it,
        // so the other path and the walk above the apex never enter it; a node that hangs from the root takes what
        // arrives and ends its walk, so a turn is the last step of its walk, and where the apex is the root, so that
        // each path may turn its own top, the two paths lie in two components. No node is decided twice, so none is
        // pushed twice.
        //
        // The flow up the root links. Within a step, the positive parts of what arrives and of the root link the node
        // takes add up to no more than the positive part of what arrived: an arc that carries what arrives hands it
        // on, one that cannot keeps the rest on the root link, and a root link that takes what arrives adds it to its
        // signed flow. The two paths start with `amount` and `−amount`, and adding at the apex adds no positive part.
        // So the flow up the root links gains at most `amount`, and the flow down them, by the same argument with the
        // signs turned, at most `amount` too: every value of the route lies within plus or minus the flow up the root
        // links before plus `amount`, exact in an `i128`, and where that is at most `NET_SUPPLY_LIMIT` no root link
        // passes `u64::MAX` and neither `resend` nor `root_link` panics.
        let Self { arcs, links, tree, root_cost } = self;
        let mut columns = RouteColumns { arcs, links, root_cost, cost_gained: 0, up_gained: 0, moved };
        let tree = columns.route(tree, from, to, i128::from(amount));

        let RouteColumns { arcs, links, cost_gained, up_gained, .. } = columns;
        let basis = Basis { arcs, links, tree, root_cost };
        if let Some(unrouted) = basis.up_past_the_limit(up_gained) {
            panic!(
                "called `Basis::route` with the amount {amount}, which leaves {unrouted} up the root links, past \
                 {NET_SUPPLY_LIMIT}"
            );
        }

        Rerouted { basis, cost_gained, up_gained }
    }

    /// Makes the arc at `slot` an arc of the width `restated` gives it, its lower bound fallen as `restated` says:
    /// before any flow moves it carries above its new lower bound what it carried above its old one plus that fall,
    /// read at the slot. The basis is then a basis of the problem with the arc's new bounds. It
    /// answers the basis with what the cost of its flow, root links counted at their costs, gained, modulo `2^128`, and
    /// what the flow up the root links gained.
    ///
    /// The restatement itself moves no flow, so it moves no cost. An arc out of the tree stays where it is restated to
    /// carry nothing or its whole new width; otherwise it goes to the bound its state names, carrying nothing at its
    /// lower bound and its whole new width at its capacity, and the flow that moves is sent through the tree from one
    /// of its ends to the other, as [`route`](Self::route) sends it. A tree arc whose restated flow lies within its new
    /// width with room toward the root stays; otherwise it carries what its width admits, and its node hangs from the
    /// root by its root link, which takes the rest, and what its parent then has to send more is sent on toward the
    /// root, as `route` sends it. Where the flow up the root links before and the amount
    /// [`KeptBasisRoom::amount_to_route`] answers add up to at most [`NET_SUPPLY_LIMIT`], the flow up the root links
    /// gains at most that amount.
    ///
    /// Each node whose subtree's potentials it moved is pushed onto `moved` as `route` pushes them, with what that
    /// says of the arcs whose reduced costs or states changed; besides those, only the arc's own width, flow and state
    /// changed.
    ///
    /// It costs O(1) where no flow moves: an arc out of the tree restated at a bound, a tree arc restated within its
    /// width with room. Otherwise it costs what `route` costs, from the arc's two ends or from its node up. It
    /// allocates nothing where `moved` has room for one more node for each node of the tree.
    ///
    /// # Panics
    /// Panics if `slot` is not below the arc count; or if the root links would carry more toward the root than
    /// [`NET_SUPPLY_LIMIT`], or one of them more than `u64::MAX`, which no restatement does whose amount adds up with
    /// the flow up the root links before to at most that limit.
    #[track_caller]
    fn restate(self, slot: Slot, restated: Restated, moved: &mut Vec<usize>) -> Rerouted<Self> {
        let Some(at) = self.arcs.slot(slot) else { restate_outside_slots(slot, self.arcs.len()) };

        // The basis is a basis of the problem with the arc's new bounds, item by item:
        //
        // - *Shape*: the arc's width is its link's capacity, written first; nothing else of the shape changes.
        // - The restatement: the arc's flow and its lower bound are restated together, so every node's net supply and
        //   what the arc carries above its lower bound move alike, and no node's balance moves: with the restated
        //   flow in place of the flow held, every node conserves its new net supply. No cost moves either, since the
        //   arc's whole flow does not; only a flow written from the restated one moves the cost, by the write that
        //   prices it before and after.
        // - An arc out of the tree goes to a bound, `ArcState::at` of what it carries, and the difference from the
        //   restated flow leaves its head and arrives at its tail through the tree: `route`'s argument keeps every
        //   item, the head holding that much more to send and the tail that much less.
        // - A tree arc holds the restated flow toward the root and takes it as a node's link takes what arrives,
        //   with nothing more arriving: `route`'s step, whose argument keeps every item, a flow outside the width
        //   clamped like any excess past it. Its node is the end the arc joins to its parent (`Basis`, Tree).
        //
        // The nodes pushed: an arc out of the tree is a route, whose argument holds; for a tree arc, a cut of its node
        // is the first step, and the walk goes on from its old parent, as a path of a route does.
        //
        // The flow up the root links. For an arc out of the tree, the route of `amount` raises it by at most that
        // (`route`). For a tree arc, the clamp at its node keeps the rest on the node's root link and hands its parent
        // the rest negated: positive parts of at most `amount` in all, and on from there `route`'s argument.
        let (state, LinkFill { flow, capacity: width }) = (self.rows().state(at), self.links.arc_fill(at));
        let restated = restated.of_flow(flow);
        let restatement = Restatement::of(state, width, restated);
        let hanging = self.hanging_node(at);
        let Ends { tail, head } = self.rows().ends(at);

        let Self { arcs, links, tree, root_cost } = self;
        let mut columns = RouteColumns { arcs, links, root_cost, cost_gained: 0, up_gained: 0, moved };
        columns.links.set_width(at, restated.width());
        let tree = match restatement {
            Restatement::OutOfTree { flow } => {
                let ends = [widen(head), widen(tail)];
                columns.restate_out_of_tree(tree, at, ends, restated.flow(), flow)
            }
            Restatement::InTree => {
                let node = hanging.expect("an arc in the tree should hang one of its ends (`Basis`, Tree)");
                columns.restate_tree_link(tree, node, restated.flow())
            }
        };

        let RouteColumns { arcs, links, cost_gained, up_gained, .. } = columns;
        let basis = Basis { arcs, links, tree, root_cost };
        if let Some(unrouted) = basis.up_past_the_limit(up_gained) {
            panic!(
                "called `Basis::restate` with {slot:?}, which leaves {unrouted} up the root links, past \
                 {NET_SUPPLY_LIMIT}"
            );
        }

        Rerouted { basis, cost_gained, up_gained }
    }

    /// The flow up the root links where a change that gained `up_gained` of it took it past [`NET_SUPPLY_LIMIT`];
    /// `None` where it is within the limit. Where the change gained nothing it is within: the flow up the root links
    /// before was (`Basis`, Flow). Where it gained some it reads the root link of each node that hangs from the root.
    fn up_past_the_limit(&self, up_gained: i128) -> Option<u128> {
        if up_gained <= 0 {
            return None;
        }

        let unrouted = self.flow_up_root_links();
        (unrouted > u128::from(NET_SUPPLY_LIMIT)).then_some(unrouted)
    }

    /// The flow up the root links, in units of flow: what the root links of the nodes that hang from the root carry
    /// where they point up.
    fn flow_up_root_links(&self) -> u128 {
        // A root link out of the tree carries nothing (`Basis`, Flow), and a node that hangs from the root hangs by its
        // own root link (`Basis`, Tree): the tops' root links that point up carry the whole flow up. Each of at most
        // `n` flows is below `2^64` and `n` below `2^32`: the sum fits a `u128`.
        let up = self.tree.tops().filter(|top| top.direction == Direction::Up);

        up.map(|top| u128::from(self.links.fill(top.link).flow)).sum()
    }

    /// The node the arc at `slot` joins to its parent, where the arc is a link of the tree; `None` where it is out of
    /// the tree.
    fn hanging_node(&self, slot: ArcSlot) -> Option<usize> {
        let ends = self.rows().ends(slot);
        let (tail, head) = (widen(ends.tail), widen(ends.head));

        debug_assert!(
            tail < self.tree.node_count() && head < self.tree.node_count(),
            "an arc's ends should be below the tree's hung nodes"
        );
        // SAFETY: both ends of an arc lie below the problem's node count `n` (`Ends`), and the tree has `n` hung nodes
        // (`Basis`, Shape).
        let (tail_hangs, head_hangs) =
            unsafe { (self.tree.hangs_by_unchecked(tail, slot.0), self.tree.hangs_by_unchecked(head, slot.0)) };

        // An arc's link is at its slot. A tree arc joins one of its ends to its parent, the other (`Basis`, Tree).
        match (tail_hangs, head_hangs) {
            (true, _) => Some(tail),
            (false, true) => Some(head),
            (false, false) => None,
        }
    }

    /// The tree of the basis, with its potentials, to be read.
    pub(crate) fn tree(&self) -> &SpanningTree<'r> {
        &self.tree
    }

    /// Ends the run on this basis, optimal or infeasible alike: each node's potential, in units of cost, node `v` at
    /// `v`, is lent for as long as the room is: what a run answers when it is over. The tree's room keeps its tree,
    /// which only a kept room reads again, beside its own marker.
    ///
    /// # Panics
    /// Panics if the tree holds no potential at all, which its `n + 1` potentials rule out (`Basis`, Shape).
    fn keep(self) -> &'r [i64] {
        let potentials = self.tree.keep();
        let (_root, nodes) = potentials.split_last().expect("a tree should hold its root's potential");

        nodes
    }
}

impl<'r> LentBasis<'r, Basis<'r>> {
    /// Brings the arc at `entering` into the basis and answers the flow it sent around the cycle, as
    /// [`Basis::pivot`] does.
    ///
    /// # Panics
    /// As [`Basis::pivot`]: if `entering` is not below the arc count, or if its arc is not priced.
    #[track_caller]
    pub(crate) fn pivot(&mut self, entering: Slot) -> u64 {
        self.basis.pivot(entering)
    }

    /// Makes `cost` the cost of the arc at `slot`, as [`Basis::set_cost`] does, and answers the node whose subtree's
    /// potentials moved.
    ///
    /// # Panics
    /// As [`Basis::set_cost`]: if `slot` is not below the arc count, or `cost` lies beyond the bound of the node
    /// count or asks for a root cost above the basis's.
    #[track_caller]
    pub(crate) fn set_cost(&mut self, slot: Slot, cost: i64) -> Option<usize> {
        self.basis.set_cost(slot, cost)
    }

    /// Makes the basis a basis of the problem in which `from` supplies `amount` more and `to` demands `amount` more,
    /// as [`Basis::route`] says, and answers it with what the cost of its flow and the flow up its root links gained.
    ///
    /// # Panics
    /// As [`Basis::route`]: if `from` or `to` is not below the node count; or if the root links would carry more
    /// toward the root than [`NET_SUPPLY_LIMIT`], or one of them more than `u64::MAX`, which no amount does that adds
    /// up with the flow up the root links before to at most that limit.
    #[track_caller]
    pub(crate) fn route(self, from: usize, to: usize, amount: u64, moved: &mut Vec<usize>) -> Rerouted<Self> {
        let Self { basis, kept } = self;
        let Rerouted { basis, cost_gained, up_gained } = basis.route(from, to, amount, moved);

        Rerouted { basis: Self { basis, kept }, cost_gained, up_gained }
    }

    /// Makes the arc at `slot` an arc of the bounds `restated` gives it, as [`Basis::restate`] says, and answers the
    /// basis with what the cost of its flow and the flow up its root links gained.
    ///
    /// # Panics
    /// As [`Basis::restate`]: if `slot` is not below the arc count; or if the root links would carry more toward the
    /// root than [`NET_SUPPLY_LIMIT`], or one of them more than `u64::MAX`, which no restatement does whose amount
    /// adds up with the flow up the root links before to at most that limit.
    #[track_caller]
    pub(crate) fn restate(self, slot: Slot, restated: Restated, moved: &mut Vec<usize>) -> Rerouted<Self> {
        let Self { basis, kept } = self;
        let Rerouted { basis, cost_gained, up_gained } = basis.restate(slot, restated, moved);

        Rerouted { basis: Self { basis, kept }, cost_gained, up_gained }
    }

    /// Ends the call on this basis, as [`Basis::keep`] does, and puts it back in the kept room it was lent from, whole,
    /// for the next start, reopen or copy; each node's potential is lent for as long as the room is.
    ///
    /// # Panics
    /// As [`Basis::keep`], which the basis's invariant rules out.
    pub(crate) fn keep(self) -> &'r [i64] {
        let Self { basis, kept } = self;
        let potentials = basis.keep();
        *kept = Kept::Basis;

        potentials
    }
}

impl<'r, 'p> LentBasis<'r, LoadedBasis<'r, 'p>> {
    /// Ends the call on this basis, as [`LoadedBasis::keep`] does, and puts it back in the kept room it was made in,
    /// whole, for the next start, reopen or copy; each node's potential is lent for as long as the room is.
    ///
    /// # Panics
    /// As [`Basis::keep`], which the basis's invariant rules out.
    pub(crate) fn keep(self) -> &'r [i64] {
        let Self { basis, kept } = self;
        let potentials = basis.keep();
        *kept = Kept::Basis;

        potentials
    }
}

#[cfg(test)]
impl<B> LentBasis<'_, B> {
    /// The basis alone, the room's marker left out: what a test runs on without a network's keep.
    pub(crate) fn into_basis(self) -> B {
        self.basis
    }
}

impl<B> Deref for LentBasis<'_, B> {
    type Target = B;

    fn deref(&self) -> &B {
        &self.basis
    }
}

impl<'r, 'p> LoadedBasis<'r, 'p> {
    /// The start the basis was made from: the problem and its net supplies.
    pub(crate) fn start(&self) -> Start<'p, 'r> {
        self.start
    }

    /// Brings the arc at `entering` into the basis and answers the flow it sent around the cycle, in units of flow, as
    /// [`Basis::pivot`] does.
    ///
    /// # Panics
    /// Panics as [`Basis::pivot`] does: if `entering` is not below the arc count, or if its arc is not priced.
    #[track_caller]
    pub(crate) fn pivot(&mut self, entering: Slot) -> u64 {
        self.basis.pivot(entering)
    }

    /// Writes each arc's flow, its lower bound included, to `flows`, arc `a` at `a`, and answers the cost of that
    /// flow, as [`arc_cost`](Self::arc_cost) answers it, in the one pass that writes. The basis is unchanged.
    ///
    /// # Panics
    /// Panics if `flows` holds another number of entries than the problem has arcs.
    #[track_caller]
    pub(crate) fn write_flows(&self, flows: &mut [u64]) -> i128 {
        let (length, arc_count) = (flows.len(), self.start.problem().arc_count());
        assert!(
            length == arc_count,
            "called `LoadedBasis::write_flows` with {length} flows, which should be the arc count {arc_count}"
        );

        match self.start.problem().arcs() {
            CheckedArcs::WithoutLowerBounds(arcs) => write_whole(self.whole_flows(arcs), flows),
            CheckedArcs::WithLowerBounds(arcs) => write_whole(self.whole_flows(arcs), flows),
        }
    }

    /// The cost of the arcs' flows, their lower bounds included, in units of cost times units of flow: each arc's cost
    /// times its whole flow, added up, root links not counted. It is exact: each term lies within the arc's cost
    /// magnitude times its capacity, and the problem's check bounds the sum of those by `i128::MAX`
    /// ([`cost_total_with`](crate::ranges::cost_total_with)), so no partial sum passes it. It reads each arc once.
    pub(crate) fn arc_cost(&self) -> i128 {
        match self.start.problem().arcs() {
            CheckedArcs::WithoutLowerBounds(arcs) => cost_of(self.whole_flows(arcs)),
            CheckedArcs::WithLowerBounds(arcs) => cost_of(self.whole_flows(arcs)),
        }
    }

    /// Ends the run on the basis, optimal or infeasible alike, as [`Basis::keep`] does: each node's potential, in units
    /// of cost, node `v` at `v`, is lent for as long as the room is.
    ///
    /// # Panics
    /// Panics if the tree holds no potential at all, which its `n + 1` potentials rule out (`Basis`, Shape).
    pub(crate) fn keep(self) -> &'r [i64] {
        self.basis.keep()
    }

    /// Each arc of `arcs`, the arcs of the loaded problem in ascending order, with its whole flow: its flow above its
    /// lower bound, read at its dealt slot, plus its lower bound.
    fn whole_flows<A: Iterator<Item = CheckedArc>>(&self, arcs: A) -> WholeFlowsIter<'_, A> {
        let slots = DealtSlotsIter::new(self.start.problem().arc_count(), self.start.problem().node_count());

        // The arcs are the loaded problem's, the slots dealt for its counts, and the flows the link column of the basis
        // loaded from it: `WholeFlowsIter`'s invariant.
        WholeFlowsIter { arcs, slots, flows: &*self.basis.links.flows }
    }
}

impl<A: Iterator<Item = CheckedArc>> Iterator for WholeFlowsIter<'_, A> {
    type Item = (CheckedArc, u64);

    fn next(&mut self) -> Option<(CheckedArc, u64)> {
        let (arc, slot) = (self.arcs.next()?, self.slots.next()?);
        let at = usize::from(slot);

        debug_assert!(at < self.flows.len(), "a dealt slot should be below the link count");
        // SAFETY: `DealtSlotsIter` yields only slots below the arc count `m` it was made for (its invariant), the
        // loaded problem's, and the flows are the link column of the basis loaded from it, `m + n` links
        // (`WholeFlowsIter`).
        let flow = unsafe { *self.flows.get_unchecked(at) };

        // A flow is at most the arc's width, so with its lower bound it is at most its capacity, within `u64`.
        Some((arc, flow + arc.lower_bound()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // The arcs and their slots are as many (`WholeFlowsIter`): the slots, which know their count, give it.
        self.slots.size_hint()
    }
}

impl<A: ExactSizeIterator<Item = CheckedArc>> ExactSizeIterator for WholeFlowsIter<'_, A> {}

impl<A: FusedIterator<Item = CheckedArc>> FusedIterator for WholeFlowsIter<'_, A> {}

impl<'r> Deref for LoadedBasis<'r, '_> {
    type Target = Basis<'r>;

    fn deref(&self) -> &Basis<'r> {
        &self.basis
    }
}

impl ArcColumns<'_> {
    /// The number of slots, `m`.
    fn len(&self) -> usize {
        self.states.len()
    }

    /// `slot` as a slot of these columns, where it is below the arc count; `None` past it.
    fn slot(&self, slot: Slot) -> Option<ArcSlot> {
        let at = usize::from(slot);

        (at < self.len()).then_some(ArcSlot(at))
    }

    /// The slot of the arc `link` is, where it is an arc's link, below the arc count; `None` for a root link.
    fn slot_of(&self, link: Link<'_>) -> Option<ArcSlot> {
        let at = usize::from(link);

        (at < self.len()).then_some(ArcSlot(at))
    }

    /// The cost of the arc at `slot`, in units of cost.
    fn cost(&self, slot: ArcSlot) -> i64 {
        debug_assert!(slot.0 < self.costs.len(), "an arc slot should be below the arc count");
        // SAFETY: an `ArcSlot` is below the arc count `m` (`ArcSlot`), the length of this column (`ArcColumns`).
        unsafe { *self.costs.get_unchecked(slot.0) }
    }

    /// Makes `cost` the cost of the arc at `slot`, in units of cost.
    fn set_cost(&mut self, slot: ArcSlot, cost: i64) {
        debug_assert!(slot.0 < self.costs.len(), "an arc slot should be below the arc count");
        // SAFETY: an `ArcSlot` is below the arc count `m` (`ArcSlot`), the length of this column (`ArcColumns`).
        unsafe { *self.costs.get_unchecked_mut(slot.0) = cost };
    }

    /// Makes `state` where the arc at `slot` stands.
    fn set_state(&mut self, slot: ArcSlot, state: ArcState) {
        debug_assert!(slot.0 < self.states.len(), "an arc slot should be below the arc count");
        // SAFETY: an `ArcSlot` is below the arc count `m` (`ArcSlot`), the length of this column (`ArcColumns`).
        unsafe { *self.states.get_unchecked_mut(slot.0) = state };
    }

    /// Moves the entering arc `arc` to its other bound, where a pivot that sent its whole width has put it.
    fn flip(&mut self, arc: &EnteringArc) {
        self.set_state(arc.slot, arc.bound.other().state());
    }

    /// Puts the entering arc `arc` in the tree, and `leaving`, where it is an arc, at the bound its flow in `links` is
    /// at: its lower bound where it carries nothing, its capacity otherwise. A root link keeps no state.
    fn exchange_states(&mut self, arc: &EnteringArc, leaving: Link<'_>, links: &LinkColumns<'_>) {
        self.set_state(arc.slot, ArcState::Unpriced);

        // A link leaves where its room was the step, so the flow sent has put it at one of its bounds; a root link only
        // at zero, since its room the way it points never blocks (`NET_SUPPLY_LIMIT`). A tree arc has width, since only
        // priced arcs enter, so a leaving arc stands at one of its two bounds.
        let LinkFill { flow, capacity } = links.fill(leaving);
        match self.states.get_mut(usize::from(leaving)) {
            Some(state) => *state = ArcState::at(flow, capacity),
            None => debug_assert_eq!(flow, 0, "a leaving root link should carry nothing"),
        }
    }
}

impl LinkColumns<'_> {
    /// What `link` carries and the most it carries.
    fn fill(&self, link: Link<'_>) -> LinkFill {
        let index = usize::from(link);
        debug_assert!(
            index < self.capacities.len() && index < self.flows.len(),
            "a link of the basis's tree should be below its m + n, the length of both link columns"
        );
        // SAFETY: a `Link` is below the link count `m + n` of the tree it was read from (`Link`), the basis's own tree,
        // whose link count is the length of both columns (`LinkColumns`).
        let (capacity, flow) = unsafe { (*self.capacities.get_unchecked(index), *self.flows.get_unchecked(index)) };

        LinkFill { flow, capacity }
    }

    /// What `link` carries, in units of flow, lent to be read and rewritten.
    fn flow_mut(&mut self, link: Link<'_>) -> &mut u64 {
        let index = usize::from(link);
        debug_assert!(index < self.flows.len(), "a link of the basis's tree should be below its m + n");
        // SAFETY: a `Link` is below the link count `m + n` of the tree it was read from (`Link`), the basis's own tree,
        // whose link count is the length of this column (`LinkColumns`).
        unsafe { self.flows.get_unchecked_mut(index) }
    }

    /// The width of the arc at `slot`, in units of flow.
    fn width(&self, slot: ArcSlot) -> u64 {
        debug_assert!(slot.0 < self.capacities.len(), "an arc slot should be below the link count");
        // SAFETY: an `ArcSlot` is below the arc count `m` (`ArcSlot`), which is at most `m + n`, the length of this
        // column (`LinkColumns`).
        unsafe { *self.capacities.get_unchecked(slot.0) }
    }

    /// What the arc at `slot` carries above its lower bound and its width, read together.
    fn arc_fill(&self, slot: ArcSlot) -> LinkFill {
        debug_assert!(
            slot.0 < self.capacities.len() && slot.0 < self.flows.len(),
            "an arc slot should be below the link count"
        );
        // SAFETY: an `ArcSlot` is below the arc count `m` (`ArcSlot`), which is at most `m + n`, the length of both
        // columns (`LinkColumns`).
        let (capacity, flow) = unsafe { (*self.capacities.get_unchecked(slot.0), *self.flows.get_unchecked(slot.0)) };

        LinkFill { flow, capacity }
    }

    /// Makes `width` the width of the arc at `slot`, in units of flow.
    fn set_width(&mut self, slot: ArcSlot, width: u64) {
        debug_assert!(slot.0 < self.capacities.len(), "an arc slot should be below the link count");
        // SAFETY: an `ArcSlot` is below the arc count `m` (`ArcSlot`), which is at most `m + n`, the length of this
        // column (`LinkColumns`).
        unsafe { *self.capacities.get_unchecked_mut(slot.0) = width };
    }

    /// Makes `flow` what the arc at `slot` carries.
    fn set_arc_flow(&mut self, slot: ArcSlot, flow: u64) {
        debug_assert!(slot.0 < self.flows.len(), "an arc slot should be below the link count");
        // SAFETY: an `ArcSlot` is below the arc count `m` (`ArcSlot`), which is at most `m + n`, the length of this
        // column (`LinkColumns`).
        unsafe { *self.flows.get_unchecked_mut(slot.0) = flow };
    }

    /// The ratio test, made on the walk to the apex: the most flow the cycle of `walk` carries, starting from `width`,
    /// the entering arc's own room, and the least room of its links, with the side whose kept node's link leaves.
    /// Among links of equal least room the last met from the apex in the cycle's direction leaves, the arc counted
    /// where it lies between the two sides; none leaves where no link's room is below `width`, or equal to it on the
    /// second side. Each side keeps the node of its own candidate; the walk is finished.
    fn least_room(&self, walk: &mut CycleWalk<'_>, width: u64) -> RatioTest {
        // The strongly feasible leaving rule. Orient the cycle as flow is sent: from the first end along the entering
        // arc to the second end, up the second side to the apex, and down the first side back to the first end. Of
        // the links of least room, the last met from the apex in that direction leaves: so on the part of the cycle
        // from the leaving link on to the apex, every link either carried the cycle's flow toward the new root or
        // gained room from the step, and on the part before it every link had more room than the least and keeps
        // some; off the cycle nothing changed. Every tree link of the new tree can then carry more toward the root.
        //
        // Met from the apex in that direction, the links come in the order: the first side from the apex down to the
        // first end, the entering arc, the second side from the second end up to the apex. On the first side the
        // cycle's flow runs down the tree, on the second up it. The same comparisons serve both bounds, because the
        // bound decides only which end is first: from the lower bound the arc's flow rises, which sends flow along it
        // from its tail; from the capacity it falls, which sends flow along it from its head.
        //
        // The walk yields each side's nodes in the order of its path from its end up, whichever way the two sides
        // interleave, so each side's choice is made on its own: the first side is walked against the order met from
        // the apex, so of its links of equal room the first walked is the last met, and a link replaces the one kept
        // only where its room is strictly less, starting from the arc's width, which is met after it; the second side
        // is walked in that order, so the last walked is the last met, and a link replaces the one kept where its room
        // is less or equal. Between the two, every second-side link is met after every first-side link and after the
        // arc: the second side's least leaves where it is at most the first side's least, the width included; else
        // the first side's leaves where its least is below the width; else none.
        //
        // The second side's least starts at `u64::MAX`, at or above every room, so its first step is always kept: it
        // kept a node exactly where it has a step. A second side of no step answers `u64::MAX`, which is at most the
        // first side's least only where both stand at the width `u64::MAX`: there no link leaves and the step is the
        // width, so the second side is named only where its walk kept a node. The first side keeps a node exactly
        // where its least fell below the width.
        let mut first_least = width;
        let mut second_least = u64::MAX;

        while let Some(at) = walk.step() {
            match at.side() {
                Side::First => {
                    let room = self.room_of(at.link(), at.direction(), Direction::Down);
                    if room < first_least {
                        first_least = room;
                        at.keep();
                    }
                }
                Side::Second => {
                    let room = self.room_of(at.link(), at.direction(), Direction::Up);
                    if room <= second_least {
                        second_least = room;
                        at.keep();
                    }
                }
            }
        }

        if second_least <= first_least && walk.kept(Side::Second) {
            RatioTest { step: second_least, leaving: Some(Side::Second) }
        } else if first_least < width {
            RatioTest { step: first_least, leaving: Some(Side::First) }
        } else {
            RatioTest { step: width, leaving: None }
        }
    }

    /// The flow update: sends `step` around `cycle`, along the entering arc `arc` away from its bound, down the tree
    /// from the apex to the first end and up from the second end to the apex. A step of zero changes nothing.
    ///
    /// Afterwards every node still conserves its net supply, and every flow lies within its bounds, since `step` is at
    /// most the room of every link on the cycle and the arc's width.
    fn send_around(&mut self, cycle: &Cycle<'_>, arc: &EnteringArc, step: u64) {
        if step == 0 {
            return;
        }

        self.set_arc_flow(arc.slot, arc.flow_after(step));
        for hanging in cycle.first_side() {
            self.send(hanging.link, hanging.direction, Direction::Down, step);
        }
        for hanging in cycle.second_side() {
            self.send(hanging.link, hanging.direction, Direction::Up, step);
        }
    }

    /// The room of `link`, which runs `direction` seen from the node below it, where the cycle's flow runs `flow_runs`
    /// along the tree there.
    fn room_of(&self, link: Link<'_>, direction: Direction, flow_runs: Direction) -> u64 {
        let LinkFill { flow, capacity } = self.fill(link);

        room(capacity, flow, direction, flow_runs)
    }

    /// Sends `step` along `link`, which runs `direction` seen from the node below it, where the cycle's flow runs
    /// `flow_runs` along the tree there: one read and one write of its flow.
    fn send(&mut self, link: Link<'_>, direction: Direction, flow_runs: Direction, step: u64) {
        let flow = self.flow_mut(link);
        *flow = sent(*flow, step, direction, flow_runs);
    }
}

impl KeptShape<'_, '_> {
    /// Whether every arc of the problem has, at its dealt slot, the tail and the head the kept arc there had.
    fn are_of_problem(&self) -> bool {
        match self.problem.arcs() {
            CheckedArcs::WithoutLowerBounds(arcs) => self.have_ends_of(arcs),
            CheckedArcs::WithLowerBounds(arcs) => self.have_ends_of(arcs),
        }
    }

    /// Whether each arc of `arcs`, in ascending order, has the ends the kept arc at its dealt slot had; the arcs past
    /// the problem's arc count are not compared.
    fn have_ends_of(&self, arcs: impl Iterator<Item = CheckedArc>) -> bool {
        let slots = DealtSlotsIter::new(self.problem.arc_count(), self.problem.node_count());

        arcs.zip(slots).all(|(arc, slot)| {
            let at = usize::from(slot);
            debug_assert!(at < self.ends.len(), "a dealt slot should be below the arc count, the length of the ends");
            // SAFETY: `DealtSlotsIter` yields only slots below the arc count `m` it was made for (its invariant), the
            // problem's, and the ends hold `m` entries (`KeptShape`).
            let kept = unsafe { *self.ends.get_unchecked(at) };

            kept == Ends::of(arc)
        })
    }

    /// What each arc of `arcs`, in ascending order, carries above its lower bound out of the kept tree, arc `a`'s at
    /// `a`: its width, as `arcs` gives it, where its slot's kept state is `AtUpper`, and nothing otherwise, a tree arc
    /// included. As many flows as both `arcs` and the problem have arcs.
    fn carried(&self, arcs: impl Iterator<Item = CheckedArc>) -> impl Iterator<Item = u64> {
        let slots = DealtSlotsIter::new(self.problem.arc_count(), self.problem.node_count());
        let states = self.states;

        arcs.zip(slots).map(move |(arc, slot)| {
            let at = usize::from(slot);
            debug_assert!(at < states.len(), "a dealt slot should be below the arc count, the length of the states");
            // SAFETY: `DealtSlotsIter` yields only slots below the arc count `m` it was made for (its invariant), the
            // problem's, and the states hold `m` entries (`KeptShape`).
            let state = unsafe { *states.get_unchecked(at) };

            state.carried(arc.width())
        })
    }
}

impl Reflow<'_, '_> {
    /// Sends each node's excess toward the root along its link, from the leaves up, each node after its whole subtree:
    /// afterwards every node conserves its net supply, root links counted; every flow lies within its bounds; every
    /// arc out of the tree stands at a bound, and every tree arc is unpriced; every tree link can carry more toward the
    /// root; and every root link carries at most the positive total of the excesses the re-flow began with.
    fn flow_up(self) -> Self {
        // Why the result is a basis of the new problem, item by item of `Basis`'s invariant, but for *Duality*, which
        // `tighten` makes:
        //
        // - *Shape* holds: the counts are the kept ones, which are the problem's, and the arcs' pass wrote every arc.
        // - *Tree*: the prune keeps the tree's invariant (`PruneWalk`); an arc that stays is made unpriced, one that
        //   leaves is put at its bound, and every arc out of the tree stood out of it before.
        // - *Flow*: node `v` holds its excess `E(v)` over the arcs out of the tree, plus what its children sent it.
        //   It sends `g` along its link, or `−g` where the link runs down to it, and keeps the remainder `r` on its
        //   root link where it hangs from the root: so it conserves. A tree arc carries `g` within `0..=w`, and one
        //   that leaves stands at a bound, since a remainder other than zero means the clamp met a bound, and an arc
        //   that leaves with none met the bound beyond which it has no room toward the root. A root link out of the
        //   tree belongs to a node that stays on its arc, whose root link carried nothing in the kept basis and is not
        //   written.
        // - *Strong feasibility*: an arc stays only with room toward the root; a root link that points down carries
        //   `|r| > 0`; one that points up carries at most the positive total `P` below, below `u64::MAX`.
        //
        // The root links carry at most `P`, the positive total of the excesses, at most `NET_SUPPLY_LIMIT`. Let `Φ`
        // be the sum of the positive parts of the excesses not yet sent and of the remainders placed. A node with
        // `e ≥ 0` splits `e` into `r ≥ 0` and a transfer `g ≥ 0` that raises its parent's positive part by at most
        // `g`; one with `e < 0` keeps `r ≤ 0` and lowers its parent's excess. So `Φ` never rises, and the flow up the
        // root links is at most `P`. Its mirror, the sum `Ψ` of the negative parts, never rises either, by the same
        // argument with the signs turned, and starts at `P` too, since the excesses add up to zero. So every excess,
        // transfer and remainder of the pass lies within `±P`: each sum is exact in an `i128`, and each remainder's
        // magnitude fits a root link's flow. With the root links' totals at most `P` the two root-link arguments of
        // `NET_SUPPLY_LIMIT` hold for the run that follows as they do for the star's.
        //
        // The star is the case of a tree whose every node hangs from the root, every arc at its lower bound: every
        // excess is then a net supply, and each node takes the root link the star gives it.
        let Self { tree, mut columns } = self;
        let mut walk = PruneWalk::new(tree);

        while let Some(step) = walk.step() {
            columns.send(step);
        }

        Self { tree: walk.finish(), columns }
    }

    /// Makes every tree link tight under the problem's costs, from the root down, the root at zero: an arc costs its
    /// cost, a root link zero where it points up and `root_cost` where it points down.
    fn tighten(self, root_cost: i64) -> Self {
        // Every potential is rewritten, so the potentials are the ones the tree implies under the new costs and the
        // new root cost, within the bounds `cost_bound` argues for any tree.
        let Self { tree, columns } = self;
        let mut walk = TightenWalk::new(tree);

        while let Some(step) = walk.step() {
            let hanging = step.hanging();
            let cost = match columns.arcs.slot_of(hanging.link) {
                Some(slot) => columns.arcs.cost(slot),
                None => root_link_cost(hanging.direction, root_cost),
            };

            step.tighten(cost);
        }

        Self { tree: walk.finish(), columns }
    }
}

impl ReflowColumns<'_, '_> {
    /// Decides the node of `step`, whose children have sent it their excesses: where it hangs by an arc, sends its
    /// excess along the arc as far as the arc carries it, gives the parent what the arc carries, and keeps the arc in
    /// the tree, unpriced, or puts it out at its bound and hangs the node from the root by its root link, which takes
    /// the rest; where it hangs from the root, puts its excess on its root link.
    fn send(&mut self, step: PruneStep<'_>) {
        let node = NodeId::from(step.node());
        debug_assert!(usize::from(node) < self.excesses.len(), "a hung node should be below the excesses' n");
        // SAFETY: a step's node is below its tree's `n` hung nodes (`PruneStep`), the problem's (`ServingStart`,
        // `Reflow`), and the excesses hold one entry per node (`ReflowColumns`).
        let excess = unsafe { *self.excesses.get_unchecked(node) };

        let hanging = step.hanging();
        let direction = hanging.direction;
        let Some(slot) = self.arcs.slot_of(hanging.link) else {
            self.hang_from_root(step, root_link(excess));
            return;
        };

        let TreeArcFlow { flow, to_parent, standing } = resend(excess, self.links.width(slot), direction);
        self.links.set_arc_flow(slot, flow);
        let parent = NodeId::from(step.parent());
        debug_assert!(usize::from(parent) < self.excesses.len(), "an arc's other end should be below the excesses' n");
        // SAFETY: a node that hangs by an arc hangs from a node below the problem's `n`, never from the root (`Basis`,
        // Tree, which the re-flow's tree keeps: `Reflow`); the excesses hold one entry per node (`ReflowColumns`).
        let parent_excess = unsafe { self.excesses.get_unchecked_mut(parent) };
        // Every excess and transfer of the pass lies within `±P` (`Reflow::flow_up`): the sum is exact.
        *parent_excess += to_parent;

        match standing {
            Standing::InTree => {
                self.arcs.set_state(slot, ArcState::Unpriced);
                step.stay();
            }
            Standing::Leaves { state, root_link } => {
                self.arcs.set_state(slot, state);
                self.hang_from_root(step, root_link);
            }
        }
    }

    /// Hangs the node of `step` from the root by its root link, carrying and pointing as `root_link` says.
    fn hang_from_root(&mut self, step: PruneStep<'_>, root_link: RootLink) {
        *self.links.flow_mut(step.root_link()) = root_link.flow;
        step.hang_from_root(root_link.direction);
    }
}

impl RouteColumns<'_, '_> {
    /// Sends `amount` more from the node `from` to the node `to` through `tree`, its sign the way it goes: from each
    /// node along its path up to their apex, `amount` from `from` and its negation from `to`, then what the two paths
    /// hand the apex on toward the root, each node taking what arrives as [`take`](Self::take) says. Answers the tree.
    ///
    /// # Panics
    /// Panics if `from` or `to` is not below the tree's node count.
    #[track_caller]
    fn route<'r>(&mut self, mut tree: LentTree<'r>, from: usize, to: usize, amount: i128) -> LentTree<'r> {
        let node_count = tree.node_count();
        let apex = tree.apex(from, to).expect("both nodes should be below the node count");

        let (tree, from_path) = self.walk(tree, from, apex, amount);
        let (tree, to_path) = self.walk(tree, to, apex, -amount);
        // A step changes nothing above its node, so each walk meets the apex, which stays an ancestor of both nodes: a
        // cut on one path moves only nodes strictly below the apex, none of them on the other path. The walk from the
        // apex ends at the root, where the last node, under the root, took what arrived: nothing is left.
        let (tree, _) = self.walk(tree, apex, node_count, from_path + to_path);

        tree
    }

    /// Restates the arc at `slot`, out of the tree, to carry `flow`, nothing or its width, where it held `held`
    /// restated for its new bounds, and stands it where [`ArcState::at`] puts that; what it carries more than `held`
    /// leaves its head and arrives at its tail through `tree`, its ends `[head, tail]`. Answers the tree.
    ///
    /// # Panics
    /// Panics if an end is not below the tree's node count.
    #[track_caller]
    fn restate_out_of_tree<'r>(
        &mut self,
        tree: LentTree<'r>,
        slot: ArcSlot,
        [head, tail]: [usize; 2],
        held: i128,
        flow: u64,
    ) -> LentTree<'r> {
        self.arcs.set_state(slot, ArcState::at(flow, self.links.width(slot)));
        self.write_arc(slot, held, flow);

        // Below `2^64` less a value within `(−2^64, 2^65)`: exact.
        match i128::from(flow) - held {
            0 => tree,
            moved => self.route(tree, head, tail, moved),
        }
    }

    /// Restates the link of `node`, an arc of the tree, to hold `held` toward the root, restated for its new bounds:
    /// the node's step takes it as what its link held, with nothing more arriving, and what the node hands its parent
    /// is sent on toward the root. Answers the tree.
    ///
    /// # Panics
    /// Panics if `node` is not below the tree's node count, or if it hangs from the root.
    #[track_caller]
    fn restate_tree_link<'r>(&mut self, tree: LentTree<'r>, node: usize, held: i128) -> LentTree<'r> {
        let root = tree.node_count();
        let mut walk = RootwardWalk::new(tree, node, root);
        let step = walk.step().expect("a node below the node count should be a hung node");
        let slot = self.arcs.slot_of(step.hanging().link).expect("the node should hang by an arc");

        let handed = self.carry(step, slot, held, 0);
        let _ = self.carry_on(&mut walk, handed);

        walk.finish()
    }

    /// Walks `tree` from `from` toward the root, ending before the tree node `stop`, or at the root, `arrived` arriving
    /// at `from` to be sent toward the root, each node taking what arrives as [`take`](Self::take) says, until a node
    /// hands on nothing; answers the tree and what the walk hands `stop`, nothing where it ended at the root or ran
    /// dry.
    ///
    /// # Panics
    /// Panics if `from` is not below the tree's `n + 1` tree nodes.
    #[track_caller]
    fn walk<'r>(&mut self, tree: LentTree<'r>, from: usize, stop: usize, arrived: i128) -> (LentTree<'r>, i128) {
        let mut walk = RootwardWalk::new(tree, from, stop);
        let handed = self.carry_on(&mut walk, arrived);

        (walk.finish(), handed)
    }

    /// Takes the steps of `walk`, `arrived` arriving at its next node to be sent toward the root, each node taking what
    /// arrives as [`take`](Self::take) says, until a node hands on nothing or the walk ends; answers what it hands the
    /// node the walk ends before, nothing where it ended at the root or ran dry.
    fn carry_on(&mut self, walk: &mut RootwardWalk<'_>, arrived: i128) -> i128 {
        // A node nothing arrived at keeps its link, carrying what it carried, with the room it had: a walk may end
        // where what it hands on is nothing. Each step decides a node of the path from the walk's start up, once: at
        // most the tree's `n` hung nodes.
        let mut arrived = arrived;
        while arrived != 0 {
            let Some(step) = walk.step() else { break };
            arrived = self.take(step, arrived);
        }

        arrived
    }

    /// Decides the node of `step`, at which `arrived` more arrives to be sent toward the root: where it hangs by an
    /// arc, [`carry`](Self::carry) decides it; where it hangs from the root, its root link takes it. Answers what the
    /// node's parent gains to send toward the root: nothing where the node hangs from the root.
    fn take(&mut self, step: RootwardStep<'_, '_>, arrived: i128) -> i128 {
        let hanging = step.hanging();
        let (slot, direction) = (self.arcs.slot_of(hanging.link), hanging.direction);
        let held = i128::from(self.links.fill(hanging.link).flow);

        match slot {
            Some(slot) => self.carry(step, slot, held, arrived),
            None => {
                // A node that hangs from the root does so by its own root link (`Basis`, Tree).
                let root_link = root_link(toward_root(held, direction) + arrived);
                self.hang_from_root(step, LinkPrice::RootLink(direction), root_link);

                0
            }
        }
    }

    /// Decides the node of `step`, which hangs by the arc at `slot` holding `held`, at which `arrived` more arrives to
    /// be sent toward the root: the arc carries what it can of what it held and what arrived, and stays, or leaves at
    /// its bound and the node hangs from the root by its root link, which takes the rest. Answers what the node's
    /// parent gains to send toward the root.
    fn carry(&mut self, step: RootwardStep<'_, '_>, slot: ArcSlot, held: i128, arrived: i128) -> i128 {
        let direction = step.hanging().direction;
        // The flow the link sends toward the root, signed, before the step.
        let before = toward_root(held, direction);

        let TreeArcFlow { flow, to_parent, standing } = resend(before + arrived, self.links.width(slot), direction);
        self.write_arc(slot, held, flow);
        match standing {
            Standing::InTree => step.stay(),
            Standing::Leaves { state, root_link } => {
                self.arcs.set_state(slot, state);
                // A root link out of the tree carries nothing (`Basis`, Flow), which costs and counts nothing whichever
                // way it points.
                self.hang_from_root(step, LinkPrice::RootLink(root_link.direction), root_link);
            }
        }

        to_parent - before
    }

    /// Hangs the node of `step` from the root by its root link, priced `before` where it held it before, which carries
    /// and points as `root_link` says; the potentials of its subtree move to make that link tight, and where they moved
    /// the node is pushed onto the nodes moved.
    fn hang_from_root(&mut self, step: RootwardStep<'_, '_>, before: LinkPrice, root_link: RootLink) {
        let link = step.root_link();
        let held = i128::from(self.links.fill(link).flow);
        *self.links.flow_mut(link) = root_link.flow;
        self.gain(before, held, LinkPrice::RootLink(root_link.direction), root_link.flow);

        if let Some(top) = step.hang_from_root(root_link.direction, self.root_cost) {
            self.moved.push(top);
        }
    }

    /// Makes `flow` what the arc at `slot` carries, where it held `held`, and moves what the route gained by the change
    /// of its flow's cost.
    fn write_arc(&mut self, slot: ArcSlot, held: i128, flow: u64) {
        let price = LinkPrice::Arc(self.arcs.cost(slot));
        self.links.set_arc_flow(slot, flow);

        self.gain(price, held, price, flow);
    }

    /// The one rule by which a flow written moves what the route gained: a link that held `held` priced `before`, and
    /// carries `flow` priced `after`, moves the cost of the flow by its cost after less its cost before, and the flow
    /// up the root links by what it sends up into the root after less before.
    fn gain(&mut self, before: LinkPrice, held: i128, after: LinkPrice, flow: u64) {
        let flow = i128::from(flow);

        // The gain is kept modulo `2^128`, as the kept cost it is added to. Each change of the flow up is within
        // `±2^65`, and a route writes at most two links a node of the tree: the sum is exact.
        let gained = after.cost(flow, self.root_cost).wrapping_sub(before.cost(held, self.root_cost));
        self.cost_gained = self.cost_gained.wrapping_add(gained);
        self.up_gained += after.up(flow) - before.up(held);
    }
}

impl LinkPrice {
    /// The cost of `flow` units on such a link, in units of cost times units of flow, modulo `2^128`: a root link that
    /// points down at `root_cost` a unit.
    fn cost(self, flow: i128, root_cost: i64) -> i128 {
        let unit = match self {
            Self::Arc(cost) => cost,
            Self::RootLink(direction) => root_link_cost(direction, root_cost),
        };

        // Exact for a flow below `2^64` in magnitude, a unit's cost being at most `2^63`; modulo `2^128` past it, as
        // every cost gained is kept.
        i128::from(unit).wrapping_mul(flow)
    }

    /// What `flow` units on such a link send up into the root, in units of flow.
    fn up(self, flow: i128) -> i128 {
        match self {
            Self::RootLink(Direction::Up) => flow,
            Self::RootLink(Direction::Down) | Self::Arc(_) => 0,
        }
    }
}

impl<'b> PricedRows<'b> {
    /// The number of rows, one per slot, `m`.
    pub(crate) fn len(&self) -> usize {
        self.states.len()
    }

    /// The ends of the arc at `slot`.
    fn ends(&self, slot: ArcSlot) -> Ends {
        debug_assert!(slot.0 < self.ends.len(), "an arc slot should be below the arc count");
        // SAFETY: an `ArcSlot` is below the arc count `m` (`ArcSlot`), the length of this column (`PricedRows`).
        unsafe { *self.ends.get_unchecked(slot.0) }
    }

    /// The cost of the arc at `slot`, in units of cost.
    fn cost(&self, slot: ArcSlot) -> i64 {
        debug_assert!(slot.0 < self.costs.len(), "an arc slot should be below the arc count");
        // SAFETY: an `ArcSlot` is below the arc count `m` (`ArcSlot`), the length of this column (`PricedRows`).
        unsafe { *self.costs.get_unchecked(slot.0) }
    }

    /// Where the arc at `slot` stands.
    fn state(&self, slot: ArcSlot) -> ArcState {
        debug_assert!(slot.0 < self.states.len(), "an arc slot should be below the arc count");
        // SAFETY: an `ArcSlot` is below the arc count `m` (`ArcSlot`), the length of this column (`PricedRows`).
        unsafe { *self.states.get_unchecked(slot.0) }
    }

    /// The reduced cost of an arc of these rows whose ends are `ends`, one of these rows' ends, and whose cost is
    /// `cost`, in units of cost.
    fn reduced_cost(&self, ends: Ends, cost: i64) -> i64 {
        let (tail, head) = (widen(ends.tail), widen(ends.head));

        debug_assert!(
            tail < self.potentials.len() && head < self.potentials.len(),
            "an arc's tail and head should be nodes of the tree beside it"
        );
        // SAFETY: an end of these rows is below `n` (`Ends`), and these rows hold the `n + 1` potentials of their
        // basis's tree (`PricedRows`).
        let (tail_potential, head_potential) =
            unsafe { (*self.potentials.get_unchecked(tail), *self.potentials.get_unchecked(head)) };

        reduced_cost(cost, tail_potential, head_potential)
    }

    /// The violation of the arc at `slot`: its pricing sign times its reduced cost.
    fn violation(&self, slot: ArcSlot) -> i64 {
        self.state(slot).violation(self.reduced_cost(self.ends(slot), self.cost(slot)))
    }

    /// The arc at `slot` read for a pivot: the bound it stands at, its ends in the order flow is sent along it, its
    /// width from `links`, the link columns of the same basis, and its reduced cost; `None` where it is not priced.
    fn entering(&self, slot: ArcSlot, links: &LinkColumns<'_>) -> Option<EnteringArc> {
        let bound = self.state(slot).bound()?;
        let ends = self.ends(slot);
        let (tail, head) = (widen(ends.tail), widen(ends.head));
        let (first, second) = match bound {
            Bound::Lower => (tail, head),
            Bound::Upper => (head, tail),
        };

        Some(EnteringArc {
            slot,
            bound,
            first,
            second,
            width: links.width(slot),
            reduced_cost: self.reduced_cost(ends, self.cost(slot)),
        })
    }

    /// Each slot of `slots` in ascending order with its arc's violation: its pricing sign times its reduced cost, in
    /// units of cost, negative exactly where the arc violates its bound. A row costs three reads in sequence and two
    /// potentials.
    ///
    /// # Panics
    /// Panics if `slots` does not lie within the rows: its start is past its end, or its end past the row count.
    #[track_caller]
    pub(crate) fn violations(&self, slots: Range<usize>) -> ViolationsIter<'b> {
        let (start, end) = (slots.start, slots.end);
        let (Some(ends), Some(costs), Some(states)) =
            (self.ends.get(start..end), self.costs.get(start..end), self.states.get(start..end))
        else {
            violations_outside_rows(start, end, self.len())
        };

        // Both lie within the rows, so at most the arc count, which a checked problem keeps within `u32::MAX`: the
        // narrowing is exact. It is a cast, not `narrow`, since every search prices its blocks through here.
        #[expect(clippy::cast_possible_truncation, reason = "both lie within the rows, at most u32::MAX of them")]
        let numbers = (start as u32)..(end as u32);

        ViolationsIter { columns: numbers.zip(ends).zip(costs).zip(states), rows: *self }
    }
}

impl Iterator for ViolationsIter<'_> {
    type Item = (Slot, i64);

    fn next(&mut self) -> Option<(Slot, i64)> {
        let (((number, &ends), &cost), &state) = self.columns.next()?;

        Some((Slot::from(number), state.violation(self.rows.reduced_cost(ends, cost))))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.columns.size_hint()
    }
}

impl DoubleEndedIterator for ViolationsIter<'_> {
    fn next_back(&mut self) -> Option<(Slot, i64)> {
        let (((number, &ends), &cost), &state) = self.columns.next_back()?;

        Some((Slot::from(number), state.violation(self.rows.reduced_cost(ends, cost))))
    }
}

impl ExactSizeIterator for ViolationsIter<'_> {}

impl FusedIterator for ViolationsIter<'_> {}

impl Ends {
    /// The tail and the head of `arc`.
    fn of(arc: CheckedArc) -> Self {
        // A checked arc's ends are below `n`, and `n + 1` is at most `u32::MAX`: neither narrowing can fail.
        Self { tail: narrow(usize::from(arc.tail())), head: narrow(usize::from(arc.head())) }
    }

    /// The node the arc leaves.
    pub(crate) fn tail(self) -> usize {
        widen(self.tail)
    }

    /// The node the arc enters.
    pub(crate) fn head(self) -> usize {
        widen(self.head)
    }
}

impl EnteringArc {
    /// What the arc carries once `step`, at most its width, is sent along the cycle: `step` where it left its lower
    /// bound, its width less `step` where it left its capacity.
    fn flow_after(&self, step: u64) -> u64 {
        match self.bound {
            Bound::Lower => step,
            Bound::Upper => self.width - step,
        }
    }
}

/// Writes each whole flow `whole` yields to `flows`, in the order yielded, until either runs out, and answers their
/// cost as [`cost_of`] sums it: one pass that writes and sums.
fn write_whole(whole: impl Iterator<Item = (CheckedArc, u64)>, flows: &mut [u64]) -> i128 {
    cost_of(whole.zip(flows).map(|((arc, flow), written)| {
        *written = flow;
        (arc, flow)
    }))
}

/// The cost of the whole flows `whole` yields: each arc's cost times its flow, added up, in units of cost times units
/// of flow. Exact where the arcs are a checked problem's and the flows within their capacities
/// ([`cost_total_with`](crate::ranges::cost_total_with)).
fn cost_of(whole: impl Iterator<Item = (CheckedArc, u64)>) -> i128 {
    whole.fold(0, |total, (arc, flow)| total + i128::from(arc.cost()) * i128::from(flow))
}

/// The root costs a basis of `problem` may be made with: from the problem's own, `(n − 1)·C + 1` for its `n` nodes and
/// largest cost magnitude `C`, to the widest of `n` nodes ([`widest_root_cost`]). The one rule both starts check a root
/// cost by.
fn root_costs(problem: CheckedProblem<'_>) -> RangeInclusive<i64> {
    // At the problem's own root cost or above, `M − (n − 1)·C ≥ 1` holds, which is all a start, `NET_SUPPLY_LIMIT` and
    // the decision whether every supply is routed ask of it. At the widest root cost or below, every potential, reduced
    // cost and partial sum stays below `2^63` in magnitude, for any tree (`widest_root_cost`).
    problem.root_cost()..=widest_root_cost(problem.node_count())
}

/// What a link that carries `flow` and runs `direction` seen from its node sends from that node toward the root, in
/// units of flow: the flow where it runs up, its negation where it runs down.
///
/// # Panics
/// Panics if `flow` is `i128::MIN` and the link runs down: a flow no basis holds, nor restates.
#[track_caller]
fn toward_root(flow: i128, direction: Direction) -> i128 {
    match direction {
        Direction::Up => flow,
        Direction::Down => flow.strict_neg(),
    }
}

/// The reduced cost of an arc of cost `cost` whose tail has the potential `tail_potential` and whose head
/// `head_potential`: `cost + tail_potential − head_potential`, in units of cost. The one place it is written.
// Always inlined: it is formed once for every row the pricing reads, in the loop a solve spends most of its time in,
// and it is two operations, so a call that was not inlined would cost more than the work it holds.
#[inline(always)]
fn reduced_cost(cost: i64, tail_potential: i64, head_potential: i64) -> i64 {
    // The partial sum and the result are below `2^63` in magnitude for the costs and potentials of a basis
    // (`cost_bound`).
    cost + tail_potential - head_potential
}

/// Panics for a `slot` not below `arc_count`, as [`Basis::violates`] documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: the slots a caller asks about are its own, so the check keeps only the comparison. Its
// caller tracks its own caller too, so the panic names the line the slot came from.
#[cold]
#[inline(never)]
#[track_caller]
fn violates_outside_slots(slot: Slot, arc_count: usize) -> ! {
    panic!("called `Basis::violates` with {slot:?}, which should be below the arc count {arc_count}")
}

/// Panics for a `slot` not below `arc_count`, as [`Basis::violation`] documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: the slots a caller asks about are its own, so the check keeps only the comparison. Its
// caller tracks its own caller too, so the panic names the line the slot came from.
#[cold]
#[inline(never)]
#[track_caller]
fn violation_outside_slots(slot: Slot, arc_count: usize) -> ! {
    panic!("called `Basis::violation` with {slot:?}, which should be below the arc count {arc_count}")
}

/// Panics for a `slot` not below `arc_count`, as [`Basis::pivot`] documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: a pivot of a slot of its basis keeps only the comparison. Its caller tracks its own caller
// too, so the panic names the line the slot came from.
#[cold]
#[inline(never)]
#[track_caller]
fn pivot_outside_slots(slot: Slot, arc_count: usize) -> ! {
    panic!("called `Basis::pivot` with {slot:?}, which should be below the arc count {arc_count}")
}

/// Panics for a `slot` whose arc is not priced, as [`Basis::pivot`] documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: a pivot of a priced arc keeps only the test of its state. Its caller tracks its own caller
// too, so the panic names the line the slot came from.
#[cold]
#[inline(never)]
#[track_caller]
fn pivot_unpriced(slot: Slot) -> ! {
    panic!("called `Basis::pivot` with {slot:?}, whose arc should be priced: out of the tree and of positive width")
}

/// Panics for a `slot` not below `arc_count`, as [`Basis::set_cost`] documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: a change of an arc of the basis keeps only the comparison. Its caller tracks its own caller
// too, so the panic names the line the slot came from.
#[cold]
#[inline(never)]
#[track_caller]
fn set_cost_outside_slots(slot: Slot, arc_count: usize) -> ! {
    panic!("called `Basis::set_cost` with {slot:?}, which should be below the arc count {arc_count}")
}

/// Panics for a `slot` not below `arc_count`, as [`Basis::restate`] documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: a restatement of an arc of the basis keeps only the comparison. Its caller tracks its own
// caller too, so the panic names the line the slot came from.
#[cold]
#[inline(never)]
#[track_caller]
fn restate_outside_slots(slot: Slot, arc_count: usize) -> ! {
    panic!("called `Basis::restate` with {slot:?}, which should be below the arc count {arc_count}")
}

/// Panics for the slots `start..end` of rows of which there are `row_count`, as [`PricedRows::violations`] documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: a stretch within the rows keeps only the comparisons. Its caller tracks its own caller too,
// so the panic names the line the stretch came from.
#[cold]
#[inline(never)]
#[track_caller]
fn violations_outside_rows(start: usize, end: usize, row_count: usize) -> ! {
    panic!(
        "called `PricedRows::violations` with the slots {start}..{end}, which should lie within the {row_count} rows"
    )
}

#[cfg(test)]
pub(super) mod tests {
    use std::collections::BTreeSet;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use kahlnws_testing::{
        BoundDraw, CostDraw, MincostNetwork, OptimalFlow, TinyBounds, TinyCosts, TinyMincostNetworks, TinySupplies,
        bound_draws, brute_force_optimum, check_against_brute_force, check_certificate, cost_draws, tiny_networks,
    };
    use num_bigint::BigInt;
    use proptest::prelude::*;
    use proptest::sample::Index;
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;
    use rand::{RngExt, SeedableRng};
    use rand_xoshiro::Xoshiro256PlusPlus;

    use super::*;
    use crate::basis::arc_state::tests::{IN_TREE, LOWER, UPPER};
    use crate::basis::plain_basis::Extended;
    use crate::problem::Problem;
    use crate::problem::tests::problem_of;
    use crate::slot_order::tests::arcs_of_slots;
    use crate::starting_basis::NetSupplies;
    use crate::tree::{DOWN, PlainHanging, UP, check_threading, cycle_walk, hangings_of};

    impl RoomColumns {
        /// The room of each of its columns, in entries, its tree's last.
        fn capacities(&self) -> Vec<usize> {
            let arcs = [self.ends.capacity(), self.costs.capacity(), self.states.capacity()];
            let links = [self.capacities.capacity(), self.flows.capacity()];

            arcs.into_iter().chain(links).chain(self.tree.capacities()).collect()
        }
    }

    impl BasisRoom {
        /// The room of each of its columns, in entries, the net supplies' first.
        fn capacities(&self) -> Vec<usize> {
            self.net_supplies.capacities().into_iter().chain(self.columns.capacities()).collect()
        }
    }

    impl KeptBasisRoom {
        /// The room of each of its columns, in entries, the excesses' first.
        pub(crate) fn capacities(&self) -> Vec<usize> {
            self.excesses.capacities().into_iter().chain(self.room.capacities()).collect()
        }
    }

    #[test]
    fn test_grow_makes_the_room_try_reserve_makes() {
        // 100 nodes and 90 000 arcs: every column's total lies above the least room an empty `Vec` makes, 8 one-byte
        // entries or 4 wider ones, so each total is the room made, and a total that drifts by one shows, the tree's
        // one-byte directions' 100 among them.
        let (mut grown, mut reserved) = (KeptBasisRoom::new(), KeptBasisRoom::new());
        grown.grow(100, 90_000);
        reserved.try_reserve(100, 90_000).expect("100 nodes and 90 000 arcs should fit");
        assert_eq!(grown.capacities(), reserved.capacities());
        assert_eq!(grown.capacities().len(), 1 + 1 + 5 + 8, "the excesses, the net supplies, five columns, the tree");

        let (mut grown, mut reserved) = (BasisRoom::new(), BasisRoom::new());
        grown.grow(100, 90_000);
        reserved.try_reserve(100, 90_000).expect("100 nodes and 90 000 arcs should fit");
        assert_eq!(grown.capacities(), reserved.capacities());
    }

    /// The depot: node 0 sends 4 units to node 2, directly by arc 0 (capacity 3, cost 5) or through node 1 by arcs 1
    /// (capacity 4, cost 1) and 2 (capacity 2, cost 1).
    fn depot() -> MincostNetwork {
        MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1)])
    }

    /// The basis of `problem` loaded into `room`.
    pub(crate) fn loaded<'r, 'p>(room: &'r mut BasisRoom, problem: &Problem<'p>) -> LoadedBasis<'r, 'p> {
        let problem = problem.checked().expect("the test's problem should be admitted");

        room.load(problem).expect("the test's net supplies should be in range")
    }

    /// The star of `problem` made in `room` and kept there, as a network's first load and its keep leave it, at the
    /// problem's own root cost.
    pub(crate) fn kept_star(room: &mut KeptBasisRoom, problem: &Problem<'_>) {
        let problem = problem.checked().expect("the test's problem should be admitted");
        let resumption = room.resume(problem).expect("the test's net supplies should be in range");

        let _ = resumption.begin(problem.root_cost()).keep();
    }

    /// The star of `network`, its lower bounds given, loaded into `room` with its whole invariant checked.
    ///
    /// # Panics
    /// Panics if the invariant fails after the load.
    fn checked_star<'r, 'n>(room: &'r mut BasisRoom, network: &'n MincostNetwork) -> LoadedBasis<'r, 'n> {
        let basis = loaded(room, &problem_of(network));
        if let Err(breach) = check(&basis, network) {
            panic!("the invariant should hold after the load: {breach}");
        }

        basis
    }

    /// How a hung node hangs, for a test to write out.
    fn hung(parent: usize, link: usize, direction: Direction) -> PlainHanging {
        PlainHanging { parent, link, direction }
    }

    /// A basis read out as plain numbers. Plain data.
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Snapshot {
        /// Per link: its flow.
        flows: Vec<u64>,
        /// Per tree node: its potential.
        potentials: Vec<i64>,
        /// Per slot: its arc's state.
        states: Vec<ArcState>,
        /// Per hung node: how it hangs.
        hangings: Vec<PlainHanging>,
        /// Per slot: its arc's violation, as the rows answer it.
        violations: Vec<i64>,
    }

    /// `basis` read out as plain numbers.
    fn snapshot(basis: &Basis<'_>) -> Snapshot {
        let rows = basis.rows();

        Snapshot {
            flows: basis.links.flows.to_vec(),
            potentials: basis.tree.potentials().to_vec(),
            states: basis.arcs.states.to_vec(),
            hangings: hangings_of(&basis.tree),
            violations: rows.violations(0..rows.len()).map(|(_, violation)| violation).collect(),
        }
    }

    /// Whether `moved`, the nodes a change of a basis answered, from `before` to `after`, tops exactly the subtrees
    /// whose potentials moved: each node once; every node whose potential changed in the subtree, after the change, of
    /// exactly one of them, by the same amount as that node, other than zero; and every other node's potential as it
    /// was.
    ///
    /// # Errors
    /// Returns the first node that breaks it.
    fn moved_checked(before: &Snapshot, after: &Snapshot, moved: &[usize]) -> Result<(), String> {
        let root = before.hangings.len();
        if let Some((at, top)) = moved.iter().enumerate().find(|&(at, top)| moved[..at].contains(top)) {
            return Err(format!("node {top} is answered twice, at {at}, in {moved:?}"));
        }

        for node in 0..root {
            let shift = |node: usize| after.potentials[node].wrapping_sub(before.potentials[node]);
            let tops: Vec<usize> =
                moved.iter().copied().filter(|&top| is_ancestor(&after.hangings, root, top, node)).collect();
            let holds = match tops.as_slice() {
                [] => shift(node) == 0,
                &[top] => shift(node) == shift(top) && shift(top) != 0,
                _ => false,
            };
            if !holds {
                return Err(format!("node {node} moved by {} below {tops:?} of {moved:?}", shift(node)));
            }
        }

        Ok(())
    }

    /// Pivots `basis` of `network` on the arc at `slot`, checks the whole invariant, and answers the basis read out.
    ///
    /// # Panics
    /// Panics if the invariant fails after the pivot.
    fn pivoted(basis: &mut LoadedBasis<'_, '_>, network: &MincostNetwork, slot: usize) -> Snapshot {
        basis.pivot(Slot::from(slot));
        if let Err(breach) = check(basis, network) {
            panic!("the invariant should hold after the pivot of slot {slot}: {breach}");
        }

        snapshot(basis)
    }

    /// The flows the basis writes out, with their cost.
    fn written(basis: &LoadedBasis<'_, '_>) -> (Vec<u64>, i128) {
        let mut flows = vec![0; basis.start.problem().arc_count()];
        let cost = basis.write_flows(&mut flows);

        (flows, cost)
    }

    /// Checks every clause of the basis invariant of `basis`, loaded from `network` at its own root cost, recomputed
    /// from the network and the columns by plain indexing; and that its rows price every slot as the definition does.
    /// Answers the first breach.
    pub(crate) fn check(basis: &Basis<'_>, network: &MincostNetwork) -> Result<(), String> {
        check_against(basis, network, &Extended::of(network))
    }

    /// Checks `basis` as [`check`] does, as a basis of `network` at the root cost `root_cost` in place of the
    /// network's own.
    pub(crate) fn check_at(basis: &Basis<'_>, network: &MincostNetwork, root_cost: i64) -> Result<(), String> {
        check_against(basis, network, &Extended::at(network, root_cost))
    }

    /// Checks every clause of the basis invariant of `basis` against `model`, the extension of `network`, and the rows.
    fn check_against(basis: &Basis<'_>, network: &MincostNetwork, model: &Extended) -> Result<(), String> {
        check_shape(basis, network, model)?;
        let links = model.links(network, &hangings_of(&basis.tree));
        let in_tree = check_tree(basis, network, &links)?;
        check_duality(basis, network, model, &links)?;
        check_flow(basis, network, model, &links, &in_tree)?;
        check_strong_feasibility(basis)?;

        check_rows(basis, network, model)
    }

    /// Shape: the lengths of every column, each slot's ends and cost and each link's capacity.
    fn check_shape(basis: &Basis<'_>, network: &MincostNetwork, model: &Extended) -> Result<(), String> {
        let (node_count, arc_count) = (network.node_count(), network.arc_count());
        let ArcColumns { ends, costs, states } = &basis.arcs;
        let LinkColumns { capacities, flows } = &basis.links;
        let lengths = [ends.len(), costs.len(), states.len(), capacities.len(), flows.len()];
        let tree = [basis.tree.node_count(), basis.tree.link_count(), basis.tree.potentials().len()];
        if lengths != [arc_count, arc_count, arc_count, arc_count + node_count, arc_count + node_count]
            || tree != [node_count, arc_count + node_count, node_count + 1]
        {
            return Err(format!("columns of lengths {lengths:?} and a tree of {tree:?} for {node_count} nodes"));
        }

        for (slot, &arc) in model.arc_of_slot.iter().enumerate() {
            let read = (widen(ends[slot].tail), widen(ends[slot].head), costs[slot]);
            if read != (network.tails[arc], network.heads[arc], network.costs[arc]) {
                return Err(format!("slot {slot} holds {read:?}, not arc {arc}"));
            }
        }
        for (link, &capacity) in capacities.iter().enumerate() {
            if capacity != model.capacity(network, link) {
                return Err(format!("link {link} has capacity {capacity}"));
            }
        }

        Ok(())
    }

    /// Tree: the parents lead to the root, each node hangs by a link that joins it to its parent the way it says, its
    /// own root link where it hangs from the root, no link twice; the thread and the columns derived with it are the
    /// ones the parents imply; every tree arc is unpriced. Answers which links are in the tree.
    fn check_tree(
        basis: &Basis<'_>,
        network: &MincostNetwork,
        links: &[(usize, usize, i128)],
    ) -> Result<Vec<bool>, String> {
        // The parents leading to the root are checked with the threading, first.
        check_threading(&basis.tree)?;
        let mut in_tree = vec![false; basis.links.flows.len()];

        for (node, hanging) in hangings_of(&basis.tree).into_iter().enumerate() {
            let (tail, head, _) = links[hanging.link];
            let joined = match hanging.direction {
                Direction::Up => (node, hanging.parent),
                Direction::Down => (hanging.parent, node),
            };
            if (tail, head) != joined || std::mem::replace(&mut in_tree[hanging.link], true) {
                return Err(format!("node {node} hangs by {hanging:?}, which joins {tail} to {head}"));
            }
        }

        let priced_in_tree = (0..network.arc_count()).find(|&slot| in_tree[slot] && basis.arcs.states[slot] != IN_TREE);
        match priced_in_tree {
            Some(slot) => Err(format!("slot {slot} is in the tree at {:?}", basis.arcs.states[slot])),
            None => Ok(in_tree),
        }
    }

    /// Duality: the root cost the basis holds the model's, the root at zero and every tree link tight.
    fn check_duality(
        basis: &Basis<'_>,
        network: &MincostNetwork,
        model: &Extended,
        links: &[(usize, usize, i128)],
    ) -> Result<(), String> {
        if i128::from(basis.root_cost) != model.root_cost {
            return Err(format!("the basis holds the root cost {}, not {}", basis.root_cost, model.root_cost));
        }

        let potentials: Vec<i128> = basis.tree.potentials().iter().map(|&potential| i128::from(potential)).collect();
        if potentials[network.node_count()] != 0 {
            return Err(format!("the root's potential is {}", potentials[network.node_count()]));
        }

        for hanging in hangings_of(&basis.tree) {
            let (tail, head, cost) = links[hanging.link];
            let reduced = cost + potentials[tail] - potentials[head];
            if reduced != 0 {
                return Err(format!("tree link {} has reduced cost {reduced}", hanging.link));
            }
        }

        Ok(())
    }

    /// Flow: conservation at every node, every flow within its capacity, the states out of the tree as the flows are,
    /// and every root link out of the tree empty.
    fn check_flow(
        basis: &Basis<'_>,
        network: &MincostNetwork,
        model: &Extended,
        links: &[(usize, usize, i128)],
        in_tree: &[bool],
    ) -> Result<(), String> {
        let (node_count, arc_count) = (network.node_count(), network.arc_count());
        let flows = &basis.links.flows;
        let mut excess = model.net_supplies.clone();
        excess.push(0);

        for (link, &flow) in flows.iter().enumerate() {
            let (tail, head, _) = links[link];
            excess[tail] -= i128::from(flow);
            excess[head] += i128::from(flow);
            let capacity = model.capacity(network, link);
            if flow > capacity {
                return Err(format!("link {link} carries {flow} above its capacity {capacity}"));
            }

            let state = basis.arcs.states.get(link).copied();
            let stands = match (in_tree[link], state) {
                (true, _) => true,
                (false, None) => flow == 0,
                (false, Some(state)) if capacity == 0 => state == IN_TREE,
                (false, Some(state)) if flow == 0 => state == LOWER,
                (false, Some(state)) => flow == capacity && state == UPPER,
            };
            if !stands {
                return Err(format!("link {link} out of the tree carries {flow} at {state:?}"));
            }
        }

        match (0..=node_count).find(|&node| excess[node] != 0) {
            Some(node) => Err(format!("node {node} keeps an excess of {} with {arc_count} arcs", excess[node])),
            None => Ok(()),
        }
    }

    /// Strong feasibility: every tree link can carry more toward the root.
    fn check_strong_feasibility(basis: &Basis<'_>) -> Result<(), String> {
        for (node, hanging) in hangings_of(&basis.tree).into_iter().enumerate() {
            let (flow, capacity) = (basis.links.flows[hanging.link], basis.links.capacities[hanging.link]);
            let toward_root = match hanging.direction {
                Direction::Up => flow < capacity,
                Direction::Down => flow > 0,
            };
            if !toward_root {
                return Err(format!("node {node} hangs by {hanging:?} carrying {flow} of {capacity}"));
            }
        }

        Ok(())
    }

    /// The rows price every slot, from either end, as its sign times its reduced cost under the tree's potentials.
    fn check_rows(basis: &Basis<'_>, network: &MincostNetwork, model: &Extended) -> Result<(), String> {
        let potentials = basis.tree.potentials();
        let mut expected = Vec::new();
        for (slot, &arc) in model.arc_of_slot.iter().enumerate() {
            let sign = match basis.arcs.states[slot] {
                ArcState::AtLower => 1,
                ArcState::Unpriced => 0,
                ArcState::AtUpper => -1,
            };
            let (tail, head) = (network.tails[arc], network.heads[arc]);
            let reduced = i128::from(network.costs[arc]) + i128::from(potentials[tail]) - i128::from(potentials[head]);
            expected.push((Slot::from(slot), i64::try_from(sign * reduced).map_err(|error| error.to_string())?));
        }

        let rows = basis.rows();
        let forward: Vec<(Slot, i64)> = rows.violations(0..rows.len()).collect();
        let mut backward: Vec<(Slot, i64)> = rows.violations(0..rows.len()).rev().collect();
        backward.reverse();
        if (forward != expected) || (backward != expected) || rows.len() != expected.len() {
            return Err(format!("the rows price {forward:?} and back {backward:?}, not {expected:?}"));
        }

        Ok(())
    }

    /// How a test chooses the entering arc among those that violate. Test functions, not settings.
    #[derive(Debug)]
    pub(crate) enum EnteringRule {
        /// The first violating arc in slot order.
        FirstEligible,
        /// The most violating arc, the first in slot order among equals.
        MostViolating,
        /// A violating arc drawn evenly by the generator.
        RandomEligible(Xoshiro256PlusPlus),
    }

    impl EnteringRule {
        /// The slot of the next entering arc of `rows` by this rule, `None` exactly when no arc violates.
        pub(crate) fn next(&mut self, rows: &PricedRows<'_>) -> Option<Slot> {
            let mut violating = rows.violations(0..rows.len()).filter(|&(_, violation)| violation < 0);
            match self {
                Self::FirstEligible => violating.next().map(|(slot, _)| slot),
                Self::MostViolating => violating.min_by_key(|&(_, violation)| violation).map(|(slot, _)| slot),
                Self::RandomEligible(generator) => {
                    let eligible: Vec<Slot> = violating.map(|(slot, _)| slot).collect();
                    (!eligible.is_empty()).then(|| eligible[generator.random_range(0..eligible.len())])
                }
            }
        }
    }

    /// What a basis had spent before a pivot: its flows, the cost of the flow with root links counted at their costs,
    /// and the entering arc's violation. Plain data.
    struct Spent {
        /// Per link: its flow.
        flows: Vec<u64>,
        /// The cost of the flow above the lower bounds, root links counted at their costs.
        cost: BigInt,
        /// The entering arc's violation, in units of cost.
        violation: i64,
    }

    impl Spent {
        /// What `basis`, loaded from `network`, has spent before the arc at `slot` enters.
        fn of(basis: &Basis<'_>, network: &MincostNetwork, slot: Slot) -> Self {
            let rows = basis.rows();
            let at = usize::from(slot).min(rows.len());
            let violation = rows.violations(at..rows.len()).next().map_or(0, |(_, violation)| violation);

            Self { flows: basis.links.flows.to_vec(), cost: extended_cost(basis, network), violation }
        }

        /// Checks the pivot that sent `step` against what the basis had spent before it: the cost fell by exactly the
        /// step times the violation in magnitude, and the step is zero exactly where no flow changed.
        ///
        /// # Errors
        /// Returns the relation that fails, with its values.
        fn check_step(&self, basis: &Basis<'_>, network: &MincostNetwork, step: u64) -> Result<(), String> {
            let fell = &self.cost - extended_cost(basis, network);
            let expected = BigInt::from(step) * BigInt::from(self.violation.unsigned_abs());
            if fell != expected {
                return Err(format!("the cost fell by {fell}, not the step {step} times |{}|", self.violation));
            }

            let unchanged = basis.links.flows == self.flows.as_slice();
            if unchanged != (step == 0) {
                return Err(format!("the step {step} with the flows unchanged: {unchanged}"));
            }

            Ok(())
        }
    }

    /// The cost of the flow of `basis`, loaded from `network`, above the lower bounds, root links counted at their
    /// costs, in big integers.
    fn extended_cost(basis: &Basis<'_>, network: &MincostNetwork) -> BigInt {
        extended_cost_against(basis, network, &Extended::of(network))
    }

    /// The cost of the flow of `basis`, a basis of `network` at the root cost `root_cost`, above the lower bounds, root
    /// links counted at their costs, in big integers.
    pub(crate) fn extended_cost_at(basis: &Basis<'_>, network: &MincostNetwork, root_cost: i64) -> BigInt {
        extended_cost_against(basis, network, &Extended::at(network, root_cost))
    }

    /// The cost of the flow of `basis` against `model`, the extension of `network`, as [`extended_cost`] answers it.
    fn extended_cost_against(basis: &Basis<'_>, network: &MincostNetwork, model: &Extended) -> BigInt {
        let links = model.links(network, &hangings_of(&basis.tree));

        basis.links.flows.iter().zip(links).map(|(&flow, (.., cost))| BigInt::from(flow) * cost).sum()
    }

    /// The flow up the root links of `basis`, read from its tree: the flow of each node's own root link where the node
    /// hangs by it pointing up.
    pub(crate) fn flow_up_the_root_links(basis: &Basis<'_>) -> u128 {
        let arc_count = basis.arcs.len();
        let up = |(node, hanging): (usize, PlainHanging)| {
            (hanging.link == arc_count + node && hanging.direction == UP).then_some(basis.links.flows[hanging.link])
        };

        hangings_of(&basis.tree).into_iter().enumerate().filter_map(up).map(u128::from).sum()
    }

    /// The slots of the arcs of `basis`, a basis of `network`, that violate their bounds, in ascending order: each
    /// priced from the network's own cost and ends and the tree's potentials, its sign by its state, apart from the
    /// rows.
    pub(crate) fn violating_slots(basis: &Basis<'_>, network: &MincostNetwork) -> Vec<Slot> {
        let potentials = basis.tree.potentials();
        let arc_of_slot = arcs_of_slots(network.arc_count(), network.node_count());
        let violates = |&(slot, &arc): &(usize, &usize)| {
            let sign = match basis.arcs.states[slot] {
                ArcState::AtLower => 1,
                ArcState::Unpriced => 0,
                ArcState::AtUpper => -1,
            };
            let (tail, head) = (network.tails[arc], network.heads[arc]);
            sign * (i128::from(network.costs[arc]) + i128::from(potentials[tail]) - i128::from(potentials[head])) < 0
        };

        arc_of_slot.iter().enumerate().filter(violates).map(|(slot, _)| Slot::from(slot)).collect()
    }

    /// Whether each arc is a link of the tree of `basis`, slot `s` at `s`.
    pub(crate) fn tree_arcs(basis: &Basis<'_>) -> Vec<bool> {
        let mut in_tree = vec![false; basis.arcs.len()];
        for hanging in hangings_of(&basis.tree) {
            if let Some(slot) = in_tree.get_mut(hanging.link) {
                *slot = true;
            }
        }

        in_tree
    }

    /// The way each node's own root link points where the node hangs by it, node `v` at `v`; `None` where it hangs by
    /// an arc.
    pub(crate) fn root_link_directions(basis: &Basis<'_>) -> Vec<Option<Direction>> {
        let arc_count = basis.arcs.len();
        let hangings = hangings_of(&basis.tree).into_iter().enumerate();

        hangings.map(|(node, hanging)| (hanging.link == arc_count + node).then_some(hanging.direction)).collect()
    }

    /// How a checked run ended. Plain data.
    #[derive(Debug, PartialEq, Eq)]
    pub(crate) struct RunEnd {
        /// The pivots it made.
        pub(crate) pivots: usize,
        /// The flows, potentials and cost it answers where every root link carries nothing; `None` where one carries
        /// flow.
        pub(crate) answer: Option<OptimalFlow>,
    }

    /// The binomial coefficient `choose(total, chosen)`.
    fn binomial(total: u128, chosen: u128) -> u128 {
        (0..chosen).fold(1, |product, at| product * (total - at) / (at + 1))
    }

    /// The most pivots a run on `basis`, loaded from `network`, makes, whatever arcs enter, if it never returns to a
    /// basis it left.
    fn pivot_bound(basis: &Basis<'_>, network: &MincostNetwork) -> u128 {
        // A pivot that sends flow lowers the cost of the flow, root links counted, by one at least (`Basis::pivot`).
        // That cost starts at the starting basis's, and never falls below the least the arcs can cost,
        // `−Σ |cost|·width`, the root links costing nothing less than zero. So at most the difference pivots send
        // flow. A stretch of pivots that send nothing stays at one flow and visits each tree once at most, and a tree
        // is a choice of `n` of the `m + n` links: before the first pivot that sends flow, between two, and after the
        // last, at most `choose(m + n, n)` pivots.
        let wide = |value: usize| u128::try_from(value).expect("a test's count should fit a u128");
        let arcs = (0..network.arc_count()).map(|arc| {
            BigInt::from(network.costs[arc].unsigned_abs()) * (network.capacities[arc] - network.lower_bounds[arc])
        });
        let sending = extended_cost(basis, network) + arcs.sum::<BigInt>();
        let sending = u128::try_from(sending).expect("a basis should cost at least the least the arcs can cost");
        let trees = binomial(wide(network.arc_count() + network.node_count()), wide(network.node_count()));

        (sending + 1) * (trees + 1)
    }

    /// A run from a start of a network, the star or a kept basis re-flowed, its whole invariant checked after the start
    /// and after every pivot, which fails once it makes more pivots than [`pivot_bound`] allows: the one place a test
    /// pivots a basis to its end, under whatever entering rule the test drives it with.
    #[derive(Debug)]
    pub(crate) struct CheckedRun<'r, 'n> {
        /// The network the basis was loaded from, which the checks recompute the invariant from.
        network: &'n MincostNetwork,
        /// The basis pivoted.
        basis: LoadedBasis<'r, 'n>,
        /// The pivots made so far.
        pivots: usize,
        /// The most pivots the run may make: [`pivot_bound`] of the starting basis.
        bound: u128,
    }

    /// Which start a resume gave a run.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Begun {
        /// The kept basis, re-flowed.
        Kept,
        /// The star.
        Star,
    }

    /// The basis `room` lends for `problem` from what it holds: the kept basis re-flowed, or the star, with which it
    /// was.
    ///
    /// # Panics
    /// Panics if the problem is refused.
    fn resumed_in<'r, 'p>(
        room: &'r mut KeptBasisRoom,
        problem: &Problem<'p>,
    ) -> (LentBasis<'r, LoadedBasis<'r, 'p>>, Begun) {
        let problem = problem.checked().expect("the test's problem should be admitted");
        let resumption = room.resume(problem).expect("the test's net supplies should be in range");
        let begun = if resumption.kept() { Begun::Kept } else { Begun::Star };

        (resumption.begin(problem.root_cost()), begun)
    }

    impl<'r, 'n> CheckedRun<'r, 'n> {
        /// The run of `problem`, which is `network` with its lower bounds given or, where they are all zero, not,
        /// loaded into `room`.
        ///
        /// # Errors
        /// Returns the breach of the invariant after the load.
        pub(crate) fn start(
            room: &'r mut BasisRoom,
            network: &'n MincostNetwork,
            problem: &Problem<'n>,
        ) -> Result<Self, String> {
            let basis = loaded(room, problem);
            check(&basis, network).map_err(|breach| format!("after the load: {breach}"))?;
            let bound = pivot_bound(&basis, network);

            Ok(Self { network, basis, pivots: 0, bound })
        }

        /// The run of `problem`, which is `network` with its lower bounds given or, where they are all zero, not, from
        /// what `room` holds once the run of `before`, from its star, has ended in it under the first eligible rule:
        /// the kept basis re-flowed, or the star; and which.
        ///
        /// # Panics
        /// Panics if either problem is refused.
        ///
        /// # Errors
        /// Returns the breach of the invariant in the run of `before`, or after the resume.
        pub(crate) fn resumed(
            room: &'r mut KeptBasisRoom,
            before: &MincostNetwork,
            network: &'n MincostNetwork,
            problem: &Problem<'n>,
        ) -> Result<(Self, Begun), String> {
            let mut rule = EnteringRule::FirstEligible;
            CheckedRun::start(&mut room.room, before, &problem_of(before))?
                .run_to_end(|rows| rule.next(rows))
                .map_err(|breach| format!("before the resume: {breach}"))?;
            // The room holds the basis the run ended on, as a network's keep leaves it.
            room.kept = Kept::Basis;

            let (basis, begun) = resumed_in(room, problem);
            let basis = basis.into_basis();
            check(&basis, network).map_err(|breach| format!("after the resume to {begun:?}: {breach}"))?;
            let bound = pivot_bound(&basis, network);

            Ok((Self { network, basis, pivots: 0, bound }, begun))
        }

        /// Pivots on the arc at `slot` and checks the invariant.
        ///
        /// # Errors
        /// Returns that the pivot would pass the bound, or the breach of the invariant after it.
        pub(crate) fn enter(&mut self, slot: Slot) -> Result<(), String> {
            self.pivots += 1;
            if u128::try_from(self.pivots).expect("a count should fit a u128") > self.bound {
                return Err(format!("pivot {} passes the bound {}: the run cycles", self.pivots, self.bound));
            }

            let before = Spent::of(&self.basis, self.network, slot);
            let step = self.basis.pivot(slot);
            check(&self.basis, self.network)
                .map_err(|breach| format!("after pivot {}, of {slot:?}: {breach}", self.pivots))?;

            before
                .check_step(&self.basis, self.network, step)
                .map_err(|breach| format!("pivot {}: {breach}", self.pivots))
        }

        /// Enters the arcs at `first` in turn, each where it violates its bound when its turn comes, as a run enters
        /// the arcs its first pivots chose.
        ///
        /// # Errors
        /// As [`enter`](Self::enter).
        pub(crate) fn enter_first(&mut self, first: &[Slot]) -> Result<(), String> {
            for &slot in first {
                if self.basis.violates(slot) {
                    self.enter(slot)?;
                }
            }

            Ok(())
        }

        /// Pivots on the arc `next` chooses from the rows and checks the invariant; answers the arc's slot, or `None`
        /// where `next` chooses none and the run is at its end.
        ///
        /// # Errors
        /// As [`enter`](Self::enter).
        pub(crate) fn pivot_next(
            &mut self,
            next: impl FnOnce(&PricedRows<'_>) -> Option<Slot>,
        ) -> Result<Option<Slot>, String> {
            let Some(slot) = next(&self.basis.rows()) else { return Ok(None) };

            self.enter(slot).map(|()| Some(slot))
        }

        /// Pivots on the arcs `next` chooses until it chooses none, and answers how the run ended.
        ///
        /// # Errors
        /// As [`enter`](Self::enter).
        pub(crate) fn run_to_end(
            mut self,
            mut next: impl FnMut(&PricedRows<'_>) -> Option<Slot>,
        ) -> Result<RunEnd, String> {
            while self.pivot_next(&mut next)?.is_some() {}

            let routed = self.basis.routes_every_supply();
            if routed != (self.basis.unrouted() == 0) {
                return Err(format!("the scan of the root links answers {routed}, the flow up them the other"));
            }
            let (flows, total_cost) = written(&self.basis);
            let potentials = self.basis.keep().to_vec();

            Ok(RunEnd { pivots: self.pivots, answer: routed.then_some(OptimalFlow { flows, potentials, total_cost }) })
        }
    }

    /// Runs `network` from the star, its lower bounds given, with the arcs `rule` chooses, to its end.
    ///
    /// # Errors
    /// As [`CheckedRun::pivot_next`].
    fn run_checked(network: &MincostNetwork, mut rule: EnteringRule) -> Result<RunEnd, String> {
        let mut room = BasisRoom::new();

        CheckedRun::start(&mut room, network, &problem_of(network))?.run_to_end(|rows| rule.next(rows))
    }

    #[test]
    fn test_the_depot_pivot_for_pivot_by_hand() {
        // Three nodes and three arcs: the stride is one, so slot `s` holds arc `s`; the root is node 3 and node `v`'s
        // root link is link `3 + v`. The root cost is `2 · 5 + 1 = 11`: node 2 hangs down at 11, nodes 0 and 1 up at 0.
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        let star = Snapshot {
            flows: vec![0, 0, 0, 4, 0, 4],
            potentials: vec![0, 0, 11, 0],
            states: vec![LOWER, LOWER, LOWER],
            hangings: vec![hung(3, 3, UP), hung(3, 4, UP), hung(3, 5, DOWN)],
            violations: vec![5 - 11, 1, 1 - 11],
        };
        assert_eq!(snapshot(&basis), star);

        // Arc 0 from its lower bound: the cycle 0 → 2, up node 2's root link against it (room 4), down node 0's
        // against it (room 4); its own width 3 is the least: it fills, nothing leaves.
        let filled = Snapshot { flows: vec![3, 0, 0, 1, 0, 1], states: vec![UPPER, LOWER, LOWER], ..star.clone() };
        assert_eq!(pivoted(&mut basis, &network, 0), Snapshot { violations: vec![6, 1, -10], ..filled.clone() });

        // Arc 2 from its lower bound: the first side is node 1, whose root link runs up and carries nothing, room 0;
        // the second side node 2, room 1. A degenerate pivot: node 1's root link leaves on the first side, node 1
        // hangs from node 2 by arc 2 running up from it, its potential up by 10, the arc's violation.
        let degenerate = Snapshot {
            potentials: vec![0, 10, 11, 0],
            states: vec![UPPER, LOWER, IN_TREE],
            hangings: vec![hung(3, 3, UP), hung(2, 2, UP), hung(3, 5, DOWN)],
            violations: vec![6, 1 - 10, 0],
            ..filled
        };
        assert_eq!(pivoted(&mut basis, &network, 2), degenerate);

        // Arc 1 from its lower bound, `1 + 0 − 10 = −9`: the cycle 0 → 1, up arc 2 along it (room 2), up node 2's root
        // link against it (room 1), down node 0's root link against it (room 1). Of the two of room 1 the second
        // side's, met last, leaves: one unit moves, and the subtree of node 2, holding node 1, hangs from node 0 by
        // arc 1, both potentials down by 9; arc 2 now hangs node 2 from node 1, turned around.
        let routed = Snapshot {
            flows: vec![3, 1, 1, 0, 0, 0],
            potentials: vec![0, 1, 2, 0],
            states: vec![UPPER, IN_TREE, IN_TREE],
            hangings: vec![hung(3, 3, UP), hung(0, 1, DOWN), hung(1, 2, DOWN)],
            violations: vec![-(5 - 2), 0, 0],
        };
        assert_eq!(pivoted(&mut basis, &network, 1), routed);

        // Arc 0 from its capacity, of reduced cost `5 + 0 − 2 = 3`: its flow falls, which sends flow along it from its
        // head, node 2, the first end, to its tail, node 0, the second end and the apex. The first side runs from node
        // 2 up through node 1, where the cycle's flow runs down the tree along arc 2 (room `2 − 1 = 1`) and arc 1
        // (room `4 − 1 = 3`); the second side is empty. Arc 2's room 1 is the least, below the width 3: arc 2 leaves at
        // its capacity, one unit moves, and node 2 hangs from node 0 by arc 0, running down to it, its potential up
        // by 3.
        let optimal = Snapshot {
            flows: vec![2, 2, 2, 0, 0, 0],
            potentials: vec![0, 1, 5, 0],
            states: vec![IN_TREE, IN_TREE, UPPER],
            hangings: vec![hung(3, 3, UP), hung(0, 1, DOWN), hung(0, 0, DOWN)],
            violations: vec![0, 0, -(1 + 1 - 5)],
        };
        assert_eq!(pivoted(&mut basis, &network, 0), optimal);

        assert_eq!(basis.unrouted(), 0);
        assert_eq!(written(&basis), (vec![2, 2, 2], 14));
        assert_eq!(basis.keep(), [0, 1, 5]);
        assert_eq!(check_certificate(&network, &[2, 2, 2], &[0, 1, 5], 14), Ok(()));
    }

    #[test]
    fn test_a_pivot_answers_the_flow_it_sent_by_hand() {
        // The pivots of the depot's hand-worked run: arc 0 fills to its width 3 with no link leaving; arc 2 enters
        // degenerately, node 1's empty root link leaving; arcs 1 and 0 each move one unit.
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);

        let steps = [0_usize, 2, 1, 0].map(|slot| basis.pivot(Slot::from(slot)));
        assert_eq!(steps, [3, 0, 1, 1]);
        assert_eq!(written(&basis), (vec![2, 2, 2], 14));
    }

    /// The depot's star begun in `room`, which holds no basis, at the root cost `root_cost`.
    fn depot_star_at<'r>(
        room: &'r mut KeptBasisRoom,
        network: &'r MincostNetwork,
        root_cost: i64,
    ) -> LoadedBasis<'r, 'r> {
        let problem = problem_of(network).checked().expect("the depot should be admitted");
        let resumption = room.resume(problem).expect("the depot's net supplies should be in range");
        assert!(!resumption.kept(), "the room should hold no basis");

        resumption.begin(root_cost).into_basis()
    }

    #[test]
    fn test_a_star_at_a_wider_root_cost_reaches_the_same_optimum_by_hand() {
        // The depot at the root cost 20 for its own 11: node 2 hangs down from the root at 20, and the arcs into it
        // violate by 9 more. The hand-worked run's pivots take the same steps: arc 0 fills, arc 2 enters degenerately,
        // node 1 now at `20 − 1 = 19`, and arc 1, at `1 + 0 − 19 = −18`, moves one unit and takes nodes 1 and 2 down by
        // 18 to 1 and 2, below node 0, the root cost gone from every tree link; arc 0 then moves the last unit.
        let network = depot();
        let mut room = KeptBasisRoom::new();
        let mut basis = depot_star_at(&mut room, &network, 20);
        let star = Snapshot {
            flows: vec![0, 0, 0, 4, 0, 4],
            potentials: vec![0, 0, 20, 0],
            states: vec![LOWER, LOWER, LOWER],
            hangings: vec![hung(3, 3, UP), hung(3, 4, UP), hung(3, 5, DOWN)],
            violations: vec![5 - 20, 1, 1 - 20],
        };
        assert_eq!(snapshot(&basis), star);

        let steps = [0_usize, 2, 1, 0].map(|slot| basis.pivot(Slot::from(slot)));
        assert_eq!(steps, [3, 0, 1, 1]);
        assert_eq!(snapshot(&basis).potentials, [0, 1, 5, 0]);
        assert_eq!(basis.unrouted(), 0);
        assert_eq!(written(&basis), (vec![2, 2, 2], 14));

        // The widest root cost three nodes admit, `2·B + 1` for `B = ⌊(2^62 − 1) / 3⌋`.
        let widest = 2 * cost_bound(3) + 1;
        let basis = depot_star_at(&mut room, &network, widest);
        assert_eq!(snapshot(&basis).potentials, [0, 0, widest, 0]);
        assert_eq!(snapshot(&basis).violations, [5 - widest, 1, 1 - widest]);
    }

    #[test]
    fn test_two_parallel_arcs_pivot_for_pivot_by_hand() {
        // Node 0 sends one unit to node 1 by arc 0 (cost 3) or arc 1 (cost 1), each of capacity 1. The root cost is
        // `1 · 3 + 1 = 4`; links 2 and 3 are the root links of nodes 0 and 1.
        let network = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 1, 3), (0, 1, 0, 1, 1)]);
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        let star = Snapshot {
            flows: vec![0, 0, 1, 1],
            potentials: vec![0, 4, 0],
            states: vec![LOWER, LOWER],
            hangings: vec![hung(2, 2, UP), hung(2, 3, DOWN)],
            violations: vec![-1, -3],
        };
        assert_eq!(snapshot(&basis), star);

        // Arc 0: both root links run against the cycle with room 1, the arc's width 1 too: the second side's leaves.
        let first = Snapshot {
            flows: vec![1, 0, 0, 0],
            potentials: vec![0, 3, 0],
            states: vec![IN_TREE, LOWER],
            hangings: vec![hung(2, 2, UP), hung(0, 0, DOWN)],
            violations: vec![0, 1 - 3],
        };
        assert_eq!(pivoted(&mut basis, &network, 0), first);

        // Arc 1: its apex is node 0, the second side node 1 on arc 0, against it with room 1, equal to arc 1's width:
        // the second side's link leaves at its lower bound, and node 1 hangs by arc 1, its potential down by 2.
        let second = Snapshot {
            flows: vec![0, 1, 0, 0],
            potentials: vec![0, 1, 0],
            states: vec![LOWER, IN_TREE],
            hangings: vec![hung(2, 2, UP), hung(0, 1, DOWN)],
            violations: vec![3 - 1, 0],
        };
        assert_eq!(pivoted(&mut basis, &network, 1), second);
        assert_eq!(basis.unrouted(), 0);
        assert_eq!(written(&basis), (vec![0, 1], 1));
    }

    #[test]
    fn test_a_loop_of_negative_cost_fills_and_one_of_positive_cost_stays_empty_by_hand() {
        // One node of no supply with two loops of capacity 2, of cost −3 and 4; the stride is two and deals them to
        // slots 0 and 1. The root cost is `0 · 4 + 1 = 1`, unused: the node's root link points up.
        let network = MincostNetwork::from_arcs(&[0], &[(0, 0, 0, 2, -3), (0, 0, 0, 2, 4)]);
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        let star = Snapshot {
            flows: vec![0, 0, 0],
            potentials: vec![0, 0],
            states: vec![LOWER, LOWER],
            hangings: vec![hung(1, 2, UP)],
            violations: vec![-3, 4],
        };
        assert_eq!(snapshot(&basis), star);

        // A loop closes a cycle of no tree link: its own width is the step, and it moves to its capacity.
        let filled = Snapshot { flows: vec![2, 0, 0], states: vec![UPPER, LOWER], violations: vec![3, 4], ..star };
        assert_eq!(pivoted(&mut basis, &network, 0), filled);
        assert_eq!(basis.unrouted(), 0);
        assert_eq!(written(&basis), (vec![2, 0], -6));
    }

    #[test]
    fn test_an_arc_of_no_width_is_never_priced_by_hand() {
        // Arc 0 has capacity 0 at cost −5, arc 1 capacity 1 at cost 2; node 0 sends one unit to node 1. The root cost
        // is `1 · 5 + 1 = 6`. Arc 0's reduced cost is `−5 − 6 = −11`, which would violate; unpriced, it does not.
        let network = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 0, -5), (0, 1, 0, 1, 2)]);
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        let star = Snapshot {
            flows: vec![0, 0, 1, 1],
            potentials: vec![0, 6, 0],
            states: vec![IN_TREE, LOWER],
            hangings: vec![hung(2, 2, UP), hung(2, 3, DOWN)],
            violations: vec![0, 2 - 6],
        };
        assert_eq!(snapshot(&basis), star);
        assert!(!basis.violates(Slot::from(0_usize)) && basis.violates(Slot::from(1_usize)));

        let routed = Snapshot {
            flows: vec![0, 1, 0, 0],
            potentials: vec![0, 2, 0],
            states: vec![IN_TREE, IN_TREE],
            hangings: vec![hung(2, 2, UP), hung(0, 1, DOWN)],
            violations: vec![0, 0],
        };
        assert_eq!(pivoted(&mut basis, &network, 1), routed);
        assert_eq!(written(&basis), (vec![0, 1], 2));
    }

    #[test]
    fn test_an_arc_whose_lower_bound_is_its_capacity_carries_it_unpriced_by_hand() {
        // Node 0 supplies 2 to node 1; arc 0 carries exactly 1 at cost 3, arc 1 up to 4 at cost 1. The net supplies
        // are 1 and −1, the root cost `1 · 3 + 1 = 4`.
        let network = MincostNetwork::from_arcs(&[2, -2], &[(0, 1, 1, 1, 3), (0, 1, 0, 4, 1)]);
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        assert_eq!(basis.links.capacities, [0, 4, u64::MAX, u64::MAX]);
        assert_eq!(snapshot(&basis).violations, [0, 1 - 4]);

        // Arc 1: the first side's root link and the second side's both have room 1, below the width 4; the second's
        // leaves.
        let routed = Snapshot {
            flows: vec![0, 1, 0, 0],
            potentials: vec![0, 1, 0],
            states: vec![IN_TREE, IN_TREE],
            hangings: vec![hung(2, 2, UP), hung(0, 1, DOWN)],
            violations: vec![0, 0],
        };
        assert_eq!(pivoted(&mut basis, &network, 1), routed);
        assert_eq!(basis.unrouted(), 0);
        assert_eq!(written(&basis), (vec![1, 1], 4));
    }

    #[test]
    fn test_arcs_flip_from_lower_to_upper_and_back_without_a_leaving_link_by_hand() {
        // Node 0 supplies 2 to node 2; arc 0: 0 → 1 (capacity 1, cost −1), arc 1: 1 → 2 (5, 5), arc 2: 0 → 2 (5, 1).
        // The root cost is `2 · 5 + 1 = 11`; links 3, 4, 5 are the root links of nodes 0, 1, 2.
        let network = MincostNetwork::from_arcs(&[2, 0, -2], &[(0, 1, 0, 1, -1), (1, 2, 0, 5, 5), (0, 2, 0, 5, 1)]);
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        let star = Snapshot {
            flows: vec![0, 0, 0, 2, 0, 2],
            potentials: vec![0, 0, 11, 0],
            states: vec![LOWER, LOWER, LOWER],
            hangings: vec![hung(3, 3, UP), hung(3, 4, UP), hung(3, 5, DOWN)],
            violations: vec![-1, 5 - 11, 1 - 11],
        };
        assert_eq!(snapshot(&basis), star);

        // Arc 0 from its lower bound: through the root, node 0's root link against the cycle (room 2) and node 1's
        // along it (room `u64::MAX`): both root links point up. Its width 1 is the least: it flips to its capacity.
        let flipped = Snapshot {
            flows: vec![1, 0, 0, 1, 1, 2],
            states: vec![UPPER, LOWER, LOWER],
            violations: vec![1, -6, -10],
            ..star
        };
        assert_eq!(pivoted(&mut basis, &network, 0), flipped);

        // Arc 1: node 1's root link, first side, room 1; node 2's, second side, room 2. Node 1's leaves on the first
        // side, one unit moving: node 1 hangs up from node 2 by arc 1, its potential up by 6.
        let second = Snapshot {
            flows: vec![1, 1, 0, 1, 0, 1],
            potentials: vec![0, 6, 11, 0],
            states: vec![UPPER, IN_TREE, LOWER],
            hangings: vec![hung(3, 3, UP), hung(2, 1, UP), hung(3, 5, DOWN)],
            violations: vec![-(-1 - 6), 0, -10],
        };
        assert_eq!(pivoted(&mut basis, &network, 1), second);

        // Arc 2: node 0's root link (first side) and node 2's (second side) both have room 1: node 2's leaves, and the
        // subtree of nodes 2 and 1 hangs from node 0 by arc 2, both potentials down by 10.
        let third = Snapshot {
            flows: vec![1, 1, 1, 0, 0, 0],
            potentials: vec![0, -4, 1, 0],
            states: vec![UPPER, IN_TREE, IN_TREE],
            hangings: vec![hung(3, 3, UP), hung(2, 1, UP), hung(0, 2, DOWN)],
            violations: vec![-(-1 + 4), 0, 0],
        };
        assert_eq!(pivoted(&mut basis, &network, 2), third);

        // Arc 0 from its capacity, `−1 + 0 + 4 = 3`: the cycle from node 1 to node 0, its apex node 0; the first side
        // node 1 on arc 1 against it (room 1, equal to the width 1, so not taken), node 2 on arc 2 along it (room 4).
        // No link leaves: the arc flips back to its lower bound, and the unit goes by arc 2.
        let back = Snapshot {
            flows: vec![0, 0, 2, 0, 0, 0],
            states: vec![LOWER, IN_TREE, IN_TREE],
            violations: vec![3, 0, 0],
            ..third
        };
        assert_eq!(pivoted(&mut basis, &network, 0), back);
        assert_eq!(basis.unrouted(), 0);
        assert_eq!(written(&basis), (vec![0, 0, 2], 2));
    }

    #[test]
    fn test_a_cycle_through_two_root_links_that_point_down_by_hand() {
        // Node 0 supplies 2; nodes 1 and 2 demand one each; arc 0: 1 → 2, capacity 3, cost −1. The root cost is
        // `2 · 1 + 1 = 3`; links 1, 2, 3 are the root links of nodes 0, 1, 2.
        let network = MincostNetwork::from_arcs(&[2, -1, -1], &[(1, 2, 0, 3, -1)]);
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        assert_eq!(snapshot(&basis).violations, [-1 + 3 - 3]);

        // The first side, node 1, runs along its root link (room `u64::MAX − 1`), the second, node 2, against its own
        // (room 1): node 2's leaves, one unit moving, and the root link along the cycle grows to 2.
        let routed = Snapshot {
            flows: vec![1, 2, 2, 0],
            potentials: vec![0, 3, 2, 0],
            states: vec![IN_TREE],
            hangings: vec![hung(3, 1, UP), hung(3, 2, DOWN), hung(1, 0, DOWN)],
            violations: vec![0],
        };
        assert_eq!(pivoted(&mut basis, &network, 0), routed);

        // Node 0 has no arc: its supply cannot leave.
        assert!(basis.unrouted() > 0);
        assert_eq!(brute_force_optimum(&network), None);
    }

    #[test]
    fn test_an_infeasible_cut_is_decided_by_hand() {
        // Node 0 supplies 2 to node 1, but arc 0: 0 → 1 carries one unit; arc 1: 1 → 0 runs from the demand to the
        // supply, along both root links of its cycle, and never violates: `1 + 2 − 0 = 3`. The root cost is 2.
        let network = MincostNetwork::from_arcs(&[2, -2], &[(0, 1, 0, 1, 1), (1, 0, 0, 5, 1)]);
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        assert_eq!(snapshot(&basis).violations, [1 - 2, 1 + 2]);

        // Arc 0: both root links against the cycle with room 2, above its width 1: it flips.
        let flipped = pivoted(&mut basis, &network, 0);
        assert_eq!((flipped.flows, flipped.violations), (vec![1, 0, 1, 1], vec![1, 3]));

        assert!(basis.unrouted() > 0);
        assert_eq!(brute_force_optimum(&network), None);
    }

    #[test]
    fn test_a_problem_of_no_node_loads_and_answers_nothing() {
        let network = MincostNetwork::default();
        let mut room = BasisRoom::new();
        let basis = checked_star(&mut room, &network);

        assert_eq!(check(&basis, &network), Ok(()));
        assert!(basis.rows().len() == 0 && basis.unrouted() == 0);
        assert_eq!(written(&basis), (vec![], 0));
        assert_eq!(basis.keep(), [0_i64; 0]);
    }

    #[test]
    fn test_the_load_and_the_write_out_without_lower_bounds_by_hand() {
        // Node 0 supplies 2 to node 1 by arc 0 (capacity 1, cost 3) or arc 1 (capacity 4, cost 1), no lower bounds
        // given: every arc's is zero. The root cost is `1 · 3 + 1 = 4`; links 2 and 3 are the root links.
        let network = MincostNetwork::from_arcs(&[2, -2], &[(0, 1, 0, 1, 3), (0, 1, 0, 4, 1)]);
        let problem = Problem { lower_bounds: None, ..problem_of(&network) };
        let mut room = BasisRoom::new();
        let mut basis = loaded(&mut room, &problem);
        assert_eq!(check(&basis, &network), Ok(()));
        let star = Snapshot {
            flows: vec![0, 0, 2, 2],
            potentials: vec![0, 4, 0],
            states: vec![LOWER, LOWER],
            hangings: vec![hung(2, 2, UP), hung(2, 3, DOWN)],
            violations: vec![3 - 4, 1 - 4],
        };
        assert_eq!((snapshot(&basis), basis.links.capacities.to_vec()), (star, vec![1, 4, u64::MAX, u64::MAX]));

        // Arc 1, of reduced cost −3: both root links run against the cycle with room 2, below the width 4, and the
        // second side's leaves: two units move, and node 1 hangs from node 0 by arc 1, its potential down by 3.
        let routed = Snapshot {
            flows: vec![0, 2, 0, 0],
            potentials: vec![0, 1, 0],
            states: vec![LOWER, IN_TREE],
            hangings: vec![hung(2, 2, UP), hung(0, 1, DOWN)],
            violations: vec![3 - 1, 0],
        };
        assert_eq!(pivoted(&mut basis, &network, 1), routed);
        assert_eq!(written(&basis), (vec![0, 2], 2));
    }

    #[test]
    fn test_the_rows_price_by_checked_indexing_from_both_ends() {
        // The depot after two pivots: arc 0 at its capacity, arc 2 in the tree, potentials 0, 10, 11.
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = loaded(&mut room, &problem_of(&network));
        basis.pivot(Slot::from(0_usize));
        basis.pivot(Slot::from(2_usize));

        let potentials = basis.tree.potentials().to_vec();
        let ArcColumns { ends, costs, states } = &basis.arcs;
        let row = |slot: usize| {
            let (tail, head) = (widen(ends[slot].tail), widen(ends[slot].head));
            let sign = match states[slot] {
                ArcState::AtLower => 1,
                ArcState::Unpriced => 0,
                ArcState::AtUpper => -1,
            };
            (Slot::from(slot), sign * (costs[slot] + potentials[tail] - potentials[head]))
        };
        let expected: Vec<(Slot, i64)> = (0..3).map(row).collect();
        assert_eq!(expected.iter().map(|&(_, violation)| violation).collect::<Vec<_>>(), [6, -9, 0]);

        let rows = basis.rows();
        assert_eq!(rows.violations(0..3).collect::<Vec<_>>(), expected);
        assert_eq!(rows.violations(1..3).rev().collect::<Vec<_>>(), [expected[2], expected[1]]);

        let mut both_ends = rows.violations(0..3);
        assert_eq!((both_ends.len(), both_ends.next_back()), (3, Some(expected[2])));
        assert_eq!((both_ends.next(), both_ends.len()), (Some(expected[0]), 1));
        assert_eq!(both_ends.next_back(), Some(expected[1]));
        assert_eq!((both_ends.next(), both_ends.next_back(), both_ends.next(), both_ends.len()), (None, None, None, 0));
        assert_eq!(rows.violations(2..2).len(), 0);
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn test_violates_answers_each_slot_by_hand() {
        let network = depot();
        let mut room = BasisRoom::new();
        let basis = loaded(&mut room, &problem_of(&network));

        let answers = [0_usize, 1, 2].map(|slot| basis.violates(Slot::from(slot)));
        assert_eq!(answers, [true, false, true]);
    }

    #[test]
    fn test_reduced_cost_by_hand() {
        assert_eq!(reduced_cost(5, 0, 11), -6);
        assert_eq!(reduced_cost(-1, 4, -4), 7);
        assert_eq!(reduced_cost(0, 0, 0), 0);
    }

    /// The slot `slot` of `basis`, made by the checked maker.
    ///
    /// # Panics
    /// Panics if `slot` is not below the arc count.
    fn arc_slot(basis: &Basis<'_>, slot: usize) -> ArcSlot {
        basis.arcs.slot(Slot::from(slot)).expect("the slot should be below the arc count")
    }

    /// The ratio test of `basis` on the cycle from `first` to `second` with an entering arc of width `width`: the
    /// step, and the link that leaves with its side, as plain numbers. The basis is unchanged.
    fn ratio_test(
        basis: &mut LoadedBasis<'_, '_>,
        [first, second]: [usize; 2],
        width: u64,
    ) -> (u64, Option<(usize, Side)>) {
        let mut walk = cycle_walk(&mut basis.basis.tree, first, second);
        let RatioTest { step, leaving } = basis.basis.links.least_room(&mut walk, width);
        let cycle = walk.into_cycle(leaving);

        (step, Exchange::of(cycle).map(|exchange| (usize::from(exchange.leaving_link()), exchange.side())))
    }

    #[test]
    fn test_the_ratio_test_keeps_the_last_link_of_least_room_from_the_apex_by_hand() {
        // The depot's star: node 0 up by link 3 carrying 4, node 1 up by link 4 carrying nothing, node 2 down by link
        // 5 carrying 4. Every cycle runs through the root.
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);

        // From 1 to 2: node 1's link, against the flow on the first side, has room 0, the least: it leaves.
        assert_eq!(ratio_test(&mut basis, [1, 2], 2), (0, Some((4, Side::First))));
        // From 0 to 2: both root links, against the flow, have room 4. Below the width 5 the second side's, met last
        // from the apex, leaves; at the width 4 too, since the arc comes before the second side; at the width 3 the
        // arc's own room is the least and nothing leaves.
        assert_eq!(ratio_test(&mut basis, [0, 2], 5), (4, Some((5, Side::Second))));
        assert_eq!(ratio_test(&mut basis, [0, 2], 4), (4, Some((5, Side::Second))));
        assert_eq!(ratio_test(&mut basis, [0, 2], 3), (3, None));
        // From 0 to 1: node 0's link on the first side has room 4, node 1's along the flow `u64::MAX`. At the width 4
        // the first side's link ties with the arc, which comes after it from the apex: nothing leaves.
        assert_eq!(ratio_test(&mut basis, [0, 1], 4), (4, None));
        assert_eq!(ratio_test(&mut basis, [0, 1], 5), (4, Some((3, Side::First))));
        // A loop closes a cycle of no link.
        assert_eq!(ratio_test(&mut basis, [2, 2], 7), (7, None));

        assert_eq!(check(&basis, &network), Ok(()), "the ratio test changes nothing");
    }

    #[test]
    fn test_the_ratio_test_breaks_ties_within_and_between_the_sides_by_hand() {
        // Four nodes of no supply, every root link up at zero flow and every potential zero in the star; three arcs of
        // width 4 at cost −1, dealt to their own slots: 0 from 1 to 0, 1 from 3 to 2, 2 from 2 to 0. Pivoting slots 2,
        // 1 and 0 hangs 2 and 1 from 0 and 3 from 2, each link up from its node, every flow still zero: each pivot's
        // first end's root link has room zero against the cycle's flow, the least, and leaves.
        let network = MincostNetwork::from_arcs(&[0, 0, 0, 0], &[(1, 0, 0, 4, -1), (3, 2, 0, 4, -1), (2, 0, 0, 4, -1)]);
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        for slot in [2_usize, 1, 0] {
            basis.pivot(Slot::from(slot));
        }
        let expected = [hung(4, 3, UP), hung(0, 0, UP), hung(0, 2, UP), hung(2, 1, UP)];
        assert_eq!(hangings_of(&basis.tree), expected);

        // From 1 to 3: the walk yields 3, 1, 2. The first side's one link, against the flow, has room zero and leaves;
        // the second side's two, along it, room 4 each.
        assert_eq!(ratio_test(&mut basis, [1, 3], 9), (0, Some((0, Side::First))));
        // From 0 to 3: the first side is empty; of the second side's two links of room 4 the last walked, nearest the
        // apex, leaves: at a width above 4 and at 4 too, since the arc comes before the second side; at 3 nothing.
        assert_eq!(ratio_test(&mut basis, [0, 3], 9), (4, Some((2, Side::Second))));
        assert_eq!(ratio_test(&mut basis, [0, 3], 4), (4, Some((2, Side::Second))));
        assert_eq!(ratio_test(&mut basis, [0, 3], 3), (3, None));
        // From 3 to 0, the same cycle as for an arc entering from its other bound: now the side of 3 and 2 is first,
        // both of room zero against the flow, and of the two the first walked, nearest the first end, leaves.
        assert_eq!(ratio_test(&mut basis, [3, 0], 9), (0, Some((1, Side::First))));
        // From 3 to 1: the same first side; the second side's link, room 4, does not reach its least.
        assert_eq!(ratio_test(&mut basis, [3, 1], 9), (0, Some((1, Side::First))));

        assert_eq!(check(&basis, &network), Ok(()), "the ratio test changes nothing");
    }

    /// What the ratio test of `basis` answers on the cycle from `first` to `second` with an entering arc of width
    /// `width`. The basis is unchanged.
    fn ratio_test_answer(basis: &mut LoadedBasis<'_, '_>, [first, second]: [usize; 2], width: u64) -> RatioTest {
        let mut walk = cycle_walk(&mut basis.basis.tree, first, second);

        basis.basis.links.least_room(&mut walk, width)
    }

    #[test]
    fn test_the_ratio_test_names_a_side_only_where_its_walk_kept_a_node_by_hand() {
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);

        // A loop closes a cycle of no link, so neither side keeps a node: none is named, at the widest width too,
        // where the second side's least, `u64::MAX` for want of a step, ties with the width.
        assert_eq!(ratio_test_answer(&mut basis, [2, 2], u64::MAX), RatioTest { step: u64::MAX, leaving: None });
        assert_eq!(ratio_test_answer(&mut basis, [2, 2], 7), RatioTest { step: 7, leaving: None });
        // From 0 to 2 each side keeps its root link, of room 4: the second side's is named, at the widest width too.
        let second = RatioTest { step: 4, leaving: Some(Side::Second) };
        assert_eq!(ratio_test_answer(&mut basis, [0, 2], u64::MAX), second);
        // From 1 to 2 the first side keeps node 1's root link, of room 0.
        assert_eq!(ratio_test_answer(&mut basis, [1, 2], 5), RatioTest { step: 0, leaving: Some(Side::First) });
    }

    #[test]
    fn test_the_entering_arc_is_read_from_its_bound_by_hand() {
        // The depot after arc 0 filled: it stands at its capacity 3, of reduced cost `5 + 0 − 11 = −6`; arc 1 at its
        // lower bound, `1 + 0 − 0 = 1`. After arc 2 enters the tree it is no longer priced.
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        basis.pivot(Slot::from(0_usize));
        let read = |basis: &Basis<'_>, slot: usize| {
            let slot = basis.arcs.slot(Slot::from(slot)).expect("the slot should be below the arc count");
            basis.rows().entering(slot, &basis.links)
        };

        let filled = EnteringArc {
            slot: arc_slot(&basis, 0),
            bound: Bound::Upper,
            first: 2,
            second: 0,
            width: 3,
            reduced_cost: -6,
        };
        let empty = EnteringArc {
            slot: arc_slot(&basis, 1),
            bound: Bound::Lower,
            first: 0,
            second: 1,
            width: 4,
            reduced_cost: 1,
        };
        assert_eq!((read(&basis, 0), read(&basis, 1)), (Some(filled), Some(empty)));

        basis.pivot(Slot::from(2_usize));
        assert_eq!(read(&basis, 2), None);
        assert_eq!(basis.arcs.slot(Slot::from(3_usize)), None, "slot 3 is past the arcs");
    }

    #[test]
    fn test_an_entering_arcs_flow_after_a_step_by_hand() {
        let network = depot();
        let mut room = BasisRoom::new();
        let basis = checked_star(&mut room, &network);
        let slot = arc_slot(&basis, 0);
        let arc = EnteringArc { slot, bound: Bound::Lower, first: 0, second: 1, width: 5, reduced_cost: -1 };

        assert_eq!((arc.flow_after(2), arc.flow_after(5)), (2, 5));
        let from_upper = EnteringArc { bound: Bound::Upper, ..arc };
        assert_eq!((from_upper.flow_after(2), from_upper.flow_after(5)), (3, 0));
    }

    #[test]
    fn test_room_of_and_send_read_and_move_one_link_by_hand() {
        // In the depot's star node 0 hangs up from the root by its root link 3, carrying 4 of `u64::MAX`.
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        let cycle = cycle_walk(&mut basis.basis.tree, 0, 2).into_cycle(None);
        let step = cycle.first_side().next().expect("node 0 should lie below the root");

        assert_eq!(basis.basis.links.room_of(step.link, step.direction, DOWN), 4, "against the link: its flow");
        assert_eq!(basis.basis.links.room_of(step.link, step.direction, UP), u64::MAX - 4, "along it: what is left");
        basis.basis.links.send(step.link, step.direction, DOWN, 3);
        assert_eq!(basis.basis.links.flows[3], 1, "against the link its flow falls");
        basis.basis.links.send(step.link, step.direction, UP, 2);
        assert_eq!(basis.links.flows[3], 3, "along it its flow rises");
    }

    #[test]
    fn test_send_around_and_flip_move_an_arc_between_its_bounds_by_hand() {
        // The depot's star. Arc 0 from its lower bound: first end 0, whose root link the flow runs down against, and
        // second end 2, whose root link it runs up against.
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        let arc = basis.rows().entering(arc_slot(&basis, 0), &basis.links).expect("arc 0 should be priced");
        {
            let cycle = cycle_walk(&mut basis.basis.tree, arc.first, arc.second).into_cycle(None);
            basis.basis.links.send_around(&cycle, &arc, 0);
            assert_eq!(basis.basis.links.flows, [0, 0, 0, 4, 0, 4], "a step of zero changes nothing");
            basis.basis.links.send_around(&cycle, &arc, 3);
            assert_eq!(basis.links.flows, [3, 0, 0, 1, 0, 1]);
        }
        basis.basis.arcs.flip(&arc);
        assert_eq!(basis.arcs.states, [UPPER, LOWER, LOWER]);

        // From its capacity the arc sends from its head, node 2, along both root links: each rises by the step.
        let back = basis.rows().entering(arc_slot(&basis, 0), &basis.links).expect("arc 0 should be priced");
        assert_eq!((back.bound, back.first, back.second), (Bound::Upper, 2, 0));
        {
            let cycle = cycle_walk(&mut basis.basis.tree, back.first, back.second).into_cycle(None);
            basis.basis.links.send_around(&cycle, &back, 1);
            assert_eq!(basis.links.flows, [2, 0, 0, 2, 0, 2]);
        }
        basis.basis.arcs.flip(&back);
        assert_eq!(basis.arcs.states, [LOWER, LOWER, LOWER]);
    }

    /// Sends around the cycle the arc at `slot` of `basis` closes the step the ratio test allows, and exchanges the
    /// states of the arc and the link that leaves; answers that link. The tree is not moved.
    ///
    /// # Panics
    /// Panics if the arc is not priced or no link leaves.
    fn exchanged_states(basis: &mut LoadedBasis<'_, '_>, slot: usize) -> usize {
        let arc = basis.rows().entering(arc_slot(basis, slot), &basis.links).expect("the arc should be priced");
        let mut walk = cycle_walk(&mut basis.basis.tree, arc.first, arc.second);
        let RatioTest { step, leaving } = basis.basis.links.least_room(&mut walk, arc.width);
        let cycle = walk.into_cycle(leaving);
        basis.basis.links.send_around(&cycle, &arc, step);
        let exchange = Exchange::of(cycle).expect("a link should leave");
        basis.basis.arcs.exchange_states(&arc, exchange.leaving_link(), &basis.basis.links);

        usize::from(exchange.leaving_link())
    }

    #[test]
    fn test_exchange_states_puts_the_leaving_link_at_its_bound_by_hand() {
        // The depot's star, arc 2: node 1's root link leaves; a root link keeps no state.
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        assert_eq!(exchanged_states(&mut basis, 2), 4);
        assert_eq!(basis.arcs.states, [LOWER, LOWER, IN_TREE]);

        // The depot after its first three pivots, arc 0 from its capacity: arc 2 leaves carrying its width 2.
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &network);
        for slot in [0_usize, 2, 1] {
            basis.pivot(Slot::from(slot));
        }
        assert_eq!(exchanged_states(&mut basis, 0), 2);
        assert_eq!(basis.arcs.states, [IN_TREE, IN_TREE, UPPER]);

        // Two parallel arcs after the first entered, the second entering: the first leaves carrying nothing.
        let parallel = network_of_two_parallel_arcs();
        let mut room = BasisRoom::new();
        let mut basis = checked_star(&mut room, &parallel);
        basis.pivot(Slot::from(0_usize));
        assert_eq!(exchanged_states(&mut basis, 1), 0);
        assert_eq!(basis.arcs.states, [LOWER, IN_TREE]);
    }

    /// Two parallel arcs from node 0, supplying one unit, to node 1: arc 0 of cost 3, arc 1 of cost 1, capacity 1.
    fn network_of_two_parallel_arcs() -> MincostNetwork {
        MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 1, 3), (0, 1, 0, 1, 1)])
    }

    /// Five arcs on two nodes, dealt with the stride three to the slots 0, 3, 1, 4, 2: arc 2 a loop of no width and
    /// arc 3 of lower bound equal to its capacity. Node 0 supplies 3; net of the lower bounds 2, node 1 −2.
    fn five_arcs() -> MincostNetwork {
        MincostNetwork::from_arcs(
            &[3, -3],
            &[(0, 1, 0, 2, 1), (1, 0, 1, 3, -2), (0, 0, 0, 0, 5), (0, 1, 2, 2, 4), (1, 1, 0, 1, -1)],
        )
    }

    /// The ends `tail` and `head`, for a test to write out.
    fn ends(tail: u32, head: u32) -> Ends {
        Ends { tail, head }
    }

    #[test]
    fn test_place_links_then_place_arcs_write_the_star_by_hand() {
        // A room that held the depot first, so that every column is written over.
        let mut room = BasisRoom::new();
        let _ = loaded(&mut room, &problem_of(&depot()));
        let network = five_arcs();
        let problem = problem_of(&network).checked().expect("the five arcs should be admitted");
        let mut net_supplies = NetSupplies::new();
        let start = net_supplies.gather(problem).expect("the net supplies should be in range");

        // The arcs' part of the links carries nothing at capacity zero; node 0's root link carries 2 up, node 1's 2
        // down.
        room.columns.place_links(&start);
        assert_eq!(room.columns.capacities, [0, 0, 0, 0, 0, u64::MAX, u64::MAX]);
        assert_eq!(room.columns.flows, [0, 0, 0, 0, 0, 2, 2]);
        assert_eq!(room.columns.states, [LOWER; 5], "every arc of the star at its lower bound");

        // By slot: arcs 0, 2, 4, 1, 3; arcs 2 and 3 have no width and are not priced.
        room.columns.place_arcs(start.problem());
        assert_eq!(room.columns.ends, [ends(0, 1), ends(0, 0), ends(1, 1), ends(1, 0), ends(0, 1)]);
        assert_eq!(room.columns.costs, [1, 5, -1, -2, 4]);
        assert_eq!(room.columns.states, [LOWER, IN_TREE, LOWER, LOWER, IN_TREE]);
        assert_eq!(room.columns.capacities, [2, 0, 1, 2, 0, u64::MAX, u64::MAX]);
        assert_eq!(room.columns.flows, [0, 0, 0, 0, 0, 2, 2], "the arcs carry nothing, the root links what they did");
    }

    #[test]
    fn test_scatter_without_lower_bounds_takes_whole_capacities_by_hand() {
        let network = five_arcs();
        let problem = Problem { lower_bounds: None, ..problem_of(&network) };
        let checked = problem.checked().expect("the five arcs should be admitted");
        let mut room = RoomColumns::new();
        room.capacities = vec![9; 7];
        room.flows = vec![9; 7];
        room.states = vec![LOWER; 5];
        let CheckedArcs::WithoutLowerBounds(arcs) = checked.arcs() else { panic!("no lower bounds are given") };

        room.scatter(arcs, 5, 2);
        assert_eq!(room.capacities, [2, 0, 1, 3, 2, 9, 9], "the widths of the arcs' slots, the rest as it was");
        assert_eq!(room.states, [LOWER, IN_TREE, LOWER, LOWER, LOWER]);
        assert_eq!(room.flows, [0, 0, 0, 0, 0, 9, 9], "the arcs carry nothing, the rest as it was");
    }

    #[test]
    fn test_scatter_restates_each_arc_from_the_state_its_slot_held_by_hand() {
        // By slot: arcs 0, 2, 4, 1, 3 of the widths 2, 0, 1, 3, 2 without lower bounds. Slots 0 and 2 held arcs at
        // their capacities, which carry their new widths; slot 1 held one at its capacity whose arc has no width now;
        // slot 3 a tree arc and slot 4 one at its lower bound, which carry nothing.
        let network = five_arcs();
        let problem = Problem { lower_bounds: None, ..problem_of(&network) };
        let checked = problem.checked().expect("the five arcs should be admitted");
        let mut room = RoomColumns::new();
        room.capacities = vec![9; 7];
        room.flows = vec![9; 7];
        room.states = vec![UPPER, UPPER, UPPER, IN_TREE, LOWER];
        let CheckedArcs::WithoutLowerBounds(arcs) = checked.arcs() else { panic!("no lower bounds are given") };

        room.scatter(arcs, 5, 2);
        assert_eq!(room.capacities, [2, 0, 1, 3, 2, 9, 9]);
        assert_eq!(room.flows, [2, 0, 1, 0, 0, 9, 9]);
        assert_eq!(room.states, [UPPER, IN_TREE, UPPER, LOWER, LOWER]);
        assert_eq!(room.ends, [ends(0, 1), ends(0, 0), ends(1, 1), ends(1, 0), ends(0, 1)]);
        assert_eq!(room.costs, [1, 5, -1, -2, 4]);
    }

    #[test]
    #[should_panic(expected = "range end index 5 out of range for slice of length 0")]
    fn test_place_arcs_refuses_a_room_whose_links_are_not_placed() {
        let network = five_arcs();
        let problem = problem_of(&network).checked().expect("the five arcs should be admitted");

        RoomColumns::new().place_arcs(problem);
    }

    #[test]
    fn test_the_write_out_adds_the_lower_bounds_back_in_arc_order_by_hand() {
        // The five arcs carrying 1, 0, 1, 2, 0 at the slots 0 to 4: arcs 0, 2, 4, 1, 3. With their lower bounds the
        // arcs carry 1, 3, 0, 2, 1 at the costs 1, −2, 5, 4, −1: `1 − 6 + 0 + 8 − 1 = 2`.
        let network = five_arcs();
        let mut room = BasisRoom::new();
        let basis = checked_star(&mut room, &network);
        basis.basis.links.flows[..5].copy_from_slice(&[1, 0, 1, 2, 0]);

        let CheckedArcs::WithLowerBounds(arcs) = basis.start.problem().arcs() else {
            panic!("the lower bounds are given")
        };
        let whole: Vec<u64> = basis.whole_flows(arcs).map(|(_, flow)| flow).collect();
        assert_eq!(whole, [1, 3, 0, 2, 1]);
        assert_eq!(basis.arc_cost(), 2);
        assert_eq!(written(&basis), (vec![1, 3, 0, 2, 1], 2));
    }

    #[test]
    #[should_panic(expected = "called `Basis::violates` with Slot 3, which should be below the arc count 3")]
    fn test_violates_refuses_a_slot_past_the_arcs() {
        let network = depot();
        let mut room = BasisRoom::new();
        let _ = loaded(&mut room, &problem_of(&network)).violates(Slot::from(3_usize));
    }

    #[test]
    #[should_panic(expected = "called `Basis::pivot` with Slot 7, which should be below the arc count 3")]
    fn test_pivot_refuses_a_slot_past_the_arcs() {
        let network = depot();
        let mut room = BasisRoom::new();
        loaded(&mut room, &problem_of(&network)).pivot(Slot::from(7_usize));
    }

    #[test]
    #[should_panic(expected = "called `Basis::pivot` with Slot 2, whose arc should be priced: out of the tree and of \
                    positive width")]
    fn test_pivot_refuses_an_arc_in_the_tree() {
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = loaded(&mut room, &problem_of(&network));
        basis.pivot(Slot::from(2_usize));
        basis.pivot(Slot::from(2_usize));
    }

    #[test]
    #[should_panic(expected = "called `Basis::pivot` with Slot 0, whose arc should be priced: out of the tree and of \
                    positive width")]
    fn test_pivot_refuses_an_arc_of_no_width() {
        let network = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 0, -5), (0, 1, 0, 1, 2)]);
        let mut room = BasisRoom::new();
        loaded(&mut room, &problem_of(&network)).pivot(Slot::from(0_usize));
    }

    #[test]
    #[should_panic(expected = "called `LoadedBasis::write_flows` with 2 flows, which should be the arc count 3")]
    fn test_write_flows_refuses_a_buffer_of_another_length() {
        let network = depot();
        let mut room = BasisRoom::new();
        let _ = loaded(&mut room, &problem_of(&network)).write_flows(&mut [0, 0]);
    }

    #[test]
    #[should_panic(
        expected = "called `PricedRows::violations` with the slots 2..4, which should lie within the 3 rows"
    )]
    fn test_violations_refuse_slots_past_the_rows() {
        let network = depot();
        let mut room = BasisRoom::new();
        let _ = loaded(&mut room, &problem_of(&network)).rows().violations(2..4);
    }

    #[test]
    #[should_panic(
        expected = "called `PricedRows::violations` with the slots 2..1, which should lie within the 3 rows"
    )]
    fn test_violations_refuse_slots_that_run_backward() {
        let network = depot();
        let mut room = BasisRoom::new();
        #[expect(clippy::reversed_empty_ranges, reason = "the range that runs backward is the case under test")]
        let backward = 2..1;
        let _ = loaded(&mut room, &problem_of(&network)).rows().violations(backward);
    }

    #[test]
    fn test_a_room_loaded_with_a_larger_problem_then_a_smaller_one_answers_as_a_fresh_room() {
        let (larger, smaller) = (
            MincostNetwork::from_arcs(
                &[3, 0, 0, -3],
                &[(0, 1, 0, 2, 4), (1, 3, 1, 3, -2), (0, 2, 0, 3, 1), (2, 3, 0, 9, 0)],
            ),
            depot(),
        );
        let pivots_of = |room: &mut BasisRoom, network: &MincostNetwork| {
            let mut rule = EnteringRule::FirstEligible;
            let mut run = CheckedRun::start(room, network, &problem_of(network)).expect("the star should hold");

            // One snapshot after the load and one a pivot: the run fails past its bound, so they stay within it.
            let mut snapshots = vec![snapshot(&run.basis)];
            while run.pivot_next(|rows| rule.next(rows)).expect("every pivot should keep the invariant").is_some() {
                snapshots.push(snapshot(&run.basis));
            }

            (snapshots, run.run_to_end(|rows| rule.next(rows)).expect("a run at its end should end"))
        };

        let fresh = pivots_of(&mut BasisRoom::new(), &smaller);
        let mut room = BasisRoom::new();
        let _ = pivots_of(&mut room, &larger);
        let held = (room.columns.flows.capacity(), room.columns.ends.capacity());
        assert_eq!(pivots_of(&mut room, &smaller), fresh);
        assert_eq!(
            (room.columns.flows.capacity(), room.columns.ends.capacity()),
            held,
            "the smaller problem fits the room held"
        );

        let panicked = catch_unwind(AssertUnwindSafe(|| {
            loaded(&mut room, &problem_of(&larger)).pivot(Slot::from(4_usize));
        }));
        assert!(panicked.is_err(), "slot 4 is past the larger problem's four arcs");
        assert_eq!(pivots_of(&mut room, &smaller), fresh, "a pivot that panicked leaves only room behind");
    }

    #[test]
    fn test_try_reserve_makes_room_for_a_load() {
        let mut room = BasisRoom::new();
        assert_eq!((room.columns.ends.capacity(), room.columns.flows.capacity()), (0, 0), "a new room holds nothing");
        room.try_reserve(3, 3).expect("room for three nodes and three arcs should be granted");

        let capacities = |room: &BasisRoom| {
            [room.columns.ends.capacity(), room.columns.costs.capacity(), room.columns.states.capacity()]
                .into_iter()
                .chain([room.columns.capacities.capacity() - 3, room.columns.flows.capacity() - 3])
                .min()
        };
        assert!(capacities(&room) >= Some(3));
        let held = (room.columns.ends.as_ptr(), room.columns.flows.as_ptr(), room.columns.flows.capacity());

        let network = depot();
        let _ = loaded(&mut room, &problem_of(&network));
        room.try_reserve(1, 1).expect("a smaller reservation should be granted");
        assert_eq!(
            (room.columns.ends.as_ptr(), room.columns.flows.as_ptr(), room.columns.flows.capacity()),
            held,
            "the load reallocated"
        );

        assert!(room.try_reserve(usize::MAX, 0).is_err());
    }

    #[test]
    fn test_the_pivot_bound_by_hand() {
        // The depot's star: node 2's root link, which points down, carries `T = 4` at `M = 11`, and the arcs cost at
        // least `−(5·3 + 1·4 + 1·2) = −21`, so 65 pivots that send flow; six links, three nodes, `choose(6, 3) = 20`
        // trees: `66 · 21`.
        assert_eq!(binomial(6, 3), 20);
        assert_eq!((binomial(4, 0), binomial(4, 4), binomial(0, 0)), (1, 1, 1));
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = loaded(&mut room, &problem_of(&network));
        assert_eq!(pivot_bound(&basis, &network), 66 * 21);

        // At the depot's optimum the flow costs 14: `14 + 21 = 35` pivots that send flow at most.
        for slot in [0_usize, 2, 1, 0] {
            basis.pivot(Slot::from(slot));
        }
        assert_eq!(pivot_bound(&basis, &network), 36 * 21);
    }

    #[test]
    fn test_the_check_names_a_broken_basis() {
        // A test of the invariant check itself: a basis whose flow or potential is changed by hand fails it.
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = loaded(&mut room, &problem_of(&network));
        basis.pivot(Slot::from(0_usize));
        assert_eq!(check(&basis, &network), Ok(()));

        basis.basis.links.flows[3] += 1;
        assert!(check(&basis, &network).is_err_and(|breach| breach.contains("excess")));
        basis.basis.links.flows[3] -= 1;

        basis.basis.links.flows[4] = 1;
        basis.basis.links.flows[1] = 1;
        assert!(check(&basis, &network).is_err(), "a root link out of the tree carries flow");
        (basis.basis.links.flows[4], basis.basis.links.flows[1]) = (0, 0);

        basis.basis.arcs.states[1] = UPPER;
        assert!(check(&basis, &network).is_err(), "a state disagrees with its flow");
        basis.basis.arcs.states[1] = LOWER;
        assert_eq!(check(&basis, &network), Ok(()));
    }

    /// Runs `network` under the first eligible, the most violating and the seeded random entering rule, each to its
    /// end with the invariant checked after every pivot, and checks each end against the brute force.
    fn check_every_rule(network: &MincostNetwork, seed: u64) -> Result<(), TestCaseError> {
        let rules = [
            EnteringRule::FirstEligible,
            EnteringRule::MostViolating,
            EnteringRule::RandomEligible(Xoshiro256PlusPlus::seed_from_u64(seed)),
        ];
        for rule in rules {
            let name = format!("{rule:?}");
            let end = run_checked(network, rule).map_err(|breach| TestCaseError::fail(format!("{name}: {breach}")))?;
            check_against_brute_force(network, end.answer.as_ref())
                .map_err(|breach| TestCaseError::fail(format!("{name}: {breach}")))?;
        }

        Ok(())
    }

    /// What a pivot did, as the basis read out before and after it shows. Plain data.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum PivotKind {
        /// No link left: the entering arc moved to its other bound.
        Flip,
        /// A link left and no flow moved.
        Degenerate,
        /// Flow moved and a root link left.
        RootLinkLeaves,
        /// Flow moved and an arc left at its lower bound.
        ArcLeavesAtLower,
        /// Flow moved and an arc left at its capacity.
        ArcLeavesAtUpper,
    }

    impl PivotKind {
        /// What the pivot from `before` to `after` did.
        ///
        /// # Panics
        /// Panics if the tree changed but no link of `before`'s tree is missing from `after`'s.
        fn of(before: &Snapshot, after: &Snapshot) -> Self {
            if before.hangings == after.hangings {
                return Self::Flip;
            }
            if before.flows == after.flows {
                return Self::Degenerate;
            }

            match after.states.get(leaving_link(before, after).expect("an exchange should take a link out")) {
                None => Self::RootLinkLeaves,
                Some(&ArcState::AtLower) => Self::ArcLeavesAtLower,
                Some(_) => Self::ArcLeavesAtUpper,
            }
        }
    }

    /// The link of `before`'s tree that `after`'s tree no longer holds; `None` where the trees hold the same links.
    fn leaving_link(before: &Snapshot, after: &Snapshot) -> Option<usize> {
        let links = |snapshot: &Snapshot| snapshot.hangings.iter().map(|hanging| hanging.link).collect::<Vec<_>>();
        let (old, new) = (links(before), links(after));

        old.into_iter().find(|link| !new.contains(link))
    }

    /// `node` and its ancestors in the tree `hangings` describe, nearest first, the root, the node count, last.
    ///
    /// # Panics
    /// Panics if the parents lead `node` around a cycle that misses the root.
    fn path_to_root(hangings: &[PlainHanging], node: usize) -> Vec<usize> {
        let root = hangings.len();
        let mut path = vec![node];
        while let Some(&at) = path.last().filter(|&&at| at != root) {
            assert!(path.len() <= root, "the parents should lead {node} to the root within {root} steps");
            path.push(hangings[at].parent);
        }

        path
    }

    /// The cycle the arc at a slot closed with the tree of a basis read out before its pivot. Plain data.
    #[derive(Debug)]
    struct CycleShape {
        /// The bound the arc entered from.
        bound: Bound,
        /// The nodes strictly below the apex on the first side, then on the second, each nearest its end first.
        sides: [Vec<usize>; 2],
        /// Whether the apex is the root.
        through_root: bool,
    }

    impl CycleShape {
        /// The cycle the arc at `slot` of `network`, which `model` extends, closes with the tree of `before`.
        fn of(network: &MincostNetwork, model: &Extended, before: &Snapshot, slot: Slot) -> Self {
            let arc = model.arc_of_slot[usize::from(slot)];
            let (tail, head) = (network.tails[arc], network.heads[arc]);
            let (bound, first, second) = match before.states[usize::from(slot)] {
                ArcState::AtUpper => (Bound::Upper, head, tail),
                _ => (Bound::Lower, tail, head),
            };

            let (first, second) = (path_to_root(&before.hangings, first), path_to_root(&before.hangings, second));
            let apex = *first.iter().find(|node| second.contains(node)).expect("the root should be above both ends");
            let below = |path: &[usize]| path.iter().copied().take_while(|&node| node != apex).collect::<Vec<_>>();

            Self { bound, sides: [below(&first), below(&second)], through_root: apex == before.hangings.len() }
        }

        /// The side of the node that hangs by `link` in the tree before the pivot, where it lies on the cycle.
        fn side_of(&self, before: &Snapshot, link: usize) -> Option<Side> {
            let on = |side: &[usize]| side.iter().any(|&node| before.hangings[node].link == link);

            match (on(&self.sides[0]), on(&self.sides[1])) {
                (true, _) => Some(Side::First),
                (_, true) => Some(Side::Second),
                _ => None,
            }
        }

        /// Whether the cycle runs through the root by two root links, of a network of `arc_count` arcs, that both
        /// point down.
        fn through_two_down_root_links(&self, before: &Snapshot, arc_count: usize) -> bool {
            let top_points_down = |side: &[usize]| {
                side.last().is_some_and(|&top| {
                    before.hangings[top].link >= arc_count && before.hangings[top].direction == DOWN
                })
            };

            self.through_root && top_points_down(&self.sides[0]) && top_points_down(&self.sides[1])
        }
    }

    #[test]
    fn test_the_tiny_runs_reach_every_kind_of_pivot_and_of_end() {
        // The cases the tiny-network property tests draw, run under the first eligible rule: every kind of pivot, an
        // arc entering from its capacity, a leaving link on either side, a cycle through two root links that point
        // down, runs that route every supply and runs that do not, and zero-width arcs and loops among the networks.
        const CASES: usize = 256;
        let mut runner = TestRunner::deterministic();
        let strategy = tiny_networks(TinyCosts::ZeroOrUniform(-9..=9), TinyBounds::Ordered);
        let (mut kinds, mut ends) = (BTreeSet::new(), BTreeSet::new());
        let (mut from_capacity, mut first_side_leaves, mut second_side_leaves, mut two_down) = (0, 0, 0, 0);
        let (mut zero_widths, mut loops) = (0, 0);

        for _ in 0..CASES {
            let network = strategy.new_tree(&mut runner).expect("a network should be drawn").current();
            zero_widths += network.capacities.iter().zip(&network.lower_bounds).filter(|(cap, low)| cap == low).count();
            loops += network.tails.iter().zip(&network.heads).filter(|(tail, head)| tail == head).count();
            let model = Extended::of(&network);

            let mut room = BasisRoom::new();
            let problem = problem_of(&network);
            let mut rule = EnteringRule::FirstEligible;
            let mut run = CheckedRun::start(&mut room, &network, &problem).expect("the star should hold");
            let mut before = snapshot(&run.basis);
            while let Some(slot) =
                run.pivot_next(|rows| rule.next(rows)).expect("every pivot should keep the invariant")
            {
                let after = snapshot(&run.basis);
                let cycle = CycleShape::of(&network, &model, &before, slot);
                kinds.insert(PivotKind::of(&before, &after));
                from_capacity += usize::from(cycle.bound == Bound::Upper);
                two_down += usize::from(cycle.through_two_down_root_links(&before, network.arc_count()));
                match leaving_link(&before, &after).and_then(|link| cycle.side_of(&before, link)) {
                    Some(Side::First) => first_side_leaves += 1,
                    Some(Side::Second) => second_side_leaves += 1,
                    None => {}
                }
                before = after;
            }
            ends.insert(run.basis.unrouted() == 0);
        }

        let every_kind = [
            PivotKind::Flip,
            PivotKind::Degenerate,
            PivotKind::RootLinkLeaves,
            PivotKind::ArcLeavesAtLower,
            PivotKind::ArcLeavesAtUpper,
        ];
        assert_eq!(kinds, BTreeSet::from(every_kind));
        assert_eq!(ends, BTreeSet::from([false, true]), "runs that route every supply and runs that do not");
        let reached = (from_capacity, first_side_leaves, second_side_leaves, two_down);
        assert!(reached.0 > 0 && reached.1 > 0 && reached.2 > 0 && reached.3 > 0, "{reached:?}");
        assert!(zero_widths > 0 && loops > 0, "{zero_widths} arcs of no width and {loops} loops");
    }

    /// The depot solved to its optimum in `room` by the pivots of its hand-worked run, the basis kept: arcs 0 and 1 in
    /// the tree, carrying 2 each, node 0 up from the root at zero flow, node 1 below it by arc 1 and node 2 by arc 0,
    /// both running down to them; arc 2 out of it at its capacity 2.
    fn kept_depot(room: &mut KeptBasisRoom) {
        let network = depot();
        let mut basis = checked_star(&mut room.room, &network);
        for slot in [0_usize, 2, 1, 0] {
            basis.pivot(Slot::from(slot));
        }

        let _ = basis.keep();
        // As a network's keep leaves it.
        room.kept = Kept::Basis;
    }

    /// The basis `room` lends for `network` from what it holds, its whole invariant checked, read out, with which
    /// start it is.
    ///
    /// # Panics
    /// Panics if the invariant fails after the resume.
    fn reflowed(room: &mut KeptBasisRoom, network: &MincostNetwork) -> (Snapshot, Begun) {
        let (basis, begun) = resumed_in(room, &problem_of(network));
        if let Err(breach) = check(&basis, network) {
            panic!("the invariant should hold after the resume to {begun:?}: {breach}");
        }

        (snapshot(&basis), begun)
    }

    /// The depot's optimum as the re-flow keeps it where nothing moves: node 0 up from the root, nodes 1 and 2 below
    /// it by arcs 1 and 0, running down to them; arc 2 at its capacity.
    fn depot_tree() -> Vec<PlainHanging> {
        vec![hung(3, 3, UP), hung(0, 1, DOWN), hung(0, 0, DOWN)]
    }

    #[test]
    fn test_the_depot_re_flowed_for_less_supply_by_hand() {
        // Node 0 sends 3 units. Arc 2 carries its width 2 out of the tree: the excesses are `3`, `0 − 2` and
        // `−3 + 2`. Node 1 takes 2 down arc 1 and node 2 one down arc 0, both within their widths and above nothing;
        // node 0 is left with `3 − 2 − 1 = 0` on its root link. The potentials are as before: no cost moved. Arc 2
        // at its capacity has the reduced cost `1 + 1 − 5 = −3`, no violation: the optimum, without a pivot.
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        let network = MincostNetwork { supplies: vec![3, 0, -3], ..depot() };

        let expected = Snapshot {
            flows: vec![1, 2, 2, 0, 0, 0],
            potentials: vec![0, 1, 5, 0],
            states: vec![IN_TREE, IN_TREE, UPPER],
            hangings: depot_tree(),
            violations: vec![0, 0, 3],
        };
        assert_eq!(reflowed(&mut room, &network), (expected, Begun::Kept));
    }

    #[test]
    fn test_the_depot_re_flowed_for_a_narrower_arc_out_of_the_tree_by_hand() {
        // Arc 2 of capacity 1 carries its new width at its capacity: the excesses are `4`, `−1` and `−4 + 1`. Node 1
        // takes 1 down arc 1, node 2 takes 3 down arc 0, its whole width, and stays, since it could bring less;
        // node 0 is left with nothing on its root link.
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        let mut network = depot();
        network.capacities[2] = 1;

        let expected = Snapshot {
            flows: vec![3, 1, 1, 0, 0, 0],
            potentials: vec![0, 1, 5, 0],
            states: vec![IN_TREE, IN_TREE, UPPER],
            hangings: depot_tree(),
            violations: vec![0, 0, 3],
        };
        assert_eq!(reflowed(&mut room, &network), (expected, Begun::Kept));
    }

    #[test]
    fn test_the_depot_re_flowed_for_a_dearer_tree_arc_by_hand() {
        // Arc 1 costs 6: the flows and the tree are kept, the root cost is `2 · 6 + 1 = 13`, and node 1 sits at 6 below
        // arc 1 running down to it. Arc 2 at its capacity now has the reduced cost `1 + 6 − 5 = 2`: it violates.
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        let mut network = depot();
        network.costs[1] = 6;

        let expected = Snapshot {
            flows: vec![2, 2, 2, 0, 0, 0],
            potentials: vec![0, 6, 5, 0],
            states: vec![IN_TREE, IN_TREE, UPPER],
            hangings: depot_tree(),
            violations: vec![0, 0, -2],
        };
        assert_eq!(reflowed(&mut room, &network), (expected, Begun::Kept));
    }

    #[test]
    fn test_the_depot_re_flowed_for_a_tree_arc_of_no_width_by_hand() {
        // Arc 1 of capacity 0: the excesses are `4`, `−2` and `−2`. Node 1 can take nothing down arc 1, which leaves
        // unpriced, and hangs from the root by its root link, link 4, which brings it 2 down, at the potential
        // `M = 11`; node 2 takes 2 down arc 0; node 0 sends 2 up its root link. Arc 2 at its capacity then has the
        // reduced cost `1 + 11 − 5 = 7`: it violates.
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        let mut network = depot();
        network.capacities[1] = 0;
        let (lent, begun) = resumed_in(&mut room, &problem_of(&network));
        let mut basis = lent.into_basis();
        assert_eq!(check(&basis, &network), Ok(()));
        let expected = Snapshot {
            flows: vec![2, 0, 2, 2, 2, 0],
            potentials: vec![0, 11, 5, 0],
            states: vec![IN_TREE, IN_TREE, UPPER],
            hangings: vec![hung(3, 3, UP), hung(3, 4, DOWN), hung(0, 0, DOWN)],
            violations: vec![0, 0, -7],
        };
        assert_eq!((snapshot(&basis), begun), (expected, Begun::Kept));

        // Arc 2 from its capacity, sending from node 2: arc 0, along the cycle's flow down to node 2, has room 1, the
        // least, below arc 2's width 2. One unit moves, arc 0 leaves at its capacity, and node 2 hangs from node 1 by
        // arc 2, at `11 + 1`. Two root links still carry flow: arc 0 alone cannot carry the 4 units.
        let ended = Snapshot {
            flows: vec![3, 0, 1, 1, 1, 0],
            potentials: vec![0, 11, 12, 0],
            states: vec![UPPER, IN_TREE, IN_TREE],
            hangings: vec![hung(3, 3, UP), hung(3, 4, DOWN), hung(1, 2, DOWN)],
            violations: vec![7, 0, 0],
        };
        assert_eq!(pivoted(&mut basis, &network, 2), ended);
        assert!(basis.unrouted() > 0);
        assert_eq!(brute_force_optimum(&network), None);
    }

    /// The start `room` decides on for `network`, begun at `root_cost`, with which start it is, read out once its whole
    /// invariant at that root cost has been checked.
    ///
    /// # Panics
    /// Panics if the problem is refused, or the invariant fails.
    fn begun_at(room: &mut KeptBasisRoom, network: &MincostNetwork, root_cost: i64) -> (Snapshot, Begun) {
        let problem = problem_of(network).checked().expect("the test's problem should be admitted");
        let resumption = room.resume(problem).expect("the test's net supplies should be in range");
        let begun = if resumption.kept() { Begun::Kept } else { Begun::Star };

        let basis = resumption.begin(root_cost);
        if let Err(breach) = check_at(&basis, network, root_cost) {
            panic!("the invariant should hold at the root cost {root_cost} after the start {begun:?}: {breach}");
        }

        (snapshot(&basis), begun)
    }

    #[test]
    fn test_the_arc_cost_adds_each_arcs_cost_times_its_whole_flow_by_hand() {
        // The depot's star carries nothing on an arc; its optimum 2 on each, at 5, 1 and 1: 14. Re-flowed with arc 1
        // switched off, arcs 0 and 2 carry 2 each, `10 + 2`; node 1's root link, which carries 2 down, is not counted.
        let mut room = KeptBasisRoom::new();
        assert_eq!(loaded(&mut room.room, &problem_of(&depot())).arc_cost(), 0);

        kept_depot(&mut room);
        let mut network = depot();
        network.capacities[1] = 0;
        let (basis, begun) = resumed_in(&mut room, &problem_of(&network));
        assert_eq!((basis.arc_cost(), begun), (12, Begun::Kept));

        // Lower bounds count: the three-node network of lower bounds 2, 0, 5 and 1 at costs 4, −1, 0 and 2 in its star,
        // each arc at its lower bound, `8 + 0 + 0 + 2`.
        let lower = MincostNetwork::from_arcs(
            &[3, 0, -3],
            &[(0, 1, 2, 4, 4), (1, 2, 0, 2, -1), (2, 2, 5, 5, 0), (2, 0, 1, 3, 2)],
        );
        assert_eq!(loaded(&mut BasisRoom::new(), &problem_of(&lower)).arc_cost(), 10);
    }

    #[test]
    fn test_the_depot_re_flowed_at_the_widest_root_cost_by_hand() {
        // The tree arc of no width above, re-flowed at the widest root cost of three nodes, `M = 2·B + 1` for
        // `B = ⌊(2^62 − 1) / 3⌋`, in place of the problem's own 11: the flows, the states and the tree are the same,
        // since the pass from the leaves up reads no cost. Node 1, down from the root, sits at `M`, and arc 2 at its
        // capacity has the reduced cost `1 + M − 5`: it violates by `4 − M`.
        let widest = 2 * cost_bound(3) + 1;
        let mut network = depot();
        network.capacities[1] = 0;
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);

        let expected = Snapshot {
            flows: vec![2, 0, 2, 2, 2, 0],
            potentials: vec![0, widest, 5, 0],
            states: vec![IN_TREE, IN_TREE, UPPER],
            hangings: vec![hung(3, 3, UP), hung(3, 4, DOWN), hung(0, 0, DOWN)],
            violations: vec![0, 0, 4 - widest],
        };
        assert_eq!(begun_at(&mut room, &network, widest), (expected, Begun::Kept));

        // A room that holds nothing begins the star, at the root cost it is given: node 2 down at `M`.
        let star = begun_at(&mut KeptBasisRoom::new(), &depot(), widest);
        assert_eq!((star.0.potentials, star.1), (vec![0, 0, widest, 0], Begun::Star));
    }

    #[test]
    #[should_panic(
        expected = "called `Resumption::begin` with the root cost 10, which should lie within 11..=3074457345618258603"
    )]
    fn test_begin_refuses_a_root_cost_below_the_problems() {
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        let _ = begun_at(&mut room, &depot(), 10);
    }

    #[test]
    #[should_panic(
        expected = "called `Resumption::begin` with the root cost 3074457345618258604, which should lie within \
                    11..=3074457345618258603"
    )]
    fn test_begin_refuses_a_root_cost_past_the_widest() {
        let _ = begun_at(&mut KeptBasisRoom::new(), &depot(), 3_074_457_345_618_258_604);
    }

    #[test]
    fn test_a_refused_root_cost_leaves_the_room_holding_its_basis() {
        // The decision writes nothing of the room, and `begin` refuses before it writes: the room still holds the
        // depot's optimum, and begins from it.
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        let refused = catch_unwind(AssertUnwindSafe(|| begun_at(&mut room, &depot(), 10)));
        assert!(refused.is_err(), "a root cost below the problem's should be refused");

        assert_eq!(room.kept(), Some(KeptColumns { arc_flows: &[2, 2, 2], potentials: &[0, 1, 5] }));
        assert_eq!(reflowed(&mut room, &depot()).1, Begun::Kept);
    }

    /// Checks that `room` lends the star of `network` for it, as a fresh room loads it, and holds no basis meanwhile.
    ///
    /// # Panics
    /// Panics if it does not.
    fn assert_resumes_to_the_star(room: &mut KeptBasisRoom, network: &MincostNetwork) {
        let star = snapshot(&checked_star(&mut BasisRoom::new(), network));

        let (basis, begun) = resumed_in(room, &problem_of(network));
        assert_eq!((snapshot(&basis), begun), (star, Begun::Star));
        assert_eq!(*basis.kept, Kept::Nothing);
    }

    #[test]
    fn test_a_room_that_holds_nothing_resumes_to_the_star() {
        assert_resumes_to_the_star(&mut KeptBasisRoom::new(), &depot());

        // A load takes out what the room held.
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        let network = depot();
        let (basis, _) = resumed_in(&mut room, &problem_of(&network));
        assert_eq!(*basis.kept, Kept::Nothing);
        assert_resumes_to_the_star(&mut room, &depot());
    }

    #[test]
    fn test_a_basis_of_other_counts_resumes_to_the_star() {
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        assert_resumes_to_the_star(&mut room, &network_of_two_parallel_arcs());

        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        let arcs = [(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1), (0, 2, 0, 1, 9)];
        let more_arcs = MincostNetwork::from_arcs(&[4, 0, -4], &arcs);
        assert_resumes_to_the_star(&mut room, &more_arcs);
    }

    #[test]
    fn test_a_basis_with_another_end_at_a_slot_resumes_to_the_star() {
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        let turned = MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (2, 1, 0, 2, 1)]);

        assert_resumes_to_the_star(&mut room, &turned);
    }

    #[test]
    fn test_excesses_past_the_limit_resume_to_the_star() {
        // Two arcs of cost −1 between two nodes of no supply. The first enters degenerately, node 0 hanging up from
        // node 1 by it; the second, at `−1 + 0 − 1 = −2`, fills the cycle with one unit, and the first leaves at its
        // capacity.
        let cycle = MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, 1, -1), (1, 0, 0, 1, -1)]);
        let mut room = KeptBasisRoom::new();
        let mut rule = EnteringRule::FirstEligible;
        let end = CheckedRun::start(&mut room.room, &cycle, &problem_of(&cycle))
            .and_then(|run| run.run_to_end(|rows| rule.next(rows)))
            .expect("the cycle should run to its end");
        room.kept = Kept::Basis;
        assert_eq!(end.answer.map(|answer| answer.flows), Some(vec![1, 1]));
        assert_eq!(room.room.columns.states, [UPPER, IN_TREE]);

        // Both arcs as wide as a `u64` holds: the first, at its capacity, carries `u64::MAX` from node 0 to node 1,
        // one past the limit.
        let wide = MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, u64::MAX, -1), (1, 0, 0, u64::MAX, -1)]);
        assert_resumes_to_the_star(&mut room, &wide);

        // One unit narrower and the excesses keep the limit: the kept basis serves.
        let mut room = KeptBasisRoom::new();
        let _ = CheckedRun::start(&mut room.room, &cycle, &problem_of(&cycle))
            .and_then(|run| run.run_to_end(|rows| EnteringRule::FirstEligible.next(rows)));
        room.kept = Kept::Basis;
        let at_limit = MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, u64::MAX - 1, -1), (1, 0, 0, u64::MAX, -1)]);
        assert_eq!(reflowed(&mut room, &at_limit).1, Begun::Kept);
    }

    #[test]
    fn test_keep_puts_the_basis_in_the_room_and_a_copy_keeps_it() {
        let mut room = KeptBasisRoom::new();
        assert_eq!(room.kept, Kept::Nothing);
        kept_depot(&mut room);

        assert_eq!(room.kept, Kept::Basis);
        assert_eq!(room.room.columns.tree.kept().map(|kept| (kept.node_count, kept.link_count)), Some((3, 6)));
        let mut copy = KeptBasisRoom::new();
        let _ = copy.copy_from(&room).map(|basis| basis.keep());
        assert_eq!(copy.kept, room.kept);
        assert_eq!(reflowed(&mut copy, &depot()).1, Begun::Kept);
    }

    #[test]
    fn test_a_run_unwound_after_the_resume_leaves_nothing_kept() {
        // The resume takes the basis out before it writes; the pivot of arc 1, in the tree, panics with the basis lent.
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);

        let network = depot();
        let unwound = catch_unwind(AssertUnwindSafe(|| {
            let (lent, begun) = resumed_in(&mut room, &problem_of(&network));
            assert_eq!(begun, Begun::Kept);
            lent.into_basis().pivot(Slot::from(1_usize));
        }));
        assert!(unwound.is_err(), "a pivot of an arc in the tree should panic");
        assert_eq!(room.kept, Kept::Nothing);
        assert_resumes_to_the_star(&mut room, &depot());
    }

    #[test]
    fn test_kept_reads_the_kept_basis_and_reopen_lends_it_again_by_hand() {
        // The depot's optimum kept: every arc carries 2, the potentials are 0, 1 and 5.
        let network = depot();
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        assert_eq!(room.kept(), Some(KeptColumns { arc_flows: &[2, 2, 2], potentials: &[0, 1, 5] }));

        // Lent again, it is the basis kept, whole; the room holds none while it is lent.
        let reopened = room.reopen().expect("the room should hold the depot's basis");
        assert_eq!(check(&reopened, &network), Ok(()));
        assert_eq!(snapshot(&reopened).flows, [2, 2, 2, 0, 0, 0]);
        assert_eq!((snapshot(&reopened).potentials, reopened.violation(Slot::from(2_usize))), (vec![0, 1, 5, 0], 3));
        let _ = reopened.keep();
        assert_eq!(room.kept(), Some(KeptColumns { arc_flows: &[2, 2, 2], potentials: &[0, 1, 5] }));

        // Lent again and dropped unkept, as an unwind drops it: the room holds nothing, and resumes to the star.
        assert!(room.reopen().is_some());
        assert_eq!((room.kept, room.kept()), (Kept::Nothing, None));
        assert!(room.reopen().is_none(), "a room that holds nothing lends nothing");
        assert_eq!(room.kept, Kept::Nothing);
        assert_resumes_to_the_star(&mut room, &network);
    }

    #[test]
    fn test_a_room_that_holds_no_basis_reads_and_reopens_nothing() {
        let mut room = KeptBasisRoom::new();
        assert_eq!(room.kept(), None);
        assert!(room.reopen().is_none());

        // A star loaded and never kept: the room holds no basis.
        let network = depot();
        let _ = loaded(&mut room.room, &problem_of(&network));
        assert_eq!(room.kept(), None);
        assert!(room.reopen().is_none());

        // A basis kept beside a tree no longer kept, which no call of the crate leaves: the tree's marker decides too.
        kept_depot(&mut room);
        let _ = room.room.columns.tree.resume();
        assert_eq!(room.kept(), None);
        assert!(room.reopen().is_none());
        assert_eq!(room.kept, Kept::Nothing);
    }

    #[test]
    fn test_kept_problem_reads_the_kept_basis_numbers_by_slot_and_nothing_where_none_is_kept_by_hand() {
        // The depot's optimum kept: arcs from 0 to 2, 0 to 1 and 1 to 2, of costs 5, 1, 1 and widths 3, 4, 2.
        let mut room = KeptBasisRoom::new();
        assert_eq!(room.kept_problem(), None);
        let _ = loaded(&mut room.room, &problem_of(&depot()));
        assert_eq!(room.kept_problem(), None, "a star loaded and not kept");

        kept_depot(&mut room);
        let kept = room.kept_problem().expect("the room should hold the depot's basis");
        let ends: Vec<(usize, usize)> = kept.ends.iter().map(|ends| (ends.tail(), ends.head())).collect();
        assert_eq!((ends, kept.costs, kept.widths), (vec![(0, 2), (0, 1), (1, 2)], &[5, 1, 1][..], &[3, 4, 2][..]));
    }

    #[test]
    fn test_a_copy_of_a_kept_basis_is_lent_as_it_was_kept_by_hand() {
        // The depot's optimum kept: every arc carries 2, the potentials are 0, 1 and 5. The copy's room held the star
        // of a problem of other counts, kept.
        let network = depot();
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        let mut copy = KeptBasisRoom::new();
        let other = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 1, 1)]);
        let _ = loaded(&mut copy.room, &problem_of(&other)).keep();

        // Lent, it is the basis kept, whole, at the depot's root cost; dropped unkept, the copy holds nothing.
        let lent = copy.copy_from(&room).expect("the source should hold a basis");
        assert_eq!(check(&lent, &network), Ok(()));
        assert_eq!((snapshot(&lent).flows, snapshot(&lent).potentials), (vec![2, 2, 2, 0, 0, 0], vec![0, 1, 5, 0]));
        assert_eq!(lent.root_cost, room.room.columns.root_cost);
        assert_eq!((copy.kept, copy.kept()), (Kept::Nothing, None));

        // Lent and kept: it reads as the source, and reopens; the source is as it was.
        let _ = copy.copy_from(&room).map(|basis| basis.keep());
        assert_eq!(copy.kept(), Some(KeptColumns { arc_flows: &[2, 2, 2], potentials: &[0, 1, 5] }));
        assert_eq!(copy.reopen().map(|basis| check(&basis, &network)), Some(Ok(())));
        assert_eq!(room.kept(), Some(KeptColumns { arc_flows: &[2, 2, 2], potentials: &[0, 1, 5] }));
    }

    #[test]
    fn test_a_copy_of_a_room_that_holds_no_basis_holds_nothing() {
        // A star loaded and never kept, then a basis kept beside a tree no longer kept: neither is lent, and the copy,
        // which held the depot's basis, holds nothing; where the source holds no basis every column is emptied.
        let mut room = KeptBasisRoom::new();
        let _ = loaded(&mut room.room, &problem_of(&depot()));
        let mut copy = KeptBasisRoom::new();
        kept_depot(&mut copy);

        assert!(copy.copy_from(&room).is_none());
        assert_eq!((copy.kept, copy.kept(), copy.room.columns.tree.kept()), (Kept::Nothing, None, None));
        let lengths = [
            copy.room.columns.ends.len(),
            copy.room.columns.costs.len(),
            copy.room.columns.states.len(),
            copy.room.columns.capacities.len(),
            copy.room.columns.flows.len(),
        ];
        assert_eq!(lengths, [0; 5]);

        kept_depot(&mut room);
        untie_the_kept_tree(&mut room);
        kept_depot(&mut copy);
        assert!(copy.copy_from(&room).is_none());
        assert_eq!((copy.kept, copy.kept(), copy.room.columns.tree.kept()), (Kept::Nothing, None, None));
    }

    #[test]
    fn test_a_copy_into_the_room_reserved_for_its_basis_allocates_nothing() {
        // Every column's room stays as reserved: a column that grew would hold more. The counting allocator of the
        // public tests counts a network's copy into reserved room as well.
        let mut room = KeptBasisRoom::new();
        kept_depot(&mut room);
        let mut copy = KeptBasisRoom::new();
        copy.try_reserve(3, 3).expect("room for three nodes and three arcs should be granted");
        let reserved = copy.capacities();

        let _ = copy.copy_from(&room).map(|basis| basis.keep());
        let _ = copy.copy_from(&room).map(|basis| basis.keep());
        assert_eq!(copy.capacities(), reserved);
        assert!(copy.kept().is_some());
    }

    #[test]
    fn test_violation_answers_each_slot_by_hand() {
        // The depot's star at its root cost 11: arcs 0 and 2 into node 2, down at 11, violate; arc 1 does not.
        let network = depot();
        let mut room = KeptBasisRoom::new();
        let mut basis = loaded(&mut room.room, &problem_of(&network));
        assert_eq!([0_usize, 1, 2].map(|slot| basis.violation(Slot::from(slot))), [5 - 11, 1, 1 - 11]);

        // Arc 0 fills and stands at its capacity, where a reduced cost of `5 − 11` keeps it: `11 − 5`. Arc 2 enters
        // degenerately: in the tree it has no violation.
        basis.pivot(Slot::from(0_usize));
        basis.pivot(Slot::from(2_usize));
        assert_eq!([0_usize, 1, 2].map(|slot| basis.violation(Slot::from(slot))), [11 - 5, 1 - 10, 0]);
        for slot in 0_usize..3 {
            assert_eq!(basis.violates(Slot::from(slot)), basis.violation(Slot::from(slot)) < 0);
        }
    }

    #[test]
    #[should_panic(expected = "called `Basis::violation` with Slot 3, which should be below the arc count 3")]
    fn test_violation_refuses_a_slot_past_the_arcs() {
        let network = depot();
        let mut room = KeptBasisRoom::new();
        let _ = loaded(&mut room.room, &problem_of(&network)).violation(Slot::from(3_usize));
    }

    /// Runs `network` from the star in `room` under the first eligible rule to its end, its invariant checked after
    /// every pivot, and keeps the basis; answers the basis read out at its end.
    ///
    /// # Errors
    /// As [`CheckedRun::pivot_next`].
    fn ended(room: &mut KeptBasisRoom, network: &MincostNetwork) -> Result<Snapshot, String> {
        let mut rule = EnteringRule::FirstEligible;
        let mut run = CheckedRun::start(&mut room.room, network, &problem_of(network))?;
        while run.pivot_next(|rows| rule.next(rows))?.is_some() {}

        let end = snapshot(&run.basis);
        let _ = run.basis.keep();
        // As a network's keep leaves it.
        room.kept = Kept::Basis;
        Ok(end)
    }

    /// Runs `network` from the basis the run of `before` ended on, under the first eligible, the most violating and
    /// the seeded random entering rule, each to its end with the invariant checked after the re-flow and after every
    /// pivot, and checks each end against the brute force.
    fn check_every_rule_from_the_kept_basis(
        before: &MincostNetwork,
        network: &MincostNetwork,
        seed: u64,
    ) -> Result<(), TestCaseError> {
        let rules = [
            EnteringRule::FirstEligible,
            EnteringRule::MostViolating,
            EnteringRule::RandomEligible(Xoshiro256PlusPlus::seed_from_u64(seed)),
        ];
        for mut rule in rules {
            let name = format!("{rule:?}");
            let fail = |breach: String| TestCaseError::fail(format!("{name}: {breach}"));
            let mut room = KeptBasisRoom::new();

            let (run, begun) = CheckedRun::resumed(&mut room, before, network, &problem_of(network)).map_err(fail)?;
            prop_assert_eq!(begun, Begun::Kept, "a pair on the same arcs of tiny numbers");
            let end = run.run_to_end(|rows| rule.next(rows)).map_err(fail)?;
            check_against_brute_force(network, end.answer.as_ref()).map_err(|breach| fail(breach.to_string()))?;
        }

        Ok(())
    }

    /// The pairs of tiny networks on the same arcs the re-flow is tried on: up to seven arcs, the most the brute force
    /// enumerates, costs that tie often, supplies half the time a flow's.
    fn tiny_pairs() -> impl Strategy<Value = (MincostNetwork, MincostNetwork)> {
        TinyMincostNetworks {
            arcs: 0..=7,
            costs: TinyCosts::ZeroOrUniform(-9..=9),
            supplies: TinySupplies::OfAFlowOrRandom,
            bounds: TinyBounds::Ordered,
        }
        .pairs()
    }

    #[test]
    fn test_the_tiny_pairs_reach_every_way_the_re_flow_goes() {
        // The cases the pair property test draws: a tree arc that leaves for its node's root link, one whose parent's
        // arc leaves too, a root link turned the other way, a kept basis that ended infeasible, a re-flow that leaves
        // the optimum as it was and one after which the run pivots, and a run from the kept basis that ends
        // infeasible.
        const CASES: usize = 256;
        let mut runner = TestRunner::deterministic();
        let strategy = tiny_pairs();
        let (mut cuts, mut nested, mut turned, mut idle, mut busy) = (0, 0, 0, 0, 0);
        let (mut kept_infeasible, mut ended_infeasible) = (0, 0);

        for _ in 0..CASES {
            let (before, network) = strategy.new_tree(&mut runner).expect("a pair should be drawn").current();
            let mut room = KeptBasisRoom::new();
            let end = ended(&mut room, &before).expect("the first run should keep the invariant");

            let (mut run, begun) = {
                let (basis, begun) = resumed_in(&mut room, &problem_of(&network));
                let basis = basis.into_basis();
                check(&basis, &network).expect("the invariant should hold after the re-flow");
                let bound = pivot_bound(&basis, &network);
                (CheckedRun { network: &network, basis, pivots: 0, bound }, begun)
            };
            assert_eq!(begun, Begun::Kept);
            let arc_count = network.arc_count();
            let hangings = hangings_of(&run.basis.tree);
            let cut = |node: usize| end.hangings[node].link < arc_count && hangings[node].link >= arc_count;
            for (node, (old, new)) in end.hangings.iter().zip(&hangings).enumerate() {
                cuts += usize::from(cut(node));
                nested += usize::from(cut(node) && old.parent < hangings.len() && cut(old.parent));
                turned += usize::from(old.link >= arc_count && old.link == new.link && old.direction != new.direction);
            }
            kept_infeasible += usize::from(end.flows[arc_count..].iter().any(|&flow| flow > 0));

            let mut rule = EnteringRule::FirstEligible;
            while run.pivot_next(|rows| rule.next(rows)).expect("every pivot should keep the invariant").is_some() {}
            if run.pivots == 0 {
                idle += 1;
            } else {
                busy += 1;
            }
            ended_infeasible += usize::from(run.basis.unrouted() > 0);
        }

        let reached = [cuts, nested, turned, kept_infeasible, idle, busy, ended_infeasible];
        assert!(reached.iter().all(|&count| count > 0), "{reached:?}");
    }

    proptest! {
        #[test]
        fn test_tiny_runs_keep_the_invariant_and_end_optimal(
            network in tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered),
            seed in any::<u64>(),
        ) {
            check_every_rule(&network, seed)?;
        }

        #[test]
        fn test_tiny_runs_with_tied_costs_keep_the_invariant_and_end_optimal(
            network in tiny_networks(TinyCosts::ZeroOrUniform(-9..=9), TinyBounds::Ordered),
            seed in any::<u64>(),
        ) {
            check_every_rule(&network, seed)?;
        }

        #[test]
        fn test_a_run_without_lower_bounds_given_is_the_run_with_zero_lower_bounds_given(
            network in tiny_networks(TinyCosts::ZeroOrUniform(-9..=9), TinyBounds::Ordered),
        ) {
            let network = MincostNetwork { lower_bounds: vec![0; network.arc_count()], ..network };
            let given = run_checked(&network, EnteringRule::FirstEligible);
            prop_assert!(given.is_ok(), "{given:?}");

            let mut room = BasisRoom::new();
            let problem = Problem { lower_bounds: None, ..problem_of(&network) };
            let mut rule = EnteringRule::FirstEligible;
            let not_given = CheckedRun::start(&mut room, &network, &problem)
                .and_then(|run| run.run_to_end(|rows| rule.next(rows)));
            prop_assert_eq!(given, not_given);
        }

        #[test]
        fn test_tiny_pairs_keep_the_invariant_from_the_kept_basis_and_end_optimal(
            (before, network) in tiny_pairs(),
            seed in any::<u64>(),
        ) {
            check_every_rule_from_the_kept_basis(&before, &network, seed)?;
        }

        #[test]
        fn test_the_same_problem_again_starts_from_its_own_end(
            network in tiny_networks(TinyCosts::ZeroOrUniform(-9..=9), TinyBounds::Ordered),
        ) {
            let mut room = KeptBasisRoom::new();
            let end = ended(&mut room, &network).map_err(TestCaseError::fail)?;
            prop_assert!(end.violations.iter().all(|&violation| violation >= 0), "{end:?}");

            let (again, begun) = reflowed(&mut room, &network);
            prop_assert_eq!(begun, Begun::Kept);
            prop_assert_eq!(again, end);
        }

        #[test]
        fn test_costs_alone_changed_keep_the_tree_and_the_flows(
            (network, costs) in tiny_networks(TinyCosts::ZeroOrUniform(-9..=9), TinyBounds::Ordered)
                .prop_flat_map(|network| {
                    let arcs = network.arc_count();
                    (Just(network), proptest::collection::vec(-9_i64..=9, arcs))
                }),
        ) {
            let mut room = KeptBasisRoom::new();
            let end = ended(&mut room, &network).map_err(TestCaseError::fail)?;

            let (again, begun) = reflowed(&mut room, &MincostNetwork { costs, ..network });
            prop_assert_eq!(begun, Begun::Kept);
            prop_assert_eq!((again.flows, again.states, again.hangings), (end.flows, end.states, end.hangings));
        }
    }

    /// The star of `network` at the root cost `root_cost`, loaded into `room`, kept and lent again: a basis a cost can
    /// be set on.
    ///
    /// # Panics
    /// Panics if the problem is refused, or the root cost is outside what the load admits.
    fn reopened_star_at<'r>(room: &'r mut KeptBasisRoom, network: &MincostNetwork, root_cost: i64) -> Basis<'r> {
        let problem = problem_of(network).checked().expect("the test's problem should be admitted");
        let resumption = room.resume(problem).expect("the test's net supplies should be in range");
        assert!(!resumption.kept(), "the room should hold no basis");

        resumption.begin(root_cost).into_basis().basis
    }

    /// Takes the tree out of `room` while the room's own marker still says it holds a basis: a room out of step, which
    /// no call of the crate leaves, in which a read of the kept basis answers and a reopen does not.
    pub(crate) fn untie_the_kept_tree(room: &mut KeptBasisRoom) {
        let _ = room.room.columns.tree.resume();
    }

    /// The depot's basis at its own root cost 11 after the pivots of the arcs at `slots`, lent from `room`.
    fn depot_after<'r>(room: &'r mut KeptBasisRoom, slots: &[usize]) -> Basis<'r> {
        let mut basis = reopened_star_at(room, &depot(), 11);
        for &slot in slots {
            basis.pivot(Slot::from(slot));
        }

        basis
    }

    /// Makes `cost` the cost of the arc at `slot` of `basis` and of the arc dealt there in `network`, then checks the
    /// whole invariant of `basis` as a basis of `network` so changed at `root_cost`, and answers the basis read out.
    ///
    /// # Panics
    /// Panics if the invariant fails after the change.
    fn cost_set(
        basis: &mut Basis<'_>,
        network: &mut MincostNetwork,
        root_cost: i64,
        slot: usize,
        cost: i64,
    ) -> Snapshot {
        let before = snapshot(basis);
        let moved = basis.set_cost(Slot::from(slot), cost);
        let arc = arcs_of_slots(network.arc_count(), network.node_count())[slot];
        network.costs[arc] = cost;
        if let Err(breach) = check_at(basis, network, root_cost) {
            panic!("the invariant should hold after the cost {cost} at slot {slot}: {breach}");
        }
        let after = snapshot(basis);
        if let Err(breach) = moved_checked(&before, &after, moved.as_slice()) {
            panic!("the cost {cost} at slot {slot} should answer the subtree it moved: {breach}");
        }

        after
    }

    #[test]
    fn test_a_cost_set_on_an_arc_out_of_the_tree_moves_no_potential_by_hand() {
        // The depot's star at 11, every arc out of the tree at its lower bound. Arc 1, from node 0 to node 1, both at
        // potential zero, from cost 1 to 4: its violation is its reduced cost, 4.
        let mut network = depot();
        let mut room = KeptBasisRoom::new();
        let mut basis = depot_after(&mut room, &[]);
        let star = snapshot(&basis);
        assert_eq!(star.violations, [5 - 11, 1, 1 - 11]);
        assert_eq!(
            cost_set(&mut basis, &mut network, 11, 1, 4),
            Snapshot { violations: vec![5 - 11, 4, 1 - 11], ..star }
        );

        // The depot's optimum: arc 2, from node 1 at 1 to node 2 at 5, at its capacity, from cost 1 to 3: its
        // violation, the negated reduced cost, from `−(1 + 1 − 5) = 3` to `−(3 + 1 − 5) = 1`.
        let mut network = depot();
        let mut basis = depot_after(&mut room, &[0, 2, 1, 0]);
        let optimal = snapshot(&basis);
        assert_eq!((optimal.states[2], optimal.violations[2]), (UPPER, 3));
        assert_eq!(cost_set(&mut basis, &mut network, 11, 2, 3), Snapshot { violations: vec![0, 0, 1], ..optimal });
    }

    #[test]
    fn test_a_cost_set_on_a_tree_arc_running_up_shifts_its_node_against_the_change_by_hand() {
        // After the depot's pivots of arcs 0 and 2, node 1 hangs from node 2 by arc 2, running up from it, at potential
        // `11 − 1 = 10`. Arc 2 from cost 1 to 4: node 1, a leaf, falls by 3 to 7, so arc 1 into it, at its lower bound,
        // prices `1 + 0 − 7 = −6`; arc 0, at its capacity between nodes 0 and 2, keeps `−(5 + 0 − 11) = 6`.
        let mut network = depot();
        let mut room = KeptBasisRoom::new();
        let mut basis = depot_after(&mut room, &[0, 2]);
        let before = snapshot(&basis);
        assert_eq!((before.hangings[1], before.potentials.as_slice()), (hung(2, 2, UP), &[0, 10, 11, 0][..]));

        let changed = cost_set(&mut basis, &mut network, 11, 2, 4);
        assert_eq!(changed, Snapshot { potentials: vec![0, 7, 11, 0], violations: vec![6, -6, 0], ..before });
    }

    #[test]
    fn test_a_cost_set_on_a_tree_arc_running_down_shifts_its_subtree_with_the_change_by_hand() {
        // After the pivots of arcs 0, 2 and 1, node 1 hangs from node 0 by arc 1, running down to it, and node 2 from
        // node 1 by arc 2: potentials 0, 1 and 2. Arc 1 from cost 1 to 3: nodes 1 and 2 rise by 2 to 3 and 4, arc 2
        // stays tight, and arc 0, at its capacity from node 0 into node 2, prices `−(5 + 0 − 4) = −1`.
        let mut network = depot();
        let mut room = KeptBasisRoom::new();
        let mut basis = depot_after(&mut room, &[0, 2, 1]);
        let before = snapshot(&basis);
        assert_eq!(before.hangings, [hung(3, 3, UP), hung(0, 1, DOWN), hung(1, 2, DOWN)]);
        assert_eq!((before.potentials.as_slice(), before.violations.as_slice()), (&[0, 1, 2, 0][..], &[-3, 0, 0][..]));

        let changed = cost_set(&mut basis, &mut network, 11, 1, 3);
        assert_eq!(changed, Snapshot { potentials: vec![0, 3, 4, 0], violations: vec![-1, 0, 0], ..before });
    }

    #[test]
    fn test_a_cost_set_on_an_arc_of_no_width_or_to_the_cost_it_has_moves_nothing_by_hand() {
        // Node 0 sends one unit to node 1, whose root link points down at `1·3 + 1 = 4`; arc 0 has no width, arc 1
        // prices `1 + 0 − 4`. Arc 0 from cost 3 to −3, unpriced, moves nothing; arc 1 set to its own cost neither.
        let mut network = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 0, 3), (0, 1, 0, 1, 1)]);
        let mut room = KeptBasisRoom::new();
        let mut basis = reopened_star_at(&mut room, &network, 4);
        let star = snapshot(&basis);
        assert_eq!((star.states.as_slice(), star.violations.as_slice()), (&[IN_TREE, LOWER][..], &[0, -3][..]));

        assert_eq!(cost_set(&mut basis, &mut network, 4, 0, -3), star);
        assert_eq!(cost_set(&mut basis, &mut network, 4, 1, 1), star);

        // In the depot's optimum, a tree arc set to its own cost moves nothing either.
        let mut network = depot();
        let mut basis = depot_after(&mut room, &[0, 2, 1, 0]);
        let optimal = snapshot(&basis);
        assert_eq!(cost_set(&mut basis, &mut network, 11, 0, 5), optimal);
    }

    #[test]
    fn test_costs_at_the_cost_bound_keep_a_basis_at_the_widest_root_cost_by_hand() {
        // The depot's star at the widest root cost of three nodes, `M = 2·B + 1` for `B = 1537228672809129301`. Arc 0,
        // into node 2 at `M`, to `B` prices `B − M = −B − 1`, to `−B` prices `−3·B − 1`.
        let (bound, widest) = (cost_bound(3), widest_root_cost(3));
        assert_eq!((bound, widest), (1_537_228_672_809_129_301, 2 * 1_537_228_672_809_129_301 + 1));
        let mut network = depot();
        let mut room = KeptBasisRoom::new();
        let mut basis = reopened_star_at(&mut room, &network, widest);
        assert_eq!(cost_set(&mut basis, &mut network, widest, 0, bound).violations, [-bound - 1, 1, 1 - widest]);
        assert_eq!(cost_set(&mut basis, &mut network, widest, 0, -bound).violations, [-3 * bound - 1, 1, 1 - widest]);

        // Arc 0 fills: 3 units, no link leaves. Arc 2 enters degenerately, node 1 hanging from node 2 by it, running
        // up, at `M − 1`. Set to `B`, it takes node 1 down by `B − 1` to `M − B = B + 1`; set to `−B`, up by `2·B` to
        // `3·B + 1`, the highest potential the bound allows a node one link below a root link that points down.
        basis.pivot(Slot::from(0_usize));
        basis.pivot(Slot::from(2_usize));
        check_at(&basis, &network, widest).expect("the invariant should hold after the pivots");
        assert_eq!(snapshot(&basis).hangings[1], hung(2, 2, UP));
        assert_eq!(cost_set(&mut basis, &mut network, widest, 2, bound).potentials, [0, bound + 1, widest, 0]);
        assert_eq!(cost_set(&mut basis, &mut network, widest, 2, -bound).potentials, [0, 3 * bound + 1, widest, 0]);
    }

    #[test]
    #[should_panic(expected = "called `Basis::set_cost` with Slot 3, which should be below the arc count 3")]
    fn test_set_cost_refuses_a_slot_past_the_arcs() {
        let mut room = KeptBasisRoom::new();
        depot_after(&mut room, &[]).set_cost(Slot::from(3_usize), 1);
    }

    #[test]
    #[should_panic(expected = "called `Basis::set_cost` with the cost -1537228672809129302, which should lie within \
                    ±1537228672809129301 for 3 nodes")]
    fn test_set_cost_refuses_a_cost_past_the_bound() {
        let mut room = KeptBasisRoom::new();
        let mut basis = reopened_star_at(&mut room, &depot(), widest_root_cost(3));
        basis.set_cost(Slot::from(0_usize), -cost_bound(3) - 1);
    }

    #[test]
    #[should_panic(expected = "called `Basis::set_cost` with the cost 6, which asks for the root cost 13, above the \
                               basis's 11")]
    fn test_set_cost_refuses_a_cost_the_root_cost_does_not_serve() {
        // At its own root cost 11 the depot's basis serves costs up to 5 in magnitude: 6 asks for `2·6 + 1 = 13`.
        let mut room = KeptBasisRoom::new();
        depot_after(&mut room, &[]).set_cost(Slot::from(1_usize), 6);
    }

    #[test]
    fn test_hanging_node_answers_the_end_a_tree_arc_hangs_by_hand() {
        // The depot after three pivots: node 1 hangs from node 0 by arc 1, node 2 from node 1 by arc 2, both running
        // down to them; arc 0 is out of the tree at its capacity. After two: node 1 hangs from node 2 by arc 2 running
        // up from it, and arcs 0 and 1 are out of the tree.
        let mut room = KeptBasisRoom::new();
        let basis = depot_after(&mut room, &[0, 2, 1]);
        let hanging = [0_usize, 1, 2].map(|slot| basis.hanging_node(ArcSlot(slot)));
        assert_eq!(hanging, [None, Some(1), Some(2)]);

        let basis = depot_after(&mut room, &[0, 2]);
        let hanging = [0_usize, 1, 2].map(|slot| basis.hanging_node(ArcSlot(slot)));
        assert_eq!(hanging, [None, None, Some(1)]);
    }

    /// One step of a random sequence on a basis. Plain data.
    #[derive(Clone, Copy, Debug)]
    enum ChangeStep {
        /// The cost drawn, at most the bound in magnitude, set on the arc at the slot drawn.
        Cost(Index, CostDraw),
        /// The first violating arc enters, where one violates.
        Pivot,
    }

    /// Sequences of one to twelve steps, two costs set to every pivot.
    fn change_steps() -> impl Strategy<Value = Vec<ChangeStep>> {
        let cost = (any::<Index>(), cost_draws(-9..=9)).prop_map(|(slot, draw)| ChangeStep::Cost(slot, draw));

        proptest::collection::vec(prop_oneof![2 => cost, 1 => Just(ChangeStep::Pivot)], 1..=12)
    }

    /// Runs `steps` on the star of `network` at the widest root cost of its node count, each cost set on `network` as
    /// on the basis. After every step the whole invariant holds against `network` as it then stands; after every cost
    /// set no flow, state or link has moved, and the cost of the flow, root links counted, has moved by the change of
    /// the cost times the arc's flow, and the potentials have moved exactly where the arc is a tree link and its cost
    /// changed. Answers what the run reached.
    fn check_costs_and_pivots(mut network: MincostNetwork, steps: &[ChangeStep]) -> Result<ChangeReach, TestCaseError> {
        let (node_count, arc_count) = (network.node_count(), network.arc_count());
        let (bound, widest) = (cost_bound(node_count), widest_root_cost(node_count));
        let arc_of_slot = arcs_of_slots(arc_count, node_count);
        let mut room = KeptBasisRoom::new();
        let mut basis = reopened_star_at(&mut room, &network, widest);
        let mut reach = ChangeReach::default();

        for &step in steps {
            match step {
                ChangeStep::Cost(slot, draw) if arc_count > 0 => {
                    let (slot, cost) = (slot.index(arc_count), draw.cost(bound).clamp(-bound, bound));
                    let arc = arc_of_slot[slot];
                    let (before, spent) = (snapshot(&basis), extended_cost_at(&basis, &network, widest));
                    let change = cost - network.costs[arc];

                    let answered = basis.set_cost(Slot::from(slot), cost);
                    network.costs[arc] = cost;
                    let after = snapshot(&basis);
                    let moved = extended_cost_at(&basis, &network, widest) - spent;
                    prop_assert_eq!(moved, BigInt::from(change) * BigInt::from(after.flows[slot]));
                    prop_assert_eq!((&after.flows, &after.states), (&before.flows, &before.states));
                    prop_assert_eq!(&after.hangings, &before.hangings);

                    let hanging = before.hangings.iter().find(|hanging| hanging.link == slot);
                    prop_assert_eq!(after.potentials != before.potentials, hanging.is_some() && change != 0);
                    let node = before.hangings.iter().position(|hanging| hanging.link == slot);
                    prop_assert_eq!(answered, node.filter(|_| change != 0));
                    moved_checked(&before, &after, answered.as_slice()).map_err(TestCaseError::fail)?;
                    match (hanging.map(|hanging| hanging.direction), before.states[slot]) {
                        (Some(Direction::Up), _) => reach.up += 1,
                        (Some(Direction::Down), _) => reach.down += 1,
                        (None, ArcState::AtUpper) => reach.at_capacity += 1,
                        (None, _) => reach.at_lower_or_without_width += 1,
                    }
                }
                ChangeStep::Cost(..) => {}
                ChangeStep::Pivot => {
                    if let Some(slot) = EnteringRule::FirstEligible.next(&basis.rows()) {
                        basis.pivot(slot);
                        reach.pivots += 1;
                    }
                }
            }
            check_at(&basis, &network, widest).map_err(TestCaseError::fail)?;
        }

        Ok(reach)
    }

    /// What a random sequence of costs set and pivots reached, counted. Plain data.
    #[derive(Clone, Copy, Debug, Default)]
    struct ChangeReach {
        /// Costs set on a tree arc that runs up from the node it hangs.
        up: usize,
        /// Costs set on a tree arc that runs down to the node it hangs.
        down: usize,
        /// Costs set on an arc out of the tree at its capacity.
        at_capacity: usize,
        /// Costs set on an arc out of the tree at its lower bound or of no width.
        at_lower_or_without_width: usize,
        /// Pivots made.
        pivots: usize,
    }

    /// Routes `amount` from `from` to `to` in `basis`, a basis of `network` at `root_cost`, moves the two supplies of
    /// `network` alike, checks the whole invariant and the nodes the route answers moved, and answers the basis, read
    /// out, with what the route gained.
    ///
    /// # Panics
    /// Panics if the invariant fails after the route, if the nodes answered are not the tops of the subtrees whose
    /// potentials moved, or if `amount` passes `i64::MAX`.
    fn routed<'r>(
        basis: Basis<'r>,
        network: &mut MincostNetwork,
        root_cost: i64,
        [from, to]: [usize; 2],
        amount: u64,
    ) -> (Basis<'r>, Snapshot, [i128; 2]) {
        let before = snapshot(&basis);
        let mut moved = vec![usize::MAX];
        let Rerouted { basis, cost_gained, up_gained } = basis.route(from, to, amount, &mut moved);
        let units = i64::try_from(amount).expect("a test's amount should fit an i64");
        network.supplies[from] += units;
        network.supplies[to] -= units;
        if let Err(breach) = check_at(&basis, network, root_cost) {
            panic!("the invariant should hold after the route of {amount} from {from} to {to}: {breach}");
        }
        let read = snapshot(&basis);
        if let Err(breach) = pushed_checked(&before, &read, &moved) {
            panic!("the route of {amount} from {from} to {to} should push the tops it moved: {breach}");
        }

        (basis, read, [cost_gained, up_gained])
    }

    /// Whether `pushed`, a buffer that held `usize::MAX` alone before a change of a basis from `before` to `after`,
    /// holds after it the nodes whose subtrees' potentials the change moved, each under the root, as
    /// [`moved_checked`] has them.
    ///
    /// # Errors
    /// Returns the first breach.
    fn pushed_checked(before: &Snapshot, after: &Snapshot, pushed: &[usize]) -> Result<(), String> {
        let Some((&usize::MAX, moved)) = pushed.split_first() else {
            return Err(format!("the buffer {pushed:?} should keep what it held first"));
        };
        let root = before.hangings.len();
        if let Some(&top) = moved.iter().find(|&&top| after.hangings[top].parent != root) {
            return Err(format!("node {top} is pushed but hangs from {}", after.hangings[top].parent));
        }

        moved_checked(before, after, moved)
    }

    #[test]
    fn test_a_link_sends_its_flow_toward_the_root_up_and_its_negation_down_by_hand() {
        assert_eq!((toward_root(5, UP), toward_root(5, DOWN), toward_root(0, DOWN)), (5, -5, 0));
        assert_eq!(toward_root(i128::from(u64::MAX), DOWN), -i128::from(u64::MAX));
        // A flow restated below its lower bound sends the other way.
        assert_eq!((toward_root(-2, UP), toward_root(-2, DOWN)), (-2, 2));
    }

    #[test]
    fn test_a_links_price_costs_its_flow_and_counts_it_up_into_the_root_by_hand() {
        let (arc, up, down) = (LinkPrice::Arc(-3), LinkPrice::RootLink(UP), LinkPrice::RootLink(DOWN));
        assert_eq!([arc, up, down].map(|price| price.cost(4, 11)), [-12, 0, 44]);
        assert_eq!([arc, up, down].map(|price| price.up(4)), [0, 4, 0]);
        assert_eq!(down.cost(i128::from(u64::MAX), 1 << 62), i128::from(u64::MAX) << 62);
        // A flow restated below its lower bound costs below zero; past `2^127` the cost wraps, as every gain does.
        assert_eq!([arc.cost(-2, 11), up.up(-2)], [6, -2]);
        assert_eq!(LinkPrice::Arc(2).cost(i128::MAX, 11), -2);
    }

    #[test]
    fn test_the_flow_up_the_root_links_by_hand() {
        // The depot's star at 11: nodes 0 and 1 up, carrying 4 and 0, node 2 down, carrying 4: four units up. Its
        // optimum routes everything.
        let mut room = KeptBasisRoom::new();
        assert_eq!(depot_after(&mut room, &[]).unrouted(), 4);
        assert_eq!(depot_after(&mut room, &[0]).unrouted(), 1);
        assert_eq!(depot_after(&mut room, &[0, 2, 1, 0]).unrouted(), 0);
    }

    #[test]
    fn test_routes_the_tree_carries_cannot_carry_and_set_back_by_hand() {
        // The depot's optimum at 11: node 0 under the root by its root link 3, up and empty; nodes 1 and 2 under node
        // 0 by arcs 1 and 0, running down to them, each carrying 2; arc 2 at its capacity 2. Potentials 0, 1, 5.
        let mut network = depot();
        let mut room = KeptBasisRoom::new();
        let basis = depot_after(&mut room, &[0, 2, 1, 0]);
        let optimal = snapshot(&basis);
        assert_eq!(optimal.hangings, [hung(3, 3, UP), hung(0, 1, DOWN), hung(0, 0, DOWN)]);
        assert_eq!(
            (optimal.flows.as_slice(), optimal.potentials.as_slice()),
            (&[2, 2, 2, 0, 0, 0][..], &[0, 1, 5, 0][..])
        );

        // One unit more from node 1 to node 2: arc 1 brings node 1 one less, arc 0 node 2 one more, up to its width 3,
        // where it can still bring less. Nothing reaches node 0. The cost gains `5 − 1`.
        let (basis, read, gained) = routed(basis, &mut network, 11, [1, 2], 1);
        assert_eq!((read.flows.as_slice(), gained), (&[3, 1, 2, 0, 0, 0][..], [4, 0]));
        assert_eq!(read, Snapshot { flows: read.flows.clone(), ..optimal.clone() });

        // Two more from node 0 to node 2: node 0 is node 2's ancestor, so its path is empty. Arc 0, at its capacity 3
        // now, can bring node 2 nothing more, and could bring less: it leaves the tree at its capacity, and node 2
        // hangs from the root by its root link 5, down, carrying both, at potential `M = 11`. Node 0 keeps the two on
        // its root link, up. The cost gains `2·11`, the flow up the root links two.
        let (basis, read, gained) = routed(basis, &mut network, 11, [0, 2], 2);
        assert_eq!(read.hangings, [hung(3, 3, UP), hung(0, 1, DOWN), hung(3, 5, DOWN)]);
        assert_eq!((read.flows.as_slice(), read.potentials.as_slice()), (&[3, 1, 2, 2, 0, 2][..], &[0, 1, 11, 0][..]));
        assert_eq!((read.states.as_slice(), gained, basis.unrouted()), (&[UPPER, IN_TREE, UPPER][..], [22, 2], 2));

        // Set back, two from node 2 to node 0: the two nodes now hang in two components, so each path ends at its top.
        // Node 2's root link, two down, comes to carry nothing and turns up: its potential falls by 11 to 0. Node 0's,
        // two up, comes to carry nothing and keeps pointing up. The cost gains `−2·11`, the flow up loses two.
        let (basis, read, gained) = routed(basis, &mut network, 11, [2, 0], 2);
        assert_eq!(read.hangings, [hung(3, 3, UP), hung(0, 1, DOWN), hung(3, 5, UP)]);
        assert_eq!((read.flows.as_slice(), read.potentials.as_slice()), (&[3, 1, 2, 0, 0, 0][..], &[0, 1, 0, 0][..]));
        assert_eq!((gained, basis.unrouted(), network.supplies.as_slice()), ([-22, -2], 0, &[4, 1, -5][..]));
    }

    #[test]
    fn test_a_route_with_two_cuts_and_a_turn_by_hand() {
        // The depot after three pivots at 11: node 0 under the root, up and empty; node 1 under it by arc 1, node 2
        // under node 1 by arc 2, both down and carrying 1; arc 0 at its capacity 3. Potentials 0, 1, 2.
        let mut network = depot();
        let mut room = KeptBasisRoom::new();
        let basis = depot_after(&mut room, &[0, 2, 1]);
        let before = snapshot(&basis);
        assert_eq!(before.hangings, [hung(3, 3, UP), hung(0, 1, DOWN), hung(1, 2, DOWN)]);
        assert_eq!(
            (before.flows.as_slice(), before.potentials.as_slice()),
            (&[3, 1, 1, 0, 0, 0][..], &[0, 1, 2, 0][..])
        );

        // Two more from node 2 to node 0. Arc 2 brings node 2 nothing then, and the rest, one, goes up node 2's root
        // link: a cut, node 2 at 0. Node 1 is handed one: arc 1 brings it nothing, with no rest, and could bring no
        // less: a cut for that alone, its root link up and empty, node 1 at 0. Node 0, the apex, is handed one and
        // must send two: its root link turns down, carrying one, and it rises to 11. The cost gains `−1 − 1 + 11`.
        let (basis, read, gained) = routed(basis, &mut network, 11, [2, 0], 2);
        assert_eq!(read.hangings, [hung(3, 3, DOWN), hung(3, 4, UP), hung(3, 5, UP)]);
        assert_eq!((read.flows.as_slice(), read.potentials.as_slice()), (&[3, 0, 0, 1, 0, 1][..], &[11, 0, 0, 0][..]));
        assert_eq!((read.states.as_slice(), gained, basis.unrouted()), (&[UPPER, LOWER, LOWER][..], [9, 1], 1));
    }

    #[test]
    #[should_panic(expected = "called `Basis::route` with the nodes 1 and 3, which should be below the node count 3")]
    fn test_route_refuses_a_node_past_the_nodes() {
        let mut room = KeptBasisRoom::new();
        let _ = depot_after(&mut room, &[]).route(1, 3, 1, &mut Vec::new());
    }

    #[test]
    #[should_panic(expected = "called `Basis::route` with the amount 18446744073709551611, which leaves \
                               18446744073709551615 up the root links, past 18446744073709551614")]
    fn test_route_refuses_an_amount_that_takes_the_root_links_past_the_limit() {
        // The depot's star carries four units up its root links. `2^64 − 5` more from node 0 to node 1, in two
        // components, go up node 0's root link, to `u64::MAX`, and down node 1's: `2^64 − 1` up in all.
        let mut room = KeptBasisRoom::new();
        let _ = depot_after(&mut room, &[]).route(0, 1, NET_SUPPLY_LIMIT - 3, &mut Vec::new());
    }

    #[test]
    #[should_panic(expected = "called `root_link` with excess 18446744073709551618, whose magnitude passes u64::MAX")]
    fn test_route_refuses_an_amount_that_takes_a_root_link_past_u64_max() {
        let mut room = KeptBasisRoom::new();
        let _ = depot_after(&mut room, &[]).route(0, 1, NET_SUPPLY_LIMIT, &mut Vec::new());
    }

    /// Restates the arc at `slot` of `basis`, a basis of `network` at `root_cost`, for the bounds
    /// `lower_bound..=capacity`, gives the arc dealt there in `network` those bounds, checks the whole invariant and
    /// the nodes the restatement answers moved, and answers the basis, read out, with what the restatement gained.
    ///
    /// # Panics
    /// Panics if the invariant fails after the restatement, if the nodes answered are not the tops of the subtrees
    /// whose potentials moved, or if the lower bound passes the capacity.
    fn restated<'r>(
        basis: Basis<'r>,
        network: &mut MincostNetwork,
        root_cost: i64,
        slot: usize,
        [lower_bound, capacity]: [u64; 2],
    ) -> (Basis<'r>, Snapshot, [i128; 2]) {
        let arc = arcs_of_slots(network.arc_count(), network.node_count())[slot];
        let width = capacity.checked_sub(lower_bound).expect("a test's lower bound should be at most its capacity");
        let restated = Restated::new(network.lower_bounds[arc], lower_bound, width);
        let before = snapshot(&basis);
        let mut moved = vec![usize::MAX];
        let Rerouted { basis, cost_gained, up_gained } = basis.restate(Slot::from(slot), restated, &mut moved);
        (network.lower_bounds[arc], network.capacities[arc]) = (lower_bound, capacity);
        if let Err(breach) = check_at(&basis, network, root_cost) {
            panic!("the invariant should hold after slot {slot} took the bounds {lower_bound}..={capacity}: {breach}");
        }
        let read = snapshot(&basis);
        if let Err(breach) = pushed_checked(&before, &read, &moved) {
            panic!("slot {slot} restated to {lower_bound}..={capacity} should push the tops it moved: {breach}");
        }

        (basis, read, [cost_gained, up_gained])
    }

    #[test]
    fn test_a_capacity_under_a_tree_arcs_flow_cuts_its_node_and_its_set_back_mends_it_by_hand() {
        // The depot's optimum at 11. Arc 0 hangs node 2 from node 0, running down, carrying 2; its capacity falls to 1.
        // It brings node 2 one unit, at its new capacity, and leaves the tree there; node 2 hangs from the root by its
        // root link 5, down, carrying the other, at potential 11. Node 0 is handed one more to send: its root link 3,
        // up and empty, takes it. The cost gains `5·(1 − 2) + 11`, the flow up one.
        let mut network = depot();
        let mut room = KeptBasisRoom::new();
        let basis = depot_after(&mut room, &[0, 2, 1, 0]);

        let (basis, read, gained) = restated(basis, &mut network, 11, 0, [0, 1]);
        assert_eq!(read.hangings, [hung(3, 3, UP), hung(0, 1, DOWN), hung(3, 5, DOWN)]);
        assert_eq!((read.flows.as_slice(), read.potentials.as_slice()), (&[1, 2, 2, 1, 0, 1][..], &[0, 1, 11, 0][..]));
        assert_eq!((read.states.as_slice(), gained, basis.unrouted()), (&[UPPER, IN_TREE, UPPER][..], [6, 1], 1));

        // Set back to 3: arc 0, out of the tree at its capacity, carries its new width 3, two more, which leave node 2
        // and arrive at node 0 through the tree. The two hang in two components: node 2's root link, one down, turns up
        // carrying one, and node 2 falls to 0; node 0's, one up, turns down carrying one, and nodes 0 and 1 rise by 11.
        // The cost gains `5·2 − 11 + 11`; the flow up stays at one.
        let (basis, read, gained) = restated(basis, &mut network, 11, 0, [0, 3]);
        assert_eq!(read.hangings, [hung(3, 3, DOWN), hung(0, 1, DOWN), hung(3, 5, UP)]);
        assert_eq!((read.flows.as_slice(), read.potentials.as_slice()), (&[3, 2, 2, 1, 0, 1][..], &[11, 12, 0, 0][..]));
        assert_eq!((read.states.as_slice(), gained, basis.unrouted()), (&[UPPER, IN_TREE, UPPER][..], [10, 0], 1));
    }

    #[test]
    fn test_a_tree_arc_switched_off_and_on_again_by_hand() {
        // The depot's optimum at 11. Arc 1 hangs node 1 from node 0, running down, carrying 2; its capacity falls to 0.
        // It carries nothing and, with no width, leaves the tree unpriced; node 1 hangs from the root by its root link
        // 4, down, carrying both, at 11. Node 0's root link takes the two up. The cost gains `−2 + 2·11`, the flow up
        // 2.
        let mut network = depot();
        let mut room = KeptBasisRoom::new();
        let basis = depot_after(&mut room, &[0, 2, 1, 0]);

        let (basis, read, gained) = restated(basis, &mut network, 11, 1, [0, 0]);
        assert_eq!(read.hangings, [hung(3, 3, UP), hung(3, 4, DOWN), hung(0, 0, DOWN)]);
        assert_eq!((read.flows.as_slice(), read.potentials.as_slice()), (&[2, 0, 2, 2, 2, 0][..], &[0, 11, 5, 0][..]));
        assert_eq!((read.states.as_slice(), gained, basis.unrouted()), (&[IN_TREE, IN_TREE, UPPER][..], [20, 2], 2));

        // On again at 4: an arc of no width out of the tree that gains some stands at its lower bound, carrying
        // nothing; nothing moves.
        let (basis, on, gained) = restated(basis, &mut network, 11, 1, [0, 4]);
        assert_eq!((on.states.as_slice(), gained, basis.unrouted()), (&[IN_TREE, LOWER, UPPER][..], [0, 0], 2));
        assert_eq!(on, Snapshot { states: on.states.clone(), violations: on.violations.clone(), ..read });
    }

    #[test]
    fn test_a_lower_bound_above_a_tree_arcs_flow_cuts_its_node_and_turns_the_top_by_hand() {
        // The depot's optimum at 11. Arc 0, carrying 2 of its capacity 3 down to node 2, takes the lower bound 3: it
        // holds `2 + 0 − 3 = −1` above it, with no width. It carries nothing then and leaves the tree unpriced; node 2
        // hangs from the root by its root link 5, up, carrying the unit it no longer needs, at 0. Node 0 sends one unit
        // less down arc 0 and must send it elsewhere: its root link 3, up and empty, turns down to bring it, and nodes
        // 0 and 1 rise by 11. The arc's whole flow rose from 2 to 3: the cost gains `5·1 + 11`, the flow up one.
        let mut network = depot();
        let mut room = KeptBasisRoom::new();
        let basis = depot_after(&mut room, &[0, 2, 1, 0]);

        let (basis, read, gained) = restated(basis, &mut network, 11, 0, [3, 3]);
        assert_eq!(read.hangings, [hung(3, 3, DOWN), hung(0, 1, DOWN), hung(3, 5, UP)]);
        assert_eq!((read.flows.as_slice(), read.potentials.as_slice()), (&[0, 2, 2, 1, 0, 1][..], &[11, 12, 0, 0][..]));
        assert_eq!((read.states.as_slice(), gained, basis.unrouted()), (&[IN_TREE, IN_TREE, UPPER][..], [16, 1], 1));
    }

    #[test]
    fn test_bounds_on_a_full_arc_out_of_the_tree_by_hand() {
        // The depot's optimum at 11: arc 2, from node 1 to node 2, out of the tree at its capacity 2.
        let mut room = KeptBasisRoom::new();
        let optimal = snapshot(&depot_after(&mut room, &[0, 2, 1, 0]));

        // Its capacity falls to 1: one unit less leaves node 1 and arrives at node 2 by arc 2; arc 1 brings node 1 one
        // less, arc 0 node 2 one more, to its capacity 3, both staying. The cost gains `−1 − 1 + 5`.
        let mut network = depot();
        let basis = depot_after(&mut room, &[0, 2, 1, 0]);
        let (_, read, gained) = restated(basis, &mut network, 11, 2, [0, 1]);
        assert_eq!((read.flows.as_slice(), gained), (&[3, 1, 1, 0, 0, 0][..], [3, 0]));
        assert_eq!(
            read,
            Snapshot { flows: read.flows.clone(), violations: read.violations.clone(), ..optimal.clone() }
        );

        // Its capacity rises to 3: one more by arc 2, one more by arc 1, one less by arc 0. The cost gains `1 + 1 − 5`.
        let mut network = depot();
        let basis = depot_after(&mut room, &[0, 2, 1, 0]);
        let (_, read, gained) = restated(basis, &mut network, 11, 2, [0, 3]);
        assert_eq!(
            (read.flows.as_slice(), read.states, gained),
            (&[1, 3, 3, 0, 0, 0][..], optimal.states.clone(), [-3, 0])
        );

        // Its lower bound rises to 1, under its flow: it carries 1 above it, its new width, and stands there. Nothing
        // moves, and no cost: its whole flow is still 2.
        let mut network = depot();
        let basis = depot_after(&mut room, &[0, 2, 1, 0]);
        let (_, read, gained) = restated(basis, &mut network, 11, 2, [1, 2]);
        assert_eq!((read.flows.as_slice(), gained), (&[2, 2, 1, 0, 0, 0][..], [0, 0]));
        assert_eq!(read, Snapshot { flows: read.flows.clone(), ..optimal.clone() });

        // Both bounds past the old ones, to `3..=4`: it holds `2 − 3 = −1` above its new lower bound and goes to its
        // new capacity, carrying its width 1 above it, its whole flow 4: two more leave node 2 and arrive at node 1.
        // Arc 0 can bring node 2 two less only to nothing, where it has no room toward the root: it leaves the tree at
        // its lower bound, and node 2 hangs from the root by its root link 5, up and empty, at 0. Arc 1 brings node 1
        // two more, to its capacity 4, and stays. The cost gains `1·2 − 5·2 + 1·2`.
        let mut network = depot();
        let basis = depot_after(&mut room, &[0, 2, 1, 0]);
        let (_, read, gained) = restated(basis, &mut network, 11, 2, [3, 4]);
        assert_eq!(read.hangings, [hung(3, 3, UP), hung(0, 1, DOWN), hung(3, 5, UP)]);
        assert_eq!((read.flows.as_slice(), read.potentials.as_slice()), (&[0, 4, 1, 0, 0, 0][..], &[0, 1, 0, 0][..]));
        assert_eq!((read.states.as_slice(), gained), (&[LOWER, IN_TREE, UPPER][..], [-6, 0]));
    }

    #[test]
    fn test_bounds_on_a_tree_arc_within_its_new_width_move_nothing_by_hand() {
        // The depot's optimum at 11. Arc 1, carrying 2 down to node 1, takes the lower bound 1: it carries 1 above it,
        // with room to bring less. Arc 0, carrying 2 down to node 2, takes the capacity 2: exactly its width, and it
        // can still bring less. Neither moves a flow or a cost.
        let mut network = depot();
        let mut room = KeptBasisRoom::new();
        let basis = depot_after(&mut room, &[0, 2, 1, 0]);
        let optimal = snapshot(&basis);

        let (basis, read, gained) = restated(basis, &mut network, 11, 1, [1, 4]);
        assert_eq!((read.flows.as_slice(), gained), (&[2, 1, 2, 0, 0, 0][..], [0, 0]));
        let (_, read, gained) = restated(basis, &mut network, 11, 0, [0, 2]);
        assert_eq!((read.flows.as_slice(), gained), (&[2, 1, 2, 0, 0, 0][..], [0, 0]));
        assert_eq!(read, Snapshot { flows: read.flows.clone(), violations: read.violations.clone(), ..optimal });
    }

    #[test]
    fn test_a_lower_bound_on_an_empty_arc_raised_and_lowered_by_hand() {
        // The depot's star at 11: node 0 up 4, node 1 up 0, node 2 down 4, every arc at its lower bound. Arc 1, from
        // node 0 to node 1, takes the lower bound 2 of its capacity 4: its whole flow rises to 2, two more leave node 1
        // and arrive at node 0, two components: node 1's root link takes two up, node 0's two less. The cost gains
        // `1·2`; the flow up stays.
        let mut network = depot();
        let mut room = KeptBasisRoom::new();
        let basis = depot_after(&mut room, &[]);
        let star = snapshot(&basis);

        let (basis, read, gained) = restated(basis, &mut network, 11, 1, [2, 4]);
        assert_eq!((read.flows.as_slice(), gained, basis.unrouted()), (&[0, 0, 0, 2, 2, 4][..], [2, 0], 4));
        assert_eq!(read, Snapshot { flows: read.flows.clone(), violations: read.violations.clone(), ..star.clone() });

        // Lowered to 1, below its flow: at its lower bound it carries its new lower bound, one less, which node 1's
        // root link gives back to node 0's. The cost gains `−1`.
        let (basis, read, gained) = restated(basis, &mut network, 11, 1, [1, 4]);
        assert_eq!((read.flows.as_slice(), gained, basis.unrouted()), (&[0, 0, 0, 3, 1, 4][..], [-1, 0], 4));
        assert_eq!(read.states, star.states);
    }

    #[test]
    fn test_bounds_on_an_arc_of_no_width_move_nothing_by_hand() {
        // Node 0 sends one unit to node 1, whose root link points down at 4; arc 0 has no width. Its capacity rises to
        // 2: it stands at its lower bound, carrying nothing; nothing moves.
        let mut network = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 0, 3), (0, 1, 0, 1, 1)]);
        let mut room = KeptBasisRoom::new();
        let basis = reopened_star_at(&mut room, &network, 4);
        let (_, read, gained) = restated(basis, &mut network, 4, 0, [0, 2]);
        assert_eq!(
            (read.states.as_slice(), read.flows.as_slice(), gained),
            (&[LOWER, LOWER][..], &[0, 0, 1, 1][..], [0, 0])
        );

        // Arc 0 carries its fixed unit from node 0 to node 1: no net supply is left. Its lower bound falls to 0: it
        // carries 1 above it, its whole width, and stands at its capacity; nothing moves.
        let mut network = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 1, 1, 3), (0, 1, 0, 1, 1)]);
        let basis = reopened_star_at(&mut room, &network, 4);
        let (_, read, gained) = restated(basis, &mut network, 4, 0, [0, 1]);
        assert_eq!(
            (read.states.as_slice(), read.flows.as_slice(), gained),
            (&[UPPER, LOWER][..], &[1, 0, 0, 0][..], [0, 0])
        );
    }

    #[test]
    fn test_the_room_reads_the_amount_a_restatement_moves_by_hand() {
        // The depot's optimum at 11, kept: arc 0 in the tree carrying 2 of 3, arc 2 out of it at its capacity 2.
        let mut room = KeptBasisRoom::new();
        let _ = depot_after(&mut room, &[0, 2, 1, 0]).keep();
        room.kept = Kept::Basis;
        let amount = |slot: usize, restated| room.amount_to_route(Slot::from(slot), restated);

        assert_eq!(amount(0, Restated::new(0, 0, 1)), Some(1), "a tree arc's capacity under its flow");
        assert_eq!(amount(0, Restated::new(0, 3, 0)), Some(1), "a tree arc's lower bound above its flow");
        assert_eq!(amount(0, Restated::new(0, 0, 5)), Some(0), "a tree arc within its width");
        assert_eq!(amount(2, Restated::new(0, 0, 1)), Some(1), "a full arc's capacity lowered");
        assert_eq!(amount(2, Restated::new(0, 3, 1)), Some(2), "a full arc's bounds crossed");
        assert_eq!(amount(2, Restated::new(0, 1, 1)), Some(0), "a full arc's lower bound under its flow");
        assert_eq!(amount(3, Restated::new(0, 0, 1)), None, "past the arcs");

        let _ = room.reopen();
        assert_eq!(room.amount_to_route(Slot::from(0_usize), Restated::new(0, 0, 1)), None, "no basis held");
    }

    #[test]
    #[should_panic(expected = "called `Basis::restate` with Slot 3, which should be below the arc count 3")]
    fn test_restate_refuses_a_slot_past_the_arcs() {
        let mut room = KeptBasisRoom::new();
        let _ = depot_after(&mut room, &[]).restate(Slot::from(3_usize), Restated::new(0, 0, 1), &mut Vec::new());
    }

    #[test]
    #[should_panic(expected = "called `Basis::restate` with Slot 1, which leaves 18446744073709551615 up the root \
                               links, past 18446744073709551614")]
    fn test_restate_refuses_bounds_that_take_the_root_links_past_the_limit() {
        // The depot's star carries four units up its root links. Arc 1, from node 0 to node 1, empty, takes the lower
        // bound `2^64 − 1` and no width: that much more leaves node 1 and arrives at node 0, in two components, up node
        // 1's root link, while node 0's turns down: `2^64 − 1` up in all.
        let mut room = KeptBasisRoom::new();
        let _ =
            depot_after(&mut room, &[]).restate(Slot::from(1_usize), Restated::new(0, u64::MAX, 0), &mut Vec::new());
    }

    /// One step of a random sequence of routes, restatements, costs and pivots on a basis. Plain data.
    #[derive(Clone, Copy, Debug)]
    enum RouteStep {
        /// The amount drawn routed from the node drawn to the node drawn.
        Route(Index, Index, u64),
        /// The arc at the slot drawn restated for the lower bound and the capacity drawn, where the first is at most
        /// the second and the flow it moves keeps the root links within the limit.
        Restate(Index, RestatedBound, RestatedBound),
        /// The cost drawn, at most the bound in magnitude, set on the arc at the slot drawn.
        Cost(Index, CostDraw),
        /// The first violating arc enters, where one violates.
        Pivot,
    }

    /// A bound a random restatement sets: drawn, or the arc's whole flow, so that arcs are restated onto a bound. Plain
    /// data.
    #[derive(Clone, Copy, Debug)]
    enum RestatedBound {
        /// The bound drawn.
        Drawn(BoundDraw),
        /// The arc's whole flow, its lower bound included, as the basis holds it.
        AtFlow,
    }

    impl RestatedBound {
        /// The bound, in units of flow, for an arc whose whole flow is `flow`.
        fn bound(self, flow: u64) -> u64 {
            match self {
                Self::Drawn(draw) => draw.bound(),
                Self::AtFlow => flow,
            }
        }
    }

    /// A route of an amount drawn from a node drawn to a node drawn.
    fn route_step() -> impl Strategy<Value = RouteStep> {
        (any::<Index>(), any::<Index>(), 1..=3_u64).prop_map(|(from, to, amount)| RouteStep::Route(from, to, amount))
    }

    /// A cost drawn set on the arc at a slot drawn.
    fn cost_step() -> impl Strategy<Value = RouteStep> {
        (any::<Index>(), cost_draws(-9..=9)).prop_map(|(slot, draw)| RouteStep::Cost(slot, draw))
    }

    /// Sequences of one to twelve steps, two routes to every cost and every pivot, and no restatement: routes dense
    /// enough to cut twice in one.
    fn route_and_cost_steps() -> impl Strategy<Value = Vec<RouteStep>> {
        let steps = prop_oneof![2 => route_step(), 1 => cost_step(), 1 => Just(RouteStep::Pivot)];

        proptest::collection::vec(steps, 1..=12)
    }

    /// Sequences of one to sixteen steps, two routes, two restatements and two pivots to every cost.
    fn route_steps() -> impl Strategy<Value = Vec<RouteStep>> {
        let bound =
            || prop_oneof![3 => bound_draws(0..=4).prop_map(RestatedBound::Drawn), 1 => Just(RestatedBound::AtFlow)];
        let restate = (any::<Index>(), bound(), bound())
            .prop_map(|(slot, lower_bound, capacity)| RouteStep::Restate(slot, lower_bound, capacity));
        let steps = prop_oneof![2 => route_step(), 2 => restate, 1 => cost_step(), 2 => Just(RouteStep::Pivot)];

        proptest::collection::vec(steps, 1..=16)
    }

    /// What a random sequence of routes reached, counted. Plain data.
    #[derive(Clone, Copy, Debug, Default)]
    struct RouteReach {
        /// Routes the tree carried: no link left it and the flow up the root links stayed.
        carried: usize,
        /// Routes that cut a node from its arc.
        cut: usize,
        /// Routes that cut two nodes or more.
        cut_twice: usize,
        /// Routes that turned a root link.
        turned: usize,
        /// Routes that raised the flow up the root links.
        raised: usize,
        /// Routes that lowered it.
        lowered: usize,
        /// Cuts of a node from an arc that runs up from it.
        cut_up: usize,
        /// Cuts of a node from an arc that runs down to it.
        cut_down: usize,
        /// Cuts whose node's root link carries nothing: for strong feasibility alone.
        cut_without_rest: usize,
        /// Routes with an empty path: one end an ancestor of the other.
        empty_leg: usize,
        /// Routes the tree carried on two paths, neither end an ancestor of the other.
        both_legs_carried: usize,
        /// Routes with an end under the root.
        end_a_top: usize,
        /// Routes that turned two tops.
        turned_twice: usize,
        /// Routes that cut a node on a path and then turned the top above their apex, the apex below the root.
        cut_then_apex_top_turned: usize,
        /// Restatements of a tree arc that left it in the tree.
        tree_arc_stayed: usize,
        /// Restatements of a tree arc that cut its node.
        tree_arc_cut: usize,
        /// Restatements of a tree arc to no width.
        tree_arc_emptied: usize,
        /// Restatements of an arc out of the tree that moved flow.
        moved_out_of_the_tree: usize,
        /// Restatements of an arc out of the tree that moved none.
        unmoved_out_of_the_tree: usize,
        /// Restatements that gave an arc of no width some.
        widened_from_none: usize,
        /// Restatements to bounds past both old ones, or below both.
        crossed: usize,
        /// Restatements that raised the flow up the root links.
        restate_raised: usize,
        /// Restatements left out for the flow they could move past the limit.
        past_the_limit: usize,
        /// Tree arcs running down restated onto their new capacity, which stayed.
        tree_arc_down_at_capacity_stayed: usize,
        /// Tree arcs running up restated onto their new lower bound, which stayed.
        tree_arc_up_at_lower_bound_stayed: usize,
        /// Tree arcs running down restated onto their new lower bound, cut with no rest.
        tree_arc_down_at_lower_bound_cut: usize,
        /// Tree arcs running up restated onto their new capacity, cut with no rest.
        tree_arc_up_at_capacity_cut: usize,
        /// Capacities raised on an arc out of the tree at its lower bound.
        empty_arc_capacity_raised: usize,
        /// Capacities lowered on an arc out of the tree at its lower bound.
        empty_arc_capacity_lowered: usize,
    }

    /// The node under the root above `node`, `node` itself where it hangs from the root, in the tree `hangings`
    /// describe, the root being their count; `node` is below that count.
    fn top_of(hangings: &[PlainHanging], node: usize) -> usize {
        let root = hangings.len();
        let mut at = node;
        for _ in 0..root {
            if hangings[at].parent == root {
                break;
            }
            at = hangings[at].parent;
        }

        at
    }

    /// Whether `top` is `node` or one of its ancestors in the tree `hangings` describe, the root being `root`.
    fn is_ancestor(hangings: &[PlainHanging], root: usize, top: usize, node: usize) -> bool {
        let mut at = node;
        for _ in 0..=hangings.len() {
            if at == top {
                return true;
            }
            if at == root {
                return false;
            }
            at = hangings[at].parent;
        }

        false
    }

    /// Runs `steps` on the star of `network` at the widest root cost of its node count, each route, restatement and
    /// cost applied to `network` as to the basis. After every step the whole invariant holds against `network` as it
    /// then stands; after every route and every restatement the cost of the flow, lower bounds and root links counted,
    /// has moved by what it answers, modulo `2^128`, and the flow up the root links by what it answers, by at most the
    /// amount. Answers what the run reached.
    fn check_routes(mut network: MincostNetwork, steps: &[RouteStep]) -> Result<RouteReach, TestCaseError> {
        let (node_count, arc_count) = (network.node_count(), network.arc_count());
        let (bound, widest) = (cost_bound(node_count), widest_root_cost(node_count));
        let arc_of_slot = arcs_of_slots(arc_count, node_count);
        let mut room = KeptBasisRoom::new();
        let mut basis = reopened_star_at(&mut room, &network, widest);
        let mut reach = RouteReach::default();

        for &step in steps {
            match step {
                // A route the root links would not admit is the caller's to refuse: after a restatement to the widest
                // bounds the flow up can stand at the limit.
                RouteStep::Route(_, _, amount)
                    if u128::from(basis.unrouted()) + u128::from(amount) > u128::from(NET_SUPPLY_LIMIT) => {}
                RouteStep::Route(from, to, amount) => {
                    let (from, to) = (from.index(node_count), to.index(node_count));
                    let (before, spent, up) =
                        (snapshot(&basis), extended_cost_at(&basis, &network, widest), basis.unrouted());
                    let (moved, read, [cost_gained, up_gained]) =
                        routed(basis, &mut network, widest, [from, to], amount);
                    basis = moved;

                    let cost = extended_cost_at(&basis, &network, widest) - spent - BigInt::from(cost_gained);
                    prop_assert_eq!(cost % (BigInt::from(1_u8) << 128), BigInt::from(0_u8));
                    let unrouted = i128::from(basis.unrouted());
                    prop_assert_eq!(unrouted - i128::from(up), up_gained);
                    prop_assert!(up_gained <= i128::from(amount));

                    let changed = (0..node_count).filter(|&node| before.hangings[node] != read.hangings[node]);
                    let cut: Vec<usize> =
                        changed.clone().filter(|&node| before.hangings[node].link < arc_count).collect();
                    let (cuts, turns) =
                        (cut.len(), changed.filter(|&node| before.hangings[node].link >= arc_count).count());
                    let carried = cuts + turns == 0 && up_gained == 0 && from != to;
                    let ancestor = |top: usize, node: usize| is_ancestor(&before.hangings, node_count, top, node);
                    let empty_leg = from != to && (ancestor(from, to) || ancestor(to, from));
                    reach.carried += usize::from(carried);
                    reach.cut_up += cut.iter().filter(|&&node| before.hangings[node].direction == UP).count();
                    reach.cut_down += cut.iter().filter(|&&node| before.hangings[node].direction == DOWN).count();
                    let rests = cut.iter().filter(|&&node| read.flows[arc_count + node] == 0).count();
                    reach.cut_without_rest += rests;
                    reach.empty_leg += usize::from(empty_leg);
                    reach.both_legs_carried += usize::from(carried && !ancestor(from, to) && !ancestor(to, from));
                    let tops = [from, to].iter().any(|&node| before.hangings[node].parent == node_count);
                    reach.end_a_top += usize::from(from != to && tops);
                    reach.turned_twice += usize::from(turns >= 2);
                    // The apex: the first node on the path from `from` up that is an ancestor of `to`.
                    let path = iter::successors(Some(from), |&node| {
                        (node != node_count).then(|| before.hangings[node].parent)
                    });
                    let apex = path.take(node_count + 1).find(|&node| ancestor(node, to));
                    let apex_top = apex.filter(|&apex| apex != node_count).map(|apex| top_of(&before.hangings, apex));
                    let top_turned = apex_top.is_some_and(|top| before.hangings[top] != read.hangings[top]);
                    reach.cut_then_apex_top_turned += usize::from(cuts > 0 && top_turned);
                    reach.cut += usize::from(cuts > 0);
                    reach.cut_twice += usize::from(cuts > 1);
                    reach.turned += usize::from(turns > 0);
                    reach.raised += usize::from(up_gained > 0);
                    reach.lowered += usize::from(up_gained < 0);
                }
                RouteStep::Restate(slot, lower_bound, capacity) if arc_count > 0 => {
                    let slot = slot.index(arc_count);
                    let arc = arc_of_slot[slot];
                    let flow = network.lower_bounds[arc].checked_add(basis.links.flows[slot]);
                    let flow =
                        flow.ok_or_else(|| TestCaseError::fail("an arc's flow should be within its capacity"))?;
                    let bounds = [lower_bound.bound(flow), capacity.bound(flow)];
                    basis = check_restatement(basis, &mut network, slot, bounds, &mut reach)?;
                }
                RouteStep::Restate(..) => {}
                RouteStep::Cost(slot, draw) if arc_count > 0 => {
                    let (slot, cost) = (slot.index(arc_count), draw.cost(bound).clamp(-bound, bound));
                    let before = snapshot(&basis);
                    let moved = basis.set_cost(Slot::from(slot), cost);
                    network.costs[arc_of_slot[slot]] = cost;
                    moved_checked(&before, &snapshot(&basis), moved.as_slice()).map_err(TestCaseError::fail)?;
                }
                RouteStep::Cost(..) => {}
                RouteStep::Pivot => {
                    if let Some(slot) = EnteringRule::FirstEligible.next(&basis.rows()) {
                        basis.pivot(slot);
                    }
                }
            }
            check_at(&basis, &network, widest).map_err(TestCaseError::fail)?;
        }

        Ok(reach)
    }

    /// The cost of the lower bounds of `network`: each arc's cost times its lower bound, added up, in big integers.
    fn lower_bound_cost(network: &MincostNetwork) -> BigInt {
        let arcs = 0..network.arc_count();

        arcs.map(|arc| BigInt::from(network.costs[arc]) * BigInt::from(network.lower_bounds[arc])).sum()
    }

    /// Restates the arc at `slot` of `basis`, a basis of `network` at the widest root cost of its node count, for the
    /// bounds `[lower_bound, capacity]` where the first is at most the second and the amount the restatement moves
    /// keeps the root links within the limit, and checks it: the whole invariant against `network` so changed, the cost
    /// of the flow, lower bounds and root links counted, moved by what it answers, modulo `2^128`, and the flow up the
    /// root links by what it answers, by at most the amount. Counts what the restatement was into `reach`.
    fn check_restatement<'r>(
        basis: Basis<'r>,
        network: &mut MincostNetwork,
        slot: usize,
        [lower_bound, capacity]: [u64; 2],
        reach: &mut RouteReach,
    ) -> Result<Basis<'r>, TestCaseError> {
        let widest = widest_root_cost(network.node_count());
        let arc = arcs_of_slots(network.arc_count(), network.node_count())[slot];
        let (before, up) = (snapshot(&basis), basis.unrouted());
        let (old_lower_bound, old_capacity) = (network.lower_bounds[arc], network.capacities[arc]);
        let Some(new_width) = capacity.checked_sub(lower_bound) else { return Ok(basis) };
        // The most a restatement moves, worked out from the test's own copy: the larger change of the two bounds.
        let largest = lower_bound.abs_diff(old_lower_bound).max(capacity.abs_diff(old_capacity));
        if u128::from(up) + u128::from(largest) > u128::from(NET_SUPPLY_LIMIT) {
            reach.past_the_limit += 1;
            return Ok(basis);
        }

        let width = old_capacity - old_lower_bound;
        let held = i128::from(before.flows[slot]) + i128::from(old_lower_bound) - i128::from(lower_bound);
        let hanging = before.hangings.iter().find(|hanging| hanging.link == slot).map(|hanging| hanging.direction);
        let crossed = lower_bound > old_capacity || capacity < old_lower_bound;
        let spent = extended_cost_at(&basis, network, widest) + lower_bound_cost(network);
        let (basis, read, [cost_gained, up_gained]) = restated(basis, network, widest, slot, [lower_bound, capacity]);

        let cost = extended_cost_at(&basis, network, widest) + lower_bound_cost(network) - spent - cost_gained;
        prop_assert_eq!(cost % (BigInt::from(1_u8) << 128), BigInt::from(0_u8));
        prop_assert_eq!(i128::from(basis.unrouted()) - i128::from(up), up_gained);
        prop_assert!(up_gained <= i128::from(largest), "the flow up gained {up_gained}, more than {largest}");

        let new_width_held = i128::from(new_width);
        match hanging {
            Some(direction) => {
                let stayed = read.hangings == before.hangings;
                reach.tree_arc_stayed += usize::from(stayed);
                reach.tree_arc_cut += usize::from(!stayed);
                reach.tree_arc_emptied += usize::from(new_width == 0);
                // A tree arc restated exactly onto a bound stays where it still has room toward the root.
                let at_capacity = held == new_width_held && new_width > 0;
                let at_lower_bound = held == 0 && new_width > 0;
                match direction {
                    Direction::Down if at_capacity => reach.tree_arc_down_at_capacity_stayed += usize::from(stayed),
                    Direction::Up if at_lower_bound => reach.tree_arc_up_at_lower_bound_stayed += usize::from(stayed),
                    Direction::Down if at_lower_bound => reach.tree_arc_down_at_lower_bound_cut += usize::from(!stayed),
                    Direction::Up if at_capacity => reach.tree_arc_up_at_capacity_cut += usize::from(!stayed),
                    Direction::Down | Direction::Up => {}
                }
                prop_assert!(!(at_capacity || at_lower_bound) || stayed == ((direction == UP) == at_lower_bound));
            }
            None => {
                let moved = read.flows[slot] != before.flows[slot] || held != i128::from(read.flows[slot]);
                reach.moved_out_of_the_tree += usize::from(moved);
                reach.unmoved_out_of_the_tree += usize::from(!moved);
                let at_lower = before.states[slot] == LOWER && lower_bound == old_lower_bound;
                reach.empty_arc_capacity_raised += usize::from(at_lower && capacity > old_capacity);
                reach.empty_arc_capacity_lowered += usize::from(at_lower && capacity < old_capacity);
            }
        }
        reach.widened_from_none += usize::from(width == 0 && new_width > 0);
        reach.crossed += usize::from(crossed);
        reach.restate_raised += usize::from(up_gained > 0);

        Ok(basis)
    }

    #[test]
    fn test_the_random_routes_reach_every_way_a_route_goes() {
        let mut runner = TestRunner::deterministic();
        let networks = || tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered);
        let mixes = [((networks(), route_and_cost_steps()).boxed(), 256), ((networks(), route_steps()).boxed(), 1024)];
        let mut drawn = Vec::new();
        for (strategy, count) in mixes {
            for _ in 0..count {
                drawn.push(strategy.new_tree(&mut runner).expect("a sequence should be drawn").current());
            }
        }

        let mut total = RouteReach::default();
        for (network, steps) in drawn {
            let reach = check_routes(network, &steps).expect("every step should keep the invariant");
            total.carried += reach.carried;
            total.cut += reach.cut;
            total.cut_twice += reach.cut_twice;
            total.turned += reach.turned;
            total.raised += reach.raised;
            total.lowered += reach.lowered;
            total.cut_up += reach.cut_up;
            total.cut_down += reach.cut_down;
            total.cut_without_rest += reach.cut_without_rest;
            total.empty_leg += reach.empty_leg;
            total.both_legs_carried += reach.both_legs_carried;
            total.end_a_top += reach.end_a_top;
            total.turned_twice += reach.turned_twice;
            total.cut_then_apex_top_turned += reach.cut_then_apex_top_turned;
            total.tree_arc_stayed += reach.tree_arc_stayed;
            total.tree_arc_cut += reach.tree_arc_cut;
            total.tree_arc_emptied += reach.tree_arc_emptied;
            total.moved_out_of_the_tree += reach.moved_out_of_the_tree;
            total.unmoved_out_of_the_tree += reach.unmoved_out_of_the_tree;
            total.widened_from_none += reach.widened_from_none;
            total.crossed += reach.crossed;
            total.restate_raised += reach.restate_raised;
            total.past_the_limit += reach.past_the_limit;
            total.tree_arc_down_at_capacity_stayed += reach.tree_arc_down_at_capacity_stayed;
            total.tree_arc_up_at_lower_bound_stayed += reach.tree_arc_up_at_lower_bound_stayed;
            total.tree_arc_down_at_lower_bound_cut += reach.tree_arc_down_at_lower_bound_cut;
            total.tree_arc_up_at_capacity_cut += reach.tree_arc_up_at_capacity_cut;
            total.empty_arc_capacity_raised += reach.empty_arc_capacity_raised;
            total.empty_arc_capacity_lowered += reach.empty_arc_capacity_lowered;
        }

        let counts = [
            total.carried,
            total.cut,
            total.cut_twice,
            total.turned,
            total.raised,
            total.lowered,
            total.cut_up,
            total.cut_down,
            total.cut_without_rest,
            total.empty_leg,
            total.both_legs_carried,
            total.end_a_top,
            total.turned_twice,
            total.cut_then_apex_top_turned,
            total.tree_arc_stayed,
            total.tree_arc_cut,
            total.tree_arc_emptied,
            total.moved_out_of_the_tree,
            total.unmoved_out_of_the_tree,
            total.widened_from_none,
            total.crossed,
            total.restate_raised,
            total.past_the_limit,
            total.tree_arc_down_at_capacity_stayed,
            total.tree_arc_up_at_lower_bound_stayed,
            total.tree_arc_down_at_lower_bound_cut,
            total.tree_arc_up_at_capacity_cut,
            total.empty_arc_capacity_raised,
            total.empty_arc_capacity_lowered,
        ];
        assert!(counts.iter().all(|&count| count > 0), "{total:?}");
    }

    proptest! {
        #[test]
        fn test_random_routes_costs_and_pivots_keep_the_invariant_at_the_widest_root_cost(
            network in tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered),
            steps in route_steps(),
        ) {
            check_routes(network, &steps)?;
        }
    }

    #[test]
    fn test_the_random_costs_and_pivots_reach_every_kind_of_arc_and_pivots() {
        let mut runner = TestRunner::deterministic();
        let strategy = (tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered), change_steps());
        let mut total = ChangeReach::default();
        for _ in 0..256 {
            let (network, steps) = strategy.new_tree(&mut runner).expect("a sequence should be drawn").current();
            let reach = check_costs_and_pivots(network, &steps).expect("every step should keep the invariant");
            total.up += reach.up;
            total.down += reach.down;
            total.at_capacity += reach.at_capacity;
            total.at_lower_or_without_width += reach.at_lower_or_without_width;
            total.pivots += reach.pivots;
        }

        let counts = [total.up, total.down, total.at_capacity, total.at_lower_or_without_width, total.pivots];
        assert!(counts.iter().all(|&count| count > 0), "{total:?}");
    }

    proptest! {
        #[test]
        fn test_random_costs_and_pivots_keep_the_invariant_at_the_widest_root_cost(
            network in tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered),
            steps in change_steps(),
        ) {
            check_costs_and_pivots(network, &steps)?;
        }
    }
}
