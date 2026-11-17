// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The solver: the room it keeps, the reservation, the two entries, `solve` and `solve_into`, and the run of one
//! problem in that room.

use std::collections::TryReserveError;

use super::block_search::BlockSearch;
use super::first_pivots::FirstPivots;
use super::optimum::Optimum;
use crate::basis::{BasisRoom, LoadedBasis};
use crate::outcome::{Answer, Outcome, Solution, Work};
use crate::problem::{CheckedProblem, Problem, ProblemError};
use crate::slot_order::Slot;

/// A primal network simplex with the room it works in: a function from a problem to its answer.
///
/// It solves a [`Problem`] in exact integer arithmetic: it finds a flow of least cost, or that no flow within the
/// bounds meets the supplies. [`solve`](Self::solve) answers the flows, the potentials and the cost owned;
/// [`solve_into`](Self::solve_into) writes the flows to a buffer the caller holds and lends the potentials. Either
/// answers its outcome as an [`Answer`], beside the [`Work`] the solve did.
///
/// Nothing of an earlier problem is kept but room: an answer, its work included, is a function of the problem alone,
/// its arc order included. The room is kept from one solve to the next and only grows: a solve of a problem with no
/// more nodes and no more arcs than the room was made for, by [`try_reserve`](Self::try_reserve) or by an earlier
/// solve that answered, optimal or infeasible, allocates nothing in `solve_into`. A solve that was refused makes no
/// such room. To hold one problem and change it, a [`Network`](crate::Network) is the state beside this function.
///
/// ```
/// use kahlnws::{NetworkSimplex, Problem, ProblemError};
///
/// fn main() -> Result<(), ProblemError> {
///     // Node 0 sends 2 units to node 1 over two parallel arcs; the cheaper one carries 1.
///     let problem = Problem {
///         tails: &[0, 0],
///         heads: &[1, 1],
///         lower_bounds: None,
///         capacities: &[2, 1],
///         costs: &[3, 1],
///         supplies: &[2, -2],
///     };
///
///     let mut solver = NetworkSimplex::new();
///     let solution = solver.solve(&problem)?.outcome.optimal().expect("two units should fit the two arcs");
///     assert_eq!((solution.flows, solution.total_cost), (vec![1, 1], 4));
///
///     // The cheaper arc narrowed to nothing: the wide arc carries both units.
///     let narrowed = Problem { capacities: &[2, 0], ..problem };
///     let solution = solver.solve(&narrowed)?.outcome.optimal().expect("the wide arc should carry both units");
///     assert_eq!((solution.flows, solution.total_cost), (vec![2, 0], 6));
///
///     Ok(())
/// }
/// ```
#[derive(Clone, Debug, Default)]
pub struct NetworkSimplex {
    /// The room of the basis a run pivots on, with the room of the net supplies it starts from.
    basis: BasisRoom,
    /// The room of the arcs tried before block search.
    first_pivots: FirstPivots,
}

/// One run of the network simplex on one problem, in the room of a solver: the basis loaded from the problem's start,
/// the arcs tried before block search, and the work done on it so far.
///
/// # Invariant
/// Only [`Run::from_star`] makes one: `first_arcs` are slots below the arc count of the problem `basis` was loaded
/// from, those the first pivots chose for the problem, whose star the basis is. `work` counts every pivot made on
/// `basis` since the load, and nothing else; every pivot made before block search is a first arc's, so
/// `first_arc_pivots` is `pivots` as the first arcs leave it.
#[derive(Debug)]
struct Run<'s, 'p> {
    /// The basis the run pivots on, loaded from the problem.
    basis: LoadedBasis<'s, 'p>,
    /// The slots of the arcs tried before block search, in the order they are tried.
    first_arcs: &'s [Slot],
    /// The pivots made on the basis so far.
    work: Work,
}

impl NetworkSimplex {
    /// A solver that holds no room yet; it allocates nothing.
    pub const fn new() -> Self {
        Self { basis: BasisRoom::new(), first_pivots: FirstPivots::new() }
    }

    /// Makes room for problems of up to `node_count` nodes and `arc_count` arcs: a [`solve_into`](Self::solve_into)
    /// of such a problem allocates nothing afterwards. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    ///
    /// ```
    /// use kahlnws::NetworkSimplex;
    ///
    /// let mut solver = NetworkSimplex::new();
    /// assert!(solver.try_reserve(1000, 4000).is_ok());
    /// ```
    pub fn try_reserve(&mut self, node_count: usize, arc_count: usize) -> Result<(), TryReserveError> {
        self.basis.try_reserve(node_count, arc_count)?;
        self.first_pivots.try_reserve(node_count, arc_count)
    }

    /// Solves `problem` and writes each arc's flow to `flows`, arc `a` at `a`: answers the cost of that flow and the
    /// potentials that prove it optimal, lent by the solver, or that no flow within the bounds meets the supplies,
    /// with the [`Work`] the solve did either way. On [`Outcome::Infeasible`] and on an error `flows` is as it was.
    ///
    /// Where the solver's room holds the problem, it allocates nothing.
    ///
    /// # Panics
    /// Panics if `flows` holds another number of entries than `problem` has tails, one per arc: the buffer is compared
    /// first, so a wrong buffer panics whether or not the problem would be refused. On a target whose `usize` has 32
    /// bits it can also panic while it grows its room, where a column's room passes `isize::MAX` bytes.
    ///
    /// # Errors
    /// Given a buffer of the right length, returns the first breach of `problem`, as [`ProblemError`] orders them.
    ///
    /// ```
    /// use kahlnws::{NetworkSimplex, Outcome, Problem};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Node 0 sends 3 units to node 1, which takes them on to node 2.
    ///     let problem = Problem {
    ///         tails: &[0, 1],
    ///         heads: &[1, 2],
    ///         lower_bounds: None,
    ///         capacities: &[5, 5],
    ///         costs: &[2, -1],
    ///         supplies: &[3, 0, -3],
    ///     };
    ///
    ///     let mut solver = NetworkSimplex::new();
    ///     solver.try_reserve(problem.supplies.len(), problem.tails.len())?;
    ///     let mut flows = vec![0; problem.tails.len()];
    ///
    ///     let Outcome::Optimal(optimum) = solver.solve_into(&problem, &mut flows)?.outcome else {
    ///         unreachable!("the path carries the supply");
    ///     };
    ///     assert_eq!((optimum.total_cost, optimum.potentials.len()), (3, 3));
    ///     assert_eq!(flows, [3, 3]);
    ///
    ///     Ok(())
    /// }
    /// ```
    #[track_caller]
    pub fn solve_into<'s>(
        &'s mut self,
        problem: &Problem<'_>,
        flows: &mut [u64],
    ) -> Result<Answer<Optimum<'s>>, ProblemError> {
        let (length, arc_count) = (flows.len(), problem.tails.len());
        assert!(
            length == arc_count,
            "called `NetworkSimplex::solve_into` with {length} flows, which should be the arc count {arc_count}"
        );

        Ok(Run::from_star(self, problem.checked()?)?.run_to_end(flows))
    }

    /// Solves `problem` and answers its flows, its potentials and the cost, owned, or that no flow within the bounds
    /// meets the supplies, with the [`Work`] the solve did either way: the work [`solve_into`](Self::solve_into)
    /// answers for the same problem.
    ///
    /// It allocates the flows and the potentials it answers, and room for a problem larger than the solver held.
    ///
    /// # Panics
    /// On a target whose `usize` has 32 bits it can panic while it allocates the flows or grows its room, where their
    /// room passes `isize::MAX` bytes.
    ///
    /// # Errors
    /// Returns the first breach of `problem`, as [`ProblemError`] orders them.
    ///
    /// ```
    /// use kahlnws::{NetworkSimplex, Outcome, Problem, ProblemError};
    ///
    /// fn main() -> Result<(), ProblemError> {
    ///     // Node 0 must send 2 units to node 1, but its one arc carries 1.
    ///     let problem = Problem {
    ///         tails: &[0],
    ///         heads: &[1],
    ///         lower_bounds: None,
    ///         capacities: &[1],
    ///         costs: &[1],
    ///         supplies: &[2, -2],
    ///     };
    ///
    ///     assert_eq!(NetworkSimplex::new().solve(&problem)?.outcome, Outcome::Infeasible);
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn solve(&mut self, problem: &Problem<'_>) -> Result<Answer<Solution>, ProblemError> {
        let mut flows = vec![0; problem.tails.len()];
        let answer = self.solve_into(problem, &mut flows)?;

        Ok(owned(answer, flows))
    }
}

impl<'s, 'p> Run<'s, 'p> {
    /// The run of `problem` in the room of `solver` from its star: its net supplies gathered, the basis loaded as the
    /// star of its start at the problem's own root cost, the first arcs chosen, and no work done yet. Nothing of an
    /// earlier run is read.
    ///
    /// # Errors
    /// Returns [`ProblemError::NetSupplyBeyondRange`] where the positive net supplies pass their limit.
    fn from_star(solver: &'s mut NetworkSimplex, problem: CheckedProblem<'p>) -> Result<Self, ProblemError> {
        let NetworkSimplex { basis, first_pivots } = solver;
        let basis = basis.load(problem)?;
        first_pivots.choose(&basis.start());

        Ok(Self { basis, first_arcs: first_pivots.chosen(), work: Work::default() })
    }

    /// The run carried to its end: its first arcs entered, then the arcs block search chooses until none violates; the
    /// answer with the work, each arc's flow written to `flows` where it is optimal.
    ///
    /// # Panics
    /// Panics if `flows` holds another number of entries than the problem has arcs.
    #[track_caller]
    fn run_to_end(mut self, flows: &mut [u64]) -> Answer<Optimum<'s>> {
        self.enter_first_arcs();
        self.enter_by_block_search();

        self.answer(flows)
    }

    /// Enters each first arc in turn where it violates its bound when its turn comes, and counts the pivots made as
    /// the first arcs'.
    fn enter_first_arcs(&mut self) {
        // Each slot is below the arc count of the basis's problem (`Run`), so neither call panics, and an arc that
        // violates is priced.
        for &slot in self.first_arcs {
            if self.basis.violates(slot) {
                self.pivot(slot);
            }
        }

        self.work.first_arc_pivots = self.work.pivots;
    }

    /// Enters the arc block search chooses until no arc violates its bound.
    fn enter_by_block_search(&mut self) {
        // The search is made for the rows it reads, so it never panics; it answers only arcs that violate, which are
        // priced. Every pivot ends whatever arc enters, so the loop ends.
        let mut search = BlockSearch::new(self.basis.rows().len());

        while let Some(slot) = search.next_entering(&self.basis.rows()) {
            self.pivot(slot);
        }
    }

    /// Brings the arc at `slot`, priced and below the arc count, into the basis, and counts the pivot in the work:
    /// as degenerate where it sent no flow.
    ///
    /// # Panics
    /// Panics if `slot` is not below the arc count or its arc is not priced, as [`LoadedBasis::pivot`] does by the
    /// checks of `Basis::pivot`.
    #[track_caller]
    fn pivot(&mut self, slot: Slot) {
        let step = self.basis.pivot(slot);

        self.work.count_pivot(step);
    }

    /// The answer of a run no arc violates in, with the work done on it: [`Outcome::Infeasible`] where a root link
    /// carries flow, `flows` left as it was; otherwise each arc's flow written to `flows`, with its cost and the
    /// potentials, lent for as long as the solver is.
    ///
    /// # Panics
    /// Panics if `flows` holds another number of entries than the problem has arcs.
    #[track_caller]
    fn answer(self, flows: &mut [u64]) -> Answer<Optimum<'s>> {
        let Self { basis, work, .. } = self;

        let outcome = if basis.routes_every_supply() {
            let total_cost = basis.write_flows(flows);
            Outcome::Optimal(Optimum { total_cost, potentials: basis.keep() })
        } else {
            basis.keep();
            Outcome::Infeasible
        };

        Answer { outcome, work }
    }
}

/// The answer of an entry that writes into `flows`, `answer`, with the flows and a copy of the lent potentials owned:
/// what the owning entries answer.
fn owned(answer: Answer<Optimum<'_>>, flows: Vec<u64>) -> Answer<Solution> {
    let Answer { outcome, work } = answer;
    let outcome = match outcome {
        // The potentials are the solver's own column, lent until the next solve; the solution owns a copy.
        Outcome::Optimal(Optimum { total_cost, potentials }) => {
            Outcome::Optimal(Solution { flows, potentials: potentials.to_vec(), total_cost })
        }
        Outcome::Infeasible => Outcome::Infeasible,
    };

    Answer { outcome, work }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use kahlnws_testing::{MincostNetwork, check_certificate};

    use super::*;
    use crate::basis::check;
    use crate::problem::tests::problem_of;

    /// The depot sending 4 units to the customer through the hub, its two hub arcs of capacity 4: arc 0 from 0 to 2
    /// (capacity 3, cost 5), arc 1 from 0 to 1 and arc 2 from 1 to 2 (capacity 4, cost 1 each). Three nodes and three
    /// arcs: slot `s` holds arc `s`, and the root cost is `2 · 5 + 1 = 11`.
    fn wide_depot() -> MincostNetwork {
        MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 4, 1)])
    }

    /// Each arc's violation in the run's basis, in slot order.
    fn violations(run: &Run<'_, '_>) -> Vec<i64> {
        let rows = run.basis.rows();

        rows.violations(0..rows.len()).map(|(_, violation)| violation).collect()
    }

    /// What a basis writes out. Plain data.
    #[derive(Debug, PartialEq, Eq)]
    struct Written {
        /// Per arc: its flow.
        flows: Vec<u64>,
        /// The cost of the flows.
        cost: i128,
    }

    /// What the run's basis writes out for its `arc_count` arcs.
    fn written(run: &Run<'_, '_>, arc_count: usize) -> Written {
        let mut flows = vec![0; arc_count];
        let cost = run.basis.write_flows(&mut flows);

        Written { flows, cost }
    }

    /// Checks the whole basis invariant of `run`, loaded from `network`.
    ///
    /// # Panics
    /// Panics if the invariant fails.
    fn assert_invariant(run: &Run<'_, '_>, network: &MincostNetwork) {
        if let Err(breach) = check(&run.basis, network) {
            panic!("the basis invariant should hold: {breach}");
        }
    }

    #[test]
    fn test_the_run_of_the_wide_depot_step_by_step_by_hand() {
        let network = wide_depot();
        let problem = problem_of(&network);
        let mut solver = NetworkSimplex::new();
        let checked = problem.checked().expect("the depot should be admitted");
        let mut run = Run::from_star(&mut solver, checked).expect("the depot's net supplies should be in range");

        // The star: nodes 0 and 1 up at potential 0, node 2 down at 11. One supplier and one demander: the search
        // back from node 2 over arcs wide enough for 4 takes arc 2 from node 1, then arc 1 from node 0, the supply.
        assert_invariant(&run, &network);
        assert_eq!(run.first_arcs, [Slot::from(2_usize), Slot::from(1_usize)]);
        assert_eq!(violations(&run), [5 - 11, 1, 1 - 11]);
        assert_eq!(written(&run, 3), Written { flows: vec![0, 0, 0], cost: 0 });
        assert_eq!(run.work, Work::default());

        // Arc 2 enters degenerately, node 1's empty root link leaving: node 1 hangs from node 2, its potential up by
        // 10. Arc 1, then `1 + 0 − 10 = −9`, carries the 4 units round by both hub arcs, node 2's root link leaving;
        // nodes 1 and 2 fall by 9 to 1 and 2. Arc 0 is left at `5 + 0 − 2 = 3`. Two pivots of the first arcs, the
        // first degenerate.
        run.enter_first_arcs();
        assert_invariant(&run, &network);
        assert_eq!(violations(&run), [3, 0, 0]);
        assert_eq!(written(&run, 3), Written { flows: vec![0, 4, 4], cost: 8 });
        assert_eq!(run.work, Work { pivots: 2, degenerate_pivots: 1, first_arc_pivots: 2 });

        // Nothing violates: block search enters nothing, and the first arcs alone reached the optimum.
        run.enter_by_block_search();
        assert_eq!(violations(&run), [3, 0, 0]);
        assert_eq!(run.work, Work { pivots: 2, degenerate_pivots: 1, first_arc_pivots: 2 });

        let mut flows = [9; 3];
        let Answer { outcome, work } = run.answer(&mut flows);
        let Outcome::Optimal(optimum) = outcome else { panic!("the depot routes its supply") };
        assert_eq!((flows, optimum.total_cost, optimum.potentials), ([0, 4, 4], 8, &[0, 1, 2][..]));
        assert_eq!(work, Work { pivots: 2, degenerate_pivots: 1, first_arc_pivots: 2 });
        assert_eq!(check_certificate(&network, &flows, optimum.potentials, optimum.total_cost), Ok(()));
    }

    #[test]
    fn test_the_run_of_the_depot_by_block_search_alone_by_hand() {
        // Arc 2 of capacity 2: no arc is wide enough for the 4 units into node 2, and nothing is chosen. Block search
        // in blocks of one enters arcs 0, 2, 1 and 0 again, as the pivots of the basis's own hand-worked run: arc 0
        // flips, sending its width 3; arc 2 enters degenerately; arcs 1 and 0 send one unit each.
        let network = MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1)]);
        let problem = problem_of(&network);
        let mut solver = NetworkSimplex::new();
        let checked = problem.checked().expect("the depot should be admitted");
        let mut run = Run::from_star(&mut solver, checked).expect("the depot's net supplies should be in range");
        assert_eq!(run.first_arcs, []);

        run.enter_first_arcs();
        assert_eq!(violations(&run), [5 - 11, 1, 1 - 11], "no first arc, no pivot");
        assert_eq!(run.work, Work::default());

        run.enter_by_block_search();
        assert_invariant(&run, &network);
        assert_eq!(violations(&run), [0, 0, 3]);
        assert_eq!(run.work, Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 });

        let mut flows = [9; 3];
        let Answer { outcome, work } = run.answer(&mut flows);
        let Outcome::Optimal(optimum) = outcome else { panic!("the depot routes its supply") };
        assert_eq!((flows, optimum.total_cost, optimum.potentials), ([2, 2, 2], 14, &[0, 1, 5][..]));
        assert_eq!(work, Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 });
    }

    #[test]
    fn test_the_answer_of_a_run_that_routes_nothing_leaves_the_flows_and_has_its_work_by_hand() {
        // Node 0 must send 2 units, but its one arc carries 1, too narrow to be chosen as a first arc. Block search
        // enters it at `1 − 2 = −1`, and it flips, sending its width 1.
        let network = MincostNetwork::from_arcs(&[2, -2], &[(0, 1, 0, 1, 1)]);
        let problem = problem_of(&network);
        let mut solver = NetworkSimplex::new();
        let checked = problem.checked().expect("the network should be admitted");
        let mut run = Run::from_star(&mut solver, checked).expect("the net supplies should be in range");
        run.enter_first_arcs();
        run.enter_by_block_search();

        let mut flows = [9];
        let work = Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 0 };
        assert_eq!(run.answer(&mut flows), Answer { outcome: Outcome::Infeasible, work });
        assert_eq!(flows, [9]);
    }

    #[test]
    fn test_a_pivot_of_the_run_counts_itself_and_whether_it_sent_flow_by_hand() {
        // The depot of the basis's hand-worked run: arc 0 flips, sending its width 3; arc 2 then enters degenerately.
        // Neither is a first arc's pivot.
        let network = MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1)]);
        let problem = problem_of(&network);
        let mut solver = NetworkSimplex::new();
        let checked = problem.checked().expect("the depot should be admitted");
        let mut run = Run::from_star(&mut solver, checked).expect("the depot's net supplies should be in range");

        run.pivot(Slot::from(0_usize));
        assert_eq!(run.work, Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 0 });

        run.pivot(Slot::from(2_usize));
        assert_invariant(&run, &network);
        assert_eq!(run.work, Work { pivots: 2, degenerate_pivots: 1, first_arc_pivots: 0 });
    }

    #[test]
    fn test_a_run_refuses_net_supplies_past_their_limit() {
        // Node 0 supplies `2^63 − 1` and an arc into it of lower bound `2^63` adds that much: one past `2^64 − 2`.
        let network = MincostNetwork::from_arcs(&[i64::MAX, i64::MIN + 1], &[(1, 0, 1 << 63, 1 << 63, 0)]);
        let problem = problem_of(&network);
        let mut solver = NetworkSimplex::new();
        let checked = problem.checked().expect("the network should pass the checks before the net supplies");

        let refused = Run::from_star(&mut solver, checked).map(|_| ());
        assert_eq!(refused, Err(ProblemError::NetSupplyBeyondRange { total: u128::from(u64::MAX) }));
    }

    #[test]
    #[should_panic(expected = "called `NetworkSimplex::solve_into` with 2 flows, which should be the arc count 3")]
    fn test_solve_into_refuses_a_buffer_of_another_length() {
        let network = wide_depot();

        let _ = NetworkSimplex::new().solve_into(&problem_of(&network), &mut [0, 0]);
    }

    #[test]
    fn test_a_solver_unwound_out_of_a_run_after_pivots_answers_as_a_fresh_one() {
        // The wide depot's run enters its two first arcs, then is made to pivot arc 1, now in the tree: the pivot
        // panics with the basis half used. The solver then answers the depot and the wide depot as a fresh one.
        let network = wide_depot();
        let depot = MincostNetwork::from_arcs(&[4, 0, -4], &[(0, 2, 0, 3, 5), (0, 1, 0, 4, 1), (1, 2, 0, 2, 1)]);
        let mut solver = NetworkSimplex::new();

        let unwound = catch_unwind(AssertUnwindSafe(|| {
            let checked = problem_of(&network).checked().expect("the depot should be admitted");
            let mut run = Run::from_star(&mut solver, checked).expect("the depot's net supplies should be in range");
            run.enter_first_arcs();
            run.basis.pivot(Slot::from(1_usize));
        }));
        assert!(unwound.is_err(), "a pivot of an arc in the tree should panic");

        for answered in [&depot, &network] {
            assert_eq!(solver.solve(&problem_of(answered)), NetworkSimplex::new().solve(&problem_of(answered)));
        }
    }

    #[test]
    fn test_owned_copies_the_lent_answer_by_hand() {
        let potentials = [0, 1, 5];
        let work = Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 };
        let lent = Answer { outcome: Outcome::Optimal(Optimum { total_cost: 14, potentials: &potentials }), work };
        let solution = Solution { flows: vec![2, 2, 2], potentials: vec![0, 1, 5], total_cost: 14 };
        assert_eq!(owned(lent, vec![2, 2, 2]), Answer { outcome: Outcome::Optimal(solution), work });

        let infeasible = Answer { outcome: Outcome::Infeasible, work };
        assert_eq!(owned(infeasible, vec![9]), Answer { outcome: Outcome::Infeasible, work });
    }

    #[test]
    fn test_a_solver_made_in_a_const_context_reserves_and_refuses_the_impossible() {
        const SOLVER: NetworkSimplex = NetworkSimplex::new();
        let mut solver = SOLVER;

        solver.try_reserve(3, 5).expect("room for three nodes and five arcs should be granted");
        assert!(solver.try_reserve(usize::MAX, 0).is_err());
        assert!(solver.try_reserve(0, usize::MAX).is_err());
    }
}
