// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The network: one problem held in the layout it is solved in, with the basis of its last solve; its invariant, its
//! load, its changes, its solve, its copy and the reads of the problem it holds.

use core::cmp::Ordering;
use core::iter;

use std::collections::TryReserveError;

use typed_index_collections::TiVec;

use super::arc_ends::ArcEnds;
use super::bounds::Bounds;
use super::change_error::ChangeError;
use super::costs::Costs;
use super::dealt_columns::DealtColumns;
use super::flow_bounds::FlowBounds;
use super::flows::Flows;
use super::held_problem::HeldProblem;
use super::kept_sums::{CostChange, KeptSums};
use super::marked_blocks::{MarkedBlocks, MarkedSearch};
use super::optimum::NetworkOptimum;
use super::unmet::Unmet;
use crate::basis::{Basis, KeptArc, KeptBasisRoom, KeptColumns, KeptProblem, LentBasis, Rerouted, Restated};
use crate::outcome::{Answer, Outcome, Work};
use crate::problem::{CheckedArc, CheckedArcs, CheckedProblem, NodeId, Problem, ProblemError};
use crate::ranges::{cost_bound, widest_root_cost, within_cost_bound};
use crate::room::{grow_total, refill, reserve_total};
use crate::slot_order::{DealtSlotsIter, Slot, SlotOrder};
use crate::tree::SpanningTree;

/// A state that holds one min-cost flow problem and a basis of it, which its changes, loads and solves carry on from.
///
/// [`load`](Self::load) hands it a [`Problem`], which it copies into the layout it solves in, starting from the basis
/// it holds where the problem has the same arcs. Its setters change one number of the problem held at once:
/// [`set_cost`](Self::set_cost), [`set_lower_bound`](Self::set_lower_bound), [`set_capacity`](Self::set_capacity) and
/// [`set_bounds`](Self::set_bounds) one arc's cost or bounds, [`set_supply`](Self::set_supply) a node's supply against
/// a node that balances it, and [`move_supply`](Self::move_supply) moves supply from one node to another.
/// [`solve`](Self::solve) answers the cost of the optimum of the problem as it stands, or that no flow within the
/// bounds meets the supplies, beside the [`Work`] the solve did, and [`optimum`](Self::optimum) lends that optimum as a
/// [`NetworkOptimum`] that borrows the network: its cost, its potentials, and a view of its flows. The columns
/// [`tails`](Self::tails), [`heads`](Self::heads), [`costs`](Self::costs), [`lower_bounds`](Self::lower_bounds),
/// [`capacities`](Self::capacities) and [`supplies`](Self::supplies), named as a [`Problem`]'s fields, and the single
/// reads [`tail`](Self::tail), [`head`](Self::head), [`cost`](Self::cost), [`lower_bound`](Self::lower_bound),
/// [`capacity`](Self::capacity), [`bounds`](Self::bounds), the pair [`set_bounds`](Self::set_bounds) takes, and
/// [`supply`](Self::supply), named as the setter without `set_`, read the problem held as it stands, each number where
/// it is asked for, so a caller keeps no copy of the problem of its own; a read changes nothing and keeps an optimum,
/// which it can be read beside.
///
/// A network keeps the basis its last solve ended on, and a change keeps it a basis of the problem as it then stands,
/// so a solve after a few changes makes only the pivots they call for. It remembers which arcs a change may have made
/// worth using: a solve prices those first, so a solve with nothing changed prices no arc and makes no pivot, a solve
/// after changes that call for no pivot prices only the arcs around them, and a solve that pivots prices at least one
/// whole pass over the arcs, as a solver's solve does. Where a change leaves much flow still to be sent, so that the
/// solve makes many pivots, a network's solve can take longer than a [`load`](Self::load) of the whole problem as it
/// then stands and the solve after it. Its answers are a function of the calls made on it, in order, and of nothing
/// else: two networks given the same calls answer alike, their work included. The outcome and the cost are those of the
/// problem as it stands, the potentials prove the flow optimal, and the flow among tied optima, the potentials and the
/// work are this network's own. Where a [`NetworkSimplex`](crate::NetworkSimplex) admits the problem as it stands, it
/// answers the same outcome and cost. After moves of supply and lower bounds a network may hold numbers a solver and a
/// [`load`](Self::load) refuse: every supply stays within `i64`, and the flow still to be sent within `2^64 − 2`, but
/// the supplies net of the lower bounds may add up past `2^64 − 2` where they are positive, where the arcs carry the
/// difference; the network still solves them exactly, since its arithmetic rests on the flow still to be sent, not on
/// that total. A load starts from the basis the network holds where that basis serves the problem loaded, and otherwise
/// from the star, as a solver's solve does; either way a network tries no arcs first and prices in another order than a
/// solver, so it may take another pivot path than a solver's solve of the same problem.
///
/// For every change: a change that is refused, and one that panics, change nothing, each check coming before the
/// first write. A change that sets the number already held changes nothing, and keeps an optimum the network held.
/// Every other change ends that optimum, also where the flow held stays optimal: [`optimum`](Self::optimum) answers
/// `None` until the next solve that finds one. Each setter answers the number it replaced, so setting that number back
/// undoes it, and a move of supply is undone by the move back: the problem is then the earlier one, and the next solve
/// answers its outcome and its cost, with a flow of that cost, which among tied optima may be another than the one held
/// before. An arc switched off by a capacity of zero is switched on again so: a caller may build every arc it may
/// ever need up front and switch each on and off.
///
/// A network never loaded holds the problem of no node and no arc, whose optimum is no flow at the cost zero, and no
/// arc a change can name. A load that is refused keeps the problem held, and its optimum where it had one. A copy, made
/// by [`try_clone`](Self::try_clone), [`try_clone_from`](Self::try_clone_from) or `clone`, holds what the network it
/// was made from holds and answers every call as it would, its work included, and changes apart from it.
///
/// Of its calls only [`load`](Self::load), [`try_reserve`](Self::try_reserve) and the copies allocate: `load` not for a
/// problem within the room `try_reserve` made, and `try_clone_from` not into a network with such room for the network
/// it copies; a copy's changes and solves allocate nothing. `try_reserve`, `try_clone` and `try_clone_from` answer
/// memory that is refused; `load` and `clone` end the process there, as a `Vec` that grows does. It holds about 57
/// bytes an arc and 105 a node: the problem in the layout it is solved in, its basis, the room a load works in, and the
/// record of which arcs a change may have made worth using.
///
/// ```
/// use kahlnws::{Network, Outcome, Problem, ProblemError};
///
/// fn main() -> Result<(), ProblemError> {
///     // Node 0 sends 4 units to node 2, directly at 5 a unit for up to 3 units, or through node 1 at 2 a unit for up
///     // to 2.
///     let problem = Problem {
///         tails: &[0, 0, 1],
///         heads: &[2, 1, 2],
///         lower_bounds: None,
///         capacities: &[3, 4, 2],
///         costs: &[5, 1, 1],
///         supplies: &[4, 0, -4],
///     };
///
///     let mut network = Network::new();
///     network.load(&problem)?;
///     let answer = network.solve();
///     assert_eq!((answer.outcome, answer.work.pivots), (Outcome::Optimal(14), 4));
///
///     // The flows, read through the optimum, beside each arc's tail and head.
///     let optimum = network.optimum().expect("the routes should carry the supply");
///     for ((tail, head), flow) in network.tails().iter().zip(network.heads()).zip(&optimum.flows) {
///         println!("{tail} -> {head}: {flow}");
///     }
///     assert_eq!(optimum.flows.iter().collect::<Vec<u64>>(), [2, 2, 2]);
///
///     // Nothing changed: the same cost again, with no pivot.
///     let again = network.solve();
///     assert_eq!((again.outcome.optimal(), again.work.pivots), (Some(14), 0));
///
///     Ok(())
/// }
/// ```
// # Invariant
//
// Where `standing` is `Unloaded`: the network has never been loaded; the room holds no basis, `lower_bounds` and
// `supplies` are empty, `order` is of no arc and no node, `marks` are laid for nothing, and `sums` are those of no
// node. Otherwise, with `n` nodes and `m` arcs in the problem held:
//
// - *Basis.* The room holds a basis of the problem held, at the widest root cost of `n` nodes, which keeps every item
//   of `Basis`'s invariant for that problem: a strongly feasible spanning-tree basis, its arcs at the slots `order`
//   deals, its potentials tight under the problem's costs.
// - *Shape.* `order` is `SlotOrder::new(m, n)`; `lower_bounds` holds `m` entries, the lower bound of the arc dealt to
//   each slot, at most its capacity, the lower bound and the width the basis holds adding up to it; `supplies` holds
//   `n` entries, each node's supply in the problem held, each within `i64`. A load writes the three before its `keep`,
//   a change of bounds one lower bound, and nothing else changes their lengths: they hold wherever the room holds a
//   basis, and through a change, which reads them while it does.
// - *Marks.* `marks` is laid for the `m` slots and the `n` nodes, by a load alone, and every arc of the basis that
//   violates its bound lies in a block it marks. `moved` is empty: a change fills it and drains it into the marks.
// - *Sums.* `sums` keeps the invariant of `KeptSums` for the problem and the basis held: its excess is the flow up the
//   root links of the basis held.
// - *Standing.* `Optimal` only where no block is marked and no root link carries flow, so where no arc violates.
//
// `load` establishes it, `set_cost`, `set_capacity`, `set_lower_bound`, `set_bounds`, `move_supply`, `set_supply` and
// `solve` keep it, and a copy (`copy_from`) holds its source's. The first write of each of these calls is `standing =
// Changed`, and its last `keep`, or the source's standing where a copy keeps no basis: between the two the room holds
// no basis and the standing is `Changed`. Every check and refusal of a call comes before its first write, and a change
// that sets the number held writes nothing. A call that unwinds between its first write and its `keep`, which only a
// defect of the crate makes, leaves the network half changed, `Changed` with no basis held, which every call that reads
// the basis refuses and `load` mends; so `Optimal` always stands beside a basis held.
#[derive(Debug)]
pub struct Network {
    /// The room of the basis, which holds the basis of the problem held between calls, with the scratch rooms of the
    /// net supplies and the excesses a load reads.
    basis: KeptBasisRoom,
    /// Per node: its supply in the problem held, in units of flow. The flows imply it, but reading it from them needs
    /// the arcs by node, which nothing holds; only a load and a move of supply write it.
    supplies: TiVec<NodeId, i64>,
    /// Per slot: the lower bound of the arc dealt to it, in units of flow.
    lower_bounds: Vec<u64>,
    /// The dealing of the problem's arcs to the slots, with the problem's node and arc counts.
    order: SlotOrder,
    /// The blocks of slots that may hold an arc that violates its bound, with the blocks of the arcs at each node.
    marks: MarkedBlocks,
    /// The nodes whose subtrees' potentials the change under way moved, which it marks the sides of before it
    /// returns: empty between calls, with room for a node each.
    moved: Vec<usize>,
    /// The cost total of the problem held and the cost of the basis's flow, kept as they change.
    sums: KeptSums,
    /// Where the network stands: never loaded, or loaded and whether its last solve found the optimum it holds.
    standing: Standing,
}

/// Where a network stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Standing {
    /// Never loaded: the problem of no node, no basis held.
    Unloaded,
    /// Loaded, and loaded or changed since its last solve, or never solved, or its last solve found no flow.
    Changed,
    /// Loaded, and the last call that loaded, changed or solved it was a solve that found a flow.
    Optimal,
}

impl Network {
    /// The network of the problem of no node and no arc, never loaded. It allocates nothing.
    ///
    /// ```
    /// use kahlnws::Network;
    ///
    /// let network = Network::new();
    /// assert_eq!((network.node_count(), network.arc_count()), (0, 0));
    /// ```
    pub const fn new() -> Self {
        Self {
            basis: KeptBasisRoom::new(),
            supplies: TiVec::new(),
            lower_bounds: Vec::new(),
            order: SlotOrder::new(0, 0),
            marks: MarkedBlocks::new(),
            moved: Vec::new(),
            sums: KeptSums::OF_NO_NODE,
            standing: Standing::Unloaded,
        }
    }

    /// Makes room for problems of up to `node_count` nodes and `arc_count` arcs: a [`load`](Self::load) of such a
    /// problem allocates nothing afterwards. Room already held is kept, and nothing else changes: the problem held,
    /// and its optimum, stay.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    ///
    /// ```
    /// use kahlnws::Network;
    ///
    /// let mut network = Network::new();
    /// assert!(network.try_reserve(1000, 4000).is_ok());
    /// ```
    pub fn try_reserve(&mut self, node_count: usize, arc_count: usize) -> Result<(), TryReserveError> {
        self.basis.try_reserve(node_count, arc_count)?;
        reserve_total(&mut self.supplies.raw, node_count)?;
        reserve_total(&mut self.lower_bounds, arc_count)?;
        reserve_total(&mut self.moved, node_count)?;
        self.marks.try_reserve(node_count, arc_count)
    }

    /// Holds `problem` from now on, in place of the problem held, with no optimum answered yet, and a basis of it to
    /// start the next [`solve`](Self::solve) from.
    ///
    /// The basis is the one the network holds, carried over to the new numbers, where it serves: where the network
    /// holds a basis of a problem with the same node count, the same arc count and the same tail and head at every arc,
    /// and the supplies net of the lower bounds and of what that basis carries on its arcs out of the tree add up to at
    /// most `2^64 − 2` where they are positive. Otherwise, and where the network holds none, never loaded or left half
    /// changed by a panic, it is the star, every arc at its lower bound, as the first load starts from. Either way the
    /// next solve prices every arc.
    ///
    /// The solve after a load answers the same outcome and the same cost whichever basis the load started from; the
    /// flow among tied optima, the potentials and the work may differ, each a function of the calls made on the
    /// network, which is deterministic. Where only some numbers changed since the problem the network held, the solve
    /// often makes far fewer pivots than one from the star; the same problem loaded again onto the optimum the network
    /// found for it, where that basis serves it, makes none.
    ///
    /// It costs a few passes over the arcs and the nodes, O(m + n) for `m` arcs and `n` nodes, and allocates only where
    /// the network's room is smaller than the problem: room made by [`try_reserve`](Self::try_reserve) or by an
    /// earlier load of a problem no smaller.
    ///
    /// # Panics
    /// On a target whose `usize` has 32 bits it can panic while it grows its room, where a column's room passes
    /// `isize::MAX` bytes. It grows every column to the problem's counts before it changes anything, so such a panic
    /// leaves the network as it was.
    ///
    /// # Errors
    /// Returns the first breach of `problem`, as [`ProblemError`] orders them: the refusals of
    /// [`NetworkSimplex::solve`](crate::NetworkSimplex::solve). The network then holds what it held, its basis and
    /// its optimum included, and the next load starts from that basis where it serves.
    ///
    /// ```
    /// use kahlnws::{Network, Problem, ProblemError};
    ///
    /// fn main() -> Result<(), ProblemError> {
    ///     // Node 0 sends 4 units to node 2, directly at 5 a unit for up to 3 units, or through node 1 at 2 a unit for
    ///     // up to 2.
    ///     let problem = Problem {
    ///         tails: &[0, 0, 1],
    ///         heads: &[2, 1, 2],
    ///         lower_bounds: None,
    ///         capacities: &[3, 4, 2],
    ///         costs: &[5, 1, 1],
    ///         supplies: &[4, 0, -4],
    ///     };
    ///     let mut network = Network::new();
    ///     network.load(&problem)?;
    ///     assert_eq!(network.solve().work.pivots, 4);
    ///
    ///     // One unit less to send: the load starts from the optimum held, which still serves, and needs no pivot.
    ///     network.load(&Problem { supplies: &[3, 0, -3], ..problem })?;
    ///     let answer = network.solve();
    ///     assert_eq!((answer.outcome.optimal(), answer.work.pivots), (Some(9), 0));
    ///
    ///     // Supplies that do not balance are refused, and the network keeps what it held.
    ///     let unbalanced = Problem { supplies: &[3, 0, -2], ..problem };
    ///     assert_eq!(network.load(&unbalanced), Err(ProblemError::Unbalanced { total: 1 }));
    ///     assert_eq!(network.optimum().map(|optimum| optimum.total_cost), Some(9));
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn load(&mut self, problem: &Problem<'_>) -> Result<(), ProblemError> {
        let checked = problem.checked()?;
        let (node_count, arc_count) = (checked.node_count(), checked.arc_count());
        // Every column the load writes grows to the problem's counts before the first write, so that nothing after it
        // can unwind for memory.
        self.grow(node_count, arc_count);
        // The net supplies are gathered, and refused past their limit, and the start decided: whether the basis held
        // serves the problem. A network half changed holds none, and its load starts from the star. None of it writes
        // what the network holds.
        let resumption = self.basis.resume(checked)?;

        // Nothing is refused from here on. The first write ends the optimum held, and the last keeps the basis.
        self.standing = Standing::Changed;
        let root_cost = widest_root_cost(node_count);

        // The widest root cost of the node count lies within the problem's root costs: the start does not panic. The
        // network does not ask which start it is: either is a basis of the problem at that root cost.
        let basis = resumption.begin(root_cost);
        refill(&mut self.supplies.raw, checked.supplies().iter().copied());
        lay_lower_bounds(&mut self.lower_bounds, checked);
        lay_marks(&mut self.marks, checked);
        self.moved.clear();
        // The cost of the arcs' flows, their lower bounds included, by the basis's one rule of it, exact for a checked
        // problem; and the flow through the root, read by the basis's one rule of it: as much flows down the root
        // links as up them, at most the positive total of the start's excesses, which the start keeps within the limit
        // (`Start`, `KeptStart`), so neither the read nor the sums panic.
        self.sums = KeptSums::of_start(checked.cost_total(), basis.arc_cost(), root_cost, basis.unrouted());
        self.order = SlotOrder::new(arc_count, node_count);
        basis.keep();

        Ok(())
    }

    /// Makes room for a problem of `node_count` nodes and `arc_count` arcs in every column a load writes, as
    /// [`try_reserve`](Self::try_reserve) does, growing as a `Vec` does where the memory runs out. Nothing else
    /// changes.
    ///
    /// # Panics
    /// Panics if a column's room passes `isize::MAX` bytes, as a growing `Vec` does.
    fn grow(&mut self, node_count: usize, arc_count: usize) {
        self.basis.grow(node_count, arc_count);
        grow_total(&mut self.supplies.raw, node_count);
        grow_total(&mut self.lower_bounds, arc_count);
        grow_total(&mut self.moved, node_count);
        self.marks.grow(node_count, arc_count);
    }

    /// Makes `cost` the cost of a unit of flow on `arc`, in units of cost, and answers the cost it had.
    ///
    /// The network then holds the problem with that cost, with a basis of it, from which the next
    /// [`solve`](Self::solve) finds the optimum of the problem as it now stands. The cost the arc already has changes
    /// nothing, and keeps an optimum the network held; any other cost ends it, as the network's doc says. Setting the
    /// cost answered back makes the problem the earlier one again.
    ///
    /// It costs a constant where the change moves no node's potential, and otherwise the nodes whose potentials it
    /// moves, which can be most of them; the next solve prices the arc, and where potentials moved, the arcs at the
    /// smaller of the two groups of nodes, those whose potentials moved and the rest, which can be half the network. It
    /// allocates nothing.
    ///
    /// # Panics
    /// Panics if `arc` is not below the arc count, which on a network never loaded no arc is; or if an earlier call on
    /// this network panicked after it began to change it, which only a defect of the crate makes: the network is then
    /// half changed, and only [`load`](Self::load) mends it. Either panic comes before any change.
    ///
    /// # Errors
    /// [`ChangeError::CostBeyondBound`] where `cost` lies beyond plus or minus [`cost_bound`] of the node count, else
    /// [`ChangeError::CostTotalBeyondRange`] where with it the magnitudes of the costs times the capacities would add
    /// up past `i128::MAX`. The network is then as it was.
    ///
    /// ```
    /// use kahlnws::{ChangeError, Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Node 0 sends 4 units to node 2, directly at 5 a unit for up to 3 units, or through node 1 at 2 a unit
    ///     // for up to 2.
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0, 0, 1],
    ///         heads: &[2, 1, 2],
    ///         lower_bounds: None,
    ///         capacities: &[3, 4, 2],
    ///         costs: &[5, 1, 1],
    ///         supplies: &[4, 0, -4],
    ///     })?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(14));
    ///
    ///     // The second leg through node 1 costs 3 now: 2 units at 4 and 2 at 5.
    ///     let had = network.set_cost(2, 3)?;
    ///     assert_eq!((had, network.optimum()), (1, None));
    ///     assert_eq!(network.solve().outcome.optimal(), Some(18));
    ///
    ///     // Set back, the problem is the earlier one; a cost past the bound is refused, and changes nothing.
    ///     network.set_cost(2, had)?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(14));
    ///     assert!(matches!(network.set_cost(2, i64::MAX), Err(ChangeError::CostBeyondBound { arc: 2, .. })));
    ///     assert!(network.optimum().is_some());
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn set_cost(&mut self, arc: usize, cost: i64) -> Result<i64, ChangeError> {
        // How the cost stated above comes about: an arc out of the spanning tree moves no potential, and only its own
        // block is marked; an arc in it moves the potentials of the nodes that hang below it, and the arcs at the nodes
        // of the smaller side of that cut are marked, the arcs whose prices may have changed.
        let Some(slot) = self.order.slot_of(arc) else {
            set_cost_unmet(Unmet::PastTheArcs { arc, arc_count: self.arc_count() })
        };
        let Some(held) = self.held_arc(slot) else { set_cost_unmet(Unmet::HalfChanged) };
        let sums = self.sums_with_cost(arc, held, cost)?;
        if cost == held.kept.cost {
            return Ok(cost);
        }

        // Nothing is refused from here on. The first write ends the optimum held, and the last keeps the basis.
        self.standing = Standing::Changed;
        let Some(mut basis) = self.basis.reopen() else { set_cost_unmet(Unmet::HalfChanged) };
        // The cost lies within the bound of the node count, admitted above, and the basis holds the widest root cost of
        // the node count (`Network`, Basis), which every such cost asks for at most: the change does not panic.
        let moved = basis.set_cost(slot, cost);
        self.sums = sums;
        // The arc's own price changed, and those of the arcs with exactly one end in the subtree whose potentials
        // moved, where one did (`Basis::set_cost`).
        self.marks.mark_slot(slot);
        mark_sides(&mut self.marks, basis.tree(), moved);
        basis.keep();

        Ok(held.kept.cost)
    }

    /// Makes `lower_bound` the least flow of `arc`, in units of flow, and answers the lower bound it had.
    ///
    /// The network then holds the problem with that lower bound, with a basis of it, from which the next
    /// [`solve`](Self::solve) finds the optimum of the problem as it now stands; every flow of it includes its
    /// arc's lower bound. The lower bound the arc already has changes nothing, and keeps an optimum the network held;
    /// any other ends it, as the network's doc says. Setting the lower bound answered back makes the problem the
    /// earlier one again. A lower bound above the arc's capacity is refused; where both bounds are to move past each
    /// other's old value, [`set_bounds`](Self::set_bounds) moves them at once. A lower bound changes no supply: a flow
    /// it forces leaves the arc's tail and enters its head, as every flow does. The supplies net of the lower bounds
    /// may so come to pass the limit [`ProblemError::NetSupplyBeyondRange`] names, where the arcs carry the excess; the
    /// network still solves them exactly, where a [`NetworkSimplex`](crate::NetworkSimplex) refuses them.
    ///
    /// It costs a constant where the arc's flow can stay where it is, and otherwise what
    /// [`move_supply`](Self::move_supply) of the flow the change moves costs, that flow being at most the change of the
    /// lower bound; an arc at its old lower bound always moves by the change. The next solve prices the arc, and what
    /// that move of supply asks it to. It allocates nothing.
    ///
    /// # Panics
    /// Panics if `arc` is not below the arc count, which on a network never loaded no arc is; or if an earlier call on
    /// this network panicked after it began to change it, which only a defect of the crate makes: the network is then
    /// half changed, and only [`load`](Self::load) mends it. Either panic comes before any change.
    ///
    /// # Errors
    /// [`ChangeError::LowerAboveCapacity`] where `lower_bound` is above the arc's capacity, else
    /// [`ChangeError::ExcessBeyondRange`] where the flow still to be sent and the flow this change moves add up
    /// past `2^64 − 2`. The network is then as it was. The lower bound answered, set back directly after,
    /// is admitted wherever the flow still to be sent after the change and the change of the lower bound add up to at
    /// most `2^64 − 2`, so wherever that flow before the change and twice the change of the lower bound do: never
    /// refused for bounds and a flow below `2^62`.
    ///
    /// ```
    /// use kahlnws::{ChangeError, Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Node 0 sends 4 units to node 2, directly at 5 a unit for up to 3 units, or through node 1 at 2 a unit for
    ///     // up to 2.
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0, 0, 1],
    ///         heads: &[2, 1, 2],
    ///         lower_bounds: None,
    ///         capacities: &[3, 4, 2],
    ///         costs: &[5, 1, 1],
    ///         supplies: &[4, 0, -4],
    ///     })?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(14));
    ///
    ///     // The direct lane must carry all its 3 units: one more than the optimum sent by it.
    ///     assert_eq!(network.set_lower_bound(0, 3)?, 0);
    ///     network.solve();
    ///     let optimum = network.optimum().expect("the lanes should carry the supply");
    ///     assert_eq!((optimum.total_cost, optimum.flows.iter().collect::<Vec<u64>>()), (17, vec![3, 1, 1]));
    ///
    ///     // A lower bound above the capacity is refused, and changes nothing.
    ///     let refused = ChangeError::LowerAboveCapacity { arc: 0, lower_bound: 4, capacity: 3 };
    ///     assert_eq!(network.set_lower_bound(0, 4), Err(refused));
    ///     assert!(network.optimum().is_some());
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn set_lower_bound(&mut self, arc: usize, lower_bound: u64) -> Result<u64, ChangeError> {
        let Some(slot) = self.order.slot_of(arc) else {
            set_lower_bound_unmet(Unmet::PastTheArcs { arc, arc_count: self.arc_count() })
        };
        let Some(held) = self.held_arc(slot) else { set_lower_bound_unmet(Unmet::HalfChanged) };
        let bounds = Bounds { lower_bound, ..held.bounds() };

        self.rebound(arc, slot, held, bounds).map(|replaced| replaced.lower_bound)
    }

    /// Makes `capacity` the most flow of `arc`, in units of flow, and answers the capacity it had.
    ///
    /// The network then holds the problem with that capacity, with a basis of it, from which the next
    /// [`solve`](Self::solve) finds the optimum of the problem as it now stands. The capacity the arc already has
    /// changes nothing, and keeps an optimum the network held; any other ends it, as the network's doc says. Setting
    /// the capacity answered back makes the problem the earlier one again. A capacity of zero on an arc whose lower
    /// bound is zero switches the arc off: no flow can use it, and the capacity it answered, set back, switches it on.
    /// So a caller can build every arc a network may ever need up front and switch the ones it does not want off. A
    /// capacity below the arc's lower bound is refused; where both bounds are to move past each other's old value,
    /// [`set_bounds`](Self::set_bounds) moves them at once.
    ///
    /// It costs a constant where the arc's flow can stay where it is, and otherwise what
    /// [`move_supply`](Self::move_supply) of the flow the change moves costs, that flow being at most the change of the
    /// capacity; switching off an arc that carries flow always moves it. The next solve prices the arc, and what that
    /// move of supply asks it to. It allocates nothing.
    ///
    /// # Panics
    /// Panics if `arc` is not below the arc count, which on a network never loaded no arc is; or if an earlier call on
    /// this network panicked after it began to change it, which only a defect of the crate makes: the network is then
    /// half changed, and only [`load`](Self::load) mends it. Either panic comes before any change.
    ///
    /// # Errors
    /// [`ChangeError::LowerAboveCapacity`] where `capacity` is below the arc's lower bound, else
    /// [`ChangeError::CostTotalBeyondRange`] where with it the magnitudes of the costs times the capacities would add
    /// up past `i128::MAX`, else [`ChangeError::ExcessBeyondRange`] where the flow still to be sent and
    /// the flow this change moves add up past `2^64 − 2`. The network is then as it was. The capacity answered,
    /// set back directly after, is admitted wherever the flow still to be sent after the change and the change of the
    /// capacity add up to at most `2^64 − 2`, so wherever that flow before the change and twice the change of the
    /// capacity do: never refused for capacities and a flow below `2^62`.
    ///
    /// ```
    /// use kahlnws::{ChangeError, Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Node 0 sends 4 units to node 2: directly at 5 a unit for up to 3 units, through node 1 at 2 a unit for up
    ///     // to 2, or by a dear second direct lane, arc 3, at 9 a unit for up to 5.
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0, 0, 1, 0],
    ///         heads: &[2, 1, 2, 2],
    ///         lower_bounds: None,
    ///         capacities: &[3, 4, 2, 5],
    ///         costs: &[5, 1, 1, 9],
    ///         supplies: &[4, 0, -4],
    ///     })?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(14));
    ///
    ///     // Arc 1, into node 1, switched off: 3 units directly and one by the dear lane.
    ///     let capacity = network.set_capacity(1, 0)?;
    ///     network.solve();
    ///     let optimum = network.optimum().expect("the direct lanes should carry the supply");
    ///     assert_eq!((optimum.total_cost, optimum.flows.iter().collect::<Vec<u64>>()), (24, vec![3, 0, 0, 1]));
    ///
    ///     // On again with the capacity it answered: the earlier optimum.
    ///     network.set_capacity(1, capacity)?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(14));
    ///
    ///     // A capacity below the lower bound is refused, and changes nothing.
    ///     network.set_lower_bound(3, 1)?;
    ///     let refused = ChangeError::LowerAboveCapacity { arc: 3, lower_bound: 1, capacity: 0 };
    ///     assert_eq!(network.set_capacity(3, 0), Err(refused));
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn set_capacity(&mut self, arc: usize, capacity: u64) -> Result<u64, ChangeError> {
        let Some(slot) = self.order.slot_of(arc) else {
            set_capacity_unmet(Unmet::PastTheArcs { arc, arc_count: self.arc_count() })
        };
        let Some(held) = self.held_arc(slot) else { set_capacity_unmet(Unmet::HalfChanged) };
        let bounds = Bounds { capacity, ..held.bounds() };

        self.rebound(arc, slot, held, bounds).map(|replaced| replaced.capacity)
    }

    /// Makes `bounds` the least and the most flow of `arc` at once, in units of flow, and answers the bounds it had.
    ///
    /// The single setters check each bound against the other the arc holds, so where both bounds move past each
    /// other's old value, the order of two calls matters and one order is refused: bounds of `0..=2` become `3..=4` by
    /// the capacity first, and `3..=4` become `0..=2` by the lower bound first. This sets both at once, with no order,
    /// and checks the pair alone. Otherwise it is [`set_capacity`](Self::set_capacity) and
    /// [`set_lower_bound`](Self::set_lower_bound) in one: the network then holds the problem with those bounds, with a
    /// basis of it; the bounds the arc already has change nothing and keep an optimum the network held; any others end
    /// it; the bounds answered, set back, make the problem the earlier one again.
    ///
    /// It costs a constant where the arc's flow can stay where it is, and otherwise what
    /// [`move_supply`](Self::move_supply) of the flow the change moves costs, that flow being at most the larger change
    /// of the two bounds. The next solve prices the arc, and what that move of supply asks it to. It allocates nothing.
    ///
    /// # Panics
    /// Panics if `arc` is not below the arc count, which on a network never loaded no arc is; or if an earlier call on
    /// this network panicked after it began to change it, which only a defect of the crate makes: the network is then
    /// half changed, and only [`load`](Self::load) mends it. Either panic comes before any change.
    ///
    /// # Errors
    /// [`ChangeError::LowerAboveCapacity`] where the lower bound of `bounds` is above its capacity, else
    /// [`ChangeError::CostTotalBeyondRange`] where with that capacity the magnitudes of the costs times the capacities
    /// would add up past `i128::MAX`, else [`ChangeError::ExcessBeyondRange`] where the flow still to be sent
    /// and the flow this change moves add up past `2^64 − 2`. The network is then as it was. The
    /// bounds answered, set back directly after, are admitted wherever the flow still to be sent after the change and
    /// the larger change of the two bounds add up to at most `2^64 − 2`, so wherever that flow before the change and
    /// twice the larger change do: never refused for bounds and a flow below `2^62`.
    ///
    /// ```
    /// use kahlnws::{Bounds, ChangeError, Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Node 0 sends 4 units to node 2, directly at 5 a unit for up to 3 units, or through node 1 at 2 a unit for
    ///     // up to 2.
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0, 0, 1],
    ///         heads: &[2, 1, 2],
    ///         lower_bounds: None,
    ///         capacities: &[3, 4, 2],
    ///         costs: &[5, 1, 1],
    ///         supplies: &[4, 0, -4],
    ///     })?;
    ///
    ///     // Arc 2 from `0..=2` to `3..=4`: the lower bound first is refused, past the capacity held.
    ///     let refused = ChangeError::LowerAboveCapacity { arc: 2, lower_bound: 3, capacity: 2 };
    ///     assert_eq!(network.set_lower_bound(2, 3), Err(refused));
    ///
    ///     // Both at once: every unit goes through node 1, at 2 a unit.
    ///     let had = network.set_bounds(2, Bounds { lower_bound: 3, capacity: 4 })?;
    ///     assert_eq!(had, Bounds { lower_bound: 0, capacity: 2 });
    ///     assert_eq!(network.solve().outcome.optimal(), Some(8));
    ///
    ///     // Set back, the earlier problem.
    ///     network.set_bounds(2, had)?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(14));
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn set_bounds(&mut self, arc: usize, bounds: Bounds) -> Result<Bounds, ChangeError> {
        let Some(slot) = self.order.slot_of(arc) else {
            set_bounds_unmet(Unmet::PastTheArcs { arc, arc_count: self.arc_count() })
        };
        let Some(held) = self.held_arc(slot) else { set_bounds_unmet(Unmet::HalfChanged) };

        self.rebound(arc, slot, held, bounds)
    }

    /// Makes `supply` the supply of `node`, in units of flow, a demand below zero, and answers the supply it had; the
    /// supply of `balancing_node` moves by the same amount the other way, so the supplies still add up to zero.
    ///
    /// A depot that is to send 2 units where it sent 4, balanced by the customer it sends to, is
    /// `set_supply(depot, 2, customer)`: the customer then takes 2 less. The same call with the supply it answered
    /// sets both nodes back. The network then holds the problem with those supplies, with a basis of it, from which the
    /// next [`solve`](Self::solve) finds the optimum of the problem as it now stands. The supply `node` already has
    /// changes nothing, is never refused, and keeps an optimum the network held, as a move of nothing does; any other
    /// ends it. A node that balances itself is given only the supply it has, which changes nothing.
    ///
    /// It is [`move_supply`](Self::move_supply) of the difference between the supply asked for and the one `node`
    /// has: from `balancing_node` to `node` where the supply rises, from `node` to `balancing_node` where it falls. It
    /// costs and marks what that move costs and marks, and allocates nothing.
    ///
    /// # Panics
    /// Panics if `node` or `balancing_node` is not below the node count, which on a network never loaded no node is;
    /// if `node` is `balancing_node` and `supply` is not the supply it has, a change no other node balances; or if an
    /// earlier call on this network panicked after it began to change it, which only a defect of the crate makes: the
    /// network is then half changed, and only [`load`](Self::load) mends it. Each panic comes before any change.
    ///
    /// # Errors
    /// [`ChangeError::SupplyBeyondRange`] where the supply of `balancing_node` would leave the range of an `i64`,
    /// naming it with the supply it would have; else [`ChangeError::ExcessBeyondRange`] where the flow still
    /// to be sent and the difference add up past `2^64 − 2`. The network is then as it was. The supply
    /// answered, set back directly after, is admitted where the move it makes would be, as `move_supply` says.
    ///
    /// ```
    /// use kahlnws::{Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // A depot, node 0, sends 4 units to a customer, node 2, directly at 5 a unit for up to 3 units, or through
    ///     // node 1 at 2 a unit for up to 2.
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0, 0, 1],
    ///         heads: &[2, 1, 2],
    ///         lower_bounds: None,
    ///         capacities: &[3, 4, 2],
    ///         costs: &[5, 1, 1],
    ///         supplies: &[4, 0, -4],
    ///     })?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(14));
    ///
    ///     // The customer is to take 5, balanced by the depot, which sends one more: 3 directly, 2 through node 1.
    ///     let supply = network.set_supply(2, -5, 0)?;
    ///     assert_eq!((supply, network.supplies()), (-4, &[5, 0, -5][..]));
    ///     assert_eq!(network.solve().outcome.optimal(), Some(19));
    ///
    ///     // Set back with the supply it answered: both nodes are as they were.
    ///     network.set_supply(2, supply, 0)?;
    ///     assert_eq!(network.supplies(), [4, 0, -4]);
    ///
    ///     Ok(())
    /// }
    /// ```
    #[doc(alias = "set_demand")]
    #[track_caller]
    pub fn set_supply(&mut self, node: usize, supply: i64, balancing_node: usize) -> Result<i64, ChangeError> {
        let pair = match self.held_pair(node, balancing_node) {
            Ok(pair) => pair,
            Err(unmet) => set_supply_unmet(unmet),
        };
        let held = pair.supplies[0];

        // The supply rises by moving the difference from the balancing node to the node, and falls by moving it back.
        let pair = match supply.cmp(&held) {
            Ordering::Equal => return Ok(held),
            Ordering::Greater | Ordering::Less if node == balancing_node => {
                set_supply_balanced_by_itself(node, held, supply)
            }
            Ordering::Greater => pair.swapped(),
            Ordering::Less => pair,
        };
        // Two supplies within `i64` differ by at most `2^64 − 1`: the difference is a `u64`, and it is not zero.
        self.move_between(pair, supply.abs_diff(held))?;

        Ok(held)
    }

    /// Lowers the supply of `from_node` by `amount` and raises the supply of `to_node` by `amount`, in units of flow,
    /// so the supplies still add up to zero; a move of demand is written from the node that will demand more, a
    /// customer that needs two more units, sent by a depot, being `move_supply(customer, depot, 2)`.
    ///
    /// A supply below zero is a demand: `to_node`, whose supply rises, sends `amount` more or takes `amount` less,
    /// and `from_node`, whose supply falls, sends `amount` less or takes `amount` more.
    ///
    /// The network then holds the problem with those supplies, with a basis of it, from which the next
    /// [`solve`](Self::solve) finds the optimum of the problem as it now stands. The same node twice, or an amount of
    /// zero, moves nothing: it changes nothing, is never refused, and keeps an optimum the network held; any other move
    /// ends it, as the network's doc says. Moving the amount back, `move_supply(to_node, from_node, amount)`, makes the
    /// problem the earlier one again. Called directly after the move, it is admitted wherever the move left the flow
    /// still to be sent as it was or lower, always where the routed flow carried the move, and wherever that flow
    /// before the move and twice the amount add up to less than `2^64 − 1`: so never for amounts and a flow below
    /// `2^62`. Other calls in between can leave that flow higher, a solve that finds no flow among them, which does not
    /// lower it. A move back that is refused stays refused until the flow still to be sent falls, by a solve that finds
    /// a flow or by other moves; a [`load`](Self::load) of the earlier problem always undoes it. A node is
    /// brought to a supply it should have, balanced by another node, by [`set_supply`](Self::set_supply). A caller with
    /// new supplies for many nodes pairs each node whose supply rises with one whose supply falls and moves the lesser
    /// of their two remaining differences, until none is left: every supply on the way lies between its old and its new
    /// value. The supplies a network comes to hold are always within `i64`; their net supplies may pass the limit
    /// [`ProblemError::NetSupplyBeyondRange`] names where the arcs carry the excess, and the network still solves them
    /// exactly, where a [`NetworkSimplex`](crate::NetworkSimplex) refuses them.
    ///
    /// It costs a walk from each of the two nodes along the flow the network routes, as far as their routes meet, where
    /// that flow can carry the amount; where it cannot, the nodes whose potentials move, up to most of the network, and
    /// where flow comes to be left to be sent, a read of each node that heads a part of the network. The next solve
    /// prices no arc for a move the routed flow carried, and otherwise the arcs at the smaller group of the nodes whose
    /// potentials moved and the rest. It allocates nothing.
    ///
    /// # Panics
    /// Panics if `from_node` or `to_node` is not below the node count, which on a network never loaded no node is; or
    /// if an earlier call on this network panicked after it began to change it, which only a defect of the crate
    /// makes: the network is then half changed, and only [`load`](Self::load) mends it. Either panic comes before any
    /// change.
    ///
    /// # Errors
    /// [`ChangeError::SupplyBeyondRange`] where the supply of `from_node` less `amount` falls below `i64::MIN`, or the
    /// supply of `to_node` plus `amount` passes `i64::MAX`, naming the first of the two in that order; else
    /// [`ChangeError::ExcessBeyondRange`] where the flow still to be sent and `amount` add up past `2^64 − 2`. The
    /// network is then as it was.
    ///
    /// ```
    /// use kahlnws::{ChangeError, Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // A depot, node 0, sends 4 units to a customer, node 2, directly at 5 a unit for up to 3 units, or through
    ///     // node 1 at 2 a unit for up to 2.
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0, 0, 1],
    ///         heads: &[2, 1, 2],
    ///         lower_bounds: None,
    ///         capacities: &[3, 4, 2],
    ///         costs: &[5, 1, 1],
    ///         supplies: &[4, 0, -4],
    ///     })?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(14));
    ///
    ///     // The customer needs one more unit, which the depot sends: it goes directly, at 5.
    ///     network.move_supply(2, 0, 1)?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(19));
    ///
    ///     // Node 1 takes two units of the depot's supply: it sends them on to the customer at 1 a unit.
    ///     network.move_supply(0, 1, 2)?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(17));
    ///
    ///     // A supply past `i64` is refused, and changes nothing.
    ///     let refused = network.move_supply(2, 1, u64::MAX);
    ///     assert!(matches!(refused, Err(ChangeError::SupplyBeyondRange { node: 2, .. })));
    ///     assert!(network.optimum().is_some());
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn move_supply(&mut self, from_node: usize, to_node: usize, amount: u64) -> Result<(), ChangeError> {
        // How the cost stated above comes about: the move walks the paths of the spanning tree from the two nodes up to
        // where they meet, where the tree carries the amount. Each link on them that cannot carry it costs the nodes
        // that hang below it, whose potentials move, and the path from it to the root. Each node under the root whose
        // root link turns costs every node under it. Where the flow up the root links rises, the root link of each node
        // under the root is read once. It marks the arcs at the nodes of the side with fewer nodes of each cut whose
        // potentials moved; where the tree carries the amount it marks nothing.
        let pair = match self.held_pair(from_node, to_node) {
            Ok(pair) => pair,
            Err(unmet) => move_supply_unmet(unmet),
        };
        // A move of nothing leaves every supply where it is and sends nothing through the tree: no refusal applies to
        // it, and it writes nothing.
        if from_node == to_node || amount == 0 {
            return Ok(());
        }

        self.move_between(pair, amount)
    }

    /// Moves `amount`, not zero, from the supply of the first node of `pair` to that of its second, two different
    /// nodes: the step [`move_supply`](Self::move_supply) and
    /// [`set_supply`](Self::set_supply) share once they have read the two supplies. A refused move changes nothing.
    ///
    /// # Panics
    /// Panics if the room holds no basis, which the supplies read from it rule out, or a node is not below the node
    /// count.
    ///
    /// # Errors
    /// [`ChangeError::SupplyBeyondRange`] where a supply would leave the range of an `i64`, naming the first node in
    /// that order that would, else [`ChangeError::ExcessBeyondRange`], as `move_supply` says.
    #[track_caller]
    fn move_between(&mut self, pair: HeldPair, amount: u64) -> Result<(), ChangeError> {
        let [from_node, to_node] = pair.nodes;
        // The step's two preconditions beside the pair are its two callers', in this file, which check them first; the
        // pair carries the third, the supplies held at its nodes (`HeldPair`).
        debug_assert!(from_node != to_node, "a move should be between two different nodes, not node {from_node} twice");
        debug_assert!(amount != 0, "a move of nothing should have been answered before the step");
        let moved = moved_supplies(pair.nodes, pair.supplies, amount)?;
        self.sums.admit(u128::from(amount)).map_err(|total| ChangeError::ExcessBeyondRange { total })?;

        // Nothing is refused from here on. The first write ends the optimum held, and the last keeps the basis.
        self.standing = Standing::Changed;
        self.set_supplies(pair, moved);
        let basis = self.basis.reopen().expect("a network whose supplies were just read should hold its basis");
        // `amount` more must leave `to_node`, whose supply rose, and arrive at `from_node`, whose supply fell. Both are
        // below the node count of the basis held, and the flow up its root links, the excess (`Network`, Sums), and
        // `amount` add up to at most the limit, as just admitted: the route does not panic.
        let Rerouted { basis, cost_gained, up_gained } = basis.route(to_node, from_node, amount, &mut self.moved);
        // The route raised the flow up the root links by at most `amount` (`Basis::route`): the excess stays within
        // the limit, and the sums do not panic.
        self.sums.rerouted(cost_gained, up_gained);
        // An arc's violation changes only with its cost, its state or one end's potential. The route changed no cost;
        // each node it pushed tops, as it ends, the subtree whose potentials moved with it, and each arc that left the
        // tree joins such a node to the rest or stands tight (`Basis::route`): a side of each marks them all.
        mark_sides(&mut self.marks, basis.tree(), self.moved.drain(..));
        basis.keep();

        Ok(())
    }

    /// Solves the problem held: answers the total cost of its optimum, or that no flow within the bounds meets the
    /// supplies, with the [`Work`] this solve did. The optimum's flows and potentials are read through
    /// [`optimum`](Self::optimum) afterwards, which lends it until the next load, change or solve. The total cost and
    /// the potentials are those of the problem with its lower bounds, and every flow includes its arc's lower bound.
    ///
    /// It pivots from the basis the network holds: the one its last solve ended on and the changes since left, or the
    /// one a load started from. A network nothing changed since its last solve answers the same again, prices no arc
    /// and makes no pivot; a solve after changes that call for no pivot prices only the arcs those changes may have
    /// made worth using; and a solve that pivots prices at least one whole pass over the arcs. Where a change leaves
    /// much flow still to be sent and the solve makes many pivots, it can take longer than a [`load`](Self::load) of
    /// the whole problem as it stands and the solve after it. Its [`Work::first_arc_pivots`] is zero, since it tries no
    /// arcs first. It allocates nothing. A network never loaded answers the cost of the problem of no node, zero, with
    /// no work.
    ///
    /// # Panics
    /// Panics if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    ///
    /// ```
    /// use kahlnws::{Network, Outcome, Problem, ProblemError};
    ///
    /// fn main() -> Result<(), ProblemError> {
    ///     // Node 0 must send 2 units to node 1, but its one arc carries 1.
    ///     let problem = Problem {
    ///         tails: &[0],
    ///         heads: &[1],
    ///         lower_bounds: None,
    ///         capacities: &[1],
    ///         costs: &[1],
    ///         supplies: &[2, -2],
    ///     };
    ///
    ///     let mut network = Network::new();
    ///     network.load(&problem)?;
    ///     assert_eq!(network.solve().outcome, Outcome::Infeasible);
    ///     assert_eq!(network.optimum(), None);
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn solve(&mut self) -> Answer<i128> {
        // How the cost stated above comes about: the solve prices the arcs of the blocks the load or the changes since
        // marked, in blocks of `⌊√m⌋` rows, and no other until it pivots; each pivot marks every block, so that from
        // then on it searches as block search does, from where it stopped. It clears each block it prices clean, and
        // ends where none is marked. Beside the pivots and the rows it prices, it reads a few words of the marks for
        // each block it finds, a word for every 64 blocks at each pivot, and the root link of each node that hangs from
        // the root at its end.
        let work = match self.standing {
            Standing::Unloaded => Work::default(),
            Standing::Changed | Standing::Optimal => self.pivot_to_the_end(),
        };
        let outcome = match self.optimum() {
            Some(optimum) => Outcome::Optimal(optimum.total_cost),
            None => Outcome::Infeasible,
        };

        Answer { outcome, work }
    }

    /// The optimum of the problem as it stands, lent for as long as the network is, where the last call that loaded,
    /// changed or solved the network was a [`solve`](Self::solve) that found one, and the optimum of the problem of no
    /// node on a network never loaded; `None` otherwise: after a load, after a change, and after a solve that found no
    /// flow. A change that is refused, or that sets the number already held, is no change: it keeps the optimum. It
    /// reads, and costs O(1).
    ///
    /// # Panics
    /// Panics if the network stands optimal without the basis its solve ended on, which only a defect of the crate
    /// makes.
    ///
    /// ```
    /// use kahlnws::{Network, Problem, ProblemError};
    ///
    /// fn main() -> Result<(), ProblemError> {
    ///     let problem = Problem {
    ///         tails: &[0],
    ///         heads: &[1],
    ///         lower_bounds: None,
    ///         capacities: &[3],
    ///         costs: &[2],
    ///         supplies: &[3, -3],
    ///     };
    ///
    ///     let mut network = Network::new();
    ///     assert_eq!(network.optimum().map(|optimum| optimum.total_cost), Some(0), "the problem of no node");
    ///
    ///     network.load(&problem)?;
    ///     assert_eq!(network.optimum(), None, "loaded, not solved");
    ///
    ///     network.solve();
    ///     assert_eq!(network.optimum().map(|optimum| optimum.total_cost), Some(6));
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn optimum(&self) -> Option<NetworkOptimum<'_>> {
        match self.standing {
            Standing::Unloaded => Some(NetworkOptimum::OF_NO_NODE),
            Standing::Changed => None,
            Standing::Optimal => {
                // An optimal network holds its basis (`Network`): only a solve makes it optimal, and keeps the basis
                // after it does.
                let KeptColumns { arc_flows, potentials } =
                    self.basis.kept().expect("an optimal network should hold the basis its solve ended on");
                // Each arc's flow lies within its width, its capacity less its lower bound (`Network`, Basis: `Basis`,
                // Flow), and `lower_bounds` holds that arc's lower bound at the same slot (`Network`, Shape): a flow
                // and its lower bound add up to the arc's capacity at most, so no read of the view overflows.
                let arc_count = self.order.arc_count();
                debug_assert!(
                    arc_flows.len() == arc_count && self.lower_bounds.len() == arc_count,
                    "the flows and the lower bounds of an optimal network should hold one entry per arc"
                );
                // SAFETY: the flows hold one entry per arc of the basis held, and the lower bounds one per arc of the
                // problem held, whose arcs that basis is of (`Network`, Shape and Basis).
                let dealt = unsafe { DealtColumns::new_unchecked([arc_flows, &self.lower_bounds], self.order) };
                let flows = Flows::new(dealt);

                Some(NetworkOptimum { total_cost: self.sums.kept_cost(), flows, potentials })
            }
        }
    }

    /// The number of nodes of the problem held.
    pub fn node_count(&self) -> usize {
        self.order.node_count()
    }

    /// The number of arcs of the problem held.
    pub fn arc_count(&self) -> usize {
        self.order.arc_count()
    }

    /// The tail of each arc of the problem held, the node it leaves, arc `a`'s at `a`, lent for as long as the network
    /// is: the tails it was loaded with, which no change moves. On a network never loaded it is the view of no arc. It
    /// reads, changes nothing, keeps an optimum the network holds, and costs O(1) and each read O(1).
    ///
    /// # Panics
    /// Panics if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    ///
    /// ```
    /// use kahlnws::{Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0, 1],
    ///         heads: &[1, 2],
    ///         lower_bounds: None,
    ///         capacities: &[3, 3],
    ///         costs: &[1, 1],
    ///         supplies: &[3, 0, -3],
    ///     })?;
    ///     assert_eq!(network.tails().iter().collect::<Vec<usize>>(), [0, 1]);
    ///     assert_eq!(Network::new().tails().len(), 0);
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn tails(&self) -> ArcEnds<'_> {
        let problem = match self.held_problem() {
            Ok(problem) => problem,
            Err(unmet) => tails_unmet(unmet),
        };

        problem.tails()
    }

    /// The head of each arc of the problem held, the node it enters, arc `a`'s at `a`, lent for as long as the network
    /// is: the heads it was loaded with, which no change moves. On a network never loaded it is the view of no arc. It
    /// reads, changes nothing, keeps an optimum the network holds, and costs O(1) and each read O(1).
    ///
    /// # Panics
    /// Panics if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    #[track_caller]
    pub fn heads(&self) -> ArcEnds<'_> {
        let problem = match self.held_problem() {
            Ok(problem) => problem,
            Err(unmet) => heads_unmet(unmet),
        };

        problem.heads()
    }

    /// The cost of a unit of flow on each arc of the problem held, in units of cost, arc `a`'s at `a`, lent for as long
    /// as the network is: as loaded, with each cost [`set_cost`](Self::set_cost) set since and no cost a refused change
    /// named. On a network never loaded it is the view of no arc. It reads, changes nothing, keeps an optimum the
    /// network holds, and costs O(1) and each read O(1).
    ///
    /// # Panics
    /// Panics if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    ///
    /// ```
    /// use kahlnws::{Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0],
    ///         heads: &[1],
    ///         lower_bounds: None,
    ///         capacities: &[3],
    ///         costs: &[2],
    ///         supplies: &[3, -3],
    ///     })?;
    ///
    ///     // Read while the optimum is lent: a read changes nothing.
    ///     network.solve();
    ///     let optimum = network.optimum().expect("the arc should carry the supply");
    ///     assert_eq!((optimum.total_cost, network.costs().get(0)), (6, Some(2)));
    ///
    ///     // A refused cost leaves the cost read as it was; a cost set is read back.
    ///     assert!(network.set_cost(0, i64::MAX).is_err());
    ///     assert_eq!(network.costs().get(0), Some(2));
    ///     network.set_cost(0, 5)?;
    ///     assert_eq!(network.costs().get(0), Some(5));
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn costs(&self) -> Costs<'_> {
        let problem = match self.held_problem() {
            Ok(problem) => problem,
            Err(unmet) => costs_unmet(unmet),
        };

        problem.costs()
    }

    /// The lower bound of each arc of the problem held, the least flow it carries, in units of flow, arc `a`'s at `a`,
    /// lent for as long as the network is: as loaded, with the bounds each setter of bounds set since and none a
    /// refused change named. On a network never loaded it is the view of no arc. It reads, changes nothing, keeps an
    /// optimum the network holds, and costs O(1) and each read O(1).
    ///
    /// # Panics
    /// Panics if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    #[track_caller]
    pub fn lower_bounds(&self) -> FlowBounds<'_> {
        let problem = match self.held_problem() {
            Ok(problem) => problem,
            Err(unmet) => lower_bounds_unmet(unmet),
        };

        problem.lower_bounds()
    }

    /// The capacity of each arc of the problem held, the most flow it carries, in units of flow, arc `a`'s at `a`,
    /// lent for as long as the network is: as loaded, with the bounds each setter of bounds set since and none a
    /// refused change named. On a network never loaded it is the view of no arc. It reads, changes nothing, keeps an
    /// optimum the network holds, and costs O(1) and each read O(1).
    ///
    /// # Panics
    /// Panics if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    #[track_caller]
    pub fn capacities(&self) -> FlowBounds<'_> {
        let problem = match self.held_problem() {
            Ok(problem) => problem,
            Err(unmet) => capacities_unmet(unmet),
        };

        problem.capacities()
    }

    /// The supply of each node of the problem held, in units of flow, node `v`'s at `v`, lent for as long as the
    /// network is: as loaded, with each move of supply made since and none a refused move named. They add up to zero.
    /// On a network never loaded it is empty. It reads, changes nothing, keeps an optimum the network holds, and costs
    /// O(1).
    ///
    /// # Panics
    /// Panics if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    ///
    /// ```
    /// use kahlnws::{Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0],
    ///         heads: &[1],
    ///         lower_bounds: None,
    ///         capacities: &[5],
    ///         costs: &[2],
    ///         supplies: &[3, -3],
    ///     })?;
    ///     network.move_supply(1, 0, 2)?;
    ///     assert_eq!(network.supplies(), [5, -5]);
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn supplies(&self) -> &[i64] {
        let problem = match self.held_problem() {
            Ok(problem) => problem,
            Err(unmet) => supplies_unmet(unmet),
        };

        problem.supplies()
    }

    /// The tail of `arc` in the problem held, the node it leaves: the tail it was loaded with, which no change
    /// moves. It reads, changes nothing, keeps an optimum the network holds, and costs two divisions and a read.
    ///
    /// # Panics
    /// Panics if `arc` is not below the arc count, which on a network never loaded no arc is, as the setter of it
    /// does; or if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    ///
    /// ```
    /// use kahlnws::{Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0],
    ///         heads: &[1],
    ///         lower_bounds: Some(&[1]),
    ///         capacities: &[3],
    ///         costs: &[2],
    ///         supplies: &[3, -3],
    ///     })?;
    ///     network.set_cost(0, 4)?;
    ///     network.set_supply(1, -2, 0)?;
    ///
    ///     assert_eq!((network.tail(0), network.head(0), network.cost(0)), (0, 1, 4));
    ///     assert_eq!((network.lower_bound(0), network.capacity(0)), (1, 3));
    ///     assert_eq!((network.supply(0), network.supply(1)), (2, -2));
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn tail(&self, arc: usize) -> usize {
        match self.held_problem().and_then(|problem| problem.tail(arc)) {
            Ok(tail) => tail,
            Err(unmet) => tail_unmet(unmet),
        }
    }

    /// The head of `arc` in the problem held, the node it enters: the head it was loaded with, which no change
    /// moves. It reads, changes nothing, keeps an optimum the network holds, and costs two divisions and a read.
    ///
    /// # Panics
    /// Panics if `arc` is not below the arc count, which on a network never loaded no arc is, as the setter of it
    /// does; or if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    #[track_caller]
    pub fn head(&self, arc: usize) -> usize {
        match self.held_problem().and_then(|problem| problem.head(arc)) {
            Ok(head) => head,
            Err(unmet) => head_unmet(unmet),
        }
    }

    /// The cost of a unit of flow on `arc` in the problem held, in units of cost: as loaded, or as
    /// [`set_cost`](Self::set_cost) last set it. It reads, changes nothing, keeps an optimum the network holds, and
    /// costs two divisions and a read.
    ///
    /// # Panics
    /// Panics if `arc` is not below the arc count, which on a network never loaded no arc is, as the setter of it
    /// does; or if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    #[track_caller]
    pub fn cost(&self, arc: usize) -> i64 {
        match self.held_problem().and_then(|problem| problem.cost(arc)) {
            Ok(cost) => cost,
            Err(unmet) => cost_unmet(unmet),
        }
    }

    /// The lower bound of `arc` in the problem held, the least flow it carries, in units of flow: as loaded, or
    /// as a setter of bounds last set it. It reads, changes nothing, keeps an optimum the network holds, and costs two
    /// divisions and a read.
    ///
    /// # Panics
    /// Panics if `arc` is not below the arc count, which on a network never loaded no arc is, as the setter of it
    /// does; or if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    #[track_caller]
    pub fn lower_bound(&self, arc: usize) -> u64 {
        match self.held_problem().and_then(|problem| problem.lower_bound(arc)) {
            Ok(lower_bound) => lower_bound,
            Err(unmet) => lower_bound_unmet(unmet),
        }
    }

    /// The capacity of `arc` in the problem held, the most flow it carries, in units of flow: as loaded, or as a
    /// setter of bounds last set it. It reads, changes nothing, keeps an optimum the network holds, and costs two
    /// divisions and two reads.
    ///
    /// # Panics
    /// Panics if `arc` is not below the arc count, which on a network never loaded no arc is, as the setter of it
    /// does; or if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    #[track_caller]
    pub fn capacity(&self, arc: usize) -> u64 {
        match self.held_problem().and_then(|problem| problem.capacity(arc)) {
            Ok(capacity) => capacity,
            Err(unmet) => capacity_unmet(unmet),
        }
    }

    /// The [`Bounds`] of `arc` in the problem held, its lower bound and its capacity in units of flow: the pair
    /// [`set_bounds`](Self::set_bounds) takes and answers, as loaded or as a setter of bounds last set them. Read and
    /// set back, they make the arc what it was. It reads, changes nothing, keeps an optimum the network holds, and
    /// costs two divisions and two reads.
    ///
    /// # Panics
    /// Panics if `arc` is not below the arc count, which on a network never loaded no arc is, as the setter of it
    /// does; or if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    ///
    /// ```
    /// use kahlnws::{Bounds, Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0],
    ///         heads: &[1],
    ///         lower_bounds: Some(&[1]),
    ///         capacities: &[3],
    ///         costs: &[2],
    ///         supplies: &[3, -3],
    ///     })?;
    ///     let read = network.bounds(0);
    ///     assert_eq!(read, Bounds { lower_bound: 1, capacity: 3 });
    ///
    ///     // A capacity raised, then the bounds read set back at once.
    ///     network.set_capacity(0, 7)?;
    ///     assert_eq!(network.bounds(0).capacity, 7);
    ///     network.set_bounds(0, read)?;
    ///     assert_eq!(network.bounds(0), read);
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn bounds(&self, arc: usize) -> Bounds {
        match self.held_problem().and_then(|problem| problem.bounds(arc)) {
            Ok(bounds) => bounds,
            Err(unmet) => bounds_unmet(unmet),
        }
    }

    /// The supply of `node` in the problem held, in units of flow, a demand below zero: as loaded, with each
    /// change of supply made since. It reads, changes nothing, keeps an optimum the network holds, and costs O(1).
    ///
    /// # Panics
    /// Panics if `node` is not below the node count, which on a network never loaded no node is, as the setter of it
    /// does; or if an earlier call on this network panicked after it began to change it, which only a defect of the
    /// crate makes: the network is then half changed, and only [`load`](Self::load) mends it.
    #[doc(alias = "demand")]
    #[track_caller]
    pub fn supply(&self, node: usize) -> i64 {
        match self.held_problem().and_then(|problem| problem.supply(node)) {
            Ok(supply) => supply,
            Err(unmet) => supply_unmet(unmet),
        }
    }

    /// A copy of this network, as [`try_clone_from`](Self::try_clone_from) makes it into a network never loaded, or the
    /// refusal of the memory for its room: the copy for a caller that must not end the process where memory runs out,
    /// which `clone` does. It allocates the copy's room.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation of the copy's room.
    ///
    /// ```
    /// use kahlnws::{Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0],
    ///         heads: &[1],
    ///         lower_bounds: None,
    ///         capacities: &[3],
    ///         costs: &[2],
    ///         supplies: &[3, -3],
    ///     })?;
    ///     network.solve();
    ///
    ///     // The copy holds the optimum, and changes apart from the network.
    ///     let mut copy = network.try_clone()?;
    ///     assert_eq!(copy.optimum().map(|optimum| optimum.total_cost), Some(6));
    ///     copy.set_cost(0, 5)?;
    ///     assert_eq!(copy.solve().outcome.optimal(), Some(15));
    ///     assert_eq!(network.optimum().map(|optimum| optimum.total_cost), Some(6));
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn try_clone(&self) -> Result<Self, TryReserveError> {
        let mut copy = Self::new();
        copy.try_clone_from(self)?;

        Ok(copy)
    }

    /// Makes this network a copy of `source`, in the room it holds: afterwards it holds what `source` holds, the
    /// problem, the basis, the marks and the optimum, answers every call as `source` would, its work included, and
    /// changes apart from it. Where its room holds a problem of `source`'s counts, made by
    /// [`try_reserve`](Self::try_reserve) or an earlier load, it allocates nothing. A copy of a network a panic left
    /// half changed is half changed too, and only [`load`](Self::load) mends it. It costs a copy of what `source`
    /// holds, a few passes over its arcs and nodes.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation of room for `source`'s counts. This network then
    /// holds what it held, its optimum included; its room may have grown.
    ///
    /// ```
    /// use kahlnws::{Network, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Node 0 sends 4 units to node 2, directly at 5 a unit for up to 3 units, or through node 1 at 2 a unit for
    ///     // up to 2.
    ///     let mut network = Network::new();
    ///     network.load(&Problem {
    ///         tails: &[0, 0, 1],
    ///         heads: &[2, 1, 2],
    ///         lower_bounds: None,
    ///         capacities: &[3, 4, 2],
    ///         costs: &[5, 1, 1],
    ///         supplies: &[4, 0, -4],
    ///     })?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(14));
    ///
    ///     // The best network found so far, kept in room made once.
    ///     let mut best = Network::new();
    ///     best.try_reserve(network.node_count(), network.arc_count())?;
    ///     best.try_clone_from(&network)?;
    ///
    ///     // A move of the search makes the network dearer; the copy keeps its optimum, and needs no pivot.
    ///     network.set_cost(2, 3)?;
    ///     assert_eq!(network.solve().outcome.optimal(), Some(18));
    ///     assert_eq!(best.optimum().map(|optimum| optimum.total_cost), Some(14));
    ///     assert_eq!(best.solve().work.pivots, 0);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn try_clone_from(&mut self, source: &Self) -> Result<(), TryReserveError> {
        self.try_reserve(source.node_count(), source.arc_count())?;

        // Nothing is refused from here on, and nothing allocates. Where `source` holds a basis, each column it holds
        // has the length a load of its counts lays (`Network`, Shape and Marks; `Basis`, Shape), and the room a
        // reservation of those counts makes holds such a load whole, `n` entries for the record of moved subtrees among
        // it; where it holds none, the copy's columns are empty.
        self.copy_from(source);

        Ok(())
    }

    /// Makes this network a copy of `source`, the one rule of what a copy holds: `source`'s standing, counts and sums;
    /// where it holds a basis, that basis, the supplies, the lower bounds and the marks; where it holds none, never
    /// loaded or half changed, no basis, and those columns as a network never loaded holds them, which only a load
    /// reads after. The record of moved subtrees is empty, with room for a node each. The net supplies and the
    /// excesses, which only a load writes and reads, are not copied. It allocates only where the room is short of what
    /// it copies, growing as a `Vec` grows.
    fn copy_from(&mut self, source: &Self) {
        // The first write ends what this network held, and the last keeps the basis copied: between the two the room
        // holds no basis and the standing is `Changed`, as in every call.
        self.standing = Standing::Changed;
        let basis = self.basis.copy_from(&source.basis);
        // A network that holds no basis holds no column a call reads but its counts, which a load rewrites with the
        // rest (`Network`, Shape and Marks).
        let nothing = Self::new();
        let problem = if basis.is_some() { source } else { &nothing };

        // Through the `Vec`, whose `clone_from` reuses its room; the typed vector's makes a new one.
        self.supplies.raw.clone_from(&problem.supplies.raw);
        self.lower_bounds.clone_from(&problem.lower_bounds);
        self.marks.copy_from(&problem.marks);
        self.moved.clear();
        // A change pushes a node at most once, so room for every node is all the record can need.
        grow_total(&mut self.moved, source.node_count());
        (self.order, self.sums) = (source.order, source.sums);

        // The standing is written before the basis is kept, and nothing between the two can panic.
        self.standing = source.standing;
        if let Some(basis) = basis {
            basis.keep();
        }
    }

    /// The problem held, read in place: the problem of no node on a network never loaded; the refusal where the room
    /// holds no basis on a network that was loaded, the network half changed, whose columns may be half written. It
    /// reads, and changes nothing.
    ///
    /// # Errors
    /// [`Unmet::HalfChanged`] where the network is half changed.
    fn held_problem(&self) -> Result<HeldProblem<'_>, Unmet> {
        let columns = match self.standing {
            Standing::Unloaded => KeptProblem { ends: &[], costs: &[], widths: &[] },
            Standing::Changed | Standing::Optimal => self.basis.kept_problem().ok_or(Unmet::HalfChanged)?,
        };

        let (arc_count, node_count) = (self.order.arc_count(), self.order.node_count());
        debug_assert!(
            [columns.ends.len(), columns.costs.len(), columns.widths.len(), self.lower_bounds.len()]
                .iter()
                .all(|&count| count == arc_count)
                && self.supplies.len() == node_count,
            "the columns of a network should be of the counts of the problem held"
        );
        // SAFETY: where the network is never loaded every column is empty and `order` deals no arc and no node, and
        // where its room holds a basis every column holds one entry per arc and the supplies one per node of the
        // problem held (`Network`, Shape).
        Ok(unsafe { HeldProblem::new_unchecked(columns, &self.lower_bounds, &self.supplies.raw, self.order) })
    }

    /// The arc dealt to `slot` as the network holds it: its cost, its bounds and its flow; `None` where the room holds
    /// no basis, the network half changed, or where `slot` is not below the arc count. It reads, and changes nothing.
    fn held_arc(&self, slot: Slot) -> Option<HeldArc> {
        let kept = self.basis.kept_arc(slot)?;
        let at = usize::from(slot);

        debug_assert!(at < self.lower_bounds.len(), "a slot of the basis held should be below the lower bounds' m");
        // SAFETY: the room holds a basis and `slot` is below its arc count `m`, as `kept_arc` just answered
        // (`KeptBasisRoom::kept_arc`); where the room holds a basis, the lower bounds hold `m` entries (`Network`,
        // Shape).
        let lower_bound = unsafe { *self.lower_bounds.get_unchecked(at) };

        Some(HeldArc { kept, lower_bound })
    }

    /// The sums once the cost of `arc`, held as `held`, is `cost`; the refusal where `cost` lies beyond the bound of
    /// the node count, or would take the cost total past `i128::MAX`. It reads, and changes nothing.
    ///
    /// # Errors
    /// [`ChangeError::CostBeyondBound`], else [`ChangeError::CostTotalBeyondRange`], as [`set_cost`](Self::set_cost)
    /// says.
    fn sums_with_cost(&self, arc: usize, held: HeldArc, cost: i64) -> Result<KeptSums, ChangeError> {
        let node_count = self.node_count();
        let bound = cost_bound(node_count);
        if !within_cost_bound(cost, bound) {
            return Err(ChangeError::CostBeyondBound { arc, cost, node_count });
        }

        // An arc's lower bound and its width add up to its capacity (`Network`, Shape), and it carries at most its
        // width above its lower bound (`Network`, Basis: `Basis`, Flow): neither sum passes the capacity.
        let HeldArc { kept: KeptArc { cost: replaced, flow, .. }, lower_bound } = held;
        let change = CostChange { replaced, cost, capacity: held.bounds().capacity, flow: lower_bound + flow };

        self.sums.cost_changed(change).ok_or(ChangeError::CostTotalBeyondRange { arc })
    }

    /// The nodes `first` and `second` with the supplies the network holds at them; the refusal where a node is not
    /// below the node count, or, after that, where the room holds no basis, the network half changed. It reads, and
    /// changes nothing.
    ///
    /// # Errors
    /// [`Unmet::PairPastTheNodes`] where a node is not below the node count, else [`Unmet::HalfChanged`].
    fn held_pair(&self, first: usize, second: usize) -> Result<HeldPair, Unmet> {
        let node_count = self.node_count();
        if first >= node_count || second >= node_count {
            return Err(Unmet::PairPastTheNodes { first, second, node_count });
        }
        self.basis.kept().ok_or(Unmet::HalfChanged)?;

        let (first_node, second_node) = (NodeId::from(first), NodeId::from(second));
        debug_assert!(
            first < self.supplies.len() && second < self.supplies.len(),
            "the supplies of a network that holds a basis should hold one entry per node"
        );
        // SAFETY: both nodes are below the node count, as just compared, and where the room holds a basis, as just
        // read, the supplies hold one entry per node of the problem held (`Network`, Shape).
        let supplies = unsafe { [*self.supplies.get_unchecked(first_node), *self.supplies.get_unchecked(second_node)] };

        Ok(HeldPair { nodes: [first, second], supplies })
    }

    /// Makes `supplies` the supplies of the two nodes of `pair`, its first then its second.
    fn set_supplies(&mut self, pair: HeldPair, [first_supply, second_supply]: [i64; 2]) {
        let [first, second] = pair.nodes;
        let (first_node, second_node) = (NodeId::from(first), NodeId::from(second));
        debug_assert!(
            first < self.supplies.len() && second < self.supplies.len(),
            "the nodes of a held pair should be below the count of the supplies"
        );
        // SAFETY: a pair's nodes are below the count of the supplies, which nothing between its read and this write
        // changes (`HeldPair`).
        unsafe {
            *self.supplies.get_unchecked_mut(first_node) = first_supply;
            *self.supplies.get_unchecked_mut(second_node) = second_supply;
        }
    }

    /// Makes `bounds` the bounds of `arc`, dealt to `slot` and held as `held`, and answers the bounds it had: the step
    /// the three setters of bounds share once they have read the arc. Bounds the arc has change nothing; bounds a check
    /// refuses change nothing either.
    ///
    /// # Panics
    /// Panics if the room holds no basis, or `slot` is not below the arc count: `held`, read at `slot` from the basis
    /// held, rules both out.
    ///
    /// # Errors
    /// As [`bounds_change`](Self::bounds_change) answers them.
    #[track_caller]
    fn rebound(&mut self, arc: usize, slot: Slot, held: HeldArc, bounds: Bounds) -> Result<Bounds, ChangeError> {
        let replaced = held.bounds();
        if bounds == replaced {
            return Ok(replaced);
        }
        let BoundsChange { sums, restated } = self.bounds_change(arc, slot, held, bounds)?;

        // Nothing is refused from here on. The first write ends the optimum held, and the last keeps the basis.
        self.standing = Standing::Changed;
        let at = usize::from(slot);
        debug_assert!(at < self.lower_bounds.len(), "a dealt slot should be below the lower bounds' m");
        // SAFETY: `slot` is a slot the dealing answered for an arc below the arc count, so it is below that count
        // (`SlotOrder`), and the lower bounds hold one entry per slot (`Network`, Shape).
        unsafe { *self.lower_bounds.get_unchecked_mut(at) = bounds.lower_bound };
        let basis = self.basis.reopen().expect("the network should hold the basis its arc was just read from");
        // The slot is below the arc count of the basis held, and the flow up its root links, the excess (`Network`,
        // Sums), and the amount the restatement moves add up to at most the limit, as just admitted: the restatement
        // does not panic.
        let Rerouted { basis, cost_gained, up_gained } = basis.restate(slot, restated, &mut self.moved);
        // The restatement raised the flow up the root links by at most its amount (`Basis::restate`): the excess stays
        // within the limit, and the sums do not panic.
        self.sums = sums;
        self.sums.rerouted(cost_gained, up_gained);
        // The arc's own width, flow and state changed, and what the flow it moved changed, as a route changes it
        // (`Basis::restate`).
        self.marks.mark_slot(slot);
        mark_sides(&mut self.marks, basis.tree(), self.moved.drain(..));
        basis.keep();

        Ok(replaced)
    }

    /// The sums once `arc`, dealt to `slot` and held as `held`, has the bounds `bounds`, but for what the flow it moves
    /// gains, beside those bounds in the basis's terms; the refusal where the change is out of range. It reads, and
    /// changes nothing.
    ///
    /// # Panics
    /// Panics if the room holds no basis, which `held`, read from it, rules out.
    ///
    /// # Errors
    /// [`ChangeError::LowerAboveCapacity`] where the lower bound of `bounds` passes its capacity, else
    /// [`ChangeError::CostTotalBeyondRange`] where with its capacity the cost total would pass `i128::MAX`, else
    /// [`ChangeError::ExcessBeyondRange`] where the flow up the root links and the flow the change moves add up past
    /// the limit.
    #[track_caller]
    fn bounds_change(
        &self,
        arc: usize,
        slot: Slot,
        held: HeldArc,
        bounds: Bounds,
    ) -> Result<BoundsChange, ChangeError> {
        let Bounds { lower_bound, capacity } = bounds;
        if lower_bound > capacity {
            return Err(ChangeError::LowerAboveCapacity { arc, lower_bound, capacity });
        }

        let HeldArc { kept: KeptArc { cost, .. }, lower_bound: held_lower_bound } = held;
        let replaced = held.bounds().capacity;
        let sums =
            self.sums.capacity_changed(cost, replaced, capacity).ok_or(ChangeError::CostTotalBeyondRange { arc })?;

        // The lower bound is at most the capacity, as just checked: the width is the difference.
        let restated = Restated::new(held_lower_bound, lower_bound, capacity - lower_bound);
        let amount =
            self.basis.amount_to_route(slot, restated).expect("the network should hold the basis it just read");
        sums.admit(amount).map_err(|total| ChangeError::ExcessBeyondRange { total })?;

        Ok(BoundsChange { sums, restated })
    }

    /// Pivots the basis held until no arc violates its bound, by the search over the marked blocks, keeping the sums
    /// and the marks; then stands optimal where no root link carries flow, and keeps the basis. Answers the work done.
    ///
    /// # Panics
    /// Panics if the room holds no basis: the network is half changed.
    #[track_caller]
    fn pivot_to_the_end(&mut self) -> Work {
        let Self { basis, marks, sums, standing, .. } = self;
        // The first write: from here until the basis is kept again the room holds none, so the network must not stand
        // optimal, whatever unwinds in between.
        *standing = Standing::Changed;
        let Some(basis) = basis.reopen() else { solve_unmet(Unmet::HalfChanged) };

        // Every pivot ends whatever arc enters, so the loop ends (`Basis::pivot`); once the search answers none, no
        // marked block holds an arc that violates, and so no block does (`Network`, Marks).
        let mut run = Run { basis, marks, sums, search: MarkedSearch::new(), work: Work::default() };
        while run.enter_next().is_some() {}

        run.end(standing)
    }
}

/// A solve of a network under way: its basis lent, with the network's marks and sums, the search over the marked
/// blocks, and the work done so far.
///
/// # Invariant
/// `basis` is the network's basis, lent by its room; `marks` are laid for its slots and nodes, and every arc of it that
/// violates its bound lies in a marked block; `sums` keep the invariant of `KeptSums` for it, but for the excess, which
/// is at least the flow up the root links until [`end`](Self::end) sets it.
#[derive(Debug)]
struct Run<'n> {
    /// The basis pivoted.
    basis: LentBasis<'n, Basis<'n>>,
    /// The network's marks.
    marks: &'n mut MarkedBlocks,
    /// The network's sums.
    sums: &'n mut KeptSums,
    /// The search over the marked blocks.
    search: MarkedSearch,
    /// The work done so far.
    work: Work,
}

impl Run<'_> {
    /// Enters the arc the search over the marked blocks answers, where one violates its bound, and answers the flow
    /// its pivot sent; `None` exactly where no arc violates. The pivot's cost is taken off the sums, the pivot counted
    /// in the work, and every block marked before the next search: so every arc that violates lies in a marked block
    /// again.
    fn enter_next(&mut self) -> Option<u64> {
        let slot = self.search.next_entering(&self.basis.rows(), self.marks)?;
        // The search answers only slots of arcs that violate, which are below the arc count and priced: neither the
        // violation nor the pivot panics.
        let violation = self.basis.violation(slot);
        let sent = self.basis.pivot(slot);
        self.marks.mark_every_block();

        self.sums.pivoted(sent, violation);
        self.work.count_pivot(sent);

        Some(sent)
    }

    /// Ends the solve: the excess set to the flow up the root links, the network standing optimal where it is zero,
    /// and the basis kept. Answers the work done.
    fn end(self, standing: &mut Standing) -> Work {
        let Self { basis, sums, work, .. } = self;

        // The flow up the root links is at most the limit (`Basis`, Flow), so the sums do not panic; where it is zero
        // every root link carries nothing, the flow down them being equal. The standing is written before the basis is
        // kept, and nothing between the two can panic.
        let unrouted = basis.unrouted();
        sums.solved(unrouted);
        *standing = if unrouted == 0 { Standing::Optimal } else { Standing::Changed };
        basis.keep();

        work
    }
}

/// One arc of the problem a network holds, with what its basis carries on it. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HeldArc {
    /// Its cost, its width and its flow above its lower bound, as the basis holds them.
    kept: KeptArc,
    /// Its least flow, in units of flow.
    lower_bound: u64,
}

impl HeldArc {
    /// The arc's bounds: its lower bound, and its lower bound and its width added up, its capacity.
    fn bounds(self) -> Bounds {
        // An arc's lower bound and its width add up to its capacity (`Network`, Shape and Basis), within `u64`.
        Bounds::of_width(self.lower_bound, self.kept.width)
    }
}

/// Two nodes of the problem a network holds, with the supplies it holds at them: what a move of supply reads before it
/// writes. Plain data within its file.
///
/// # Invariant
/// Only [`Network::held_pair`] makes one, on a network whose room holds a basis: both nodes are below the count of the
/// supplies (`Network`, Shape), and `supplies` are those held at them. It lives within the call that made it, and
/// nothing in that call changes the supplies' count or, before [`Network::set_supplies`], the supplies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HeldPair {
    /// The two nodes, the first the one a move takes from.
    nodes: [usize; 2],
    /// The supply held at each, in units of flow.
    supplies: [i64; 2],
}

impl HeldPair {
    /// The same two nodes the other way round.
    fn swapped(self) -> Self {
        let ([first, second], [first_supply, second_supply]) = (self.nodes, self.supplies);

        Self { nodes: [second, first], supplies: [second_supply, first_supply] }
    }
}

/// What a change of an arc's bounds writes once it is admitted. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoundsChange {
    /// The sums with the arc's new capacity, before the flow the change moves gains them anything.
    sums: KeptSums,
    /// The arc's new bounds in the basis's terms: its new width and the fall of its lower bound.
    restated: Restated,
}

impl Clone for Network {
    /// A copy of the network, as [`Network::try_clone`] makes it: it answers every call as the network would, its work
    /// included, prints as it does, changes apart from it, and has room of its own for every change and for a load of
    /// its counts, so that a change, a solve or such a load of the copy allocates nothing. It allocates the copy's room
    /// and, where that memory is refused, ends the process, as a `Vec`'s `clone` does; a caller that must not end it
    /// calls [`Network::try_clone`], which answers the refusal.
    fn clone(&self) -> Self {
        let mut copy = Self::new();
        copy.copy_from(self);

        copy
    }

    /// Makes this network a copy of `source`, as [`Network::try_clone_from`] does, in the room it holds, which it grows
    /// where it is short; where that memory is refused it ends the process, as a `Vec`'s `clone_from` does.
    fn clone_from(&mut self, source: &Self) {
        self.copy_from(source);
    }
}

impl Default for Network {
    /// The network of the problem of no node, as [`Network::new`] makes it.
    fn default() -> Self {
        Self::new()
    }
}

/// The supplies of two different nodes, `from_node` then `to_node`, which `held` holds, once `amount` moves from the
/// first's to the second's; the refusal where one would leave the range of an `i64`.
///
/// # Errors
/// [`ChangeError::SupplyBeyondRange`] naming the first node in that order that would leave it, with the supply it
/// would have.
fn moved_supplies(
    [from_node, to_node]: [usize; 2],
    [from_supply, to_supply]: [i64; 2],
    amount: u64,
) -> Result<[i64; 2], ChangeError> {
    // A supply within `i64` and an amount below `2^64` add up within `±2^65`: exact in an `i128`.
    let within =
        |node: usize, supply: i128| i64::try_from(supply).map_err(|_| ChangeError::SupplyBeyondRange { node, supply });
    let from = within(from_node, i128::from(from_supply) - i128::from(amount))?;
    let to = within(to_node, i128::from(to_supply) + i128::from(amount))?;

    Ok([from, to])
}

/// Rewrites `lower_bounds` as the lower bound of each arc of `problem` at the slot the dealing gives it, `m` entries.
fn lay_lower_bounds(lower_bounds: &mut Vec<u64>, problem: CheckedProblem<'_>) {
    let (arc_count, node_count) = (problem.arc_count(), problem.node_count());
    refill(lower_bounds, iter::repeat_n(0, arc_count));

    let CheckedArcs::WithLowerBounds(arcs) = problem.arcs() else {
        return;
    };

    for (arc, slot) in arcs.zip(DealtSlotsIter::new(arc_count, node_count)) {
        let at = usize::from(slot);
        debug_assert!(at < lower_bounds.len(), "a dealt slot should be below the arc count, the length of the column");
        // SAFETY: `DealtSlotsIter` yields only slots below the arc count it was made for (its invariant), the length
        // the column was refilled to above.
        unsafe { *lower_bounds.get_unchecked_mut(at) = arc.lower_bound() };
    }
}

/// Marks in `marks` the arcs at the nodes of the smaller side of the cut above each node of `tops`, a node of `tree`
/// whose subtree's potentials moved: every arc whose reduced cost the move changed has an end there.
///
/// # Panics
/// Panics if a node of `tops` is not below the node count of `tree`, or if `marks` were laid for fewer nodes than
/// `tree` has: a side holds nodes of its tree, the root among them.
#[track_caller]
fn mark_sides(marks: &mut MarkedBlocks, tree: &SpanningTree<'_>, tops: impl IntoIterator<Item = usize>) {
    for top in tops {
        let side = tree.smaller_side(top).expect("the top of a moved subtree should be a hung node");
        marks.mark_nodes(side);
    }
}

/// Lays `marks` for the arcs of `problem`, each at the slot the dealing gives it with its two ends, and marks every
/// block.
fn lay_marks(marks: &mut MarkedBlocks, problem: CheckedProblem<'_>) {
    let (arc_count, node_count) = (problem.arc_count(), problem.node_count());
    let slots = DealtSlotsIter::new(arc_count, node_count);
    // A checked problem's arcs have their ends below its node count, and the dealing gives each arc a slot below its
    // arc count: the lay does not panic.
    let at_slot = |(arc, slot): (CheckedArc, Slot)| (slot, usize::from(arc.tail()), usize::from(arc.head()));

    match problem.arcs() {
        CheckedArcs::WithoutLowerBounds(arcs) => marks.lay(arc_count, node_count, arcs.zip(slots).map(at_slot)),
        CheckedArcs::WithLowerBounds(arcs) => marks.lay(arc_count, node_count, arcs.zip(slots).map(at_slot)),
    }
}

/// Panics for a `node` that is its own balancing node and is to change from `held` to `supply`, as
/// [`Network::set_supply`] documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: a call that names one node twice has no change to make. Its caller tracks its own caller
// too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn set_supply_balanced_by_itself(node: usize, held: i64, supply: i64) -> ! {
    panic!(
        "called `Network::set_supply` with node {node} as its own balancing node, whose supply {held} should then \
         stay, not become {supply}"
    )
}

/// Panics for a call of [`Network::set_cost`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn set_cost_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::set_cost` {unmet}")
}

/// Panics for a call of [`Network::set_lower_bound`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn set_lower_bound_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::set_lower_bound` {unmet}")
}

/// Panics for a call of [`Network::set_capacity`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn set_capacity_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::set_capacity` {unmet}")
}

/// Panics for a call of [`Network::set_bounds`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn set_bounds_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::set_bounds` {unmet}")
}

/// Panics for a call of [`Network::set_supply`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn set_supply_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::set_supply` {unmet}")
}

/// Panics for a call of [`Network::move_supply`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn move_supply_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::move_supply` {unmet}")
}

/// Panics for a call of [`Network::solve`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn solve_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::solve` {unmet}")
}

/// Panics for a call of [`Network::tails`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn tails_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::tails` {unmet}")
}

/// Panics for a call of [`Network::heads`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn heads_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::heads` {unmet}")
}

/// Panics for a call of [`Network::costs`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn costs_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::costs` {unmet}")
}

/// Panics for a call of [`Network::lower_bounds`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn lower_bounds_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::lower_bounds` {unmet}")
}

/// Panics for a call of [`Network::capacities`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn capacities_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::capacities` {unmet}")
}

/// Panics for a call of [`Network::supplies`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn supplies_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::supplies` {unmet}")
}

/// Panics for a call of [`Network::tail`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn tail_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::tail` {unmet}")
}

/// Panics for a call of [`Network::head`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn head_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::head` {unmet}")
}

/// Panics for a call of [`Network::cost`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn cost_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::cost` {unmet}")
}

/// Panics for a call of [`Network::lower_bound`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn lower_bound_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::lower_bound` {unmet}")
}

/// Panics for a call of [`Network::capacity`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn capacity_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::capacity` {unmet}")
}

/// Panics for a call of [`Network::bounds`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn bounds_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::bounds` {unmet}")
}

/// Panics for a call of [`Network::supply`] whose condition `unmet` names, as it documents.
///
/// # Panics
/// Always: that is its job.
// Cold and never inlined: only a caller's arc or node past the counts, or a defect of the crate, reaches it, so
// the call keeps only its comparisons. Its caller tracks its own caller too, so the panic names the line of the call.
#[cold]
#[inline(never)]
#[track_caller]
fn supply_unmet(unmet: Unmet) -> ! {
    panic!("called `Network::supply` {unmet}")
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use kahlnws_testing::{
        AmountDraw, BoundDraw, CostDraw, Family, MincostNetwork, TinyBounds, TinyCosts, amount_draws, bound_draws,
        brute_force_optimum, cost_draws, tiny_networks,
    };
    use num_bigint::BigInt;
    use proptest::prelude::*;
    use proptest::sample::Index;
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;
    use rand::{RngExt, SeedableRng};
    use rand_xoshiro::SplitMix64;

    use super::*;
    use crate::basis::{
        check_at, extended_cost_at, flow_up_the_root_links, root_link_directions, tree_arcs, untie_the_kept_tree,
        violating_slots,
    };
    use crate::network::marked_blocks::tests::{clear_every_block, is_marked, priced_rows};
    use crate::problem::tests::problem_of;
    use crate::ranges::NET_SUPPLY_LIMIT;
    use crate::tree::Direction;

    impl Network {
        /// The room of each of its columns, in entries: its basis's, then its supplies', lower bounds', moved nodes'
        /// and marks'.
        fn room_capacities(&self) -> Vec<usize> {
            let held = [self.supplies.capacity(), self.lower_bounds.capacity(), self.moved.capacity()];

            self.basis.capacities().into_iter().chain(held).chain(self.marks.capacities()).collect()
        }
    }

    #[test]
    fn test_grow_makes_the_room_try_reserve_makes() {
        // 100 nodes and 90 000 arcs: every column's total lies above the least room an empty `Vec` makes, 8 one-byte
        // entries or 4 wider ones, so each total is the room made, and a total that drifts by one shows. The one-byte
        // directions hold 100, and the marks' words those of `⌈90 000 / 300⌉ = 300` blocks, `⌈300 / 64⌉ = 5`.
        let (mut grown, mut reserved) = (Network::new(), Network::new());
        grown.grow(100, 90_000);
        reserved.try_reserve(100, 90_000).expect("100 nodes and 90 000 arcs should fit");
        assert_eq!(grown.room_capacities(), reserved.room_capacities());
    }

    /// The depot: node 0 sends 4 units to node 2, directly by arc 0 (capacity 3, cost 5) or through node 1 by arcs 1
    /// (capacity 4, cost 1) and 2 (capacity 2, cost 1). Three nodes and three arcs: slot `s` holds arc `s`.
    fn depot() -> MincostNetwork {
        MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1)])
    }

    /// A network that holds `network`, loaded.
    fn loaded(network: &MincostNetwork) -> Network {
        let mut held = Network::new();
        held.load(&problem_of(network)).expect("the test's problem should be admitted");

        held
    }

    #[test]
    fn test_a_load_holds_the_star_at_the_widest_root_cost_by_hand() {
        // The depot's star at `M = 3074457345618258603`, the widest root cost of three nodes: node 2 hangs down from
        // the root with its demand of 4, so the flow of the extended network costs `4·M`. Nothing is optimal yet.
        let network = loaded(&depot());
        let widest: i64 = 3_074_457_345_618_258_603;

        assert_eq!(network.standing, Standing::Changed);
        assert_eq!(network.order, SlotOrder::new(3, 3));
        assert_eq!(network.lower_bounds, [0, 0, 0]);
        assert_eq!(network.sums.kept_cost(), 4 * i128::from(widest));
        let kept = network.basis.kept().expect("a loaded network should hold its basis");
        assert_eq!(kept, KeptColumns { arc_flows: &[0, 0, 0], potentials: &[0, 0, widest] });
    }

    #[test]
    fn test_the_depot_solved_from_the_widest_star_pivot_by_pivot_by_hand() {
        // Blocks of one row. Arc 0, at `5 − M`, fills: 3 units, no link leaves. Arc 2, at `1 − M`, enters
        // degenerately, node 1's empty root link leaving: node 1 hangs from node 2 at `M − 1`. Arc 1, at
        // `1 − (M − 1)`, sends one unit round by both hub arcs, node 2's root link leaving: nodes 1 and 2 at 1 and 2.
        // Arc 0, at its capacity of reduced cost `5 − 2 = 3`, gives one unit back to the hub, arc 2 filling and
        // leaving: node 2 hangs from node 0 at 5. The cost falls from `4·M` by `3·(M − 5)`, 0, `M − 2` and 3 to 14.
        let mut network = loaded(&depot());

        let answer = network.solve();
        assert_eq!(
            answer,
            Answer {
                outcome: Outcome::Optimal(14),
                work: Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 }
            }
        );
        let optimum = network.optimum().expect("the depot should route its supply");
        assert_eq!(optimum.total_cost, 14);
        assert_eq!(optimum.flows.iter().collect::<Vec<_>>(), [2, 2, 2]);
        assert_eq!(optimum.potentials, [0, 1, 5]);
        assert_eq!(network.standing, Standing::Optimal);
    }

    #[test]
    fn test_a_load_marks_every_block_and_a_solve_clears_them_all_by_hand() {
        // The depot's three slots, in blocks of one row: all three marked by the load. The solve prices each at least
        // once and ends with none marked; solved again, nothing marked, it prices no row and makes no pivot.
        let mut network = loaded(&depot());
        let marked = |network: &Network| (0..3_usize).filter(|&at| is_marked(&network.marks, Slot::from(at))).count();
        assert_eq!((marked(&network), priced_rows(&network.marks)), (3, 0));

        let _ = network.solve();
        let priced = priced_rows(&network.marks);
        assert!(priced >= 3, "the first solve prices every row once at least, not {priced}");
        assert_eq!(marked(&network), 0);

        assert_eq!(network.solve().work, Work::default());
        assert_eq!(priced_rows(&network.marks), priced, "nothing marked: no row priced");
    }

    #[test]
    fn test_lay_marks_deals_each_arc_to_its_slot_with_its_ends_by_hand() {
        // Ten arcs on four nodes, dealt to the slots 0, 3, 6, 9, 1, 4, 7, 2, 5, 8, in blocks of three: arc `a` from
        // node `a % 4` to node `(a + 1) % 4`. Node 0 is an end of arcs 0, 3, 4, 7 and 8, at slots 0, 9, 1, 2 and 5:
        // blocks 0, 3 and 1, which hold slots 0 to 5 and 9.
        let mut network = MincostNetwork::default();
        for _ in 0..4 {
            network.add_node(0);
        }
        for arc in 0..10 {
            network.add_arc(arc % 4, (arc + 1) % 4, 0, 1, 1);
        }
        let problem = problem_of(&network);
        let mut marks = MarkedBlocks::new();
        lay_marks(&mut marks, problem.checked().expect("the ten arcs should be admitted"));

        let mut node_zero = marks.clone();
        clear_every_block(&mut node_zero);
        node_zero.mark_nodes([0]);
        let slots: Vec<usize> = (0..10_usize).filter(|&at| is_marked(&node_zero, Slot::from(at))).collect();
        assert_eq!(slots, [0, 1, 2, 3, 4, 5, 9]);
        assert!((0..10_usize).all(|at| is_marked(&marks, Slot::from(at))), "a lay marks every block");
    }

    #[test]
    fn test_lay_lower_bounds_deals_them_to_their_slots_by_hand() {
        // Ten arcs on four nodes, dealt to the slots 0, 3, 6, 9, 1, 4, 7, 2, 5, 8: arc `a` of lower bound `a`, cost
        // `a − 4` and capacity 20.
        let mut network = MincostNetwork::default();
        for _ in 0..4 {
            network.add_node(0);
        }
        for arc in 0..10_u8 {
            network.add_arc(0, 1, u64::from(arc), 20, i64::from(arc) - 4);
        }
        let problem = problem_of(&network);
        let checked = problem.checked().expect("the ten arcs should be admitted");

        let mut column = vec![7; 2];
        lay_lower_bounds(&mut column, checked);
        assert_eq!(column, [0, 4, 7, 1, 5, 8, 2, 6, 9, 3]);

        let without = Problem { lower_bounds: None, ..problem };
        let checked = without.checked().expect("the ten arcs should be admitted");
        lay_lower_bounds(&mut column, checked);
        assert_eq!(column, [0; 10]);
    }

    #[test]
    fn test_a_network_never_loaded_stands_unloaded_and_answers_the_problem_of_no_node() {
        let mut network = Network::default();
        assert_eq!(network.standing, Standing::Unloaded);
        assert_eq!(network.basis.kept(), None);

        let answer = network.solve();
        assert_eq!(answer, Answer { outcome: Outcome::Optimal(0), work: Work::default() });
        assert_eq!(network.standing, Standing::Unloaded, "a solve of no node writes nothing");
    }

    /// The state an unwind after the first write of a call leaves, made by the private steps themselves: the standing
    /// written, the basis taken out of the room and dropped unkept.
    fn half_changed(network: &mut Network) {
        network.standing = Standing::Changed;
        assert!(network.basis.reopen().is_some(), "the network should hold its basis");
    }

    #[test]
    fn test_a_half_changed_network_answers_no_optimum_keeps_its_counts_and_is_mended_by_a_load() {
        let mut network = loaded(&depot());
        let _ = network.solve();
        half_changed(&mut network);

        assert_eq!(network.optimum(), None);
        assert_eq!((network.node_count(), network.arc_count()), (3, 3));
        assert_eq!(network.basis.kept(), None);
        let copy = network.clone();
        assert_eq!((copy.standing, copy.basis.kept()), (Standing::Changed, None), "it copies as half changed");
        assert!(network.try_reserve(4, 4).is_ok());
        assert!(!format!("{network:?}").is_empty());

        network.load(&problem_of(&depot())).expect("the depot should be admitted");
        let answer = network.solve();
        assert_eq!(answer.outcome.optimal(), Some(14));
    }

    #[test]
    fn test_a_solve_of_a_half_changed_network_panics() {
        let mut network = loaded(&depot());
        half_changed(&mut network);

        let unwound = catch_unwind(AssertUnwindSafe(|| {
            let _ = network.solve();
        }));
        let message = unwound.expect_err("the solve should panic");
        assert_eq!(panic_text(&message), Some("called `Network::solve` on a network a panic left half changed"));
        assert_eq!((network.optimum(), network.node_count()), (None, 3), "the panic changed nothing");
    }

    #[test]
    fn test_a_solve_from_an_optimal_network_that_unwinds_leaves_it_changed() {
        // A solved, optimal network whose basis is taken out of the room, the standing left as it was: the state a
        // solve that unwound between its reopen and its keep would leave, were the standing written last. A solve of
        // it unwinds at its reopen; afterwards the network must not stand optimal without a basis held.
        let mut network = loaded(&depot());
        let _ = network.solve();
        assert_eq!(network.standing, Standing::Optimal);
        assert!(network.basis.reopen().is_some(), "the network should hold its basis");

        let unwound = catch_unwind(AssertUnwindSafe(|| {
            let _ = network.solve();
        }));
        assert!(unwound.is_err(), "a solve of a network that holds no basis should panic");

        let optimal_without_basis = network.standing == Standing::Optimal && network.basis.kept().is_none();
        assert!(!optimal_without_basis, "an optimal network should hold its basis");
        assert_eq!(network.standing, Standing::Changed);
    }

    #[test]
    fn test_a_load_empties_the_record_of_moved_subtrees_an_unwind_left() {
        // A network half changed with a node of ten pushed and left behind, as a route that unwound would leave it,
        // then loaded with the depot of three nodes: a cut of node 2 marks the side of node 2 alone.
        let mut network = loaded(&MincostNetwork::from_arcs(&[0; 10], &[(0, 9, 0, 1, 1)]));
        half_changed(&mut network);
        network.moved.push(9);

        network.load(&problem_of(&depot())).expect("the depot should be admitted");
        let _ = network.solve();
        assert_eq!(network.set_capacity(0, 1), Ok(3));
        assert_eq!((network.moved.len(), marked_slots(&network)), (0, vec![0, 2]));
    }

    #[test]
    fn test_a_half_changed_network_never_loaded_before_is_mended_by_a_load() {
        // A load that unwinds after its first write leaves a network never loaded half changed too: no counts yet.
        let mut network = Network::new();
        network.standing = Standing::Changed;

        assert_eq!((network.optimum(), network.node_count(), network.arc_count()), (None, 0, 0));
        network.load(&problem_of(&depot())).expect("the depot should be admitted");
        assert_eq!(network.solve().work.pivots, 4);
    }

    /// The depot loaded and solved: its optimum carries 2 on every arc; node 0 hangs up from the root carrying nothing,
    /// nodes 1 and 2 below it by arcs 1 and 0, which run down to them; arc 2 stands at its capacity out of the tree.
    /// The potentials are 0, 1 and 5.
    ///
    /// # Panics
    /// Panics if its solve does not answer the optimum 14.
    fn solved_depot() -> Network {
        let mut network = loaded(&depot());
        assert_eq!(solved(&mut network), (Some(14), Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 }));

        network
    }

    /// The flows of the arcs and the potentials of the nodes of the basis `network` holds, read in place.
    ///
    /// # Panics
    /// Panics if the network holds no basis.
    fn kept_of(network: &Network) -> (Vec<u64>, Vec<i64>) {
        let kept = network.basis.kept().expect("the network should hold its basis");

        (kept.arc_flows.to_vec(), kept.potentials.to_vec())
    }

    /// The cost total, the kept cost and the excess `network` keeps.
    fn sums_held(network: &Network) -> (u128, i128, u64) {
        (network.sums.cost_total(), network.sums.kept_cost(), network.sums.excess())
    }

    #[test]
    fn test_a_load_of_less_supply_starts_from_the_basis_held_by_hand() {
        // The depot's optimum held, then the depot with 3 units to send. Arc 2, at its capacity out of the tree,
        // carries its width 2: the excesses are 3, −2 and −1. From the leaves up, node 2 takes one unit down arc 0 and
        // node 1 two down arc 1, each within its width and above nothing, so both stay; node 0 is left nothing, up its
        // root link. The potentials are the depot's, 0, 1 and 5, node 0 up from the root at nothing whatever the root
        // cost. The flow costs `5 + 2 + 2 = 9`, nothing on a root link; the cost total is `15 + 4 + 2 = 21`. Every
        // block is marked, and the solve makes no pivot: arc 2 at its capacity prices `1 + 1 − 5 = −3`.
        let fewer = MincostNetwork { supplies: vec![3, 0, -3], ..depot() };
        let mut network = solved_depot();

        network.load(&problem_of(&fewer)).expect("the depot should be admitted");
        assert_eq!((network.standing, network.optimum()), (Standing::Changed, None));
        assert_eq!(kept_of(&network), (vec![1, 2, 2], vec![0, 1, 5]));
        assert_eq!(sums_held(&network), (21, 9, 0));
        assert_eq!(marked_slots(&network), [0, 1, 2]);
        assert_eq!(checked(&mut network, &fewer), Ok(vec![0, 1, 5]));
        assert_eq!(solved(&mut network), (Some(9), Work::default()));

        // A network that held nothing starts from the star, and pivots.
        let fresh = solved(&mut loaded(&fewer));
        assert_eq!(fresh.0, Some(9));
        assert!(fresh.1.pivots > 0, "{fresh:?}");
    }

    #[test]
    fn test_a_load_that_switches_a_tree_arc_off_hangs_its_node_at_the_widest_root_cost_by_hand() {
        // Arc 1 of capacity 0: the excesses are 4, −2 and −2. Node 2 takes its 2 down arc 0 and stays; node 1 can take
        // nothing down arc 1, which leaves the tree unpriced, and hangs down from the root by its root link, which
        // brings it 2, at `M`, the widest root cost; node 0 sends 2 up its root link. The arcs' flow costs
        // `5·2 + 1·2 = 12`, node 1's root link `2·M`, and 2 units run up the root links; the cost total is 17.
        let widest = DEPOT_ROOT_COST;
        assert_eq!(widest, widest_root_cost(3));
        let mut held = depot();
        held.capacities[1] = 0;
        let mut network = solved_depot();

        network.load(&problem_of(&held)).expect("the depot should be admitted");
        assert_eq!(kept_of(&network), (vec![2, 0, 2], vec![0, widest, 5]));
        assert_eq!(sums_held(&network), (17, 12 + 2 * i128::from(widest), 2));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, widest, 5]));

        // Arc 2 at its capacity prices `1 + M − 5` and enters, sending from node 2: arc 0 has room for one unit, the
        // least, and leaves at its capacity; node 2 hangs from node 1 by arc 2 at `M + 1`. The cost falls by `M − 4` to
        // `16 + M`, one unit still up and one down the root links: no flow, as a fresh network finds too.
        let work = Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 0 };
        assert_eq!(solved(&mut network), (None, work));
        assert_eq!(kept_of(&network), (vec![3, 0, 1], vec![0, widest, widest + 1]));
        assert_eq!(sums_held(&network), (17, 16 + i128::from(widest), 1));
        assert_eq!(solved(&mut loaded(&held)).0, None);
    }

    #[test]
    fn test_a_load_of_a_dearer_tree_arc_keeps_the_tree_and_the_flows_by_hand() {
        // Arc 1 at 6: nothing moves, node 1 sits at 6 below arc 1, and the flow costs `10 + 12 + 2 = 24` of the total
        // `15 + 24 + 2 = 41`. Arc 2 at its capacity prices `1 + 6 − 5 = 2` and enters, sending from node 2: arc 0 has
        // room for one unit, the least, and leaves at its capacity; node 2 hangs from node 1 at 7. One unit moves from
        // the way through node 1 to arc 0: `24 − 2 = 22`.
        let mut held = depot();
        held.costs[1] = 6;
        let mut network = solved_depot();

        network.load(&problem_of(&held)).expect("the depot should be admitted");
        assert_eq!(kept_of(&network), (vec![2, 2, 2], vec![0, 6, 5]));
        assert_eq!(sums_held(&network), (41, 24, 0));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 6, 5]));

        assert_eq!(solved(&mut network), (Some(22), Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 0 }));
        assert_eq!(kept_of(&network), (vec![3, 1, 1], vec![0, 6, 7]));
    }

    #[test]
    fn test_the_same_problem_loaded_again_makes_no_pivot_by_hand() {
        // The depot's optimum held, and the depot loaded again: the re-flow leaves it as it was, the flow of cost 14,
        // and the solve makes no pivot where a fresh network makes four. Then the depot with 3 units, and the depot
        // again from that basis: arc 2 carries 2 again, node 2 takes 2 down arc 0 and node 1 2 down arc 1, the optimum.
        let mut network = solved_depot();
        let before = kept_of(&network);

        network.load(&problem_of(&depot())).expect("the depot should be admitted");
        assert_eq!((kept_of(&network), sums_held(&network)), (before.clone(), (21, 14, 0)));
        assert_eq!(solved(&mut network), (Some(14), Work::default()));

        let fewer = MincostNetwork { supplies: vec![3, 0, -3], ..depot() };
        network.load(&problem_of(&fewer)).expect("the depot should be admitted");
        assert_eq!(solved(&mut network), (Some(9), Work::default()));
        network.load(&problem_of(&depot())).expect("the depot should be admitted");
        assert_eq!(kept_of(&network), before);
        assert_eq!(solved(&mut network), (Some(14), Work::default()));
    }

    /// Checks that `network`, loaded with `held`, holds what a network never loaded holds once loaded with it: the
    /// basis column for column, the sums, the standing and the blocks marked; and that its solve answers what that
    /// network's answers, the optimum and the work.
    ///
    /// # Panics
    /// Panics at the first difference, or if the problem is refused.
    fn assert_loads_as_fresh(network: &mut Network, held: &MincostNetwork) {
        network.load(&problem_of(held)).expect("the test's problem should be admitted");
        let mut fresh = loaded(held);

        assert_eq!(format!("{:?}", network.basis), format!("{:?}", fresh.basis));
        assert_eq!((network.sums, network.standing), (fresh.sums, fresh.standing));
        assert_eq!(marked_slots(network), marked_slots(&fresh));
        assert_eq!(answered(network), answered(&mut fresh));
    }

    #[test]
    fn test_every_way_to_the_star_loads_as_a_fresh_network() {
        // Nothing held: a network never loaded.
        assert_loads_as_fresh(&mut Network::new(), &depot());

        // Other counts: a node more, an arc more, an arc fewer.
        let more_nodes =
            MincostNetwork::from_arcs(&[4, 0, -4, 0], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1)]);
        let more_arcs = MincostNetwork::from_arcs(
            &[4, 0, -4],
            &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1), (0, 2, 0, 5, 9)],
        );
        let fewer_arcs = MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1)]);
        // The same counts, arc 2 turned around: another tail and head at its slot.
        let turned = MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (2, 1, 0, 2, 1)]);
        for other in [&more_nodes, &more_arcs, &fewer_arcs, &turned] {
            assert_loads_as_fresh(&mut solved_depot(), other);
        }

        // A network a panic left half changed holds no basis.
        let mut network = solved_depot();
        half_changed(&mut network);
        assert_loads_as_fresh(&mut network, &depot());
    }

    #[test]
    fn test_excesses_past_the_limit_load_as_a_fresh_network_and_at_it_from_the_basis_held() {
        // Two arcs of cost −1 between two nodes of no supply, each of capacity 1: the solve fills the cycle, one arc
        // carrying its unit in the tree and the other at its capacity out of it. Loaded with that arc as wide as a
        // `u64` holds, it carries `2^64 − 1` from its tail to its head out of the tree: one past the limit, the star.
        // One unit narrower, its excess is the limit, `2^64 − 2`, and one unit narrower still one below it: each starts
        // from the basis held, whose tree arc carries what arrives, within its width.
        let cycle = MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, 1, -1), (1, 0, 0, 1, -1)]);
        let held_cycle = || {
            let mut network = loaded(&cycle);
            assert_eq!(solved(&mut network).0, Some(-2));
            network
        };
        let in_tree = tree_arcs_of(&mut held_cycle());
        assert_eq!(in_tree.iter().filter(|&&in_tree| in_tree).count(), 1, "one arc in the tree: {in_tree:?}");
        let full = in_tree.iter().position(|&in_tree| !in_tree).expect("one arc should be out of the tree");
        let widened = |width: u64| {
            let mut capacities = vec![u64::MAX; 2];
            capacities[full] = width;
            MincostNetwork { capacities, ..cycle.clone() }
        };

        assert_loads_as_fresh(&mut held_cycle(), &widened(u64::MAX));
        for width in [u64::MAX - 1, u64::MAX - 2] {
            let held = widened(width);
            let mut network = held_cycle();
            network.load(&problem_of(&held)).expect("the cycle should be admitted");
            assert_eq!(tree_arcs_of(&mut network), in_tree, "the tree arc stays");
            assert_eq!(sums_held(&network).2, 0, "the tree arc carries the excess");
            assert!(checked(&mut network, &held).is_ok());
            assert_eq!(solved(&mut network), (solvers_cost(&held), Work::default()));
        }
    }

    #[test]
    fn test_a_refused_load_between_two_loads_leaves_the_network_as_it_was() {
        // Refused at its check, by supplies that do not balance, or at its net supplies, past their limit, the network
        // holds exactly what it held, as its whole `Debug` shows: the scratch rooms a refusal at the net supplies
        // rewrites are left out of it. The solve after either keeps the optimum, and the next load starts from the
        // basis held.
        let unbalanced = MincostNetwork { supplies: vec![4, 0, -3], ..depot() };
        let past_limit = MincostNetwork::from_arcs(&[i64::MAX, i64::MIN + 1], &[(1, 0, 1 << 63, 1 << 63, 0)]);
        let mut network = solved_depot();

        let whole = format!("{network:?}");
        assert_eq!(network.load(&problem_of(&unbalanced)), Err(ProblemError::Unbalanced { total: 1 }));
        assert_eq!(format!("{network:?}"), whole);
        let refused = ProblemError::NetSupplyBeyondRange { total: u128::from(u64::MAX) };
        assert_eq!(network.load(&problem_of(&past_limit)), Err(refused));
        assert_eq!(format!("{network:?}"), whole);
        assert_eq!(network.optimum().map(|optimum| optimum.total_cost), Some(14));
        assert_eq!(solved(&mut network), (Some(14), Work::default()));

        let fewer = MincostNetwork { supplies: vec![3, 0, -3], ..depot() };
        network.load(&problem_of(&fewer)).expect("the depot should be admitted");
        assert_eq!(solved(&mut network), (Some(9), Work::default()));
    }

    #[test]
    fn test_a_load_after_a_load_of_other_counts_starts_from_the_star_then_from_its_basis() {
        // The depot held, then a problem of two nodes, then the depot: the third load holds no basis of the depot's
        // counts and starts from the star, as a fresh network; a fourth, of fewer supplies, from the depot's basis.
        let mut network = solved_depot();
        network
            .load(&problem_of(&MincostNetwork::from_arcs(&[2, -2], &[(0, 1, 0, 1, 1)])))
            .expect("the problem should be admitted");
        assert_eq!(solved(&mut network).0, None);

        assert_loads_as_fresh(&mut network, &depot());
        let fewer = MincostNetwork { supplies: vec![3, 0, -3], ..depot() };
        network.load(&problem_of(&fewer)).expect("the depot should be admitted");
        assert_eq!(kept_of(&network), (vec![1, 2, 2], vec![0, 1, 5]));
    }

    #[test]
    fn test_a_copy_loads_as_its_source_does() {
        // A copy of the depot's optimum, by `clone` and into a network that held another problem, loaded with fewer
        // supplies: each starts from the basis it holds, as its source does, and holds and answers alike.
        let fewer = MincostNetwork { supplies: vec![3, 0, -3], ..depot() };
        let mut source = solved_depot();
        let mut cloned = source.clone();
        let mut copied = loaded(&MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 1, 1)]));
        copied.try_clone_from(&source).expect("room for the copy should be granted");

        for network in [&mut source, &mut cloned, &mut copied] {
            network.load(&problem_of(&fewer)).expect("the depot should be admitted");
        }
        assert_eq!((format!("{cloned:?}"), format!("{copied:?}")), (format!("{source:?}"), format!("{source:?}")));
        assert_eq!(kept_of(&source), (vec![1, 2, 2], vec![0, 1, 5]));
        for network in [&mut cloned, &mut copied] {
            assert_eq!(answered(network), answered(&mut source.clone()));
        }
    }

    /// The problem `network` holds, as its reads answer it.
    fn read_back(network: &Network) -> MincostNetwork {
        MincostNetwork {
            tails: network.tails().iter().collect(),
            heads: network.heads().iter().collect(),
            lower_bounds: network.lower_bounds().iter().collect(),
            capacities: network.capacities().iter().collect(),
            costs: network.costs().iter().collect(),
            supplies: network.supplies().to_vec(),
        }
    }

    #[test]
    fn test_the_reads_answer_the_problem_loaded_with_lower_bounds_by_hand() {
        // Five arcs on two nodes, dealt by a stride of three to the slots 0, 3, 1, 4, 2: every read goes through the
        // dealing. The arcs need no flow to be read.
        let held = MincostNetwork::from_arcs(
            &[3, -3],
            &[(0, 1, 1, 4, 2), (1, 0, 0, 2, 1), (0, 1, 0, 3, 5), (0, 1, 2, 2, -1), (1, 0, 0, 1, 3)],
        );
        let network = loaded(&held);

        assert_eq!(network.tails().iter().collect::<Vec<_>>(), [0, 1, 0, 0, 1]);
        assert_eq!(network.heads().iter().collect::<Vec<_>>(), [1, 0, 1, 1, 0]);
        assert_eq!(network.costs().iter().collect::<Vec<_>>(), [2, 1, 5, -1, 3]);
        assert_eq!(network.bounds(3), Bounds { lower_bound: 2, capacity: 2 });
        assert_eq!(network.bounds(0), Bounds { lower_bound: 1, capacity: 4 });
        assert_eq!((network.supplies(), network.costs().get(5)), (&[3, -3][..], None));
        assert_eq!(read_back(&network), held);
    }

    #[test]
    fn test_each_change_reads_back_the_changed_problem_and_nothing_else_by_hand() {
        // The depot solved, then each kind of change in turn, each read back against the test's copy changed alike;
        // between them, refused changes read back the problem as it was.
        let mut held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();
        assert_eq!(read_back(&network), held);

        assert_eq!(network.set_cost(2, 3), Ok(1));
        held.costs[2] = 3;
        assert_eq!(read_back(&network), held);
        assert_eq!(network.set_capacity(1, 6), Ok(4));
        held.capacities[1] = 6;
        assert_eq!(read_back(&network), held);
        assert_eq!(network.set_lower_bound(0, 1), Ok(0));
        held.lower_bounds[0] = 1;
        assert_eq!(read_back(&network), held);
        assert_eq!(
            network.set_bounds(2, Bounds { lower_bound: 1, capacity: 3 }),
            Ok(Bounds { lower_bound: 0, capacity: 2 })
        );
        (held.lower_bounds[2], held.capacities[2]) = (1, 3);
        assert_eq!(read_back(&network), held);
        assert_eq!(network.move_supply(2, 0, 1), Ok(()));
        (held.supplies[2], held.supplies[0]) = (-5, 5);
        assert_eq!(read_back(&network), held);

        assert!(network.set_cost(0, i64::MAX).is_err());
        assert!(network.set_capacity(0, 0).is_err());
        assert!(network.set_lower_bound(1, 7).is_err());
        assert!(network.set_bounds(1, Bounds { lower_bound: 2, capacity: 1 }).is_err());
        assert!(network.move_supply(2, 1, u64::MAX).is_err());
        assert_eq!(read_back(&network), held, "a refused change reads as the problem before it");

        // The changed depot solves as a fresh load of the problem read back.
        let fresh = solved(&mut loaded(&read_back(&network))).0;
        assert_eq!(solved(&mut network).0, fresh);
    }

    #[test]
    fn test_a_network_never_loaded_reads_the_problem_of_no_node() {
        let network = Network::new();

        assert_eq!(read_back(&network), MincostNetwork::default());
        assert!(network.tails().is_empty() && network.heads().is_empty() && network.costs().is_empty());
        assert!(network.lower_bounds().is_empty() && network.capacities().is_empty() && network.supplies().is_empty());
    }

    /// The text of a panic's payload, whether it was a literal or formatted; `None` for any other payload.
    fn panic_text(payload: &Box<dyn Any + Send>) -> Option<&str> {
        payload.downcast_ref::<String>().map(String::as_str).or_else(|| payload.downcast_ref::<&str>().copied())
    }

    /// Checks that `read`, the read `name` of a half-changed network, panics with its message.
    ///
    /// # Panics
    /// Panics if it does not.
    fn assert_refused_half_changed<T>(name: &str, read: impl FnOnce() -> T) {
        let unwound = catch_unwind(AssertUnwindSafe(read)).map(|_| ()).expect_err("the read should panic");
        let expected = format!("called `Network::{name}` on a network a panic left half changed");
        assert_eq!(unwound.downcast_ref::<String>(), Some(&expected));
    }

    #[test]
    fn test_a_read_of_a_half_changed_network_panics_and_one_mended_by_a_load_reads() {
        // Half changed after a solve, and half changed by a load that unwound on a network never loaded.
        let mut solved_first = loaded(&depot());
        let _ = solved_first.solve();
        half_changed(&mut solved_first);
        let mut never_loaded = Network::new();
        never_loaded.standing = Standing::Changed;

        for network in [&solved_first, &never_loaded] {
            assert_refused_half_changed("tails", || network.tails().len());
            assert_refused_half_changed("heads", || network.heads().len());
            assert_refused_half_changed("costs", || network.costs().len());
            assert_refused_half_changed("bounds", || network.bounds(0));
            assert_refused_half_changed("supplies", || network.supplies().len());
            assert_refused_half_changed("lower_bounds", || network.lower_bounds().len());
            assert_refused_half_changed("capacities", || network.capacities().len());
            assert_refused_half_changed("tail", || network.tail(0));
            assert_refused_half_changed("head", || network.head(0));
            assert_refused_half_changed("cost", || network.cost(0));
            assert_refused_half_changed("lower_bound", || network.lower_bound(0));
            assert_refused_half_changed("capacity", || network.capacity(0));
            assert_refused_half_changed("supply", || network.supply(0));
        }

        solved_first.load(&problem_of(&depot())).expect("the depot should be admitted");
        assert_eq!(read_back(&solved_first), depot());
    }

    #[test]
    fn test_a_copy_reads_as_its_source() {
        let mut network = loaded(&depot());
        let _ = network.solve();
        assert_eq!(network.set_cost(0, 2), Ok(5));
        assert_eq!(network.move_supply(1, 0, 3), Ok(()));
        let mut copy = loaded(&MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 1, 1)]));

        copy.try_clone_from(&network).expect("room for the copy should be granted");
        assert_eq!(read_back(&copy), read_back(&network));
        assert_eq!(read_back(&network.clone()), read_back(&network));
        assert_eq!(read_back(&copy).costs, [2, 1, 1]);
    }

    /// An optimum a network answered, as plain values. Plain data.
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct PlainOptimum {
        /// Its cost, in units of cost.
        total_cost: i128,
        /// Per arc: its flow, in units of flow.
        flows: Vec<u64>,
        /// Per node: its potential, in units of cost.
        potentials: Vec<i64>,
    }

    /// What a solve of `network` answers, as plain values: its optimum, `None` without one, and the work.
    fn answered(network: &mut Network) -> (Option<PlainOptimum>, Work) {
        let answer = network.solve();
        let optimum = network.optimum().map(|optimum| PlainOptimum {
            total_cost: optimum.total_cost,
            flows: optimum.flows.iter().collect(),
            potentials: optimum.potentials.to_vec(),
        });

        (optimum, answer.work)
    }

    /// Checks that `copy`, a copy of `source`, holds what `source` holds and answers its optimum, its counts and a
    /// solve as `source` does, the work included; then that a cost set on either leaves the other as it was, where
    /// they hold an arc.
    ///
    /// # Panics
    /// Panics at the first difference.
    fn assert_a_copy(source: &mut Network, copy: &mut Network) {
        assert_eq!(format!("{copy:?}"), format!("{source:?}"));
        assert_eq!(copy.optimum(), source.optimum());
        assert_eq!((copy.node_count(), copy.arc_count()), (source.node_count(), source.arc_count()));
        assert_eq!(answered(copy), answered(source));
        assert_eq!(format!("{copy:?}"), format!("{source:?}"), "solved alike, they hold alike");

        if source.arc_count() > 0 {
            let before = format!("{source:?}");
            assert!(copy.set_cost(0, 7).is_ok());
            assert_eq!(format!("{source:?}"), before, "a change of the copy leaves the source");
            let copied = format!("{copy:?}");
            assert!(source.set_cost(0, -7).is_ok());
            assert_eq!(format!("{copy:?}"), copied, "a change of the source leaves the copy");
        }
    }

    /// The networks of each standing, by hand: never loaded; the depot loaded and not solved; solved, optimal; a
    /// network of no flow solved, infeasible; and the depot changed after a solve, the direct lane at 1 a unit and a
    /// unit of the depot's supply moved to node 1.
    fn networks_of_each_standing() -> [Network; 5] {
        let mut optimal = loaded(&depot());
        let _ = optimal.solve();
        let mut infeasible = loaded(&MincostNetwork::from_arcs(&[2, -2], &[(0, 1, 0, 1, 1)]));
        let _ = infeasible.solve();
        let mut changed = loaded(&depot());
        let _ = changed.solve();
        assert_eq!(changed.set_cost(0, 1), Ok(5));
        assert_eq!(changed.move_supply(0, 1, 1), Ok(()));

        [Network::new(), loaded(&depot()), optimal, infeasible, changed]
    }

    #[test]
    fn test_networks_of_each_standing_answer_as_worked_by_hand() {
        // What the copies below must answer alike: no flow at no cost and no work; the depot from its star, 14 by 4
        // pivots; again with no pivot; no flow, again with no pivot; and the depot changed, whose flow of 2 on each
        // arc it must leave: the depot's 3 units directly at 1 and node 1's unit by arc 2 at 1, 4 in all.
        let answers = networks_of_each_standing().map(|mut network| {
            let (optimum, work) = answered(&mut network);
            (optimum.map(|optimum| optimum.total_cost), work.pivots > 0)
        });

        assert_eq!(answers, [(Some(0), false), (Some(14), true), (Some(14), false), (None, false), (Some(4), true)]);
    }

    /// The network of `standing`, the index of one of the five of [`networks_of_each_standing`], made anew.
    fn network_of_standing(standing: usize) -> Network {
        networks_of_each_standing().into_iter().nth(standing).expect("the standing should be one of the five")
    }

    #[test]
    fn test_a_copy_of_each_standing_by_each_call_answers_what_its_source_answers() {
        for standing in 0..5 {
            let mut cloned = network_of_standing(standing).clone();
            assert_a_copy(&mut network_of_standing(standing), &mut cloned);

            let mut tried = network_of_standing(standing).try_clone().expect("room for the copy should be granted");
            assert_a_copy(&mut network_of_standing(standing), &mut tried);

            let mut cloned_into = loaded(&MincostNetwork::from_arcs(&[0; 10], &[(0, 9, 0, 1, 1)]));
            cloned_into.clone_from(&network_of_standing(standing));
            assert_a_copy(&mut network_of_standing(standing), &mut cloned_into);

            // `clone` and `try_clone` make the same network, the net supplies of neither written.
            let tried = network_of_standing(standing).try_clone().expect("room for the copy should be granted");
            assert_eq!(format!("{:?}", network_of_standing(standing).clone()), format!("{tried:?}"));
        }
    }

    #[test]
    fn test_a_copy_into_a_network_that_held_another_problem_answers_what_its_source_answers() {
        // Destinations solved: one larger in both counts, one smaller, one of fewer nodes and more arcs.
        let larger = MincostNetwork::from_arcs(
            &[3, 0, 0, 0, 0, 0, -3],
            &[(0, 6, 0, 5, 2), (0, 1, 0, 5, 1), (1, 6, 0, 5, 1), (2, 3, 0, 1, 1)],
        );
        let smaller = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 1, 1)]);
        let other =
            MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 1, 4), (0, 1, 0, 1, 2), (1, 0, 0, 1, 1), (0, 1, 0, 1, 3)]);

        for destination in [&larger, &smaller, &other] {
            for standing in 0..5 {
                let mut copy = loaded(destination);
                let _ = copy.solve();

                copy.try_clone_from(&network_of_standing(standing)).expect("room for the copy should be granted");
                let fresh = network_of_standing(standing).clone();
                assert_eq!(format!("{copy:?}"), format!("{fresh:?}"), "nothing of the problem held before is left");
                assert_a_copy(&mut network_of_standing(standing), &mut copy);
            }
        }
    }

    #[test]
    fn test_a_copy_of_a_half_changed_network_is_half_changed_and_mended_by_a_load() {
        // The depot solved and left half changed, copied into a network that held a problem of two nodes, solved, and
        // by the other calls: each copy stands changed with no basis and the depot's counts, refuses a change, and is
        // mended by a load; the three copies hold alike.
        let mut source = loaded(&depot());
        let _ = source.solve();
        half_changed(&mut source);
        let mut copy = loaded(&MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 1, 1)]));
        let _ = copy.solve();

        copy.try_clone_from(&source).expect("room for the copy should be granted");
        let (cloned, tried) = (source.clone(), source.try_clone().expect("room for the copy should be granted"));
        assert_eq!((format!("{cloned:?}"), format!("{tried:?}")), (format!("{copy:?}"), format!("{copy:?}")));
        assert_eq!((copy.standing, copy.basis.kept(), copy.optimum()), (Standing::Changed, None, None));
        assert_eq!((copy.node_count(), copy.arc_count()), (3, 3));

        let unwound = catch_unwind(AssertUnwindSafe(|| copy.set_cost(0, 7))).expect_err("the change should panic");
        assert_eq!(panic_text(&unwound), Some("called `Network::set_cost` on a network a panic left half changed"));
        copy.load(&problem_of(&depot())).expect("the depot should be admitted");
        assert_eq!(solved(&mut copy), (Some(14), Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 }));
    }

    #[test]
    fn test_a_refused_copy_leaves_the_network_as_it_was() {
        // The source: half changed, given counts no allocator grants, which no network holds. The network the copy is
        // refused into holds the depot solved and then changed; it holds and answers what it did, 18 for the dearer
        // leg.
        let mut source = Network::new();
        source.standing = Standing::Changed;
        source.order = SlotOrder::new(0, usize::MAX);
        let mut network = loaded(&depot());
        let _ = network.solve();
        assert_eq!(network.set_cost(2, 3), Ok(1));
        let whole = format!("{network:?}");

        assert!(network.try_clone_from(&source).is_err());
        assert!(source.try_clone().is_err());
        assert_eq!(format!("{network:?}"), whole);
        assert_eq!(solved(&mut network).0, Some(18));

        // Refused into an optimal network, the network keeps its optimum.
        assert!(network.try_clone_from(&source).is_err());
        assert_eq!((network.optimum().map(|optimum| optimum.total_cost), network.solve().work.pivots), (Some(18), 0));
    }

    /// What a network holds, checked against `held`, the network's problem as the test keeps it: the whole invariant
    /// of the basis at the widest root cost of the node count; every arc that violates, as a model prices it, in a
    /// marked block, and no block marked where the network stands optimal; the supplies; the cost total of `held`'s
    /// costs and capacities; the kept cost congruent modulo `2^128` to the cost of the flow, lower bounds and root
    /// links counted; the excess the flow up the root links, read from the tree. Answers the potentials.
    ///
    /// # Errors
    /// Returns the first breach.
    fn checked(network: &mut Network, held: &MincostNetwork) -> Result<Vec<i64>, String> {
        let widest = widest_root_cost(held.node_count());
        let basis = network.basis.reopen().ok_or("the network should hold its basis")?;
        let checked = check_at(&basis, held, widest);
        let marked = marks_hold(&basis, &network.marks, held);
        let flow_cost = extended_cost_at(&basis, held, widest);
        let flow_up = flow_up_the_root_links(&basis);
        let potentials = basis.keep().to_vec();
        checked?;
        marked?;
        let any_marked = (0..held.arc_count()).any(|at| is_marked(&network.marks, Slot::from(at)));
        if network.standing == Standing::Optimal && any_marked {
            return Err("an optimal network should hold no marked block".to_owned());
        }
        if network.supplies.raw != held.supplies || u128::from(network.sums.excess()) != flow_up {
            return Err(format!(
                "the supplies {:?} and {:?}, {flow_up} up the root links",
                network.supplies, network.sums
            ));
        }

        let arcs = 0..held.arc_count();
        let term = |arc: usize| u128::from(held.costs[arc].unsigned_abs()) * u128::from(held.capacities[arc]);
        let total: u128 = arcs.clone().map(term).sum();
        let lower_bound_cost: BigInt =
            arcs.map(|arc| BigInt::from(held.costs[arc]) * BigInt::from(held.lower_bounds[arc])).sum();
        let difference = lower_bound_cost + flow_cost - BigInt::from(network.sums.kept_cost());
        let wrapped = difference % (BigInt::from(1_u8) << 128);
        if network.sums.cost_total() != total || wrapped != BigInt::from(0_u8) {
            return Err(format!("the sums {:?} for the cost total {total}", network.sums));
        }

        Ok(potentials)
    }

    /// Whether every arc of `basis`, a basis of `held`, that violates its bound, as a model prices it apart from the
    /// rows, lies in a block `marks` marks.
    ///
    /// # Errors
    /// Returns the first such arc that lies in a clear block.
    fn marks_hold(basis: &Basis<'_>, marks: &MarkedBlocks, held: &MincostNetwork) -> Result<(), String> {
        match violating_slots(basis, held).into_iter().find(|&slot| !is_marked(marks, slot)) {
            Some(slot) => Err(format!("the arc at {slot:?} violates its bound in a block not marked")),
            None => Ok(()),
        }
    }

    /// `network`, which holds `held`, solved as its solve does, pivot by pivot, with the marks checked against the
    /// model before the first pivot and after every pivot, each search pricing no more rows than the marked blocks
    /// hold, every block marked after each pivot, the last search pricing exactly the marked rows, and the pivots
    /// bounded; answers the work, or the first breach.
    ///
    /// # Errors
    /// Returns the first breach of the marks, or the bound passed.
    fn solved_step_by_step(network: &mut Network, held: &MincostNetwork) -> Result<Work, String> {
        let Network { basis, marks, sums, standing, .. } = network;
        *standing = Standing::Changed;
        let basis = basis.reopen().ok_or("the network should hold its basis")?;
        let bound = pivot_bound(&basis, held);
        let mut run = Run { basis, marks, sums, search: MarkedSearch::new(), work: Work::default() };

        loop {
            marks_hold(&run.basis, run.marks, held)?;
            // The rows of the marked blocks: each slot whose block is marked is one.
            let marked = (0..held.arc_count()).filter(|&at| is_marked(run.marks, Slot::from(at))).count();
            let priced = priced_rows(run.marks);
            let entered = run.enter_next();
            let searched = priced_rows(run.marks) - priced;

            // A search prices each marked block once at most, and the last search every marked block, clearing them.
            match entered {
                None if searched == marked => break,
                Some(_) if 0 < searched && searched <= marked => {}
                _ => return Err(format!("a search priced {searched} rows of {marked} marked, entering {entered:?}")),
            }
            let every = (0..held.arc_count()).all(|at| is_marked(run.marks, Slot::from(at)));
            if !every {
                return Err(format!("a pivot sending {entered:?} should mark every block"));
            }
            if BigInt::from(run.work.pivots) > bound {
                return Err(format!("the solve made more than {bound} pivots"));
            }
        }

        Ok(run.end(standing))
    }

    /// The most pivots a solve of `basis`, a basis of `held` at the widest root cost, makes, if it never returns to a
    /// basis it left: one more than its flow's cost can fall by one, times one more than the trees of `m + n` links.
    fn pivot_bound(basis: &Basis<'_>, held: &MincostNetwork) -> BigInt {
        // A pivot that sends flow lowers the cost of the flow, root links counted, by one at least, from where it
        // starts to no less than the least the arcs can cost; a stretch of pivots that send nothing visits each tree
        // once at most, a choice of `n` of the `m + n` links (`Basis::pivot`).
        let widest = widest_root_cost(held.node_count());
        let arcs = (0..held.arc_count())
            .map(|arc| BigInt::from(held.costs[arc].unsigned_abs()) * (held.capacities[arc] - held.lower_bounds[arc]));
        let sending = extended_cost_at(basis, held, widest) + arcs.sum::<BigInt>();
        let (links, nodes) = (held.arc_count() + held.node_count(), held.node_count());
        let trees = (0..nodes).fold(BigInt::from(1), |product, at| product * (links - at) / (at + 1));

        (sending + 1) * (trees + 1)
    }

    /// `network` solved, with its outcome's cost, `None` where it has no flow, and the work.
    fn solved(network: &mut Network) -> (Option<i128>, Work) {
        let answer = network.solve();

        (answer.outcome.optimal(), answer.work)
    }

    #[test]
    fn test_held_arc_reads_the_arc_at_a_slot_with_its_lower_bound_by_hand() {
        // The depot's optimum: arc 2, of width 2, carries 2 at cost 1. With lower bounds, arcs 0 and 1 of the problem
        // of bounds `1..=5` and `0..=5` carry 3 each, arc 0 two above its lower bound.
        let mut network = loaded(&depot());
        let _ = network.solve();
        let arc = HeldArc { kept: KeptArc { cost: 1, width: 2, flow: 2 }, lower_bound: 0 };
        assert_eq!(network.held_arc(Slot::from(2_usize)), Some(arc));
        assert_eq!(network.held_arc(Slot::from(3_usize)), None, "past the arcs");

        let mut bounded = loaded(&MincostNetwork::from_arcs(&[3, 0, -3], &[(0, 1, 1, 5, 2), (1, 2, 0, 5, -1)]));
        let _ = bounded.solve();
        let arc = HeldArc { kept: KeptArc { cost: 2, width: 4, flow: 2 }, lower_bound: 1 };
        assert_eq!(bounded.held_arc(Slot::from(0_usize)), Some(arc));

        half_changed(&mut network);
        assert_eq!(network.held_arc(Slot::from(2_usize)), None, "a half-changed network holds no arc");
    }

    #[test]
    fn test_held_pair_reads_two_supplies_or_names_what_is_unmet_by_hand() {
        // The depot's supplies 4, 0 and −4: a pair in either order, a node past the three, and a half-changed network.
        let mut network = loaded(&depot());
        let pair = network.held_pair(0, 2).expect("nodes 0 and 2 should be below the node count");
        assert_eq!(pair, HeldPair { nodes: [0, 2], supplies: [4, -4] });
        assert_eq!(pair.swapped(), HeldPair { nodes: [2, 0], supplies: [-4, 4] });
        assert_eq!(network.held_pair(1, 1), Ok(HeldPair { nodes: [1, 1], supplies: [0, 0] }));
        assert_eq!(network.held_pair(0, 3), Err(Unmet::PairPastTheNodes { first: 0, second: 3, node_count: 3 }));

        network.set_supplies(pair, [3, -3]);
        assert_eq!(network.supplies(), [3, 0, -3]);

        half_changed(&mut network);
        assert_eq!(network.held_pair(0, 2), Err(Unmet::HalfChanged));
        assert_eq!(network.held_pair(0, 3), Err(Unmet::PairPastTheNodes { first: 0, second: 3, node_count: 3 }));
    }

    #[test]
    fn test_sums_with_cost_admit_or_refuse_a_cost_by_hand() {
        // The depot's arc 2, of capacity 2 and carrying 2, from cost 1: to 3 the total `21 − 2 + 6` and the cost
        // `14 + 2·2`; to the bound of three nodes and one past it.
        let mut network = loaded(&depot());
        let _ = network.solve();
        let arc = HeldArc { kept: KeptArc { cost: 1, width: 2, flow: 2 }, lower_bound: 0 };

        let sums = network.sums_with_cost(2, arc, 3).expect("the cost 3 should be admitted");
        assert_eq!((sums.cost_total(), sums.kept_cost()), (25, 18));
        let bound = cost_bound(3);
        let at_bound = network.sums_with_cost(2, arc, -bound).expect("the bound should be admitted");
        assert_eq!(at_bound.kept_cost(), 14 + 2 * (-i128::from(bound) - 1));
        let refused = ChangeError::CostBeyondBound { arc: 2, cost: bound + 1, node_count: 3 };
        assert_eq!(network.sums_with_cost(2, arc, bound + 1), Err(refused));

        // A lower bound counts in the capacity and the flow: arc 0, of bounds `1..=5` and carrying 3 at cost 2, to 4.
        // The total `2·5 + 1·5 = 15` gains `2·5`; the optimum's cost `2·3 − 3 = 3` gains `2·3`.
        let mut bounded = loaded(&MincostNetwork::from_arcs(&[3, 0, -3], &[(0, 1, 1, 5, 2), (1, 2, 0, 5, -1)]));
        let _ = bounded.solve();
        let arc = HeldArc { kept: KeptArc { cost: 2, width: 4, flow: 2 }, lower_bound: 1 };
        let sums = bounded.sums_with_cost(0, arc, 4).expect("the cost 4 should be admitted");
        assert_eq!((sums.cost_total(), sums.kept_cost()), (25, 3 + 6));
    }

    #[test]
    fn test_a_cost_out_of_the_tree_moves_the_sums_and_no_potential_by_hand() {
        // The depot's optimum: arc 2 at its capacity, carrying 2, out of the tree. From cost 1 to 3: the cost total
        // `15 + 4 + 2 = 21` becomes 25, the kept cost 14 becomes `14 + 2·2 = 18`; no potential moves, and arc 2's
        // violation, `−(3 + 1 − 5) = 1`, calls for no pivot: 2 units through node 1 at 4 and 2 directly at 5.
        let mut held = depot();
        let mut network = loaded(&held);
        assert_eq!(solved(&mut network), (Some(14), Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 }));
        assert_eq!((network.sums.cost_total(), checked(&mut network, &held)), (21, Ok(vec![0, 1, 5])));

        assert_eq!(network.set_cost(2, 3), Ok(1));
        held.costs[2] = 3;
        assert_eq!((network.standing, network.optimum()), (Standing::Changed, None));
        assert_eq!((network.sums.cost_total(), network.sums.kept_cost()), (25, 18));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 5]));

        assert_eq!(solved(&mut network), (Some(18), Work::default()));
        assert_eq!(network.standing, Standing::Optimal);
    }

    /// The slots whose blocks `network` marks, ascending.
    fn marked_slots(network: &Network) -> Vec<usize> {
        (0..network.arc_count()).filter(|&at| is_marked(&network.marks, Slot::from(at))).collect()
    }

    /// `network` solved, with its outcome's cost, the work, and the rows the solve priced.
    fn solved_and_priced(network: &mut Network) -> (Option<i128>, Work, usize) {
        let before = priced_rows(&network.marks);
        let (cost, work) = solved(network);

        (cost, work, priced_rows(&network.marks) - before)
    }

    #[test]
    fn test_each_change_of_the_solved_depot_marks_its_arc_and_the_smaller_sides_it_moved_by_hand() {
        // The depot's optimum, three slots in blocks of one, slot `s` holding arc `s`: node 0 under the root, up and
        // empty; nodes 1 and 2 under it by arcs 1 and 0, running down; arc 2 out of the tree at its capacity 2;
        // potentials 0, 1, 5. Four tree nodes, the root among them. Nothing is marked.
        let mut optimal = loaded(&depot());
        let _ = optimal.solve();
        assert_eq!(marked_slots(&optimal), Vec::<usize>::new());

        // Arc 2's cost, 1 to 3, out of the tree: its own slot alone. Its violation, `−(3 + 1 − 5)`, is none: the solve
        // prices that row and pivots not.
        let mut network = optimal.clone();
        assert_eq!(network.set_cost(2, 3), Ok(1));
        assert_eq!(marked_slots(&network), [2]);
        assert_eq!(solved_and_priced(&mut network), (Some(18), Work::default(), 1));

        // Arc 0's cost, 5 to 4: it hangs node 2, whose subtree, node 2 alone, is the smaller side; the arcs at node 2
        // are arcs 0 and 2. Node 2 falls to 4, and arc 2, at its capacity, prices at `1 + 1 − 4`, no violation.
        let mut network = optimal.clone();
        assert_eq!(network.set_cost(0, 4), Ok(5));
        assert_eq!(marked_slots(&network), [0, 2]);
        assert_eq!(solved_and_priced(&mut network), (Some(12), Work::default(), 2));

        // One unit more from node 1 to node 2, which the tree carries: no potential and no state moves, so nothing is
        // marked, and the solve prices no row.
        let mut network = optimal.clone();
        assert_eq!(network.move_supply(2, 1, 1), Ok(()));
        assert_eq!(marked_slots(&network), Vec::<usize>::new());
        assert_eq!(solved_and_priced(&mut network), (Some(18), Work::default(), 0));

        // Arc 2's capacity, 2 to 3, at its capacity out of the tree: it carries 3, the unit more going from node 2 to
        // node 1 through the tree, which carries it. Its own slot alone.
        let mut network = optimal.clone();
        assert_eq!(network.set_capacity(2, 3), Ok(2));
        assert_eq!(marked_slots(&network), [2]);
        assert_eq!(solved_and_priced(&mut network), (Some(11), Work::default(), 1));

        // Arc 0's capacity, 3 to 1, under its flow of 2: node 2 is cut, down, at `M`, its subtree node 2 alone; the
        // arc's slot and the arcs at node 2, slots 0 and 2.
        let mut network = optimal.clone();
        assert_eq!(network.set_capacity(0, 1), Ok(3));
        assert_eq!(marked_slots(&network), [0, 2]);

        // Set back to 3, from its capacity 1: two units go from node 2 to node 0 through the root, and both tops turn:
        // node 2's subtree, node 2, and node 0's, nodes 0 and 1, two of four, the subtree taken on a tie. Every arc has
        // an end there.
        let solved_cut = network.solve().outcome.optimal().is_none();
        assert!(solved_cut, "the depot with arc 0 at 1 has no flow");
        assert_eq!(marked_slots(&network), Vec::<usize>::new());
        let tops = root_links_of(&mut network);
        assert_eq!(tops, [Some(Direction::Up), None, Some(Direction::Down)]);
        assert_eq!(network.set_capacity(0, 3), Ok(1));
        assert_eq!(root_links_of(&mut network), [Some(Direction::Down), None, Some(Direction::Up)], "both turned");
        assert_eq!(marked_slots(&network), [0, 1, 2]);
        assert_eq!(solved(&mut network).0, Some(14));
    }

    #[test]
    fn test_a_raised_cost_of_an_empty_arc_prices_its_block_alone_and_a_carried_move_no_row() {
        // The grid of side 18: every arc out of the tree at its lower bound, its cost raised by one, priced at no
        // violation still. The solve prices the arc's block, `⌊√m⌋` rows or the last block's fewer, and pivots not.
        let held = Family::Grid.network(18);
        let arc_count = held.arc_count();
        let mut optimal = loaded(&held);
        let full = solved_and_priced(&mut optimal);
        assert!(full.0.is_some() && full.2 >= arc_count, "the first solve prices every row once at least");
        let block = arc_count.isqrt();
        let in_tree = tree_arcs_of(&mut optimal);

        let mut raised = 0_usize;
        for arc in 0..arc_count {
            let slot = optimal.order.slot_of(arc).expect("an arc below the arc count should have a slot");
            let kept = optimal.held_arc(slot).expect("the network should hold its arc").kept;
            if in_tree[usize::from(slot)] || kept.flow != 0 || kept.width == 0 {
                continue;
            }

            let mut network = optimal.clone();
            assert_eq!(network.set_cost(arc, kept.cost + 1), Ok(kept.cost));
            let start = usize::from(slot) / block * block;
            let rows = (start + block).min(arc_count) - start;
            assert_eq!(solved_and_priced(&mut network), (full.0, Work::default(), rows), "arc {arc}");
            raised += 1;
        }
        assert!(raised > 0, "some arcs of the grid stand empty");

        // A unit more from the source to the sink, whose paths in the tree carry it: nothing marked, no row priced.
        let (source, sink) = (0, held.node_count() - 1);
        let mut network = optimal.clone();
        assert_eq!(network.move_supply(sink, source, 1), Ok(()));
        let (cost, work, rows) = solved_and_priced(&mut network);
        assert_eq!((work, rows, marked_slots(&network)), (Work::default(), 0, vec![]));
        let mut moved = held.clone();
        (moved.supplies[source], moved.supplies[sink]) = (moved.supplies[source] + 1, moved.supplies[sink] - 1);
        assert_eq!(cost, solvers_cost(&moved));
    }

    #[test]
    fn test_a_cost_on_a_tree_arc_running_down_shifts_its_subtree_by_hand() {
        // The depot's optimum: node 0 at the top, nodes 1 and 2 hanging from it by arcs 1 and 0, both running down to
        // them. Arc 1 from cost 1 to 4: node 1 rises by 3 to 4; the cost total gains `3·4`, the kept cost `3·2`. Arc
        // 2, at its capacity, then prices `−(1 + 4 − 5) = 0`: the flow stays, tied with sending more directly, at 20.
        let mut held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();

        assert_eq!(network.set_cost(1, 4), Ok(1));
        held.costs[1] = 4;
        assert_eq!((network.sums.cost_total(), network.sums.kept_cost()), (33, 20));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 4, 5]));
        assert_eq!(solved(&mut network), (Some(20), Work::default()));

        // Arc 0 from cost 5 to 2: node 2 falls by 3 to 2. Arc 2 now prices `−(1 + 4 − 2) = −3`: it gives its 2 units
        // back to arc 0, which fills at 3, one unit going on through node 1: `3·2 + 4 + 1 = 11`.
        assert_eq!(network.set_cost(0, 2), Ok(5));
        held.costs[0] = 2;
        assert_eq!((network.sums.cost_total(), network.sums.kept_cost()), (24, 14));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 4, 2]));
        let (cost, work) = solved(&mut network);
        assert_eq!((cost, work.pivots > 0), (Some(11), true));
        assert_eq!(network.optimum().map(|optimum| optimum.flows.iter().collect::<Vec<_>>()), Some(vec![3, 1, 1]));
    }

    #[test]
    fn test_a_cost_on_a_tree_arc_running_up_shifts_its_node_against_the_change_by_hand() {
        // The depot turned around: node 2 sends 4 units to node 0, directly by arc 0 or through node 1 by arcs 2 and 1.
        // Solved from the widest star, node 2 is the top, node 0 hangs from it by arc 0 running down, node 1 from node
        // 0 by arc 1 running up from node 1: potentials 5, 4 and 0, at the same cost 14.
        let mut held = MincostNetwork::from_arcs(&[-4, 0, 4], &[(2, 0, 0, 3, 5), (1, 0, 0, 4, 1), (2, 1, 0, 2, 1)]);
        let mut network = loaded(&held);
        assert_eq!(solved(&mut network), (Some(14), Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 }));
        assert_eq!(checked(&mut network, &held), Ok(vec![5, 4, 0]));

        // Arc 1 from cost 1 to 3: node 1 falls by 2 to 2; arc 2 at its capacity prices `−(1 + 0 − 2) = 1`, no pivot.
        assert_eq!(network.set_cost(1, 3), Ok(1));
        held.costs[1] = 3;
        assert_eq!(checked(&mut network, &held), Ok(vec![5, 2, 0]));
        assert_eq!(solved(&mut network), (Some(18), Work::default()));

        // Arc 0, which hangs nodes 0 and 1, from cost 5 to 2: both fall by 3, to 2 and −1.
        assert_eq!(network.set_cost(0, 2), Ok(5));
        held.costs[0] = 2;
        assert_eq!(checked(&mut network, &held), Ok(vec![2, -1, 0]));
        assert_eq!(network.sums.kept_cost(), 18 - 3 * 2);
    }

    #[test]
    fn test_a_cost_set_to_the_cost_held_changes_nothing_and_keeps_the_optimum_by_hand() {
        let held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();
        let sums = network.sums;

        assert_eq!((network.set_cost(0, 5), network.set_cost(2, 1)), (Ok(5), Ok(1)));
        assert_eq!((network.standing, network.sums), (Standing::Optimal, sums));
        assert_eq!(network.optimum().map(|optimum| optimum.total_cost), Some(14));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 5]));
    }

    #[test]
    fn test_a_refused_cost_changes_nothing_by_hand() {
        // Past the cost bound of three nodes, either way; and, on four loops at one node whose costs times capacities
        // add up to `i128::MAX` exactly, any cost of a larger magnitude.
        let held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();
        let sums = network.sums;
        let bound = cost_bound(3);
        for cost in [bound + 1, -bound - 1, i64::MAX, i64::MIN] {
            assert_eq!(network.set_cost(0, cost), Err(ChangeError::CostBeyondBound { arc: 0, cost, node_count: 3 }));
        }
        assert_eq!((network.standing, network.sums), (Standing::Optimal, sums));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 5]));

        let (bound, wide) = (cost_bound(1), u64::MAX);
        let arcs = [(0, 0, 0, wide, bound), (0, 0, 0, wide, -bound), (0, 0, 0, wide, 2), (0, 0, 0, (1 << 63) - 1, 1)];
        let held = MincostNetwork::from_arcs(&[0], &arcs);
        let mut network = loaded(&held);
        assert_eq!(network.sums.cost_total(), i128::MAX.cast_unsigned());
        let sums = network.sums;
        for (arc, cost) in [(3, 2), (3, -2), (2, 3), (2, -3)] {
            assert_eq!(network.set_cost(arc, cost), Err(ChangeError::CostTotalBeyondRange { arc }));
        }
        assert_eq!((network.standing, network.sums), (Standing::Changed, sums));
        assert_eq!(network.set_cost(2, -2), Ok(2), "the same magnitude keeps the total");
        assert_eq!(network.sums.cost_total(), i128::MAX.cast_unsigned());
    }

    #[test]
    fn test_a_cost_on_a_network_never_loaded_panics_at_its_arc() {
        let mut network = Network::new();
        let unwound = catch_unwind(AssertUnwindSafe(|| network.set_cost(0, 1)));
        let message = unwound.expect_err("a network never loaded has no arc");
        assert_eq!(
            message.downcast_ref::<String>().map(String::as_str),
            Some("called `Network::set_cost` with arc 0, which should be below the arc count 0")
        );
        assert_eq!(network.standing, Standing::Unloaded);
    }

    #[test]
    fn test_a_cost_on_a_half_changed_network_panics_before_any_change() {
        let mut network = loaded(&depot());
        let _ = network.solve();
        half_changed(&mut network);
        let sums = network.sums;

        let unwound = catch_unwind(AssertUnwindSafe(|| network.set_cost(0, 7)));
        let message = unwound.expect_err("the change should panic");
        assert_eq!(panic_text(&message), Some("called `Network::set_cost` on a network a panic left half changed"));
        assert_eq!((network.standing, network.sums, network.optimum()), (Standing::Changed, sums, None));

        network.load(&problem_of(&depot())).expect("the depot should be admitted");
        assert_eq!(network.set_cost(0, 7), Ok(5), "a load mends it");
    }

    #[test]
    fn test_a_cost_that_unwinds_after_its_first_write_leaves_the_network_changed() {
        // A solved network whose room still says it holds a basis but whose tree is no longer kept: the change reads
        // the arc and admits the cost, writes its standing, and unwinds at its reopen. It must not stand optimal
        // without a basis held, and a load mends it.
        let mut network = loaded(&depot());
        let _ = network.solve();
        untie_the_kept_tree(&mut network.basis);

        let unwound = catch_unwind(AssertUnwindSafe(|| network.set_cost(0, 7)));
        assert!(unwound.is_err(), "a change of a network whose basis cannot be lent again should panic");
        assert_eq!((network.standing, network.basis.kept(), network.optimum()), (Standing::Changed, None, None));

        network.load(&problem_of(&depot())).expect("the depot should be admitted");
        assert_eq!(solved(&mut network).0, Some(14));
    }

    /// The widest root cost of three nodes, `2·⌊(2^62 − 1) / 3⌋ + 1`: what a root link pointing down costs in the
    /// depot.
    const DEPOT_ROOT_COST: i64 = 3_074_457_345_618_258_603;

    /// The depot turned around: node 2 sends 4 units to node 0, directly by arc 0 or through node 1 by arcs 2 and 1.
    fn depot_turned_around() -> MincostNetwork {
        MincostNetwork::from_arcs(&[-4, 0, 4], &[(2, 0, 0, 3, 5), (1, 0, 0, 4, 1), (2, 1, 0, 2, 1)])
    }

    #[test]
    fn test_moved_supplies_take_from_one_give_to_the_other_or_name_the_first_out_of_range_by_hand() {
        assert_eq!(moved_supplies([2, 0], [-4, 4], 3), Ok([-7, 7]));
        assert_eq!(moved_supplies([0, 1], [i64::MIN + 3, i64::MAX - 3], 3), Ok([i64::MIN, i64::MAX]));

        let below = ChangeError::SupplyBeyondRange { node: 0, supply: i128::from(i64::MIN) - 1 };
        assert_eq!(moved_supplies([0, 1], [i64::MIN + 3, 0], 4), Err(below));
        let above = ChangeError::SupplyBeyondRange { node: 1, supply: i128::from(i64::MAX) + 1 };
        assert_eq!(moved_supplies([0, 1], [0, i64::MAX - 3], 4), Err(above));

        // Both past: the node the amount is taken from is named, `0 − (2^64 − 1)`.
        let both = ChangeError::SupplyBeyondRange { node: 5, supply: -18_446_744_073_709_551_615 };
        assert_eq!(moved_supplies([5, 2], [0, 1], u64::MAX), Err(both));
    }

    #[test]
    fn test_a_move_the_tree_carries_moves_the_kept_cost_and_no_potential_by_hand() {
        // The depot's optimum: node 0 under the root, nodes 1 and 2 under node 0 by arcs 1 and 0, running down,
        // potentials 0, 1, 5; arc 2 at its capacity 2. Node 2 gives one unit of supply to node 1: arc 1 brings node 1
        // one less, arc 0 node 2 one more, up to its capacity 3, so the kept cost gains `5 − 1` and the root links
        // carry nothing. The flow is optimal: node 1's unit goes on by arc 2, then full, and of the depot's four one
        // goes through node 1 and three directly: `3·5 + 1 + 2 = 18`.
        let mut held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();

        assert_eq!(network.move_supply(2, 1, 1), Ok(()));
        (held.supplies[2], held.supplies[1]) = (-5, 1);
        assert_eq!((network.standing, network.optimum()), (Standing::Changed, None));
        assert_eq!((network.sums.kept_cost(), network.sums.excess()), (18, 0));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 5]));
        assert_eq!(solved(&mut network), (Some(18), Work::default()));
    }

    #[test]
    fn test_a_move_past_a_link_running_down_strands_flow_and_its_set_back_mends_it_by_hand() {
        // The depot's optimum, and node 2 needs two more units, sent by node 0, its ancestor: node 0's path is empty.
        // Arc 0 can bring node 2 one of them, to its capacity 3, and leaves the tree there; node 2 hangs from the root
        // by its root link, down, carrying the other, at potential `M`. Node 0, under the root, keeps that unit on its
        // root link, up. The kept cost gains `5 + M`; one unit up the root links. No arc can carry six units to node 2:
        // the solve finds no flow, and pivots nothing.
        let mut held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();
        let root_cost = i128::from(DEPOT_ROOT_COST);

        assert_eq!(network.move_supply(2, 0, 2), Ok(()));
        (held.supplies[2], held.supplies[0]) = (-6, 6);
        assert_eq!((network.sums.kept_cost(), network.sums.excess()), (19 + root_cost, 1));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, DEPOT_ROOT_COST]));
        assert_eq!(solved(&mut network), (None, Work::default()));
        assert_eq!((network.standing, network.sums.excess()), (Standing::Changed, 1));

        // Set back: the two nodes hang in two components, so each path ends at its top. Node 2's root link turns up,
        // carrying one, and falls to 0; node 0's turns down, carrying one, and it and node 1 rise by `M`. The kept cost
        // gains `−M + M`; still one unit up. The solve finds the depot's optimum again.
        assert_eq!(network.move_supply(0, 2, 2), Ok(()));
        (held.supplies[2], held.supplies[0]) = (-4, 4);
        assert_eq!((network.sums.kept_cost(), network.sums.excess()), (19 + root_cost, 1));
        assert_eq!(checked(&mut network, &held), Ok(vec![DEPOT_ROOT_COST, DEPOT_ROOT_COST + 1, 0]));
        let (cost, work) = solved(&mut network);
        assert_eq!((cost, work.pivots > 0, network.sums.excess()), (Some(14), true, 0));
    }

    #[test]
    fn test_a_move_that_fills_a_link_running_up_cuts_it_with_or_without_a_rest_by_hand() {
        // The depot turned around, solved: node 2 under the root, node 0 under it by arc 0 running down, at 5, node 1
        // under node 0 by arc 1 running up, at 4, every arc carrying 2.
        let mut held = depot_turned_around();
        let mut network = loaded(&held);
        assert_eq!(solved(&mut network).0, Some(14));
        assert_eq!(checked(&mut network, &held), Ok(vec![5, 4, 0]));
        let copy = network.clone();

        // Node 1 takes two of node 0's demand: arc 1 sends two more, to its capacity 4, with no rest, and can send no
        // more: it leaves the tree, and node 1 hangs from the root by its root link, up and empty, at 0. The kept cost
        // gains `1·2`. The flow is optimal: `2·5 + 4 + 2 = 16`.
        assert_eq!(network.move_supply(0, 1, 2), Ok(()));
        (held.supplies[0], held.supplies[1]) = (-6, 2);
        assert_eq!((network.sums.kept_cost(), network.sums.excess()), (16, 0));
        assert_eq!(checked(&mut network, &held), Ok(vec![5, 0, 0]));
        assert_eq!(solved(&mut network).0, Some(16));

        // From the solved network again, three: arc 1 sends two more, to 4, and the rest, one, goes up node 1's root
        // link. Node 0, node 1's ancestor, is handed two and must take three: arc 0 brings it one more, to its capacity
        // 3, where it can still bring less, and hands node 2 one less to send, which its root link, up and empty, turns
        // down to bring: node 2 and node 0 rise by `M`. The kept cost gains `2 + 5 + M`; one unit up the root links.
        // The solve sends it round: `3·5 + 4 + 1 = 20`.
        let mut network = copy;
        let mut held = depot_turned_around();
        assert_eq!(network.move_supply(0, 1, 3), Ok(()));
        (held.supplies[0], held.supplies[1]) = (-7, 3);
        assert_eq!((network.sums.kept_cost(), network.sums.excess()), (21 + i128::from(DEPOT_ROOT_COST), 1));
        assert_eq!(checked(&mut network, &held), Ok(vec![5 + DEPOT_ROOT_COST, 0, DEPOT_ROOT_COST]));
        assert_eq!((solved(&mut network).0, network.sums.excess()), (Some(20), 0));
    }

    #[test]
    fn test_a_move_of_nothing_changes_nothing_and_keeps_the_optimum_by_hand() {
        let held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();
        let sums = network.sums;

        for (from_node, to_node, amount) in [(1, 1, 5), (2, 2, u64::MAX), (0, 2, 0)] {
            assert_eq!(network.move_supply(from_node, to_node, amount), Ok(()));
        }
        assert_eq!((network.standing, network.sums), (Standing::Optimal, sums));
        assert_eq!(network.optimum().map(|optimum| optimum.total_cost), Some(14));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 5]));
    }

    #[test]
    fn test_a_refused_move_changes_nothing_by_hand() {
        // The depot solved: a supply past `i64` either way, the node the amount is taken from named first.
        let held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();
        let sums = network.sums;
        let refusals = [
            (0, 1, u64::MAX, 0, 4 - i128::from(u64::MAX)),
            (1, 2, (1 << 63) + 1, 1, -(1 << 63) - 1),
            (1, 0, i64::MAX.cast_unsigned(), 0, i128::from(i64::MAX) + 4),
        ];
        for (from_node, to_node, amount, node, supply) in refusals {
            let refused = network.move_supply(from_node, to_node, amount);
            assert_eq!(refused, Err(ChangeError::SupplyBeyondRange { node, supply }));
        }
        assert_eq!((network.standing, network.sums), (Standing::Optimal, sums));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 5]));

        // Four nodes and no arc: two moves of `2^63 − 1` strand their flow, `2^64 − 2` up the root links in all; a move
        // of one unit more is refused, the flow up and its amount adding up to `2^64 − 1`.
        let mut held = MincostNetwork::from_arcs(&[0, 0, 0, 0], &[]);
        let mut network = loaded(&held);
        let half = i64::MAX.cast_unsigned();
        assert_eq!((network.move_supply(2, 0, half), network.move_supply(3, 1, half)), (Ok(()), Ok(())));
        held.supplies = vec![i64::MAX, i64::MAX, -i64::MAX, -i64::MAX];
        assert_eq!(network.sums.excess(), NET_SUPPLY_LIMIT);
        let sums = network.sums;
        let total = u128::from(u64::MAX);
        assert_eq!(network.move_supply(2, 3, 1), Err(ChangeError::ExcessBeyondRange { total }));
        assert_eq!((network.standing, network.sums), (Standing::Changed, sums));
        assert!(checked(&mut network, &held).is_ok());
    }

    #[test]
    fn test_a_large_move_the_tree_carries_and_its_set_back_are_admitted_by_hand() {
        // Node 0 sends one unit to node 1 over an arc of capacity `2^64 − 1`, solved: the arc is in the tree. A move of
        // `2^63 − 2` the tree carries leaves nothing up the root links, so its set-back is admitted before any solve.
        let mut held = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, u64::MAX, 1)]);
        let mut network = loaded(&held);
        assert_eq!(solved(&mut network).0, Some(1));
        let amount = i64::MAX.cast_unsigned() - 1;

        assert_eq!(network.move_supply(1, 0, amount), Ok(()));
        held.supplies = vec![i64::MAX, -i64::MAX];
        assert_eq!(network.sums.excess(), 0);
        assert!(checked(&mut network, &held).is_ok());
        assert_eq!(network.move_supply(0, 1, amount), Ok(()));
        held.supplies = vec![1, -1];
        assert_eq!((network.sums.excess(), network.sums.kept_cost()), (0, 1));
        assert!(checked(&mut network, &held).is_ok());
        assert_eq!(solved(&mut network), (Some(1), Work::default()));
    }

    #[test]
    fn test_a_move_on_a_network_never_loaded_panics_at_its_nodes() {
        let mut network = Network::new();
        let unwound = catch_unwind(AssertUnwindSafe(|| network.move_supply(0, 0, 1)));
        let message = unwound.expect_err("a network never loaded has no node");
        assert_eq!(
            message.downcast_ref::<String>().map(String::as_str),
            Some("called `Network::move_supply` with the nodes 0 and 0, which should be below the node count 0")
        );
        assert_eq!(network.standing, Standing::Unloaded);
    }

    #[test]
    fn test_a_move_on_a_half_changed_network_panics_before_any_change() {
        let mut network = loaded(&depot());
        let _ = network.solve();
        half_changed(&mut network);
        let (sums, supplies) = (network.sums, network.supplies.clone());

        // A move of nothing panics too: the half-changed check comes first.
        for (from_node, to_node, amount) in [(2, 0, 1), (1, 1, 0)] {
            let unwound = catch_unwind(AssertUnwindSafe(|| network.move_supply(from_node, to_node, amount)));
            let message = unwound.expect_err("the move should panic");
            assert_eq!(
                panic_text(&message),
                Some("called `Network::move_supply` on a network a panic left half changed")
            );
        }
        assert_eq!((network.standing, network.sums, &network.supplies), (Standing::Changed, sums, &supplies));

        network.load(&problem_of(&depot())).expect("the depot should be admitted");
        assert_eq!(network.move_supply(2, 1, 1), Ok(()), "a load mends it");
    }

    #[test]
    fn test_a_move_that_unwinds_after_its_first_write_leaves_the_network_half_changed() {
        // Five nodes and no arc, supplies `i64::MAX`, `i64::MAX`, 0, `−i64::MAX`, `−i64::MAX`: the star's root links
        // carry `2^64 − 2` up. The sums are made to read no flow up, by the step a solve ends with, so a move of one
        // unit from node 4 to node 2 is admitted: node 2's root link takes it up, and the route asserts `2^64 − 1` up,
        // past the limit, after the standing and the supplies were written.
        let held = MincostNetwork::from_arcs(&[i64::MAX, i64::MAX, 0, -i64::MAX, -i64::MAX], &[]);
        let mut network = loaded(&held);
        network.sums.solved(0);

        let unwound = catch_unwind(AssertUnwindSafe(|| network.move_supply(4, 2, 1)));
        let message = unwound.expect_err("the route should assert the flow up the root links");
        assert_eq!(
            message.downcast_ref::<String>().map(String::as_str),
            Some(
                "called `Basis::route` with the amount 1, which leaves 18446744073709551615 up the root links, past \
                 18446744073709551614"
            )
        );

        // Half changed, as documented: changed, no basis held, no optimum; the counts and the supplies written.
        assert_eq!((network.standing, network.basis.kept(), network.optimum()), (Standing::Changed, None, None));
        assert_eq!((network.node_count(), network.arc_count()), (5, 0));
        assert_eq!(network.supplies.raw, [i64::MAX, i64::MAX, 1, -i64::MAX, i64::MIN]);
        assert!(network.try_reserve(5, 0).is_ok() && !format!("{:?}", network.clone()).is_empty());

        // The calls that read the basis panic before any change.
        let half = "on a network a panic left half changed";
        let moved = catch_unwind(AssertUnwindSafe(|| network.move_supply(0, 1, 1))).expect_err("the move should panic");
        assert_eq!(panic_text(&moved), Some("called `Network::move_supply` on a network a panic left half changed"));
        let unwound_solve = catch_unwind(AssertUnwindSafe(|| {
            let _ = network.solve();
        }));
        let unwound_solve = unwound_solve.expect_err("the solve should panic");
        assert!(panic_text(&unwound_solve).is_some_and(|text| text.ends_with(half)));

        // A load mends it.
        network.load(&problem_of(&held)).expect("the star should be admitted");
        assert!(checked(&mut network, &held).is_ok());
        assert_eq!(solved(&mut network), (None, Work::default()));
    }

    /// The sums of `network`: the kept cost, the excess and the cost total.
    fn sums_of(network: &Network) -> (i128, u64, u128) {
        (network.sums.kept_cost(), network.sums.excess(), network.sums.cost_total())
    }

    #[test]
    fn test_a_capacity_under_a_tree_arc_running_down_strands_flow_and_its_set_back_mends_it_by_hand() {
        // The depot's optimum: node 2 hangs from node 0 by arc 0, running down, carrying 2 of 3. Its capacity falls to
        // 1: it brings node 2 one unit and leaves the tree at its capacity; node 2 hangs from the root by its root
        // link, down, carrying the other, at `M`; node 0's root link takes the unit up. The kept cost gains `−5 + M`,
        // the excess one, the cost total `5·(1 − 3)`. Node 0 cannot send 4 units by 1 and 2: no flow, and no arc
        // violates.
        let mut held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();
        let root_cost = i128::from(DEPOT_ROOT_COST);

        assert_eq!(network.set_capacity(0, 1), Ok(3));
        held.capacities[0] = 1;
        assert_eq!((network.standing, network.optimum()), (Standing::Changed, None));
        assert_eq!(sums_of(&network), (9 + root_cost, 1, 11));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, DEPOT_ROOT_COST]));
        assert_eq!(solved(&mut network), (None, Work::default()));
        assert_eq!(solvers_cost(&held), None);

        // Set back to 3: arc 0, out of the tree at its capacity, carries 3, two more, which leave node 2 for node 0 in
        // two components: node 2's root link turns up, node 2 falling to 0; node 0's turns down, it and node 1 rising
        // by `M`. The kept cost gains `5·2 − M + M`; the excess stays one; then the depot's optimum again.
        assert_eq!(network.set_capacity(0, 3), Ok(1));
        held.capacities[0] = 3;
        assert_eq!(sums_of(&network), (19 + root_cost, 1, 21));
        assert_eq!(checked(&mut network, &held), Ok(vec![DEPOT_ROOT_COST, DEPOT_ROOT_COST + 1, 0]));
        let (cost, work) = solved(&mut network);
        assert_eq!((cost, work.pivots > 0, network.sums.excess()), (Some(14), true, 0));
    }

    #[test]
    fn test_a_capacity_under_a_tree_arc_running_up_cuts_it_and_turns_the_top_by_hand() {
        // The depot turned around, solved: node 2 under the root, node 0 under it by arc 0 running down, node 1 under
        // node 0 by arc 1 running up from it, carrying 2 of 4. Its capacity falls to 1: it sends one, at its new
        // capacity, with no room left, and node 1's root link takes the other up, node 1 at 0. Node 0 gets one less
        // from node 1: arc 0 brings it one more, to its capacity 3, where it can still bring less; node 2 must send one
        // more, which its root link, up and empty, turns down to bring: nodes 2 and 0 rise by `M`. The kept cost gains
        // `−1 + 5 + M`, the excess one, the cost total `1·(1 − 4)`. Then 3 directly and one by arcs 2 and 1: 17.
        let mut held = depot_turned_around();
        let mut network = loaded(&held);
        let _ = network.solve();

        assert_eq!(network.set_capacity(1, 1), Ok(4));
        held.capacities[1] = 1;
        assert_eq!(sums_of(&network), (18 + i128::from(DEPOT_ROOT_COST), 1, 18));
        assert_eq!(checked(&mut network, &held), Ok(vec![5 + DEPOT_ROOT_COST, 0, DEPOT_ROOT_COST]));
        assert_eq!((solved(&mut network).0, solvers_cost(&held)), (Some(17), Some(17)));
    }

    #[test]
    fn test_a_tree_arc_switched_off_and_on_again_by_hand() {
        // The depot's optimum: arc 1 hangs node 1 from node 0, running down, carrying 2. Switched off, it carries
        // nothing and, with no width, leaves the tree unpriced; node 1 hangs from the root by its root link, down,
        // carrying both, at `M`; node 0's root link takes the two up. The kept cost gains `−2 + 2·M`, the excess two,
        // the cost total `−4`. Node 0 can send only 3 units directly: no flow.
        let mut held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();

        assert_eq!(network.set_capacity(1, 0), Ok(4));
        held.capacities[1] = 0;
        assert_eq!(sums_of(&network), (12 + 2 * i128::from(DEPOT_ROOT_COST), 2, 17));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, DEPOT_ROOT_COST, 5]));
        assert_eq!((solved(&mut network).0, solvers_cost(&held)), (None, None));

        // On again: an arc of no width out of the tree that gains some stands at its lower bound; nothing moves but
        // the cost total.
        let (kept, excess, _) = sums_of(&network);
        assert_eq!(network.set_capacity(1, 4), Ok(0));
        held.capacities[1] = 4;
        assert_eq!(sums_of(&network), (kept, excess, 21));
        assert!(checked(&mut network, &held).is_ok());
        assert_eq!(solved(&mut network).0, Some(14));
    }

    #[test]
    fn test_a_lower_bound_above_a_tree_arcs_flow_cuts_it_and_turns_the_top_by_hand() {
        // The depot's optimum: arc 0 carries 2 down to node 2. Its lower bound rises to its capacity 3: it holds one
        // below it, with no width, carries nothing above it and leaves the tree; node 2's root link takes up the unit
        // it no longer needs, at 0; node 0's, up and empty, turns down to bring the unit it sends more, nodes 0 and 1
        // rising by `M`. The arc's whole flow rose by one: the kept cost gains `5 + M`; the excess one; the cost total
        // stays. Then 3 directly and one through node 1: 17.
        let mut held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();

        assert_eq!(network.set_lower_bound(0, 3), Ok(0));
        held.lower_bounds[0] = 3;
        assert_eq!((network.lower_bounds.as_slice(), &network.supplies.raw), (&[3, 0, 0][..], &held.supplies));
        assert_eq!(sums_of(&network), (19 + i128::from(DEPOT_ROOT_COST), 1, 21));
        assert_eq!(checked(&mut network, &held), Ok(vec![DEPOT_ROOT_COST, DEPOT_ROOT_COST + 1, 0]));
        assert_eq!((solved(&mut network).0, solvers_cost(&held)), (Some(17), Some(17)));
    }

    #[test]
    fn test_bounds_on_a_full_arc_out_of_the_tree_by_hand() {
        // The depot's optimum: arc 2, from node 1 to node 2, out of the tree at its capacity 2.
        let mut network = loaded(&depot());
        let _ = network.solve();
        let solved_depot = network.clone();

        // Its capacity falls to 1: one unit less by it, one less by arc 1, one more by arc 0. The kept cost gains
        // `−1 − 1 + 5`, the cost total `−1`: `3·5 + 1 + 1 = 17`, optimal.
        let mut held = depot();
        assert_eq!(network.set_capacity(2, 1), Ok(2));
        held.capacities[2] = 1;
        assert_eq!(sums_of(&network), (17, 0, 20));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 5]));
        assert_eq!(solved(&mut network), (Some(17), Work::default()));

        // Its capacity rises to 3: one more by it and by arc 1, one less by arc 0: `5 + 3 + 3 = 11`, optimal.
        let (mut network, mut held) = (solved_depot.clone(), depot());
        assert_eq!(network.set_capacity(2, 3), Ok(2));
        held.capacities[2] = 3;
        assert_eq!(sums_of(&network), (11, 0, 22));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 5]));
        assert_eq!(solved(&mut network), (Some(11), Work::default()));

        // Its lower bound rises to 1, under its flow: nothing moves, not even the kept cost.
        let (mut network, mut held) = (solved_depot, depot());
        assert_eq!(network.set_lower_bound(2, 1), Ok(0));
        held.lower_bounds[2] = 1;
        assert_eq!((network.standing, sums_of(&network)), (Standing::Changed, (14, 0, 21)));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 5]));
        assert_eq!(solved(&mut network), (Some(14), Work::default()));
    }

    #[test]
    fn test_bounds_that_cross_in_one_call_and_in_two_by_hand() {
        // The depot's optimum, arc 2 at its capacity 2. To `3..=4`: the lower bound first is refused, past the capacity
        // held. Both at once: arc 2 goes to its new capacity, its whole flow 4; arc 0 gives node 2 two less, to
        // nothing, and leaves the tree, node 2 hanging up and empty at 0; arc 1 brings node 1 two more. The kept cost
        // gains `1·2 − 5·2 + 1·2`, the cost total `1·2`: every unit through node 1, `4 + 4 = 8`.
        let mut held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();
        let mut two_calls = network.clone();
        let whole = format!("{network:?}");

        let refused = ChangeError::LowerAboveCapacity { arc: 2, lower_bound: 3, capacity: 2 };
        assert_eq!(network.set_lower_bound(2, 3), Err(refused));
        assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");
        let crossed = Bounds { lower_bound: 3, capacity: 4 };
        assert_eq!(network.set_bounds(2, crossed), Ok(Bounds { lower_bound: 0, capacity: 2 }));
        (held.lower_bounds[2], held.capacities[2]) = (3, 4);
        assert_eq!(sums_of(&network), (8, 0, 23));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 0]));

        // In two calls, the capacity first: it moves the same flow; the lower bound then restates it under it.
        assert_eq!((two_calls.set_capacity(2, 4), two_calls.set_lower_bound(2, 3)), (Ok(2), Ok(0)));
        assert_eq!(sums_of(&two_calls), (8, 0, 23));
        assert_eq!(checked(&mut two_calls, &held), Ok(vec![0, 1, 0]));
        assert_eq!((solved(&mut network).0, solved(&mut two_calls).0), (Some(8), Some(8)));

        // Back to `0..=2`: the capacity first is refused, below the lower bound held; both at once, the depot again.
        let refused = ChangeError::LowerAboveCapacity { arc: 2, lower_bound: 3, capacity: 2 };
        let whole = format!("{network:?}");
        assert_eq!(network.set_capacity(2, 2), Err(refused));
        assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");
        assert_eq!(network.set_bounds(2, Bounds { lower_bound: 0, capacity: 2 }), Ok(crossed));
        assert!(checked(&mut network, &depot()).is_ok());
        assert_eq!(solved(&mut network).0, Some(14));
    }

    #[test]
    fn test_bounds_on_an_arc_of_no_width_move_nothing_by_hand() {
        // The depot with arc 3 carrying one unit from node 0 to node 1, fixed, at 4: solved at 17. Its lower bound
        // falls to 0: it carries 1 above it, its whole width, at its capacity; nothing moves, not the kept cost either.
        // The solve then lets arc 1 carry that unit at 1: 14.
        let mut held = depot();
        held.add_arc(0, 1, 1, 1, 4);
        let mut network = loaded(&held);
        assert_eq!(solved(&mut network).0, Some(17));
        let solved_fixed = network.clone();

        assert_eq!(network.set_lower_bound(3, 0), Ok(1));
        held.lower_bounds[3] = 0;
        assert_eq!(sums_of(&network), (17, 0, 25));
        assert!(checked(&mut network, &held).is_ok());
        assert_eq!(solved(&mut network).0, Some(14));

        // Its capacity rises to 3 instead: it stands at its lower bound, carrying the fixed unit; nothing moves, and
        // the cost total gains `4·2`. The solve sends nothing more by it, at 4: 17.
        let (mut network, mut held) = (solved_fixed, depot());
        held.add_arc(0, 1, 1, 3, 4);
        assert_eq!(network.set_capacity(3, 3), Ok(1));
        assert_eq!(sums_of(&network), (17, 0, 33));
        assert!(checked(&mut network, &held).is_ok());
        assert_eq!(solved(&mut network), (Some(17), Work::default()));
    }

    #[test]
    fn test_bounds_set_to_the_bounds_held_change_nothing_and_keep_the_optimum_by_hand() {
        let held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();
        let sums = network.sums;

        assert_eq!((network.set_capacity(0, 3), network.set_lower_bound(1, 0)), (Ok(3), Ok(0)));
        assert_eq!(
            network.set_bounds(2, Bounds { lower_bound: 0, capacity: 2 }),
            Ok(Bounds { lower_bound: 0, capacity: 2 })
        );
        assert_eq!((network.standing, network.sums), (Standing::Optimal, sums));
        assert_eq!(network.optimum().map(|optimum| optimum.total_cost), Some(14));
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 5]));
    }

    #[test]
    fn test_refused_bounds_change_nothing_by_hand() {
        // A lower bound above the capacity, by each setter.
        let held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();
        let (sums, whole) = (network.sums, format!("{network:?}"));
        let refused = |lower_bound, capacity| Err(ChangeError::LowerAboveCapacity { arc: 0, lower_bound, capacity });
        assert_eq!(network.set_lower_bound(0, 4), refused(4, 3));
        let crossed = Err(ChangeError::LowerAboveCapacity { arc: 0, lower_bound: 2, capacity: 1 });
        assert_eq!(network.set_bounds(0, Bounds { lower_bound: 2, capacity: 1 }), crossed);
        assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");
        assert_eq!(network.set_lower_bound(0, 1), Ok(0));
        let whole = format!("{network:?}");
        assert_eq!(network.set_capacity(0, 0), refused(1, 0));
        assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");
        assert_eq!(network.set_lower_bound(0, 0), Ok(1));
        assert_eq!(network.sums, sums, "the lower bound set back moves the sums back");
        assert_eq!(checked(&mut network, &held), Ok(vec![0, 1, 5]));

        // Four loops at one node whose costs times capacities add up to `i128::MAX` exactly: a capacity that raises a
        // term is refused; one that keeps or lowers it is not.
        let (bound, wide) = (cost_bound(1), u64::MAX);
        let arcs = [(0, 0, 0, wide, bound), (0, 0, 0, wide, -bound), (0, 0, 0, wide, 2), (0, 0, 0, (1 << 63) - 1, 1)];
        let mut held = MincostNetwork::from_arcs(&[0], &arcs);
        let mut network = loaded(&held);
        let whole = format!("{network:?}");
        assert_eq!(network.set_capacity(3, 1 << 63), Err(ChangeError::CostTotalBeyondRange { arc: 3 }));
        let crossed = Bounds { lower_bound: 1, capacity: 1 << 63 };
        assert_eq!(network.set_bounds(3, crossed), Err(ChangeError::CostTotalBeyondRange { arc: 3 }));
        assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");
        assert_eq!(network.set_capacity(3, (1 << 63) - 2), Ok((1 << 63) - 1));
        held.capacities[3] = (1 << 63) - 2;
        assert_eq!(network.sums.cost_total(), i128::MAX.cast_unsigned() - 1);
        assert!(checked(&mut network, &held).is_ok());
    }

    #[test]
    fn test_a_lower_bound_that_moves_more_than_the_root_links_admit_is_refused_by_hand() {
        // Two nodes of no supply and an arc between them of capacity `2^64 − 1` at 1, empty at the optimum. A lower
        // bound of `2^64 − 1` would move that much: refused. One of `2^64 − 2` moves the limit exactly: it leaves node
        // 1 up its root link and arrives at node 0 down its own, which turns, at `M`. The kept cost gains `L + M·L`.
        let mut held = MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, u64::MAX, 1)]);
        let mut network = loaded(&held);
        assert_eq!(solved(&mut network).0, Some(0));
        let whole = format!("{network:?}");

        let total = u128::from(u64::MAX);
        assert_eq!(network.set_lower_bound(0, u64::MAX), Err(ChangeError::ExcessBeyondRange { total }));
        assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");

        let (limit, root_cost) = (i128::from(NET_SUPPLY_LIMIT), i128::from(widest_root_cost(2)));
        assert_eq!(network.set_lower_bound(0, NET_SUPPLY_LIMIT), Ok(0));
        held.lower_bounds[0] = NET_SUPPLY_LIMIT;
        assert_eq!(sums_of(&network), (limit + root_cost * limit, NET_SUPPLY_LIMIT, total));
        assert_eq!(checked(&mut network, &held), Ok(vec![widest_root_cost(2), 0]));
        assert_eq!((solved(&mut network), solvers_cost(&held)), ((None, Work::default()), None));

        // Set back directly: the flow up the root links and twice the change pass the limit, and the move back of the
        // flow is refused, as documented; a load of the earlier problem undoes it.
        let (back, whole) = (2 * u128::from(NET_SUPPLY_LIMIT), format!("{network:?}"));
        assert_eq!(network.set_lower_bound(0, 0), Err(ChangeError::ExcessBeyondRange { total: back }));
        assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");
        held.lower_bounds[0] = 0;
        network.load(&problem_of(&held)).expect("the earlier problem should be admitted");
        assert_eq!(solved(&mut network).0, Some(0));
    }

    #[test]
    fn test_bounds_on_a_network_never_loaded_panic_at_their_arc() {
        let messages = [
            ("set_capacity", catch_unwind(AssertUnwindSafe(|| Network::new().set_capacity(0, 1).map(|_| ())))),
            ("set_lower_bound", catch_unwind(AssertUnwindSafe(|| Network::new().set_lower_bound(0, 1).map(|_| ())))),
            (
                "set_bounds",
                catch_unwind(AssertUnwindSafe(|| {
                    Network::new().set_bounds(0, Bounds { lower_bound: 0, capacity: 1 }).map(|_| ())
                })),
            ),
        ];
        for (name, unwound) in messages {
            let message = unwound.expect_err("a network never loaded has no arc");
            let expected = format!("called `Network::{name}` with arc 0, which should be below the arc count 0");
            assert_eq!(message.downcast_ref::<String>(), Some(&expected));
        }
    }

    #[test]
    fn test_bounds_on_a_half_changed_network_panic_before_any_change() {
        let mut network = loaded(&depot());
        let _ = network.solve();
        half_changed(&mut network);
        let (sums, lower_bounds) = (network.sums, network.lower_bounds.clone());

        let bounds = Bounds { lower_bound: 1, capacity: 3 };
        let unwound = [
            ("set_capacity", catch_unwind(AssertUnwindSafe(|| network.set_capacity(0, 1).map(|_| ())))),
            ("set_lower_bound", catch_unwind(AssertUnwindSafe(|| network.set_lower_bound(0, 1).map(|_| ())))),
            ("set_bounds", catch_unwind(AssertUnwindSafe(|| network.set_bounds(0, bounds).map(|_| ())))),
        ];
        for (name, unwound) in unwound {
            let message = unwound.expect_err("the change should panic");
            let expected = format!("called `Network::{name}` on a network a panic left half changed");
            assert_eq!(panic_text(&message).map(str::to_owned), Some(expected));
        }
        assert_eq!((network.standing, network.sums, &network.lower_bounds), (Standing::Changed, sums, &lower_bounds));

        network.load(&problem_of(&depot())).expect("the depot should be admitted");
        assert_eq!(network.set_bounds(0, bounds), Ok(Bounds { lower_bound: 0, capacity: 3 }), "a load mends it");
    }

    #[test]
    fn test_bounds_that_unwind_after_their_first_write_leave_the_network_half_changed() {
        // A solved network whose room still says it holds a basis but whose tree is no longer kept: each setter reads
        // the arc and admits the bounds, writes its standing and the lower bound, and unwinds at its reopen. Half
        // changed, as documented; a load mends it.
        let bounds = Bounds { lower_bound: 1, capacity: 2 };
        for setter in [Setter::Capacity(bounds), Setter::LowerBound(bounds), Setter::Both(bounds)] {
            let mut network = loaded(&depot());
            let _ = network.solve();
            untie_the_kept_tree(&mut network.basis);

            let unwound = catch_unwind(AssertUnwindSafe(|| match setter {
                Setter::Capacity(bounds) => network.set_capacity(0, bounds.capacity).map(|_| ()),
                Setter::LowerBound(bounds) => network.set_lower_bound(0, bounds.lower_bound).map(|_| ()),
                Setter::Both(bounds) => network.set_bounds(0, bounds).map(|_| ()),
            }));
            assert!(unwound.is_err(), "a change of a network whose basis cannot be lent again should panic");
            assert_eq!((network.standing, network.basis.kept(), network.optimum()), (Standing::Changed, None, None));
            assert_eq!((network.node_count(), network.arc_count()), (3, 3));

            network.load(&problem_of(&depot())).expect("the depot should be admitted");
            assert_eq!(solved(&mut network).0, Some(14));
        }

        // Five nodes, supplies `i64::MAX`, `i64::MAX`, 0, `−i64::MAX`, `−i64::MAX`, and one arc from node 4 to node 2
        // of capacity 1: the star's root links carry `2^64 − 2` up. The sums are made to read no flow up, so a lower
        // bound of 1 on the arc is admitted: one more leaves node 2 up its root link, and the restatement asserts
        // `2^64 − 1` up, after the standing and the lower bound were written.
        let held = MincostNetwork::from_arcs(&[i64::MAX, i64::MAX, 0, -i64::MAX, -i64::MAX], &[(4, 2, 0, 1, 0)]);
        let mut network = loaded(&held);
        network.sums.solved(0);

        let unwound = catch_unwind(AssertUnwindSafe(|| network.set_lower_bound(0, 1)));
        let message = unwound.expect_err("the restatement should assert the flow up the root links");
        assert_eq!(
            message.downcast_ref::<String>().map(String::as_str),
            Some(
                "called `Basis::restate` with Slot 0, which leaves 18446744073709551615 up the root links, past \
                 18446744073709551614"
            )
        );
        assert_eq!(
            (network.standing, network.basis.kept(), network.lower_bounds.as_slice()),
            (Standing::Changed, None, &[1][..])
        );
        let again = catch_unwind(AssertUnwindSafe(|| network.set_capacity(0, 2))).expect_err("the change should panic");
        assert_eq!(panic_text(&again), Some("called `Network::set_capacity` on a network a panic left half changed"));
        network.load(&problem_of(&held)).expect("the star should be admitted");
        assert!(checked(&mut network, &held).is_ok());
    }

    /// One call of a random sequence on a network. Plain data.
    #[derive(Clone, Copy, Debug)]
    enum Call {
        /// The cost drawn set on the arc drawn.
        SetCost(Index, CostDraw),
        /// The amount drawn moved from the supply of the node drawn to that of the node drawn.
        MoveSupply(Index, Index, AmountDraw),
        /// The capacity drawn set on the arc drawn.
        SetCapacity(Index, BoundDraw),
        /// The lower bound drawn set on the arc drawn.
        SetLowerBound(Index, BoundDraw),
        /// The lower bound and the capacity drawn set on the arc drawn at once.
        SetBounds(Index, BoundDraw, BoundDraw),
        /// A load of the problem drawn from the problem held.
        Load(LoadDraw),
        /// A solve.
        Solve,
    }

    /// A problem a random call loads, drawn from the problem held. Plain data.
    #[derive(Clone, Copy, Debug)]
    enum LoadDraw {
        /// The problem held with numbers drawn anew from this seed, on the same arcs: each arc's cost from `-9..=9` and
        /// its width from `0..=2`, each half the time, and up to 2 units of supply moved from one node to another.
        Moved(u64),
        /// The problem held with the arc drawn turned around: another tail and head at its slot, unless it is a loop.
        Turned(Index),
        /// The problem held with one node more, of no supply: other counts.
        NodeAdded,
        /// The problem held with one unit more supply at node 0, or one less where it is `i64::MAX`: refused, its
        /// supplies not balancing.
        Unbalanced,
    }

    /// A load of a problem drawn from the problem held.
    fn load_call() -> impl Strategy<Value = Call> {
        let draw = prop_oneof![
            4 => any::<u64>().prop_map(LoadDraw::Moved),
            1 => any::<Index>().prop_map(LoadDraw::Turned),
            1 => Just(LoadDraw::NodeAdded),
            1 => Just(LoadDraw::Unbalanced),
        ];

        draw.prop_map(Call::Load)
    }

    /// The problem `draw` makes of `held`.
    fn load_drawn(held: &MincostNetwork, draw: LoadDraw) -> MincostNetwork {
        let mut next = held.clone();
        match draw {
            LoadDraw::Moved(seed) => {
                let mut draws = SplitMix64::seed_from_u64(seed);
                for arc in 0..held.arc_count() {
                    if draws.random_bool(0.5) {
                        next.costs[arc] = draws.random_range(-9..=9);
                    }
                    if draws.random_bool(0.5) {
                        next.capacities[arc] = held.lower_bounds[arc].saturating_add(draws.random_range(0..=2));
                    }
                }
                // A move of up to 2 units between two other nodes, where both supplies stay within `i64`.
                let (from_node, to_node) =
                    (draws.random_range(0..held.node_count()), draws.random_range(0..held.node_count()));
                let amount = draws.random_range(0..=2);
                let moved = (held.supplies[from_node].checked_sub(amount), held.supplies[to_node].checked_add(amount));
                if let (Some(from), Some(to)) = moved
                    && from_node != to_node
                {
                    (next.supplies[from_node], next.supplies[to_node]) = (from, to);
                }
            }
            LoadDraw::Turned(arc) if held.arc_count() > 0 => {
                let arc = arc.index(held.arc_count());
                (next.tails[arc], next.heads[arc]) = (held.heads[arc], held.tails[arc]);
            }
            LoadDraw::Turned(_) => {}
            LoadDraw::NodeAdded => {
                next.add_node(0);
            }
            LoadDraw::Unbalanced => {
                let supply = &mut next.supplies[0];
                *supply = supply.checked_add(1).unwrap_or(i64::MAX - 1);
            }
        }

        next
    }

    /// A cost drawn set on an arc drawn.
    fn cost_call() -> impl Strategy<Value = Call> {
        (any::<Index>(), cost_draws(-9..=9)).prop_map(|(arc, draw)| Call::SetCost(arc, draw))
    }

    /// An amount drawn moved from the supply of a node drawn to that of a node drawn.
    fn supply_call() -> impl Strategy<Value = Call> {
        (any::<Index>(), any::<Index>(), amount_draws(0..=3))
            .prop_map(|(from_node, to_node, draw)| Call::MoveSupply(from_node, to_node, draw))
    }

    /// Sequences of one to twelve calls, two costs set and two moves of supply to every solve, and no bounds: moves
    /// enough between solves that large ones strand more than the root links admit.
    fn cost_and_supply_calls() -> impl Strategy<Value = Vec<Call>> {
        proptest::collection::vec(prop_oneof![2 => cost_call(), 2 => supply_call(), 1 => Just(Call::Solve)], 1..=12)
    }

    /// Sequences of one to sixteen calls of every kind: to every two solves two costs set, four moves of supply, a
    /// capacity set, a lower bound set and two pairs of bounds set.
    fn calls() -> impl Strategy<Value = Vec<Call>> {
        let capacity = (any::<Index>(), bound_draws(0..=4)).prop_map(|(arc, draw)| Call::SetCapacity(arc, draw));
        let lower_bound = (any::<Index>(), bound_draws(0..=4)).prop_map(|(arc, draw)| Call::SetLowerBound(arc, draw));
        let bounds = (any::<Index>(), bound_draws(0..=4), bound_draws(0..=4))
            .prop_map(|(arc, lower_bound, capacity)| Call::SetBounds(arc, lower_bound, capacity));
        let call = prop_oneof![
            2 => cost_call(),
            4 => supply_call(),
            1 => capacity,
            1 => lower_bound,
            2 => bounds,
            2 => Just(Call::Solve),
        ];

        proptest::collection::vec(call, 1..=16)
    }

    /// Sequences of one to sixteen calls of every kind, loads among them: to every two solves two costs set, four moves
    /// of supply, a capacity set, a lower bound set, two pairs of bounds set and two loads.
    fn calls_with_loads() -> impl Strategy<Value = Vec<Call>> {
        let capacity = (any::<Index>(), bound_draws(0..=4)).prop_map(|(arc, draw)| Call::SetCapacity(arc, draw));
        let lower_bound = (any::<Index>(), bound_draws(0..=4)).prop_map(|(arc, draw)| Call::SetLowerBound(arc, draw));
        let bounds = (any::<Index>(), bound_draws(0..=4), bound_draws(0..=4))
            .prop_map(|(arc, lower_bound, capacity)| Call::SetBounds(arc, lower_bound, capacity));
        let call = prop_oneof![
            2 => cost_call(),
            4 => supply_call(),
            1 => capacity,
            1 => lower_bound,
            2 => bounds,
            2 => load_call(),
            2 => Just(Call::Solve),
        ];

        proptest::collection::vec(call, 1..=16)
    }

    /// The amount `draw` names for a move from a node of supply `from_supply` to one of `to_supply`: the largest amount
    /// where one past an end would pass it.
    fn amount_of(draw: AmountDraw, from_supply: i64, to_supply: i64) -> u64 {
        match draw {
            AmountDraw::PastLeastSupply if from_supply == i64::MAX => u64::MAX,
            AmountDraw::PastMostSupply if to_supply == i64::MIN => u64::MAX,
            _ => draw.amount(from_supply, to_supply),
        }
    }

    /// What a random sequence of calls reached, counted. Plain data.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct Reached {
        /// Costs set that moved a potential.
        shifted: usize,
        /// Costs set that changed the cost and moved no potential.
        unshifted: usize,
        /// Costs set to the cost held.
        unchanged: usize,
        /// Costs refused.
        refused: usize,
        /// Solves that pivoted.
        pivoted: usize,
        /// Moves of supply the tree carried, the flow up the root links left as it was.
        carried: usize,
        /// Moves of supply that raised the flow up the root links.
        raised: usize,
        /// Moves of supply of nothing.
        moved_nothing: usize,
        /// Moves of supply refused for a supply past `i64`.
        refused_supply: usize,
        /// Moves of supply refused for the flow up the root links.
        refused_excess: usize,
        /// Moves that left the flow up the root links where it was or lower, set back on a copy and admitted.
        set_back: usize,
        /// Moves that turned the root link of a node under the root.
        turned: usize,
        /// Bounds set to the bounds held.
        unchanged_bounds: usize,
        /// Bounds refused for a lower bound above the capacity.
        refused_lower: usize,
        /// Bounds refused for the flow up the root links.
        refused_bounds_excess: usize,
        /// Bounds that moved no flow.
        unmoved_bounds: usize,
        /// Bounds that raised the flow up the root links.
        raised_by_bounds: usize,
        /// Capacities of zero set on an arc that carried flow above its lower bound.
        switched_off: usize,
        /// Arcs of no width given some.
        switched_on: usize,
        /// Capacities raised on an arc that carried its capacity.
        at_capacity_raised: usize,
        /// Capacities lowered on an arc that carried its capacity.
        at_capacity_lowered: usize,
        /// Lower bounds raised above an arc's flow.
        lower_above_flow: usize,
        /// Lower bounds lowered below an arc's flow.
        lower_below_flow: usize,
        /// Tree arcs left with no width.
        tree_arc_emptied: usize,
        /// Bounds set at once past both old ones, or below both.
        crossed_at_once: usize,
        /// Single bounds refused past the other bound held, which the two calls of a crossing in the wrong order meet.
        crossed_in_two: usize,
        /// Crossings made by the two single setters in the admitted order, one call after the other.
        crossed_in_two_admitted: usize,
        /// Lower bounds raised, staying below an arc's flow.
        lower_raised_below_flow: usize,
        /// Lower bounds raised to an arc's flow exactly.
        lower_raised_to_flow: usize,
        /// Bounds set back on a copy and admitted.
        bounds_set_back: usize,
        /// Nodes cut by a change whose subtree's potentials moved, which marks the smaller side of it.
        cut_and_moved: usize,
        /// Nodes cut by a change whose potentials did not move: an arc that left the tree, tight, marking nothing.
        cut_and_unmoved: usize,
        /// Changes that turned two nodes under the root, each of which marks its smaller side.
        turned_two_tops: usize,
        /// Loads that started from the basis held.
        kept_loads: usize,
        /// Loads from the basis held after which an arc of its tree stayed in the tree.
        kept_tree_arcs: usize,
        /// Loads from the basis held after which an arc of its tree left it.
        kept_cuts: usize,
        /// Loads of other counts, from the star.
        loads_of_other_counts: usize,
        /// Loads of the same counts and another tail or head at a slot, from the star.
        loads_of_other_ends: usize,
        /// Loads refused.
        refused_loads: usize,
    }

    /// The way each node's own root link points where the node hangs by it; `None` where it hangs by an arc.
    ///
    /// # Panics
    /// Panics if the network holds no basis.
    fn root_links_of(network: &mut Network) -> Vec<Option<Direction>> {
        let basis = network.basis.reopen().expect("the network should hold its basis");
        let directions = root_link_directions(&basis);
        basis.keep();

        directions
    }

    /// Runs `calls` on `held` loaded into a network, the test's copy of the problem changed as each cost set is
    /// admitted. After every call the network is [`checked`] against the copy; a cost set answers the cost held, a
    /// refused or unchanged one changes nothing, any other ends the optimum; a solve answers the outcome and cost of a
    /// solver's solve of the copy, and copies of the network made just before it hold what it holds and answer what it
    /// answers, the work included; a load is checked as [`load_checked`] says, and the copy becomes the problem loaded.
    /// Answers what the run reached.
    fn check_calls(mut held: MincostNetwork, calls: &[Call]) -> Result<Reached, TestCaseError> {
        let mut network = loaded(&held);
        let mut spare = loaded(&depot());
        let mut reached = Reached::default();
        let mut potentials = checked(&mut network, &held).map_err(TestCaseError::fail)?;
        // The single setter of bounds the call before made, where it changed the bounds: the arc, the setter and the
        // bounds before it.
        let mut next: Option<(usize, Setter, Bounds)> = None;

        for &call in calls {
            // A load may change the counts.
            let (node_count, arc_count) = (held.node_count(), held.arc_count());
            let bound = cost_bound(node_count);
            let last = next.take();
            let roots = root_links_of(&mut network);
            match call {
                Call::SetCost(arc, draw) if arc_count > 0 => {
                    let (arc, cost) = (arc.index(arc_count), draw.cost(bound));
                    let whole = format!("{network:?}");
                    let answered = network.set_cost(arc, cost);
                    if !(-bound..=bound).contains(&cost) {
                        prop_assert_eq!(answered, Err(ChangeError::CostBeyondBound { arc, cost, node_count }));
                        prop_assert_eq!(format!("{network:?}"), whole);
                        reached.refused += 1;
                    } else if cost == held.costs[arc] {
                        prop_assert_eq!(answered, Ok(cost));
                        prop_assert_eq!(format!("{network:?}"), whole);
                        reached.unchanged += 1;
                    } else {
                        prop_assert_eq!(answered, Ok(held.costs[arc]));
                        prop_assert_eq!((network.standing, network.optimum()), (Standing::Changed, None));
                        held.costs[arc] = cost;
                        let shifted = checked(&mut network, &held).map_err(TestCaseError::fail)? != potentials;
                        reached.shifted += usize::from(shifted);
                        reached.unshifted += usize::from(!shifted);
                    }
                }
                Call::SetCost(..) => {}
                Call::MoveSupply(from_node, to_node, draw) => {
                    let (from_node, to_node) = (from_node.index(node_count), to_node.index(node_count));
                    let amount = amount_of(draw, held.supplies[from_node], held.supplies[to_node]);
                    move_checked(&mut network, &mut held, [from_node, to_node], amount, &mut reached)?;
                }
                Call::SetCapacity(arc, capacity) if arc_count > 0 => {
                    let arc = arc.index(arc_count);
                    let earlier = bounds_of(&held, arc);
                    let setter = Setter::Capacity(Bounds { capacity: capacity.bound(), ..earlier });
                    if bounds_checked(&mut network, &mut held, arc, setter, &mut reached)? {
                        let crossed = crossed_by_two(last, arc, setter, bounds_of(&held, arc));
                        reached.crossed_in_two_admitted += usize::from(crossed);
                        next = Some((arc, setter, earlier));
                    }
                }
                Call::SetLowerBound(arc, lower_bound) if arc_count > 0 => {
                    let arc = arc.index(arc_count);
                    let earlier = bounds_of(&held, arc);
                    let setter = Setter::LowerBound(Bounds { lower_bound: lower_bound.bound(), ..earlier });
                    if bounds_checked(&mut network, &mut held, arc, setter, &mut reached)? {
                        let crossed = crossed_by_two(last, arc, setter, bounds_of(&held, arc));
                        reached.crossed_in_two_admitted += usize::from(crossed);
                        next = Some((arc, setter, earlier));
                    }
                }
                Call::SetBounds(arc, lower_bound, capacity) if arc_count > 0 => {
                    let arc = arc.index(arc_count);
                    let bounds = Bounds { lower_bound: lower_bound.bound(), capacity: capacity.bound() };
                    let _ = bounds_checked(&mut network, &mut held, arc, Setter::Both(bounds), &mut reached)?;
                }
                Call::SetCapacity(..) | Call::SetLowerBound(..) | Call::SetBounds(..) => {}
                Call::Load(draw) => load_checked(&mut network, &mut held, draw, &mut reached)?,
                Call::Solve => {
                    // A copy solved pivot by pivot, the marks checked after each, does what the solve does. A copy by
                    // `try_clone`, and one by `try_clone_from` into the network the last solve copied into, which held
                    // another problem at first, hold what the network holds, and solve as it does, the work included.
                    let mut stepped = network.clone();
                    let mut tried = network.try_clone().map_err(|error| TestCaseError::fail(error.to_string()))?;
                    spare.try_clone_from(&network).map_err(|error| TestCaseError::fail(error.to_string()))?;
                    prop_assert_eq!(format!("{tried:?}"), format!("{network:?}"));
                    prop_assert_eq!(format!("{spare:?}"), format!("{network:?}"));
                    let stepped_work = solved_step_by_step(&mut stepped, &held).map_err(TestCaseError::fail)?;
                    let copies = [answered(&mut tried), answered(&mut spare)];
                    let whole = answered(&mut network);
                    let (cost, work) = (whole.0.as_ref().map(|optimum| optimum.total_cost), whole.1);
                    prop_assert_eq!(cost, solved_by_a_solver(&held)?);
                    prop_assert_eq!(work, stepped_work);
                    prop_assert_eq!(&copies, &[whole.clone(), whole]);
                    prop_assert_eq!(format!("{stepped:?}"), format!("{network:?}"));
                    prop_assert_eq!(format!("{tried:?}"), format!("{network:?}"));
                    reached.pivoted += usize::from(work.pivots > 0);
                }
            }

            prop_assert_eq!(read_back(&network), held.clone(), "the reads answer the problem as it stands");
            let now = checked(&mut network, &held).map_err(TestCaseError::fail)?;
            if !matches!(call, Call::Solve | Call::Load(_)) {
                count_moves(&roots, &root_links_of(&mut network), [&potentials, &now], &mut reached);
            }
            potentials = now;
        }

        Ok(reached)
    }

    /// Counts into `reached` what a change did to the nodes, from the way each hung by its root link before and after,
    /// `None` where by an arc, and its potentials before and after: the nodes cut that moved and those that moved not,
    /// whose link left the tree as no potential moved, and the changes that turned two nodes under the root.
    fn count_moves(
        before: &[Option<Direction>],
        after: &[Option<Direction>],
        [was, is]: [&[i64]; 2],
        reached: &mut Reached,
    ) {
        let nodes = before.iter().zip(after).zip(was.iter().zip(is)).map(|((&before, &after), (was, is))| {
            let cut = before.is_none() && after.is_some();
            let turned = before.zip(after).is_some_and(|(before, after)| before != after);
            (cut && was != is, cut && was == is, turned)
        });

        let mut turns = 0;
        for (moved, unmoved, turned) in nodes {
            reached.cut_and_moved += usize::from(moved);
            reached.cut_and_unmoved += usize::from(unmoved);
            turns += usize::from(turned);
        }
        reached.turned_two_tops += usize::from(turns >= 2);
    }

    #[test]
    fn test_the_random_calls_reach_every_kind_of_change() {
        let mut runner = TestRunner::deterministic();
        let networks = || tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered);
        let mut drawn = Vec::new();
        for (strategy, count) in [
            ((networks(), cost_and_supply_calls()).boxed(), 256),
            ((networks(), calls()).boxed(), 512),
            ((networks(), calls_with_loads()).boxed(), 512),
        ] {
            for _ in 0..count {
                drawn.push(strategy.new_tree(&mut runner).expect("a sequence should be drawn").current());
            }
        }

        let mut total = Reached::default();
        for (held, calls) in drawn {
            let reached = check_calls(held, &calls).expect("every call should keep the invariant");
            total.shifted += reached.shifted;
            total.unshifted += reached.unshifted;
            total.unchanged += reached.unchanged;
            total.refused += reached.refused;
            total.pivoted += reached.pivoted;
            total.carried += reached.carried;
            total.raised += reached.raised;
            total.moved_nothing += reached.moved_nothing;
            total.refused_supply += reached.refused_supply;
            total.refused_excess += reached.refused_excess;
            total.set_back += reached.set_back;
            total.turned += reached.turned;
            total.unchanged_bounds += reached.unchanged_bounds;
            total.refused_lower += reached.refused_lower;
            total.refused_bounds_excess += reached.refused_bounds_excess;
            total.unmoved_bounds += reached.unmoved_bounds;
            total.raised_by_bounds += reached.raised_by_bounds;
            total.switched_off += reached.switched_off;
            total.switched_on += reached.switched_on;
            total.at_capacity_raised += reached.at_capacity_raised;
            total.at_capacity_lowered += reached.at_capacity_lowered;
            total.lower_above_flow += reached.lower_above_flow;
            total.lower_below_flow += reached.lower_below_flow;
            total.tree_arc_emptied += reached.tree_arc_emptied;
            total.crossed_at_once += reached.crossed_at_once;
            total.crossed_in_two += reached.crossed_in_two;
            total.crossed_in_two_admitted += reached.crossed_in_two_admitted;
            total.lower_raised_below_flow += reached.lower_raised_below_flow;
            total.lower_raised_to_flow += reached.lower_raised_to_flow;
            total.bounds_set_back += reached.bounds_set_back;
            total.cut_and_moved += reached.cut_and_moved;
            total.cut_and_unmoved += reached.cut_and_unmoved;
            total.turned_two_tops += reached.turned_two_tops;
            total.kept_loads += reached.kept_loads;
            total.kept_tree_arcs += reached.kept_tree_arcs;
            total.kept_cuts += reached.kept_cuts;
            total.loads_of_other_counts += reached.loads_of_other_counts;
            total.loads_of_other_ends += reached.loads_of_other_ends;
            total.refused_loads += reached.refused_loads;
        }

        let counts = [total.shifted, total.unshifted, total.unchanged, total.refused, total.pivoted];
        let moves = [total.carried, total.raised, total.moved_nothing, total.refused_supply, total.refused_excess];
        let ends = [total.set_back, total.turned];
        let bounds = [
            total.unchanged_bounds,
            total.refused_lower,
            total.refused_bounds_excess,
            total.unmoved_bounds,
            total.raised_by_bounds,
            total.switched_off,
            total.switched_on,
            total.at_capacity_raised,
            total.at_capacity_lowered,
            total.lower_above_flow,
            total.lower_below_flow,
            total.tree_arc_emptied,
            total.crossed_at_once,
            total.crossed_in_two,
            total.crossed_in_two_admitted,
            total.lower_raised_below_flow,
            total.lower_raised_to_flow,
            total.bounds_set_back,
        ];
        let marks = [total.cut_and_moved, total.cut_and_unmoved, total.turned_two_tops];
        let loads = [
            total.kept_loads,
            total.kept_tree_arcs,
            total.kept_cuts,
            total.loads_of_other_counts,
            total.loads_of_other_ends,
            total.refused_loads,
        ];
        let reached = counts.iter().chain(&moves).chain(&ends).chain(&bounds).chain(&marks).chain(&loads);
        assert!(reached.copied().all(|count| count > 0), "{total:?}");
    }

    /// Loads the problem `draw` makes of `held` into `network`, which holds `held`, and checks the answer against the
    /// rule: a problem a network never loaded refuses, supplies that do not balance among them, is refused alike and
    /// changes nothing the network reads, its whole `Debug` but where the net supplies were refused; any other problem
    /// is held from then
    /// on, with no optimum. Where it has the counts of `held` and the tail and the head of each arc, the load starts
    /// from the basis held, whose tree it keeps but for arcs that leave it; otherwise from the star, holding the basis
    /// and the sums a network never loaded holds once loaded with it. The caller checks the rest of the invariant and
    /// the sums. Counts what the load was into `reached`.
    fn load_checked(
        network: &mut Network,
        held: &mut MincostNetwork,
        draw: LoadDraw,
        reached: &mut Reached,
    ) -> Result<(), TestCaseError> {
        let next = load_drawn(held, draw);
        let counts = (next.node_count(), next.arc_count()) == (held.node_count(), held.arc_count());
        let ends = next.tails == held.tails && next.heads == held.heads;
        let before = tree_arcs_of(network);

        let whole = format!("{network:?}");
        // What a network never loaded answers for the problem: a perturbation can take a cost set at its bound past it,
        // or a supply moved to an end of `i64` past it, and the net supplies past their limit.
        let refusal = Network::new().load(&problem_of(&next)).err();

        let answered = network.load(&problem_of(&next));
        if let Some(refusal) = refusal {
            prop_assert_eq!(answered, Err(refusal));
            let unbalanced = matches!(refusal, ProblemError::Unbalanced { .. });
            prop_assert!(!unbalanced || matches!(draw, LoadDraw::Unbalanced), "refused {:?}", refusal);
            // A refusal leaves the network as it was: the scratch rooms a refusal at the net supplies rewrites are left
            // out of its `Debug`.
            prop_assert_eq!(format!("{network:?}"), whole);
            reached.refused_loads += 1;
            return Ok(());
        }
        prop_assert!(!matches!(draw, LoadDraw::Unbalanced), "supplies that do not balance should be refused");
        prop_assert_eq!((answered, network.standing, network.optimum()), (Ok(()), Standing::Changed, None));
        *held = next;

        let after = tree_arcs_of(network);
        if counts && ends {
            // The re-flow only takes arcs out of the tree.
            let kept = after.iter().zip(&before).all(|(&now, &was)| !now || was);
            prop_assert!(kept, "the tree arcs {after:?} after the load, {before:?} before");
            reached.kept_loads += 1;
            reached.kept_tree_arcs += usize::from(after.contains(&true));
            reached.kept_cuts += usize::from(after != before);
        } else {
            let fresh = loaded(held);
            prop_assert_eq!(format!("{:?}", network.basis), format!("{:?}", fresh.basis));
            prop_assert_eq!(network.sums, fresh.sums);
            reached.loads_of_other_counts += usize::from(!counts);
            reached.loads_of_other_ends += usize::from(counts);
        }

        Ok(())
    }

    /// The cost a solver's solve of `held` answers, `None` where it finds no flow; where the solver refuses the
    /// problem, its net supplies past the limit though the network's arcs carry them, the brute force's.
    fn solved_by_a_solver(held: &MincostNetwork) -> Result<Option<i128>, TestCaseError> {
        match crate::NetworkSimplex::new().solve(&problem_of(held)) {
            Ok(answer) => Ok(answer.outcome.optimal().map(|solution| solution.total_cost)),
            Err(ProblemError::NetSupplyBeyondRange { .. }) => Ok(brute_force_optimum(held)),
            Err(error) => Err(TestCaseError::fail(error.to_string())),
        }
    }

    /// The cost a solver's solve of `held` answers, as [`solved_by_a_solver`] gives it, for a test by hand.
    ///
    /// # Panics
    /// Panics if neither the solver nor the brute force answers.
    fn solvers_cost(held: &MincostNetwork) -> Option<i128> {
        solved_by_a_solver(held).unwrap_or_else(|error| panic!("the problem held should be solved: {error}"))
    }

    /// Moves `amount` from the supply of `from_node` to that of `to_node` on `network`, which holds `held`, and checks
    /// the answer against the rule: a move of nothing changes nothing; a supply past `i64` is refused, else the flow up
    /// the root links and the amount past the limit, each changing nothing; otherwise `held` moves alike, the optimum
    /// ends, the flow up the root links gains at most the amount, and where it did not rise the move set back on a copy
    /// is admitted and solves to the cost the problem had. Counts what the move was into `reached`.
    fn move_checked(
        network: &mut Network,
        held: &mut MincostNetwork,
        [from_node, to_node]: [usize; 2],
        amount: u64,
        reached: &mut Reached,
    ) -> Result<(), TestCaseError> {
        let (sums, before) = (network.sums, held.clone());
        let tops = root_links_of(network);
        let whole = format!("{network:?}");
        let answered = network.move_supply(from_node, to_node, amount);
        let from_supply = i128::from(held.supplies[from_node]) - i128::from(amount);
        let to_supply = i128::from(held.supplies[to_node]) + i128::from(amount);
        let total = u128::from(sums.excess()) + u128::from(amount);
        let untouched = format!("{network:?}") == whole;

        if from_node == to_node || amount == 0 {
            prop_assert_eq!((answered, untouched), (Ok(()), true));
            reached.moved_nothing += 1;
        } else if i64::try_from(from_supply).is_err() {
            let refused = ChangeError::SupplyBeyondRange { node: from_node, supply: from_supply };
            prop_assert_eq!((answered, untouched), (Err(refused), true));
            reached.refused_supply += 1;
        } else if i64::try_from(to_supply).is_err() {
            let refused = ChangeError::SupplyBeyondRange { node: to_node, supply: to_supply };
            prop_assert_eq!((answered, untouched), (Err(refused), true));
            reached.refused_supply += 1;
        } else if total > u128::from(NET_SUPPLY_LIMIT) {
            prop_assert_eq!((answered, untouched), (Err(ChangeError::ExcessBeyondRange { total }), true));
            reached.refused_excess += 1;
        } else {
            prop_assert_eq!((answered, network.standing, network.optimum()), (Ok(()), Standing::Changed, None));
            held.supplies[from_node] =
                i64::try_from(from_supply).map_err(|error| TestCaseError::fail(error.to_string()))?;
            held.supplies[to_node] =
                i64::try_from(to_supply).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let raised = network.sums.excess() > sums.excess();
            prop_assert!(network.sums.excess() - sums.excess().min(network.sums.excess()) <= amount);
            reached.raised += usize::from(raised);
            let turned = tops
                .iter()
                .zip(root_links_of(network))
                .any(|(&was, now)| was.zip(now).is_some_and(|(was, now)| was != now));
            reached.turned += usize::from(turned);
            reached.carried += usize::from(network.sums.excess() == sums.excess());

            if !raised {
                let mut copy = network.clone();
                prop_assert_eq!(copy.move_supply(to_node, from_node, amount), Ok(()));
                prop_assert_eq!(solved(&mut copy).0, solved_by_a_solver(&before)?);
                reached.set_back += 1;
            }
        }

        Ok(())
    }

    /// The bounds of `arc` in `held`.
    fn bounds_of(held: &MincostNetwork, arc: usize) -> Bounds {
        Bounds { lower_bound: held.lower_bounds[arc], capacity: held.capacities[arc] }
    }

    /// Which setter of bounds a random call makes, with the bounds the arc is to have. Plain data.
    #[derive(Clone, Copy, Debug)]
    enum Setter {
        /// `set_capacity`, the lower bound kept.
        Capacity(Bounds),
        /// `set_lower_bound`, the capacity kept.
        LowerBound(Bounds),
        /// `set_bounds`.
        Both(Bounds),
    }

    /// Makes the change `setter` names on `arc` of `network`, which holds `held`, and checks the answer against the
    /// rule: bounds the arc has change nothing; a lower bound above the capacity is refused, else a cost total past
    /// `i128::MAX`, else, only where the flow up the root links and the larger change of the two bounds pass the limit,
    /// the flow up and the flow moved past it, each changing nothing; otherwise `held` takes the bounds, the optimum
    /// ends, the flow up the root links gains at most the larger change of the two bounds, the supplies stay, and where
    /// that flow and that change add up to at most the limit the bounds answered, set back on a copy, are admitted and
    /// solve to the cost the problem had. Counts what the change was into `reached`.
    fn bounds_checked(
        network: &mut Network,
        held: &mut MincostNetwork,
        arc: usize,
        setter: Setter,
        reached: &mut Reached,
    ) -> Result<bool, TestCaseError> {
        let (standing, sums, before) = (network.standing, network.sums, held.clone());
        let replaced = bounds_of(held, arc);
        let slot = network.order.slot_of(arc).ok_or_else(|| TestCaseError::fail("the arc should have a slot"))?;
        let flow = network.held_arc(slot).ok_or_else(|| TestCaseError::fail("the arc should be held"))?.kept.flow;
        let in_tree = tree_arcs_of(network)[usize::from(slot)];
        let whole = format!("{network:?}");
        let (answered, bounds) = match setter {
            Setter::Capacity(bounds) => (network.set_capacity(arc, bounds.capacity).map(|_| replaced), bounds),
            Setter::LowerBound(bounds) => (network.set_lower_bound(arc, bounds.lower_bound).map(|_| replaced), bounds),
            Setter::Both(bounds) => (network.set_bounds(arc, bounds), bounds),
        };
        let unmoved = (network.standing, network.sums);
        let untouched = format!("{network:?}") == whole;

        let Bounds { lower_bound, capacity } = bounds;
        let term = |cost: i64, capacity: u64| u128::from(cost.unsigned_abs()) * u128::from(capacity);
        let total = sums.cost_total() - term(held.costs[arc], replaced.capacity) + term(held.costs[arc], capacity);
        let largest = lower_bound.abs_diff(replaced.lower_bound).max(capacity.abs_diff(replaced.capacity));
        let admitted = u128::from(sums.excess()) + u128::from(largest) <= u128::from(NET_SUPPLY_LIMIT);

        if bounds == replaced {
            prop_assert_eq!((answered, unmoved, untouched), (Ok(replaced), (standing, sums), true));
            reached.unchanged_bounds += 1;
        } else if lower_bound > capacity {
            let refused = ChangeError::LowerAboveCapacity { arc, lower_bound, capacity };
            prop_assert_eq!((answered, unmoved, untouched), (Err(refused), (standing, sums), true));
            reached.refused_lower += 1;
            reached.crossed_in_two += usize::from(!matches!(setter, Setter::Both(_)));
        } else if total > i128::MAX.cast_unsigned() {
            let refused = Err(ChangeError::CostTotalBeyondRange { arc });
            prop_assert_eq!((answered, unmoved, untouched), (refused, (standing, sums), true));
        } else if let Err(refused) = answered {
            prop_assert!(!admitted, "{refused:?} for a change the limit admits");
            let past =
                matches!(refused, ChangeError::ExcessBeyondRange { total } if total > u128::from(NET_SUPPLY_LIMIT));
            prop_assert!(past, "{:?} should be the flow up the root links refused", refused);
            prop_assert_eq!((unmoved, untouched), ((standing, sums), true));
            reached.refused_bounds_excess += 1;
        } else {
            prop_assert_eq!((answered, network.standing, network.optimum()), (Ok(replaced), Standing::Changed, None));
            (held.lower_bounds[arc], held.capacities[arc]) = (lower_bound, capacity);
            prop_assert_eq!(&network.supplies.raw, &held.supplies);
            let gained = network.sums.excess().saturating_sub(sums.excess());
            prop_assert!(gained <= largest, "the flow up gained {gained}, more than {largest}");

            let width = replaced.capacity - replaced.lower_bound;
            let unmoved_flow = network.sums.kept_cost() == sums.kept_cost() && network.sums.excess() == sums.excess();
            reached.unmoved_bounds += usize::from(unmoved_flow);
            reached.raised_by_bounds += usize::from(gained > 0);
            reached.switched_off += usize::from(capacity == 0 && flow > 0);
            reached.switched_on += usize::from(width == 0 && capacity > lower_bound);
            let full = width > 0 && flow == width;
            reached.at_capacity_raised += usize::from(full && capacity > replaced.capacity);
            reached.at_capacity_lowered += usize::from(full && capacity < replaced.capacity);
            reached.lower_above_flow += usize::from(lower_bound > replaced.lower_bound + flow);
            reached.lower_below_flow += usize::from(lower_bound < replaced.lower_bound && flow > 0);
            let whole_flow = replaced.lower_bound + flow;
            let raised = lower_bound > replaced.lower_bound;
            reached.lower_raised_below_flow += usize::from(raised && lower_bound < whole_flow);
            reached.lower_raised_to_flow += usize::from(raised && lower_bound == whole_flow);
            reached.tree_arc_emptied += usize::from(in_tree && capacity == lower_bound);
            let crossed = lower_bound > replaced.capacity || capacity < replaced.lower_bound;
            reached.crossed_at_once += usize::from(crossed && matches!(setter, Setter::Both(_)));

            if u128::from(network.sums.excess()) + u128::from(largest) <= u128::from(NET_SUPPLY_LIMIT) {
                let mut copy = network.clone();
                prop_assert_eq!(copy.set_bounds(arc, replaced), Ok(bounds));
                prop_assert_eq!(solved(&mut copy).0, solved_by_a_solver(&before)?);
                reached.bounds_set_back += 1;
            }

            return Ok(true);
        }

        Ok(false)
    }

    /// Whether the single setter `setter` of `arc`, which changed its bounds to `now`, ends a crossing in two calls in
    /// the admitted order: the call before it, `last`, was the other single setter on the same arc, which changed it
    /// from `earlier`, and each bound now lies past the other's value then.
    fn crossed_by_two(last: Option<(usize, Setter, Bounds)>, arc: usize, setter: Setter, now: Bounds) -> bool {
        let Some((last_arc, last_setter, earlier)) = last else { return false };
        let other = matches!(
            (last_setter, setter),
            (Setter::Capacity(_), Setter::LowerBound(_)) | (Setter::LowerBound(_), Setter::Capacity(_))
        );

        last_arc == arc && other && (now.lower_bound > earlier.capacity || now.capacity < earlier.lower_bound)
    }

    /// Whether each arc of the basis `network` holds is a link of its tree, slot `s` at `s`.
    ///
    /// # Panics
    /// Panics if the network holds no basis.
    fn tree_arcs_of(network: &mut Network) -> Vec<bool> {
        let basis = network.basis.reopen().expect("the network should hold its basis");
        let in_tree = tree_arcs(&basis);
        basis.keep();

        in_tree
    }

    proptest! {
        #[test]
        fn test_random_costs_and_solves_keep_the_invariant_of_the_network(
            held in tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered),
            calls in calls(),
        ) {
            check_calls(held, &calls)?;
        }

        #[test]
        fn test_random_loads_among_changes_and_solves_keep_the_invariant_of_the_network(
            held in tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered),
            calls in calls_with_loads(),
        ) {
            check_calls(held, &calls)?;
        }
    }
}
