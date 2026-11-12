// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The arcs a run tries before block search, chosen from the problem's structure. Each enters only if it violates its
//! bound when its turn comes, so the choice changes how fast a run ends, never whether it ends optimal or infeasible.

use core::iter;
use core::slice;

use std::collections::TryReserveError;

use itertools::Itertools;
use typed_index_collections::TiSlice;

use crate::problem::{CheckedArc, CheckedArcs, CheckedProblem, NodeId};
use crate::ranges::{narrow, widen};
use crate::room::{grow_total, refill, reserve_total};
use crate::slot_order::{DealtSlotsIter, Slot};
use crate::starting_basis::Start;

/// The arcs a run tries before block search, with the room their choice works in, kept by the solver from one solve
/// to the next.
///
/// With exactly one node of positive net supply and one of negative, the choice is the arcs by which a search backwards
/// from the demand node, over arcs wide enough to carry the whole supply, first reaches each node, rows of entering
/// arcs read in ascending arc order, until it takes the supply node: entered in turn, a path of them routes the supply
/// at once. Otherwise it is each demand node's cheapest entering arc, the lowest arc among equals, demand nodes in
/// ascending order: the arc a demand would most likely be fed by. Nothing is chosen where nothing is supplied.
///
/// The room only grows and holds no answer: every choice rewrites what it reads.
#[derive(Clone, Debug, Default)]
pub(crate) struct FirstPivots {
    /// The slots of the arcs the last choice chose, in the order they are tried.
    chosen: Vec<Slot>,
    /// Per node of the last choice among many terminals: the cheapest arc that enters it, `None` where none does.
    cheapest: Vec<Option<Cheapest>>,
    /// The arcs of the last backward search wide enough for the whole supply, by the node they enter.
    rows: WideRows,
    /// The nodes the last backward search reached, and those it had still to scan.
    search: BackwardSearch,
}

/// The entries each buffer of a [`FirstPivots`] holds room for, so that the choice of a problem of a given size
/// allocates nothing whichever way it goes. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RoomTotals {
    /// The slots chosen: at most one a node.
    chosen: usize,
    /// The cheapest arc of each node.
    cheapest: usize,
    /// The row starts: one a node and one more.
    starts: usize,
    /// The wide arcs: at most every arc.
    wide_arcs: usize,
    /// The marks of the nodes reached: one a node.
    reached: usize,
    /// The nodes stacked: each node at most once.
    stack: usize,
}

/// How the supply of a start is spread, as the choice reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Terminals {
    /// No node has a positive net supply.
    Nothing,
    /// One node supplies and one demands.
    One {
        /// The node of positive net supply.
        supply: NodeId,
        /// The node of negative net supply.
        demand: NodeId,
        /// The supply, in units of flow.
        total: u128,
    },
    /// Some other spread.
    Many,
}

/// The cheapest arc found so far that enters one node, the lowest among equals. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cheapest {
    /// Its cost, in units of cost.
    cost: i64,
    /// Its slot.
    slot: Slot,
}

/// The room of the rows a backward search reads: the arcs wide enough for a supply, grouped by the node they enter. It
/// has no invariant; [`sort`](Self::sort) rewrites both columns and lends them as [`SortedRows`].
#[derive(Clone, Debug, Default)]
struct WideRows {
    /// Per node, and one more: where the node's row starts in `arcs`.
    starts: Vec<usize>,
    /// The wide arcs, row by row.
    arcs: Vec<WideArc>,
}

/// An arc wide enough for a supply, in the row of the node it enters. Plain data.
///
/// Its tail is held at 32 bits beside the 32-bit slot, not as a native `NodeId`: every solver holds room for one per
/// arc, whichever way its choices go, 8 bytes an arc against 16.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WideArc {
    /// Its slot.
    slot: Slot,
    /// The node it leaves, at 32 bits.
    tail: u32,
}

/// The arcs of one problem wide enough for its supply, grouped by the node they enter, as a sort lends them.
///
/// # Invariant
/// Only [`WideRows::sort`] makes one, from the arcs of one checked problem of `n` nodes: `starts` holds `n + 1`
/// offsets, ascending from zero to the length of `arcs`, so that node `v`'s row is `arcs[starts[v]..starts[v + 1]]`;
/// each row lists its arcs in ascending arc order; and every tail in `arcs` is below `n`.
#[derive(Clone, Copy, Debug)]
struct SortedRows<'r> {
    /// Per node, and one more: where the node's row starts in `arcs`; the last is where the last row ends.
    starts: &'r [usize],
    /// The wide arcs, row by row.
    arcs: &'r [WideArc],
}

/// The rows of one sort with their offsets counted and no arc placed yet: the problem and the width they are of
/// travel with the offsets, so the arcs placed are the arcs counted.
///
/// # Invariant
/// Only [`RowOffsets::count`] makes one: `starts` holds `n + 1` entries for the `n` nodes of `problem`, `starts[0]`
/// zero and `starts[v + 1]` where node `v`'s row starts, each row as long as the arcs of `problem` that enter `v` and
/// are in the rows for `total` ([`in_rows`]); `wide_count` is the number of those arcs, all rows together.
#[derive(Debug)]
struct RowOffsets<'r, 'p> {
    /// Per node, and one more: where the node's row starts, one place on.
    starts: &'r mut [usize],
    /// The number of wide arcs.
    wide_count: usize,
    /// The problem the rows are of.
    problem: CheckedProblem<'p>,
    /// What an arc must carry to be wide, in units of flow.
    total: u128,
}

/// The room of a backward search: which nodes it reached, and those it has still to scan. It has no invariant; a
/// [`Walk`] rewrites both before it reads them.
#[derive(Clone, Debug, Default)]
struct BackwardSearch {
    /// Per node: whether the search reached it.
    reached: Vec<bool>,
    /// The nodes reached and not yet scanned, the last reached on top.
    stack: Vec<NodeId>,
}

/// One backward search under way over the rows of one sort, in the room of a [`BackwardSearch`].
///
/// # Invariant
/// Only [`Walk::start`] makes one: `reached` holds one mark per node of `rows`, `n`, and every node on `stack` is below
/// `n`. The walk's methods keep it: they stack only tails of `rows`, each below `n` (`SortedRows`).
#[derive(Debug)]
struct Walk<'s, 'r> {
    /// The rows searched.
    rows: SortedRows<'r>,
    /// Per node of the rows: whether the search reached it.
    reached: &'s mut [bool],
    /// The nodes reached and not yet scanned, the last reached on top.
    stack: &'s mut Vec<NodeId>,
}

/// A node a walk took off its stack, and so below the node count of the walk's rows (`Walk`). Only
/// [`Walk::next_node`] makes one, and only that walk reads a row by it.
#[derive(Debug, PartialEq, Eq)]
struct StackedNode(usize);

impl FirstPivots {
    /// A room that holds nothing yet; it allocates nothing.
    pub(crate) const fn new() -> Self {
        Self {
            chosen: Vec::new(),
            cheapest: Vec::new(),
            rows: WideRows { starts: Vec::new(), arcs: Vec::new() },
            search: BackwardSearch { reached: Vec::new(), stack: Vec::new() },
        }
    }

    /// Makes room for the choice of a problem of up to `node_count` nodes and `arc_count` arcs, whichever way the
    /// choice goes: such a choice allocates nothing afterwards. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    pub(crate) fn try_reserve(&mut self, node_count: usize, arc_count: usize) -> Result<(), TryReserveError> {
        let totals = RoomTotals::of(node_count, arc_count);

        reserve_total(&mut self.chosen, totals.chosen)?;
        reserve_total(&mut self.cheapest, totals.cheapest)?;
        reserve_total(&mut self.rows.starts, totals.starts)?;
        reserve_total(&mut self.rows.arcs, totals.wide_arcs)?;
        reserve_total(&mut self.search.reached, totals.reached)?;
        reserve_total(&mut self.search.stack, totals.stack)
    }

    /// Grows the room to hold the choice of a problem of `node_count` nodes and `arc_count` arcs whichever way it
    /// goes, as [`try_reserve`](Self::try_reserve) makes it, growing as a `Vec` does.
    ///
    /// # Panics
    /// Panics if a buffer's room passes `isize::MAX` bytes, as a growing `Vec` does.
    fn grow(&mut self, node_count: usize, arc_count: usize) {
        let totals = RoomTotals::of(node_count, arc_count);

        grow_total(&mut self.chosen, totals.chosen);
        grow_total(&mut self.cheapest, totals.cheapest);
        grow_total(&mut self.rows.starts, totals.starts);
        grow_total(&mut self.rows.arcs, totals.wide_arcs);
        grow_total(&mut self.search.reached, totals.reached);
        grow_total(&mut self.search.stack, totals.stack);
    }

    /// Chooses the arcs for `start` and keeps their slots, in the order they are tried, for
    /// [`chosen`](Self::chosen): at most one a node. Nothing of an earlier choice is kept but the room, which grows to
    /// hold the choice of this problem whichever way it goes, so that a later choice of a problem no larger allocates
    /// nothing. It costs a pass over the net supplies and, where something is supplied, one or two passes over the
    /// arcs.
    ///
    /// # Panics
    /// On a target whose `usize` has 32 bits it can panic while it grows a buffer, where the buffer's room passes
    /// `isize::MAX` bytes: the rows take `8·m`.
    pub(crate) fn choose(&mut self, start: &Start<'_, '_>) {
        let problem = start.problem();
        self.grow(problem.node_count(), problem.arc_count());
        self.chosen.clear();

        match Terminals::of(start.net_supplies()) {
            Terminals::Nothing => {}
            Terminals::One { supply, demand, total } => self.search_backward(problem, supply, demand, total),
            Terminals::Many => self.cheapest_into_demands(start),
        }
    }

    /// The slots of the arcs the last choice chose, in the order they are tried.
    pub(crate) fn chosen(&self) -> &[Slot] {
        &self.chosen
    }

    /// Chooses the arcs by which a search backwards from `demand`, over the arcs of `problem` wide enough to carry
    /// `total`, first reaches each node, until it takes `supply`.
    fn search_backward(&mut self, problem: CheckedProblem<'_>, supply: NodeId, demand: NodeId, total: u128) {
        let rows = self.rows.sort(problem, total);

        self.search.run(rows, supply, demand, &mut self.chosen);
    }

    /// Chooses, demand nodes in ascending order, the cheapest arc entering each node of negative net supply of
    /// `start`, the lowest arc among equals; a demand node no arc enters gets none.
    fn cheapest_into_demands(&mut self, start: &Start<'_, '_>) {
        self.keep_cheapest(start.problem());

        let demands = start.net_supplies().iter().zip(&self.cheapest).filter(|&(&net_supply, _)| net_supply < 0);
        self.chosen.extend(demands.filter_map(|(_, cheapest)| cheapest.map(|cheapest| cheapest.slot)));
    }

    /// Rewrites `cheapest` as the cheapest arc of `problem` that can feed each of its nodes ([`feeds`]), the lowest arc
    /// among equals; `None` where no such arc enters a node.
    fn keep_cheapest(&mut self, problem: CheckedProblem<'_>) {
        refill(&mut self.cheapest, iter::repeat_n(None, problem.node_count()));

        let cheapest = &mut self.cheapest;
        let mut offer = |(arc, slot): (CheckedArc, Slot)| {
            let head = usize::from(arc.head());
            debug_assert!(head < cheapest.len(), "an arc's head should be a node");
            // SAFETY: the head of a checked arc is below its problem's node count (`CheckedArc`), and the arcs below
            // are those of `problem`, to whose node count `cheapest` was refilled above.
            let kept = unsafe { cheapest.get_unchecked_mut(head) };

            // The arcs come in ascending order and only a cheaper one replaces the one kept, so the lowest of equals
            // stays.
            if kept.is_none_or(|held| arc.cost() < held.cost) {
                *kept = Some(Cheapest { cost: arc.cost(), slot });
            }
        };

        // The cost, the ends and whether an arc has width read alike with lower bounds or without.
        let slots = DealtSlotsIter::new(problem.arc_count(), problem.node_count());
        match problem.arcs() {
            CheckedArcs::WithoutLowerBounds(arcs) => arcs.zip(slots).filter(|(arc, _)| feeds(arc)).for_each(&mut offer),
            CheckedArcs::WithLowerBounds(arcs) => arcs.zip(slots).filter(|(arc, _)| feeds(arc)).for_each(&mut offer),
        }
    }
}

impl RoomTotals {
    /// The totals for a problem of `node_count` nodes and `arc_count` arcs.
    fn of(node_count: usize, arc_count: usize) -> Self {
        Self {
            chosen: node_count,
            cheapest: node_count,
            starts: node_count.saturating_add(1),
            wide_arcs: arc_count,
            reached: node_count,
            stack: node_count,
        }
    }
}

impl Terminals {
    /// The spread of `net_supplies`.
    fn of(net_supplies: &TiSlice<NodeId, i128>) -> Self {
        let suppliers = net_supplies.iter_enumerated().filter(|&(_, &net_supply)| net_supply > 0);
        let demanders = net_supplies.iter_enumerated().filter(|&(_, &net_supply)| net_supply < 0);

        match (suppliers.at_most_one(), demanders.at_most_one()) {
            (Ok(None), _) => Self::Nothing,
            (Ok(Some((supply, &total))), Ok(Some((demand, _)))) => {
                Self::One { supply, demand, total: total.unsigned_abs() }
            }
            _ => Self::Many,
        }
    }
}

impl WideRows {
    /// Rewrites the rows as the arcs of `problem` that can feed a node and are wide enough to carry `total`
    /// ([`in_rows`]), grouped by the node they enter, each row in ascending arc order, and lends them: one counting
    /// sort, a pass over the arcs to count each row and one to place each arc.
    fn sort(&mut self, problem: CheckedProblem<'_>, total: u128) -> SortedRows<'_> {
        RowOffsets::count(&mut self.starts, problem, total).place(&mut self.arcs)
    }
}

impl<'r, 'p> RowOffsets<'r, 'p> {
    /// The offsets of the rows of the arcs of `problem` in the rows for `total` ([`in_rows`]), written to `starts`.
    fn count(starts: &'r mut Vec<usize>, problem: CheckedProblem<'p>, total: u128) -> Self {
        // A checked problem keeps `n + 1` within the index limit.
        refill(starts, iter::repeat_n(0, problem.node_count() + 1));

        // Each row's count first goes to the entry after its own, `starts[v + 1]`.
        let mut count = |arc: CheckedArc| {
            let after = usize::from(arc.head()) + 1;
            debug_assert!(after < starts.len(), "an arc's head should be a node");
            // SAFETY: the head of a checked arc is below its problem's node count `n` (`CheckedArc`), and the arcs
            // below are those of `problem`, to whose `n + 1` the offsets were refilled above.
            unsafe { *starts.get_unchecked_mut(after) += 1 };
        };
        match problem.arcs() {
            CheckedArcs::WithoutLowerBounds(arcs) => arcs.filter(|arc| in_rows(arc, total)).for_each(&mut count),
            CheckedArcs::WithLowerBounds(arcs) => arcs.filter(|arc| in_rows(arc, total)).for_each(&mut count),
        }

        // Each entry then becomes the sum of those before it: `starts[0]` zero, and `starts[v + 1]` the counts of the
        // rows before `v`, where row `v` starts.
        let wide_count = into_offsets(starts);

        Self { starts, wide_count, problem, total }
    }

    /// Places each wide arc at its row's next free entry of `arcs`, rewritten to the wide count, and lends the rows.
    fn place(self, arcs: &'r mut Vec<WideArc>) -> SortedRows<'r> {
        let Self { starts, wide_count, problem, total } = self;
        refill(arcs, iter::repeat_n(WideArc { slot: Slot::from(0_u32), tail: 0 }, wide_count));

        // A row's next free entry is `starts[v + 1]`, which moves on with each arc placed. A row of `c` arcs starting
        // at `s` takes the entries `s..s + c`, all below the wide count; once every arc is placed, `starts[v + 1]` is
        // where row `v` ends, so where row `v + 1` starts, and `starts[0]` is still zero.
        let mut place = |(arc, slot): (CheckedArc, Slot)| {
            let after = usize::from(arc.head()) + 1;
            debug_assert!(after < starts.len(), "an arc's head should be a node");
            // SAFETY: the head of a checked arc is below its problem's node count `n` (`CheckedArc`); the arcs below
            // are those of `problem`, for whose `n` nodes the offsets hold `n + 1` entries (`RowOffsets`).
            let next_free = unsafe { starts.get_unchecked_mut(after) };
            let at = *next_free;
            *next_free += 1;
            // A checked arc's tail is below `n`, and `n + 1` is at most `u32::MAX`: the narrowing cannot fail.
            let wide = WideArc { slot, tail: narrow(usize::from(arc.tail())) };

            debug_assert!(at < arcs.len(), "a row's next free entry should be below the wide count");
            // SAFETY: the arcs placed are the arcs of `problem` the offsets counted, those in the rows for `total`
            // (`RowOffsets`), so a row's next free entry stays within the row, below the wide count, the length the
            // entries were refilled to above.
            unsafe { *arcs.get_unchecked_mut(at) = wide };
        };
        let slots = DealtSlotsIter::new(problem.arc_count(), problem.node_count());
        match problem.arcs() {
            CheckedArcs::WithoutLowerBounds(arcs) => {
                arcs.zip(slots).filter(|(arc, _)| in_rows(arc, total)).for_each(&mut place);
            }
            CheckedArcs::WithLowerBounds(arcs) => {
                arcs.zip(slots).filter(|(arc, _)| in_rows(arc, total)).for_each(&mut place);
            }
        }

        SortedRows { starts, arcs }
    }
}

impl<'r> SortedRows<'r> {
    /// The number of nodes the rows are of, `n`.
    fn node_count(&self) -> usize {
        // `starts` holds `n + 1` offsets (`SortedRows`).
        self.starts.len() - 1
    }

    /// The row of the arcs that enter `node`, a node a walk over these rows took off its stack.
    fn row(&self, node: &StackedNode) -> slice::Iter<'r, WideArc> {
        let (at, after) = (node.0, node.0 + 1);

        debug_assert!(
            at < self.starts.len() && after < self.starts.len(),
            "a stacked node should be below the node count, so its offset and the next within the n + 1 offsets"
        );
        // SAFETY: a stacked node is below the node count `n` of the walk's rows (`StackedNode`), these rows, so `at`
        // and `at + 1` are at most `n`, and the rows hold `n + 1` offsets (`SortedRows`).
        let (start, end) = unsafe { (*self.starts.get_unchecked(at), *self.starts.get_unchecked(after)) };

        debug_assert!(start <= end && end <= self.arcs.len(), "a row should lie within the wide arcs");
        // SAFETY: the offsets ascend from zero to the length of the wide arcs (`SortedRows`).
        unsafe { self.arcs.get_unchecked(start..end) }.iter()
    }
}

impl BackwardSearch {
    /// Searches backwards from `demand` over `rows`, scanning the node reached last first, and pushes to `chosen`, in
    /// the order the search makes them, the arcs by which it first reaches each node, until it takes `supply` or has
    /// scanned every node it reached. Nothing of an earlier search is kept but the room.
    ///
    /// # Panics
    /// Panics if `demand` is not below the node count of `rows`.
    #[track_caller]
    fn run(&mut self, rows: SortedRows<'_>, supply: NodeId, demand: NodeId, chosen: &mut Vec<Slot>) {
        let mut walk = Walk::start(self, rows, demand);

        while let Some(node) = walk.next_node() {
            if node.0 == usize::from(supply) {
                return;
            }

            walk.scan(&node, chosen);
        }
    }
}

impl<'s, 'r> Walk<'s, 'r> {
    /// The walk over `rows` in the room of `search`, nothing reached but `demand`, which is the one node to scan.
    ///
    /// # Panics
    /// Panics if `demand` is not below the node count of `rows`.
    #[track_caller]
    fn start(search: &'s mut BackwardSearch, rows: SortedRows<'r>, demand: NodeId) -> Self {
        let node_count = rows.node_count();
        refill(&mut search.reached, iter::repeat_n(false, node_count));
        search.stack.clear();

        // The one node stacked from outside the rows is checked here.
        let Some(demand_reached) = search.reached.get_mut(usize::from(demand)) else {
            panic!("called `Walk::start` with the demand {demand:?}, which should be below the node count {node_count}")
        };
        *demand_reached = true;
        search.stack.push(demand);

        Self { rows, reached: &mut search.reached, stack: &mut search.stack }
    }

    /// The node reached last and not yet scanned, taken off the stack; `None` once every node reached is scanned.
    fn next_node(&mut self) -> Option<StackedNode> {
        self.stack.pop().map(|node| StackedNode(usize::from(node)))
    }

    /// Scans the row of `node`, which this walk took off its stack: reaches each tail not reached yet, stacks it,
    /// and pushes the arc that reached it to `chosen`.
    fn scan(&mut self, node: &StackedNode, chosen: &mut Vec<Slot>) {
        for wide in self.rows.row(node) {
            let tail = widen(wide.tail);
            debug_assert!(tail < self.reached.len(), "a tail of the rows should be below the node count");
            // SAFETY: every tail of the rows is below their node count `n` (`SortedRows`), and `reached` holds a mark
            // per node of the walk's rows (`Walk`).
            let reached = unsafe { self.reached.get_unchecked_mut(tail) };

            if !*reached {
                *reached = true;
                self.stack.push(NodeId::from(tail));
                chosen.push(wide.slot);
            }
        }
    }
}

/// Whether `arc` can feed the node it enters: it has width and is no loop.
fn feeds(arc: &CheckedArc) -> bool {
    // An arc of no width is never priced, so it never enters: chosen, it would only shadow the arc of its node that
    // could. A loop closes a cycle of no tree link: entering, it moves flow round itself and none into its node.
    // Neither can carry flow toward a node, so neither is a first arc.
    arc.width() > 0 && arc.tail() != arc.head()
}

/// Whether `arc` belongs in the rows of a backward search for a supply of `total`: it can feed a node ([`feeds`]) and
/// is wide enough to carry the whole supply at once ([`carries`]).
fn in_rows(arc: &CheckedArc, total: u128) -> bool {
    feeds(arc) && carries(arc, total)
}

/// Whether `arc` is wide enough to carry `total` units at once: its width is at least `total`.
fn carries(arc: &CheckedArc, total: u128) -> bool {
    u128::from(arc.width()) >= total
}

/// Turns each of `counts` into the sum of the counts before it, and answers the sum of them all.
fn into_offsets(counts: &mut [usize]) -> usize {
    counts.iter_mut().fold(0, |sum, count| {
        let next = sum + *count;
        *count = sum;

        next
    })
}

#[cfg(test)]
mod tests {
    use kahlnws_testing::{MincostNetwork, TinyBounds, TinyCosts, check_against_brute_force, tiny_networks};
    use proptest::prelude::*;
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;

    use super::*;
    use crate::basis::BasisRoom;
    use crate::basis::PricedRows;
    use crate::basis::tests::{CheckedRun, EnteringRule, RunEnd};
    use crate::pricing::BlockSearch;
    use crate::problem::Problem;
    use crate::problem::tests::problem_of;
    use crate::slot_order::tests::{arcs_of_slots, slot};
    use crate::starting_basis::NetSupplies;

    /// The node at `index`.
    fn node(index: usize) -> NodeId {
        NodeId::from(index)
    }

    /// The arcs `room` chooses for `problem`, of the network `network`, in the order they are tried.
    fn chosen_in(room: &mut FirstPivots, network: &MincostNetwork, problem: &Problem<'_>) -> Vec<usize> {
        let mut net_supplies = NetSupplies::new();
        let problem = problem.checked().expect("the test's problem should be admitted");
        let start = net_supplies.gather(problem).expect("the test's net supplies should be in range");
        room.choose(&start);

        let arcs = arcs_of_slots(network.arc_count(), network.node_count());
        room.chosen().iter().map(|&slot| arcs[usize::from(slot)]).collect()
    }

    /// The arcs a fresh room chooses for `network`, its lower bounds given.
    fn chosen(network: &MincostNetwork) -> Vec<usize> {
        chosen_in(&mut FirstPivots::new(), network, &problem_of(network))
    }

    /// Five arcs on four nodes, node 0 supplying 5 to node 3: arc 0 from 0 to 1, arc 1 from 1 to 3, arc 2 from 2 to 3
    /// of capacity 4, arc 3 a loop at 1 of width 4 (lower bound 5, capacity 9), which leaves every net supply as it
    /// is, arc 4 from 2 to 1. Arcs 0, 1 and 4 are wide enough for the supply. The stride is two: the arcs are dealt to
    /// the slots 0, 2, 4, 1, 3.
    fn five_arcs() -> MincostNetwork {
        MincostNetwork::from_arcs(
            &[5, 0, 0, -5],
            &[(0, 1, 0, 9, 1), (1, 3, 0, 5, 1), (2, 3, 0, 4, 1), (1, 1, 5, 9, 0), (2, 1, 0, 6, 3)],
        )
    }

    #[test]
    fn test_terminals_of_every_spread_by_hand() {
        let of = |net_supplies: &[i128]| Terminals::of(TiSlice::from_ref(net_supplies));

        assert_eq!(of(&[]), Terminals::Nothing);
        assert_eq!(of(&[0, 0]), Terminals::Nothing);
        assert_eq!(of(&[0, 4, 0, -4]), Terminals::One { supply: node(1), demand: node(3), total: 4 });
        assert_eq!(of(&[-7, 7]), Terminals::One { supply: node(1), demand: node(0), total: 7 });
        assert_eq!(of(&[2, 2, -4]), Terminals::Many);
        assert_eq!(of(&[4, -2, -2]), Terminals::Many);
        assert_eq!(of(&[3, 0, -1, 1, -3]), Terminals::Many);
    }

    #[test]
    fn test_the_room_totals_hold_one_entry_a_node_and_every_arc_by_hand() {
        let totals = RoomTotals { chosen: 3, cheapest: 3, starts: 4, wide_arcs: 7, reached: 3, stack: 3 };
        assert_eq!(RoomTotals::of(3, 7), totals);
        assert_eq!(RoomTotals::of(usize::MAX, 0).starts, usize::MAX, "the row starts saturate");
    }

    #[test]
    fn test_into_offsets_turns_counts_into_starts_by_hand() {
        let mut counts = [2, 0, 3, 1];
        assert_eq!(into_offsets(&mut counts), 6);
        assert_eq!(counts, [0, 2, 2, 5]);

        assert_eq!(into_offsets(&mut []), 0);
    }

    #[test]
    fn test_an_arc_carries_a_total_exactly_where_its_width_reaches_it_by_hand() {
        // Arc 0 of width 5 (lower bound 2, capacity 7), arc 1 of no width, arc 2 as wide as a `u64` holds.
        let network = MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 2, 7, 0), (0, 1, 4, 4, 0), (0, 1, 0, u64::MAX, 0)]);
        let problem = problem_of(&network).checked().expect("the three arcs should be admitted");
        let CheckedArcs::WithLowerBounds(arcs) = problem.arcs() else { panic!("the lower bounds are given") };
        let arcs: Vec<CheckedArc> = arcs.collect();

        assert!(carries(&arcs[0], 5) && !carries(&arcs[0], 6), "a width equal to the total, and one below it");
        assert!(carries(&arcs[1], 0) && !carries(&arcs[1], 1), "no width carries nothing");
        let widest = u128::from(u64::MAX);
        assert!(carries(&arcs[2], widest) && !carries(&arcs[2], widest + 1), "the widest arc carries its width");
    }

    #[test]
    fn test_the_count_writes_where_each_row_starts_by_hand() {
        // Wide enough for 5: arcs 0 and 4 enter node 1, arc 1 enters node 3. The counts 0, 2, 0, 1 of the rows of
        // nodes 0 to 3 put row 1 at 0 and rows 2 and 3 at 2, each one entry on.
        let network = five_arcs();
        let problem = problem_of(&network).checked().expect("the five arcs should be admitted");
        let mut starts = vec![9; 2];

        let offsets = RowOffsets::count(&mut starts, problem, 5);
        assert_eq!((&*offsets.starts, offsets.wide_count, offsets.total), (&[0, 0, 0, 2, 2][..], 3, 5));
    }

    #[test]
    fn test_keep_cheapest_finds_each_nodes_cheapest_entering_arc_by_hand() {
        // Five arcs on four nodes, dealt with the stride two to the slots 0, 2, 4, 1, 3. Node 1 is entered by arc 2
        // alone; node 2 by arcs 0, 1 and 3, of which 1 and 3 tie at cost 2 and the lower stays; node 3 by arc 4.
        let network = MincostNetwork::from_arcs(
            &[4, 0, -2, -2],
            &[(0, 2, 0, 9, 5), (1, 2, 0, 9, 2), (0, 1, 0, 9, 1), (0, 2, 0, 9, 2), (1, 3, 0, 9, 7)],
        );
        let problem = problem_of(&network).checked().expect("the five arcs should be admitted");
        let mut room = FirstPivots::new();
        room.cheapest = vec![Some(Cheapest { cost: -9, slot: slot(7) }); 6];

        room.keep_cheapest(problem);
        let cheapest = |cost: i64, at: usize| Some(Cheapest { cost, slot: slot(at) });
        assert_eq!(room.cheapest, [None, cheapest(1, 4), cheapest(2, 2), cheapest(7, 3)]);
    }

    #[test]
    fn test_the_sort_groups_the_wide_arcs_by_the_node_they_enter_by_hand() {
        // Wide enough for 5: arcs 0 and 4 enter node 1, from 0 and 2, at the slots 0 and 3; arc 1 enters node 3 from
        // 1 at slot 2. Rows of a room that held more are rewritten.
        let network = five_arcs();
        let problem = problem_of(&network).checked().expect("the five arcs should be admitted");
        let mut rows = WideRows { starts: vec![7; 9], arcs: vec![WideArc { slot: slot(6), tail: 6 }; 9] };

        let sorted = rows.sort(problem, 5);
        assert_eq!(sorted.starts, [0, 0, 2, 2, 3]);
        let wide = |at: usize, tail: u32| WideArc { slot: slot(at), tail };
        assert_eq!(sorted.arcs, [wide(0, 0), wide(3, 2), wide(2, 1)]);
        assert_eq!(sorted.node_count(), 4);

        // Wide enough for 7: arc 0 alone. The loop at node 1, of width 4, is no row's at any width.
        assert_eq!(rows.sort(problem, 7).arcs, [wide(0, 0)]);
        assert_eq!(rows.sort(problem, 1).arcs, [wide(0, 0), wide(3, 2), wide(2, 1), wide(4, 2)]);

        // Arc 0 of width 4 by its lower bound 2, arc 1 of width 5: without the lower bounds both carry 5.
        let pair = MincostNetwork::from_arcs(&[0, 0, 0], &[(0, 1, 2, 6, 1), (2, 1, 0, 5, 1)]);
        let given = problem_of(&pair).checked().expect("the two arcs should be admitted");
        assert_eq!(rows.sort(given, 5).arcs, [wide(1, 2)]);
        let without = Problem { lower_bounds: None, ..problem_of(&pair) };
        let without = without.checked().expect("the two arcs should be admitted");
        assert_eq!(rows.sort(without, 5).arcs, [wide(0, 0), wide(1, 2)]);
    }

    #[test]
    fn test_a_row_is_the_stretch_its_offsets_name_by_hand() {
        // The five arcs' rows for 5, read by every node a walk could stack, against slicing by checked indexing.
        let network = five_arcs();
        let problem = problem_of(&network).checked().expect("the five arcs should be admitted");
        let mut rows = WideRows::default();
        let sorted = rows.sort(problem, 5);

        for node in 0..sorted.node_count() {
            let expected = &sorted.arcs[sorted.starts[node]..sorted.starts[node + 1]];
            assert!(sorted.row(&StackedNode(node)).eq(expected), "the row of node {node}");
        }
        assert_eq!(sorted.row(&StackedNode(1)).len(), 2);
    }

    #[test]
    fn test_a_walk_reaches_each_node_once_and_scans_the_last_reached_first_by_hand() {
        // From node 3 of the five arcs' rows for 5: scanning node 3 reaches node 1 by slot 2; scanning node 1 reaches
        // nodes 0 and 2 by slots 0 and 3; node 2 is then on top.
        let network = five_arcs();
        let problem = problem_of(&network).checked().expect("the five arcs should be admitted");
        let mut rows = WideRows::default();
        let mut search = BackwardSearch { reached: vec![true; 7], stack: vec![node(6)] };
        let mut chosen = Vec::new();

        let mut walk = Walk::start(&mut search, rows.sort(problem, 5), node(3));
        assert_eq!((&*walk.reached, &**walk.stack), (&[false, false, false, true][..], &[node(3)][..]));
        let first = walk.next_node().expect("the demand node is stacked");
        assert_eq!(first, StackedNode(3));
        walk.scan(&first, &mut chosen);
        let second = walk.next_node().expect("node 1 is stacked");
        walk.scan(&second, &mut chosen);
        assert_eq!(chosen, [slot(2), slot(0), slot(3)]);
        assert_eq!(walk.next_node(), Some(StackedNode(2)), "the node reached last comes first");
        walk.scan(&StackedNode(2), &mut chosen);
        assert_eq!(chosen.len(), 3, "node 2's row is empty");
        assert_eq!((walk.next_node(), walk.next_node()), (Some(StackedNode(0)), None));
    }

    #[test]
    fn test_an_arc_feeds_its_head_exactly_where_it_has_width_and_is_no_loop_by_hand() {
        let network =
            MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, 1, 0), (0, 1, 3, 3, 0), (1, 1, 0, 9, 0), (0, 1, 2, 9, 0)]);
        let problem = problem_of(&network).checked().expect("the four arcs should be admitted");
        let CheckedArcs::WithLowerBounds(arcs) = problem.arcs() else { panic!("the lower bounds are given") };
        let arcs: Vec<CheckedArc> = arcs.collect();

        assert_eq!(arcs.iter().map(feeds).collect::<Vec<_>>(), [true, false, false, true]);
        assert_eq!(arcs.iter().map(|arc| in_rows(arc, 7)).collect::<Vec<_>>(), [false, false, false, true]);
        assert_eq!(arcs.iter().map(|arc| in_rows(arc, 1)).collect::<Vec<_>>(), [true, false, false, true]);
    }

    #[test]
    fn test_an_arc_of_no_width_shadows_nothing_and_a_loop_is_never_chosen_by_hand() {
        // Node 2 demands. Arc 0 of no width at cost −9 and the loop arc 1 at cost −5 would be its cheapest; arc 2 is
        // chosen. Node 3 demands and only a loop enters it: nothing is chosen for it.
        let arcs = [(0, 2, 0, 0, -9), (2, 2, 0, 4, -5), (1, 2, 0, 4, 3), (3, 3, 0, 4, -1)];
        assert_eq!(chosen(&MincostNetwork::from_arcs(&[2, 1, -2, -1], &arcs)), [2]);

        // One supplier, one demander: the search over the loop at node 2 finds node 2 reached, never takes the loop.
        let arcs = [(2, 2, 0, 4, -5), (0, 2, 0, 4, 3)];
        assert_eq!(chosen(&MincostNetwork::from_arcs(&[2, 0, -2], &arcs)), [1]);
    }

    #[test]
    fn test_the_backward_search_scans_the_node_reached_last_first_by_hand() {
        // The five arcs' rows for 5. From node 3: arc 1 reaches node 1; node 1's row reaches node 0 by arc 0 and node
        // 2 by arc 4. Node 2, reached last, is scanned first and reaches nothing new; then node 0, the supply, ends the
        // search.
        let network = five_arcs();
        let problem = problem_of(&network).checked().expect("the five arcs should be admitted");
        let mut rows = WideRows::default();
        let mut search = BackwardSearch { reached: vec![true; 9], stack: vec![node(8)] };
        let mut chosen = vec![slot(9)];

        search.run(rows.sort(problem, 5), node(0), node(3), &mut chosen);
        assert_eq!(chosen, [slot(9), slot(2), slot(0), slot(3)], "pushed after what was there");
        assert_eq!(search.reached, [true, true, true, true], "started afresh for four nodes");
        assert_eq!(search.stack, [], "node 0 was the last on the stack");

        // Toward node 2 instead, which nothing reaches before node 1's row is scanned: the search ends after node 2.
        chosen.clear();
        search.run(rows.sort(problem, 5), node(2), node(3), &mut chosen);
        assert_eq!(chosen, [slot(2), slot(0), slot(3)]);
        assert_eq!(search.stack, [node(0)], "node 0 was reached but not scanned");
    }

    #[test]
    fn test_a_backward_search_that_never_takes_the_supply_scans_everything_it_reaches_by_hand() {
        // Node 1 supplies, but no wide arc leaves it: the search from node 3 reaches only node 2.
        let network = MincostNetwork::from_arcs(&[0, 2, 0, -2], &[(2, 3, 0, 2, 1), (1, 0, 0, 1, 1), (0, 3, 0, 9, 1)]);
        let problem = problem_of(&network).checked().expect("the network should be admitted");
        let mut rows = WideRows::default();
        let mut search = BackwardSearch::default();
        let mut chosen = Vec::new();

        search.run(rows.sort(problem, 2), node(1), node(3), &mut chosen);
        assert_eq!(chosen, [slot(0), slot(2)], "arcs 0 and 2 reach nodes 2 and 0");
        assert_eq!(search.reached, [true, false, true, true]);
    }

    #[test]
    #[should_panic(expected = "called `Walk::start` with the demand NodeId 4, which should be below the node count 4")]
    fn test_the_backward_search_refuses_a_demand_outside_the_nodes() {
        let network = five_arcs();
        let problem = problem_of(&network).checked().expect("the five arcs should be admitted");
        let mut rows = WideRows::default();

        BackwardSearch::default().run(rows.sort(problem, 5), node(0), node(4), &mut Vec::new());
    }

    #[test]
    fn test_one_supply_and_one_demand_choose_by_the_backward_search_by_hand() {
        assert_eq!(chosen(&five_arcs()), [1, 0, 4]);

        // Arcs 0 and 1 enter node 3 from nodes 1 and 2; node 2, reached last, is scanned first and reaches node 0, the
        // supply, by arc 3, which ends the search before node 1's row is read.
        let network = MincostNetwork::from_arcs(
            &[5, 0, 0, -5],
            &[(1, 3, 0, 5, 1), (2, 3, 0, 5, 1), (0, 1, 0, 5, 1), (0, 2, 0, 5, 1)],
        );
        assert_eq!(chosen(&network), [0, 1, 3]);
    }

    #[test]
    fn test_many_terminals_choose_each_demand_nodes_cheapest_entering_arc_by_hand() {
        // Node 2 demands; its cheapest entering arcs 1 and 3 tie at 2: the lower, 1. Node 3 demands, and only arc 4
        // enters it.
        let arcs = [(0, 2, 0, 9, 5), (1, 2, 0, 9, 2), (0, 1, 0, 9, 1), (0, 2, 0, 9, 2), (1, 3, 0, 9, 7)];
        assert_eq!(chosen(&MincostNetwork::from_arcs(&[4, 0, -2, -2], &arcs)), [1, 4]);

        // Node 1 demands too, fed by arc 2 alone; node 3 now supplies, and arc 4 no longer counts.
        assert_eq!(chosen(&MincostNetwork::from_arcs(&[3, -1, -3, 1], &arcs)), [2, 1]);

        // A demand no arc enters gets none.
        assert_eq!(chosen(&MincostNetwork::from_arcs(&[3, 0, -2, -1], &arcs[..2])), [1]);

        // A lower bound of 1 on arc 1 makes node 1 a demand, fed by arc 2, and leaves node 2 one unit to take.
        let bounded = [(0, 2, 0, 9, 5), (1, 2, 1, 9, 2), (0, 1, 0, 9, 1), (0, 2, 0, 9, 2), (1, 3, 0, 9, 7)];
        assert_eq!(chosen(&MincostNetwork::from_arcs(&[4, 0, -2, -2], &bounded)), [2, 1, 4]);
    }

    #[test]
    fn test_nothing_supplied_chooses_nothing() {
        assert_eq!(chosen(&MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, 1, -1)])), Vec::<usize>::new());
        assert_eq!(chosen(&MincostNetwork::from_arcs(&[], &[])), Vec::<usize>::new());
    }

    #[test]
    fn test_a_choice_rewrites_a_room_an_earlier_choice_filled() {
        // A backward search, a choice among many terminals, and the backward search again: each as in a fresh room.
        let many = MincostNetwork::from_arcs(&[4, 0, -2, -2], &[(0, 2, 0, 9, 5), (1, 2, 0, 9, 2), (0, 3, 0, 9, 1)]);
        let mut room = FirstPivots::new();

        assert_eq!(chosen_in(&mut room, &five_arcs(), &problem_of(&five_arcs())), chosen(&five_arcs()));
        assert_eq!(chosen_in(&mut room, &many, &problem_of(&many)), chosen(&many));
        assert_eq!(chosen_in(&mut room, &five_arcs(), &problem_of(&five_arcs())), [1, 0, 4]);
    }

    #[test]
    fn test_a_choice_makes_room_for_either_way_a_later_choice_goes() {
        // A choice among many terminals makes room for a backward search of the same size too, and the reverse.
        let many = MincostNetwork::from_arcs(
            &[4, 0, -2, -2],
            &[(0, 2, 0, 9, 5), (1, 2, 0, 9, 2), (0, 3, 0, 9, 1), (2, 3, 0, 9, 1)],
        );
        let capacities = |room: &FirstPivots| {
            [room.chosen.capacity(), room.cheapest.capacity(), room.rows.starts.capacity() - 1]
                .into_iter()
                .chain([room.rows.arcs.capacity(), room.search.reached.capacity(), room.search.stack.capacity()])
                .collect::<Vec<_>>()
        };

        let mut room = FirstPivots::new();
        chosen_in(&mut room, &many, &problem_of(&many));
        assert!(capacities(&room).iter().zip([4, 4, 4, 4, 4, 4]).all(|(&held, least)| held >= least));

        let mut room = FirstPivots::new();
        chosen_in(&mut room, &five_arcs(), &problem_of(&five_arcs()));
        assert!(capacities(&room).iter().zip([4, 4, 4, 5, 4, 4]).all(|(&held, least)| held >= least));
    }

    #[test]
    fn test_try_reserve_makes_room_and_refuses_the_impossible() {
        let mut room = FirstPivots::new();
        room.try_reserve(3, 7).expect("room for three nodes and seven arcs should be granted");

        assert!(room.chosen.capacity() >= 3 && room.cheapest.capacity() >= 3 && room.rows.starts.capacity() >= 4);
        assert!(room.rows.arcs.capacity() >= 7 && room.search.reached.capacity() >= 3);
        assert!(room.search.stack.capacity() >= 3);
        assert!(room.try_reserve(usize::MAX, 0).is_err());
    }

    /// Runs `network` from the star with the arcs the first pivots choose entered first, then under each entering rule
    /// a test has and under block search, each pivot checked and bounded, and checks each end against the brute force.
    fn check_every_rule_after_the_first_pivots(network: &MincostNetwork, seed: u64) -> Result<(), TestCaseError> {
        let first = chosen_slots(network);
        let fail = |rule: &str, breach: String| TestCaseError::fail(format!("{rule}: {breach}"));

        let rules = [
            EnteringRule::FirstEligible,
            EnteringRule::MostViolating,
            EnteringRule::RandomEligible(Xoshiro256PlusPlus::seed_from_u64(seed)),
        ];
        for mut rule in rules {
            let name = format!("{rule:?}");
            let end = run_after(network, &first, |rows| rule.next(rows)).map_err(|breach| fail(&name, breach))?;
            check_against_brute_force(network, end.answer.as_ref())
                .map_err(|breach| fail(&name, breach.to_string()))?;
        }

        let mut search = BlockSearch::new(network.arc_count());
        let end = run_after(network, &first, |rows| search.next_entering(rows))
            .map_err(|breach| fail("block search", breach))?;
        check_against_brute_force(network, end.answer.as_ref())
            .map_err(|breach| fail("block search", breach.to_string()))
    }

    /// The slots the first pivots of a fresh room choose for `network`, its lower bounds given.
    fn chosen_slots(network: &MincostNetwork) -> Vec<Slot> {
        let mut room = FirstPivots::new();
        let mut net_supplies = NetSupplies::new();
        let problem = problem_of(network).checked().expect("a tiny network should be admitted");
        room.choose(&net_supplies.gather(problem).expect("tiny net supplies should be in range"));

        room.chosen().to_vec()
    }

    /// The end of the checked run of `network` with the arcs at `first` entered first, then the arcs `next` chooses.
    fn run_after(
        network: &MincostNetwork,
        first: &[Slot],
        next: impl FnMut(&PricedRows<'_>) -> Option<Slot>,
    ) -> Result<RunEnd, String> {
        let mut room = BasisRoom::new();
        let mut run = CheckedRun::start(&mut room, network, &problem_of(network))?;
        run.enter_first(first)?;

        run.run_to_end(next)
    }

    proptest! {
        #[test]
        fn test_tiny_runs_after_the_first_pivots_keep_the_invariant_and_end_optimal(
            network in tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered),
            seed in any::<u64>(),
        ) {
            check_every_rule_after_the_first_pivots(&network, seed)?;
        }

        #[test]
        fn test_tiny_runs_with_tied_costs_after_the_first_pivots_keep_the_invariant_and_end_optimal(
            network in tiny_networks(TinyCosts::ZeroOrUniform(-9..=9), TinyBounds::Ordered),
            seed in any::<u64>(),
        ) {
            check_every_rule_after_the_first_pivots(&network, seed)?;
        }
    }
}
