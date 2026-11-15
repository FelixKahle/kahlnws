// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! One solver solving again: each day starts from the basis the day before ended on.
//!
//! The network of the example `transport` is solved for the three days of the example `reuse`: the warehouses hold 20
//! and 30 units every day, and the shops ask for (10, 25, 15), (20, 20, 10) and (5, 15, 30). Day 1 is solved; each
//! later day is resolved, from the basis the solver kept. Each day prints its cost and its pivots, beside the pivots a
//! fresh solve of the same day makes.
//!
//! Run it from the repository root with `cargo run -p kahlnws --example resolve`.

use kahlnws::{NetworkSimplex, Problem, ProblemError};

fn main() -> Result<(), ProblemError> {
    let tails = [0, 0, 0, 1, 1, 1];
    let heads = [2, 3, 4, 2, 3, 4];
    let capacities = [20; 6];
    let costs = [8, 6, 10, 9, 12, 13];
    let demands = [[10, 25, 15], [20, 20, 10], [5, 15, 30]];

    // By hand. The costs are those of the example `reuse`. A fresh solve starts from the star, every lane empty, and
    // tries warehouse 0's lanes 0, 1 and 2 first, the cheapest into each shop. On day 1 lanes 0 and 1 enter, lane 2
    // does not violate at its turn, and block search enters lanes 3, 2 (sending nothing), 5 and 4: 6 pivots. On day 2
    // lanes 0, 1 (sending nothing), 3, 2 (sending nothing) and 5 enter: 5. On day 3 lanes 0, 1, 2 (sending nothing),
    // 3, 4, 5 and 2 again: 7.
    //
    // Day 1 ends with lanes 2 to 5 in the tree below warehouse 1, lane 2 empty, and lane 1 full out of it; the
    // potentials are 3 and 0 at the warehouses, 9, 12 and 13 at the shops. Day 2 keeps that tree, and lane 1 full: the
    // tree carries 20, 0 and 10 to the shops, the optimum already, but lane 4, empty, can carry nothing back toward
    // warehouse 1, so shop 1 hangs from the root instead, at potential 0. Lane 1, full, then has the reduced cost
    // 6 + 3 - 0 = 9 above zero and enters, sending nothing as shop 1's root link leaves: 1 pivot. Day 3 keeps that
    // tree, lane 1 in it and lanes 0 and 4 empty out of it. Shop 2 asks for 30 and lane 5 carries at most 20, so lane 5
    // leaves the tree full; 5 units come to shop 2 down a root link, and warehouse 1 sends 5 up its own. Lane 4 is the
    // first violation the search meets and enters, sending those 5 units from warehouse 1 to shop 1, back over lane 1
    // to warehouse 0 and over lane 2 to shop 2: 1 pivot.
    let hand_worked = [(465, 6, 6), (430, 1, 5), (525, 1, 7)];

    // The solver that keeps each day's basis, and one that solves each day from the star.
    let mut solver = NetworkSimplex::new();
    let mut fresh = NetworkSimplex::new();

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

        let answer = if day == 1 { solver.solve(&problem)? } else { solver.resolve(&problem)? };
        let fresh_pivots = fresh.solve(&problem)?.work.pivots;
        let cost = answer.outcome.optimal().expect("the warehouses should cover every day's demand").total_cost;
        println!("day {day}: cost {cost}, pivots {} (a fresh solve: {fresh_pivots})", answer.work.pivots);
        assert_eq!((cost, answer.work.pivots, fresh_pivots), expected);
    }

    Ok(())
}
