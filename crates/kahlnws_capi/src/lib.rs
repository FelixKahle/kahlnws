// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The C interface of kahlnws: a primal network simplex for the min-cost flow problem on exact integers.
//!
//! The header `include/kahlnws.h` declares it, and the library `libkahlnws_capi`, static or dynamic, provides it. The
//! header is read by a C compiler from C99 on and by a C++ compiler from C++11 on.
//!
//! Use. A program makes a solver with `kahlnws_solver_new` and, where it solves in a loop, reserves its room once
//! with `kahlnws_solver_reserve`. It fills in a `kahlnws_problem` with the two counts and a pointer to each of its
//! tables, and calls `kahlnws_solver_solve` with storage of its own for the flows and, where it wants them, the
//! potentials, the cost, the work and the refusal. It reads the `kahlnws_status` the call answers, which
//! `kahlnws_status_name` names; where the solve ran, optimal or infeasible, it may read the pivots it made from the
//! `kahlnws_work`; on a refusal, it reads the breach from the `kahlnws_refusal`, or its text from
//! `kahlnws_refusal_message`. It frees the solver with `kahlnws_solver_free`.
//!
//! Ownership. The library owns only the solvers it makes, from `kahlnws_solver_new` to `kahlnws_solver_free`. Every
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
//! Answers. Every entry answers, and none aborts or unwinds into the caller. Memory refused for a solver's room is
//! answered as `KAHLNWS_STATUS_NO_ROOM`, and an internal error of the library, a defect of it, as
//! `KAHLNWS_STATUS_INTERNAL_ERROR`.
//!
//! Threads. A solver is used by one thread at a time: no two calls that take the same solver run at once. It may
//! move from one thread to another between calls, and distinct solvers may be used on distinct threads at once, also
//! to solve the same problem tables. The entries that take no solver may be called from any thread at any time.
//!
//! Numbers. Node and arc counts and node numbers are `size_t`; flows, lower bounds, capacities and the counts of a
//! `kahlnws_work` `uint64_t`; costs, supplies and potentials `int64_t`. A 128-bit integer, such as the cost of a
//! flow, comes as its two 64-bit halves, `kahlnws_int128` or `kahlnws_uint128`: the value is `high * 2^64 + low`,
//! with `low` the value modulo `2^64`.
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
//! `kahlnws::error`. Where exceptions are enabled, `solve` and `solve_into` throw instead. The tests in
//! `tests/header.rs` build `tests/network_simplex.cpp` and `tests/int128.cpp` against it in C++11, 14, 17 and 20, and
//! C++23 where the compiler has it, with exceptions and without.

mod boundary;
mod guard;
mod halves;
mod problem;
mod refusal;
mod solver;
mod status;
mod version;
mod work;

use core::ffi::c_char;
use core::ptr;

pub use halves::{Int128, UInt128};
pub use problem::Problem;
pub use refusal::{Refusal, RefusalKind};
pub use solver::Solver;
pub use status::StatusCode;
pub use work::Work;

use guard::{guarded, status_of};
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
    guarded(ptr::null_mut(), boundary::solver_handle)
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
        unsafe { boundary::free_solver(solver) }
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
/// - `KAHLNWS_STATUS_RESERVED`: the room is made;
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
        let solver = unsafe { boundary::solver_mut(solver) }?;

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
        let solver = unsafe { boundary::solver_mut(solver) }?;
        // SAFETY: C guarantees `problem` and the outputs point to what this entry's doc states, null only where it
        // allows, with no output overlapping another argument, nothing writing the inputs and nothing else using the
        // outputs meanwhile.
        let call = unsafe { boundary::call(problem, flows, potentials, total_cost, work, refusal) }?;

        Ok(solver.solve(call))
    })
}

/// Writes the message of the breach `refusal` names to `buffer`, as UTF-8 ending in a NUL.
///
/// The message is the text kahlnws gives the breach, with its values. A buffer of at least the length answered plus
/// one receives it whole; a shorter one receives as much of it as ends on a character and leaves a byte for the NUL;
/// a capacity of zero, or a null `buffer`, receives nothing, so a first call with neither tells the length to make
/// room for. Where `refusal` is null, not aligned for a refusal, or of a kind that names no breach, the message is
/// empty and the answer 0 (a breach's message is never empty); whenever the buffer has a byte, it ends in a NUL.
/// Where `capacity` passes `PTRDIFF_MAX`, nothing is written and the answer is 0. It allocates nothing, and may be
/// called from any thread.
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
/// - `buffer` is null, or `capacity` is zero, or `buffer` points to writable storage for `capacity` bytes within one
///   object, whatever it held before;
/// - the buffer overlaps nothing of the refusal; nothing writes the refusal, and nothing else reads or writes the
///   buffer, while the call runs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kahlnws_refusal_message(
    refusal: *const Refusal,
    buffer: *mut c_char,
    capacity: usize,
) -> usize {
    guarded(0, || {
        // SAFETY: C guarantees `buffer` is null or writable storage for `capacity` bytes that overlaps nothing else
        // of the call and that nothing else uses meanwhile.
        let Ok(buffer) = (unsafe { boundary::bytes_mut(buffer, capacity) }) else {
            return 0;
        };
        // SAFETY: C guarantees `refusal` is null or points to an initialised refusal nothing writes meanwhile.
        let refusal = unsafe { boundary::refusal_ref(refusal) }.copied().unwrap_or_default();

        refusal.write_message(buffer)
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
