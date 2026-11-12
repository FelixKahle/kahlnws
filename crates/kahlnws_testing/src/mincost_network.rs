// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A min-cost network as plain numbers, its fingerprint, and the view of a network whose shape holds.

use core::fmt;
use core::iter::FusedIterator;
use core::slice;

use itertools::Itertools;

/// A min-cost network as plain numbers: node `v` has supply `supplies[v]`; arc `a` runs from `tails[a]` to `heads[a]`
/// and carries between `lower_bounds[a]` and `capacities[a]` units at `costs[a]` a unit.
///
/// The nodes are `0..supplies.len()` and the arcs `0..tails.len()`, so neither count is a field of its own, and the
/// columns are laid out as a solver's problem borrows them. Self-loops and parallel arcs are allowed. Plain data:
/// every function taking one checks its shape first, refusing arc columns of different lengths and an end that is not
/// a node. A lower bound above its capacity is no breach of shape: such a network has no flow within its bounds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct MincostNetwork {
    /// The node each arc leaves, one per arc.
    pub tails: Vec<usize>,
    /// The node each arc enters, one per arc.
    pub heads: Vec<usize>,
    /// The least flow of each arc, in units of flow, one per arc.
    pub lower_bounds: Vec<u64>,
    /// The most flow of each arc, in units of flow, one per arc.
    pub capacities: Vec<u64>,
    /// The cost of a unit of flow on each arc, in units of cost, of either sign, one per arc.
    pub costs: Vec<i64>,
    /// The supply of each node, in units of flow: positive where it sends flow, negative where it takes flow.
    pub supplies: Vec<i64>,
}

impl MincostNetwork {
    /// The number of nodes: one per supply.
    pub fn node_count(&self) -> usize {
        self.supplies.len()
    }

    /// The number of arcs: one per tail.
    pub fn arc_count(&self) -> usize {
        self.tails.len()
    }

    /// Adds a node of supply `supply` and answers its index.
    pub fn add_node(&mut self, supply: i64) -> usize {
        self.supplies.push(supply);

        self.supplies.len() - 1
    }

    /// Adds an arc from `tail` to `head` with its bounds and cost, and answers its index.
    pub fn add_arc(&mut self, tail: usize, head: usize, lower_bound: u64, capacity: u64, cost: i64) -> usize {
        self.tails.push(tail);
        self.heads.push(head);
        self.lower_bounds.push(lower_bound);
        self.capacities.push(capacity);
        self.costs.push(cost);

        self.tails.len() - 1
    }

    /// The network of the nodes of supply `supplies`, in order, and the arcs `arcs`, in order, each given as its tail,
    /// head, lower bound, capacity and cost: the short form of a hand-written fixture.
    pub fn from_arcs(supplies: &[i64], arcs: &[(usize, usize, u64, u64, i64)]) -> Self {
        let mut network = Self::default();
        for &supply in supplies {
            network.add_node(supply);
        }
        for &(tail, head, lower_bound, capacity, cost) in arcs {
            network.add_arc(tail, head, lower_bound, capacity, cost);
        }

        network
    }

    /// A fingerprint of every number of the network: the FNV-1a fold of each arc's tail and head in turn, then the
    /// lower bounds, the capacities, the costs and the supplies, each number as 64 bits, in column order. It changes
    /// when any number or the order of the arcs does, so a test pins a generated network by it.
    ///
    /// # Panics
    /// Panics if the shape of the network is broken, or an end does not fit 64 bits, which no index of a machine's
    /// memory passes.
    pub fn fingerprint(&self) -> u64 {
        let shaped = self.shaped().unwrap_or_else(|breach| {
            panic!("called `MincostNetwork::fingerprint` on a network of broken shape: {breach}")
        });

        let index = |value: usize| u64::try_from(value).expect("an index should fit a u64");
        let numbers = shaped
            .arcs()
            .flat_map(|arc| [index(arc.tail), index(arc.head)])
            .chain(self.lower_bounds.iter().copied())
            .chain(self.capacities.iter().copied())
            .chain(self.costs.iter().map(|cost| cost.cast_unsigned()))
            .chain(self.supplies.iter().map(|supply| supply.cast_unsigned()));

        numbers.fold(FNV_OFFSET_BASIS, |fold, value| (fold ^ value).wrapping_mul(FNV_PRIME))
    }

    /// The network as a view whose shape holds.
    ///
    /// # Errors
    /// Returns the first breach of shape: [`ShapeBreach::Length`] for the first arc column, in the order heads, lower
    /// bounds, capacities, costs, whose length is not the tails'; otherwise [`ShapeBreach::EndOutside`] for the lowest
    /// arc with an end that is not a node, its tail before its head.
    pub(crate) fn shaped(&self) -> Result<ShapedNetwork<'_>, ShapeBreach> {
        match length_breach(self).or_else(|| end_breach(&self.tails, &self.heads, self.node_count())) {
            Some(breach) => Err(breach),
            None => Ok(ShapedNetwork { network: self }),
        }
    }
}

/// The offset basis of the 64-bit FNV-1a fold.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;

/// The prime of the 64-bit FNV-1a fold.
const FNV_PRIME: u64 = 0x100_0000_01b3;

/// One of the arc columns of a [`MincostNetwork`] whose length is compared with the tails'.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Table {
    /// The heads.
    Heads,
    /// The lower bounds.
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

/// How the shape of a [`MincostNetwork`] is broken.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub(crate) enum ShapeBreach {
    /// An arc column holds another number of entries than there are tails.
    #[error("the {table} hold {length} entries for {arc_count} arcs")]
    Length {
        /// The column.
        table: Table,
        /// Its number of entries.
        length: usize,
        /// The number of tails, one per arc.
        arc_count: usize,
    },
    /// An arc has an end that is not a node.
    #[error("arc {arc} has end {end}, outside the {node_count} nodes")]
    EndOutside {
        /// The arc.
        arc: usize,
        /// Its first end that is not a node, the tail before the head.
        end: usize,
        /// The number of nodes.
        node_count: usize,
    },
}

/// The first arc column of `network`, in the order heads, lower bounds, capacities, costs, whose length is not the
/// tails'.
fn length_breach(network: &MincostNetwork) -> Option<ShapeBreach> {
    let arc_count = network.arc_count();
    let lengths = [
        (Table::Heads, network.heads.len()),
        (Table::LowerBounds, network.lower_bounds.len()),
        (Table::Capacities, network.capacities.len()),
        (Table::Costs, network.costs.len()),
    ];

    lengths.into_iter().find(|&(_, length)| length != arc_count).map(|(table, length)| ShapeBreach::Length {
        table,
        length,
        arc_count,
    })
}

/// The lowest arc of `tails` and `heads` with an end at or past `node_count`, and that end, the tail before the head.
fn end_breach(tails: &[usize], heads: &[usize], node_count: usize) -> Option<ShapeBreach> {
    tails.iter().zip(heads).enumerate().find_map(|(arc, (&tail, &head))| {
        [tail, head].into_iter().find(|&end| end >= node_count).map(|end| ShapeBreach::EndOutside {
            arc,
            end,
            node_count,
        })
    })
}

/// A [`MincostNetwork`] whose shape holds. Only [`MincostNetwork::shaped`] makes one.
///
/// # Invariant
/// Every arc column holds one entry per tail, and every tail and head is below the node count.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShapedNetwork<'n> {
    network: &'n MincostNetwork,
}

/// One arc of a [`ShapedNetwork`]. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct NetworkArc {
    /// The node it leaves, below the node count.
    pub(crate) tail: usize,
    /// The node it enters, below the node count.
    pub(crate) head: usize,
    /// The least flow it carries, in units of flow.
    pub(crate) lower_bound: u64,
    /// The most flow it carries, in units of flow.
    pub(crate) capacity: u64,
    /// The cost of a unit of flow on it, in units of cost.
    pub(crate) cost: i64,
}

impl<'n> ShapedNetwork<'n> {
    /// The number of nodes.
    pub(crate) fn node_count(&self) -> usize {
        self.network.node_count()
    }

    /// The number of arcs.
    pub(crate) fn arc_count(&self) -> usize {
        self.network.arc_count()
    }

    /// The supply of each node.
    pub(crate) fn supplies(&self) -> &'n [i64] {
        &self.network.supplies
    }

    /// The least flow of each arc, one per arc.
    pub(crate) fn lower_bounds(&self) -> &'n [u64] {
        &self.network.lower_bounds
    }

    /// The most flow of each arc, one per arc.
    pub(crate) fn capacities(&self) -> &'n [u64] {
        &self.network.capacities
    }

    /// The arcs in ascending order.
    pub(crate) fn arcs(&self) -> ArcsIter<'n> {
        let network = self.network;

        ArcsIter {
            tails: network.tails.iter(),
            heads: network.heads.iter(),
            lower_bounds: network.lower_bounds.iter(),
            capacities: network.capacities.iter(),
            costs: network.costs.iter(),
        }
    }

    /// The lowest arc at which the running sum of the magnitudes of the costs, each times its arc's capacity, passes
    /// `limit`; `None` where the whole sum stays within it.
    pub(crate) fn arc_past_cost_total(&self, limit: u128) -> Option<usize> {
        let mut total = 0_u128;

        self.arcs().position(|arc| {
            // A product is below `2^63 · 2^64 = 2^127`, so it fits a `u128`; only the sum can pass.
            let product = u128::from(arc.cost.unsigned_abs()) * u128::from(arc.capacity);
            match total.checked_add(product) {
                Some(sum) if sum <= limit => {
                    total = sum;
                    false
                }
                _ => true,
            }
        })
    }

    /// Writes to `excesses`, cleared first, what `flows`, one per arc, leaves at each node: its supply less the flow
    /// leaving it plus the flow entering it. A self-loop leaves and enters its node alike.
    ///
    /// # Panics
    /// Panics if `flows` does not hold one value per arc.
    pub(crate) fn excesses_into(&self, flows: &[u64], excesses: &mut Vec<i128>) {
        let arc_count = self.arc_count();
        assert!(
            flows.len() == arc_count,
            "called `ShapedNetwork::excesses_into` with {} flows, which should be the arc count (is {arc_count})",
            flows.len()
        );

        excesses.clear();
        excesses.extend(self.supplies().iter().map(|&supply| i128::from(supply)));

        // No sum overflows: each is a supply and at most one flow below `2^64` per arc, far from `2^127` for any arc
        // count a machine holds.
        for (arc, &flow) in self.arcs().zip_eq(flows) {
            excesses[arc.tail] -= i128::from(flow);
            excesses[arc.head] += i128::from(flow);
        }
    }

    /// The cost of `flows`, one per arc, in units of cost times units of flow.
    ///
    /// # Panics
    /// Panics if `flows` does not hold one value per arc, or if a partial sum passes the range of an `i128`, which a
    /// flow within the capacities of a network whose costs at capacity sum to at most `i128::MAX` in magnitude never
    /// does.
    pub(crate) fn cost_of(&self, flows: &[u64]) -> i128 {
        let arc_count = self.arc_count();
        assert!(
            flows.len() == arc_count,
            "called `ShapedNetwork::cost_of` with {} flows, which should be the arc count (is {arc_count})",
            flows.len()
        );

        // Each product is of an `i64` and a `u64`, so within `±2^127`: only the sum can pass.
        self.arcs()
            .zip(flows)
            .fold(0_i128, |total, (arc, &flow)| total.strict_add(i128::from(arc.cost) * i128::from(flow)))
    }
}

/// The arcs of a [`ShapedNetwork`], in ascending order.
///
/// # Invariant
/// The five column iterators have equal lengths, which the [`ShapedNetwork`] they come from guarantees, so they end
/// together from either side.
#[derive(Clone, Debug)]
pub(crate) struct ArcsIter<'n> {
    tails: slice::Iter<'n, usize>,
    heads: slice::Iter<'n, usize>,
    lower_bounds: slice::Iter<'n, u64>,
    capacities: slice::Iter<'n, u64>,
    costs: slice::Iter<'n, i64>,
}

impl Iterator for ArcsIter<'_> {
    type Item = NetworkArc;

    fn next(&mut self) -> Option<NetworkArc> {
        Some(NetworkArc {
            tail: *self.tails.next()?,
            head: *self.heads.next()?,
            lower_bound: *self.lower_bounds.next()?,
            capacity: *self.capacities.next()?,
            cost: *self.costs.next()?,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.tails.size_hint()
    }
}

impl DoubleEndedIterator for ArcsIter<'_> {
    fn next_back(&mut self) -> Option<NetworkArc> {
        Some(NetworkArc {
            tail: *self.tails.next_back()?,
            head: *self.heads.next_back()?,
            lower_bound: *self.lower_bounds.next_back()?,
            capacity: *self.capacities.next_back()?,
            cost: *self.costs.next_back()?,
        })
    }
}

impl ExactSizeIterator for ArcsIter<'_> {}

impl FusedIterator for ArcsIter<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Node 0 of supply 3 sends to node 1 of supply -3 over arc 0 of bounds 1..=4 and cost 5; a self-loop at node 1
    /// of bounds 0..=2 and cost -1.
    fn pair() -> MincostNetwork {
        let mut network = MincostNetwork::default();
        let (from, to) = (network.add_node(3), network.add_node(-3));
        network.add_arc(from, to, 1, 4, 5);
        network.add_arc(to, to, 0, 2, -1);

        network
    }

    /// The view of `network`, whose shape the test knows to hold.
    fn shaped(network: &MincostNetwork) -> ShapedNetwork<'_> {
        network.shaped().expect("the test's network should be of sound shape")
    }

    /// The excesses `flows` leaves on `network`.
    fn excesses(network: &MincostNetwork, flows: &[u64]) -> Vec<i128> {
        let mut excesses = vec![7; 5];
        shaped(network).excesses_into(flows, &mut excesses);

        excesses
    }

    #[test]
    fn test_add_node_and_add_arc_answer_their_indices_and_fill_every_column() {
        let mut network = MincostNetwork::default();
        assert_eq!((network.add_node(2), network.add_node(-2)), (0, 1));
        assert_eq!((network.add_arc(0, 1, 0, 3, 5), network.add_arc(1, 1, 1, 1, -1)), (0, 1));

        assert_eq!((network.node_count(), network.arc_count()), (2, 2));
        assert_eq!((network.tails, network.heads), (vec![0, 1], vec![1, 1]));
        assert_eq!((network.lower_bounds, network.capacities), (vec![0, 1], vec![3, 1]));
        assert_eq!((network.costs, network.supplies), (vec![5, -1], vec![2, -2]));
    }

    #[test]
    fn test_from_arcs_adds_the_nodes_then_the_arcs_in_order_by_hand() {
        assert_eq!(MincostNetwork::from_arcs(&[3, -3], &[(0, 1, 1, 4, 5), (1, 1, 0, 2, -1)]), pair());
        assert_eq!(MincostNetwork::from_arcs(&[], &[]), MincostNetwork::default());
    }

    #[test]
    fn test_shaped_names_the_first_breach_in_column_order() {
        assert!(pair().shaped().is_ok());

        let mut short = pair();
        short.costs.pop();
        short.heads.push(0);
        short.tails[0] = 7;

        let breach = short.shaped().expect_err("the shape should be refused: the heads are one too many");
        assert_eq!(breach, ShapeBreach::Length { table: Table::Heads, length: 3, arc_count: 2 });

        short.heads.pop();
        let breach = short.shaped().expect_err("the shape should be refused: a cost is missing");
        assert_eq!(breach, ShapeBreach::Length { table: Table::Costs, length: 1, arc_count: 2 });
        assert_eq!(breach.to_string(), "the costs hold 1 entries for 2 arcs");

        let mut long = pair();
        long.lower_bounds.push(0);
        long.capacities.push(0);
        let breach = long.shaped().expect_err("the shape should be refused: a lower bound is too many");
        assert_eq!(breach, ShapeBreach::Length { table: Table::LowerBounds, length: 3, arc_count: 2 });

        let mut outside = pair();
        outside.heads[1] = 2;
        outside.tails[1] = 3;
        let breach = outside.shaped().expect_err("the shape should be refused: nodes 2 and 3 are no nodes");
        assert_eq!(breach, ShapeBreach::EndOutside { arc: 1, end: 3, node_count: 2 });
        assert_eq!(breach.to_string(), "arc 1 has end 3, outside the 2 nodes");
    }

    #[test]
    fn test_end_breach_takes_the_lowest_arc_and_its_tail_first() {
        assert_eq!(end_breach(&[0, 1], &[1, 1], 2), None);
        assert_eq!(
            end_breach(&[0, 1, 0], &[1, 2, 5], 2),
            Some(ShapeBreach::EndOutside { arc: 1, end: 2, node_count: 2 })
        );
        assert_eq!(end_breach(&[4], &[3], 2), Some(ShapeBreach::EndOutside { arc: 0, end: 4, node_count: 2 }));
        assert_eq!(end_breach(&[], &[], 0), None);
    }

    #[test]
    fn test_table_names_its_column() {
        let names = [Table::Heads, Table::LowerBounds, Table::Capacities, Table::Costs].map(|table| table.to_string());

        assert_eq!(names, ["heads", "lower bounds", "capacities", "costs"]);
    }

    #[test]
    fn test_the_fingerprint_folds_every_number_in_column_order_by_hand() {
        // The offset basis of the fold, unchanged by no number.
        assert_eq!(MincostNetwork::default().fingerprint(), 0xcbf2_9ce4_8422_2325);

        // One node of supply -1: one step of the fold over 2^64 - 1, worked out apart from the crate.
        let mut one = MincostNetwork::default();
        one.add_node(-1);
        assert_eq!(one.fingerprint(), 0x509c_41b3_79fe_466e);

        // The tail 0, the head 1, the lower bound 0, the capacity 2, the cost 3 and the two supplies 0, in turn,
        // worked out apart from the crate.
        let mut arc = MincostNetwork::default();
        arc.add_node(0);
        arc.add_node(0);
        arc.add_arc(0, 1, 0, 2, 3);
        assert_eq!(arc.fingerprint(), 0x74fb_ced1_2ae2_274d);

        let mut swapped = arc.clone();
        (swapped.capacities[0], swapped.costs[0]) = (3, 2);
        assert_ne!(arc.fingerprint(), swapped.fingerprint(), "a number in another column");
    }

    #[test]
    #[should_panic(
        expected = "called `MincostNetwork::fingerprint` on a network of broken shape: the heads hold 0 entries for 1 \
                    arcs"
    )]
    fn test_the_fingerprint_refuses_a_broken_shape() {
        let mut network = pair();
        network.heads.clear();
        network.tails.truncate(1);

        let _ = network.fingerprint();
    }

    #[test]
    fn test_arcs_reads_every_column_from_either_end() {
        let network = pair();
        let arcs: Vec<NetworkArc> = shaped(&network).arcs().collect();
        let first = NetworkArc { tail: 0, head: 1, lower_bound: 1, capacity: 4, cost: 5 };
        let second = NetworkArc { tail: 1, head: 1, lower_bound: 0, capacity: 2, cost: -1 };
        assert_eq!(arcs, [first, second]);

        let mut both_ends = shaped(&network).arcs();
        assert_eq!(both_ends.len(), 2);
        assert_eq!(both_ends.next_back(), Some(second));
        assert_eq!(both_ends.len(), 1);
        assert_eq!(both_ends.next(), Some(first));
        assert_eq!((both_ends.next(), both_ends.next_back(), both_ends.len()), (None, None, 0));
    }

    #[test]
    fn test_excesses_into_moves_each_flow_from_tail_to_head_by_hand() {
        let network = pair();

        assert_eq!(excesses(&network, &[2, 2]), [1, -1], "the loop nets to nothing");
        assert_eq!(excesses(&network, &[3, 0]), [0, 0]);
        assert_eq!(excesses(&network, &[0, 0]), [3, -3]);
    }

    #[test]
    #[should_panic(
        expected = "called `ShapedNetwork::excesses_into` with 1 flows, which should be the arc count (is 2)"
    )]
    fn test_excesses_into_refuses_a_flow_of_another_length() {
        let network = pair();

        let _ = excesses(&network, &[1]);
    }

    #[test]
    fn test_cost_of_sums_each_flow_times_its_cost_by_hand() {
        let network = pair();

        assert_eq!(shaped(&network).cost_of(&[3, 2]), 15 - 2);
        assert_eq!(shaped(&network).cost_of(&[0, 0]), 0);

        let mut extreme = MincostNetwork::default();
        extreme.add_node(0);
        extreme.add_arc(0, 0, 0, u64::MAX, i64::MIN);
        assert_eq!(shaped(&extreme).cost_of(&[u64::MAX]), i128::from(i64::MIN) * i128::from(u64::MAX));
    }

    #[test]
    #[should_panic(expected = "attempt to add with overflow")]
    fn test_cost_of_refuses_a_sum_past_i128() {
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 0, 0, u64::MAX, i64::MAX);
        network.add_arc(0, 0, 0, u64::MAX, i64::MAX);

        let _ = shaped(&network).cost_of(&[u64::MAX, u64::MAX]);
    }

    #[test]
    #[should_panic(expected = "called `ShapedNetwork::cost_of` with 3 flows, which should be the arc count (is 2)")]
    fn test_cost_of_refuses_a_flow_of_another_length() {
        let network = pair();

        let _ = shaped(&network).cost_of(&[1, 1, 1]);
    }

    #[test]
    fn test_arc_past_cost_total_names_the_arc_where_the_sum_passes_by_hand() {
        // Arc 0 costs 5 · 4 = 20 at capacity, arc 1 costs 1 · 2 = 2.
        let network = pair();
        assert_eq!(shaped(&network).arc_past_cost_total(22), None);
        assert_eq!(shaped(&network).arc_past_cost_total(21), Some(1));
        assert_eq!(shaped(&network).arc_past_cost_total(19), Some(0));

        // Two products of 2^63 · (2^64 − 1) make 2^128 − 2^64, within a u128; a third passes it.
        let mut wide = MincostNetwork::default();
        wide.add_node(0);
        for _ in 0..3 {
            wide.add_arc(0, 0, 0, u64::MAX, i64::MIN);
        }
        assert_eq!(shaped(&wide).arc_past_cost_total(u128::MAX), Some(2));
        assert_eq!(shaped(&wide).arc_past_cost_total(u128::MAX - u128::from(u64::MAX) - 1), Some(1));
    }
}
