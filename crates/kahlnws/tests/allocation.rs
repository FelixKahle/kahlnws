// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a solve into the caller's buffer allocates: nothing after a reservation for problems of the reserved size and
//! smaller, feasible or not; nothing from the second solve on for a problem no larger than the first, whatever its
//! shape; and room made by a solve that answered, not by one that was refused. A network allocates in its load alone,
//! and not there after a reservation, from the basis it holds or from the star: its solve, every read of its optimum,
//! every cost set on it and every move of supply, refused or not, the solve after them, and every read of the problem
//! it holds allocate nothing. A copy of a network into one with room for it, even room for exactly its source,
//! allocates nothing, and nor do a copy's changes, solves and loads of its source's counts. The binary installs the
//! counting allocator, so it stands alone.

use kahlnws::{Bounds, Network, NetworkSimplex, Outcome, Problem, ProblemError, Work, cost_bound};
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

/// What one load and one solve of a network allocated, each on the calling thread, the solve with every read of its
/// optimum, and whether it found a flow. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LoadedAndSolved {
    /// What the load allocated.
    loaded: Allocations,
    /// What the solve and the reads of its optimum allocated.
    solved: Allocations,
    /// Whether the solve answered an optimal flow.
    optimal: bool,
}

/// Loads `network` into `held` and solves it, reading every flow, a flow by its arc and every potential of an optimum,
/// and answers what each step allocated.
///
/// # Panics
/// Panics if the problem is refused.
fn loaded_and_solved(held: &mut Network, network: &MincostNetwork) -> LoadedAndSolved {
    let before = allocations_on_this_thread();
    held.load(&problem_of(network)).expect("the test's problem should be admitted");
    let loaded = allocations_on_this_thread() - before;

    let before = allocations_on_this_thread();
    let answered = held.solve().outcome.optimal();
    let optimal = match held.optimum() {
        Some(optimum) => {
            let flowing = optimum.flows.iter().filter(|&flow| flow > 0).count();
            let first = optimum.flows.get(0);
            let lowest = optimum.potentials.iter().min();
            flowing <= optimum.flows.len() && (first.is_some() || flowing == 0) && lowest.is_some()
        }
        None => false,
    };
    let read_again = held.optimum().map(|optimum| optimum.total_cost);
    let solved = allocations_on_this_thread() - before;
    assert_eq!(read_again, answered, "the optimum is read exactly where a cost was answered, at that cost");

    LoadedAndSolved { loaded, solved, optimal }
}

#[test]
fn test_no_load_or_solve_of_a_network_allocates_after_a_reservation() {
    // The networks of the solver's test above, with and without a flow, each loaded into one network in turn.
    let mut networks = vec![Family::Supply.network(20), Family::Assignment.network(32), Family::Grid.network(9)];
    for seed in [[3_u64, 1, 4, 1], [5, 9, 2, 6], [92, 65, 35, 89]] {
        networks.extend([supply_shaped(&seed), assignment_shaped(&seed), general(&seed)]);
    }
    networks.push(without_flow());

    let node_count = networks.iter().map(MincostNetwork::node_count).max().unwrap_or(0);
    let arc_count = networks.iter().map(MincostNetwork::arc_count).max().unwrap_or(0);
    let mut held = Network::new();
    held.try_reserve(node_count, arc_count).expect("the room for the networks should be granted");

    let steps: Vec<LoadedAndSolved> = networks.iter().map(|network| loaded_and_solved(&mut held, network)).collect();
    assert!(steps.iter().all(|step| step.loaded == Allocations::NONE && step.solved == Allocations::NONE), "{steps:?}");
    assert!(steps.iter().any(|step| step.optimal) && steps.iter().any(|step| !step.optimal), "{steps:?}");

    let refused = MincostNetwork { supplies: vec![1, 0], ..without_flow() };
    let before = allocations_on_this_thread();
    assert!(held.load(&problem_of(&refused)).is_err());
    assert_eq!(allocations_on_this_thread() - before, Allocations::NONE, "a refusal allocates nothing either");
}

#[test]
fn test_without_a_reservation_a_network_allocates_in_its_first_load_alone() {
    // The first load starts from the star and makes the room, that of the excesses over a basis held among it; its
    // solve, a second load of the same problem, which starts from the basis held, and its solve allocate nothing. The
    // second solve makes no pivot, which only a start from the basis held gives.
    let network = Family::Supply.network(80);
    let mut held = Network::new();

    let first = loaded_and_solved(&mut held, &network);
    assert!(first.loaded != Allocations::NONE, "the first load makes the room");
    assert_eq!(first.solved, Allocations::NONE, "{first:?}");
    assert!(first.optimal);

    let again = loaded_and_solved(&mut held, &network);
    assert_eq!(again, LoadedAndSolved { loaded: Allocations::NONE, solved: Allocations::NONE, optimal: true });
    assert_eq!(held.solve().work, Work::default());
}

/// Loads `network` into `held` and solves it, and answers what the load alone allocated, with the work of the solve.
///
/// # Panics
/// Panics if the problem is refused.
fn loaded_with_work(held: &mut Network, network: &MincostNetwork) -> (Allocations, Work) {
    let before = allocations_on_this_thread();
    held.load(&problem_of(network)).expect("the test's problem should be admitted");
    let allocated = allocations_on_this_thread() - before;

    (allocated, held.solve().work)
}

#[test]
fn test_no_load_allocates_after_a_reservation_from_the_basis_held_or_the_star() {
    // Per seed, each seeded shape, then the same network a few numbers moved, then itself again, loaded in turn into
    // one network reserved for the largest: the first load of a shape follows a network of other counts and starts
    // from the star; the next two start from the basis held, the last with no pivot, which only the basis held gives.
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
    let mut held = Network::new();
    held.try_reserve(node_count, arc_count).expect("the room for the networks should be granted");

    let counts = |network: &MincostNetwork| (network.node_count(), network.arc_count());
    assert!(
        networks.windows(2).all(|pair| counts(&pair[0][2]) != counts(&pair[1][0])),
        "each shape after other counts"
    );

    let mut kept_without_pivots = 0;
    for [network, moved, again] in &networks {
        let star = loaded_with_work(&mut held, network);
        let kept = [loaded_with_work(&mut held, moved), loaded_with_work(&mut held, again)];
        assert!(
            [star, kept[0], kept[1]].iter().all(|(allocated, _)| *allocated == Allocations::NONE),
            "{star:?}, {kept:?}"
        );
        kept_without_pivots += usize::from(kept[1].1.pivots == 0);
    }

    assert_eq!(kept_without_pivots, networks.len(), "the same network again starts from its own end");
}

#[test]
fn test_no_cost_set_on_a_network_or_solve_after_it_allocates() {
    // Seeded networks loaded and solved; then every arc's cost negated, a cost past the bound refused, and a solve,
    // in turns of a few arcs: none of it allocates, from the first change on.
    let networks = [supply_shaped(&[3, 1, 4, 1]), assignment_shaped(&[5, 9, 2, 6]), general(&[92, 65, 35, 89])];
    let mut pivoted = 0;
    for network in &networks {
        let mut held = Network::new();
        let _ = loaded_and_solved(&mut held, network);
        let (bound, arcs) = (cost_bound(network.node_count()), (0..network.arc_count()).collect::<Vec<_>>());

        let before = allocations_on_this_thread();
        for arcs in arcs.chunks(5) {
            for &arc in arcs {
                assert_eq!(held.set_cost(arc, -network.costs[arc]), Ok(network.costs[arc]));
                assert!(held.set_cost(arc, bound + 1).is_err());
            }
            pivoted += held.solve().work.pivots;
        }
        assert_eq!(allocations_on_this_thread() - before, Allocations::NONE, "{network:?}");
    }

    assert!(pivoted > 0, "the changes call for pivots");
}

#[test]
fn test_no_move_of_supply_on_a_network_or_solve_after_it_allocates() {
    // Seeded networks loaded and solved; then two units moved from each node to the next, a move past `i64` refused,
    // and a solve, in turns of a few nodes; then every move set back and a solve: none of it allocates, from the
    // first move on.
    let networks = [supply_shaped(&[3, 1, 4, 1]), assignment_shaped(&[5, 9, 2, 6]), general(&[92, 65, 35, 89])];
    let mut pivoted = 0;
    for network in &networks {
        let mut held = Network::new();
        let _ = loaded_and_solved(&mut held, network);
        let node_count = network.node_count();
        let nodes: Vec<usize> = (0..node_count).collect();

        let before = allocations_on_this_thread();
        for nodes in nodes.chunks(5) {
            for &node in nodes {
                assert_eq!(held.move_supply(node, (node + 1) % node_count, 2), Ok(()));
                assert!(held.move_supply(node, (node + 1) % node_count, u64::MAX).is_err());
            }
            pivoted += held.solve().work.pivots;
        }
        for &node in nodes.iter().rev() {
            assert_eq!(held.move_supply((node + 1) % node_count, node, 2), Ok(()));
        }
        pivoted += held.solve().work.pivots;
        assert_eq!(allocations_on_this_thread() - before, Allocations::NONE, "{network:?}");
    }

    assert!(pivoted > 0, "the moves call for pivots");
}

#[test]
fn test_no_bounds_set_on_a_network_or_solve_after_them_allocate() {
    // Seeded networks loaded and solved; then every arc switched off, its lower bound raised by one where it has room,
    // a lower bound above its capacity refused, and a solve, in turns of a few arcs; then every arc's bounds set back
    // at once and a solve: none of it allocates, from the first change on.
    let networks = [supply_shaped(&[3, 1, 4, 1]), assignment_shaped(&[5, 9, 2, 6]), general(&[92, 65, 35, 89])];
    let mut pivoted = 0;
    for network in &networks {
        let mut held = Network::new();
        let _ = loaded_and_solved(&mut held, network);
        let arcs: Vec<usize> = (0..network.arc_count()).collect();
        let bounds_of =
            |arc: usize| Bounds { lower_bound: network.lower_bounds[arc], capacity: network.capacities[arc] };

        let before = allocations_on_this_thread();
        for arcs in arcs.chunks(5) {
            for &arc in arcs {
                let Bounds { lower_bound, capacity } = bounds_of(arc);
                assert!(held.set_lower_bound(arc, capacity.saturating_add(1)).is_err());
                if lower_bound < capacity {
                    assert_eq!(held.set_lower_bound(arc, lower_bound + 1), Ok(lower_bound));
                    assert_eq!(held.set_lower_bound(arc, lower_bound), Ok(lower_bound + 1));
                }
                if lower_bound == 0 {
                    assert_eq!(held.set_capacity(arc, 0), Ok(capacity));
                }
            }
            pivoted += held.solve().work.pivots;
        }
        for &arc in arcs.iter().rev() {
            assert!(held.set_bounds(arc, bounds_of(arc)).is_ok());
        }
        pivoted += held.solve().work.pivots;
        assert_eq!(allocations_on_this_thread() - before, Allocations::NONE, "{network:?}");
    }

    assert!(pivoted > 0, "the changes call for pivots");
}

#[test]
fn test_no_copy_into_a_network_with_room_for_it_or_change_or_solve_of_the_copy_allocates() {
    // Seeded networks loaded, solved and changed, each copied in turn into one network reserved for the largest, so
    // that each copy but the first goes into a network that held another problem; then on the copy every other arc of
    // lower bound zero switched off and two units moved from each node to the next, with solves between. Nothing
    // allocates from the copy on, and the copy's first solve answers what the network's does, its work included.
    let networks = [supply_shaped(&[3, 1, 4, 1]), assignment_shaped(&[5, 9, 2, 6]), general(&[92, 65, 35, 89])];
    let node_count = networks.iter().map(MincostNetwork::node_count).max().unwrap_or(0);
    let arc_count = networks.iter().map(MincostNetwork::arc_count).max().unwrap_or(0);
    let mut copy = Network::new();
    copy.try_reserve(node_count, arc_count).expect("the room for the networks should be granted");

    let mut pivoted = 0;
    for network in &networks {
        let mut held = Network::new();
        let _ = loaded_and_solved(&mut held, network);
        assert_eq!(held.set_cost(0, -network.costs[0]), Ok(network.costs[0]));
        assert_eq!(held.move_supply(0, network.node_count() - 1, 1), Ok(()));
        let node_count = network.node_count();

        let before = allocations_on_this_thread();
        copy.try_clone_from(&held).expect("the copy should fit the room reserved");
        let copied = copy.solve().work;
        for arc in (0..network.arc_count()).step_by(2).filter(|&arc| network.lower_bounds[arc] == 0) {
            assert!(copy.set_capacity(arc, 0).is_ok());
        }
        pivoted += copy.solve().work.pivots;
        for node in 0..node_count {
            assert_eq!(copy.move_supply(node, (node + 1) % node_count, 2), Ok(()));
        }
        pivoted += copy.solve().work.pivots;
        assert_eq!(allocations_on_this_thread() - before, Allocations::NONE, "{network:?}");

        assert_eq!(copied, held.solve().work, "the copy solves as its source does");
        pivoted += copied.pivots;
    }

    assert!(pivoted > 0, "the changes call for pivots");
}

#[test]
fn test_a_copy_into_room_reserved_for_exactly_its_source_allocates_nothing() {
    // Seeded networks loaded, solved and changed. One copy goes into a network that held nothing and was reserved for
    // exactly the source's counts; another into a network that held nothing, whose own reservation of those counts is
    // counted apart: the copy itself allocates nothing in either, so `try_clone_from`'s reservation covers all it
    // writes. A copy made either way, and one made by `clone`, then loads its source's problem without allocating.
    let networks = [supply_shaped(&[3, 1, 4, 1]), assignment_shaped(&[5, 9, 2, 6]), general(&[92, 65, 35, 89])];
    for network in &networks {
        let mut held = Network::new();
        let _ = loaded_and_solved(&mut held, network);
        assert_eq!(held.move_supply(0, network.node_count() - 1, 1), Ok(()));
        let (node_count, arc_count) = (held.node_count(), held.arc_count());

        let mut reserved = Network::new();
        reserved.try_reserve(node_count, arc_count).expect("the room for the copy should be granted");
        let before = allocations_on_this_thread();
        reserved.try_clone_from(&held).expect("the copy should fit the room reserved");
        assert_eq!(allocations_on_this_thread() - before, Allocations::NONE, "{network:?}");

        let mut fresh = Network::new();
        let before = allocations_on_this_thread();
        fresh.try_reserve(node_count, arc_count).expect("the room for the copy should be granted");
        let reserving = allocations_on_this_thread() - before;
        fresh.try_clone_from(&held).expect("the copy should fit the room reserved");
        assert_eq!(allocations_on_this_thread() - before, reserving, "the copy allocates beyond its reservation");

        let mut cloned = held.clone();
        for copy in [&mut reserved, &mut fresh, &mut cloned] {
            assert_eq!(format!("{copy:?}"), format!("{held:?}"), "a copy prints as its source");
            let before = allocations_on_this_thread();
            copy.load(&problem_of(network)).expect("the seeded network should be admitted");
            assert_eq!(allocations_on_this_thread() - before, Allocations::NONE, "{network:?}");
        }
    }
}

#[test]
fn test_no_read_of_the_problem_a_network_holds_allocates() {
    // Seeded networks loaded, solved and changed; every arc's ends, cost and bounds read one by one and by the views'
    // walks, every supply read, each against the network's own tables: nothing allocates.
    let networks = [supply_shaped(&[3, 1, 4, 1]), assignment_shaped(&[5, 9, 2, 6]), general(&[92, 65, 35, 89])];
    for network in &networks {
        let mut held = Network::new();
        let _ = loaded_and_solved(&mut held, network);
        let cost = -network.costs[0];
        assert_eq!(held.set_cost(0, cost), Ok(network.costs[0]));

        let before = allocations_on_this_thread();
        let (tails, heads, costs) = (held.tails(), held.heads(), held.costs());
        let (lower_bounds, capacities) = (held.lower_bounds(), held.capacities());
        let each = (0..network.arc_count()).all(|arc| {
            let expected_cost = if arc == 0 { cost } else { network.costs[arc] };
            let expected_bounds = Bounds { lower_bound: network.lower_bounds[arc], capacity: network.capacities[arc] };
            (tails.get(arc), heads.get(arc), costs.get(arc), lower_bounds.get(arc), capacities.get(arc))
                == (
                    Some(network.tails[arc]),
                    Some(network.heads[arc]),
                    Some(expected_cost),
                    Some(network.lower_bounds[arc]),
                    Some(network.capacities[arc]),
                )
                && (held.tail(arc), held.head(arc), held.cost(arc), held.bounds(arc))
                    == (network.tails[arc], network.heads[arc], expected_cost, expected_bounds)
                && (held.lower_bound(arc), held.capacity(arc)) == (network.lower_bounds[arc], network.capacities[arc])
        });
        let walked = tails.iter().eq(network.tails.iter().copied())
            && heads.iter().rev().eq(network.heads.iter().rev().copied())
            && costs.iter().skip(1).eq(network.costs.iter().skip(1).copied())
            && lower_bounds.iter().eq(network.lower_bounds.iter().copied())
            && capacities.iter().rev().eq(network.capacities.iter().rev().copied());
        let supplies = held.supplies() == network.supplies.as_slice()
            && (0..network.node_count()).all(|node| held.supply(node) == network.supplies[node]);
        assert_eq!(allocations_on_this_thread() - before, Allocations::NONE, "{network:?}");
        assert!(each && walked && supplies, "{network:?}");
    }
}

#[test]
fn test_no_change_or_solve_of_a_copy_of_a_network_allocates() {
    // Seeded networks loaded and solved, then copied: on each copy every other arc of lower bound zero switched off and
    // two units moved from each node to the next, with solves between, which cut nodes from their arcs; the copy
    // allocates nothing once made.
    let networks = [supply_shaped(&[3, 1, 4, 1]), assignment_shaped(&[5, 9, 2, 6]), general(&[92, 65, 35, 89])];
    let mut pivoted = 0;
    for network in &networks {
        let mut held = Network::new();
        let _ = loaded_and_solved(&mut held, network);
        let mut copy = held.clone();
        let node_count = network.node_count();

        let before = allocations_on_this_thread();
        for arc in (0..network.arc_count()).step_by(2).filter(|&arc| network.lower_bounds[arc] == 0) {
            assert!(copy.set_capacity(arc, 0).is_ok());
        }
        pivoted += copy.solve().work.pivots;
        for node in 0..node_count {
            assert_eq!(copy.move_supply(node, (node + 1) % node_count, 2), Ok(()));
        }
        pivoted += copy.solve().work.pivots;
        assert_eq!(allocations_on_this_thread() - before, Allocations::NONE, "{network:?}");
    }

    assert!(pivoted > 0, "the changes call for pivots");
}

/// The bytes a network reserves for a problem of `node_count` nodes and `arc_count` arcs.
fn reserved_bytes(node_count: usize, arc_count: usize) -> u64 {
    let mut network = Network::new();
    let before = allocations_on_this_thread();
    network.try_reserve(node_count, arc_count).expect("the room should be granted");

    (allocations_on_this_thread() - before).bytes
}

#[test]
fn test_a_network_holds_the_bytes_its_doc_states_for_an_arc_and_a_node() {
    // The network's doc states 57 bytes an arc and 105 a node. A million arcs on one node and a million nodes with no
    // arc: what a count of the other adds, and the bits of the blocks, stay below one byte in a million.
    let million = 1_000_000;
    assert_eq!(reserved_bytes(1, million) / 1_000_000, 57);
    assert_eq!(reserved_bytes(million, 0) / 1_000_000, 105);
}
