// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The benchmark suite of cold solves: the seeded networks of every family at every size the rows sweep, the supply
//! and the assignment shape at tying and at distinct costs, so that a solve with ties stands beside one of the same
//! network without, and the grid at its own costs from a few arcs to about a quarter of a million.
//!
//! Rows `cold/<family>/<costs>/<size>`, the size being the number of demands of the supply and the assignment shape and
//! the side of the grid. A row times one [`NetworkSimplex::solve_into`] of one network by a solver reserved for it,
//! into a buffer of flows made once, so nothing is allocated inside the timing. Every solve starts from the star of
//! root links and keeps nothing of the solve before, so a row repeats one solve of one network and nothing is reset
//! between iterations: the work of an iteration is fixed by the network alone, and every sample runs the same number of
//! iterations. The throughput is in arcs.
//!
//! Before a row is timed, the very solve that gets timed runs twice: both must answer the same flows, potentials and
//! cost, and the answer must pass the testing crate's certificate, its flow within the bounds and meeting every
//! supply, its potentials pricing it optimal and its cost as stated.
//!
//! Run with `cargo bench -p kahlnws --bench cold`; append `-- cold/grid/` (or any other prefix of a row's name) to run
//! part of it. Under `cargo test -p kahlnws --benches` the suite is a smoke run, which makes every row's checks and
//! runs the row once, untimed.

#[path = "rows/sweep.rs"]
mod sweep;

use criterion::Criterion;
use kahlnws::{NetworkSimplex, Optimum, Outcome};
use kahlnws_testing::{MincostNetwork, OptimalFlow};
use sweep::{Row, arcs, assert_certified, flat_group, problem_of};

/// A cold solve of one network: the network as plain numbers, the solver reserved for it and the buffer its flows are
/// written to, made once.
///
/// # Invariant
/// `flows` holds one entry per arc of `network`.
#[derive(Debug)]
struct ColdSolve {
    /// The network solved.
    network: MincostNetwork,
    /// The solver, reserved for the network's nodes and arcs.
    solver: NetworkSimplex,
    /// The flow of each arc, written by every solve.
    flows: Vec<u64>,
}

impl ColdSolve {
    /// The solve of `network` by a solver reserved for its nodes and arcs, with a buffer of one flow per arc.
    ///
    /// # Panics
    /// Panics if the room cannot be reserved.
    fn new(network: MincostNetwork) -> Self {
        let mut solver = NetworkSimplex::new();
        solver.try_reserve(network.node_count(), network.arc_count()).expect("the room should be reserved");
        let flows = vec![0; network.arc_count()];

        Self { network, solver, flows }
    }

    /// Solves the network from scratch into the buffer and answers the cost: the call a row times.
    ///
    /// # Panics
    /// Panics if the network is refused or infeasible, which the suite's generators rule out.
    fn solve(&mut self) -> i128 {
        self.optimum().total_cost
    }

    /// Solves the network from scratch into the buffer and answers the optimum, its potentials lent by the solver.
    ///
    /// # Panics
    /// Panics if the network is refused or infeasible, which the suite's generators rule out.
    fn optimum(&mut self) -> Optimum<'_> {
        let answer = self
            .solver
            .solve_into(&problem_of(&self.network), &mut self.flows)
            .expect("the network should be admitted");

        match answer.outcome {
            Outcome::Optimal(optimum) => optimum,
            Outcome::Infeasible => panic!("the suite's network should be feasible"),
        }
    }

    /// Solves the network twice and answers the first answer, which the certificate proves optimal and the second
    /// solve repeats.
    ///
    /// # Panics
    /// Panics as [`solve`](Self::solve) does, with the first breach of the certificate, or if the second answer
    /// differs from the first.
    fn checked_answer(&mut self) -> OptimalFlow {
        let first = self.certified_answer();
        assert_eq!(self.certified_answer(), first, "a second solve should repeat the first answer");

        first
    }

    /// Solves the network once and reads the answer, which the certificate proves optimal.
    ///
    /// # Panics
    /// Panics as [`solve`](Self::solve) does, or with the first breach of the certificate.
    fn certified_answer(&mut self) -> OptimalFlow {
        // The potentials are lent by the solver and the flows written to the buffer, both until the next solve: the
        // answer keeps copies, to be compared with that solve's.
        let (total_cost, potentials) = {
            let Optimum { total_cost, potentials } = self.optimum();
            (total_cost, potentials.to_vec())
        };
        let answer = OptimalFlow { flows: self.flows.to_vec(), potentials, total_cost };
        assert_certified(&self.network, &answer);

        answer
    }
}

/// Checks and times every row of the suite, one after the other.
///
/// # Panics
/// Panics if a row's checks fail before its timing.
fn cold(criterion: &mut Criterion) {
    let mut group = flat_group(criterion, "cold");

    for row in Row::all() {
        let mut solve = ColdSolve::new(row.network());
        solve.checked_answer();

        group.throughput(arcs(solve.network.arc_count()));
        group.bench_function(row.name(), |bencher| bencher.iter(|| solve.solve()));
    }

    group.finish();
}

/// Runs the suite under criterion's settings, as the command line overrides them.
fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    cold(&mut criterion);
    criterion.final_summary();
}
