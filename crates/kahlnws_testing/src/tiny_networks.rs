// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tiny min-cost networks for property tests, few enough nodes and arcs for the brute force, with the arc count, the
//! costs, whether infeasible supplies occur and whether a lower bound may pass its capacity chosen by the test that
//! draws them.

use core::ops::RangeInclusive;

use proptest::collection::vec;
use proptest::prelude::*;

use crate::mincost_network::MincostNetwork;

/// What a tiny min-cost network is drawn from. Every network has one to four nodes and arcs between any two of them,
/// self-loops and parallel arcs among them, each of lower bound 0 or 1 and width 0 to 2, so fixed arcs and arcs
/// without room occur; its supplies always add up to zero; the fields choose the rest. Plain data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TinyMincostNetworks {
    /// How many arcs a network has.
    pub arcs: RangeInclusive<usize>,
    /// How the cost of every arc is drawn.
    pub costs: TinyCosts,
    /// How the supplies are drawn.
    pub supplies: TinySupplies,
    /// Whether a lower bound may pass its capacity.
    pub bounds: TinyBounds,
}

/// How the cost of every arc of a tiny network is drawn, in units of cost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TinyCosts {
    /// Evenly from the range.
    Uniform(RangeInclusive<i64>),
    /// Zero half the time and evenly from the range otherwise, so that optima tie often.
    ZeroOrUniform(RangeInclusive<i64>),
}

/// How the supplies of a tiny network are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TinySupplies {
    /// Always the net outflows of a flow within the bounds, so that every network with ordered bounds is feasible.
    OfAFlow,
    /// Half the time the net outflows of a flow within the bounds; the other half -3 to 3 at every node but node 0,
    /// which takes what balances them, so that a network is often infeasible.
    OfAFlowOrRandom,
}

/// Whether the arcs of a tiny network may have a lower bound above their capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TinyBounds {
    /// Every lower bound at most its capacity.
    Ordered,
    /// About one arc in six with a lower bound of 1 or 2, one above its capacity, so that its network has no flow
    /// within its bounds whatever its supplies.
    OrInverted,
}

impl TinyMincostNetworks {
    /// The strategy that draws networks as `self` describes them. The flow whose net outflows give the supplies
    /// carries `lower + pick % (width + 1)` on every arc of ordered bounds, `pick` drawn from 0 to 3, and its lower
    /// bound on an arc of inverted bounds.
    pub fn strategy(self) -> impl Strategy<Value = MincostNetwork> {
        let Self { arcs, costs, supplies, bounds } = self;
        let (fewest, most) = arcs.into_inner();

        (1_usize..=4)
            .prop_flat_map(move |nodes| {
                let drawn = vec(DrawnArc::strategy(nodes, costs.strategy(), bounds.strategy()), fewest..=most);
                (Just(nodes), drawn, supplies.strategy(nodes))
            })
            .prop_map(|(nodes, arcs, random)| network_of(nodes, &arcs, random))
    }
}

impl TinyCosts {
    /// The strategy of one arc's cost.
    fn strategy(&self) -> BoxedStrategy<i64> {
        match self {
            Self::Uniform(costs) => (*costs.start()..=*costs.end()).boxed(),
            Self::ZeroOrUniform(costs) => prop_oneof![Just(0), *costs.start()..=*costs.end()].boxed(),
        }
    }
}

impl TinyBounds {
    /// The strategy of whether one arc's bounds are inverted.
    fn strategy(self) -> BoxedStrategy<bool> {
        match self {
            Self::Ordered => Just(false).boxed(),
            Self::OrInverted => prop_oneof![5 => Just(false), 1 => Just(true)].boxed(),
        }
    }
}

impl TinySupplies {
    /// The strategy of the supplies of the nodes `1..nodes`, or `None` where the supplies are a flow's.
    fn strategy(self, nodes: usize) -> BoxedStrategy<Option<Vec<i64>>> {
        match self {
            Self::OfAFlow => Just(None).boxed(),
            Self::OfAFlowOrRandom => proptest::option::of(vec(-3_i64..=3, nodes - 1)).boxed(),
        }
    }
}

/// The tiny networks the solver's tests draw: one to four nodes and up to seven arcs, the most the brute force
/// enumerates, costs drawn by `costs`, bounds as `bounds` says, and supplies half the time a flow's and half the time
/// drawn, so that a network is often infeasible.
pub fn tiny_networks(costs: TinyCosts, bounds: TinyBounds) -> impl Strategy<Value = MincostNetwork> {
    TinyMincostNetworks { arcs: 0..=7, costs, supplies: TinySupplies::OfAFlowOrRandom, bounds }.strategy()
}

/// One arc as drawn, with the flow it carries in the flow the supplies may be the net outflows of. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DrawnArc {
    /// The node it leaves.
    tail: usize,
    /// The node it enters.
    head: usize,
    /// Its least flow: 0 or 1 where its bounds are ordered, 1 or 2 where they are inverted.
    lower_bound: u64,
    /// Its most flow: up to 2 above its lower bound where its bounds are ordered, 1 below it where they are inverted.
    capacity: u64,
    /// Its cost of a unit of flow.
    cost: i64,
    /// Its flow: within `lower_bound..=capacity` where the bounds are ordered; the lower bound where they are not.
    flow: u64,
}

impl DrawnArc {
    /// The strategy of one arc between nodes of `0..nodes`, its cost drawn by `cost` and whether its bounds are
    /// inverted by `inverted`.
    fn strategy(nodes: usize, cost: BoxedStrategy<i64>, inverted: BoxedStrategy<bool>) -> impl Strategy<Value = Self> {
        (0..nodes, 0..nodes, 0_u64..=1, 0_u64..=2, cost, 0_u64..=3, inverted).prop_map(
            |(tail, head, lower_bound, width, cost, pick, inverted)| {
                if inverted {
                    let lower_bound = lower_bound + 1;
                    Self { tail, head, lower_bound, capacity: lower_bound - 1, cost, flow: lower_bound }
                } else {
                    let capacity = lower_bound + width;
                    Self { tail, head, lower_bound, capacity, cost, flow: lower_bound + pick % (width + 1) }
                }
            },
        )
    }
}

/// The network of `nodes` nodes and the arcs `arcs`; its supplies the net outflows of the arcs' flows where `random`
/// is `None`, and otherwise `random` at nodes `1..nodes`, balanced at node 0.
///
/// # Panics
/// Panics if an arc's end is not below `nodes`, or a flow does not fit an `i64`, which no tiny arc's does.
fn network_of(nodes: usize, arcs: &[DrawnArc], random: Option<Vec<i64>>) -> MincostNetwork {
    let mut network = MincostNetwork::default();
    for arc in arcs {
        network.add_arc(arc.tail, arc.head, arc.lower_bound, arc.capacity, arc.cost);
    }

    network.supplies = match random {
        None => net_outflows(nodes, arcs),
        Some(rest) => balanced(rest),
    };

    network
}

/// The net outflow of every node of `0..nodes` under the arcs' flows.
///
/// # Panics
/// Panics if an arc's end is not below `nodes`, or a flow does not fit an `i64`, which no tiny arc's does.
fn net_outflows(nodes: usize, arcs: &[DrawnArc]) -> Vec<i64> {
    let mut supplies = vec![0; nodes];

    for arc in arcs {
        let flow = i64::try_from(arc.flow).expect("a tiny arc's flow should fit an i64");
        supplies[arc.tail] += flow;
        supplies[arc.head] -= flow;
    }

    supplies
}

/// The supplies `rest` of nodes `1..`, preceded by node 0's, which balances them.
fn balanced(rest: Vec<i64>) -> Vec<i64> {
    let first = -rest.iter().sum::<i64>();

    core::iter::once(first).chain(rest).collect()
}

#[cfg(test)]
mod tests {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    use super::*;
    use crate::brute_force_optimum;

    fn arc(tail: usize, head: usize, lower_bound: u64, capacity: u64, flow: u64) -> DrawnArc {
        DrawnArc { tail, head, lower_bound, capacity, cost: 1, flow }
    }

    #[test]
    fn test_net_outflows_send_each_flow_from_its_tail_to_its_head_by_hand() {
        // 0 -> 1 carrying 2, 1 -> 2 carrying 1, and a self-loop at 2 carrying 1, which nets to nothing.
        let arcs = [arc(0, 1, 0, 2, 2), arc(1, 2, 1, 1, 1), arc(2, 2, 0, 2, 1)];

        assert_eq!(net_outflows(3, &arcs), vec![2, -1, -1]);
        assert_eq!(net_outflows(2, &[]), vec![0, 0]);
    }

    #[test]
    fn test_balanced_puts_minus_the_sum_of_the_rest_at_node_0_by_hand() {
        assert_eq!(balanced(vec![3, -1, 2]), vec![-4, 3, -1, 2]);
        assert_eq!(balanced(vec![]), vec![0]);
    }

    #[test]
    fn test_an_inverted_arc_counts_its_lower_bound_in_the_supplies_by_hand() {
        // A loop is invisible to the supplies; an arc 0 -> 1 of lower bound 2 and capacity 1 sends its lower bound.
        let arcs = [arc(0, 1, 2, 1, 2), arc(1, 1, 1, 0, 1)];

        assert_eq!(network_of(2, &arcs, None).supplies, vec![2, -2]);
    }

    #[test]
    fn test_network_of_takes_the_flows_supplies_or_the_drawn_ones_by_hand() {
        let arcs = [arc(0, 1, 1, 3, 2)];

        let of_flow = network_of(2, &arcs, None);
        assert_eq!((of_flow.tails, of_flow.heads), (vec![0], vec![1]));
        assert_eq!((of_flow.lower_bounds, of_flow.capacities), (vec![1], vec![3]));
        assert_eq!((of_flow.costs, of_flow.supplies), (vec![1], vec![2, -2]));

        assert_eq!(network_of(3, &arcs, Some(vec![-3, 1])).supplies, vec![2, -3, 1]);
    }

    /// Every field set to the wider of the choices the tests draw from.
    fn wide() -> TinyMincostNetworks {
        TinyMincostNetworks {
            arcs: 0..=6,
            costs: TinyCosts::ZeroOrUniform(-4..=4),
            supplies: TinySupplies::OfAFlowOrRandom,
            bounds: TinyBounds::OrInverted,
        }
    }

    proptest! {
        #[test]
        fn test_a_drawn_network_keeps_to_its_ranges_and_balances(network in wide().strategy()) {
            prop_assert!((1..=4).contains(&network.node_count()));
            prop_assert!(network.arc_count() <= 6);
            prop_assert!(network.shaped().is_ok());

            for arc in 0..network.arc_count() {
                let (lower_bound, capacity) = (network.lower_bounds[arc], network.capacities[arc]);
                let ordered = lower_bound <= 1 && capacity >= lower_bound && capacity - lower_bound <= 2;
                let inverted = (1..=2).contains(&lower_bound) && capacity + 1 == lower_bound;
                prop_assert!(ordered || inverted, "bounds {}..={}", lower_bound, capacity);
                prop_assert!((-4..=4).contains(&network.costs[arc]));
            }

            prop_assert_eq!(network.supplies.iter().sum::<i64>(), 0);
        }

        #[test]
        fn test_supplies_of_a_flow_are_always_feasible(
            network in TinyMincostNetworks {
                arcs: 1..=6,
                costs: TinyCosts::Uniform(-3..=3),
                supplies: TinySupplies::OfAFlow,
                bounds: TinyBounds::Ordered,
            }.strategy(),
        ) {
            prop_assert!((1..=6).contains(&network.arc_count()));
            prop_assert!(network.costs.iter().all(|cost| (-3..=3).contains(cost)));

            prop_assert!(brute_force_optimum(&network).is_some());
        }
    }

    #[test]
    fn test_tiny_networks_stay_within_what_the_brute_force_enumerates() {
        let mut runner = TestRunner::deterministic();
        let strategy = tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::OrInverted);
        let (mut most_arcs, mut inverted, mut infeasible) = (0, 0, 0);

        for _ in 0..256 {
            let network = strategy.new_tree(&mut runner).expect("a network should be drawn").current();
            most_arcs = most_arcs.max(network.arc_count());
            inverted += usize::from(network.lower_bounds.iter().zip(&network.capacities).any(|(low, cap)| low > cap));
            infeasible += usize::from(brute_force_optimum(&network).is_none());
        }

        assert_eq!(most_arcs, 7, "the most arcs are drawn and no more");
        assert!(inverted > 0 && infeasible > inverted, "{inverted} inverted, {infeasible} infeasible");
    }

    #[test]
    fn test_drawn_networks_reach_every_corner() {
        let mut runner = TestRunner::deterministic();
        let (mut loops, mut parallel, mut fixed, mut empty, mut infeasible) = (false, false, false, false, false);
        let (mut alone, mut inverted, mut gaining) = (false, false, false);
        let networks = wide().strategy();

        for _ in 0..400 {
            let network = networks.new_tree(&mut runner).expect("a network should be drawn").current();
            let ends: Vec<(usize, usize)> = network.tails.iter().copied().zip(network.heads.iter().copied()).collect();

            alone |= network.node_count() == 1;
            inverted |= network.lower_bounds.iter().zip(&network.capacities).any(|(lower, upper)| lower > upper);
            loops |= ends.iter().any(|&(tail, head)| tail == head);
            parallel |= ends.iter().enumerate().any(|(arc, pair)| ends[..arc].contains(pair));
            fixed |=
                network.lower_bounds.iter().zip(&network.capacities).any(|(lower, upper)| lower == upper && *lower > 0);
            empty |= network.capacities.contains(&0);
            gaining |= network.costs.iter().any(|&cost| cost < 0);
            infeasible |= brute_force_optimum(&network).is_none();
        }

        assert!(loops && parallel && fixed && empty && infeasible, "{loops} {parallel} {fixed} {empty} {infeasible}");
        assert!(alone && inverted && gaining, "{alone} {inverted} {gaining}");

        let ordered = TinyMincostNetworks { bounds: TinyBounds::Ordered, ..wide() }.strategy();
        for _ in 0..400 {
            let network = ordered.new_tree(&mut runner).expect("a network should be drawn").current();
            assert!(network.lower_bounds.iter().zip(&network.capacities).all(|(lower, upper)| lower <= upper));
        }
    }
}
