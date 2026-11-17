// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The min-cost model of last resort: every integral flow of a tiny network tried in turn.
//!
//! It knows no potential, no tree and no residual network: a flow is feasible if it lies within the bounds and leaves
//! no node an excess, and the optimum is the least cost of the feasible flows. So it shares no reasoning with a solver
//! checked against it.

use itertools::izip;

use crate::certificate::{CertificateBreach, check_certificate};
use crate::mincost_network::{MincostNetwork, ShapedNetwork};

/// The most flows the brute force tries: three an arc, the most an arc of a tiny network admits with its width of at
/// most two, on seven arcs, the most a test of the workspace draws: `3^7`.
pub const BRUTE_FORCE_FLOWS: u128 = 3_u128.pow(7);

/// The least cost of an integral flow of `network` that lies within its bounds and meets every supply, found by
/// trying every flow; `None` where no such flow exists.
///
/// It tries the product of the arcs' ranges `lower..=capacity`, so it is for networks of a handful of arcs with narrow
/// ranges. A lower bound above its capacity admits no flow. The cost is exact in `i128`, which the bound it checks
/// first guarantees: the magnitudes of the costs, each times its arc's capacity, sum to at most `i128::MAX`.
///
/// # Panics
/// Panics if the shape of `network` is broken, if the magnitudes of its costs at capacity sum past `i128::MAX`, or if
/// its arcs of ordered bounds admit more than [`BRUTE_FORCE_FLOWS`] flows together: the product of `capacity −
/// lower + 1` over them.
pub fn brute_force_optimum(network: &MincostNetwork) -> Option<i128> {
    let shaped = network
        .shaped()
        .unwrap_or_else(|breach| panic!("called `brute_force_optimum` on a network of broken shape: {breach}"));

    if let Err(beyond) = admission(shaped) {
        panic!("called `brute_force_optimum` on {beyond}");
    }

    let mut odometer = Odometer::new(shaped)?;
    let mut excesses = Vec::with_capacity(shaped.node_count());
    let mut least = None;

    loop {
        if is_balanced(shaped, odometer.flows(), &mut excesses) {
            let cost = shaped.cost_of(odometer.flows());
            least = Some(least.map_or(cost, |least: i128| least.min(cost)));
        }
        if !odometer.advance() {
            return least;
        }
    }
}

/// Whether [`brute_force_optimum`] tries `network`: its costs' magnitudes at capacity sum to at most `i128::MAX`, and
/// its arcs of ordered bounds admit at most [`BRUTE_FORCE_FLOWS`] flows together, the product of `capacity − lower + 1`
/// over them. The one statement of what the brute force admits, which a test that draws past it asks first.
///
/// # Panics
/// Panics if the shape of `network` is broken.
pub fn brute_force_admits(network: &MincostNetwork) -> bool {
    let shaped = network
        .shaped()
        .unwrap_or_else(|breach| panic!("called `brute_force_admits` on a network of broken shape: {breach}"));

    admission(shaped).is_ok()
}

/// Why the brute force does not try a network of sound shape, with the values that say so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Beyond {
    /// The magnitudes of the costs at capacity sum past `i128::MAX` at this arc.
    CostTotal {
        /// The arc at which the running sum passes.
        arc: usize,
    },
    /// The arcs of ordered bounds admit more flows together than [`BRUTE_FORCE_FLOWS`].
    Flows {
        /// The number of flows they admit, `u128::MAX` past it.
        flows: u128,
    },
}

impl core::fmt::Display for Beyond {
    /// The condition, as it follows "called `brute_force_optimum` on" in a panic.
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::CostTotal { arc } => {
                write!(formatter, "costs whose magnitudes at capacity sum past i128::MAX at arc {arc}")
            }
            Self::Flows { flows } => {
                write!(formatter, "a network of {flows} flows, which should be at most {BRUTE_FORCE_FLOWS}")
            }
        }
    }
}

/// Whether the brute force tries `network`, and why not where it does not: the one statement of its two conditions.
///
/// # Errors
/// [`Beyond::CostTotal`] where the costs' magnitudes at capacity sum past `i128::MAX`, else [`Beyond::Flows`] where the
/// arcs of ordered bounds admit more than [`BRUTE_FORCE_FLOWS`] flows.
fn admission(network: ShapedNetwork<'_>) -> Result<(), Beyond> {
    if let Some(arc) = network.arc_past_cost_total(i128::MAX.cast_unsigned()) {
        return Err(Beyond::CostTotal { arc });
    }
    let flows = flow_count(network);

    if flows <= BRUTE_FORCE_FLOWS { Ok(()) } else { Err(Beyond::Flows { flows }) }
}

/// An optimal answer of a min-cost solve as plain numbers: the flows, the potentials that price them optimal, and their
/// cost. Plain data.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct OptimalFlow {
    /// The flow of each arc, in units of flow, one per arc.
    pub flows: Vec<u64>,
    /// Each node's potential, in units of cost, one per node.
    pub potentials: Vec<i64>,
    /// The cost of the flows, in units of cost times units of flow.
    pub total_cost: i128,
}

/// How an answer fails [`check_against_brute_force`]. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum BruteForceBreach {
    /// The answer says no flow exists, but the brute force finds one.
    #[error("no flow is answered, but the least cost of a flow is {best}")]
    MissesAFlow {
        /// The least cost of a flow, in units of cost times units of flow.
        best: i128,
    },
    /// The answer gives a flow where the brute force finds none.
    #[error("a flow is answered where none exists")]
    FlowWhereNoneExists,
    /// The answer's cost is not the least.
    #[error("the answered cost is {answered}, but the least is {best}")]
    MissesTheOptimum {
        /// The cost answered.
        answered: i128,
        /// The least cost of a flow.
        best: i128,
    },
    /// The answer has the least cost but fails the certificate.
    #[error("the answer fails its certificate: {0}")]
    Certificate(#[source] CertificateBreach),
}

/// Checks `answer`, what a solve answered for `network`, against the brute force: `None` exactly where no flow exists,
/// and otherwise the least cost, with flows and potentials that pass [`check_certificate`].
///
/// # Errors
/// Returns the first breach in the order of [`BruteForceBreach`]'s variants that applies.
///
/// # Panics
/// As [`brute_force_optimum`], and as [`check_certificate`] where `answer` holds another number of flows or
/// potentials than `network` has arcs or nodes.
pub fn check_against_brute_force(
    network: &MincostNetwork,
    answer: Option<&OptimalFlow>,
) -> Result<(), BruteForceBreach> {
    match (answer, brute_force_optimum(network)) {
        (None, None) => Ok(()),
        (None, Some(best)) => Err(BruteForceBreach::MissesAFlow { best }),
        (Some(_), None) => Err(BruteForceBreach::FlowWhereNoneExists),
        (Some(answer), Some(best)) if answer.total_cost != best => {
            Err(BruteForceBreach::MissesTheOptimum { answered: answer.total_cost, best })
        }
        (Some(answer), Some(_)) => check_certificate(network, &answer.flows, &answer.potentials, answer.total_cost)
            .map_err(BruteForceBreach::Certificate),
    }
}

/// The number of flows the arcs of `network` with ordered bounds admit together: the product of `capacity − lower + 1`
/// over them, `u128::MAX` past it.
fn flow_count(network: ShapedNetwork<'_>) -> u128 {
    let bounds = network.lower_bounds().iter().zip(network.capacities());

    bounds
        .filter_map(|(&lower, &capacity)| capacity.checked_sub(lower))
        .fold(1, |count, width| count.saturating_mul(u128::from(width) + 1))
}

/// Whether `flows`, one per arc, leaves no node of `network` an excess; `excesses` is the room the excesses are
/// worked out in.
///
/// # Panics
/// Panics if `flows` does not hold one value per arc.
fn is_balanced(network: ShapedNetwork<'_>, flows: &[u64], excesses: &mut Vec<i128>) -> bool {
    network.excesses_into(flows, excesses);

    excesses.iter().all(|&excess| excess == 0)
}

/// Every integral flow within the bounds of a network, one at a time, in odometer order: the first arc turning
/// fastest.
///
/// # Invariant
/// `flows`, `lower_bounds` and `capacities` hold one entry per arc of one shaped network, and every flow lies within
/// its arc's bounds.
#[derive(Clone, Debug)]
struct Odometer<'n> {
    /// The flow it stands at, one per arc, in units of flow.
    flows: Vec<u64>,
    /// The least flow of each arc.
    lower_bounds: &'n [u64],
    /// The most flow of each arc.
    capacities: &'n [u64],
}

impl<'n> Odometer<'n> {
    /// The odometer of `network`'s bounds, at the flow of every arc at its lower bound; `None` where some lower bound
    /// passes its capacity, so that no flow lies within the bounds.
    fn new(network: ShapedNetwork<'n>) -> Option<Self> {
        let (lower_bounds, capacities) = (network.lower_bounds(), network.capacities());
        let ordered = lower_bounds.iter().zip(capacities).all(|(lower, capacity)| lower <= capacity);
        ordered.then(|| Self { flows: lower_bounds.to_vec(), lower_bounds, capacities })
    }

    /// The flow it stands at, one per arc.
    fn flows(&self) -> &[u64] {
        &self.flows
    }

    /// Moves to the next flow; answers whether there was one. Past the last flow it wraps every arc back to its lower
    /// bound and answers `false`.
    fn advance(&mut self) -> bool {
        for (flow, &lower, &capacity) in izip!(&mut self.flows, self.lower_bounds, self.capacities) {
            if *flow < capacity {
                *flow += 1;
                return true;
            }
            *flow = lower;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The view of `network`, whose shape the test knows to hold.
    fn shaped(network: &MincostNetwork) -> ShapedNetwork<'_> {
        network.shaped().expect("the test's network should be of sound shape")
    }

    /// Two parallel arcs from node 0 to node 1 of capacity 1, costing 2 and 3, with 1 to send.
    fn parallel() -> MincostNetwork {
        let mut network = MincostNetwork::default();
        let (from, to) = (network.add_node(1), network.add_node(-1));
        network.add_arc(from, to, 0, 1, 2);
        network.add_arc(from, to, 0, 1, 3);

        network
    }

    #[test]
    fn test_the_brute_force_admits_up_to_its_limit_of_flows_by_hand() {
        // Seven loops of width 2 at one node: `3^7`, the limit. One widened to 3: `4·3^6`, past it. No arc: one flow.
        let at_the_limit = MincostNetwork::from_arcs(&[0], &[(0, 0, 0, 2, 1); 7]);
        assert!(brute_force_admits(&at_the_limit));
        assert_eq!(brute_force_optimum(&at_the_limit), Some(0));

        let mut past = at_the_limit.clone();
        past.capacities[3] = 3;
        assert!(!brute_force_admits(&past));
        assert!(brute_force_admits(&MincostNetwork::from_arcs(&[0, 0], &[])));

        // An arc whose lower bound passes its capacity admits no flow and counts for none.
        let mut crossed = past.clone();
        crossed.lower_bounds[3] = 4;
        assert!(brute_force_admits(&crossed));
    }

    #[test]
    fn test_the_admission_names_the_condition_not_met_by_hand() {
        let dear = (0, 1, u64::MAX - 1, u64::MAX, i64::MAX);
        let twice = MincostNetwork::from_arcs(&[0, 0], &[dear, dear]);
        assert_eq!(admission(shaped(&twice)), Err(Beyond::CostTotal { arc: 1 }));
        let wide = MincostNetwork::from_arcs(&[0], &[(0, 0, 0, 2, 1); 8]);
        assert_eq!(admission(shaped(&wide)), Err(Beyond::Flows { flows: 3_u128.pow(8) }));
        assert_eq!(admission(shaped(&MincostNetwork::from_arcs(&[0], &[(0, 0, 0, 2, 1); 7]))), Ok(()));
        assert_eq!(Beyond::Flows { flows: 6561 }.to_string(), "a network of 6561 flows, which should be at most 2187");
        assert_eq!(
            Beyond::CostTotal { arc: 1 }.to_string(),
            "costs whose magnitudes at capacity sum past i128::MAX at arc 1"
        );
    }

    #[test]
    fn test_the_brute_force_admits_no_costs_whose_magnitudes_at_capacity_pass_i128_max_by_hand() {
        // Arcs of width 1 at the top of `u64`, costing `i64::MAX`: two flows each, so the count admits them; one costs
        // `(2^63 − 1)·(2^64 − 1)` at capacity, below `i128::MAX`, and two pass it.
        let dear = (0, 1, u64::MAX - 1, u64::MAX, i64::MAX);
        assert!(brute_force_admits(&MincostNetwork::from_arcs(&[0, 0], &[dear])));
        assert!(!brute_force_admits(&MincostNetwork::from_arcs(&[0, 0], &[dear, dear])));
    }

    #[test]
    fn test_the_cheaper_of_two_parallel_arcs_carries_the_unit() {
        assert_eq!(brute_force_optimum(&parallel()), Some(2));

        let mut swapped = parallel();
        swapped.costs = vec![4, -1];
        assert_eq!(brute_force_optimum(&swapped), Some(-1));
    }

    #[test]
    fn test_a_negative_cost_fills_room_no_supply_needs() {
        // At cost -2 the second arc carries the unit, and a loop at node 0 of cost -1 and room 2 is filled: -2 - 2.
        let mut gaining = parallel();
        gaining.costs[1] = -2;
        gaining.add_arc(0, 0, 0, 2, -1);
        assert_eq!(brute_force_optimum(&gaining), Some(-4));

        // Arc 1 at cost -2 carries the unit, and one more unit goes around the cycle back over arc 2 at cost 0: -4.
        let mut cycle = parallel();
        cycle.costs[1] = -2;
        cycle.capacities[1] = 2;
        cycle.add_arc(1, 0, 0, 1, 0);
        assert_eq!(brute_force_optimum(&cycle), Some(-4));
    }

    #[test]
    fn test_a_lower_bound_is_carried_whatever_it_costs() {
        // A loop of cost 5 with lower bound 2 carries 2 at 10, beside the unit on arc 0 at 2.
        let mut bound = parallel();
        bound.add_arc(1, 1, 2, 3, 5);
        assert_eq!(brute_force_optimum(&bound), Some(12));

        // A lower bound of 1 on the dearer arc forces the unit there.
        let mut forced = parallel();
        forced.lower_bounds[1] = 1;
        assert_eq!(brute_force_optimum(&forced), Some(3));
    }

    #[test]
    fn test_lower_bounds_can_turn_a_supplier_into_a_demander() {
        // Arc 1 -> 0 must carry 3, so node 0, which supplies 1, sends 4 over arc 0 -> 1: 4 at cost 1, 3 at cost 2.
        let mut network = MincostNetwork::default();
        let (first, second) = (network.add_node(1), network.add_node(-1));
        network.add_arc(first, second, 0, 5, 1);
        network.add_arc(second, first, 3, 3, 2);

        assert_eq!(brute_force_optimum(&network), Some(4 + 6));
    }

    #[test]
    fn test_infeasible_supplies_and_bounds() {
        let mut short = parallel();
        short.supplies = vec![3, -3];
        assert_eq!(brute_force_optimum(&short), None, "two arcs of capacity 1 cannot carry 3");

        let mut inverted = parallel();
        inverted.lower_bounds[0] = 2;
        assert_eq!(brute_force_optimum(&inverted), None, "a lower bound above its capacity");

        let mut forced = parallel();
        forced.lower_bounds = vec![1, 1];
        assert_eq!(brute_force_optimum(&forced), None, "lower bounds that send more than the supply");

        let mut without_arcs = MincostNetwork::default();
        without_arcs.add_node(1);
        without_arcs.add_node(-1);

        assert_eq!(brute_force_optimum(&without_arcs), None);
    }

    #[test]
    fn test_a_network_without_arcs_and_balanced_supplies_has_the_empty_flow() {
        let mut network = MincostNetwork::default();
        network.add_node(0);

        assert_eq!(brute_force_optimum(&network), Some(0));
        assert_eq!(brute_force_optimum(&MincostNetwork::default()), Some(0));
    }

    #[test]
    fn test_costs_at_the_i128_bound_are_exact() {
        // A loop of cost i64::MIN fills up to u64::MAX beside a loop of cost 1 fixed at 2^63 - 1: their magnitudes at
        // capacity sum to exactly i128::MAX, and the least cost is -(2^127 - 2^63) + 2^63 - 1.
        let half = (1_u64 << 63) - 1;
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 0, u64::MAX - 1, u64::MAX, i64::MIN);
        network.add_arc(0, 0, half, half, 1);

        let least = i128::from(i64::MIN) * i128::from(u64::MAX) + i128::from(half);
        assert_eq!(brute_force_optimum(&network), Some(least));
    }

    #[test]
    #[should_panic(
        expected = "called `brute_force_optimum` on costs whose magnitudes at capacity sum past i128::MAX at arc 1"
    )]
    fn test_costs_one_past_the_i128_bound_are_refused() {
        let half = 1_u64 << 63;
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 0, u64::MAX - 1, u64::MAX, i64::MIN);
        network.add_arc(0, 0, half, half, 1);

        let _ = brute_force_optimum(&network);
    }

    #[test]
    #[should_panic(
        expected = "called `brute_force_optimum` on a network of broken shape: the costs hold 1 entries for 2 \
                    arcs"
    )]
    fn test_a_broken_shape_is_refused() {
        let mut short = parallel();
        short.costs.pop();

        let _ = brute_force_optimum(&short);
    }

    /// A network of one node with a loop of width `width` at cost zero.
    fn one_loop(width: u64) -> MincostNetwork {
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 0, 3, 3 + width, 0);

        network
    }

    #[test]
    fn test_the_flows_are_counted_over_the_ordered_arcs_by_hand() {
        let mut network = parallel();
        network.add_arc(0, 1, 1, 3, 0);
        assert_eq!(flow_count(shaped(&network)), 2 * 2 * 3);

        network.add_arc(0, 1, 2, 1, 0);
        assert_eq!(flow_count(shaped(&network)), 2 * 2 * 3, "an arc of inverted bounds is not counted");

        network.add_arc(0, 0, 0, u64::MAX, 0);
        network.add_arc(0, 0, 0, u64::MAX, 0);
        network.add_arc(0, 0, 0, u64::MAX, 0);
        assert_eq!(flow_count(shaped(&network)), u128::MAX, "the count saturates");
    }

    #[test]
    fn test_the_brute_force_admits_exactly_its_most_flows() {
        let width = u64::try_from(BRUTE_FORCE_FLOWS - 1).expect("the most flows should fit a u64");

        assert_eq!(brute_force_optimum(&one_loop(width)), Some(0));
    }

    #[test]
    #[should_panic(expected = "called `brute_force_optimum` on a network of 2188 flows, which should be at most 2187")]
    fn test_the_brute_force_refuses_one_flow_past_its_most() {
        let width = u64::try_from(BRUTE_FORCE_FLOWS).expect("the most flows should fit a u64");

        let _ = brute_force_optimum(&one_loop(width));
    }

    #[test]
    fn test_is_balanced_wants_every_excess_zero() {
        let network = parallel();
        let mut excesses = vec![9; 7];

        assert!(is_balanced(shaped(&network), &[1, 0], &mut excesses));
        assert!(is_balanced(shaped(&network), &[0, 1], &mut excesses));
        assert!(!is_balanced(shaped(&network), &[1, 1], &mut excesses), "node 0 sends 2 of its 1");
        assert!(!is_balanced(shaped(&network), &[0, 0], &mut excesses), "node 0 keeps its 1");
        assert_eq!(excesses, [1, -1], "the room holds the last flow's excesses");
    }

    #[test]
    fn test_the_odometer_steps_through_every_flow_and_wraps_to_the_lower_bounds() {
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 0, 0, 1, 0);
        network.add_arc(0, 0, 1, 2, 0);

        let mut odometer = Odometer::new(shaped(&network)).expect("the bounds should be ordered");

        let mut seen = vec![odometer.flows().to_vec()];
        while odometer.advance() {
            seen.push(odometer.flows().to_vec());
        }
        assert_eq!(seen, [[0, 1], [1, 1], [0, 2], [1, 2]]);
        assert_eq!(odometer.flows(), [0, 1], "the last step wraps every arc back to its lower bound");

        let empty = MincostNetwork::default();
        let mut odometer = Odometer::new(shaped(&empty)).expect("an empty network should have no inverted bounds");
        assert!(!odometer.advance(), "an empty network has the one empty flow");
    }

    #[test]
    fn test_answers_are_checked_against_the_brute_force_by_hand() {
        // The unit goes by the arc of cost 2; potentials 0 and 2 price it optimal.
        let network = parallel();
        let right = OptimalFlow { flows: vec![1, 0], potentials: vec![0, 2], total_cost: 2 };
        assert_eq!(check_against_brute_force(&network, Some(&right)), Ok(()));

        assert_eq!(check_against_brute_force(&network, None), Err(BruteForceBreach::MissesAFlow { best: 2 }));
        let dear = OptimalFlow { flows: vec![0, 1], total_cost: 3, ..right.clone() };
        let missed = BruteForceBreach::MissesTheOptimum { answered: 3, best: 2 };
        assert_eq!(check_against_brute_force(&network, Some(&dear)), Err(missed));
        let unproven = OptimalFlow { potentials: vec![0, 9], ..right.clone() };
        let breach = check_against_brute_force(&network, Some(&unproven));
        assert!(matches!(breach, Err(BruteForceBreach::Certificate(_))), "{breach:?}");

        let without = MincostNetwork { supplies: vec![3, -3], ..network };
        assert_eq!(check_against_brute_force(&without, None), Ok(()));
        let invented = OptimalFlow { flows: vec![1, 1], potentials: vec![0, 0], total_cost: 5 };
        assert_eq!(check_against_brute_force(&without, Some(&invented)), Err(BruteForceBreach::FlowWhereNoneExists));
        assert_eq!(
            BruteForceBreach::MissesTheOptimum { answered: 3, best: 2 }.to_string(),
            "the answered cost is 3, but the least is 2"
        );
    }

    #[test]
    fn test_the_odometer_refuses_inverted_bounds() {
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 0, 1, 1, 0);
        assert!(Odometer::new(shaped(&network)).is_some(), "a fixed arc is ordered");

        network.add_arc(0, 0, 2, 1, 0);
        assert!(Odometer::new(shaped(&network)).is_none());
    }
}
