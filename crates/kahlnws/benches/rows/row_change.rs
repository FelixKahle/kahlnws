// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the suites that change a row's network share: what a change moved of it, the costs a change redraws from, the
//! row's costs applied to a changed network, the network and the solver that solved the network before the change,
//! which every iteration starts from, and the cost an answer holds.

use kahlnws::{Answer, Network, NetworkSimplex, Outcome};
use kahlnws_testing::{MincostNetwork, OptimalFlow};
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::sweep::{Row, problem_of};

/// What a change moved of a network, counted against the network before it. Plain data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Moved {
    /// The nodes whose supply changed.
    pub(crate) supplies: usize,
    /// The units the supplies moved: half the sum of the changes' magnitudes, so a move of one unit counts one.
    pub(crate) supply_units: u64,
    /// The arcs whose capacity changed.
    pub(crate) capacities: usize,
    /// The arcs whose cost changed.
    pub(crate) costs: usize,
}

/// A solver whose room holds a network, made by a solve of it, and the counts of that network: room, and nothing else
/// a solver keeps.
///
/// # Invariant
/// `solver` ended a solve of a network of `node_count` nodes and `arc_count` arcs, so its room holds such a network.
#[derive(Debug)]
pub(crate) struct SolverRoom {
    /// The solver, with the room of the network it solved.
    solver: NetworkSimplex,
    /// The node count of the network it solved.
    node_count: usize,
    /// The arc count of the network it solved.
    arc_count: usize,
}

/// A network that loaded a network and solved it, holding the basis its solve ended on, and the counts of that network.
///
/// # Invariant
/// `network` holds a network of `node_count` nodes and `arc_count` arcs and was solved last.
#[derive(Clone, Debug)]
pub(crate) struct KeptNetwork {
    /// The network, as its solve left it.
    network: Network,
    /// The node count of the network it holds.
    node_count: usize,
    /// The arc count of the network it holds.
    arc_count: usize,
}

/// The two states an iteration starts from: a network and a solver that each solved one network.
#[derive(Debug)]
pub(crate) struct Kept {
    /// The network that solved it, with the basis it holds.
    pub(crate) network: KeptNetwork,
    /// The solver that solved it, whose room holds it.
    pub(crate) solver: SolverRoom,
}

impl Moved {
    /// What changed from `before` to `after`, two networks on the same nodes and arcs.
    pub(crate) fn between(before: &MincostNetwork, after: &MincostNetwork) -> Self {
        let supply_changes = before.supplies.iter().zip(&after.supplies).map(|(old, new)| old.abs_diff(*new));

        Self {
            supplies: changed(&before.supplies, &after.supplies),
            supply_units: supply_changes.sum::<u64>() / 2,
            capacities: changed(&before.capacities, &after.capacities),
            costs: changed(&before.costs, &after.costs),
        }
    }
}

impl SolverRoom {
    /// A solver whose room a solve of `network` made.
    ///
    /// # Panics
    /// Panics if `network` is refused, which the suite's generators rule out.
    pub(crate) fn new(network: &MincostNetwork) -> Self {
        let mut solver = NetworkSimplex::new();
        solver.solve(&problem_of(network)).expect("the row's network should be admitted");

        Self { solver, node_count: network.node_count(), arc_count: network.arc_count() }
    }

    /// A clone of the solver, its room reserved for the network it solved: what an iteration starts from. A solve of a
    /// network of the same counts allocates nothing in it, and runs on columns the clone wrote a moment before.
    ///
    /// # Panics
    /// Panics if the room cannot be reserved.
    pub(crate) fn ready(&self) -> NetworkSimplex {
        // A clone holds each column's room at that column's length: the reservation makes the rest.
        let mut solver = self.solver.clone();
        solver.try_reserve(self.node_count, self.arc_count).expect("the room should be reserved");

        solver
    }
}

impl KeptNetwork {
    /// A network that loaded `network` and solved it.
    ///
    /// # Panics
    /// Panics if `network` is refused, which the suite's generators rule out.
    pub(crate) fn new(network: &MincostNetwork) -> Self {
        let mut held = Network::new();
        held.load(&problem_of(network)).expect("the row's network should be admitted");
        let _ = held.solve();

        Self { network: held, node_count: network.node_count(), arc_count: network.arc_count() }
    }

    /// A clone of the network, its room reserved for the network it holds: what an iteration starts from. It holds
    /// the same basis and marks, and neither a change of a number, nor a load of a network of the same counts, nor a
    /// solve allocates in it.
    ///
    /// # Panics
    /// Panics if the room cannot be reserved.
    pub(crate) fn ready(&self) -> Network {
        // A clone holds each column's room at that column's length, the record of the subtrees a change moves and the
        // excesses a load reads empty: the reservation makes their room.
        let mut network = self.network.clone();
        network.try_reserve(self.node_count, self.arc_count).expect("the room should be reserved");

        network
    }
}

impl Kept {
    /// The network and the solver that each solved `network`.
    ///
    /// # Panics
    /// Panics if `network` is refused, which the suite's generators rule out.
    pub(crate) fn new(network: &MincostNetwork) -> Self {
        Self { network: KeptNetwork::new(network), solver: SolverRoom::new(network) }
    }
}

/// How many entries of `after` differ from the entry at the same place of `before`.
fn changed<T: PartialEq>(before: &[T], after: &[T]) -> usize {
    before.iter().zip(after).filter(|(old, new)| old != new).count()
}

/// The cost of the flow `answer` holds; `None` where no flow exists.
pub(crate) fn total_cost(answer: &Answer<OptimalFlow>) -> Option<i128> {
    match &answer.outcome {
        Outcome::Optimal(optimum) => Some(optimum.total_cost),
        Outcome::Infeasible => None,
    }
}

/// `network`, a network of `row`'s family on the arcs of `own`, the family's network of the row, at the row's costs:
/// each cost made as the row makes the costs of `own`, from the capacities of `own`. At distinct costs a cost is
/// scaled by a multiplier the capacities give; taking it from `own` leaves every cost a change does not name as the
/// row's own network has it.
pub(crate) fn at_row_costs(row: Row, own: &MincostNetwork, network: MincostNetwork) -> MincostNetwork {
    let scaled = MincostNetwork { capacities: own.capacities.clone(), ..network.clone() };

    MincostNetwork { costs: row.at_costs(scaled).costs, ..network }
}

/// The costs of `row`'s family's network with its costs drawn from a stream of their own: `draws`, the generator of
/// the change, jumped ahead by `2^128` draws, so that the two never draw the same numbers.
pub(crate) fn redrawn_costs(row: Row, draws: &Xoshiro256PlusPlus) -> Vec<i64> {
    let mut costs = draws.clone();
    costs.jump();

    row.family.network_with_costs(row.size, costs).costs
}
