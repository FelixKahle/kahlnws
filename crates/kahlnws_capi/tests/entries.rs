// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Every entry of the C interface, called as C calls it, by hand: the depot solved, its work counted, also into
//! storage that held no values; an infeasible cut and its work; each breach refused with its values and its message,
//! cut short and not written at all; a null pointer and a misaligned one in every position with every other argument
//! valid, and counts past `isize::MAX` bytes; the outputs as they were on every status but optimal, the work written
//! on infeasible too; a reservation; a solver reused; the cost bound, the status names and the version.
//!
//! `kahlnws_solver_resolve` takes the arguments `kahlnws_solver_solve` takes, and every test of pointers and storage
//! calls both. A resolve of a solver that never solved answers each problem as a solve does; one of the problem solved
//! last makes no pivot, one of fewer units moves them along the kept tree without a pivot, one of other counts
//! answers as a solve, and a refused call between a solve and a resolve keeps the basis.
//!
//! Two answers of the entries that solve are not reached here. `KAHLNWS_STATUS_NO_ROOM` needs counts whose room the
//! memory refuses, and a refusal as too large needs `2^32` nodes or arcs; both need tables of that many entries, more
//! than a test holds, since the tables are lent for the counts before the room is made. The refusal as too large is
//! checked through `kahlnws_refusal_message` from a refusal made by hand, and the room's refusal through
//! `kahlnws_solver_reserve`, which both entries make their room by.

use core::ffi::{CStr, c_char};
use core::mem::MaybeUninit;
use core::ptr;

use kahlnws_capi::{
    Int128, Problem, Refusal, RefusalKind, Solver, StatusCode, UInt128, Work, kahlnws_cost_bound,
    kahlnws_refusal_message, kahlnws_solver_free, kahlnws_solver_new, kahlnws_solver_reserve, kahlnws_solver_resolve,
    kahlnws_solver_solve, kahlnws_status_name, kahlnws_version,
};

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

/// Both entries that solve, each with its name.
const SOLVE_ENTRIES: [(&str, SolveEntry); 2] =
    [("kahlnws_solver_solve", kahlnws_solver_solve), ("kahlnws_solver_resolve", kahlnws_solver_resolve)];

/// The tables of a problem, owned by the test; [`Tables::problem`] points a `kahlnws_problem` at them.
#[derive(Clone, Debug, Default)]
struct Tables {
    /// The node each arc leaves.
    tails: Vec<usize>,
    /// The node each arc enters.
    heads: Vec<usize>,
    /// The least flow of each arc; empty where the problem gives none.
    lower_bounds: Vec<u64>,
    /// The most flow of each arc.
    capacities: Vec<u64>,
    /// The cost of a unit of flow on each arc.
    costs: Vec<i64>,
    /// The supply of each node.
    supplies: Vec<i64>,
}

impl Tables {
    /// The problem of these tables as C gives it: its lower bounds null where there are none.
    fn problem(&self) -> Problem {
        Problem {
            node_count: self.supplies.len(),
            arc_count: self.tails.len(),
            tails: self.tails.as_ptr(),
            heads: self.heads.as_ptr(),
            lower_bounds: if self.lower_bounds.is_empty() { ptr::null() } else { self.lower_bounds.as_ptr() },
            capacities: self.capacities.as_ptr(),
            costs: self.costs.as_ptr(),
            supplies: self.supplies.as_ptr(),
        }
    }
}

/// The depot: node 0 sends 4 units to node 2, directly by arc 0 (capacity 3, cost 5) or through node 1 by arcs 1 and
/// 2 (capacity 4 and 2, cost 1 each). Two units go each way, at a cost of 14.
fn depot() -> Tables {
    Tables {
        tails: vec![0, 0, 1],
        heads: vec![2, 1, 2],
        lower_bounds: vec![],
        capacities: vec![3, 4, 2],
        costs: vec![5, 1, 1],
        supplies: vec![4, 0, -4],
    }
}

/// The outputs of one solve, owned by the test, each starting at a marker so that one left as it was is seen.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Outputs {
    /// The flow of each arc.
    flows: Vec<u64>,
    /// The potential of each node.
    potentials: Vec<i64>,
    /// The cost.
    total_cost: Int128,
    /// The work.
    work: Work,
    /// The refusal.
    refusal: Refusal,
}

impl Outputs {
    /// The outputs for `tables` before a solve: every entry and every count at 7, the refusal naming no breach.
    fn marked(tables: &Tables) -> Self {
        Self {
            flows: vec![7; tables.tails.len()],
            potentials: vec![7; tables.supplies.len()],
            total_cost: Int128 { low: 7, high: 7 },
            work: Work { pivots: 7, degenerate_pivots: 7, first_arc_pivots: 7 },
            refusal: Refusal { arc: 7, ..Refusal::default() },
        }
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

/// Solves `tables` with `solver` by `entry` into `outputs`, every output given, and answers the status.
fn solve(entry: SolveEntry, solver: *mut Solver, tables: &Tables, outputs: &mut Outputs) -> StatusCode {
    let problem = tables.problem();

    // SAFETY: `solver` is the test's live solver; the problem points into `tables` and every output into `outputs`,
    // each for the counts the problem states, and nothing else uses them during the call.
    unsafe {
        entry(
            solver,
            &raw const problem,
            outputs.flows.as_mut_ptr(),
            outputs.potentials.as_mut_ptr(),
            &raw mut outputs.total_cost,
            &raw mut outputs.work,
            &raw mut outputs.refusal,
        )
    }
}

/// What a fresh solver answers and writes for `tables` by `entry`.
fn solved(entry: SolveEntry, tables: &Tables) -> (StatusCode, Outputs) {
    let solver = new_solver();
    let mut outputs = Outputs::marked(tables);
    let status = solve(entry, solver, tables, &mut outputs);
    // SAFETY: `solver` is the live solver made above, used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };

    (status, outputs)
}

/// The message `kahlnws_refusal_message` writes for `refusal` into a buffer of `capacity` bytes, every byte starting
/// at `b'#'`, and the length it answers.
fn message(refusal: &Refusal, capacity: usize) -> (Vec<u8>, usize) {
    let mut buffer = vec![b'#'; capacity];

    // SAFETY: the refusal is the test's own, and the buffer holds `capacity` bytes that nothing else uses meanwhile.
    let length = unsafe { kahlnws_refusal_message(refusal, buffer.as_mut_ptr().cast::<c_char>(), capacity) };

    (buffer, length)
}

/// The message the refusal of a refused solve of `tables` names, written whole, as text.
///
/// # Panics
/// Panics if `tables` is not refused, or the message is not whole, NUL-terminated UTF-8.
fn refused_with(tables: &Tables) -> (Refusal, String) {
    let (status, outputs) = solved(kahlnws_solver_solve, tables);
    assert_eq!(status, StatusCode::REFUSED, "the problem should be refused");
    assert_eq!(
        Outputs { refusal: Refusal { arc: 7, ..Refusal::default() }, ..outputs.clone() },
        Outputs::marked(tables)
    );

    let (buffer, length) = message(&outputs.refusal, 256);
    assert_eq!(buffer.get(length), Some(&0), "the message should end in a NUL right after its length");
    let text = String::from_utf8(buffer[..length].to_vec()).expect("the message should be UTF-8");

    (outputs.refusal, text)
}

/// The work of the depot: four pivots, one of them degenerate, none of a first arc. No arc into node 2 is wide enough
/// for its 4 units, so no first arc is chosen; the search enters arc 0, which fills to its capacity 3; arc 2, which
/// sends nothing; then arcs 1 and 0, which move one unit each.
const DEPOT_WORK: Work = Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 };

/// The depot with every arc of capacity 1: no flow carries the 4 units.
fn narrow_depot() -> Tables {
    Tables { capacities: vec![1, 1, 1], ..depot() }
}

/// The work of the narrow depot: three pivots, one of them degenerate, none of a first arc. Arc 0 fills to its
/// capacity 1; arc 2 enters sending nothing, node 1's empty root link leaving; arc 1 moves one unit, arc 2 leaving at
/// its capacity; then nothing violates while root links still carry flow.
const NARROW_DEPOT_WORK: Work = Work { pivots: 3, degenerate_pivots: 1, first_arc_pivots: 0 };

/// The chain the refusals are made from: arc 0 from 0 to 1, arc 1 from 1 to 2, arc 2 from 2 to 3, each of capacity 5
/// and cost 1; node 0 supplies 2 to node 3.
fn chain() -> Tables {
    Tables {
        tails: vec![0, 1, 2],
        heads: vec![1, 2, 3],
        lower_bounds: vec![],
        capacities: vec![5, 5, 5],
        costs: vec![1, 1, 1],
        supplies: vec![2, 0, 0, -2],
    }
}

#[test]
fn test_the_depot_is_solved_with_its_flows_potentials_and_cost() {
    let (status, outputs) = solved(kahlnws_solver_solve, &depot());

    assert_eq!(status, StatusCode::OPTIMAL);
    assert_eq!(
        outputs,
        Outputs {
            flows: vec![2, 2, 2],
            potentials: vec![0, 1, 5],
            total_cost: Int128 { low: 14, high: 0 },
            work: DEPOT_WORK,
            ..Outputs::marked(&depot())
        }
    );
}

#[test]
fn test_lower_bounds_given_as_zeros_answer_as_none_given() {
    let zeros = Tables { lower_bounds: vec![0; 3], ..depot() };

    assert_eq!(solved(kahlnws_solver_solve, &zeros), solved(kahlnws_solver_solve, &depot()));
}

#[test]
fn test_lower_bounds_are_carried() {
    // Arc 0 must carry 3, its capacity: one unit is left to go through the hub, at a cost of `3 · 5 + 1 + 1 = 17`.
    let bounded = Tables { lower_bounds: vec![3, 0, 0], ..depot() };
    let (status, outputs) = solved(kahlnws_solver_solve, &bounded);

    assert_eq!(status, StatusCode::OPTIMAL);
    assert_eq!((outputs.flows, outputs.total_cost), (vec![3, 1, 1], Int128 { low: 17, high: 0 }));
}

#[test]
fn test_a_negative_cost_comes_back_in_two_halves() {
    // One unit over an arc of cost −5.
    let tables = Tables {
        tails: vec![0],
        heads: vec![1],
        lower_bounds: vec![],
        capacities: vec![1],
        costs: vec![-5],
        supplies: vec![1, -1],
    };
    let (status, outputs) = solved(kahlnws_solver_solve, &tables);

    assert_eq!(status, StatusCode::OPTIMAL);
    assert_eq!(outputs.total_cost, Int128 { low: u64::MAX - 4, high: -1 });
    assert_eq!(i128::from(outputs.total_cost), -5);
}

#[test]
fn test_an_infeasible_cut_writes_the_work_alone() {
    // Node 0 must send 2 units to node 1, but the one arc from 0 to 1 carries 1; arc 1 runs the other way. Arc 0
    // enters and fills, with no link leaving; then nothing violates.
    let tables = Tables {
        tails: vec![0, 1],
        heads: vec![1, 0],
        lower_bounds: vec![],
        capacities: vec![1, 5],
        costs: vec![1, 1],
        supplies: vec![2, -2],
    };
    let work = Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 0 };

    assert_eq!(
        solved(kahlnws_solver_solve, &tables),
        (StatusCode::INFEASIBLE, Outputs { work, ..Outputs::marked(&tables) })
    );
    assert_eq!(
        solved(kahlnws_solver_solve, &narrow_depot()).1,
        Outputs { work: NARROW_DEPOT_WORK, ..Outputs::marked(&narrow_depot()) }
    );
}

#[test]
fn test_a_problem_of_no_node_and_no_arc_needs_no_storage() {
    for (name, entry) in SOLVE_ENTRIES {
        let problem = Problem {
            node_count: 0,
            arc_count: 0,
            tails: ptr::null(),
            heads: ptr::null(),
            lower_bounds: ptr::null(),
            capacities: ptr::null(),
            costs: ptr::null(),
            supplies: ptr::null(),
        };
        let solver = new_solver();
        let mut total_cost = Int128 { low: 7, high: 7 };
        let mut work = Work { pivots: 7, degenerate_pivots: 7, first_arc_pivots: 7 };

        // SAFETY: the solver is live; every count is zero, so no table and no flow is read, and the cost and the work
        // are the test's.
        let status = unsafe {
            entry(
                solver,
                &raw const problem,
                ptr::null_mut(),
                ptr::null_mut(),
                &raw mut total_cost,
                &raw mut work,
                ptr::null_mut(),
            )
        };
        // SAFETY: the solver is live and used by nothing else from here on.
        unsafe { kahlnws_solver_free(solver) };

        let expected = (StatusCode::OPTIMAL, Int128::default(), Work::default());
        assert_eq!((status, total_cost, work), expected, "{name}: no pivot");
    }
}

#[test]
fn test_an_end_outside_the_nodes_is_refused_with_its_values_and_message() {
    let (refusal, text) = refused_with(&Tables { tails: vec![0, 1, 4], ..chain() });
    let expected =
        Refusal { kind: RefusalKind::TAIL_OUTSIDE_NODES, arc: 2, node: 4, node_count: 4, ..Refusal::default() };
    assert_eq!((refusal, text.as_str()), (expected, "arc 2 leaves node 4, not below the node count 4"));

    let (refusal, text) = refused_with(&Tables { heads: vec![9, 2, 3], ..chain() });
    let expected =
        Refusal { kind: RefusalKind::HEAD_OUTSIDE_NODES, arc: 0, node: 9, node_count: 4, ..Refusal::default() };
    assert_eq!((refusal, text.as_str()), (expected, "arc 0 enters node 9, not below the node count 4"));
}

#[test]
fn test_a_lower_bound_above_its_capacity_is_refused_with_its_values_and_message() {
    let (refusal, text) = refused_with(&Tables { lower_bounds: vec![0, 6, 0], ..chain() });
    let expected =
        Refusal { kind: RefusalKind::LOWER_ABOVE_CAPACITY, arc: 1, lower_bound: 6, capacity: 5, ..Refusal::default() };

    assert_eq!((refusal, text.as_str()), (expected, "arc 1 has lower bound 6 above its capacity 5"));
}

#[test]
fn test_a_cost_beyond_the_bound_is_refused_with_its_values_and_message() {
    let bound = kahlnws_cost_bound(4);
    let (refusal, text) = refused_with(&Tables { costs: vec![bound, -bound - 1, 0], ..chain() });
    let expected =
        Refusal { kind: RefusalKind::COST_BEYOND_BOUND, arc: 1, cost: -bound - 1, node_count: 4, ..Refusal::default() };

    assert_eq!(refusal, expected);
    assert_eq!(text, format!("arc 1 has cost {} beyond the bound ±{bound} for the node count 4", -bound - 1));
}

#[test]
fn test_a_cost_total_beyond_the_range_is_refused_at_its_arc_with_its_message() {
    // On one node the bound is `2^62 − 1`; arcs at it and of capacity `2^64 − 1` pass `2^127 − 1` at the third.
    let bound = kahlnws_cost_bound(1);
    let tables = Tables {
        tails: vec![0; 3],
        heads: vec![0; 3],
        lower_bounds: vec![],
        capacities: vec![u64::MAX; 3],
        costs: vec![bound; 3],
        supplies: vec![0],
    };
    let (refusal, text) = refused_with(&tables);

    assert_eq!(refusal, Refusal { kind: RefusalKind::COST_TOTAL_BEYOND_RANGE, arc: 2, ..Refusal::default() });
    assert_eq!(text, format!("the costs times the capacities, in magnitude, sum past {} at arc 2", i128::MAX));
}

#[test]
fn test_supplies_that_do_not_balance_are_refused_with_their_total_and_message() {
    let (refusal, text) = refused_with(&Tables { supplies: vec![i64::MAX, i64::MAX], ..Tables::default() });
    let total = 2 * i128::from(i64::MAX);
    let expected = Refusal { kind: RefusalKind::UNBALANCED, supply_total: Int128::from(total), ..Refusal::default() };

    assert_eq!(refusal, expected);
    assert_eq!(refusal.supply_total, Int128 { low: u64::MAX - 1, high: 0 });
    assert_eq!(text, format!("the supplies add up to {total}, not zero"));

    let (refusal, text) = refused_with(&Tables { supplies: vec![i64::MIN, i64::MIN], ..Tables::default() });
    assert_eq!(refusal.supply_total, Int128 { low: 0, high: -1 }, "−2^64");
    assert_eq!(text, "the supplies add up to -18446744073709551616, not zero");
}

#[test]
fn test_net_supplies_beyond_their_range_are_refused_with_their_total_and_message() {
    // Node 0 supplies `2^63 − 1`, and an arc into it of lower bound `2^63` adds that much.
    let tables = Tables {
        tails: vec![1],
        heads: vec![0],
        lower_bounds: vec![1 << 63],
        capacities: vec![1 << 63],
        costs: vec![0],
        supplies: vec![i64::MAX, i64::MIN + 1],
    };
    let (refusal, text) = refused_with(&tables);
    let expected = Refusal {
        kind: RefusalKind::NET_SUPPLY_BEYOND_RANGE,
        net_supply_total: UInt128 { low: u64::MAX, high: 0 },
        ..Refusal::default()
    };

    assert_eq!(refusal, expected);
    assert_eq!(
        text,
        "the positive supplies net of the lower bounds add up to 18446744073709551615, past 18446744073709551614"
    );
}

#[test]
fn test_a_problem_too_large_has_its_message() {
    // A problem past the size a solve counts needs `2^32` supplies or tails, which no test holds: the refusal is made
    // by hand, as `kahlnws_solver_solve` would write it.
    let refusal =
        Refusal { kind: RefusalKind::TOO_LARGE, node_count: 4_294_967_295, arc_count: 7, ..Refusal::default() };
    let expected = "the node count 4294967295 and the arc count 7 are too large: nodes + 1 and arcs + nodes are each \
                    limited to 4294967295";
    let (buffer, length) = message(&refusal, 200);

    assert_eq!((&buffer[..length], buffer[length]), (expected.as_bytes(), 0));
}

#[test]
fn test_a_message_too_long_for_its_buffer_is_cut_at_a_character_and_answers_its_whole_length() {
    // "arc 1 has cost … beyond the bound ±…": the `±` is two bytes, at bytes 39 and 40 of the message.
    let bound = kahlnws_cost_bound(4);
    let (refusal, text) = refused_with(&Tables { costs: vec![bound, -bound - 1, 0], ..chain() });
    let sign = text.find('±').expect("the message should hold a ±");

    let (buffer, length) = message(&refusal, sign + 2);
    assert_eq!(length, text.len(), "the whole length, whatever the capacity");
    assert_eq!(&buffer[..sign], &text.as_bytes()[..sign], "cut before the ±, which does not fit with the NUL");
    assert_eq!(&buffer[sign..], &[0, b'#'], "nothing past the NUL");

    let (buffer, _) = message(&refusal, sign + 3);
    assert_eq!(&buffer[..sign + 2], &text.as_bytes()[..sign + 2], "the ± whole");
    assert_eq!(buffer[sign + 2], 0);
}

#[test]
fn test_a_message_with_no_room_writes_nothing_and_answers_its_length() {
    let refusal = Refusal { kind: RefusalKind::COST_TOTAL_BEYOND_RANGE, arc: 2, ..Refusal::default() };
    let whole = format!("the costs times the capacities, in magnitude, sum past {} at arc 2", i128::MAX).len();

    let (buffer, length) = message(&refusal, 0);
    assert_eq!((buffer.len(), length), (0, whole), "a capacity of zero");

    let mut untouched = [b'#'; 4];
    // SAFETY: the refusal is the test's own; a capacity of zero reads no buffer.
    let length = unsafe { kahlnws_refusal_message(&raw const refusal, untouched.as_mut_ptr().cast(), 0) };
    assert_eq!((untouched, length), ([b'#'; 4], whole), "a buffer given with a capacity of zero");

    // SAFETY: the refusal is the test's own; a null buffer is never written.
    let length = unsafe { kahlnws_refusal_message(&raw const refusal, ptr::null_mut(), 64) };
    assert_eq!(length, whole, "a null buffer");

    let (buffer, length) = message(&refusal, 1);
    assert_eq!((buffer, length), (vec![0], whole), "room for the NUL alone");
}

#[test]
fn test_a_message_of_no_breach_or_of_null_is_empty_and_only_terminated() {
    let terminated: Vec<u8> = [0].into_iter().chain([b'#'; 7]).collect();

    for kind in [0, 9, u32::MAX] {
        // A kind no constant has, as C may write it into the field.
        let refusal = Refusal { kind: RefusalKind(kind), arc: 3, ..Refusal::default() };

        assert_eq!(message(&refusal, 8), (terminated.clone(), 0), "kind {kind}");
    }

    let mut buffer = [b'#'; 8];
    // SAFETY: a null refusal is never read; the buffer holds eight bytes nothing else uses.
    let length = unsafe { kahlnws_refusal_message(ptr::null(), buffer.as_mut_ptr().cast(), 8) };
    assert_eq!((buffer.to_vec(), length), (terminated.clone(), 0), "a null refusal");

    let refusals = [Refusal::default(); 2];
    let misaligned = refusals.as_ptr().cast::<u8>().wrapping_add(1).cast::<Refusal>();
    let mut buffer = [b'#'; 8];
    // SAFETY: a misaligned refusal is checked and never read; the buffer holds eight bytes nothing else uses.
    let length = unsafe { kahlnws_refusal_message(misaligned, buffer.as_mut_ptr().cast(), 8) };
    assert_eq!((buffer.to_vec(), length), (terminated, 0), "a misaligned refusal");
}

#[test]
fn test_a_message_with_a_capacity_past_ptrdiff_max_writes_nothing_and_answers_zero() {
    let refusal = Refusal { kind: RefusalKind::COST_TOTAL_BEYOND_RANGE, arc: 2, ..Refusal::default() };
    let mut buffer = [b'#'; 8];

    // SAFETY: the refusal is the test's own; a capacity past `PTRDIFF_MAX` is checked and the buffer never written.
    let length = unsafe { kahlnws_refusal_message(&raw const refusal, buffer.as_mut_ptr().cast(), usize::MAX) };
    assert_eq!((buffer, length), ([b'#'; 8], 0));
}

#[test]
fn test_a_message_is_written_into_storage_that_held_no_values() {
    let refusal =
        Refusal { kind: RefusalKind::TAIL_OUTSIDE_NODES, arc: 2, node: 9, node_count: 4, ..Refusal::default() };
    let mut buffer = [MaybeUninit::<c_char>::uninit(); 64];

    // SAFETY: the refusal is the test's own, and the buffer is storage for 64 bytes nothing else uses.
    let length = unsafe { kahlnws_refusal_message(&raw const refusal, buffer.as_mut_ptr().cast(), 64) };
    // SAFETY: the call wrote the message and its NUL, `length + 1` bytes from the start.
    let written = unsafe { CStr::from_ptr(buffer.as_ptr().cast::<c_char>()) };
    assert_eq!(written.to_str(), Ok("arc 2 leaves node 9, not below the node count 4"));
    assert_eq!(length, written.count_bytes());
}

#[test]
fn test_a_null_pointer_in_every_needed_position_writes_nothing() {
    for (name, entry) in SOLVE_ENTRIES {
        let tables = depot();
        let problem = tables.problem();
        let refused = [
            Problem { tails: ptr::null(), ..problem },
            Problem { heads: ptr::null(), ..problem },
            Problem { capacities: ptr::null(), ..problem },
            Problem { costs: ptr::null(), ..problem },
            Problem { supplies: ptr::null(), ..problem },
        ];
        let solver = new_solver();

        for problem in refused {
            let mut outputs = Outputs::marked(&tables);
            // SAFETY: the solver is live; every table of the problem but the null one points into `tables`, the null
            // one is checked and never read; every output points into `outputs` for the problem's counts.
            let status = unsafe {
                entry(
                    solver,
                    &raw const problem,
                    outputs.flows.as_mut_ptr(),
                    outputs.potentials.as_mut_ptr(),
                    &raw mut outputs.total_cost,
                    &raw mut outputs.work,
                    &raw mut outputs.refusal,
                )
            };
            assert_eq!((status, outputs), (StatusCode::NULL_POINTER, Outputs::marked(&tables)), "{name}, {problem:?}");
        }

        let mut outputs = Outputs::marked(&tables);
        let (flows, potentials) = (outputs.flows.as_mut_ptr(), outputs.potentials.as_mut_ptr());
        let (total_cost, work, refusal) =
            (&raw mut outputs.total_cost, &raw mut outputs.work, &raw mut outputs.refusal);
        // SAFETY: each call passes the live solver, the depot and the test's outputs, but for the one null pointer,
        // which is checked and never read.
        let statuses = unsafe {
            [
                entry(ptr::null_mut(), &raw const problem, flows, potentials, total_cost, work, refusal),
                entry(solver, ptr::null(), flows, potentials, total_cost, work, refusal),
                entry(solver, &raw const problem, ptr::null_mut(), potentials, total_cost, work, refusal),
            ]
        };
        assert_eq!(statuses, [StatusCode::NULL_POINTER; 3], "{name}");
        assert_eq!(outputs, Outputs::marked(&tables), "{name}");

        // SAFETY: the solver is live and used by nothing else from here on.
        unsafe { kahlnws_solver_free(solver) };
    }
}

#[test]
fn test_outputs_passed_as_null_are_not_wanted() {
    for (name, entry) in SOLVE_ENTRIES {
        let tables = depot();
        let problem = tables.problem();
        let solver = new_solver();
        let mut flows = [7; 3];

        // SAFETY: the solver is live, the problem points into `tables` and the flows into `flows`; the other outputs
        // are null, which asks for none of them.
        let status = unsafe {
            entry(
                solver,
                &raw const problem,
                flows.as_mut_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        assert_eq!((status, flows), (StatusCode::OPTIMAL, [2, 2, 2]), "{name}");

        let unbalanced = Tables { supplies: vec![4, 0, -3], ..depot() };
        let problem = unbalanced.problem();
        // SAFETY: the solver is live, the problem points into `unbalanced` and the flows into `flows`; the other
        // outputs are null, which asks for none of them.
        let status = unsafe {
            entry(
                solver,
                &raw const problem,
                flows.as_mut_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        assert_eq!((status, flows), (StatusCode::REFUSED, [2, 2, 2]), "{name}: a refusal with nowhere to go");

        // SAFETY: the solver is live and used by nothing else from here on.
        unsafe { kahlnws_solver_free(solver) };
    }
}

#[test]
fn test_a_reservation_answers_reserved_no_room_or_a_null_pointer() {
    let solver = new_solver();

    // SAFETY: the solver is live and used by nothing else meanwhile.
    let statuses = unsafe {
        [
            kahlnws_solver_reserve(solver, 3, 3),
            kahlnws_solver_reserve(solver, usize::MAX, 1),
            kahlnws_solver_reserve(solver, 1, usize::MAX),
            kahlnws_solver_reserve(ptr::null_mut(), 3, 3),
        ]
    };
    assert_eq!(statuses, [StatusCode::RESERVED, StatusCode::NO_ROOM, StatusCode::NO_ROOM, StatusCode::NULL_POINTER]);

    let mut outputs = Outputs::marked(&depot());
    assert_eq!(
        solve(kahlnws_solver_solve, solver, &depot(), &mut outputs),
        StatusCode::OPTIMAL,
        "a refused reservation leaves it usable"
    );
    assert_eq!(outputs, solved(kahlnws_solver_solve, &depot()).1);

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

#[test]
fn test_one_solver_answers_each_problem_as_a_fresh_one() {
    let problems = [
        depot(),
        Tables { supplies: vec![4, 0, -3], ..depot() },
        narrow_depot(),
        chain(),
        Tables { lower_bounds: vec![3, 0, 0], ..depot() },
        depot(),
    ];
    let solver = new_solver();

    for tables in &problems {
        let mut outputs = Outputs::marked(tables);
        let status = solve(kahlnws_solver_solve, solver, tables, &mut outputs);

        assert_eq!((status, outputs), solved(kahlnws_solver_solve, tables), "{tables:?}");
    }

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

#[test]
fn test_a_solve_after_a_resolve_answers_as_a_fresh_one() {
    let problems = [depot(), Tables { supplies: vec![3, 0, -3], ..depot() }, narrow_depot(), chain(), depot()];
    let solver = new_solver();

    for tables in &problems {
        let mut outputs = Outputs::marked(tables);
        assert_ne!(solve(kahlnws_solver_resolve, solver, tables, &mut outputs), StatusCode::INTERNAL_ERROR);

        let mut outputs = Outputs::marked(tables);
        let status = solve(kahlnws_solver_solve, solver, tables, &mut outputs);
        assert_eq!((status, outputs), solved(kahlnws_solver_solve, tables), "{tables:?}");
    }

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

#[test]
fn test_a_resolve_of_a_solver_that_never_solved_answers_what_a_solve_answers() {
    let problems = [
        depot(),
        Tables { lower_bounds: vec![3, 0, 0], ..depot() },
        narrow_depot(),
        Tables { supplies: vec![4, 0, -3], ..depot() },
        Tables { tails: vec![0, 1, 4], ..chain() },
        Tables { lower_bounds: vec![0, 6, 0], ..chain() },
    ];

    for tables in &problems {
        assert_eq!(solved(kahlnws_solver_resolve, tables), solved(kahlnws_solver_solve, tables), "{tables:?}");
    }
    assert_eq!(solved(kahlnws_solver_resolve, &depot()).1.work, DEPOT_WORK, "the work of a solve from the star");
}

/// What `solver` answers and writes for `tables` by `kahlnws_solver_resolve`, every output given.
fn resolved(solver: *mut Solver, tables: &Tables) -> (StatusCode, Outputs) {
    let mut outputs = Outputs::marked(tables);
    let status = solve(kahlnws_solver_resolve, solver, tables, &mut outputs);

    (status, outputs)
}

#[test]
fn test_a_resolve_of_the_problem_solved_last_makes_no_pivot_and_answers_its_optimum() {
    let solver = new_solver();
    let (status, solved_outputs) = solved(kahlnws_solver_solve, &depot());
    let mut outputs = Outputs::marked(&depot());
    assert_eq!(solve(kahlnws_solver_solve, solver, &depot(), &mut outputs), status);

    // The basis the solve ended on is the optimum of the same numbers: nothing violates, and nothing moves.
    let expected = Outputs { work: Work::default(), ..solved_outputs };
    assert_eq!(resolved(solver, &depot()), (StatusCode::OPTIMAL, expected));

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

#[test]
fn test_a_resolve_of_fewer_units_sends_them_along_the_kept_tree_without_a_pivot() {
    // The depot ends with arcs 0 and 1 in the tree, below node 0, and arc 2 full out of it, at potentials 0, 1 and 5.
    // With 3 units, arc 2 still carries its 2, arc 0 the 1 node 2 lacks and arc 1 the 2 that node 1 passes on: the
    // flows 1, 2, 2 at a cost of `5 + 2 + 2 = 9`, every arc as it stood, so nothing violates.
    let fewer = Tables { supplies: vec![3, 0, -3], ..depot() };
    let solver = new_solver();
    let mut outputs = Outputs::marked(&depot());
    assert_eq!(solve(kahlnws_solver_solve, solver, &depot(), &mut outputs), StatusCode::OPTIMAL);

    let expected = Outputs {
        flows: vec![1, 2, 2],
        potentials: vec![0, 1, 5],
        total_cost: Int128 { low: 9, high: 0 },
        work: Work::default(),
        ..Outputs::marked(&fewer)
    };
    assert_eq!(resolved(solver, &fewer), (StatusCode::OPTIMAL, expected));
    let (status, fresh) = solved(kahlnws_solver_solve, &fewer);
    assert_eq!((status, fresh.total_cost), (StatusCode::OPTIMAL, Int128 { low: 9, high: 0 }), "the same cost");
    assert_ne!(fresh.work, Work::default(), "a solve of the same problem makes pivots");

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

#[test]
fn test_a_resolve_of_other_counts_answers_what_a_solve_answers() {
    // The chain has four nodes where the depot has three: the depot's basis cannot serve it.
    let solver = new_solver();
    let mut outputs = Outputs::marked(&depot());
    assert_eq!(solve(kahlnws_solver_solve, solver, &depot(), &mut outputs), StatusCode::OPTIMAL);

    assert_eq!(resolved(solver, &chain()), solved(kahlnws_solver_solve, &chain()));

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

#[test]
fn test_a_call_refused_between_a_solve_and_a_resolve_keeps_the_basis() {
    let solver = new_solver();
    let mut outputs = Outputs::marked(&depot());
    assert_eq!(solve(kahlnws_solver_solve, solver, &depot(), &mut outputs), StatusCode::OPTIMAL);

    let unbalanced = Tables { supplies: vec![4, 0, -3], ..depot() };
    assert_eq!(resolved(solver, &unbalanced).0, StatusCode::REFUSED);
    let tables = depot();
    let problem = tables.problem();
    // SAFETY: the solver is live and the problem points into `tables`; the null flows are checked and never written.
    let status = unsafe {
        kahlnws_solver_resolve(
            solver,
            &raw const problem,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    assert_eq!(status, StatusCode::NULL_POINTER);
    // SAFETY: the solver is live and used by nothing else meanwhile.
    assert_eq!(unsafe { kahlnws_solver_reserve(solver, usize::MAX, 1) }, StatusCode::NO_ROOM);

    let (status, outputs) = resolved(solver, &depot());
    assert_eq!((status, outputs.work), (StatusCode::OPTIMAL, Work::default()), "the kept basis makes no pivot");

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

#[test]
fn test_freeing_null_frees_nothing() {
    // SAFETY: null is accepted and frees nothing.
    unsafe { kahlnws_solver_free(ptr::null_mut()) };
}

#[test]
fn test_the_cost_bound_by_hand() {
    assert_eq!(kahlnws_cost_bound(0), (1 << 62) - 1, "no node");
    assert_eq!(kahlnws_cost_bound(1), (1 << 62) - 1);
    assert_eq!(kahlnws_cost_bound(4), 1_152_921_504_606_846_975);
    assert_eq!(kahlnws_cost_bound(1000), 4_611_686_018_427_387);
    assert_eq!(kahlnws_cost_bound(usize::MAX), 0, "more nodes than any problem has");
}

#[test]
fn test_the_depot_is_solved_into_storage_that_held_no_values() {
    for (name, entry) in SOLVE_ENTRIES {
        let tables = depot();
        let problem = tables.problem();
        let solver = new_solver();
        let mut flows = [MaybeUninit::<u64>::uninit(); 3];
        let mut potentials = [MaybeUninit::<i64>::uninit(); 3];
        let mut total_cost = MaybeUninit::<Int128>::uninit();
        let mut work = MaybeUninit::<Work>::uninit();

        // SAFETY: the solver is live, the problem points into `tables`, and each output is storage of the test's for
        // the counts the problem states, used by nothing else meanwhile.
        let status = unsafe {
            entry(
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

        assert_eq!(status, StatusCode::OPTIMAL, "{name}");
        // SAFETY: an optimal solve wrote every flow, every potential and the cost.
        let written =
            unsafe { (flows.map(|flow| flow.assume_init()), potentials.map(|potential| potential.assume_init())) };
        assert_eq!(written, ([2, 2, 2], [0, 1, 5]));
        // SAFETY: an optimal solve wrote the cost.
        assert_eq!(unsafe { total_cost.assume_init() }, Int128 { low: 14, high: 0 });
        // SAFETY: an optimal solve wrote the work.
        assert_eq!(unsafe { work.assume_init() }, DEPOT_WORK);
    }
}

#[test]
fn test_the_work_of_an_infeasible_solve_is_written_into_storage_that_held_no_value() {
    for (name, entry) in SOLVE_ENTRIES {
        let tables = narrow_depot();
        let problem = tables.problem();
        let solver = new_solver();
        let mut flows = [MaybeUninit::<u64>::uninit(); 3];
        let mut work = MaybeUninit::<Work>::uninit();

        // SAFETY: the solver is live, the problem points into `tables`, and the flows and the work are storage of the
        // test's, used by nothing else meanwhile.
        let status = unsafe {
            entry(
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

        assert_eq!(status, StatusCode::INFEASIBLE, "{name}");
        // SAFETY: an infeasible solve wrote the work.
        assert_eq!(unsafe { work.assume_init() }, NARROW_DEPOT_WORK);
    }
}

#[test]
fn test_a_refusal_is_written_into_storage_that_held_no_value() {
    for (name, entry) in SOLVE_ENTRIES {
        let tables = Tables { supplies: vec![4, 0, -3], ..depot() };
        let problem = tables.problem();
        let solver = new_solver();
        let mut flows = [MaybeUninit::<u64>::uninit(); 3];
        let mut refusal = MaybeUninit::<Refusal>::uninit();

        // SAFETY: the solver is live, the problem points into `tables`, and the flows and the refusal are storage of
        // the test's, used by nothing else meanwhile.
        let status = unsafe {
            entry(
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

        assert_eq!(status, StatusCode::REFUSED, "{name}");
        // SAFETY: a refused solve wrote the refusal.
        let refusal = unsafe { refusal.assume_init() };
        assert_eq!(
            refusal,
            Refusal { kind: RefusalKind::UNBALANCED, supply_total: Int128::from(1), ..Refusal::default() }
        );
    }
}

/// A pointer one byte past `start`: never aligned for a type wider than a byte.
fn misaligned<T>(start: *const T) -> *const T {
    start.cast::<u8>().wrapping_add(1).cast::<T>()
}

#[test]
fn test_a_misaligned_pointer_in_every_position_is_invalid_and_writes_nothing() {
    for (name, entry) in SOLVE_ENTRIES {
        let tables = depot();
        let problem = tables.problem();
        let refused = [
            Problem { tails: misaligned(problem.tails), ..problem },
            Problem { heads: misaligned(problem.heads), ..problem },
            Problem { lower_bounds: misaligned(tables.capacities.as_ptr()), ..problem },
            Problem { capacities: misaligned(problem.capacities), ..problem },
            Problem { costs: misaligned(problem.costs), ..problem },
            Problem { supplies: misaligned(problem.supplies), ..problem },
        ];
        let solver = new_solver();

        for problem in refused {
            let mut outputs = Outputs::marked(&tables);
            // SAFETY: the solver is live; every table of the problem but the misaligned one points into `tables`, the
            // misaligned one is checked and never read; every output points into `outputs` for the problem's counts.
            let status = unsafe {
                entry(
                    solver,
                    &raw const problem,
                    outputs.flows.as_mut_ptr(),
                    outputs.potentials.as_mut_ptr(),
                    &raw mut outputs.total_cost,
                    &raw mut outputs.work,
                    &raw mut outputs.refusal,
                )
            };
            assert_eq!(
                (status, outputs),
                (StatusCode::INVALID_ARGUMENT, Outputs::marked(&tables)),
                "{name}, {problem:?}"
            );
        }

        let mut outputs = Outputs::marked(&tables);
        let mut storage = [Int128::default(); 2];
        let mut work_storage = [Work::default(); 2];
        let (flows, potentials) = (outputs.flows.as_mut_ptr(), outputs.potentials.as_mut_ptr());
        let (total_cost, work, refusal) =
            (&raw mut outputs.total_cost, &raw mut outputs.work, &raw mut outputs.refusal);
        let problems = [problem; 2];
        let misaligned_problem = misaligned(problems.as_ptr());
        let misaligned_cost = misaligned(storage.as_mut_ptr().cast_const()).cast_mut();
        let misaligned_work = misaligned(work_storage.as_mut_ptr().cast_const()).cast_mut();
        let misaligned_refusal = misaligned(refusal.cast_const()).cast_mut();
        // SAFETY: each call passes the live solver, the depot and the test's outputs, but for the one misaligned
        // pointer, which is checked and never read or written.
        let statuses = unsafe {
            [
                entry(solver, misaligned_problem, flows, potentials, total_cost, work, refusal),
                entry(solver, &raw const problem, misaligned(flows).cast_mut(), potentials, total_cost, work, refusal),
                entry(solver, &raw const problem, flows, misaligned(potentials).cast_mut(), total_cost, work, refusal),
                entry(solver, &raw const problem, flows, potentials, misaligned_cost, work, refusal),
                entry(solver, &raw const problem, flows, potentials, total_cost, misaligned_work, refusal),
                entry(solver, &raw const problem, flows, potentials, total_cost, work, misaligned_refusal),
                entry(misaligned(solver).cast_mut(), &raw const problem, flows, potentials, total_cost, work, refusal),
                kahlnws_solver_reserve(misaligned(solver).cast_mut(), 3, 3),
            ]
        };
        assert_eq!(statuses, [StatusCode::INVALID_ARGUMENT; 8], "{name}");
        assert_eq!(outputs, Outputs::marked(&tables));
        assert_eq!((storage, work_storage), ([Int128::default(); 2], [Work::default(); 2]));

        // SAFETY: the solver is live and used by nothing else from here on.
        unsafe { kahlnws_solver_free(solver) };
    }
}

#[test]
fn test_counts_past_ptrdiff_max_bytes_are_invalid_and_write_nothing() {
    for (name, entry) in SOLVE_ENTRIES {
        // The counts are checked before any table is read or the room is made, so tables of three entries serve.
        let tables = depot();
        let solver = new_solver();
        let past = usize::try_from(isize::MAX).expect("isize::MAX should fit a usize") / 8 + 1;

        for problem in
            [Problem { arc_count: past, ..tables.problem() }, Problem { node_count: past, ..tables.problem() }]
        {
            let mut outputs = Outputs::marked(&tables);
            // SAFETY: the solver is live; the count past `PTRDIFF_MAX` bytes is checked before any table it counts is
            // read; the outputs point into `outputs`, and none of them is written.
            let status = unsafe {
                entry(
                    solver,
                    &raw const problem,
                    outputs.flows.as_mut_ptr(),
                    outputs.potentials.as_mut_ptr(),
                    &raw mut outputs.total_cost,
                    &raw mut outputs.work,
                    &raw mut outputs.refusal,
                )
            };
            assert_eq!(
                (status, outputs),
                (StatusCode::INVALID_ARGUMENT, Outputs::marked(&tables)),
                "{name}, {problem:?}"
            );
        }

        // SAFETY: the solver is live and used by nothing else from here on.
        unsafe { kahlnws_solver_free(solver) };
    }
}

/// The name `kahlnws_status_name` answers for `status`, as text.
fn status_name(status: StatusCode) -> &'static str {
    // SAFETY: the entry answers a static NUL-terminated string.
    let name = unsafe { CStr::from_ptr(kahlnws_status_name(status)) };

    name.to_str().expect("a status name should be UTF-8")
}

#[test]
fn test_every_status_has_its_name_and_an_unknown_number_one_too() {
    let names = [
        (StatusCode::OPTIMAL, "optimal"),
        (StatusCode::INFEASIBLE, "infeasible"),
        (StatusCode::RESERVED, "reserved"),
        (StatusCode::REFUSED, "refused"),
        (StatusCode::NULL_POINTER, "null pointer"),
        (StatusCode::INVALID_ARGUMENT, "invalid argument"),
        (StatusCode::NO_ROOM, "no room"),
        (StatusCode::INTERNAL_ERROR, "internal error"),
        (StatusCode(8), "unknown status"),
        (StatusCode(u32::MAX), "unknown status"),
    ];

    for (status, name) in names {
        assert_eq!(status_name(status), name, "{status:?}");
    }
}

#[test]
fn test_the_version_is_the_crate_s_encoded() {
    let version = kahlnws_version();
    let parts = [version >> 16, (version >> 8) & 0xFF, version & 0xFF].map(|part| part.to_string()).join(".");

    assert_eq!(parts, env!("CARGO_PKG_VERSION"));
}
