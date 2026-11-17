// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The rows every benchmark suite sweeps: the seeded networks of every family at every size, the supply and the
//! assignment shape at tying and at distinct costs, so that a solve with ties stands beside one of the same network
//! without, and the grid at its own costs from a few arcs to about a quarter of a million. With them, what a row's
//! measurement shares: the problem a network is handed in as, the throughput in arcs, the group sampled flat, and the
//! certificate an answer passes before a row is timed.

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, SamplingMode, Throughput};
use kahlnws::Problem;
use kahlnws_testing::{Family, MincostNetwork, OptimalFlow, check_certificate, distinct_costs, suite_tie_breaks};

/// The costs a row's network is solved at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowCosts {
    /// The family's own costs: small ones, at which the supply and the assignment shape tie through their twin arcs and
    /// the grid by the few values its costs take.
    Tying,
    /// The family's own costs made distinct by the suite's tie-breaks, so that the same network has one optimum.
    Distinct,
}

/// One row of the suites: a family, the costs its network is solved at, and its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    /// The family the network is drawn from.
    pub(crate) family: Family,
    /// The costs it is solved at.
    costs: RowCosts,
    /// Its size: the number of demands, or the grid's side.
    pub(crate) size: usize,
}

impl RowCosts {
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
    /// Every row of the suites, family by family in the order [`Family::ALL`] gives, then size by size ascending, then
    /// tying before distinct costs.
    pub(crate) fn all() -> impl Iterator<Item = Self> {
        Family::ALL.into_iter().flat_map(Self::of_family)
    }

    /// The rows of `family`, size by size ascending, then tying before distinct costs.
    fn of_family(family: Family) -> impl Iterator<Item = Self> {
        let costs = RowCosts::of(family);

        family.sizes().iter().flat_map(move |&size| costs.iter().map(move |&costs| Self { family, costs, size }))
    }

    /// The row's name within a suite's group: `<family>/<costs>/<size>`.
    pub(crate) fn name(self) -> String {
        format!("{}/{}/{}", family_name(self.family), self.costs.name(), self.size)
    }

    /// The row's network, drawn from the suite's seeds at the row's costs.
    ///
    /// # Panics
    /// Panics if a generator refuses the row's size, which the suite's sizes rule out.
    pub(crate) fn network(self) -> MincostNetwork {
        self.at_costs(self.family.network(self.size))
    }

    /// `network`, a network of the row's family at the family's own costs, at the row's costs: the row's network where
    /// `network` is the family's, and a changed one at the row's costs where it is changed.
    pub(crate) fn at_costs(self, network: MincostNetwork) -> MincostNetwork {
        self.costs.applied(network)
    }
}

/// The problem of `network`. A network whose lower bounds are all zero is handed in without them, as a caller of such
/// a network would hand it in.
pub(crate) fn problem_of(network: &MincostNetwork) -> Problem<'_> {
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
pub(crate) fn arcs(arc_count: usize) -> Throughput {
    Throughput::Elements(u64::try_from(arc_count).expect("an arc count should fit a u64"))
}

/// The group `name` of `criterion`, sampled flat: every sample runs the same number of iterations.
///
/// Criterion's default, linear sampling runs the `k`-th sample `k` times as many iterations as the first, so a row of
/// tens of milliseconds would run for many times its measurement time; flat sampling keeps it near that time.
pub(crate) fn flat_group<'c>(criterion: &'c mut Criterion, name: &str) -> BenchmarkGroup<'c, WallTime> {
    let mut group = criterion.benchmark_group(name);
    group.sampling_mode(SamplingMode::Flat);

    group
}

/// Checks that the testing crate's certificate proves `answer` an optimum of `network`: its flow within the bounds and
/// meeting every supply, its potentials pricing it optimal and its cost as stated.
///
/// # Panics
/// Panics with the first breach of the certificate, or if `answer` does not hold one flow per arc and one potential
/// per node.
pub(crate) fn assert_certified(network: &MincostNetwork, answer: &OptimalFlow) {
    if let Err(breach) = check_certificate(network, &answer.flows, &answer.potentials, answer.total_cost) {
        panic!("the answer should be certified optimal: {breach}");
    }
}
