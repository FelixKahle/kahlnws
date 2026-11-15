// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The problem as the caller gives it, why a solve may refuse it, and the checked problem that proves it was not.

use core::fmt;
use core::iter::{self, FusedIterator, Zip};
use core::slice;

use derive_more::{From, Into};
use itertools::izip;

use crate::ranges::{INDEX_LIMIT, NET_SUPPLY_LIMIT, cost_bound, cost_total_with, root_cost};

/// One min-cost flow problem, borrowed: arcs `0..tails.len()` between nodes `0..supplies.len()`.
///
/// Arc `a` is position `a` of every per-arc table and node `v` position `v` of the supplies: the index is the id.
/// Self-loops and parallel arcs are allowed. Plain data: it may hold any numbers, and [`ProblemError`] names each way
/// they can lie outside the range a solve admits.
#[derive(Clone, Copy, Debug)]
pub struct Problem<'a> {
    /// The node each arc leaves, one per arc.
    pub tails: &'a [usize],
    /// The node each arc enters, one per arc.
    pub heads: &'a [usize],
    /// The least flow of each arc, in units of flow, one per arc; `None` where every arc's is zero.
    pub lower_bounds: Option<&'a [u64]>,
    /// The most flow of each arc, in units of flow, one per arc.
    pub capacities: &'a [u64],
    /// The cost of a unit of flow on each arc, in units of cost, of either sign, one per arc.
    pub costs: &'a [i64],
    /// The supply of each node, in units of flow: positive where it sends flow, negative where it takes flow.
    pub supplies: &'a [i64],
}

/// One of the per-arc tables of a [`Problem`] whose length is compared with the tails'.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Table {
    /// The heads.
    Heads,
    /// The lower bounds, where the problem gives them.
    LowerBounds,
    /// The capacities.
    Capacities,
    /// The costs.
    Costs,
}

impl fmt::Display for Table {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Heads => "heads",
            Self::LowerBounds => "lower bounds",
            Self::Capacities => "capacities",
            Self::Costs => "costs",
        })
    }
}

/// Why a [`Problem`] is no problem a solve admits. Plain data.
///
/// A problem with several breaches is refused with the first in the order of the variants, where the variants about
/// one arc, from [`TailOutsideNodes`](Self::TailOutsideNodes) to [`CostTotalBeyondRange`](Self::CostTotalBeyondRange),
/// are taken arc by arc: the lowest arc with any of them, and its first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum ProblemError {
    /// A per-arc table holds another number of entries than the tails.
    #[error("the {table} have length {length} where the tails have length {expected}")]
    Length {
        /// The table.
        table: Table,
        /// Its number of entries.
        length: usize,
        /// The number of tails, one per arc.
        expected: usize,
    },
    /// The problem has more nodes or arcs than an index of the solve counts: `node_count + 1` or
    /// `arc_count + node_count` passes `u32::MAX`.
    #[error(
        "the node count {node_count} and the arc count {arc_count} are too large: nodes + 1 and arcs + nodes are \
         each limited to {INDEX_LIMIT}"
    )]
    TooLarge {
        /// The number of nodes, one per supply.
        node_count: usize,
        /// The number of arcs, one per tail.
        arc_count: usize,
    },
    /// An arc leaves a node the problem does not have.
    #[error("arc {arc} leaves node {tail}, not below the node count {node_count}")]
    TailOutsideNodes {
        /// The arc.
        arc: usize,
        /// Its tail.
        tail: usize,
        /// The number of nodes.
        node_count: usize,
    },
    /// An arc enters a node the problem does not have.
    #[error("arc {arc} enters node {head}, not below the node count {node_count}")]
    HeadOutsideNodes {
        /// The arc.
        arc: usize,
        /// Its head.
        head: usize,
        /// The number of nodes.
        node_count: usize,
    },
    /// An arc's lower bound passes its capacity.
    #[error("arc {arc} has lower bound {lower_bound} above its capacity {capacity}")]
    LowerAboveCapacity {
        /// The arc.
        arc: usize,
        /// Its lower bound, in units of flow.
        lower_bound: u64,
        /// Its capacity, in units of flow.
        capacity: u64,
    },
    /// An arc's cost lies beyond plus or minus [`cost_bound`] of the node count.
    #[error(
        "arc {arc} has cost {cost} beyond the bound ±{bound} for the node count {node_count}",
        bound = cost_bound(*node_count)
    )]
    CostBeyondBound {
        /// The arc.
        arc: usize,
        /// Its cost, in units of cost.
        cost: i64,
        /// The number of nodes, which sets the bound.
        node_count: usize,
    },
    /// The magnitudes of the costs times the capacities, added up in arc order, pass `i128::MAX`.
    #[error("the costs times the capacities, in magnitude, sum past {max} at arc {arc}", max = i128::MAX)]
    CostTotalBeyondRange {
        /// The arc at which the sum passes `i128::MAX`.
        arc: usize,
    },
    /// The supplies do not add up to zero.
    #[error("the supplies add up to {total}, not zero")]
    Unbalanced {
        /// Their sum, in units of flow.
        total: i128,
    },
    /// The supplies net of the lower bounds add up past `2^64 − 2` where they are positive. A node's net supply is
    /// its supply, less the lower bounds of the arcs that leave it, plus those of the arcs that enter it.
    #[error("the positive supplies net of the lower bounds add up to {total}, past {NET_SUPPLY_LIMIT}")]
    NetSupplyBeyondRange {
        /// Their sum, in units of flow.
        total: u128,
    },
}

/// A node of a problem, below its node count wherever it comes out of a [`CheckedProblem`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, From, Into, derive_more::Debug)]
#[debug("NodeId {_0}")]
pub(crate) struct NodeId(usize);

/// A problem every check of [`Problem::checked`] passed. Only that function makes one.
///
/// # Invariant
/// With `n` nodes and `m` arcs:
/// - every per-arc table holds one entry per arc;
/// - `n + 1` and `m + n` are at most `u32::MAX`;
/// - every tail and head is below `n`;
/// - every lower bound is at most its capacity, every cost within plus or minus `cost_bound(n)`;
/// - the costs' magnitudes times the capacities add up to at most `i128::MAX`;
/// - the supplies add up to zero;
/// - `largest_cost` is the largest magnitude of a cost.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CheckedProblem<'a> {
    /// The problem.
    problem: Problem<'a>,
    /// The largest magnitude of a cost, `C`, in units of cost; zero without arcs.
    largest_cost: u64,
}

/// One arc of a [`CheckedProblem`].
///
/// # Invariant
/// Only [`ArcsWithoutLowerBoundsIter`] and [`ArcsWithLowerBoundsIter`] make one, from the problem they walk: its tail
/// and head are below that problem's node count, its lower bound is at most its capacity, and its cost lies within
/// plus or minus the problem's cost bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CheckedArc {
    /// The node it leaves.
    tail: NodeId,
    /// The node it enters.
    head: NodeId,
    /// Its least flow, in units of flow.
    lower_bound: u64,
    /// Its most flow, in units of flow.
    capacity: u64,
    /// The cost of a unit of flow on it, in units of cost.
    cost: i64,
}

/// The lower bound of one arc of a [`CheckedProblem`] that gives lower bounds, with the arc's ends.
///
/// # Invariant
/// Only [`ArcLowerBoundsIter`] makes one, from the problem it walks: its tail and head are below that problem's node
/// count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ArcLowerBound {
    /// The node the arc leaves.
    tail: NodeId,
    /// The node the arc enters.
    head: NodeId,
    /// The arc's least flow, in units of flow.
    lower_bound: u64,
}

impl<'a> Problem<'a> {
    /// The problem with the proof that its numbers lie in the range a solve admits, but for its net supplies, which
    /// need a buffer to be gathered in.
    ///
    /// # Errors
    /// Returns the first breach: a [`Length`](ProblemError::Length), else [`TooLarge`](ProblemError::TooLarge), else
    /// the breach of the lowest arc that has one, its first in the order of the variants
    /// ([`TailOutsideNodes`](ProblemError::TailOutsideNodes), [`HeadOutsideNodes`](ProblemError::HeadOutsideNodes),
    /// [`LowerAboveCapacity`](ProblemError::LowerAboveCapacity), [`CostBeyondBound`](ProblemError::CostBeyondBound),
    /// [`CostTotalBeyondRange`](ProblemError::CostTotalBeyondRange)), else [`Unbalanced`](ProblemError::Unbalanced).
    pub(crate) fn checked(&self) -> Result<CheckedProblem<'a>, ProblemError> {
        let (node_count, arc_count) = (self.supplies.len(), self.tails.len());
        if let Some(breach) = length_breach(self).or_else(|| size_breach(node_count, arc_count)) {
            return Err(breach);
        }

        let largest_cost = match self.lower_bounds {
            Some(lower_bounds) => {
                largest_cost_of_arcs_in_range(self.arc_numbers(lower_bounds.iter().copied()), node_count)
            }
            None => largest_cost_of_arcs_in_range(self.arc_numbers(iter::repeat(0)), node_count),
        }?;

        if let Some(breach) = balance_breach(self.supplies) {
            return Err(breach);
        }

        Ok(CheckedProblem { problem: *self, largest_cost })
    }

    /// The numbers of the arcs in ascending order, each as the problem gives it, the lower bounds read from
    /// `lower_bounds`, one per arc or more.
    ///
    /// Generic over the lower bounds, so that a problem without them reads zeros without a test per arc.
    fn arc_numbers(self, lower_bounds: impl Iterator<Item = u64>) -> impl Iterator<Item = ArcNumbers> {
        debug_assert!(length_breach(&self).is_none(), "the per-arc tables should hold one entry per arc");

        // The zip of the four slices tests one index an arc; the lower bounds join it last, as a slice's or as zeros.
        let columns = izip!(self.tails, self.heads, self.capacities, self.costs);

        columns.zip(lower_bounds).map(|((&tail, &head, &capacity, &cost), lower_bound)| ArcNumbers {
            tail,
            head,
            lower_bound,
            capacity,
            cost,
        })
    }
}

/// The first per-arc table of `problem`, in the order heads, lower bounds, capacities, costs, whose length is not the
/// tails'; lower bounds the problem does not give have no length to compare.
fn length_breach(problem: &Problem<'_>) -> Option<ProblemError> {
    let expected = problem.tails.len();
    let lengths = [
        (Table::Heads, Some(problem.heads.len())),
        (Table::LowerBounds, problem.lower_bounds.map(<[u64]>::len)),
        (Table::Capacities, Some(problem.capacities.len())),
        (Table::Costs, Some(problem.costs.len())),
    ];

    lengths.into_iter().find_map(|(table, length)| {
        length.filter(|&length| length != expected).map(|length| ProblemError::Length { table, length, expected })
    })
}

/// The breach of a problem of `node_count` nodes and `arc_count` arcs that has more tree nodes, `node_count + 1`, or
/// more links, `arc_count + node_count`, than [`INDEX_LIMIT`].
fn size_breach(node_count: usize, arc_count: usize) -> Option<ProblemError> {
    // `node_count < INDEX_LIMIT` is settled first, so the difference is formed only where it is at least one.
    let fits = node_count < INDEX_LIMIT && arc_count <= INDEX_LIMIT - node_count;

    (!fits).then_some(ProblemError::TooLarge { node_count, arc_count })
}

/// The largest magnitude of a cost of `arcs`, the arcs of a problem of `node_count` nodes in ascending order, once
/// every arc lies in range; zero without arcs.
///
/// Generic over the arcs, which come with a problem's lower bounds or with zeros.
///
/// # Errors
/// Returns the breach of the lowest arc that has one: its own ([`arc_breach`]) first, then
/// [`ProblemError::CostTotalBeyondRange`] where the magnitudes of the costs times the capacities, added up to it,
/// pass `i128::MAX`.
fn largest_cost_of_arcs_in_range(
    arcs: impl Iterator<Item = ArcNumbers>,
    node_count: usize,
) -> Result<u64, ProblemError> {
    let bound = cost_bound(node_count);
    let mut total = 0_u128;
    let mut largest_cost = 0_u64;

    for (arc, numbers) in arcs.enumerate() {
        if let Some(breach) = arc_breach(arc, numbers, node_count, bound) {
            return Err(breach);
        }

        total =
            cost_total_with(total, numbers.cost, numbers.capacity).ok_or(ProblemError::CostTotalBeyondRange { arc })?;
        largest_cost = largest_cost.max(numbers.cost.unsigned_abs());
    }

    Ok(largest_cost)
}

/// The first breach of the own numbers of `arc` in a problem of `node_count` nodes, whose cost bound is `bound`: its
/// tail, then its head outside the nodes, its lower bound above its capacity, its cost beyond plus or minus `bound`.
fn arc_breach(arc: usize, numbers: ArcNumbers, node_count: usize, bound: i64) -> Option<ProblemError> {
    let ArcNumbers { tail, head, lower_bound, capacity, cost } = numbers;

    if tail >= node_count {
        Some(ProblemError::TailOutsideNodes { arc, tail, node_count })
    } else if head >= node_count {
        Some(ProblemError::HeadOutsideNodes { arc, head, node_count })
    } else if lower_bound > capacity {
        Some(ProblemError::LowerAboveCapacity { arc, lower_bound, capacity })
    } else if !(-bound..=bound).contains(&cost) {
        Some(ProblemError::CostBeyondBound { arc, cost, node_count })
    } else {
        None
    }
}

/// The breach of `supplies` that do not add up to zero.
fn balance_breach(supplies: &[i64]) -> Option<ProblemError> {
    // A slice of `i64` holds fewer than `2^61` of them, each of magnitude at most `2^63`: the sum lies within
    // `±2^124`, inside an `i128`.
    let total: i128 = supplies.iter().map(|&supply| i128::from(supply)).sum();

    (total != 0).then_some(ProblemError::Unbalanced { total })
}

impl<'a> CheckedProblem<'a> {
    /// The number of nodes, `n`.
    pub(crate) fn node_count(&self) -> usize {
        self.problem.supplies.len()
    }

    /// The number of arcs, `m`.
    pub(crate) fn arc_count(&self) -> usize {
        self.problem.tails.len()
    }

    /// The largest magnitude of a cost, `C`, in units of cost; zero without arcs.
    pub(crate) fn largest_cost(&self) -> u64 {
        self.largest_cost
    }

    /// The cost of a root link that points down, in units of cost: `(n − 1)·C + 1` for the `n` nodes and the largest
    /// cost magnitude `C`, at most `2^62`.
    pub(crate) fn root_cost(&self) -> i64 {
        // The costs lie within the cost bound, so the root cost is at most `2^62` and `root_cost` does not panic.
        root_cost(self.node_count(), self.largest_cost())
    }

    /// The supply of each node, in units of flow, one per node; they add up to zero.
    pub(crate) fn supplies(&self) -> &'a [i64] {
        self.problem.supplies
    }

    /// The arcs in ascending order, by one of two walks chosen here, once, by whether the problem gives lower bounds:
    /// so a pass over them tests no option per arc.
    pub(crate) fn arcs(&self) -> CheckedArcs<'a> {
        let Problem { tails, heads, lower_bounds, capacities, costs, .. } = self.problem;
        let columns = tails.iter().zip(heads).zip(capacities).zip(costs);

        match lower_bounds {
            None => CheckedArcs::WithoutLowerBounds(ArcsWithoutLowerBoundsIter { columns }),
            Some(lower_bounds) => {
                CheckedArcs::WithLowerBounds(ArcsWithLowerBoundsIter { columns: columns.zip(lower_bounds.iter()) })
            }
        }
    }

    /// Each arc's lower bound with its ends, in ascending arc order; `None` where the problem gives no lower bounds,
    /// so that every arc's is zero.
    pub(crate) fn lower_bounds(&self) -> Option<ArcLowerBoundsIter<'a>> {
        let Problem { tails, heads, lower_bounds, .. } = self.problem;

        lower_bounds.map(|lower_bounds| ArcLowerBoundsIter { columns: tails.iter().zip(heads).zip(lower_bounds) })
    }
}

impl CheckedArc {
    /// The node it leaves, below the node count.
    pub(crate) fn tail(&self) -> NodeId {
        self.tail
    }

    /// The node it enters, below the node count.
    pub(crate) fn head(&self) -> NodeId {
        self.head
    }

    /// Its least flow, in units of flow; at most its capacity.
    pub(crate) fn lower_bound(&self) -> u64 {
        self.lower_bound
    }

    /// The cost of a unit of flow on it, in units of cost; within the cost bound.
    pub(crate) fn cost(&self) -> i64 {
        self.cost
    }

    /// What the arc can carry above its lower bound, in units of flow: the one place a width is computed.
    pub(crate) fn width(&self) -> u64 {
        // Never below zero: the lower bound is at most the capacity (the invariant).
        self.capacity - self.lower_bound
    }
}

impl ArcLowerBound {
    /// The node the arc leaves, below the node count.
    pub(crate) fn tail(&self) -> NodeId {
        self.tail
    }

    /// The node the arc enters, below the node count.
    pub(crate) fn head(&self) -> NodeId {
        self.head
    }

    /// The arc's least flow, in units of flow.
    pub(crate) fn lower_bound(&self) -> u64 {
        self.lower_bound
    }
}

/// The numbers of one arc as a problem gives them. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArcNumbers {
    /// The node it leaves.
    tail: usize,
    /// The node it enters.
    head: usize,
    /// Its least flow, in units of flow; zero where the problem gives no lower bounds.
    lower_bound: u64,
    /// Its most flow, in units of flow.
    capacity: u64,
    /// The cost of a unit of flow on it, in units of cost.
    cost: i64,
}

/// The four per-arc columns every problem gives, zipped in the order tails, heads, capacities, costs.
type ArcColumnsZip<'a> =
    Zip<Zip<Zip<slice::Iter<'a, usize>, slice::Iter<'a, usize>>, slice::Iter<'a, u64>>, slice::Iter<'a, i64>>;

/// The arcs of a [`CheckedProblem`] in ascending order, walked as the problem gives its lower bounds: a choice made
/// once, so that the walk is a zip of slices with one index test an arc.
#[derive(Clone, Debug)]
pub(crate) enum CheckedArcs<'a> {
    /// The problem gives no lower bounds: every arc's is zero.
    WithoutLowerBounds(ArcsWithoutLowerBoundsIter<'a>),
    /// The problem gives a lower bound per arc.
    WithLowerBounds(ArcsWithLowerBoundsIter<'a>),
}

/// The arcs of a [`CheckedProblem`] that gives no lower bounds, in ascending order, each of lower bound zero.
///
/// # Invariant
/// It walks the arcs of one checked problem, so every arc it makes keeps [`CheckedArc`]'s invariant.
#[derive(Clone, Debug)]
pub(crate) struct ArcsWithoutLowerBoundsIter<'a> {
    /// The tails, heads, capacities and costs still to read.
    columns: ArcColumnsZip<'a>,
}

/// The arcs of a [`CheckedProblem`] that gives lower bounds, in ascending order.
///
/// # Invariant
/// It walks the arcs of one checked problem, so every arc it makes keeps [`CheckedArc`]'s invariant.
#[derive(Clone, Debug)]
pub(crate) struct ArcsWithLowerBoundsIter<'a> {
    /// The tails, heads, capacities and costs still to read, zipped with the lower bounds.
    columns: Zip<ArcColumnsZip<'a>, slice::Iter<'a, u64>>,
}

/// The checked arc of `columns`, an entry read from a checked problem's tails, heads, capacities and costs, with the
/// lower bound `lower_bound`.
fn checked_arc(columns: (((&usize, &usize), &u64), &i64), lower_bound: u64) -> CheckedArc {
    let (((&tail, &head), &capacity), &cost) = columns;

    CheckedArc { tail: NodeId(tail), head: NodeId(head), lower_bound, capacity, cost }
}

impl Iterator for ArcsWithoutLowerBoundsIter<'_> {
    type Item = CheckedArc;

    fn next(&mut self) -> Option<CheckedArc> {
        self.columns.next().map(|columns| checked_arc(columns, 0))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.columns.size_hint()
    }
}

impl DoubleEndedIterator for ArcsWithoutLowerBoundsIter<'_> {
    fn next_back(&mut self) -> Option<CheckedArc> {
        self.columns.next_back().map(|columns| checked_arc(columns, 0))
    }
}

impl ExactSizeIterator for ArcsWithoutLowerBoundsIter<'_> {}

impl FusedIterator for ArcsWithoutLowerBoundsIter<'_> {}

impl Iterator for ArcsWithLowerBoundsIter<'_> {
    type Item = CheckedArc;

    fn next(&mut self) -> Option<CheckedArc> {
        self.columns.next().map(|(columns, &lower_bound)| checked_arc(columns, lower_bound))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.columns.size_hint()
    }
}

impl DoubleEndedIterator for ArcsWithLowerBoundsIter<'_> {
    fn next_back(&mut self) -> Option<CheckedArc> {
        self.columns.next_back().map(|(columns, &lower_bound)| checked_arc(columns, lower_bound))
    }
}

impl ExactSizeIterator for ArcsWithLowerBoundsIter<'_> {}

impl FusedIterator for ArcsWithLowerBoundsIter<'_> {}

/// Each arc's lower bound with its ends, of a [`CheckedProblem`] that gives lower bounds, in ascending arc order.
///
/// # Invariant
/// It walks the arcs of one checked problem, so every entry it makes keeps [`ArcLowerBound`]'s invariant.
#[derive(Clone, Debug)]
pub(crate) struct ArcLowerBoundsIter<'a> {
    /// The tails, heads and lower bounds still to read.
    columns: Zip<Zip<slice::Iter<'a, usize>, slice::Iter<'a, usize>>, slice::Iter<'a, u64>>,
}

impl ArcLowerBoundsIter<'_> {
    /// The entry of `columns`, an entry this iterator read.
    fn entry(columns: ((&usize, &usize), &u64)) -> ArcLowerBound {
        let ((&tail, &head), &lower_bound) = columns;

        ArcLowerBound { tail: NodeId(tail), head: NodeId(head), lower_bound }
    }
}

impl Iterator for ArcLowerBoundsIter<'_> {
    type Item = ArcLowerBound;

    fn next(&mut self) -> Option<ArcLowerBound> {
        self.columns.next().map(Self::entry)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.columns.size_hint()
    }
}

impl DoubleEndedIterator for ArcLowerBoundsIter<'_> {
    fn next_back(&mut self) -> Option<ArcLowerBound> {
        self.columns.next_back().map(Self::entry)
    }
}

impl ExactSizeIterator for ArcLowerBoundsIter<'_> {}

impl FusedIterator for ArcLowerBoundsIter<'_> {}

#[cfg(test)]
pub(crate) mod tests {
    use kahlnws_testing::{MincostNetwork, TinyBounds, TinyCosts, TinyMincostNetworks, TinySupplies};
    use num_bigint::BigInt;
    use proptest::prelude::*;

    use super::*;

    /// A problem with every per-arc table empty and the supplies `supplies`.
    fn nodes_only(supplies: &[i64]) -> Problem<'_> {
        Problem { tails: &[], heads: &[], lower_bounds: None, capacities: &[], costs: &[], supplies }
    }

    /// The problem of `network`, its lower bounds given.
    pub(crate) fn problem_of(network: &MincostNetwork) -> Problem<'_> {
        Problem {
            tails: &network.tails,
            heads: &network.heads,
            lower_bounds: Some(&network.lower_bounds),
            capacities: &network.capacities,
            costs: &network.costs,
            supplies: &network.supplies,
        }
    }

    /// Three nodes of supply 2, 0 and -2; arcs 0 -> 1 (bounds 1..=4, cost 3), 1 -> 2 (0..=2, cost -5) and a loop at 2
    /// (2..=2, cost 0).
    fn three_nodes() -> MincostNetwork {
        let mut network = MincostNetwork::default();
        let nodes = [network.add_node(2), network.add_node(0), network.add_node(-2)];
        network.add_arc(nodes[0], nodes[1], 1, 4, 3);
        network.add_arc(nodes[1], nodes[2], 0, 2, -5);
        network.add_arc(nodes[2], nodes[2], 2, 2, 0);

        network
    }

    /// The numbers of an arc from `tail` to `head` with bounds `lower_bound..=capacity` and cost `cost`.
    fn numbers(tail: usize, head: usize, lower_bound: u64, capacity: u64, cost: i64) -> ArcNumbers {
        ArcNumbers { tail, head, lower_bound, capacity, cost }
    }

    #[test]
    fn test_length_breach_names_the_first_table_of_another_length_by_hand() {
        let network = three_nodes();
        assert_eq!(length_breach(&problem_of(&network)), None);
        assert_eq!(length_breach(&Problem { lower_bounds: None, ..problem_of(&network) }), None);

        let breach = length_breach(&Problem { lower_bounds: Some(&[1]), costs: &[], ..problem_of(&network) });
        assert_eq!(breach, Some(ProblemError::Length { table: Table::LowerBounds, length: 1, expected: 3 }));

        let breach = length_breach(&Problem { costs: &[1, 2, 3, 4], ..problem_of(&network) });
        assert_eq!(breach, Some(ProblemError::Length { table: Table::Costs, length: 4, expected: 3 }));

        let breach = length_breach(&Problem { heads: &[], capacities: &[], ..problem_of(&network) });
        assert_eq!(breach, Some(ProblemError::Length { table: Table::Heads, length: 0, expected: 3 }));

        let breach = length_breach(&Problem { capacities: &[0, 0], ..problem_of(&network) });
        assert_eq!(breach, Some(ProblemError::Length { table: Table::Capacities, length: 2, expected: 3 }));

        let breach = length_breach(&Problem { tails: &[0], ..problem_of(&network) });
        assert_eq!(breach, Some(ProblemError::Length { table: Table::Heads, length: 3, expected: 1 }));
    }

    #[test]
    fn test_size_breach_at_the_index_limit_against_big_integers() {
        // The limit is `2^32 − 1` tree nodes and links: `n + 1` and `m + n` at most that, recomputed apart.
        let limit: BigInt = (BigInt::from(1_u8) << 32) - 1;
        let count = |number: BigInt| usize::try_from(number).expect("a count near 2^32 should fit a usize");
        let most_nodes = count(&limit - 1);

        assert_eq!(size_breach(most_nodes, 1), None, "n + 1 and m + n at the limit");
        assert_eq!(size_breach(0, count(limit.clone())), None, "m at the limit");
        assert_eq!(
            size_breach(most_nodes + 1, 0),
            Some(ProblemError::TooLarge { node_count: most_nodes + 1, arc_count: 0 })
        );
        assert_eq!(size_breach(most_nodes, 2), Some(ProblemError::TooLarge { node_count: most_nodes, arc_count: 2 }));
        assert_eq!(
            size_breach(5, count(&limit - 4)),
            Some(ProblemError::TooLarge { node_count: 5, arc_count: count(&limit - 4) })
        );
        assert_eq!(size_breach(5, count(&limit - 5)), None);
        assert_eq!(
            size_breach(usize::MAX, usize::MAX),
            Some(ProblemError::TooLarge { node_count: usize::MAX, arc_count: usize::MAX })
        );
        assert_eq!(size_breach(0, 0), None);
    }

    #[test]
    fn test_arc_breach_takes_the_ends_then_the_bounds_then_the_cost_by_hand() {
        let bound = cost_bound(3);
        assert_eq!(arc_breach(4, numbers(0, 2, 1, 1, bound), 3, bound), None);
        assert_eq!(arc_breach(4, numbers(2, 2, 0, 0, -bound), 3, bound), None, "at both limits");

        // Every breach at its edge: an end at the node count, a lower bound one above its capacity, a cost one past.
        let every = numbers(3, 3, 2, 1, bound + 1);
        assert_eq!(
            arc_breach(4, every, 3, bound),
            Some(ProblemError::TailOutsideNodes { arc: 4, tail: 3, node_count: 3 })
        );

        let head_on = ArcNumbers { tail: 0, ..every };
        assert_eq!(
            arc_breach(4, head_on, 3, bound),
            Some(ProblemError::HeadOutsideNodes { arc: 4, head: 3, node_count: 3 })
        );

        let bounds_on = ArcNumbers { head: 1, ..head_on };
        assert_eq!(
            arc_breach(4, bounds_on, 3, bound),
            Some(ProblemError::LowerAboveCapacity { arc: 4, lower_bound: 2, capacity: 1 })
        );

        let cost_on = ArcNumbers { capacity: 2, ..bounds_on };
        assert_eq!(
            arc_breach(4, cost_on, 3, bound),
            Some(ProblemError::CostBeyondBound { arc: 4, cost: bound + 1, node_count: 3 })
        );

        for cost in [-bound - 1, i64::MIN, i64::MAX] {
            let breach = arc_breach(0, numbers(0, 0, 0, 0, cost), 3, bound);
            assert_eq!(breach, Some(ProblemError::CostBeyondBound { arc: 0, cost, node_count: 3 }));
        }
    }

    #[test]
    fn test_the_cost_bound_at_its_edge_against_big_integers() {
        // At four nodes, `4 · bound ≤ 2^62 − 1 < 4 · (bound + 1)`, recomputed apart from `cost_bound`.
        let bound = i64::try_from((BigInt::from(1_u8) << 62) / 4 - 1).expect("the bound should fit an i64");
        let mut network = MincostNetwork::default();
        for _ in 0..4 {
            network.add_node(0);
        }
        network.add_arc(3, 0, 0, 1, 0);

        for cost in [bound, -bound] {
            network.costs[0] = cost;
            let checked = problem_of(&network).checked().map(|checked| checked.largest_cost());
            assert_eq!(checked, Ok(bound.unsigned_abs()), "cost {cost}");
        }

        for cost in [bound + 1, -bound - 1] {
            network.costs[0] = cost;
            let breach = problem_of(&network).checked().map(|checked| checked.largest_cost());
            assert_eq!(breach, Err(ProblemError::CostBeyondBound { arc: 0, cost, node_count: 4 }));
        }
    }

    #[test]
    fn test_balance_breach_by_hand_and_at_the_extremes() {
        assert_eq!(balance_breach(&[]), None);
        assert_eq!(balance_breach(&[3, -1, -2]), None);
        assert_eq!(balance_breach(&[3, -1]), Some(ProblemError::Unbalanced { total: 2 }));
        assert_eq!(balance_breach(&[i64::MIN, i64::MAX]), Some(ProblemError::Unbalanced { total: -1 }));
        assert_eq!(balance_breach(&[i64::MIN, i64::MAX, 1]), None);

        let supplies = [i64::MIN; 9];
        let total = i128::try_from(BigInt::from(i64::MIN) * 9).expect("nine supplies should sum inside an i128");
        assert_eq!(balance_breach(&supplies), Some(ProblemError::Unbalanced { total }));
    }

    #[test]
    fn test_the_cost_total_is_refused_exactly_past_i128_max() {
        // Arcs of cost `±cost_bound(2)` as loops at node 0 of capacity `u64::MAX`, as many as fit under `i128::MAX`,
        // then one whose capacity brings the total to the edge, recomputed in big integers; then one a unit wider.
        let cost = cost_bound(2);
        let (max, unit) = (BigInt::from(i128::MAX), BigInt::from(cost));
        let full = BigInt::from(u64::MAX) * &unit;
        let whole = &max / &full;
        let count = usize::try_from(&whole).expect("a handful of full arcs should fit a usize");
        let rest = u64::try_from((&max - &whole * &full) / &unit).expect("the rest should lie below a full arc");

        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_node(0);
        for at in 0..count {
            network.add_arc(0, 0, 0, u64::MAX, if at % 2 == 0 { cost } else { -cost });
        }
        let last = network.add_arc(1, 1, 0, rest, cost);

        let checked = problem_of(&network).checked().expect("the total at the edge should be admitted");
        assert_eq!(checked.largest_cost(), cost.unsigned_abs());

        network.capacities[last] = rest + 1;
        let breach = problem_of(&network).checked().map(|checked| checked.largest_cost());
        assert_eq!(breach, Err(ProblemError::CostTotalBeyondRange { arc: last }));
    }

    #[test]
    fn test_checked_answers_the_first_breach_in_the_order_of_the_variants() {
        let mut network = three_nodes();
        network.tails[2] = 7;
        network.costs[0] = i64::MIN;
        network.supplies[0] = 9;

        let first_arc = problem_of(&network).checked().map(|checked| checked.node_count());
        assert_eq!(
            first_arc,
            Err(ProblemError::CostBeyondBound { arc: 0, cost: i64::MIN, node_count: 3 }),
            "arc 0 first"
        );

        network.lower_bounds.pop();
        let lengths = problem_of(&network).checked().map(|checked| checked.node_count());
        assert_eq!(lengths, Err(ProblemError::Length { table: Table::LowerBounds, length: 2, expected: 3 }));

        let without = Problem { lower_bounds: None, ..problem_of(&network) };
        assert_eq!(
            without.checked().map(|checked| checked.node_count()),
            Err(ProblemError::CostBeyondBound { arc: 0, cost: i64::MIN, node_count: 3 }),
            "lower bounds not given have no length"
        );

        network.costs[0] = 3;
        let tail = Problem { lower_bounds: None, ..problem_of(&network) }.checked().map(|checked| checked.node_count());
        assert_eq!(
            tail,
            Err(ProblemError::TailOutsideNodes { arc: 2, tail: 7, node_count: 3 }),
            "the arcs before the balance"
        );

        network.tails[2] = 2;
        let balance =
            Problem { lower_bounds: None, ..problem_of(&network) }.checked().map(|checked| checked.node_count());
        assert_eq!(balance, Err(ProblemError::Unbalanced { total: 7 }));
    }

    #[test]
    fn test_checked_takes_the_cost_total_after_the_arcs_own_numbers_arc_by_arc() {
        // Three loops of the largest cost one node admits, each at capacity `u64::MAX`: two stay below `i128::MAX`,
        // the third passes it, and arc 3 has a tail outside the nodes.
        let cost = cost_bound(1);
        let (one, max) = (BigInt::from(cost) * BigInt::from(u64::MAX), BigInt::from(i128::MAX));
        assert!(&one * 2_u8 <= max && &one * 3_u8 > max, "the third loop should pass the total");

        let mut network = MincostNetwork::default();
        network.add_node(0);
        for _ in 0..3 {
            network.add_arc(0, 0, 0, u64::MAX, cost);
        }
        network.add_arc(5, 0, 0, 1, 1);

        let checked = problem_of(&network).checked().map(|checked| checked.node_count());
        assert_eq!(checked, Err(ProblemError::CostTotalBeyondRange { arc: 2 }), "the lower arc first");

        network.tails[2] = 7;
        let checked = problem_of(&network).checked().map(|checked| checked.node_count());
        let breach = ProblemError::TailOutsideNodes { arc: 2, tail: 7, node_count: 1 };
        assert_eq!(checked, Err(breach), "an arc's own numbers before the total at it");
    }

    #[test]
    fn test_checked_admits_a_problem_and_reads_it_back_by_hand() {
        let network = three_nodes();
        let checked = problem_of(&network).checked().expect("the problem should be admitted");

        assert_eq!((checked.node_count(), checked.arc_count(), checked.largest_cost()), (3, 3, 5));
        assert_eq!(checked.supplies(), [2, 0, -2]);

        let without = Problem { lower_bounds: None, ..problem_of(&network) };
        let without = without.checked().expect("the problem without lower bounds should be admitted too");
        assert!(without.lower_bounds().is_none());
        let CheckedArcs::WithoutLowerBounds(arcs) = without.arcs() else { panic!("no lower bounds are given") };
        assert_eq!(arcs.map(|arc| arc.lower_bound()).collect::<Vec<_>>(), [0, 0, 0]);

        let empty = nodes_only(&[]).checked().expect("the empty problem should be admitted");
        assert_eq!((empty.node_count(), empty.arc_count(), empty.largest_cost()), (0, 0, 0));
        let CheckedArcs::WithoutLowerBounds(arcs) = empty.arcs() else { panic!("no lower bounds are given") };
        assert_eq!(arcs.len(), 0);

        let one_node = nodes_only(&[0]).checked().expect("one node of no supply should be admitted");
        assert_eq!((one_node.node_count(), one_node.largest_cost()), (1, 0));
    }

    #[test]
    fn test_the_root_cost_of_a_checked_problem_by_hand() {
        // Three nodes and a largest cost of 5: `(3 − 1) · 5 + 1`.
        let network = three_nodes();
        assert_eq!(problem_of(&network).checked().map(|checked| checked.root_cost()), Ok(11));

        // Without a path of an arc, or without a node, a root link that points down costs 1.
        assert_eq!(nodes_only(&[0]).checked().map(|checked| checked.root_cost()), Ok(1));
        assert_eq!(nodes_only(&[]).checked().map(|checked| checked.root_cost()), Ok(1));
    }

    #[test]
    fn test_checked_arcs_reads_every_column_from_either_end_by_hand() {
        let network = three_nodes();
        let checked = problem_of(&network).checked().expect("the problem should be admitted");

        let first = CheckedArc { tail: NodeId(0), head: NodeId(1), lower_bound: 1, capacity: 4, cost: 3 };
        let middle = CheckedArc { tail: NodeId(1), head: NodeId(2), lower_bound: 0, capacity: 2, cost: -5 };
        let last = CheckedArc { tail: NodeId(2), head: NodeId(2), lower_bound: 2, capacity: 2, cost: 0 };
        let CheckedArcs::WithLowerBounds(arcs) = checked.arcs() else { panic!("the lower bounds are given") };
        assert_eq!(arcs.clone().collect::<Vec<_>>(), [first, middle, last]);

        let mut both_ends = arcs;
        assert_eq!(both_ends.len(), 3);
        assert_eq!(both_ends.next_back(), Some(last));
        assert_eq!(both_ends.next(), Some(first));
        assert_eq!(both_ends.len(), 1);
        assert_eq!(both_ends.next_back(), Some(middle));
        assert_eq!((both_ends.next(), both_ends.next_back(), both_ends.len()), (None, None, 0));

        let without = Problem { lower_bounds: None, ..problem_of(&network) };
        let without = without.checked().expect("the problem without lower bounds should be admitted too");
        let CheckedArcs::WithoutLowerBounds(mut arcs) = without.arcs() else { panic!("no lower bounds are given") };
        assert_eq!(arcs.len(), 3);
        assert_eq!(arcs.next_back(), Some(CheckedArc { lower_bound: 0, ..last }));
        assert_eq!(arcs.next(), Some(CheckedArc { lower_bound: 0, ..first }));
        assert_eq!(arcs.next_back(), Some(CheckedArc { lower_bound: 0, ..middle }));
        assert_eq!((arcs.next(), arcs.next_back(), arcs.len()), (None, None, 0));
    }

    #[test]
    fn test_a_checked_arcs_accessors_and_width_by_hand() {
        let arc = CheckedArc { tail: NodeId(3), head: NodeId(1), lower_bound: 2, capacity: 7, cost: -4 };

        assert_eq!((arc.tail(), arc.head()), (NodeId::from(3), NodeId::from(1)));
        assert_eq!((arc.lower_bound(), arc.cost()), (2, -4));
        assert_eq!(arc.width(), 5);
        assert_eq!(CheckedArc { lower_bound: 7, ..arc }.width(), 0);
        assert_eq!(CheckedArc { lower_bound: 0, capacity: u64::MAX, ..arc }.width(), u64::MAX);
    }

    #[test]
    fn test_arc_numbers_read_each_arc_with_the_lower_bounds_given_or_zeros_by_hand() {
        let network = three_nodes();
        let given = [numbers(0, 1, 1, 4, 3), numbers(1, 2, 0, 2, -5), numbers(2, 2, 2, 2, 0)];
        let read = problem_of(&network).arc_numbers(network.lower_bounds.iter().copied()).collect::<Vec<_>>();
        assert_eq!(read, given);

        let zeros = given.map(|arc| ArcNumbers { lower_bound: 0, ..arc });
        let read = problem_of(&network).arc_numbers(iter::repeat(0)).collect::<Vec<_>>();
        assert_eq!(read, zeros);

        assert_eq!(nodes_only(&[1]).arc_numbers(iter::repeat(0)).count(), 0);
    }

    #[test]
    fn test_largest_cost_of_arcs_in_range_by_hand() {
        let bound = cost_bound(3);
        assert_eq!(largest_cost_of_arcs_in_range(iter::empty(), 3), Ok(0));

        let arcs = [numbers(0, 1, 0, 4, 3), numbers(1, 2, 1, 2, -7), numbers(2, 0, 0, 0, 5)];
        assert_eq!(largest_cost_of_arcs_in_range(arcs.into_iter(), 3), Ok(7));

        let at_bound = [numbers(0, 1, 0, 1, -bound), numbers(1, 1, 0, 1, bound)];
        assert_eq!(largest_cost_of_arcs_in_range(at_bound.into_iter(), 3), Ok(bound.unsigned_abs()));

        // Arc 1's own breach is reported, arc 2's never read.
        let breaches = [numbers(0, 1, 0, 4, 3), numbers(1, 2, 3, 2, 1), numbers(5, 0, 0, 0, 0)];
        let breach = ProblemError::LowerAboveCapacity { arc: 1, lower_bound: 3, capacity: 2 };
        assert_eq!(largest_cost_of_arcs_in_range(breaches.into_iter(), 3), Err(breach));

        // Loops at the bound of one node and capacity `u64::MAX`, each nearly `2^126`: the third passes `i128::MAX`.
        let full = [numbers(0, 0, 0, u64::MAX, cost_bound(1)); 4];
        let breach = ProblemError::CostTotalBeyondRange { arc: 2 };
        assert_eq!(largest_cost_of_arcs_in_range(full.into_iter(), 1), Err(breach));
    }

    #[test]
    fn test_lower_bounds_read_each_arc_with_its_ends_from_either_end_by_hand() {
        let network = three_nodes();
        let checked = problem_of(&network).checked().expect("the problem should be admitted");
        let entry = |tail, head, lower_bound| ArcLowerBound { tail: NodeId(tail), head: NodeId(head), lower_bound };

        let mut entries = checked.lower_bounds().expect("the problem should give lower bounds");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries.next_back(), Some(entry(2, 2, 2)));
        assert_eq!(entries.next(), Some(entry(0, 1, 1)));
        assert_eq!(entries.next(), Some(entry(1, 2, 0)));
        assert_eq!((entries.next(), entries.next_back(), entries.len()), (None, None, 0));

        let read = entry(4, 3, 9);
        assert_eq!((read.tail(), read.head(), read.lower_bound()), (NodeId::from(4), NodeId::from(3), 9));
    }

    #[test]
    fn test_node_id_converts_and_names_itself() {
        assert_eq!(usize::from(NodeId::from(4)), 4);
        assert_eq!(format!("{:?}", NodeId::from(4)), "NodeId 4");
    }

    #[test]
    fn test_every_error_reads_its_values() {
        let messages = [
            (
                ProblemError::Length { table: Table::Capacities, length: 2, expected: 3 },
                "the capacities have length 2 where the tails have length 3".to_owned(),
            ),
            (
                ProblemError::TooLarge { node_count: 7, arc_count: 4_294_967_290 },
                "the node count 7 and the arc count 4294967290 are too large: nodes + 1 and arcs + nodes are each \
                 limited to 4294967295"
                    .to_owned(),
            ),
            (
                ProblemError::TailOutsideNodes { arc: 1, tail: 5, node_count: 3 },
                "arc 1 leaves node 5, not below the node count 3".to_owned(),
            ),
            (
                ProblemError::HeadOutsideNodes { arc: 2, head: 3, node_count: 3 },
                "arc 2 enters node 3, not below the node count 3".to_owned(),
            ),
            (
                ProblemError::LowerAboveCapacity { arc: 0, lower_bound: 5, capacity: 4 },
                "arc 0 has lower bound 5 above its capacity 4".to_owned(),
            ),
            (
                ProblemError::CostBeyondBound { arc: 1, cost: -1_152_921_504_606_846_976, node_count: 4 },
                "arc 1 has cost -1152921504606846976 beyond the bound ±1152921504606846975 for the node count 4"
                    .to_owned(),
            ),
            (
                ProblemError::CostTotalBeyondRange { arc: 2 },
                "the costs times the capacities, in magnitude, sum past 170141183460469231731687303715884105727 at \
                 arc 2"
                    .to_owned(),
            ),
            (ProblemError::Unbalanced { total: -3 }, "the supplies add up to -3, not zero".to_owned()),
            (
                ProblemError::NetSupplyBeyondRange { total: 18_446_744_073_709_551_615 },
                "the positive supplies net of the lower bounds add up to 18446744073709551615, past \
                 18446744073709551614"
                    .to_owned(),
            ),
        ];

        for (error, message) in messages {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn test_table_names_its_column() {
        let names = [Table::Heads, Table::LowerBounds, Table::Capacities, Table::Costs].map(|table| table.to_string());

        assert_eq!(names, ["heads", "lower bounds", "capacities", "costs"]);
    }

    /// Tiny networks whose lower bounds pass their capacities where `bounds` lets them, and whose supplies may not be
    /// feasible.
    fn tiny(bounds: TinyBounds) -> impl Strategy<Value = MincostNetwork> {
        TinyMincostNetworks {
            arcs: 0..=6,
            costs: TinyCosts::Uniform(-9..=9),
            supplies: TinySupplies::OfAFlowOrRandom,
            bounds,
        }
        .strategy()
    }

    /// The length of `arcs` and the arcs it yields front to back and back to front, each read by `read`.
    fn both_ways<T>(
        arcs: impl DoubleEndedIterator<Item = CheckedArc> + ExactSizeIterator + Clone,
        read: impl Fn(CheckedArc) -> T,
    ) -> (usize, Vec<T>, Vec<T>) {
        (arcs.len(), arcs.clone().map(&read).collect(), arcs.rev().map(read).collect())
    }

    proptest! {
        #[test]
        fn test_a_tiny_problem_is_refused_exactly_at_its_lowest_inverted_arc(network in tiny(TinyBounds::OrInverted)) {
            let inverted = (0..network.arc_count()).find(|&arc| network.lower_bounds[arc] > network.capacities[arc]);

            match (problem_of(&network).checked(), inverted) {
                (Ok(checked), None) => {
                    let largest = network.costs.iter().map(|cost| cost.unsigned_abs()).max().unwrap_or(0);
                    prop_assert_eq!(checked.largest_cost(), largest);
                }
                (Err(breach), Some(arc)) => {
                    let (lower_bound, capacity) = (network.lower_bounds[arc], network.capacities[arc]);
                    prop_assert_eq!(breach, ProblemError::LowerAboveCapacity { arc, lower_bound, capacity });
                }
                (answer, _) => prop_assert!(false, "{:?} for inverted arc {:?}", answer.map(|_| ()), inverted),
            }
        }

        #[test]
        fn test_checked_arcs_equal_plain_indexing(network in tiny(TinyBounds::Ordered), given in any::<bool>()) {
            let problem = Problem { lower_bounds: given.then_some(&network.lower_bounds[..]), ..problem_of(&network) };
            let checked = problem.checked().expect("a tiny network of ordered bounds should be admitted");

            let mut indexed = Vec::new();
            for arc in 0..network.arc_count() {
                let lower_bound = if given { network.lower_bounds[arc] } else { 0 };
                let (tail, head) = (network.tails[arc], network.heads[arc]);
                let (capacity, cost) = (network.capacities[arc], network.costs[arc]);
                indexed.push((tail, head, lower_bound, capacity, cost, capacity - lower_bound));
            }
            let read = |arc: CheckedArc| {
                let (tail, head) = (usize::from(arc.tail()), usize::from(arc.head()));
                (tail, head, arc.lower_bound(), arc.capacity, arc.cost(), arc.width())
            };

            let (length, forward, backward) = match checked.arcs() {
                CheckedArcs::WithoutLowerBounds(arcs) => {
                    prop_assert!(!given, "the lower bounds were given");
                    both_ways(arcs, read)
                }
                CheckedArcs::WithLowerBounds(arcs) => {
                    prop_assert!(given, "no lower bounds were given");
                    both_ways(arcs, read)
                }
            };
            prop_assert_eq!(length, indexed.len());
            prop_assert_eq!(forward, indexed.clone());
            indexed.reverse();
            prop_assert_eq!(backward, indexed);
        }

        #[test]
        fn test_lower_bounds_equal_plain_indexing(network in tiny(TinyBounds::Ordered)) {
            let checked = problem_of(&network).checked().expect("a tiny network of ordered bounds should be admitted");
            let entries = checked.lower_bounds().expect("the problem should give lower bounds");

            let mut indexed: Vec<(usize, usize, u64)> = (0..network.arc_count())
                .map(|arc| (network.tails[arc], network.heads[arc], network.lower_bounds[arc]))
                .collect();
            let read = |entry: ArcLowerBound| {
                (usize::from(entry.tail()), usize::from(entry.head()), entry.lower_bound())
            };

            prop_assert_eq!(entries.len(), indexed.len());
            prop_assert_eq!(entries.clone().map(read).collect::<Vec<_>>(), indexed.clone());
            indexed.reverse();
            prop_assert_eq!(entries.rev().map(read).collect::<Vec<_>>(), indexed);
        }
    }
}
