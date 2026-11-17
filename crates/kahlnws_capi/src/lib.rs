// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The C interface of kahlnws: a primal network simplex for the min-cost flow problem on exact integers.
//!
//! The header `include/kahlnws.h` declares it, and the library `libkahlnws_capi`, static or dynamic, provides it. The
//! header is read by a C compiler from C99 on and by a C++ compiler from C++11 on.
//!
//! Two handles, one contract each. The solver, `kahlnws_solver`, is a function: a problem in, an answer out. It keeps
//! room and nothing else, so what a solve answers is a function of its problem alone. A network, `kahlnws_network`, is
//! a state: it holds one problem and a basis of it, which its changes, its loads of new numbers on the same arcs and
//! its solves carry on from. Reach for the solver to answer one problem; for a network to hold a problem, change it,
//! load new numbers on the same arcs, and solve it again and again.
//!
//! Use. A program makes a solver with `kahlnws_solver_new` and, where it solves in a loop, reserves its room once
//! with `kahlnws_solver_reserve`. It fills in a `kahlnws_problem` with the two counts and a pointer to each of its
//! tables, and calls `kahlnws_solver_solve` with storage of its own for the flows and, where it wants them, the
//! potentials, the cost, the work and the refusal. It reads the `kahlnws_status` the call answers, which
//! `kahlnws_status_name` names; where the solve ran, optimal or infeasible, it may read the pivots it made from the
//! `kahlnws_work`; on a refusal, it reads the breach from the `kahlnws_refusal`, or its text from
//! `kahlnws_refusal_message`. It frees the solver with `kahlnws_solver_free`.
//!
//! A problem held. A program that changes a few numbers of one problem many times, as a search does that tries a move,
//! keeps it or sets it back, makes a network with `kahlnws_network_new` and hands it the problem once with
//! `kahlnws_network_load`; a later load of new numbers on the same arcs carries over the basis the network holds, so
//! a network that holds a problem over days carries its basis from day to day. Each `kahlnws_network_set_cost`,
//! `kahlnws_network_set_capacity`, `kahlnws_network_set_lower_bound`, `kahlnws_network_set_bounds` and
//! `kahlnws_network_set_supply` changes one number of the problem held, or one pair, at once, and writes the number it
//! replaced where the caller asks for it; `kahlnws_network_move_supply` moves supply from one node to another and
//! writes no number. Each answers `KAHLNWS_STATUS_OK` where the change is made, `KAHLNWS_STATUS_REFUSED` with the
//! breach where the number lies outside the ranges a network keeps, and `KAHLNWS_STATUS_INVALID_ARGUMENT` for an arc
//! or a node not below its count. `kahlnws_network_solve` solves the problem as it then stands, and
//! `kahlnws_network_total_cost`, `kahlnws_network_flow`, `kahlnws_network_flows` and `kahlnws_network_potentials` read
//! its optimum, each answering `KAHLNWS_STATUS_NO_OPTIMUM` where the network holds none, since nothing in C can borrow
//! it. The problem as it stands is read back by `kahlnws_network_tail`, `_head`, `_cost`, `_lower_bound`, `_capacity`,
//! `_bounds` and `_supply`, one arc or node, and `kahlnws_network_tails`, `_heads`, `_costs`, `_lower_bounds`,
//! `_capacities` and `_supplies`, every one into a buffer; a read of the problem needs no optimum and ends none. A
//! change costs what it moves, and a solve after it the pivots it calls for: a solve that makes no pivot prices only
//! the arcs the changes may have made worth using, while a change that leaves much flow still to be sent can cost
//! more than a load of the whole problem as it stands and the solve after it. Setting a number back, or moving a
//! supply back, undoes a change; `kahlnws_network_clone` copies a network whole into a new one, and
//! `kahlnws_network_copy` into one that exists, reusing its room. It frees the network with `kahlnws_network_free`.
//!
//! Ownership. The library owns only the solvers and the networks it makes, from `kahlnws_solver_new` to
//! `kahlnws_solver_free` and from `kahlnws_network_new` or `kahlnws_network_clone` to `kahlnws_network_free`. Every
//! other pointer belongs to the caller: a call reads or writes through it while it runs, and keeps none.
//!
//! Pointers. Each entry states what each of its pointers must point to. An input is initialised, and nothing writes
//! it while the call runs. An output needs writable storage for as many entries as the call states, which nothing
//! else reads or writes while the call runs and which overlaps no other argument; what it held before does not
//! matter. An array lies within one object. A table of no entries needs no storage: where its count is zero, its
//! pointer is not read and may be null. A pointer the call needs that is null is answered as
//! `KAHLNWS_STATUS_NULL_POINTER`; a pointer not aligned for its type, or a count of entries that passes `PTRDIFF_MAX`
//! bytes, as `KAHLNWS_STATUS_INVALID_ARGUMENT`. Either way the call writes nothing.
//!
//! Answers. Every entry answers, and none aborts or unwinds into the caller. Memory refused for the room of a solver
//! or of a network is answered as `KAHLNWS_STATUS_NO_ROOM`, or as null by an entry that answers a handle, and an
//! internal error of the library, a defect of it, as `KAHLNWS_STATUS_INTERNAL_ERROR`.
//!
//! Threads. A solver or a network is used by one thread at a time: no two calls that take the same one run at once.
//! It may move from one thread to another between calls, and distinct ones may be used on distinct threads at once,
//! also to solve or load the same problem tables. The entries that take neither may be called from any thread at any
//! time.
//!
//! Numbers. Node and arc counts and node numbers are `size_t`; flows, lower bounds, capacities and the counts of a
//! `kahlnws_work` `uint64_t`; costs, supplies and potentials `int64_t`. A 128-bit integer, such as the cost of a
//! flow, comes as its two 64-bit halves, `kahlnws_int128` or `kahlnws_uint128`: the value is `high * 2^64 + low`,
//! with `low` the value modulo `2^64`. `kahlnws_int128_format` and `kahlnws_uint128_format` write one in decimal.
//!
//! Version. `KAHLNWS_VERSION_MAJOR`, `KAHLNWS_VERSION_MINOR` and `KAHLNWS_VERSION_PATCH` give the version of the
//! header, and `KAHLNWS_VERSION` all three in one number, encoded as `kahlnws_version` answers the version of the
//! library linked. From the first release on, a program built against a header works with a library of the same
//! major version whose minor version is at least the header's, and while the major version is 0, only with a library
//! of the same minor version. Until that release the interface may change while the version stays, so a header and a
//! library must be built from the same sources, which comparing `KAHLNWS_VERSION` with `kahlnws_version` cannot
//! check.
//!
//! # Maintaining the header
//!
//! The overview above, up to this heading, opens the header; what follows stays in these docs. The build writes the
//! header from the library's sources to its output folder, and a test fails where the committed
//! `include/kahlnws.h` differs from it; `KAHLNWS_WRITE_HEADER=1 cargo build -p kahlnws_capi` rewrites the committed
//! header.
//!
//! # From C++
//!
//! `include/kahlnws.hpp` is the C++ interface over these entries: one hand-written, header-only file for C++11 and
//! later that includes `kahlnws.h` and puts everything in `namespace kahlnws`. A `kahlnws::network_simplex` solves a
//! `kahlnws::problem` of six named views; `try_solve` answers the flows, the potentials and the cost owned, and
//! `try_solve_into` writes them into buffers the caller holds, each as a `kahlnws::result` that holds the answer or a
//! `kahlnws::error`. A `kahlnws::network` holds a problem: `try_load`, the changes `try_set_cost`, `try_set_capacity`,
//! `try_set_lower_bound`, `try_set_bounds`, `try_set_supply` and `try_move_supply`, `try_solve`, and the reads of its
//! optimum; it is copied through `kahlnws_network_clone`. Where exceptions are enabled, the forms without `try_` throw
//! instead. The tests in `tests/header.rs` build `tests/network_simplex.cpp`, `tests/network.cpp` and
//! `tests/int128.cpp` against it in C++11, 14, 17 and 20, and C++23 where the compiler has it, with exceptions and
//! without.

mod boundary;
mod guard;
mod halves;
mod network;
mod problem;
mod refusal;
mod solver;
mod status;
mod terminated_text;
mod version;
mod work;

use core::ffi::c_char;
use core::ptr;

pub use halves::{Int128, UInt128};
pub use network::Network;
pub use problem::Problem;
pub use refusal::{Refusal, RefusalKind};
pub use solver::Solver;
pub use status::StatusCode;
pub use work::Work;

use guard::{guarded, status_of};
use kahlnws::Bounds;
use network::{OptimumRead, ProblemRead};
use status::Status;

/// A new solver that holds no room yet, for the other entries to use and `kahlnws_solver_free` to free.
///
/// It allocates the solver itself, and no room. Safe to call from any thread.
///
/// # Returns
/// The solver, or null where the memory for it is refused; nothing else answers null, since nothing in it can fail
/// otherwise, and an internal error would be answered as null too.
#[must_use]
#[unsafe(no_mangle)]
pub extern "C" fn kahlnws_solver_new() -> *mut Solver {
    guarded(ptr::null_mut(), || boundary::handle(Solver::new()))
}

/// Frees `solver` with all its room.
///
/// Null is accepted and frees nothing, and so is a pointer not aligned for a solver, which `kahlnws_solver_new`
/// never answers. After the call the solver is freed: no call may use the pointer again. A solver may be freed on
/// any thread.
///
/// # Parameters
/// - `solver` (in): the solver to free, or null.
///
/// # Safety
/// - `solver` is null, or a pointer `kahlnws_solver_new` answered that has not been freed since and that no other
///   call uses meanwhile.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_solver_free(solver: *mut Solver) {
    guarded((), || {
        // SAFETY: C guarantees `solver` is null or a solver `kahlnws_solver_new` answered, not freed and not in use.
        unsafe { boundary::free_handle(solver) }
    });
}

/// Makes room in `solver` for problems of up to `node_count` nodes and `arc_count` arcs.
///
/// A solve of such a problem allocates nothing afterwards. Room already held is kept.
///
/// # Parameters
/// - `solver` (in, out): the solver whose room is made.
/// - `node_count` (in): the most nodes a problem will have.
/// - `arc_count` (in): the most arcs a problem will have.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: the room is made;
/// - `KAHLNWS_STATUS_NO_ROOM`: the memory was refused; the room held before is kept, and the solver may be used;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `solver` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `solver` is not aligned for a solver;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught.
///
/// # Safety
/// - `solver` is null, or a pointer `kahlnws_solver_new` answered that has not been freed since and that no other
///   call uses meanwhile: one thread at a time uses a solver.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_solver_reserve(
    solver: *mut Solver,
    node_count: usize,
    arc_count: usize,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `solver` is null or a solver `kahlnws_solver_new` answered, not freed and not in use.
        let solver = unsafe { boundary::handle_mut(solver) }?;

        Ok(solver.reserve(node_count, arc_count))
    })
}

/// Solves `problem` with `solver`: finds a flow of least cost, or that no flow within the bounds meets the
/// supplies, in exact integers.
///
/// On every status but `KAHLNWS_STATUS_OPTIMAL` the flows, the potentials and the cost are as they were; the work is
/// written on `KAHLNWS_STATUS_OPTIMAL` and `KAHLNWS_STATUS_INFEASIBLE` alone, and the refusal on
/// `KAHLNWS_STATUS_REFUSED` alone; an internal error leaves the outputs unspecified.
///
/// Room. Before the problem is checked, the solver's room is grown to the problem's counts and kept, so that refused
/// memory is answered rather than ending the process. So a refused problem may still have grown the room, and a
/// problem too large for memory answers `KAHLNWS_STATUS_NO_ROOM` before it could be refused as too large. Where the
/// room already holds the problem, by `kahlnws_solver_reserve` or an earlier solve, the call allocates nothing.
///
/// The solver keeps its room and nothing else: what the call answers is a function of `problem` alone, and nothing of
/// an earlier problem is read again.
///
/// # Parameters
/// - `solver` (in, out): the solver, whose room the solve uses and may grow.
/// - `problem` (in): the problem to solve.
/// - `flows` (out): where the flow of each arc is written, arc `a` at `a`: storage for `problem->arc_count`
///   entries, which may be null where that count is zero.
/// - `potentials` (out): where each node's potential is written, node `v` at `v`: storage for
///   `problem->node_count` entries, or null where the potentials are not wanted.
/// - `total_cost` (out): where the cost of the flow is written, or null where it is not wanted.
/// - `work` (out): where the work of the solve is written, or null where it is not wanted.
/// - `refusal` (out): where the breach of a refused problem is written, or null where it is not wanted.
///
/// # Returns
/// - `KAHLNWS_STATUS_OPTIMAL`: the flow of each arc is written to `flows`, arc `a` at `a`; where `potentials` is not
///   null, each node's potential to it, node `v` at `v`; where `total_cost` is not null, the cost of the flow to it;
///   where `work` is not null, the work of the solve to it. With the potentials,
///   `cost + potential[tail] - potential[head]` is at least zero on every arc whose flow is below its capacity and at
///   most zero on every arc whose flow is above its lower bound, which proves the flow optimal;
/// - `KAHLNWS_STATUS_INFEASIBLE`: no flow within the bounds meets the supplies; where `work` is not null, the work of
///   the solve is written to it;
/// - `KAHLNWS_STATUS_REFUSED`: the problem lies outside what a solve admits; where `refusal` is not null, the breach
///   is written to it;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `solver`, `problem` or `flows` is null, or a table of the problem other than its
///   lower bounds is null while its count is above zero;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer the call reads or writes is not aligned for its type, or a count of
///   entries passes `PTRDIFF_MAX` bytes;
/// - `KAHLNWS_STATUS_NO_ROOM`: the memory for the room the problem needs was refused;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught.
///
/// # Safety
/// - `solver` is null, or a pointer `kahlnws_solver_new` answered that has not been freed since and that no other
///   call uses meanwhile: one thread at a time uses a solver;
/// - `problem` is null or points to a `kahlnws_problem` whose every pointer points to what its doc states;
/// - `flows` points to writable storage for `problem->arc_count` entries, and may be null where that count is zero;
/// - `potentials` is null or points to writable storage for `problem->node_count` entries;
/// - `total_cost`, `work` and `refusal` are each null or point to writable storage for one value;
/// - an array lies within one object; what an output held before does not matter;
/// - no output overlaps another output, the problem or its tables; nothing writes the problem or its tables, and
///   nothing else reads or writes the outputs, while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_solver_solve(
    solver: *mut Solver,
    problem: *const Problem,
    flows: *mut u64,
    potentials: *mut i64,
    total_cost: *mut Int128,
    work: *mut Work,
    refusal: *mut Refusal,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `solver` is null or a solver `kahlnws_solver_new` answered, not freed and not in use.
        let solver = unsafe { boundary::handle_mut(solver) }?;
        // SAFETY: C guarantees `problem` and the outputs point to what this entry's doc states, null only where it
        // allows, with no output overlapping another argument, nothing writing the inputs and nothing else using the
        // outputs meanwhile.
        let call = unsafe { boundary::call(problem, flows, potentials, total_cost, work, refusal) }?;

        Ok(solver.solve(call))
    })
}

/// A new network, never loaded, for the other entries to use and `kahlnws_network_free` to free.
///
/// It holds the problem of no node and no arc. It allocates the network itself, and no room. Safe to call from any
/// thread.
///
/// # Returns
/// The network, or null where the memory for it is refused; nothing else answers null, since nothing in it can fail
/// otherwise, and an internal error would be answered as null too.
#[must_use]
#[unsafe(no_mangle)]
pub extern "C" fn kahlnws_network_new() -> *mut Network {
    guarded(ptr::null_mut(), || boundary::handle(Network::new()))
}

/// A copy of `network`, for the other entries to use and `kahlnws_network_free` to free: it holds the problem, the
/// basis and the optimum `network` holds, and changes apart from it.
///
/// A copy made before a change is the earlier state itself: a search may copy a network, change and solve the copy,
/// and keep whichever it wants. The copy answers every call as `network` would, its work included. It allocates its
/// own handle and room, as large as the problem held, and its changes and solves allocate nothing; a network half
/// changed copies as half changed. `network` is read, never changed. `kahlnws_network_copy` copies into a network
/// that already exists, reusing its room.
///
/// # Parameters
/// - `network` (in): the network to copy.
///
/// # Returns
/// The copy, or null where `network` is null or not aligned for a network, or where the memory for the copy's handle
/// or for its room is refused; nothing else answers null, and an internal error would be answered as null too.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_clone(network: *const Network) -> *mut Network {
    guarded(ptr::null_mut(), || {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let Ok(network) = (unsafe { boundary::handle_ref(network) }) else {
            return ptr::null_mut();
        };

        network.try_clone().map_or(ptr::null_mut(), boundary::handle)
    })
}

/// Makes `destination` a copy of `source`: it holds the problem, the basis and the optimum `source` holds, and changes
/// apart from it, as a copy `kahlnws_network_clone` makes does.
///
/// It reuses the room `destination` holds: where that room holds a problem of the counts of the problem `source` holds,
/// by `kahlnws_network_reserve` or an earlier load or copy, the call allocates nothing. So a search may keep a network
/// for each state it may return to and copy into it without allocating. A network half changed copies as half
/// changed. `source` is read, never changed; the same network as both is a copy of itself, which changes nothing.
///
/// # Parameters
/// - `destination` (in, out): the network made a copy, whatever it held before.
/// - `source` (in): the network copied.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: `destination` is a copy of `source`;
/// - `KAHLNWS_STATUS_NO_ROOM`: the memory for the room the copy needs was refused; `destination` holds what it held,
///   its optimum included, and its room may have grown;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `destination` or `source` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `destination` or `source` is not aligned for a network;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught; `destination` may be half changed.
///
/// # Safety
/// - `destination` and `source` are each null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone`
///   answered that has not been freed since and that no other call uses meanwhile: one thread at a time uses a
///   network;
/// - they are the same pointer, or point to two different networks.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_copy(destination: *mut Network, source: *const Network) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `source` is null or a network the library answered, not freed and not in use.
        let source = unsafe { boundary::handle_ref(source) }?;
        if ptr::eq(destination.cast_const(), source) {
            return Ok(Status::Ok);
        }
        // SAFETY: C guarantees `destination` is null or a network the library answered, not freed and not in use, and
        // another than `source`, as just compared: the two borrows do not alias.
        let destination = unsafe { boundary::handle_mut(destination) }?;

        Ok(destination.copy_from(source))
    })
}

/// Frees `network` with all its room.
///
/// Null is accepted and frees nothing, and so is a pointer not aligned for a network, which `kahlnws_network_new` and
/// `kahlnws_network_clone` never answer. After the call the network is freed: no call may use the pointer again. A
/// network may be freed on any thread.
///
/// # Parameters
/// - `network` (in): the network to free, or null.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_free(network: *mut Network) {
    guarded((), || {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        unsafe { boundary::free_handle(network) }
    });
}

/// Makes room in `network` for problems of up to `node_count` nodes and `arc_count` arcs.
///
/// A load of such a problem allocates nothing afterwards. Room already held is kept, and nothing else changes: the
/// problem held, and its optimum, stay.
///
/// # Parameters
/// - `network` (in, out): the network whose room is made.
/// - `node_count` (in): the most nodes a problem will have.
/// - `arc_count` (in): the most arcs a problem will have.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: the room is made;
/// - `KAHLNWS_STATUS_NO_ROOM`: the memory was refused; the room held before is kept, and the network may be used;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `network` is not aligned for a network;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_reserve(
    network: *mut Network,
    node_count: usize,
    arc_count: usize,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_mut(network) }?;

        Ok(network.reserve(node_count, arc_count))
    })
}

/// Makes `problem` the problem `network` holds, in place of the one it held, with no optimum answered yet, and a
/// basis of it to start the next `kahlnws_network_solve` from.
///
/// The basis is the one the network holds, carried over to the new numbers, where it serves: where the network holds
/// a basis of a problem with the same node count, the same arc count and the same tail and head at every arc, and the
/// supplies net of the lower bounds and of what that basis carries on its arcs out of the tree add up to at most
/// `2^64 - 2` where they are positive. Otherwise, and where the network holds none, never loaded or left half changed,
/// it is the star, every arc at its lower bound, as the first load starts from. Either way the next solve prices every
/// arc.
///
/// The solve after a load answers the same status and the same cost whichever basis the load started from; the flow
/// among tied optima, the potentials and the work may differ, each a function of the calls made on the network, which
/// is deterministic. Where only some numbers changed since the problem the network held, the solve often makes far
/// fewer pivots than one from the star; the same problem loaded again, from the network's optimum, makes none. So a
/// network that loads new numbers on the same arcs each day carries its basis from day to day.
///
/// The problem's tables are copied: the caller may change or free them after the call. The load refuses what
/// `kahlnws_solver_solve` refuses, with the same breach; a refused load, and refused memory, leave the network as it
/// was, its basis and its optimum included, and the next load starts from that basis where it serves. A load mends a
/// network half changed. It costs a few passes over the arcs and the nodes.
///
/// Room. Before the problem is checked, the network's room is grown to the problem's counts and kept, so that refused
/// memory is answered rather than ending the process; where the room already holds the problem, by
/// `kahlnws_network_reserve` or an earlier load, the call allocates nothing.
///
/// # Parameters
/// - `network` (in, out): the network, whose room the load uses and may grow.
/// - `problem` (in): the problem to hold.
/// - `refusal` (out): where the breach of a refused problem is written, or null where it is not wanted.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: the network holds the problem;
/// - `KAHLNWS_STATUS_REFUSED`: the problem lies outside what a solve admits; where `refusal` is not null, the breach
///   is written to it;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` or `problem` is null, or a table of the problem other than its lower
///   bounds is null while its count is above zero;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer the call reads or writes is not aligned for its type, or a count of
///   entries passes `PTRDIFF_MAX` bytes;
/// - `KAHLNWS_STATUS_NO_ROOM`: the memory for the room the problem needs was refused;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `problem` is null or points to a `kahlnws_problem` whose every pointer points to what its doc states;
/// - `refusal` is null or points to writable storage for one value, whatever it held before;
/// - the refusal overlaps neither the problem nor its tables; nothing writes the problem or its tables, and nothing
///   else reads or writes the refusal, while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_load(
    network: *mut Network,
    problem: *const Problem,
    refusal: *mut Refusal,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_mut(network) }?;
        // SAFETY: C guarantees `problem` is null or points to a `kahlnws_problem` whose tables point to what its doc
        // states, which nothing writes meanwhile.
        let problem = unsafe { boundary::lent_problem(problem) }?;
        // SAFETY: C guarantees `refusal` is null or points to writable storage for one value of its own.
        let refusal = unsafe { boundary::optional_output(refusal) }?;

        Ok(network.load(problem, refusal))
    })
}

/// The number of nodes of the problem `network` holds: 0 for a network never loaded.
///
/// Every node a change or a read names is below it. It reads, and never fails.
///
/// # Parameters
/// - `network` (in): the network, or null.
///
/// # Returns
/// The number of nodes; 0 where `network` is null or not aligned for a network.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_node_count(network: *const Network) -> usize {
    guarded(0, || {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        unsafe { boundary::handle_ref(network) }.map_or(0, Network::node_count)
    })
}

/// The number of arcs of the problem `network` holds: 0 for a network never loaded.
///
/// Every arc a change or a read names is below it, and the flows `kahlnws_network_flows` writes are as many. It reads,
/// and never fails.
///
/// # Parameters
/// - `network` (in): the network, or null.
///
/// # Returns
/// The number of arcs; 0 where `network` is null or not aligned for a network.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_arc_count(network: *const Network) -> usize {
    guarded(0, || {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        unsafe { boundary::handle_ref(network) }.map_or(0, Network::arc_count)
    })
}

/// Makes `cost` the cost of a unit of flow on `arc` of the problem `network` holds, in units of cost.
///
/// The network then holds the problem with that cost, with a basis of it, from which the next `kahlnws_network_solve`
/// answers the optimum of the problem as it now stands. The cost the arc already has changes nothing, and keeps an
/// optimum the network held; any other cost ends it. Setting the cost written to `previous` back makes the problem
/// the earlier one again.
///
/// It costs a few steps where the change moves no node's potential, and otherwise the nodes whose potentials it
/// moves, which can be most of them; the next solve prices the arc, and where potentials moved, the arcs at the
/// smaller of the two groups of nodes, those whose potentials moved and the rest, which can be half the network. It
/// allocates nothing.
///
/// # Parameters
/// - `network` (in, out): the network.
/// - `arc` (in): the arc, below `kahlnws_network_arc_count(network)`.
/// - `cost` (in): its new cost, in units of cost.
/// - `previous` (out): where the cost the arc had is written, or null where it is not wanted.
/// - `refusal` (out): where the breach of a refused change is written, or null where it is not wanted.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: the arc has the cost; where `previous` is not null, the cost it had is written to it;
/// - `KAHLNWS_STATUS_REFUSED`: nothing changed; where `refusal` is not null, the breach is written to it, of kind
///   `KAHLNWS_REFUSAL_KIND_COST_BEYOND_BOUND` where `cost` lies beyond plus or minus `kahlnws_cost_bound` of the node
///   count, else `KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE` where with it the magnitudes of the costs times the
///   capacities would add up past `2^127 - 1`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `arc` is not below the arc count, which on a network never loaded no arc is,
///   or a pointer is not aligned for its type; nothing changed;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `previous` and `refusal` are each null or point to writable storage for one value, whatever it held before;
/// - neither overlaps the other, and nothing else reads or writes them while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_set_cost(
    network: *mut Network,
    arc: usize,
    cost: i64,
    previous: *mut i64,
    refusal: *mut Refusal,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_mut(network) }?;
        // SAFETY: C guarantees `previous` is null or points to writable storage for one value of its own.
        let previous = unsafe { boundary::optional_output(previous) }?;
        // SAFETY: C guarantees `refusal` is null or points to writable storage for one value of its own.
        let refusal = unsafe { boundary::optional_output(refusal) }?;

        Ok(network.set_cost(arc, cost, previous, refusal))
    })
}

/// Makes `capacity` the most flow of `arc` of the problem `network` holds, in units of flow.
///
/// The network then holds the problem with that capacity, with a basis of it, from which the next
/// `kahlnws_network_solve` answers the optimum of the problem as it now stands. The capacity the arc already has
/// changes nothing, and keeps an optimum the network held; any other ends it. Setting the capacity written to
/// `previous` back makes the problem the earlier one again. A capacity of zero on an arc whose lower bound is zero
/// switches the arc off, and the capacity it had, set back, switches it on: a caller may build every arc it may ever
/// need up front and switch each on and off. Where both bounds are to move past each other's old value,
/// `kahlnws_network_set_bounds` moves them at once.
///
/// It costs a few steps where the arc's flow can stay where it is, and otherwise what `kahlnws_network_move_supply`
/// of the flow the change moves costs, that flow being at most the change of the capacity; switching off an arc that
/// carries flow always moves it. The next solve prices the arc, and what that move of supply asks it to. It allocates
/// nothing.
///
/// # Parameters
/// - `network` (in, out): the network.
/// - `arc` (in): the arc, below `kahlnws_network_arc_count(network)`.
/// - `capacity` (in): its new capacity, in units of flow.
/// - `previous` (out): where the capacity the arc had is written, or null where it is not wanted.
/// - `refusal` (out): where the breach of a refused change is written, or null where it is not wanted.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: the arc has the capacity; where `previous` is not null, the capacity it had is written
///   to it;
/// - `KAHLNWS_STATUS_REFUSED`: nothing changed; where `refusal` is not null, the breach is written to it, of kind
///   `KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY` where `capacity` is below the arc's lower bound, else
///   `KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE` where with it the magnitudes of the costs times the capacities
///   would add up past `2^127 - 1`, else `KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE` where the flow still to be sent
///   and the flow this change moves add up past `2^64 - 2`. The capacity written to `previous`, set back directly
///   after, is admitted wherever the flow still to be sent after the change and the change of the capacity add up to
///   at most `2^64 - 2`, so wherever that flow before the change and twice the change of the capacity do: never
///   refused for capacities and a flow below `2^62`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `arc` is not below the arc count, which on a network never loaded no arc is,
///   or a pointer is not aligned for its type; nothing changed;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `previous` and `refusal` are each null or point to writable storage for one value, whatever it held before;
/// - neither overlaps the other, and nothing else reads or writes them while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_set_capacity(
    network: *mut Network,
    arc: usize,
    capacity: u64,
    previous: *mut u64,
    refusal: *mut Refusal,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_mut(network) }?;
        // SAFETY: C guarantees `previous` is null or points to writable storage for one value of its own.
        let previous = unsafe { boundary::optional_output(previous) }?;
        // SAFETY: C guarantees `refusal` is null or points to writable storage for one value of its own.
        let refusal = unsafe { boundary::optional_output(refusal) }?;

        Ok(network.set_capacity(arc, capacity, previous, refusal))
    })
}

/// Makes `lower_bound` the least flow of `arc` of the problem `network` holds, in units of flow.
///
/// The network then holds the problem with that lower bound, with a basis of it, from which the next
/// `kahlnws_network_solve` answers the optimum of the problem as it now stands; every flow it answers includes its
/// arc's lower bound. The lower bound the arc already has changes nothing, and keeps an optimum the network held; any
/// other ends it. Setting the lower bound written to `previous` back makes the problem the earlier one again. Where
/// both bounds are to move past each other's old value, `kahlnws_network_set_bounds` moves them at once. A lower bound
/// changes no supply: the supplies net of the lower bounds may so come to pass the limit
/// `KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE` names, where the arcs carry the excess, and the network still solves
/// them exactly, where a solver refuses them.
///
/// It costs a few steps where the arc's flow can stay where it is, and otherwise what `kahlnws_network_move_supply`
/// of the flow the change moves costs, that flow being at most the change of the lower bound; an arc at its old lower
/// bound always moves by the change. The next solve prices the arc, and what that move of supply asks it to. It
/// allocates nothing.
///
/// # Parameters
/// - `network` (in, out): the network.
/// - `arc` (in): the arc, below `kahlnws_network_arc_count(network)`.
/// - `lower_bound` (in): its new lower bound, in units of flow.
/// - `previous` (out): where the lower bound the arc had is written, or null where it is not wanted.
/// - `refusal` (out): where the breach of a refused change is written, or null where it is not wanted.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: the arc has the lower bound; where `previous` is not null, the lower bound it had is
///   written to it;
/// - `KAHLNWS_STATUS_REFUSED`: nothing changed; where `refusal` is not null, the breach is written to it, of kind
///   `KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY` where `lower_bound` is above the arc's capacity, else
///   `KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE` where the flow still to be sent and the flow this change moves add up
///   past `2^64 - 2`. The lower bound written to `previous`, set back directly after, is admitted wherever the flow
///   still to be sent after the change and the change of the lower bound add up to at most `2^64 - 2`, so wherever
///   that flow before the change and twice the change of the lower bound do: never refused for bounds and a flow below
///   `2^62`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `arc` is not below the arc count, which on a network never loaded no arc is,
///   or a pointer is not aligned for its type; nothing changed;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `previous` and `refusal` are each null or point to writable storage for one value, whatever it held before;
/// - neither overlaps the other, and nothing else reads or writes them while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_set_lower_bound(
    network: *mut Network,
    arc: usize,
    lower_bound: u64,
    previous: *mut u64,
    refusal: *mut Refusal,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_mut(network) }?;
        // SAFETY: C guarantees `previous` is null or points to writable storage for one value of its own.
        let previous = unsafe { boundary::optional_output(previous) }?;
        // SAFETY: C guarantees `refusal` is null or points to writable storage for one value of its own.
        let refusal = unsafe { boundary::optional_output(refusal) }?;

        Ok(network.set_lower_bound(arc, lower_bound, previous, refusal))
    })
}

/// Makes `lower_bound` and `capacity` the least and the most flow of `arc` of the problem `network` holds at once, in
/// units of flow.
///
/// The single setters check each bound against the other the arc holds, so where both bounds move past each other's
/// old value, the order of two calls matters and one order is refused: bounds of 0 to 2 become 3 to 4 by the capacity
/// first, and 3 to 4 become 0 to 2 by the lower bound first. This sets both at once, with no order, and checks the
/// pair alone. Otherwise it is `kahlnws_network_set_capacity` and `kahlnws_network_set_lower_bound` in one: the
/// network then holds the problem with those bounds, with a basis of it; the bounds the arc already has change nothing
/// and keep an optimum the network held; any others end it; the bounds written to `previous_lower_bound` and
/// `previous_capacity`, set back, make the problem the earlier one again.
///
/// It costs a few steps where the arc's flow can stay where it is, and otherwise what `kahlnws_network_move_supply` of
/// the flow the change moves costs, that flow being at most the larger change of the two bounds. The next solve
/// prices the arc, and what that move of supply asks it to. It allocates nothing.
///
/// # Parameters
/// - `network` (in, out): the network.
/// - `arc` (in): the arc, below `kahlnws_network_arc_count(network)`.
/// - `lower_bound` (in): its new lower bound, in units of flow.
/// - `capacity` (in): its new capacity, in units of flow.
/// - `previous_lower_bound` (out): where the lower bound the arc had is written, or null where it is not wanted.
/// - `previous_capacity` (out): where the capacity the arc had is written, or null where it is not wanted.
/// - `refusal` (out): where the breach of a refused change is written, or null where it is not wanted.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: the arc has the bounds; where `previous_lower_bound` and `previous_capacity` are not
///   null, the lower bound and the capacity it had are written to them;
/// - `KAHLNWS_STATUS_REFUSED`: nothing changed; where `refusal` is not null, the breach is written to it, of kind
///   `KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY` where `lower_bound` is above `capacity`, else
///   `KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE` where with that capacity the magnitudes of the costs times the
///   capacities would add up past `2^127 - 1`, else `KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE` where the flow still to
///   be sent and the flow this change moves add up past `2^64 - 2`. The bounds written, set back directly after, are
///   admitted wherever the flow still to be sent after the change and the larger change of the two bounds add up to
///   at most `2^64 - 2`, so wherever that flow before the change and twice the larger change do: never refused for
///   bounds and a flow below `2^62`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `arc` is not below the arc count, which on a network never loaded no arc is,
///   or a pointer is not aligned for its type; nothing changed;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `previous_lower_bound`, `previous_capacity` and `refusal` are each null or point to writable storage for one
///   value, whatever it held before;
/// - none overlaps another, and nothing else reads or writes them while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_set_bounds(
    network: *mut Network,
    arc: usize,
    lower_bound: u64,
    capacity: u64,
    previous_lower_bound: *mut u64,
    previous_capacity: *mut u64,
    refusal: *mut Refusal,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_mut(network) }?;
        // SAFETY: C guarantees `previous_lower_bound` is null or points to writable storage for one value of its own.
        let previous_lower_bound = unsafe { boundary::optional_output(previous_lower_bound) }?;
        // SAFETY: C guarantees `previous_capacity` is null or points to writable storage for one value of its own.
        let previous_capacity = unsafe { boundary::optional_output(previous_capacity) }?;
        // SAFETY: C guarantees `refusal` is null or points to writable storage for one value of its own.
        let refusal = unsafe { boundary::optional_output(refusal) }?;
        let bounds = Bounds { lower_bound, capacity };

        Ok(network.set_bounds(arc, bounds, previous_lower_bound, previous_capacity, refusal))
    })
}

/// Makes `supply` the supply of `node` in the problem `network` holds, in units of flow, a demand below zero; the
/// supply of `balancing_node` moves by the same amount the other way, so the supplies still add up to zero.
///
/// A depot that is to send 2 units where it sent 4, balanced by the customer it sends to, is
/// `kahlnws_network_set_supply(network, depot, 2, customer, &had, NULL)`: the customer then takes 2 less. The same call
/// with the supply written to `previous` sets both nodes back. The network then holds the problem with those supplies,
/// with a basis of it, from which the next `kahlnws_network_solve` answers the optimum of the problem as it now stands.
/// The supply `node` already has changes nothing, is never refused, and keeps an optimum the network held, as a move of
/// nothing does; any other ends it. A node that balances itself is given only the supply it has, which changes nothing;
/// any other supply for it answers `KAHLNWS_STATUS_INVALID_ARGUMENT`, as an index not below its count does.
///
/// It is `kahlnws_network_move_supply` of the difference between the supply asked for and the one `node` has: from
/// `balancing_node` to `node` where the supply rises, from `node` to `balancing_node` where it falls. It costs and
/// marks what that move costs and marks, and allocates nothing.
///
/// # Parameters
/// - `network` (in, out): the network.
/// - `node` (in): the node whose supply is set, below `kahlnws_network_node_count(network)`.
/// - `supply` (in): its new supply, in units of flow: positive where it sends, negative where it takes.
/// - `balancing_node` (in): the node whose supply moves the other way, below `kahlnws_network_node_count(network)`.
/// - `previous` (out): where the supply `node` had is written, or null where it is not wanted.
/// - `refusal` (out): where the breach of a refused change is written, or null where it is not wanted.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: `node` has the supply, and `balancing_node` the difference the other way; where `previous`
///   is not null, the supply `node` had is written to it;
/// - `KAHLNWS_STATUS_REFUSED`: nothing changed; where `refusal` is not null, the breach is written to it, of kind
///   `KAHLNWS_REFUSAL_KIND_SUPPLY_BEYOND_RANGE` where the supply of `balancing_node` would leave the range of
///   `int64_t`, naming it with the supply it would have; else `KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE` where the flow
///   still to be sent and the difference add up past `2^64 - 2`. The supply written to `previous`, set back directly
///   after, is admitted where the move it makes would be, as `kahlnws_network_move_supply` says;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `node` or `balancing_node` is not below the node count, which on a network
///   never loaded no node is; or `node` is `balancing_node` and `supply` is not the supply it has; or a pointer is not
///   aligned for its type; nothing changed;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `previous` and `refusal` are each null or point to writable storage for one value, whatever it held before;
/// - neither overlaps the other, and nothing else reads or writes them while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_set_supply(
    network: *mut Network,
    node: usize,
    supply: i64,
    balancing_node: usize,
    previous: *mut i64,
    refusal: *mut Refusal,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_mut(network) }?;
        // SAFETY: C guarantees `previous` is null or points to writable storage for one value of its own.
        let previous = unsafe { boundary::optional_output(previous) }?;
        // SAFETY: C guarantees `refusal` is null or points to writable storage for one value of its own.
        let refusal = unsafe { boundary::optional_output(refusal) }?;

        Ok(network.set_supply(node, supply, balancing_node, previous, refusal))
    })
}

/// Lowers the supply of `from_node` by `amount` and raises the supply of `to_node` by `amount`, in units of flow, in
/// the problem `network` holds, so the supplies still add up to zero; a move of demand is written from the node that
/// will demand more, a customer that needs two more units, sent by a depot, being
/// `kahlnws_network_move_supply(network, customer, depot, 2, NULL)`.
///
/// A node that sends gains supply, one that receives gains demand, a supply below zero. A node is brought to a supply
/// it should have, balanced by another node, by `kahlnws_network_set_supply`. The network then holds the problem with
/// those supplies, with a basis of it, from which the next `kahlnws_network_solve` answers the optimum of the problem
/// as it now stands. The same node twice, or an amount of zero, moves nothing: it changes nothing, is never refused,
/// and keeps an optimum the network held; any other move ends it. The move back,
/// `kahlnws_network_move_supply(network, to_node, from_node, amount, NULL)`, makes the problem the earlier one again.
/// Called directly after the move, it is admitted wherever the move left the flow still to be sent as it was or lower,
/// always where the routed flow carried the move, and wherever that flow before the move and twice the amount add up
/// to at most `2^64 - 2`: so never for amounts and a flow below `2^62`. Other calls in between can leave that flow
/// higher, a solve that finds no flow among them, which does not lower it. A move back that is refused stays refused
/// until the flow still to be sent falls, by a solve that finds a flow or by other moves; a `kahlnws_network_load` of
/// the earlier problem always undoes it. A caller that holds new supplies whole pairs each node whose supply rises with
/// one whose supply falls and moves the lesser of their two remaining differences, until none is left: every supply on
/// the way lies between its old and its new value. The supplies a network comes to hold always lie within `int64_t`;
/// their net supplies may pass the limit `KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE` names, where the arcs carry the
/// difference, and the network still solves them exactly, where a solver refuses them.
///
/// It costs a walk from each of the two nodes along the flow the network routes, as far as their routes meet, where
/// that flow can carry the amount; where it cannot, the nodes whose potentials move, up to most of the network, and
/// where flow comes to be left to be sent, a read of each node that heads a part of the network. The next solve prices
/// no arc for a move the routed flow carried, and otherwise the arcs at the smaller group of the nodes whose
/// potentials moved and the rest. It allocates nothing.
///
/// # Parameters
/// - `network` (in, out): the network.
/// - `from_node` (in): the node whose supply falls, below `kahlnws_network_node_count(network)`.
/// - `to_node` (in): the node whose supply rises, below `kahlnws_network_node_count(network)`.
/// - `amount` (in): the units of flow moved.
/// - `refusal` (out): where the breach of a refused change is written, or null where it is not wanted.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: the supplies are moved, or nothing was to move;
/// - `KAHLNWS_STATUS_REFUSED`: nothing changed; where `refusal` is not null, the breach is written to it, of kind
///   `KAHLNWS_REFUSAL_KIND_SUPPLY_BEYOND_RANGE` where the supply of `from_node` less `amount` falls below the least
///   `int64_t`, or the supply of `to_node` plus `amount` passes the greatest, naming the first of the two in that
///   order; else `KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE` where the flow still to be sent and `amount` add up past
///   `2^64 - 2`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `from_node` or `to_node` is not below the node count, which on a network never
///   loaded no node is, or `refusal` is not aligned for its type; nothing changed;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `refusal` is null or points to writable storage for one value, whatever it held before, that nothing else reads
///   or writes while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_move_supply(
    network: *mut Network,
    from_node: usize,
    to_node: usize,
    amount: u64,
    refusal: *mut Refusal,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_mut(network) }?;
        // SAFETY: C guarantees `refusal` is null or points to writable storage for one value of its own.
        let refusal = unsafe { boundary::optional_output(refusal) }?;

        Ok(network.move_supply(from_node, to_node, amount, refusal))
    })
}

/// Solves the problem `network` holds: finds a flow of least cost for the problem as it stands, or that no flow
/// within the bounds meets the supplies, in exact integers, and keeps it for the reads.
///
/// It pivots from the basis the network holds: the one its last solve ended on and the changes since left, or the one
/// a load started from. A network nothing changed since its last solve answers the same again, prices no arc and makes
/// no pivot; a solve after changes that call for no pivot prices only the arcs those changes may have made worth
/// using; and a solve that pivots prices at least one whole pass over the arcs. Where a change leaves much flow still
/// to be sent and the solve makes many pivots, it can take longer than a load of the whole problem as it stands and
/// the solve after it. It tries no arcs first, so the `first_arc_pivots` it writes are 0. A network never loaded
/// answers the optimum of no flow at the cost zero, with no work. It allocates nothing.
///
/// After `KAHLNWS_STATUS_OPTIMAL` the reads answer that optimum until the next load or change that changes the
/// problem; after `KAHLNWS_STATUS_INFEASIBLE` they answer `KAHLNWS_STATUS_NO_OPTIMUM`. The total cost and the
/// potentials are those of the problem with its lower bounds, and every flow includes its arc's lower bound.
///
/// # Parameters
/// - `network` (in, out): the network.
/// - `total_cost` (out): where the cost of the flow is written, or null where it is not wanted.
/// - `work` (out): where the work of the solve is written, or null where it is not wanted.
///
/// # Returns
/// - `KAHLNWS_STATUS_OPTIMAL`: a flow of least cost is found; where `total_cost` is not null, its cost is written to
///   it, and where `work` is not null, the work of the solve;
/// - `KAHLNWS_STATUS_INFEASIBLE`: no flow within the bounds meets the supplies; where `work` is not null, the work of
///   the solve is written to it;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer is not aligned for its type;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `total_cost` and `work` are each null or point to writable storage for one value, whatever it held before;
/// - neither overlaps the other, and nothing else reads or writes them while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_solve(
    network: *mut Network,
    total_cost: *mut Int128,
    work: *mut Work,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_mut(network) }?;
        // SAFETY: C guarantees `total_cost` is null or points to writable storage for one value of its own.
        let total_cost = unsafe { boundary::optional_output(total_cost) }?;
        // SAFETY: C guarantees `work` is null or points to writable storage for one value of its own.
        let work = unsafe { boundary::optional_output(work) }?;

        Ok(network.solve(total_cost, work))
    })
}

/// Writes the cost of the optimum `network` holds, in units of cost times units of flow.
///
/// A network holds an optimum where the last call that loaded, changed or solved it was a `kahlnws_network_solve`
/// that answered `KAHLNWS_STATUS_OPTIMAL`, and on a network never loaded, whose optimum is that of no node; a change
/// that was refused, or that set the number already held, is no change. It reads, changes nothing, and costs a few
/// steps.
///
/// # Parameters
/// - `network` (in): the network.
/// - `total_cost` (out): where the cost is written.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: the cost is written to `total_cost`;
/// - `KAHLNWS_STATUS_NO_OPTIMUM`: the network holds no optimum; nothing is written;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` or `total_cost` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer is not aligned for its type;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `total_cost` is null or points to writable storage for one value, whatever it held before, that nothing else
///   reads or writes while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_total_cost(network: *const Network, total_cost: *mut Int128) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `total_cost` is null or points to writable storage for one value of its own.
        let total_cost = unsafe { boundary::output(total_cost) }?;

        Ok(network.read_optimum(OptimumRead::TotalCost(total_cost)))
    })
}

/// Writes the flow of `arc` in the optimum `network` holds, in units of flow, its lower bound included.
///
/// A network holds an optimum as `kahlnws_network_total_cost` says. It reads one flow, changes nothing, and costs a
/// few steps.
///
/// # Parameters
/// - `network` (in): the network.
/// - `arc` (in): the arc, below `kahlnws_network_arc_count(network)`.
/// - `flow` (out): where the flow is written.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: the flow is written to `flow`;
/// - `KAHLNWS_STATUS_NO_OPTIMUM`: the network holds no optimum; nothing is written;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` or `flow` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `arc` is not below the arc count, or a pointer is not aligned for its type;
///   nothing is written;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `flow` is null or points to writable storage for one value, whatever it held before, that nothing else reads or
///   writes while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_flow(network: *const Network, arc: usize, flow: *mut u64) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `flow` is null or points to writable storage for one value of its own.
        let flow = unsafe { boundary::output(flow) }?;

        Ok(network.read_optimum(OptimumRead::Flow { arc, flow }))
    })
}

/// Writes the flow of every arc in the optimum `network` holds, arc `a` at `a`, in units of flow, each with its arc's
/// lower bound.
///
/// A network holds an optimum as `kahlnws_network_total_cost` says. It reads every flow once, changes nothing, and
/// allocates nothing.
///
/// # Parameters
/// - `network` (in): the network.
/// - `flows` (out): where the flows are written: storage for `kahlnws_network_arc_count(network)` entries, which may
///   be null where that count is zero.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: the flow of each arc is written to `flows`;
/// - `KAHLNWS_STATUS_NO_OPTIMUM`: the network holds no optimum; nothing is written;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null, or `flows` is null while the arc count is above zero;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer is not aligned for its type;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `flows` points to writable storage for `kahlnws_network_arc_count(network)` entries within one object, whatever
///   it held before, and may be null where that count is zero; nothing else reads or writes it while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_flows(network: *const Network, flows: *mut u64) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `flows` points to writable storage of its own for one entry per arc of the network.
        let flows = unsafe { boundary::output_table(flows, network.arc_count()) }?;

        Ok(network.read_optimum(OptimumRead::Flows(flows)))
    })
}

/// Writes each node's potential in the optimum `network` holds, node `v` at `v`, in units of cost.
///
/// With the potentials, `cost + potential[tail] - potential[head]` is at least zero on every arc whose flow is below
/// its capacity and at most zero on every arc whose flow is above its lower bound, which proves the flow optimal. A
/// network holds an optimum as `kahlnws_network_total_cost` says. It reads every potential once, changes nothing,
/// and allocates nothing.
///
/// # Parameters
/// - `network` (in): the network.
/// - `potentials` (out): where the potentials are written: storage for `kahlnws_network_node_count(network)` entries,
///   which may be null where that count is zero.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: each node's potential is written to `potentials`;
/// - `KAHLNWS_STATUS_NO_OPTIMUM`: the network holds no optimum; nothing is written;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null, or `potentials` is null while the node count is above zero;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer is not aligned for its type;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `potentials` points to writable storage for `kahlnws_network_node_count(network)` entries within one object,
///   whatever it held before, and may be null where that count is zero; nothing else reads or writes it while the
///   call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_potentials(network: *const Network, potentials: *mut i64) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `potentials` points to writable storage of its own for one entry per node of the
        // network.
        let potentials = unsafe { boundary::output_table(potentials, network.node_count()) }?;

        Ok(network.read_optimum(OptimumRead::Potentials(potentials)))
    })
}

/// Writes the node `arc` leaves in the problem `network` holds.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
///
/// # Parameters
/// - `network` (in): the network.
/// - `arc` (in): the arc, below `kahlnws_network_arc_count(network)`.
/// - `tail` (out): where its tail is written.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: its tail is written to `tail`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` or `tail` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `arc` is not below the arc count, or a pointer is not aligned for its type;
///   nothing is written;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `tail` is null or points to writable storage for one value, whatever it held before, that nothing else reads or
///   writes while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_tail(network: *const Network, arc: usize, tail: *mut usize) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `tail` is null or points to writable storage for one value of its own.
        let tail = unsafe { boundary::output(tail) }?;

        Ok(network.read_problem(ProblemRead::Tail { arc, tail }))
    })
}

/// Writes the node `arc` enters in the problem `network` holds.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
///
/// # Parameters
/// - `network` (in): the network.
/// - `arc` (in): the arc, below `kahlnws_network_arc_count(network)`.
/// - `head` (out): where its head is written.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: its head is written to `head`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` or `head` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `arc` is not below the arc count, or a pointer is not aligned for its type;
///   nothing is written;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `head` is null or points to writable storage for one value, whatever it held before, that nothing else reads or
///   writes while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_head(network: *const Network, arc: usize, head: *mut usize) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `head` is null or points to writable storage for one value of its own.
        let head = unsafe { boundary::output(head) }?;

        Ok(network.read_problem(ProblemRead::Head { arc, head }))
    })
}

/// Writes the cost of a unit of flow on `arc`, in units of cost, in the problem `network` holds.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
///
/// # Parameters
/// - `network` (in): the network.
/// - `arc` (in): the arc, below `kahlnws_network_arc_count(network)`.
/// - `cost` (out): where its cost is written.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: its cost is written to `cost`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` or `cost` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `arc` is not below the arc count, or a pointer is not aligned for its type;
///   nothing is written;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `cost` is null or points to writable storage for one value, whatever it held before, that nothing else reads or
///   writes while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_cost(network: *const Network, arc: usize, cost: *mut i64) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `cost` is null or points to writable storage for one value of its own.
        let cost = unsafe { boundary::output(cost) }?;

        Ok(network.read_problem(ProblemRead::Cost { arc, cost }))
    })
}

/// Writes the least flow of `arc`, in units of flow, in the problem `network` holds.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
///
/// # Parameters
/// - `network` (in): the network.
/// - `arc` (in): the arc, below `kahlnws_network_arc_count(network)`.
/// - `lower_bound` (out): where its lower bound is written.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: its lower bound is written to `lower_bound`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` or `lower_bound` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `arc` is not below the arc count, or a pointer is not aligned for its type;
///   nothing is written;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `lower_bound` is null or points to writable storage for one value, whatever it held before, that nothing else
///   reads or writes while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_lower_bound(
    network: *const Network,
    arc: usize,
    lower_bound: *mut u64,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `lower_bound` is null or points to writable storage for one value of its own.
        let lower_bound = unsafe { boundary::output(lower_bound) }?;

        Ok(network.read_problem(ProblemRead::LowerBound { arc, lower_bound }))
    })
}

/// Writes the most flow of `arc`, in units of flow, in the problem `network` holds.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
///
/// # Parameters
/// - `network` (in): the network.
/// - `arc` (in): the arc, below `kahlnws_network_arc_count(network)`.
/// - `capacity` (out): where its capacity is written.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: its capacity is written to `capacity`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` or `capacity` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `arc` is not below the arc count, or a pointer is not aligned for its type;
///   nothing is written;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `capacity` is null or points to writable storage for one value, whatever it held before, that nothing else reads
///   or writes while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_capacity(
    network: *const Network,
    arc: usize,
    capacity: *mut u64,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `capacity` is null or points to writable storage for one value of its own.
        let capacity = unsafe { boundary::output(capacity) }?;

        Ok(network.read_problem(ProblemRead::Capacity { arc, capacity }))
    })
}

/// Writes the lower bound and the capacity of `arc`, in units of flow, in the problem `network` holds.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
///
/// # Parameters
/// - `network` (in): the network.
/// - `arc` (in): the arc, below `kahlnws_network_arc_count(network)`.
/// - `lower_bound` (out): where its lower bound is written.
/// - `capacity` (out): where its capacity is written.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: its lower bound is written to `lower_bound` and its capacity to `capacity`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network`, `lower_bound` or `capacity` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `arc` is not below the arc count, or a pointer is not aligned for its type;
///   nothing is written;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `lower_bound` and `capacity` are each null or point to writable storage for one value, whatever it held before;
/// - neither overlaps the other, and nothing else reads or writes them while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_bounds(
    network: *const Network,
    arc: usize,
    lower_bound: *mut u64,
    capacity: *mut u64,
) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `lower_bound` is null or points to writable storage for one value of its own.
        let lower_bound = unsafe { boundary::output(lower_bound) }?;
        // SAFETY: C guarantees `capacity` is null or points to writable storage for one value of its own.
        let capacity = unsafe { boundary::output(capacity) }?;

        Ok(network.read_problem(ProblemRead::Bounds { arc, lower_bound, capacity }))
    })
}

/// Writes the supply of `node` in the problem `network` holds, in units of flow: positive where it sends, negative
/// where it takes.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no node is below its count. It costs a few steps.
///
/// # Parameters
/// - `network` (in): the network.
/// - `node` (in): the node, below `kahlnws_network_node_count(network)`.
/// - `supply` (out): where its supply is written.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: its supply is written to `supply`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` or `supply` is null;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: `node` is not below the node count, or a pointer is not aligned for its type;
///   nothing is written;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `supply` is null or points to writable storage for one value, whatever it held before, that nothing else reads or
///   writes while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_supply(network: *const Network, node: usize, supply: *mut i64) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `supply` is null or points to writable storage for one value of its own.
        let supply = unsafe { boundary::output(supply) }?;

        Ok(network.read_problem(ProblemRead::Supply { node, supply }))
    })
}

/// Writes the node each arc leaves in the problem `network` holds, arc `a` at `a`.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no arc is held, and nothing is written. It reads every entry
/// once, and allocates nothing.
///
/// # Parameters
/// - `network` (in): the network.
/// - `tails` (out): where they are written: storage for `kahlnws_network_arc_count(network)` entries, which may be null
///   where that count is zero.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: they are written to `tails`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null, or `tails` is null while the arc count is above zero;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer is not aligned for its type;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `tails` points to writable storage for `kahlnws_network_arc_count(network)` entries within one object, whatever it
///   held before, and may be null where that count is zero; nothing else reads or writes it while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_tails(network: *const Network, tails: *mut usize) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `tails` points to writable storage of its own for one entry per arc of the network.
        let tails = unsafe { boundary::output_table(tails, network.arc_count()) }?;

        Ok(network.read_problem(ProblemRead::Tails(tails)))
    })
}

/// Writes the node each arc enters in the problem `network` holds, arc `a` at `a`.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no arc is held, and nothing is written. It reads every entry
/// once, and allocates nothing.
///
/// # Parameters
/// - `network` (in): the network.
/// - `heads` (out): where they are written: storage for `kahlnws_network_arc_count(network)` entries, which may be null
///   where that count is zero.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: they are written to `heads`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null, or `heads` is null while the arc count is above zero;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer is not aligned for its type;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `heads` points to writable storage for `kahlnws_network_arc_count(network)` entries within one object, whatever it
///   held before, and may be null where that count is zero; nothing else reads or writes it while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_heads(network: *const Network, heads: *mut usize) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `heads` points to writable storage of its own for one entry per arc of the network.
        let heads = unsafe { boundary::output_table(heads, network.arc_count()) }?;

        Ok(network.read_problem(ProblemRead::Heads(heads)))
    })
}

/// Writes the cost of a unit of flow on each arc, in units of cost, in the problem `network` holds, arc `a` at `a`.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no arc is held, and nothing is written. It reads every entry
/// once, and allocates nothing.
///
/// # Parameters
/// - `network` (in): the network.
/// - `costs` (out): where they are written: storage for `kahlnws_network_arc_count(network)` entries, which may be null
///   where that count is zero.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: they are written to `costs`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null, or `costs` is null while the arc count is above zero;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer is not aligned for its type;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `costs` points to writable storage for `kahlnws_network_arc_count(network)` entries within one object, whatever it
///   held before, and may be null where that count is zero; nothing else reads or writes it while the call runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_costs(network: *const Network, costs: *mut i64) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `costs` points to writable storage of its own for one entry per arc of the network.
        let costs = unsafe { boundary::output_table(costs, network.arc_count()) }?;

        Ok(network.read_problem(ProblemRead::Costs(costs)))
    })
}

/// Writes the lower bound of each arc, in units of flow, in the problem `network` holds, arc `a` at `a`.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no arc is held, and nothing is written. It reads every entry
/// once, and allocates nothing.
///
/// # Parameters
/// - `network` (in): the network.
/// - `lower_bounds` (out): where they are written: storage for `kahlnws_network_arc_count(network)` entries, which may
///   be null where that count is zero.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: they are written to `lower_bounds`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null, or `lower_bounds` is null while the arc count is above zero;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer is not aligned for its type;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `lower_bounds` points to writable storage for `kahlnws_network_arc_count(network)` entries within one object,
///   whatever it held before, and may be null where that count is zero; nothing else reads or writes it while the call
///   runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_lower_bounds(network: *const Network, lower_bounds: *mut u64) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `lower_bounds` points to writable storage of its own for one entry per arc of the
        // network.
        let lower_bounds = unsafe { boundary::output_table(lower_bounds, network.arc_count()) }?;

        Ok(network.read_problem(ProblemRead::LowerBounds(lower_bounds)))
    })
}

/// Writes the capacity of each arc, in units of flow, in the problem `network` holds, arc `a` at `a`.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no arc is held, and nothing is written. It reads every entry
/// once, and allocates nothing.
///
/// # Parameters
/// - `network` (in): the network.
/// - `capacities` (out): where they are written: storage for `kahlnws_network_arc_count(network)` entries, which may be
///   null where that count is zero.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: they are written to `capacities`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null, or `capacities` is null while the arc count is above zero;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer is not aligned for its type;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `capacities` points to writable storage for `kahlnws_network_arc_count(network)` entries within one object,
///   whatever it held before, and may be null where that count is zero; nothing else reads or writes it while the call
///   runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_capacities(network: *const Network, capacities: *mut u64) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `capacities` points to writable storage of its own for one entry per arc of the network.
        let capacities = unsafe { boundary::output_table(capacities, network.arc_count()) }?;

        Ok(network.read_problem(ProblemRead::Capacities(capacities)))
    })
}

/// Writes the supply of each node, in units of flow, in the problem `network` holds, node `v` at `v`.
///
/// It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
/// network holds and needs none. On a network never loaded no node is held, and nothing is written. It reads every
/// entry once, and allocates nothing.
///
/// # Parameters
/// - `network` (in): the network.
/// - `supplies` (out): where they are written: storage for `kahlnws_network_node_count(network)` entries, which may be
///   null where that count is zero.
///
/// # Returns
/// - `KAHLNWS_STATUS_OK`: they are written to `supplies`;
/// - `KAHLNWS_STATUS_NULL_POINTER`: `network` is null, or `supplies` is null while the node count is above zero;
/// - `KAHLNWS_STATUS_INVALID_ARGUMENT`: a pointer is not aligned for its type;
/// - `KAHLNWS_STATUS_INTERNAL_ERROR`: an internal error was caught, or the network is half changed.
///
/// # Safety
/// - `network` is null, or a pointer `kahlnws_network_new` or `kahlnws_network_clone` answered that has not been freed
///   since and that no other call uses meanwhile: one thread at a time uses a network;
/// - `supplies` points to writable storage for `kahlnws_network_node_count(network)` entries within one object,
///   whatever it held before, and may be null where that count is zero; nothing else reads or writes it while the call
///   runs.
#[must_use]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_network_supplies(network: *const Network, supplies: *mut i64) -> StatusCode {
    status_of(|| {
        // SAFETY: C guarantees `network` is null or a network the library answered, not freed and not in use.
        let network = unsafe { boundary::handle_ref(network) }?;
        // SAFETY: C guarantees `supplies` points to writable storage of its own for one entry per node of the network.
        let supplies = unsafe { boundary::output_table(supplies, network.node_count()) }?;

        Ok(network.read_problem(ProblemRead::Supplies(supplies)))
    })
}

/// Writes the message of the breach `refusal` names to `buffer`, as UTF-8 ending in a NUL.
///
/// The message is the text kahlnws gives the breach, with its values. A buffer of at least the length answered plus
/// one receives it whole; a shorter one receives as much of it as ends on a character and leaves a byte for the NUL;
/// a capacity of zero, or a null `buffer` whatever `capacity` is, receives nothing, so a first call with neither tells
/// the length to make room for. Where `refusal` is null, not aligned for a refusal, or of a kind that names no breach,
/// the message is empty and the answer 0 (a breach's message is never empty); whenever the buffer has a byte, it ends
/// in a NUL. Where `buffer` is not null and `capacity` passes `PTRDIFF_MAX`, nothing is written and the answer is 0.
/// It allocates nothing, and may be called from any thread.
///
/// # Parameters
/// - `refusal` (in): the refusal whose message is written, or null.
/// - `buffer` (out): where the message is written: storage for `capacity` bytes, or null.
/// - `capacity` (in): the number of bytes `buffer` holds.
///
/// # Returns
/// The length of the whole message in bytes, the NUL not counted; 0 where there is no message.
///
/// # Safety
/// - `refusal` is null or points to an initialised `kahlnws_refusal`;
/// - `buffer` is null, or `capacity` is zero or passes `PTRDIFF_MAX`, or `buffer` points to writable storage for
///   `capacity` bytes within one object, whatever it held before;
/// - the buffer overlaps nothing of the refusal; nothing writes the refusal, and nothing else reads or writes the
///   buffer, while the call runs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_refusal_message(
    refusal: *const Refusal,
    buffer: *mut c_char,
    capacity: usize,
) -> usize {
    guarded(0, || {
        // SAFETY: C guarantees `buffer` is null, or `capacity` is zero or past `PTRDIFF_MAX` and checked, or `buffer`
        // is writable storage for `capacity` bytes that overlaps nothing else of the call and that nothing else uses
        // meanwhile.
        let Ok(buffer) = (unsafe { boundary::bytes_mut(buffer, capacity) }) else {
            return 0;
        };
        // SAFETY: C guarantees `refusal` is null or points to an initialised refusal nothing writes meanwhile.
        let refusal = unsafe { boundary::refusal_ref(refusal) }.copied().unwrap_or_default();

        refusal.write_message(buffer)
    })
}

/// Writes `value` in decimal to `buffer`, with a minus sign where it is negative, as ASCII ending in a NUL.
///
/// The text is the value exact, every digit of it, such as "-18446744073709551616": the form C has no `printf`
/// conversion for. A buffer of at least the length answered plus one receives it whole, and 41 bytes hold every value;
/// a shorter one receives as much of it as leaves a byte for the NUL; a capacity of zero, or a null `buffer` whatever
/// `capacity` is, receives nothing, so a first call with neither tells the length to make room for. Where `buffer` is
/// not null and `capacity` passes `PTRDIFF_MAX`, nothing is written and the answer is 0. It allocates nothing, and may
/// be called from any thread.
///
/// # Parameters
/// - `value` (in): the value written.
/// - `buffer` (out): where the text is written: storage for `capacity` bytes, or null.
/// - `capacity` (in): the number of bytes `buffer` holds.
///
/// # Returns
/// The length of the whole text in bytes, the NUL not counted, from 1 to 40; 0 only where `buffer` is not null and
/// `capacity` passes `PTRDIFF_MAX`, or for an internal error, which nothing in it can raise.
///
/// # Safety
/// - `buffer` is null, or `capacity` is zero or passes `PTRDIFF_MAX`, or `buffer` points to writable storage for
///   `capacity` bytes within one object, whatever it held before, that nothing else reads or writes while the call
///   runs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_int128_format(value: Int128, buffer: *mut c_char, capacity: usize) -> usize {
    guarded(0, || {
        // SAFETY: C guarantees `buffer` is null, or `capacity` is zero or past `PTRDIFF_MAX` and checked, or `buffer`
        // is writable storage for `capacity` bytes that nothing else uses meanwhile.
        let Ok(buffer) = (unsafe { boundary::bytes_mut(buffer, capacity) }) else {
            return 0;
        };

        value.write_decimal(buffer)
    })
}

/// Writes `value` in decimal to `buffer`, as ASCII ending in a NUL.
///
/// It writes as `kahlnws_int128_format` does, for an unsigned value, such as "18446744073709551616": 40 bytes hold
/// every value.
///
/// # Parameters
/// - `value` (in): the value written.
/// - `buffer` (out): where the text is written: storage for `capacity` bytes, or null.
/// - `capacity` (in): the number of bytes `buffer` holds.
///
/// # Returns
/// The length of the whole text in bytes, the NUL not counted, from 1 to 39; 0 only where `buffer` is not null and
/// `capacity` passes `PTRDIFF_MAX`, or for an internal error, which nothing in it can raise.
///
/// # Safety
/// - `buffer` is null, or `capacity` is zero or passes `PTRDIFF_MAX`, or `buffer` points to writable storage for
///   `capacity` bytes within one object, whatever it held before, that nothing else reads or writes while the call
///   runs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_uint128_format(value: UInt128, buffer: *mut c_char, capacity: usize) -> usize {
    guarded(0, || {
        // SAFETY: C guarantees `buffer` is null, or `capacity` is zero or past `PTRDIFF_MAX` and checked, or `buffer`
        // is writable storage for `capacity` bytes that nothing else uses meanwhile.
        let Ok(buffer) = (unsafe { boundary::bytes_mut(buffer, capacity) }) else {
            return 0;
        };

        value.write_decimal(buffer)
    })
}

/// The name of `status` for a person to read.
///
/// Safe to call from any thread.
///
/// # Parameters
/// - `status` (in): the status to name.
///
/// # Returns
/// A static NUL-terminated string the caller does not free, such as "optimal" or "null pointer"; "unknown status" for
/// a number no status has.
#[unsafe(no_mangle)]
pub extern "C" fn kahlnws_status_name(status: StatusCode) -> *const c_char {
    let name = guarded(c"internal error", || Status::of_code(status).map_or(c"unknown status", Status::name));

    name.as_ptr()
}

/// The largest magnitude a cost may have in a problem of `node_count` nodes: every cost of a problem a solve admits
/// lies within plus or minus it.
///
/// Safe to call from any thread.
///
/// # Parameters
/// - `node_count` (in): the number of nodes of the problem.
///
/// # Returns
/// `(2^62 - 1) / node_count`, rounded down, so `node_count` times it stays below `2^62`; for a problem of no node,
/// `2^62 - 1`. It is never negative: -1 would answer an internal error, which nothing in it can raise.
#[unsafe(no_mangle)]
pub extern "C" fn kahlnws_cost_bound(node_count: usize) -> i64 {
    guarded(-1, || kahlnws::cost_bound(node_count))
}

/// The version of the library linked.
///
/// Compare it with `KAHLNWS_VERSION`, the same number made of the version of the header. Safe to call from any
/// thread.
///
/// # Returns
/// `(major << 16) | (minor << 8) | patch`, where the minor and the patch version are each below 256.
#[unsafe(no_mangle)]
pub extern "C" fn kahlnws_version() -> u32 {
    guarded(0, || version::VERSION)
}

#[cfg(test)]
mod tests {
    //! The entries given storage that held no values, as C may give them. Each test shows that an entry never reads
    //! what an output held, which only an interpreter that tracks initialisation sees: a native run cannot tell such
    //! storage from any other. Each reads back only what the entry's doc says it wrote, through
    //! [`written`](crate::boundary::written), whose precondition is that the entry wrote it; that each entry writes is
    //! pinned by the tests of `tests/entries.rs`, whose storage starts as a value the entry cannot answer.

    use core::ffi::c_char;
    use core::mem::MaybeUninit;

    use super::*;
    use crate::boundary::{written, written_entries};

    /// The node each arc of the depot leaves.
    static TAILS: [usize; 3] = [0, 0, 1];
    /// The node each arc of the depot enters.
    static HEADS: [usize; 3] = [2, 1, 2];
    /// The cost of each arc of the depot.
    static COSTS: [i64; 3] = [5, 1, 1];
    /// The capacity of each arc of the depot.
    static CAPACITIES: [u64; 3] = [3, 4, 2];
    /// The capacity of each arc of the narrow depot, too little for its 4 units.
    static NARROW_CAPACITIES: [u64; 3] = [1, 1, 1];
    /// The supply of each node of the depot.
    static SUPPLIES: [i64; 3] = [4, 0, -4];
    /// Supplies of the depot's nodes that add up to 1, not zero.
    static UNBALANCED_SUPPLIES: [i64; 3] = [4, 0, -3];

    /// The depot: node 0 sends 4 units to node 2, directly by arc 0 (capacity 3, cost 5) or through node 1 by arcs 1
    /// and 2 (capacity 4 and 2, cost 1 each). Two units go each way, at a cost of 14, after four pivots, one of them
    /// degenerate.
    fn depot() -> Problem {
        Problem {
            node_count: 3,
            arc_count: 3,
            tails: TAILS.as_ptr(),
            heads: HEADS.as_ptr(),
            lower_bounds: ptr::null(),
            capacities: CAPACITIES.as_ptr(),
            costs: COSTS.as_ptr(),
            supplies: SUPPLIES.as_ptr(),
        }
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
    fn test_a_message_is_written_into_storage_that_held_no_values() {
        let refusal =
            Refusal { kind: RefusalKind::TAIL_OUTSIDE_NODES, arc: 2, node: 9, node_count: 4, ..Refusal::default() };
        let mut buffer = [MaybeUninit::<c_char>::uninit(); 64];

        // SAFETY: the refusal is the test's own, and the buffer is storage for 64 bytes nothing else uses.
        let length = unsafe { kahlnws_refusal_message(&raw const refusal, buffer.as_mut_ptr().cast(), 64) };
        // SAFETY: the message fits the buffer, so the call wrote it and its NUL, the first `length + 1` bytes.
        let message = unsafe { written_entries(&buffer[..=length]) };

        let expected = c"arc 2 leaves node 9, not below the node count 4";
        assert_eq!(message, expected.to_bytes_with_nul().iter().map(|&byte| byte.cast_signed()).collect::<Vec<_>>());
    }

    #[test]
    fn test_the_depot_is_solved_into_storage_that_held_no_values() {
        let problem = depot();
        let solver = new_solver();
        let (mut flows, mut potentials) = ([MaybeUninit::<u64>::uninit(); 3], [MaybeUninit::<i64>::uninit(); 3]);
        let (mut total_cost, mut work) = (MaybeUninit::<Int128>::uninit(), MaybeUninit::<Work>::uninit());

        // SAFETY: the solver is live, the problem points into the depot's tables, and each output is storage of the
        // test's for the counts the problem states, used by nothing else meanwhile.
        let status = unsafe {
            kahlnws_solver_solve(
                solver,
                &raw const problem,
                flows.as_mut_ptr().cast(),
                potentials.as_mut_ptr().cast(),
                total_cost.as_mut_ptr(),
                work.as_mut_ptr(),
                ptr::null_mut(),
            )
        };
        // SAFETY: the solver is live and used by nothing else from here on.
        unsafe { kahlnws_solver_free(solver) };

        assert_eq!(status, StatusCode::OPTIMAL);
        // SAFETY: an optimal solve wrote every flow, every potential, the cost and the work.
        let answered =
            unsafe { (written_entries(&flows), written_entries(&potentials), written(&total_cost), written(&work)) };
        let depot_work = Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 };
        assert_eq!(answered, (vec![2, 2, 2], vec![0, 1, 5], Int128::from(14), depot_work));
    }

    #[test]
    fn test_the_work_of_an_infeasible_solve_is_written_into_storage_that_held_no_value() {
        let problem = Problem { capacities: NARROW_CAPACITIES.as_ptr(), ..depot() };
        let solver = new_solver();
        let mut flows = [MaybeUninit::<u64>::uninit(); 3];
        let mut work = MaybeUninit::<Work>::uninit();

        // SAFETY: the solver is live, the problem points into the narrow depot's tables, and the flows and the work
        // are storage of the test's, used by nothing else meanwhile.
        let status = unsafe {
            kahlnws_solver_solve(
                solver,
                &raw const problem,
                flows.as_mut_ptr().cast(),
                ptr::null_mut(),
                ptr::null_mut(),
                work.as_mut_ptr(),
                ptr::null_mut(),
            )
        };
        // SAFETY: the solver is live and used by nothing else from here on.
        unsafe { kahlnws_solver_free(solver) };

        assert_eq!(status, StatusCode::INFEASIBLE);
        // SAFETY: an infeasible solve wrote the work.
        let work = unsafe { written(&work) };
        assert_eq!(work, Work { pivots: 3, degenerate_pivots: 1, first_arc_pivots: 0 });
    }

    #[test]
    fn test_a_refusal_is_written_into_storage_that_held_no_value() {
        let problem = Problem { supplies: UNBALANCED_SUPPLIES.as_ptr(), ..depot() };
        let solver = new_solver();
        let mut flows = [MaybeUninit::<u64>::uninit(); 3];
        let mut refusal = MaybeUninit::<Refusal>::uninit();

        // SAFETY: the solver is live, the problem points into the depot's tables, and the flows and the refusal are
        // storage of the test's, used by nothing else meanwhile.
        let status = unsafe {
            kahlnws_solver_solve(
                solver,
                &raw const problem,
                flows.as_mut_ptr().cast(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                refusal.as_mut_ptr(),
            )
        };
        // SAFETY: the solver is live and used by nothing else from here on.
        unsafe { kahlnws_solver_free(solver) };

        assert_eq!(status, StatusCode::REFUSED);
        // SAFETY: a refused solve wrote the refusal.
        let refusal = unsafe { written(&refusal) };
        assert_eq!(
            refusal,
            Refusal { kind: RefusalKind::UNBALANCED, supply_total: Int128::from(1), ..Refusal::default() }
        );
    }

    #[test]
    fn test_a_network_solve_and_reads_write_into_storage_that_held_no_values() {
        let problem = depot();
        let network = kahlnws_network_new();
        assert!(!network.is_null(), "the memory for a network should be granted");
        let (mut total_cost, mut work) = (MaybeUninit::<Int128>::uninit(), MaybeUninit::<Work>::uninit());
        let (mut flows, mut potentials) = ([MaybeUninit::<u64>::uninit(); 3], [MaybeUninit::<i64>::uninit(); 3]);
        let (mut flow, mut cost) = (MaybeUninit::<u64>::uninit(), MaybeUninit::<Int128>::uninit());

        // SAFETY: the network is live, the problem points into the depot's tables, and every output is the test's own
        // storage, with room for the depot's counts.
        let statuses = unsafe {
            [
                kahlnws_network_load(network, &raw const problem, ptr::null_mut()),
                kahlnws_network_solve(network, total_cost.as_mut_ptr(), work.as_mut_ptr()),
                kahlnws_network_total_cost(network, cost.as_mut_ptr()),
                kahlnws_network_flow(network, 0, flow.as_mut_ptr()),
                kahlnws_network_flows(network, flows.as_mut_ptr().cast()),
                kahlnws_network_potentials(network, potentials.as_mut_ptr().cast()),
            ]
        };
        // SAFETY: the network is live and used by nothing else from here on.
        unsafe { kahlnws_network_free(network) };

        let ok = StatusCode::OK;
        assert_eq!(statuses, [ok, StatusCode::OPTIMAL, ok, ok, ok, ok]);
        // SAFETY: the optimal solve wrote the cost and the work, and each read that answered `OK` wrote its output.
        let answered = unsafe {
            (
                written(&total_cost),
                written(&work),
                written(&cost),
                written(&flow),
                written_entries(&flows),
                written_entries(&potentials),
            )
        };
        let (fourteen, depot_work) = (Int128::from(14), Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 });
        assert_eq!(answered, (fourteen, depot_work, fourteen, 2, vec![2, 2, 2], vec![0, 1, 5]));
    }
}
