// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The benchmark suite of loads on a kept basis: for every row of the cold suite, its network again or changed, loaded
//! into a [`Network`] that holds the basis a solve of the row's own network ended on and solved, and solved by a
//! [`NetworkSimplex`](kahlnws::NetworkSimplex) from the star, so that the two stand side by side on the same network.
//!
//! Rows `warm/<family>/<costs>/<size>/<change>/<entry>`: the row of the cold suite, how its network was changed, and
//! the entry timed, `load` or `solve`. The changes:
//!
//! - `same`: nothing changed, the network the kept basis ended on: what a load and its solve cost where the solve makes
//!   no pivot.
//! - `supplies`: three supply moves, each of one to three units from one node to another of the same role, a
//!   supplier's supply to another supplier or a demander's demand to another demander, the giver keeping its role. The
//!   grid has one supplier and one demander, so its rows have no `supplies` pair.
//! - `arcs`: three capacities, each moved by one or two units, and the costs of three arcs, each redrawn.
//! - `broad`: the arcs' moves of the testing crate's `perturbed`, which moves the capacity of about four arcs in
//!   fifteen and the cost of about as many, each by one or two units: a share of the network, not a few numbers.
//! - `costs-hundredth`, `costs-tenth` and `costs-all`: the costs of a share of the arcs, rounded up, redrawn. The
//!   shares are decades, each ten times the last, from a few arcs of the larger networks to all of them.
//!
//! A few is three, the same on every network, so that `supplies` and `arcs` stay a few numbers however large the
//! network around them; three is the most supply moves the testing crate's perturbation makes, and its units (three for
//! a supply, two for a capacity) are that perturbation's too. A redrawn cost is the one the arc takes in the family's
//! network with its costs drawn from another stream: each arc's cost drawn from the range of its role in the family, a
//! derived cost such as the grid's slack arc given by its rule, and only arcs whose cost the redraw moves are chosen.
//! Every change is made to the family's own network, and the row's costs, tying or distinct, are applied after it, as
//! to the row's own network, the distinct costs scaled by the multiplier of the row's own network, which the capacities
//! give; so a change of a distinct row moves the costs it names, not only their tie-breaks, and no other cost where it
//! moves a capacity. Every change is drawn from one fixed seed, so every run of the suite makes the same changes. A
//! change of costs leaves a network's flow in place; with its supplies or capacities moved a network may have none, and
//! the pair then times two answers that no flow exists.
//!
//! Every iteration of a row starts from the same state: for `load`, a network that loaded the row's own network, solved
//! it and holds the basis that solve ended on; for `solve`, a solver that solved the row's own network, whose room
//! holds it. Each is cloned in criterion's untimed setup and its room reserved for the changed network, so that nothing
//! is allocated inside the timing. A row times one [`Network::load`] of the changed network by that clone, the
//! [`Network::solve`] after it and every flow of its optimum written into a buffer made once, or one
//! [`NetworkSimplex::solve_into`](kahlnws::NetworkSimplex::solve_into) of the changed network into that buffer: both
//! rows write the same flows out and answer the cost; the problem handed in is made once too, so the work of an
//! iteration is fixed by the two networks alone. Each iteration needs a clone of its own, so the clock is read around
//! each iteration: a row's time includes that reading, the same addend in both rows of a pair, which shrinks the ratio
//! of a pair whose times come near it. The throughput is in arcs.
//!
//! Before a pair of rows is timed, the change must have changed the network, unless it is `same`. Each entry runs
//! twice, from two clones: both runs must answer the same outcome, flows, potentials, cost and work, and an optimal
//! answer must pass the testing crate's certificate. The two entries must answer the same outcome and cost. A load that
//! cannot start from the kept basis starts from the star, and its solve answers exactly what a load into a network that
//! held nothing and its solve answer, the work included; so the load's work must differ from that one's, which proves
//! it started from the kept basis. On `same` the load's solve must make no pivot: the kept basis is the problem's own
//! end. A network whose kept basis cannot serve it would stop the suite there; none of the suite's can, since their
//! supplies and capacities are far below the limit of the excesses. So would a load from the kept basis whose work
//! happened to equal a star load's, which the work could then not tell from a star load; none of the suite's rows
//! does.
//!
//! The suite then prints a line with the network's arcs, what the change moved, and what the checks found, the outcome
//! and each entry's pivots, such as `checked grid/tying/9/arcs: 235 arcs, moved 0 supplies by 0 units, 3 capacities,
//! 3 costs; optimal at 89305; pivots: 1 loading, 249 solving`.
//!
//! Run with `cargo bench -p kahlnws --bench warm`; append `-- warm/grid/` (or any other prefix of a row's name) to run
//! part of it. Under `cargo test -p kahlnws --benches` the suite is a smoke run, which makes every pair's checks and
//! runs each row once, untimed.

#[path = "rows/row_change.rs"]
mod row_change;
#[path = "rows/sweep.rs"]
mod sweep;

use core::iter;

use criterion::{BatchSize, Bencher, Criterion};
use kahlnws::{Answer, Network, NetworkOptimum, Optimum, Outcome, Work};
use kahlnws_testing::{
    MincostNetwork, OptimalFlow, capacities_moved, costs_taken, perturbation_draws, perturbed, supplies_moved,
};
use rand::{Rng, SeedableRng};
use rand_xoshiro::{SplitMix64, Xoshiro256PlusPlus};
use row_change::{Kept, Moved, at_row_costs, redrawn_costs, total_cost};
use sweep::{Row, arcs, assert_certified, flat_group, problem_of};

/// The seed every change is drawn from: any value serves, and it is fixed so that every run of the suite makes the
/// same changes to every network.
const CHANGE_SEED: u64 = 0x7761_726d_2073_7461;

/// How many supplies, capacities and costs a small change moves: three of each, the same on every network, the most
/// supply moves the testing crate's perturbation makes.
const FEW: usize = 3;

/// The most units a supply move takes: three, as the testing crate's perturbation moves a supply.
const SUPPLY_UNITS: u64 = 3;

/// The most units a capacity moves by: two, as the testing crate's perturbation moves a capacity.
const CAPACITY_UNITS: u64 = 2;

/// How a row's network is changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    /// Nothing: the network the kept basis ended on, again.
    Same,
    /// [`FEW`] supply moves, each of one to [`SUPPLY_UNITS`] units from one node to another of the same role.
    Supplies,
    /// [`FEW`] capacities, each moved by one to [`CAPACITY_UNITS`] units, and the costs of [`FEW`] arcs, redrawn.
    Arcs,
    /// The arcs' moves of the testing crate's `perturbed`: the capacity of about four arcs in fifteen and the cost of
    /// about as many, each moved by one or two units.
    Broad,
    /// The costs of a share of the arcs, redrawn.
    Costs(Share),
}

/// A share of a network's arcs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Share {
    /// One arc in a hundred.
    Hundredth,
    /// One arc in ten.
    Tenth,
    /// Every arc.
    All,
}

/// The entry a row times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// [`Network::load`], from the basis the network holds, then [`Network::solve`].
    Load,
    /// [`NetworkSimplex::solve_into`](kahlnws::NetworkSimplex::solve_into), from the star.
    Solve,
}

/// A row's network changed: at the row's costs, with what the change moved of it.
#[derive(Debug)]
struct ChangedNetwork {
    /// The changed network at the row's costs.
    network: MincostNetwork,
    /// What the change moved, counted on the family's own costs.
    moved: Moved,
}

/// A pair of rows: a network changed from a row's own, and the states every iteration starts from.
///
/// # Invariant
/// `kept` solved a network of the nodes and the arcs of `network`, each arc with its tail and its head; `flows` holds
/// one entry per arc of `network`.
#[derive(Debug)]
struct WarmStart<'k> {
    /// The changed network, which both entries solve.
    network: MincostNetwork,
    /// The network and the solver that solved the row's own network.
    kept: &'k Kept,
    /// The flow of each arc, written by every solve of the solver.
    flows: Vec<u64>,
}

/// What the checks of a pair found, which the suite prints before the pair is timed. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CheckedPair {
    /// The arcs of the network both entries solved.
    arc_count: usize,
    /// The cost both entries answered; `None` where both answered that no flow exists.
    total_cost: Option<i128>,
    /// The work of the load's solve.
    loaded: Work,
    /// The work of the solver's solve.
    solved: Work,
}

impl Change {
    /// Every change.
    const ALL: [Self; 7] = [
        Self::Same,
        Self::Supplies,
        Self::Arcs,
        Self::Broad,
        Self::Costs(Share::Hundredth),
        Self::Costs(Share::Tenth),
        Self::Costs(Share::All),
    ];

    /// The name of this change in a row's name.
    const fn name(self) -> &'static str {
        match self {
            Self::Same => "same",
            Self::Supplies => "supplies",
            Self::Arcs => "arcs",
            Self::Broad => "broad",
            Self::Costs(Share::Hundredth) => "costs-hundredth",
            Self::Costs(Share::Tenth) => "costs-tenth",
            Self::Costs(Share::All) => "costs-all",
        }
    }

    /// `row`'s network changed so, drawn from the suite's seed: the change made to the family's own network, then the
    /// row's costs applied; `None` where the network has no such change, as the grid has no two nodes of one role to
    /// move a supply between.
    ///
    /// # Panics
    /// Panics if a change other than `same` leaves the network as it was, or if at the row's costs it moves other
    /// numbers than at the family's.
    fn applied(self, row: Row) -> Option<ChangedNetwork> {
        let own = row.family.network(row.size);
        let mut draws = Xoshiro256PlusPlus::seed_from_u64(CHANGE_SEED);

        let changed = match self {
            Self::Same => own.clone(),
            Self::Supplies => supplies_moved(&own, FEW, SUPPLY_UNITS, &mut draws)?,
            Self::Arcs => {
                let moved = capacities_moved(&own, FEW, CAPACITY_UNITS, &mut draws);
                costs_taken(&moved, &redrawn_costs(row, &draws), FEW, &mut draws)
            }
            // The numbers the perturbation does not move as `broad` takes it, the supplies, are `own`'s.
            Self::Broad => {
                let perturbation = perturbation(&own);
                MincostNetwork { capacities: perturbation.capacities, costs: perturbation.costs, ..own.clone() }
            }
            Self::Costs(share) => costs_taken(&own, &redrawn_costs(row, &draws), share.of(own.arc_count()), &mut draws),
        };

        let moved = Moved::between(&own, &changed);
        assert!(
            self == Self::Same || moved != Moved::default(),
            "the change {} should change the network of {}",
            self.name(),
            row.name()
        );

        let network = at_row_costs(row, &own, changed);
        assert_eq!(
            Moved::between(&row.network(), &network),
            moved,
            "the change {} of {} should move at the row's costs what it moves at the family's",
            self.name(),
            row.name()
        );

        Some(ChangedNetwork { network, moved })
    }

    /// Checks that the work of a load through this change proves it started from the kept basis: it differs from
    /// `from_star`, the work of a load of the same network into a network that held nothing and its solve, which a
    /// load that cannot start from the kept basis answers exactly; and where nothing changed, the load's solve made no
    /// pivot.
    ///
    /// # Panics
    /// Panics if either does not hold.
    fn assert_started_from_the_kept_basis(self, loaded: Work, from_star: Work) {
        assert!(
            loaded != from_star,
            "the load should start from the kept basis, whose work differs from a start from the star: {loaded:?}"
        );
        assert!(
            self != Self::Same || loaded.pivots == 0,
            "a load of the network the kept basis ended on should make no pivot: {loaded:?} against {from_star:?}"
        );
    }
}

impl Share {
    /// The arcs of this share of `arc_count`, rounded up, so that every network but one of no arc has at least one.
    const fn of(self, arc_count: usize) -> usize {
        match self {
            Self::Hundredth => arc_count.div_ceil(100),
            Self::Tenth => arc_count.div_ceil(10),
            Self::All => arc_count,
        }
    }
}

impl Entry {
    /// Both entries, in the order a pair's rows are timed.
    const BOTH: [Self; 2] = [Self::Load, Self::Solve];

    /// The name of this entry in a row's name.
    const fn name(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Solve => "solve",
        }
    }
}

impl<'k> WarmStart<'k> {
    /// The pair of `network`, a network on the nodes and arcs of the network `kept` solved, each arc with its tail and
    /// its head.
    fn new(network: MincostNetwork, kept: &'k Kept) -> Self {
        let flows = vec![0; network.arc_count()];

        Self { network, kept, flows }
    }

    /// Checks what the pair through `change` measures, and answers what it found: each entry answers twice alike, the
    /// certificate proves an optimal answer optimal, the two entries answer the same outcome and cost, and the work
    /// proves the load started from the kept basis, as [`Change::assert_started_from_the_kept_basis`] checks.
    ///
    /// # Panics
    /// Panics if a check fails, with what it found.
    fn check(&mut self, change: Change) -> CheckedPair {
        let loaded = self.repeated(Entry::Load);
        let solved = self.repeated(Entry::Solve);
        assert_eq!(total_cost(&loaded), total_cost(&solved), "a load and a solve should answer the same outcome");

        let mut fresh = Network::new();
        fresh.load(&problem_of(&self.network)).expect("the network should be admitted");
        let from_star = fresh.solve().work;
        change.assert_started_from_the_kept_basis(loaded.work, from_star);

        CheckedPair {
            arc_count: self.network.arc_count(),
            total_cost: total_cost(&solved),
            loaded: loaded.work,
            solved: solved.work,
        }
    }

    /// The answer through `entry` from a clone of its kept state, which a second clone repeats, its work included.
    ///
    /// # Panics
    /// Panics as [`certified`](Self::certified) does, or if the second answer differs from the first.
    fn repeated(&mut self, entry: Entry) -> Answer<OptimalFlow> {
        let first = self.certified(entry);
        assert_eq!(self.certified(entry), first, "a second {} should repeat the first answer", entry.name());

        first
    }

    /// The answer through `entry` from a clone of its kept state, an optimal one proved optimal by the certificate.
    ///
    /// # Panics
    /// Panics if the network is refused, which the suite's generators and changes rule out, or with the first breach
    /// of the certificate.
    fn certified(&mut self, entry: Entry) -> Answer<OptimalFlow> {
        let Self { network, kept, flows } = self;
        let problem = problem_of(network);
        let answer = match entry {
            Entry::Load => {
                let mut held = kept.network.ready();
                held.load(&problem).expect("the network should be admitted");
                let Answer { outcome, work } = held.solve();
                let outcome = match (outcome, held.optimum()) {
                    (Outcome::Optimal(total_cost), Some(NetworkOptimum { flows, potentials, .. })) => {
                        Outcome::Optimal(OptimalFlow {
                            flows: flows.iter().collect(),
                            potentials: potentials.to_vec(),
                            total_cost,
                        })
                    }
                    (Outcome::Optimal(_), None) => unreachable!("a solve that found a flow should leave its optimum"),
                    (Outcome::Infeasible, _) => Outcome::Infeasible,
                };
                Answer { outcome, work }
            }
            Entry::Solve => {
                let mut solver = kept.solver.ready();
                let Answer { outcome, work } =
                    solver.solve_into(&problem, flows).expect("the network should be admitted");
                let outcome = match outcome {
                    // The flows are copied out of the buffer, which the next solve overwrites, and the potentials out
                    // of the solver, which lends them.
                    Outcome::Optimal(Optimum { total_cost, potentials }) => Outcome::Optimal(OptimalFlow {
                        flows: flows.to_vec(),
                        potentials: potentials.to_vec(),
                        total_cost,
                    }),
                    Outcome::Infeasible => Outcome::Infeasible,
                };
                Answer { outcome, work }
            }
        };

        if let Outcome::Optimal(optimum) = &answer.outcome {
            assert_certified(network, optimum);
        }

        answer
    }

    /// Times `entry`: each iteration one load of the network and the solve after it, or one solve of it, by a clone of
    /// the kept state made in the untimed setup. The clone is handed back with the work, so that it is dropped outside
    /// the timing too.
    ///
    /// # Panics
    /// Panics if the network is refused, which [`check`](Self::check) rules out.
    fn time(&mut self, bencher: &mut Bencher<'_>, entry: Entry) {
        let Self { network, kept, flows } = self;
        let problem = problem_of(network);

        // One clone, made just before its iteration: each solve runs on columns written a moment before, warm in the
        // cache, as a load runs on the network that just solved. A batch would make many clones first and solve most
        // of them after the later ones had pushed them out of the cache, more so the larger the network, so that rows
        // of different sizes would be timed in different states of the cache.
        match entry {
            Entry::Load => bencher.iter_batched(
                || kept.network.ready(),
                |mut held| {
                    held.load(&problem).expect("the network should be admitted");
                    let Answer { work, .. } = held.solve();
                    // The flows written out of the optimum into the buffer, as the solver's row writes them: the two
                    // rows of a pair answer the same output.
                    if let Some(optimum) = held.optimum() {
                        for (written, flow) in flows.iter_mut().zip(optimum.flows) {
                            *written = flow;
                        }
                    }
                    (work, held)
                },
                BatchSize::PerIteration,
            ),
            Entry::Solve => bencher.iter_batched(
                || kept.solver.ready(),
                |mut solver| {
                    let work = solver.solve_into(&problem, flows).expect("the network should be admitted").work;
                    (work, solver)
                },
                BatchSize::PerIteration,
            ),
        }
    }
}

impl CheckedPair {
    /// The line the suite prints for the pair `name`, whose change moved `moved`: the arcs, what moved, the outcome,
    /// and the pivots of each entry.
    fn line(self, name: &str, moved: Moved) -> String {
        let Self { arc_count, total_cost, loaded, solved } = self;
        let Moved { supplies, supply_units, capacities, costs } = moved;
        let outcome = match total_cost {
            Some(total_cost) => format!("optimal at {total_cost}"),
            None => "infeasible".to_owned(),
        };

        format!(
            "checked {name}: {arc_count} arcs, moved {supplies} supplies by {supply_units} units, {capacities} \
             capacities, {costs} costs; {outcome}; pivots: {} loading, {} solving",
            loaded.pivots, solved.pivots
        )
    }
}

/// The perturbation of `network` drawn from the suite's seed: the testing crate's `perturbed`, from the first outputs
/// of SplitMix64 seeded with [`CHANGE_SEED`], as many as a perturbation of that network reads at most, so that no two
/// of its draws share a number.
///
/// # Panics
/// Panics if `network`'s shape is broken, which the suite's generators rule out.
fn perturbation(network: &MincostNetwork) -> MincostNetwork {
    let mut numbers = SplitMix64::seed_from_u64(CHANGE_SEED);
    let seed: Vec<u64> =
        iter::repeat_with(|| numbers.next_u64()).take(perturbation_draws(network.arc_count())).collect();

    perturbed(network, &seed)
}

/// Checks and times every pair of rows of the suite, one after the other.
///
/// # Panics
/// Panics if a pair's checks fail before its timing.
fn warm(criterion: &mut Criterion) {
    let mut group = flat_group(criterion, "warm");

    for row in Row::all() {
        let kept = Kept::new(&row.network());

        for change in Change::ALL {
            let Some(ChangedNetwork { network, moved }) = change.applied(row) else { continue };
            let mut pair = WarmStart::new(network, &kept);
            let name = format!("{}/{}", row.name(), change.name());
            println!("{}", pair.check(change).line(&name, moved));

            group.throughput(arcs(pair.network.arc_count()));
            for entry in Entry::BOTH {
                group.bench_function(format!("{name}/{}", entry.name()), |bencher| pair.time(bencher, entry));
            }
        }
    }

    group.finish();
}

/// Runs the suite under criterion's settings, as the command line overrides them.
fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    warm(&mut criterion);
    criterion.final_summary();
}
