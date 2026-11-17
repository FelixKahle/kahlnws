// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A network held over three days: each day is loaded on the same lanes and solved from the basis the day before
//! ended on.
//!
//! The network of the example `transport` is held for the three days of the example `reuse`: the warehouses hold 20
//! and 30 units every day, and the shops ask for (10, 25, 15), (20, 20, 10) and (5, 15, 30). Day 1 is loaded into a
//! network that held nothing and solved; each later day is loaded with that day's numbers and solved from the basis the
//! network kept. Each day prints its cost and the pivots its solve made, beside the pivots of a fresh solve of the same
//! day by the solver, which keeps nothing from one day to the next.
//!
//! Run it from the repository root with `cargo run -p kahlnws --example network`.

use kahlnws::{Network, NetworkSimplex, Problem, ProblemError};

fn main() -> Result<(), ProblemError> {
    // Node w is warehouse w and node 2 + s shop s; lane 3 * w + s leads from warehouse w to shop s.
    let tails = [0, 0, 0, 1, 1, 1];
    let heads = [2, 3, 4, 2, 3, 4];
    let capacities = [20; 6];
    let costs = [8, 6, 10, 9, 12, 13];
    let demands = [[10, 25, 15], [20, 20, 10], [5, 15, 30]];

    // Each day's cost, worked by hand: a flow costs what warehouse 1 would pay to serve every shop alone, less what
    // each unit warehouse 0 sends in its place saves, 1, 6 and 3 at shops 0, 1 and 2. Beside it, the pivots of the
    // network's solve and of a fresh one, which are not worked by hand: they are the counts this library's solves make,
    // pinned so that the example shows a later day, started from the basis the day before ended on, taking far fewer.
    let days = [
        // Warehouse 1 alone: 9 * 10 + 12 * 25 + 13 * 15 = 585; warehouse 0's 20 units save 6 each at shop 1.
        (585 - 6 * 20, 6, 6),
        // Warehouse 1 alone: 180 + 240 + 130 = 550; warehouse 0's 20 units save 6 each at shop 1.
        (550 - 6 * 20, 1, 5),
        // Warehouse 1 alone: 45 + 180 + 390 = 615; its lane carries 20 of shop 2's 30, so warehouse 0 sends the
        // other 10 there, saving 3 each, and its last 10 to shop 1, saving 6 each.
        (615 - 3 * 10 - 6 * 10, 1, 7),
    ];

    // The network that keeps each day's basis, and the solver that solves each day from nothing kept.
    let mut network = Network::new();
    let mut fresh = NetworkSimplex::new();

    for (day, (&[shop_0, shop_1, shop_2], expected)) in (1..).zip(demands.iter().zip(days)) {
        let supplies = [20, 30, -shop_0, -shop_1, -shop_2];
        let problem = Problem {
            tails: &tails,
            heads: &heads,
            lower_bounds: None,
            capacities: &capacities,
            costs: &costs,
            supplies: &supplies,
        };

        network.load(&problem)?;
        let answer = network.solve();
        let fresh_pivots = fresh.solve(&problem)?.work.pivots;
        let cost = answer.outcome.optimal().expect("the warehouses should cover every day's demand");
        println!("day {day}: cost {cost}, pivots {} (a fresh solve: {fresh_pivots})", answer.work.pivots);
        assert_eq!((cost, answer.work.pivots, fresh_pivots), expected);
    }

    Ok(())
}
