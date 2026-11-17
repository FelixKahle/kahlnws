// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The model the basis's tests check against: a network extended by a root, with the arc dealt to each slot, each
//! node's supply net of the lower bounds, the root cost, and every link's ends, cost and capacity, recomputed from the
//! network alone by plain indexing.

use kahlnws_testing::MincostNetwork;

use crate::slot_order::tests::arcs_of_slots;
use crate::tree::{Direction, PlainHanging};

/// What the invariant check recomputes from a network apart from the basis. Plain data.
pub(super) struct Extended {
    /// Per slot: the arc dealt to it.
    pub(super) arc_of_slot: Vec<usize>,
    /// Per node: its supply, less the lower bounds of the arcs leaving it, plus those of the arcs entering it.
    pub(super) net_supplies: Vec<i128>,
    /// The cost of a root link that points down: `(n − 1)·C + 1`, or the root cost the model was made at.
    pub(super) root_cost: i128,
}

impl Extended {
    /// What `network` extends to.
    pub(super) fn of(network: &MincostNetwork) -> Self {
        let (node_count, arc_count) = (network.node_count(), network.arc_count());
        let arc_of_slot = arcs_of_slots(arc_count, node_count);

        let mut net_supplies: Vec<i128> = network.supplies.iter().map(|&supply| i128::from(supply)).collect();
        for arc in 0..arc_count {
            net_supplies[network.tails[arc]] -= i128::from(network.lower_bounds[arc]);
            net_supplies[network.heads[arc]] += i128::from(network.lower_bounds[arc]);
        }

        let largest = network.costs.iter().map(|&cost| i128::from(cost).abs()).max().unwrap_or(0);
        let paths = i128::try_from(node_count.saturating_sub(1)).expect("a test's node count should fit");

        Self { arc_of_slot, net_supplies, root_cost: paths * largest + 1 }
    }

    /// What `network` extends to at the root cost `root_cost` in place of its own.
    pub(super) fn at(network: &MincostNetwork, root_cost: i64) -> Self {
        Self { root_cost: i128::from(root_cost), ..Self::of(network) }
    }

    /// The tail, head and cost of every link of `network` in a basis whose nodes hang as `hangings` says: an
    /// arc's at its slot; a root link's as its node hangs by it, up at cost zero or down at the root cost; and a
    /// root link out of the tree up, which matters to nothing, since it carries nothing.
    pub(super) fn links(&self, network: &MincostNetwork, hangings: &[PlainHanging]) -> Vec<(usize, usize, i128)> {
        let (node_count, arc_count) = (network.node_count(), network.arc_count());
        let arcs = self
            .arc_of_slot
            .iter()
            .map(|&arc| (network.tails[arc], network.heads[arc], i128::from(network.costs[arc])));
        let root_links = (0..node_count).map(|node| {
            let in_tree = hangings.get(node).filter(|hanging| hanging.link == arc_count + node);
            match in_tree.map(|hanging| hanging.direction) {
                Some(Direction::Down) => (node_count, node, self.root_cost),
                Some(Direction::Up) | None => (node, node_count, 0),
            }
        });

        arcs.chain(root_links).collect()
    }

    /// What `link` of `network` can carry: an arc's width, `u64::MAX` for a root link.
    pub(super) fn capacity(&self, network: &MincostNetwork, link: usize) -> u64 {
        match self.arc_of_slot.get(link) {
            Some(&arc) => network.capacities[arc] - network.lower_bounds[arc],
            None => u64::MAX,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_model_extends_a_network_with_lower_bounds_and_a_stride_by_hand() {
        // Five arcs on two nodes, dealt with the stride three to the slots 0, 3, 1, 4, 2, so that the slots hold the
        // arcs 0, 2, 4, 1, 3. Node 0 supplies 3, less the lower bound 2 of arc 3, which leaves it, plus the 1 of arc 1,
        // which enters it: 2; node 1 the negation. The largest cost is 5: the root cost is `1 · 5 + 1 = 6`.
        let network = MincostNetwork::from_arcs(
            &[3, -3],
            &[(0, 1, 0, 2, 1), (1, 0, 1, 3, -2), (0, 0, 0, 0, 5), (0, 1, 2, 2, 4), (1, 1, 0, 1, -1)],
        );
        let model = Extended::of(&network);
        assert_eq!((model.arc_of_slot.as_slice(), model.net_supplies.as_slice()), (&[0, 2, 4, 1, 3][..], &[2, -2][..]));
        assert_eq!(model.root_cost, 6);

        // Each slot's width, its capacity less its lower bound, then the root links'.
        let capacities: Vec<u64> = (0..7).map(|link| model.capacity(&network, link)).collect();
        assert_eq!(capacities, [2, 0, 1, 2, 0, u64::MAX, u64::MAX]);

        // Node 0 hangs up from the root, node 2, by its root link 5, at cost zero; node 1 down by its own, 6, at the
        // root cost. Hung up from node 0 by arc 1 at slot 3 instead, node 1 has its root link out of the tree, up.
        let hanging = |parent: usize, link: usize, direction: Direction| PlainHanging { parent, link, direction };
        let arcs = [(0, 1, 1), (0, 0, 5), (1, 1, -1), (1, 0, -2), (0, 1, 4)];
        let star = [hanging(2, 5, Direction::Up), hanging(2, 6, Direction::Down)];
        let expected: Vec<(usize, usize, i128)> = arcs.into_iter().chain([(0, 2, 0), (2, 1, 6)]).collect();
        assert_eq!(model.links(&network, &star), expected);

        let hung = [hanging(2, 5, Direction::Up), hanging(0, 3, Direction::Up)];
        let expected: Vec<(usize, usize, i128)> = arcs.into_iter().chain([(0, 2, 0), (1, 2, 0)]).collect();
        assert_eq!(model.links(&network, &hung), expected);

        // At the root cost 9 in place of its own 6: a root link that points down costs 9.
        let wider = Extended::at(&network, 9);
        assert_eq!(
            (wider.root_cost, wider.arc_of_slot, wider.net_supplies),
            (9, model.arc_of_slot, model.net_supplies)
        );
        let expected: Vec<(usize, usize, i128)> = arcs.into_iter().chain([(0, 2, 0), (2, 1, 9)]).collect();
        assert_eq!(Extended::at(&network, 9).links(&network, &star), expected);
    }
}
