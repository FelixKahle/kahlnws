// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The sums a network keeps of its problem and its basis, so that a change is checked and a solve answers its cost
//! without a pass over the arcs.

use crate::ranges::{NET_SUPPLY_LIMIT, cost_total_with};

/// The cost total of a network's problem, the cost of the flow its basis carries, root links counted, and the flow up
/// its root links, kept as the problem and the basis change.
///
/// # Invariant
/// `cost_total` is the sum over the arcs of the network's problem of each cost's magnitude times its capacity, at most
/// `i128::MAX`. `kept_cost` is congruent modulo `2^128` to the cost of the flow of the network's basis in the network
/// extended by the root: every arc's cost times its flow, its lower bound included, plus the root cost times the flow
/// down the root links, which cost nothing where they point up. `excess` is at most [`NET_SUPPLY_LIMIT`] and at least
/// the flow up the root links of the network's basis, which equals the flow down them. It is that flow exactly between
/// the network's calls; during a solve, whose pivots never raise that flow, it is at least that flow until
/// [`solved`](Self::solved) sets it as the solve ends. Only its own methods make one: the
/// cost total checked against `i128::MAX`, the kept cost by wrapping arithmetic, the excess checked against the limit.
/// Where the root links carry nothing the cost lies within plus or minus the cost total, at most `i128::MAX`, so the
/// congruence is equality: `kept_cost` is the cost of the flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct KeptSums {
    /// The magnitudes of the costs times the capacities, added up, in units of cost times units of flow.
    cost_total: u128,
    /// The cost of the flow, root links counted, in units of cost times units of flow, modulo `2^128`.
    kept_cost: i128,
    /// The flow up the root links, in units of flow.
    excess: u64,
}

/// A change of one arc's cost, with what the arc can carry and carries: what the sums of a network follow. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CostChange {
    /// The cost the arc had, in units of cost.
    pub(super) replaced: i64,
    /// The cost it has once changed, in units of cost.
    pub(super) cost: i64,
    /// Its capacity, in units of flow.
    pub(super) capacity: u64,
    /// Its flow, its lower bound included, in units of flow.
    pub(super) flow: u64,
}

impl KeptSums {
    /// The sums of the problem of no node: no arc, no flow, no cost.
    pub(super) const OF_NO_NODE: Self = Self { cost_total: 0, kept_cost: 0, excess: 0 };

    /// The sums of a start of a problem whose cost total is `cost_total`: the arcs' flows, their lower bounds included,
    /// cost `arcs_cost` modulo `2^128`, and the root links that point down carry `flow_down` in all, each unit at
    /// `root_cost`. In the star every arc carries its lower bound; a kept basis re-flowed carries more on some.
    ///
    /// # Panics
    /// Panics if `flow_down` passes [`NET_SUPPLY_LIMIT`], which no start's does: it is at most the positive total of
    /// the start's excesses, which a start keeps within the limit.
    #[track_caller]
    pub(super) fn of_start(cost_total: u128, arcs_cost: i128, root_cost: i64, flow_down: u64) -> Self {
        let Some(excess) = excess_of(i128::from(flow_down)) else {
            panic!("called `KeptSums::of_start` with the flow down {flow_down}, past {NET_SUPPLY_LIMIT}")
        };

        // A root cost is at most `2^62` and the flow below `2^64`, so the product is below `2^126`, inside an `i128`;
        // only the sum with the arcs' cost can pass it, where the congruence is all the invariant asks.
        let root_links_cost = i128::from(root_cost) * i128::from(flow_down);

        // The root conserves: as much flows up the root links as down them.
        Self { cost_total, kept_cost: arcs_cost.wrapping_add(root_links_cost), excess }
    }

    /// The sums once a pivot has sent `sent` units of flow around the cycle of an arc whose violation was `violation`:
    /// the cost of the flow falls by the flow sent times the violation's magnitude. The excess is left as it is, at
    /// least the flow up the root links, which no pivot raises, until [`solved`](Self::solved).
    pub(super) fn pivoted(&mut self, sent: u64, violation: i64) {
        // Below `2^64 · 2^63 = 2^127`, inside an `i128`.
        let fall = i128::from(sent) * i128::from(violation.unsigned_abs());

        self.kept_cost = self.kept_cost.wrapping_sub(fall);
    }

    /// The sums once one arc's cost has changed as `change` says: the cost total without the arc's term at its old
    /// cost and with its term at its new one, and the kept cost moved by the change of the cost times the arc's flow.
    /// `None` where the cost total would pass `i128::MAX`. It reads, and changes nothing.
    ///
    /// # Panics
    /// Panics if the arc's term at its old cost, that cost's magnitude times its capacity, is more than the cost total
    /// held: the numbers of no arc of the problem the sums are of.
    #[track_caller]
    pub(super) fn cost_changed(&self, change: CostChange) -> Option<Self> {
        let CostChange { replaced, cost, capacity, flow } = change;
        let cost_total = self.cost_total_replacing([replaced, cost], [capacity, capacity])?;

        // The flow's cost moves by the change of the arc's cost times its flow: modulo `2^128`, as the invariant asks.
        let moved = i128::from(cost).wrapping_sub(i128::from(replaced)).wrapping_mul(i128::from(flow));

        Some(Self { kept_cost: self.kept_cost.wrapping_add(moved), cost_total, ..*self })
    }

    /// The sums once one arc's capacity, at the cost `cost`, has changed from `replaced` to `capacity`: the cost total
    /// without the arc's term at its old capacity and with its term at its new one. The kept cost is left as it is: a
    /// capacity moves no flow by itself, and what a change of it moves is added by [`rerouted`](Self::rerouted).
    /// `None` where the cost total would pass `i128::MAX`. It reads, and changes nothing.
    ///
    /// # Panics
    /// Panics if the arc's term at its old capacity, its cost's magnitude times that capacity, is more than the cost
    /// total held: the numbers of no arc of the problem the sums are of.
    #[track_caller]
    pub(super) fn capacity_changed(&self, cost: i64, replaced: u64, capacity: u64) -> Option<Self> {
        let cost_total = self.cost_total_replacing([cost, cost], [replaced, capacity])?;

        Some(Self { cost_total, ..*self })
    }

    /// The cost total with one arc's term at the cost `replaced_cost` and the capacity `replaced_capacity` replaced by
    /// its term at `cost` and `capacity`; `None` where it would pass `i128::MAX`. The one rule by which a change of an
    /// arc's numbers moves the cost total.
    ///
    /// # Panics
    /// Panics if the old term is more than the cost total held.
    #[track_caller]
    fn cost_total_replacing(
        &self,
        [replaced_cost, cost]: [i64; 2],
        [replaced_capacity, capacity]: [u64; 2],
    ) -> Option<u128> {
        // The arc's old term is part of the total, so for its numbers nothing is taken that is not there.
        let replaced = u128::from(replaced_cost.unsigned_abs()) * u128::from(replaced_capacity);

        cost_total_with(self.cost_total.strict_sub(replaced), cost, capacity)
    }

    /// Whether a change that moves `amount` units of flow keeps the flow up the root links within
    /// [`NET_SUPPLY_LIMIT`], as no change raises it by more than the flow it moves. It reads, and changes nothing.
    ///
    /// # Panics
    /// Panics if that sum passes `u128::MAX`, which no change's amount comes near: a move's is below `2^64`, a
    /// restatement's below `2^66`.
    ///
    /// # Errors
    /// Returns the flow up the root links plus `amount`, in units of flow, where it passes the limit.
    #[track_caller]
    pub(super) fn admit(&self, amount: u128) -> Result<(), u128> {
        let total = u128::from(self.excess).strict_add(amount);

        if total <= u128::from(NET_SUPPLY_LIMIT) { Ok(()) } else { Err(total) }
    }

    /// The sums once a route has moved the basis: the kept cost moved by `cost_gained`, what the flow's cost gained
    /// modulo `2^128`, and the excess by `up_gained`, what the flow up the root links gained.
    ///
    /// # Panics
    /// Panics if the excess would leave `0..=NET_SUPPLY_LIMIT`, which no route of an amount [`admit`](Self::admit)
    /// admitted leaves it.
    #[track_caller]
    pub(super) fn rerouted(&mut self, cost_gained: i128, up_gained: i128) {
        let excess = i128::from(self.excess).checked_add(up_gained).and_then(excess_of);
        let Some(excess) = excess else {
            panic!(
                "called `KeptSums::rerouted` with the gain {up_gained} of the excess {}, which leaves \
                 0..={NET_SUPPLY_LIMIT}",
                self.excess
            )
        };

        self.kept_cost = self.kept_cost.wrapping_add(cost_gained);
        self.excess = excess;
    }

    /// The sums once a solve has ended with `unrouted` units of flow up the root links: the excess.
    ///
    /// # Panics
    /// Panics if `unrouted` passes [`NET_SUPPLY_LIMIT`], which the flow up a basis's root links never does.
    #[track_caller]
    pub(super) fn solved(&mut self, unrouted: u64) {
        let Some(excess) = excess_of(i128::from(unrouted)) else {
            panic!("called `KeptSums::solved` with the flow up the root links {unrouted}, past {NET_SUPPLY_LIMIT}")
        };

        self.excess = excess;
    }

    /// The cost of the flow, root links counted, modulo `2^128`: the cost of the flow itself where the root links carry
    /// nothing.
    pub(super) fn kept_cost(&self) -> i128 {
        self.kept_cost
    }

    /// The magnitudes of the costs times the capacities, added up, in units of cost times units of flow: what the tests
    /// of a network check the total against.
    #[cfg(test)]
    pub(super) fn cost_total(&self) -> u128 {
        self.cost_total
    }

    /// The flow up the root links, in units of flow: what the tests of a network check the root links against.
    #[cfg(test)]
    pub(super) fn excess(&self) -> u64 {
        self.excess
    }
}

/// `excess` as the excess a network keeps, the flow up its root links in units of flow; `None` outside
/// `0..=NET_SUPPLY_LIMIT`. The one test of the excess against its limit, which every write of it makes.
fn excess_of(excess: i128) -> Option<u64> {
    u64::try_from(excess).ok().filter(|&excess| excess <= NET_SUPPLY_LIMIT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ranges::widest_root_cost;

    #[test]
    fn test_an_excess_lies_within_zero_and_the_limit_by_hand() {
        let limit = i128::from(NET_SUPPLY_LIMIT);
        assert_eq!([0, 5, limit].map(excess_of), [Some(0), Some(5), Some(NET_SUPPLY_LIMIT)]);
        assert_eq!([-1, limit + 1, i128::MAX, i128::MIN].map(excess_of), [None; 4]);
    }

    #[test]
    fn test_the_sums_of_no_node_and_of_a_start_by_hand() {
        assert_eq!(KeptSums::OF_NO_NODE, KeptSums { cost_total: 0, kept_cost: 0, excess: 0 });
        assert_eq!(KeptSums::of_start(30, 0, 11, 4), KeptSums { cost_total: 30, kept_cost: 44, excess: 4 });
        assert_eq!(KeptSums::of_start(0, -7, 11, 0).kept_cost(), -7);

        // The depot at the widest root cost of three nodes, `M = 3074457345618258603`, four units down: `4·M`.
        let widest = widest_root_cost(3);
        assert_eq!(KeptSums::of_start(21, 0, widest, 4).kept_cost(), 12_297_829_382_473_034_412);

        // The depot's optimum re-flowed with arc 1 switched off: the arcs carry `5·2 + 1·2 = 12`, two units come down
        // node 1's root link, and the cost total is 17: `12 + 2·M`.
        let start = KeptSums::of_start(17, 12, widest, 2);
        assert_eq!(start, KeptSums { cost_total: 17, kept_cost: 12 + 2 * i128::from(widest), excess: 2 });
    }

    #[test]
    fn test_a_pivot_lowers_the_kept_cost_by_the_flow_sent_times_the_violation_by_hand() {
        // The depot's pivots at its root cost 11: arc 0 fills, 3 units at `5 − 11`; arc 2 enters degenerately; arcs 1
        // and 0 then move one unit each at `1 − 10` and `−3`. From `4·11 = 44` to the optimum's 14.
        let mut sums = KeptSums::of_start(21, 0, 11, 4);
        for (sent, violation) in [(3, 5 - 11), (0, 1 - 11), (1, -9), (1, -3)] {
            sums.pivoted(sent, violation);
        }

        assert_eq!(sums, KeptSums { cost_total: 21, kept_cost: 44 - 18 - 9 - 3, excess: 4 });
    }

    #[test]
    fn test_the_kept_cost_wraps_past_i128_max_and_comes_back_by_hand() {
        // The arcs' cost at `i128::MAX` and one unit down a root link of cost 1: one past, wrapped to `i128::MIN`;
        // a pivot that sends that unit back at a violation of one restores it.
        let mut sums = KeptSums::of_start(0, i128::MAX, 1, 1);
        assert_eq!(sums.kept_cost(), i128::MIN);

        sums.pivoted(1, -1);
        assert_eq!(sums.kept_cost(), i128::MAX);
    }

    #[test]
    fn test_the_widest_pivot_and_star_fit_by_hand() {
        // The most flow a pivot sends times the widest violation: `(2^64 − 1)·2^63`, below `2^127`.
        let mut sums = KeptSums::of_start(0, 0, 0, 0);
        sums.pivoted(u64::MAX, i64::MIN);
        assert_eq!(sums.kept_cost(), -(i128::from(u64::MAX) << 63));

        // The widest star: `2^62` times `2^64 − 2` down, the most a start's root links carry.
        let star = KeptSums::of_start(0, 0, 1 << 62, NET_SUPPLY_LIMIT);
        assert_eq!((star.kept_cost(), star.excess()), (i128::from(NET_SUPPLY_LIMIT) << 62, NET_SUPPLY_LIMIT));
    }

    #[test]
    fn test_a_cost_changed_moves_the_total_and_the_kept_cost_by_hand() {
        // The depot's arc 2, of capacity 2 and carrying 2, from cost 1 to 3: the total `15 + 4 + 2 = 21` loses 2 and
        // gains 6; the optimum's 14 gains `(3 − 1)·2`.
        let sums = KeptSums { cost_total: 21, kept_cost: 14, excess: 0 };
        let change = CostChange { replaced: 1, cost: 3, capacity: 2, flow: 2 };
        assert_eq!(sums.cost_changed(change), Some(KeptSums { cost_total: 25, kept_cost: 18, excess: 0 }));

        // To a cost of the other sign, on an arc carrying nothing: the total moves by the magnitudes, the cost not.
        let change = CostChange { replaced: 1, cost: -7, capacity: 2, flow: 0 };
        assert_eq!(sums.cost_changed(change), Some(KeptSums { cost_total: 33, kept_cost: 14, excess: 0 }));

        // The same cost again: nothing moves.
        let change = CostChange { replaced: 5, cost: 5, capacity: 3, flow: 2 };
        assert_eq!(sums.cost_changed(change), Some(sums));
    }

    #[test]
    fn test_a_cost_changed_reaches_i128_max_and_is_refused_one_past_it_by_hand() {
        // An arc of capacity 2 at cost 1 in a total two short of `i128::MAX`: at cost 2 the total is `i128::MAX`
        // exactly; at cost 3 it would be two past it, each unit of cost being worth two. The flow of 1 makes the cost
        // dearer by one.
        let max = i128::MAX.cast_unsigned();
        let sums = KeptSums { cost_total: max - 2, kept_cost: 0, excess: 0 };
        let raised = CostChange { replaced: 1, cost: 2, capacity: 2, flow: 1 };
        assert_eq!(sums.cost_changed(raised), Some(KeptSums { cost_total: max, kept_cost: 1, excess: 0 }));
        assert_eq!(sums.cost_changed(CostChange { cost: 3, ..raised }), None);
        assert_eq!(sums.cost_changed(CostChange { cost: -3, ..raised }), None, "a magnitude, either sign");

        // An arc of capacity 1 in a total at `i128::MAX`: raised by one unit, one past it.
        let full = KeptSums { cost_total: max, kept_cost: 0, excess: 0 };
        let one_more = CostChange { replaced: 4, cost: 5, capacity: 1, flow: 0 };
        assert_eq!(full.cost_changed(one_more), None);
        assert_eq!(full.cost_changed(CostChange { cost: -4, ..one_more }), Some(full));
    }

    #[test]
    fn test_a_cost_changed_wraps_the_kept_cost_by_hand() {
        // A kept cost at `i128::MAX` with a unit of flow made dearer by one: wrapped to `i128::MIN`, as a pivot wraps.
        let sums = KeptSums { cost_total: 10, kept_cost: i128::MAX, excess: 0 };
        let change = CostChange { replaced: 0, cost: 1, capacity: 10, flow: 1 };
        assert_eq!(sums.cost_changed(change), Some(KeptSums { cost_total: 20, kept_cost: i128::MIN, excess: 0 }));

        // The widest change: from `−B` to `B` at three nodes, `B = 1537228672809129301`, on `2^64 − 1` units.
        let bound = 1_537_228_672_809_129_301_i64;
        let wide =
            KeptSums { cost_total: u128::from(bound.unsigned_abs()) * u128::from(u64::MAX), kept_cost: 0, excess: 0 };
        let change = CostChange { replaced: -bound, cost: bound, capacity: u64::MAX, flow: u64::MAX };
        let moved = 2 * i128::from(bound) * i128::from(u64::MAX);
        assert_eq!(wide.cost_changed(change), Some(KeptSums { kept_cost: moved, ..wide }));
    }

    #[test]
    fn test_a_capacity_changed_moves_the_total_alone_by_hand() {
        // The depot's arc 0 at cost 5, carrying 2 of its capacity 3: to 1, the total `15 + 4 + 2 = 21` loses `5·2`; to
        // 7 it gains `5·4`; the kept cost does not move. An arc of a negative cost counts by its magnitude.
        let sums = KeptSums { cost_total: 21, kept_cost: 14, excess: 1 };
        assert_eq!(sums.capacity_changed(5, 3, 1), Some(KeptSums { cost_total: 11, kept_cost: 14, excess: 1 }));
        assert_eq!(sums.capacity_changed(5, 3, 7), Some(KeptSums { cost_total: 41, kept_cost: 14, excess: 1 }));
        assert_eq!(sums.capacity_changed(-5, 3, 0), Some(KeptSums { cost_total: 6, ..sums }));
        assert_eq!(sums.capacity_changed(5, 3, 3), Some(sums), "the same capacity: nothing moves");
    }

    #[test]
    fn test_a_capacity_changed_reaches_i128_max_and_is_refused_one_past_it_by_hand() {
        // An arc of cost 2 and capacity 1 in a total two short of `i128::MAX`: at capacity 2 the total is `i128::MAX`
        // exactly; at 3 it would be two past it. Lowering a capacity is never refused.
        let max = i128::MAX.cast_unsigned();
        let sums = KeptSums { cost_total: max - 2, kept_cost: 0, excess: 0 };
        assert_eq!(sums.capacity_changed(2, 1, 2), Some(KeptSums { cost_total: max, ..sums }));
        assert_eq!(sums.capacity_changed(-2, 1, 3), None);
        assert_eq!(sums.capacity_changed(2, 1, 0), Some(KeptSums { cost_total: max - 4, ..sums }));

        // The widest arc, `2^62 − 1` times `2^64 − 1`, alone in a total that holds its term.
        let (cost, wide) = ((1_i64 << 62) - 1, u64::MAX);
        let term = u128::from(cost.unsigned_abs()) * u128::from(wide);
        let alone = KeptSums { cost_total: term, kept_cost: 0, excess: 0 };
        assert_eq!(alone.capacity_changed(cost, wide, 0), Some(KeptSums { cost_total: 0, ..alone }));
    }

    #[test]
    #[should_panic(expected = "attempt to subtract with overflow")]
    fn test_a_capacity_changed_refuses_an_old_term_past_the_total() {
        let sums = KeptSums { cost_total: 5, kept_cost: 0, excess: 0 };
        let _ = sums.capacity_changed(3, 2, 1);
    }

    #[test]
    #[should_panic(expected = "attempt to subtract with overflow")]
    fn test_a_cost_changed_refuses_an_old_term_past_the_total() {
        let sums = KeptSums { cost_total: 5, kept_cost: 0, excess: 0 };
        let _ = sums.cost_changed(CostChange { replaced: 3, cost: 1, capacity: 2, flow: 0 });
    }

    #[test]
    #[should_panic(expected = "called `KeptSums::of_start` with the flow down 18446744073709551615, past \
                               18446744073709551614")]
    fn test_a_start_refuses_a_flow_down_past_the_limit() {
        let _ = KeptSums::of_start(0, 0, 1, u64::MAX);
    }

    #[test]
    fn test_the_excess_admits_an_amount_up_to_the_limit_and_refuses_one_past_it_by_hand() {
        // Five units up the root links: an amount of `2^64 − 7` reaches the limit `2^64 − 2`, one more passes it.
        let sums = KeptSums { cost_total: 0, kept_cost: 0, excess: 5 };
        let max = u128::from(u64::MAX);
        assert_eq!(sums.admit(0), Ok(()));
        assert_eq!(sums.admit(max - 6), Ok(()));
        assert_eq!(sums.admit(max - 5), Err(max));
        assert_eq!(sums.admit(max), Err(max + 5));

        // Nothing up the root links: the largest move is one past the limit; a restatement may move more.
        assert_eq!(KeptSums::OF_NO_NODE.admit(max), Err(max));
        assert_eq!(KeptSums::OF_NO_NODE.admit(u128::from(NET_SUPPLY_LIMIT)), Ok(()));
        assert_eq!(sums.admit(1 << 66), Err((1 << 66) + 5));
    }

    #[test]
    fn test_a_route_moves_the_kept_cost_and_the_excess_by_what_they_gained_by_hand() {
        // Five units up; a route gains 3 up and 7 in cost, then gives back 8 up at the cost of 2.
        let mut sums = KeptSums { cost_total: 9, kept_cost: 40, excess: 5 };
        sums.rerouted(7, 3);
        assert_eq!(sums, KeptSums { cost_total: 9, kept_cost: 47, excess: 8 });
        sums.rerouted(-2, -8);
        assert_eq!(sums, KeptSums { cost_total: 9, kept_cost: 45, excess: 0 });

        // The kept cost wraps, as a pivot's does; the excess reaches the limit exactly.
        let mut sums = KeptSums { cost_total: 0, kept_cost: i128::MAX, excess: 0 };
        sums.rerouted(1, i128::from(NET_SUPPLY_LIMIT));
        assert_eq!(sums, KeptSums { cost_total: 0, kept_cost: i128::MIN, excess: NET_SUPPLY_LIMIT });
    }

    #[test]
    #[should_panic(expected = "called `KeptSums::rerouted` with the gain -6 of the excess 5, which leaves \
                               0..=18446744073709551614")]
    fn test_a_route_refuses_an_excess_below_zero() {
        KeptSums { cost_total: 0, kept_cost: 0, excess: 5 }.rerouted(0, -6);
    }

    #[test]
    #[should_panic(expected = "called `KeptSums::rerouted` with the gain 1 of the excess 18446744073709551614, which \
                               leaves 0..=18446744073709551614")]
    fn test_a_route_refuses_an_excess_past_the_limit() {
        KeptSums { cost_total: 0, kept_cost: 0, excess: NET_SUPPLY_LIMIT }.rerouted(0, 1);
    }

    #[test]
    fn test_a_solve_sets_the_excess_to_the_flow_up_the_root_links_by_hand() {
        let mut sums = KeptSums { cost_total: 3, kept_cost: -4, excess: 9 };
        sums.solved(2);
        assert_eq!(sums, KeptSums { cost_total: 3, kept_cost: -4, excess: 2 });
        sums.solved(0);
        assert_eq!(sums.excess(), 0);
        sums.solved(NET_SUPPLY_LIMIT);
        assert_eq!(sums.excess(), NET_SUPPLY_LIMIT);
    }

    #[test]
    #[should_panic(expected = "called `KeptSums::solved` with the flow up the root links 18446744073709551615, past \
                               18446744073709551614")]
    fn test_a_solve_refuses_an_excess_past_the_limit() {
        let mut sums = KeptSums::OF_NO_NODE;
        sums.solved(u64::MAX);
    }
}
