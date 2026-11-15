// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a solve into the caller's buffer allocates: nothing after a reservation for problems of the reserved size and
//! smaller, feasible or not; nothing from the second solve on for a problem no larger than the first, whatever its
//! shape; and room made by a solve that answered, not by one that was refused. A resolve into the caller's buffer
//! allocates nothing after a reservation either, from the kept basis or from the star. The binary installs the
//! counting allocator, so it stands alone.

use kahlnws::{NetworkSimplex, Outcome, Problem, ProblemError, Work};
use kahlnws_testing::{
    Allocations, CountingAllocator, Family, MincostNetwork, allocations_on_this_thread, assignment_shaped, general,
    perturbed, supply_shaped,
};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// The problem of `network`, its lower bounds given.
fn problem_of(network: &MincostNetwork) -> Problem<'_> {
    Problem {
        tails: &network.tails,
        heads: &network.heads,
        lower_bounds: Some(&network.lower_bounds),
        capacities: &network.capacities,
        costs: &network.costs,
        supplies: &network.supplies,
    }
}

/// How one solve went: what it allocated, and whether it found a flow. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Solved {
    /// What the solve allocated on the calling thread.
    allocated: Allocations,
    /// Whether it answered an optimal flow.
    optimal: bool,
}

/// Solves `network` with `solver` into a buffer made beforehand, and answers what the solve alone allocated.
///
/// # Panics
/// Panics if the problem is refused.
fn solved(solver: &mut NetworkSimplex, network: &MincostNetwork) -> Solved {
    let problem = problem_of(network);
    let mut flows = vec![0; network.arc_count()];

    let before = allocations_on_this_thread();
    let answer = solver.solve_into(&problem, &mut flows).expect("the test's problem should be admitted");
    let allocated = allocations_on_this_thread() - before;

    Solved { allocated, optimal: matches!(answer.outcome, Outcome::Optimal(_)) }
}

/// A network with no flow: node 0 must send 2 units over an arc that carries 1.
fn without_flow() -> MincostNetwork {
    let mut network = MincostNetwork::default();
    let (supply, demand) = (network.add_node(2), network.add_node(-2));
    network.add_arc(supply, demand, 0, 1, 1);

    network
}

#[test]
fn test_no_solve_allocates_after_a_reservation() {
    // The benchmark's smaller networks of each family and seeded ones, many suppliers and one, with and without a
    // flow, and a problem refused.
    let mut networks = vec![Family::Supply.network(20), Family::Assignment.network(32), Family::Grid.network(9)];
    for seed in [[3_u64, 1, 4, 1], [5, 9, 2, 6], [92, 65, 35, 89]] {
        networks.extend([supply_shaped(&seed), assignment_shaped(&seed), general(&seed)]);
    }
    networks.push(without_flow());

    let node_count = networks.iter().map(MincostNetwork::node_count).max().unwrap_or(0);
    let arc_count = networks.iter().map(MincostNetwork::arc_count).max().unwrap_or(0);
    let mut solver = NetworkSimplex::new();
    solver.try_reserve(node_count, arc_count).expect("the room for the networks should be granted");

    let solves: Vec<Solved> = networks.iter().map(|network| solved(&mut solver, network)).collect();
    assert!(solves.iter().all(|solve| solve.allocated == Allocations::NONE), "{solves:?}");
    assert!(solves.iter().any(|solve| solve.optimal) && solves.iter().any(|solve| !solve.optimal), "{solves:?}");

    let refused = MincostNetwork { supplies: vec![1, 0], ..without_flow() };
    let mut flows = [0];
    let before = allocations_on_this_thread();
    assert!(solver.solve_into(&problem_of(&refused), &mut flows).is_err());
    assert_eq!(allocations_on_this_thread() - before, Allocations::NONE, "a refusal allocates nothing either");
}

/// `arc_count` arcs among ten nodes, node 0 supplying 5 to node 9: arc `a` from node `a mod 10` to node
/// `(7a + ⌊a / 10⌋) mod 10`, of capacity `5 + a mod 3` and cost `a mod 7`, every one wide enough for the supply.
fn dense(arc_count: usize) -> MincostNetwork {
    let mut network = MincostNetwork::default();
    for node in 0..10 {
        network.add_node(match node {
            0 => 5,
            9 => -5,
            _ => 0,
        });
    }
    for arc in 0..arc_count {
        let small = |value: usize| u8::try_from(value).expect("a remainder below ten should fit a byte");
        let (capacity, cost) = (5 + u64::from(small(arc % 3)), i64::from(small(arc % 7)));
        network.add_arc(arc % 10, (7 * arc + arc / 10) % 10, 0, capacity, cost);
    }

    network
}

/// The number of nodes of `network` whose supply has the sign `sign`: 1 for those that send flow, −1 for those that
/// take it.
fn nodes_of_sign(network: &MincostNetwork, sign: i64) -> usize {
    network.supplies.iter().filter(|&&supply| supply.signum() == sign).count()
}

#[test]
fn test_no_solve_allocates_from_the_second_on_for_a_problem_no_larger_than_the_first() {
    // The first problem has many suppliers. The later ones are no larger in nodes or arcs. Among them, ten nodes with
    // as many arcs as the first, one supplier and one demander: its first pivots search backwards over rows that hold
    // every arc, which the first problem never needed; a grid of one supplier and one demander too; and a network with
    // no flow.
    let first = Family::Supply.network(80);
    let later = [first.clone(), dense(first.arc_count()), Family::Grid.network(9), without_flow(), first.clone()];
    assert!(nodes_of_sign(&first, 1) > 1, "the first problem has many suppliers");
    assert!(later.iter().all(|network| network.node_count() <= first.node_count()));
    assert!(later.iter().all(|network| network.arc_count() <= first.arc_count()));
    assert_eq!((later[1].node_count(), later[1].arc_count()), (10, first.arc_count()));
    assert!(later[1].lower_bounds.iter().chain(&later[2].lower_bounds).all(|&lower_bound| lower_bound == 0));
    assert!(later[1..3].iter().all(|network| nodes_of_sign(network, 1) == 1 && nodes_of_sign(network, -1) == 1));

    let mut solver = NetworkSimplex::new();
    let opening = solved(&mut solver, &first);
    assert!(opening.allocated != Allocations::NONE, "the first solve makes the room");

    let solves: Vec<Solved> = later.iter().map(|network| solved(&mut solver, network)).collect();
    assert!(solves.iter().all(|solve| solve.allocated == Allocations::NONE), "{solves:?}");
    assert_eq!(solves.iter().map(|solve| solve.optimal).collect::<Vec<_>>(), [true, true, true, false, true]);
}

#[test]
fn test_a_solve_refused_at_its_net_supplies_makes_no_room_and_one_that_answers_does() {
    // Node 0 supplies `2^63 − 1`, and an arc into it of lower bound `2^63` adds as much: the problem passes every check
    // before its net supplies are gathered, and is refused there. The answered problem has as many nodes and arcs.
    let refused = MincostNetwork::from_arcs(&[i64::MAX, i64::MIN + 1], &[(1, 0, 1 << 63, 1 << 63, 0), (0, 1, 0, 5, 1)]);
    let answered = MincostNetwork::from_arcs(&[2, -2], &[(1, 0, 0, 5, 0), (0, 1, 0, 5, 1)]);
    let mut solver = NetworkSimplex::new();

    let mut flows = [0; 2];
    let refusal = solver.solve_into(&problem_of(&refused), &mut flows).map(|_| ());
    assert!(matches!(refusal, Err(ProblemError::NetSupplyBeyondRange { .. })), "{refusal:?}");

    assert_ne!(solved(&mut solver, &answered).allocated, Allocations::NONE, "a refused solve makes no room");
    assert_eq!(solved(&mut solver, &answered).allocated, Allocations::NONE, "a solve that answered does");
}

/// Resolves `network` with `solver` into a buffer made beforehand, and answers what the resolve alone allocated, with
/// its work.
///
/// # Panics
/// Panics if the problem is refused.
fn resolved(solver: &mut NetworkSimplex, network: &MincostNetwork) -> (Allocations, Work) {
    let problem = problem_of(network);
    let mut flows = vec![0; network.arc_count()];

    let before = allocations_on_this_thread();
    let answer = solver.resolve_into(&problem, &mut flows).expect("the test's problem should be admitted");
    let allocated = allocations_on_this_thread() - before;

    (allocated, answer.work)
}

#[test]
fn test_no_resolve_allocates_after_a_reservation_from_the_kept_basis_or_the_star() {
    // Per seed, each seeded shape, then the same network a few numbers moved, then itself again: the first resolve of
    // a shape follows a network of other counts and starts from the star; the next two start from the kept basis,
    // the last with no pivot, which only the kept basis gives.
    let seeds = [[3_u64, 1, 4, 1], [5, 9, 2, 6], [92, 65, 35, 89]];
    let mut networks = Vec::new();
    for seed in seeds {
        for network in [supply_shaped(&seed), assignment_shaped(&seed), general(&seed)] {
            let moved = perturbed(&network, &seed);
            networks.push([network, moved.clone(), moved]);
        }
    }

    let node_count = networks.iter().flatten().map(MincostNetwork::node_count).max().unwrap_or(0);
    let arc_count = networks.iter().flatten().map(MincostNetwork::arc_count).max().unwrap_or(0);
    let mut solver = NetworkSimplex::new();
    solver.try_reserve(node_count, arc_count).expect("the room for the networks should be granted");

    let counts = |network: &MincostNetwork| (network.node_count(), network.arc_count());
    assert!(
        networks.windows(2).all(|pair| counts(&pair[0][2]) != counts(&pair[1][0])),
        "each shape after other counts"
    );

    let mut kept_without_pivots = 0;
    for [network, moved, again] in &networks {
        let star = resolved(&mut solver, network);
        let kept = [resolved(&mut solver, moved), resolved(&mut solver, again)];
        assert!(
            [star, kept[0], kept[1]].iter().all(|(allocated, _)| *allocated == Allocations::NONE),
            "{star:?}, {kept:?}"
        );
        kept_without_pivots += usize::from(kept[1].1.pivots == 0);
    }

    assert_eq!(kept_without_pivots, networks.len(), "the same network again starts from its own end");
}

#[test]
fn test_only_a_resolve_from_a_kept_basis_makes_the_room_of_the_next() {
    // A fresh solver, no reservation. The first resolve starts from the star and makes the room a solve makes. The
    // second starts from the kept basis, which reads each node's excess over it: it makes that room, which no start
    // from the star makes. The third, from the kept basis too, allocates nothing.
    let depot = MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1)]);
    let mut solver = NetworkSimplex::new();

    let (star, from_star) = resolved(&mut solver, &depot);
    let (first, from_kept) = resolved(&mut solver, &depot);
    let (second, again) = resolved(&mut solver, &depot);
    assert!(from_star.pivots > 0 && from_kept.pivots == 0 && again.pivots == 0, "{from_star:?}, {from_kept:?}");
    assert_ne!(star, Allocations::NONE, "the star makes the room of a solve");
    assert_ne!(first, Allocations::NONE, "the first start from a kept basis makes the room of the excesses");
    assert_eq!(second, Allocations::NONE, "a resolve after one from a kept basis allocates nothing");
}
