// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The public surface of the network: hand-worked problems loaded and solved with their flows, costs, potentials and
//! work written out; the network never loaded; a solve of a network nothing changed, which makes no pivot; a load after
//! a load, from the basis held where the arcs are the same, by hand, on tiny pairs and on sequences of perturbed seeded
//! networks, and from the star where they are not or its excesses pass their limit, and a refused load, which keeps
//! what was held; a copy, which loads as its original; the view of the flows, whose checked and unchecked
//! reads agree; costs set on arcs in and out of the tree, at the bound and past it, supplies moved, by the tree, past
//! what it carries, to the end of `i64` and past it, and bounds set, under a flow and over it, to no width and from
//! none, crossing in one call and refused in two, all set back, by hand and in random sequences; two networks given the
//! same calls, which answer alike, their work included; and every answer against the brute force on tiny networks and
//! against a solver's solve of the same problem and the linear-programming oracle on seeded ones: the outcome, the cost
//! and a valid certificate, never the flow among tied optima or the work, which a network's widest root cost may
//! change.

use kahlnws::{
    Answer, Bounds, ChangeError, Flows, Network, NetworkSimplex, Outcome, Problem, ProblemError, Solution, Work,
    cost_bound,
};
use kahlnws_testing::{
    CostDraw, MincostNetwork, OptimalFlow, TinyBounds, TinyCosts, TinyMincostNetworks, TinySupplies, assignment_shaped,
    brute_force_admits, check_against_brute_force, check_certificate, cost_draws, general, lp_optimum,
    perturbation_draws, perturbed, supply_shaped, tiny_networks,
};
use proptest::prelude::*;
use proptest::sample::Index;

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

/// The depot: node 0 sends 4 units to node 2, directly by arc 0 (capacity 3, cost 5) or through node 1 by arcs 1
/// (capacity 4, cost 1) and 2 (capacity 2, cost 1). Its one optimum carries 2 on every arc, at the cost 14.
fn depot() -> MincostNetwork {
    MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1)])
}

/// Node 0 must send 2 units to node 1, but its one arc carries 1.
fn without_flow() -> MincostNetwork {
    MincostNetwork::from_arcs(&[2, -2], &[(0, 1, 0, 1, 1)])
}

/// A network that holds `network`, loaded.
///
/// # Panics
/// Panics if the problem is refused.
fn loaded(network: &MincostNetwork) -> Network {
    let mut held = Network::new();
    held.load(&problem_of(network)).expect("the test's problem should be admitted");

    held
}

/// The answer of a solve of `network`, with the optimum it then holds copied out, owned.
///
/// # Panics
/// Panics if the cost the solve answered is not the cost of the optimum the network then lends, or if a solve that
/// found no flow leaves an optimum lent.
fn owned(network: &mut Network) -> Answer<Solution> {
    let Answer { outcome, work } = network.solve();
    let outcome = match outcome {
        Outcome::Optimal(cost) => {
            let optimum = network.optimum().expect("a network whose solve found a flow should hold its optimum");
            assert_eq!(cost, optimum.total_cost, "the cost answered should be the cost of the optimum lent");
            Outcome::Optimal(Solution::from(optimum))
        }
        Outcome::Infeasible => {
            assert_eq!(network.optimum(), None, "a solve that found no flow should leave no optimum lent");
            Outcome::Infeasible
        }
    };

    Answer { outcome, work }
}

/// Whether `after` has the node count of `before` and the tail and the head of each of its arcs: a problem a load
/// starts from the basis a network holds of `before`, where the excesses over it keep their limit.
fn same_arcs(before: &MincostNetwork, after: &MincostNetwork) -> bool {
    before.node_count() == after.node_count() && before.tails == after.tails && before.heads == after.heads
}

/// The cost of `answer`'s optimum, `None` where it found no flow.
fn cost_of(answer: &Answer<Solution>) -> Option<i128> {
    answer.outcome.clone().optimal().map(|solution| solution.total_cost)
}

/// `answer`'s optimum as the brute force checks it, `None` where it found no flow.
fn flow_of(answer: &Answer<Solution>) -> Option<OptimalFlow> {
    answer.outcome.clone().optimal().map(|solution| OptimalFlow {
        flows: solution.flows,
        potentials: solution.potentials,
        total_cost: solution.total_cost,
    })
}

/// The solution of the flows `flows`, the potentials `potentials` and the cost `total_cost`, for a test to write out.
fn solution(flows: &[u64], potentials: &[i64], total_cost: i128) -> Solution {
    Solution { flows: flows.to_vec(), potentials: potentials.to_vec(), total_cost }
}

#[test]
fn test_the_depot_loaded_and_solved_by_hand() {
    // From the star at the widest root cost of three nodes, `M = 2·⌊(2^62 − 1) / 3⌋ + 1`, in blocks of one row: arc
    // 0 fills, sending 3; arc 2 enters with no flow, node 1 hanging from node 2; arc 1 sends one unit round by both
    // hub arcs; arc 0 gives one unit back from its capacity, arc 2 filling. Four pivots, one degenerate.
    let mut network = loaded(&depot());
    assert_eq!((network.node_count(), network.arc_count(), network.optimum()), (3, 3, None));

    let answer = owned(&mut network);
    let work = Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 };
    assert_eq!(answer, Answer { outcome: Outcome::Optimal(solution(&[2, 2, 2], &[0, 1, 5], 14)), work });

    // Nothing changed: the same optimum, with no pivot.
    assert_eq!(owned(&mut network), Answer { work: Work::default(), ..answer });
}

#[test]
fn test_lower_bounds_are_counted_in_the_flows_the_cost_and_the_potentials_by_hand() {
    // Node 0 sends 3 units on to node 2 by node 1: arc 0 of bounds 1..=5 and cost 2, arc 1 of bounds 0..=5 and cost
    // −1. The net supplies are 2, 1 and −3. Arc 1, at `−1 − M`, sends one unit, node 1's empty root link leaving;
    // arc 0, at `2 − (M + 1)`, sends two, node 2's root link leaving. Every flow is 3, of cost `6 − 3`.
    let network = MincostNetwork::from_arcs(&[3, 0, -3], &[(0, 1, 1, 5, 2), (1, 2, 0, 5, -1)]);
    let mut held = loaded(&network);

    let answer = owned(&mut held);
    let work = Work { pivots: 2, degenerate_pivots: 0, first_arc_pivots: 0 };
    assert_eq!(answer, Answer { outcome: Outcome::Optimal(solution(&[3, 3], &[0, 2, 1], 3)), work });
}

#[test]
fn test_a_network_without_a_flow_by_hand() {
    // Arc 0, at `1 − M`, fills: one unit, no link leaves; at its capacity it no longer violates.
    let mut network = loaded(&without_flow());

    let answer = network.solve();
    assert_eq!(answer, Answer { outcome: Outcome::Infeasible, work: Work { pivots: 1, ..Work::default() } });
    assert_eq!(network.optimum(), None);
    assert_eq!(network.solve(), Answer { outcome: Outcome::Infeasible, work: Work::default() }, "no pivot again");
}

#[test]
fn test_nodes_without_arcs_by_hand() {
    // Balanced at zero: no flow is needed, and every node sits at potential zero below its empty root link.
    let mut network = loaded(&MincostNetwork::from_arcs(&[0, 0], &[]));
    let answer = owned(&mut network);
    assert_eq!(answer, Answer { outcome: Outcome::Optimal(solution(&[], &[0, 0], 0)), work: Work::default() });

    // A supply with no arc to carry it.
    let mut network = loaded(&MincostNetwork::from_arcs(&[1, -1], &[]));
    assert_eq!(network.solve(), Answer { outcome: Outcome::Infeasible, work: Work::default() });
}

#[test]
fn test_a_network_never_loaded_holds_the_problem_of_no_node() {
    const NETWORK: Network = Network::new();
    let mut network = NETWORK;
    assert_eq!((network.node_count(), network.arc_count()), (0, 0));

    let optimum = network.optimum().expect("the problem of no node should have its optimum");
    assert_eq!((optimum.total_cost, optimum.flows.len(), optimum.potentials), (0, 0, &[][..]));
    assert_eq!(owned(&mut network), Answer { outcome: Outcome::Optimal(solution(&[], &[], 0)), work: Work::default() });
    assert_eq!(owned(&mut Network::default()), owned(&mut network));
}

#[test]
fn test_a_load_after_a_load_answers_as_a_fresh_network() {
    // The depot, then a network without a flow, then the depot again: each load ends the optimum held, and each
    // solve answers what a fresh network answers, its work included.
    let mut network = loaded(&depot());
    let first = owned(&mut network);

    network.load(&problem_of(&without_flow())).expect("the problem should be admitted");
    assert_eq!((network.node_count(), network.arc_count(), network.optimum()), (2, 1, None));
    assert_eq!(owned(&mut network), owned(&mut loaded(&without_flow())));

    network.load(&problem_of(&depot())).expect("the depot should be admitted");
    assert_eq!(network.optimum(), None, "a load ends the optimum, even of the same numbers");
    assert_eq!(owned(&mut network), first);
}

#[test]
fn test_a_refused_load_keeps_what_was_held() {
    // Refused at its checks, by supplies that do not balance; and at its net supplies, past their limit.
    let unbalanced = MincostNetwork { supplies: vec![4, 0, -3], ..depot() };
    let past_limit = MincostNetwork::from_arcs(&[i64::MAX, i64::MIN + 1], &[(1, 0, 1 << 63, 1 << 63, 0)]);
    let refusals = [
        (unbalanced, ProblemError::Unbalanced { total: 1 }),
        (past_limit, ProblemError::NetSupplyBeyondRange { total: u128::from(u64::MAX) }),
    ];

    for (refused, error) in &refusals {
        let mut network = loaded(&depot());
        let solved = owned(&mut network);
        let whole = format!("{network:?}");
        assert_eq!(network.load(&problem_of(refused)), Err(*error));
        assert_eq!(format!("{network:?}"), whole, "a refusal leaves the network as it was");

        assert_eq!((network.node_count(), network.arc_count()), (3, 3));
        assert_eq!(network.optimum().map(Solution::from), solved.outcome.clone().optimal());
        assert_eq!(owned(&mut network), Answer { work: Work::default(), ..solved }, "the basis is kept");

        // The next load starts from the basis held: one unit less to send needs no pivot.
        let fewer = MincostNetwork { supplies: vec![3, 0, -3], ..depot() };
        network.load(&problem_of(&fewer)).expect("the depot should be admitted");
        assert_eq!(network.solve().work, Work::default());

        // A network never loaded stays so.
        let mut fresh = Network::new();
        assert_eq!(fresh.load(&problem_of(refused)), Err(*error));
        assert_eq!((fresh.node_count(), fresh.optimum().map(Solution::from)), (0, Some(solution(&[], &[], 0))));
    }
}

#[test]
fn test_a_load_of_the_same_arcs_starts_from_the_basis_held_by_hand() {
    // The depot's optimum held: node 0 up from the root, nodes 1 and 2 below it by arcs 1 and 0, arc 2 full out of the
    // tree. Loaded with one unit less to send, the tree carries 2 down arc 1 and one down arc 0, arc 2 still full: the
    // optimum, `5 + 2 + 2 = 9`, with no pivot where a fresh network pivots. Loaded with arc 1 at 6, the flows stay and
    // arc 2 at its capacity prices `1 + 6 − 5 > 0`: one pivot moves a unit to arc 0, `3·5 + 6 + 1 = 22`.
    let mut network = loaded(&depot());
    let _ = network.solve();
    let fewer = MincostNetwork { supplies: vec![3, 0, -3], ..depot() };

    network.load(&problem_of(&fewer)).expect("the depot should be admitted");
    assert_eq!(network.optimum(), None, "a load ends the optimum");
    let answer = owned(&mut network);
    assert_eq!(
        answer,
        Answer { outcome: Outcome::Optimal(solution(&[1, 2, 2], &[0, 1, 5], 9)), work: Work::default() }
    );
    let fresh = owned(&mut loaded(&fewer));
    assert_eq!((cost_of(&fresh), fresh.work.pivots > 0), (Some(9), true));

    let mut dearer = depot();
    dearer.costs[1] = 6;
    network.load(&problem_of(&dearer)).expect("the depot should be admitted");
    let answer = owned(&mut network);
    let work = Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 0 };
    assert_eq!(answer, Answer { outcome: Outcome::Optimal(solution(&[3, 1, 1], &[0, 6, 7], 22)), work });
}

#[test]
fn test_a_load_that_switches_a_tree_arc_off_answers_that_no_flow_exists_by_hand() {
    // The depot's optimum held, loaded with arc 1 of capacity 0: node 1 hangs from the root, down, and one pivot moves
    // a unit by arc 0; node 0 can send no more than 3 of its 4 units, as a solver finds too.
    let mut network = loaded(&depot());
    let _ = network.solve();
    let mut held = depot();
    held.capacities[1] = 0;

    network.load(&problem_of(&held)).expect("the depot should be admitted");
    let work = Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 0 };
    assert_eq!(network.solve(), Answer { outcome: Outcome::Infeasible, work });
    assert_eq!(NetworkSimplex::new().solve(&problem_of(&held)).map(|answer| answer.outcome), Ok(Outcome::Infeasible));
}

#[test]
fn test_a_copy_changes_apart_from_its_original() {
    let mut network = loaded(&depot());
    let solved = owned(&mut network);

    let mut copy = network.clone();
    network.load(&problem_of(&without_flow())).expect("the problem should be admitted");
    assert_eq!(copy.optimum().map(Solution::from), solved.outcome.clone().optimal());
    assert_eq!(owned(&mut copy), Answer { work: Work::default(), ..solved.clone() }, "the copy keeps its basis");
    assert_eq!(network.solve().outcome, Outcome::Infeasible);

    // A copy of a network loaded but not solved solves as its original does.
    let loaded_only = loaded(&depot());
    assert_eq!(owned(&mut loaded_only.clone()), solved);

    // A copy holds the basis of its original: loaded with one unit less to send, it starts from it, as the original.
    let fewer = MincostNetwork { supplies: vec![3, 0, -3], ..depot() };
    let mut original = loaded(&depot());
    let _ = original.solve();
    let mut copy = original.try_clone().expect("room for the copy should be granted");
    for network in [&mut original, &mut copy] {
        network.load(&problem_of(&fewer)).expect("the depot should be admitted");
    }
    let answer = owned(&mut original);
    assert_eq!((cost_of(&answer), answer.work), (Some(9), Work::default()));
    assert_eq!(owned(&mut copy), answer);
}

#[test]
fn test_the_view_of_the_flows_reads_each_arc_checked_and_unchecked_by_hand() {
    // Eleven parallel arcs from node 0 to node 1, arc `a` of lower bound `a` and capacity `a`, with nothing else to
    // send: every arc carries its lower bound, dealt to the slots of a stride of six.
    let arcs: Vec<(usize, usize, u64, u64, i64)> = (0..11).map(|arc| (0, 1, arc, arc, 1)).collect();
    let network = MincostNetwork::from_arcs(&[55, -55], &arcs);
    let mut held = loaded(&network);
    held.solve();
    let optimum = held.optimum().expect("the lower bounds should meet the supplies");
    let flows: Flows<'_> = optimum.flows;

    assert_eq!((flows.len(), flows.is_empty()), (11, false));
    for (arc, lower_bound) in (0..11).zip(0_u64..) {
        debug_assert!(arc < flows.len(), "the index should be below its count, as the comment below says");
        // SAFETY: `arc` is below the arc count.
        let unchecked = unsafe { flows.get_unchecked(arc) };
        assert_eq!((flows.get(arc), unchecked), (Some(lower_bound), lower_bound));
    }
    assert_eq!((flows.get(11), flows.get(usize::MAX)), (None, None));
    assert_eq!(flows.iter().rev().collect::<Vec<_>>(), (0..11).rev().collect::<Vec<u64>>());
    assert_eq!((&flows).into_iter().len(), 11);
    assert_eq!(optimum.total_cost, 55);
}

/// Checks what a network answers for `network` against the brute force and against a solver's solve of the same
/// problem: refused exactly as the solve is; otherwise the same outcome and cost, a valid certificate, the cost
/// answered that of the optimum lent, no arc tried before block search; and a second solve the same answer with no
/// pivot. Answers the network's first answer, `None` where it was refused.
fn check_against_the_brute_force_and_a_solve(
    network: &MincostNetwork,
) -> Result<Option<Answer<Solution>>, TestCaseError> {
    let solved = NetworkSimplex::new().solve(&problem_of(network));
    let mut held = Network::new();
    match (held.load(&problem_of(network)), &solved) {
        (Err(refused), Err(error)) => {
            prop_assert_eq!(refused, *error);
            return Ok(None);
        }
        (Ok(()), Ok(_)) => {}
        (loaded, solved) => return Err(TestCaseError::fail(format!("loaded {loaded:?} where solved {solved:?}"))),
    }
    let solved = solved.map_err(|error| TestCaseError::fail(error.to_string()))?;

    let answer = owned(&mut held);
    prop_assert_eq!(answer.work.first_arc_pivots, 0);
    prop_assert!(answer.work.degenerate_pivots <= answer.work.pivots);
    prop_assert_eq!(
        answer.outcome.clone().optimal().map(|solution| solution.total_cost),
        solved.outcome.optimal().map(|solution| solution.total_cost)
    );

    let flow = answer.outcome.clone().optimal().map(|solution| OptimalFlow {
        flows: solution.flows,
        potentials: solution.potentials,
        total_cost: solution.total_cost,
    });
    check_against_brute_force(network, flow.as_ref()).map_err(|breach| TestCaseError::fail(breach.to_string()))?;

    prop_assert_eq!(owned(&mut held), Answer { work: Work::default(), ..answer.clone() });

    Ok(Some(answer))
}

proptest! {
    #[test]
    fn test_tiny_networks_against_the_brute_force_and_a_solve(
        network in tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::OrInverted),
    ) {
        check_against_the_brute_force_and_a_solve(&network)?;
    }

    #[test]
    fn test_tiny_networks_with_tied_costs_against_the_brute_force_and_a_solve(
        network in tiny_networks(TinyCosts::ZeroOrUniform(-9..=9), TinyBounds::OrInverted),
    ) {
        check_against_the_brute_force_and_a_solve(&network)?;
    }

    #[test]
    fn test_tiny_networks_loaded_in_turn_answer_as_fresh_networks(
        networks in proptest::collection::vec(tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered), 1..5),
    ) {
        // One network loads each problem in turn and solves it twice. Where the problem before had other counts or
        // another tail or head at an arc, the load starts from the star and the first answer is a fresh network's, its
        // work included; otherwise it is of a fresh network's outcome and cost, certified against the brute force.
        let mut held = Network::new();
        let mut before: Option<&MincostNetwork> = None;
        for network in &networks {
            held.load(&problem_of(network)).map_err(|error| TestCaseError::fail(error.to_string()))?;
            let fresh = owned(&mut loaded(network));
            let answer = owned(&mut held);
            if before.is_some_and(|before| same_arcs(before, network)) {
                prop_assert_eq!(cost_of(&answer), cost_of(&fresh));
                check_against_brute_force(network, flow_of(&answer).as_ref())
                    .map_err(|breach| TestCaseError::fail(breach.to_string()))?;
            } else {
                prop_assert_eq!(&answer, &fresh);
            }
            prop_assert_eq!(owned(&mut held), Answer { work: Work::default(), ..answer });
            before = Some(network);
        }
    }

    #[test]
    fn test_tiny_pairs_loaded_one_after_the_other_answer_as_the_brute_force(
        (first, second) in TinyMincostNetworks {
            arcs: 0..=7,
            costs: TinyCosts::ZeroOrUniform(-9..=9),
            supplies: TinySupplies::OfAFlowOrRandom,
            bounds: TinyBounds::Ordered,
        }.pairs(),
    ) {
        // The second of each pair on the arcs of the first, loaded after the first was solved: from the basis held,
        // of the brute force's outcome and cost, certified, and solved again with no pivot.
        let mut held = loaded(&first);
        let _ = held.solve();
        held.load(&problem_of(&second)).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let answer = owned(&mut held);
        check_against_brute_force(&second, flow_of(&answer).as_ref())
            .map_err(|breach| TestCaseError::fail(breach.to_string()))?;
        prop_assert_eq!(answer.work.first_arc_pivots, 0);
        prop_assert_eq!(owned(&mut held), Answer { work: Work::default(), ..answer });
    }
}

#[test]
fn test_the_tiny_networks_reach_every_kind_of_answer() {
    // The properties above can fail only where their networks reach refusals, optima, networks without a flow, and
    // solves that pivot: counted over a fixed run of the same strategy.
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let strategy = tiny_networks(TinyCosts::ZeroOrUniform(-9..=9), TinyBounds::OrInverted);
    let mut counts = [0_usize; 4];
    for _ in 0..512 {
        let network = strategy.new_tree(&mut runner).expect("a tiny network should be drawn").current();
        let checked = check_against_the_brute_force_and_a_solve(&network).expect("the network should pass");
        let Some(answer) = checked else {
            counts[0] += 1;
            continue;
        };

        counts[1] += usize::from(matches!(answer.outcome, Outcome::Optimal(_)));
        counts[2] += usize::from(answer.outcome == Outcome::Infeasible);
        counts[3] += usize::from(answer.work.pivots > 0);
    }

    assert!(counts.iter().all(|&count| count > 0), "refused, optimal, without a flow, pivoting: {counts:?}");
}

/// The seeds the seeded generators draw from in these tests: forty of twelve numbers each, the steps of a linear
/// congruential generator from the seed's index.
fn seeds() -> impl Iterator<Item = [u64; 12]> {
    (0..40_u64).map(|index| {
        let mut state = index;
        core::array::from_fn(|_| {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            state >> 33
        })
    })
}

#[test]
fn test_the_seeded_shapes_against_a_solve_and_the_oracle() {
    // Per shape and seed: the network's outcome and cost are the solver's and the oracle's, its optimum certified.
    let mut outcomes = [0_usize; 2];
    for seed in seeds() {
        for (shape, network) in
            [("supply", supply_shaped(&seed)), ("assignment", assignment_shaped(&seed)), ("general", general(&seed))]
        {
            let name = format!("{shape} {seed:?}");
            let mut held = loaded(&network);
            let answer = owned(&mut held);
            let solved =
                NetworkSimplex::new().solve(&problem_of(&network)).expect("the seeded network should be admitted");

            let cost = answer.outcome.clone().optimal().map(|solution| solution.total_cost);
            assert_eq!(cost, solved.outcome.optimal().map(|solution| solution.total_cost), "{name}");
            assert_eq!(cost, lp_optimum(&network), "{name}");
            if let Outcome::Optimal(found) = &answer.outcome {
                let certified = check_certificate(&network, &found.flows, &found.potentials, found.total_cost);
                assert_eq!(certified, Ok(()), "{name}");
            }
            outcomes[usize::from(cost.is_none())] += 1;
        }
    }

    assert!(outcomes.iter().all(|&count| count > 0), "optimal and without a flow: {outcomes:?}");
}

/// The three seeded shapes drawn from `seed`.
fn seeded_shapes(seed: &[u64]) -> [MincostNetwork; 3] {
    [supply_shaped(seed), assignment_shaped(seed), general(seed)]
}

#[test]
fn test_the_same_problem_loaded_again_makes_no_pivot_where_a_fresh_network_makes_some() {
    // The basis held is the problem's own end, which the re-flow leaves as it is: the same answer, with no pivot, its
    // outcome optimal or not. The work tells the start from the basis held from the star wherever the solve pivots.
    let mut pivoting = 0;
    for seed in seeds() {
        for network in seeded_shapes(&seed) {
            let mut held = loaded(&network);
            let solved = owned(&mut held);
            pivoting += usize::from(solved.work.pivots > 0);

            held.load(&problem_of(&network)).expect("the seeded network should be admitted");
            assert_eq!(owned(&mut held), Answer { work: Work::default(), ..solved }, "{seed:?}");
        }
    }

    assert!(pivoting > 10, "{pivoting} of the test's problems pivot in a solve");
}

#[test]
fn test_a_problem_of_other_counts_loads_as_into_a_fresh_network() {
    // Per seed, the three shapes loaded in turn into one network, each after a solve of the one before: wherever the
    // counts differ, the load starts from the star, and the answer is a fresh network's, its work included.
    let mut star_loads = 0;
    for seed in seeds() {
        let mut held = Network::new();
        let mut before: Option<MincostNetwork> = None;
        for network in seeded_shapes(&seed) {
            held.load(&problem_of(&network)).expect("the seeded network should be admitted");
            let answer = owned(&mut held);
            if before.as_ref().is_none_or(|before| !same_arcs(before, &network)) {
                assert_eq!(answer, owned(&mut loaded(&network)), "{seed:?}");
                star_loads += 1;
            }
            before = Some(network);
        }
    }

    assert!(star_loads > 80, "{star_loads} loads of other counts");
}

/// The networks of a sequence of `length`, each perturbed from the one before by its own seed drawn from `seed`, the
/// first `network` itself.
fn perturbed_sequence(network: MincostNetwork, seed: &[u64], length: u64) -> Vec<MincostNetwork> {
    let mut sequence = vec![network];
    for step in 1..length {
        let last = sequence.last().expect("the sequence should hold its first network");
        let next = perturbed(last, &[seed[0], seed[1], seed[2], step]);
        sequence.push(next);
    }

    sequence
}

#[test]
fn test_sequences_of_perturbed_networks_loaded_in_turn_answer_as_a_solve() {
    // Per shape and seed, a sequence of eight networks, each a few numbers moved from the one before, loaded in turn
    // into one network and solved: each answer of the outcome and the cost a solver's solve answers, certified, with
    // no arc tried first. Every load starts from the basis held, so that the pivots of all solves, summed, stay below
    // those of a fresh network's solves of the same problems.
    let (mut held_pivots, mut fresh_pivots, mut outcomes) = (0, 0, [0_usize; 2]);

    for seed in seeds().take(24) {
        for network in seeded_shapes(&seed) {
            let sequence = perturbed_sequence(network, &seed, 8);
            let mut held = loaded(&sequence[0]);
            let _ = held.solve();

            for (step, network) in sequence.iter().enumerate().skip(1) {
                held.load(&problem_of(network)).expect("a perturbed network should be admitted");
                let answer = solved_as_a_solver_solves(&mut held, network);
                assert_eq!(answer.work.first_arc_pivots, 0, "{seed:?}, step {step}");
                held_pivots += answer.work.pivots;
                fresh_pivots += loaded(network).solve().work.pivots;
                outcomes[usize::from(answer.outcome == Outcome::Infeasible)] += 1;
            }
        }
    }

    assert!(outcomes.iter().all(|&count| count > 0), "optimal and infeasible loads: {outcomes:?}");
    assert!(held_pivots < fresh_pivots, "{held_pivots} pivots from the basis held, {fresh_pivots} from the star");
}

#[test]
fn test_the_same_problem_loaded_past_the_limit_answers_as_a_fresh_network() {
    // Two nodes of no supply, an arc each way of capacity `u64::MAX` at cost −1: the solve ends with one arc carrying
    // `u64::MAX` out of the tree, so the excesses over that basis add up to `u64::MAX`, past the limit. The basis held
    // cannot serve, and the load of the same problem starts from the star, as a fresh network's does.
    let network = MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, u64::MAX, -1), (1, 0, 0, u64::MAX, -1)]);
    let mut held = loaded(&network);
    let solved = owned(&mut held);
    assert!(solved.work.pivots > 0, "{solved:?}");

    held.load(&problem_of(&network)).expect("the network should be admitted");
    assert_eq!(owned(&mut held), solved);
}

/// One change of a seeded network of at least one arc, drawn from its step `step` alone: a cost, a capacity, or a
/// move of one unit of supply, on the arc or between the nodes the step names. Answers what the change answered, its
/// number as an `i128`.
fn change_of_step(network: &mut Network, step: usize) -> Result<i128, ChangeError> {
    let (arc, node_count) = (step * 7 % network.arc_count(), network.node_count());
    match step % 3 {
        0 => network.set_cost(arc, i64::try_from(step % 11).expect("a small step should fit") - 5).map(i128::from),
        1 => network.set_capacity(arc, u64::try_from(step % 4).expect("a small step should fit")).map(i128::from),
        _ => network.move_supply(step % node_count, (3 * step + 1) % node_count, 1).map(|()| 0),
    }
}

#[test]
fn test_two_networks_given_the_same_calls_answer_alike_their_work_included() {
    // Per shape and seed, two networks loaded with the same problem are given the same changes and solves in the same
    // order: every change answers alike, every solve answers the same outcome, flows, potentials and work, and the two
    // stand alike throughout.
    let mut pivoted = 0;
    for seed in seeds().take(12) {
        for network in [supply_shaped(&seed), general(&seed)].into_iter().filter(|network| network.arc_count() > 0) {
            let (mut first, mut second) = (loaded(&network), loaded(&network));
            for step in 0..24 {
                assert_eq!(change_of_step(&mut first, step), change_of_step(&mut second, step), "step {step}");
                if step % 2 == 1 {
                    let answer = owned(&mut first);
                    assert_eq!(answer, owned(&mut second), "step {step} of {seed:?}");
                    pivoted += usize::from(answer.work.pivots > 0);
                }
                assert_eq!(format!("{first:?}"), format!("{second:?}"), "step {step} of {seed:?}");
            }
        }
    }

    assert!(pivoted > 0, "some solve after a change pivots");
}

/// Solves `network`, which holds `held`, and checks the answer against a solver's solve of `held`: the same outcome
/// and cost, a valid certificate, and the cost answered that of the optimum lent. Answers the answer, owned.
///
/// # Panics
/// Panics if `held` is refused, or the answer is not the solver's.
fn solved_as_a_solver_solves(network: &mut Network, held: &MincostNetwork) -> Answer<Solution> {
    let answer = owned(network);
    let solved = NetworkSimplex::new().solve(&problem_of(held)).expect("the problem held should be admitted");
    let cost = answer.outcome.clone().optimal().map(|solution| solution.total_cost);

    assert_eq!(cost, solved.outcome.optimal().map(|solution| solution.total_cost));
    if let Outcome::Optimal(found) = &answer.outcome {
        assert_eq!(check_certificate(held, &found.flows, &found.potentials, found.total_cost), Ok(()));
    }

    answer
}

/// The depot with a dearer arc 3 beside arc 0, of capacity 5 and cost 9, which its optimum leaves empty.
fn depot_with_a_dear_arc() -> MincostNetwork {
    MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1), (0, 2, 0, 5, 9)])
}

#[test]
fn test_a_cost_out_of_the_tree_at_its_capacity_by_hand() {
    // The depot's optimum: node 0 at potential 0, node 1 at 1, node 2 at 5; arc 2, from node 1 to node 2, at its
    // capacity 2. From cost 1 to 3 it still prices `3 + 1 − 5 ≤ 0` at its capacity: no pivot, the same flows, and
    // the cost `2·5 + 2·1 + 2·3 = 18`.
    let mut held = depot();
    let mut network = loaded(&held);
    let _ = network.solve();

    assert_eq!(network.set_cost(2, 3), Ok(1));
    held.costs[2] = 3;
    assert_eq!(network.optimum(), None, "a change ends the optimum, even one the flow keeps");
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(
        answer,
        Answer { outcome: Outcome::Optimal(solution(&[2, 2, 2], &[0, 1, 5], 18)), work: Work::default() }
    );
}

#[test]
fn test_a_cost_out_of_the_tree_at_its_lower_bound_by_hand() {
    // Arc 3, empty at cost 9, from cost 9 to 3: cheaper than arc 0's 5, it takes the two units arc 0 carried. Two
    // units through node 1 at 2 and two by arc 3 at 3: `4 + 6 = 10`.
    let mut held = depot_with_a_dear_arc();
    let mut network = loaded(&held);
    let solved = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(solved.outcome.optimal().map(|optimum| optimum.flows), Some(vec![2, 2, 2, 0]));

    assert_eq!(network.set_cost(3, 3), Ok(9));
    held.costs[3] = 3;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    let optimum = answer.outcome.optimal().expect("the routes should carry the supply");
    assert_eq!((optimum.flows, optimum.total_cost), (vec![0, 2, 2, 2], 10));
    assert!(answer.work.pivots > 0);
}

#[test]
fn test_a_cost_on_tree_arcs_running_down_by_hand() {
    // In the depot's optimum nodes 1 and 2 hang from node 0 by arcs 1 and 0, both running down to them. Arc 1 from
    // cost 1 to 4: node 1 rises by 3 to 4, and the route through it costs 5, as arc 0 does: the flow stays, at 20.
    let mut held = depot();
    let mut network = loaded(&held);
    let _ = network.solve();

    assert_eq!(network.set_cost(1, 4), Ok(1));
    held.costs[1] = 4;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(
        answer,
        Answer { outcome: Outcome::Optimal(solution(&[2, 2, 2], &[0, 4, 5], 20)), work: Work::default() }
    );

    // Arc 0 from cost 5 to 2: node 2 falls by 3 to 2, and arc 2 at its capacity now prices `1 + 4 − 2 > 0`, so it
    // gives back what arc 0 can take: arc 0 fills at 3, one unit goes through node 1, `3·2 + 4 + 1 = 11`.
    assert_eq!(network.set_cost(0, 2), Ok(5));
    held.costs[0] = 2;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    let optimum = answer.outcome.optimal().expect("the routes should carry the supply");
    assert_eq!((optimum.flows, optimum.total_cost), (vec![3, 1, 1], 11));
    assert!(answer.work.pivots > 0);
}

#[test]
fn test_a_cost_on_a_tree_arc_running_up_by_hand() {
    // The depot turned around: node 2 sends 4 units to node 0, directly by arc 0 at 5 or through node 1 by arcs 2
    // and 1 at 1 each. Solved, node 2 is at 0, node 0 at 5 below arc 0, and node 1 at 4, hanging from node 0 by arc
    // 1, which runs up from it. Arc 1 from cost 1 to 3: node 1 falls by 2 to 2; arc 2 at its capacity prices
    // `1 + 0 − 2 ≤ 0`, so the flow stays: `2·5 + 2·3 + 2·1 = 18`.
    let mut held = MincostNetwork::from_arcs(&[-4, 0, 4], &[(2, 0, 0, 3, 5), (1, 0, 0, 4, 1), (2, 1, 0, 2, 1)]);
    let mut network = loaded(&held);
    let solved = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(solved.outcome.optimal().map(|optimum| optimum.potentials), Some(vec![5, 4, 0]));

    assert_eq!(network.set_cost(1, 3), Ok(1));
    held.costs[1] = 3;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(
        answer,
        Answer { outcome: Outcome::Optimal(solution(&[2, 2, 2], &[5, 2, 0], 18)), work: Work::default() }
    );
}

#[test]
fn test_a_cost_on_an_arc_of_no_width_by_hand() {
    // Arc 3 carries one unit from node 0 to node 1, fixed, at 4. Node 1 can pass on 2 by arc 2, so arc 1 brings one
    // more and arc 0 the other two: `2·5 + 1 + 2 + 4 = 17`. At cost −7 the fixed unit costs 11 less, and nothing moves.
    let mut held = depot();
    held.add_arc(0, 1, 1, 1, 4);
    let mut network = loaded(&held);
    let solved = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(
        solved.outcome.optimal().map(|optimum| (optimum.flows, optimum.total_cost)),
        Some((vec![2, 1, 2, 1], 17))
    );

    assert_eq!(network.set_cost(3, -7), Ok(4));
    held.costs[3] = -7;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    let optimum = answer.outcome.optimal().expect("the routes should carry the supply");
    assert_eq!((optimum.flows, optimum.total_cost, answer.work.pivots), (vec![2, 1, 2, 1], 6, 0));
}

#[test]
fn test_a_cost_set_to_the_cost_held_keeps_the_optimum() {
    let held = depot();
    let mut network = loaded(&held);
    let solved = owned(&mut network);

    for (arc, cost) in [(0, 5), (1, 1), (2, 1)] {
        assert_eq!(network.set_cost(arc, cost), Ok(cost));
    }
    assert_eq!(network.optimum().map(Solution::from), solved.outcome.clone().optimal());
    assert_eq!(owned(&mut network), Answer { work: Work::default(), ..solved });
}

#[test]
fn test_costs_at_the_bound_are_set_and_one_past_it_refused_by_hand() {
    // The bound of three nodes, `B = 1537228672809129301`. Arc 0 at `B`: two units through node 1 and two by arc 0,
    // `2 + 2 + 2·B`. At `−B`: arc 0 fills, `−3·B`, and one unit goes through node 1, `2`.
    let bound = cost_bound(3);
    assert_eq!(bound, 1_537_228_672_809_129_301);
    let mut held = depot();
    let mut network = loaded(&held);
    let _ = network.solve();

    assert_eq!(network.set_cost(0, bound), Ok(5));
    held.costs[0] = bound;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(answer.outcome.optimal().map(|optimum| optimum.total_cost), Some(4 + 2 * i128::from(bound)));

    assert_eq!(network.set_cost(0, -bound), Ok(bound));
    held.costs[0] = -bound;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    let at_negative_bound = Some(2 - 3 * i128::from(bound));
    assert_eq!(answer.outcome.clone().optimal().map(|optimum| optimum.total_cost), at_negative_bound);

    // One past either end: refused, and nothing changed, the optimum held among it.
    for cost in [bound + 1, -bound - 1] {
        assert_eq!(network.set_cost(0, cost), Err(ChangeError::CostBeyondBound { arc: 0, cost, node_count: 3 }));
        assert_eq!(network.optimum().map(Solution::from), answer.outcome.clone().optimal());
    }
    assert_eq!(owned(&mut network), Answer { work: Work::default(), ..answer });
}

#[test]
fn test_a_cost_that_would_take_the_cost_total_past_its_range_is_refused_by_hand() {
    // Four loops at one node, of the bound `B = 2^62 − 1` of one node: `B` and `−B` at `2^64 − 1`, 2 at `2^64 − 1`, and
    // 1 at `2^63 − 1`: their costs times their capacities add up to `i128::MAX` exactly. Any change that raises a
    // magnitude is refused; one of the same magnitude is not. Loops of negative cost fill: `−(B + 2)·(2^64 − 1)`.
    let (bound, wide) = (cost_bound(1), u64::MAX);
    let arcs = [(0, 0, 0, wide, bound), (0, 0, 0, wide, -bound), (0, 0, 0, wide, 2), (0, 0, 0, (1 << 63) - 1, 1)];
    let mut held = MincostNetwork::from_arcs(&[0], &arcs);
    let mut network = loaded(&held);

    for (arc, cost) in [(3, 2), (3, -2), (2, 3), (0, bound + 1)] {
        let refused = if cost > bound {
            ChangeError::CostBeyondBound { arc, cost, node_count: 1 }
        } else {
            ChangeError::CostTotalBeyondRange { arc }
        };
        assert_eq!(network.set_cost(arc, cost), Err(refused));
    }
    assert_eq!(network.set_cost(2, -2), Ok(2));
    held.costs[2] = -2;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    let expected = -(i128::from(bound) + 2) * i128::from(u64::MAX);
    assert_eq!(answer.outcome.optimal().map(|optimum| optimum.total_cost), Some(expected));
}

#[test]
#[should_panic(expected = "called `Network::set_cost` with arc 3, which should be below the arc count 3")]
fn test_a_cost_on_an_arc_past_the_arcs_panics() {
    let _ = loaded(&depot()).set_cost(3, 1);
}

#[test]
#[should_panic(expected = "called `Network::set_cost` with arc 0, which should be below the arc count 0")]
fn test_a_cost_on_a_network_never_loaded_panics() {
    let _ = Network::new().set_cost(0, 1);
}

/// The depot turned around: node 2 sends 4 units to node 0, directly by arc 0 at 5 or through node 1 by arcs 2 and 1
/// at 1 each. Its one optimum carries 2 on every arc, at the cost 14.
fn depot_turned_around() -> MincostNetwork {
    MincostNetwork::from_arcs(&[-4, 0, 4], &[(2, 0, 0, 3, 5), (1, 0, 0, 4, 1), (2, 1, 0, 2, 1)])
}

#[test]
fn test_a_move_the_tree_carries_by_hand() {
    // The depot's optimum, potentials 0, 1, 5. The customer, node 2, needs one more unit, which node 1 sends: arc 2 is
    // full, so node 1's unit takes one of the depot's places there and the depot sends one more directly, arc 1 one
    // less. Every leg carries it; no pivot: `3·5 + 1 + 2 = 18`.
    let mut held = depot();
    let mut network = loaded(&held);
    let _ = network.solve();

    assert_eq!(network.move_supply(2, 1, 1), Ok(()));
    held.supplies = vec![4, 1, -5];
    assert_eq!(network.optimum(), None, "a move ends the optimum, even one the flow keeps");
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(
        answer,
        Answer { outcome: Outcome::Optimal(solution(&[3, 1, 2], &[0, 1, 5], 18)), work: Work::default() }
    );
}

#[test]
fn test_a_move_past_what_the_arcs_carry_and_its_set_back_by_hand() {
    // The customer, node 2, needs two more units, sent by the depot, node 0, above it: arc 0 fills, and nothing can
    // bring node 2 a sixth unit. Set back, the depot's optimum again, at 14.
    let mut held = depot();
    let mut network = loaded(&held);
    let solved = owned(&mut network);

    assert_eq!(network.move_supply(2, 0, 2), Ok(()));
    held.supplies = vec![6, 0, -6];
    assert_eq!(network.solve(), Answer { outcome: Outcome::Infeasible, work: Work::default() });
    assert_eq!(NetworkSimplex::new().solve(&problem_of(&held)).map(|answer| answer.outcome), Ok(Outcome::Infeasible));

    assert_eq!(network.move_supply(0, 2, 2), Ok(()));
    held.supplies = vec![4, 0, -4];
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(answer.outcome.optimal().map(|optimum| optimum.total_cost), Some(14));
    assert_eq!(solved.outcome.optimal().map(|optimum| optimum.total_cost), Some(14));
}

#[test]
fn test_a_move_that_fills_a_link_running_up_by_hand() {
    // The depot turned around, solved. Node 1 takes two units of node 0's demand: arc 1, which runs up from node 1,
    // fills exactly; the flow stays optimal, two more through node 1 and two directly: `2·5 + 2 + 4 = 16`.
    let mut held = depot_turned_around();
    let mut network = loaded(&held);
    let _ = network.solve();
    let copy = network.clone();

    assert_eq!(network.move_supply(0, 1, 2), Ok(()));
    held.supplies = vec![-6, 2, 4];
    let answer = solved_as_a_solver_solves(&mut network, &held);
    let optimum = answer.outcome.optimal().expect("the routes should carry the supply");
    assert_eq!((optimum.flows, optimum.total_cost), (vec![2, 4, 2], 16));

    // Three instead of two: arc 1 can take only two more; the third goes round by arcs 2 and 1 in place of one of
    // node 2's, which goes directly: `3·5 + 4 + 1 = 20`.
    let mut network = copy;
    let mut held = depot_turned_around();
    assert_eq!(network.move_supply(0, 1, 3), Ok(()));
    held.supplies = vec![-7, 3, 4];
    let answer = solved_as_a_solver_solves(&mut network, &held);
    let optimum = answer.outcome.optimal().expect("the routes should carry the supply");
    assert_eq!((optimum.flows, optimum.total_cost), (vec![3, 4, 1], 20));
}

#[test]
fn test_a_move_of_nothing_keeps_the_optimum() {
    let held = depot();
    let mut network = loaded(&held);
    let solved = owned(&mut network);

    for (from_node, to_node, amount) in [(0, 0, 3), (2, 2, u64::MAX), (1, 2, 0)] {
        assert_eq!(network.move_supply(from_node, to_node, amount), Ok(()));
    }
    assert_eq!(network.optimum().map(Solution::from), solved.outcome.clone().optimal());
    assert_eq!(owned(&mut network), Answer { work: Work::default(), ..solved });
}

#[test]
fn test_supplies_to_the_end_of_i64_and_one_past_it_by_hand() {
    // Node 1 sends node 0's supply over an arc of capacity `2^64 − 1` at 1 a unit. Node 0 takes `2^63 − 1` of node 1's
    // supply: its own is `i64::MAX`, node 1's `−i64::MAX`, and the arc carries it all.
    let mut held = MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, u64::MAX, 1)]);
    let mut network = loaded(&held);
    let _ = network.solve();
    let most = i64::MAX.cast_unsigned();

    assert_eq!(network.move_supply(1, 0, most), Ok(()));
    held.supplies = vec![i64::MAX, -i64::MAX];
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(answer.outcome.clone().optimal().map(|optimum| optimum.flows), Some(vec![most]));

    // One more for node 0 passes `i64::MAX`; the largest amount takes node 1's supply past `i64::MIN`, named first.
    // Refused, either changes nothing.
    let past = ChangeError::SupplyBeyondRange { node: 0, supply: i128::from(i64::MAX) + 1 };
    assert_eq!(network.move_supply(1, 0, 1), Err(past));
    let below = ChangeError::SupplyBeyondRange { node: 1, supply: -i128::from(i64::MAX) - i128::from(u64::MAX) };
    assert_eq!(network.move_supply(1, 0, u64::MAX), Err(below));
    assert_eq!(network.optimum().map(Solution::from), answer.outcome.clone().optimal());

    // Set back: nothing to send, at no cost.
    assert_eq!(network.move_supply(0, 1, most), Ok(()));
    held.supplies = vec![0, 0];
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(answer.outcome.optimal().map(|optimum| optimum.total_cost), Some(0));
}

#[test]
fn test_a_move_right_after_a_load_is_refused_past_what_the_load_left_to_be_sent_by_hand() {
    // Five nodes and no arc, supplies `2^63 − 1` twice, `−(2^63 − 1)` twice and 0: the positive net supplies add up to
    // `2^64 − 2`, the limit, and the load is admitted. Its star sends nothing along an arc, so it leaves all of them to
    // be sent. One unit from node 2 to node 4 keeps both supplies in range, `i64::MIN` and 1, but `2^64 − 2 + 1` to be
    // sent passes the limit: refused, nothing changed.
    let mut network = loaded(&MincostNetwork::from_arcs(&[i64::MAX, i64::MAX, -i64::MAX, -i64::MAX, 0], &[]));
    let whole = format!("{network:?}");
    let refused = ChangeError::ExcessBeyondRange { total: u128::from(u64::MAX) };
    assert_eq!(network.move_supply(2, 4, 1), Err(refused));
    assert_eq!(format!("{network:?}"), whole);

    // The solve finds no flow, with no arc to send it along, and leaves the root links as they were: the same move is
    // refused with the same total.
    assert_eq!(network.solve(), Answer { outcome: Outcome::Infeasible, work: Work::default() });
    assert_eq!(network.move_supply(2, 4, 1), Err(refused));
    assert_eq!(network.supplies(), [i64::MAX, i64::MAX, -i64::MAX, -i64::MAX, 0]);
}

#[test]
fn test_a_move_past_the_flow_the_root_links_admit_is_refused_by_hand() {
    // Four nodes and no arc: two moves of `2^63 − 1` strand their flow, `2^64 − 2` up the root links. A third move,
    // of the largest amount whose supplies stay within `i64`, or of one unit, is refused, and changes nothing.
    let mut held = MincostNetwork::from_arcs(&[0, 0, 0, 0], &[]);
    let mut network = loaded(&held);
    let most = i64::MAX.cast_unsigned();
    assert_eq!((network.move_supply(2, 0, most), network.move_supply(3, 1, most)), (Ok(()), Ok(())));
    held.supplies = vec![i64::MAX, i64::MAX, -i64::MAX, -i64::MAX];

    let limit = u128::from(u64::MAX - 1);
    assert_eq!(network.move_supply(2, 3, 1), Err(ChangeError::ExcessBeyondRange { total: limit + 1 }));
    assert_eq!(
        network.move_supply(1, 3, most),
        Err(ChangeError::ExcessBeyondRange { total: limit + u128::from(most) })
    );
    assert_eq!(network.solve(), Answer { outcome: Outcome::Infeasible, work: Work::default() });

    // The solve found no flow and left the root links as they were: still refused.
    assert_eq!(network.move_supply(2, 3, 1), Err(ChangeError::ExcessBeyondRange { total: limit + 1 }));
}

#[test]
fn test_a_network_never_loaded_answers_the_cost_zero_and_lends_the_optimum_of_no_node() {
    // The problem of no node: the empty flow, at the cost zero, with no work; its optimum is lent, with no flow and no
    // potential, before the solve and after it.
    let mut network = Network::new();
    let empty = |network: &Network| {
        network.optimum().map(|optimum| (optimum.total_cost, optimum.flows.len(), optimum.potentials.len()))
    };
    assert_eq!(empty(&network), Some((0, 0, 0)));
    assert_eq!(network.solve(), Answer { outcome: Outcome::Optimal(0), work: Work::default() });
    assert_eq!(empty(&network), Some((0, 0, 0)));
}

/// The depot's network, solved: its optimum costs 14.
fn solved_depot() -> Network {
    let mut network = loaded(&depot());
    assert_eq!(cost_of(&owned(&mut network)), Some(14));

    network
}

#[test]
fn test_a_supplier_that_falls_and_a_consumer_whose_demand_rises_by_hand() {
    // The depot is to send 2: the customer, balancing it, takes 2. Both go through node 1, at 2 a unit: 4.
    let mut network = solved_depot();
    assert_eq!(network.set_supply(0, 2, 2), Ok(4));
    let cost = cost_of(&owned(&mut network));
    assert_eq!((network.supplies(), cost), (&[2, 0, -2][..], Some(4)));

    // The customer is to take 5, as the node set and as the balancing node alike: 2 through node 1, 3 directly, 19.
    for (node, supply, balancing_node, replaced) in [(2, -5, 0, -4), (0, 5, 2, 4)] {
        let mut network = solved_depot();
        assert_eq!(network.set_supply(node, supply, balancing_node), Ok(replaced));
        let cost = cost_of(&owned(&mut network));
        assert_eq!((network.supplies(), cost), (&[5, 0, -5][..], Some(19)));

        // The answer set back restores both nodes and the cost.
        assert_eq!(network.set_supply(node, replaced, balancing_node), Ok(supply));
        let cost = cost_of(&owned(&mut network));
        assert_eq!((network.supplies(), cost), (&[4, 0, -4][..], Some(14)));
    }
}

#[test]
fn test_a_node_that_changes_sides_by_hand() {
    // Node 1 is to send 2, balanced by the depot, which sends 2 less; then node 1 is to take 1, balanced by the
    // customer, which takes 3 less. The depot sends 1 to node 1 and 1 on to the customer: `2 + 1 = 3`.
    let mut network = solved_depot();
    assert_eq!(network.set_supply(1, 2, 0), Ok(0));
    assert_eq!(network.set_supply(1, -1, 2), Ok(2));
    let cost = cost_of(&owned(&mut network));
    assert_eq!((network.supplies(), cost), (&[2, -1, -1][..], Some(3)));
}

#[test]
fn test_a_supply_set_to_what_it_is_changes_nothing_by_hand() {
    // As a move of nothing: the optimum stands, and the solve after makes no pivot.
    let mut network = solved_depot();
    let optimum = network.optimum().map(Solution::from);
    assert_eq!((network.set_supply(0, 4, 2), network.set_supply(1, 0, 1)), (Ok(4), Ok(0)));
    assert_eq!(network.optimum().map(Solution::from), optimum);
    assert_eq!(network.solve().work, Work::default());
}

#[test]
fn test_a_supply_that_rises_past_what_its_balancing_node_can_give_is_refused_by_hand() {
    // Node 0 takes `2^63 − 1`, node 1 sends it, node 2 holds nothing. Node 2 is to send 2, balanced by node 0, which
    // would take 2 more: `−(2^63 − 1) − 2 = −2^63 − 1`, below `i64::MIN`, so node 0 is named. Nothing changes.
    let mut network = loaded(&MincostNetwork::from_arcs(&[-i64::MAX, i64::MAX, 0], &[]));
    let whole = format!("{network:?}");
    let below = ChangeError::SupplyBeyondRange { node: 0, supply: i128::from(i64::MIN) - 1 };
    assert_eq!(network.set_supply(2, 2, 0), Err(below));
    assert_eq!(format!("{network:?}"), whole);
}

#[test]
fn test_supplies_set_to_either_end_of_i64_and_the_largest_difference_by_hand() {
    // Three nodes and no arc, every supply zero. Node 0 is set to `i64::MAX`, balanced by node 1, which falls to
    // `−i64::MAX`; nothing is sent, so `2^63 − 1` is left to be sent. Node 1 is then set to `i64::MIN`, one lower,
    // balanced by node 2, which rises to 1: `2^63` left to be sent, within `2^64 − 2`.
    let mut network = loaded(&MincostNetwork::from_arcs(&[0, 0, 0], &[]));
    assert_eq!(network.set_supply(0, i64::MAX, 1), Ok(0));
    assert_eq!(network.set_supply(1, i64::MIN, 2), Ok(-i64::MAX));
    assert_eq!((network.supply(0), network.supply(1), network.supply(2)), (i64::MAX, i64::MIN, 1));

    // The largest difference: node 0 from `i64::MAX` to `i64::MIN`, balanced by node 1 from `i64::MIN` to `i64::MAX`.
    // Both supplies stay in range, but the `2^64 − 1` moved, with the `2^63` already left to be sent, pass the limit.
    let whole = format!("{network:?}");
    let total = u128::from(u64::MAX) + (1 << 63);
    assert_eq!(network.set_supply(0, i64::MIN, 1), Err(ChangeError::ExcessBeyondRange { total }));
    assert_eq!(format!("{network:?}"), whole);
}

#[test]
fn test_a_supply_whose_balancing_node_would_leave_its_range_is_refused_by_hand() {
    // The customer is to take `2^63`: the depot would send `2^63`, past `i64::MAX`, and is named. Nothing changes.
    let mut network = solved_depot();
    let optimum = network.optimum().map(Solution::from);
    let past = ChangeError::SupplyBeyondRange { node: 0, supply: i128::from(i64::MAX) + 1 };
    assert_eq!(network.set_supply(2, i64::MIN, 0), Err(past));
    assert_eq!((network.supplies(), network.optimum().map(Solution::from)), (&[4, 0, -4][..], optimum));
}

#[test]
fn test_a_supply_past_the_flow_the_root_links_admit_is_refused_by_hand() {
    // Four nodes and no arc, `2^64 − 2` stranded up the root links: one unit more taken by node 3 from node 2 is
    // refused, as the move of it is.
    let mut network = loaded(&MincostNetwork::from_arcs(&[0, 0, 0, 0], &[]));
    let most = i64::MAX.cast_unsigned();
    assert_eq!((network.move_supply(2, 0, most), network.move_supply(3, 1, most)), (Ok(()), Ok(())));
    let limit = u128::from(u64::MAX - 1);
    assert_eq!(network.set_supply(3, i64::MIN, 2), Err(ChangeError::ExcessBeyondRange { total: limit + 1 }));
    assert_eq!(network.supplies(), [i64::MAX, i64::MAX, -i64::MAX, -i64::MAX]);
}

#[test]
fn test_a_node_that_balances_itself_changes_nothing_where_its_supply_stays_by_hand() {
    let mut network = solved_depot();
    assert_eq!(network.set_supply(1, 0, 1), Ok(0));
    assert_eq!(network.supplies(), [4, 0, -4]);
}

#[test]
#[should_panic(
    expected = "called `Network::set_supply` with node 0 as its own balancing node, whose supply 4 should then stay, \
                not become 5"
)]
fn test_a_node_that_balances_itself_and_is_to_change_panics() {
    let _ = solved_depot().set_supply(0, 5, 0);
}

#[test]
#[should_panic(
    expected = "called `Network::set_supply` with the nodes 3 and 0, which should be below the node count 3"
)]
fn test_a_supply_of_a_node_past_the_nodes_panics() {
    let _ = loaded(&depot()).set_supply(3, 1, 0);
}

#[test]
#[should_panic(
    expected = "called `Network::set_supply` with the nodes 0 and 4, which should be below the node count 3"
)]
fn test_a_supply_balanced_by_a_node_past_the_nodes_panics() {
    let _ = loaded(&depot()).set_supply(0, 1, 4);
}

#[test]
#[should_panic(
    expected = "called `Network::move_supply` with the nodes 0 and 3, which should be below the node count 3"
)]
fn test_a_move_to_a_node_past_the_nodes_panics() {
    let _ = loaded(&depot()).move_supply(0, 3, 1);
}

#[test]
#[should_panic(
    expected = "called `Network::move_supply` with the nodes 0 and 0, which should be below the node count 0"
)]
fn test_a_move_on_a_network_never_loaded_panics() {
    let _ = Network::new().move_supply(0, 0, 0);
}

/// `answer`'s optimum as its flows and its cost, `None` where it found no flow.
fn flows_and_cost(answer: &Answer<Solution>) -> Option<(Vec<u64>, i128)> {
    answer.outcome.clone().optimal().map(|optimum| (optimum.flows, optimum.total_cost))
}

#[test]
fn test_a_capacity_under_a_tree_arcs_flow_and_its_set_back_by_hand() {
    // The depot's optimum sends 2 units directly by arc 0; its capacity falls to 1: node 0 can send only 3 units in
    // all, and no flow exists, as a solver finds too. Set back, the depot's optimum again.
    let mut held = depot();
    let mut network = loaded(&held);
    let _ = network.solve();

    assert_eq!(network.set_capacity(0, 1), Ok(3));
    held.capacities[0] = 1;
    assert_eq!(network.optimum(), None, "a change ends the optimum");
    assert_eq!(network.solve(), Answer { outcome: Outcome::Infeasible, work: Work::default() });
    assert_eq!(NetworkSimplex::new().solve(&problem_of(&held)).map(|answer| answer.outcome), Ok(Outcome::Infeasible));

    assert_eq!(network.set_capacity(0, 3), Ok(1));
    held.capacities[0] = 3;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(flows_and_cost(&answer), Some((vec![2, 2, 2], 14)));
}

#[test]
fn test_a_capacity_under_a_tree_arc_running_up_by_hand() {
    // The depot turned around: node 1 sends its two units on to node 0 by arc 1, up from it in the solver's tree. Its
    // capacity falls to 1: 3 units directly and one round by arcs 2 and 1, `15 + 1 + 1 = 17`.
    let mut held = depot_turned_around();
    let mut network = loaded(&held);
    let _ = network.solve();

    assert_eq!(network.set_capacity(1, 1), Ok(4));
    held.capacities[1] = 1;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(flows_and_cost(&answer), Some((vec![3, 1, 1], 17)));
}

#[test]
fn test_capacities_over_an_empty_arc_and_under_a_full_one_move_nothing_or_what_they_must_by_hand() {
    // The depot with a dear arc 3, empty at its optimum: its capacity raised to 7, nothing moves and nothing pivots.
    let mut held = depot_with_a_dear_arc();
    let mut network = loaded(&held);
    let _ = network.solve();
    assert_eq!(network.set_capacity(3, 7), Ok(5));
    held.capacities[3] = 7;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!((flows_and_cost(&answer), answer.work), (Some((vec![2, 2, 2, 0], 14)), Work::default()));

    // The depot's arc 2, full at its capacity 2, lowered to 1: one unit less through node 1, one more directly,
    // `15 + 1 + 1 = 17`; raised to 3, one more through node 1, one less directly, `5 + 3 + 3 = 11`. Neither pivots.
    for (capacity, flows, cost) in [(1, vec![3, 1, 1], 17), (3, vec![1, 3, 3], 11)] {
        let mut held = depot();
        let mut network = loaded(&held);
        let _ = network.solve();
        assert_eq!(network.set_capacity(2, capacity), Ok(2));
        held.capacities[2] = capacity;
        let answer = solved_as_a_solver_solves(&mut network, &held);
        assert_eq!((flows_and_cost(&answer), answer.work), (Some((flows, cost)), Work::default()));
    }
}

#[test]
fn test_an_arc_switched_off_and_on_again_by_hand() {
    // The depot with a dear arc 3. Arc 1, into node 1, switched off: 3 units directly and one by the dear lane,
    // `15 + 9 = 24`. On again with the capacity it answered: the depot's optimum, 14.
    let mut held = depot_with_a_dear_arc();
    let mut network = loaded(&held);
    let _ = network.solve();

    let capacity = network.set_capacity(1, 0).expect("a capacity of zero should switch an arc off");
    assert_eq!(capacity, 4);
    held.capacities[1] = 0;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(flows_and_cost(&answer), Some((vec![3, 0, 0, 1], 24)));

    assert_eq!(network.set_capacity(1, capacity), Ok(0));
    held.capacities[1] = capacity;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(flows_and_cost(&answer), Some((vec![2, 2, 2, 0], 14)));
}

#[test]
fn test_lower_bounds_under_a_full_arc_over_an_empty_one_and_above_a_tree_arcs_flow_by_hand() {
    // The depot's arc 2, full at 2, takes the lower bound 1: nothing moves, nothing pivots, the cost stays 14.
    let mut held = depot();
    let mut network = loaded(&held);
    let _ = network.solve();
    assert_eq!(network.set_lower_bound(2, 1), Ok(0));
    held.lower_bounds[2] = 1;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!((flows_and_cost(&answer), answer.work), (Some((vec![2, 2, 2], 14)), Work::default()));

    // The depot's arc 0, carrying 2 of 3, takes the lower bound 3: 3 directly, one through node 1, 17.
    let mut held = depot();
    let mut network = loaded(&held);
    let _ = network.solve();
    assert_eq!(network.set_lower_bound(0, 3), Ok(0));
    held.lower_bounds[0] = 3;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(flows_and_cost(&answer), Some((vec![3, 1, 1], 17)));

    // The dear arc 3, empty, takes the lower bound 2: two units by it at 9, two through node 1, `18 + 4 = 22`.
    let mut held = depot_with_a_dear_arc();
    let mut network = loaded(&held);
    let _ = network.solve();
    assert_eq!(network.set_lower_bound(3, 2), Ok(0));
    held.lower_bounds[3] = 2;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(flows_and_cost(&answer), Some((vec![0, 2, 2, 2], 22)));
}

#[test]
fn test_bounds_on_an_arc_of_no_width_from_either_side_by_hand() {
    // Arc 3 carries one unit from node 0 to node 1, fixed, at 4: `2·5 + 1 + 2 + 4 = 17`. Its lower bound falls to 0:
    // arc 1 carries that unit instead at 1, 14. Its capacity rises to 3 instead: nothing more by it at 4, 17 again,
    // and no pivot.
    let mut fixed = depot();
    fixed.add_arc(0, 1, 1, 1, 4);
    let mut network = loaded(&fixed);
    let solved = solved_as_a_solver_solves(&mut network, &fixed);
    assert_eq!(flows_and_cost(&solved), Some((vec![2, 1, 2, 1], 17)));
    let copy = network.clone();

    let mut held = fixed.clone();
    assert_eq!(network.set_lower_bound(3, 0), Ok(1));
    held.lower_bounds[3] = 0;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(flows_and_cost(&answer), Some((vec![2, 2, 2, 0], 14)));

    let (mut network, mut held) = (copy, fixed);
    assert_eq!(network.set_capacity(3, 3), Ok(1));
    held.capacities[3] = 3;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!((flows_and_cost(&answer), answer.work), (Some((vec![2, 1, 2, 1], 17)), Work::default()));
}

#[test]
fn test_bounds_that_cross_in_one_call_and_are_refused_in_the_wrong_order_by_hand() {
    // The depot's arc 2 from `0..=2` to `3..=4`: the lower bound first is refused, past the capacity held, and changes
    // nothing. Both at once: every unit through node 1, `4 + 4 = 8`. Back, the capacity first is refused, below the
    // lower bound held; both at once give the depot's optimum again.
    let mut held = depot();
    let mut network = loaded(&held);
    let solved = owned(&mut network);

    let refused = ChangeError::LowerAboveCapacity { arc: 2, lower_bound: 3, capacity: 2 };
    let whole = format!("{network:?}");
    assert_eq!(network.set_lower_bound(2, 3), Err(refused));
    assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");
    assert_eq!(network.optimum().map(Solution::from), solved.outcome.clone().optimal());

    let crossed = Bounds { lower_bound: 3, capacity: 4 };
    let had = network.set_bounds(2, crossed).expect("the pair should be in order");
    assert_eq!(had, Bounds { lower_bound: 0, capacity: 2 });
    (held.lower_bounds[2], held.capacities[2]) = (3, 4);
    let answer = solved_as_a_solver_solves(&mut network, &held);
    assert_eq!(flows_and_cost(&answer), Some((vec![0, 4, 4], 8)));

    let refused = ChangeError::LowerAboveCapacity { arc: 2, lower_bound: 3, capacity: 2 };
    let whole = format!("{network:?}");
    assert_eq!(network.set_capacity(2, 2), Err(refused));
    assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");
    assert_eq!(network.set_bounds(2, had), Ok(crossed));
    let answer = solved_as_a_solver_solves(&mut network, &depot());
    assert_eq!(flows_and_cost(&answer), Some((vec![2, 2, 2], 14)));
}

#[test]
fn test_bounds_set_to_the_bounds_held_keep_the_optimum() {
    let held = depot();
    let mut network = loaded(&held);
    let solved = owned(&mut network);

    assert_eq!((network.set_capacity(0, 3), network.set_lower_bound(1, 0)), (Ok(3), Ok(0)));
    let bounds = Bounds { lower_bound: 0, capacity: 2 };
    assert_eq!(network.set_bounds(2, bounds), Ok(bounds));
    assert_eq!(network.optimum().map(Solution::from), solved.outcome.clone().optimal());
    assert_eq!(owned(&mut network), Answer { work: Work::default(), ..solved });
}

#[test]
fn test_refused_bounds_change_nothing_by_hand() {
    // A lower bound above the capacity, by each setter; refused, the optimum held stays.
    let mut network = loaded(&depot());
    let solved = owned(&mut network);
    let lower_above = |lower_bound, capacity| ChangeError::LowerAboveCapacity { arc: 0, lower_bound, capacity };
    let whole = format!("{network:?}");
    assert_eq!(network.set_lower_bound(0, 4), Err(lower_above(4, 3)));
    assert_eq!(network.set_bounds(0, Bounds { lower_bound: 2, capacity: 1 }), Err(lower_above(2, 1)));
    assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");
    assert_eq!(network.optimum().map(Solution::from), solved.outcome.clone().optimal());

    // Four loops at one node whose costs times capacities add up to `i128::MAX` exactly: a capacity that raises a term
    // is refused, one that lowers it is not.
    let (bound, wide) = (cost_bound(1), u64::MAX);
    let arcs = [(0, 0, 0, wide, bound), (0, 0, 0, wide, -bound), (0, 0, 0, wide, 2), (0, 0, 0, (1 << 63) - 1, 1)];
    let mut held = MincostNetwork::from_arcs(&[0], &arcs);
    let mut network = loaded(&held);
    let whole = format!("{network:?}");
    assert_eq!(network.set_capacity(3, 1 << 63), Err(ChangeError::CostTotalBeyondRange { arc: 3 }));
    assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");
    assert_eq!(network.set_capacity(2, wide - 1), Ok(wide));
    held.capacities[2] = wide - 1;
    let answer = solved_as_a_solver_solves(&mut network, &held);
    // The loop of cost `−B` fills; the others stay empty.
    let expected = -(i128::from(bound) * i128::from(wide));
    assert_eq!(answer.outcome.optimal().map(|optimum| optimum.total_cost), Some(expected));

    // Two nodes and an empty arc of capacity `2^64 − 1`: a lower bound of `2^64 − 1` would move more than the root
    // links admit, and is refused; the optimum held stays.
    let mut network = loaded(&MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, u64::MAX, 1)]));
    let solved = owned(&mut network);
    let (total, whole) = (u128::from(u64::MAX), format!("{network:?}"));
    assert_eq!(network.set_lower_bound(0, u64::MAX), Err(ChangeError::ExcessBeyondRange { total }));
    assert_eq!(format!("{network:?}"), whole, "a refusal changes nothing at all");
    assert_eq!(network.optimum().map(Solution::from), solved.outcome.clone().optimal());
}

#[test]
#[should_panic(expected = "called `Network::set_capacity` with arc 3, which should be below the arc count 3")]
fn test_a_capacity_on_an_arc_past_the_arcs_panics() {
    let _ = loaded(&depot()).set_capacity(3, 1);
}

#[test]
#[should_panic(expected = "called `Network::set_lower_bound` with arc 3, which should be below the arc count 3")]
fn test_a_lower_bound_on_an_arc_past_the_arcs_panics() {
    let _ = loaded(&depot()).set_lower_bound(3, 1);
}

#[test]
#[should_panic(expected = "called `Network::set_bounds` with arc 0, which should be below the arc count 0")]
fn test_bounds_on_a_network_never_loaded_panic() {
    let _ = Network::new().set_bounds(0, Bounds { lower_bound: 0, capacity: 1 });
}

#[test]
fn test_every_table_reads_back_as_set_by_hand() {
    // The depot with a lower bound of 1 on arc 1; then a cost, a capacity, a lower bound and a move of supply set.
    let mut network =
        loaded(&MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 1, 4, 1), (1, 2, 0, 2, 1)]));
    assert_eq!(network.tails().iter().collect::<Vec<_>>(), [0, 0, 1]);
    assert_eq!(network.heads().iter().collect::<Vec<_>>(), [2, 1, 2]);
    assert_eq!(network.lower_bounds().iter().collect::<Vec<_>>(), [0, 1, 0]);
    assert_eq!(network.capacities().iter().collect::<Vec<_>>(), [3, 4, 2]);
    assert_eq!([0, 1, 2].map(|arc| (network.tail(arc), network.head(arc))), [(0, 2), (0, 1), (1, 2)]);

    assert_eq!(network.set_cost(2, 7), Ok(1));
    assert_eq!(network.set_capacity(0, 6), Ok(3));
    assert_eq!(network.set_lower_bound(2, 2), Ok(0));
    assert_eq!(network.move_supply(2, 1, 1), Ok(()));

    assert_eq!([0, 1, 2].map(|arc| network.cost(arc)), [5, 1, 7]);
    assert_eq!([0, 1, 2].map(|arc| network.lower_bound(arc)), [0, 1, 2]);
    assert_eq!([0, 1, 2].map(|arc| network.capacity(arc)), [6, 4, 2]);
    assert_eq!(network.bounds(2), Bounds { lower_bound: 2, capacity: 2 });
    assert_eq!([0, 1, 2].map(|node| network.supply(node)), [4, 1, -5]);
    assert_eq!(network.costs().iter().collect::<Vec<_>>(), [5, 1, 7]);
    assert_eq!(network.lower_bounds().iter().rev().collect::<Vec<_>>(), [2, 1, 0]);
    assert_eq!((&network.capacities()).into_iter().collect::<Vec<_>>(), [6, 4, 2]);
    assert_eq!(network.supplies(), [4, 1, -5]);

    // A refused change leaves every read as it was.
    let refused = ChangeError::LowerAboveCapacity { arc: 0, lower_bound: 7, capacity: 6 };
    assert_eq!(network.set_lower_bound(0, 7), Err(refused));
    assert_eq!((network.lower_bound(0), network.capacity(0)), (0, 6));
}

#[test]
fn test_the_bound_columns_name_their_bound_and_compare_by_their_numbers_by_hand() {
    let network = loaded(&MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 1, 1, 1), (0, 1, 0, 2, 1)]));
    let (lower_bounds, capacities) = (network.lower_bounds(), network.capacities());
    assert_eq!(format!("{lower_bounds:?}"), "FlowBounds lower_bounds [1, 0]");
    assert_eq!(format!("{capacities:?}"), "FlowBounds capacities [1, 2]");
    assert_eq!((lower_bounds.get(1), capacities.get(1), capacities.get(2)), (Some(0), Some(2), None));
    let mut walk = capacities.iter();
    assert_eq!((walk.len(), walk.next_back(), walk.next(), walk.next()), (2, Some(2), Some(1), None));
    assert_ne!(lower_bounds, capacities);
    assert_eq!(Network::new().capacities(), Network::new().lower_bounds(), "two views of no arc");
}

#[test]
#[should_panic(expected = "called `Network::tail` with arc 3, which should be below the arc count 3")]
fn test_a_tail_past_the_arcs_panics() {
    let _ = loaded(&depot()).tail(3);
}

#[test]
#[should_panic(expected = "called `Network::head` with arc 3, which should be below the arc count 3")]
fn test_a_head_past_the_arcs_panics() {
    let _ = loaded(&depot()).head(3);
}

#[test]
#[should_panic(expected = "called `Network::cost` with arc 0, which should be below the arc count 0")]
fn test_a_cost_read_on_a_network_never_loaded_panics() {
    let _ = Network::new().cost(0);
}

#[test]
#[should_panic(
    expected = "called `Network::lower_bound` with arc 18446744073709551615, which should be below the arc \
                           count 3"
)]
fn test_a_lower_bound_past_the_arcs_panics() {
    let _ = loaded(&depot()).lower_bound(usize::MAX);
}

#[test]
#[should_panic(expected = "called `Network::bounds` with arc 3, which should be below the arc count 3")]
fn test_the_bounds_of_an_arc_past_the_arcs_panic() {
    let _ = loaded(&depot()).bounds(3);
}

#[test]
fn test_the_bounds_read_are_those_set_back_by_hand() {
    // Arc 0 of the depot, `0..=3`: its capacity raised, then the bounds read before set back at once.
    let mut network = loaded(&depot());
    let read = network.bounds(0);
    assert_eq!(read, Bounds { lower_bound: 0, capacity: 3 });
    assert_eq!(network.set_capacity(0, 7), Ok(3));
    assert_eq!(network.bounds(0), Bounds { lower_bound: 0, capacity: 7 });
    assert_eq!(network.set_bounds(0, read), Ok(Bounds { lower_bound: 0, capacity: 7 }));
    assert_eq!(network.bounds(0), read);
}

#[test]
#[should_panic(expected = "called `Network::capacity` with arc 4, which should be below the arc count 3")]
fn test_a_capacity_past_the_arcs_panics() {
    let _ = loaded(&depot()).capacity(4);
}

#[test]
#[should_panic(expected = "called `Network::supply` with node 3, which should be below the node count 3")]
fn test_a_supply_past_the_nodes_panics() {
    let _ = loaded(&depot()).supply(3);
}

/// One call of a random sequence on a network. Plain data.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// The cost drawn set on the arc drawn.
    SetCost(Index, CostDraw),
    /// The amount drawn moved from the supply of the node drawn to that of the node drawn.
    MoveSupply(Index, Index, u64),
    /// The supply drawn set on the node drawn, balanced by the node drawn: where the two are one node, the supply it
    /// holds.
    SetSupply(Index, i64, Index),
    /// The capacity drawn set on the arc drawn.
    SetCapacity(Index, u64),
    /// The lower bound drawn set on the arc drawn.
    SetLowerBound(Index, u64),
    /// The lower bound and the capacity drawn set on the arc drawn at once.
    SetBounds(Index, u64, u64),
    /// A load of the problem held, its costs and supplies moved by the testing crate's perturbation from this seed.
    Load(u64),
    /// A solve.
    Solve,
}

/// Sequences of one to sixteen calls: to every two solves two costs set, two moves of zero to three units, a supply of
/// −4 to 4 set, a capacity, a lower bound and a pair of bounds of zero to three units set, and a load of the problem
/// held with its costs and supplies moved.
fn calls() -> impl Strategy<Value = Vec<Call>> {
    let cost = (any::<Index>(), cost_draws(-9..=9)).prop_map(|(arc, draw)| Call::SetCost(arc, draw));
    let supply = (any::<Index>(), any::<Index>(), 0..=3_u64)
        .prop_map(|(from_node, to_node, amount)| Call::MoveSupply(from_node, to_node, amount));
    let capacity = (any::<Index>(), 0..=3_u64).prop_map(|(arc, capacity)| Call::SetCapacity(arc, capacity));
    let lower_bound = (any::<Index>(), 0..=3_u64).prop_map(|(arc, lower_bound)| Call::SetLowerBound(arc, lower_bound));
    let bounds = (any::<Index>(), 0..=3_u64, 0..=3_u64)
        .prop_map(|(arc, lower_bound, capacity)| Call::SetBounds(arc, lower_bound, capacity));
    let call = prop_oneof![
        2 => cost,
        2 => supply,
        1 => (any::<Index>(), -4..=4_i64, any::<Index>())
            .prop_map(|(node, supply, balancing_node)| Call::SetSupply(node, supply, balancing_node)),
        1 => capacity,
        1 => lower_bound,
        1 => bounds,
        1 => any::<u64>().prop_map(Call::Load),
        2 => Just(Call::Solve),
    ];

    proptest::collection::vec(call, 1..=16)
}

/// A change a random sequence made, which it sets back at its end. Plain data.
#[derive(Clone, Copy, Debug)]
enum Change {
    /// The arc whose cost was set, and the cost it had.
    Cost(usize, i64),
    /// The nodes and the amount of a move of supply.
    Supply(usize, usize, u64),
    /// The node whose supply was set, the supply it had, and the node that balanced it.
    SetSupply(usize, i64, usize),
    /// The arc whose bounds were set, and the bounds it had.
    Bounds(usize, Bounds),
}

/// What a random sequence of calls reached, counted. Plain data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Reached {
    /// Costs set that changed a cost.
    changed: usize,
    /// Costs set to the cost held.
    unchanged: usize,
    /// Costs refused.
    refused: usize,
    /// Solves that found an optimum.
    optimal: usize,
    /// Solves that found no flow.
    infeasible: usize,
    /// Solves that pivoted.
    pivoted: usize,
    /// Moves of supply that moved some.
    moved: usize,
    /// Moves of supply of nothing.
    moved_nothing: usize,
    /// Supplies set above the supply held.
    supply_rose: usize,
    /// Supplies set below the supply held.
    supply_fell: usize,
    /// Supplies set to the supply held, balanced by another node.
    supply_unchanged: usize,
    /// Supplies set on a node that balances itself, to the supply it holds.
    supply_self_balanced: usize,
    /// Bounds set that changed a bound.
    bounds_changed: usize,
    /// Bounds set to the bounds held.
    bounds_unchanged: usize,
    /// Bounds refused, a lower bound above the capacity.
    bounds_refused: usize,
    /// Arcs switched off: a capacity of zero set over a lower bound of zero.
    switched_off: usize,
    /// Loads admitted.
    loads: usize,
    /// Loads refused.
    refused_loads: usize,
    /// Solves checked against the brute force too.
    brute_forced: usize,
    /// Solves of a problem of more flows than the brute force admits, checked against a solver's solve alone.
    past_brute_force: usize,
}

/// Runs `calls` on `held` loaded and solved, the test keeping its own copy of the problem's numbers. A cost set answers
/// the cost held and ends the optimum, or, set to the cost held or refused, changes nothing; a move of supply ends the
/// optimum, or, of nothing, changes nothing; a supply set answers the supply held and ends the optimum, or, set to the
/// supply held, changes nothing; bounds set answer the bounds held and end the optimum, or, set to the
/// bounds held or refused for a lower bound above the capacity, change nothing; a load is refused as a network never
/// loaded refuses it, changing nothing at all, or ends the optimum, and the copy becomes the problem loaded, whose
/// changes are set back from then on; every solve answers what a solver's solve of the copy answers, checked against
/// the brute force too where it admits the copy. Then every change since the last load is set back, last first, each
/// admitted, and a solve answers the outcome and cost of the problem first held or last loaded. Answers what the run
/// reached.
fn check_calls(mut held: MincostNetwork, calls: &[Call]) -> Result<Reached, TestCaseError> {
    let mut network = loaded(&held);
    let mut first = cost_of(&owned(&mut network));
    let (mut reached, mut replaced) = (Reached::default(), Vec::new());

    for &call in calls {
        reads_as(&network, &held)?;
        let (node_count, arc_count) = (held.node_count(), held.arc_count());
        let bound = cost_bound(node_count);
        match call {
            Call::SetCost(arc, draw) if arc_count > 0 => {
                let (arc, cost) = (arc.index(arc_count), draw.cost(bound));
                let optimum = network.optimum().map(Solution::from);
                let answered = network.set_cost(arc, cost);
                if !(-bound..=bound).contains(&cost) {
                    prop_assert_eq!(answered, Err(ChangeError::CostBeyondBound { arc, cost, node_count }));
                    prop_assert_eq!(network.optimum().map(Solution::from), optimum);
                    reached.refused += 1;
                } else if cost == held.costs[arc] {
                    prop_assert_eq!(answered, Ok(cost));
                    prop_assert_eq!(network.optimum().map(Solution::from), optimum);
                    reached.unchanged += 1;
                } else {
                    prop_assert_eq!(answered, Ok(held.costs[arc]));
                    prop_assert_eq!(network.optimum(), None);
                    replaced.push(Change::Cost(arc, held.costs[arc]));
                    held.costs[arc] = cost;
                    reached.changed += 1;
                }
            }
            Call::SetCost(..) => {}
            Call::MoveSupply(from_node, to_node, amount) => {
                let (from_node, to_node) = (from_node.index(node_count), to_node.index(node_count));
                let optimum = network.optimum().map(Solution::from);
                prop_assert_eq!(network.move_supply(from_node, to_node, amount), Ok(()));
                if from_node == to_node || amount == 0 {
                    prop_assert_eq!(network.optimum().map(Solution::from), optimum);
                    reached.moved_nothing += 1;
                } else {
                    prop_assert_eq!(network.optimum(), None);
                    let moved = i64::try_from(amount).map_err(|error| TestCaseError::fail(error.to_string()))?;
                    held.supplies[from_node] -= moved;
                    held.supplies[to_node] += moved;
                    replaced.push(Change::Supply(from_node, to_node, amount));
                    reached.moved += 1;
                }
            }
            Call::SetSupply(node, supply, balancing_node) => {
                let (node, balancing_node) = (node.index(node_count), balancing_node.index(node_count));
                let replaced_supply = held.supplies[node];
                let supply = if node == balancing_node { replaced_supply } else { supply };
                let optimum = network.optimum().map(Solution::from);
                prop_assert_eq!(network.set_supply(node, supply, balancing_node), Ok(replaced_supply));
                if node == balancing_node {
                    prop_assert_eq!(network.optimum().map(Solution::from), optimum);
                    reached.supply_self_balanced += 1;
                } else if supply == replaced_supply {
                    prop_assert_eq!(network.optimum().map(Solution::from), optimum);
                    reached.supply_unchanged += 1;
                } else {
                    prop_assert_eq!(network.optimum(), None);
                    held.supplies[balancing_node] -= supply - replaced_supply;
                    held.supplies[node] = supply;
                    replaced.push(Change::SetSupply(node, replaced_supply, balancing_node));
                    if supply > replaced_supply {
                        reached.supply_rose += 1;
                    } else {
                        reached.supply_fell += 1;
                    }
                }
            }
            Call::SetCapacity(arc, capacity) if arc_count > 0 => {
                let arc = arc.index(arc_count);
                let settled = bounds_checked(&mut network, &mut held, arc, Setter::Capacity(capacity))?;
                settle(settled, arc, &mut reached, &mut replaced);
            }
            Call::SetLowerBound(arc, lower_bound) if arc_count > 0 => {
                let arc = arc.index(arc_count);
                let settled = bounds_checked(&mut network, &mut held, arc, Setter::LowerBound(lower_bound))?;
                settle(settled, arc, &mut reached, &mut replaced);
            }
            Call::SetBounds(arc, lower_bound, capacity) if arc_count > 0 => {
                let arc = arc.index(arc_count);
                let bounds = Bounds { lower_bound, capacity };
                let settled = bounds_checked(&mut network, &mut held, arc, Setter::Both(bounds))?;
                settle(settled, arc, &mut reached, &mut replaced);
            }
            Call::SetCapacity(..) | Call::SetLowerBound(..) | Call::SetBounds(..) => {}
            Call::Load(seed) => {
                let numbers = (0..).map(|step| seed.wrapping_add(step)).take(perturbation_draws(arc_count));
                // The capacities stay those held, so that the brute force enumerates no more flows than before.
                let next = MincostNetwork {
                    capacities: held.capacities.clone(),
                    ..perturbed(&held, &numbers.collect::<Vec<_>>())
                };
                let refusal = Network::new().load(&problem_of(&next)).err();
                let whole = format!("{network:?}");
                let answered = network.load(&problem_of(&next));
                if let Some(refusal) = refusal {
                    prop_assert_eq!((answered, format!("{network:?}")), (Err(refusal), whole));
                    reached.refused_loads += 1;
                } else {
                    prop_assert_eq!((answered, network.optimum()), (Ok(()), None));
                    first = NetworkSimplex::new()
                        .solve(&problem_of(&next))
                        .map_err(|error| TestCaseError::fail(error.to_string()))?
                        .outcome
                        .optimal()
                        .map(|solution| solution.total_cost);
                    (held, replaced) = (next, Vec::new());
                    reached.loads += 1;
                }
            }
            Call::Solve => {
                let answer = solved_as_the_brute_force_solves(&mut network, &held, &mut reached)?;
                reached.optimal += usize::from(matches!(answer.outcome, Outcome::Optimal(_)));
                reached.infeasible += usize::from(answer.outcome == Outcome::Infeasible);
                reached.pivoted += usize::from(answer.work.pivots > 0);
            }
        }
    }

    for &change in replaced.iter().rev() {
        match change {
            Change::Cost(arc, cost) => {
                prop_assert_eq!(network.set_cost(arc, cost), Ok(held.costs[arc]));
                held.costs[arc] = cost;
            }
            Change::SetSupply(node, supply, balancing_node) => {
                prop_assert_eq!(network.set_supply(node, supply, balancing_node), Ok(held.supplies[node]));
                held.supplies[balancing_node] += held.supplies[node] - supply;
                held.supplies[node] = supply;
            }
            Change::Supply(from_node, to_node, amount) => {
                prop_assert_eq!(network.move_supply(to_node, from_node, amount), Ok(()));
                let moved = i64::try_from(amount).map_err(|error| TestCaseError::fail(error.to_string()))?;
                held.supplies[to_node] -= moved;
                held.supplies[from_node] += moved;
            }
            Change::Bounds(arc, bounds) => {
                prop_assert_eq!(network.set_bounds(arc, bounds), Ok(bounds_of(&held, arc)));
                (held.lower_bounds[arc], held.capacities[arc]) = (bounds.lower_bound, bounds.capacity);
            }
        }
    }
    reads_as(&network, &held)?;
    let again = solved_as_the_brute_force_solves(&mut network, &held, &mut reached)?;
    prop_assert_eq!(cost_of(&again), first);

    Ok(reached)
}

/// Checks that `network` reads back `held`, the problem the test holds, through each column and each single read: every
/// table of a `Problem`, arc by arc and node by node, and nothing past the counts.
fn reads_as(network: &Network, held: &MincostNetwork) -> Result<(), TestCaseError> {
    let arc_count = held.arc_count();
    prop_assert_eq!(network.tails().iter().collect::<Vec<_>>(), held.tails.clone());
    prop_assert_eq!(network.heads().iter().collect::<Vec<_>>(), held.heads.clone());
    prop_assert_eq!(network.costs().iter().collect::<Vec<_>>(), held.costs.clone());
    prop_assert_eq!(network.lower_bounds().iter().collect::<Vec<_>>(), held.lower_bounds.clone());
    prop_assert_eq!(network.capacities().iter().collect::<Vec<_>>(), held.capacities.clone());
    prop_assert_eq!(network.supplies(), held.supplies.as_slice());
    prop_assert_eq!((network.lower_bounds().len(), network.capacities().get(arc_count)), (arc_count, None));

    for arc in 0..arc_count {
        let read = (network.tail(arc), network.head(arc), network.cost(arc));
        prop_assert_eq!(read, (held.tails[arc], held.heads[arc], held.costs[arc]));
        prop_assert_eq!(
            (network.lower_bound(arc), network.capacity(arc)),
            (held.lower_bounds[arc], held.capacities[arc])
        );
        prop_assert_eq!(network.bounds(arc), bounds_of(held, arc));
    }
    for (node, &supply) in held.supplies.iter().enumerate() {
        prop_assert_eq!(network.supply(node), supply);
    }

    Ok(())
}

/// The bounds of `arc` in `held`.
fn bounds_of(held: &MincostNetwork, arc: usize) -> Bounds {
    Bounds { lower_bound: held.lower_bounds[arc], capacity: held.capacities[arc] }
}

/// Which setter of bounds a random call makes, with the number or the pair it sets. Plain data.
#[derive(Clone, Copy, Debug)]
enum Setter {
    /// `set_capacity` with this capacity.
    Capacity(u64),
    /// `set_lower_bound` with this lower bound.
    LowerBound(u64),
    /// `set_bounds` with these bounds.
    Both(Bounds),
}

/// How a setter of bounds settled. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Settled {
    /// The bounds changed from these, and to a capacity of zero over a lower bound of zero where `switched_off`.
    Changed {
        /// The bounds the arc had.
        replaced: Bounds,
        /// Whether the change switched the arc off.
        switched_off: bool,
    },
    /// The bounds set were those held.
    Unchanged,
    /// A lower bound above the capacity, refused.
    Refused,
}

/// Makes the change `setter` names on `arc` of `network`, which holds `held`, and checks the answer: bounds the arc
/// has, or a lower bound above the capacity, which is refused, change nothing and keep the optimum; any other bounds
/// answer the number or the pair held and end the optimum, and `held` takes them. Answers how it settled.
fn bounds_checked(
    network: &mut Network,
    held: &mut MincostNetwork,
    arc: usize,
    setter: Setter,
) -> Result<Settled, TestCaseError> {
    let (optimum, replaced) = (network.optimum().map(Solution::from), bounds_of(held, arc));
    let (answered, bounds) = match setter {
        Setter::Capacity(capacity) => {
            let answered = network.set_capacity(arc, capacity).map(|capacity| Bounds { capacity, ..replaced });
            (answered, Bounds { capacity, ..replaced })
        }
        Setter::LowerBound(lower_bound) => {
            let answered =
                network.set_lower_bound(arc, lower_bound).map(|lower_bound| Bounds { lower_bound, ..replaced });
            (answered, Bounds { lower_bound, ..replaced })
        }
        Setter::Both(bounds) => (network.set_bounds(arc, bounds), bounds),
    };

    let Bounds { lower_bound, capacity } = bounds;
    if bounds == replaced {
        prop_assert_eq!((answered, network.optimum().map(Solution::from)), (Ok(replaced), optimum));
        Ok(Settled::Unchanged)
    } else if lower_bound > capacity {
        let refused = ChangeError::LowerAboveCapacity { arc, lower_bound, capacity };
        prop_assert_eq!((answered, network.optimum().map(Solution::from)), (Err(refused), optimum));
        Ok(Settled::Refused)
    } else {
        prop_assert_eq!((answered, network.optimum()), (Ok(replaced), None));
        (held.lower_bounds[arc], held.capacities[arc]) = (lower_bound, capacity);
        Ok(Settled::Changed { replaced, switched_off: capacity == 0 })
    }
}

/// Counts how a setter of bounds on `arc` settled into `reached`, and keeps the bounds a change replaced in
/// `replaced`, to be set back.
fn settle(settled: Settled, arc: usize, reached: &mut Reached, replaced: &mut Vec<Change>) {
    match settled {
        Settled::Changed { replaced: bounds, switched_off } => {
            replaced.push(Change::Bounds(arc, bounds));
            reached.bounds_changed += 1;
            reached.switched_off += usize::from(switched_off);
        }
        Settled::Unchanged => reached.bounds_unchanged += 1,
        Settled::Refused => reached.bounds_refused += 1,
    }
}

/// Solves `network`, which holds `held`, and checks the answer against the brute force and a solver's solve of `held`:
/// the same outcome and cost, a valid certificate, and the cost answered that of the optimum lent. Where `held`
/// admits more flows than the brute force tries, it is checked against the solve alone; `reached` counts which.
///
/// A tiny network holds at most seven arcs, each of width at most 2 as drawn: `3^7` flows, the brute force's limit. A
/// setter draws bounds of up to 3, so it can widen an arc to 3, and seven arcs, one of width 3, admit `4·3^6` flows.
fn solved_as_the_brute_force_solves(
    network: &mut Network,
    held: &MincostNetwork,
    reached: &mut Reached,
) -> Result<Answer<Solution>, TestCaseError> {
    let answer = owned(network);
    let solved =
        NetworkSimplex::new().solve(&problem_of(held)).map_err(|error| TestCaseError::fail(error.to_string()))?;
    prop_assert_eq!(
        answer.outcome.clone().optimal().map(|solution| solution.total_cost),
        solved.outcome.optimal().map(|solution| solution.total_cost)
    );

    let flow = answer.outcome.clone().optimal().map(|solution| OptimalFlow {
        flows: solution.flows,
        potentials: solution.potentials,
        total_cost: solution.total_cost,
    });
    if brute_force_admits(held) {
        check_against_brute_force(held, flow.as_ref()).map_err(|breach| TestCaseError::fail(breach.to_string()))?;
        reached.brute_forced += 1;
    } else {
        reached.past_brute_force += 1;
    }

    Ok(answer)
}

proptest! {
    #[test]
    fn test_random_changes_and_solves_answer_as_a_solve_of_the_problem_as_it_stands(
        held in tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered),
        calls in calls(),
    ) {
        check_calls(held, &calls)?;
    }
}

#[test]
fn test_a_capacity_set_past_what_the_brute_force_admits_is_checked_against_a_solve() {
    // Seven loops of width 2 at one node: `3^7 = 2187` flows, the most the brute force admits. A capacity of 3 set on
    // one of them, any, takes the network to `4·3^6 = 2916`.
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let held = MincostNetwork::from_arcs(&[0], &[(0, 0, 0, 2, 1); 7]);
    let arc = any::<Index>().new_tree(&mut TestRunner::deterministic()).expect("an index should be drawn").current();
    let reached =
        check_calls(held, &[Call::SetCapacity(arc, 3), Call::Solve]).expect("the solve should answer as a solve does");

    // The solve past the limit, then the solve with the capacity set back, within it.
    assert_eq!((reached.bounds_changed, reached.past_brute_force, reached.brute_forced), (1, 1, 1));
}

#[test]
fn test_the_random_changes_and_solves_reach_every_kind_of_call() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let mut runner = TestRunner::deterministic();
    let strategy = (tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::Ordered), calls());
    let mut total = Reached::default();
    for _ in 0..256 {
        let (held, calls) = strategy.new_tree(&mut runner).expect("a sequence should be drawn").current();
        let reached = check_calls(held, &calls).expect("every call should answer as a solve does");
        total.changed += reached.changed;
        total.unchanged += reached.unchanged;
        total.refused += reached.refused;
        total.optimal += reached.optimal;
        total.infeasible += reached.infeasible;
        total.pivoted += reached.pivoted;
        total.moved += reached.moved;
        total.moved_nothing += reached.moved_nothing;
        total.supply_rose += reached.supply_rose;
        total.supply_fell += reached.supply_fell;
        total.supply_unchanged += reached.supply_unchanged;
        total.supply_self_balanced += reached.supply_self_balanced;
        total.bounds_changed += reached.bounds_changed;
        total.bounds_unchanged += reached.bounds_unchanged;
        total.bounds_refused += reached.bounds_refused;
        total.switched_off += reached.switched_off;
        total.loads += reached.loads;
        total.refused_loads += reached.refused_loads;
        total.brute_forced += reached.brute_forced;
        total.past_brute_force += reached.past_brute_force;
    }

    let counts = [
        total.changed,
        total.unchanged,
        total.refused,
        total.optimal,
        total.infeasible,
        total.pivoted,
        total.moved,
        total.moved_nothing,
        total.supply_rose,
        total.supply_fell,
        total.supply_unchanged,
        total.supply_self_balanced,
        total.bounds_changed,
        total.bounds_unchanged,
        total.bounds_refused,
        total.switched_off,
        total.loads,
        total.refused_loads,
        total.brute_forced,
    ];
    assert!(counts.iter().all(|&count| count > 0), "{total:?}");
    // Most solves are checked against the brute force: only a setter's widening passes its limit.
    assert!(total.past_brute_force < total.brute_forced / 10, "{total:?}");
}
