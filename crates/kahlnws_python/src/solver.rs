// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The solver Python holds: its room, its copy of the problem, and the two levels of a solve.

use std::collections::TryReserveError;

use kahlnws::{Optimum, Outcome, Problem};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

use crate::answer::Answer;
use crate::argument::Count;
use crate::breach::refusal;
use crate::integer;
use crate::output::Outputs;
use crate::solution::Solution;
use crate::tables::{Arguments, Tables, no_room};

/// A primal network simplex for the min-cost flow problem on exact integers, with the room it works in.
///
/// A problem is arcs between nodes, given as columns by keyword: arc `a` leaves node `tails[a]`, enters node
/// `heads[a]`, carries at least `lower_bounds[a]` and at most `capacities[a]` units of flow, and costs `costs[a]` per
/// unit; node `v` has the supply `supplies[v]`, positive where it sends flow and negative where it takes it. A solve
/// finds a flow of least cost that meets every supply within the bounds, or that no such flow exists. Every number is
/// an integer, and every sum a solve forms is exact.
///
/// Each column is a sequence of `int` (a list, a tuple, a range: anything with a length and indexing that is no
/// mapping) or a buffer of integers of one dimension (a `numpy.ndarray`, an `array.array`, a `memoryview`), strided
/// or not. Every column is copied before the solve starts. A buffer whose items are exactly the column's type is
/// copied in one block: unsigned integers as wide as `size_t` for the tails and the heads (64-bit on the platforms
/// tested), unsigned 64-bit for the lower bounds and the capacities, signed 64-bit for the costs and the supplies;
/// with NumPy, `numpy.uintp`, `numpy.uint64` and `numpy.int64`. A buffer of other integers, such as tails of
/// `numpy.int64`, NumPy's default, is converted item by item with a range check; a buffer of `bool` reads as 0 and 1,
/// and a buffer of Python objects as the sequence it is. Integers in the other byte order than the machine's, floats,
/// characters and items of no size are refused.
///
/// `solve` answers an `Answer[Solution]`, whose outcome owns plain lists; `solve_into` writes into buffers the
/// caller owns, made once and reused, and answers an `Answer[int]` with the cost. Both answer the `Work` the solve
/// did. The solver keeps its room and nothing else: an answer, its work included, is a function of the problem alone,
/// whatever the solver solved before. The room is kept from one solve to the next and only grows: a solve of a problem
/// with no more nodes and no more arcs than the room was made for, by `reserve` or by an earlier solve, allocates no
/// room, and a solve that is refused may still have grown it. A problem held and changed, or loaded again with new
/// numbers on the same arcs, is a `Network`'s.
///
/// The solve runs with the interpreter released, on the solver's own copy of the columns: other threads run
/// meanwhile, and nothing they do reaches the solve. A column that another thread writes, without the interpreter,
/// while it is being copied is the caller's race: its values may be torn, and the answer is that of the columns as
/// read. A running solve cannot be interrupted; a `KeyboardInterrupt` is raised once it returns. One solver serves one
/// call at a time: a call on a solver that is already in a call, from another thread or from code a column runs while
/// it is read, raises `RuntimeError`. A solver cannot be pickled or copied. The package needs the GIL: it has not been
/// tested on free-threaded interpreters.
///
/// Example:
///     >>> import kahlnws
///     >>> solver = kahlnws.NetworkSimplex()
///     >>> answer = solver.solve(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
///     ...                       supplies=[4, 0, -4])
///     >>> answer.outcome.flows, answer.outcome.total_cost
///     ([2, 2, 2], 14)
#[pyclass(module = "kahlnws")]
#[derive(Debug, Default)]
pub(crate) struct NetworkSimplex {
    /// The library's solver with the room for its flows.
    solver: Solver,
    /// The solver's copy of the problem it is handed.
    tables: Tables,
}

/// The library's solver, with the room its flows are solved into.
#[derive(Debug, Default)]
struct Solver {
    /// The solver, with its room.
    simplex: kahlnws::NetworkSimplex,
    /// The flow of each arc of the problem last solved, one per arc; its contents mean nothing between solves.
    flows: Vec<u64>,
}

/// What an optimal solve answers, lent by the solver: the flows beside the optimum.
#[derive(Clone, Copy, Debug)]
struct Lent<'s> {
    /// The flow of each arc, one per arc.
    flows: &'s [u64],
    /// The cost of the flow and the potentials.
    optimum: Optimum<'s>,
}

#[pymethods]
impl NetworkSimplex {
    /// Make a solver that holds no room yet.
    #[new]
    fn new() -> Self {
        Self::default()
    }

    /// Make room for problems of up to `node_count` nodes and `arc_count` arcs.
    ///
    /// A solve of such a problem allocates no room afterwards. Room already held is kept.
    ///
    /// Args:
    ///     node_count: The most nodes a problem will have.
    ///     arc_count: The most arcs a problem will have.
    ///
    /// Raises:
    ///     MemoryError: The memory is refused; the room held before is kept, and the solver may be used.
    ///     OverflowError: A count is below zero or past the platform's `size_t`.
    ///     TypeError: A count is no integer.
    ///     RuntimeError: The solver is already in a call.
    ///
    /// Example:
    ///     >>> kahlnws.NetworkSimplex().reserve(node_count=1000, arc_count=4000)
    #[pyo3(signature = (node_count, arc_count))]
    fn reserve(slf: &Bound<'_, Self>, node_count: &Bound<'_, PyAny>, arc_count: &Bound<'_, PyAny>) -> PyResult<()> {
        let node_count = integer::count(Count::NodeCount, node_count)?;
        let arc_count = integer::count(Count::ArcCount, arc_count)?;
        let mut this = borrow(slf)?;
        let this = &mut *this;

        this.solver
            .try_reserve(node_count, arc_count)
            .and_then(|()| this.tables.try_reserve(node_count, arc_count))
            .map_err(|_| no_room(node_count, arc_count))
    }

    /// Solve the problem and answer what the solve found with the work it did.
    ///
    /// Args:
    ///     tails: The node each arc leaves, one per arc.
    ///     heads: The node each arc enters, one per arc.
    ///     capacities: The most flow of each arc, one per arc.
    ///     costs: The cost of a unit of flow on each arc, of either sign, one per arc.
    ///     supplies: The supply of each node: positive where it sends flow, negative where it takes it.
    ///     lower_bounds: The least flow of each arc, one per arc; left out where every arc's is zero.
    ///
    /// Returns:
    ///     An `Answer[Solution]`: its `outcome` is the `Solution` (the flows, the potentials that prove them optimal,
    ///     and the cost), or `None` where no flow within the bounds meets the supplies; its `work` is the `Work` the
    ///     solve did either way.
    ///
    /// Raises:
    ///     ProblemError: The problem is one no solve admits; its `breach` says why. A `ValueError`.
    ///     TypeError: A column is neither a sequence nor a buffer of integers, an entry is no integer, or a buffer's
    ///         items are no integers in the machine's byte order.
    ///     OverflowError: An entry lies outside the range of its column.
    ///     ValueError: A buffer has other than one dimension.
    ///     MemoryError: The memory for the copy of a column or for the room of the problem is refused.
    ///     RuntimeError: The solver is already in a call.
    ///
    /// What an exporter, a sequence or an entry's `__index__` raises is passed on, with a note that names the argument.
    ///
    /// Example:
    ///     >>> solver = kahlnws.NetworkSimplex()
    ///     >>> solver.solve(tails=[0, 1], heads=[1, 0], capacities=[1, 5], costs=[1, 1], supplies=[2, -2])
    ///     Answer(outcome=None, work=Work(pivots=1, degenerate_pivots=0, first_arc_pivots=0))
    #[pyo3(signature = (*, tails, heads, capacities, costs, supplies, lower_bounds = None))]
    fn solve<'py>(
        slf: &Bound<'py, Self>,
        tails: Bound<'py, PyAny>,
        heads: Bound<'py, PyAny>,
        capacities: Bound<'py, PyAny>,
        costs: Bound<'py, PyAny>,
        supplies: Bound<'py, PyAny>,
        lower_bounds: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Answer> {
        let arguments = Arguments { tails, heads, lower_bounds, capacities, costs, supplies };

        Self::owned_answer(slf, &arguments)
    }

    /// Solve the problem, write its flows, and its potentials where asked, into buffers the caller owns, and answer
    /// the cost with the work the solve did.
    ///
    /// Both outputs are checked before the solve (their type, shape, length, writability, and that they share no
    /// memory) and written only where it finds a flow, each by one copy; where the problem is infeasible or the call
    /// raises, both hold what they held before. Only an exporter that breaks the buffer protocol could make a copy
    /// fail after the checks, and then the flows may be written and the potentials not. The columns are copied before
    /// anything is written, so an output may also be one of them. Where the room holds the problem, the solver
    /// allocates nothing; what a call still makes is a `memoryview` and an export record for each buffer it is
    /// handed, and the answer.
    ///
    /// Args:
    ///     tails: The node each arc leaves, one per arc.
    ///     heads: The node each arc enters, one per arc.
    ///     capacities: The most flow of each arc, one per arc.
    ///     costs: The cost of a unit of flow on each arc, of either sign, one per arc.
    ///     supplies: The supply of each node: positive where it sends flow, negative where it takes it.
    ///     flows: A writable buffer of one dimension with one unsigned 64-bit integer per arc, such as
    ///         `numpy.empty(arc_count, dtype=numpy.uint64)` or `array.array("Q", ...)`.
    ///     lower_bounds: The least flow of each arc, one per arc; left out where every arc's is zero.
    ///     potentials: A writable buffer of one dimension with one signed 64-bit integer per node, such as
    ///         `numpy.empty(node_count, dtype=numpy.int64)`, sharing no memory with `flows`; left out where the
    ///         potentials are not wanted.
    ///
    /// Returns:
    ///     An `Answer[int]`: its `outcome` is the cost of the flow written, an exact `int`, or `None` where no flow
    ///     within the bounds meets the supplies; its `work` is the `Work` the solve did either way.
    ///
    /// Raises:
    ///     ProblemError: The problem is one no solve admits; its `breach` says why. A `ValueError`.
    ///     TypeError: A column is neither a sequence nor a buffer of integers, an entry is no integer, a buffer's
    ///         items are no integers in the machine's byte order, an output is no buffer, or its items are not
    ///         exactly the integers written.
    ///     OverflowError: An entry lies outside the range of its column.
    ///     ValueError: A buffer has other than one dimension, an output is read-only, of another length than the
    ///         problem's arcs or nodes, strided with items that cannot be written as they lie, or shares memory with
    ///         the other.
    ///     MemoryError: The memory for the copy of a column or for the room of the problem is refused.
    ///     RuntimeError: The solver is already in a call.
    ///
    /// What an exporter, a sequence or an entry's `__index__` raises is passed on, with a note that names the argument.
    ///
    /// Example:
    ///     >>> from array import array
    ///     >>> flows = array("Q", [0, 0, 0])
    ///     >>> answer = kahlnws.NetworkSimplex().solve_into(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2],
    ///     ...                                             costs=[5, 1, 1], supplies=[4, 0, -4], flows=flows)
    ///     >>> answer.outcome, flows.tolist()
    ///     (14, [2, 2, 2])
    #[pyo3(signature = (*, tails, heads, capacities, costs, supplies, flows, lower_bounds = None, potentials = None))]
    #[expect(clippy::too_many_arguments, reason = "Python's parameters, one per column of the problem and the answer")]
    fn solve_into<'py>(
        slf: &Bound<'py, Self>,
        tails: Bound<'py, PyAny>,
        heads: Bound<'py, PyAny>,
        capacities: Bound<'py, PyAny>,
        costs: Bound<'py, PyAny>,
        supplies: Bound<'py, PyAny>,
        flows: Bound<'py, PyAny>,
        lower_bounds: Option<Bound<'py, PyAny>>,
        potentials: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Answer> {
        let arguments = Arguments { tails, heads, lower_bounds, capacities, costs, supplies };

        Self::written_answer(slf, &arguments, &flows, potentials.as_ref())
    }
}

impl NetworkSimplex {
    /// What a solve answers for the problem `arguments` hold: the `Solution` it finds, owned by Python, or `None`
    /// where the problem is infeasible, with the work.
    ///
    /// # Errors
    /// Raises `RuntimeError` where the solver is already in a call, what [`Tables::read`] raises for a column that
    /// cannot be read, what [`Solver::solve`] raises, and the `MemoryError` of an answer that cannot be made.
    fn owned_answer(slf: &Bound<'_, Self>, arguments: &Arguments<'_>) -> PyResult<Answer> {
        let py = slf.py();
        let mut this = borrow(slf)?;
        let this = &mut *this;
        let problem = this.tables.read(arguments)?;

        let kahlnws::Answer { outcome, work } = this.solver.solve(py, &problem)?;
        let solution = match outcome {
            Outcome::Optimal(Lent { flows, optimum }) => {
                Some(Solution::of_columns(py, flows, optimum.potentials, optimum.total_cost)?)
            }
            Outcome::Infeasible => None,
        };

        Answer::answered(py, solution, work)
    }

    /// What a solve answers for the problem `arguments` hold, written into `flows` and `potentials` where it finds a
    /// flow: the cost, or `None` where the problem is infeasible, with the work.
    ///
    /// # Errors
    /// Raises `RuntimeError` where the solver is already in a call, what [`Tables::read`] raises for a column that
    /// cannot be read, what [`Outputs::new`] raises for an output that cannot take the answer, what [`Solver::solve`]
    /// raises, what a write that fails after the checks raises, and the `MemoryError` of an answer that cannot be
    /// made.
    fn written_answer(
        slf: &Bound<'_, Self>,
        arguments: &Arguments<'_>,
        flows: &Bound<'_, PyAny>,
        potentials: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Answer> {
        let py = slf.py();
        let mut this = borrow(slf)?;
        let this = &mut *this;
        let problem = this.tables.read(arguments)?;
        let outputs = Outputs::new(flows, potentials, problem.tails.len(), problem.supplies.len())?;

        let kahlnws::Answer { outcome, work } = this.solver.solve(py, &problem)?;
        let total_cost = match outcome {
            Outcome::Optimal(Lent { flows, optimum }) => {
                outputs.write(py, flows, optimum.potentials)?;
                Some(optimum.total_cost)
            }
            Outcome::Infeasible => None,
        };

        Answer::answered(py, total_cost, work)
    }
}

/// The solver `slf` holds, borrowed for one call.
///
/// # Errors
/// Raises `RuntimeError` where the solver is already in a call, on another thread or further up this one.
fn borrow<'py>(slf: &Bound<'py, NetworkSimplex>) -> PyResult<PyRefMut<'py, NetworkSimplex>> {
    slf.try_borrow_mut().map_err(|_| PyRuntimeError::new_err("the solver is already in a call"))
}

impl Solver {
    /// Makes room for problems of up to `node_count` nodes and `arc_count` arcs. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    fn try_reserve(&mut self, node_count: usize, arc_count: usize) -> Result<(), TryReserveError> {
        self.simplex.try_reserve(node_count, arc_count)?;
        self.flows.clear();
        self.flows.try_reserve(arc_count)
    }

    /// Solves `problem` with the interpreter released, and answers the flows with the optimum, or that the problem is
    /// infeasible, with the work the solve did.
    ///
    /// The problem is the solver's own copy and the flows its own room, so no other thread can reach what the solve
    /// reads or writes while the interpreter is released.
    ///
    /// # Errors
    /// Raises `MemoryError` where the room for the problem is refused, and `ProblemError` for a problem no solve
    /// admits.
    fn solve(&mut self, py: Python<'_>, problem: &Problem<'_>) -> PyResult<kahlnws::Answer<Lent<'_>>> {
        // The room is made before the solve, by a reservation that can be refused: a solve that grew its own room
        // could only end the process where the memory runs out.
        let (node_count, arc_count) = (problem.supplies.len(), problem.tails.len());
        self.try_reserve(node_count, arc_count).map_err(|_| no_room(node_count, arc_count))?;

        let Self { simplex, flows } = self;
        flows.resize(arc_count, 0);

        let solved = py.detach(|| simplex.solve_into(problem, flows));
        match solved {
            Ok(kahlnws::Answer { outcome: Outcome::Optimal(optimum), work }) => {
                Ok(kahlnws::Answer { outcome: Outcome::Optimal(Lent { flows, optimum }), work })
            }
            Ok(kahlnws::Answer { outcome: Outcome::Infeasible, work }) => {
                Ok(kahlnws::Answer { outcome: Outcome::Infeasible, work })
            }
            Err(breach) => Err(refusal(py, breach)),
        }
    }
}

#[cfg(test)]
mod tests {
    use kahlnws_testing::{Allocations, allocations_on_this_thread};
    use pyo3::types::{PyInt, PyList};

    use super::*;

    /// The depot's columns as lists, with an `array.array` of three zeros for its flows and one for its potentials.
    struct Depot<'py> {
        /// The tails, the heads, the capacities, the costs and the supplies.
        columns: [Bound<'py, PyAny>; 5],
        /// The room for the flows, of type code `Q`.
        flows: Bound<'py, PyAny>,
        /// The room for the potentials, of type code `q`.
        potentials: Bound<'py, PyAny>,
    }

    impl<'py> Depot<'py> {
        /// The depot of `supplies`, made in `py`.
        fn new(py: Python<'py>, supplies: [i64; 3]) -> PyResult<Self> {
            let list = |entries: &[i64]| PyList::new(py, entries).map(Bound::into_any);
            let array = py.import("array")?.getattr("array")?;

            Ok(Self {
                columns: [list(&[0, 0, 1])?, list(&[2, 1, 2])?, list(&[3, 4, 2])?, list(&[5, 1, 1])?, list(&supplies)?],
                flows: array.call1(("Q", [0, 0, 0]))?,
                potentials: array.call1(("q", [0, 0, 0]))?,
            })
        }

        /// What `solver` allocates on the Rust heap of this thread while it solves the depot into its two rooms, and
        /// the answer it gives.
        fn solve_into(&self, solver: &Bound<'py, NetworkSimplex>) -> PyResult<(Allocations, Bound<'py, PyAny>)> {
            let [tails, heads, capacities, costs, supplies] = self.columns.clone();
            let arguments = Arguments { tails, heads, lower_bounds: None, capacities, costs, supplies };

            let before = allocations_on_this_thread();
            let answer = NetworkSimplex::written_answer(solver, &arguments, &self.flows, Some(&self.potentials))?;
            let after = allocations_on_this_thread();

            Ok((after - before, Bound::new(solver.py(), answer)?.into_any()))
        }
    }

    /// The cost `answer` holds, `None` where it is infeasible, and the pivots of its work.
    fn cost_and_pivots(answer: &Bound<'_, PyAny>) -> PyResult<(Option<i128>, u64)> {
        Ok((answer.getattr("outcome")?.extract()?, answer.getattr("work")?.getattr("pivots")?.extract()?))
    }

    #[test]
    fn test_a_reserved_solve_into_allocates_on_the_rust_heap_only_the_export_records_of_its_buffers() {
        // The interpreter is embedded in this binary. What Python allocates (the views, the answer, its work, the
        // cost) goes through Python's own allocator and is not counted: the count is the solver's and the binding's
        // alone, which is the record the buffer protocol's binding keeps on the Rust heap for each export, two for
        // each output: of its `memoryview`, and of the view of its bytes. Each depot is solved twice in a row, and
        // both solves start from the star: the second makes as many pivots as the first.
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let (optimal, infeasible) = (Depot::new(py, [4, 0, -4])?, Depot::new(py, [9, 0, -9])?);
            let solver = Bound::new(py, NetworkSimplex::new())?;
            NetworkSimplex::reserve(&solver, PyInt::new(py, 3).as_any(), PyInt::new(py, 3).as_any())?;

            // The first call makes the classes of the answer, once for the process; every call after it is measured.
            optimal.solve_into(&solver)?;

            for _ in 0..2 {
                for (depot, cost) in [(&optimal, Some(14)), (&infeasible, None)] {
                    let (allocated, first) = depot.solve_into(&solver)?;
                    let (first_cost, first_pivots) = cost_and_pivots(&first)?;
                    assert_eq!(first_cost, cost);
                    assert!(first_pivots > 0, "a start from the star pivots on either depot");
                    assert_eq!(allocated.blocks, 4, "{allocated}");

                    let (allocated, again) = depot.solve_into(&solver)?;
                    assert_eq!(cost_and_pivots(&again)?, (cost, first_pivots), "the solver keeps no basis");
                    assert_eq!(allocated.blocks, 4, "{allocated}");
                }
            }

            Ok(())
        })
        .expect("the depot should be solved into its rooms");
    }

    /// The depot: node 0 sends 4 units to node 2, directly at 5 a unit for up to 3 units, or through node 1 at 2 a
    /// unit.
    const DEPOT: Problem<'static> = Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    };

    /// The flows, the potentials and the cost of an optimal solve.
    type Solved = (Vec<u64>, Vec<i64>, i128);

    /// What `solver` answers for `problem`: the flows, the potentials and the cost where it is optimal, with the work.
    fn answered(solver: &mut Solver, problem: &Problem<'_>) -> (Option<Solved>, kahlnws::Work) {
        Python::attach(|py| {
            let kahlnws::Answer { outcome, work } = solver.solve(py, problem).expect("the problem should be admitted");
            let solved = match outcome {
                Outcome::Optimal(Lent { flows, optimum }) => {
                    Some((flows.to_vec(), optimum.potentials.to_vec(), optimum.total_cost))
                }
                Outcome::Infeasible => None,
            };

            (solved, work)
        })
    }

    #[test]
    fn test_one_solver_answers_every_problem_as_a_fresh_solver_whatever_it_solved_before() {
        // The chain has four nodes where the depot has three; the infeasible depot's arcs carry 1 unit each. An answer
        // is a function of the problem alone: the depot solved again makes its 4 pivots again.
        let chain = Problem {
            tails: &[0, 1, 2],
            heads: &[1, 2, 3],
            lower_bounds: None,
            capacities: &[5, 5, 5],
            costs: &[1, 1, 1],
            supplies: &[2, 0, 0, -2],
        };
        let infeasible = Problem { capacities: &[1, 1, 1], ..DEPOT };
        let fewer = Problem { supplies: &[3, 0, -3], ..DEPOT };
        Python::initialize();

        let mut solver = Solver::default();
        let (solved, work) = answered(&mut solver, &DEPOT);
        assert_eq!((solved, work.pivots), (Some((vec![2, 2, 2], vec![0, 1, 5], 14)), 4));
        for problem in [DEPOT, infeasible, fewer, chain, DEPOT, fewer] {
            assert_eq!(answered(&mut solver, &problem), answered(&mut Solver::default(), &problem), "{problem:?}");
        }
    }

    #[test]
    fn test_a_call_on_a_solver_in_a_call_raises_our_runtime_error() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let solver = Bound::new(py, NetworkSimplex::new())?;
            let held = solver.borrow_mut();

            let error = NetworkSimplex::reserve(&solver, PyInt::new(py, 1).as_any(), PyInt::new(py, 1).as_any())
                .expect_err("the solver is borrowed");
            assert!(error.is_instance_of::<PyRuntimeError>(py));
            assert_eq!(error.value(py).to_string(), "the solver is already in a call");

            drop(held);
            NetworkSimplex::reserve(&solver, PyInt::new(py, 1).as_any(), PyInt::new(py, 1).as_any())
        })
        .expect("the solver should take the call once it is free");
    }
}
