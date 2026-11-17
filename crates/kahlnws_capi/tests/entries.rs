// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Every entry of the C interface, called as C calls it, by hand: the depot solved, its work counted; an infeasible cut
//! and its work; each breach refused with its values and its message, cut short and not written at all; a null pointer
//! and a misaligned one in every position with every other argument valid, and counts past `isize::MAX` bytes; the
//! outputs as they were on every status but optimal, the work written on infeasible too; a reservation; a solver
//! reused; a 128-bit value in decimal, whole, cut and with no room; the cost bound, the status names and the version.
//! Every output a test reads back starts as a value the entry cannot answer, so an entry that stops writing is seen.
//! Storage that held no values is given to the entries by the tests of the crate's root, which read it back through
//! the boundary's own helper.
//!
//! The network's entries, likewise: a network never loaded and its optimum of no node; the depot loaded and solved,
//! with its work, and solved again with no pivot; each change, and a solve after it, against a fresh solve of the
//! changed problem; each change set back; a change of the number held, which keeps the optimum; an arc switched off
//! that strands flow, and on again; each refusal of a load and of a change with its values and its message, nothing
//! changed; an arc or a node not below its count, and a node that balances itself and is to change; a supply set by
//! hand, set back, and a node that changes sides; every read where the network holds no optimum, writing nothing; a
//! null pointer and a misaligned one in every position, and counts past `isize::MAX` bytes; outputs not wanted; a copy
//! that changes apart from its network, by a new handle and into a network that exists, solving from the copied basis;
//! the reads of the problem held, after a load, after each change and after a refusal, on a network never loaded and on
//! a copy, with no optimum held, an index past its count, and null and misaligned pointers; and a load from the basis
//! held: the same problem again with no pivot, one unit less carried by the basis held, other counts or other ends
//! answered as by a fresh network, and a refused load between two loads that keeps the basis. A refused copy is not
//! reached: it needs a source too large for memory to hold.
//!
//! Two answers of the entries that solve and load are not reached here. `KAHLNWS_STATUS_NO_ROOM` needs counts whose
//! room the memory refuses, and a refusal as too large needs `2^32` nodes or arcs; both need tables of that many
//! entries, more than a test holds, since the tables are lent for the counts before the room is made. The refusal as
//! too large is checked through `kahlnws_refusal_message` from a refusal made by hand, and the room's refusal through
//! `kahlnws_solver_reserve` and `kahlnws_network_reserve`, by which the solve and the load make their room.

use core::ffi::{CStr, c_char};
use core::ptr;

use kahlnws_capi::{
    Int128, Network, Problem, Refusal, RefusalKind, Solver, StatusCode, UInt128, Work, kahlnws_cost_bound,
    kahlnws_int128_format, kahlnws_network_arc_count, kahlnws_network_bounds, kahlnws_network_capacities,
    kahlnws_network_capacity, kahlnws_network_clone, kahlnws_network_copy, kahlnws_network_cost, kahlnws_network_costs,
    kahlnws_network_flow, kahlnws_network_flows, kahlnws_network_free, kahlnws_network_head, kahlnws_network_heads,
    kahlnws_network_load, kahlnws_network_lower_bound, kahlnws_network_lower_bounds, kahlnws_network_move_supply,
    kahlnws_network_new, kahlnws_network_node_count, kahlnws_network_potentials, kahlnws_network_reserve,
    kahlnws_network_set_bounds, kahlnws_network_set_capacity, kahlnws_network_set_cost,
    kahlnws_network_set_lower_bound, kahlnws_network_set_supply, kahlnws_network_solve, kahlnws_network_supplies,
    kahlnws_network_supply, kahlnws_network_tail, kahlnws_network_tails, kahlnws_network_total_cost,
    kahlnws_refusal_message, kahlnws_solver_free, kahlnws_solver_new, kahlnws_solver_reserve, kahlnws_solver_solve,
    kahlnws_status_name, kahlnws_uint128_format, kahlnws_version,
};

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

/// Solves `tables` with `solver` into `outputs`, every output given, and answers the status.
fn solve(solver: *mut Solver, tables: &Tables, outputs: &mut Outputs) -> StatusCode {
    let problem = tables.problem();

    // SAFETY: `solver` is the test's live solver; the problem points into `tables` and every output into `outputs`,
    // each for the counts the problem states, and nothing else uses them during the call.
    unsafe {
        kahlnws_solver_solve(
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

/// What a fresh solver answers and writes for `tables`.
fn solved(tables: &Tables) -> (StatusCode, Outputs) {
    let solver = new_solver();
    let mut outputs = Outputs::marked(tables);
    let status = solve(solver, tables, &mut outputs);
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
    let (status, outputs) = solved(tables);
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
    let (status, outputs) = solved(&depot());

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

    assert_eq!(solved(&zeros), solved(&depot()));
}

#[test]
fn test_lower_bounds_are_carried() {
    // Arc 0 must carry 3, its capacity: one unit is left to go through the hub, at a cost of `3 · 5 + 1 + 1 = 17`.
    let bounded = Tables { lower_bounds: vec![3, 0, 0], ..depot() };
    let (status, outputs) = solved(&bounded);

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
    let (status, outputs) = solved(&tables);

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

    assert_eq!(solved(&tables), (StatusCode::INFEASIBLE, Outputs { work, ..Outputs::marked(&tables) }));
    assert_eq!(solved(&narrow_depot()).1, Outputs { work: NARROW_DEPOT_WORK, ..Outputs::marked(&narrow_depot()) });
}

#[test]
fn test_a_problem_of_no_node_and_no_arc_needs_no_storage() {
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
        kahlnws_solver_solve(
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
    assert_eq!((status, total_cost, work), expected, "no pivot");
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

    for kind in [0, 11, u32::MAX] {
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
fn test_a_message_with_a_capacity_past_ptrdiff_max_writes_nothing_and_answers_zero_or_its_length_where_null() {
    let refusal = Refusal { kind: RefusalKind::COST_TOTAL_BEYOND_RANGE, arc: 2, ..Refusal::default() };
    let whole = format!("the costs times the capacities, in magnitude, sum past {} at arc 2", i128::MAX).len();
    let mut buffer = [b'#'; 8];

    // SAFETY: the refusal is the test's own; a capacity past `PTRDIFF_MAX` is checked and the buffer never written.
    let length = unsafe { kahlnws_refusal_message(&raw const refusal, buffer.as_mut_ptr().cast(), usize::MAX) };
    assert_eq!((buffer, length), ([b'#'; 8], 0));

    // SAFETY: the refusal is the test's own; a null buffer is never written, whatever the capacity.
    let length = unsafe { kahlnws_refusal_message(&raw const refusal, ptr::null_mut(), usize::MAX) };
    assert_eq!(length, whole, "a null buffer tells the length");
}

#[test]
fn test_a_null_pointer_in_every_needed_position_writes_nothing() {
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
            kahlnws_solver_solve(
                solver,
                &raw const problem,
                outputs.flows.as_mut_ptr(),
                outputs.potentials.as_mut_ptr(),
                &raw mut outputs.total_cost,
                &raw mut outputs.work,
                &raw mut outputs.refusal,
            )
        };
        assert_eq!((status, outputs), (StatusCode::NULL_POINTER, Outputs::marked(&tables)), "{problem:?}");
    }

    let mut outputs = Outputs::marked(&tables);
    let (flows, potentials) = (outputs.flows.as_mut_ptr(), outputs.potentials.as_mut_ptr());
    let (total_cost, work, refusal) = (&raw mut outputs.total_cost, &raw mut outputs.work, &raw mut outputs.refusal);
    // SAFETY: each call passes the live solver, the depot and the test's outputs, but for the one null pointer,
    // which is checked and never read.
    let statuses = unsafe {
        [
            kahlnws_solver_solve(ptr::null_mut(), &raw const problem, flows, potentials, total_cost, work, refusal),
            kahlnws_solver_solve(solver, ptr::null(), flows, potentials, total_cost, work, refusal),
            kahlnws_solver_solve(solver, &raw const problem, ptr::null_mut(), potentials, total_cost, work, refusal),
        ]
    };
    assert_eq!(statuses, [StatusCode::NULL_POINTER; 3]);
    assert_eq!(outputs, Outputs::marked(&tables));

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

#[test]
fn test_outputs_passed_as_null_are_not_wanted() {
    let tables = depot();
    let problem = tables.problem();
    let solver = new_solver();
    let mut flows = [7; 3];

    // SAFETY: the solver is live, the problem points into `tables` and the flows into `flows`; the other outputs
    // are null, which asks for none of them.
    let status = unsafe {
        kahlnws_solver_solve(
            solver,
            &raw const problem,
            flows.as_mut_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    assert_eq!((status, flows), (StatusCode::OPTIMAL, [2, 2, 2]));

    let unbalanced = Tables { supplies: vec![4, 0, -3], ..depot() };
    let problem = unbalanced.problem();
    // SAFETY: the solver is live, the problem points into `unbalanced` and the flows into `flows`; the other
    // outputs are null, which asks for none of them.
    let status = unsafe {
        kahlnws_solver_solve(
            solver,
            &raw const problem,
            flows.as_mut_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    assert_eq!((status, flows), (StatusCode::REFUSED, [2, 2, 2]), "a refusal with nowhere to go");

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

#[test]
fn test_a_reservation_answers_ok_no_room_or_a_null_pointer() {
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
    assert_eq!(statuses, [StatusCode::OK, StatusCode::NO_ROOM, StatusCode::NO_ROOM, StatusCode::NULL_POINTER]);

    let mut outputs = Outputs::marked(&depot());
    assert_eq!(solve(solver, &depot(), &mut outputs), StatusCode::OPTIMAL, "a refused reservation leaves it usable");
    assert_eq!(outputs, solved(&depot()).1);

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
        let status = solve(solver, tables, &mut outputs);

        assert_eq!((status, outputs), solved(tables), "{tables:?}");
    }

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

/// A pointer one byte past `start`: never aligned for a type wider than a byte.
fn misaligned<T>(start: *const T) -> *const T {
    start.cast::<u8>().wrapping_add(1).cast::<T>()
}

#[test]
fn test_a_misaligned_pointer_in_every_position_is_invalid_and_writes_nothing() {
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
            kahlnws_solver_solve(
                solver,
                &raw const problem,
                outputs.flows.as_mut_ptr(),
                outputs.potentials.as_mut_ptr(),
                &raw mut outputs.total_cost,
                &raw mut outputs.work,
                &raw mut outputs.refusal,
            )
        };
        assert_eq!((status, outputs), (StatusCode::INVALID_ARGUMENT, Outputs::marked(&tables)), "{problem:?}");
    }

    let mut outputs = Outputs::marked(&tables);
    let mut storage = [Int128::default(); 2];
    let mut work_storage = [Work::default(); 2];
    let (flows, potentials) = (outputs.flows.as_mut_ptr(), outputs.potentials.as_mut_ptr());
    let (total_cost, work, refusal) = (&raw mut outputs.total_cost, &raw mut outputs.work, &raw mut outputs.refusal);
    let problems = [problem; 2];
    let misaligned_problem = misaligned(problems.as_ptr());
    let misaligned_cost = misaligned(storage.as_mut_ptr().cast_const()).cast_mut();
    let misaligned_work = misaligned(work_storage.as_mut_ptr().cast_const()).cast_mut();
    let misaligned_refusal = misaligned(refusal.cast_const()).cast_mut();
    // SAFETY: each call passes the live solver, the depot and the test's outputs, but for the one misaligned
    // pointer, which is checked and never read or written.
    let statuses = unsafe {
        [
            kahlnws_solver_solve(solver, misaligned_problem, flows, potentials, total_cost, work, refusal),
            kahlnws_solver_solve(
                solver,
                &raw const problem,
                misaligned(flows).cast_mut(),
                potentials,
                total_cost,
                work,
                refusal,
            ),
            kahlnws_solver_solve(
                solver,
                &raw const problem,
                flows,
                misaligned(potentials).cast_mut(),
                total_cost,
                work,
                refusal,
            ),
            kahlnws_solver_solve(solver, &raw const problem, flows, potentials, misaligned_cost, work, refusal),
            kahlnws_solver_solve(solver, &raw const problem, flows, potentials, total_cost, misaligned_work, refusal),
            kahlnws_solver_solve(solver, &raw const problem, flows, potentials, total_cost, work, misaligned_refusal),
            kahlnws_solver_solve(
                misaligned(solver).cast_mut(),
                &raw const problem,
                flows,
                potentials,
                total_cost,
                work,
                refusal,
            ),
            kahlnws_solver_reserve(misaligned(solver).cast_mut(), 3, 3),
        ]
    };
    assert_eq!(statuses, [StatusCode::INVALID_ARGUMENT; 8]);
    assert_eq!(outputs, Outputs::marked(&tables));
    assert_eq!((storage, work_storage), ([Int128::default(); 2], [Work::default(); 2]));

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

#[test]
fn test_counts_past_ptrdiff_max_bytes_are_invalid_and_write_nothing() {
    // The counts are checked before any table is read or the room is made, so tables of three entries serve.
    let tables = depot();
    let solver = new_solver();
    let past = usize::try_from(isize::MAX).expect("isize::MAX should fit a usize") / 8 + 1;

    for problem in [Problem { arc_count: past, ..tables.problem() }, Problem { node_count: past, ..tables.problem() }] {
        let mut outputs = Outputs::marked(&tables);
        // SAFETY: the solver is live; the count past `PTRDIFF_MAX` bytes is checked before any table it counts is
        // read; the outputs point into `outputs`, and none of them is written.
        let status = unsafe {
            kahlnws_solver_solve(
                solver,
                &raw const problem,
                outputs.flows.as_mut_ptr(),
                outputs.potentials.as_mut_ptr(),
                &raw mut outputs.total_cost,
                &raw mut outputs.work,
                &raw mut outputs.refusal,
            )
        };
        assert_eq!((status, outputs), (StatusCode::INVALID_ARGUMENT, Outputs::marked(&tables)), "{problem:?}");
    }

    // SAFETY: the solver is live and used by nothing else from here on.
    unsafe { kahlnws_solver_free(solver) };
}

/// The name `kahlnws_status_name` answers for `status`, as text.
fn status_name(status: StatusCode) -> &'static str {
    // SAFETY: the entry answers a static NUL-terminated string.
    let name = unsafe { CStr::from_ptr(kahlnws_status_name(status)) };

    name.to_str().expect("a status name should be UTF-8")
}

/// The bytes `kahlnws_int128_format` writes for `value` into a buffer of `capacity` bytes that start at 7, and the
/// length it answers.
fn int128_text(value: i128, capacity: usize) -> (Vec<u8>, usize) {
    let mut buffer = vec![7_u8; capacity];
    // SAFETY: the buffer is the test's own, with room for `capacity` bytes.
    let length = unsafe { kahlnws_int128_format(Int128::from(value), buffer.as_mut_ptr().cast(), capacity) };

    (buffer, length)
}

#[test]
fn test_a_128_bit_value_is_written_in_decimal_whole_or_cut_with_its_length_by_hand() {
    let least = b"-170141183460469231731687303715884105728\0";
    assert_eq!(int128_text(i128::MIN, 41), (least.to_vec(), 40), "41 bytes hold the longest value");
    assert_eq!(int128_text(14, 4), (b"14\0\x07".to_vec(), 2));
    assert_eq!(int128_text(-14, 3), (b"-1\0".to_vec(), 3), "cut, its NUL kept and its whole length answered");
    assert_eq!(int128_text(-14, 0), (vec![], 3), "no room: only the length");

    // SAFETY: a null buffer is never written.
    let length = unsafe { kahlnws_int128_format(Int128::from(1 << 64), ptr::null_mut(), 0) };
    assert_eq!(length, 20, "the length to make room for");

    let mut buffer = [7_u8; 40];
    // SAFETY: the buffer is the test's own, with room for 40 bytes.
    let length = unsafe { kahlnws_uint128_format(UInt128::from(u128::MAX), buffer.as_mut_ptr().cast(), 40) };
    assert_eq!(&buffer[..], b"340282366920938463463374607431768211455\0");
    assert_eq!(length, 39, "40 bytes hold the longest unsigned value");

    let mut untouched = [7_u8; 4];
    // SAFETY: a capacity past `PTRDIFF_MAX` is checked and the buffer never written.
    let refused = unsafe { kahlnws_int128_format(Int128::from(14), untouched.as_mut_ptr().cast(), usize::MAX) };
    assert_eq!((untouched, refused), ([7; 4], 0), "a capacity past PTRDIFF_MAX writes nothing");
    // SAFETY: a capacity past `PTRDIFF_MAX` is checked and the buffer never written.
    let refused = unsafe { kahlnws_uint128_format(UInt128::from(14), untouched.as_mut_ptr().cast(), usize::MAX) };
    assert_eq!((untouched, refused), ([7; 4], 0), "a capacity past PTRDIFF_MAX writes nothing, unsigned");

    // SAFETY: a null buffer is never written, whatever the capacity.
    let length = unsafe { kahlnws_int128_format(Int128::from(-14), ptr::null_mut(), usize::MAX) };
    assert_eq!(length, 3, "a null buffer tells the length");
}

#[test]
fn test_every_status_has_its_name_and_an_unknown_number_one_too() {
    let names = [
        (StatusCode::OPTIMAL, "optimal"),
        (StatusCode::INFEASIBLE, "infeasible"),
        (StatusCode::OK, "ok"),
        (StatusCode::REFUSED, "refused"),
        (StatusCode::NO_OPTIMUM, "no optimum"),
        (StatusCode::NULL_POINTER, "null pointer"),
        (StatusCode::INVALID_ARGUMENT, "invalid argument"),
        (StatusCode::NO_ROOM, "no room"),
        (StatusCode::INTERNAL_ERROR, "internal error"),
        (StatusCode(9), "unknown status"),
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

/// A network made by `kahlnws_network_new`.
///
/// # Panics
/// Panics if the memory for it is refused.
fn new_network() -> *mut Network {
    let network = kahlnws_network_new();
    assert!(!network.is_null(), "the memory for a network should be granted");

    network
}

/// A new network that holds `tables`, not yet solved.
///
/// # Panics
/// Panics if the load does not answer `KAHLNWS_STATUS_OK`.
fn loaded(tables: &Tables) -> *mut Network {
    let network = new_network();
    let problem = tables.problem();

    // SAFETY: the network is live, the problem points into `tables`, and no refusal is asked for.
    let status = unsafe { kahlnws_network_load(network, &raw const problem, ptr::null_mut()) };
    assert_eq!(status, StatusCode::OK, "{tables:?}");

    network
}

/// A new network that holds `tables`, solved to its optimum.
///
/// # Panics
/// Panics if the load or the solve does not answer as it should.
fn solved_network(tables: &Tables) -> *mut Network {
    let network = loaded(tables);
    assert_eq!(network_solve(network).0, StatusCode::OPTIMAL, "{tables:?}");

    network
}

/// Frees `network`, which the test made and uses no more.
fn free_network(network: *mut Network) {
    // SAFETY: the network is live and used by nothing else from here on.
    unsafe { kahlnws_network_free(network) };
}

/// What `kahlnws_network_solve` answers and writes for `network`, its cost and work starting at 7.
fn network_solve(network: *mut Network) -> (StatusCode, Int128, Work) {
    let mut total_cost = Int128 { low: 7, high: 7 };
    let mut work = Work { pivots: 7, degenerate_pivots: 7, first_arc_pivots: 7 };

    // SAFETY: the network is live, and the outputs are the test's own.
    let status = unsafe { kahlnws_network_solve(network, &raw mut total_cost, &raw mut work) };

    (status, total_cost, work)
}

/// What `kahlnws_network_total_cost` answers and writes for `network`, the cost starting at 7.
fn network_total_cost(network: *const Network) -> (StatusCode, Int128) {
    let mut total_cost = Int128 { low: 7, high: 7 };

    // SAFETY: the network is live, and the output is the test's own.
    let status = unsafe { kahlnws_network_total_cost(network, &raw mut total_cost) };

    (status, total_cost)
}

/// What `kahlnws_network_flows` answers and writes for `network`, room for `arc_count` flows starting at 7.
fn network_flows(network: *const Network, arc_count: usize) -> (StatusCode, Vec<u64>) {
    let mut flows = vec![7; arc_count];

    // SAFETY: the network is live, and the flows have room for the arc count the caller gives, the network's.
    let status = unsafe { kahlnws_network_flows(network, flows.as_mut_ptr()) };

    (status, flows)
}

/// What `kahlnws_network_potentials` answers and writes for `network`, room for `node_count` potentials starting at
/// 7.
fn network_potentials(network: *const Network, node_count: usize) -> (StatusCode, Vec<i64>) {
    let mut potentials = vec![7; node_count];

    // SAFETY: the network is live, and the potentials have room for the node count the caller gives, the network's.
    let status = unsafe { kahlnws_network_potentials(network, potentials.as_mut_ptr()) };

    (status, potentials)
}

/// What `kahlnws_network_flow` answers and writes for `arc` of `network`, the flow starting at 7.
fn network_flow(network: *const Network, arc: usize) -> (StatusCode, u64) {
    let mut flow = 7;

    // SAFETY: the network is live, and the output is the test's own.
    let status = unsafe { kahlnws_network_flow(network, arc, &raw mut flow) };

    (status, flow)
}

/// Whether `potentials` prove `flows` optimal for `tables`: on every arc, `cost + potential[tail] - potential[head]`
/// at least zero where the flow is below the capacity, and at most zero where it is above the lower bound.
fn certifies(tables: &Tables, flows: &[u64], potentials: &[i64]) -> bool {
    (0..tables.tails.len()).all(|arc| {
        let reduced = i128::from(tables.costs[arc]) + i128::from(potentials[tables.tails[arc]])
            - i128::from(potentials[tables.heads[arc]]);
        let lower_bound = tables.lower_bounds.get(arc).copied().unwrap_or(0);
        (flows[arc] == tables.capacities[arc] || reduced >= 0) && (flows[arc] == lower_bound || reduced <= 0)
    })
}

/// Asserts that `network`, solved, holds the optimum a fresh solver finds for `tables`, the problem the network
/// holds: the same status, cost and flows, and potentials that prove the flows optimal. Every problem it is given has
/// one optimal flow.
///
/// # Panics
/// Panics if it does not.
fn assert_answers_as_a_fresh_solve(network: *mut Network, tables: &Tables) {
    let (status, expected) = solved(tables);
    assert_eq!(status, StatusCode::OPTIMAL, "{tables:?}");

    let (solved_status, total_cost, work) = network_solve(network);
    assert_eq!((solved_status, total_cost, work.first_arc_pivots), (status, expected.total_cost, 0), "{tables:?}");
    let (_, flows) = network_flows(network, tables.tails.len());
    let (_, potentials) = network_potentials(network, tables.supplies.len());
    assert_eq!(flows, expected.flows, "{tables:?}");
    assert!(certifies(tables, &flows, &potentials), "{tables:?}: {potentials:?}");
}

/// The work of the depot from a network's star: four pivots, one of them degenerate, as a solver's from its star,
/// since no arc into node 2 is wide enough for a solver to try first.
const NETWORK_DEPOT_WORK: Work = Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 };

#[test]
fn test_a_new_network_holds_the_problem_of_no_node_and_solves_it_with_no_work() {
    let network = new_network();

    // SAFETY: the network is live; flows and potentials of no entry need no storage.
    let (nodes, arcs, flows, potentials) = unsafe {
        (
            kahlnws_network_node_count(network),
            kahlnws_network_arc_count(network),
            kahlnws_network_flows(network, ptr::null_mut()),
            kahlnws_network_potentials(network, ptr::null_mut()),
        )
    };
    assert_eq!((nodes, arcs, flows, potentials), (0, 0, StatusCode::OK, StatusCode::OK));
    assert_eq!(network_total_cost(network), (StatusCode::OK, Int128::default()), "the optimum of no node");
    assert_eq!(network_flow(network, 0), (StatusCode::INVALID_ARGUMENT, 7), "no arc is below the count 0");
    assert_eq!(network_solve(network), (StatusCode::OPTIMAL, Int128::default(), Work::default()));

    free_network(network);
}

#[test]
fn test_the_depot_is_loaded_and_solved_with_its_cost_flows_and_potentials() {
    let network = loaded(&depot());

    // SAFETY: the network is live.
    let counts = unsafe { (kahlnws_network_node_count(network), kahlnws_network_arc_count(network)) };
    assert_eq!(counts, (3, 3));
    assert_eq!(network_total_cost(network), (StatusCode::NO_OPTIMUM, Int128 { low: 7, high: 7 }), "not solved yet");

    assert_eq!(network_solve(network), (StatusCode::OPTIMAL, Int128 { low: 14, high: 0 }, NETWORK_DEPOT_WORK));
    assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 14, high: 0 }));
    assert_eq!(network_flows(network, 3), (StatusCode::OK, vec![2, 2, 2]));
    assert_eq!([0, 1, 2].map(|arc| network_flow(network, arc)), [(StatusCode::OK, 2); 3]);
    // Arcs 0 and 1 carry flow strictly between their bounds, so the potentials are p, p + 1 and p + 5; the network's
    // tree hangs node 0 from the root by an empty link, at the potential 0.
    assert_eq!(network_potentials(network, 3), (StatusCode::OK, vec![0, 1, 5]));

    // Nothing changed: the same optimum again, with no pivot.
    assert_eq!(network_solve(network), (StatusCode::OPTIMAL, Int128 { low: 14, high: 0 }, Work::default()));

    free_network(network);
}

#[test]
fn test_a_reservation_answers_ok_no_room_or_a_null_pointer_and_keeps_the_optimum() {
    let network = solved_network(&depot());

    // SAFETY: the network is live and used by nothing else meanwhile; the null one is checked and never read.
    let statuses = unsafe {
        [
            kahlnws_network_reserve(network, 10, 20),
            kahlnws_network_reserve(network, usize::MAX, 1),
            kahlnws_network_reserve(network, 1, usize::MAX),
            kahlnws_network_reserve(ptr::null_mut(), 3, 3),
        ]
    };
    assert_eq!(statuses, [StatusCode::OK, StatusCode::NO_ROOM, StatusCode::NO_ROOM, StatusCode::NULL_POINTER]);
    assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 14, high: 0 }));

    free_network(network);
}

#[test]
fn test_a_refused_load_writes_its_breach_and_keeps_the_problem_and_the_optimum() {
    let network = solved_network(&depot());
    let refusals = [
        (Tables { supplies: vec![4, 0, -3], ..depot() }, "the supplies add up to 1, not zero"),
        (Tables { heads: vec![2, 1, 3], ..depot() }, "arc 2 enters node 3, not below the node count 3"),
        (Tables { lower_bounds: vec![0, 5, 0], ..depot() }, "arc 1 has lower bound 5 above its capacity 4"),
    ];

    for (tables, message) in refusals {
        let problem = tables.problem();
        let mut refusal = Refusal { arc: 7, ..Refusal::default() };
        // SAFETY: the network is live, the problem points into `tables`, and the refusal is the test's own.
        let status = unsafe { kahlnws_network_load(network, &raw const problem, &raw mut refusal) };

        assert_eq!(status, StatusCode::REFUSED, "{message}");
        assert_eq!(refused_with(&tables).0, refusal, "the refusal a solve writes: {message}");
        assert_eq!(text_of(&refusal), message);
        assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 14, high: 0 }), "{message}");
    }

    free_network(network);
}

/// The message `kahlnws_refusal_message` writes for `refusal`, whole, as text.
///
/// # Panics
/// Panics if the message is not whole, NUL-terminated UTF-8.
fn text_of(refusal: &Refusal) -> String {
    let (bytes, length) = message(refusal, 256);
    let written = CStr::from_bytes_until_nul(&bytes).expect("the message should be NUL-terminated");
    assert_eq!(written.count_bytes(), length, "the message should be whole");

    written.to_str().expect("the message should be UTF-8").to_owned()
}

#[test]
fn test_a_load_of_other_counts_holds_the_problem_whole_and_solves_it_as_a_fresh_one() {
    let network = solved_network(&depot());

    // The chain has four nodes where the depot has three: the network holds it whole, and solves it from its star.
    let tables = chain();
    let problem = tables.problem();
    // SAFETY: the network is live, the problem points into the chain's tables, and no refusal is asked for.
    let status = unsafe { kahlnws_network_load(network, &raw const problem, ptr::null_mut()) };
    // SAFETY: the network is live.
    let counts = unsafe { (kahlnws_network_node_count(network), kahlnws_network_arc_count(network)) };
    assert_eq!((status, counts), (StatusCode::OK, (4, 3)));
    assert_eq!(network_total_cost(network).0, StatusCode::NO_OPTIMUM, "a load ends the optimum held");
    assert_answers_as_a_fresh_solve(network, &tables);

    free_network(network);
}

#[test]
fn test_each_change_and_a_solve_after_it_answer_a_fresh_solve_of_the_changed_problem() {
    // Each change of the depot, with the tables it leaves and what it replaced; each changed problem has one optimal
    // flow. A cost of 3 on arc 2 makes the leg through node 1 cost 4: 2 units by it and 2 direct, 18. A lower bound of
    // 3 on arc 0: 3 direct and 1 by node 1, 17. Arc 0 widened to 4: still 2 by node 1, at 2 a unit, and 2 direct, 14.
    // Arc 2 bounded to 3 to 4: every unit by node 1, 8. The customer's one
    // more unit from the depot goes direct: 3 direct and 2 by node 1, 19. The depot set to send 2, balanced by the
    // customer: both by node 1, 4. The customer set to take 5, balanced by the depot, or the depot set to send 5,
    // balanced by the customer: 19 again. Arc 1 switched off: 3 direct, and the fourth unit has no way, so no flow
    // exists; that one is checked below.
    let changed = [
        (Change::Cost(2, 3), Tables { costs: vec![5, 1, 3], ..depot() }, 1),
        (Change::LowerBound(0, 3), Tables { lower_bounds: vec![3, 0, 0], ..depot() }, 0),
        (Change::Capacity(0, 4), Tables { capacities: vec![4, 4, 2], ..depot() }, 3),
        (Change::Bounds(2, 3, 4), Tables { lower_bounds: vec![0, 0, 3], capacities: vec![3, 4, 4], ..depot() }, 0),
        (Change::Supply(2, 0, 1), Tables { supplies: vec![5, 0, -5], ..depot() }, 7),
        (Change::SetSupply(0, 2, 2), Tables { supplies: vec![2, 0, -2], ..depot() }, 4),
        (Change::SetSupply(2, -5, 0), Tables { supplies: vec![5, 0, -5], ..depot() }, -4),
        (Change::SetSupply(0, 5, 2), Tables { supplies: vec![5, 0, -5], ..depot() }, 4),
    ];

    for (change, tables, replaced) in changed {
        let network = solved_network(&depot());
        let (status, previous) = change.applied(network);

        assert_eq!((status, previous), (StatusCode::OK, replaced), "{change:?}");
        assert_eq!(network_total_cost(network).0, StatusCode::NO_OPTIMUM, "{change:?} ends the optimum");
        assert_answers_as_a_fresh_solve(network, &tables);

        free_network(network);
    }
}

/// One change of a network by its entry: the arc or nodes it names, and the numbers it sets.
#[derive(Clone, Copy, Debug)]
enum Change {
    /// `kahlnws_network_set_cost(arc, cost)`.
    Cost(usize, i64),
    /// `kahlnws_network_set_capacity(arc, capacity)`.
    Capacity(usize, u64),
    /// `kahlnws_network_set_lower_bound(arc, lower_bound)`.
    LowerBound(usize, u64),
    /// `kahlnws_network_set_bounds(arc, lower_bound, capacity)`.
    Bounds(usize, u64, u64),
    /// `kahlnws_network_move_supply(from_node, to_node, amount)`.
    Supply(usize, usize, u64),
    /// `kahlnws_network_set_supply(node, supply, balancing_node)`.
    SetSupply(usize, i64, usize),
}

impl Change {
    /// Applies the change to `network` by its entry, every output given, and answers the status and what it wrote as
    /// replaced: the number for a setter, the supply of the node set for a supply, the lower bound for a change of
    /// both bounds, which writes its capacity to the second output, and 7, the marker, for a move of supply, which
    /// writes none.
    fn applied(self, network: *mut Network) -> (StatusCode, i128) {
        let (status, replaced, _) = self.answered(network);

        (status, replaced)
    }

    /// Applies the change to `network` by its entry, every output given and starting at 7, and answers the status,
    /// what it wrote as replaced, as [`applied`](Self::applied) says, and the refusal it wrote.
    fn answered(self, network: *mut Network) -> (StatusCode, i128, Refusal) {
        let mut refusal = Refusal { arc: 7, ..Refusal::default() };
        let (mut cost, mut bound, mut capacity, mut supply) = (7_i64, 7_u64, 7_u64, 7_i64);
        let refusal_output = &raw mut refusal;

        // SAFETY: the network is live, and every output is the test's own.
        let (status, replaced) = unsafe {
            match self {
                Self::Cost(arc, set) => {
                    (kahlnws_network_set_cost(network, arc, set, &raw mut cost, refusal_output), i128::from(cost))
                }
                Self::Capacity(arc, set) => {
                    (kahlnws_network_set_capacity(network, arc, set, &raw mut bound, refusal_output), i128::from(bound))
                }
                Self::LowerBound(arc, set) => (
                    kahlnws_network_set_lower_bound(network, arc, set, &raw mut bound, refusal_output),
                    i128::from(bound),
                ),
                Self::Bounds(arc, lower_bound, set) => (
                    kahlnws_network_set_bounds(
                        network,
                        arc,
                        lower_bound,
                        set,
                        &raw mut bound,
                        &raw mut capacity,
                        refusal_output,
                    ),
                    i128::from(bound),
                ),
                Self::Supply(from_node, to_node, amount) => {
                    (kahlnws_network_move_supply(network, from_node, to_node, amount, refusal_output), 7)
                }
                Self::SetSupply(node, set, balancing_node) => (
                    kahlnws_network_set_supply(network, node, set, balancing_node, &raw mut supply, refusal_output),
                    i128::from(supply),
                ),
            }
        };
        if let Self::Bounds(..) = self {
            assert!(status != StatusCode::OK || capacity != 7, "a change of bounds writes the capacity it had");
        }

        (status, replaced, refusal)
    }
}

#[test]
fn test_a_supply_set_and_set_back_moves_both_nodes_by_hand() {
    // The customer is to take 5, balanced by the depot, which sends one more: 3 directly, 2 through node 1, 19. The
    // supply it answered, set back, makes both nodes and the cost as they were, 14.
    let network = solved_network(&depot());
    assert_eq!(Change::SetSupply(2, -5, 0).applied(network), (StatusCode::OK, -4));
    assert_eq!(read_back(network).0.supplies, [5, 0, -5]);
    assert_eq!(network_solve(network).1, Int128 { low: 19, high: 0 });
    assert_eq!(Change::SetSupply(2, -4, 0).applied(network), (StatusCode::OK, -5));
    assert_eq!(read_back(network).0.supplies, [4, 0, -4]);
    assert_eq!(network_solve(network).1, Int128 { low: 14, high: 0 });

    // Node 1 is to send 2, balanced by the depot, which sends 2 less; then node 1 is to take 1, balanced by the
    // customer, which takes 3 less. The depot sends 1 to node 1 and 1 on to the customer: `2 + 1 = 3`.
    assert_eq!(Change::SetSupply(1, 2, 0).applied(network), (StatusCode::OK, 0));
    assert_eq!(Change::SetSupply(1, -1, 2).applied(network), (StatusCode::OK, 2));
    assert_eq!(read_back(network).0.supplies, [2, -1, -1]);
    assert_eq!(network_solve(network).1, Int128 { low: 3, high: 0 });

    free_network(network);
}

#[test]
fn test_a_change_of_both_bounds_writes_the_lower_bound_and_the_capacity_it_had() {
    let network = solved_network(&depot());
    let (mut lower_bound, mut capacity) = (7, 7);

    // SAFETY: the network is live, and the outputs are the test's own; no refusal is asked for.
    let status = unsafe {
        kahlnws_network_set_bounds(network, 2, 3, 4, &raw mut lower_bound, &raw mut capacity, ptr::null_mut())
    };
    assert_eq!((status, lower_bound, capacity), (StatusCode::OK, 0, 2));

    free_network(network);
}

#[test]
fn test_a_change_set_back_answers_the_earlier_optimum() {
    let network = solved_network(&depot());

    let changes = [
        (Change::Cost(2, 3), Change::Cost(2, 1)),
        (Change::Capacity(1, 0), Change::Capacity(1, 4)),
        (Change::LowerBound(0, 3), Change::LowerBound(0, 0)),
        (Change::Bounds(2, 3, 4), Change::Bounds(2, 0, 2)),
        (Change::Supply(2, 0, 1), Change::Supply(0, 2, 1)),
        (Change::SetSupply(2, -5, 0), Change::SetSupply(2, -4, 0)),
    ];
    for (change, back) in changes {
        assert_eq!(change.applied(network).0, StatusCode::OK, "{change:?}");
        assert_eq!(back.applied(network).0, StatusCode::OK, "{back:?}");
        assert_eq!(network_total_cost(network).0, StatusCode::NO_OPTIMUM, "{change:?} and its set-back are changes");
        assert_answers_as_a_fresh_solve(network, &depot());
    }

    free_network(network);
}

#[test]
fn test_a_change_that_sets_the_number_held_changes_nothing_and_keeps_the_optimum() {
    let network = solved_network(&depot());

    let unchanged = [
        (Change::Cost(0, 5), 5),
        (Change::Capacity(1, 4), 4),
        (Change::LowerBound(2, 0), 0),
        (Change::Bounds(2, 0, 2), 0),
        (Change::Supply(1, 1, 9), 7),
        (Change::Supply(0, 2, 0), 7),
        (Change::SetSupply(0, 4, 2), 4),
        (Change::SetSupply(1, 0, 1), 0),
    ];
    for (change, held) in unchanged {
        assert_eq!(change.applied(network), (StatusCode::OK, held), "{change:?}");
        assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 14, high: 0 }), "{change:?}");
    }
    assert_eq!(network_solve(network).2, Work::default(), "nothing changed, so nothing pivots");

    free_network(network);
}

#[test]
fn test_a_switched_off_arc_that_strands_flow_solves_to_infeasible_with_its_work_and_no_optimum() {
    let network = solved_network(&depot());

    // Arc 1 off: node 1 can no longer be reached, so the fourth unit has no way to node 2.
    assert_eq!(Change::Capacity(1, 0).applied(network), (StatusCode::OK, 4));
    let (status, total_cost, work) = network_solve(network);
    assert_eq!((status, total_cost, work.first_arc_pivots), (StatusCode::INFEASIBLE, Int128 { low: 7, high: 7 }, 0));
    assert_eq!(network_total_cost(network), (StatusCode::NO_OPTIMUM, Int128 { low: 7, high: 7 }));
    assert_eq!(network_flows(network, 3), (StatusCode::NO_OPTIMUM, vec![7, 7, 7]));
    assert_eq!(network_potentials(network, 3), (StatusCode::NO_OPTIMUM, vec![7, 7, 7]));
    assert_eq!(network_flow(network, 0), (StatusCode::NO_OPTIMUM, 7));

    // On again with the capacity it answered: the earlier optimum.
    assert_eq!(Change::Capacity(1, 4).applied(network), (StatusCode::OK, 0));
    assert_answers_as_a_fresh_solve(network, &depot());

    free_network(network);
}

#[test]
fn test_a_read_answers_no_optimum_after_a_load_and_after_a_change_and_writes_nothing() {
    let network = loaded(&depot());

    assert_eq!(network_total_cost(network), (StatusCode::NO_OPTIMUM, Int128 { low: 7, high: 7 }));
    assert_eq!(network_flows(network, 3), (StatusCode::NO_OPTIMUM, vec![7, 7, 7]));
    assert_eq!(network_potentials(network, 3), (StatusCode::NO_OPTIMUM, vec![7, 7, 7]));
    assert_eq!(network_flow(network, 2), (StatusCode::NO_OPTIMUM, 7));

    assert_eq!(network_solve(network).0, StatusCode::OPTIMAL);
    assert_eq!(Change::Cost(0, 4).applied(network).0, StatusCode::OK);
    assert_eq!(network_total_cost(network), (StatusCode::NO_OPTIMUM, Int128 { low: 7, high: 7 }));
    assert_eq!(network_flows(network, 3), (StatusCode::NO_OPTIMUM, vec![7, 7, 7]));
    assert_eq!(network_potentials(network, 3), (StatusCode::NO_OPTIMUM, vec![7, 7, 7]));
    assert_eq!(network_flow(network, 2), (StatusCode::NO_OPTIMUM, 7));

    free_network(network);
}

/// One node with three loops, each of the widest capacity: the cost total of a network of it passes `2^127 - 1`
/// once three of its costs lie at the bound, `2^62 - 1`, since three times `(2^62 - 1) * (2^64 - 1)` passes it while
/// two times does not.
fn widest_loops() -> Tables {
    Tables {
        tails: vec![0, 0, 0],
        heads: vec![0, 0, 0],
        lower_bounds: vec![],
        capacities: vec![u64::MAX; 3],
        costs: vec![0, 0, 0],
        supplies: vec![0],
    }
}

/// Four nodes and no arc: a move of supply between two of them goes up and down their root links, which carry it
/// toward the root as far as they admit.
fn four_nodes() -> Tables {
    Tables { supplies: vec![0, 0, 0, 0], ..Tables::default() }
}

#[test]
fn test_each_refusal_of_a_change_writes_its_breach_and_message_and_changes_nothing() {
    let bound = kahlnws_cost_bound(3);
    let refused = [
        (
            Change::Cost(1, bound + 1),
            Refusal {
                kind: RefusalKind::COST_BEYOND_BOUND,
                arc: 1,
                cost: bound + 1,
                node_count: 3,
                ..Refusal::default()
            },
            format!("arc 1 has cost {} beyond the bound ±{bound} for the node count 3", bound + 1),
        ),
        (
            Change::LowerBound(0, 4),
            Refusal {
                kind: RefusalKind::LOWER_ABOVE_CAPACITY,
                arc: 0,
                lower_bound: 4,
                capacity: 3,
                ..Refusal::default()
            },
            "arc 0 has lower bound 4 above its capacity 3".to_owned(),
        ),
        (
            Change::Bounds(2, 3, 2),
            Refusal {
                kind: RefusalKind::LOWER_ABOVE_CAPACITY,
                arc: 2,
                lower_bound: 3,
                capacity: 2,
                ..Refusal::default()
            },
            "arc 2 has lower bound 3 above its capacity 2".to_owned(),
        ),
        (
            Change::Supply(2, 1, u64::MAX),
            Refusal {
                kind: RefusalKind::SUPPLY_BEYOND_RANGE,
                node: 2,
                supply: Int128::from(-4 - i128::from(u64::MAX)),
                ..Refusal::default()
            },
            "the supply of node 2 would be -18446744073709551619, outside the range of a signed 64-bit integer"
                .to_owned(),
        ),
        (
            Change::SetSupply(2, i64::MIN, 0),
            Refusal {
                kind: RefusalKind::SUPPLY_BEYOND_RANGE,
                node: 0,
                supply: Int128::from(i128::from(i64::MAX) + 1),
                ..Refusal::default()
            },
            "the supply of node 0 would be 9223372036854775808, outside the range of a signed 64-bit integer"
                .to_owned(),
        ),
    ];

    for (change, expected, message) in refused {
        let network = solved_network(&depot());
        let (status, replaced, refusal) = change.answered(network);

        assert_eq!((status, replaced, refusal), (StatusCode::REFUSED, 7, expected), "{change:?}");
        assert_eq!(text_of(&refusal), message);
        assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 14, high: 0 }), "{change:?}");
        assert_eq!(network_solve(network).2, Work::default(), "{change:?} changed nothing");

        free_network(network);
    }
}

#[test]
fn test_a_cost_that_takes_the_cost_total_past_its_range_is_refused_at_the_arc_changed() {
    let bound = kahlnws_cost_bound(1);
    let network = solved_network(&widest_loops());

    assert_eq!(Change::Cost(0, bound).applied(network), (StatusCode::OK, 0));
    assert_eq!(Change::Cost(1, -bound).applied(network), (StatusCode::OK, 0));
    let (status, replaced, refusal) = Change::Cost(2, bound).answered(network);

    let expected = Refusal { kind: RefusalKind::COST_TOTAL_BEYOND_RANGE, arc: 2, ..Refusal::default() };
    assert_eq!((status, replaced, refusal), (StatusCode::REFUSED, 7, expected));
    assert_eq!(
        text_of(&refusal),
        format!("the costs times the capacities, in magnitude, sum past {} at arc 2", i128::MAX)
    );

    free_network(network);
}

#[test]
fn test_a_move_past_the_flow_the_root_links_admit_is_refused_with_the_sum_and_changes_nothing() {
    let network = solved_network(&four_nodes());
    let most = u64::MAX >> 1;

    // No arc carries either move, so each goes up one root link and down another: the flow up the root links reaches
    // 2 * (2^63 - 1) = 2^64 - 2, the most admitted, exactly.
    assert_eq!(Change::Supply(0, 1, most).applied(network).0, StatusCode::OK);
    assert_eq!(Change::Supply(2, 3, most).applied(network).0, StatusCode::OK);
    // SAFETY: the network is live.
    let before = unsafe { kahlnws_network_clone(network) };
    assert!(!before.is_null(), "the memory for a copy should be granted");
    let (status, _, refusal) = Change::Supply(2, 0, 1).answered(network);

    let total = UInt128 { low: u64::MAX, high: 0 };
    let expected = Refusal { kind: RefusalKind::EXCESS_BEYOND_RANGE, excess_total: total, ..Refusal::default() };
    assert_eq!((status, refusal), (StatusCode::REFUSED, expected));
    assert_eq!(
        text_of(&refusal),
        "this change would leave 18446744073709551615 units of flow still to be sent, past \
         18446744073709551614; a solve that finds a flow leaves none"
    );
    // The supply of node 0 set one lower, balanced by node 2, moves the same unit the other way: as refused.
    assert_eq!(Change::SetSupply(0, i64::MIN, 2).answered(network), (StatusCode::REFUSED, 7, expected));

    // Nothing changed: with no arc, the moved supplies have no way, and the network solves as its copy made before
    // the refused move does, to no flow, with the same work.
    let (status, total_cost, work) = network_solve(network);
    assert_eq!((status, total_cost), (StatusCode::INFEASIBLE, Int128 { low: 7, high: 7 }));
    assert_eq!(network_solve(before), (status, total_cost, work));

    free_network(before);
    free_network(network);
}

#[test]
fn test_an_arc_or_a_node_not_below_its_count_is_an_invalid_argument_and_changes_nothing() {
    let network = solved_network(&depot());

    let invalid = [
        Change::Cost(3, 1),
        Change::Capacity(3, 1),
        Change::LowerBound(usize::MAX, 0),
        Change::Bounds(3, 0, 1),
        Change::Supply(3, 0, 1),
        Change::Supply(0, 3, 1),
        Change::SetSupply(3, 1, 0),
        Change::SetSupply(0, 1, 3),
        // A node that balances itself is given only the supply it has: any other is a call written wrongly.
        Change::SetSupply(0, 5, 0),
        Change::SetSupply(1, i64::MIN, 1),
    ];
    for change in invalid {
        let (status, replaced, refusal) = change.answered(network);
        assert_eq!((status, replaced), (StatusCode::INVALID_ARGUMENT, 7), "{change:?}");
        assert_eq!(refusal, Refusal { arc: 7, ..Refusal::default() }, "{change:?} writes no refusal");
    }
    assert_eq!(network_flow(network, 3), (StatusCode::INVALID_ARGUMENT, 7));
    assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 14, high: 0 }), "nothing changed");

    // A network never loaded has no arc and no node.
    let never_loaded = new_network();
    assert_eq!(Change::Cost(0, 1).applied(never_loaded).0, StatusCode::INVALID_ARGUMENT);
    assert_eq!(Change::Supply(0, 0, 1).applied(never_loaded).0, StatusCode::INVALID_ARGUMENT);
    assert_eq!(Change::SetSupply(0, 0, 0).applied(never_loaded).0, StatusCode::INVALID_ARGUMENT);

    free_network(never_loaded);
    free_network(network);
}

#[test]
fn test_outputs_of_a_network_passed_as_null_are_not_wanted() {
    let network = solved_network(&depot());

    // SAFETY: the network is live; every output is null, which asks for none of them.
    let statuses = unsafe {
        [
            kahlnws_network_set_cost(network, 2, 3, ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_set_capacity(network, 0, 4, ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_set_lower_bound(network, 0, 9, ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_set_bounds(network, 1, 0, 5, ptr::null_mut(), ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_move_supply(network, 2, 1, u64::MAX, ptr::null_mut()),
            kahlnws_network_set_supply(network, 0, 5, 2, ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_set_supply(network, 2, i64::MIN, 0, ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_solve(network, ptr::null_mut(), ptr::null_mut()),
        ]
    };
    let refused = StatusCode::REFUSED;
    let changed = StatusCode::OK;
    assert_eq!(statuses, [changed, changed, refused, changed, refused, changed, refused, StatusCode::OPTIMAL]);
    // Node 0 sends 5 and node 2 takes 5. With arc 2 at cost 3, arc 0 at capacity 4 and arc 1 at capacity 5, the way
    // through node 1 costs 1 + 3 = 4 a unit for the 2 units arc 2 carries, and the other 3 go direct at 5: 8 + 15 = 23.
    assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 23, high: 0 }));
    let mut supply = 7_i64;
    // SAFETY: the network is live and `supply` a value of the test's own.
    let status = unsafe { kahlnws_network_supply(network, 2, &raw mut supply) };
    assert_eq!((status, supply), (StatusCode::OK, -5));

    free_network(network);
}

#[test]
fn test_a_null_pointer_in_every_needed_position_of_a_network_writes_nothing() {
    let tables = depot();
    let problem = tables.problem();
    let network = solved_network(&tables);
    let (mut cost, mut flow) = (Int128 { low: 7, high: 7 }, 7_u64);

    // SAFETY: each call passes the live network, the depot and the test's outputs, but for the one null pointer,
    // which is checked and never read or written.
    let statuses = unsafe {
        [
            kahlnws_network_reserve(ptr::null_mut(), 3, 3),
            kahlnws_network_load(ptr::null_mut(), &raw const problem, ptr::null_mut()),
            kahlnws_network_load(network, ptr::null(), ptr::null_mut()),
            kahlnws_network_set_cost(ptr::null_mut(), 0, 1, ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_set_capacity(ptr::null_mut(), 0, 1, ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_set_lower_bound(ptr::null_mut(), 0, 1, ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_set_bounds(ptr::null_mut(), 0, 0, 1, ptr::null_mut(), ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_move_supply(ptr::null_mut(), 0, 1, 1, ptr::null_mut()),
            kahlnws_network_set_supply(ptr::null_mut(), 0, 1, 1, ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_solve(ptr::null_mut(), &raw mut cost, ptr::null_mut()),
            kahlnws_network_total_cost(ptr::null(), &raw mut cost),
            kahlnws_network_total_cost(network, ptr::null_mut()),
            kahlnws_network_flow(ptr::null(), 0, &raw mut flow),
            kahlnws_network_flow(network, 0, ptr::null_mut()),
            kahlnws_network_flows(ptr::null(), ptr::null_mut()),
            kahlnws_network_flows(network, ptr::null_mut()),
            kahlnws_network_potentials(ptr::null(), ptr::null_mut()),
            kahlnws_network_potentials(network, ptr::null_mut()),
        ]
    };
    assert_eq!(statuses, [StatusCode::NULL_POINTER; 18]);
    assert_eq!((cost, flow), (Int128 { low: 7, high: 7 }, 7));

    let refused = [
        Problem { tails: ptr::null(), ..problem },
        Problem { heads: ptr::null(), ..problem },
        Problem { capacities: ptr::null(), ..problem },
        Problem { costs: ptr::null(), ..problem },
        Problem { supplies: ptr::null(), ..problem },
    ];
    for problem in refused {
        // SAFETY: the network is live; every table of the problem but the null one points into `tables`, the null one
        // is checked and never read.
        let status = unsafe { kahlnws_network_load(network, &raw const problem, ptr::null_mut()) };
        assert_eq!(status, StatusCode::NULL_POINTER, "{problem:?}");
    }

    // SAFETY: null networks are checked and never read.
    let (nodes, arcs, copy) = unsafe {
        (
            kahlnws_network_node_count(ptr::null()),
            kahlnws_network_arc_count(ptr::null()),
            kahlnws_network_clone(ptr::null()),
        )
    };
    assert_eq!((nodes, arcs, copy), (0, 0, ptr::null_mut()));
    assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 14, high: 0 }), "nothing changed");

    free_network(network);
}

#[test]
fn test_a_misaligned_pointer_in_every_position_of_a_network_is_invalid_and_writes_nothing() {
    let tables = depot();
    let problem = tables.problem();
    let network = solved_network(&tables);
    let skewed = misaligned(network).cast_mut();
    let mut cost_storage = [Int128::default(); 2];
    let mut work_storage = [Work::default(); 2];
    let mut refusal_storage = [Refusal::default(); 2];
    let (mut previous_cost, mut previous_bound) = ([7_i64; 2], [7_u64; 3]);
    let (mut flows, mut potentials) = ([7_u64; 4], [7_i64; 4]);
    let problems = [problem; 2];
    let cost = misaligned(cost_storage.as_mut_ptr().cast_const()).cast_mut();
    let work = misaligned(work_storage.as_mut_ptr().cast_const()).cast_mut();
    let refusal = misaligned(refusal_storage.as_mut_ptr().cast_const()).cast_mut();
    let cost_previous = misaligned(previous_cost.as_mut_ptr().cast_const()).cast_mut();
    let bound_previous = misaligned(previous_bound.as_mut_ptr().cast_const()).cast_mut();
    let null_refusal = ptr::null_mut();

    // SAFETY: each call passes the live network, the depot and the test's outputs, but for the one misaligned
    // pointer, which is checked and never read or written.
    let statuses = unsafe {
        [
            kahlnws_network_reserve(skewed, 3, 3),
            kahlnws_network_load(skewed, &raw const problem, null_refusal),
            kahlnws_network_load(network, misaligned(problems.as_ptr()), null_refusal),
            kahlnws_network_load(network, &raw const problem, refusal),
            kahlnws_network_set_cost(skewed, 0, 1, ptr::null_mut(), null_refusal),
            kahlnws_network_set_cost(network, 0, 1, cost_previous, null_refusal),
            kahlnws_network_set_cost(network, 0, 1, ptr::null_mut(), refusal),
            kahlnws_network_set_capacity(skewed, 0, 1, ptr::null_mut(), null_refusal),
            kahlnws_network_set_capacity(network, 0, 1, bound_previous, null_refusal),
            kahlnws_network_set_capacity(network, 0, 1, ptr::null_mut(), refusal),
            kahlnws_network_set_lower_bound(skewed, 0, 1, ptr::null_mut(), null_refusal),
            kahlnws_network_set_lower_bound(network, 0, 1, bound_previous, null_refusal),
            kahlnws_network_set_lower_bound(network, 0, 1, ptr::null_mut(), refusal),
            kahlnws_network_set_bounds(skewed, 0, 0, 1, ptr::null_mut(), ptr::null_mut(), null_refusal),
            kahlnws_network_set_bounds(network, 0, 0, 1, bound_previous, ptr::null_mut(), null_refusal),
            kahlnws_network_set_bounds(network, 0, 0, 1, ptr::null_mut(), bound_previous, null_refusal),
            kahlnws_network_set_bounds(network, 0, 0, 1, ptr::null_mut(), ptr::null_mut(), refusal),
            kahlnws_network_move_supply(skewed, 0, 1, 1, null_refusal),
            kahlnws_network_move_supply(network, 0, 1, 1, refusal),
            kahlnws_network_set_supply(skewed, 0, 3, 2, ptr::null_mut(), null_refusal),
            kahlnws_network_set_supply(network, 0, 3, 2, cost_previous, null_refusal),
            kahlnws_network_set_supply(network, 0, 3, 2, ptr::null_mut(), refusal),
            kahlnws_network_solve(skewed, ptr::null_mut(), ptr::null_mut()),
            kahlnws_network_solve(network, cost, ptr::null_mut()),
            kahlnws_network_solve(network, ptr::null_mut(), work),
            kahlnws_network_total_cost(skewed, cost_storage.as_mut_ptr()),
            kahlnws_network_total_cost(network, cost),
            kahlnws_network_flow(skewed, 0, flows.as_mut_ptr()),
            kahlnws_network_flow(network, 0, misaligned(flows.as_mut_ptr().cast_const()).cast_mut()),
            kahlnws_network_flows(skewed, flows.as_mut_ptr()),
            kahlnws_network_flows(network, misaligned(flows.as_mut_ptr().cast_const()).cast_mut()),
            kahlnws_network_potentials(skewed, potentials.as_mut_ptr()),
            kahlnws_network_potentials(network, misaligned(potentials.as_mut_ptr().cast_const()).cast_mut()),
        ]
    };
    assert_eq!(statuses, [StatusCode::INVALID_ARGUMENT; 33]);
    assert_eq!((cost_storage, work_storage, refusal_storage), Default::default(), "nothing written");
    assert_eq!((previous_cost, previous_bound, flows, potentials), ([7; 2], [7; 3], [7; 4], [7; 4]));

    // SAFETY: a misaligned network is checked and never read, copied or freed.
    let (nodes, arcs, copy) = unsafe {
        (kahlnws_network_node_count(skewed), kahlnws_network_arc_count(skewed), kahlnws_network_clone(skewed))
    };
    // SAFETY: a misaligned network is checked and never freed.
    unsafe { kahlnws_network_free(skewed) };
    assert_eq!((nodes, arcs, copy), (0, 0, ptr::null_mut()));
    assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 14, high: 0 }), "nothing changed");
    assert_eq!(network_solve(network).2, Work::default(), "no change was applied");

    free_network(network);
}

#[test]
fn test_a_load_of_counts_past_ptrdiff_max_bytes_is_invalid_and_keeps_what_the_network_held() {
    let tables = depot();
    let problem = tables.problem();
    let network = solved_network(&tables);
    let too_many = usize::MAX / 8 + 1;

    for refused in [Problem { arc_count: too_many, ..problem }, Problem { node_count: too_many, ..problem }] {
        // SAFETY: the network is live; the counts past `PTRDIFF_MAX` bytes are checked and no table is read for them.
        let status = unsafe { kahlnws_network_load(network, &raw const refused, ptr::null_mut()) };
        assert_eq!(status, StatusCode::INVALID_ARGUMENT, "{refused:?}");
    }
    assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 14, high: 0 }));

    free_network(network);
}

#[test]
fn test_a_copy_holds_what_its_network_held_and_changes_apart_from_it() {
    let network = solved_network(&depot());

    // SAFETY: the network is live.
    let copy = unsafe { kahlnws_network_clone(network) };
    assert!(!copy.is_null(), "the memory for a copy should be granted");
    assert_eq!(network_total_cost(copy), (StatusCode::OK, Int128 { low: 14, high: 0 }), "the optimum copied");
    assert_eq!(network_flows(copy, 3), (StatusCode::OK, vec![2, 2, 2]));

    assert_eq!(Change::Cost(2, 3).applied(copy), (StatusCode::OK, 1));
    assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 14, high: 0 }), "the network stays");
    assert_eq!(network_total_cost(copy).0, StatusCode::NO_OPTIMUM);
    assert_answers_as_a_fresh_solve(copy, &Tables { costs: vec![5, 1, 3], ..depot() });
    assert_eq!(network_solve(network).2, Work::default(), "the network was not changed by its copy's change");

    // A copy made before a change is the earlier state: freed, the network and its copy are each whole.
    free_network(network);
    assert_eq!(network_total_cost(copy), (StatusCode::OK, Int128 { low: 18, high: 0 }));
    free_network(copy);

    let never_loaded = new_network();
    // SAFETY: the network is live.
    let copy = unsafe { kahlnws_network_clone(never_loaded) };
    assert_eq!(network_solve(copy), (StatusCode::OPTIMAL, Int128::default(), Work::default()));
    free_network(copy);
    free_network(never_loaded);
}

#[test]
fn test_freeing_a_null_network_frees_nothing() {
    // SAFETY: freeing null does nothing.
    unsafe { kahlnws_network_free(ptr::null_mut()) };
}

/// The problem `network` holds, read back by the reads of whole columns, its lower bounds empty where all are zero; the
/// statuses the six reads answered.
fn read_back(network: *const Network) -> (Tables, [StatusCode; 6]) {
    // SAFETY: the network is live.
    let (arcs, nodes) = unsafe { (kahlnws_network_arc_count(network), kahlnws_network_node_count(network)) };
    let mut read = Tables {
        tails: vec![7; arcs],
        heads: vec![7; arcs],
        lower_bounds: vec![7; arcs],
        capacities: vec![7; arcs],
        costs: vec![7; arcs],
        supplies: vec![7; nodes],
    };

    // SAFETY: the network is live, and every buffer has room for the network's counts.
    let statuses = unsafe {
        [
            kahlnws_network_tails(network, read.tails.as_mut_ptr()),
            kahlnws_network_heads(network, read.heads.as_mut_ptr()),
            kahlnws_network_costs(network, read.costs.as_mut_ptr()),
            kahlnws_network_lower_bounds(network, read.lower_bounds.as_mut_ptr()),
            kahlnws_network_capacities(network, read.capacities.as_mut_ptr()),
            kahlnws_network_supplies(network, read.supplies.as_mut_ptr()),
        ]
    };
    if read.lower_bounds.iter().all(|&bound| bound == 0) {
        read.lower_bounds.clear();
    }

    (read, statuses)
}

/// Asserts that `network` reads back `tables`, column by column and arc by arc.
///
/// # Panics
/// Panics if it does not.
fn assert_reads(network: *const Network, tables: &Tables) {
    let (read, statuses) = read_back(network);
    assert_eq!(statuses, [StatusCode::OK; 6]);
    assert_eq!(read.tails, tables.tails);
    assert_eq!(read.heads, tables.heads);
    assert_eq!(read.costs, tables.costs);
    assert_eq!(read.capacities, tables.capacities);
    assert_eq!(read.supplies, tables.supplies);
    assert_eq!(read.lower_bounds, tables.lower_bounds);

    for arc in 0..tables.tails.len() {
        let (mut tail, mut head, mut cost, mut lower_bound, mut capacity) = (7, 7, 7, 7, 7);
        let (mut lower_bound_alone, mut capacity_alone) = (7, 7);
        // SAFETY: the network is live, and every output is the test's own.
        let statuses = unsafe {
            [
                kahlnws_network_tail(network, arc, &raw mut tail),
                kahlnws_network_head(network, arc, &raw mut head),
                kahlnws_network_cost(network, arc, &raw mut cost),
                kahlnws_network_bounds(network, arc, &raw mut lower_bound, &raw mut capacity),
                kahlnws_network_lower_bound(network, arc, &raw mut lower_bound_alone),
                kahlnws_network_capacity(network, arc, &raw mut capacity_alone),
            ]
        };
        let lower = tables.lower_bounds.get(arc).copied().unwrap_or(0);
        assert_eq!(statuses, [StatusCode::OK; 6], "arc {arc}");
        let expected = (tables.tails[arc], tables.heads[arc], tables.costs[arc], lower, tables.capacities[arc]);
        assert_eq!((tail, head, cost, lower_bound, capacity), expected, "arc {arc}");
        assert_eq!((lower_bound_alone, capacity_alone), (lower, tables.capacities[arc]), "arc {arc}");
    }
    for (node, &expected) in tables.supplies.iter().enumerate() {
        let mut supply = 7;
        // SAFETY: the network is live, and the output is the test's own.
        let status = unsafe { kahlnws_network_supply(network, node, &raw mut supply) };
        assert_eq!((status, supply), (StatusCode::OK, expected), "node {node}");
    }
}

#[test]
fn test_the_reads_of_the_problem_answer_it_after_a_load_and_need_no_optimum() {
    let tables = Tables { lower_bounds: vec![1, 0, 2], ..depot() };
    let network = loaded(&tables);

    assert_reads(network, &tables);
    assert_eq!(network_total_cost(network).0, StatusCode::NO_OPTIMUM, "a read needs no optimum and makes none");
    assert_eq!(network_solve(network).0, StatusCode::OPTIMAL);
    assert_reads(network, &tables);
    assert_eq!(network_total_cost(network).0, StatusCode::OK, "a read keeps the optimum");

    free_network(network);
}

#[test]
fn test_the_reads_of_the_problem_answer_each_change_and_not_a_refused_one() {
    let network = solved_network(&depot());
    let mut expected = depot();

    let changes = [
        Change::Cost(2, 3),
        Change::Capacity(0, 4),
        Change::LowerBound(1, 1),
        Change::Bounds(2, 1, 3),
        Change::Supply(2, 0, 1),
        Change::SetSupply(1, 2, 0),
    ];
    for change in changes {
        assert_eq!(change.applied(network).0, StatusCode::OK, "{change:?}");
        match change {
            Change::Cost(arc, cost) => expected.costs[arc] = cost,
            Change::Capacity(arc, capacity) => expected.capacities[arc] = capacity,
            Change::LowerBound(arc, lower_bound) => {
                expected.lower_bounds.resize(3, 0);
                expected.lower_bounds[arc] = lower_bound;
            }
            Change::Bounds(arc, lower_bound, capacity) => {
                expected.lower_bounds.resize(3, 0);
                expected.lower_bounds[arc] = lower_bound;
                expected.capacities[arc] = capacity;
            }
            Change::Supply(from_node, to_node, amount) => {
                let amount = i64::try_from(amount).expect("the amount should fit an i64");
                expected.supplies[from_node] -= amount;
                expected.supplies[to_node] += amount;
            }
            Change::SetSupply(node, supply, balancing_node) => {
                expected.supplies[balancing_node] -= supply - expected.supplies[node];
                expected.supplies[node] = supply;
            }
        }
        assert_reads(network, &expected);
    }

    let refused_changes = [
        Change::Cost(0, i64::MAX),
        Change::LowerBound(0, 9),
        Change::Supply(2, 1, u64::MAX),
        // The customer, at -5, would fall to `-2^63 - 1` to balance the depot's rise from 3 to `2^63 - 1`.
        Change::SetSupply(0, i64::MAX, 2),
    ];
    for refused in refused_changes {
        assert_eq!(refused.applied(network).0, StatusCode::REFUSED, "{refused:?}");
        assert_reads(network, &expected);
    }

    free_network(network);
}

#[test]
fn test_a_network_never_loaded_reads_no_number_and_an_index_past_its_count_is_invalid() {
    let never_loaded = new_network();
    // SAFETY: the network is live; columns of no entry need no storage.
    let statuses = unsafe {
        [
            kahlnws_network_tails(never_loaded, ptr::null_mut()),
            kahlnws_network_heads(never_loaded, ptr::null_mut()),
            kahlnws_network_costs(never_loaded, ptr::null_mut()),
            kahlnws_network_lower_bounds(never_loaded, ptr::null_mut()),
            kahlnws_network_capacities(never_loaded, ptr::null_mut()),
            kahlnws_network_supplies(never_loaded, ptr::null_mut()),
        ]
    };
    assert_eq!(statuses, [StatusCode::OK; 6]);

    let network = solved_network(&depot());
    for (held, arc, node) in [(never_loaded, 0, 0), (network, 3, 3)] {
        let (mut index, mut number, mut bound, mut capacity) = (7_usize, 7_i64, 7_u64, 7_u64);
        // SAFETY: the network is live, and every output is the test's own.
        let statuses = unsafe {
            [
                kahlnws_network_tail(held, arc, &raw mut index),
                kahlnws_network_head(held, arc, &raw mut index),
                kahlnws_network_cost(held, arc, &raw mut number),
                kahlnws_network_bounds(held, arc, &raw mut bound, &raw mut capacity),
                kahlnws_network_lower_bound(held, arc, &raw mut bound),
                kahlnws_network_capacity(held, arc, &raw mut capacity),
                kahlnws_network_supply(held, node, &raw mut number),
            ]
        };
        assert_eq!(statuses, [StatusCode::INVALID_ARGUMENT; 7]);
        assert_eq!((index, number, bound, capacity), (7, 7, 7, 7), "nothing written");
    }

    free_network(network);
    free_network(never_loaded);
}

#[test]
fn test_a_null_or_misaligned_pointer_in_a_read_of_the_problem_writes_nothing() {
    let network = solved_network(&depot());
    let skewed = misaligned(network).cast_mut();
    let (mut index, mut number, mut bound) = ([7_usize; 4], [7_i64; 4], [7_u64; 4]);
    let index_skewed = misaligned(index.as_mut_ptr().cast_const()).cast_mut();
    let number_skewed = misaligned(number.as_mut_ptr().cast_const()).cast_mut();
    let bound_skewed = misaligned(bound.as_mut_ptr().cast_const()).cast_mut();
    let (index_ok, number_ok, bound_ok) = (index.as_mut_ptr(), number.as_mut_ptr(), bound.as_mut_ptr());

    // SAFETY: each call passes the live network and the test's storage, with room for the depot's counts, but for
    // the one null or misaligned pointer, which is checked and never read or written.
    let null = unsafe {
        [
            kahlnws_network_tail(ptr::null(), 0, index_ok),
            kahlnws_network_tail(network, 0, ptr::null_mut()),
            kahlnws_network_head(network, 0, ptr::null_mut()),
            kahlnws_network_cost(network, 0, ptr::null_mut()),
            kahlnws_network_bounds(network, 0, ptr::null_mut(), bound_ok),
            kahlnws_network_bounds(network, 0, bound_ok, ptr::null_mut()),
            kahlnws_network_lower_bound(ptr::null(), 0, bound_ok),
            kahlnws_network_lower_bound(network, 0, ptr::null_mut()),
            kahlnws_network_capacity(ptr::null(), 0, bound_ok),
            kahlnws_network_capacity(network, 0, ptr::null_mut()),
            kahlnws_network_supply(network, 0, ptr::null_mut()),
            kahlnws_network_tails(network, ptr::null_mut()),
            kahlnws_network_heads(network, ptr::null_mut()),
            kahlnws_network_costs(network, ptr::null_mut()),
            kahlnws_network_lower_bounds(network, ptr::null_mut()),
            kahlnws_network_capacities(network, ptr::null_mut()),
            kahlnws_network_supplies(ptr::null(), number_ok),
            kahlnws_network_supplies(network, ptr::null_mut()),
        ]
    };
    // SAFETY: each call passes the live network and the test's storage, with room for the depot's counts, but for
    // the one misaligned pointer, which is checked and never read or written.
    let skewed_statuses = unsafe {
        [
            kahlnws_network_tail(skewed, 0, index_ok),
            kahlnws_network_tail(network, 0, index_skewed),
            kahlnws_network_head(network, 0, index_skewed),
            kahlnws_network_cost(network, 0, number_skewed),
            kahlnws_network_bounds(network, 0, bound_skewed, bound_ok),
            kahlnws_network_bounds(network, 0, bound_ok, bound_skewed),
            kahlnws_network_lower_bound(skewed, 0, bound_ok),
            kahlnws_network_lower_bound(network, 0, bound_skewed),
            kahlnws_network_capacity(skewed, 0, bound_ok),
            kahlnws_network_capacity(network, 0, bound_skewed),
            kahlnws_network_supply(network, 0, number_skewed),
            kahlnws_network_tails(network, index_skewed),
            kahlnws_network_heads(skewed, index_ok),
            kahlnws_network_costs(network, number_skewed),
            kahlnws_network_lower_bounds(network, bound_skewed),
            kahlnws_network_capacities(network, bound_skewed),
            kahlnws_network_supplies(network, number_skewed),
        ]
    };
    assert_eq!(null, [StatusCode::NULL_POINTER; 18]);
    assert_eq!(skewed_statuses, [StatusCode::INVALID_ARGUMENT; 17]);
    assert_eq!((index, number, bound), ([7; 4], [7; 4], [7; 4]), "nothing written");

    free_network(network);
}

#[test]
fn test_a_copy_by_a_new_handle_or_into_a_network_reads_as_its_source_and_solves_from_its_basis() {
    let tables = Tables { lower_bounds: vec![1, 0, 0], ..depot() };
    let network = solved_network(&tables);

    // SAFETY: the network is live.
    let cloned = unsafe { kahlnws_network_clone(network) };
    assert!(!cloned.is_null(), "the memory for a copy should be granted");
    let destination = solved_network(&chain());
    // SAFETY: both networks are live and distinct.
    let status = unsafe { kahlnws_network_copy(destination, network) };
    assert_eq!(status, StatusCode::OK);

    for copy in [cloned, destination] {
        assert_reads(copy, &tables);
        let held = network_total_cost(network);
        assert_eq!(network_total_cost(copy), held, "the copy holds the optimum");
        assert_eq!(network_solve(copy), (StatusCode::OPTIMAL, held.1, Work::default()), "from the copied basis");
        assert_eq!(Change::Cost(2, 3).applied(copy).0, StatusCode::OK);
        assert_eq!(network_total_cost(network), held, "the network stays");
    }

    // A network copied onto itself is itself: nothing changes, and the optimum stays.
    // SAFETY: the network is live; the same pointer as both is a copy of itself.
    let status = unsafe { kahlnws_network_copy(network, network) };
    assert_eq!((status, network_solve(network).2), (StatusCode::OK, Work::default()));

    // A copy of a network never loaded holds the problem of no node.
    let never_loaded = new_network();
    // SAFETY: both networks are live and distinct.
    let status = unsafe { kahlnws_network_copy(cloned, never_loaded) };
    // SAFETY: the network is live.
    let counts = unsafe { (kahlnws_network_node_count(cloned), kahlnws_network_arc_count(cloned)) };
    assert_eq!((status, counts), (StatusCode::OK, (0, 0)));

    for held in [never_loaded, cloned, destination, network] {
        free_network(held);
    }
}

#[test]
fn test_a_copy_with_a_null_or_misaligned_network_writes_nothing() {
    let network = solved_network(&depot());
    let destination = solved_network(&chain());
    let skewed = misaligned(network).cast_mut();

    // SAFETY: each call passes live networks but for the one null or misaligned pointer, which is checked and never
    // read or written.
    let statuses = unsafe {
        [
            kahlnws_network_copy(ptr::null_mut(), network),
            kahlnws_network_copy(destination, ptr::null()),
            kahlnws_network_copy(skewed, network),
            kahlnws_network_copy(destination, skewed),
        ]
    };
    // SAFETY: a misaligned network is checked and never read.
    let copy = unsafe { kahlnws_network_clone(skewed) };
    assert_eq!(
        statuses,
        [
            StatusCode::NULL_POINTER,
            StatusCode::NULL_POINTER,
            StatusCode::INVALID_ARGUMENT,
            StatusCode::INVALID_ARGUMENT
        ]
    );
    assert!(copy.is_null());
    assert_reads(destination, &chain());

    free_network(destination);
    free_network(network);
}

/// What a load of `tables` into `network` answers.
fn load(network: *mut Network, tables: &Tables) -> StatusCode {
    let problem = tables.problem();

    // SAFETY: the network is live, the problem points into `tables`, and no refusal is asked for.
    unsafe { kahlnws_network_load(network, &raw const problem, ptr::null_mut()) }
}

#[test]
fn test_the_same_problem_loaded_again_makes_no_pivot_where_a_fresh_network_makes_some() {
    let network = solved_network(&depot());

    assert_eq!(load(network, &depot()), StatusCode::OK);
    assert_eq!(network_total_cost(network).0, StatusCode::NO_OPTIMUM, "a load ends the optimum");
    assert_eq!(network_solve(network), (StatusCode::OPTIMAL, Int128 { low: 14, high: 0 }, Work::default()));
    let fresh = loaded(&depot());
    assert_eq!(network_solve(fresh).2, NETWORK_DEPOT_WORK, "a fresh network starts from the star");

    free_network(fresh);
    free_network(network);
}

#[test]
fn test_a_load_of_one_unit_less_starts_from_the_basis_held_by_hand() {
    // The depot's optimum hangs nodes 1 and 2 from node 0 by arcs 1 and 0, arc 2 full out of the tree. With 3 units,
    // arc 2 stays full, node 1 passes its 2 on and node 2 takes 1 more by arc 0: 1, 2 and 2 units at 5 + 2 + 2 = 9, the
    // optimum, which the kept basis carries with no pivot; a fresh network makes four.
    let network = solved_network(&depot());
    let fewer = Tables { supplies: vec![3, 0, -3], ..depot() };

    assert_eq!(load(network, &fewer), StatusCode::OK);
    assert_eq!(network_solve(network), (StatusCode::OPTIMAL, Int128 { low: 9, high: 0 }, Work::default()));
    assert_eq!(network_flows(network, 3), (StatusCode::OK, vec![1, 2, 2]));
    let fresh = loaded(&fewer);
    assert_ne!(network_solve(fresh).2, Work::default(), "a fresh network pivots");

    free_network(fresh);
    free_network(network);
}

#[test]
fn test_a_load_of_other_counts_or_ends_answers_as_a_fresh_network() {
    let turned = Tables { tails: vec![0, 1, 1], heads: vec![2, 0, 2], ..depot() };

    for other in [chain(), turned] {
        let network = solved_network(&depot());
        let fresh = loaded(&other);

        assert_eq!(load(network, &other), StatusCode::OK);
        assert_eq!(network_solve(network), network_solve(fresh), "{other:?}");

        free_network(fresh);
        free_network(network);
    }
}

#[test]
fn test_a_refused_load_between_two_loads_keeps_the_basis() {
    let network = solved_network(&depot());
    let mut refusal = Refusal::default();
    let unbalanced = Tables { supplies: vec![4, 0, -3], ..depot() };
    let problem = unbalanced.problem();

    // SAFETY: the network is live, the problem points into `unbalanced`, and the refusal is the test's own.
    let status = unsafe { kahlnws_network_load(network, &raw const problem, &raw mut refusal) };
    assert_eq!((status, refusal.kind), (StatusCode::REFUSED, RefusalKind::UNBALANCED));
    assert_eq!(network_total_cost(network), (StatusCode::OK, Int128 { low: 14, high: 0 }), "the optimum stays");

    assert_eq!(load(network, &depot()), StatusCode::OK);
    assert_eq!(network_solve(network).2, Work::default(), "the next load starts from the basis kept");

    free_network(network);
}
