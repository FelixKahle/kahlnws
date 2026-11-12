// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The answers that are no optimal flow: a problem with no flow, which is an outcome, and a refused problem, which is
//! an error.
//!
//! The network of the example `transport` is solved twice more. With every lane cut to 5 units, warehouse 0 can send
//! 15 of its 20 units at most: the solve answers that no flow exists. With the last lane drawn to node 5, which the
//! five nodes do not have, the problem is refused before anything runs, and the error names the breach.
//!
//! Run it from the repository root with `cargo run -p kahlnws --example outcomes`.

use kahlnws::{Answer, NetworkSimplex, Outcome, Problem, ProblemError, Solution};

/// Prints what a solve answered: the cost, that no flow exists, or why the problem was refused.
fn report(answered: &Result<Answer<Solution>, ProblemError>) {
    match answered {
        Ok(Answer { outcome: Outcome::Optimal(solution), .. }) => println!("cost: {}", solution.total_cost),
        Ok(Answer { outcome: Outcome::Infeasible, .. }) => println!("no flow exists"),
        Err(error) => println!("refused: {error}"),
    }
}

fn main() {
    let narrow = Problem {
        tails: &[0, 0, 0, 1, 1, 1],
        heads: &[2, 3, 4, 2, 3, 4],
        lower_bounds: None,
        capacities: &[5; 6],
        costs: &[8, 6, 10, 9, 12, 13],
        supplies: &[20, 30, -10, -25, -15],
    };
    let misdrawn = Problem { capacities: &[20; 6], heads: &[2, 3, 4, 2, 3, 5], ..narrow };

    let mut solver = NetworkSimplex::new();
    let no_flow = solver.solve(&narrow);
    report(&no_flow);
    let refused = solver.solve(&misdrawn);
    report(&refused);

    // By hand: warehouse 0's three lanes carry 3 * 5 = 15 of its 20 units; arc 5 enters node 5 of five nodes.
    assert!(matches!(no_flow, Ok(Answer { outcome: Outcome::Infeasible, .. })));
    assert_eq!(refused, Err(ProblemError::HeadOutsideNodes { arc: 5, head: 5, node_count: 5 }));
}
