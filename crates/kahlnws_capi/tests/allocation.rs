// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the C interface allocates: a new solver, exactly its own block and no room; a solve by either entry, nothing
//! after `kahlnws_solver_reserve` for problems of the reserved size and smaller, optimal, infeasible or refused, from
//! the star or from the kept basis, and nothing from the second solve on for a problem no larger than the first; the
//! message of a refusal, nothing. The binary installs the counting allocator, so it stands alone.

use core::ffi::c_char;
use core::mem::{MaybeUninit, size_of};
use core::ptr;

use kahlnws_capi::{
    Int128, Problem, Refusal, RefusalKind, Solver, StatusCode, Work, kahlnws_refusal_message, kahlnws_solver_free,
    kahlnws_solver_new, kahlnws_solver_reserve, kahlnws_solver_resolve, kahlnws_solver_solve,
};
use kahlnws_testing::{Allocations, CountingAllocator, allocations_on_this_thread};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// An entry that solves, as C declares `kahlnws_solver_solve` and `kahlnws_solver_resolve`, which take the same
/// arguments.
type SolveEntry = unsafe extern "C" fn(
    *mut Solver,
    *const Problem,
    *mut u64,
    *mut i64,
    *mut Int128,
    *mut Work,
    *mut Refusal,
) -> StatusCode;

/// Where an entry starts a solve of the problem its solver solved last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    /// From the star, as from a new solver: `kahlnws_solver_solve`.
    Fresh,
    /// From the basis the last solve ended on: `kahlnws_solver_resolve`.
    Kept,
}

/// Both entries that solve, each with its name and where it starts the problem solved last.
const SOLVE_ENTRIES: [(&str, SolveEntry, Start); 2] = [
    ("kahlnws_solver_solve", kahlnws_solver_solve, Start::Fresh),
    ("kahlnws_solver_resolve", kahlnws_solver_resolve, Start::Kept),
];

/// The tables of one problem, borrowed by the `kahlnws_problem` that [`problem_of`] points at them.
#[derive(Clone, Copy, Debug)]
struct Tables<'a> {
    /// The node each arc leaves.
    tails: &'a [usize],
    /// The node each arc enters.
    heads: &'a [usize],
    /// The most flow of each arc.
    capacities: &'a [u64],
    /// The cost of a unit of flow on each arc.
    costs: &'a [i64],
    /// The supply of each node.
    supplies: &'a [i64],
}

/// The problem of `tables` as C gives it, without lower bounds.
fn problem_of(tables: &Tables<'_>) -> Problem {
    Problem {
        node_count: tables.supplies.len(),
        arc_count: tables.tails.len(),
        tails: tables.tails.as_ptr(),
        heads: tables.heads.as_ptr(),
        lower_bounds: ptr::null(),
        capacities: tables.capacities.as_ptr(),
        costs: tables.costs.as_ptr(),
        supplies: tables.supplies.as_ptr(),
    }
}

/// The depot, solved optimally: node 0 sends 4 units to node 2, directly or through node 1.
const DEPOT: Tables<'static> =
    Tables { tails: &[0, 0, 1], heads: &[2, 1, 2], capacities: &[3, 4, 2], costs: &[5, 1, 1], supplies: &[4, 0, -4] };

/// The depot with arcs too narrow for the supply: infeasible.
const NARROW: Tables<'static> = Tables { capacities: &[1, 1, 1], ..DEPOT };

/// The depot with supplies that do not balance: refused.
const UNBALANCED: Tables<'static> = Tables { supplies: &[4, 0, -3], ..DEPOT };

/// A chain of four nodes, larger than the depot in nodes and arcs.
const CHAIN: Tables<'static> = Tables {
    tails: &[0, 1, 2],
    heads: &[1, 2, 3],
    capacities: &[5, 5, 5],
    costs: &[1, 1, 1],
    supplies: &[2, 0, 0, -2],
};

/// How one solve went: its status, and what it allocated on the calling thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Solved {
    /// The status the solve answered.
    status: StatusCode,
    /// What the solve alone allocated.
    allocated: Allocations,
    /// The work the solve wrote; `None` where its status writes none.
    work: Option<Work>,
}

/// Solves `tables` with `solver` by `entry` into storage made beforehand that holds no values, every output given, and
/// answers how it went.
fn solved(entry: SolveEntry, solver: *mut Solver, tables: &Tables<'_>) -> Solved {
    let problem = problem_of(tables);
    let mut flows = vec![MaybeUninit::<u64>::uninit(); tables.tails.len()];
    let mut potentials = vec![MaybeUninit::<i64>::uninit(); tables.supplies.len()];
    let (mut total_cost, mut refusal) = (MaybeUninit::<Int128>::uninit(), MaybeUninit::<Refusal>::uninit());
    let mut work = MaybeUninit::<Work>::uninit();

    let before = allocations_on_this_thread();
    // SAFETY: `solver` is the test's live solver; the problem points into `tables` and every output into the test's
    // own buffers, each for the counts the problem states, and nothing else uses them during the call.
    let status = unsafe {
        entry(
            solver,
            &raw const problem,
            flows.as_mut_ptr().cast(),
            potentials.as_mut_ptr().cast(),
            total_cost.as_mut_ptr(),
            work.as_mut_ptr(),
            refusal.as_mut_ptr(),
        )
    };
    let allocated = allocations_on_this_thread() - before;
    let work = match status {
        // SAFETY: an optimal or an infeasible solve wrote the work.
        StatusCode::OPTIMAL | StatusCode::INFEASIBLE => Some(unsafe { work.assume_init() }),
        _ => None,
    };

    Solved { status, allocated, work }
}

/// A solver made by `kahlnws_solver_new`.
///
/// # Panics
/// Panics if the memory for it is refused.
fn new_solver() -> *mut Solver {
    let solver = kahlnws_solver_new();
    assert!(!solver.is_null(), "the memory for a solver should be granted");

    solver
}

#[test]
fn test_no_solve_allocates_after_a_reservation() {
    // By `kahlnws_solver_resolve`, the first chain, the depot after it and the chain after the refused depot start
    // from the star, of other counts than the basis kept; the infeasible depot after the depot and the last chain
    // after the chain start from the kept basis.
    for (name, entry, start) in SOLVE_ENTRIES {
        let solver = new_solver();
        // SAFETY: the solver is live and used by nothing else meanwhile.
        let reserved = unsafe { kahlnws_solver_reserve(solver, CHAIN.supplies.len(), CHAIN.tails.len()) };
        assert_eq!(reserved, StatusCode::RESERVED);

        let solves = [CHAIN, DEPOT, NARROW, UNBALANCED, CHAIN, CHAIN].map(|tables| solved(entry, solver, &tables));
        let statuses = solves.map(|solve| solve.status);
        let optimal = StatusCode::OPTIMAL;
        let expected = [optimal, optimal, StatusCode::INFEASIBLE, StatusCode::REFUSED, optimal, optimal];
        assert_eq!(statuses, expected, "{name}");
        assert!(solves.iter().all(|solve| solve.allocated == Allocations::NONE), "{name}: {solves:?}");
        assert_last_chain_started_as(start, &solves, name);

        // SAFETY: the solver is live and used by nothing else from here on.
        unsafe { kahlnws_solver_free(solver) };
    }
}

/// Asserts that the last of `solves`, a chain solved right after a chain from the star, started as `start` says: from
/// the star, it made the work of the chain before it, some pivots; from the kept basis, the optimum of the same
/// numbers, it made none.
///
/// # Panics
/// Panics if it did not, naming the entry `name`.
fn assert_last_chain_started_as(start: Start, solves: &[Solved], name: &str) {
    let [.., before, last] = solves else { panic!("{name}: the solves should end with two chains") };
    let fresh = before.work.expect("the chain before should be optimal");
    assert!(fresh.pivots > 0, "{name}: the chain from the star makes pivots");

    let expected = match start {
        Start::Fresh => fresh,
        Start::Kept => Work::default(),
    };
    assert_eq!(last.work, Some(expected), "{name}: the last chain should start {start:?}");
}

#[test]
fn test_no_solve_allocates_from_the_second_on_for_a_problem_no_larger_than_the_first() {
    for (name, entry, start) in SOLVE_ENTRIES {
        let solver = new_solver();

        let first = solved(entry, solver, &CHAIN);
        assert_eq!(first.status, StatusCode::OPTIMAL, "{name}");
        assert_ne!(first.allocated, Allocations::NONE, "{name}: the first solve makes the room");

        let solves = [DEPOT, NARROW, UNBALANCED, CHAIN, CHAIN].map(|tables| solved(entry, solver, &tables));
        assert!(solves.iter().all(|solve| solve.allocated == Allocations::NONE), "{name}: {solves:?}");
        assert_last_chain_started_as(start, &solves, name);

        // SAFETY: the solver is live and used by nothing else from here on.
        unsafe { kahlnws_solver_free(solver) };
    }
}

#[test]
fn test_the_message_of_a_refusal_allocates_nothing() {
    let refusal =
        Refusal { kind: RefusalKind::TAIL_OUTSIDE_NODES, arc: 2, node: 9, node_count: 4, ..Refusal::default() };
    let mut buffer: [c_char; 64] = [0; 64];

    let before = allocations_on_this_thread();
    // SAFETY: the refusal is the test's own, and the buffer holds 64 bytes that nothing else uses meanwhile.
    let length = unsafe { kahlnws_refusal_message(&raw const refusal, buffer.as_mut_ptr(), 64) };
    let allocated = allocations_on_this_thread() - before;

    assert_eq!(length, "arc 2 leaves node 9, not below the node count 4".len());
    assert_eq!(allocated, Allocations::NONE);
}

#[test]
fn test_a_new_solver_allocates_exactly_itself_and_no_room() {
    let before = allocations_on_this_thread();
    let solver = new_solver();
    let allocated = allocations_on_this_thread() - before;

    let solver_bytes = u64::try_from(size_of::<Solver>()).expect("a solver's size should fit a u64");
    assert_eq!(allocated, Allocations { blocks: 1, bytes: solver_bytes });

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}
