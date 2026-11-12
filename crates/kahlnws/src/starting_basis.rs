// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the starting basis is made from: each node's supply net of the lower bounds, and the root link it starts on.

use core::iter::FusedIterator;
use core::slice;

use std::collections::TryReserveError;

use typed_index_collections::{TiSlice, TiVec};

use crate::problem::{CheckedProblem, NodeId, ProblemError};
use crate::ranges::{NET_SUPPLY_LIMIT, root_cost};
use crate::room::{refill, reserve_total};
use crate::tree::Direction;

/// Each node's supply net of the lower bounds, in the room the solver keeps from one solve to the next.
///
/// The room only grows; nothing of an earlier problem is read again: [`gather`](Self::gather) rewrites every entry it
/// lends.
#[derive(Clone, Debug, Default)]
pub(crate) struct NetSupplies {
    /// Per node of the last problem gathered: its supply, less the lower bounds of the arcs that leave it, plus those
    /// of the arcs that enter it, in units of flow.
    values: TiVec<NodeId, i128>,
}

/// A checked problem with its net supplies: what a run starts from. Only [`NetSupplies::gather`] makes one, so the
/// two always belong together.
///
/// # Invariant
/// `net_supplies` holds one entry per node of `problem`: each node's supply net of the lower bounds. Where they are
/// positive they add up to at most [`NET_SUPPLY_LIMIT`], and every one of them has a magnitude of at most
/// [`NET_SUPPLY_LIMIT`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Start<'p, 's> {
    /// The problem.
    problem: CheckedProblem<'p>,
    /// Per node of the problem: its supply net of the lower bounds, in units of flow.
    net_supplies: &'s TiSlice<NodeId, i128>,
}

/// The root link a node starts with. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RootLink {
    /// What it carries, in units of flow: the magnitude of the node's net supply.
    pub(crate) flow: u64,
    /// Up to the root for a net supply of zero or more, down from it for a demand.
    pub(crate) direction: Direction,
}

impl NetSupplies {
    /// Net supplies that hold no room yet.
    pub(crate) const fn new() -> Self {
        Self { values: TiVec::new() }
    }

    /// Makes room for the net supplies of up to `node_count` nodes: a gather of such a problem allocates nothing
    /// afterwards. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of a refused reservation.
    pub(crate) fn try_reserve(&mut self, node_count: usize) -> Result<(), TryReserveError> {
        reserve_total(&mut self.values.raw, node_count)
    }

    /// The start of `problem`: each node's supply net of the lower bounds, written to the room these net supplies
    /// hold, with the problem. Without lower bounds it copies the supplies; with them it makes one more pass over the
    /// arcs.
    ///
    /// # Errors
    /// Returns [`ProblemError::NetSupplyBeyondRange`] where the positive net supplies add up past
    /// [`NET_SUPPLY_LIMIT`].
    pub(crate) fn gather<'p>(&mut self, problem: CheckedProblem<'p>) -> Result<Start<'p, '_>, ProblemError> {
        self.write_net_supplies(problem);

        let total = positive_total(&self.values);
        if total > u128::from(NET_SUPPLY_LIMIT) {
            return Err(ProblemError::NetSupplyBeyondRange { total });
        }

        // The net supplies add up to the supplies' sum, zero, since each lower bound is taken from one node and given
        // to another. So the negative ones add up to the negation of the positive total, and every net supply has a
        // magnitude of at most the limit: `Start`'s invariant.
        Ok(Start { problem, net_supplies: &self.values })
    }

    /// Rewrites the values as the net supplies of `problem`, one per node.
    fn write_net_supplies(&mut self, problem: CheckedProblem<'_>) {
        refill(&mut self.values.raw, problem.supplies().iter().map(|&supply| i128::from(supply)));

        let Some(lower_bounds) = problem.lower_bounds() else {
            return;
        };

        // A net supply is a supply, at most `2^63` in magnitude, moved by at most `m` lower bounds below `2^64`
        // each: within `2^63 + m·2^64 < 2^97` for the at most `2^32` arcs of a checked problem, inside an `i128`.
        for arc in lower_bounds {
            let (tail, head, lower_bound) = (arc.tail(), arc.head(), i128::from(arc.lower_bound()));

            debug_assert!(
                usize::from(tail) < self.values.len() && usize::from(head) < self.values.len(),
                "an arc's tail and head should be nodes"
            );
            // SAFETY: both ends are below their problem's node count (`ArcLowerBound`'s invariant, kept by
            // `CheckedProblem`), and `values` holds one entry per node of that problem since the copy above.
            unsafe {
                *self.values.get_unchecked_mut(tail) -= lower_bound;
                *self.values.get_unchecked_mut(head) += lower_bound;
            }
        }
    }
}

impl<'p, 's> Start<'p, 's> {
    /// The problem.
    pub(crate) fn problem(&self) -> CheckedProblem<'p> {
        self.problem
    }

    /// Each node's supply net of the lower bounds, in units of flow, one per node of the problem.
    pub(crate) fn net_supplies(&self) -> &'s TiSlice<NodeId, i128> {
        self.net_supplies
    }

    /// The root link of each node, in ascending node order.
    pub(crate) fn root_links(&self) -> RootLinksIter<'s> {
        RootLinksIter { net_supplies: self.net_supplies.iter() }
    }

    /// The cost of a root link that points down, in units of cost: `(n − 1)·C + 1` for the problem's `n` nodes and
    /// largest cost magnitude `C`.
    pub(crate) fn root_cost(&self) -> i64 {
        // The problem's costs lie within its cost bound, so the root cost is at most `2^62` and `root_cost` does not
        // panic.
        root_cost(self.problem.node_count(), self.problem.largest_cost())
    }
}

/// The root link of each node of a [`Start`], in ascending node order.
///
/// # Invariant
/// It walks the net supplies of one start, so every one of them has a magnitude of at most [`NET_SUPPLY_LIMIT`]
/// ([`Start`]'s invariant), and its root link does not panic.
#[derive(Clone, Debug)]
pub(crate) struct RootLinksIter<'s> {
    /// The net supplies still to read.
    net_supplies: slice::Iter<'s, i128>,
}

impl Iterator for RootLinksIter<'_> {
    type Item = RootLink;

    fn next(&mut self) -> Option<RootLink> {
        self.net_supplies.next().map(|&net_supply| root_link(net_supply))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.net_supplies.size_hint()
    }
}

impl DoubleEndedIterator for RootLinksIter<'_> {
    fn next_back(&mut self) -> Option<RootLink> {
        self.net_supplies.next_back().map(|&net_supply| root_link(net_supply))
    }
}

impl ExactSizeIterator for RootLinksIter<'_> {}

impl FusedIterator for RootLinksIter<'_> {}

/// The root link of a node of net supply `net_supply`: up, carrying it, where it is zero or more; down, carrying its
/// magnitude, where it is a demand.
///
/// # Panics
/// Panics if the magnitude of `net_supply` passes `u64::MAX`.
fn root_link(net_supply: i128) -> RootLink {
    let flow = match u64::try_from(net_supply.unsigned_abs()) {
        Ok(flow) => flow,
        Err(_) => net_supply_past_u64(net_supply),
    };
    let direction = if net_supply < 0 { Direction::Down } else { Direction::Up };

    RootLink { flow, direction }
}

/// Panics for a `net_supply` whose magnitude passes `u64::MAX`, as [`root_link`] documents.
///
/// # Panics
/// Always: that is its job.
// Cold: no net supply of a start passes the limit, so the root links' pass keeps only the comparison. Never inlined, so
// that the message is built outside that pass. It reports the line in `root_link` that refused the net supply.
#[cold]
#[inline(never)]
#[track_caller]
fn net_supply_past_u64(net_supply: i128) -> ! {
    panic!("called `root_link` with net supply {net_supply}, whose magnitude passes u64::MAX")
}

/// The sum of `net_supplies` where they are positive, in units of flow.
///
/// # Panics
/// Panics if the sum passes `u128::MAX`, which no net supplies of a checked problem do.
fn positive_total(net_supplies: &TiSlice<NodeId, i128>) -> u128 {
    // Each supply adds to one node's net supply and each lower bound to one node's, so the positive total is at most
    // the supplies' magnitudes and the lower bounds added up: below `n·2^63 + m·2^64 < 2^97` for the at most `2^32`
    // nodes and arcs of a checked problem, inside a `u128`.
    net_supplies.iter().filter_map(|&net_supply| u128::try_from(net_supply).ok()).fold(0, u128::strict_add)
}

#[cfg(test)]
mod tests {
    use kahlnws_testing::{MincostNetwork, TinyBounds, TinyCosts, TinyMincostNetworks, TinySupplies};
    use num_bigint::BigInt;
    use proptest::prelude::*;

    use super::*;
    use crate::problem::Problem;
    use crate::problem::tests::problem_of;

    /// The checked problem of `problem`, which the test knows to be admitted.
    fn checked<'a>(problem: &Problem<'a>) -> CheckedProblem<'a> {
        problem.checked().expect("the test's problem should be admitted")
    }

    /// Three nodes of supply 3, 0 and -3; arcs 0 -> 1 of lower bound 2, 1 -> 2 of lower bound 0, a loop at 2 of lower
    /// bound 5 and 2 -> 0 of lower bound 1, costs 4, -1, 0 and 2.
    fn three_nodes() -> MincostNetwork {
        let mut network = MincostNetwork::default();
        let nodes = [network.add_node(3), network.add_node(0), network.add_node(-3)];
        network.add_arc(nodes[0], nodes[1], 2, 4, 4);
        network.add_arc(nodes[1], nodes[2], 0, 2, -1);
        network.add_arc(nodes[2], nodes[2], 5, 5, 0);
        network.add_arc(nodes[2], nodes[0], 1, 3, 2);

        network
    }

    /// The net supplies `gather` lends for `problem`, read out.
    fn gathered(problem: &Problem<'_>) -> Result<Vec<i128>, ProblemError> {
        let mut net_supplies = NetSupplies::new();
        let start = net_supplies.gather(checked(problem))?;

        Ok(start.net_supplies().iter().copied().collect())
    }

    #[test]
    fn test_net_supplies_take_the_lower_bounds_out_and_in_by_hand() {
        // Node 0: 3 less 2 leaving, plus 1 entering. Node 1: 0 plus 2 entering. Node 2: -3 less 1 leaving; its loop
        // takes 5 out and puts 5 back.
        let network = three_nodes();

        assert_eq!(gathered(&problem_of(&network)), Ok(vec![2, 2, -4]));
        assert_eq!(gathered(&Problem { lower_bounds: None, ..problem_of(&network) }), Ok(vec![3, 0, -3]));
    }

    #[test]
    fn test_gather_rewrites_the_room_of_an_earlier_problem() {
        let (large, small) = (three_nodes(), MincostNetwork { supplies: vec![1, -1], ..MincostNetwork::default() });
        let mut net_supplies = NetSupplies::new();
        net_supplies.try_reserve(2).expect("room for two nodes should be granted");

        let first = net_supplies.gather(checked(&problem_of(&large))).expect("the net supplies should be in range");
        assert_eq!(first.net_supplies().iter().copied().collect::<Vec<_>>(), [2, 2, -4]);

        let second = net_supplies.gather(checked(&problem_of(&small))).expect("the net supplies should be in range");
        assert_eq!(second.net_supplies().iter().copied().collect::<Vec<_>>(), [1, -1]);
        assert_eq!(second.problem().node_count(), 2);
    }

    #[test]
    fn test_try_reserve_makes_room_and_keeps_what_is_held() {
        let mut net_supplies = NetSupplies::default();
        net_supplies.try_reserve(9).expect("room for nine nodes should be granted");
        assert!(net_supplies.values.capacity() >= 9);

        let network = three_nodes();
        net_supplies.gather(checked(&problem_of(&network))).expect("the net supplies should be in range");
        net_supplies.try_reserve(2).expect("a smaller reservation should be granted");
        assert!(net_supplies.values.capacity() >= 9, "room already held is kept");
        assert_eq!(net_supplies.values.len(), 3, "a reservation changes no net supply");

        assert!(net_supplies.try_reserve(usize::MAX).is_err());
    }

    #[test]
    fn test_the_net_supply_limit_at_2_to_the_64_minus_2_and_one_past_it() {
        // Two supplies of `2^63 − 1` balanced by `−2^63` and `−2^63 + 2`: the positive total is `2^64 − 2`, the limit.
        let mut network = MincostNetwork::default();
        for supply in [i64::MAX, i64::MAX, i64::MIN, i64::MIN + 2] {
            network.add_node(supply);
        }
        let limit = (BigInt::from(1_u8) << 64) - 2;

        assert_eq!(BigInt::from(i64::MAX) * 2, limit);
        assert!(gathered(&problem_of(&network)).is_ok());

        // An arc 2 -> 0 of lower bound 1 moves one more unit from node 2 to node 0: one past the limit.
        network.add_arc(2, 0, 1, 1, 0);
        let past = u128::try_from(&limit + 1).expect("one past the limit should fit a u128");
        assert_eq!(gathered(&problem_of(&network)), Err(ProblemError::NetSupplyBeyondRange { total: past }));

        // The same arc without its lower bound leaves the total at the limit.
        assert!(gathered(&Problem { lower_bounds: None, ..problem_of(&network) }).is_ok());
    }

    #[test]
    fn test_root_link_at_zero_both_signs_and_the_limit_by_hand() {
        let (up, down) = (Direction::Up, Direction::Down);
        let limit = i128::from(NET_SUPPLY_LIMIT);

        assert_eq!(root_link(0), RootLink { flow: 0, direction: up });
        assert_eq!(root_link(5), RootLink { flow: 5, direction: up });
        assert_eq!(root_link(-5), RootLink { flow: 5, direction: down });
        assert_eq!(root_link(limit), RootLink { flow: NET_SUPPLY_LIMIT, direction: up });
        assert_eq!(root_link(-limit), RootLink { flow: NET_SUPPLY_LIMIT, direction: down });
        assert_eq!(root_link(-limit - 1), RootLink { flow: u64::MAX, direction: down });
    }

    #[test]
    #[should_panic(
        expected = "called `root_link` with net supply 18446744073709551616, whose magnitude passes u64::MAX"
    )]
    fn test_root_link_past_u64_max_panics() {
        let _ = root_link(i128::from(u64::MAX) + 1);
    }

    #[test]
    fn test_positive_total_adds_the_positive_net_supplies_by_hand() {
        assert_eq!(positive_total(TiSlice::from_ref(&[3, -1, 0, 4])), 7);
        assert_eq!(positive_total(TiSlice::from_ref(&[-3, -1])), 0);
        assert_eq!(positive_total(TiSlice::from_ref(&[])), 0);

        let widest = [i128::MAX, i128::MAX, -1];
        assert_eq!(positive_total(TiSlice::from_ref(&widest)), u128::MAX - 1);
    }

    #[test]
    fn test_a_start_lends_its_root_links_and_root_cost_by_hand() {
        let network = three_nodes();
        let mut net_supplies = NetSupplies::new();
        let start = net_supplies.gather(checked(&problem_of(&network))).expect("the net supplies should be in range");

        let (up, down) = (Direction::Up, Direction::Down);
        let links = [
            RootLink { flow: 2, direction: up },
            RootLink { flow: 2, direction: up },
            RootLink { flow: 4, direction: down },
        ];
        assert_eq!(start.root_links().collect::<Vec<_>>(), links);

        let mut both_ends = start.root_links();
        assert_eq!((both_ends.len(), both_ends.next_back()), (3, Some(links[2])));
        assert_eq!((both_ends.next(), both_ends.next_back()), (Some(links[0]), Some(links[1])));
        assert_eq!((both_ends.next(), both_ends.next_back(), both_ends.len()), (None, None, 0));

        // Three nodes and a largest cost of 4: `(3 − 1) · 4 + 1`.
        assert_eq!(start.root_cost(), 9);
        assert_eq!((start.problem().node_count(), start.problem().arc_count()), (3, 4));
    }

    proptest! {
        #[test]
        fn test_net_supplies_equal_the_supplies_moved_by_plain_indexing(
            network in TinyMincostNetworks {
                arcs: 0..=6,
                costs: TinyCosts::Uniform(-5..=5),
                supplies: TinySupplies::OfAFlowOrRandom,
                bounds: TinyBounds::Ordered,
            }.strategy(),
        ) {
            let mut expected: Vec<i128> = network.supplies.iter().map(|&supply| i128::from(supply)).collect();
            for arc in 0..network.arc_count() {
                expected[network.tails[arc]] -= i128::from(network.lower_bounds[arc]);
                expected[network.heads[arc]] += i128::from(network.lower_bounds[arc]);
            }

            let mut net_supplies = NetSupplies::new();
            let start = net_supplies.gather(checked(&problem_of(&network)));
            let start = start.expect("tiny net supplies should be in range");
            prop_assert_eq!(start.net_supplies().iter().copied().collect::<Vec<_>>(), expected.clone());

            let links: Vec<RootLink> = start.root_links().collect();
            prop_assert_eq!(links.len(), expected.len());
            for (link, net_supply) in links.into_iter().zip(expected) {
                prop_assert_eq!(i128::from(link.flow), net_supply.abs());
                prop_assert_eq!(link.direction == Direction::Down, net_supply < 0);
            }
        }
    }
}
