// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a starting basis is made from: each node's supply net of the lower bounds, which the star starts from; each
//! node's excess over a kept basis, which a kept basis starts from; and the root link a node starts on.
//!
//! It is shared ground of the two front ends: both gather the net supplies of the star. The excesses over a kept basis
//! serve the network's load alone, and stay here beside the start because the basis re-flows a kept basis from them.

use core::iter::FusedIterator;
use core::slice;

use std::collections::TryReserveError;

use typed_index_collections::{TiSlice, TiVec};

use crate::problem::{CheckedArc, CheckedArcs, CheckedProblem, NodeId, ProblemError};
use crate::ranges::NET_SUPPLY_LIMIT;
use crate::room::{grow_total, refill, reserve_total};
use crate::tree::Direction;

/// Each node's supply net of the lower bounds, in the room a basis is made in.
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
/// `net_supplies` holds one entry per node of `problem`: each node's supply net of the lower bounds. They add up to
/// zero; where they are positive they add up to at most [`NET_SUPPLY_LIMIT`], and so every one of them has a magnitude
/// of at most [`NET_SUPPLY_LIMIT`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Start<'p, 's> {
    /// The problem.
    problem: CheckedProblem<'p>,
    /// Per node of the problem: its supply net of the lower bounds, in units of flow.
    net_supplies: &'s TiSlice<NodeId, i128>,
}

/// Each node's excess over a kept basis, in the room a network keeps from one start to the next: its net supply less
/// what the arcs out of the kept tree carry from it, plus what they carry into it.
///
/// The room only grows; nothing of an earlier problem is read again: [`gather`](Self::gather) rewrites every entry it
/// lends.
#[derive(Clone, Debug, Default)]
pub(crate) struct Excesses {
    /// Per node of the last problem gathered: its excess, in units of flow.
    values: TiVec<NodeId, i128>,
}

/// A checked problem with each node's excess over a kept basis: what a run from a kept basis starts from. Only
/// [`Excesses::gather`] makes one, so the two always belong together.
///
/// # Invariant
/// `excesses` holds one entry per node of `problem`: its net supply less what the arcs out of the kept tree carry from
/// it above their lower bounds, plus what they carry into it. They add up to zero, and where they are positive to at
/// most [`NET_SUPPLY_LIMIT`], so every one of them has a magnitude of at most [`NET_SUPPLY_LIMIT`].
#[derive(Debug)]
pub(crate) struct KeptStart<'p, 'e> {
    /// The problem.
    problem: CheckedProblem<'p>,
    /// Per node of the problem: its excess, in units of flow.
    excesses: &'e mut TiSlice<NodeId, i128>,
}

/// The root link a node hangs from the root by, made from its excess. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RootLink {
    /// What it carries, in units of flow: the magnitude of the node's excess, its net supply in the star.
    pub(crate) flow: u64,
    /// Up to the root for an excess of zero or more, down from it for one below zero.
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

    /// Makes room for the net supplies of up to `node_count` nodes, as [`try_reserve`](Self::try_reserve) does, growing
    /// as a `Vec` does where the memory runs out: a gather of such a problem allocates nothing afterwards.
    ///
    /// # Panics
    /// Panics if the room passes `isize::MAX` bytes, as a growing `Vec` does: the net supplies take `16·n`.
    pub(crate) fn grow(&mut self, node_count: usize) {
        grow_total(&mut self.values.raw, node_count);
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
        within_limit(&self.values).map_err(|total| ProblemError::NetSupplyBeyondRange { total })?;

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
            let (tail, head) = (arc.tail(), arc.head());
            debug_assert!(
                usize::from(tail) < self.values.len() && usize::from(head) < self.values.len(),
                "a lower-bounded arc's ends should be nodes of its problem"
            );
            // SAFETY: an arc's ends are below its problem's node count (`ArcLowerBound`, kept by `CheckedProblem`),
            // and the values hold one entry per node of that problem since the copy of its supplies above.
            unsafe { move_along_unchecked(&mut self.values, tail, head, i128::from(arc.lower_bound())) };
        }
    }
}

impl Excesses {
    /// Excesses that hold no room yet.
    pub(crate) const fn new() -> Self {
        Self { values: TiVec::new() }
    }

    /// Makes room for the excesses of up to `node_count` nodes: a gather of such a problem allocates nothing
    /// afterwards. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of a refused reservation.
    pub(crate) fn try_reserve(&mut self, node_count: usize) -> Result<(), TryReserveError> {
        reserve_total(&mut self.values.raw, node_count)
    }

    /// Makes room for the excesses of up to `node_count` nodes, as [`try_reserve`](Self::try_reserve) does, growing as
    /// a `Vec` does where the memory runs out: a gather of such a problem allocates nothing afterwards.
    ///
    /// # Panics
    /// Panics if the room passes `isize::MAX` bytes, as a growing `Vec` does: the excesses take `16·n`.
    pub(crate) fn grow(&mut self, node_count: usize) {
        grow_total(&mut self.values.raw, node_count);
    }

    /// The start of the problem of `start` from a kept basis whose arcs out of the tree carry `carried` above their
    /// lower bounds, arc `a`'s flow at `a`: each node's excess, written to the room these excesses hold, with the
    /// problem; `None` where the positive excesses add up past [`NET_SUPPLY_LIMIT`]. `start` is unchanged. It makes two
    /// passes over the nodes, to copy the net supplies and to add up the positive excesses, and one over the arcs.
    ///
    /// # Panics
    /// Panics if `carried` yields another number of flows than the problem has arcs. The flows are counted as they are
    /// taken; the room may then hold anything, which nothing reads.
    #[track_caller]
    pub(crate) fn gather<'p, 'e>(
        &'e mut self,
        start: &Start<'p, '_>,
        mut carried: impl Iterator<Item = u64>,
    ) -> Option<KeptStart<'p, 'e>> {
        let arc_count = start.problem().arc_count();
        let taken = self.write_excesses(start, &mut carried);
        // The flows are counted where they were taken, never by what the iterator says of its length.
        let given = taken + carried.count();
        assert!(given == arc_count, "called `Excesses::gather` with {given} flows for {arc_count} arcs");

        within_limit(&self.values).ok()?;

        // The net supplies add up to zero (`Start`), and each flow is taken from one node and given to another, so
        // the excesses add up to zero too: the negative ones add up to the negation of the positive total, and every
        // excess has a magnitude of at most the limit, `KeptStart`'s invariant.
        Some(KeptStart { problem: start.problem(), excesses: &mut self.values })
    }

    /// Rewrites the values as the excesses of the problem of `start` where its arcs carry `carried`, arc `a`'s flow
    /// at `a`, one per node; answers how many flows it took, at most one per arc.
    fn write_excesses(&mut self, start: &Start<'_, '_>, carried: &mut impl Iterator<Item = u64>) -> usize {
        refill(&mut self.values.raw, start.net_supplies().iter().copied());

        match start.problem().arcs() {
            CheckedArcs::WithoutLowerBounds(arcs) => self.carry(arcs, carried),
            CheckedArcs::WithLowerBounds(arcs) => self.carry(arcs, carried),
        }
    }

    /// Moves what each arc of `arcs`, the arcs of the problem whose net supplies the values hold, in ascending order,
    /// carries, as `carried` gives it in the same order, from its tail's value to its head's; answers how many flows
    /// it took, one per arc until either runs out.
    fn carry(&mut self, arcs: impl Iterator<Item = CheckedArc>, carried: &mut impl Iterator<Item = u64>) -> usize {
        let mut taken = 0;

        // An excess is a net supply, at most `2^64` in magnitude (`Start`), moved by at most `m` flows below `2^64`
        // each: within `2^64 + m·2^64 < 2^97` for the at most `2^32` arcs of a checked problem, inside an `i128`.
        for (arc, flow) in arcs.zip(carried) {
            let (tail, head) = (arc.tail(), arc.head());
            debug_assert!(
                usize::from(tail) < self.values.len() && usize::from(head) < self.values.len(),
                "an arc's ends should be nodes of its problem"
            );
            // SAFETY: an arc's ends are below its problem's node count (`CheckedArc`, kept by `CheckedProblem`), and
            // the values hold one entry per node of that problem since the copy of its net supplies (`Start`).
            unsafe { move_along_unchecked(&mut self.values, tail, head, i128::from(flow)) };
            taken += 1;
        }

        taken
    }
}

impl<'p, 'e> KeptStart<'p, 'e> {
    /// The problem.
    pub(crate) fn problem(&self) -> CheckedProblem<'p> {
        self.problem
    }

    /// Each node's excess, in units of flow, one per node of the problem, lent to be spent: the re-flow adds each
    /// node's transfer to its parent's.
    pub(crate) fn into_excesses(self) -> &'e mut TiSlice<NodeId, i128> {
        self.excesses
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

/// The root link of a node that hangs from the root with the excess `excess`: up, carrying it, where it is zero or
/// more; down, carrying its magnitude, where it is below zero. Every start hangs a node from the root by this rule: the
/// star with the node's net supply, its excess there, and a kept basis with the excess the re-flow leaves the node.
///
/// # Panics
/// Panics if the magnitude of `excess` passes `u64::MAX`.
pub(crate) fn root_link(excess: i128) -> RootLink {
    let flow = match u64::try_from(excess.unsigned_abs()) {
        Ok(flow) => flow,
        Err(_) => excess_past_u64(excess),
    };
    let direction = if excess < 0 { Direction::Down } else { Direction::Up };

    RootLink { flow, direction }
}

/// Panics for an `excess` whose magnitude passes `u64::MAX`, as [`root_link`] documents.
///
/// # Panics
/// Always: that is its job.
// Cold: no excess of a start passes the limit, so the passes that make root links keep only the comparison. Never
// inlined, so that the message is built outside those passes. It reports the line in `root_link` that refused it.
#[cold]
#[inline(never)]
#[track_caller]
fn excess_past_u64(excess: i128) -> ! {
    panic!("called `root_link` with excess {excess}, whose magnitude passes u64::MAX")
}

/// Moves `amount` from the value of `tail` to that of `head`, in `values`, one value per node, as
/// [`move_along_unchecked`] does: its checked twin, which its tests check it against.
///
/// # Panics
/// Panics if `tail` or `head` is not below `values.len()`.
#[cfg(test)]
#[track_caller]
fn move_along(values: &mut TiSlice<NodeId, i128>, tail: NodeId, head: NodeId, amount: i128) {
    assert!(
        usize::from(tail) < values.len() && usize::from(head) < values.len(),
        "called `move_along` with the tail {tail:?} or the head {head:?} past {} values",
        values.len()
    );

    debug_assert!(usize::from(tail) < values.len() && usize::from(head) < values.len(), "as just asserted");
    // SAFETY: both ends are below the values' length, as just asserted.
    unsafe { move_along_unchecked(values, tail, head, amount) };
}

/// Moves `amount` from the value of `tail` to that of `head`, in `values`, one value per node: the one step of the
/// netting of the lower bounds and of what the arcs out of a kept tree carry.
///
/// # Safety
/// `tail` and `head` are below `values.len()`.
unsafe fn move_along_unchecked(values: &mut TiSlice<NodeId, i128>, tail: NodeId, head: NodeId, amount: i128) {
    debug_assert!(
        usize::from(tail) < values.len() && usize::from(head) < values.len(),
        "called `move_along_unchecked` with the tail {tail:?} or the head {head:?} past {} values",
        values.len()
    );
    // SAFETY: both ends are below the values' length, the caller's promise.
    unsafe {
        *values.get_unchecked_mut(tail) -= amount;
        *values.get_unchecked_mut(head) += amount;
    }
}

/// Whether the positive entries of `values`, one per node, add up to at most [`NET_SUPPLY_LIMIT`]: the limit a start's
/// values keep, so that no root link carries more than they add up to.
///
/// # Errors
/// Returns their positive total, in units of flow, where it passes the limit.
fn within_limit(values: &TiSlice<NodeId, i128>) -> Result<(), u128> {
    let total = positive_total(values);

    if total <= u128::from(NET_SUPPLY_LIMIT) { Ok(()) } else { Err(total) }
}

/// The sum of `values` where they are positive, in units of flow.
///
/// # Panics
/// Panics if the sum passes `u128::MAX`, which neither the net supplies nor the excesses of a checked problem do.
fn positive_total(values: &TiSlice<NodeId, i128>) -> u128 {
    // Each supply adds to one node's value, each lower bound to one node's, and each flow an arc out of a kept tree
    // carries, at most its width, to one node's: so the positive total is at most the supplies' magnitudes and, arc by
    // arc, its lower bound and its width added up: below `n·2^63 + 2m·2^64 < 2^98` for the at most `2^32` nodes and
    // arcs of a checked problem, inside a `u128`.
    values.iter().filter_map(|&value| u128::try_from(value).ok()).fold(0, u128::strict_add)
}

#[cfg(test)]
mod tests {
    use kahlnws_testing::{MincostNetwork, TinyBounds, TinyCosts, TinyMincostNetworks, TinySupplies};
    use num_bigint::BigInt;
    use proptest::prelude::*;

    use super::*;
    use crate::problem::Problem;
    use crate::problem::tests::problem_of;

    impl NetSupplies {
        /// The room of its one column, in entries.
        pub(crate) fn capacities(&self) -> [usize; 1] {
            [self.values.capacity()]
        }
    }

    impl Excesses {
        /// The room of its one column, in entries.
        pub(crate) fn capacities(&self) -> [usize; 1] {
            [self.values.capacity()]
        }
    }

    #[test]
    fn test_grow_makes_the_room_try_reserve_makes() {
        // 100 nodes: above the least room an empty `Vec` makes, 4 entries of 16 bytes, so the total is the room made,
        // and a total that drifts by one shows.
        let (mut grown, mut reserved) = (NetSupplies::new(), NetSupplies::new());
        grown.grow(100);
        reserved.try_reserve(100).expect("100 values should fit");
        assert_eq!((grown.capacities(), reserved.capacities()), ([100], [100]));

        let (mut grown, mut reserved) = (Excesses::new(), Excesses::new());
        grown.grow(100);
        reserved.try_reserve(100).expect("100 values should fit");
        assert_eq!((grown.capacities(), reserved.capacities()), ([100], [100]));
    }

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
    #[should_panic(expected = "called `root_link` with excess 18446744073709551616, whose magnitude passes u64::MAX")]
    fn test_root_link_past_u64_max_panics() {
        let _ = root_link(i128::from(u64::MAX) + 1);
    }

    #[test]
    fn test_move_along_takes_from_the_tail_and_gives_to_the_head_by_hand() {
        let mut values = [5_i128, -2, 0];
        let along = |values: &mut [i128], tail: usize, head: usize, amount| {
            move_along(TiSlice::from_mut(values), NodeId::from(tail), NodeId::from(head), amount);
        };
        along(&mut values, 0, 2, 3);
        assert_eq!(values, [2, -2, 3]);

        // A loop takes and gives back; a negative amount runs the other way.
        along(&mut values, 1, 1, 7);
        along(&mut values, 2, 1, -1);
        assert_eq!(values, [2, -3, 4]);
    }

    #[test]
    #[should_panic(expected = "called `move_along` with the tail NodeId 0 or the head NodeId 3 past 3 values")]
    fn test_move_along_past_the_values_panics() {
        move_along(TiSlice::from_mut(&mut [0_i128; 3]), NodeId::from(0_usize), NodeId::from(3_usize), 1);
    }

    proptest! {
        #[test]
        fn test_move_along_and_its_unchecked_twin_move_alike(
            (values, tail, head, amount) in proptest::collection::vec(-1000_i128..1000, 1..8)
                .prop_flat_map(|values| {
                    let count = values.len();
                    (Just(values), 0..count, 0..count, -1000_i128..1000)
                }),
        ) {
            let (tail, head) = (NodeId::from(tail), NodeId::from(head));
            let mut checked = values.clone();
            move_along(TiSlice::from_mut(&mut checked), tail, head, amount);
            let mut unchecked = values.clone();
            debug_assert!(
                usize::from(tail) < values.len() && usize::from(head) < values.len(),
                "both ends should be below the values' count"
            );
            // SAFETY: both ends were drawn below the values' length.
            unsafe { move_along_unchecked(TiSlice::from_mut(&mut unchecked), tail, head, amount) };
            prop_assert_eq!(&checked, &unchecked);
            prop_assert_eq!(checked.iter().sum::<i128>(), values.iter().sum::<i128>());
        }
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
    fn test_within_limit_at_the_limit_and_one_past_it_by_hand() {
        let limit = i128::from(NET_SUPPLY_LIMIT);

        assert_eq!(within_limit(TiSlice::from_ref(&[])), Ok(()));
        assert_eq!(within_limit(TiSlice::from_ref(&[3, -5, 2])), Ok(()));
        assert_eq!(within_limit(TiSlice::from_ref(&[limit - 1, 1, -limit])), Ok(()), "exactly at the limit");
        assert_eq!(within_limit(TiSlice::from_ref(&[-limit, limit])), Ok(()));

        let past = u128::from(NET_SUPPLY_LIMIT) + 1;
        assert_eq!(within_limit(TiSlice::from_ref(&[limit, 1, -limit - 1])), Err(past), "the total passes it");
        assert_eq!(within_limit(TiSlice::from_ref(&[limit + 1, -limit - 1])), Err(past), "one value passes it");
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
        assert_eq!(start.problem().root_cost(), 9);
        assert_eq!((start.problem().node_count(), start.problem().arc_count()), (3, 4));
    }

    /// The excesses `gather` lends for `problem` where its arcs out of the tree carry `carried`, read out; `None` past
    /// the limit.
    fn excesses_of(problem: &Problem<'_>, carried: &[u64]) -> Option<Vec<i128>> {
        let mut net_supplies = NetSupplies::new();
        let start = net_supplies.gather(checked(problem)).expect("the test's net supplies should be in range");
        let mut excesses = Excesses::new();
        let kept = excesses.gather(&start, carried.iter().copied())?;

        Some(kept.into_excesses().iter().copied().collect())
    }

    #[test]
    fn test_excesses_move_what_the_arcs_carry_from_tail_to_head_by_hand() {
        // The net supplies are 2, 2 and −4. Arc 1, from 1 to 2, carries its width 2: node 1 sends it, node 2 takes it.
        let network = three_nodes();
        assert_eq!(excesses_of(&problem_of(&network), &[0, 2, 0, 0]), Some(vec![2, 0, -2]));

        // And arc 3, from 2 to 0, its width 2 as well; arc 0 one unit above its lower bound, from 0 to 1.
        assert_eq!(excesses_of(&problem_of(&network), &[1, 2, 0, 2]), Some(vec![3, 1, -4]));

        // Nothing carried: the excesses are the net supplies, those of the star.
        assert_eq!(excesses_of(&problem_of(&network), &[0; 4]), Some(vec![2, 2, -4]));
        let without = Problem { lower_bounds: None, ..problem_of(&network) };
        assert_eq!(excesses_of(&without, &[0, 2, 0, 0]), Some(vec![3, -2, -1]));
    }

    #[test]
    fn test_excesses_at_the_limit_and_one_past_it() {
        // Net supplies whose positive total is `2^64 − 2`, the limit, and an arc 2 -> 0 of no lower bound.
        let mut network = MincostNetwork::default();
        for supply in [i64::MAX, i64::MAX, i64::MIN, i64::MIN + 2] {
            network.add_node(supply);
        }
        network.add_arc(2, 0, 0, 1, 0);

        assert!(excesses_of(&problem_of(&network), &[0]).is_some());
        // The arc carries one unit to node 0: one past the limit.
        assert_eq!(excesses_of(&problem_of(&network), &[1]), None);
    }

    #[test]
    fn test_excesses_leave_the_start_and_rewrite_the_room_of_an_earlier_problem() {
        let (large, small) = (three_nodes(), MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 3, 1)]));
        let mut excesses = Excesses::new();
        excesses.try_reserve(2).expect("room for two nodes should be granted");

        let mut net_supplies = NetSupplies::new();
        let start = net_supplies.gather(checked(&problem_of(&large))).expect("the net supplies should be in range");
        let kept = excesses.gather(&start, [0, 2, 0, 0].into_iter()).expect("the excesses should be in range");
        assert_eq!((kept.problem().node_count(), kept.problem().arc_count()), (3, 4));
        assert_eq!(kept.into_excesses().iter().copied().collect::<Vec<_>>(), [2, 0, -2]);
        assert_eq!(start.net_supplies().iter().copied().collect::<Vec<_>>(), [2, 2, -4], "the start is unchanged");

        let start = net_supplies.gather(checked(&problem_of(&small))).expect("the net supplies should be in range");
        let kept = excesses.gather(&start, [3].into_iter()).expect("the excesses should be in range");
        assert_eq!(kept.into_excesses().iter().copied().collect::<Vec<_>>(), [-2, 2]);
    }

    #[test]
    fn test_excesses_try_reserve_makes_room_and_keeps_what_is_held() {
        let mut excesses = Excesses::default();
        excesses.try_reserve(9).expect("room for nine nodes should be granted");
        assert!(excesses.values.capacity() >= 9);

        excesses.try_reserve(2).expect("a smaller reservation should be granted");
        assert!(excesses.values.capacity() >= 9, "room already held is kept");
        assert!(excesses.try_reserve(usize::MAX).is_err());
    }

    #[test]
    fn test_excesses_grow_to_room_for_a_gather_and_keep_what_is_held() {
        let mut excesses = Excesses::new();
        excesses.grow(4);
        let (start, room) = (excesses.values.as_ptr(), excesses.values.capacity());
        assert!(room >= 4);

        let network = three_nodes();
        let mut net_supplies = NetSupplies::new();
        let start_of =
            net_supplies.gather(checked(&problem_of(&network))).expect("the net supplies should be in range");
        let _ = excesses.gather(&start_of, [0; 4].into_iter()).expect("the excesses should be in range");
        excesses.grow(2);
        assert_eq!((excesses.values.as_ptr(), excesses.values.capacity()), (start, room), "the gather grew nothing");
    }

    #[test]
    #[should_panic(expected = "called `Excesses::gather` with 3 flows for 4 arcs")]
    fn test_excesses_refuse_another_number_of_flows_than_arcs() {
        let _ = excesses_of(&problem_of(&three_nodes()), &[0, 0, 0]);
    }

    #[test]
    #[should_panic(expected = "called `Excesses::gather` with 6 flows for 4 arcs")]
    fn test_excesses_refuse_flows_left_over_past_the_arcs() {
        let _ = excesses_of(&problem_of(&three_nodes()), &[0; 6]);
    }

    proptest! {
        #[test]
        fn test_excesses_equal_the_net_supplies_moved_by_plain_indexing(
            (network, picks) in TinyMincostNetworks {
                arcs: 0..=6,
                costs: TinyCosts::Uniform(-5..=5),
                supplies: TinySupplies::OfAFlowOrRandom,
                bounds: TinyBounds::Ordered,
            }.strategy().prop_flat_map(|network| {
                let arcs = network.arc_count();
                (Just(network), proptest::collection::vec(any::<bool>(), arcs))
            }),
        ) {
            // Each arc carries nothing or its width, as an arc out of a kept tree does.
            let widths = network.capacities.iter().zip(&network.lower_bounds).map(|(capacity, lower)| capacity - lower);
            let carried: Vec<u64> = widths.zip(&picks).map(|(width, &full)| if full { width } else { 0 }).collect();
            let mut expected: Vec<i128> = network.supplies.iter().map(|&supply| i128::from(supply)).collect();
            for arc in 0..network.arc_count() {
                let moved = i128::from(network.lower_bounds[arc]) + i128::from(carried[arc]);
                expected[network.tails[arc]] -= moved;
                expected[network.heads[arc]] += moved;
            }

            prop_assert_eq!(excesses_of(&problem_of(&network), &carried), Some(expected));
        }

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
