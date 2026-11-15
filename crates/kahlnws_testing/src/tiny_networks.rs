// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Tiny min-cost networks for property tests, few enough nodes and arcs for the brute force, with the arc count, the
//! costs, whether infeasible supplies occur and whether a lower bound may pass its capacity chosen by the test that
//! draws them.

use core::ops::RangeInclusive;

use itertools::Itertools;
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
                let drawn = vec(ArcDraw::strategy(nodes, costs.strategy(), bounds.strategy()), fewest..=most);
                (Just(nodes), drawn, supplies.strategy(nodes))
            })
            .prop_map(|(nodes, draws, random)| {
                network_of(nodes, &draws.into_iter().map(ArcDraw::arc).collect_vec(), random)
            })
    }

    /// The strategy that draws pairs of networks on the same arcs, each as `self` describes it: the second has the
    /// first's nodes and its arcs' tails and heads, and its supplies, its lower bounds, its widths and its costs are
    /// each the first's or drawn anew, by a coin of their own; an arc's capacity is its lower bound and its width as
    /// they came out, so every arc keeps the ranges of [`strategy`](Self::strategy). So the same network comes up
    /// about once in sixteen pairs, and so does the same network at other costs.
    pub fn pairs(self) -> impl Strategy<Value = (MincostNetwork, MincostNetwork)> {
        let Self { arcs, costs, supplies, bounds } = self;
        let (fewest, most) = arcs.into_inner();
        let redrawn_costs = costs.clone();

        (1_usize..=4)
            .prop_flat_map(move |nodes| {
                let drawn = vec(ArcDraw::strategy(nodes, costs.strategy(), bounds.strategy()), fewest..=most);
                (Just(nodes), drawn, supplies.strategy(nodes))
            })
            .prop_flat_map(move |(nodes, first, random)| {
                let redrawn = vec(ArcNumbers::strategy(redrawn_costs.strategy(), bounds.strategy()), first.len());
                (Just(FirstDraw { nodes, arcs: first, random }), redrawn, supplies.strategy(nodes), Redraws::strategy())
            })
            .prop_map(|(first, redrawn, random, redraws)| first.paired(&redrawn, random, redraws))
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

/// One arc as drawn, before its bounds and its flow are worked out: its ends and its numbers. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArcDraw {
    /// The node it leaves.
    tail: usize,
    /// The node it enters.
    head: usize,
    /// The numbers its bounds, its cost and its flow are worked out from.
    numbers: ArcNumbers,
}

/// The numbers one arc is drawn with, its ends apart. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ArcNumbers {
    /// Its lower bound where its bounds are ordered, one less than it where they are inverted: 0 or 1.
    lower: u64,
    /// Its width where its bounds are ordered: 0 to 2.
    width: u64,
    /// Its cost of a unit of flow.
    cost: i64,
    /// What picks its flow among the flows within its ordered bounds: 0 to 3.
    pick: u64,
    /// Whether its lower bound passes its capacity.
    inverted: bool,
}

/// The first network of a pair as drawn: its nodes, its arcs, and the supplies drawn for it. Plain data.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FirstDraw {
    /// The number of nodes.
    nodes: usize,
    /// The arcs as drawn.
    arcs: Vec<ArcDraw>,
    /// The supplies of nodes `1..nodes`, or `None` where they are the arcs' flows'.
    random: Option<Vec<i64>>,
}

/// Which columns of the second network of a pair are drawn anew, each by a coin of its own. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Redraws {
    /// The supplies.
    supplies: Coin,
    /// The lower bounds.
    lower_bounds: Coin,
    /// The widths, with whether the bounds are inverted.
    widths: Coin,
    /// The costs.
    costs: Coin,
}

/// Whether a column of the second network of a pair is the first's or drawn anew.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Coin {
    /// The first network's.
    Keep,
    /// Drawn anew.
    Redraw,
}

impl ArcDraw {
    /// The strategy of one arc between nodes of `0..nodes`, its cost drawn by `cost` and whether its bounds are
    /// inverted by `inverted`.
    fn strategy(nodes: usize, cost: BoxedStrategy<i64>, inverted: BoxedStrategy<bool>) -> impl Strategy<Value = Self> {
        (0..nodes, 0..nodes, ArcNumbers::strategy(cost, inverted)).prop_map(|(tail, head, numbers)| Self {
            tail,
            head,
            numbers,
        })
    }

    /// The arc of this draw: where its bounds are ordered, its lower bound and its capacity that plus its width, and a
    /// flow of `lower + pick % (width + 1)`; where they are inverted, a lower bound of one more than drawn, a capacity
    /// one below it, and its lower bound as its flow.
    fn arc(self) -> DrawnArc {
        let Self { tail, head, numbers: ArcNumbers { lower, width, cost, pick, inverted } } = self;

        if inverted {
            let lower_bound = lower + 1;
            DrawnArc { tail, head, lower_bound, capacity: lower_bound - 1, cost, flow: lower_bound }
        } else {
            DrawnArc { tail, head, lower_bound: lower, capacity: lower + width, cost, flow: lower + pick % (width + 1) }
        }
    }
}

impl ArcNumbers {
    /// The strategy of the numbers of one arc, its cost drawn by `cost` and whether its bounds are inverted by
    /// `inverted`.
    fn strategy(cost: BoxedStrategy<i64>, inverted: BoxedStrategy<bool>) -> impl Strategy<Value = Self> {
        (0_u64..=1, 0_u64..=2, cost, 0_u64..=3, inverted).prop_map(|(lower, width, cost, pick, inverted)| Self {
            lower,
            width,
            cost,
            pick,
            inverted,
        })
    }

    /// These numbers with the columns `redraws` draws anew taken from `redrawn`: the lower bound, the width with
    /// whether the bounds are inverted, and the cost. The pick of the flow is `redrawn`'s, read only where the supplies
    /// are drawn anew as a flow's.
    fn mixed(self, redrawn: Self, redraws: Redraws) -> Self {
        let (width, inverted) = redraws.widths.pick((self.width, self.inverted), (redrawn.width, redrawn.inverted));

        Self {
            lower: redraws.lower_bounds.pick(self.lower, redrawn.lower),
            width,
            cost: redraws.costs.pick(self.cost, redrawn.cost),
            pick: redrawn.pick,
            inverted,
        }
    }
}

impl FirstDraw {
    /// The pair of this draw: its own network, and the network on the same arcs whose columns `redraws` names are
    /// drawn anew, from `redrawn`, one per arc, and from `random`, the supplies drawn for it.
    ///
    /// # Panics
    /// Panics if `redrawn` holds another number of entries than the draw has arcs.
    fn paired(
        self,
        redrawn: &[ArcNumbers],
        random: Option<Vec<i64>>,
        redraws: Redraws,
    ) -> (MincostNetwork, MincostNetwork) {
        let Self { nodes, arcs, random: first_random } = self;
        let second_arcs = arcs
            .iter()
            .zip_eq(redrawn)
            .map(|(&draw, &numbers)| ArcDraw { numbers: draw.numbers.mixed(numbers, redraws), ..draw }.arc())
            .collect_vec();
        let first = network_of(nodes, &arcs.into_iter().map(ArcDraw::arc).collect_vec(), first_random);

        let mut second = network_of(nodes, &second_arcs, random);
        second.supplies = redraws.supplies.pick(first.supplies.clone(), second.supplies);

        (first, second)
    }
}

impl Redraws {
    /// The strategy of four fair coins.
    fn strategy() -> impl Strategy<Value = Self> {
        let coin = || prop_oneof![Just(Coin::Keep), Just(Coin::Redraw)];

        (coin(), coin(), coin(), coin()).prop_map(|(supplies, lower_bounds, widths, costs)| Self {
            supplies,
            lower_bounds,
            widths,
            costs,
        })
    }
}

impl Coin {
    /// `kept` where this coin keeps the first's column, `redrawn` where it draws it anew.
    fn pick<T>(self, kept: T, redrawn: T) -> T {
        match self {
            Self::Keep => kept,
            Self::Redraw => redrawn,
        }
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

        #[test]
        fn test_a_pair_shares_its_arcs_and_keeps_to_the_ranges((first, second) in wide().pairs()) {
            prop_assert_eq!(second.node_count(), first.node_count());
            prop_assert_eq!((&second.tails, &second.heads), (&first.tails, &first.heads));
            prop_assert!(first.arc_count() <= 6 && second.shaped().is_ok());

            for arc in 0..second.arc_count() {
                let (lower_bound, capacity) = (second.lower_bounds[arc], second.capacities[arc]);
                let ordered = lower_bound <= 1 && capacity >= lower_bound && capacity - lower_bound <= 2;
                let inverted = (1..=2).contains(&lower_bound) && capacity + 1 == lower_bound;
                prop_assert!(ordered || inverted, "bounds {}..={}", lower_bound, capacity);
                prop_assert!((-4..=4).contains(&second.costs[arc]));
            }
            prop_assert_eq!(second.supplies.iter().sum::<i64>(), 0);

            // Within what the brute force enumerates, which it refuses with a panic.
            let _ = brute_force_optimum(&second);
        }
    }

    #[test]
    fn test_pairs_reach_the_same_network_and_each_column_changed_alone() {
        let mut runner = TestRunner::deterministic();
        let pairs = TinyMincostNetworks { bounds: TinyBounds::Ordered, ..wide() }.pairs();
        let (mut same, mut costs, mut supplies, mut bounds, mut everything) = (0, 0, 0, 0, 0);

        for _ in 0..400 {
            let (first, second) = pairs.new_tree(&mut runner).expect("a pair should be drawn").current();
            let changed = [
                first.supplies != second.supplies,
                first.lower_bounds != second.lower_bounds || first.capacities != second.capacities,
                first.costs != second.costs,
            ];
            match changed {
                [false, false, false] => same += 1,
                [false, false, true] => costs += 1,
                [true, false, false] => supplies += 1,
                [false, true, false] => bounds += 1,
                [true, true, true] => everything += 1,
                _ => {}
            }
        }

        assert!(same > 0 && costs > 0 && supplies > 0, "{same} same, {costs} costs, {supplies} supplies");
        assert!(bounds > 0 && everything > 0, "{bounds} bounds, {everything} everything");
    }

    /// The numbers of an arc, for a test to write out.
    fn numbers(lower: u64, width: u64, cost: i64, pick: u64, inverted: bool) -> ArcNumbers {
        ArcNumbers { lower, width, cost, pick, inverted }
    }

    #[test]
    fn test_an_arc_draw_works_out_its_bounds_and_its_flow_by_hand() {
        // Ordered: a lower bound of 1 and a width of 2 make a capacity of 3; the pick 3 a flow of 1 + 3 % 3 = 1.
        let draw = ArcDraw { tail: 0, head: 1, numbers: numbers(1, 2, 5, 3, false) };
        assert_eq!(draw.arc(), DrawnArc { tail: 0, head: 1, lower_bound: 1, capacity: 3, cost: 5, flow: 1 });

        // Inverted: a lower bound of one more than drawn, a capacity one below it, and the lower bound as the flow.
        let inverted = ArcDraw { numbers: numbers(1, 2, 5, 3, true), ..draw };
        assert_eq!(inverted.arc(), DrawnArc { tail: 0, head: 1, lower_bound: 2, capacity: 1, cost: 5, flow: 2 });
    }

    #[test]
    fn test_mixed_numbers_take_each_column_by_its_coin_by_hand() {
        let (kept, redrawn) = (numbers(1, 2, 5, 3, false), numbers(0, 0, -4, 1, true));
        let (keep, redraw) = (Coin::Keep, Coin::Redraw);
        let redraws = |lower_bounds, widths, costs| Redraws { supplies: keep, lower_bounds, widths, costs };

        // The pick of the flow is always the redrawn one's.
        assert_eq!(kept.mixed(redrawn, redraws(keep, keep, keep)), numbers(1, 2, 5, 1, false));
        assert_eq!(kept.mixed(redrawn, redraws(redraw, keep, keep)), numbers(0, 2, 5, 1, false));
        assert_eq!(kept.mixed(redrawn, redraws(keep, redraw, keep)), numbers(1, 0, 5, 1, true), "with the inversion");
        assert_eq!(kept.mixed(redrawn, redraws(keep, keep, redraw)), numbers(1, 2, -4, 1, false));
        assert_eq!(kept.mixed(redrawn, redraws(redraw, redraw, redraw)), redrawn);
    }

    #[test]
    fn test_a_pair_keeps_or_redraws_each_column_by_hand() {
        // Two nodes and an arc 0 -> 1 of lower bound 1, width 2 and cost 5, carrying 1 + 3 % 3 = 1 in the flow its
        // supplies are of.
        let arcs = vec![ArcDraw { tail: 0, head: 1, numbers: numbers(1, 2, 5, 3, false) }];
        let first = FirstDraw { nodes: 2, arcs, random: None };
        let redrawn = [numbers(0, 1, -4, 0, false)];
        let all = |coin| Redraws { supplies: coin, lower_bounds: coin, widths: coin, costs: coin };

        let (network, same) = first.clone().paired(&redrawn, Some(vec![2]), all(Coin::Keep));
        assert_eq!(network, MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 1, 3, 5)]));
        assert_eq!(same, network, "every column kept");

        // Every column drawn anew: a lower bound of 0, a width of 1 and a cost of -4; node 1 supplies 2, node 0
        // balances it.
        let (_, other) = first.clone().paired(&redrawn, Some(vec![2]), all(Coin::Redraw));
        assert_eq!(other, MincostNetwork::from_arcs(&[-2, 2], &[(0, 1, 0, 1, -4)]));

        // The supplies drawn anew as a flow's: the new arc carries 0 + 0 % 2 = 0.
        let (_, of_a_flow) = first.clone().paired(&redrawn, None, all(Coin::Redraw));
        assert_eq!(of_a_flow.supplies, [0, 0]);

        // Only the costs drawn anew.
        let costs = Redraws { costs: Coin::Redraw, ..all(Coin::Keep) };
        let (_, recosted) = first.paired(&redrawn, None, costs);
        assert_eq!(recosted, MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 1, 3, -4)]));
    }

    #[test]
    #[should_panic(expected = "zip_eq")]
    fn test_a_pair_refuses_redrawn_numbers_of_another_count() {
        let first = FirstDraw { nodes: 1, arcs: vec![], random: None };
        let keep = Coin::Keep;
        let redraws = Redraws { supplies: keep, lower_bounds: keep, widths: keep, costs: keep };

        let _ = first.paired(&[numbers(0, 0, 0, 0, false)], None, redraws);
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
