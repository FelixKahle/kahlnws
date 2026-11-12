// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The benchmark suite: cold solves of the seeded networks of every family at every size it sweeps, the supply and the
//! assignment shape at tying and at distinct costs, so that a solve with ties stands beside one of the same network
//! without, and the grid at its own costs from a few arcs to about a quarter of a million.
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

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, SamplingMode, Throughput};
use kahlnws::{NetworkSimplex, Optimum, Outcome, Problem};
use kahlnws_testing::{Family, MincostNetwork, OptimalFlow, check_certificate, distinct_costs, suite_tie_breaks};

/// The costs a row's network is solved at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Costs {
    /// The family's own costs: small ones, at which the supply and the assignment shape tie through their twin arcs and
    /// the grid by the few values its costs take.
    Tying,
    /// The family's own costs made distinct by the suite's tie-breaks, so that the same network has one optimum.
    Distinct,
}

/// One row of the suite: a family, the costs its network is solved at, and its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Row {
    /// The family the network is drawn from.
    family: Family,
    /// The costs it is solved at.
    costs: Costs,
    /// Its size: the number of demands, or the grid's side.
    size: usize,
}

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

impl Costs {
    /// The costs the rows of `family` are solved at: tying and distinct for the shapes whose twins tie, the family's
    /// own for the grid, whose ties are no twins a tie-break would be argued from.
    const fn of(family: Family) -> &'static [Self] {
        match family {
            Family::Supply | Family::Assignment => &[Self::Tying, Self::Distinct],
            Family::Grid => &[Self::Tying],
        }
    }

    /// The name of these costs in a row's name.
    const fn name(self) -> &'static str {
        match self {
            Self::Tying => "tying",
            Self::Distinct => "distinct",
        }
    }

    /// `network`, drawn at its family's own costs, at these costs.
    fn applied(self, network: MincostNetwork) -> MincostNetwork {
        match self {
            Self::Tying => network,
            Self::Distinct => {
                let tie_breaks = suite_tie_breaks(network.arc_count());
                MincostNetwork { costs: distinct_costs(&network, &tie_breaks), ..network }
            }
        }
    }
}

impl Row {
    /// Every row of the suite, family by family in the order [`Family::ALL`] gives, then size by size ascending, then
    /// tying before distinct costs.
    fn all() -> impl Iterator<Item = Self> {
        Family::ALL.into_iter().flat_map(Self::of_family)
    }

    /// The rows of `family`, size by size ascending, then tying before distinct costs.
    fn of_family(family: Family) -> impl Iterator<Item = Self> {
        let costs = Costs::of(family);

        family.sizes().iter().flat_map(move |&size| costs.iter().map(move |&costs| Self { family, costs, size }))
    }

    /// The row's name within the group `cold`: `<family>/<costs>/<size>`.
    fn name(self) -> String {
        format!("{}/{}/{}", family_name(self.family), self.costs.name(), self.size)
    }

    /// The row's network, drawn from the suite's seeds at the row's costs.
    ///
    /// # Panics
    /// Panics if a generator refuses the row's size, which the suite's sizes rule out.
    fn network(self) -> MincostNetwork {
        self.costs.applied(self.family.network(self.size))
    }
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

        if let Err(breach) = check_certificate(&self.network, &answer.flows, &answer.potentials, answer.total_cost) {
            panic!("the answer should be certified optimal: {breach}");
        }

        answer
    }
}

/// The problem of `network`. A network whose lower bounds are all zero is handed in without them, as a caller of such
/// a network would hand it in.
fn problem_of(network: &MincostNetwork) -> Problem<'_> {
    let has_lower_bounds = network.lower_bounds.iter().any(|&lower_bound| lower_bound > 0);

    Problem {
        tails: &network.tails,
        heads: &network.heads,
        lower_bounds: has_lower_bounds.then_some(network.lower_bounds.as_slice()),
        capacities: &network.capacities,
        costs: &network.costs,
        supplies: &network.supplies,
    }
}

/// The name of `family` in a row's name.
const fn family_name(family: Family) -> &'static str {
    match family {
        Family::Supply => "supply",
        Family::Assignment => "assignment",
        Family::Grid => "grid",
    }
}

/// The throughput of one solve of a network of `arc_count` arcs: its arcs.
///
/// # Panics
/// Panics if `arc_count` does not fit 64 bits, which no network in memory passes.
fn arcs(arc_count: usize) -> Throughput {
    Throughput::Elements(u64::try_from(arc_count).expect("an arc count should fit a u64"))
}

/// The group `cold` of `criterion`, sampled flat: every sample runs the same number of iterations.
///
/// Criterion's default, linear sampling runs the `k`-th sample `k` times as many iterations as the first, so a row of
/// tens of milliseconds would run for many times its measurement time; flat sampling keeps it near that time.
fn cold_group(criterion: &mut Criterion) -> BenchmarkGroup<'_, WallTime> {
    let mut group = criterion.benchmark_group("cold");
    group.sampling_mode(SamplingMode::Flat);

    group
}

/// Checks and times every row of the suite, one after the other.
///
/// # Panics
/// Panics if a row's checks fail before its timing.
fn cold(criterion: &mut Criterion) {
    let mut group = cold_group(criterion);

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
