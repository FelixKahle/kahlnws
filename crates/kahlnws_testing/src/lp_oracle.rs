// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The optimum cost of a [`MincostNetwork`] by an independent linear-programming solver: the oracle for min-cost
//! flows larger than [`brute_force_optimum`](crate::brute_force_optimum) enumerates.
//!
//! It lives here rather than beside the tests that use it so that the crate under test does not name the solver among
//! its own dev-dependencies, which would put it, and the native build behind it, into its manifest.
//!
//! The solver works in `f64` with tolerances, scaling and presolve, so no bound on the numbers makes its answer exact
//! by itself. The oracle admits the range below, and its own tests show it right there: hand cases with bounds and
//! supplies at `2^26` and an optimum of `±2^53`, and a property test of tiny networks whose flows are scaled by up to
//! `2^21 + 1` and whose costs by up to `2^24 − 1`, whose optimum must scale exactly.

use core::num::NonZeroUsize;

use culmen::prelude::*;
use itertools::Itertools;

use crate::mincost_network::{MincostNetwork, ShapedNetwork};

/// The largest magnitude of every lower bound, capacity, supply and cost the oracle admits: `2^26`. An `f64` near it
/// is spaced `2^-26` apart, finer than the solver's default feasibility and optimality tolerances of `1e-7`, so the
/// solver's tests on single numbers are decided by the numbers and not by their rounding. Every such number fits an
/// `i32`, and so an `f64` exactly.
const NUMBER_RANGE: u64 = 1 << 26;

/// The largest sum the oracle admits of the magnitudes of the costs, each times its capacity: `2^53`, so that the
/// objective value of every flow within the bounds is an integer an `f64` holds exactly, and the solver's choice among
/// vertices is decided by the costs and not by the rounding of their sums.
const OBJECTIVE_RANGE: u128 = 1 << 53;

/// How far from an integer a flow the solver reports may lie: its default primal feasibility tolerance, `1e-7`. The
/// solver answers a vertex of the program, and every vertex is integral, since the node–arc incidence matrix is
/// totally unimodular and every bound and supply is an integer; a reported column value lies within that tolerance of
/// the vertex's coordinate.
const FEASIBILITY_TOLERANCE: f64 = 1e-7;

/// The optimum cost of `network` by a linear-programming solver, as a linear program over the node–arc incidence
/// matrix, whose vertices are integral; `None` where no flow within the bounds meets the supplies.
///
/// It admits every lower bound, capacity, supply and cost within `2^26` in magnitude, and the magnitudes of the costs
/// times the capacities summing to at most `2^53`. The solver runs on the calling thread alone. The cost answered is
/// that of the integral flow the solver reports, summed exactly.
///
/// # Panics
/// Panics if the shape of `network` is broken, if a number passes the range above, or if the solver refuses the model,
/// ends otherwise than optimal or infeasible, or reports a flow farther than `1e-7` from an integer. Such an end is the
/// oracle's failure, not the failure of the solve a test compares with it: the test has asked the oracle something it
/// cannot answer.
pub fn lp_optimum(network: &MincostNetwork) -> Option<i128> {
    let shaped =
        network.shaped().unwrap_or_else(|breach| panic!("called `lp_optimum` on a network of broken shape: {breach}"));

    if let Some(breach) = range_breach(shaped) {
        panic!("called `lp_optimum` on a network outside its range: {breach}");
    }

    if has_inverted_bounds(shaped) {
        return None;
    }

    let flows = optimal_flows(program_of(shaped))?;

    Some(shaped.cost_of(&flows))
}

/// How a network passes the range the oracle admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
enum RangeBreach {
    /// A lower bound, capacity, supply or cost passes [`NUMBER_RANGE`] in magnitude.
    #[error("a bound, supply or cost of magnitude {magnitude}, past 2^26")]
    Number {
        /// The first such magnitude, in the order lower bounds, capacities, supplies, costs.
        magnitude: u64,
    },
    /// The magnitudes of the costs, each times its capacity, sum past [`OBJECTIVE_RANGE`].
    #[error("costs times capacities summing past 2^53 at arc {arc}")]
    CostTotal {
        /// The lowest arc at which the running sum passes.
        arc: usize,
    },
}

/// The first way `network` passes the range the oracle admits: a number past [`NUMBER_RANGE`] in magnitude, then the
/// costs at capacity summing past [`OBJECTIVE_RANGE`].
fn range_breach(network: ShapedNetwork<'_>) -> Option<RangeBreach> {
    let bounds = network.lower_bounds().iter().chain(network.capacities()).copied();
    let supplies = network.supplies().iter().map(|supply| supply.unsigned_abs());
    let costs = network.arcs().map(|arc| arc.cost.unsigned_abs());
    let mut magnitudes = bounds.chain(supplies).chain(costs);

    magnitudes
        .find(|&magnitude| magnitude > NUMBER_RANGE)
        .map(|magnitude| RangeBreach::Number { magnitude })
        .or_else(|| network.arc_past_cost_total(OBJECTIVE_RANGE).map(|arc| RangeBreach::CostTotal { arc }))
}

/// Whether some arc's lower bound passes its capacity: no flow meets its bounds, and the solver would refuse the
/// column rather than call the network infeasible.
fn has_inverted_bounds(network: ShapedNetwork<'_>) -> bool {
    network.arcs().any(|arc| arc.lower_bound > arc.capacity)
}

/// The linear program of `network`, whose numbers lie within the range and whose bounds are ordered: one column per
/// arc, in arc order, within its bounds; one row per node that conserves its supply; and the cost of the flow to
/// minimise.
///
/// # Panics
/// Panics if a number passes the range, or if the solver refuses the model.
fn program_of(network: ShapedNetwork<'_>) -> Program {
    let mut model = Model::new();
    let mut conservation: Vec<LinExpr> = (0..network.node_count()).map(|_| LinExpr::new()).collect();
    let mut objective = LinExpr::new();

    for arc in network.arcs() {
        let flow = model.add_var(Var::continuous().bounds(exact_bound(arc.lower_bound), exact_bound(arc.capacity)));
        // A self-loop leaves and enters its node alike: its two terms in its node's row add up to nothing, as the
        // solver adds up the terms of a repeated column when the model is built.
        conservation[arc.tail].add_term(flow, 1.0);
        conservation[arc.head].add_term(flow, -1.0);
        // The solver takes a negative cost as it is.
        objective.add_term(flow, exact_signed(arc.cost));
    }
    for (row, &supply) in conservation.into_iter().zip_eq(network.supplies()) {
        model.add_constraint(row.equals(exact_signed(supply)));
    }
    model.minimize(objective);

    model.build().expect("the oracle's model should build")
}

/// The flows of an optimal vertex of `program`, one per column, each the integer the solver's value lies at; `None`
/// where the solver proves the program infeasible. The solver runs on the calling thread alone.
///
/// # Panics
/// Panics if the solver refuses the program, ends otherwise than optimal or infeasible, or reports a value that
/// [`integral_flow`] refuses.
fn optimal_flows(program: Program) -> Option<Vec<u64>> {
    let one_thread = Settings { threads: Some(NonZeroUsize::MIN), ..Settings::new() };
    // The solver's log reaches only callbacks unless the request asks for the console, so the solve is quiet.
    let mut solver = Solver::new(Highs::new(), program).expect("the solver should load the oracle's model");
    let solved = solver.solve(Request::new().settings_from(one_thread)).expect("the solver should solve the model");

    match solved.outcome {
        Outcome::Optimal(solution) => Some(solution.values.iter().map(|&value| integral_flow(value)).collect()),
        // Every column is bounded, so a model the solver cannot tell unbounded from infeasible is infeasible.
        Outcome::Infeasible | Outcome::InfeasibleOrUnbounded => None,
        other => panic!("the solver should prove the oracle's model optimal or infeasible, not {other:?}"),
    }
}

/// A bound within [`NUMBER_RANGE`] as the `f64` it is exactly.
///
/// # Panics
/// Panics if `bound` passes `u32::MAX`, which the range rules out.
fn exact_bound(bound: u64) -> f64 {
    f64::from(u32::try_from(bound).expect("a bound within the oracle's range should fit a u32"))
}

/// A supply or a cost within [`NUMBER_RANGE`] in magnitude as the `f64` it is exactly.
///
/// # Panics
/// Panics if `number` lies outside the range of an `i32`, which the range rules out.
fn exact_signed(number: i64) -> f64 {
    f64::from(i32::try_from(number).expect("a number within the oracle's range should fit an i32"))
}

/// The flow a reported column `value` stands for: the integer within [`FEASIBILITY_TOLERANCE`] of it.
///
/// # Panics
/// Panics if `value` lies farther than the tolerance from every integer, or its integer lies outside
/// `0..=NUMBER_RANGE`, which no flow within the oracle's range does.
fn integral_flow(value: f64) -> u64 {
    let rounded = value.round();

    assert!(
        (value - rounded).abs() <= FEASIBILITY_TOLERANCE && (0.0..=exact_bound(NUMBER_RANGE)).contains(&rounded),
        "called `integral_flow` with {value}, which should lie within 1e-7 of an integer in 0..=2^26"
    );

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is a whole number in 0..=2^26, asserted above, which a u64 holds exactly"
    )]
    let flow = rounded as u64;

    flow
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::{TinyBounds, TinyCosts, TinyMincostNetworks, TinySupplies, brute_force_optimum};

    /// The view of `network`, whose shape the test knows to hold.
    fn shaped(network: &MincostNetwork) -> ShapedNetwork<'_> {
        network.shaped().expect("the test's network should be of sound shape")
    }

    /// The largest number the oracle admits, `2^26`, as a cost or a supply.
    const EDGE: i64 = 1 << 26;

    /// A chain 0 -> 1 -> 2 that sends `2^26` from node 0 to node 2 over two arcs of bounds `lower..=2^26`, each
    /// costing `cost`.
    fn edge_chain(lower: u64, cost: i64) -> MincostNetwork {
        let mut network = MincostNetwork::default();
        let (from, middle, to) = (network.add_node(EDGE), network.add_node(0), network.add_node(-EDGE));
        network.add_arc(from, middle, lower, NUMBER_RANGE, cost);
        network.add_arc(middle, to, lower, NUMBER_RANGE, cost);

        network
    }

    #[test]
    fn test_self_loops_take_their_cost_and_bounds_but_no_matrix_entry() {
        // Two units over one arc of cost 3; a loop of cost -2 with room up to 4 is filled (-8), a loop of cost 5 with
        // lower bound 2 carries its bound (10): 6 - 8 + 10 = 8.
        let mut network = MincostNetwork::default();
        let (sender, taker) = (network.add_node(2), network.add_node(-2));
        network.add_arc(sender, taker, 0, 5, 3);
        network.add_arc(sender, sender, 1, 4, -2);
        network.add_arc(taker, taker, 2, 6, 5);

        assert_eq!(lp_optimum(&network), Some(8));
    }

    #[test]
    fn test_a_supply_no_arc_can_carry_is_infeasible() {
        let mut network = MincostNetwork::default();
        let (sender, taker) = (network.add_node(3), network.add_node(-3));
        network.add_arc(sender, taker, 0, 2, 1);

        assert_eq!(lp_optimum(&network), None);
    }

    #[test]
    fn test_a_network_without_arcs_is_decided_by_its_supplies() {
        let mut network = MincostNetwork::default();
        network.add_node(0);
        assert_eq!(lp_optimum(&network), Some(0));

        network.add_node(1);
        network.add_node(-1);
        assert_eq!(lp_optimum(&network), None);

        assert_eq!(lp_optimum(&MincostNetwork::default()), Some(0));
    }

    #[test]
    fn test_a_cheaper_detour_wins_over_a_direct_arc() {
        // 0 -> 2 directly at 5, or 0 -> 1 -> 2 at 1 + 2 with room 3: four units send three by the detour, one direct.
        let mut network = MincostNetwork::default();
        let (from, middle, to) = (network.add_node(4), network.add_node(0), network.add_node(-4));
        network.add_arc(from, to, 0, 10, 5);
        network.add_arc(from, middle, 0, 3, 1);
        network.add_arc(middle, to, 0, 10, 2);

        assert_eq!(lp_optimum(&network), Some(3 * 3 + 5));
    }

    #[test]
    fn test_the_edge_of_the_range_is_answered_exactly() {
        // 2^26 units over two arcs of cost ±2^26: an optimum of ±2^53, the costs at capacity summing to 2^53.
        assert_eq!(lp_optimum(&edge_chain(0, EDGE)), Some(1 << 53));
        assert_eq!(lp_optimum(&edge_chain(0, -EDGE)), Some(-(1 << 53)));

        // Lower bounds at 2^26: both arcs are fixed at their bounds.
        assert_eq!(lp_optimum(&edge_chain(NUMBER_RANGE, EDGE)), Some(1 << 53));

        // A cost of 2^26 on one arc and -2^26 on the other: the two cancel.
        let mut cancelling = edge_chain(0, EDGE);
        cancelling.costs[1] = -EDGE;
        assert_eq!(lp_optimum(&cancelling), Some(0));

        // One unit of supply too many for capacities of 2^26.
        let mut short = edge_chain(0, 1);
        short.capacities[1] = NUMBER_RANGE - 1;
        assert_eq!(lp_optimum(&short), None);

        // A detour of one unit's room at cost 1 beside the direct arc at 2^26 - 1: the detour takes one unit.
        let mut detour = MincostNetwork::default();
        let (from, middle, to) = (detour.add_node(EDGE), detour.add_node(0), detour.add_node(-EDGE));
        detour.add_arc(from, to, 0, NUMBER_RANGE, EDGE - 1);
        detour.add_arc(from, middle, 0, 1, 0);
        detour.add_arc(middle, to, 0, 1, 1);
        assert_eq!(lp_optimum(&detour), Some(i128::from((EDGE - 1).strict_mul(EDGE - 1) + 1)));
    }

    #[test]
    fn test_the_program_of_a_network_has_its_optimal_flows() {
        // Three units from node 0 to node 2, directly at 4 with room 1, or through node 1 at 1 + 1 with room 2.
        let mut network = MincostNetwork::default();
        let (from, middle, to) = (network.add_node(3), network.add_node(0), network.add_node(-3));
        network.add_arc(from, to, 0, 1, 4);
        network.add_arc(from, middle, 0, 2, 1);
        network.add_arc(middle, to, 0, 5, 1);
        assert_eq!(optimal_flows(program_of(shaped(&network))), Some(vec![1, 2, 2]));

        network.supplies = vec![4, 0, -4];
        assert_eq!(optimal_flows(program_of(shaped(&network))), None, "the cut around node 0 carries 3");
    }

    #[test]
    fn test_a_lower_bound_above_its_capacity_is_infeasible_as_the_brute_force_says() {
        // One arc 0 -> 1 of lower bound 2 and capacity 1, and a loop of lower bound 3 and capacity 2: both
        // infeasible, where the solver would refuse the column.
        let mut arc = MincostNetwork::default();
        let (from, to) = (arc.add_node(2), arc.add_node(-2));
        arc.add_arc(from, to, 2, 1, 1);
        assert_eq!((lp_optimum(&arc), brute_force_optimum(&arc)), (None, None));

        let mut looped = MincostNetwork::default();
        let only = looped.add_node(0);
        looped.add_arc(only, only, 3, 2, 0);
        assert_eq!((lp_optimum(&looped), brute_force_optimum(&looped)), (None, None));
    }

    #[test]
    fn test_has_inverted_bounds_by_hand() {
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 0, 1, 1, 0);
        assert!(!has_inverted_bounds(shaped(&network)), "a fixed arc is ordered");

        network.add_arc(0, 0, 2, 1, 0);
        assert!(has_inverted_bounds(shaped(&network)));

        assert!(!has_inverted_bounds(shaped(&MincostNetwork::default())));
    }

    #[test]
    fn test_range_breach_by_hand() {
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 0, 0, 1 << 26, -(1 << 26));
        assert_eq!(range_breach(shaped(&network)), None, "2^26 · 2^26 is 2^52");

        network.add_arc(0, 0, 0, 1 << 26, 1 << 26);
        assert_eq!(range_breach(shaped(&network)), None, "two of them sum to 2^53");

        network.add_arc(0, 0, 0, 1, 1);
        assert_eq!(range_breach(shaped(&network)), Some(RangeBreach::CostTotal { arc: 2 }), "one more passes 2^53");

        let mut wide = MincostNetwork::default();
        wide.add_node(0);
        wide.add_arc(0, 0, 0, (1 << 26) + 1, 0);
        assert_eq!(range_breach(shaped(&wide)), Some(RangeBreach::Number { magnitude: (1 << 26) + 1 }));

        wide.capacities[0] = 1;
        wide.lower_bounds[0] = (1 << 26) + 2;
        assert_eq!(range_breach(shaped(&wide)), Some(RangeBreach::Number { magnitude: (1 << 26) + 2 }));

        wide.lower_bounds[0] = 0;
        wide.supplies[0] = -(1 << 26) - 3;
        assert_eq!(range_breach(shaped(&wide)), Some(RangeBreach::Number { magnitude: (1 << 26) + 3 }));

        wide.supplies[0] = -(1 << 26);
        assert_eq!(range_breach(shaped(&wide)), None, "a supply at -2^26");

        wide.costs[0] = (1 << 26) + 4;
        assert_eq!(range_breach(shaped(&wide)), Some(RangeBreach::Number { magnitude: (1 << 26) + 4 }));
    }

    #[test]
    fn test_range_breaches_give_their_values() {
        assert_eq!(
            RangeBreach::Number { magnitude: 7 }.to_string(),
            "a bound, supply or cost of magnitude 7, past 2^26"
        );
        assert_eq!(RangeBreach::CostTotal { arc: 3 }.to_string(), "costs times capacities summing past 2^53 at arc 3");
    }

    #[test]
    fn test_exact_conversions_by_hand() {
        assert_eq!(exact_bound(1 << 26), 67_108_864.0);
        assert_eq!(exact_signed(-(1 << 26)), -67_108_864.0);

        assert_eq!(integral_flow(7.999_999_95), 8);
        assert_eq!(integral_flow(0.0), 0);
        assert_eq!(integral_flow(67_108_864.0), 1 << 26);
    }

    #[test]
    #[should_panic(
        expected = "called `integral_flow` with -8.4, which should lie within 1e-7 of an integer in 0..=2^26"
    )]
    fn test_integral_flow_refuses_a_value_far_from_an_integer() {
        let _ = integral_flow(-8.4);
    }

    #[test]
    #[should_panic(expected = "called `integral_flow` with 8.000001, which should lie within 1e-7 of an integer")]
    fn test_integral_flow_refuses_a_value_just_past_the_tolerance() {
        let _ = integral_flow(8.000_001);
    }

    #[test]
    #[should_panic(expected = "called `integral_flow` with -1, which should lie within 1e-7 of an integer in 0..=2^26")]
    fn test_integral_flow_refuses_a_negative_flow() {
        let _ = integral_flow(-1.0);
    }

    #[test]
    #[should_panic(
        expected = "called `integral_flow` with inf, which should lie within 1e-7 of an integer in 0..=2^26"
    )]
    fn test_integral_flow_refuses_an_infinite_value() {
        let _ = integral_flow(f64::INFINITY);
    }

    #[test]
    #[should_panic(
        expected = "called `lp_optimum` on a network outside its range: a bound, supply or cost of magnitude 67108865, \
                    past 2^26"
    )]
    fn test_a_network_outside_the_range_is_refused() {
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 0, 0, (1 << 26) + 1, 0);

        let _ = lp_optimum(&network);
    }

    #[test]
    #[should_panic(expected = "called `lp_optimum` on a network of broken shape: arc 0 has end 1, outside the 1 nodes")]
    fn test_a_broken_shape_is_refused() {
        let mut network = MincostNetwork::default();
        network.add_node(0);
        network.add_arc(0, 1, 0, 1, 1);

        let _ = lp_optimum(&network);
    }

    /// `network` with its bounds, capacities and supplies times `flow_factor` and its costs times `cost_factor`.
    fn scaled(network: &MincostNetwork, flow_factor: i64, cost_factor: i64) -> MincostNetwork {
        let flows = flow_factor.cast_unsigned();

        MincostNetwork {
            tails: network.tails.clone(),
            heads: network.heads.clone(),
            lower_bounds: network.lower_bounds.iter().map(|&bound| bound * flows).collect(),
            capacities: network.capacities.iter().map(|&capacity| capacity * flows).collect(),
            costs: network.costs.iter().map(|&cost| cost * cost_factor).collect(),
            supplies: network.supplies.iter().map(|&supply| supply * flow_factor).collect(),
        }
    }

    /// A factor for the flows: up to `2^21 + 1`, so that a tiny network's supplies, at most 18, stay within `2^26`.
    fn flow_factor() -> impl Strategy<Value = i64> {
        prop_oneof![Just(1), Just((1 << 21) + 1), Just((1 << 21) - 1), Just(1_048_573), 1_i64..=(1 << 21)]
    }

    /// A factor for the costs: up to `2^24 - 1`, so that a tiny network's costs, at most 4, stay within `2^26`.
    fn cost_factor() -> impl Strategy<Value = i64> {
        prop_oneof![Just(1), Just((1 << 24) - 1), Just((1 << 23) + 1), Just(8_388_607), 1_i64..(1 << 24)]
    }

    proptest! {
        #[test]
        fn test_the_oracle_and_the_brute_force_agree_on_tiny_networks(
            network in TinyMincostNetworks {
                arcs: 0..=6,
                costs: TinyCosts::ZeroOrUniform(-4..=4),
                supplies: TinySupplies::OfAFlowOrRandom,
                bounds: TinyBounds::OrInverted,
            }.strategy(),
        ) {
            prop_assert_eq!(lp_optimum(&network), brute_force_optimum(&network));
        }

        #[test]
        fn test_a_scaled_network_has_its_optimum_scaled(
            network in TinyMincostNetworks {
                arcs: 0..=6,
                costs: TinyCosts::Uniform(-4..=4),
                supplies: TinySupplies::OfAFlowOrRandom,
                bounds: TinyBounds::Ordered,
            }.strategy(),
            flow_factor in flow_factor(),
            cost_factor in cost_factor(),
        ) {
            // The program is homogeneous in the flow numbers and linear in the costs, and its vertices are integral,
            // so the optimum of the scaled network is the brute force's times both factors.
            let expected =
                brute_force_optimum(&network).map(|least| least * i128::from(flow_factor) * i128::from(cost_factor));
            let large = scaled(&network, flow_factor, cost_factor);

            prop_assert_eq!(range_breach(shaped(&large)), None);
            prop_assert_eq!(lp_optimum(&large), expected);
        }
    }
}
