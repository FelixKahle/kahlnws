// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The smallest useful call: two warehouses supply three shops at the least cost.
//!
//! Warehouses 0 and 1 hold 20 and 30 units; shops 0, 1 and 2 ask for 10, 25 and 15. A lane runs from every warehouse
//! to every shop and carries up to 20 units. One solve answers the cost of the cheapest delivery and the flow on each
//! lane.
//!
//! Run it from the repository root with `cargo run -p kahlnws --example transport`.

use kahlnws::{NetworkSimplex, Problem, ProblemError};

/// The number of warehouses: nodes `0..WAREHOUSES` are the warehouses, the nodes after them the shops.
const WAREHOUSES: usize = 2;

fn main() -> Result<(), ProblemError> {
    // Nodes 0 and 1 are the warehouses, nodes 2, 3 and 4 the shops; the arcs are the six lanes.
    let problem = Problem {
        tails: &[0, 0, 0, 1, 1, 1],
        heads: &[2, 3, 4, 2, 3, 4],
        lower_bounds: None,
        capacities: &[20; 6],
        costs: &[8, 6, 10, 9, 12, 13],
        supplies: &[20, 30, -10, -25, -15],
    };

    let solution = NetworkSimplex::new()
        .solve(&problem)?
        .outcome
        .optimal()
        .expect("the warehouses should hold what the shops ask for, and the lanes carry it");

    println!("cost: {}", solution.total_cost);
    for ((warehouse, shop), flow) in problem.tails.iter().zip(problem.heads).zip(&solution.flows) {
        println!("warehouse {warehouse} -> shop {}: {flow}", shop - WAREHOUSES);
    }

    // By hand: a lane from warehouse 1 costs 1, 6 and 3 more than the lane from warehouse 0 to shops 0, 1 and 2. Were
    // every unit sent from warehouse 1, the cost would be 9 * 10 + 12 * 25 + 13 * 15 = 585; each unit warehouse 0
    // sends instead saves that difference. Its 20 units save the most, 6 * 20 = 120, all on the lane to shop 1, which
    // carries 20; warehouse 1 sends the rest, 10, 5 and 15, within its lanes. So 585 - 120 = 465, and no other flow
    // reaches it.
    assert_eq!(solution.total_cost, 465);
    assert_eq!(solution.flows, [0, 20, 0, 10, 5, 15]);

    Ok(())
}
