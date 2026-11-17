// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the C interface allocates: a new solver, exactly its own block and no room; a solve, nothing after
//! `kahlnws_solver_reserve` for problems of the reserved size and smaller, optimal, infeasible or refused, each
//! answered from its problem alone, and nothing from the second solve on for a problem no larger than the first; the
//! message of a refusal, nothing. A new network, exactly its own block and no room; after `kahlnws_network_reserve`,
//! nothing in a load of a problem within the room, refused or not, nor in any change, solve or read, while a solve
//! after a change starts from the basis the network held and not from its star; a copy, its own room, and nothing in
//! its changes and solves; a copy into a network reserved for its source's counts, nothing, nor its changes and
//! solves; a read of the problem held, nothing; a load within the room, nothing, from the basis held or the star, the
//! work showing which. The binary installs the counting allocator, so it stands alone.

use core::ffi::c_char;
use core::mem::size_of;
use core::ptr;

use kahlnws_capi::{
    Int128, Network, Problem, Refusal, RefusalKind, Solver, StatusCode, Work, kahlnws_network_bounds,
    kahlnws_network_capacities, kahlnws_network_capacity, kahlnws_network_clone, kahlnws_network_copy,
    kahlnws_network_cost, kahlnws_network_costs, kahlnws_network_flow, kahlnws_network_flows, kahlnws_network_free,
    kahlnws_network_head, kahlnws_network_heads, kahlnws_network_load, kahlnws_network_lower_bound,
    kahlnws_network_lower_bounds, kahlnws_network_move_supply, kahlnws_network_new, kahlnws_network_potentials,
    kahlnws_network_reserve, kahlnws_network_set_bounds, kahlnws_network_set_capacity, kahlnws_network_set_cost,
    kahlnws_network_set_lower_bound, kahlnws_network_set_supply, kahlnws_network_solve, kahlnws_network_supplies,
    kahlnws_network_supply, kahlnws_network_tail, kahlnws_network_tails, kahlnws_network_total_cost,
    kahlnws_refusal_message, kahlnws_solver_free, kahlnws_solver_new, kahlnws_solver_reserve, kahlnws_solver_solve,
};
use kahlnws_testing::{Allocations, CountingAllocator, allocations_on_this_thread};

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

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

/// The work a solve's storage starts as: no problem of these tests makes that many pivots, so a solve that answers
/// its work and leaves this is seen.
const UNWRITTEN_WORK: Work = Work { pivots: u64::MAX, degenerate_pivots: u64::MAX, first_arc_pivots: u64::MAX };

/// Solves `tables` with `solver` into storage made beforehand, every output given, and answers how it went.
///
/// # Panics
/// Panics if an optimal or an infeasible solve leaves its work unwritten.
fn solved(solver: *mut Solver, tables: &Tables<'_>) -> Solved {
    let problem = problem_of(tables);
    let (mut flows, mut potentials) = (vec![7_u64; tables.tails.len()], vec![7_i64; tables.supplies.len()]);
    let (mut total_cost, mut refusal) = (Int128 { low: 7, high: 7 }, Refusal::default());
    let mut work = UNWRITTEN_WORK;

    let before = allocations_on_this_thread();
    // SAFETY: `solver` is the test's live solver; the problem points into `tables` and every output into the test's
    // own buffers, each for the counts the problem states, and nothing else uses them during the call.
    let status = unsafe {
        kahlnws_solver_solve(
            solver,
            &raw const problem,
            flows.as_mut_ptr(),
            potentials.as_mut_ptr(),
            &raw mut total_cost,
            &raw mut work,
            &raw mut refusal,
        )
    };
    let allocated = allocations_on_this_thread() - before;
    let work = match status {
        StatusCode::OPTIMAL | StatusCode::INFEASIBLE => {
            assert_ne!(work, UNWRITTEN_WORK, "an optimal or an infeasible solve writes its work");
            Some(work)
        }
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
    let solver = new_solver();
    // SAFETY: the solver is live and used by nothing else meanwhile.
    let reserved = unsafe { kahlnws_solver_reserve(solver, CHAIN.supplies.len(), CHAIN.tails.len()) };
    assert_eq!(reserved, StatusCode::OK);

    let solves = [CHAIN, DEPOT, NARROW, UNBALANCED, CHAIN, CHAIN].map(|tables| solved(solver, &tables));
    let statuses = solves.map(|solve| solve.status);
    let optimal = StatusCode::OPTIMAL;
    let expected = [optimal, optimal, StatusCode::INFEASIBLE, StatusCode::REFUSED, optimal, optimal];
    assert_eq!(statuses, expected);
    assert!(solves.iter().all(|solve| solve.allocated == Allocations::NONE), "{solves:?}");
    assert_keeps_nothing(&solves);

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

/// Asserts that the last two of `solves`, the same chain twice, made the same work, some pivots: a solve answers its
/// problem alone, and the second starts from nothing the first left.
///
/// # Panics
/// Panics if they did not.
fn assert_keeps_nothing(solves: &[Solved]) {
    let [.., before, last] = solves else { panic!("the solves should end with two chains") };
    let work = before.work.expect("the chain before should be optimal");

    assert!(work.pivots > 0, "the chain makes pivots");
    assert_eq!(last.work, Some(work), "the same chain again should make the same work");
}

#[test]
fn test_no_solve_allocates_from_the_second_on_for_a_problem_no_larger_than_the_first() {
    let solver = new_solver();

    let first = solved(solver, &CHAIN);
    assert_eq!(first.status, StatusCode::OPTIMAL);
    assert_ne!(first.allocated, Allocations::NONE, "the first solve makes the room");

    let solves = [DEPOT, NARROW, UNBALANCED, CHAIN, CHAIN].map(|tables| solved(solver, &tables));
    assert!(solves.iter().all(|solve| solve.allocated == Allocations::NONE), "{solves:?}");
    assert_keeps_nothing(&solves);

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
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

/// A network made by `kahlnws_network_new`.
///
/// # Panics
/// Panics if the memory for it is refused.
fn new_network() -> *mut Network {
    let network = kahlnws_network_new();
    assert!(!network.is_null(), "the memory for a network should be granted");

    network
}

/// What `kahlnws_network_load` of `tables` into `network` answers, and what the call alone allocated.
fn loaded(network: *mut Network, tables: &Tables<'_>) -> (StatusCode, Allocations) {
    let problem = problem_of(tables);
    let mut refusal = Refusal::default();

    let before = allocations_on_this_thread();
    // SAFETY: the network is live, the problem points into `tables`, and the refusal is the test's own storage.
    let status = unsafe { kahlnws_network_load(network, &raw const problem, &raw mut refusal) };

    (status, allocations_on_this_thread() - before)
}

/// What `kahlnws_network_solve` of `network` answers, the work it wrote, and what the call alone allocated.
fn network_solved(network: *mut Network) -> (StatusCode, Work, Allocations) {
    let mut total_cost = Int128 { low: 7, high: 7 };
    let mut work = Work { pivots: 7, degenerate_pivots: 7, first_arc_pivots: 7 };

    let before = allocations_on_this_thread();
    // SAFETY: the network is live, and the outputs are the test's own storage.
    let status = unsafe { kahlnws_network_solve(network, &raw mut total_cost, &raw mut work) };

    (status, work, allocations_on_this_thread() - before)
}

/// Applies to `network`, which holds the depot's arcs, each change the network has, refused and admitted, a move of
/// one unit of the customer's demand back to the depot and the depot's supply set to 5, balanced by the customer,
/// among them, and a node that balances itself and is to change, and answers each status and what the calls together
/// allocated.
fn changed(network: *mut Network) -> ([StatusCode; 8], Allocations) {
    let (mut cost, mut bound, mut capacity, mut supply) = (0_i64, 0_u64, 0_u64, 0_i64);
    let mut refusal = Refusal::default();
    let refusal_output = &raw mut refusal;

    let before = allocations_on_this_thread();
    // SAFETY: the network is live, holds the depot, and every output is the test's own storage.
    let statuses = unsafe {
        [
            kahlnws_network_set_cost(network, 2, 3, &raw mut cost, refusal_output),
            kahlnws_network_set_capacity(network, 1, 0, &raw mut bound, refusal_output),
            kahlnws_network_set_capacity(network, 1, 4, &raw mut bound, refusal_output),
            kahlnws_network_set_lower_bound(network, 0, 9, &raw mut bound, refusal_output),
            kahlnws_network_set_bounds(network, 2, 0, 2, &raw mut bound, &raw mut capacity, refusal_output),
            kahlnws_network_move_supply(network, 0, 2, 1, refusal_output),
            kahlnws_network_set_supply(network, 0, 5, 2, &raw mut supply, refusal_output),
            kahlnws_network_set_supply(network, 1, 9, 1, &raw mut supply, refusal_output),
        ]
    };

    (statuses, allocations_on_this_thread() - before)
}

/// Reads every part of the optimum `network` holds, the depot's counts, and answers each status and what the reads
/// together allocated.
fn read(network: *const Network) -> ([StatusCode; 4], Allocations) {
    let (mut total_cost, mut flow) = (Int128::default(), 0_u64);
    let (mut flows, mut potentials) = ([0_u64; 3], [0_i64; 3]);

    let before = allocations_on_this_thread();
    // SAFETY: the network is live, holds the depot's counts, and every output is the test's own storage.
    let statuses = unsafe {
        [
            kahlnws_network_total_cost(network, &raw mut total_cost),
            kahlnws_network_flow(network, 1, &raw mut flow),
            kahlnws_network_flows(network, flows.as_mut_ptr()),
            kahlnws_network_potentials(network, potentials.as_mut_ptr()),
        ]
    };

    (statuses, allocations_on_this_thread() - before)
}

#[test]
fn test_a_new_network_allocates_exactly_itself_and_no_room() {
    let before = allocations_on_this_thread();
    let network = new_network();
    let allocated = allocations_on_this_thread() - before;

    let network_bytes = u64::try_from(size_of::<Network>()).expect("a network's size should fit a u64");
    assert_eq!(allocated, Allocations { blocks: 1, bytes: network_bytes });

    // SAFETY: the network is live and used by nothing else from here on.
    unsafe { kahlnws_network_free(network) };
}

#[test]
fn test_no_load_change_solve_or_read_of_a_network_allocates_after_a_reservation() {
    let network = new_network();
    // SAFETY: the network is live and used by nothing else meanwhile.
    let reserved = unsafe { kahlnws_network_reserve(network, CHAIN.supplies.len(), CHAIN.tails.len()) };
    assert_eq!(reserved, StatusCode::OK);

    let loads = [CHAIN, UNBALANCED, DEPOT].map(|tables| loaded(network, &tables));
    assert_eq!(loads.map(|load| load.0), [StatusCode::OK, StatusCode::REFUSED, StatusCode::OK]);
    let (first, first_work, solve_allocated) = network_solved(network);
    assert_eq!((first, first_work.pivots), (StatusCode::OPTIMAL, 4), "the depot from the star: four pivots");

    // The customer's one more unit from the depot: the depot's optimum hangs node 2 from node 0 by arc 0, which
    // carries 2 of its 3, so the tree carries the unit and marks nothing. A network that started again from its star
    // would pivot four times; this solve prices nothing and pivots none.
    let before = allocations_on_this_thread();
    // SAFETY: the network is live, and no refusal is asked for.
    let moved = unsafe { kahlnws_network_move_supply(network, 2, 0, 1, ptr::null_mut()) };
    let move_allocated = allocations_on_this_thread() - before;
    let (carried, carried_work, carried_allocated) = network_solved(network);
    assert_eq!((moved, carried, carried_work.pivots), (StatusCode::OK, StatusCode::OPTIMAL, 0));

    let (statuses, changes_allocated) = changed(network);
    let (ok, refused, invalid) = (StatusCode::OK, StatusCode::REFUSED, StatusCode::INVALID_ARGUMENT);
    assert_eq!(statuses, [ok, ok, ok, refused, ok, ok, ok, invalid]);
    let (after, _, after_allocated) = network_solved(network);
    assert_eq!(after, StatusCode::OPTIMAL);
    let (reads, reads_allocated) = read(network);
    assert_eq!(reads, [StatusCode::OK; 4]);

    let none = Allocations::NONE;
    assert_eq!(loads.map(|load| load.1), [none; 3]);
    let calls =
        [solve_allocated, move_allocated, carried_allocated, changes_allocated, after_allocated, reads_allocated];
    assert_eq!(calls, [none; 6]);

    // SAFETY: the network is live and used by nothing else from here on.
    unsafe { kahlnws_network_free(network) };
}

#[test]
fn test_a_copy_of_a_network_allocates_its_room_and_its_changes_and_solves_nothing() {
    let network = new_network();
    assert_eq!(loaded(network, &DEPOT).0, StatusCode::OK);
    assert_eq!(network_solved(network).0, StatusCode::OPTIMAL);

    let before = allocations_on_this_thread();
    // SAFETY: the network is live and used by nothing else meanwhile.
    let copy = unsafe { kahlnws_network_clone(network) };
    let copied = allocations_on_this_thread() - before;
    assert!(!copy.is_null(), "the memory for a copy should be granted");
    assert_ne!(copied, Allocations::NONE, "a copy allocates its own room");

    let (statuses, changes_allocated) = changed(copy);
    assert_eq!(statuses.iter().filter(|&&status| status == StatusCode::OK).count(), 6);
    let (solved, _, solve_allocated) = network_solved(copy);
    assert_eq!(
        (solved, changes_allocated, solve_allocated),
        (StatusCode::OPTIMAL, Allocations::NONE, Allocations::NONE)
    );

    // SAFETY: the network and its copy are live and used by nothing else from here on.
    unsafe {
        kahlnws_network_free(copy);
        kahlnws_network_free(network);
    }
}

#[test]
fn test_a_copy_into_a_network_with_room_for_it_and_the_copy_s_changes_and_solves_allocate_nothing() {
    let (source, destination) = (new_network(), new_network());
    assert_eq!(loaded(source, &DEPOT).0, StatusCode::OK);
    assert_eq!(network_solved(source).0, StatusCode::OPTIMAL);
    // SAFETY: the network is live and used by nothing else meanwhile.
    let reserved = unsafe { kahlnws_network_reserve(destination, CHAIN.supplies.len(), CHAIN.tails.len()) };
    assert_eq!(reserved, StatusCode::OK);
    assert_eq!(loaded(destination, &CHAIN).0, StatusCode::OK);

    let before = allocations_on_this_thread();
    // SAFETY: both networks are live, distinct, and used by nothing else meanwhile.
    let copied = unsafe { kahlnws_network_copy(destination, source) };
    let copy_allocated = allocations_on_this_thread() - before;
    let (statuses, changes_allocated) = changed(destination);
    let (solved, _, solve_allocated) = network_solved(destination);

    assert_eq!((copied, solved), (StatusCode::OK, StatusCode::OPTIMAL));
    assert_eq!(statuses.iter().filter(|&&status| status == StatusCode::OK).count(), 6);
    assert_eq!([copy_allocated, changes_allocated, solve_allocated], [Allocations::NONE; 3]);

    // SAFETY: the networks are live and used by nothing else from here on.
    unsafe {
        kahlnws_network_free(destination);
        kahlnws_network_free(source);
    }
}

#[test]
fn test_no_read_of_the_problem_a_network_holds_allocates() {
    let network = new_network();
    assert_eq!(loaded(network, &DEPOT).0, StatusCode::OK);
    let (mut index, mut number, mut bound, mut capacity) = (0_usize, 0_i64, 0_u64, 0_u64);
    let (mut indices, mut numbers, mut bounds) = ([0_usize; 3], [0_i64; 3], [0_u64; 3]);

    let before = allocations_on_this_thread();
    // SAFETY: the network is live, holds the depot's counts, and every output is the test's own storage.
    let statuses = unsafe {
        [
            kahlnws_network_tail(network, 2, &raw mut index),
            kahlnws_network_head(network, 2, &raw mut index),
            kahlnws_network_cost(network, 2, &raw mut number),
            kahlnws_network_bounds(network, 2, &raw mut bound, &raw mut capacity),
            kahlnws_network_lower_bound(network, 2, &raw mut bound),
            kahlnws_network_capacity(network, 2, &raw mut capacity),
            kahlnws_network_supply(network, 2, &raw mut number),
            kahlnws_network_tails(network, indices.as_mut_ptr()),
            kahlnws_network_heads(network, indices.as_mut_ptr()),
            kahlnws_network_costs(network, numbers.as_mut_ptr()),
            kahlnws_network_lower_bounds(network, bounds.as_mut_ptr()),
            kahlnws_network_capacities(network, bounds.as_mut_ptr()),
            kahlnws_network_supplies(network, numbers.as_mut_ptr()),
        ]
    };
    let allocated = allocations_on_this_thread() - before;

    assert_eq!(statuses, [StatusCode::OK; 13]);
    assert_eq!((number, numbers, bounds, allocated), (-4, [4, 0, -4], [3, 4, 2], Allocations::NONE));

    // SAFETY: the network is live and used by nothing else from here on.
    unsafe { kahlnws_network_free(network) };
}

#[test]
fn test_no_load_allocates_after_a_reservation_from_the_basis_held_or_the_star() {
    let network = new_network();
    // SAFETY: the network is live and used by nothing else meanwhile.
    let reserved = unsafe { kahlnws_network_reserve(network, CHAIN.supplies.len(), CHAIN.tails.len()) };
    assert_eq!(reserved, StatusCode::OK);
    assert_eq!(loaded(network, &DEPOT).0, StatusCode::OK);
    let (first, first_work, _) = network_solved(network);
    assert_eq!((first, first_work.pivots), (StatusCode::OPTIMAL, 4), "the depot from the star: four pivots");

    // The same depot again starts from the basis held, its optimum: no pivot. The chain, of other counts, from the
    // star: some pivots. Neither load nor solve allocates.
    let (kept_load, kept_allocated) = loaded(network, &DEPOT);
    let (kept, kept_work, kept_solve_allocated) = network_solved(network);
    let (star_load, star_allocated) = loaded(network, &CHAIN);
    let (star, star_work, star_solve_allocated) = network_solved(network);

    assert_eq!((kept_load, kept, kept_work.pivots), (StatusCode::OK, StatusCode::OPTIMAL, 0));
    assert_eq!((star_load, star), (StatusCode::OK, StatusCode::OPTIMAL));
    assert!(star_work.pivots > 0, "a load of other counts starts from the star");
    let none = Allocations::NONE;
    assert_eq!([kept_allocated, kept_solve_allocated, star_allocated, star_solve_allocated], [none; 4]);

    // SAFETY: the network is live and used by nothing else from here on.
    unsafe { kahlnws_network_free(network) };
}
