// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What an optimal min-cost answer must satisfy, checked from first principles in `i128`: the flow within its bounds
//! and meeting every supply, the potentials pricing it optimal, and its cost as stated.

use crate::mincost_network::{MincostNetwork, NetworkArc, ShapedNetwork};

/// The first way an answer fails [`check_certificate`]. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum CertificateBreach {
    /// An arc carries a flow outside its bounds.
    #[error("arc {arc} carries {flow}, outside its bounds {lower_bound}..={capacity}")]
    OutsideBounds {
        /// The arc.
        arc: usize,
        /// Its flow, in units of flow.
        flow: u64,
        /// Its least flow.
        lower_bound: u64,
        /// Its most flow.
        capacity: u64,
    },
    /// A node keeps an excess: its supply less the flow leaving it plus the flow entering it is not zero.
    #[error("node {node} keeps an excess of {excess}")]
    Unbalanced {
        /// The node.
        node: usize,
        /// Its excess, in units of flow.
        excess: i128,
    },
    /// An arc whose flow is below its capacity has a negative reduced cost: more flow on it would cost less.
    #[error("arc {arc} can carry more at the negative reduced cost {reduced_cost}")]
    CanCarryMore {
        /// The arc.
        arc: usize,
        /// Its cost plus its tail's potential less its head's, in units of cost.
        reduced_cost: i128,
    },
    /// An arc whose flow is above its lower bound has a positive reduced cost: less flow on it would cost less.
    #[error("arc {arc} can carry less at the positive reduced cost {reduced_cost}")]
    CanCarryLess {
        /// The arc.
        arc: usize,
        /// Its cost plus its tail's potential less its head's, in units of cost.
        reduced_cost: i128,
    },
    /// The total cost stated is not the cost of the flow.
    #[error("total cost {stated} stated for a flow that costs {recomputed}")]
    TotalCost {
        /// The total cost the answer states.
        stated: i128,
        /// The cost of the flow, recomputed.
        recomputed: i128,
    },
}

/// Checks that `flows`, `potentials` and `total_cost` prove an optimal answer for `network`.
///
/// The flow of each arc lies within its bounds; every node's supply, less the flow leaving it plus the flow entering
/// it, is zero; every arc whose flow is below its capacity has a reduced cost `cost + potential[tail] −
/// potential[head]` of at least zero, and every arc whose flow is above its lower bound one of at most zero; and
/// `total_cost` is the cost of the flow. By complementary slackness these prove the flow of least cost: any other flow
/// meeting the supplies differs from it by cycles of steps that can move, each of reduced cost at least zero. Every
/// sum is taken in `i128`.
///
/// # Errors
/// Returns the first breach, in the order of [`CertificateBreach`]'s variants and within one the lowest arc or node:
/// a flow outside its bounds, a node with an excess, an arc priced to carry more, or less, and a wrong total cost.
///
/// # Panics
/// Panics if the shape of `network` is broken, if `flows` does not hold one value per arc or `potentials` one per
/// node, or if the cost of the flow passes the range of an `i128`.
pub fn check_certificate(
    network: &MincostNetwork,
    flows: &[u64],
    potentials: &[i64],
    total_cost: i128,
) -> Result<(), CertificateBreach> {
    let shaped = network
        .shaped()
        .unwrap_or_else(|breach| panic!("called `check_certificate` on a network of broken shape: {breach}"));

    let (arc_count, node_count) = (shaped.arc_count(), shaped.node_count());
    assert!(
        flows.len() == arc_count,
        "called `check_certificate` with {} flows, which should be the arc count (is {arc_count})",
        flows.len()
    );
    assert!(
        potentials.len() == node_count,
        "called `check_certificate` with {} potentials, which should be the node count (is {node_count})",
        potentials.len()
    );

    let breach = bounds_breach(shaped, flows)
        .or_else(|| balance_breach(shaped, flows))
        .or_else(|| more_breach(shaped, flows, potentials))
        .or_else(|| less_breach(shaped, flows, potentials))
        .or_else(|| cost_breach(shaped, flows, total_cost));

    match breach {
        Some(breach) => Err(breach),
        None => Ok(()),
    }
}

/// The lowest arc of `network` whose flow in `flows`, one per arc, lies outside its bounds.
fn bounds_breach(network: ShapedNetwork<'_>, flows: &[u64]) -> Option<CertificateBreach> {
    network.arcs().zip(flows).enumerate().find_map(|(arc, (bounds, &flow))| {
        let within = bounds.lower_bound <= flow && flow <= bounds.capacity;

        (!within).then_some(CertificateBreach::OutsideBounds {
            arc,
            flow,
            lower_bound: bounds.lower_bound,
            capacity: bounds.capacity,
        })
    })
}

/// The lowest node of `network` that `flows`, one per arc, leaves an excess.
///
/// # Panics
/// Panics if `flows` does not hold one value per arc.
fn balance_breach(network: ShapedNetwork<'_>, flows: &[u64]) -> Option<CertificateBreach> {
    let mut excesses = Vec::with_capacity(network.node_count());
    network.excesses_into(flows, &mut excesses);

    excesses
        .into_iter()
        .enumerate()
        .find(|&(_, excess)| excess != 0)
        .map(|(node, excess)| CertificateBreach::Unbalanced { node, excess })
}

/// The lowest arc of `network` whose flow in `flows`, one per arc, is below its capacity at a negative reduced cost
/// under `potentials`, one per node: more flow on it would cost less.
///
/// # Panics
/// Panics if an end of an arc has no potential.
fn more_breach(network: ShapedNetwork<'_>, flows: &[u64], potentials: &[i64]) -> Option<CertificateBreach> {
    network.arcs().zip(flows).enumerate().find_map(|(arc, (priced, &flow))| {
        let reduced_cost = reduced_cost(priced, potentials);

        (flow < priced.capacity && reduced_cost < 0).then_some(CertificateBreach::CanCarryMore { arc, reduced_cost })
    })
}

/// The lowest arc of `network` whose flow in `flows`, one per arc, is above its lower bound at a positive reduced cost
/// under `potentials`, one per node: less flow on it would cost less.
///
/// # Panics
/// Panics if an end of an arc has no potential.
fn less_breach(network: ShapedNetwork<'_>, flows: &[u64], potentials: &[i64]) -> Option<CertificateBreach> {
    network.arcs().zip(flows).enumerate().find_map(|(arc, (priced, &flow))| {
        let reduced_cost = reduced_cost(priced, potentials);

        (flow > priced.lower_bound && reduced_cost > 0).then_some(CertificateBreach::CanCarryLess { arc, reduced_cost })
    })
}

/// The breach where `total_cost` is not the cost of `flows`, one per arc, on `network`.
///
/// # Panics
/// Panics if `flows` does not hold one value per arc, or if its cost passes the range of an `i128`.
fn cost_breach(network: ShapedNetwork<'_>, flows: &[u64], total_cost: i128) -> Option<CertificateBreach> {
    let recomputed = network.cost_of(flows);

    (recomputed != total_cost).then_some(CertificateBreach::TotalCost { stated: total_cost, recomputed })
}

/// The reduced cost of `arc` under `potentials`, one per node: its cost plus its tail's potential less its head's,
/// exact in `i128`.
///
/// # Panics
/// Panics if an end of `arc` has no potential.
fn reduced_cost(arc: NetworkArc, potentials: &[i64]) -> i128 {
    i128::from(arc.cost) + i128::from(potentials[arc.tail]) - i128::from(potentials[arc.head])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Node 0 sends 2 to node 1 over two parallel arcs of cost 3 and 4, capacity 2 each.
    fn parallel() -> MincostNetwork {
        let mut network = MincostNetwork::default();
        let (from, to) = (network.add_node(2), network.add_node(-2));
        network.add_arc(from, to, 0, 2, 3);
        network.add_arc(from, to, 0, 2, 4);

        network
    }

    /// The view of `network`, whose shape the test knows to hold.
    fn shaped(network: &MincostNetwork) -> ShapedNetwork<'_> {
        network.shaped().expect("the test's network should be of sound shape")
    }

    #[test]
    fn test_a_certificate_takes_the_cheap_arc_and_refuses_the_dear_one_by_hand() {
        let network = parallel();

        // Potentials 0 and 3: arc 0 tight at 3 + 0 − 3 = 0, arc 1 empty at 4 − 3 = 1 ≥ 0. The flow costs 6.
        assert_eq!(check_certificate(&network, &[2, 0], &[0, 3], 6), Ok(()));

        // Potentials 0 and 4 make arc 1 tight but price arc 0, empty, at −1.
        assert_eq!(
            check_certificate(&network, &[0, 2], &[0, 4], 8),
            Err(CertificateBreach::CanCarryMore { arc: 0, reduced_cost: -1 })
        );

        // Potentials 0 and 2 price the full arc 0 at 1: it should carry less.
        assert_eq!(
            check_certificate(&network, &[2, 0], &[0, 2], 6),
            Err(CertificateBreach::CanCarryLess { arc: 0, reduced_cost: 1 })
        );

        assert_eq!(
            check_certificate(&network, &[2, 0], &[0, 3], 7),
            Err(CertificateBreach::TotalCost { stated: 7, recomputed: 6 })
        );
    }

    #[test]
    fn test_the_breaches_come_in_the_order_of_the_variants() {
        let network = parallel();

        // Arc 0 past its capacity, node 0 unbalanced, arc 1 priced to carry more, a wrong cost: the bounds first.
        assert_eq!(
            check_certificate(&network, &[3, 0], &[0, 9], 0),
            Err(CertificateBreach::OutsideBounds { arc: 0, flow: 3, lower_bound: 0, capacity: 2 })
        );

        // Within the bounds but unbalanced, priced wrong and costed wrong: the balance next.
        assert_eq!(
            check_certificate(&network, &[1, 0], &[0, 9], 0),
            Err(CertificateBreach::Unbalanced { node: 0, excess: 1 })
        );

        // Balanced, priced wrong and costed wrong: the pricing next.
        assert_eq!(
            check_certificate(&network, &[2, 0], &[0, 9], 0),
            Err(CertificateBreach::CanCarryMore { arc: 1, reduced_cost: -5 })
        );
    }

    #[test]
    fn test_a_lower_bound_counts_as_room_to_carry_less() {
        // One fixed arc 0 -> 1 of bounds 2..=2 at cost 5: its reduced cost may have either sign.
        let mut fixed = MincostNetwork::default();
        let (from, to) = (fixed.add_node(2), fixed.add_node(-2));
        fixed.add_arc(from, to, 2, 2, 5);

        assert_eq!(check_certificate(&fixed, &[2], &[0, 0], 10), Ok(()));
        assert_eq!(check_certificate(&fixed, &[2], &[0, 100], 10), Ok(()));

        // With room above its lower bound, a positive reduced cost asks for less.
        fixed.capacities[0] = 3;
        assert_eq!(check_certificate(&fixed, &[2], &[0, 0], 10), Ok(()), "at its lower bound it cannot carry less");

        fixed.supplies = vec![3, -3];
        assert_eq!(
            check_certificate(&fixed, &[3], &[0, 0], 15),
            Err(CertificateBreach::CanCarryLess { arc: 0, reduced_cost: 5 })
        );
        assert_eq!(check_certificate(&fixed, &[3], &[0, 5], 15), Ok(()));

        assert_eq!(
            check_certificate(&fixed, &[1], &[0, 5], 5),
            Err(CertificateBreach::OutsideBounds { arc: 0, flow: 1, lower_bound: 2, capacity: 3 })
        );
    }

    #[test]
    fn test_an_arc_priced_to_carry_more_comes_before_one_priced_to_carry_less() {
        // Potentials 0 and 1 with costs 3 and 0: arc 0, full, at 3 − 1 = 2 asks for less; arc 1, empty, at 0 − 1 = −1
        // asks for more. The variant that asks for more comes first, although its arc is the higher one.
        let mut network = parallel();
        network.costs[1] = 0;

        assert_eq!(
            check_certificate(&network, &[2, 0], &[0, 1], 6),
            Err(CertificateBreach::CanCarryMore { arc: 1, reduced_cost: -1 })
        );
    }

    #[test]
    fn test_a_self_loop_is_priced_at_its_own_cost() {
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 0, 0, 4, -1);

        assert_eq!(check_certificate(&network, &[4], &[7], -4), Ok(()));
        assert_eq!(
            check_certificate(&network, &[3], &[7], -3),
            Err(CertificateBreach::CanCarryMore { arc: 0, reduced_cost: -1 })
        );
    }

    #[test]
    fn test_reduced_costs_at_the_extremes_are_exact() {
        let arc = |cost| NetworkArc { tail: 0, head: 1, lower_bound: 0, capacity: 1, cost };

        assert_eq!(reduced_cost(arc(3), &[5, 9]), -1);
        assert_eq!(reduced_cost(arc(i64::MIN), &[i64::MIN, i64::MAX]), 3 * i128::from(i64::MIN) + 1);
        assert_eq!(reduced_cost(arc(i64::MAX), &[i64::MAX, i64::MIN]), 3 * i128::from(i64::MAX) + 1);
    }

    #[test]
    fn test_each_step_alone_by_hand() {
        let network = parallel();

        assert_eq!(bounds_breach(shaped(&network), &[2, 2]), None);
        assert_eq!(
            bounds_breach(shaped(&network), &[0, 5]),
            Some(CertificateBreach::OutsideBounds { arc: 1, flow: 5, lower_bound: 0, capacity: 2 })
        );

        assert_eq!(balance_breach(shaped(&network), &[1, 1]), None);
        assert_eq!(
            balance_breach(shaped(&network), &[2, 1]),
            Some(CertificateBreach::Unbalanced { node: 0, excess: -1 })
        );

        assert_eq!(more_breach(shaped(&network), &[2, 0], &[0, 3]), None, "the cheap arc full and tight");
        assert_eq!(
            more_breach(shaped(&network), &[2, 0], &[0, 5]),
            Some(CertificateBreach::CanCarryMore { arc: 1, reduced_cost: -1 })
        );

        assert_eq!(less_breach(shaped(&network), &[2, 0], &[0, 3]), None, "the dear arc empty");
        assert_eq!(
            less_breach(shaped(&network), &[1, 1], &[0, 3]),
            Some(CertificateBreach::CanCarryLess { arc: 1, reduced_cost: 1 })
        );

        assert_eq!(cost_breach(shaped(&network), &[1, 1], 7), None);
        assert_eq!(
            cost_breach(shaped(&network), &[1, 1], -7),
            Some(CertificateBreach::TotalCost { stated: -7, recomputed: 7 })
        );
    }

    #[test]
    fn test_messages_give_the_values() {
        let messages = [
            CertificateBreach::OutsideBounds { arc: 1, flow: 5, lower_bound: 0, capacity: 2 }.to_string(),
            CertificateBreach::Unbalanced { node: 0, excess: -1 }.to_string(),
            CertificateBreach::CanCarryMore { arc: 2, reduced_cost: -3 }.to_string(),
            CertificateBreach::CanCarryLess { arc: 3, reduced_cost: 4 }.to_string(),
            CertificateBreach::TotalCost { stated: 7, recomputed: 6 }.to_string(),
        ];

        assert_eq!(
            messages,
            [
                "arc 1 carries 5, outside its bounds 0..=2",
                "node 0 keeps an excess of -1",
                "arc 2 can carry more at the negative reduced cost -3",
                "arc 3 can carry less at the positive reduced cost 4",
                "total cost 7 stated for a flow that costs 6",
            ]
        );
    }

    #[test]
    #[should_panic(expected = "called `check_certificate` with 1 flows, which should be the arc count (is 2)")]
    fn test_a_flow_of_another_length_is_refused() {
        let _ = check_certificate(&parallel(), &[2], &[0, 3], 6);
    }

    #[test]
    #[should_panic(expected = "called `check_certificate` with 3 potentials, which should be the node count (is 2)")]
    fn test_potentials_of_another_length_are_refused() {
        let _ = check_certificate(&parallel(), &[2, 0], &[0, 3, 0], 6);
    }

    #[test]
    #[should_panic(
        expected = "called `check_certificate` on a network of broken shape: arc 0 has end 2, outside the 2 nodes"
    )]
    fn test_a_broken_shape_is_refused() {
        let mut network = parallel();
        network.heads[0] = 2;

        let _ = check_certificate(&network, &[2, 0], &[0, 3], 6);
    }
}
