// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Min-cost networks of a given size drawn from seeded generators, for the benchmarks: the supply shape and the
//! assignment shape, each with twin arcs that make its optimum tie, the same networks with their costs made distinct,
//! and the square grid with costs.
//!
//! The seeded networks of [`supply_shaped`](crate::supply_shaped) and [`assignment_shaped`](crate::assignment_shaped)
//! draw a handful of nodes, as their seed decides, for the tests; these take the size a benchmark names. Each kind of
//! number is drawn from a generator of its own, so the numbers of a network depend only on the order of the draws
//! within each, never on how the draws of two kinds interleave. Every draw is a uniform draw from a range, so a change
//! to how a range is sampled changes every network.

use std::ops::{Range, RangeInclusive};

use itertools::{Itertools, iproduct};
use rand::RngExt;
use rand::seq::SliceRandom;
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::mincost_network::MincostNetwork;

/// The most a single draw arc, carry arc or holding arc carries: more than any node's supply.
const WIDE: u64 = 1000;

/// The range a grid arc's capacity is drawn from, in units of flow.
const GRID_CAPACITIES: RangeInclusive<u64> = 1..=100;

/// The range a grid arc's cost is drawn from: small, as a model's coefficients usually are, so the optimum often ties.
const GRID_COSTS: RangeInclusive<i64> = 1..=20;

/// The cost of a unit on the grid's slack arc per node of its side: about twice what a unit pays to cross a row at the
/// mean cost, so that the slack takes what crossing the grid would cost more than that, and the grid the rest.
const GRID_SLACK_COST_PER_SIDE: i64 = 20;

/// The generators a network of the supply or the assignment shape is drawn from, one per kind of number. Plain data.
#[derive(Clone, Debug)]
pub struct ShapeDraws {
    /// Which nodes and arcs the network has.
    pub topology: Xoshiro256PlusPlus,
    /// Its supplies, volumes and write-off capacities.
    pub supplies: Xoshiro256PlusPlus,
    /// The cost of every arc.
    pub costs: Xoshiro256PlusPlus,
}

/// The supply-shaped network of `demands` demands: `demands` chains of one to three holding nodes, the first holding
/// a volume of 1 to 40 and every link costing 0 to 2; each holding node drawing to two demands by twin arcs of
/// capacity 20 costing 0 to 9, and writing off to the sink within 0 to 7 at a cost of 4 to 9; each chain's last node
/// leaving its rest to the sink at 0 to 4; each demand needing 1 to 30, covered by the slack source at 100 to 149.
/// Every demand can be covered by the slack and every volume can reach the sink, so it is feasible. Its twin arcs, two
/// parallel arcs of one capacity and one cost, make its optimum tie.
///
/// # Panics
/// Panics if `demands` is zero.
pub fn supply_sized(demands: usize, draws: &mut ShapeDraws) -> MincostNetwork {
    assert!(demands > 0, "called `supply_sized` with no demands");

    let mut construction = Construction::new(draws);
    let chains = construction.holding_chains(demands);
    let takers = construction.takers(demands);
    let slack = construction.network.add_node(-takers.total);
    let sink = construction.network.add_node(-chains.volume);

    construction.chain_arcs(&chains, &takers.nodes, sink);
    for &taker in &takers.nodes {
        construction.arc(slack, taker, WIDE, 100..150);
    }
    construction.arc(slack, sink, WIDE, 0..1);

    construction.network
}

/// The assignment-shaped network of `demands` demands: `demands / 2` sources (at least one), each supplying 1 to 20,
/// feeding three demands' assigned nodes by twin arcs of capacity 20 costing 0 to 11 and the surplus sink at 2 to 4;
/// each demand an assigned node feeding its own served node at no cost, every demand but the last also one later
/// demand's served node at 1 to 4 (a carry, one hop on), and the surplus at 3; each served node needing 1 to 15,
/// covered by the slack source at 50 to 69. Every need can be covered by the slack and every size can reach the
/// surplus, so it is feasible. Its twin arcs make its optimum tie.
///
/// # Panics
/// Panics if `demands` is zero.
pub fn assignment_sized(demands: usize, draws: &mut ShapeDraws) -> MincostNetwork {
    assert!(demands > 0, "called `assignment_sized` with no demands");

    let mut construction = Construction::new(draws);
    let sources = construction.sources((demands / 2).max(1));
    let pairs = construction.demand_pairs(demands);
    let slack = construction.network.add_node(pairs.quantity);
    let surplus = construction.network.add_node(-sources.total);
    let carried_to = carries(&mut construction.draws.topology, demands);

    construction.source_arcs(&sources.nodes, &pairs, surplus);
    construction.demand_arcs(&pairs, &carried_to, slack, surplus);
    construction.arc(slack, surplus, WIDE, 0..1);

    construction.network
}

/// The costs of `network` made distinct by `tie_breaks`, one per arc: each cost `c` of arc `a` made
/// `c · M + tie_breaks[a]`, where `M`, one more than the largest tie-break times the sum of the capacities, passes the
/// tie-break cost of every flow within the capacities. A flow's cost then orders flows by their cost at the old costs
/// first and by their tie-break cost second, so an optimum at the new costs is one at the old costs too.
///
/// # Panics
/// Panics if `tie_breaks` does not hold one value per arc, or a cost does not fit an `i64`; whether it lies within the
/// range a solver admits is the solver's to check.
pub fn distinct_costs(network: &MincostNetwork, tie_breaks: &[u64]) -> Vec<i64> {
    let arc_count = network.arc_count();
    assert!(
        tie_breaks.len() == arc_count,
        "called `distinct_costs` with {} tie-breaks, which should be the arc count (is {arc_count})",
        tie_breaks.len()
    );

    let multiplier =
        i128::try_from(distinct_multiplier(network, tie_breaks)).expect("the multiplier should fit an i128");

    network
        .costs
        .iter()
        .zip(tie_breaks)
        .map(|(&cost, &tie_break)| {
            let distinct = i128::from(cost).strict_mul(multiplier).strict_add(i128::from(tie_break));
            i64::try_from(distinct).expect("a distinct cost should fit an i64")
        })
        .collect()
}

/// The multiplier of [`distinct_costs`]: one more than the largest of `tie_breaks` times the sum of the capacities of
/// `network`.
///
/// # Panics
/// Panics if the product passes `u128::MAX`, which no network of `u64` capacities with fewer than `2^52` arcs and
/// tie-breaks below `2^12` comes near.
fn distinct_multiplier(network: &MincostNetwork, tie_breaks: &[u64]) -> u128 {
    let largest = tie_breaks.iter().copied().max().unwrap_or(0);
    let capacity_sum: u128 = network.capacities.iter().map(|&capacity| u128::from(capacity)).sum();

    u128::from(largest).strict_mul(capacity_sum).strict_add(1)
}

/// The `side` by `side` grid with costs: the source feeding the first node of every row, every node feeding its right
/// neighbour and both its vertical neighbours, the last node of every row feeding the sink; then one arc more, from the
/// source to the sink.
///
/// Nodes come row by row after the source; the sink is last. Arcs come in order: the source to each row's first node;
/// then, row by row and column by column, each node's arc to the right, down and up where that neighbour exists; then
/// each row's last node to the sink; then the slack arc from the source to the sink. Each grid arc's capacity is drawn
/// from 1 to 100 by `capacities`, then, in the same order, its cost from 1 to 20 by `costs`. The source supplies what
/// its arcs into the grid can carry and the sink takes it; the slack arc carries as much at twenty times the side a
/// unit, so the network is feasible. About three arcs per node, no twins.
///
/// # Panics
/// Panics if `side` is zero, or so large that the source's supply or the slack arc's cost passes `i64::MAX`.
pub fn costed_grid(side: usize, capacities: &mut Xoshiro256PlusPlus, costs: &mut Xoshiro256PlusPlus) -> MincostNetwork {
    assert!(side > 0, "called `costed_grid` with a side of zero");

    let mut grid = Grid::new(side, capacities, costs);
    grid.source_arcs();
    grid.neighbour_arcs();
    grid.sink_arcs();

    grid.balanced()
}

/// The grid with costs under construction, and the generators its arcs draw from.
///
/// # Invariant
/// `rows` holds `side` rows of `side` nodes each, numbered row by row after `source`, and `sink` is numbered last.
#[derive(Debug)]
struct Grid<'d> {
    /// The network as drawn so far.
    network: MincostNetwork,
    /// The nodes of each row, from the left.
    rows: Vec<Vec<usize>>,
    /// The node that feeds the first node of every row.
    source: usize,
    /// The node the last node of every row feeds.
    sink: usize,
    /// The generator of the arcs' capacities.
    capacities: &'d mut Xoshiro256PlusPlus,
    /// The generator of the arcs' costs.
    costs: &'d mut Xoshiro256PlusPlus,
}

impl<'d> Grid<'d> {
    /// The nodes of the `side` by `side` grid, its source and its sink, all of no supply, and no arc yet.
    fn new(side: usize, capacities: &'d mut Xoshiro256PlusPlus, costs: &'d mut Xoshiro256PlusPlus) -> Self {
        let mut network = MincostNetwork::default();
        let source = network.add_node(0);
        let rows = (0..side).map(|_| (0..side).map(|_| network.add_node(0)).collect()).collect();
        let sink = network.add_node(0);

        Self { network, rows, source, sink, capacities, costs }
    }

    /// Adds an arc from `tail` to `head`, its capacity drawn from 1 to 100 and its cost from 1 to 20.
    fn arc(&mut self, tail: usize, head: usize) {
        let capacity = self.capacities.random_range(GRID_CAPACITIES);
        let cost = self.costs.random_range(GRID_COSTS);

        self.network.add_arc(tail, head, 0, capacity, cost);
    }

    /// The arcs from the source to the first node of every row, from the top.
    fn source_arcs(&mut self) {
        for row in 0..self.rows.len() {
            self.arc(self.source, self.rows[row][0]);
        }
    }

    /// The arcs from every node to its right, lower and upper neighbour where it exists, row by row and column by
    /// column.
    fn neighbour_arcs(&mut self) {
        let side = self.rows.len();

        for (row, column) in iproduct!(0..side, 0..side) {
            let node = self.rows[row][column];
            let right = (column + 1 < side).then(|| self.rows[row][column + 1]);
            let down = (row + 1 < side).then(|| self.rows[row + 1][column]);
            let up = row.checked_sub(1).map(|above| self.rows[above][column]);
            for neighbour in [right, down, up].into_iter().flatten() {
                self.arc(node, neighbour);
            }
        }
    }

    /// The arcs from the last node of every row to the sink, from the top.
    fn sink_arcs(&mut self) {
        let side = self.rows.len();

        for row in 0..side {
            self.arc(self.rows[row][side - 1], self.sink);
        }
    }

    /// The network with its supplies: the source supplies what its arcs into the grid can carry and the sink takes it;
    /// the slack arc from the source to the sink carries as much at twenty times the side a unit.
    ///
    /// # Panics
    /// Panics if the source's supply or the slack arc's cost passes `i64::MAX`.
    fn balanced(mut self) -> MincostNetwork {
        let side = self.rows.len();
        // The source's arcs are the first `side` arcs, one per row.
        let supply: u64 = self.network.capacities[..side].iter().sum();
        let signed = i64::try_from(supply).expect("the source's capacity should fit an i64");
        (self.network.supplies[self.source], self.network.supplies[self.sink]) = (signed, -signed);

        let side_cost = i64::try_from(side).expect("a side should fit an i64").strict_mul(GRID_SLACK_COST_PER_SIDE);
        self.network.add_arc(self.source, self.sink, 0, supply, side_cost);

        self.network
    }
}

/// A network of the supply or the assignment shape under construction, and the generators it draws from.
#[derive(Debug)]
struct Construction<'d> {
    /// The network as drawn so far.
    network: MincostNetwork,
    /// The generators its numbers are drawn from.
    draws: &'d mut ShapeDraws,
}

/// Nodes laid out with their supplies, and the sum of those supplies. Plain data.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Supplied {
    /// The nodes, in the order laid out.
    nodes: Vec<usize>,
    /// The supplies of `nodes` summed, a demand counting negative.
    total: i64,
}

/// The holding chains of the supply shape, each a run of nodes the first of which holds a volume, and the volumes
/// summed. Plain data.
#[derive(Clone, PartialEq, Eq, Debug)]
struct HoldingChains {
    /// The nodes of each chain, the one holding its volume first.
    chains: Vec<Vec<usize>>,
    /// The volumes of the chains' first nodes, summed.
    volume: i64,
}

/// The demands of the assignment shape, each an assigned node and a served node needing a quantity, and the
/// quantities summed. Plain data.
#[derive(Clone, PartialEq, Eq, Debug)]
struct DemandPairs {
    /// Every demand's assigned node, then its served node.
    pairs: Vec<(usize, usize)>,
    /// The quantities the served nodes need, summed.
    quantity: i64,
}

impl<'d> Construction<'d> {
    /// An empty network whose numbers come from `draws`.
    fn new(draws: &'d mut ShapeDraws) -> Self {
        Self { network: MincostNetwork::default(), draws }
    }

    /// Adds an arc from `tail` to `head` of capacity `upper`, costing a draw from `cost_range`.
    ///
    /// # Panics
    /// Panics if `cost_range` is empty.
    fn arc(&mut self, tail: usize, head: usize, upper: u64, cost_range: Range<u64>) {
        let cost = self.draw_cost(cost_range);

        self.network.add_arc(tail, head, 0, upper, cost);
    }

    /// Adds two parallel arcs from `tail` to `head`, each of capacity `upper` and one cost drawn from `cost_range`.
    ///
    /// # Panics
    /// Panics if `cost_range` is empty.
    fn twin_arcs(&mut self, tail: usize, head: usize, upper: u64, cost_range: Range<u64>) {
        let cost = self.draw_cost(cost_range);

        self.network.add_arc(tail, head, 0, upper, cost);
        self.network.add_arc(tail, head, 0, upper, cost);
    }

    /// A cost drawn from `cost_range`. The costs stay non-negative and are drawn as unsigned numbers: changing how a
    /// cost is drawn changes every pinned network. Negative costs are the seeded test networks' to cover.
    ///
    /// # Panics
    /// Panics if `cost_range` is empty, or reaches past `i64::MAX`, which no shape's range does.
    fn draw_cost(&mut self, cost_range: Range<u64>) -> i64 {
        i64::try_from(self.draws.costs.random_range(cost_range)).expect("a drawn cost should fit an i64")
    }

    /// The supply shape's `demands` chains of one to three holding nodes, the first holding a volume of 1 to 40.
    fn holding_chains(&mut self, demands: usize) -> HoldingChains {
        let mut volume = 0;

        let chains = (0..demands)
            .map(|_| {
                let held: i64 = self.draws.supplies.random_range(1..=40);
                volume += held;
                let length: usize = self.draws.topology.random_range(1..=3);
                (0..length).map(|position| self.network.add_node(if position == 0 { held } else { 0 })).collect()
            })
            .collect();

        HoldingChains { chains, volume }
    }

    /// The supply shape's `count` demand nodes, each needing 1 to 30.
    fn takers(&mut self, count: usize) -> Supplied {
        let amounts: Vec<i64> = (0..count).map(|_| self.draws.supplies.random_range(1..=30)).collect();
        let nodes = amounts.iter().map(|&amount| self.network.add_node(-amount)).collect();

        Supplied { nodes, total: -amounts.iter().sum::<i64>() }
    }

    /// The supply shape's arcs out of its chains: each link at 0 to 2; each holding node's twin arcs of capacity 20 to
    /// two takers at 0 to 9, and its write-off of 0 to 7 to the sink at 4 to 9; each chain's last node to the sink at
    /// 0 to 4.
    ///
    /// # Panics
    /// Panics if a chain is empty, or a taker drawn is not one of `takers`.
    fn chain_arcs(&mut self, chains: &HoldingChains, takers: &[usize], sink: usize) {
        let mut drawn_takers = DistinctDraws::new(takers.len());

        for chain in &chains.chains {
            for (&from, &to) in chain.iter().tuple_windows() {
                self.arc(from, to, WIDE, 0..3);
            }
            for &holding in chain {
                for &taker in drawn_takers.draw(&mut self.draws.topology, takers.len().min(2)) {
                    self.twin_arcs(holding, takers[taker], 20, 0..10);
                }
                let write_off = self.draws.supplies.random_range(0..8);
                self.arc(holding, sink, write_off, 4..10);
            }
            self.arc(chain[chain.len() - 1], sink, WIDE, 0..5);
        }
    }

    /// The assignment shape's `count` sources, each supplying 1 to 20.
    fn sources(&mut self, count: usize) -> Supplied {
        let sizes: Vec<i64> = (0..count).map(|_| self.draws.supplies.random_range(1..=20)).collect();
        let nodes = sizes.iter().map(|&size| self.network.add_node(size)).collect();

        Supplied { nodes, total: sizes.iter().sum() }
    }

    /// The assignment shape's `demands` demands, each an assigned node and a served node needing 1 to 15.
    fn demand_pairs(&mut self, demands: usize) -> DemandPairs {
        let quantities: Vec<i64> = (0..demands).map(|_| self.draws.supplies.random_range(1..=15)).collect();
        let pairs =
            quantities.iter().map(|&quantity| (self.network.add_node(0), self.network.add_node(-quantity))).collect();

        DemandPairs { pairs, quantity: quantities.iter().sum() }
    }

    /// The assignment shape's arcs out of its sources: twin arcs of capacity 20 from each to three demands' assigned
    /// nodes at 0 to 11, and one to the surplus sink at 2 to 4.
    fn source_arcs(&mut self, sources: &[usize], demands: &DemandPairs, surplus: usize) {
        let mut drawn_demands = DistinctDraws::new(demands.pairs.len());

        for &source in sources {
            for &demand in drawn_demands.draw(&mut self.draws.topology, demands.pairs.len().min(3)) {
                self.twin_arcs(source, demands.pairs[demand].0, 20, 0..12);
            }
            self.arc(source, surplus, WIDE, 2..5);
        }
    }

    /// The assignment shape's arcs out of its demands: each assigned node to its own served node at no cost, to the
    /// served node `carried_to` names at 1 to 4 where it names one, and to the surplus at 3; the slack source to each
    /// served node at 50 to 69.
    ///
    /// # Panics
    /// Panics if `carried_to` holds another number of entries than `demands` holds pairs, or names a demand that
    /// `demands` does not hold.
    fn demand_arcs(&mut self, demands: &DemandPairs, carried_to: &[Option<usize>], slack: usize, surplus: usize) {
        for (&(assigned, served), &later) in demands.pairs.iter().zip_eq(carried_to) {
            self.arc(assigned, served, WIDE, 0..1);
            if let Some(later) = later {
                self.arc(assigned, demands.pairs[later].1, WIDE, 1..5);
            }
            self.arc(assigned, surplus, WIDE, 3..4);
            self.arc(slack, served, WIDE, 50..70);
        }
    }
}

/// A pool of indices that distinct ones are drawn from, again and again.
///
/// # Invariant
/// The pool stays a permutation of the indices it was made with: a draw shuffles part of it and sorts that part in
/// place, so every draw is uniform over all indices, whatever the draws before it took.
#[derive(Clone, Debug)]
struct DistinctDraws {
    pool: Vec<usize>,
}

impl DistinctDraws {
    /// The draws from the indices `0..size`.
    fn new(size: usize) -> Self {
        Self { pool: (0..size).collect() }
    }

    /// `count` distinct indices of the pool drawn uniformly by `rng`, ascending.
    ///
    /// # Panics
    /// Panics if `count` passes the size of the pool.
    fn draw(&mut self, rng: &mut Xoshiro256PlusPlus, count: usize) -> &[usize] {
        let size = self.pool.len();
        assert!(
            count <= size,
            "called `DistinctDraws::draw` for {count} indices, which should be <= the pool's size (is {size})"
        );

        let (chosen, _) = self.pool.partial_shuffle(rng, count);
        chosen.sort_unstable();
        chosen
    }
}

/// The demand each of `demands` demands carries to, one hop on, drawn by `topology`: a later demand for every demand
/// but the last, which has none.
fn carries(topology: &mut Xoshiro256PlusPlus, demands: usize) -> Vec<Option<usize>> {
    (0..demands)
        .map(|position| (position + 1 < demands).then(|| topology.random_range(position + 1..demands)))
        .collect()
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;

    /// The generator seeded with `seed`.
    fn rng(seed: u64) -> Xoshiro256PlusPlus {
        Xoshiro256PlusPlus::seed_from_u64(seed)
    }

    /// Draws for a network, each kind from its own seed.
    fn draws() -> ShapeDraws {
        ShapeDraws { topology: rng(1), supplies: rng(2), costs: rng(3) }
    }

    /// A construction with `count` nodes of no supply, numbered from zero.
    fn with_nodes(draws: &mut ShapeDraws, count: usize) -> Construction<'_> {
        let mut construction = Construction::new(draws);
        for _ in 0..count {
            construction.network.add_node(0);
        }

        construction
    }

    /// Asserts that each arc of `firsts` and the arc after it are twins of `network`: one tail of `tails`, one head of
    /// `heads`, capacity 20 and one cost within `costs`.
    fn assert_twins(network: &MincostNetwork, firsts: &[usize], tails: &[usize], heads: &[usize], costs: Range<i64>) {
        for &first in firsts {
            let second = first + 1;
            assert_eq!((network.tails[first], network.heads[first]), (network.tails[second], network.heads[second]));
            assert_eq!((network.capacities[first], network.capacities[second]), (20, 20), "twin {first}");
            assert_eq!(network.costs[first], network.costs[second], "twin {first}");
            assert!(costs.contains(&network.costs[first]), "twin {first} costs {}", network.costs[first]);

            let (tail, head) = (network.tails[first], network.heads[first]);
            assert!(tails.contains(&tail) && heads.contains(&head), "twin {first} runs from {tail} to {head}");
        }
    }

    #[test]
    fn test_distinct_draws_are_ascending_distinct_and_keep_the_pool() {
        let mut pool = DistinctDraws::new(20);
        let mut topology = rng(11);

        for count in [0, 1, 7, 20] {
            let drawn = pool.draw(&mut topology, count).to_vec();
            assert_eq!(drawn.len(), count);
            assert!(drawn.is_sorted_by(|earlier, later| earlier < later), "{drawn:?}");

            let mut kept = pool.pool.clone();
            kept.sort_unstable();
            assert_eq!(kept, (0..20).collect::<Vec<_>>());
        }
    }

    #[test]
    #[should_panic(expected = "called `DistinctDraws::draw` for 4 indices, which should be <= the pool's size (is 3)")]
    fn test_a_draw_past_the_pool_is_refused() {
        let _ = DistinctDraws::new(3).draw(&mut rng(0), 4);
    }

    #[test]
    fn test_holding_chains_number_their_nodes_in_turn_and_sum_their_volumes() {
        let mut draws = draws();
        let mut construction = Construction::new(&mut draws);
        let chains = construction.holding_chains(5);

        let nodes: Vec<usize> = chains.chains.iter().flatten().copied().collect();
        assert_eq!(nodes, (0..nodes.len()).collect::<Vec<_>>());
        assert!(chains.chains.iter().all(|chain| (1..=3).contains(&chain.len())), "{chains:?}");

        let supplies = &construction.network.supplies;
        let firsts: i64 = chains.chains.iter().map(|chain| supplies[chain[0]]).sum();
        assert_eq!(firsts, chains.volume);
        assert_eq!(supplies.iter().sum::<i64>(), chains.volume, "only the first node of a chain holds a volume");
        assert!(chains.chains.iter().all(|chain| (1..=40).contains(&supplies[chain[0]])), "{supplies:?}");
    }

    #[test]
    fn test_demands_and_sources_sum_their_supplies() {
        let mut draws = draws();
        let mut construction = Construction::new(&mut draws);
        let sources = construction.sources(3);
        let demands = construction.takers(4);
        let pairs = construction.demand_pairs(2);

        assert_eq!((sources.nodes, demands.nodes.clone()), (vec![0, 1, 2], vec![3, 4, 5, 6]));
        assert_eq!(pairs.pairs, [(7, 8), (9, 10)]);

        let supplies = &construction.network.supplies;
        assert_eq!(sources.total, supplies[0..3].iter().sum::<i64>());
        assert_eq!(demands.total, supplies[3..7].iter().sum::<i64>());
        assert!(demands.total < 0 && sources.total > 0);
        assert_eq!((supplies[7], supplies[9]), (0, 0), "an assigned node holds no supply");
        assert_eq!(pairs.quantity, -(supplies[8] + supplies[10]));
    }

    #[test]
    fn test_chain_arcs_by_hand() {
        // Holding nodes 0 -> 1 and 2, takers 3, 4 and 5, sink 6.
        let mut draws = draws();
        let mut construction = with_nodes(&mut draws, 7);
        let chains = HoldingChains { chains: vec![vec![0, 1], vec![2]], volume: 0 };
        construction.chain_arcs(&chains, &[3, 4, 5], 6);
        let network = &construction.network;

        // Per holding node two twin pairs and a write-off; per chain its links and its last node's arc to the sink.
        assert_eq!(network.arc_count(), 18);
        assert_eq!((network.tails[0], network.heads[0], network.capacities[0]), (0, 1, WIDE));
        assert!((0..3).contains(&network.costs[0]));

        for (write_off, holding) in [(5, 0), (10, 1), (16, 2)] {
            assert_eq!((network.tails[write_off], network.heads[write_off]), (holding, 6), "arc {write_off}");
            assert!(network.capacities[write_off] < 8, "arc {write_off}");
            assert!((4..10).contains(&network.costs[write_off]), "arc {write_off}");
        }
        for (last, holding) in [(11, 1), (17, 2)] {
            assert_eq!((network.tails[last], network.heads[last], network.capacities[last]), (holding, 6, WIDE));
            assert!((0..5).contains(&network.costs[last]), "arc {last}");
        }

        let firsts = [1, 3, 6, 8, 12, 14];
        assert_twins(network, &firsts, &[0, 1, 2], &[3, 4, 5], 0..10);
        for holding_twins in firsts.chunks(2) {
            let heads = [network.heads[holding_twins[0]], network.heads[holding_twins[1]]];
            assert!(heads[0] < heads[1], "a holding node's two takers should be distinct and ascending: {heads:?}");
        }
    }

    #[test]
    fn test_source_arcs_by_hand() {
        // Sources 0 and 1, demands (2, 3) to (8, 9), surplus 10.
        let mut draws = draws();
        let mut construction = with_nodes(&mut draws, 11);
        let demands = DemandPairs { pairs: vec![(2, 3), (4, 5), (6, 7), (8, 9)], quantity: 0 };
        construction.source_arcs(&[0, 1], &demands, 10);
        let network = &construction.network;

        // Per source three twin pairs, then its arc to the surplus.
        assert_eq!(network.arc_count(), 14);
        for (surplus_arc, source) in [(6, 0), (13, 1)] {
            let arc = (network.tails[surplus_arc], network.heads[surplus_arc], network.capacities[surplus_arc]);
            assert_eq!(arc, (source, 10, WIDE), "arc {surplus_arc}");
            assert!((2..5).contains(&network.costs[surplus_arc]), "arc {surplus_arc}");
        }

        let firsts = [0, 2, 4, 7, 9, 11];
        assert_twins(network, &firsts, &[0, 1], &[2, 4, 6, 8], 0..12);
        for source_twins in firsts.chunks(3) {
            let heads: Vec<usize> = source_twins.iter().map(|&first| network.heads[first]).collect();
            assert!(heads.is_sorted_by(|earlier, later| earlier < later), "{heads:?}");
        }
    }

    #[test]
    fn test_demand_arcs_by_hand() {
        // Demands (0, 1), (2, 3) and (4, 5), slack 6, surplus 7; the first two carry to the last.
        let mut draws = draws();
        let mut construction = with_nodes(&mut draws, 8);
        let demands = DemandPairs { pairs: vec![(0, 1), (2, 3), (4, 5)], quantity: 0 };
        construction.demand_arcs(&demands, &[Some(2), Some(2), None], 6, 7);
        let network = &construction.network;

        assert_eq!(network.tails, [0, 0, 0, 6, 2, 2, 2, 6, 4, 4, 6]);
        assert_eq!(network.heads, [1, 5, 7, 1, 3, 5, 7, 3, 5, 7, 5]);
        assert!(network.capacities.iter().all(|&capacity| capacity == WIDE));

        let ranges = [0..1, 1..5, 3..4, 50..70, 0..1, 1..5, 3..4, 50..70, 0..1, 3..4, 50..70];
        for (arc, range) in ranges.into_iter().enumerate() {
            assert!(range.contains(&network.costs[arc]), "arc {arc} costs {}", network.costs[arc]);
        }
    }

    #[test]
    fn test_carries_go_one_hop_on_to_a_later_demand() {
        let mut topology = rng(7);
        let carried_to = carries(&mut topology, 6);

        assert_eq!(carried_to.len(), 6);
        for (position, &later) in carried_to.iter().enumerate().take(5) {
            let later = later.expect("every demand but the last should carry");
            assert!((position + 1..6).contains(&later), "demand {position} carries to {later}");
        }
        assert_eq!(carried_to[5], None, "the last demand has no later one");
        assert_eq!(carries(&mut topology, 1), [None]);
    }

    #[test]
    fn test_both_shapes_are_balanced_and_of_their_size() {
        for demands in [1, 2, 7, 40] {
            let supply = supply_sized(demands, &mut draws());
            let assignment = assignment_sized(demands, &mut draws());

            for network in [&supply, &assignment] {
                assert!(network.shaped().is_ok(), "{demands} demands");
                assert_eq!(network.supplies.iter().sum::<i64>(), 0, "{demands} demands");
                assert!(network.lower_bounds.iter().all(|&lower| lower == 0));
                assert!(network.costs.iter().all(|&cost| cost >= 0));
            }

            // Supply: the demands, then the slack source and the sink, after the holding nodes.
            let takers = supply.supplies.iter().filter(|&&supply| supply < 0).count();
            assert_eq!(takers, demands + 1, "every demand and the sink take");
            // Assignment: a source per two demands, two nodes per demand, the slack source and the surplus.
            assert_eq!(assignment.node_count(), (demands / 2).max(1) + 2 * demands + 2);
        }
    }

    #[test]
    fn test_distinct_costs_by_hand() {
        let mut network = MincostNetwork::default();
        let (from, to) = (network.add_node(1), network.add_node(-1));
        network.add_arc(from, to, 0, 2, 3);
        network.add_arc(from, to, 0, 1, 0);

        // The largest tie-break 7 times the capacities' sum 3, plus one.
        assert_eq!(distinct_multiplier(&network, &[7, 5]), 22);
        assert_eq!(distinct_costs(&network, &[7, 5]), [3 * 22 + 7, 5]);
    }

    #[test]
    #[should_panic(expected = "called `distinct_costs` with 1 tie-breaks, which should be the arc count (is 2)")]
    fn test_distinct_costs_refuse_tie_breaks_of_another_length() {
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 0, 0, 1, 1);
        network.add_arc(0, 0, 0, 1, 1);

        let _ = distinct_costs(&network, &[1]);
    }

    #[test]
    fn test_the_costed_grid_by_hand() {
        let grid = costed_grid(2, &mut rng(5), &mut rng(6));

        // Nodes: source 0, row 0 = 1 2, row 1 = 3 4, sink 5; the grid's ten arcs, then the slack arc.
        assert_eq!(grid.tails, [0, 0, 1, 1, 2, 3, 3, 4, 2, 4, 0]);
        assert_eq!(grid.heads, [1, 3, 2, 3, 4, 4, 1, 2, 5, 5, 5]);

        let supply = grid.capacities[0] + grid.capacities[1];
        assert_eq!(grid.capacities[10], supply);
        let signed = i64::try_from(supply).expect("a small supply should fit an i64");
        assert_eq!(grid.supplies, [signed, 0, 0, 0, 0, -signed]);

        assert!(grid.capacities[..10].iter().all(|capacity| GRID_CAPACITIES.contains(capacity)));
        assert!(grid.costs[..10].iter().all(|cost| GRID_COSTS.contains(cost)), "{:?}", grid.costs);
        assert_eq!(grid.costs[10], 2 * GRID_SLACK_COST_PER_SIDE);
        assert!(grid.lower_bounds.iter().all(|&lower| lower == 0));
    }

    #[test]
    fn test_the_grid_steps_by_hand() {
        // Source 0, row 0 = 1 2, row 1 = 3 4, sink 5.
        let (mut capacities, mut costs) = (rng(5), rng(6));
        let mut grid = Grid::new(2, &mut capacities, &mut costs);
        assert_eq!(grid.rows, [[1, 2], [3, 4]]);
        assert_eq!((grid.source, grid.sink, grid.network.node_count()), (0, 5, 6));

        grid.source_arcs();
        assert_eq!((grid.network.tails.clone(), grid.network.heads.clone()), (vec![0, 0], vec![1, 3]));

        grid.neighbour_arcs();
        assert_eq!(grid.network.tails[2..], [1, 1, 2, 3, 3, 4]);
        assert_eq!(grid.network.heads[2..], [2, 3, 4, 4, 1, 2]);

        grid.sink_arcs();
        assert_eq!((grid.network.tails[8..].to_vec(), grid.network.heads[8..].to_vec()), (vec![2, 4], vec![5, 5]));

        let supply = grid.network.capacities[0] + grid.network.capacities[1];
        let network = grid.balanced();
        assert_eq!(network.capacities[10], supply, "the slack arc carries what the source's arcs can");
        assert_eq!((network.tails[10], network.heads[10], network.costs[10]), (0, 5, 40));
    }

    #[test]
    fn test_the_costed_grid_has_three_arcs_a_node() {
        for side in [1, 2, 5, 9] {
            let grid = costed_grid(side, &mut rng(5), &mut rng(6));
            let counts = (grid.node_count(), grid.arc_count());
            assert_eq!(counts, (side * side + 2, 3 * side * side - side + 1), "side {side}");
            assert!(grid.shaped().is_ok());
        }
    }

    #[test]
    fn test_the_generators_draw_the_pinned_networks() {
        // Pinned: a change of any draw, of an arc's place in the order or of any number changes these fingerprints,
        // and every benchmark input with them.
        let supply = supply_sized(20, &mut draws());
        let assignment = assignment_sized(32, &mut draws());
        let tie_breaks: Vec<u64> = (1..=4095).cycle().take(supply.arc_count()).collect();
        let distinct = MincostNetwork { costs: distinct_costs(&supply, &tie_breaks), ..supply.clone() };
        let grid = costed_grid(9, &mut rng(5), &mut rng(6));

        let drawn = [&supply, &assignment, &distinct, &grid].map(MincostNetwork::fingerprint);
        assert_eq!(drawn, [0x4135_f27a_59fa_69a8, 0x714b_b9c3_29e7_e990, 0xa1e7_63b4_fe47_8b5f, 0x1780_7c45_638a_b55e]);
    }
}
