// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Everything an answer holds beyond the flow: the potentials that prove it optimal, and the work of the solve.
//!
//! The network of the example `transport` is solved once more. Each lane's reduced cost is its cost plus the potential
//! of its warehouse less the potential of its shop. The flow is optimal because every lane below its capacity has a
//! reduced cost of at least zero and every lane that carries flow one of at most zero, which the example checks
//! without trusting the solver. The work counts the pivots the solve made.
//!
//! Run it from the repository root with `cargo run -p kahlnws --example potentials`.

use kahlnws::{Answer, NetworkSimplex, Problem, ProblemError};

/// The number of warehouses: nodes `0..WAREHOUSES` are the warehouses, the nodes after them the shops.
const WAREHOUSES: usize = 2;

fn main() -> Result<(), ProblemError> {
    let problem = Problem {
        tails: &[0, 0, 0, 1, 1, 1],
        heads: &[2, 3, 4, 2, 3, 4],
        lower_bounds: None,
        capacities: &[20; 6],
        costs: &[8, 6, 10, 9, 12, 13],
        supplies: &[20, 30, -10, -25, -15],
    };

    let Answer { outcome, work } = NetworkSimplex::new().solve(&problem)?;
    let solution =
        outcome.optimal().expect("the warehouses should hold what the shops ask for, and the lanes carry it");
    println!("cost: {}", solution.total_cost);

    let (warehouses, shops) = solution.potentials.split_at(WAREHOUSES);
    for (warehouse, potential) in warehouses.iter().enumerate() {
        println!("potential of warehouse {warehouse}: {potential}");
    }
    for (shop, potential) in shops.iter().enumerate() {
        println!("potential of shop {shop}: {potential}");
    }

    let mut optimal = true;
    for arc in 0..problem.tails.len() {
        let (warehouse, shop) = (problem.tails[arc], problem.heads[arc]);
        let (capacity, flow) = (problem.capacities[arc], solution.flows[arc]);
        // In `i128`, where a cost and two potentials add up without overflow.
        let reduced_cost = i128::from(problem.costs[arc]) + i128::from(solution.potentials[warehouse])
            - i128::from(solution.potentials[shop]);
        println!("reduced cost of warehouse {warehouse} -> shop {}: {reduced_cost}", shop - WAREHOUSES);

        optimal &= (flow == capacity || reduced_cost >= 0) && (flow == 0 || reduced_cost <= 0);
    }
    println!("{}", if optimal { "the optimality conditions hold" } else { "the optimality conditions fail" });

    println!("pivots: {}", work.pivots);
    println!("degenerate pivots: {}", work.degenerate_pivots);
    println!("first arc pivots: {}", work.first_arc_pivots);

    // By hand, as in the example `transport`: the optimum is 465, and the potentials prove it.
    assert_eq!(solution.total_cost, 465);
    assert!(optimal);

    Ok(())
}
