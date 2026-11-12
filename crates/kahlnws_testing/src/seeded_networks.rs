// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Min-cost networks drawn from a seed, larger than a brute force enumerates: the supply shape, the assignment shape
//! and general networks with lower bounds, for the tests that check a solver against the LP oracle. A seed is a list
//! of numbers read cyclically, so a network is a function of its seed alone.

use itertools::Itertools;

use crate::mincost_network::MincostNetwork;

/// The capacity of an arc that never binds: more than the supplies of a seeded network add up to.
const WIDE: u64 = 1000;

/// A network of the supply shape, drawn from `seed`: a few chains of holding nodes, each carrying the volume put in
/// at its first node on along arcs of small cost; a few demand nodes drawing what they need from the holding nodes,
/// some draws with a least part as a lower bound; a slack source covering any shortage at a cost of large magnitude;
/// and a sink taking what is left over from each chain's last node and what is written off from any holding node
/// within a cap. The costs are as described where the seed's [`Signs`] are non-negative; where they are mixed, every
/// drawn cost takes a drawn sign, so about half of them are negative, and a negative cost on a capacitated arc pays
/// for flow no supply needs.
///
/// # Panics
/// Panics if `seed` is empty.
pub fn supply_shaped(seed: &[u64]) -> MincostNetwork {
    assert!(!seed.is_empty(), "called `supply_shaped` with an empty seed");

    let mut construction = Construction::new(seed);
    let chain_count = 1 + construction.draws.below(3);
    let demand_count = 1 + construction.draws.below(4);

    let chains = construction.holding_chains(chain_count);
    let takers = construction.takers(demand_count);
    let slack = construction.network.add_node(takers.need);
    let sink = construction.network.add_node(-chains.volume);

    construction.holding_arcs(&chains, &takers.nodes, sink);
    construction.slack_arcs(slack, &takers.nodes, sink);

    construction.network
}

/// A network of the assignment shape, drawn from `seed`: a few source nodes, each supplying its size; a few
/// demands, each an assigned node fed by arcs from the sources and a served node taking its quantity from its own
/// assigned node and, one hop on, from later demands' assigned nodes; a slack source for what stays unmet; and a
/// surplus sink. Where the seed's [`Signs`] are mixed, every drawn cost takes a drawn sign, so about half of them are
/// negative.
///
/// # Panics
/// Panics if `seed` is empty.
pub fn assignment_shaped(seed: &[u64]) -> MincostNetwork {
    assert!(!seed.is_empty(), "called `assignment_shaped` with an empty seed");

    let mut construction = Construction::new(seed);
    let source_count = 1 + construction.draws.below(4);
    let sizes = construction.amounts(source_count, 20);
    let demand_count = 1 + construction.draws.below(5);
    let quantities = construction.amounts(demand_count, 15);

    let nodes = construction.assignment_nodes(&sizes, &quantities);

    construction.source_arcs(&nodes);
    construction.demand_arcs(&nodes);

    construction.network
}

/// A general network drawn from `seed`: two to eleven nodes with small supplies, often infeasible, and up to thirty
/// arcs drawn between any two nodes, so self-loops and parallel arcs occur; about a third of the arcs carry a lower
/// bound, about one in six has `lower == upper` above zero, and about one in six has neither room nor lower bound.
/// Costs are drawn from `[0, 19]` where the seed's [`Signs`] are non-negative, and from `[-19, 19]` where they are
/// mixed, so that self-loops and cycles of negative cost occur.
///
/// # Panics
/// Panics if `seed` is empty.
pub fn general(seed: &[u64]) -> MincostNetwork {
    assert!(!seed.is_empty(), "called `general` with an empty seed");

    let mut construction = Construction::new(seed);
    let node_count = 2 + construction.draws.below(10);
    construction.balanced_nodes(node_count);

    let arc_count = construction.draws.below(31);
    for _ in 0..arc_count {
        construction.general_arc();
    }

    construction.network
}

/// A network under construction and the draws it is made from.
#[derive(Debug)]
struct Construction<'a> {
    /// The numbers still to be drawn.
    draws: Draws<'a>,
    /// The network as drawn so far.
    network: MincostNetwork,
}

/// The holding chains of the supply shape, each a run of nodes the first of which holds a volume, and the volumes
/// summed. Plain data.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HoldingChains {
    /// The nodes of each chain, the one holding its volume first.
    chains: Vec<Vec<usize>>,
    /// The volumes of the chains' first nodes, summed.
    volume: i64,
}

/// The demand nodes of the supply shape and what they need. Plain data.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Takers {
    /// The nodes, in the order drawn.
    nodes: Vec<usize>,
    /// What they need, summed: the negated sum of their supplies.
    need: i64,
}

/// The nodes of the assignment shape. Plain data.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AssignmentNodes {
    /// The source nodes, each supplying its size.
    sources: Vec<usize>,
    /// Each demand's assigned node, then its served node.
    demands: Vec<(usize, usize)>,
    /// The slack source, supplying what the served nodes need.
    slack: usize,
    /// The surplus sink, taking what the sources supply.
    surplus: usize,
}

impl<'a> Construction<'a> {
    /// An empty network to be drawn from `seed`.
    ///
    /// # Panics
    /// Panics if `seed` is empty.
    fn new(seed: &'a [u64]) -> Self {
        Self { draws: Draws::new(seed), network: MincostNetwork::default() }
    }

    /// The supply shape's `count` chains of one to three holding nodes, the first holding a volume below 40, each
    /// link of the chain at a cost below 3.
    fn holding_chains(&mut self, count: u64) -> HoldingChains {
        let mut chains = Vec::new();
        let mut volume = 0;

        for _ in 0..count {
            let held = self.draws.supply_below(40);
            volume += held;

            let length = 1 + self.draws.below(3);
            let chain: Vec<usize> =
                (0..length).map(|position| self.network.add_node(if position == 0 { held } else { 0 })).collect();
            for (&from, &to) in chain.iter().tuple_windows() {
                let cost = self.draws.cost(0, 3);
                self.network.add_arc(from, to, 0, WIDE, cost);
            }
            chains.push(chain);
        }

        HoldingChains { chains, volume }
    }

    /// The supply shape's `count` demand nodes, each needing less than 30.
    fn takers(&mut self, count: u64) -> Takers {
        let mut need = 0;

        let nodes = (0..count)
            .map(|_| {
                let needed = self.draws.supply_below(30);
                need += needed;
                self.network.add_node(-needed)
            })
            .collect();

        Takers { nodes, need }
    }

    /// The supply shape's arcs out of its holding nodes: to each taker, two times in three, with a lower bound below
    /// 4 one time in five, a width below 25 and a cost below 10; to the sink, a write-off below 8 at a cost of 4 to 9;
    /// and from each chain's last node to the sink at a cost below 5.
    ///
    /// # Panics
    /// Panics if a chain is empty.
    fn holding_arcs(&mut self, chains: &HoldingChains, takers: &[usize], sink: usize) {
        for chain in &chains.chains {
            for &holding in chain {
                for &taker in takers {
                    if self.draws.below(3) > 0 {
                        let lower = if self.draws.below(5) == 0 { self.draws.below(4) } else { 0 };
                        let width = self.draws.below(25);
                        let cost = self.draws.cost(0, 10);
                        self.network.add_arc(holding, taker, lower, lower + width, cost);
                    }
                }
                let write_off = self.draws.below(8);
                let cost = self.draws.cost(4, 6);
                self.network.add_arc(holding, sink, 0, write_off, cost);
            }

            let last = *chain.last().expect("a chain should hold its first node");
            let cost = self.draws.cost(0, 5);
            self.network.add_arc(last, sink, 0, WIDE, cost);
        }
    }

    /// The supply shape's arcs out of its slack source: to each taker at a cost of 100 to 149, and to the sink at no
    /// cost.
    fn slack_arcs(&mut self, slack: usize, takers: &[usize], sink: usize) {
        for &taker in takers {
            let cost = self.draws.cost(100, 50);
            self.network.add_arc(slack, taker, 0, WIDE, cost);
        }

        self.network.add_arc(slack, sink, 0, WIDE, 0);
    }

    /// `count` amounts, each below `bound`.
    ///
    /// # Panics
    /// Panics if `bound` is zero or passes `i64::MAX`.
    fn amounts(&mut self, count: u64, bound: u64) -> Vec<i64> {
        (0..count).map(|_| self.draws.supply_below(bound)).collect()
    }

    /// The assignment shape's nodes: a source supplying each of `sizes`; per quantity of `quantities` an assigned node
    /// and a served node needing it; the slack source and the surplus sink that balance them. It draws nothing.
    fn assignment_nodes(&mut self, sizes: &[i64], quantities: &[i64]) -> AssignmentNodes {
        let network = &mut self.network;
        let sources = sizes.iter().map(|&size| network.add_node(size)).collect();
        let demands = quantities.iter().map(|&quantity| (network.add_node(0), network.add_node(-quantity))).collect();
        let slack = network.add_node(quantities.iter().sum());
        let surplus = network.add_node(-sizes.iter().sum::<i64>());

        AssignmentNodes { sources, demands, slack, surplus }
    }

    /// The assignment shape's arcs out of its sources: to each demand's assigned node three times in four at a cost
    /// below 12, and to the surplus at a cost of 2 to 4.
    fn source_arcs(&mut self, nodes: &AssignmentNodes) {
        for &source in &nodes.sources {
            for &(assigned, _) in &nodes.demands {
                if self.draws.below(4) > 0 {
                    let cost = self.draws.cost(0, 12);
                    self.network.add_arc(source, assigned, 0, WIDE, cost);
                }
            }
            let cost = self.draws.cost(2, 3);
            self.network.add_arc(source, nodes.surplus, 0, WIDE, cost);
        }
    }

    /// The assignment shape's arcs around its demands: each assigned node to its own served node at no cost, to each
    /// later demand's served node one time in three at a cost of 1 to 4, and to the surplus at 3; the slack source to
    /// each served node at a cost of 50 to 69; then the slack source to the surplus at no cost.
    fn demand_arcs(&mut self, nodes: &AssignmentNodes) {
        for (position, &(assigned, served)) in nodes.demands.iter().enumerate() {
            self.network.add_arc(assigned, served, 0, WIDE, 0);
            for &(_, later) in &nodes.demands[position + 1..] {
                if self.draws.below(3) == 0 {
                    let cost = self.draws.cost(1, 4);
                    self.network.add_arc(assigned, later, 0, WIDE, cost);
                }
            }
            self.network.add_arc(assigned, nodes.surplus, 0, WIDE, 3);
            let cost = self.draws.cost(50, 20);
            self.network.add_arc(nodes.slack, served, 0, WIDE, cost);
        }

        self.network.add_arc(nodes.slack, nodes.surplus, 0, WIDE, 0);
    }

    /// `count` nodes, every one but the last with a supply of -6 to 6, the last balancing them.
    fn balanced_nodes(&mut self, count: u64) {
        let mut total = 0;

        for _ in 1..count {
            let supply = self.draws.supply_below(13) - 6;
            total += supply;
            self.network.add_node(supply);
        }

        self.network.add_node(-total);
    }

    /// One arc of a general network between any two of its nodes: one time in six fixed at a lower bound of 1 to 3,
    /// one time in six of no room, otherwise of a width below 9 above a lower bound that is 1 to 3 one time in four
    /// and zero otherwise; its cost below 20.
    ///
    /// # Panics
    /// Panics if the network has no node.
    fn general_arc(&mut self) {
        let node_count = self.network.node_count();
        let (tail, head) = (self.draws.index_below(node_count), self.draws.index_below(node_count));
        let (lower, width) = match self.draws.below(6) {
            0 => (1 + self.draws.below(3), 0),
            1 => (0, 0),
            _ => (if self.draws.below(4) == 0 { 1 + self.draws.below(3) } else { 0 }, self.draws.below(9)),
        };
        let cost = self.draws.cost(0, 20);

        self.network.add_arc(tail, head, lower, lower + width, cost);
    }
}

/// A stream of numbers below given bounds, read cyclically from a seed, with the signs its costs take.
///
/// # Invariant
/// The seed is not empty, so the cycle over it never ends.
#[derive(Debug)]
struct Draws<'a> {
    seed: core::iter::Cycle<core::slice::Iter<'a, u64>>,
    signs: Signs,
}

impl<'a> Draws<'a> {
    /// The stream of `seed`, its costs signed as [`Signs::of`] the seed says, which consumes no draw.
    ///
    /// # Panics
    /// Panics if `seed` is empty.
    fn new(seed: &'a [u64]) -> Self {
        Self { seed: seed.iter().cycle(), signs: Signs::of(seed) }
    }

    /// The next number of the seed, reduced below `bound`.
    ///
    /// # Panics
    /// Panics if `bound` is zero.
    fn below(&mut self, bound: u64) -> u64 {
        *self.seed.next().expect("a cycle over a non-empty seed should not end") % bound
    }

    /// The next number of the seed, reduced below `count`, as an index.
    ///
    /// # Panics
    /// Panics if `count` is zero.
    fn index_below(&mut self, count: usize) -> usize {
        let drawn = self.below(u64::try_from(count).expect("a count should fit a u64"));

        usize::try_from(drawn).expect("a draw below a count should fit a usize")
    }

    /// The next number of the seed, reduced below `bound`, as a supply.
    ///
    /// # Panics
    /// Panics if `bound` is zero or passes `i64::MAX`.
    fn supply_below(&mut self, bound: u64) -> i64 {
        i64::try_from(self.below(bound)).expect("a supply drawn below a small bound should fit an i64")
    }

    /// A cost of `base` plus a number below `spread`: as it is where the signs are [`Signs::NonNegative`], so those
    /// networks take exactly the draws of a generator without signs, and with a drawn sign, negative on an odd draw,
    /// where they are [`Signs::Mixed`].
    ///
    /// # Panics
    /// Panics if `spread` is zero, or the cost passes `i64::MAX`.
    fn cost(&mut self, base: u64, spread: u64) -> i64 {
        let magnitude =
            i64::try_from(base + self.below(spread)).expect("a cost drawn below a small bound should fit an i64");
        let signs = self.signs;

        match signs {
            Signs::NonNegative => magnitude,
            Signs::Mixed if self.below(2) == 1 => -magnitude,
            Signs::Mixed => magnitude,
        }
    }
}

/// Which signs a generated network's costs take, chosen once per network from its seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signs {
    /// Every cost zero or above, as many applications have them: about one seed in three.
    NonNegative,
    /// Every drawn cost with a drawn sign, so about half of them are negative.
    Mixed,
}

impl Signs {
    /// The signs of the network drawn from `seed`: [`NonNegative`](Self::NonNegative) where the seed's first number
    /// divided by 30 is a multiple of three, about one seed in three, read from the high part of that number, which
    /// the generators' first small draw hardly depends on. It consumes no draw, so a non-negative network takes the
    /// same draws in the same order as a generator that never signs a cost.
    ///
    /// # Panics
    /// Panics if `seed` is empty.
    pub fn of(seed: &[u64]) -> Self {
        assert!(!seed.is_empty(), "called `Signs::of` with an empty seed");

        if (seed[0] / 30).is_multiple_of(3) { Self::NonNegative } else { Self::Mixed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generators_balance_their_supplies_and_keep_their_bounds() {
        for seed in [[0u64].as_slice(), &[7, 1, 9, 3], &[u64::MAX, 2, 5, 11, 4]] {
            for net in [supply_shaped(seed), assignment_shaped(seed), general(seed)] {
                assert_eq!(net.supplies.iter().sum::<i64>(), 0, "{seed:?}");
                assert!(net.shaped().is_ok(), "{seed:?}");
                assert!(net.lower_bounds.iter().zip(&net.capacities).all(|(lower, upper)| lower <= upper));
            }
        }
    }

    #[test]
    fn test_every_generator_draws_both_sign_modes_and_keeps_non_negative_networks_non_negative() {
        let generators: [fn(&[u64]) -> MincostNetwork; 3] = [supply_shaped, assignment_shaped, general];
        let seeds: Vec<[u64; 5]> =
            (0..300u64).map(|first| [first.wrapping_mul(0x9e37_79b9_7f4a_7c15), 7, 1, 9, 3]).collect();
        let non_negative = seeds.iter().filter(|seed| Signs::of(seed.as_slice()) == Signs::NonNegative).count();
        assert!((75..=126).contains(&non_negative), "about one seed in three is non-negative: {non_negative} of 300");

        for generate in generators {
            let mut gaining = false;
            for seed in &seeds {
                let net = generate(seed);
                match Signs::of(seed) {
                    Signs::NonNegative => assert!(net.costs.iter().all(|&cost| cost >= 0), "{seed:?}: {:?}", net.costs),
                    Signs::Mixed => gaining |= net.costs.iter().any(|&cost| cost < 0),
                }
            }
            assert!(gaining, "a mixed network has a negative cost");
        }
    }

    #[test]
    fn test_general_networks_take_every_corner() {
        let (mut loops, mut parallel, mut fixed, mut empty) = (false, false, false, false);
        let (mut negative, mut positive, mut gaining_loops) = (0_usize, 0_usize, false);

        for first in 0..200u64 {
            let net = general(&[first, first * 7 + 3, first * 13 + 5, first * 31 + 1, 17, 4, 9, 2]);
            let ends: Vec<(usize, usize)> = net.tails.iter().copied().zip(net.heads.iter().copied()).collect();
            loops |= ends.iter().any(|&(tail, head)| tail == head);
            parallel |= ends.iter().enumerate().any(|(arc, pair)| ends[..arc].contains(pair));
            fixed |= net.lower_bounds.iter().zip(&net.capacities).any(|(lower, upper)| lower == upper && *lower > 0);
            empty |= net.capacities.contains(&0);
            negative += net.costs.iter().filter(|&&cost| cost < 0).count();
            positive += net.costs.iter().filter(|&&cost| cost > 0).count();
            gaining_loops |= ends.iter().zip(&net.costs).enumerate().any(|(arc, (&(tail, head), &cost))| {
                tail == head && cost < 0 && net.capacities[arc] > net.lower_bounds[arc]
            });
        }

        assert!(loops && parallel && fixed && empty, "{loops} {parallel} {fixed} {empty}");
        assert!(gaining_loops, "a self-loop of negative cost with room occurs");
        assert!(
            negative * 3 > positive && positive * 3 > negative,
            "{negative} negative and {positive} positive costs"
        );
    }

    #[test]
    fn test_draws_read_the_seed_cyclically_and_sign_only_mixed_costs() {
        let mut draws = Draws::new(&[7, 4, 9]);
        assert_eq!([draws.below(5), draws.below(5), draws.below(5), draws.below(5)], [2, 4, 4, 2]);
        assert_eq!(Draws::new(&[7]).index_below(3), 1);
        assert_eq!(Draws::new(&[7]).supply_below(5), 2);

        // Non-negative: one draw a cost, as without signs.
        let mut plain = Draws::new(&[7, 1, 5]);
        assert_eq!(plain.signs, Signs::NonNegative);
        assert_eq!([plain.cost(2, 10), plain.cost(2, 10), plain.cost(2, 10)], [9, 3, 7]);

        // Mixed: a second draw a cost, its sign.
        let mut mixed = Draws::new(&[31, 1, 5]);
        assert_eq!(mixed.signs, Signs::Mixed);
        assert_eq!([mixed.cost(2, 10), mixed.cost(2, 10), mixed.cost(2, 10)], [-3, -7, -3]);

        // Mixed and an even sign draw: the cost stays positive. 31 % 10 = 1 and 2 % 2 = 0 give 2 + 1 = 3; then
        // 5 % 10 = 5 and 31 % 2 = 1 give -(2 + 5) = -7.
        let mut positive = Draws::new(&[31, 2, 5]);
        assert_eq!([positive.cost(2, 10), positive.cost(2, 10)], [3, -7]);
    }

    /// The node count, the arc count and the fingerprint of `net`.
    fn fingerprint(net: &MincostNetwork) -> [u64; 3] {
        let index = |value: usize| u64::try_from(value).expect("an index should fit a u64");

        [index(net.node_count()), index(net.arc_count()), net.fingerprint()]
    }

    /// The fingerprints of the networks the generators draw from a non-negative and a mixed seed. A change to any
    /// generator's draws changes these fingerprints, and with them every test and benchmark input drawn from it.
    const PINNED: [[u64; 3]; 6] = [
        [7, 11, 141_769_627_454_446_512],
        [7, 11, 707_048_502_417_890_004],
        [8, 12, 332_076_433_336_722_499],
        [8, 11, 3_323_613_904_835_121_138],
        [9, 9, 15_160_454_978_569_184_393],
        [3, 3, 18_279_250_941_191_537_802],
    ];

    #[test]
    fn test_generators_draw_the_pinned_networks() {
        let generators: [fn(&[u64]) -> MincostNetwork; 3] = [supply_shaped, assignment_shaped, general];
        let seeds: [&[u64]; 2] = [&[7, 1, 9, 3, 12, 40, 5], &[31, 1, 9, 3, 12, 40, 5]];
        assert_eq!(Signs::of(seeds[0]), Signs::NonNegative);
        assert_eq!(Signs::of(seeds[1]), Signs::Mixed);

        let drawn: Vec<[u64; 3]> =
            generators.iter().flat_map(|generate| seeds.map(|seed| fingerprint(&generate(seed)))).collect();
        assert_eq!(drawn, PINNED);
    }

    #[test]
    #[should_panic(expected = "called `supply_shaped` with an empty seed")]
    fn test_supply_shaped_refuses_an_empty_seed() {
        let _ = supply_shaped(&[]);
    }

    #[test]
    #[should_panic(expected = "called `assignment_shaped` with an empty seed")]
    fn test_assignment_shaped_refuses_an_empty_seed() {
        let _ = assignment_shaped(&[]);
    }

    #[test]
    #[should_panic(expected = "called `general` with an empty seed")]
    fn test_general_refuses_an_empty_seed() {
        let _ = general(&[]);
    }

    #[test]
    fn test_holding_chains_by_hand() {
        // Seed 7, 5, 11, non-negative: chain one holds 7 over 1 + 5 % 3 = 3 nodes, its links at 11 % 3 = 2 and
        // 7 % 3 = 1; chain two holds 5 over 1 + 11 % 3 = 3 nodes, its links at 7 % 3 = 1 and 5 % 3 = 2.
        let mut construction = Construction::new(&[7, 5, 11]);
        let chains = construction.holding_chains(2);

        assert_eq!(chains, HoldingChains { chains: vec![vec![0, 1, 2], vec![3, 4, 5]], volume: 12 });
        let network = &construction.network;
        assert_eq!(network.supplies, [7, 0, 0, 5, 0, 0]);
        assert_eq!((network.tails.clone(), network.heads.clone()), (vec![0, 1, 3, 4], vec![1, 2, 4, 5]));
        assert_eq!((network.capacities.clone(), network.costs.clone()), (vec![WIDE; 4], vec![2, 1, 1, 2]));
    }

    #[test]
    fn test_takers_need_what_they_draw_by_hand() {
        let mut construction = Construction::new(&[7, 5, 31]);
        let takers = construction.takers(3);

        assert_eq!(takers, Takers { nodes: vec![0, 1, 2], need: 7 + 5 + 1 });
        assert_eq!(construction.network.supplies, [-7, -5, -1]);
    }

    #[test]
    fn test_holding_arcs_by_hand() {
        // Seed 3: no arc to the taker, as 3 % 3 = 0; a write-off of 3 % 8 = 3 at 4 + 3 % 6 = 7; the last node's
        // arc at 3 % 5 = 3.
        let mut quiet = Construction::new(&[3]);
        quiet.balanced_nodes(3);
        quiet.holding_arcs(&HoldingChains { chains: vec![vec![0]], volume: 0 }, &[1], 2);
        let network = &quiet.network;
        assert_eq!((network.tails.clone(), network.heads.clone()), (vec![0, 0], vec![2, 2]));
        assert_eq!((network.capacities.clone(), network.costs.clone()), (vec![3, WIDE], vec![7, 3]));

        // Seed 4: an arc to the taker, as 4 % 3 = 1, without a lower bound, as 4 % 5 = 4, of width 4 % 25 = 4 at
        // 4 % 10 = 4; a write-off of 4 at 4 + 4 = 8; the last node's arc at 4.
        let mut drawing = Construction::new(&[4]);
        drawing.balanced_nodes(3);
        drawing.holding_arcs(&HoldingChains { chains: vec![vec![0]], volume: 0 }, &[1], 2);
        let network = &drawing.network;
        assert_eq!((network.tails.clone(), network.heads.clone()), (vec![0, 0, 0], vec![1, 2, 2]));
        assert_eq!(network.lower_bounds, [0, 0, 0]);
        assert_eq!((network.capacities.clone(), network.costs.clone()), (vec![4, 4, WIDE], vec![4, 8, 4]));
    }

    #[test]
    fn test_slack_arcs_by_hand() {
        // Seed 4: each taker at 100 + 4 % 50 = 104, the sink at no cost.
        let mut construction = Construction::new(&[4]);
        construction.slack_arcs(0, &[1, 2], 3);

        let network = &construction.network;
        assert_eq!((network.tails.clone(), network.heads.clone()), (vec![0, 0, 0], vec![1, 2, 3]));
        assert_eq!(network.costs, [104, 104, 0]);
    }

    #[test]
    fn test_amounts_and_assignment_nodes_by_hand() {
        let mut construction = Construction::new(&[7, 5, 31]);
        assert_eq!(construction.amounts(3, 20), [7, 5, 11]);

        let nodes = construction.assignment_nodes(&[7, 5], &[3]);
        let expected = AssignmentNodes { sources: vec![0, 1], demands: vec![(2, 3)], slack: 4, surplus: 5 };
        assert_eq!(nodes, expected);
        assert_eq!(construction.network.supplies, [7, 5, 0, -3, 3, -12]);
        assert_eq!(construction.network.arc_count(), 0, "the nodes draw no arc");
    }

    #[test]
    fn test_source_arcs_by_hand() {
        // Seed 5: every assigned node fed, as 5 % 4 = 1, at 5 % 12 = 5; the surplus at 2 + 5 % 3 = 4.
        let nodes = AssignmentNodes { sources: vec![0], demands: vec![(1, 2), (3, 4)], slack: 6, surplus: 5 };
        let mut construction = Construction::new(&[5]);
        construction.source_arcs(&nodes);

        let network = &construction.network;
        assert_eq!((network.tails.clone(), network.heads.clone()), (vec![0, 0, 0], vec![1, 3, 5]));
        assert_eq!(network.costs, [5, 5, 4]);
    }

    #[test]
    fn test_demand_arcs_by_hand() {
        // Seed 3: the first demand carries to the second's served node, as 3 % 3 = 0, at 1 + 3 % 4 = 4; the slack
        // feeds each served node at 50 + 3 % 20 = 53.
        let nodes = AssignmentNodes { sources: Vec::new(), demands: vec![(0, 1), (2, 3)], slack: 4, surplus: 5 };
        let mut construction = Construction::new(&[3]);
        construction.demand_arcs(&nodes);

        let network = &construction.network;
        assert_eq!(network.tails, [0, 0, 0, 4, 2, 2, 4, 4]);
        assert_eq!(network.heads, [1, 3, 5, 1, 3, 5, 3, 5]);
        assert_eq!(network.costs, [0, 4, 3, 53, 0, 3, 53, 0]);
    }

    #[test]
    fn test_balanced_nodes_and_a_general_arc_by_hand() {
        // Seed 9: two nodes of supply 9 % 13 - 6 = 3, the last of -6.
        let mut nodes = Construction::new(&[9]);
        nodes.balanced_nodes(3);
        assert_eq!(nodes.network.supplies, [3, 3, -6]);

        // Seed 5, 1, 0, 2 on three nodes: from 5 % 3 = 2 to 1 % 3 = 1, fixed, as 0 % 6 = 0, at 1 + 2 % 3 = 3; its
        // cost 5 % 20 = 5.
        let mut arc = Construction::new(&[5, 1, 0, 2]);
        arc.network.supplies = vec![0; 3];
        arc.general_arc();
        let network = &arc.network;
        assert_eq!((network.tails.clone(), network.heads.clone()), (vec![2], vec![1]));
        assert_eq!((network.lower_bounds.clone(), network.capacities.clone()), (vec![3], vec![3]));
        assert_eq!(network.costs, [5]);
    }
}
