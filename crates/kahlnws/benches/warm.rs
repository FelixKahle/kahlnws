// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The benchmark suite of solves from a kept basis: for every row of the cold suite, its network again or changed,
//! solved by a resolve from the basis a solve of the row's own network ended on, and by a solve, so that the two stand
//! side by side on the same network.
//!
//! Rows `warm/<family>/<costs>/<size>/<change>/<entry>`: the row of the cold suite, how its network was changed, and
//! the entry timed, `resolve` or `solve`. The changes:
//!
//! - `same`: nothing changed, the network the kept basis ended on: what a resolve costs where it makes no pivot.
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
//! network around them; three is the most supply moves the testing crate's perturbation makes, and its units (three
//! for a supply, two for a capacity) are that perturbation's too. A redrawn cost is the one the arc takes in the
//! family's network with its costs drawn from another stream: each arc's cost drawn from the range of its role in the
//! family, a derived cost such as the grid's slack arc given by its rule, and only arcs whose cost the redraw moves are
//! chosen. Every change is made to the family's own network, and the row's costs, tying or distinct, are applied after
//! it, as to the row's own network; so a change of a distinct row moves costs, not only their tie-breaks. Every change
//! is drawn from one fixed seed, so every run of the suite makes the same changes. A change of costs leaves a network's
//! flow in place; with its supplies or capacities moved a network may have none, and the pair then times two answers
//! that no flow exists.
//!
//! Every iteration of a row starts from the same solver: one that solved the row's own network and kept the basis that
//! solve ended on, cloned in criterion's untimed setup and its room reserved for the changed network, so that nothing
//! is allocated inside the timing. A row times one [`NetworkSimplex::resolve_into`] or one
//! [`NetworkSimplex::solve_into`] of the changed network by that clone, into a buffer of flows made once, the problem
//! handed in made once too; so the work of an iteration is fixed by the two networks alone. Each iteration needs a
//! clone of its own, so the clock is read around each iteration: a row's time includes that reading, the same addend in
//! both rows of a pair, which shrinks the ratio of a pair whose times come near it. The throughput is in arcs.
//!
//! Before a pair of rows is timed, the change must have changed the network, unless it is `same`. Each entry runs
//! twice, from two clones: both runs must answer the same outcome, flows, potentials, cost and work, and an optimal
//! answer must pass the testing crate's certificate. The two entries must answer the same outcome and cost. The resolve
//! must try no arc before block search, which a start from the star does wherever a solve's first arcs pivot; and since
//! a resolve that cannot start from the kept basis answers exactly a solve's work, one whose work differs from the
//! solve's started from it. Where the two works are equal and neither tried an arc first, the work cannot tell the
//! starts apart, and the suite says so. On `same` the resolve must make no pivot: the kept basis is the problem's own
//! end. A network whose kept basis cannot serve it would stop the suite there; none of the suite's can, since their
//! supplies and capacities are far below the limit of the excesses.
//!
//! The suite then prints a line with the network's arcs, what the change moved, and what the checks found, the outcome
//! and each entry's pivots, such as `checked grid/tying/9/arcs: 235 arcs, moved 0 supplies by 0 units, 3 capacities,
//! 3 costs; optimal at 89305; pivots: 1 resolving, 249 solving`.
//!
//! Run with `cargo bench -p kahlnws --bench warm`; append `-- warm/grid/` (or any other prefix of a row's name) to run
//! part of it. Under `cargo test -p kahlnws --benches` the suite is a smoke run, which makes every pair's checks and
//! runs each row once, untimed.

mod rows;

use core::iter;

use criterion::{BatchSize, Bencher, Criterion};
use kahlnws::{Answer, NetworkSimplex, Optimum, Outcome, Problem, ProblemError, Work};
use kahlnws_testing::{
    MincostNetwork, OptimalFlow, capacities_moved, costs_taken, perturbation_draws, perturbed, supplies_moved,
};
use rand::{Rng, SeedableRng};
use rand_xoshiro::{SplitMix64, Xoshiro256PlusPlus};
use rows::{Row, arcs, assert_certified, flat_group, problem_of};

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

/// The entry of the solver a row times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// [`NetworkSimplex::resolve_into`], from the basis the solver kept.
    Resolve,
    /// [`NetworkSimplex::solve_into`], which reads no kept basis.
    Solve,
}

/// What the work of a pair shows of where the resolve started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    /// From the kept basis: its work differs from the solve's, which a start from the star answers exactly.
    Kept,
    /// Not told apart: the two works are equal and neither entry tried an arc before block search, as a kept start and
    /// a start from the star both may.
    NotToldApart,
}

/// What a change moved of a network, counted against the network before it. Plain data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Moved {
    /// The nodes whose supply changed.
    supplies: usize,
    /// The units the supplies moved: half the sum of the changes' magnitudes, so a move of one unit counts one.
    supply_units: u64,
    /// The arcs whose capacity changed.
    capacities: usize,
    /// The arcs whose cost changed.
    costs: usize,
}

/// A row's network changed: at the row's costs, with what the change moved of it.
#[derive(Debug)]
struct ChangedNetwork {
    /// The changed network at the row's costs.
    network: MincostNetwork,
    /// What the change moved, counted on the family's own costs.
    moved: Moved,
}

/// A solver that solved a row's own network, and the counts of that network.
///
/// # Invariant
/// `solver` ended a solve of a network of `node_count` nodes and `arc_count` arcs and keeps the basis it ended on.
#[derive(Debug)]
struct KeptSolver {
    /// The solver, with the basis it kept.
    solver: NetworkSimplex,
    /// The node count of the network it solved.
    node_count: usize,
    /// The arc count of the network it solved.
    arc_count: usize,
}

/// A pair of rows: a network changed from a row's own, and the solver every iteration starts from.
///
/// # Invariant
/// `kept` solved a network of the nodes and the arcs of `network`, each arc with its tail and its head; `flows` holds
/// one entry per arc of `network`.
#[derive(Debug)]
struct WarmStart<'k> {
    /// The changed network, which both entries solve.
    network: MincostNetwork,
    /// The solver that solved the row's own network.
    kept: &'k KeptSolver,
    /// The flow of each arc, written by every solve.
    flows: Vec<u64>,
}

/// What the checks of a pair found, which the suite prints before the pair is timed. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CheckedPair {
    /// The arcs of the network both entries solved.
    arc_count: usize,
    /// The cost both entries answered; `None` where both answered that no flow exists.
    total_cost: Option<i128>,
    /// The work of the resolve.
    resolved: Work,
    /// The work of the solve.
    solved: Work,
    /// Where the work shows the resolve started.
    start: Start,
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
    /// Panics if a change other than `same` leaves the network as it was.
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

        Some(ChangedNetwork { network: row.at_costs(changed), moved })
    }

    /// Checks what the work of a pair through this change shows: the resolve tried no arc before block search, and
    /// made no pivot where nothing changed; answers where it started.
    ///
    /// # Panics
    /// Panics if the resolve tried an arc before block search, as only a start from the star does, or made a pivot of
    /// the network its kept basis ended on, which that basis then could not serve.
    fn start(self, resolved: Work, solved: Work) -> Start {
        assert!(
            resolved.first_arc_pivots == 0,
            "the resolve should start from the kept basis, which tries no arc first: {resolved:?} against {solved:?}"
        );
        assert!(
            self != Self::Same || resolved.pivots == 0,
            "a resolve of the network the kept basis ended on should make no pivot: {resolved:?}"
        );

        // A resolve that cannot start from the kept basis answers exactly what a solve answers, its work included.
        if resolved == solved { Start::NotToldApart } else { Start::Kept }
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

impl Moved {
    /// What changed from `before` to `after`, two networks on the same nodes and arcs.
    fn between(before: &MincostNetwork, after: &MincostNetwork) -> Self {
        let supply_changes = before.supplies.iter().zip(&after.supplies).map(|(old, new)| old.abs_diff(*new));

        Self {
            supplies: changed(&before.supplies, &after.supplies),
            supply_units: supply_changes.sum::<u64>() / 2,
            capacities: changed(&before.capacities, &after.capacities),
            costs: changed(&before.costs, &after.costs),
        }
    }
}

impl Entry {
    /// Both entries, in the order a pair's rows are timed.
    const BOTH: [Self; 2] = [Self::Resolve, Self::Solve];

    /// The name of this entry in a row's name.
    const fn name(self) -> &'static str {
        match self {
            Self::Resolve => "resolve",
            Self::Solve => "solve",
        }
    }

    /// What `solver` answers for `problem` through this entry, each arc's flow written to `flows`.
    ///
    /// # Errors
    /// Returns the first breach of `problem`, as the entry does.
    ///
    /// # Panics
    /// Panics if `flows` holds another number of entries than `problem` has arcs.
    fn answer<'s>(
        self,
        solver: &'s mut NetworkSimplex,
        problem: &Problem<'_>,
        flows: &mut [u64],
    ) -> Result<Answer<Optimum<'s>>, ProblemError> {
        match self {
            Self::Resolve => solver.resolve_into(problem, flows),
            Self::Solve => solver.solve_into(problem, flows),
        }
    }
}

impl KeptSolver {
    /// A solver that solved `network` and keeps the basis that solve ended on.
    ///
    /// # Panics
    /// Panics if `network` is refused, which the suite's generators rule out.
    fn new(network: &MincostNetwork) -> Self {
        let mut solver = NetworkSimplex::new();
        solver.solve(&problem_of(network)).expect("the row's network should be admitted");

        Self { solver, node_count: network.node_count(), arc_count: network.arc_count() }
    }

    /// A clone of the solver, its room reserved for the network it solved: what an iteration starts from. It keeps
    /// the same basis, and neither entry allocates in a solve of a network of the same counts.
    ///
    /// # Panics
    /// Panics if the room cannot be reserved.
    fn ready(&self) -> NetworkSimplex {
        // A clone holds each column's room at that column's length, and a solve from the star leaves the room of the
        // excesses over a kept basis empty: the reservation makes it, and keeps the basis.
        let mut solver = self.solver.clone();
        solver.try_reserve(self.node_count, self.arc_count).expect("the room should be reserved");

        solver
    }
}

impl<'k> WarmStart<'k> {
    /// The pair of `network`, a network on the nodes and arcs of the network `kept` solved, each arc with its tail and
    /// its head.
    fn new(network: MincostNetwork, kept: &'k KeptSolver) -> Self {
        let flows = vec![0; network.arc_count()];

        Self { network, kept, flows }
    }

    /// Checks what the pair through `change` measures, and answers what it found: each entry answers twice alike, the
    /// certificate proves an optimal answer optimal, the two entries answer the same outcome and cost, and the work
    /// shows what [`Change::start`] checks of the resolve's start.
    ///
    /// # Panics
    /// Panics if a check fails, with what it found.
    fn check(&mut self, change: Change) -> CheckedPair {
        let resolved = self.repeated(Entry::Resolve);
        let solved = self.repeated(Entry::Solve);

        assert_eq!(total_cost(&resolved), total_cost(&solved), "a resolve and a solve should answer the same outcome");

        CheckedPair {
            arc_count: self.network.arc_count(),
            total_cost: total_cost(&solved),
            resolved: resolved.work,
            solved: solved.work,
            start: change.start(resolved.work, solved.work),
        }
    }

    /// The answer through `entry` from a clone of the kept solver, which a second clone repeats, its work included.
    ///
    /// # Panics
    /// Panics as [`certified`](Self::certified) does, or if the second answer differs from the first.
    fn repeated(&mut self, entry: Entry) -> Answer<OptimalFlow> {
        let first = self.certified(entry);
        assert_eq!(self.certified(entry), first, "a second {} should repeat the first answer", entry.name());

        first
    }

    /// The answer through `entry` from a clone of the kept solver, an optimal one proved optimal by the certificate.
    ///
    /// # Panics
    /// Panics if the network is refused, which the suite's generators and changes rule out, or with the first breach
    /// of the certificate.
    fn certified(&mut self, entry: Entry) -> Answer<OptimalFlow> {
        let Self { network, kept, flows } = self;
        let mut solver = kept.ready();
        let Answer { outcome, work } =
            entry.answer(&mut solver, &problem_of(network), flows).expect("the network should be admitted");

        let outcome = match outcome {
            Outcome::Optimal(Optimum { total_cost, potentials }) => {
                // The flows are copied out of the buffer, which the next solve overwrites, and the potentials out of
                // the solver, which lends them.
                let optimum = OptimalFlow { flows: flows.to_vec(), potentials: potentials.to_vec(), total_cost };
                assert_certified(network, &optimum);
                Outcome::Optimal(optimum)
            }
            Outcome::Infeasible => Outcome::Infeasible,
        };

        Answer { outcome, work }
    }

    /// Times `entry`: each iteration one solve of the network through it, by a clone of the kept solver made in the
    /// untimed setup. The clone is handed back with the work, so that it is dropped outside the timing too.
    ///
    /// # Panics
    /// Panics if the network is refused, which [`check`](Self::check) rules out.
    fn time(&mut self, bencher: &mut Bencher<'_>, entry: Entry) {
        let Self { network, kept, flows } = self;
        let problem = problem_of(network);

        bencher.iter_batched(
            || kept.ready(),
            |mut solver| {
                let work = entry.answer(&mut solver, &problem, flows).expect("the network should be admitted").work;
                (work, solver)
            },
            // One clone, made just before its iteration: each solve runs on columns written a moment before, warm in
            // the cache, as a resolve runs on the solver that just solved. A batch would make many clones first and
            // solve most of them after the later ones had pushed them out of the cache, more so the larger the
            // network, so that rows of different sizes would be timed in different states of the cache.
            BatchSize::PerIteration,
        );
    }
}

impl CheckedPair {
    /// The line the suite prints for the pair `name`, whose change moved `moved`: the arcs, what moved, the outcome,
    /// and the pivots of each entry, with a last clause where the work does not tell the starts apart.
    fn line(self, name: &str, moved: Moved) -> String {
        let Self { arc_count, total_cost, resolved, solved, start } = self;
        let Moved { supplies, supply_units, capacities, costs } = moved;
        let outcome = match total_cost {
            Some(total_cost) => format!("optimal at {total_cost}"),
            None => "infeasible".to_owned(),
        };
        let start = match start {
            Start::Kept => "",
            Start::NotToldApart => "; the work does not tell the start",
        };

        format!(
            "checked {name}: {arc_count} arcs, moved {supplies} supplies by {supply_units} units, {capacities} \
             capacities, {costs} costs; {outcome}; pivots: {} resolving, {} solving{start}",
            resolved.pivots, solved.pivots
        )
    }
}

/// How many entries of `after` differ from the entry at the same place of `before`.
fn changed<T: PartialEq>(before: &[T], after: &[T]) -> usize {
    before.iter().zip(after).filter(|(old, new)| old != new).count()
}

/// The cost of the flow `answer` holds; `None` where no flow exists.
fn total_cost(answer: &Answer<OptimalFlow>) -> Option<i128> {
    match &answer.outcome {
        Outcome::Optimal(optimum) => Some(optimum.total_cost),
        Outcome::Infeasible => None,
    }
}

/// The costs of `row`'s family's network with its costs drawn from a stream of their own: `draws`, the generator of
/// the change, jumped ahead by `2^128` draws, so that the two never draw the same numbers.
fn redrawn_costs(row: Row, draws: &Xoshiro256PlusPlus) -> Vec<i64> {
    let mut costs = draws.clone();
    costs.jump();

    row.family.network_with_costs(row.size, costs).costs
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
        let kept = KeptSolver::new(&row.network());

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
