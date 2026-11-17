// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Changes of a network on the same arcs, each drawn by a generator, for the benchmarks that solve a network again
//! after a change: supplies moved between nodes of one role, capacities moved by a few units, and costs taken from the
//! same network with its costs drawn anew. Every change these make changes the number it touches.

use rand::RngExt;
use rand::seq::SliceRandom;
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::mincost_network::MincostNetwork;

/// The role of a node with a supply: it sends flow, or it takes flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// A supply above zero.
    Supplier,
    /// A supply below zero.
    Demander,
}

impl Role {
    /// Both roles.
    const BOTH: [Self; 2] = [Self::Supplier, Self::Demander];

    /// The sign of the supply of a node of this role.
    const fn sign(self) -> i64 {
        match self {
            Self::Supplier => 1,
            Self::Demander => -1,
        }
    }

    /// The nodes of this role in `supplies`, ascending.
    fn nodes(self, supplies: &[i64]) -> Vec<usize> {
        (0..supplies.len()).filter(|&node| supplies[node].signum() == self.sign()).collect()
    }

    /// The nodes of this role in `supplies` that can give at least one unit and keep their role: a magnitude of two
    /// or more, ascending. Empty where the role has fewer than two nodes, since a giver needs another to take.
    fn givers(self, supplies: &[i64]) -> Vec<usize> {
        let nodes = self.nodes(supplies);
        if nodes.len() < 2 {
            return Vec::new();
        }

        nodes.into_iter().filter(|&node| supplies[node].unsigned_abs() >= 2).collect()
    }
}

/// `network` with `moves` amounts moved between nodes of one role: each move takes from one node an amount of one to
/// `most` units and gives it to another node of the same role, a supplier's supply to another supplier, a demander's
/// demand to another demander. The giver keeps at least one unit of its supply or demand, so every node keeps its role
/// and the supplies still balance. A move draws, each alike among those that can make it, its role, its giver, its
/// taker among the role's other nodes, and its amount, at most what the giver can give. `None` where a move cannot be
/// made, as in a network with one supplier and one demander.
///
/// # Panics
/// Panics if `most` is zero, or a supply passes the range of an `i64`.
pub fn supplies_moved(
    network: &MincostNetwork,
    moves: usize,
    most: u64,
    draws: &mut Xoshiro256PlusPlus,
) -> Option<MincostNetwork> {
    assert!(most > 0, "called `supplies_moved` with moves of at most zero units");
    let mut supplies = network.supplies.clone();

    for _ in 0..moves {
        let roles: Vec<(Role, Vec<usize>)> = Role::BOTH
            .into_iter()
            .map(|role| (role, role.givers(&supplies)))
            .filter(|(_, givers)| !givers.is_empty())
            .collect();
        if roles.is_empty() {
            return None;
        }

        let (role, givers) = &roles[draws.random_range(0..roles.len())];
        let giver = givers[draws.random_range(0..givers.len())];
        let takers: Vec<usize> = role.nodes(&supplies).into_iter().filter(|&node| node != giver).collect();
        let taker = takers[draws.random_range(0..takers.len())];
        let amount = draws.random_range(1..=most.min(supplies[giver].unsigned_abs() - 1));

        let moved = role.sign().strict_mul(i64::try_from(amount).expect("an amount below a supply should fit an i64"));
        supplies[giver] = supplies[giver].strict_sub(moved);
        supplies[taker] = supplies[taker].strict_add(moved);
    }

    Some(MincostNetwork { supplies, ..network.clone() })
}

/// `network` with the capacities of `count` distinct arcs, drawn alike, each moved by one to `most` units, drawn
/// alike: down where a coin says so and the capacity stays at or above the arc's lower bound, up otherwise. Every
/// capacity it touches changes; every other number is as it was.
///
/// # Panics
/// Panics if `count` passes the arc count of `network`, if `most` is zero, or if a capacity passes `u64::MAX`.
pub fn capacities_moved(
    network: &MincostNetwork,
    count: usize,
    most: u64,
    draws: &mut Xoshiro256PlusPlus,
) -> MincostNetwork {
    let arc_count = network.arc_count();
    assert!(count <= arc_count, "called `capacities_moved` with {count} arcs to move of a network of {arc_count}");
    assert!(most > 0, "called `capacities_moved` with moves of at most zero units");

    let mut arcs: Vec<usize> = (0..arc_count).collect();
    let (chosen, _) = arcs.partial_shuffle(draws, count);
    let mut capacities = network.capacities.clone();
    for &arc in chosen.iter() {
        let amount = draws.random_range(1..=most);
        let down = draws.random_bool(0.5);
        capacities[arc] = match capacities[arc].checked_sub(amount) {
            Some(lowered) if down && lowered >= network.lower_bounds[arc] => lowered,
            _ => capacities[arc].strict_add(amount),
        };
    }

    MincostNetwork { capacities, ..network.clone() }
}

/// `network` with the costs of `count` of its arcs taken from `costs`, the costs of a network on the same arcs: the
/// arcs drawn alike among those whose cost in `costs` differs from their own, all of those where fewer differ, so that
/// every cost it takes changes. Every other number is as it was.
///
/// # Panics
/// Panics if `costs` does not hold one cost per arc of `network`, or `count` passes its arc count.
pub fn costs_taken(
    network: &MincostNetwork,
    costs: &[i64],
    count: usize,
    draws: &mut Xoshiro256PlusPlus,
) -> MincostNetwork {
    let arc_count = network.arc_count();
    assert!(costs.len() == arc_count, "called `costs_taken` with {} costs for {arc_count} arcs", costs.len());
    assert!(count <= arc_count, "called `costs_taken` with {count} costs to take for a network of {arc_count} arcs");

    let mut differing: Vec<usize> = (0..arc_count).filter(|&arc| costs[arc] != network.costs[arc]).collect();
    let taken = count.min(differing.len());
    let (chosen, _) = differing.partial_shuffle(draws, taken);
    let mut moved = network.costs.clone();
    for &arc in chosen.iter() {
        moved[arc] = costs[arc];
    }

    MincostNetwork { costs: moved, ..network.clone() }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;

    /// The generator seeded with `seed`.
    fn rng(seed: u64) -> Xoshiro256PlusPlus {
        Xoshiro256PlusPlus::seed_from_u64(seed)
    }

    #[test]
    fn test_a_supply_move_keeps_every_role_and_the_balance_by_hand() {
        // Suppliers 4 and 3, demanders 5 and 2. A move from supplier 4 gives 1 to 3 units to supplier 3, one from 3
        // gives 1 or 2 to 4; a move of demand from 5 gives 1 to 3 to demander 2, one from 2 gives 1 to 5. So exactly
        // two nodes of one role change, by the same amount of one to three units, and over the seeds every giver and
        // every amount it may give is reached.
        let network = MincostNetwork::from_arcs(&[4, 3, -5, -2], &[(0, 2, 0, 9, 1)]);
        let mut reached = Vec::new();

        for seed in 0..256 {
            let moved = supplies_moved(&network, 1, 3, &mut rng(seed)).expect("both roles should be able to move");
            assert_eq!(MincostNetwork { supplies: network.supplies.clone(), ..moved.clone() }, network);
            assert!(
                moved.supplies.iter().zip(&network.supplies).all(|(after, before)| after.signum() == before.signum())
            );

            let changed: Vec<usize> = (0..4).filter(|&node| moved.supplies[node] != network.supplies[node]).collect();
            let [giver, taker] = changed[..] else { panic!("two nodes should change: {:?}", moved.supplies) };
            let (giver, taker) = if moved.supplies[giver].abs() < network.supplies[giver].abs() {
                (giver, taker)
            } else {
                (taker, giver)
            };
            let amount = network.supplies[giver].abs() - moved.supplies[giver].abs();
            assert_eq!(moved.supplies[taker].abs() - network.supplies[taker].abs(), amount);
            assert_eq!(network.supplies[giver].signum(), network.supplies[taker].signum());

            if !reached.contains(&(giver, amount)) {
                reached.push((giver, amount));
            }
        }

        reached.sort_unstable();
        assert_eq!(reached, [(0, 1), (0, 2), (0, 3), (1, 1), (1, 2), (2, 1), (2, 2), (2, 3), (3, 1)]);
    }

    #[test]
    fn test_supplies_without_two_nodes_of_a_role_that_can_give_cannot_move() {
        // One supplier and one demander, as the grid has; and two suppliers of one unit each beside one demander.
        let lone = MincostNetwork::from_arcs(&[5, 0, -5], &[(0, 1, 0, 9, 1), (1, 2, 0, 9, 1)]);
        let thin = MincostNetwork::from_arcs(&[1, 1, -2], &[(0, 2, 0, 9, 1), (1, 2, 0, 9, 1)]);

        assert_eq!(supplies_moved(&lone, 1, 3, &mut rng(3)), None);
        assert_eq!(supplies_moved(&thin, 1, 3, &mut rng(3)), None);
        assert_eq!(supplies_moved(&lone, 0, 3, &mut rng(3)), Some(lone));
    }

    #[test]
    #[should_panic(expected = "called `supplies_moved` with moves of at most zero units")]
    fn test_supplies_moved_refuse_moves_of_no_unit() {
        let network = MincostNetwork::from_arcs(&[4, 3, -7], &[(0, 2, 0, 9, 1)]);
        let _ = supplies_moved(&network, 1, 0, &mut rng(1));
    }

    #[test]
    fn test_moved_capacities_change_by_one_or_two_units_and_never_pass_below_a_lower_bound() {
        // Arc 0 stands at its lower bound 2, so it can only rise; arc 1 of capacity 10 can go either way. Both are
        // moved, each by one or two units, and over the seeds arc 1 goes both ways by both amounts.
        let network = MincostNetwork::from_arcs(&[3, -3], &[(0, 1, 2, 2, 1), (0, 1, 0, 10, 4)]);
        let mut reached = Vec::new();

        for seed in 0..64 {
            let moved = capacities_moved(&network, 2, 2, &mut rng(seed));
            assert_eq!(MincostNetwork { capacities: network.capacities.clone(), ..moved.clone() }, network);
            assert!([3, 4].contains(&moved.capacities[0]), "{:?}", moved.capacities);
            assert!([8, 9, 11, 12].contains(&moved.capacities[1]), "{:?}", moved.capacities);
            if !reached.contains(&moved.capacities[1]) {
                reached.push(moved.capacities[1]);
            }
        }

        reached.sort_unstable();
        assert_eq!(reached, [8, 9, 11, 12]);
        assert_eq!(capacities_moved(&network, 0, 2, &mut rng(1)), network);
    }

    #[test]
    #[should_panic(expected = "called `capacities_moved` with 3 arcs to move of a network of 2")]
    fn test_capacities_moved_refuse_more_arcs_than_the_network_has() {
        let network = MincostNetwork::from_arcs(&[3, -3], &[(0, 1, 0, 2, 1), (0, 1, 0, 10, 4)]);
        let _ = capacities_moved(&network, 3, 2, &mut rng(1));
    }

    #[test]
    fn test_costs_taken_come_from_the_arcs_whose_cost_differs_by_hand() {
        // Costs 1, 2, 3, 4 against 1, 9, 3, 8: arcs 1 and 3 differ. One cost taken is one of the two, changed; four
        // asked for take both; none asked for take none.
        let network =
            MincostNetwork::from_arcs(&[3, -3], &[(0, 1, 0, 9, 1), (0, 1, 0, 9, 2), (0, 1, 0, 9, 3), (0, 1, 0, 9, 4)]);
        let costs = [1, 9, 3, 8];
        let mut reached = Vec::new();

        for seed in 0..32 {
            let moved = costs_taken(&network, &costs, 1, &mut rng(seed));
            assert_eq!(MincostNetwork { costs: network.costs.clone(), ..moved.clone() }, network);
            assert!(moved.costs == [1, 9, 3, 4] || moved.costs == [1, 2, 3, 8], "{:?}", moved.costs);
            if !reached.contains(&moved.costs) {
                reached.push(moved.costs.clone());
            }
        }

        assert_eq!(reached.len(), 2);
        assert_eq!(costs_taken(&network, &costs, 4, &mut rng(1)).costs, costs);
        assert_eq!(costs_taken(&network, &costs, 0, &mut rng(1)), network);
    }

    #[test]
    #[should_panic(expected = "called `costs_taken` with 3 costs for 4 arcs")]
    fn test_costs_taken_refuse_costs_of_another_arc_count() {
        let network =
            MincostNetwork::from_arcs(&[3, -3], &[(0, 1, 0, 9, 1), (0, 1, 0, 9, 2), (0, 1, 0, 9, 3), (0, 1, 0, 9, 4)]);
        let _ = costs_taken(&network, &[1, 2, 3], 1, &mut rng(1));
    }

    #[test]
    #[should_panic(expected = "called `costs_taken` with 5 costs to take for a network of 4 arcs")]
    fn test_costs_taken_refuse_more_arcs_than_the_network_has() {
        let network =
            MincostNetwork::from_arcs(&[3, -3], &[(0, 1, 0, 9, 1), (0, 1, 0, 9, 2), (0, 1, 0, 9, 3), (0, 1, 0, 9, 4)]);
        let _ = costs_taken(&network, &[1, 2, 3, 4], 5, &mut rng(1));
    }
}
