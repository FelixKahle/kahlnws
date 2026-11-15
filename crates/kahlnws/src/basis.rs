// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The columns of a run with the tree as one strongly feasible spanning-tree basis: the room they are kept in, the
//! basis a start loads into it, from the star or from the basis the last run ended on, re-flowed; the rows the pricing
//! reads, the pivot, the decision whether every supply is routed, the write-out of the flows, and the end of a run,
//! which keeps its basis in the room.

use core::hint::select_unpredictable;
use core::iter::{self, FusedIterator, Zip};
use core::mem;
use core::ops::Range;
use core::slice;

use std::collections::TryReserveError;

use itertools::izip;
use typed_index_collections::TiSlice;

use crate::problem::{CheckedArc, CheckedArcs, CheckedProblem, NodeId};
use crate::ranges::{narrow, widen};
use crate::room::{refill, reserve_total};
use crate::slot_order::{DealtSlotsIter, Slot};
use crate::starting_basis::{Excesses, KeptStart, RootLink, Start, root_link};
use crate::tree::{
    Cycle, CycleWalk, Direction, Exchange, LentTree, Link, PruneStep, PruneWalk, Side, TightenWalk, TreeRoom,
    root_link_cost,
};

/// The room a basis is kept in from one solve to the next: the arc and link columns and the room of the tree, holding
/// whatever the last basis left, and whether that is a whole basis.
///
/// It only grows. Nothing is read from it but through the [`Basis`] that [`load`](Self::load) writes into it, or that
/// [`resume`](Self::resume) re-flows in it, and lends, and by the decision of `resume`, which reads the kept ends and
/// states where the room holds a basis.
///
/// # Invariant
/// `held` guards the arc and link columns: where it is `Held::Basis`, they are those of the basis a run ended on, as
/// [`Basis::keep`] left them, beside the tree that keep left kept in the tree's room, which guards its own columns.
/// Every load and every resume takes it out before its first write to a column, and only [`Basis::keep`] puts it in.
#[derive(Clone, Debug, Default)]
pub(crate) struct BasisRoom {
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
    /// Whether the arc and link columns hold the basis a run ended on.
    held: Held,
}

/// What a basis room's arc and link columns hold between runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Held {
    /// No whole basis: before the first run's end, and from the first write of a load or a resume until its run ends.
    #[default]
    Nothing,
    /// The basis the last run ended on.
    Basis,
}

/// The basis a resume lends, by the start it was made from.
#[derive(Debug)]
pub(crate) enum Resumed<'r, 'p> {
    /// The basis the last run ended on, re-flowed for the problem.
    Kept(Basis<'r, 'p>),
    /// The star, as [`BasisRoom::load`] makes it: the room held no basis, or the one it held cannot serve the problem.
    Star(Basis<'r, 'p>),
}

/// A strongly feasible spanning-tree basis of one problem, extended by a root and one root link per node, loaded into
/// the room it borrows for `'r`: what a run pivots on, from its start to its outcome.
///
/// The basis works on the problem with its lower bounds taken out: an arc of width `capacity − lower bound`, carrying
/// flow above its lower bound, and each node's supply net of the lower bounds. Each node also has a root link that
/// joins it to the root, of capacity `u64::MAX`, up from the node at cost zero or down to it at the root cost `M`. A
/// link is an arc at its slot, or node `v`'s root link at `m + v`. Every potential is the tree node's, the root's last.
///
/// # Invariant
/// With `n` nodes and `m` arcs in the problem it holds:
/// - *Shape.* The arc columns hold `m` slots, the arc dealt to slot `s` at `s`, with its ends, both below `n`, and its
///   cost; the link columns hold `m + n` links, an arc's capacity being its width; the tree has `n` hung nodes,
///   `n + 1` potentials, the root's last, and `m + n` links.
/// - *Tree.* An arc is in the tree exactly where it is some node's link; it then has the state `Unpriced`. Each hung
///   node's link joins it to its parent, the way its direction says: an arc's tail and head are the node and its
///   parent, so a node that hangs by an arc hangs from a node below `n`, never from the root; a root link joins its
///   own node and the root.
/// - *Duality.* The root's potential is zero and every tree link is tight: its cost plus its tail's potential less its
///   head's is zero. A root link costs zero where it points up and `M` where it points down; the start sets which way
///   a root link in the tree points, and no pivot changes it.
/// - *Flow.* Every node conserves its net supply, root links counted; every flow lies within `0..=capacity`; an arc out
///   of the tree carries nothing (`AtLower`), its width (`AtUpper`), or has no width (`Unpriced`); a root link out of
///   the tree carries nothing.
/// - *Strong feasibility.* Every tree link can carry more flow toward the root: a link that runs up from its node
///   carries less than its capacity, one that runs down to it more than nothing.
///
/// [`BasisRoom::load`] and [`BasisRoom::resume`] establish it; [`pivot`](Self::pivot) keeps it; [`keep`](Self::keep)
/// ends the run on it and leaves it in the room.
#[derive(Debug)]
pub(crate) struct Basis<'r, 'p> {
    /// What the pricing reads of each arc, and its state.
    arcs: ArcColumns<'r>,
    /// What each link can carry and carries.
    links: LinkColumns<'r>,
    /// The tree, with each node's potential, lent by the tree's room.
    tree: LentTree<'r>,
    /// The problem the basis was loaded from, which the write-out reads again: kept here so that no other problem of
    /// the same arc count can be written out with these flows.
    problem: CheckedProblem<'p>,
    /// What the room holds between runs: nothing while the basis is lent, the basis once [`keep`](Self::keep) ends
    /// the run.
    held: &'r mut Held,
}

/// What the pricing reads of each arc of a loaded basis, in slot order, with the arcs' states, which a pivot writes.
///
/// # Invariant
/// The three columns hold one entry per slot, `m` each, and every end is below the problem's node count `n`
/// ([`Ends`]).
#[derive(Debug)]
struct ArcColumns<'r> {
    /// Per slot: the arc's tail and head.
    ends: &'r [Ends],
    /// Per slot: the arc's cost, in units of cost.
    costs: &'r [i64],
    /// Per slot: where the arc stands.
    states: &'r mut [ArcState],
}

/// What each link of a loaded basis can carry and carries: the arcs at their slots, then each node's root link.
///
/// # Invariant
/// Both columns hold exactly as many entries as the tree beside them in the [`Basis`] or the [`Reflow`] has links,
/// `m + n`, so every [`Link`] of that tree indexes them; and no other tree's link ever does: this file reads links
/// only from the tree held beside them, the one they were loaded or re-flowed with, and a link does not outlive that
/// tree's borrow. Since `m` is at most `m + n`, every [`ArcSlot`] indexes them too.
#[derive(Debug)]
struct LinkColumns<'r> {
    /// Per link: the most it carries, in units of flow: an arc's width, `u64::MAX` for a root link.
    capacities: &'r [u64],
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
/// Only [`BasisRoom::scatter`] writes one into a room's ends column, from an arc of the [`CheckedProblem`] a load or a
/// re-flow places: both ends lie below that problem's node count `n`, so below the `n + 1` potentials of the tree
/// beside them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ends {
    /// The node the arc leaves.
    tail: u32,
    /// The node the arc enters.
    head: u32,
}

/// Where an arc stands: one of three states, an enum of one byte.
///
/// Its discriminant is chosen as its pricing sign, so that the sign is the byte read, widened: the arc violates its
/// bound exactly where the sign times its reduced cost is negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i8)]
enum ArcState {
    /// Out of the tree, at its capacity: it violates where its reduced cost is positive.
    AtUpper = -1,
    /// In the tree, or of no width: no pivot moves it.
    Unpriced = 0,
    /// Out of the tree, carrying nothing above its lower bound: it violates where its reduced cost is negative.
    AtLower = 1,
}

/// The bound an entering arc leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    /// It carries nothing above its lower bound: the pivot raises its flow.
    Lower,
    /// It carries its width: the pivot lowers its flow.
    Upper,
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

/// The entering arc as the link of the end inside the subtree an exchange moves: which way it runs from that end, and
/// the shift that makes it tight there. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EnteringLink {
    /// Which way the entering arc runs, seen from that end.
    direction: Direction,
    /// What every potential of the moved subtree gains, in units of cost.
    shift: i64,
}

/// What one link of a loaded basis carries and the most it carries, read together. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LinkFill {
    /// What it carries, in units of flow above the arc's lower bound.
    flow: u64,
    /// The most it carries, in units of flow: an arc's width, `u64::MAX` for a root link.
    capacity: u64,
}

/// The ends and the states of a held basis, of the counts of one problem: what is read of the kept basis to decide
/// whether it can serve that problem, and what its arcs out of the tree carry.
///
/// # Invariant
/// Only [`BasisRoom::held_arcs`] makes one, from the columns of a room that holds a whole basis: `ends` and `states`
/// hold one entry per arc of `problem`, `m` each, so every slot a [`DealtSlotsIter`] deals for `m` arcs indexes them.
#[derive(Clone, Copy, Debug)]
struct HeldArcs<'r, 'p> {
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
/// What it proves: only [`BasisRoom::kept_start`] makes one, and only after it found, in the room it read, that the
/// tree room keeps a tree of the problem's `n` hung nodes and `m + n` links, that the arc columns hold `m` slots and
/// the link columns `m + n` links, and that every slot holds the ends of the problem's arc dealt to it; and `start`
/// holds the problem's excesses over that basis, within the limit, which [`KeptStart`] proves of itself.
///
/// What remains this file's discipline, since the value holds no borrow of the room: it is handed to the `reflow` of
/// the same room, with nothing written to that room in between. [`BasisRoom::resume`], its one maker's one caller,
/// makes the two calls one after the other.
#[derive(Debug)]
struct ServingStart<'p, 'e> {
    /// The problem with its excesses.
    start: KeptStart<'p, 'e>,
}

/// The columns of a held basis while the tree beside them is re-flowed for a new problem: the tree, and what the
/// re-flow writes besides it.
///
/// # Invariant
/// Only [`BasisRoom::reflow`] makes one, from a [`ServingStart`], with the problem's arcs just placed, which keeps
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

/// How a tree arc stands once its node's excess is sent along it toward the root. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TreeArcFlow {
    /// What the arc carries, in units of flow above its lower bound: within `0..=width`.
    flow: u64,
    /// What the excess of the node's parent gains, in units of flow: the flow where the arc runs up from the node, its
    /// negation where it runs down to it.
    to_parent: i128,
    /// Whether the arc stays in the tree.
    standing: Standing,
}

/// Whether a tree arc stays in the tree once its node's excess is sent along it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Standing {
    /// It stays, carrying all of the node's excess, and can carry more toward the root.
    InTree,
    /// It leaves the tree at `state`, a bound, and its node hangs from the root by `root_link`, which carries what the
    /// arc could not.
    Leaves {
        /// Where the arc stands out of the tree.
        state: ArcState,
        /// The node's root link.
        root_link: RootLink,
    },
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
        Self {
            ends: Vec::new(),
            costs: Vec::new(),
            states: Vec::new(),
            capacities: Vec::new(),
            flows: Vec::new(),
            tree: TreeRoom::new(),
            held: Held::Nothing,
        }
    }

    /// Makes room for the basis of a problem of up to `node_count` nodes and `arc_count` arcs: a load of such a
    /// problem, and every pivot of its basis, allocates nothing afterwards. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    pub(crate) fn try_reserve(&mut self, node_count: usize, arc_count: usize) -> Result<(), TryReserveError> {
        let link_count = arc_count.saturating_add(node_count);

        reserve_total(&mut self.ends, arc_count)?;
        reserve_total(&mut self.costs, arc_count)?;
        reserve_total(&mut self.states, arc_count)?;
        reserve_total(&mut self.capacities, link_count)?;
        reserve_total(&mut self.flows, link_count)?;
        self.tree.try_reserve(node_count)
    }

    /// The star of `start`, written into this room and lent from it as a basis: every arc at its dealt slot with its
    /// width, out of the tree and carrying nothing, `AtLower`, or `Unpriced` where it has no width; every node hanging
    /// from the root by its root link, carrying the magnitude of its net supply, at potential zero below a link that
    /// points up and the root cost below one that points down. Nothing of an earlier basis is kept but the room: a
    /// basis the room held is taken out before the first write.
    ///
    /// # Panics
    /// On a target whose `usize` has 32 bits it can panic while it refills a column, where the column's room passes
    /// `isize::MAX` bytes: the flows take `8·(m + n)`.
    pub(crate) fn load<'p>(&mut self, start: &Start<'p, '_>) -> Basis<'_, 'p> {
        let problem = start.problem();

        self.held = Held::Nothing;
        self.place_links(start);
        self.place_arcs(problem);

        let Self { ends, costs, states, capacities, flows, tree, held } = self;
        let directions = start.root_links().map(|root_link| root_link.direction);
        // A checked problem keeps `n + 1` and `m + n` within the index limit, and the start gives one root link a
        // node: the star does not panic.
        let tree = tree.star(problem.node_count(), problem.arc_count(), directions, problem.root_cost());

        Basis {
            arcs: ArcColumns { ends, costs, states },
            links: LinkColumns { capacities, flows },
            tree,
            problem,
            held,
        }
    }

    /// The basis of `start` from the basis the last run in this room ended on, re-flowed, where the room holds one of
    /// the problem's node and arc counts and of the same tail and head at every arc, and the excesses over it keep
    /// the limit of the start's net supplies; otherwise the star, as [`load`](Self::load) makes it.
    ///
    /// The re-flow keeps the tree and where each arc out of it stands, and reads every number anew from the problem:
    /// an arc out of the tree carries its new width at its capacity and nothing at its lower bound; each node's excess
    /// is sent toward the root along its link, from the leaves up, and a link that cannot carry it, or could carry no
    /// more toward the root, leaves the tree for its node's root link; every potential is made tight from the root
    /// down. It costs a few passes over the arcs and the nodes, O(m + n), and allocates nothing where `excesses`
    /// holds room for the problem's nodes. Either way the room holds no basis until the run ends.
    ///
    /// # Panics
    /// As [`load`](Self::load), and on such a target where `excesses` grows past `isize::MAX` bytes: they take `16·n`.
    pub(crate) fn resume<'p>(&mut self, start: &Start<'p, '_>, excesses: &mut Excesses) -> Resumed<'_, 'p> {
        if mem::take(&mut self.held) == Held::Nothing {
            return Resumed::Star(self.load(start));
        }

        match self.kept_start(start, excesses) {
            Some(serving) => Resumed::Kept(self.reflow(serving)),
            None => Resumed::Star(self.load(start)),
        }
    }

    /// The start of the problem of `start` over the kept basis, written into `excesses`, where the room holds that
    /// basis, of the problem's counts and ends, and the excesses keep the limit; `None` otherwise. It writes nothing
    /// but `excesses`.
    fn kept_start<'p, 'e>(&self, start: &Start<'p, '_>, excesses: &'e mut Excesses) -> Option<ServingStart<'p, 'e>> {
        let held = self.held_arcs(start.problem()).filter(HeldArcs::are_of_problem)?;

        // Each arc out of the tree carries what its kept state gives it of its new width; a tree arc carries nothing
        // here, and the re-flow of the tree writes it.
        let start = match start.problem().arcs() {
            CheckedArcs::WithoutLowerBounds(arcs) => excesses.gather(start, held.carried(arcs)),
            CheckedArcs::WithLowerBounds(arcs) => excesses.gather(start, held.carried(arcs)),
        }?;

        // `held_arcs` compared the kept tree's counts and every column's length with the problem's counts, and
        // `are_of_problem` every slot's ends: `ServingStart`'s invariant.
        Some(ServingStart { start })
    }

    /// The ends and states of the basis this room holds, where the tree its tree room keeps is of the counts of
    /// `problem`, `n` nodes and `m` arcs, and the arc and link columns hold them; `None` otherwise.
    fn held_arcs<'p>(&self, problem: CheckedProblem<'p>) -> Option<HeldArcs<'_, 'p>> {
        let (node_count, arc_count) = (problem.node_count(), problem.arc_count());
        let kept = self.tree.kept()?;
        // A checked problem keeps `m + n` within the index limit: the sum does not overflow.
        let link_count = arc_count + node_count;
        let counts = [kept.node_count, kept.link_count, self.ends.len(), self.states.len()];
        let links = [self.capacities.len(), self.flows.len()];

        (counts == [node_count, link_count, arc_count, arc_count] && links == [link_count, link_count])
            .then_some(HeldArcs { ends: &self.ends, states: &self.states, problem })
    }

    /// The basis `serving` begins from: the basis this room holds, which serves its problem, re-flowed.
    fn reflow<'p>(&mut self, serving: ServingStart<'p, '_>) -> Basis<'_, 'p> {
        let ServingStart { start } = serving;
        let problem = start.problem();

        // The arcs' pass: every arc at its slot with its new ends, cost and width, carrying what its kept state gives
        // it. The ends are the kept ones, so the tree's links join the same nodes as before.
        self.place_arcs(problem);

        let Self { ends, costs, states, capacities, flows, tree, held } = self;
        // The tree room keeps a tree (`ServingStart`): the resume does not panic.
        let tree = tree.resume();
        let columns = ReflowColumns {
            arcs: ArcColumns { ends, costs, states },
            links: LinkColumns { capacities, flows },
            excesses: start.into_excesses(),
        };
        let Reflow { tree, columns: ReflowColumns { arcs, links, .. } } =
            Reflow { tree, columns }.flow_up().tighten(problem.root_cost());

        Basis { arcs, links, tree, problem, held }
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

impl<'r> Basis<'r, '_> {
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

    /// Whether every root link carries nothing: then the arcs' flows, their lower bounds added, meet every supply.
    /// Where no arc violates its bound, it is false exactly where no flow within the bounds meets the supplies.
    pub(crate) fn routes_every_supply(&self) -> bool {
        // Where no arc violates and a root link carries flow, no flow within the bounds meets the supplies. Suppose one
        // does, `x*`, and a root link that points down, into `d`, carries flow in the basis's `x`. The difference
        // `x* − x` is a circulation of the extended network that only lowers root-link flows, so it decomposes into
        // cycles conformal to it, one of which enters the root by lowering the flow down `d`'s link and leaves it by
        // lowering the flow up some `s`'s link, which points up; the rest of it is a simple path from `s` to `d` of at
        // most `n − 1` arcs, so its cost is at most `−M + (n − 1)·C = −1` (`root_cost`). Every step of it can move in
        // `x`: each arc out of the tree stands at the bound it moves away from and does not violate, so its reduced
        // cost in the step's direction is at least zero; each tree link, the two root links among them, which carry
        // flow, is tight. So the cycle's cost, the sum of its steps' reduced costs, is at least zero: a contradiction.
        // Every root link that points down carries nothing, and with the root conserving, every one that points up.
        // No property of the start is used: which way each root link points is whatever the start set.
        //
        // Where every root link carries nothing, the arcs' flows meet the net supplies, and with the lower bounds
        // added the supplies. No arc of positive width violates, each tree arc is tight, and an arc of no width
        // carries its lower bound in every flow within the bounds: so the arcs meet the optimality conditions of the
        // problem itself under the potentials of the first `n` nodes, and the flow is of least cost. Nothing is claimed
        // of the root links out of the tree, which are never priced.
        self.links.flows.iter().skip(self.arcs.len()).all(|&flow| flow == 0)
    }

    /// Writes each arc's flow, its lower bound included, to `flows`, arc `a` at `a`, and answers the cost of that
    /// flow, in units of cost times units of flow. The basis is unchanged.
    ///
    /// # Panics
    /// Panics if `flows` holds another number of entries than the problem has arcs.
    #[track_caller]
    pub(crate) fn write_flows(&self, flows: &mut [u64]) -> i128 {
        let (length, arc_count) = (flows.len(), self.problem.arc_count());
        assert!(
            length == arc_count,
            "called `Basis::write_flows` with {length} flows, which should be the arc count {arc_count}"
        );

        match self.problem.arcs() {
            CheckedArcs::WithoutLowerBounds(arcs) => self.gather(arcs, flows),
            CheckedArcs::WithLowerBounds(arcs) => self.gather(arcs, flows),
        }
    }

    /// Ends the run on this basis, optimal or infeasible alike: the room keeps it whole for the next
    /// [`resume`](BasisRoom::resume), and each node's potential, in units of cost, node `v` at `v`, is lent for as
    /// long as the room is: what a run answers when it is over.
    ///
    /// # Panics
    /// Panics if the tree holds no potential at all, which its `n + 1` potentials rule out (`Basis`, Shape).
    pub(crate) fn keep(self) -> &'r [i64] {
        let potentials = self.tree.keep();
        *self.held = Held::Basis;
        let (_root, nodes) = potentials.split_last().expect("a tree should hold its root's potential");

        nodes
    }

    /// Writes the flow of each arc of `arcs`, the arcs of the loaded problem in ascending order, read at its dealt
    /// slot, plus its lower bound, to `flows`, arc `a` at `a`; answers the cost of those flows.
    fn gather(&self, arcs: impl Iterator<Item = CheckedArc>, flows: &mut [u64]) -> i128 {
        let slots = DealtSlotsIter::new(self.problem.arc_count(), self.problem.node_count());
        let mut total = 0_i128;

        for (arc, slot, written) in izip!(arcs, slots, flows) {
            let at = usize::from(slot);
            debug_assert!(at < self.links.flows.len(), "a dealt slot should be below the link count");
            // SAFETY: `DealtSlotsIter` yields only slots below the arc count `m` it was made for (its invariant), the
            // loaded problem's, and the link columns hold that problem's `m + n` links (`Basis`, Shape).
            let flow = unsafe { *self.links.flows.get_unchecked(at) };

            // A flow is at most the arc's width, so with its lower bound it is at most its capacity. Each term is
            // within the arc's cost magnitude times its capacity, and the problem's check bounds the sum of those by
            // `i128::MAX` (`cost_total_with`): no sum passes it.
            *written = flow + arc.lower_bound();
            total += i128::from(arc.cost()) * i128::from(*written);
        }

        total
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

impl HeldArcs<'_, '_> {
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
            // problem's, and the ends hold `m` entries (`HeldArcs`).
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
            // problem's, and the states hold `m` entries (`HeldArcs`).
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

impl ArcState {
    /// Where an arc out of the tree of width `width` stands that carries `flow`, nothing or its whole width: unpriced
    /// where it has no width, at its lower bound where it carries nothing, and at its capacity where it carries its
    /// width.
    fn at(flow: u64, width: u64) -> Self {
        debug_assert!(flow == 0 || flow == width, "an arc out of the tree should carry nothing or its width");

        // An arc of no width carries its lower bound in every flow within the bounds: a constant of the problem,
        // already counted in the net supplies. No basis needs it: every node has its root link, so a spanning tree
        // exists without it, and in a strongly feasible tree it could not stand, since it can carry no flow toward the
        // root. At the end its reduced cost may have either sign: no step can move its flow, so it meets the
        // optimality conditions either way.
        match (flow, width) {
            (_, 0) => Self::Unpriced,
            (0, _) => Self::AtLower,
            _ => Self::AtUpper,
        }
    }

    /// What an arc out of the tree that stands here carries of its width `width`, in units of flow: its width where it
    /// stands at its capacity, nothing otherwise.
    fn carried(self, width: u64) -> u64 {
        match self {
            Self::AtUpper => width,
            Self::Unpriced | Self::AtLower => 0,
        }
    }

    /// The violation of an arc that stands here at the reduced cost `reduced_cost`: the pricing sign times it.
    fn violation(self, reduced_cost: i64) -> i64 {
        // The sign is the discriminant, so the match is the byte read, widened. A reduced cost is below `2^63` in
        // magnitude (`cost_bound`), so its negation is exact.
        let sign = match self {
            Self::AtUpper => -1,
            Self::Unpriced => 0,
            Self::AtLower => 1,
        };

        sign * reduced_cost
    }

    /// The bound an arc that stands here leaves when it enters; `None` where it is not priced.
    fn bound(self) -> Option<Bound> {
        match self {
            Self::AtLower => Some(Bound::Lower),
            Self::AtUpper => Some(Bound::Upper),
            Self::Unpriced => None,
        }
    }
}

impl Bound {
    /// The other bound.
    fn other(self) -> Self {
        match self {
            Self::Lower => Self::Upper,
            Self::Upper => Self::Lower,
        }
    }

    /// The state of an arc out of the tree at this bound.
    fn state(self) -> ArcState {
        match self {
            Self::Lower => ArcState::AtLower,
            Self::Upper => ArcState::AtUpper,
        }
    }
}

impl Ends {
    /// The tail and the head of `arc`.
    fn of(arc: CheckedArc) -> Self {
        // A checked arc's ends are below `n`, and `n + 1` is at most `u32::MAX`: neither narrowing can fail.
        Self { tail: narrow(usize::from(arc.tail())), head: narrow(usize::from(arc.head())) }
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

/// The entering arc, of reduced cost `reduced_cost`, that left `bound`, as the link of the end of the cycle's `side`
/// once an exchange moves the subtree holding it: which way it runs seen from that end, and the shift that makes it
/// tight.
fn entering_link(bound: Bound, side: Side, reduced_cost: i64) -> EnteringLink {
    // The first end is the tail at the lower bound and the head at the capacity, and the moved subtree holds the end
    // of the leaving link's side: so it holds the tail exactly at the first side from the lower bound and at the
    // second from the capacity. The arc is tight once `cost + π(tail) − π(head)` is zero: a shift `δ` of the subtree
    // adds `δ` to it where the subtree holds the tail and subtracts `δ` where it holds the head.
    match (bound, side) {
        (Bound::Lower, Side::First) | (Bound::Upper, Side::Second) => {
            EnteringLink { direction: Direction::Up, shift: -reduced_cost }
        }
        (Bound::Lower, Side::Second) | (Bound::Upper, Side::First) => {
            EnteringLink { direction: Direction::Down, shift: reduced_cost }
        }
    }
}

/// The room of a link of capacity `capacity` that carries `flow` and runs `direction` seen from the node below it,
/// where the cycle's flow runs `flow_runs` along the tree there: its capacity less its flow where the two agree, its
/// flow where they do not.
fn room(capacity: u64, flow: u64, direction: Direction, flow_runs: Direction) -> u64 {
    // Both are formed and one is selected. A flow lies within `0..=capacity` (`Basis`), so neither underflows; which
    // way a link runs is data, one way as often as the other, so a branch on it would be mispredicted half the time.
    select_unpredictable(direction == flow_runs, capacity - flow, flow)
}

/// The flow of a link that carries `flow` and runs `direction` seen from the node below it, once `step` is sent along
/// the cycle where its flow runs `flow_runs` along the tree there: `step` more where the two agree, `step` less where
/// they do not.
fn sent(flow: u64, step: u64, direction: Direction, flow_runs: Direction) -> u64 {
    // The step is at most the link's room (`least_room`), so neither passes the link's bounds.
    if direction == flow_runs { flow + step } else { flow - step }
}

/// How a tree arc of width `width`, running `direction` seen from its node, stands once the node's excess `excess` is
/// sent along it toward the root: it carries as much of the excess as it can, `g`, the excess where the arc runs up
/// from the node and its negation where it runs down to it, clamped to `0..=width`; the parent gains `g` or `−g`. The
/// arc stays where it carries the whole excess and can carry more toward the root, less than its width running up and
/// more than nothing running down. Otherwise it leaves at the bound `g` stands at, and the node's root link takes the
/// rest of the excess, as [`root_link`] gives it.
///
/// # Panics
/// Panics if the rest of the excess passes `u64::MAX` in magnitude, which no excess of a start within the limit
/// leaves (`Reflow::flow_up`).
fn resend(excess: i128, width: u64, direction: Direction) -> TreeArcFlow {
    // Within `±P` the negation is exact; past it the strict form states the bound.
    let (flow, to_parent, rest, room) = match direction {
        Direction::Up => {
            let flow = within_width(excess, width);
            (flow, i128::from(flow), excess - i128::from(flow), flow < width)
        }
        Direction::Down => {
            let flow = within_width(excess.strict_neg(), width);
            (flow, -i128::from(flow), excess + i128::from(flow), flow > 0)
        }
    };

    // A rest other than zero means the clamp met a bound; no room toward the root means the flow stands at the bound
    // beyond which the arc has none. Either way the arc carries nothing or its width, and stands where `at` puts it.
    let standing = if rest == 0 && room {
        Standing::InTree
    } else {
        Standing::Leaves { state: ArcState::at(flow, width), root_link: root_link(rest) }
    };

    TreeArcFlow { flow, to_parent, standing }
}

/// What an arc of width `width` carries of `demand`, in units of flow: nothing where it is zero or less, the demand
/// up to the width otherwise.
fn within_width(demand: i128, width: u64) -> u64 {
    match u64::try_from(demand) {
        Ok(demand) => demand.min(width),
        Err(_) if demand < 0 => 0,
        Err(_) => width,
    }
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
pub(crate) mod tests {
    use std::collections::BTreeSet;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use kahlnws_testing::{
        MincostNetwork, OptimalFlow, TinyBounds, TinyCosts, TinyMincostNetworks, TinySupplies, brute_force_optimum,
        check_against_brute_force, check_certificate, tiny_networks,
    };
    use num_bigint::BigInt;
    use proptest::prelude::*;
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;
    use rand::{RngExt, SeedableRng};
    use rand_xoshiro::Xoshiro256PlusPlus;

    use super::*;
    use crate::problem::Problem;
    use crate::problem::tests::problem_of;
    use crate::ranges::NET_SUPPLY_LIMIT;
    use crate::slot_order::tests::arcs_of_slots;
    use crate::starting_basis::NetSupplies;
    use crate::tree::{PlainHanging, check_threading, cycle_walk, hangings_of};

    const UP: Direction = Direction::Up;
    const DOWN: Direction = Direction::Down;
    const LOWER: ArcState = ArcState::AtLower;
    const UPPER: ArcState = ArcState::AtUpper;
    const IN_TREE: ArcState = ArcState::Unpriced;

    /// The depot: node 0 sends 4 units to node 2, directly by arc 0 (capacity 3, cost 5) or through node 1 by arcs 1
    /// (capacity 4, cost 1) and 2 (capacity 2, cost 1).
    fn depot() -> MincostNetwork {
        MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1)])
    }

    /// The basis of `problem` loaded into `room`.
    pub(crate) fn loaded<'r, 'p>(room: &'r mut BasisRoom, problem: &Problem<'p>) -> Basis<'r, 'p> {
        let problem = problem.checked().expect("the test's problem should be admitted");
        let mut net_supplies = NetSupplies::new();
        let start = net_supplies.gather(problem).expect("the test's net supplies should be in range");

        room.load(&start)
    }

    /// The star of `network`, its lower bounds given, loaded into `room` with its whole invariant checked.
    ///
    /// # Panics
    /// Panics if the invariant fails after the load.
    fn checked_star<'r, 'n>(room: &'r mut BasisRoom, network: &'n MincostNetwork) -> Basis<'r, 'n> {
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
    fn snapshot(basis: &Basis<'_, '_>) -> Snapshot {
        let rows = basis.rows();

        Snapshot {
            flows: basis.links.flows.to_vec(),
            potentials: basis.tree.potentials().to_vec(),
            states: basis.arcs.states.to_vec(),
            hangings: hangings_of(&basis.tree),
            violations: rows.violations(0..rows.len()).map(|(_, violation)| violation).collect(),
        }
    }

    /// Pivots `basis` of `network` on the arc at `slot`, checks the whole invariant, and answers the basis read out.
    ///
    /// # Panics
    /// Panics if the invariant fails after the pivot.
    fn pivoted(basis: &mut Basis<'_, '_>, network: &MincostNetwork, slot: usize) -> Snapshot {
        basis.pivot(Slot::from(slot));
        if let Err(breach) = check(basis, network) {
            panic!("the invariant should hold after the pivot of slot {slot}: {breach}");
        }

        snapshot(basis)
    }

    /// The flows the basis writes out, with their cost.
    fn written(basis: &Basis<'_, '_>) -> (Vec<u64>, i128) {
        let mut flows = vec![0; basis.problem.arc_count()];
        let cost = basis.write_flows(&mut flows);

        (flows, cost)
    }

    /// What the invariant check recomputes from a network apart from the basis. Plain data.
    struct Extended {
        /// Per slot: the arc dealt to it.
        arc_of_slot: Vec<usize>,
        /// Per node: its supply, less the lower bounds of the arcs leaving it, plus those of the arcs entering it.
        net_supplies: Vec<i128>,
        /// The cost of a root link that points down: `(n − 1)·C + 1`.
        root_cost: i128,
    }

    impl Extended {
        /// What `network` extends to.
        fn of(network: &MincostNetwork) -> Self {
            let (node_count, arc_count) = (network.node_count(), network.arc_count());
            let arc_of_slot = arcs_of_slots(arc_count, node_count);

            let mut net_supplies: Vec<i128> = network.supplies.iter().map(|&supply| i128::from(supply)).collect();
            for arc in 0..arc_count {
                net_supplies[network.tails[arc]] -= i128::from(network.lower_bounds[arc]);
                net_supplies[network.heads[arc]] += i128::from(network.lower_bounds[arc]);
            }

            let largest = network.costs.iter().map(|&cost| i128::from(cost).abs()).max().unwrap_or(0);
            let paths = i128::try_from(node_count.saturating_sub(1)).expect("a test's node count should fit");

            Self { arc_of_slot, net_supplies, root_cost: paths * largest + 1 }
        }

        /// The tail, head and cost of every link of `network` in a basis whose nodes hang as `hangings` says: an
        /// arc's at its slot; a root link's as its node hangs by it, up at cost zero or down at the root cost; and a
        /// root link out of the tree up, which matters to nothing, since it carries nothing.
        fn links(&self, network: &MincostNetwork, hangings: &[PlainHanging]) -> Vec<(usize, usize, i128)> {
            let (node_count, arc_count) = (network.node_count(), network.arc_count());
            let arcs = self
                .arc_of_slot
                .iter()
                .map(|&arc| (network.tails[arc], network.heads[arc], i128::from(network.costs[arc])));
            let root_links = (0..node_count).map(|node| {
                let in_tree = hangings.get(node).filter(|hanging| hanging.link == arc_count + node);
                match in_tree.map(|hanging| hanging.direction) {
                    Some(Direction::Down) => (node_count, node, self.root_cost),
                    Some(Direction::Up) | None => (node, node_count, 0),
                }
            });

            arcs.chain(root_links).collect()
        }

        /// What `link` of `network` can carry: an arc's width, `u64::MAX` for a root link.
        fn capacity(&self, network: &MincostNetwork, link: usize) -> u64 {
            match self.arc_of_slot.get(link) {
                Some(&arc) => network.capacities[arc] - network.lower_bounds[arc],
                None => u64::MAX,
            }
        }
    }

    /// Checks every clause of the basis invariant of `basis`, loaded from `network`, recomputed from the network and
    /// the columns by plain indexing; and that its rows price every slot as the definition does. Answers the first
    /// breach.
    pub(crate) fn check(basis: &Basis<'_, '_>, network: &MincostNetwork) -> Result<(), String> {
        let model = Extended::of(network);
        check_shape(basis, network, &model)?;
        let links = model.links(network, &hangings_of(&basis.tree));
        let in_tree = check_tree(basis, network, &links)?;
        check_duality(basis, network, &links)?;
        check_flow(basis, network, &model, &links, &in_tree)?;
        check_strong_feasibility(basis)?;

        check_rows(basis, network, &model)
    }

    /// Shape: the lengths of every column, each slot's ends and cost and each link's capacity.
    fn check_shape(basis: &Basis<'_, '_>, network: &MincostNetwork, model: &Extended) -> Result<(), String> {
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
        basis: &Basis<'_, '_>,
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

    /// Duality: the root at zero and every tree link tight.
    fn check_duality(
        basis: &Basis<'_, '_>,
        network: &MincostNetwork,
        links: &[(usize, usize, i128)],
    ) -> Result<(), String> {
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
        basis: &Basis<'_, '_>,
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
    fn check_strong_feasibility(basis: &Basis<'_, '_>) -> Result<(), String> {
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
    fn check_rows(basis: &Basis<'_, '_>, network: &MincostNetwork, model: &Extended) -> Result<(), String> {
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
        fn of(basis: &Basis<'_, '_>, network: &MincostNetwork, slot: Slot) -> Self {
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
        fn check_step(&self, basis: &Basis<'_, '_>, network: &MincostNetwork, step: u64) -> Result<(), String> {
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
    fn extended_cost(basis: &Basis<'_, '_>, network: &MincostNetwork) -> BigInt {
        let links = Extended::of(network).links(network, &hangings_of(&basis.tree));

        basis.links.flows.iter().zip(links).map(|(&flow, (.., cost))| BigInt::from(flow) * cost).sum()
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
    fn pivot_bound(basis: &Basis<'_, '_>, network: &MincostNetwork) -> u128 {
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
        let sending = u128::try_from(sending).expect("a basis costs at least the least the arcs can cost");
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
        basis: Basis<'r, 'n>,
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
        room: &'r mut BasisRoom,
        excesses: &mut Excesses,
        problem: &Problem<'p>,
    ) -> (Basis<'r, 'p>, Begun) {
        let problem = problem.checked().expect("the test's problem should be admitted");
        let mut net_supplies = NetSupplies::new();
        let start = net_supplies.gather(problem).expect("the test's net supplies should be in range");

        match room.resume(&start, excesses) {
            Resumed::Kept(basis) => (basis, Begun::Kept),
            Resumed::Star(basis) => (basis, Begun::Star),
        }
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
        /// # Errors
        /// Returns the breach of the invariant in the run of `before`, or after the resume.
        ///
        /// # Panics
        /// Panics if either problem is refused.
        pub(crate) fn resumed(
            room: &'r mut BasisRoom,
            excesses: &mut Excesses,
            before: &MincostNetwork,
            network: &'n MincostNetwork,
            problem: &Problem<'n>,
        ) -> Result<(Self, Begun), String> {
            let mut rule = EnteringRule::FirstEligible;
            CheckedRun::start(&mut *room, before, &problem_of(before))?
                .run_to_end(|rows| rule.next(rows))
                .map_err(|breach| format!("before the resume: {breach}"))?;

            let (basis, begun) = resumed_in(room, excesses, problem);
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

        assert!(basis.routes_every_supply());
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
        assert!(basis.routes_every_supply());
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
        assert!(basis.routes_every_supply());
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
        assert!(basis.routes_every_supply());
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
        assert!(basis.routes_every_supply());
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
        assert!(!basis.routes_every_supply());
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

        assert!(!basis.routes_every_supply());
        assert_eq!(brute_force_optimum(&network), None);
    }

    #[test]
    fn test_a_problem_of_no_node_loads_and_answers_nothing() {
        let network = MincostNetwork::default();
        let mut room = BasisRoom::new();
        let basis = checked_star(&mut room, &network);

        assert_eq!(check(&basis, &network), Ok(()));
        assert!(basis.rows().len() == 0 && basis.routes_every_supply());
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
    fn test_room_on_all_its_cases_by_hand() {
        assert_eq!(room(5, 2, UP, UP), 3, "along a link up: what is left of its capacity");
        assert_eq!(room(5, 2, DOWN, DOWN), 3, "along a link down");
        assert_eq!(room(5, 2, UP, DOWN), 2, "against a link up: its flow");
        assert_eq!(room(5, 2, DOWN, UP), 2, "against a link down");
        assert_eq!((room(5, 5, UP, UP), room(5, 0, DOWN, UP)), (0, 0), "at a bound");
        assert_eq!(room(u64::MAX, 7, UP, UP), u64::MAX - 7, "a root link the way it points");
        assert_eq!(room(u64::MAX, 7, DOWN, UP), 7, "a root link against the way it points");
    }

    #[test]
    fn test_sent_on_all_its_cases_by_hand() {
        assert_eq!(sent(2, 3, UP, UP), 5);
        assert_eq!(sent(2, 3, DOWN, DOWN), 5);
        assert_eq!(sent(4, 3, UP, DOWN), 1);
        assert_eq!(sent(4, 3, DOWN, UP), 1);
        assert_eq!(sent(4, 0, DOWN, UP), 4, "a step of zero");
    }

    #[test]
    fn test_entering_link_on_all_its_cases_by_hand() {
        // From its lower bound the first end is the tail; from its capacity the head.
        let link = |direction: Direction, shift: i64| EnteringLink { direction, shift };
        assert_eq!(entering_link(Bound::Lower, Side::First, -7), link(UP, 7), "the tail moves");
        assert_eq!(entering_link(Bound::Lower, Side::Second, -7), link(DOWN, -7), "the head moves");
        assert_eq!(entering_link(Bound::Upper, Side::First, 7), link(DOWN, 7), "the head moves");
        assert_eq!(entering_link(Bound::Upper, Side::Second, 7), link(UP, -7), "the tail moves");
    }

    #[test]
    fn test_reduced_cost_by_hand() {
        assert_eq!(reduced_cost(5, 0, 11), -6);
        assert_eq!(reduced_cost(-1, 4, -4), 7);
        assert_eq!(reduced_cost(0, 0, 0), 0);
    }

    #[test]
    fn test_the_states_and_bounds_by_hand() {
        assert_eq!((ArcState::at(0, 0), ArcState::at(0, 1), ArcState::at(0, u64::MAX)), (IN_TREE, LOWER, LOWER));
        assert_eq!([LOWER, IN_TREE, UPPER].map(|state| state.violation(-4)), [-4, 0, 4]);
        assert_eq!([LOWER, IN_TREE, UPPER].map(ArcState::bound), [Some(Bound::Lower), None, Some(Bound::Upper)]);
        assert_eq!((Bound::Lower.other(), Bound::Upper.other()), (Bound::Upper, Bound::Lower));
        assert_eq!((Bound::Lower.state(), Bound::Upper.state()), (LOWER, UPPER));
    }

    #[test]
    fn test_where_an_arc_stands_for_what_it_carries_by_hand() {
        // No width: unpriced, whatever it carries, which is nothing.
        assert_eq!(ArcState::at(0, 0), IN_TREE);
        // Nothing of a positive width: at its lower bound; its whole width: at its capacity.
        assert_eq!((ArcState::at(0, 3), ArcState::at(3, 3)), (LOWER, UPPER));
        assert_eq!((ArcState::at(0, u64::MAX), ArcState::at(u64::MAX, u64::MAX)), (LOWER, UPPER));
    }

    #[test]
    fn test_what_an_arc_carries_where_it_stands_by_hand() {
        assert_eq!([LOWER, IN_TREE, UPPER].map(|state| state.carried(5)), [0, 0, 5]);
        assert_eq!([LOWER, IN_TREE, UPPER].map(|state| state.carried(0)), [0, 0, 0]);
        assert_eq!(UPPER.carried(u64::MAX), u64::MAX);

        // What an arc carries puts it back where it stood, but for an arc whose width has gone, which is unpriced.
        for state in [LOWER, UPPER] {
            assert_eq!(ArcState::at(state.carried(4), 4), state);
            assert_eq!(ArcState::at(state.carried(0), 0), IN_TREE);
        }
        assert_eq!(ArcState::at(IN_TREE.carried(4), 4), LOWER, "a tree arc carries nothing out of the tree");
    }

    /// The slot `slot` of `basis`, made by the checked maker.
    ///
    /// # Panics
    /// Panics if `slot` is not below the arc count.
    fn arc_slot(basis: &Basis<'_, '_>, slot: usize) -> ArcSlot {
        basis.arcs.slot(Slot::from(slot)).expect("the slot should be below the arc count")
    }

    /// The ratio test of `basis` on the cycle from `first` to `second` with an entering arc of width `width`: the
    /// step, and the link that leaves with its side, as plain numbers. The basis is unchanged.
    fn ratio_test(basis: &mut Basis<'_, '_>, [first, second]: [usize; 2], width: u64) -> (u64, Option<(usize, Side)>) {
        let mut walk = cycle_walk(&mut basis.tree, first, second);
        let RatioTest { step, leaving } = basis.links.least_room(&mut walk, width);
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
    fn ratio_test_answer(basis: &mut Basis<'_, '_>, [first, second]: [usize; 2], width: u64) -> RatioTest {
        let mut walk = cycle_walk(&mut basis.tree, first, second);

        basis.links.least_room(&mut walk, width)
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
        let read = |basis: &Basis<'_, '_>, slot: usize| {
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
        let cycle = cycle_walk(&mut basis.tree, 0, 2).into_cycle(None);
        let step = cycle.first_side().next().expect("node 0 lies below the root");

        assert_eq!(basis.links.room_of(step.link, step.direction, DOWN), 4, "against the link: its flow");
        assert_eq!(basis.links.room_of(step.link, step.direction, UP), u64::MAX - 4, "along it: what is left");
        basis.links.send(step.link, step.direction, DOWN, 3);
        assert_eq!(basis.links.flows[3], 1, "against the link its flow falls");
        basis.links.send(step.link, step.direction, UP, 2);
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
            let cycle = cycle_walk(&mut basis.tree, arc.first, arc.second).into_cycle(None);
            basis.links.send_around(&cycle, &arc, 0);
            assert_eq!(basis.links.flows, [0, 0, 0, 4, 0, 4], "a step of zero changes nothing");
            basis.links.send_around(&cycle, &arc, 3);
            assert_eq!(basis.links.flows, [3, 0, 0, 1, 0, 1]);
        }
        basis.arcs.flip(&arc);
        assert_eq!(basis.arcs.states, [UPPER, LOWER, LOWER]);

        // From its capacity the arc sends from its head, node 2, along both root links: each rises by the step.
        let back = basis.rows().entering(arc_slot(&basis, 0), &basis.links).expect("arc 0 should be priced");
        assert_eq!((back.bound, back.first, back.second), (Bound::Upper, 2, 0));
        {
            let cycle = cycle_walk(&mut basis.tree, back.first, back.second).into_cycle(None);
            basis.links.send_around(&cycle, &back, 1);
            assert_eq!(basis.links.flows, [2, 0, 0, 2, 0, 2]);
        }
        basis.arcs.flip(&back);
        assert_eq!(basis.arcs.states, [LOWER, LOWER, LOWER]);
    }

    /// Sends around the cycle the arc at `slot` of `basis` closes the step the ratio test allows, and exchanges the
    /// states of the arc and the link that leaves; answers that link. The tree is not moved.
    ///
    /// # Panics
    /// Panics if the arc is not priced or no link leaves.
    fn exchanged_states(basis: &mut Basis<'_, '_>, slot: usize) -> usize {
        let arc = basis.rows().entering(arc_slot(basis, slot), &basis.links).expect("the arc should be priced");
        let mut walk = cycle_walk(&mut basis.tree, arc.first, arc.second);
        let RatioTest { step, leaving } = basis.links.least_room(&mut walk, arc.width);
        let cycle = walk.into_cycle(leaving);
        basis.links.send_around(&cycle, &arc, step);
        let exchange = Exchange::of(cycle).expect("a link should leave");
        basis.arcs.exchange_states(&arc, exchange.leaving_link(), &basis.links);

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
        room.place_links(&start);
        assert_eq!(room.capacities, [0, 0, 0, 0, 0, u64::MAX, u64::MAX]);
        assert_eq!(room.flows, [0, 0, 0, 0, 0, 2, 2]);
        assert_eq!(room.states, [LOWER; 5], "every arc of the star at its lower bound");

        // By slot: arcs 0, 2, 4, 1, 3; arcs 2 and 3 have no width and are not priced.
        room.place_arcs(start.problem());
        assert_eq!(room.ends, [ends(0, 1), ends(0, 0), ends(1, 1), ends(1, 0), ends(0, 1)]);
        assert_eq!(room.costs, [1, 5, -1, -2, 4]);
        assert_eq!(room.states, [LOWER, IN_TREE, LOWER, LOWER, IN_TREE]);
        assert_eq!(room.capacities, [2, 0, 1, 2, 0, u64::MAX, u64::MAX]);
        assert_eq!(room.flows, [0, 0, 0, 0, 0, 2, 2], "the arcs carry nothing, the root links what they did");
    }

    #[test]
    fn test_scatter_without_lower_bounds_takes_whole_capacities_by_hand() {
        let network = five_arcs();
        let problem = Problem { lower_bounds: None, ..problem_of(&network) };
        let checked = problem.checked().expect("the five arcs should be admitted");
        let mut room = BasisRoom::new();
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
        let mut room = BasisRoom::new();
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

        BasisRoom::new().place_arcs(problem);
    }

    #[test]
    fn test_the_write_out_adds_the_lower_bounds_back_in_arc_order_by_hand() {
        // The five arcs carrying 1, 0, 1, 2, 0 at the slots 0 to 4: arcs 0, 2, 4, 1, 3. With their lower bounds the
        // arcs carry 1, 3, 0, 2, 1 at the costs 1, −2, 5, 4, −1: `1 − 6 + 0 + 8 − 1 = 2`.
        let network = five_arcs();
        let mut room = BasisRoom::new();
        let basis = checked_star(&mut room, &network);
        basis.links.flows[..5].copy_from_slice(&[1, 0, 1, 2, 0]);

        let mut flows = [7; 5];
        let CheckedArcs::WithLowerBounds(arcs) = basis.problem.arcs() else { panic!("the lower bounds are given") };
        assert_eq!(basis.gather(arcs, &mut flows), 2);
        assert_eq!(flows, [1, 3, 0, 2, 1]);
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
    #[should_panic(
        expected = "called `Basis::pivot` with Slot 2, whose arc should be priced: out of the tree and of positive \
                    width"
    )]
    fn test_pivot_refuses_an_arc_in_the_tree() {
        let network = depot();
        let mut room = BasisRoom::new();
        let mut basis = loaded(&mut room, &problem_of(&network));
        basis.pivot(Slot::from(2_usize));
        basis.pivot(Slot::from(2_usize));
    }

    #[test]
    #[should_panic(
        expected = "called `Basis::pivot` with Slot 0, whose arc should be priced: out of the tree and of positive \
                    width"
    )]
    fn test_pivot_refuses_an_arc_of_no_width() {
        let network = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 0, -5), (0, 1, 0, 1, 2)]);
        let mut room = BasisRoom::new();
        loaded(&mut room, &problem_of(&network)).pivot(Slot::from(0_usize));
    }

    #[test]
    #[should_panic(expected = "called `Basis::write_flows` with 2 flows, which should be the arc count 3")]
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
        let held = (room.flows.capacity(), room.ends.capacity());
        assert_eq!(pivots_of(&mut room, &smaller), fresh);
        assert_eq!((room.flows.capacity(), room.ends.capacity()), held, "the smaller problem fits the room held");

        let panicked = catch_unwind(AssertUnwindSafe(|| {
            loaded(&mut room, &problem_of(&larger)).pivot(Slot::from(4_usize));
        }));
        assert!(panicked.is_err(), "slot 4 is past the larger problem's four arcs");
        assert_eq!(pivots_of(&mut room, &smaller), fresh, "a pivot that panicked leaves only room behind");
    }

    #[test]
    fn test_try_reserve_makes_room_for_a_load() {
        let mut room = BasisRoom::new();
        assert_eq!((room.ends.capacity(), room.flows.capacity()), (0, 0), "a new room holds nothing");
        room.try_reserve(3, 3).expect("room for three nodes and three arcs should be granted");

        let capacities = |room: &BasisRoom| {
            [room.ends.capacity(), room.costs.capacity(), room.states.capacity()]
                .into_iter()
                .chain([room.capacities.capacity() - 3, room.flows.capacity() - 3])
                .min()
        };
        assert!(capacities(&room) >= Some(3));
        let held = (room.ends.as_ptr(), room.flows.as_ptr(), room.flows.capacity());

        let network = depot();
        let _ = loaded(&mut room, &problem_of(&network));
        room.try_reserve(1, 1).expect("a smaller reservation should be granted");
        assert_eq!((room.ends.as_ptr(), room.flows.as_ptr(), room.flows.capacity()), held, "the load reallocated");

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

        basis.links.flows[3] += 1;
        assert!(check(&basis, &network).is_err_and(|breach| breach.contains("excess")));
        basis.links.flows[3] -= 1;

        basis.links.flows[4] = 1;
        basis.links.flows[1] = 1;
        assert!(check(&basis, &network).is_err(), "a root link out of the tree carries flow");
        (basis.links.flows[4], basis.links.flows[1]) = (0, 0);

        basis.arcs.states[1] = UPPER;
        assert!(check(&basis, &network).is_err(), "a state disagrees with its flow");
        basis.arcs.states[1] = LOWER;
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
            ends.insert(run.basis.routes_every_supply());
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

    /// A tree arc that stays, carrying `flow` and giving its parent `to_parent`, for a test to write out.
    fn stays(flow: u64, to_parent: i128) -> TreeArcFlow {
        TreeArcFlow { flow, to_parent, standing: Standing::InTree }
    }

    /// A tree arc that leaves at `state`, carrying `flow` and giving its parent `to_parent`, its node hanging from the
    /// root by a root link carrying `rest` the way `direction` says, for a test to write out.
    fn leaves(flow: u64, to_parent: i128, state: ArcState, rest: u64, direction: Direction) -> TreeArcFlow {
        TreeArcFlow {
            flow,
            to_parent,
            standing: Standing::Leaves { state, root_link: RootLink { flow: rest, direction } },
        }
    }

    #[test]
    fn test_resend_up_an_arc_on_every_case_by_hand() {
        // The node is the arc's tail and sends its excess up the arc of width 5, as much as fits.
        assert_eq!(resend(-3, 5, UP), leaves(0, 0, LOWER, 3, DOWN), "a demand: nothing up, the root link brings it");
        assert_eq!(resend(0, 5, UP), stays(0, 0), "nothing to send, room to send more");
        assert_eq!(resend(3, 5, UP), stays(3, 3), "within the width, room left");
        assert_eq!(resend(5, 5, UP), leaves(5, 5, UPPER, 0, UP), "exactly the width: no room left toward the root");
        assert_eq!(resend(8, 5, UP), leaves(5, 5, UPPER, 3, UP), "past the width: the rest up the root link");
    }

    #[test]
    fn test_resend_down_an_arc_on_every_case_by_hand() {
        // The node is the arc's head: the arc brings it the negation of its excess from the parent, as much as fits.
        assert_eq!(resend(-3, 5, DOWN), stays(3, -3), "within the width, room to bring less");
        assert_eq!(resend(0, 5, DOWN), leaves(0, 0, LOWER, 0, UP), "nothing brought: no room toward the root");
        assert_eq!(resend(3, 5, DOWN), leaves(0, 0, LOWER, 3, UP), "a supply: nothing down, the root link takes it");
        assert_eq!(resend(-5, 5, DOWN), stays(5, -5), "exactly the width: it can still bring less");
        assert_eq!(resend(-8, 5, DOWN), leaves(5, -5, UPPER, 3, DOWN), "past the width: the rest down the root link");
    }

    #[test]
    fn test_resend_along_an_arc_of_no_width_always_leaves_by_hand() {
        assert_eq!(resend(2, 0, UP), leaves(0, 0, IN_TREE, 2, UP));
        assert_eq!(resend(0, 0, UP), leaves(0, 0, IN_TREE, 0, UP));
        assert_eq!(resend(-2, 0, DOWN), leaves(0, 0, IN_TREE, 2, DOWN));
        assert_eq!(resend(0, 0, DOWN), leaves(0, 0, IN_TREE, 0, UP));
    }

    #[test]
    fn test_resend_at_the_limit_and_the_widest_width_by_hand() {
        let limit = i128::from(NET_SUPPLY_LIMIT);

        assert_eq!(resend(limit, u64::MAX, UP), stays(NET_SUPPLY_LIMIT, limit));
        assert_eq!(resend(-limit, u64::MAX, DOWN), stays(NET_SUPPLY_LIMIT, -limit));
        assert_eq!(resend(-limit, 1, UP), leaves(0, 0, LOWER, NET_SUPPLY_LIMIT, DOWN));
        assert_eq!(resend(limit, 1, UP), leaves(1, 1, UPPER, NET_SUPPLY_LIMIT - 1, UP));
    }

    #[test]
    fn test_within_width_clamps_by_hand() {
        assert_eq!([-7, 0, 3, 5, 9].map(|demand| within_width(demand, 5)), [0, 0, 3, 5, 5]);
        assert_eq!(within_width(i128::from(u64::MAX) + 1, u64::MAX), u64::MAX);
        assert_eq!(within_width(i128::MIN, 4), 0);
        assert_eq!(within_width(i128::MAX, 4), 4);
    }

    /// The depot solved to its optimum in `room` by the pivots of its hand-worked run, the basis kept: arcs 0 and 1 in
    /// the tree, carrying 2 each, node 0 up from the root at zero flow, node 1 below it by arc 1 and node 2 by arc 0,
    /// both running down to them; arc 2 out of it at its capacity 2.
    fn kept_depot(room: &mut BasisRoom) {
        let network = depot();
        let mut basis = checked_star(room, &network);
        for slot in [0_usize, 2, 1, 0] {
            basis.pivot(Slot::from(slot));
        }

        let _ = basis.keep();
    }

    /// The basis `room` lends for `network` from what it holds, its whole invariant checked, read out, with which
    /// start it is.
    ///
    /// # Panics
    /// Panics if the invariant fails after the resume.
    fn reflowed(room: &mut BasisRoom, network: &MincostNetwork) -> (Snapshot, Begun) {
        let mut excesses = Excesses::new();
        let (basis, begun) = resumed_in(room, &mut excesses, &problem_of(network));
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
        let mut room = BasisRoom::new();
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
        let mut room = BasisRoom::new();
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
        let mut room = BasisRoom::new();
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
        let mut room = BasisRoom::new();
        kept_depot(&mut room);
        let mut network = depot();
        network.capacities[1] = 0;

        let mut excesses = Excesses::new();
        let (mut basis, begun) = resumed_in(&mut room, &mut excesses, &problem_of(&network));
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
        assert!(!basis.routes_every_supply());
        assert_eq!(brute_force_optimum(&network), None);
    }

    /// Checks that `room` lends the star of `network` for it, as a fresh room loads it, and holds no basis meanwhile.
    ///
    /// # Panics
    /// Panics if it does not.
    fn assert_resumes_to_the_star(room: &mut BasisRoom, network: &MincostNetwork) {
        let star = snapshot(&checked_star(&mut BasisRoom::new(), network));
        let mut excesses = Excesses::new();

        let (basis, begun) = resumed_in(room, &mut excesses, &problem_of(network));
        assert_eq!((snapshot(&basis), begun), (star, Begun::Star));
        assert_eq!(*basis.held, Held::Nothing);
    }

    #[test]
    fn test_a_room_that_holds_nothing_resumes_to_the_star() {
        assert_resumes_to_the_star(&mut BasisRoom::new(), &depot());

        // A load takes out what the room held.
        let mut room = BasisRoom::new();
        kept_depot(&mut room);
        assert_eq!(*loaded(&mut room, &problem_of(&depot())).held, Held::Nothing);
        assert_resumes_to_the_star(&mut room, &depot());
    }

    #[test]
    fn test_a_basis_of_other_counts_resumes_to_the_star() {
        let mut room = BasisRoom::new();
        kept_depot(&mut room);
        assert_resumes_to_the_star(&mut room, &network_of_two_parallel_arcs());

        let mut room = BasisRoom::new();
        kept_depot(&mut room);
        let arcs = [(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1), (0, 2, 0, 1, 9)];
        let more_arcs = MincostNetwork::from_arcs(&[4, 0, -4], &arcs);
        assert_resumes_to_the_star(&mut room, &more_arcs);
    }

    #[test]
    fn test_a_basis_with_another_end_at_a_slot_resumes_to_the_star() {
        let mut room = BasisRoom::new();
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
        let mut room = BasisRoom::new();
        let mut rule = EnteringRule::FirstEligible;
        let end = CheckedRun::start(&mut room, &cycle, &problem_of(&cycle))
            .and_then(|run| run.run_to_end(|rows| rule.next(rows)))
            .expect("the cycle should run to its end");
        assert_eq!(end.answer.map(|answer| answer.flows), Some(vec![1, 1]));
        assert_eq!(room.states, [UPPER, IN_TREE]);

        // Both arcs as wide as a `u64` holds: the first, at its capacity, carries `u64::MAX` from node 0 to node 1,
        // one past the limit.
        let wide = MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, u64::MAX, -1), (1, 0, 0, u64::MAX, -1)]);
        assert_resumes_to_the_star(&mut room, &wide);

        // One unit narrower and the excesses keep the limit: the kept basis serves.
        let mut room = BasisRoom::new();
        let _ = CheckedRun::start(&mut room, &cycle, &problem_of(&cycle))
            .and_then(|run| run.run_to_end(|rows| EnteringRule::FirstEligible.next(rows)));
        let at_limit = MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, u64::MAX - 1, -1), (1, 0, 0, u64::MAX, -1)]);
        assert_eq!(reflowed(&mut room, &at_limit).1, Begun::Kept);
    }

    #[test]
    fn test_keep_puts_the_basis_in_the_room_and_a_clone_keeps_it() {
        let mut room = BasisRoom::new();
        assert_eq!(room.held, Held::Nothing);
        kept_depot(&mut room);

        assert_eq!(room.held, Held::Basis);
        assert_eq!(room.tree.kept().map(|kept| (kept.node_count, kept.link_count)), Some((3, 6)));
        let mut copy = room.clone();
        assert_eq!(copy.held, room.held);
        assert_eq!(reflowed(&mut copy, &depot()).1, Begun::Kept);
    }

    #[test]
    fn test_a_run_unwound_after_the_resume_leaves_nothing_kept() {
        // The resume takes the basis out before it writes; the pivot of arc 1, in the tree, panics with the basis lent.
        let mut room = BasisRoom::new();
        kept_depot(&mut room);

        let network = depot();
        let unwound = catch_unwind(AssertUnwindSafe(|| {
            let mut excesses = Excesses::new();
            let (mut basis, begun) = resumed_in(&mut room, &mut excesses, &problem_of(&network));
            assert_eq!(begun, Begun::Kept);
            basis.pivot(Slot::from(1_usize));
        }));
        assert!(unwound.is_err(), "a pivot of an arc in the tree should panic");
        assert_eq!(room.held, Held::Nothing);
        assert_resumes_to_the_star(&mut room, &depot());
    }

    /// Runs `network` from the star in `room` under the first eligible rule to its end, its invariant checked after
    /// every pivot, and keeps the basis; answers the basis read out at its end.
    ///
    /// # Errors
    /// As [`CheckedRun::pivot_next`].
    fn ended(room: &mut BasisRoom, network: &MincostNetwork) -> Result<Snapshot, String> {
        let mut rule = EnteringRule::FirstEligible;
        let mut run = CheckedRun::start(room, network, &problem_of(network))?;
        while run.pivot_next(|rows| rule.next(rows))?.is_some() {}

        let end = snapshot(&run.basis);
        let _ = run.basis.keep();
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
            let (mut room, mut excesses) = (BasisRoom::new(), Excesses::new());

            let (run, begun) =
                CheckedRun::resumed(&mut room, &mut excesses, before, network, &problem_of(network)).map_err(fail)?;
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
            let mut room = BasisRoom::new();
            let end = ended(&mut room, &before).expect("the first run should keep the invariant");

            let (mut run, begun) = {
                let mut excesses = Excesses::new();
                let (basis, begun) = resumed_in(&mut room, &mut excesses, &problem_of(&network));
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
            ended_infeasible += usize::from(!run.basis.routes_every_supply());
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
            let mut room = BasisRoom::new();
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
            let mut room = BasisRoom::new();
            let end = ended(&mut room, &network).map_err(TestCaseError::fail)?;

            let (again, begun) = reflowed(&mut room, &MincostNetwork { costs, ..network });
            prop_assert_eq!(begun, Begun::Kept);
            prop_assert_eq!((again.flows, again.states, again.hangings), (end.flows, end.states, end.hangings));
        }
    }
}
