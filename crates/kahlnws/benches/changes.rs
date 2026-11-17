// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The benchmark suite of changes to a held network: for every row of the cold suite, a [`Network`] that holds the
//! row's network and its last optimum, changed by a few numbers and solved again, beside the same changed network
//! loaded whole into that network and solved, and solved by a solver.
//!
//! Rows `changes/<family>/<costs>/<size>/<change>/<entry>`: the row of the cold suite, the change, and the entry
//! timed. The changes, each named for what it moves:
//!
//! - `same`: nothing; the network solved again as its last solve left it: what a network's solve costs where nothing
//!   changed, the floor.
//! - `cost`: the cost of one arc, redrawn.
//! - `switch-off`: one arc that carries flow in the network's optimum switched off, its capacity set to zero.
//! - `switch-on`: the arc `switch-off` switches off switched on again, its capacity set back, on the network solved
//!   without it.
//! - `supply-pair`: one amount of one to three units moved from one node's supply to another's of the same role, a
//!   supplier's supply to another supplier or a demander's demand to another demander. The grid has one supplier and
//!   one demander, so its rows have no such change.
//! - `costs-3`, `switch-offs-3` and `supply-pairs-3`: three of each, the same count on every network.
//!
//! A redrawn cost is the one the arc takes in the family's network with its costs drawn from another stream, as the
//! warm suite draws it, and only arcs whose cost the redraw moves are chosen; an arc switched off is drawn among the
//! arcs of lower bound zero that carry flow in the optimum the network holds; an amount moved is drawn as the warm
//! suite draws a supply move. Every change is made to the family's own network, and the row's costs are applied after
//! it as to the row's own network, the distinct costs with the multiplier of the row's own network, which a capacity
//! changes, so that a change moves the numbers it names and no other; every change is drawn from one fixed seed, so
//! every run makes the same changes. A network may have no flow once an arc is switched off or a supply moved; its rows
//! then time answers that no flow exists.
//!
//! The entries, each timing one change and the solve after it:
//!
//! - `network`: a [`Network`] that held the network before the change and its optimum, cloned in criterion's untimed
//!   setup and its room reserved, given the change by its setters and [`Network::move_supply`], whose supplies are
//!   moved in pairs, and solved.
//! - `load`: the same clone given the changed network whole by [`Network::load`], which starts from the basis it
//!   holds, and solved.
//! - `solve`: a [`NetworkSimplex`] that solved the network before the change, cloned and reserved likewise, given the
//!   changed network by [`NetworkSimplex::solve_into`], from the star.
//!
//! So the network's row times the calls that change it with its solve, and the rows of the load and the solve time a
//! solve of a problem handed in whole, made once outside the timing. Each iteration needs a clone of its own, so the
//! clock is read around each one: a row's time includes that reading. Nothing is allocated inside the timing. The
//! throughput is in arcs.
//!
//! Before the rows of a change are timed, the change must have changed the network, unless it is `same`; each entry
//! runs twice, from two clones, and must answer alike, its work included; an optimal answer must pass the testing
//! crate's certificate; and the three entries must answer the same outcome and cost. Where nothing changed, neither
//! the network's changes nor the load may make a pivot. The suite prints a line with the network's arcs, what the
//! change moved and in how many calls, the outcome and each entry's pivots, such as `checked grid/tying/9/cost: 235
//! arcs, moved 0 supplies by 0 units, 0 capacities, 1 costs in 1 calls; optimal at 89361; pivots: 1 network, 1
//! loading, 238 solving`.
//!
//! Run with `cargo bench -p kahlnws --bench changes`; append `-- changes/grid/` (or any other prefix of a row's name)
//! to run part of it. Under `cargo test -p kahlnws --benches` the suite is a smoke run, which makes every change's
//! checks and runs each row once, untimed.

#[path = "rows/row_change.rs"]
mod row_change;
#[path = "rows/sweep.rs"]
mod sweep;

use criterion::{BatchSize, Bencher, Criterion};
use kahlnws::{Answer, ChangeError, Network, NetworkOptimum, Optimum, Outcome, Work};
use kahlnws_testing::{MincostNetwork, OptimalFlow, costs_taken, supplies_moved};
use rand::SeedableRng;
use rand::seq::SliceRandom;
use rand_xoshiro::Xoshiro256PlusPlus;
use row_change::{Kept, Moved, at_row_costs, redrawn_costs, total_cost};
use sweep::{Row, arcs, assert_certified, flat_group, problem_of};

/// The seed every change is drawn from: any value serves, and it is fixed so that every run of the suite makes the
/// same changes to every network.
const CHANGE_SEED: u64 = 0x6368_616e_6765_7321;

/// How many numbers of each kind a change of a few moves: three, the same on every network, as the warm suite's few.
const FEW: usize = 3;

/// The most units a supply move takes: three, as the warm suite's supply moves take.
const SUPPLY_UNITS: u64 = 3;

/// How a row's network is changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    /// Nothing: the network solved again.
    Same,
    /// The costs of this many arcs, redrawn.
    Costs(usize),
    /// This many arcs that carry flow switched off.
    SwitchOffs(usize),
    /// The arc one switch-off switches off, switched on again from the network solved without it.
    SwitchOn,
    /// This many supply moves, each of one to [`SUPPLY_UNITS`] units between two nodes of one role.
    SupplyPairs(usize),
}

/// The entry a row times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// The network's setters and moves, then its solve.
    Network,
    /// [`Network::load`] of the changed network whole, from the basis the network holds, then its solve.
    Load,
    /// [`NetworkSimplex::solve_into`](kahlnws::NetworkSimplex::solve_into) of the changed network, from the star.
    Solve,
}

/// One call of a change on a network. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    /// `set_cost` of `arc` to `cost`.
    Cost {
        /// The arc.
        arc: usize,
        /// Its new cost, in units of cost.
        cost: i64,
    },
    /// `set_capacity` of `arc` to `capacity`.
    Capacity {
        /// The arc.
        arc: usize,
        /// Its new capacity, in units of flow.
        capacity: u64,
    },
    /// `move_supply` of `amount` from `from_node` to `to_node`.
    Supply {
        /// The node whose supply falls.
        from_node: usize,
        /// The node whose supply rises.
        to_node: usize,
        /// The units moved.
        amount: u64,
    },
}

/// A change of a row's network, from the network before it to the one after it, both at the row's costs. Plain data.
#[derive(Clone, Debug)]
struct ChangedPair {
    /// The network before the change.
    before: MincostNetwork,
    /// The network after it, which every entry answers for.
    after: MincostNetwork,
    /// The calls that change a network that holds `before` into one that holds `after`.
    calls: Vec<Call>,
    /// What the change moved, counted on the family's own costs.
    moved: Moved,
}

/// The rows of one change: the change, the states it starts from, and a buffer of flows for the solver's entries.
///
/// # Invariant
/// `kept` solved `pair.before`, a network of the nodes and arcs of `pair.after`, each arc with its tail and head;
/// `flows` holds one entry per arc of `pair.after`.
#[derive(Debug)]
struct ChangeRows<'k> {
    /// The change.
    pair: ChangedPair,
    /// The states it starts from.
    kept: &'k Kept,
    /// The flow of each arc, written by every solve of a solver.
    flows: Vec<u64>,
}

/// What the checks of a change found, which the suite prints before its rows are timed. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CheckedChange {
    /// The arcs of the network every entry solved.
    arc_count: usize,
    /// The cost every entry answered; `None` where each answered that no flow exists.
    total_cost: Option<i128>,
    /// The work of the network's solve after its changes.
    network: Work,
    /// The work of the network's solve after the load.
    loaded: Work,
    /// The work of the solver's solve.
    solved: Work,
}

impl Change {
    /// Every change, in the order its rows run.
    const ALL: [Self; 8] = [
        Self::Same,
        Self::Costs(1),
        Self::SwitchOffs(1),
        Self::SwitchOn,
        Self::SupplyPairs(1),
        Self::Costs(FEW),
        Self::SwitchOffs(FEW),
        Self::SupplyPairs(FEW),
    ];

    /// The name of this change in a row's name.
    fn name(self) -> String {
        match self {
            Self::Same => "same".to_owned(),
            Self::Costs(1) => "cost".to_owned(),
            Self::Costs(count) => format!("costs-{count}"),
            Self::SwitchOffs(1) => "switch-off".to_owned(),
            Self::SwitchOffs(count) => format!("switch-offs-{count}"),
            Self::SwitchOn => "switch-on".to_owned(),
            Self::SupplyPairs(1) => "supply-pair".to_owned(),
            Self::SupplyPairs(count) => format!("supply-pairs-{count}"),
        }
    }

    /// `row`'s network changed so, drawn from the suite's seed, the arcs to switch off among those that carry flow in
    /// `optimum`, the row's network's: the change made to the family's own network, then the row's costs applied;
    /// `None` where the network has no such change, as the grid has no two nodes of one role to move a supply between,
    /// and a network with no flow, or none on an arc it may switch off, has no arc to switch.
    ///
    /// # Panics
    /// Panics if a change other than `same` leaves the network as it was.
    fn applied(self, row: Row, optimum: Option<&[u64]>) -> Option<ChangedPair> {
        let own = row.family.network(row.size);
        let mut draws = Xoshiro256PlusPlus::seed_from_u64(CHANGE_SEED);

        let (before, after) = match self {
            Self::Same => (own.clone(), own.clone()),
            Self::Costs(count) => {
                let changed = costs_taken(&own, &redrawn_costs(row, &draws), count, &mut draws);
                (own.clone(), changed)
            }
            Self::SwitchOffs(count) => (own.clone(), switched_off(&own, optimum?, count, &mut draws)?),
            Self::SwitchOn => (switched_off(&own, optimum?, 1, &mut draws)?, own.clone()),
            Self::SupplyPairs(count) => (own.clone(), supplies_moved(&own, count, SUPPLY_UNITS, &mut draws)?),
        };

        let moved = Moved::between(&before, &after);
        assert!(
            self == Self::Same || moved != Moved::default(),
            "the change {} should change the network of {}",
            self.name(),
            row.name()
        );
        let (before, after) = (at_row_costs(row, &own, before), at_row_costs(row, &own, after));
        let calls = calls_between(&before, &after);

        Some(ChangedPair { before, after, calls, moved })
    }
}

impl Entry {
    /// Every entry, in the order a change's rows are timed.
    const ALL: [Self; 3] = [Self::Network, Self::Load, Self::Solve];

    /// The name of this entry in a row's name.
    const fn name(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Load => "load",
            Self::Solve => "solve",
        }
    }
}

impl Call {
    /// Makes this call on `network`.
    ///
    /// # Errors
    /// Returns the refusal of the setter or the move.
    ///
    /// # Panics
    /// Panics if the call names an arc or a node past the network's counts.
    fn made(self, network: &mut Network) -> Result<(), ChangeError> {
        match self {
            Self::Cost { arc, cost } => network.set_cost(arc, cost).map(drop),
            Self::Capacity { arc, capacity } => network.set_capacity(arc, capacity).map(drop),
            Self::Supply { from_node, to_node, amount } => network.move_supply(from_node, to_node, amount),
        }
    }
}

impl<'k> ChangeRows<'k> {
    /// The rows of `pair`, starting from `kept`, which solved its network before the change.
    fn new(pair: ChangedPair, kept: &'k Kept) -> Self {
        let flows = vec![0; pair.after.arc_count()];

        Self { pair, kept, flows }
    }

    /// Checks what the rows of `change` measure, and answers what it found: each entry answers twice alike, the
    /// certificate proves an optimal answer optimal, the three answer the same outcome and cost, and where nothing
    /// changed neither the network's changes nor the load pivots.
    ///
    /// # Panics
    /// Panics if a check fails, with what it found.
    fn check(&mut self, change: Change) -> CheckedChange {
        let network = self.repeated(Entry::Network);
        let loaded = self.repeated(Entry::Load);
        let solved = self.repeated(Entry::Solve);

        let costs = [&network, &loaded, &solved].map(total_cost);
        assert!(costs.iter().all(|&cost| cost == costs[0]), "the three entries should answer alike: {costs:?}");
        assert!(
            change != Change::Same || (network.work.pivots, loaded.work.pivots) == (0, 0),
            "a network's changes and a load of nothing changed should make no pivot: {:?}, {:?}",
            network.work,
            loaded.work
        );

        CheckedChange {
            arc_count: self.pair.after.arc_count(),
            total_cost: costs[0],
            network: network.work,
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
    /// Panics if a call of the change or the changed network is refused, which the suite's changes rule out, or with
    /// the first breach of the certificate.
    fn certified(&mut self, entry: Entry) -> Answer<OptimalFlow> {
        let answer = match entry {
            Entry::Network => {
                let mut network = self.kept.network.ready();
                changed(&mut network, &self.pair.calls);
                owned(&mut network)
            }
            Entry::Load => {
                let mut network = self.kept.network.ready();
                network.load(&problem_of(&self.pair.after)).expect("the changed network should be admitted");
                owned(&mut network)
            }
            Entry::Solve => self.solver_answer(),
        };

        if let Outcome::Optimal(optimum) = &answer.outcome {
            assert_certified(&self.pair.after, optimum);
        }

        answer
    }

    /// The answer of the solver's solve from a clone of the solver that solved the network before the change, its
    /// flows copied out.
    ///
    /// # Panics
    /// Panics if the changed network is refused, which the suite's changes rule out.
    fn solver_answer(&mut self) -> Answer<OptimalFlow> {
        let Self { pair, kept, flows } = self;
        let mut solver = kept.solver.ready();
        let answer = solver.solve_into(&problem_of(&pair.after), flows);
        let Answer { outcome, work } = answer.expect("the changed network should be admitted");

        let outcome = match outcome {
            // The flows are copied out of the buffer, which the next solve overwrites, and the potentials out of the
            // solver, which lends them.
            Outcome::Optimal(Optimum { total_cost, potentials }) => {
                Outcome::Optimal(OptimalFlow { flows: flows.to_vec(), potentials: potentials.to_vec(), total_cost })
            }
            Outcome::Infeasible => Outcome::Infeasible,
        };

        Answer { outcome, work }
    }

    /// Times `entry`: each iteration one change and the solve after it, from a clone of the kept state made in the
    /// untimed setup. The clone is handed back with the work, so that it is dropped outside the timing too.
    ///
    /// # Panics
    /// Panics if a call of the change or the changed network is refused, which [`check`](Self::check) rules out.
    fn time(&mut self, bencher: &mut Bencher<'_>, entry: Entry) {
        let Self { pair, kept, flows } = self;
        let problem = problem_of(&pair.after);

        // One clone, made just before its iteration: each solve runs on columns written a moment before, warm in the
        // cache, as a search's solve runs on the network or the solver that just solved.
        match entry {
            Entry::Network => bencher.iter_batched(
                || kept.network.ready(),
                |mut network| {
                    changed(&mut network, &pair.calls);
                    let work = network.solve().work;
                    (work, network)
                },
                BatchSize::PerIteration,
            ),
            Entry::Load => bencher.iter_batched(
                || kept.network.ready(),
                |mut network| {
                    network.load(&problem).expect("the changed network should be admitted");
                    let work = network.solve().work;
                    (work, network)
                },
                BatchSize::PerIteration,
            ),
            Entry::Solve => bencher.iter_batched(
                || kept.solver.ready(),
                |mut solver| {
                    let answer = solver.solve_into(&problem, flows);
                    (answer.expect("the changed network should be admitted").work, solver)
                },
                BatchSize::PerIteration,
            ),
        }
    }
}

impl CheckedChange {
    /// The line the suite prints for the change `name`, which moved `moved` in `calls` calls: the arcs, what moved,
    /// the outcome, and the pivots of each entry.
    fn line(self, name: &str, moved: Moved, calls: usize) -> String {
        let Self { arc_count, total_cost, network, loaded, solved } = self;
        let Moved { supplies, supply_units, capacities, costs } = moved;
        let outcome = match total_cost {
            Some(total_cost) => format!("optimal at {total_cost}"),
            None => "infeasible".to_owned(),
        };

        format!(
            "checked {name}: {arc_count} arcs, moved {supplies} supplies by {supply_units} units, {capacities} \
             capacities, {costs} costs in {calls} calls; {outcome}; pivots: {} network, {} loading, {} solving",
            network.pivots, loaded.pivots, solved.pivots
        )
    }
}

/// The answer of a solve of `network`, with the optimum it then holds copied out, owned.
fn owned(network: &mut Network) -> Answer<OptimalFlow> {
    let Answer { outcome, work } = network.solve();
    let outcome = match (outcome, network.optimum()) {
        (Outcome::Optimal(total_cost), Some(NetworkOptimum { flows, potentials, .. })) => {
            Outcome::Optimal(OptimalFlow { flows: flows.iter().collect(), potentials: potentials.to_vec(), total_cost })
        }
        (Outcome::Optimal(_), None) => unreachable!("a solve that found a flow should leave its optimum"),
        (Outcome::Infeasible, _) => Outcome::Infeasible,
    };

    Answer { outcome, work }
}

/// Makes every call of `calls` on `network`.
///
/// # Panics
/// Panics if a call is refused, which the suite's changes, each well within the ranges, rule out.
fn changed(network: &mut Network, calls: &[Call]) {
    for &call in calls {
        call.made(network).expect("a change of the suite should be admitted");
    }
}

/// `network` with `count` arcs switched off, their capacities set to zero: drawn alike among its arcs of lower bound
/// zero that carry flow in `optimum`, one flow per arc; `None` where fewer than `count` do.
fn switched_off(
    network: &MincostNetwork,
    optimum: &[u64],
    count: usize,
    draws: &mut Xoshiro256PlusPlus,
) -> Option<MincostNetwork> {
    let mut carrying: Vec<usize> =
        (0..network.arc_count()).filter(|&arc| network.lower_bounds[arc] == 0 && optimum[arc] > 0).collect();
    if carrying.len() < count {
        return None;
    }

    let (chosen, _) = carrying.partial_shuffle(draws, count);
    let mut capacities = network.capacities.clone();
    for &arc in chosen.iter() {
        capacities[arc] = 0;
    }

    Some(MincostNetwork { capacities, ..network.clone() })
}

/// The calls that change a network that holds `before` into one that holds `after`, two networks on the same arcs
/// and lower bounds: each cost that differs set, then each capacity, then the supplies moved in pairs, each node whose
/// supply rises paired, in ascending order, with each whose supply falls, by the lesser of what the two have left to
/// move, as a caller holding new supplies whole moves them.
fn calls_between(before: &MincostNetwork, after: &MincostNetwork) -> Vec<Call> {
    let arcs = 0..after.arc_count();
    let costs = arcs.clone().filter(|&arc| before.costs[arc] != after.costs[arc]);
    let capacities = arcs.filter(|&arc| before.capacities[arc] != after.capacities[arc]);
    let mut calls: Vec<Call> = costs.map(|arc| Call::Cost { arc, cost: after.costs[arc] }).collect();
    calls.extend(capacities.map(|arc| Call::Capacity { arc, capacity: after.capacities[arc] }));

    // Each supply's change, in an `i128`, where a difference of two `i64` fits.
    let mut left: Vec<i128> =
        before.supplies.iter().zip(&after.supplies).map(|(&old, &new)| i128::from(new) - i128::from(old)).collect();
    let (mut rising, mut falling) = (0, 0);
    // Each move ends what one of the two has left to move, so at most one move a node.
    while let (Some(to_node), Some(from_node)) =
        ((rising..left.len()).find(|&node| left[node] > 0), (falling..left.len()).find(|&node| left[node] < 0))
    {
        let amount = left[to_node].min(-left[from_node]);
        left[to_node] -= amount;
        left[from_node] += amount;
        (rising, falling) = (to_node, from_node);
        let amount = u64::try_from(amount).expect("a move between two supplies should fit a u64");
        calls.push(Call::Supply { from_node, to_node, amount });
    }

    calls
}

/// Checks and times the rows of every change of every row of the suite, one after the other.
///
/// # Panics
/// Panics if a change's checks fail before its timing.
fn changes(criterion: &mut Criterion) {
    let mut group = flat_group(criterion, "changes");

    for row in Row::all() {
        let own = row.network();
        let kept = Kept::new(&own);
        // The flow of each arc in the optimum the network holds, read from a copy; `None` where its solve found none.
        let optimum: Option<Vec<u64>> = kept.network.ready().optimum().map(|optimum| optimum.flows.iter().collect());

        for change in Change::ALL {
            let Some(pair) = change.applied(row, optimum.as_deref()) else { continue };
            // A change from another network than the row's own starts from states that solved that one.
            let from_other = (pair.before != own).then(|| Kept::new(&pair.before));
            let (moved, calls) = (pair.moved, pair.calls.len());
            let mut rows = ChangeRows::new(pair, from_other.as_ref().unwrap_or(&kept));
            let name = format!("{}/{}", row.name(), change.name());
            println!("{}", rows.check(change).line(&name, moved, calls));

            group.throughput(arcs(rows.pair.after.arc_count()));
            for entry in Entry::ALL {
                group.bench_function(format!("{name}/{}", entry.name()), |bencher| rows.time(bencher, entry));
            }
        }
    }

    group.finish();
}

/// Runs the suite under criterion's settings, as the command line overrides them.
fn main() {
    let mut criterion = Criterion::default().configure_from_args();
    changes(&mut criterion);
    criterion.final_summary();
}
