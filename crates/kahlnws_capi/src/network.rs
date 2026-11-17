// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The network C holds a pointer to, and what its reservation, its load, its changes, its solve and the reads of its
//! optimum do once every pointer has been read.

use core::mem::MaybeUninit;

use kahlnws::{Answer, Bounds, ChangeError, Outcome};

use crate::halves::Int128;
use crate::problem::LentProblem;
use crate::refusal::Refusal;
use crate::status::Status;
use crate::work::Work;

/// A network: a state that holds one min-cost flow problem and a basis of it, which its changes, loads and solves
/// carry on from.
///
/// Opaque: C holds it only by the pointer `kahlnws_network_new` or `kahlnws_network_clone` answers, passes that pointer
/// to the other entries, and gives it back to `kahlnws_network_free`. `kahlnws_network_load` hands it a problem, which
/// it copies; `kahlnws_network_set_cost`, `kahlnws_network_set_capacity`, `kahlnws_network_set_lower_bound` and
/// `kahlnws_network_set_bounds` change one arc of the problem held, `kahlnws_network_set_supply` sets the supply of one
/// node, balanced by another, and `kahlnws_network_move_supply` moves supply from one node to another, each at once and
/// only where the change reaches; `kahlnws_network_solve` solves the problem as it then stands, and the reads
/// `kahlnws_network_total_cost`, `kahlnws_network_flow`, `kahlnws_network_flows` and `kahlnws_network_potentials` read
/// the optimum it found. The reads `kahlnws_network_tail`, `_head`, `_cost`, `_lower_bound`, `_capacity`, `_bounds`,
/// `_supply` and their columns read the problem as it stands, which needs no optimum and ends none.
/// `kahlnws_network_copy` makes one network a copy of another.
///
/// Beside the solver. The solver answers the problem each call hands it, from nothing kept. A network holds its
/// problem and a basis of it: a change costs what it moves, a load of new numbers on the same arcs carries over the
/// basis held, and a solve after either makes the pivots they call for. A solve that makes no pivot prices only the
/// arcs the changes may have made worth using, none for a move of supply the flow held carries; a solve that pivots
/// prices at least one whole pass over the arcs. Where a change leaves much flow still to be sent, so that the solve
/// makes many pivots, a network's solve can take longer than a load of the whole problem as it stands and the solve
/// after it.
///
/// A network never loaded holds the problem of no node and no arc: its solve answers the optimum of no flow at the cost
/// zero, and no arc or node is below its counts. Each setter answers the number it replaced, so setting that number
/// back, or moving the supply back, makes the problem the earlier one again: the next solve answers its status and its
/// cost, with a flow of that cost, which among tied optima may be another than before. A capacity of zero on an arc
/// whose lower bound is zero switches it off, and the capacity it answered switches it on again. A change that is
/// refused, or that sets the number already held, changes nothing, and keeps the optimum the network held; every
/// other change ends it, as do a load and a solve that finds no flow: the reads then answer
/// `KAHLNWS_STATUS_NO_OPTIMUM` until a solve finds one, and never a number of an earlier state.
///
/// Its answers are a function of the problem loaded and of the calls made since, in order: two networks given the
/// same calls answer alike, their work included. The status and the cost are those of the problem as it stands, the
/// potentials prove the flow optimal, and the flow among tied optima, the potentials and the work are the network's
/// own. Where a solver admits the problem as it stands, it answers the same status and cost. After moves of supply
/// and changes of lower bounds a network may hold numbers a solver and a load refuse: every supply stays within
/// `int64_t`, but the supplies net of the lower bounds may pass the limit
/// `KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE` names, where the arcs carry the difference; the network still solves
/// them exactly.
///
/// After a call that answered `KAHLNWS_STATUS_INTERNAL_ERROR`, which only a defect of the library makes, the network
/// may be half changed. An arc or a node not below the counts of the problem held before is still answered as
/// `KAHLNWS_STATUS_INVALID_ARGUMENT` first; beyond that, every change, `kahlnws_network_solve` and every read of the
/// problem answer `KAHLNWS_STATUS_INTERNAL_ERROR`, the reads of the optimum `KAHLNWS_STATUS_NO_OPTIMUM`, the counts
/// are those of the problem held before, `kahlnws_network_reserve` makes room, a copy is as half changed, and
/// `kahlnws_network_load` mends it.
///
/// Room. Only `kahlnws_network_reserve`, `kahlnws_network_load` and `kahlnws_network_copy` allocate, the load and the
/// copy not for a problem within the room held. `kahlnws_network_clone` allocates the copy, whose changes and solves
/// allocate nothing.
///
/// One thread at a time uses a network: no two calls that take the same network may run at once. A network may move
/// from one thread to another between calls, and distinct networks may be used on distinct threads at once.
#[derive(Debug)]
pub struct Network {
    /// The network of the library, with its problem, its basis and its room.
    held: kahlnws::Network,
}

/// What a read of a network's optimum writes, and where: the one argument [`Network::read_optimum`] takes.
#[derive(Debug)]
pub(crate) enum OptimumRead<'a> {
    /// The cost of the flow.
    TotalCost(&'a mut MaybeUninit<Int128>),
    /// The flow of one arc.
    Flow {
        /// The arc whose flow is read.
        arc: usize,
        /// Where its flow goes.
        flow: &'a mut MaybeUninit<u64>,
    },
    /// The flow of every arc, arc `a` at `a`: one entry per arc of the problem held.
    Flows(&'a mut [MaybeUninit<u64>]),
    /// The potential of every node, node `v` at `v`: one entry per node of the problem held.
    Potentials(&'a mut [MaybeUninit<i64>]),
}

/// What a read of the problem a network holds writes, and where: the one argument [`Network::read_problem`] takes.
#[derive(Debug)]
pub(crate) enum ProblemRead<'a> {
    /// The node an arc leaves.
    Tail {
        /// The arc read.
        arc: usize,
        /// Where its tail goes.
        tail: &'a mut MaybeUninit<usize>,
    },
    /// The node an arc enters.
    Head {
        /// The arc read.
        arc: usize,
        /// Where its head goes.
        head: &'a mut MaybeUninit<usize>,
    },
    /// The cost of a unit of flow on an arc.
    Cost {
        /// The arc read.
        arc: usize,
        /// Where its cost goes.
        cost: &'a mut MaybeUninit<i64>,
    },
    /// The lower bound of an arc.
    LowerBound {
        /// The arc read.
        arc: usize,
        /// Where its lower bound goes.
        lower_bound: &'a mut MaybeUninit<u64>,
    },
    /// The capacity of an arc.
    Capacity {
        /// The arc read.
        arc: usize,
        /// Where its capacity goes.
        capacity: &'a mut MaybeUninit<u64>,
    },
    /// The lower bound and the capacity of an arc.
    Bounds {
        /// The arc read.
        arc: usize,
        /// Where its lower bound goes.
        lower_bound: &'a mut MaybeUninit<u64>,
        /// Where its capacity goes.
        capacity: &'a mut MaybeUninit<u64>,
    },
    /// The supply of a node.
    Supply {
        /// The node read.
        node: usize,
        /// Where its supply goes.
        supply: &'a mut MaybeUninit<i64>,
    },
    /// The tail of every arc, arc `a` at `a`: one entry per arc of the problem held.
    Tails(&'a mut [MaybeUninit<usize>]),
    /// The head of every arc, arc `a` at `a`: one entry per arc.
    Heads(&'a mut [MaybeUninit<usize>]),
    /// The cost of every arc, arc `a` at `a`: one entry per arc.
    Costs(&'a mut [MaybeUninit<i64>]),
    /// The lower bound of every arc, arc `a` at `a`: one entry per arc.
    LowerBounds(&'a mut [MaybeUninit<u64>]),
    /// The capacity of every arc, arc `a` at `a`: one entry per arc.
    Capacities(&'a mut [MaybeUninit<u64>]),
    /// The supply of every node, node `v` at `v`: one entry per node of the problem held.
    Supplies(&'a mut [MaybeUninit<i64>]),
}

impl Network {
    /// A network never loaded, which holds the problem of no node; it allocates nothing.
    pub(crate) const fn new() -> Self {
        Self { held: kahlnws::Network::new() }
    }

    /// Makes room for problems of up to `node_count` nodes and `arc_count` arcs, and answers [`Status::Ok`];
    /// [`Status::NoRoom`] where the memory is refused, the room already held kept. The problem held and its optimum
    /// stay.
    pub(crate) fn reserve(&mut self, node_count: usize, arc_count: usize) -> Status {
        match self.held.try_reserve(node_count, arc_count) {
            Ok(()) => Status::Ok,
            Err(_) => Status::NoRoom,
        }
    }

    /// Holds `problem` from now on, starting from the basis held where it serves, as [`kahlnws::Network::load`] does,
    /// and answers [`Status::Ok`]; or refuses it, writing its breach to `refusal` where C wants it, and answers
    /// [`Status::Refused`]; or answers [`Status::NoRoom`] where the memory for the room the problem needs is refused. A
    /// refused problem, or refused memory, leaves the network as it was, its basis and its optimum included.
    pub(crate) fn load(&mut self, problem: LentProblem<'_>, refusal: Option<&mut MaybeUninit<Refusal>>) -> Status {
        // The room is made before the load, by a reservation that can be refused: a load that grew its own room could
        // only end the process where the memory runs out.
        if self.reserve(problem.node_count(), problem.arc_count()) == Status::NoRoom {
            return Status::NoRoom;
        }

        match self.held.load(problem.problem()) {
            Ok(()) => Status::Ok,
            Err(breach) => {
                // Never `ProblemError::Length`, which `Refusal::of` refuses: a lent problem rules it out
                // (`LentProblem`).
                write_wanted(refusal, Refusal::of(breach));
                Status::Refused
            }
        }
    }

    /// A copy of the network that changes apart from it, as [`kahlnws::Network::try_clone`] makes it; `None` where
    /// the memory for the copy's room is refused.
    pub(crate) fn try_clone(&self) -> Option<Self> {
        self.held.try_clone().ok().map(|held| Self { held })
    }

    /// Makes the network a copy of `source`, reusing its own room, as [`kahlnws::Network::try_clone_from`] does, and
    /// answers [`Status::Ok`]; [`Status::NoRoom`] where the memory for the room is refused, the network then holding
    /// what it held.
    pub(crate) fn copy_from(&mut self, source: &Self) -> Status {
        match self.held.try_clone_from(&source.held) {
            Ok(()) => Status::Ok,
            Err(_) => Status::NoRoom,
        }
    }

    /// The number of nodes of the problem held.
    pub(crate) fn node_count(&self) -> usize {
        self.held.node_count()
    }

    /// The number of arcs of the problem held.
    pub(crate) fn arc_count(&self) -> usize {
        self.held.arc_count()
    }

    /// Makes `cost` the cost of `arc`, as [`kahlnws::Network::set_cost`] does, and answers how it went: where `arc`
    /// is below the arc count, [`Status::Ok`] with the cost it had written to `previous`, or [`Status::Refused`]
    /// with the breach written to `refusal`, each where C wants it; [`Status::InvalidArgument`] otherwise, nothing
    /// written.
    ///
    /// # Panics
    /// Panics if the network is half changed, as [`kahlnws::Network::set_cost`] does, before any change.
    pub(crate) fn set_cost(
        &mut self,
        arc: usize,
        cost: i64,
        previous: Option<&mut MaybeUninit<i64>>,
        refusal: Option<&mut MaybeUninit<Refusal>>,
    ) -> Status {
        if !self.holds_arc(arc) {
            return Status::InvalidArgument;
        }
        let Some(had) = answered(self.held.set_cost(arc, cost), refusal) else { return Status::Refused };

        write_wanted(previous, had);
        Status::Ok
    }

    /// Makes `capacity` the capacity of `arc`, as [`kahlnws::Network::set_capacity`] does, and answers how it went, as
    /// [`set_cost`](Self::set_cost) says, the capacity it had written to `previous`.
    ///
    /// # Panics
    /// Panics if the network is half changed, as [`kahlnws::Network::set_capacity`] does, before any change.
    pub(crate) fn set_capacity(
        &mut self,
        arc: usize,
        capacity: u64,
        previous: Option<&mut MaybeUninit<u64>>,
        refusal: Option<&mut MaybeUninit<Refusal>>,
    ) -> Status {
        if !self.holds_arc(arc) {
            return Status::InvalidArgument;
        }
        let Some(had) = answered(self.held.set_capacity(arc, capacity), refusal) else { return Status::Refused };

        write_wanted(previous, had);
        Status::Ok
    }

    /// Makes `lower_bound` the lower bound of `arc`, as [`kahlnws::Network::set_lower_bound`] does, and answers how it
    /// went, as [`set_cost`](Self::set_cost) says, the lower bound it had written to `previous`.
    ///
    /// # Panics
    /// Panics if the network is half changed, as [`kahlnws::Network::set_lower_bound`] does, before any change.
    pub(crate) fn set_lower_bound(
        &mut self,
        arc: usize,
        lower_bound: u64,
        previous: Option<&mut MaybeUninit<u64>>,
        refusal: Option<&mut MaybeUninit<Refusal>>,
    ) -> Status {
        if !self.holds_arc(arc) {
            return Status::InvalidArgument;
        }
        let Some(had) = answered(self.held.set_lower_bound(arc, lower_bound), refusal) else {
            return Status::Refused;
        };

        write_wanted(previous, had);
        Status::Ok
    }

    /// Makes `bounds` the bounds of `arc` at once, as [`kahlnws::Network::set_bounds`] does, and answers how it went,
    /// as [`set_cost`](Self::set_cost) says, the lower bound it had written to `previous_lower_bound` and the capacity
    /// to `previous_capacity`.
    ///
    /// # Panics
    /// Panics if the network is half changed, as [`kahlnws::Network::set_bounds`] does, before any change.
    pub(crate) fn set_bounds(
        &mut self,
        arc: usize,
        bounds: Bounds,
        previous_lower_bound: Option<&mut MaybeUninit<u64>>,
        previous_capacity: Option<&mut MaybeUninit<u64>>,
        refusal: Option<&mut MaybeUninit<Refusal>>,
    ) -> Status {
        if !self.holds_arc(arc) {
            return Status::InvalidArgument;
        }
        let Some(had) = answered(self.held.set_bounds(arc, bounds), refusal) else { return Status::Refused };

        write_wanted(previous_lower_bound, had.lower_bound);
        write_wanted(previous_capacity, had.capacity);
        Status::Ok
    }

    /// Moves `amount` of supply from `from_node` to `to_node`, as [`kahlnws::Network::move_supply`] does, and answers
    /// how it went: where both nodes are below the node count, [`Status::Ok`], or [`Status::Refused`] with the
    /// breach written to `refusal` where C wants it; [`Status::InvalidArgument`] otherwise, nothing written.
    ///
    /// # Panics
    /// Panics if the network is half changed, as [`kahlnws::Network::move_supply`] does, before any change.
    pub(crate) fn move_supply(
        &mut self,
        from_node: usize,
        to_node: usize,
        amount: u64,
        refusal: Option<&mut MaybeUninit<Refusal>>,
    ) -> Status {
        if !self.holds_node(from_node) || !self.holds_node(to_node) {
            return Status::InvalidArgument;
        }
        let Some(()) = answered(self.held.move_supply(from_node, to_node, amount), refusal) else {
            return Status::Refused;
        };

        Status::Ok
    }

    /// Makes `supply` the supply of `node`, the supply of `balancing_node` moving by the same amount the other way, as
    /// [`kahlnws::Network::set_supply`] does, and answers how it went: where [`admits_supply`](Self::admits_supply)
    /// holds, [`Status::Ok`] with the supply `node` had written to `previous`, or [`Status::Refused`] with the breach
    /// written to `refusal`, each where C wants it; [`Status::InvalidArgument`] otherwise, nothing written.
    ///
    /// # Panics
    /// Panics if the network is half changed, as [`kahlnws::Network::set_supply`] does, before any change.
    pub(crate) fn set_supply(
        &mut self,
        node: usize,
        supply: i64,
        balancing_node: usize,
        previous: Option<&mut MaybeUninit<i64>>,
        refusal: Option<&mut MaybeUninit<Refusal>>,
    ) -> Status {
        if !self.admits_supply(node, supply, balancing_node) {
            return Status::InvalidArgument;
        }
        let Some(had) = answered(self.held.set_supply(node, supply, balancing_node), refusal) else {
            return Status::Refused;
        };

        write_wanted(previous, had);
        Status::Ok
    }

    /// Solves the problem held, and answers [`Status::Optimal`] with the cost written to `total_cost`, or
    /// [`Status::Infeasible`]; the work of the solve is written to `work` on both, each where C wants it.
    ///
    /// # Panics
    /// Panics if the network is half changed, as [`kahlnws::Network::solve`] does.
    pub(crate) fn solve(
        &mut self,
        total_cost: Option<&mut MaybeUninit<Int128>>,
        work: Option<&mut MaybeUninit<Work>>,
    ) -> Status {
        let Answer { outcome, work: done } = self.held.solve();
        write_wanted(work, Work::from(done));

        match outcome {
            Outcome::Optimal(cost) => {
                write_wanted(total_cost, Int128::from(cost));
                Status::Optimal
            }
            Outcome::Infeasible => Status::Infeasible,
        }
    }

    /// Writes what `read` asks of the optimum the network holds and answers [`Status::Ok`]; where it holds none, as
    /// [`kahlnws::Network::optimum`] says, it answers [`Status::NoOptimum`], and for the flow of an arc not below the
    /// arc count [`Status::InvalidArgument`], writing nothing either way. It changes nothing, and reads only what it
    /// writes.
    ///
    /// # Panics
    /// Panics if `read` holds room for every flow or every potential of another length than the problem held has arcs
    /// or nodes.
    #[track_caller]
    pub(crate) fn read_optimum(&self, read: OptimumRead<'_>) -> Status {
        let (node_count, arc_count) = (self.node_count(), self.arc_count());
        match &read {
            OptimumRead::Flow { arc, .. } if !self.holds_arc(*arc) => return Status::InvalidArgument,
            OptimumRead::Flows(flows) => assert!(
                flows.len() == arc_count,
                "called `Network::read_optimum` with room for {} flows, which should be the arc count {arc_count}",
                flows.len()
            ),
            OptimumRead::Potentials(potentials) => assert!(
                potentials.len() == node_count,
                "called `Network::read_optimum` with room for {} potentials, which should be the node count \
                 {node_count}",
                potentials.len()
            ),
            OptimumRead::TotalCost(_) | OptimumRead::Flow { .. } => {}
        }
        let Some(optimum) = self.held.optimum() else { return Status::NoOptimum };

        // The optimum is that of the problem held: one flow per arc and one potential per node, the lengths checked.
        match read {
            OptimumRead::TotalCost(total_cost) => {
                total_cost.write(Int128::from(optimum.total_cost));
            }
            OptimumRead::Flow { arc, flow } => {
                flow.write(optimum.flows.get(arc).expect("an arc below the arc count should have a flow"));
            }
            OptimumRead::Flows(flows) => write_all(flows, optimum.flows),
            OptimumRead::Potentials(potentials) => {
                potentials.write_copy_of_slice(optimum.potentials);
            }
        }

        Status::Ok
    }

    /// Writes what `read` asks of the problem the network holds, as it now stands, and answers [`Status::Ok`];
    /// answers [`Status::InvalidArgument`] for an arc or a node not below its count, writing nothing. It changes
    /// nothing, keeps an optimum the network holds and needs none.
    ///
    /// # Panics
    /// Panics if `read` holds room for every arc's or every node's number of another length than the problem held has
    /// arcs or nodes, or if the network is half changed, as the reads of [`kahlnws::Network`] do.
    #[track_caller]
    pub(crate) fn read_problem(&self, read: ProblemRead<'_>) -> Status {
        let (node_count, arc_count) = (self.node_count(), self.arc_count());
        let reach = match &read {
            ProblemRead::Tail { arc, .. }
            | ProblemRead::Head { arc, .. }
            | ProblemRead::Cost { arc, .. }
            | ProblemRead::LowerBound { arc, .. }
            | ProblemRead::Capacity { arc, .. }
            | ProblemRead::Bounds { arc, .. } => Reach::Arc { arc: *arc },
            ProblemRead::Supply { node, .. } => Reach::Node { node: *node },
            ProblemRead::Tails(entries) | ProblemRead::Heads(entries) => Reach::column(entries, arc_count),
            ProblemRead::Costs(entries) => Reach::column(entries, arc_count),
            ProblemRead::LowerBounds(entries) | ProblemRead::Capacities(entries) => Reach::column(entries, arc_count),
            ProblemRead::Supplies(entries) => Reach::column(entries, node_count),
        };
        match reach {
            Reach::Arc { arc } if !self.holds_arc(arc) => return Status::InvalidArgument,
            Reach::Node { node } if !self.holds_node(node) => return Status::InvalidArgument,
            Reach::Arc { .. } | Reach::Node { .. } => {}
            Reach::Column { length, count } => assert!(
                length == count,
                "called `Network::read_problem` with room for {length} entries, which should be the count {count}"
            ),
        }

        // The index is below its count, and every column has one entry per arc or node, as just checked: no single read
        // panics.
        match read {
            ProblemRead::Tail { arc, tail } => {
                tail.write(self.held.tail(arc));
            }
            ProblemRead::Head { arc, head } => {
                head.write(self.held.head(arc));
            }
            ProblemRead::Cost { arc, cost } => {
                cost.write(self.held.cost(arc));
            }
            ProblemRead::LowerBound { arc, lower_bound } => {
                lower_bound.write(self.held.lower_bound(arc));
            }
            ProblemRead::Capacity { arc, capacity } => {
                capacity.write(self.held.capacity(arc));
            }
            ProblemRead::Bounds { arc, lower_bound, capacity } => {
                let bounds = self.held.bounds(arc);
                lower_bound.write(bounds.lower_bound);
                capacity.write(bounds.capacity);
            }
            ProblemRead::Supply { node, supply } => {
                supply.write(self.held.supply(node));
            }
            ProblemRead::Tails(entries) => write_all(entries, self.held.tails()),
            ProblemRead::Heads(entries) => write_all(entries, self.held.heads()),
            ProblemRead::Costs(entries) => write_all(entries, self.held.costs()),
            ProblemRead::LowerBounds(entries) => write_all(entries, self.held.lower_bounds()),
            ProblemRead::Capacities(entries) => write_all(entries, self.held.capacities()),
            ProblemRead::Supplies(entries) => {
                entries.write_copy_of_slice(self.held.supplies());
            }
        }

        Status::Ok
    }

    /// Whether `arc` is below the arc count of the problem held.
    fn holds_arc(&self, arc: usize) -> bool {
        arc < self.arc_count()
    }

    /// Whether `node` is below the node count of the problem held.
    fn holds_node(&self, node: usize) -> bool {
        node < self.node_count()
    }

    /// Whether `supply` for `node`, balanced by `balancing_node`, is a call [`kahlnws::Network::set_supply`] takes
    /// without a panic of the caller's making: both nodes are below the node count, and a node that balances itself
    /// is given only the supply it has.
    ///
    /// # Panics
    /// Panics if the network is half changed and `node` is `balancing_node`, as the read of its supply does.
    fn admits_supply(&self, node: usize, supply: i64, balancing_node: usize) -> bool {
        self.holds_node(node)
            && self.holds_node(balancing_node)
            && (node != balancing_node || self.held.supply(node) == supply)
    }
}

/// How much of the problem held a read reaches: one arc, one node, or a whole column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    /// One arc.
    Arc {
        /// The arc read.
        arc: usize,
    },
    /// One node.
    Node {
        /// The node read.
        node: usize,
    },
    /// Every arc or every node, into room for `length` entries.
    Column {
        /// The entries the room holds.
        length: usize,
        /// The arc count or the node count of the problem held, which `length` should be.
        count: usize,
    },
}

impl Reach {
    /// The reach of a read of a whole column of `count` entries into `entries`.
    fn column<T>(entries: &[T], count: usize) -> Self {
        Self::Column { length: entries.len(), count }
    }
}

/// What a change replaced, where `answer` is that of a change that was not refused; `None` for a refused one, whose
/// breach is written to `refusal` where C wants it.
fn answered<T>(answer: Result<T, ChangeError>, refusal: Option<&mut MaybeUninit<Refusal>>) -> Option<T> {
    match answer {
        Ok(replaced) => Some(replaced),
        Err(breach) => {
            write_wanted(refusal, Refusal::of_change(breach));
            None
        }
    }
}

/// Writes the numbers of `numbers`, in order, to `entries`, one each, as many as both hold.
fn write_all<T>(entries: &mut [MaybeUninit<T>], numbers: impl IntoIterator<Item = T>) {
    for (entry, number) in entries.iter_mut().zip(numbers) {
        entry.write(number);
    }
}

/// Writes `value` to `output` where C wants it, and nothing where it passed null.
fn write_wanted<T>(output: Option<&mut MaybeUninit<T>>, value: T) {
    if let Some(output) = output {
        output.write(value);
    }
}

#[cfg(test)]
mod tests {
    use kahlnws::ProblemError;

    use super::*;
    use crate::boundary::{written, written_entries};
    use crate::refusal::RefusalKind;

    /// The depot: node 0 sends 4 units to node 2, directly by arc 0 (capacity 3, cost 5) or through node 1 by arcs 1
    /// and 2 (capacity 4 and 2, cost 1 each). Two units go each way, at a cost of 14.
    fn depot() -> LentProblem<'static> {
        LentProblem::new(kahlnws::Problem {
            tails: &[0, 0, 1],
            heads: &[2, 1, 2],
            lower_bounds: None,
            capacities: &[3, 4, 2],
            costs: &[5, 1, 1],
            supplies: &[4, 0, -4],
        })
    }

    /// A network that holds the depot, solved.
    fn solved_depot() -> Network {
        let mut network = Network::new();
        assert_eq!(network.load(depot(), None), Status::Ok);
        assert_eq!(network.solve(None, None), Status::Optimal);

        network
    }

    /// The cost the network's optimum answers, read through [`OptimumRead::TotalCost`]; `None` where it holds none.
    fn total_cost(network: &Network) -> Option<i128> {
        let mut cost = MaybeUninit::new(Int128 { low: 7, high: 7 });
        let status = network.read_optimum(OptimumRead::TotalCost(&mut cost));

        // SAFETY: the test made the cost with `MaybeUninit::new`, and the read writes it only whole.
        let cost = unsafe { written(&cost) };

        (status == Status::Ok).then(|| i128::from(cost))
    }

    #[test]
    fn test_a_new_network_holds_the_problem_of_no_node_and_solves_it_with_no_work() {
        let mut network = Network::new();
        let (mut cost, mut work) = (MaybeUninit::new(Int128 { low: 7, high: 7 }), MaybeUninit::new(Work::default()));

        assert_eq!((network.node_count(), network.arc_count()), (0, 0));
        assert_eq!(network.solve(Some(&mut cost), Some(&mut work)), Status::Optimal);
        // SAFETY: the test made both with `MaybeUninit::new`, and the solve writes an output only whole.
        let answered = unsafe { (written(&cost), written(&work)) };
        assert_eq!(answered, (Int128 { low: 0, high: 0 }, Work::default()));
        assert_eq!(network.read_optimum(OptimumRead::Flows(&mut [])), Status::Ok);
    }

    #[test]
    fn test_a_load_answers_ok_and_holds_no_optimum_until_a_solve() {
        let mut network = Network::new();
        assert_eq!(network.load(depot(), None), Status::Ok);
        assert_eq!((network.node_count(), network.arc_count(), total_cost(&network)), (3, 3, None));

        let mut cost = MaybeUninit::new(Int128 { low: 7, high: 7 });
        let mut work = MaybeUninit::new(Work { pivots: 7, degenerate_pivots: 7, first_arc_pivots: 7 });
        assert_eq!(network.solve(Some(&mut cost), Some(&mut work)), Status::Optimal);
        // SAFETY: the test made both with `MaybeUninit::new`, and the solve writes an output only whole.
        let (cost, work) = unsafe { (written(&cost), written(&work)) };
        // The network's first solve starts from the star and tries no arcs before block search.
        assert_eq!((i128::from(cost), work.first_arc_pivots), (14, 0));
        assert_eq!(total_cost(&network), Some(14));
    }

    #[test]
    fn test_a_refused_load_writes_its_breach_and_keeps_what_the_network_held() {
        let mut network = solved_depot();
        let unbalanced = LentProblem::new(kahlnws::Problem { supplies: &[4, 0, -3], ..*depot().problem() });
        let mut refusal = MaybeUninit::new(Refusal::default());

        assert_eq!(network.load(unbalanced, Some(&mut refusal)), Status::Refused);
        // SAFETY: the test made the refusal with `MaybeUninit::new`, and the load writes it only whole.
        assert_eq!(unsafe { written(&refusal) }, Refusal::of(ProblemError::Unbalanced { total: 1 }));
        assert_eq!(total_cost(&network), Some(14), "the optimum held is kept");
    }

    #[test]
    fn test_a_refused_reservation_answers_no_room_and_keeps_the_optimum_held() {
        let mut network = solved_depot();

        assert_eq!(network.reserve(usize::MAX, 0), Status::NoRoom);
        assert_eq!(network.reserve(4, 4), Status::Ok);
        assert_eq!(total_cost(&network), Some(14), "a reservation keeps the optimum");
    }

    #[test]
    fn test_a_change_answers_ok_with_what_it_replaced_and_ends_the_optimum() {
        let mut network = solved_depot();
        let (mut cost, mut capacity, mut lower) = (MaybeUninit::new(7), MaybeUninit::new(7), MaybeUninit::new(7));
        let (mut lower_bound, mut capacity_had) = (MaybeUninit::new(7), MaybeUninit::new(7));

        // The second leg through node 1 costs 3: 2 units at 4 and 2 at 5, 18.
        assert_eq!(network.set_cost(2, 3, Some(&mut cost), None), Status::Ok);
        // SAFETY: the test made the cost with `MaybeUninit::new`, and the change writes it only whole.
        assert_eq!((unsafe { written(&cost) }, total_cost(&network)), (1, None));
        assert_eq!(network.solve(None, None), Status::Optimal);
        assert_eq!(total_cost(&network), Some(18));

        assert_eq!(network.set_capacity(0, 5, Some(&mut capacity), None), Status::Ok);
        assert_eq!(network.set_lower_bound(1, 1, Some(&mut lower), None), Status::Ok);
        let bounds = Bounds { lower_bound: 0, capacity: 3 };
        assert_eq!(network.set_bounds(2, bounds, Some(&mut lower_bound), Some(&mut capacity_had), None), Status::Ok);
        assert_eq!(network.move_supply(2, 0, 1, None), Status::Ok);
        // SAFETY: the test made each with `MaybeUninit::new`, and the changes write an output only whole.
        let replaced = unsafe { (written(&capacity), written(&lower), written(&lower_bound), written(&capacity_had)) };
        assert_eq!(replaced, (3, 0, 0, 2));
        assert_eq!(total_cost(&network), None);
    }

    #[test]
    fn test_a_change_of_the_number_held_keeps_the_optimum() {
        let mut network = solved_depot();
        let mut previous = MaybeUninit::new(7);

        assert_eq!(network.set_cost(0, 5, Some(&mut previous), None), Status::Ok);
        assert_eq!(network.move_supply(1, 1, 3, None), Status::Ok);
        // SAFETY: the test made it with `MaybeUninit::new`, and the change writes it only whole.
        assert_eq!((unsafe { written(&previous) }, total_cost(&network)), (5, Some(14)));
    }

    #[test]
    fn test_a_refused_change_writes_its_breach_and_changes_nothing() {
        let mut network = solved_depot();
        let mut previous = MaybeUninit::new(7_u64);
        let mut refusal = MaybeUninit::new(Refusal::default());

        let status = network.set_lower_bound(0, 4, Some(&mut previous), Some(&mut refusal));
        // SAFETY: the test made both with `MaybeUninit::new`, and the change writes an output only whole.
        let (previous, written_refusal) = unsafe { (written(&previous), written(&refusal)) };
        assert_eq!((status, previous), (Status::Refused, 7), "nothing replaced, nothing written");
        let expected = Refusal::of_change(ChangeError::LowerAboveCapacity { arc: 0, lower_bound: 4, capacity: 3 });
        assert_eq!(written_refusal, expected);

        assert_eq!(network.move_supply(2, 1, u64::MAX, Some(&mut refusal)), Status::Refused);
        // SAFETY: the test made the refusal with `MaybeUninit::new`, and the move writes it only whole.
        assert_eq!(unsafe { written(&refusal) }.kind, RefusalKind::SUPPLY_BEYOND_RANGE);
        assert_eq!(total_cost(&network), Some(14), "the optimum held is kept");
    }

    /// What `network.set_supply(node, supply, balancing_node)` answers, the supply it had read back from storage that
    /// starts at 7, a supply no node of the depot has at any step of these tests, and the refusal it wrote, from
    /// storage that starts as the default.
    fn supply_set(network: &mut Network, node: usize, supply: i64, balancing_node: usize) -> (Status, i64, Refusal) {
        let (mut previous, mut refusal) = (MaybeUninit::new(7), MaybeUninit::new(Refusal::default()));
        let status = network.set_supply(node, supply, balancing_node, Some(&mut previous), Some(&mut refusal));

        // SAFETY: the test made both with `MaybeUninit::new`, and the change writes an output only whole.
        let (previous, refusal) = unsafe { (written(&previous), written(&refusal)) };

        (status, previous, refusal)
    }

    /// The cost of the optimum a solve of `network` finds, `None` where it finds none.
    fn solved_cost(network: &mut Network) -> Option<i128> {
        assert!(matches!(network.solve(None, None), Status::Optimal | Status::Infeasible));

        total_cost(network)
    }

    #[test]
    fn test_a_supplier_that_falls_and_a_consumer_whose_demand_rises_by_hand() {
        // The depot is to send 2: the customer, balancing it, takes 2. Both go through node 1, at 2 a unit: 4.
        let mut network = solved_depot();
        assert_eq!(supply_set(&mut network, 0, 2, 2), (Status::Ok, 4, Refusal::default()));
        assert_eq!((total_cost(&network), solved_cost(&mut network)), (None, Some(4)));
        assert_eq!(columns(&network).supplies, [2, 0, -2]);

        // The customer is to take 5, as the node set and as the balancing node alike: 2 through node 1, 3 directly, 19.
        for (node, supply, balancing_node, replaced) in [(2, -5, 0, -4), (0, 5, 2, 4)] {
            let mut network = solved_depot();
            assert_eq!(supply_set(&mut network, node, supply, balancing_node).0, Status::Ok);
            assert_eq!(solved_cost(&mut network), Some(19));
            assert_eq!(columns(&network).supplies, [5, 0, -5]);

            // The answer set back restores both nodes and the cost.
            assert_eq!(supply_set(&mut network, node, replaced, balancing_node).1, supply);
            assert_eq!(solved_cost(&mut network), Some(14));
            assert_eq!(columns(&network).supplies, [4, 0, -4]);
        }
    }

    #[test]
    fn test_a_node_that_changes_sides_by_hand() {
        // Node 1 is to send 2, balanced by the depot, which sends 2 less; then node 1 is to take 1, balanced by the
        // customer, which takes 3 less. The depot sends 1 to node 1 and 1 on to the customer: `2 + 1 = 3`.
        let mut network = solved_depot();
        assert_eq!(supply_set(&mut network, 1, 2, 0).1, 0);
        assert_eq!(supply_set(&mut network, 1, -1, 2).1, 2);
        assert_eq!(solved_cost(&mut network), Some(3));
        assert_eq!(columns(&network).supplies, [2, -1, -1]);
    }

    #[test]
    fn test_a_supply_set_to_what_it_is_changes_nothing_by_hand() {
        // As a move of nothing: the optimum stands, and the solve after makes no pivot.
        let mut network = solved_depot();
        assert_eq!(supply_set(&mut network, 0, 4, 2), (Status::Ok, 4, Refusal::default()));
        assert_eq!(supply_set(&mut network, 1, 0, 1), (Status::Ok, 0, Refusal::default()), "its own balancing node");
        assert_eq!(total_cost(&network), Some(14));

        let mut work = MaybeUninit::new(Work { pivots: 7, degenerate_pivots: 7, first_arc_pivots: 7 });
        assert_eq!(network.solve(None, Some(&mut work)), Status::Optimal);
        // SAFETY: the test made the work with `MaybeUninit::new`, and the solve writes it only whole.
        assert_eq!(unsafe { written(&work) }, Work::default());
    }

    #[test]
    fn test_a_supply_whose_balancing_node_would_leave_its_range_is_refused_by_hand() {
        // The customer is to take `2^63`: the depot would send `2^63`, past `i64::MAX`, and is named. Nothing changes.
        let mut network = solved_depot();
        let past = ChangeError::SupplyBeyondRange { node: 0, supply: i128::from(i64::MAX) + 1 };
        assert_eq!(supply_set(&mut network, 2, i64::MIN, 0), (Status::Refused, 7, Refusal::of_change(past)));
        assert_eq!((columns(&network).supplies, total_cost(&network)), (vec![4, 0, -4], Some(14)));
    }

    #[test]
    fn test_a_node_that_balances_itself_and_is_to_change_is_an_invalid_argument_and_changes_nothing() {
        let mut network = solved_depot();
        let untouched = (Status::InvalidArgument, 7, Refusal::default());

        assert_eq!(supply_set(&mut network, 0, 5, 0), untouched);
        assert_eq!(supply_set(&mut network, 1, i64::MIN, 1), untouched);
        assert_eq!((columns(&network).supplies, total_cost(&network)), (vec![4, 0, -4], Some(14)));
    }

    #[test]
    fn test_an_arc_or_a_node_not_below_its_count_is_an_invalid_argument_and_changes_nothing() {
        let mut network = solved_depot();
        let mut previous = MaybeUninit::new(7);
        let mut flow = MaybeUninit::new(7);

        assert_eq!(network.set_cost(3, 1, Some(&mut previous), None), Status::InvalidArgument);
        assert_eq!(network.set_capacity(3, 1, None, None), Status::InvalidArgument);
        assert_eq!(network.set_lower_bound(usize::MAX, 1, None, None), Status::InvalidArgument);
        assert_eq!(
            network.set_bounds(3, Bounds { lower_bound: 0, capacity: 1 }, None, None, None),
            Status::InvalidArgument
        );
        assert_eq!(network.move_supply(3, 0, 1, None), Status::InvalidArgument);
        assert_eq!(network.move_supply(0, 3, 1, None), Status::InvalidArgument);
        assert_eq!(network.set_supply(3, 1, 0, Some(&mut previous), None), Status::InvalidArgument);
        assert_eq!(network.set_supply(0, 1, 3, Some(&mut previous), None), Status::InvalidArgument);
        assert_eq!(network.read_optimum(OptimumRead::Flow { arc: 3, flow: &mut flow }), Status::InvalidArgument);
        // SAFETY: the test made both with `MaybeUninit::new`, and no call wrote them.
        let untouched = unsafe { (written(&previous), written(&flow)) };
        assert_eq!((untouched, total_cost(&network)), ((7, 7), Some(14)));
    }

    #[test]
    fn test_the_reads_write_the_optimum_and_nothing_where_the_network_holds_none() {
        let mut network = solved_depot();
        let mut flow = MaybeUninit::new(7);
        let mut flows = [MaybeUninit::new(7); 3];
        let mut potentials = [MaybeUninit::new(7); 3];

        assert_eq!(network.read_optimum(OptimumRead::Flow { arc: 1, flow: &mut flow }), Status::Ok);
        assert_eq!(network.read_optimum(OptimumRead::Flows(&mut flows)), Status::Ok);
        assert_eq!(network.read_optimum(OptimumRead::Potentials(&mut potentials)), Status::Ok);
        // SAFETY: the test made each with `MaybeUninit::new`, and the reads write an output only whole.
        let (flow, flows, potentials) =
            unsafe { (written(&flow), written_entries(&flows), written_entries(&potentials)) };
        assert_eq!((flow, flows), (2, vec![2, 2, 2]));
        // Arcs 0 and 1 carry flow strictly between their bounds, so their reduced costs are zero: the potentials are
        // p, p + 1 and p + 5 for some p.
        assert_eq!((potentials[1] - potentials[0], potentials[2] - potentials[0]), (1, 5));

        assert_eq!(network.set_capacity(0, 2, None, None), Status::Ok);
        let (mut flows_after, mut potentials_after) = ([MaybeUninit::new(9); 3], [MaybeUninit::new(9); 3]);
        let mut flow_after = MaybeUninit::new(9);
        assert_eq!(network.read_optimum(OptimumRead::Flow { arc: 1, flow: &mut flow_after }), Status::NoOptimum);
        assert_eq!(network.read_optimum(OptimumRead::Flows(&mut flows_after)), Status::NoOptimum);
        assert_eq!(network.read_optimum(OptimumRead::Potentials(&mut potentials_after)), Status::NoOptimum);
        // SAFETY: the test made each with `MaybeUninit::new`, and the reads write an output only whole.
        let read = unsafe { (written_entries(&flows_after), written_entries(&potentials_after), written(&flow_after)) };
        assert_eq!(read, (vec![9, 9, 9], vec![9, 9, 9], 9), "nothing of the earlier optimum is written");
    }

    #[test]
    fn test_a_solve_that_finds_no_flow_answers_infeasible_with_its_work_and_holds_no_optimum() {
        let mut network = solved_depot();
        let mut cost = MaybeUninit::new(Int128 { low: 7, high: 7 });
        let mut work = MaybeUninit::new(Work { pivots: 7, degenerate_pivots: 7, first_arc_pivots: 7 });

        // Arc 0 at capacity 1 and arc 2 at capacity 2 carry 3 of the 4 units.
        assert_eq!(network.set_capacity(0, 1, None, None), Status::Ok);
        assert_eq!(network.solve(Some(&mut cost), Some(&mut work)), Status::Infeasible);
        // SAFETY: the test made both with `MaybeUninit::new`, and the solve writes an output only whole.
        let (cost, work) = unsafe { (written(&cost), written(&work)) };
        assert_eq!(cost, Int128 { low: 7, high: 7 }, "no cost is written");
        assert_eq!(work.first_arc_pivots, 0, "the work is written");
        assert_eq!(total_cost(&network), None);
    }

    #[test]
    fn test_a_copy_changes_apart_from_its_network() {
        let network = solved_depot();
        let mut copy = network.try_clone().expect("the memory for a copy should be granted");

        assert_eq!(copy.set_cost(2, 3, None, None), Status::Ok);
        assert_eq!((total_cost(&network), total_cost(&copy)), (Some(14), None));
    }

    #[test]
    #[should_panic(expected = "called `Network::read_optimum` with room for 2 flows, which should be the arc count 3")]
    fn test_a_read_of_the_flows_into_room_of_another_length_panics() {
        let _ = solved_depot().read_optimum(OptimumRead::Flows(&mut [MaybeUninit::uninit(); 2]));
    }

    #[test]
    #[should_panic(
        expected = "called `Network::read_optimum` with room for 4 potentials, which should be the node count 3"
    )]
    fn test_a_read_of_the_potentials_into_room_of_another_length_panics() {
        let _ = solved_depot().read_optimum(OptimumRead::Potentials(&mut [MaybeUninit::uninit(); 4]));
    }

    #[test]
    fn test_a_copy_into_a_network_answers_ok_and_holds_its_source() {
        let source = solved_depot();
        let mut destination = Network::new();
        assert_eq!(
            destination.load(LentProblem::new(kahlnws::Problem { supplies: &[3, 0, -3], ..*depot().problem() }), None),
            Status::Ok
        );

        assert_eq!(destination.copy_from(&source), Status::Ok);
        assert_eq!(total_cost(&destination), Some(14), "the copy holds the source's optimum");
        let mut work = MaybeUninit::new(Work { pivots: 7, degenerate_pivots: 7, first_arc_pivots: 7 });
        assert_eq!(destination.solve(None, Some(&mut work)), Status::Optimal);
        // SAFETY: the test made the work with `MaybeUninit::new`, and the solve writes it only whole.
        assert_eq!(unsafe { written(&work) }, Work::default(), "the copy solves from the copied basis");
    }

    /// Every number of a problem, by column.
    #[derive(Debug, Default, PartialEq, Eq)]
    struct Columns {
        /// The node each arc leaves.
        tails: Vec<usize>,
        /// The node each arc enters.
        heads: Vec<usize>,
        /// Each arc's cost.
        costs: Vec<i64>,
        /// Each arc's lower bound.
        lower_bounds: Vec<u64>,
        /// Each arc's capacity.
        capacities: Vec<u64>,
        /// Each node's supply.
        supplies: Vec<i64>,
    }

    /// Every number of the problem `network` holds, read by the reads of a whole column.
    fn columns(network: &Network) -> Columns {
        let (arcs, nodes) = (network.arc_count(), network.node_count());
        let (mut tails, mut heads) = (vec![MaybeUninit::new(7); arcs], vec![MaybeUninit::new(7); arcs]);
        let mut costs = vec![MaybeUninit::new(7); arcs];
        let (mut lower, mut capacities) = (vec![MaybeUninit::new(7); arcs], vec![MaybeUninit::new(7); arcs]);
        let mut supplies = vec![MaybeUninit::new(7); nodes];

        for status in [
            network.read_problem(ProblemRead::Tails(&mut tails)),
            network.read_problem(ProblemRead::Heads(&mut heads)),
            network.read_problem(ProblemRead::Costs(&mut costs)),
            network.read_problem(ProblemRead::LowerBounds(&mut lower)),
            network.read_problem(ProblemRead::Capacities(&mut capacities)),
            network.read_problem(ProblemRead::Supplies(&mut supplies)),
        ] {
            assert_eq!(status, Status::Ok);
        }

        // SAFETY: the test made every entry with `MaybeUninit::new`, and the reads write an entry only whole.
        let (tails, heads, costs, lower_bounds, capacities, supplies) = unsafe {
            (
                written_entries(&tails),
                written_entries(&heads),
                written_entries(&costs),
                written_entries(&lower),
                written_entries(&capacities),
                written_entries(&supplies),
            )
        };

        Columns { tails, heads, costs, lower_bounds, capacities, supplies }
    }

    #[test]
    fn test_the_reads_of_the_problem_answer_it_as_it_stands_and_keep_the_optimum() {
        let mut network = solved_depot();
        let depot_columns = Columns {
            tails: vec![0, 0, 1],
            heads: vec![2, 1, 2],
            costs: vec![5, 1, 1],
            lower_bounds: vec![0; 3],
            capacities: vec![3, 4, 2],
            supplies: vec![4, 0, -4],
        };
        assert_eq!(columns(&network), depot_columns);
        assert_eq!(total_cost(&network), Some(14), "a read keeps the optimum");

        assert_eq!(network.set_bounds(2, Bounds { lower_bound: 1, capacity: 3 }, None, None, None), Status::Ok);
        assert_eq!(network.move_supply(2, 0, 1, None), Status::Ok);
        assert_eq!(network.set_cost(0, 4, None, None), Status::Ok);
        assert_eq!(network.set_lower_bound(0, 9, None, None), Status::Refused);
        let changed = Columns {
            costs: vec![4, 1, 1],
            lower_bounds: vec![0, 0, 1],
            capacities: vec![3, 4, 3],
            supplies: vec![5, 0, -5],
            ..depot_columns
        };
        assert_eq!(columns(&network), changed);

        let (mut tail, mut head, mut cost) = (MaybeUninit::new(7), MaybeUninit::new(7), MaybeUninit::new(7));
        let (mut lower, mut capacity, mut supply) = (MaybeUninit::new(7), MaybeUninit::new(7), MaybeUninit::new(7));
        assert_eq!(network.read_problem(ProblemRead::Tail { arc: 2, tail: &mut tail }), Status::Ok);
        assert_eq!(network.read_problem(ProblemRead::Head { arc: 2, head: &mut head }), Status::Ok);
        assert_eq!(network.read_problem(ProblemRead::Cost { arc: 0, cost: &mut cost }), Status::Ok);
        let bounds = ProblemRead::Bounds { arc: 2, lower_bound: &mut lower, capacity: &mut capacity };
        assert_eq!(network.read_problem(bounds), Status::Ok);
        assert_eq!(network.read_problem(ProblemRead::Supply { node: 2, supply: &mut supply }), Status::Ok);
        // SAFETY: the test made each with `MaybeUninit::new`, and the reads write an output only whole.
        let read = unsafe {
            (written(&tail), written(&head), written(&cost), written(&lower), written(&capacity), written(&supply))
        };
        assert_eq!(read, (1, 2, 4, 1, 3, -5));

        let (mut lower_alone, mut capacity_alone) = (MaybeUninit::new(7), MaybeUninit::new(7));
        let lower_bound = ProblemRead::LowerBound { arc: 2, lower_bound: &mut lower_alone };
        assert_eq!(network.read_problem(lower_bound), Status::Ok);
        let capacity = ProblemRead::Capacity { arc: 2, capacity: &mut capacity_alone };
        assert_eq!(network.read_problem(capacity), Status::Ok);
        // SAFETY: the test made both with `MaybeUninit::new`, and the reads write an output only whole.
        let alone = unsafe { (written(&lower_alone), written(&capacity_alone)) };
        assert_eq!(alone, (1, 3), "each bound read alone");
    }

    #[test]
    fn test_a_read_of_the_problem_past_its_count_is_an_invalid_argument_and_writes_nothing() {
        let network = solved_depot();
        let (mut index, mut number, mut bound) =
            (MaybeUninit::new(7_usize), MaybeUninit::new(7_i64), MaybeUninit::new(7));
        let mut capacity = MaybeUninit::new(7);

        assert_eq!(network.read_problem(ProblemRead::Tail { arc: 3, tail: &mut index }), Status::InvalidArgument);
        assert_eq!(network.read_problem(ProblemRead::Head { arc: 3, head: &mut index }), Status::InvalidArgument);
        assert_eq!(network.read_problem(ProblemRead::Cost { arc: 3, cost: &mut number }), Status::InvalidArgument);
        let bounds = ProblemRead::Bounds { arc: 3, lower_bound: &mut bound, capacity: &mut capacity };
        assert_eq!(network.read_problem(bounds), Status::InvalidArgument);
        let lower_bound = ProblemRead::LowerBound { arc: 3, lower_bound: &mut bound };
        assert_eq!(network.read_problem(lower_bound), Status::InvalidArgument);
        assert_eq!(
            network.read_problem(ProblemRead::Capacity { arc: 3, capacity: &mut capacity }),
            Status::InvalidArgument
        );
        assert_eq!(network.read_problem(ProblemRead::Supply { node: 3, supply: &mut number }), Status::InvalidArgument);
        // SAFETY: the test made each with `MaybeUninit::new`, and no read wrote them.
        let untouched = unsafe { (written(&index), written(&number), written(&bound), written(&capacity)) };
        assert_eq!(untouched, (7, 7, 7, 7));
        assert_eq!(columns(&Network::new()), Default::default(), "a network never loaded holds no number");
    }

    #[test]
    #[should_panic(expected = "called `Network::read_problem` with room for 2 entries, which should be the count 3")]
    fn test_a_read_of_a_column_into_room_of_another_length_panics() {
        let _ = solved_depot().read_problem(ProblemRead::Costs(&mut [MaybeUninit::uninit(); 2]));
    }

    #[test]
    fn test_a_network_may_move_between_threads() {
        /// Compiles only for a type that may move to another thread.
        fn movable<T: Send>() {}

        movable::<Network>();
    }
}
