// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The solver C holds a pointer to, and what a reservation and a solve by either entry do once every pointer has been
//! read.

use core::mem::MaybeUninit;

use kahlnws::{Answer, NetworkSimplex, Optimum, Outcome, ProblemError};

use crate::halves::Int128;
use crate::refusal::Refusal;
use crate::status::Status;
use crate::work::Work;

/// A solver: a primal network simplex with the room it works in, kept from one solve to the next.
///
/// Opaque: C holds it only by the pointer `kahlnws_solver_new` answers, passes that pointer to the other entries, and
/// gives it back to `kahlnws_solver_free`. The room only grows, by `kahlnws_solver_reserve` or by a solve of a
/// problem larger than the room held; a solve of a problem within the room allocates nothing.
///
/// It also keeps the basis its last solve ended on, by either entry, optimal or infeasible, which only
/// `kahlnws_solver_resolve` reads: what `kahlnws_solver_solve` finds depends on its problem alone.
///
/// One thread at a time uses a solver: no two calls that take the same solver may run at once. A solver may move
/// from one thread to another between calls, and distinct solvers may be used on distinct threads at once.
#[derive(Debug)]
pub struct Solver {
    /// The solver of the library, with its room.
    simplex: NetworkSimplex,
    /// The room the flows are solved into before an optimal solve copies them out, one entry per arc of the problem
    /// being solved; its contents mean nothing between solves.
    flows: Vec<u64>,
}

/// One call of `kahlnws_solver_solve` or `kahlnws_solver_resolve`, its pointers read: the problem, and where what the
/// solve finds goes. The outputs are written, never read, so their storage need not hold values before the call.
///
/// # Invariant
/// Only [`Call::new`] makes one: every per-arc table of `problem` holds one entry per tail, so a solve never refuses
/// it as `ProblemError::Length`; `flows` holds one entry per arc, and `potentials`, where given, one per node.
#[derive(Debug)]
pub(crate) struct Call<'a> {
    /// The problem, its tables borrowed from C.
    problem: kahlnws::Problem<'a>,
    /// Where each arc's flow goes, arc `a` at `a`.
    flows: &'a mut [MaybeUninit<u64>],
    /// Where each node's potential goes, node `v` at `v`; `None` where C does not want them.
    potentials: Option<&'a mut [MaybeUninit<i64>]>,
    /// Where the cost of the flow goes; `None` where C does not want it.
    total_cost: Option<&'a mut MaybeUninit<Int128>>,
    /// Where the work of a solve that answered goes; `None` where C does not want it.
    work: Option<&'a mut MaybeUninit<Work>>,
    /// Where the refusal of a refused problem goes; `None` where C does not want it.
    refusal: Option<&'a mut MaybeUninit<Refusal>>,
}

/// The entry a solve is called by, which decides the basis it starts from.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Entry {
    /// `kahlnws_solver_solve`: from the problem alone.
    Solve,
    /// `kahlnws_solver_resolve`: from the basis the solver's last solve ended on, where it serves the problem.
    Resolve,
}

impl Solver {
    /// A solver that holds no room yet; it allocates nothing.
    pub(crate) const fn new() -> Self {
        Self { simplex: NetworkSimplex::new(), flows: Vec::new() }
    }

    /// Makes room for problems of up to `node_count` nodes and `arc_count` arcs, and answers
    /// [`Status::Reserved`]; [`Status::NoRoom`] where the memory is refused, the room already held kept.
    pub(crate) fn reserve(&mut self, node_count: usize, arc_count: usize) -> Status {
        // `try_reserve` counts from the entries held, so room for `arc_count` in all asks for that many less them.
        let flows = self.flows.try_reserve(arc_count.saturating_sub(self.flows.len()));

        match (self.simplex.try_reserve(node_count, arc_count), flows) {
            (Ok(()), Ok(())) => Status::Reserved,
            _ => Status::NoRoom,
        }
    }

    /// Solves the problem of `call` as `entry` does, writes what it found and its work where `call` says, and answers
    /// how it went: [`Status::Optimal`], [`Status::Infeasible`], [`Status::Refused`], or [`Status::NoRoom`] where the
    /// memory for the room the problem needs is refused. Only an optimal solve writes the flows, the potentials and
    /// the cost, an optimal or an infeasible one the work, and only a refused one the refusal. A solve that answers
    /// keeps the basis it ended on; one that does not keeps the basis kept before.
    pub(crate) fn solve(&mut self, entry: Entry, mut call: Call<'_>) -> Status {
        // The room is made before the solve, by a reservation that can be refused: a solve that grew its own room
        // could only end the process where the memory runs out.
        if self.reserve(call.node_count(), call.arc_count()) == Status::NoRoom {
            return Status::NoRoom;
        }

        // Within the room just reserved, so this allocates nothing; entries already held keep what they hold, since
        // the solve writes every flow it answers and nothing reads them before.
        self.flows.resize(call.arc_count(), 0);

        let answered = match entry {
            Entry::Solve => self.simplex.solve_into(&call.problem, &mut self.flows),
            Entry::Resolve => self.simplex.resolve_into(&call.problem, &mut self.flows),
        };
        let Answer { outcome, work } = match answered {
            Ok(answer) => answer,
            Err(breach) => {
                call.write_refusal(breach);
                return Status::Refused;
            }
        };

        call.write_work(work);
        match outcome {
            Outcome::Optimal(optimum) => {
                call.write_optimum(&self.flows, optimum);
                Status::Optimal
            }
            Outcome::Infeasible => Status::Infeasible,
        }
    }
}

impl<'a> Call<'a> {
    /// The call of `problem` whose answer goes to `flows` and, where given, to `potentials`, `total_cost`, `work` and
    /// `refusal`.
    ///
    /// # Panics
    /// Panics if a per-arc table of `problem` holds another number of entries than its tails, if `flows` holds
    /// another number of entries than `problem` has arcs, or `potentials` another number than it has nodes.
    #[track_caller]
    pub(crate) fn new(
        problem: kahlnws::Problem<'a>,
        flows: &'a mut [MaybeUninit<u64>],
        potentials: Option<&'a mut [MaybeUninit<i64>]>,
        total_cost: Option<&'a mut MaybeUninit<Int128>>,
        work: Option<&'a mut MaybeUninit<Work>>,
        refusal: Option<&'a mut MaybeUninit<Refusal>>,
    ) -> Self {
        let (arc_count, node_count) = (problem.tails.len(), problem.supplies.len());
        let lower_bounds = problem.lower_bounds.map_or(arc_count, <[u64]>::len);
        let tables = [problem.heads.len(), lower_bounds, problem.capacities.len(), problem.costs.len()];
        assert!(
            tables.iter().all(|&length| length == arc_count),
            "called `Call::new` with per-arc tables of lengths {tables:?}, which should each be the arc count \
             {arc_count}"
        );
        assert!(
            flows.len() == arc_count,
            "called `Call::new` with {} flows, which should be the arc count {arc_count}",
            flows.len()
        );
        if let Some(potentials) = &potentials {
            assert!(
                potentials.len() == node_count,
                "called `Call::new` with {} potentials, which should be the node count {node_count}",
                potentials.len()
            );
        }

        Self { problem, flows, potentials, total_cost, work, refusal }
    }

    /// The number of nodes of the problem.
    fn node_count(&self) -> usize {
        self.problem.supplies.len()
    }

    /// The number of arcs of the problem.
    fn arc_count(&self) -> usize {
        self.problem.tails.len()
    }

    /// Writes `flows`, and the potentials and the cost of `optimum` where C wants them.
    fn write_optimum(self, flows: &[u64], optimum: Optimum<'_>) {
        // One flow per arc and one potential per node on both sides: the solve answers them for the problem.
        self.flows.write_copy_of_slice(flows);
        if let Some(potentials) = self.potentials {
            potentials.write_copy_of_slice(optimum.potentials);
        }
        if let Some(total_cost) = self.total_cost {
            total_cost.write(Int128::from(optimum.total_cost));
        }
    }

    /// Writes `work`, what a solve that was not refused did, where C wants it.
    fn write_work(&mut self, work: kahlnws::Work) {
        if let Some(output) = &mut self.work {
            output.write(Work::from(work));
        }
    }

    /// Writes the refusal that names `breach` where C wants it.
    fn write_refusal(self, breach: ProblemError) {
        if let Some(refusal) = self.refusal {
            // Never `ProblemError::Length`, which `Refusal::of` refuses: the call's invariant rules it out.
            refusal.write(Refusal::of(breach));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The depot: node 0 sends 4 units to node 2, directly by arc 0 (capacity 3, cost 5) or through node 1 by arcs 1
    /// and 2 (capacity 4 and 2, cost 1 each).
    fn depot() -> kahlnws::Problem<'static> {
        kahlnws::Problem {
            tails: &[0, 0, 1],
            heads: &[2, 1, 2],
            lower_bounds: None,
            capacities: &[3, 4, 2],
            costs: &[5, 1, 1],
            supplies: &[4, 0, -4],
        }
    }

    /// What one solve of `problem` by `solver` answers and writes, every output given and starting at 7.
    #[derive(Debug, PartialEq, Eq)]
    struct Written {
        /// The status.
        status: Status,
        /// The flows.
        flows: Vec<u64>,
        /// The potentials.
        potentials: Vec<i64>,
        /// The cost.
        total_cost: Int128,
        /// The work.
        work: Work,
        /// The refusal.
        refusal: Refusal,
    }

    /// The values of `entries`, every one of which was written.
    fn written<T: Copy>(entries: &[MaybeUninit<T>]) -> Vec<T> {
        entries
            .iter()
            .map(|entry| {
                // SAFETY: every output of these tests starts written with a marker or is written by the solve.
                unsafe { entry.assume_init() }
            })
            .collect()
    }

    /// The value of `entry`, which was written.
    fn written_one<T: Copy>(entry: &MaybeUninit<T>) -> T {
        written(core::slice::from_ref(entry))[0]
    }

    /// What a solve that wrote nothing leaves of `problem`'s outputs, with `status`: every entry and every count 7,
    /// the refusal naming no breach at arc 7.
    fn untouched(problem: kahlnws::Problem<'_>, status: Status) -> Written {
        Written {
            status,
            flows: vec![7; problem.tails.len()],
            potentials: vec![7; problem.supplies.len()],
            total_cost: Int128 { low: 7, high: 7 },
            work: Work { pivots: 7, degenerate_pivots: 7, first_arc_pivots: 7 },
            refusal: Refusal { arc: 7, ..Refusal::default() },
        }
    }

    /// Solves `problem` with `solver` as `entry` does, every output given and starting as [`untouched`] says, and
    /// answers what it wrote.
    fn solved(solver: &mut Solver, entry: Entry, problem: kahlnws::Problem<'_>) -> Written {
        let before = untouched(problem, Status::Optimal);
        let mut flows: Vec<_> = before.flows.into_iter().map(MaybeUninit::new).collect();
        let mut potentials: Vec<_> = before.potentials.into_iter().map(MaybeUninit::new).collect();
        let mut total_cost = MaybeUninit::new(before.total_cost);
        let mut work = MaybeUninit::new(before.work);
        let mut refusal = MaybeUninit::new(before.refusal);

        let call = Call::new(
            problem,
            &mut flows,
            Some(&mut potentials),
            Some(&mut total_cost),
            Some(&mut work),
            Some(&mut refusal),
        );
        let status = solver.solve(entry, call);

        Written {
            status,
            flows: written(&flows),
            potentials: written(&potentials),
            total_cost: written_one(&total_cost),
            work: written_one(&work),
            refusal: written_one(&refusal),
        }
    }

    /// The work of the depot: four pivots, one of them degenerate, none of a first arc.
    const DEPOT_WORK: Work = Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 };

    #[test]
    fn test_an_optimal_solve_writes_the_flows_the_potentials_the_cost_and_the_work() {
        let written = solved(&mut Solver::new(), Entry::Solve, depot());

        assert_eq!(
            written,
            Written {
                status: Status::Optimal,
                flows: vec![2, 2, 2],
                potentials: vec![0, 1, 5],
                total_cost: Int128 { low: 14, high: 0 },
                work: DEPOT_WORK,
                ..untouched(depot(), Status::Optimal)
            }
        );
    }

    #[test]
    fn test_an_optimal_solve_needs_no_values_in_its_outputs() {
        let mut flows = [MaybeUninit::uninit(); 3];
        let mut potentials = [MaybeUninit::uninit(); 3];
        let mut total_cost = MaybeUninit::uninit();
        let mut work = MaybeUninit::uninit();
        let call = Call::new(depot(), &mut flows, Some(&mut potentials), Some(&mut total_cost), Some(&mut work), None);

        assert_eq!(Solver::new().solve(Entry::Solve, call), Status::Optimal);
        assert_eq!((written(&flows), written(&potentials)), (vec![2, 2, 2], vec![0, 1, 5]));
        assert_eq!((written_one(&total_cost), written_one(&work)), (Int128 { low: 14, high: 0 }, DEPOT_WORK));
    }

    #[test]
    fn test_an_infeasible_solve_writes_the_work_and_nothing_else() {
        // Node 0 must send 2 units to node 1, but the one arc from 0 to 1 carries 1; arc 1 runs the other way. Arc 0
        // enters and fills, with no link leaving; then nothing violates.
        let problem = kahlnws::Problem {
            tails: &[0, 1],
            heads: &[1, 0],
            lower_bounds: None,
            capacities: &[1, 5],
            costs: &[1, 1],
            supplies: &[2, -2],
        };

        let work = Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 0 };

        assert_eq!(
            solved(&mut Solver::new(), Entry::Solve, problem),
            Written { work, ..untouched(problem, Status::Infeasible) }
        );
    }

    #[test]
    fn test_a_refused_solve_writes_the_refusal_and_nothing_else() {
        let problem = kahlnws::Problem { supplies: &[4, 0, -3], ..depot() };
        let refusal = Refusal::of(ProblemError::Unbalanced { total: 1 });

        assert_eq!(
            solved(&mut Solver::new(), Entry::Solve, problem),
            Written { refusal, ..untouched(problem, Status::Refused) }
        );
    }

    #[test]
    fn test_outputs_not_wanted_are_not_written() {
        let mut flows = [MaybeUninit::uninit(); 3];
        let call = Call::new(depot(), &mut flows, None, None, None, None);

        assert_eq!(Solver::new().solve(Entry::Solve, call), Status::Optimal);
        assert_eq!(written(&flows), [2, 2, 2]);
    }

    #[test]
    fn test_a_reservation_answers_reserved_or_no_room_and_keeps_the_solver_usable() {
        let mut solver = Solver::new();

        assert_eq!(solver.reserve(3, 3), Status::Reserved);
        assert!(solver.flows.capacity() >= 3, "the flows have room for three arcs");
        assert_eq!(solver.reserve(usize::MAX, 0), Status::NoRoom);
        assert_eq!(solver.reserve(0, usize::MAX), Status::NoRoom);
        assert_eq!(solved(&mut solver, Entry::Solve, depot()).status, Status::Optimal);
    }

    #[test]
    fn test_a_solver_may_move_between_threads() {
        /// Compiles only for a type that may move to another thread.
        fn movable<T: Send>() {}

        movable::<Solver>();
    }

    #[test]
    fn test_one_solver_answers_each_problem_as_a_fresh_one() {
        let mut solver = Solver::new();
        let refused = kahlnws::Problem { supplies: &[4, 0, -3], ..depot() };

        assert_eq!(solved(&mut solver, Entry::Solve, refused).status, Status::Refused);
        assert_eq!(solved(&mut solver, Entry::Solve, depot()), solved(&mut Solver::new(), Entry::Solve, depot()));
        assert_eq!(solved(&mut solver, Entry::Solve, refused), solved(&mut Solver::new(), Entry::Solve, refused));
    }

    #[test]
    fn test_a_resolve_of_the_problem_solved_last_makes_no_pivot_and_answers_its_optimum() {
        let mut solver = Solver::new();
        let first = solved(&mut solver, Entry::Solve, depot());

        // The basis the solve ended on is the optimum of the same numbers: nothing violates, and nothing moves.
        let again = solved(&mut solver, Entry::Resolve, depot());
        assert_eq!(first.work, DEPOT_WORK);
        assert_eq!(again, Written { work: Work::default(), ..first });
    }

    #[test]
    fn test_a_resolve_of_a_solver_that_never_solved_answers_what_a_solve_answers() {
        let infeasible = kahlnws::Problem { capacities: &[1, 1, 1], ..depot() };
        let refused = kahlnws::Problem { supplies: &[4, 0, -3], ..depot() };

        for problem in [depot(), infeasible, refused] {
            let resolved = solved(&mut Solver::new(), Entry::Resolve, problem);

            assert_eq!(resolved, solved(&mut Solver::new(), Entry::Solve, problem), "{problem:?}");
        }
    }

    #[test]
    fn test_a_resolve_of_other_counts_answers_what_a_solve_answers() {
        // The chain has four nodes where the depot has three: the depot's basis cannot serve it.
        let chain = kahlnws::Problem {
            tails: &[0, 1, 2],
            heads: &[1, 2, 3],
            lower_bounds: None,
            capacities: &[5, 5, 5],
            costs: &[1, 1, 1],
            supplies: &[2, 0, 0, -2],
        };
        let mut solver = Solver::new();
        assert_eq!(solved(&mut solver, Entry::Solve, depot()).status, Status::Optimal);

        assert_eq!(solved(&mut solver, Entry::Resolve, chain), solved(&mut Solver::new(), Entry::Solve, chain));
    }

    #[test]
    fn test_a_refused_resolve_and_a_refused_reservation_keep_the_basis() {
        let mut solver = Solver::new();
        assert_eq!(solved(&mut solver, Entry::Solve, depot()).work, DEPOT_WORK);

        let refused = kahlnws::Problem { supplies: &[4, 0, -3], ..depot() };
        assert_eq!(solved(&mut solver, Entry::Resolve, refused).status, Status::Refused);
        assert_eq!(solver.reserve(usize::MAX, 0), Status::NoRoom);

        assert_eq!(solved(&mut solver, Entry::Resolve, depot()).work, Work::default());
    }

    #[test]
    #[should_panic(expected = "called `Call::new` with per-arc tables of lengths [3, 3, 2, 3], which should each be \
                               the arc count 3")]
    fn test_a_call_with_a_table_of_another_length_panics() {
        let problem = kahlnws::Problem { capacities: &[3, 4], ..depot() };
        let _ = Call::new(problem, &mut [MaybeUninit::uninit(); 3], None, None, None, None);
    }

    #[test]
    #[should_panic(expected = "called `Call::new` with 2 flows, which should be the arc count 3")]
    fn test_a_call_with_flows_of_another_length_panics() {
        let _ = Call::new(depot(), &mut [MaybeUninit::uninit(); 2], None, None, None, None);
    }

    #[test]
    #[should_panic(expected = "called `Call::new` with 4 potentials, which should be the node count 3")]
    fn test_a_call_with_potentials_of_another_length_panics() {
        let potentials = Some(&mut [MaybeUninit::uninit(); 4][..]);
        let _ = Call::new(depot(), &mut [MaybeUninit::uninit(); 3], potentials, None, None, None);
    }
}
