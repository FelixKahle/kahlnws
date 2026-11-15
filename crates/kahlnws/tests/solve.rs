// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The public surface of the solver: hand-worked problems with their flows, costs and potentials written out, and the
//! work of several pivot by pivot; every refusal with its values and message, and the order of refusals that meet;
//! the brute force on tiny networks; the linear-programming oracle on seeded ones; the ends of every range; that the
//! room a solver keeps never changes the answer of a solve, its work included; and that a resolve answers the outcome
//! and the cost a solve answers, from the basis the solve before it ended on where that serves and exactly as a solve
//! where it does not. Every optimal answer is checked by the certificate, and the work of every answer against the
//! relations its counts keep.

use std::panic::{AssertUnwindSafe, catch_unwind};

use kahlnws::{Answer, NetworkSimplex, Optimum, Outcome, Problem, ProblemError, Solution, Table, Work, cost_bound};
use kahlnws_testing::{
    Family, MincostNetwork, OptimalFlow, TinyBounds, TinyCosts, assignment_shaped, brute_force_optimum,
    check_against_brute_force, check_certificate, general, lp_optimum, perturbed, supply_shaped, tiny_networks,
};
use num_bigint::BigInt;
use proptest::prelude::*;

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

/// What a fresh solver answers for `network`, its lower bounds given, with an optimal answer checked by the
/// certificate and the work by [`check_work`].
///
/// # Panics
/// Panics if an optimal answer fails the certificate, or the work its relations.
fn solved(network: &MincostNetwork) -> Result<Answer<Solution>, ProblemError> {
    let answer = NetworkSimplex::new().solve(&problem_of(network))?;
    if let Outcome::Optimal(solution) = &answer.outcome {
        let certified = check_certificate(network, &solution.flows, &solution.potentials, solution.total_cost);
        assert_eq!(certified, Ok(()), "the answer {solution:?} should be certified optimal");
    }
    assert_eq!(check_work(network, &answer), Ok(()), "the work of {answer:?}");

    Ok(answer)
}

/// Checks the relations the work of `answer`, an answer for `network`, keeps: neither the degenerate pivots nor the
/// pivots of the first arcs are more than the pivots; and, where the answer is optimal, some pivot sent flow exactly
/// where some arc carries more than its lower bound.
///
/// # Errors
/// Returns the first relation that fails, with the work and the flows.
fn check_work(network: &MincostNetwork, answer: &Answer<Solution>) -> Result<(), String> {
    let Work { pivots, degenerate_pivots, first_arc_pivots } = answer.work;
    if degenerate_pivots > pivots || first_arc_pivots > pivots {
        return Err(format!("{:?} counts more of some kind of pivot than pivots", answer.work));
    }

    // The star carries every arc's lower bound, and only a pivot that sends flow changes an arc's flow: so an arc
    // above its lower bound needs one. Conversely, such a pivot lowers the cost of the flow above the lower bounds,
    // root links counted at their costs, by one at least, from the star's `T·M`, `T` the positive net supplies' total
    // and `M` the root cost; no pivot raises it. An optimal answer carries nothing on a root link, so were every arc
    // at its lower bound, that cost would end at zero, below `T·M`: `T` would be above zero, a net supply the arcs at
    // their lower bounds do not meet.
    let Outcome::Optimal(solution) = &answer.outcome else { return Ok(()) };
    let moved = solution.flows.iter().zip(&network.lower_bounds).any(|(flow, lower_bound)| flow > lower_bound);
    if moved != (pivots > degenerate_pivots) {
        return Err(format!("{:?} for the flows {:?}", answer.work, solution.flows));
    }

    Ok(())
}

/// The solution of `network`, which the test knows to be optimal, checked by the certificate.
///
/// # Panics
/// Panics if the problem is refused or infeasible, or the answer fails the certificate.
fn optimal(network: &MincostNetwork) -> Solution {
    match solved(network) {
        Ok(Answer { outcome: Outcome::Optimal(solution), .. }) => solution,
        other => panic!("the network should be solved optimally, not answered {other:?}"),
    }
}

/// What a fresh solver answers for `network`, without the work.
fn outcome_of(network: &MincostNetwork) -> Result<Outcome<Solution>, ProblemError> {
    solved(network).map(|answer| answer.outcome)
}

/// The solution of the flows `flows`, the potentials `potentials` and the cost `total_cost`, for a test to write out.
fn solution(flows: &[u64], potentials: &[i64], total_cost: i128) -> Solution {
    Solution { flows: flows.to_vec(), potentials: potentials.to_vec(), total_cost }
}

/// What `solve_into` answers for `problem` into a buffer of sevens, made owned as `solve` answers it, its work
/// included.
///
/// # Panics
/// Panics if the buffer changed where the answer is not optimal.
fn lent_as_owned(problem: &Problem<'_>) -> Result<Answer<Solution>, ProblemError> {
    let mut solver = NetworkSimplex::new();
    let mut flows = vec![7; problem.tails.len()];

    match solver.solve_into(problem, &mut flows) {
        Ok(Answer { outcome: Outcome::Optimal(Optimum { total_cost, potentials }), work }) => {
            let solution = Solution { flows, potentials: potentials.to_vec(), total_cost };
            Ok(Answer { outcome: Outcome::Optimal(solution), work })
        }
        unchanged => {
            assert!(
                flows.iter().all(|&flow| flow == 7),
                "{unchanged:?} should leave the flows as they were: {flows:?}"
            );
            unchanged.map(|answer| Answer { outcome: Outcome::Infeasible, work: answer.work })
        }
    }
}

/// The work that `solve` and `solve_into` both answer for `network`, its lower bounds given.
///
/// # Panics
/// Panics if the problem is refused, or the two answer different work.
fn work_of(network: &MincostNetwork) -> Work {
    let owned = solved(network).expect("the test's problem should be admitted");
    let lent = lent_as_owned(&problem_of(network)).expect("the test's problem should be admitted");
    assert_eq!(lent.work, owned.work, "both entries should answer the same work");

    owned.work
}

#[test]
fn test_the_depot_by_hand() {
    // Node 0 sends 4 units to node 2: directly by arc 0 (capacity 3, cost 5) or through node 1 by arcs 1 and 2
    // (capacity 4 and 2, cost 1 each). Two units go each way: `2 · 5 + 2 · 1 + 2 · 1 = 14`. With the potentials 0, 1,
    // 5 the arcs through the hub cost nothing reduced, the direct arc, carrying less than its capacity, `5 − 5 = 0`,
    // and arc 2, at its capacity, `1 + 1 − 5 = −3`.
    let network = MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1)]);

    assert_eq!(optimal(&network), solution(&[2, 2, 2], &[0, 1, 5], 14));
}

#[test]
fn test_two_parallel_arcs_by_hand() {
    // One unit from node 0 to node 1 by arc 0 (cost 3) or arc 1 (cost 1), each of capacity 1: arc 1 carries it.
    let network = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 1, 3), (0, 1, 0, 1, 1)]);

    assert_eq!(optimal(&network), solution(&[0, 1], &[0, 1], 1));
}

#[test]
fn test_a_loop_of_negative_cost_fills_and_one_of_positive_cost_stays_empty_by_hand() {
    let network = MincostNetwork::from_arcs(&[0], &[(0, 0, 0, 2, -3), (0, 0, 0, 2, 4)]);

    assert_eq!(optimal(&network), solution(&[2, 0], &[0], -6));
}

#[test]
fn test_an_arc_of_capacity_zero_carries_nothing_whatever_its_cost_by_hand() {
    // Arc 0 would carry the unit at cost −5, but has no room; arc 1 carries it at cost 2.
    let network = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 0, -5), (0, 1, 0, 1, 2)]);

    assert_eq!(optimal(&network), solution(&[0, 1], &[0, 2], 2));
}

#[test]
fn test_an_arc_whose_lower_bound_is_its_capacity_carries_it_by_hand() {
    // Node 0 sends 2 units to node 1: arc 0 carries exactly 1 at cost 3, arc 1 the other at cost 1.
    let network = MincostNetwork::from_arcs(&[2, -2], &[(0, 1, 1, 1, 3), (0, 1, 0, 4, 1)]);

    assert_eq!(optimal(&network), solution(&[1, 1], &[0, 1], 4));
}

#[test]
fn test_lower_bounds_that_turn_a_supplier_into_a_demander_by_hand() {
    // Node 0 supplies 1, but arc 0 must carry 3 out of it: node 0 takes 2 back from node 1 by arc 1. With the
    // potentials 1 and 0, arc 0, at its lower bound, costs `1 + 1 − 0 = 2` reduced, arc 1 nothing.
    let network = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 3, 5, 1), (1, 0, 0, 5, 1)]);

    assert_eq!(optimal(&network), solution(&[3, 2], &[1, 0], 5));
}

#[test]
fn test_a_single_node_by_hand() {
    assert_eq!(optimal(&MincostNetwork::from_arcs(&[0], &[])), solution(&[], &[0], 0));
}

#[test]
fn test_no_node_by_hand() {
    assert_eq!(optimal(&MincostNetwork::from_arcs(&[], &[])), solution(&[], &[], 0));
}

#[test]
fn test_nodes_without_arcs_balanced_and_not_by_hand() {
    assert_eq!(optimal(&MincostNetwork::from_arcs(&[0, 0, 0], &[])), solution(&[], &[0, 0, 0], 0));
    assert_eq!(outcome_of(&MincostNetwork::from_arcs(&[3, -1, -2], &[])), Ok(Outcome::Infeasible));
}

#[test]
fn test_an_infeasible_cut_leaves_the_flows_as_they_were_by_hand() {
    // Node 0 must send 2 units to node 1, but the one arc from 0 to 1 carries 1; arc 1 runs the other way.
    let network = MincostNetwork::from_arcs(&[2, -2], &[(0, 1, 0, 1, 1), (1, 0, 0, 5, 1)]);

    assert_eq!(lent_as_owned(&problem_of(&network)).map(|answer| answer.outcome), Ok(Outcome::Infeasible));
    assert_eq!(brute_force_optimum(&network), None);
}

#[test]
fn test_the_work_of_the_depot_pivot_for_pivot_by_hand() {
    // The depot of three nodes and three arcs, dealt to slots 0, 1, 2. One supplier and one demander, but no arc into
    // node 2 is wide enough for its 4 units: no first arc. Block search in blocks of one row: arc 0 at `5 − 11` flips,
    // sending its width 3; arc 2 at `1 − 11` enters degenerately, node 1's empty root link leaving; arc 1 at
    // `1 + 0 − 10` moves one unit, and arc 0 from its capacity at `5 + 0 − 2` one more. A fifth search finds nothing.
    let network = MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1)]);

    assert_eq!(work_of(&network), Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 });
}

#[test]
fn test_the_work_of_first_arcs_that_alone_reach_the_optimum_by_hand() {
    // The depot with both hub arcs of capacity 4: the search back from node 2 over arcs wide enough for 4 takes arc 2
    // from node 1, then arc 1 from node 0. Arc 2 enters degenerately, node 1's empty root link leaving; arc 1 carries
    // the 4 units round by the hub. Nothing violates after them: the one search finds nothing.
    let network = MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 4, 1)]);

    assert_eq!(optimal(&network), solution(&[0, 4, 4], &[0, 1, 2], 8));
    assert_eq!(work_of(&network), Work { pivots: 2, degenerate_pivots: 1, first_arc_pivots: 2 });
}

#[test]
fn test_the_work_of_a_first_arc_and_a_search_by_hand() {
    // Two parallel arcs of capacity 1, cost 3 and 1, for one unit. The search back from node 1 takes arc 0, the first
    // of its row: it enters at `3 − 4`, sending the unit. Block search then enters arc 1 at `1 − 3`, which takes the
    // unit over, arc 0 leaving at its lower bound.
    let network = MincostNetwork::from_arcs(&[1, -1], &[(0, 1, 0, 1, 3), (0, 1, 0, 1, 1)]);

    assert_eq!(work_of(&network), Work { pivots: 2, degenerate_pivots: 0, first_arc_pivots: 1 });
}

#[test]
fn test_an_infeasible_answer_has_its_work_by_hand() {
    // Node 0 must send 2 units to node 1, but arc 0 carries 1, too narrow to be a first arc. Block search enters it at
    // `1 − 2`, and it flips, sending its width; arc 1, from the demand to the supply, never violates.
    let network = MincostNetwork::from_arcs(&[2, -2], &[(0, 1, 0, 1, 1), (1, 0, 0, 5, 1)]);
    let work = Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 0 };

    assert_eq!(solved(&network), Ok(Answer { outcome: Outcome::Infeasible, work }));
    assert_eq!(work_of(&network), work);
}

#[test]
fn test_first_arcs_that_alone_reach_an_infeasible_outcome_by_hand() {
    // Node 0 supplies 1 to each of nodes 1 and 2, but its one arc, to node 1, carries 1. Two demands: the first arc
    // is node 1's cheapest entering arc, which enters at `1 − 3` and carries the unit, node 1's root link leaving.
    // Nothing violates after it: the one search finds nothing, and node 2's demand stays unmet.
    let network = MincostNetwork::from_arcs(&[2, -1, -1], &[(0, 1, 0, 1, 1)]);
    let work = Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 1 };

    assert_eq!(solved(&network), Ok(Answer { outcome: Outcome::Infeasible, work }));
    assert_eq!(work_of(&network), work);
}

#[test]
fn test_a_problem_of_no_arc_does_no_work() {
    // No arc ever violates its bound: no pivot, and the one search finds nothing, feasible or not.
    for supplies in [&[][..], &[0], &[0, 0, 0], &[3, -1, -2]] {
        assert_eq!(work_of(&MincostNetwork::from_arcs(supplies, &[])), Work::default(), "{supplies:?}");
    }
}

#[test]
fn test_supplies_that_do_not_balance_are_refused_and_leave_the_flows_as_they_were() {
    let network = MincostNetwork::from_arcs(&[2, -1], &[(0, 1, 0, 5, 1)]);

    assert_eq!(lent_as_owned(&problem_of(&network)), Err(ProblemError::Unbalanced { total: 1 }));
}

/// The four nodes and three arcs the refusals are made from: arc 0 from 0 to 1, arc 1 from 1 to 2, arc 2 from 2 to 3,
/// each of capacity 5 and cost 1; node 0 supplies 2 to node 3.
fn chain() -> MincostNetwork {
    MincostNetwork::from_arcs(&[2, 0, 0, -2], &[(0, 1, 0, 5, 1), (1, 2, 0, 5, 1), (2, 3, 0, 5, 1)])
}

/// The refusal of `problem`.
///
/// # Panics
/// Panics if `problem` is not refused.
fn refusal(problem: &Problem<'_>) -> ProblemError {
    NetworkSimplex::new().solve(problem).expect_err("the problem should be refused")
}

/// Checks that `problem` is refused with `error`, whose message is `message`.
///
/// # Panics
/// Panics if it is not.
fn assert_refused(problem: &Problem<'_>, error: ProblemError, message: &str) {
    assert_eq!(refusal(problem), error);
    assert_eq!(error.to_string(), message);
}

#[test]
fn test_a_table_of_another_length_is_refused_with_its_values() {
    let network = chain();
    let base = problem_of(&network);

    let error = ProblemError::Length { table: Table::Heads, length: 2, expected: 3 };
    assert_refused(&Problem { heads: &[1, 2], ..base }, error, "the heads have length 2 where the tails have length 3");

    let error = ProblemError::Length { table: Table::LowerBounds, length: 4, expected: 3 };
    let message = "the lower bounds have length 4 where the tails have length 3";
    assert_refused(&Problem { lower_bounds: Some(&[0; 4]), ..base }, error, message);

    let error = ProblemError::Length { table: Table::Capacities, length: 0, expected: 3 };
    assert_refused(
        &Problem { capacities: &[], ..base },
        error,
        "the capacities have length 0 where the tails have length 3",
    );

    let error = ProblemError::Length { table: Table::Costs, length: 5, expected: 3 };
    assert_refused(&Problem { costs: &[1; 5], ..base }, error, "the costs have length 5 where the tails have length 3");
}

#[test]
fn test_a_problem_too_large_is_named_with_its_counts() {
    // A problem past the index limit needs `2^32` supplies or tails, which no test holds; the refusal itself is checked
    // at its edge where the size is checked. Here its values and message.
    let error = ProblemError::TooLarge { node_count: 4_294_967_295, arc_count: 7 };

    assert_eq!(
        error.to_string(),
        "the node count 4294967295 and the arc count 7 are too large: nodes + 1 and arcs + nodes are each limited to \
         4294967295"
    );
}

#[test]
fn test_an_end_outside_the_nodes_is_refused_with_its_values() {
    let network = chain();
    let base = problem_of(&network);

    let error = ProblemError::TailOutsideNodes { arc: 2, tail: 4, node_count: 4 };
    assert_refused(&Problem { tails: &[0, 1, 4], ..base }, error, "arc 2 leaves node 4, not below the node count 4");

    let error = ProblemError::HeadOutsideNodes { arc: 0, head: 9, node_count: 4 };
    assert_refused(&Problem { heads: &[9, 2, 3], ..base }, error, "arc 0 enters node 9, not below the node count 4");
}

#[test]
fn test_a_lower_bound_above_its_capacity_is_refused_with_its_values() {
    let network = chain();
    let error = ProblemError::LowerAboveCapacity { arc: 1, lower_bound: 6, capacity: 5 };

    assert_refused(
        &Problem { lower_bounds: Some(&[0, 6, 0]), ..problem_of(&network) },
        error,
        "arc 1 has lower bound 6 above its capacity 5",
    );
}

#[test]
fn test_a_cost_beyond_the_bound_is_refused_with_its_values() {
    // Four nodes: the bound is `⌊(2^62 − 1) / 4⌋`.
    let bound = cost_bound(4);
    assert_eq!(BigInt::from(bound), ((BigInt::from(1_u8) << 62) - 1) / 4);
    let network = chain();
    let base = problem_of(&network);

    let answer = lent_as_owned(&Problem { costs: &[bound, -bound, 0], ..base });
    assert!(matches!(answer, Ok(Answer { outcome: Outcome::Optimal(_), .. })), "{answer:?}");

    let error = ProblemError::CostBeyondBound { arc: 1, cost: -bound - 1, node_count: 4 };
    let message = format!("arc 1 has cost {} beyond the bound ±{bound} for the node count 4", -bound - 1);
    assert_refused(&Problem { costs: &[bound, -bound - 1, 0], ..base }, error, &message);
}

#[test]
fn test_a_cost_total_beyond_the_range_is_refused_at_its_arc() {
    // On one node the bound is `2^62 − 1`; arcs at it and of capacity `u64::MAX` pass `i128::MAX` at the third.
    let bound = cost_bound(1);
    let network = MincostNetwork::from_arcs(&[0], &[(0, 0, 0, u64::MAX, bound); 3]);
    let message = format!("the costs times the capacities, in magnitude, sum past {} at arc 2", i128::MAX);

    assert_refused(&problem_of(&network), ProblemError::CostTotalBeyondRange { arc: 2 }, &message);
}

#[test]
fn test_net_supplies_beyond_their_range_are_refused_with_their_total() {
    // Node 0 supplies `2^63 − 1`, and an arc into it of lower bound `2^63` adds that much.
    let network = MincostNetwork::from_arcs(&[i64::MAX, i64::MIN + 1], &[(1, 0, 1 << 63, 1 << 63, 0)]);
    let error = ProblemError::NetSupplyBeyondRange { total: u128::from(u64::MAX) };
    let message =
        "the positive supplies net of the lower bounds add up to 18446744073709551615, past 18446744073709551614";

    assert_refused(&problem_of(&network), error, message);
}

#[test]
fn test_the_message_of_unbalanced_supplies_gives_their_total() {
    let network = MincostNetwork::from_arcs(&[i64::MAX, i64::MAX], &[]);
    let total = 2 * i128::from(i64::MAX);

    assert_refused(
        &problem_of(&network),
        ProblemError::Unbalanced { total },
        &format!("the supplies add up to {total}, not zero"),
    );
}

#[test]
fn test_where_two_breaches_meet_the_first_in_the_order_is_answered() {
    let network = chain();
    let base = problem_of(&network);

    // Two lengths: the heads before the costs. A length before any arc's breach.
    let both = refusal(&Problem { heads: &[1], costs: &[1], ..base });
    assert_eq!(both, ProblemError::Length { table: Table::Heads, length: 1, expected: 3 });
    let length_first = refusal(&Problem { tails: &[9, 1, 2], costs: &[1], ..base });
    assert_eq!(length_first, ProblemError::Length { table: Table::Costs, length: 1, expected: 3 });

    // The lowest arc with a breach: arc 0's cost before arc 1's tail.
    let lowest = refusal(&Problem { tails: &[0, 9, 2], costs: &[i64::MAX, 1, 1], ..base });
    assert_eq!(lowest, ProblemError::CostBeyondBound { arc: 0, cost: i64::MAX, node_count: 4 });

    // Of one arc: the tail, then the head, the bounds, the cost.
    let all_four = Problem { tails: &[0, 9, 2], heads: &[1, 8, 3], lower_bounds: Some(&[0, 6, 0]), ..base };
    let tail = refusal(&Problem { costs: &[1, i64::MIN, 1], ..all_four });
    assert_eq!(tail, ProblemError::TailOutsideNodes { arc: 1, tail: 9, node_count: 4 });
    let head = refusal(&Problem { tails: &[0, 1, 2], costs: &[1, i64::MIN, 1], ..all_four });
    assert_eq!(head, ProblemError::HeadOutsideNodes { arc: 1, head: 8, node_count: 4 });
    let bounds = refusal(&Problem { tails: &[0, 1, 2], heads: &[1, 2, 3], costs: &[1, i64::MIN, 1], ..all_four });
    assert_eq!(bounds, ProblemError::LowerAboveCapacity { arc: 1, lower_bound: 6, capacity: 5 });

    // An arc's own breach before the cost total it would pass; the cost total at an arc before a later arc's breach.
    // On one node the bound is `2^62 − 1`, and three loops at it of capacity `u64::MAX` pass `i128::MAX` at the third.
    let wide = Problem {
        tails: &[0; 4],
        heads: &[0, 0, 0, 9],
        lower_bounds: None,
        capacities: &[u64::MAX; 4],
        costs: &[cost_bound(1); 4],
        supplies: &[0],
    };
    assert_eq!(refusal(&wide), ProblemError::CostTotalBeyondRange { arc: 2 }, "arc 2 before arc 3's head");
    let past = cost_bound(1) + 1;
    let own_first = refusal(&Problem { costs: &[cost_bound(1), cost_bound(1), past, 0], ..wide });
    assert_eq!(own_first, ProblemError::CostBeyondBound { arc: 2, cost: past, node_count: 1 });

    // Any arc's breach before supplies that do not balance; supplies that do not balance before their net supplies.
    let unbalanced_too = refusal(&Problem { tails: &[0, 1, 7], supplies: &[1, 0, 0, 0], ..base });
    assert_eq!(unbalanced_too, ProblemError::TailOutsideNodes { arc: 2, tail: 7, node_count: 4 });
    let lower_bounds = Some(&[u64::MAX, 0, 0][..]);
    let huge = Problem { lower_bounds, capacities: &[u64::MAX, 5, 5], supplies: &[1, 0, 0, 0], ..base };
    assert_eq!(refusal(&huge), ProblemError::Unbalanced { total: 1 }, "net supplies far past their limit");
}

/// Checks what a fresh solver answers for `network` against the brute force: refused exactly where a lower bound
/// passes its capacity, at the first such arc; otherwise infeasible exactly where the brute force finds no flow, and
/// else of the brute force's cost, certified. And `solve_into` answers as `solve`.
fn check_against_the_brute_force(network: &MincostNetwork) -> Result<(), TestCaseError> {
    let inverted = network.lower_bounds.iter().zip(&network.capacities).position(|(lower, capacity)| lower > capacity);
    let answer = solved(network);
    prop_assert_eq!(&lent_as_owned(&problem_of(network)), &answer);

    let answer = match (answer, inverted) {
        (Err(ProblemError::LowerAboveCapacity { arc, .. }), Some(first)) => {
            prop_assert_eq!(arc, first);
            return Ok(());
        }
        (Ok(answer), None) => answer.outcome.optimal().map(|solution| OptimalFlow {
            flows: solution.flows,
            potentials: solution.potentials,
            total_cost: solution.total_cost,
        }),
        (answer, inverted) => {
            return Err(TestCaseError::fail(format!("{answer:?} with inverted bounds at {inverted:?}")));
        }
    };

    check_against_brute_force(network, answer.as_ref()).map_err(|breach| TestCaseError::fail(breach.to_string()))
}

proptest! {
    #[test]
    fn test_tiny_networks_against_the_brute_force(
        network in tiny_networks(TinyCosts::Uniform(-9..=9), TinyBounds::OrInverted),
    ) {
        check_against_the_brute_force(&network)?;
    }

    #[test]
    fn test_tiny_networks_with_tied_costs_against_the_brute_force(
        network in tiny_networks(TinyCosts::ZeroOrUniform(-9..=9), TinyBounds::OrInverted),
    ) {
        check_against_the_brute_force(&network)?;
    }

    #[test]
    fn test_tiny_networks_without_lower_bounds_given_against_the_brute_force(
        network in tiny_networks(TinyCosts::ZeroOrUniform(-9..=9), TinyBounds::OrInverted),
    ) {
        // The same widths with every lower bound zero, solved with the lower bounds not given: a bound above its
        // capacity leaves an arc of no width.
        let bounds = network.capacities.iter().zip(&network.lower_bounds);
        let widths = bounds.map(|(&capacity, &lower_bound)| capacity.saturating_sub(lower_bound)).collect();
        let zeros = vec![0; network.arc_count()];
        let network = MincostNetwork { capacities: widths, lower_bounds: zeros, ..network };
        let without = NetworkSimplex::new().solve(&Problem { lower_bounds: None, ..problem_of(&network) });
        prop_assert_eq!(without, NetworkSimplex::new().solve(&problem_of(&network)));
        check_against_the_brute_force(&network)?;
    }
}

/// The seeds the seeded generators draw from in these tests: a hundred and sixty of twelve numbers each, the steps of
/// a linear congruential generator from the seed's index, so that no draw repeats within a network and both sign modes
/// occur.
fn seeds() -> impl Iterator<Item = [u64; 12]> {
    (0..160_u64).map(|index| {
        let mut state = index;
        core::array::from_fn(|_| {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            state >> 33
        })
    })
}

/// The three seeded shapes drawn from `seed`, each with its name.
fn seeded_shapes(seed: &[u64]) -> [(&'static str, MincostNetwork); 3] {
    [("supply", supply_shaped(seed)), ("assignment", assignment_shaped(seed)), ("general", general(seed))]
}

/// Checks the answer of `network` against the linear-programming oracle: infeasible exactly where it finds no flow,
/// and otherwise of its cost, certified.
///
/// # Panics
/// Panics if the answers differ or the problem is refused.
fn check_against_the_oracle(network: &MincostNetwork, name: &str) {
    match (outcome_of(network), lp_optimum(network)) {
        (Ok(Outcome::Infeasible), None) => {}
        (Ok(Outcome::Optimal(solution)), Some(best)) => assert_eq!(solution.total_cost, best, "{name}"),
        (answer, best) => panic!("{name}: {answer:?} where the oracle answers {best:?}"),
    }
}

#[test]
fn test_the_seeded_shapes_against_the_oracle() {
    // Per shape: how many networks the oracle finds optimal and how many infeasible.
    let mut outcomes = [[0_usize; 2]; 3];
    for seed in seeds() {
        for (outcome, (shape, network)) in outcomes.iter_mut().zip(seeded_shapes(&seed)) {
            check_against_the_oracle(&network, &format!("{shape} {seed:?}"));
            outcome[usize::from(lp_optimum(&network).is_none())] += 1;
        }
    }

    // The supply and the general shape reach both outcomes; the assignment shape always has a flow, its slack source
    // covering whatever stays unmet.
    let [supply, assignment, general] = outcomes;
    assert!(supply.iter().chain(&general).all(|&count| count > 0), "{outcomes:?}");
    assert_eq!(assignment[1], 0, "{outcomes:?}");
}

#[test]
fn test_the_benchmark_networks_of_a_few_thousand_arcs_against_the_oracle() {
    for family in Family::ALL {
        for &size in family.sizes().iter().filter(|&&size| family != Family::Grid || size <= 37) {
            let network = family.network(size);
            assert!(network.arc_count() <= 6000, "{family:?} {size} has {} arcs", network.arc_count());
            check_against_the_oracle(&network, &format!("{family:?} {size}"));
        }
    }
}

#[test]
fn test_costs_at_both_ends_of_the_cost_bound() {
    // Five nodes in a row, node 0 sending 2 units to node 4 either along the row, at the bound on every arc but one
    // at its negation, or across by arcs at the bound; the potentials run up to about four times the bound.
    let bound = cost_bound(5);
    assert_eq!(BigInt::from(bound), ((BigInt::from(1_u8) << 62) - 1) / 5);
    assert!(BigInt::from(bound) * 5 < BigInt::from(1_u8) << 62);
    let arcs = [
        (0, 1, 0, 2, bound),
        (1, 2, 0, 2, bound),
        (2, 3, 0, 1, -bound),
        (3, 4, 0, 2, bound),
        (0, 4, 0, 1, bound),
        (2, 4, 0, 2, bound),
        (4, 0, 0, 1, -bound),
    ];
    let row = MincostNetwork::from_arcs(&[2, 0, 0, 0, -2], &arcs);

    let answer = optimal(&row);
    let (best, highest) = (brute_force_optimum(&row), answer.potentials.iter().map(|&p| p.unsigned_abs()).max());
    assert_eq!(Some(answer.total_cost), best);
    assert!(highest.is_some_and(|highest| highest > bound.unsigned_abs()), "{answer:?}");

    let one_past = MincostNetwork::from_arcs(&[2, 0, 0, 0, -2], &[(0, 4, 0, 2, -bound - 1)]);
    assert!(matches!(outcome_of(&one_past), Err(ProblemError::CostBeyondBound { arc: 0, .. })));
}

#[test]
fn test_capacities_at_u64_max_and_supplies_at_both_ends_of_i64() {
    // Node 0 takes `2^63` from nodes 1 and 2, which supply `2^63 − 1` and 1, over arcs as wide as a `u64` holds.
    let network = MincostNetwork::from_arcs(
        &[i64::MIN, i64::MAX, 1],
        &[(1, 0, 0, u64::MAX, 3), (2, 0, 0, u64::MAX, -2), (2, 1, 0, u64::MAX, 1)],
    );
    let answer = optimal(&network);
    let expected = 3 * BigInt::from(i64::MAX) - 2;

    assert_eq!(answer.flows, [i64::MAX.unsigned_abs(), 1, 0]);
    assert_eq!(BigInt::from(answer.total_cost), expected);
}

#[test]
fn test_net_supplies_at_their_limit_and_one_past_it() {
    // Node 0 supplies `2^63 − 1` and an arc into it of lower bound `2^63 − 1` adds as much: the positive net supplies
    // add up to `2^64 − 2`, the limit, which node 0 sends back over an arc as wide as a `u64` holds.
    let limit = (BigInt::from(1_u8) << 64) - 2;
    let fixed = i64::MAX.unsigned_abs();
    assert_eq!(BigInt::from(i64::MAX) + fixed, limit);
    let at_limit =
        MincostNetwork::from_arcs(&[i64::MAX, i64::MIN + 1], &[(1, 0, fixed, fixed, 0), (0, 1, 0, u64::MAX, 1)]);

    let answer = optimal(&at_limit);
    assert_eq!(answer.flows, [fixed, u64::MAX - 1]);
    assert_eq!(BigInt::from(answer.total_cost), limit);

    let one_past = MincostNetwork::from_arcs(
        &[i64::MAX, i64::MIN + 1],
        &[(1, 0, fixed + 1, fixed + 1, 0), (0, 1, 0, u64::MAX, 1)],
    );
    let total = u128::try_from(limit + 1).expect("one past the limit should fit a u128");
    assert_eq!(outcome_of(&one_past), Err(ProblemError::NetSupplyBeyondRange { total }));
}

#[test]
fn test_the_cost_total_at_i128_max_and_one_past_it() {
    // On one node, loops of negative cost fill: two at the cost bound `2^62 − 1` and two of cost 1, all of capacity
    // `u64::MAX`, and one of cost 1 and capacity `2^63 − 1` make the magnitudes times the capacities add up to exactly
    // `i128::MAX`, and the flow costs its negation.
    let bound = cost_bound(1);
    let (widest, rest) = (u64::MAX, i64::MAX.unsigned_abs());
    let loops = |last: u64| {
        [
            (0, 0, 0, widest, -bound),
            (0, 0, 0, widest, -bound),
            (0, 0, 0, widest, -1),
            (0, 0, 0, widest, -1),
            (0, 0, 0, last, -1),
        ]
    };
    let terms = loops(rest).map(|(.., capacity, cost)| BigInt::from(cost) * capacity);
    assert_eq!(terms.into_iter().sum::<BigInt>(), -BigInt::from(i128::MAX));

    let answer = optimal(&MincostNetwork::from_arcs(&[0], &loops(rest)));
    assert_eq!(answer.flows, [widest, widest, widest, widest, rest]);
    assert_eq!(answer.total_cost, -i128::MAX);

    assert_eq!(
        outcome_of(&MincostNetwork::from_arcs(&[0], &loops(rest + 1))),
        Err(ProblemError::CostTotalBeyondRange { arc: 4 })
    );
}

/// The networks the room tests solve in turn, in the order solved: a larger supply shape; the seeded shapes of the
/// first three seeds whose general network has a flow and of the first three whose general network has none, as they
/// come; and a network with no flow.
///
/// # Panics
/// Panics if the seeds hold fewer than three of either.
fn networks_in_turn() -> Vec<(String, MincostNetwork)> {
    let mut networks = vec![("supply 80".to_string(), Family::Supply.network(80))];
    let mut taken = [0_usize; 2];

    for seed in seeds() {
        let without_flow = usize::from(lp_optimum(&general(&seed)).is_none());
        if taken[without_flow] < 3 {
            taken[without_flow] += 1;
            networks.extend(seeded_shapes(&seed).map(|(shape, network)| (format!("{shape} {seed:?}"), network)));
        }
    }
    assert_eq!(taken, [3, 3], "general networks with a flow and without");
    networks.push(("no flow".to_string(), MincostNetwork::from_arcs(&[2, -2], &[(0, 1, 0, 1, 1)])));

    networks
}

#[test]
fn test_one_solver_answers_every_problem_as_a_fresh_one() {
    // The same problem twice by one solver; after a larger problem, after an infeasible one, and after any other. The
    // answers compared hold the work, which is the same too.
    let networks = networks_in_turn();
    let mut solver = NetworkSimplex::new();

    for (name, network) in &networks {
        let fresh = NetworkSimplex::new().solve(&problem_of(network));
        assert_eq!(solver.solve(&problem_of(network)), fresh, "{name} in the room of the problems before it");
        assert_eq!(solver.solve(&problem_of(network)), fresh, "{name} solved again");
    }
}

#[test]
fn test_a_refused_buffer_leaves_the_solver_answering_as_a_fresh_one() {
    let networks = networks_in_turn();
    let mut solver = NetworkSimplex::new();
    let (_, larger) = &networks[0];
    assert!(matches!(solver.solve(&problem_of(larger)), Ok(Answer { outcome: Outcome::Optimal(_), .. })));

    for (name, network) in &networks[1..] {
        let too_long = vec![0; network.arc_count() + 1];
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            let mut too_long = too_long;
            let _ = solver.solve_into(&problem_of(network), &mut too_long);
        }));
        assert!(panicked.is_err(), "{name}: a buffer of another length should be refused");

        let fresh = NetworkSimplex::new().solve(&problem_of(network));
        assert_eq!(solver.solve(&problem_of(network)), fresh, "{name} after the refused buffer");
    }
}

#[test]
fn test_a_resolve_of_a_fresh_solver_answers_as_a_solve() {
    // Nothing is kept: the answer is the solve's, its flows, potentials and work included.
    for (name, network) in &networks_in_turn() {
        let fresh = NetworkSimplex::new().solve(&problem_of(network));
        assert_eq!(NetworkSimplex::new().resolve(&problem_of(network)), fresh, "{name}");
    }
}

#[test]
fn test_the_same_problem_resolved_makes_no_pivot_where_a_solve_makes_some() {
    // The kept basis is the problem's own end, which the re-flow leaves as it is: the same answer, with no pivot.
    // The work tells the kept start from the star wherever the solve pivots.
    let mut pivoting = 0;
    for (name, network) in &networks_in_turn() {
        let mut solver = NetworkSimplex::new();
        let solved = solver.solve(&problem_of(network)).expect("the test's problem should be admitted");
        pivoting += usize::from(solved.work.pivots > 0);

        let resolved = solver.resolve(&problem_of(network)).expect("the test's problem should be admitted");
        assert_eq!(resolved, Answer { work: Work::default(), ..solved }, "{name}");
    }

    assert!(pivoting > 10, "{pivoting} of the test's problems pivot in a solve");
}

#[test]
fn test_a_problem_of_other_counts_resolves_as_a_solve() {
    // Each network after the one before it in turn, whose node or arc count differs: the kept basis cannot serve.
    let networks = networks_in_turn();
    let mut solver = NetworkSimplex::new();

    for pair in networks.windows(2) {
        let [(_, before), (name, network)] = pair else { unreachable!("windows of two") };
        let counts = |network: &MincostNetwork| (network.node_count(), network.arc_count());
        assert_ne!(counts(before), counts(network), "{name}: the test's networks in turn differ in their counts");

        let _ = solver.solve(&problem_of(before)).expect("the test's problem should be admitted");
        let fresh = NetworkSimplex::new().solve(&problem_of(network));
        assert_eq!(solver.resolve(&problem_of(network)), fresh, "{name} after a basis of other counts");
    }
}

/// The networks of a sequence of `length`, each perturbed from the one before by its own seed drawn from `seed`, the
/// first `network` itself.
fn perturbed_sequence(network: MincostNetwork, seed: &[u64], length: u64) -> Vec<MincostNetwork> {
    let mut sequence = vec![network];
    for step in 1..length {
        let last = sequence.last().expect("the sequence holds its first network");
        let next = perturbed(last, &[seed[0], seed[1], seed[2], step]);
        sequence.push(next);
    }

    sequence
}

/// Checks the answer `resolved` of `network` against the answer `solved` of a fresh solver: refused alike, of the same
/// outcome and the same cost, certified, and of work whose counts keep their relations.
///
/// # Panics
/// Panics if it does not.
fn assert_resolved_as_solved(
    network: &MincostNetwork,
    resolved: &Result<Answer<Solution>, ProblemError>,
    solved: &Result<Answer<Solution>, ProblemError>,
    name: &str,
) {
    assert_eq!(cost_of(resolved), cost_of(solved), "{name}");

    let Ok(Answer { outcome, work }) = resolved else { return };
    assert!(work.degenerate_pivots <= work.pivots && work.first_arc_pivots <= work.pivots, "{name}: {work:?}");
    if let Outcome::Optimal(solution) = outcome {
        let certified = check_certificate(network, &solution.flows, &solution.potentials, solution.total_cost);
        assert_eq!(certified, Ok(()), "{name}: the resolve should be certified optimal");
    }
}

/// The cost `answer` found, `None` where it is infeasible; or its refusal.
fn cost_of(answer: &Result<Answer<Solution>, ProblemError>) -> Result<Option<i128>, ProblemError> {
    answer.clone().map(|answer| answer.outcome.optimal().map(|solution| solution.total_cost))
}

#[test]
fn test_sequences_of_perturbed_networks_resolve_as_they_solve() {
    // Per shape and seed, a sequence of eight networks, each a few numbers moved from the one before, resolved in turn
    // by one solver: each answer of the cost a fresh solve answers. Every resolve starts from the kept basis, so that
    // the counts of all resolves, summed, stay below those of the solves.
    let (mut resolved_pivots, mut solved_pivots, mut outcomes) = (0, 0, [0_usize; 2]);

    for seed in seeds().take(24) {
        for (shape, network) in seeded_shapes(&seed) {
            let mut solver = NetworkSimplex::new();
            let _ = solver.solve(&problem_of(&network)).expect("a seeded network should be admitted");

            for (step, network) in perturbed_sequence(network, &seed, 8).iter().enumerate().skip(1) {
                let name = format!("{shape} {seed:?}, step {step}");
                let resolved = solver.resolve(&problem_of(network));
                let solved = NetworkSimplex::new().solve(&problem_of(network));
                assert_resolved_as_solved(network, &resolved, &solved, &name);

                let (Ok(resolved), Ok(solved)) = (resolved, solved) else { continue };
                assert_eq!(resolved.work.first_arc_pivots, 0, "{name}: a kept start tries no first arc");
                resolved_pivots += resolved.work.pivots;
                solved_pivots += solved.work.pivots;
                outcomes[usize::from(resolved.outcome == Outcome::Infeasible)] += 1;
            }
        }
    }

    assert!(outcomes.iter().all(|&count| count > 0), "optimal and infeasible resolves: {outcomes:?}");
    assert!(resolved_pivots < solved_pivots, "{resolved_pivots} pivots resolving, {solved_pivots} solving");
}

#[test]
fn test_the_same_problem_resolved_past_the_limit_answers_as_a_solve() {
    // Two nodes of no supply, an arc each way of capacity `u64::MAX` at cost −1: the solve ends with one arc carrying
    // `u64::MAX` out of the tree, so the excesses over that basis add up to `u64::MAX`, past the limit. The kept basis
    // cannot serve, and the resolve of the same problem is a solve, its work included.
    let network = MincostNetwork::from_arcs(&[0, 0], &[(0, 1, 0, u64::MAX, -1), (1, 0, 0, u64::MAX, -1)]);
    let mut solver = NetworkSimplex::new();

    let solved = solver.solve(&problem_of(&network));
    assert!(solved.as_ref().is_ok_and(|answer| answer.work.pivots > 0), "{solved:?}");
    assert_eq!(solver.resolve(&problem_of(&network)), solved);
}
