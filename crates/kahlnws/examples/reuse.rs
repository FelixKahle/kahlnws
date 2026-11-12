// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! One solver kept for many solves: its room reserved once, each answer written into a buffer the caller holds.
//!
//! The network of the example `transport` is solved for three days. The warehouses hold 20 and 30 units every day;
//! the shops ask for (10, 25, 15) on day 1, (20, 20, 10) on day 2 and (5, 15, 30) on day 3. After the reservation, no
//! solve of the loop allocates.
//!
//! Run it from the repository root with `cargo run -p kahlnws --example reuse`.

use kahlnws::{NetworkSimplex, Problem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tails = [0, 0, 0, 1, 1, 1];
    let heads = [2, 3, 4, 2, 3, 4];
    let capacities = [20; 6];
    let costs = [8, 6, 10, 9, 12, 13];
    let demands = [[10, 25, 15], [20, 20, 10], [5, 15, 30]];

    // By hand, as in the example `transport`: everything from warehouse 1 would cost 585, 550 and 615; warehouse 0's
    // 20 units save 1, 6 and 3 a unit to shops 0, 1 and 2. Day 1 and day 2 send all 20 to shop 1 and save 120. On day
    // 3 shop 2 asks for 30 and a lane carries 20, so warehouse 0 sends shop 2 at least 10, and shop 1 the other 10:
    // a saving of 90.
    let hand_worked = [585 - 120, 550 - 120, 615 - 90];

    let mut solver = NetworkSimplex::new();
    solver.try_reserve(5, 6)?;
    let mut flows = [0; 6];

    for (day, (&[shop_0, shop_1, shop_2], expected)) in (1..).zip(demands.iter().zip(hand_worked)) {
        let supplies = [20, 30, -shop_0, -shop_1, -shop_2];
        let problem = Problem {
            tails: &tails,
            heads: &heads,
            lower_bounds: None,
            capacities: &capacities,
            costs: &costs,
            supplies: &supplies,
        };

        let answer = solver.solve_into(&problem, &mut flows)?; // allocates nothing
        let optimum = answer.outcome.optimal().expect("the warehouses should cover every day's demand");
        println!("day {day}: cost {}", optimum.total_cost);
        assert_eq!(optimum.total_cost, expected);
    }

    Ok(())
}
