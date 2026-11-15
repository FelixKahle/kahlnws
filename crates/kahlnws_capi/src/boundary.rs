// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The one place pointers from C are read or written: each is checked in every build for null, for its alignment
//! and for a count of entries whose bytes pass `isize::MAX`, and only then turned into a reference or a slice for
//! the length the call states; and the solver's heap block is made and freed here.
//!
//! No check can prove what a C pointer points to, so every helper that reads one is an `unsafe fn` whose `# Safety`
//! states what C must guarantee. An output is lent as `MaybeUninit` storage: it is written and never read, so C need
//! not have given it values. A pointer that cannot be read is answered as a [`PointerFault`], never read.

use core::ffi::c_char;
use core::mem::{MaybeUninit, size_of};
use core::ptr;
use core::slice;
use std::alloc::{self, Layout};

use crate::halves::Int128;
use crate::problem::Problem;
use crate::refusal::Refusal;
use crate::solver::{Call, Solver};
use crate::status::PointerFault;
use crate::work::Work;

/// The layout of the heap block of one solver: the block [`solver_handle`] allocates and [`free_solver`] frees as a
/// `Box<Solver>`, whose layout it is.
const SOLVER_LAYOUT: Layout = Layout::new::<Solver>();

// The global allocator may be asked only for a block of at least one byte.
const _: () = assert!(SOLVER_LAYOUT.size() > 0, "a solver should take at least one byte");

/// A new solver that holds no room, in a heap block of its own, as the pointer C holds; null where the memory for the
/// block is refused. [`free_solver`] frees it.
pub(crate) fn solver_handle() -> *mut Solver {
    // SAFETY: the layout has a size of at least one byte, as the assertion beside `SOLVER_LAYOUT` checks.
    let block = unsafe { alloc::alloc(SOLVER_LAYOUT) }.cast::<Solver>();
    if block.is_null() {
        return ptr::null_mut();
    }

    // SAFETY: `block` is not null and was just allocated with the layout of one solver, so it is valid for writing
    // one, aligned, and nothing else holds it.
    unsafe { block.write(Solver::new()) };

    block
}

/// Frees the solver `solver` points to, with its room; nothing where `solver` is null or not aligned for a solver,
/// which no pointer `kahlnws_solver_new` answered is.
///
/// # Safety
/// `solver` is null, or a pointer `kahlnws_solver_new` answered that no call of `kahlnws_solver_free` has freed since
/// and that no other call uses meanwhile.
pub(crate) unsafe fn free_solver(solver: *mut Solver) {
    if checked(solver.cast_const(), 1).is_err() {
        return;
    }

    // SAFETY: C guarantees `solver` came from `solver_handle`, a block allocated with the layout of one solver and
    // holding one, which is the layout a `Box<Solver>` frees; not freed since and not in use.
    drop(unsafe { Box::from_raw(solver) });
}

/// The solver `solver` points to, lent for the call.
///
/// # Errors
/// Returns [`PointerFault::Null`] where `solver` is null, and [`PointerFault::Invalid`] where it is not aligned for
/// a solver.
///
/// # Safety
/// `solver` is null, or a pointer `kahlnws_solver_new` answered that no call of `kahlnws_solver_free` has freed since
/// and that no other call uses meanwhile.
pub(crate) unsafe fn solver_mut<'a>(solver: *mut Solver) -> Result<&'a mut Solver, PointerFault> {
    checked(solver.cast_const(), 1)?;

    // SAFETY: `solver` is aligned and not null, and C guarantees it is a live solver of `solver_handle` that nothing
    // else uses.
    Ok(unsafe { &mut *solver })
}

/// The call of `kahlnws_solver_solve` or `kahlnws_solver_resolve` with these pointers: the problem `problem` points
/// to, its tables lent, and the outputs, each lent as storage to be written for the length the problem's counts
/// state; the outputs C passes as null are not wanted.
///
/// # Errors
/// Returns [`PointerFault::Null`] where `problem` is null, or `flows` or a table of the problem other than its lower
/// bounds is null while its count is above zero; [`PointerFault::Invalid`] where a pointer that is read or written is
/// not aligned for its type, or its count's bytes pass `isize::MAX`.
///
/// # Safety
/// `problem` is null or points to a `kahlnws_problem` whose every pointer points to what its doc states, which
/// nothing writes during the call. `flows` points to storage for `arc_count` entries, `potentials` is null or points
/// to storage for `node_count` entries, and `total_cost`, `work` and `refusal` are null or point to storage for one
/// value each: writable, within one object, and neither read nor written by anything else during the call; their
/// contents do not matter. No output overlaps another, the problem or its tables.
pub(crate) unsafe fn call<'a>(
    problem: *const Problem,
    flows: *mut u64,
    potentials: *mut i64,
    total_cost: *mut Int128,
    work: *mut Work,
    refusal: *mut Refusal,
) -> Result<Call<'a>, PointerFault> {
    // SAFETY: C guarantees `problem` is null or points to a `kahlnws_problem` that nothing writes meanwhile.
    let problem = unsafe { value_ref(problem) }?;
    let (node_count, arc_count) = (problem.node_count, problem.arc_count);
    // SAFETY: C guarantees each table of the problem points to the entries its count states, or is null with a count
    // of zero, or is the lower bounds and null, and that nothing writes them meanwhile.
    let tables = unsafe { tables_of(problem) }?;

    // SAFETY: C guarantees `flows` points to writable storage for `arc_count` entries of its own.
    let flows = unsafe { output_table(flows, arc_count) }?;
    // SAFETY: C guarantees `potentials` is null or points to writable storage for `node_count` entries of its own.
    let potentials = unsafe { optional_output_table(potentials, node_count) }?;
    // SAFETY: C guarantees `total_cost` is null or points to writable storage for one value of its own.
    let total_cost = unsafe { optional_output(total_cost) }?;
    // SAFETY: C guarantees `work` is null or points to writable storage for one value of its own.
    let work = unsafe { optional_output(work) }?;
    // SAFETY: C guarantees `refusal` is null or points to writable storage for one value of its own.
    let refusal = unsafe { optional_output(refusal) }?;

    Ok(Call::new(tables, flows, potentials, total_cost, work, refusal))
}

/// The refusal `refusal` points to, lent for the call.
///
/// # Errors
/// Returns [`PointerFault::Null`] where `refusal` is null, and [`PointerFault::Invalid`] where it is not aligned for
/// a refusal.
///
/// # Safety
/// `refusal` is null, or points to an initialised `kahlnws_refusal` that nothing writes during the call.
pub(crate) unsafe fn refusal_ref<'a>(refusal: *const Refusal) -> Result<&'a Refusal, PointerFault> {
    // SAFETY: C guarantees `refusal` is null or points to an initialised refusal nothing writes meanwhile.
    unsafe { value_ref(refusal) }
}

/// The `capacity` bytes from `buffer`, lent as storage to be written for the call; empty where `buffer` is null or
/// `capacity` is zero.
///
/// # Errors
/// Returns [`PointerFault::Invalid`] where `capacity` passes `isize::MAX` bytes.
///
/// # Safety
/// `buffer` is null, or `capacity` is zero, or `buffer` points to writable storage for `capacity` bytes within one
/// object that nothing else reads or writes during the call and that overlaps no other argument of the call; their
/// contents do not matter.
pub(crate) unsafe fn bytes_mut<'a>(
    buffer: *mut c_char,
    capacity: usize,
) -> Result<&'a mut [MaybeUninit<u8>], PointerFault> {
    // SAFETY: C guarantees `buffer` is null or points to writable storage for `capacity` bytes of its own; a `char`
    // has the size and alignment of a byte.
    let bytes = unsafe { optional_output_table(buffer.cast::<u8>(), capacity) }?;

    Ok(bytes.unwrap_or_default())
}

/// Whether `pointer` may be read for `count` entries: not null, aligned for `T`, and `count` entries of at most
/// `isize::MAX` bytes. It reads nothing.
///
/// # Errors
/// Returns [`PointerFault::Null`] where `pointer` is null, and [`PointerFault::Invalid`] where it is not aligned or
/// the bytes pass `isize::MAX`.
fn checked<T>(pointer: *const T, count: usize) -> Result<(), PointerFault> {
    let bytes = count.checked_mul(size_of::<T>()).and_then(|bytes| isize::try_from(bytes).ok());

    match (pointer.is_null(), pointer.is_aligned(), bytes) {
        (true, _, _) => Err(PointerFault::Null),
        (false, true, Some(_)) => Ok(()),
        (false, _, _) => Err(PointerFault::Invalid),
    }
}

/// The value `pointer` points to, lent for the call.
///
/// # Errors
/// As [`checked`] for one entry.
///
/// # Safety
/// `pointer` is null, or points to an initialised value that nothing writes during the call.
unsafe fn value_ref<'a, T>(pointer: *const T) -> Result<&'a T, PointerFault> {
    checked(pointer, 1)?;

    // SAFETY: `pointer` is aligned and not null, and C guarantees it points to an initialised value nothing writes
    // meanwhile.
    Ok(unsafe { &*pointer })
}

/// The tables of `problem`, lent as the library's problem.
///
/// # Errors
/// As [`table`] for each table, the lower bounds as [`optional_table`].
///
/// # Safety
/// Every pointer of `problem` points to what the doc of `kahlnws_problem` states, and nothing writes the tables
/// during the call.
unsafe fn tables_of<'a>(problem: &Problem) -> Result<kahlnws::Problem<'a>, PointerFault> {
    let (nodes, arcs) = (problem.node_count, problem.arc_count);

    // SAFETY: C guarantees each table points to the entries its count states, initialised, in one object that
    // nothing writes during the call; or is null with a count of zero, or is the lower bounds and null.
    unsafe {
        Ok(kahlnws::Problem {
            tails: table(problem.tails, arcs)?,
            heads: table(problem.heads, arcs)?,
            lower_bounds: optional_table(problem.lower_bounds, arcs)?,
            capacities: table(problem.capacities, arcs)?,
            costs: table(problem.costs, arcs)?,
            supplies: table(problem.supplies, nodes)?,
        })
    }
}

/// The `length` entries from `start`, lent for the call: empty, without reading `start`, where `length` is zero.
///
/// # Errors
/// Where `length` is above zero, as [`checked`].
///
/// # Safety
/// Where `length` is above zero, `start` is null or points to `length` initialised entries within one object that
/// nothing writes during the call.
unsafe fn table<'a, T>(start: *const T, length: usize) -> Result<&'a [T], PointerFault> {
    if length == 0 {
        return Ok(&[]);
    }
    checked(start, length)?;

    // SAFETY: `start` is aligned and not null, the entries span at most `isize::MAX` bytes, and C guarantees they are
    // `length` initialised entries in one object, unwritten meanwhile.
    Ok(unsafe { slice::from_raw_parts(start, length) })
}

/// The `length` entries from `start` as [`table`] lends them; `None` where `start` is null, whatever `length` is.
///
/// # Errors
/// As [`table`] for a pointer that is not null.
///
/// # Safety
/// As [`table`].
unsafe fn optional_table<'a, T>(start: *const T, length: usize) -> Result<Option<&'a [T]>, PointerFault> {
    if start.is_null() {
        return Ok(None);
    }

    // SAFETY: C guarantees what `table` asks of `start`.
    unsafe { table(start, length) }.map(Some)
}

/// Storage for one value at `pointer`, lent to be written; `None` where `pointer` is null.
///
/// # Errors
/// Returns [`PointerFault::Invalid`] where `pointer` is not aligned for `T`.
///
/// # Safety
/// `pointer` is null, or points to writable storage for one value that nothing else reads or writes during the call.
unsafe fn optional_output<'a, T>(pointer: *mut T) -> Result<Option<&'a mut MaybeUninit<T>>, PointerFault> {
    match checked(pointer.cast_const(), 1) {
        Err(PointerFault::Null) => Ok(None),
        Err(fault) => Err(fault),
        // SAFETY: `pointer` is aligned and not null, and C guarantees it is writable storage for one value of its
        // own; as `MaybeUninit`, its contents need not be a value.
        Ok(()) => Ok(Some(unsafe { &mut *pointer.cast::<MaybeUninit<T>>() })),
    }
}

/// Storage for the `length` entries from `start`, lent to be written: empty, without touching `start`, where `length`
/// is zero.
///
/// # Errors
/// Where `length` is above zero, as [`checked`].
///
/// # Safety
/// Where `length` is above zero, `start` is null or points to writable storage for `length` entries within one
/// object that nothing else reads or writes during the call.
unsafe fn output_table<'a, T>(start: *mut T, length: usize) -> Result<&'a mut [MaybeUninit<T>], PointerFault> {
    if length == 0 {
        return Ok(&mut []);
    }
    checked(start.cast_const(), length)?;

    // SAFETY: `start` is aligned and not null, the entries span at most `isize::MAX` bytes, and C guarantees it is
    // writable storage for `length` entries in one object, of its own meanwhile; as `MaybeUninit`, its contents need
    // not be values.
    Ok(unsafe { slice::from_raw_parts_mut(start.cast::<MaybeUninit<T>>(), length) })
}

/// Storage for the `length` entries from `start` as [`output_table`] lends it; `None` where `start` is null, whatever
/// `length` is.
///
/// # Errors
/// As [`output_table`] for a pointer that is not null.
///
/// # Safety
/// As [`output_table`].
unsafe fn optional_output_table<'a, T>(
    start: *mut T,
    length: usize,
) -> Result<Option<&'a mut [MaybeUninit<T>]>, PointerFault> {
    if start.is_null() {
        return Ok(None);
    }

    // SAFETY: C guarantees what `output_table` asks of `start`.
    unsafe { output_table(start, length) }.map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::Status;

    /// A pointer one byte past `start`: never aligned for a type wider than a byte, and never read or written.
    fn misaligned<T>(start: *const T) -> *mut T {
        start.cast::<u8>().wrapping_add(1).cast::<T>().cast_mut()
    }

    #[test]
    fn test_checked_refuses_null_misaligned_and_oversized_pointers_by_hand() {
        let entries = [0_u64; 2];
        let start = entries.as_ptr();
        let largest = usize::try_from(isize::MAX).expect("isize::MAX should fit a usize") / 8;

        assert_eq!(checked(start, 2), Ok(()));
        assert_eq!(checked(start, largest), Ok(()), "exactly isize::MAX bytes or fewer, never read");
        assert_eq!(checked(start, largest + 1), Err(PointerFault::Invalid), "past isize::MAX bytes");
        assert_eq!(checked(start, usize::MAX), Err(PointerFault::Invalid), "past usize::MAX bytes");
        assert_eq!(checked(ptr::null::<u64>(), 1), Err(PointerFault::Null));
        assert_eq!(checked(misaligned(entries.as_ptr()).cast_const(), 1), Err(PointerFault::Invalid));
    }

    #[test]
    fn test_a_table_is_lent_for_its_length() {
        let entries = [3_u64, 1, 4];

        // SAFETY: the pointer is the start of `entries`, which holds three, and outlives the slice.
        assert_eq!(unsafe { table(entries.as_ptr(), 3) }, Ok(&entries[..]));
        // SAFETY: the pointer is the start of `entries`, which holds more than two, and outlives the slice.
        assert_eq!(unsafe { table(entries.as_ptr(), 2) }, Ok(&entries[..2]));
    }

    #[test]
    fn test_a_table_of_no_entries_needs_no_storage() {
        // SAFETY: a length of zero reads no pointer, null or dangling.
        assert_eq!(unsafe { table(ptr::null::<u64>(), 0) }, Ok(&[][..]));
        // SAFETY: a length of zero reads no pointer, so a dangling one is never read either.
        assert_eq!(unsafe { table(ptr::dangling::<u64>(), 0) }, Ok(&[][..]));
        // SAFETY: a length of zero touches no pointer, so the null one is never written.
        assert!(unsafe { output_table(ptr::null_mut::<u64>(), 0) }.is_ok_and(|lent| lent.is_empty()));
    }

    #[test]
    fn test_a_table_that_cannot_be_read_is_a_pointer_fault() {
        let entries = [0_u64; 4];

        // SAFETY: a null pointer is checked and never read.
        assert_eq!(unsafe { table(ptr::null::<u64>(), 3) }, Err(PointerFault::Null));
        // SAFETY: a misaligned pointer is checked and never read.
        assert_eq!(unsafe { table(misaligned(entries.as_ptr()).cast_const(), 2) }, Err(PointerFault::Invalid));
        // SAFETY: a count past `isize::MAX` bytes is checked and never read.
        assert_eq!(unsafe { table(entries.as_ptr(), usize::MAX / 4) }, Err(PointerFault::Invalid));
        // SAFETY: a null pointer is checked and never written.
        assert!(matches!(unsafe { output_table(ptr::null_mut::<u64>(), 3) }, Err(PointerFault::Null)));
        // SAFETY: a misaligned pointer is checked and never written.
        assert!(matches!(unsafe { output_table(misaligned(entries.as_ptr()), 2) }, Err(PointerFault::Invalid)));
    }

    #[test]
    fn test_an_optional_table_is_none_only_where_null() {
        let entries = [2_i64, 7];

        // SAFETY: a null pointer is checked and never read.
        assert_eq!(unsafe { optional_table(ptr::null::<i64>(), 2) }, Ok(None));
        // SAFETY: a null pointer is checked and never written.
        assert!(matches!(unsafe { optional_output_table(ptr::null_mut::<i64>(), 2) }, Ok(None)));
        // SAFETY: the pointer is the start of `entries`, which holds two, and outlives the slice.
        assert_eq!(unsafe { optional_table(entries.as_ptr(), 2) }, Ok(Some(&[2, 7][..])));
        // SAFETY: a misaligned pointer is checked and never read.
        assert_eq!(unsafe { optional_table(misaligned(entries.as_ptr()).cast_const(), 1) }, Err(PointerFault::Invalid));
    }

    #[test]
    fn test_an_output_table_writes_through_storage_that_held_no_values() {
        let mut storage = [MaybeUninit::<u64>::uninit(); 3];

        // SAFETY: the pointer is the start of `storage`, which has room for three, outlives the slice and is not
        // otherwise used while it is held.
        let lent = unsafe { output_table(storage.as_mut_ptr().cast::<u64>(), 3) }.expect("the storage should be lent");
        lent.write_copy_of_slice(&[2, 2, 2]);

        // SAFETY: every entry of `storage` was just written.
        assert_eq!(unsafe { storage.assume_init_ref() }, &[2, 2, 2]);
    }

    #[test]
    fn test_an_optional_output_is_none_where_null_and_refused_where_misaligned() {
        let mut storage = [MaybeUninit::<Int128>::uninit(); 2];

        // SAFETY: a null pointer is checked and never written.
        assert!(matches!(unsafe { optional_output(ptr::null_mut::<Int128>()) }, Ok(None)));
        let misaligned = storage.as_mut_ptr().cast::<u8>().wrapping_add(1).cast::<Int128>();
        // SAFETY: a misaligned pointer is checked and never written.
        assert!(matches!(unsafe { optional_output(misaligned) }, Err(PointerFault::Invalid)));
        // SAFETY: the pointer is the start of `storage`, which has room for one value, and is not otherwise used.
        let lent = unsafe { optional_output(storage.as_mut_ptr().cast::<Int128>()) };
        lent.expect("the storage should be lent").expect("the pointer is not null").write(Int128 { low: 14, high: 0 });

        // SAFETY: the first entry of `storage` was just written.
        assert_eq!(unsafe { storage[0].assume_init() }, Int128 { low: 14, high: 0 });
    }

    #[test]
    fn test_the_bytes_of_a_buffer_are_empty_where_it_is_null_and_refused_past_isize_max() {
        let mut buffer = [MaybeUninit::<c_char>::uninit(); 4];

        // SAFETY: a null pointer is checked and never written.
        assert!(unsafe { bytes_mut(ptr::null_mut(), 4) }.is_ok_and(|bytes| bytes.is_empty()));
        // SAFETY: a capacity past `isize::MAX` is checked and the buffer never written.
        assert!(matches!(unsafe { bytes_mut(buffer.as_mut_ptr().cast(), usize::MAX) }, Err(PointerFault::Invalid)));
        // SAFETY: the pointer is the start of `buffer`, which has room for four bytes, outlives the slice and is not
        // otherwise used while it is held.
        let bytes = unsafe { bytes_mut(buffer.as_mut_ptr().cast(), 4) }.expect("the buffer should be lent");
        assert_eq!(bytes.len(), 4);
    }

    #[test]
    fn test_the_tables_of_a_problem_are_lent_and_one_that_cannot_be_read_refused() {
        let (tails, heads, supplies) = ([0_usize, 1], [1_usize, 2], [3_i64, 0, -3]);
        let (capacities, costs) = ([5_u64, 5], [1_i64, 2]);
        let problem = Problem {
            node_count: 3,
            arc_count: 2,
            tails: tails.as_ptr(),
            heads: heads.as_ptr(),
            lower_bounds: ptr::null(),
            capacities: capacities.as_ptr(),
            costs: costs.as_ptr(),
            supplies: supplies.as_ptr(),
        };

        // SAFETY: every table points to the entries its count states, and outlives the problem lent.
        let lent = unsafe { tables_of(&problem) }.expect("every table should be lent");
        assert_eq!((lent.tails, lent.heads, lent.lower_bounds), (&tails[..], &heads[..], None));
        assert_eq!((lent.capacities, lent.costs, lent.supplies), (&capacities[..], &costs[..], &supplies[..]));

        let lower_bounds_misaligned = capacities.as_ptr().cast::<u8>().wrapping_add(1).cast::<u64>();
        for (refused, fault) in [
            (Problem { tails: ptr::null(), ..problem }, PointerFault::Null),
            (Problem { heads: ptr::null(), ..problem }, PointerFault::Null),
            (Problem { capacities: ptr::null(), ..problem }, PointerFault::Null),
            (Problem { costs: ptr::null(), ..problem }, PointerFault::Null),
            (Problem { supplies: ptr::null(), ..problem }, PointerFault::Null),
            (Problem { tails: misaligned(tails.as_ptr()).cast_const(), ..problem }, PointerFault::Invalid),
            (Problem { lower_bounds: lower_bounds_misaligned, ..problem }, PointerFault::Invalid),
        ] {
            // SAFETY: every table but the faulty one points to the entries its count states and outlives the problem
            // lent; the faulty one is checked and never read.
            assert_eq!(unsafe { tables_of(&refused) }.map(|_| ()), Err(fault), "{refused:?}");
        }
    }

    #[test]
    fn test_a_problem_of_no_node_and_no_arc_needs_no_table() {
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

        // SAFETY: every count is zero, so no pointer is read.
        let lent = unsafe { tables_of(&problem) }.expect("tables of no entries need no pointer");
        assert!(lent.tails.is_empty() && lent.supplies.is_empty() && lent.lower_bounds.is_none());
    }

    #[test]
    fn test_a_solver_handle_is_lent_and_freed_and_null_or_misaligned_refused_and_ignored() {
        let handle = solver_handle();
        assert!(!handle.is_null());

        // SAFETY: the handle is the live solver `solver_handle` answered, used by nothing else.
        let solver = unsafe { solver_mut(handle) }.expect("the handle should be lent");
        assert_eq!(solver.reserve(2, 1), Status::Reserved);
        // SAFETY: the handle is not freed since, and the solver lent from it is no longer used.
        unsafe { free_solver(handle) };

        // SAFETY: a null pointer is checked and never read.
        assert!(matches!(unsafe { solver_mut(ptr::null_mut()) }, Err(PointerFault::Null)));
        let misaligned = ptr::dangling_mut::<Solver>().cast::<u8>().wrapping_add(1).cast::<Solver>();
        // SAFETY: a misaligned pointer is checked and never read.
        assert!(matches!(unsafe { solver_mut(misaligned) }, Err(PointerFault::Invalid)));
        // SAFETY: freeing null does nothing.
        unsafe { free_solver(ptr::null_mut()) };
        // SAFETY: a misaligned pointer is checked and never freed.
        unsafe { free_solver(misaligned) };
    }

    #[test]
    fn test_a_refusal_is_lent_and_null_refused() {
        let refusal = Refusal::default();

        // SAFETY: the pointer is to `refusal`, which outlives the reference and is not written meanwhile.
        assert_eq!(unsafe { refusal_ref(&raw const refusal) }, Ok(&refusal));
        // SAFETY: a null pointer is checked and never read.
        assert_eq!(unsafe { refusal_ref(ptr::null()) }, Err(PointerFault::Null));
    }
}
