// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! One column of a problem, copied out of the Python argument that holds it into a column the solver owns.
//!
//! A column is a buffer of integers or a sequence. A buffer is read through a `memoryview` of it, which states its
//! shape, its strides and its format for any exporter, and its bytes are copied by the interpreter's own copy into
//! memory the solver owns; a sequence is read entry by entry, each converted from its Python object. Either way its
//! length is known before anything is copied, and the room for it is reserved first, where a refusal is raised: no
//! argument makes the solver's memory grow without bound or ends the process. Nothing here keeps a reference to
//! memory Python owns, so what the solve reads is the solver's copy alone.

use std::collections::TryReserveError;

use bytemuck::Pod;
use pyo3::buffer::{PyBuffer, PyUntypedBuffer};
use pyo3::conversion::FromPyObjectOwned;
use pyo3::exceptions::PyTypeError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyMapping, PyMemoryView, PyTuple};

use crate::argument::{Argument, ArgumentError, Input, IntegerValue, raised_reading, type_name};
use crate::integer::{self, Converted};
use crate::items::{Items, Width};

/// An integer type a column of a problem is held in, with the items a buffer states where it holds exactly this type.
pub(crate) trait Held: Pod + Default + TryFrom<i128> {
    /// The items of a buffer of exactly this type, in the machine's byte order.
    const ITEMS: Items;
}

impl Held for usize {
    const ITEMS: Items = Items::Integers { signed: false, width: width_of(size_of::<Self>()) };
}

impl Held for u64 {
    const ITEMS: Items = Items::Integers { signed: false, width: Width::Eight };
}

impl Held for i64 {
    const ITEMS: Items = Items::Integers { signed: true, width: Width::Eight };
}

/// The width of an integer of `bytes` bytes.
///
/// # Panics
/// Panics, at compile time where it is a constant, if no integer item has that width.
const fn width_of(bytes: usize) -> Width {
    match Width::of(bytes) {
        Some(width) => width,
        None => panic!("called `width_of` with a size no integer item has"),
    }
}

/// The room a buffer of other items than its column's is copied into before each item is converted, with the reads
/// of a column that use it.
///
/// A column of `u64` words, so that a view of it as any integer type of at most 8 bytes is aligned. It is kept from
/// one read to the next and only grows; its contents mean nothing between reads.
#[derive(Debug, Default)]
pub(crate) struct Staging {
    /// The staged items, packed from the start, with up to 7 bytes left over at the end.
    words: Vec<u64>,
}

/// An entry of a staged buffer outside the range of the column it is read into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OutOfRange {
    /// The entry's position in the buffer, from 0.
    position: usize,
    /// The entry.
    value: i128,
}

/// The shape of a buffer of one dimension that a column is read from, as its export states it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    /// Its number of items.
    count: usize,
    /// Its bytes, `count` times the item size.
    bytes: usize,
    /// Whether its items lie next to each other, in order.
    contiguous: bool,
}

impl Shape {
    /// The shape of `buffer`, of one dimension; of no item where its items have no size, which no column reads.
    fn of(buffer: &PyUntypedBuffer) -> Self {
        let bytes = buffer.len_bytes();
        let count = bytes.checked_div(buffer.item_size()).unwrap_or(0);

        Self { count, bytes, contiguous: buffer.is_c_contiguous() }
    }
}

/// The number of dimensions of the buffer `view` shows, as the view states it: an export of no dimension states no
/// shape, which the buffer's typed record refuses to hold.
///
/// # Errors
/// Passes on what reading the attribute raises, which a `memoryview` never does.
pub(crate) fn dimensions_of(view: &Bound<'_, PyMemoryView>) -> PyResult<usize> {
    view.getattr(intern!(view.py(), "ndim"))?.extract()
}

impl Staging {
    /// Makes room for buffers of up to `entries` items: a read of one allocates nothing afterwards. Room already held
    /// is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of a refused reservation.
    pub(crate) fn try_reserve(&mut self, entries: usize) -> Result<(), TryReserveError> {
        // An item has at most the 8 bytes of a word, so `entries` words hold any buffer of `entries` items.
        self.words.clear();
        self.words.try_reserve(entries)
    }

    /// Reads `argument`, the object Python gave for `input`, into `column`, which afterwards holds its entries in
    /// order and nothing else.
    ///
    /// A list, a tuple, and any other object that exports no buffer but is a sequence (it has a length and is indexed
    /// by position, and is no mapping) is read entry by entry, each converted from a Python integer with a range
    /// check. A buffer of one dimension of integers in the machine's byte order is copied: into `column` where its
    /// items are exactly `T`, else into the staging room, and then converted item by item with a range check; a
    /// buffer of booleans is read as 0 and 1, and a buffer of Python objects as the sequence it is.
    ///
    /// # Errors
    /// Returns the [`ArgumentError`], as a Python exception, of an argument that is neither a buffer nor a sequence,
    /// of a buffer of other than one dimension or of items no column reads, of the first entry that is no integer or
    /// lies outside the range of `T`, and of a column whose room is refused. Passes on, with a note that names the
    /// argument, what Python raises while the argument is exported, measured or indexed, or an entry's `__index__`
    /// runs.
    pub(crate) fn read<'py, T>(
        &mut self,
        input: Input,
        argument: &Bound<'py, PyAny>,
        column: &mut Vec<T>,
    ) -> PyResult<()>
    where
        T: Held + FromPyObjectOwned<'py>,
    {
        column.clear();

        // A list or a tuple exports no buffer: asking for one first would only raise and discard an exception.
        if argument.is_instance_of::<PyList>() || argument.is_instance_of::<PyTuple>() {
            return read_sequence(input, argument, column);
        }

        let py = argument.py();
        match PyMemoryView::from(argument) {
            Ok(view) => self.read_view(input, argument, &view, column),
            Err(error) if error.is_instance_of::<PyTypeError>(py) => read_sequence(input, argument, column),
            Err(error) => Err(raised_reading(py, error, input)),
        }
    }

    /// Reads the buffer `view` shows, exported by `argument`, the object Python gave for `input`, into `column`,
    /// which is empty.
    ///
    /// # Errors
    /// As [`read`](Self::read), for a buffer.
    fn read_view<'py, T>(
        &mut self,
        input: Input,
        argument: &Bound<'py, PyAny>,
        view: &Bound<'py, PyMemoryView>,
        column: &mut Vec<T>,
    ) -> PyResult<()>
    where
        T: Held + FromPyObjectOwned<'py>,
    {
        let py = view.py();
        let noted = |error| raised_reading(py, error, input);
        let dimensions = dimensions_of(view).map_err(noted)?;
        if dimensions != 1 {
            return Err(ArgumentError::InputDimensions { input, dimensions }.into());
        }

        let buffer = PyUntypedBuffer::get(view.as_any()).map_err(noted)?;
        let items = Items::of(buffer.format(), buffer.item_size());
        let written = || buffer.format().to_string_lossy().into_owned();
        let shape = Shape::of(&buffer);

        let flag = |byte: u8| i128::from(byte != 0);
        match items {
            Items::Objects => read_sequence(input, argument, column),
            Items::ForeignOrder => {
                Err(ArgumentError::ForeignOrder { argument: Argument::Input(input), format: written() }.into())
            }
            Items::Empty => Err(ArgumentError::EmptyItems { argument: Argument::Input(input) }.into()),
            Items::Other => Err(ArgumentError::ItemsNotIntegers { input, format: written() }.into()),
            _ if shape.count == 0 => Ok(()),
            _ if items == T::ITEMS => copy_exact(input, view, shape, column),
            Items::Booleans => self.stage(input, view, shape, column, flag),
            Items::Integers { signed: true, width: Width::One } => {
                self.stage::<i8, _>(input, view, shape, column, i128::from)
            }
            Items::Integers { signed: true, width: Width::Two } => {
                self.stage::<i16, _>(input, view, shape, column, i128::from)
            }
            Items::Integers { signed: true, width: Width::Four } => {
                self.stage::<i32, _>(input, view, shape, column, i128::from)
            }
            Items::Integers { signed: true, width: Width::Eight } => {
                self.stage::<i64, _>(input, view, shape, column, i128::from)
            }
            Items::Integers { signed: false, width: Width::One } => {
                self.stage::<u8, _>(input, view, shape, column, i128::from)
            }
            Items::Integers { signed: false, width: Width::Two } => {
                self.stage::<u16, _>(input, view, shape, column, i128::from)
            }
            Items::Integers { signed: false, width: Width::Four } => {
                self.stage::<u32, _>(input, view, shape, column, i128::from)
            }
            Items::Integers { signed: false, width: Width::Eight } => {
                self.stage::<u64, _>(input, view, shape, column, i128::from)
            }
        }
    }

    /// Copies the bytes of the buffer `view` shows, of `shape` and of items of type `S`, into the staging room, and
    /// appends each item to `column`, which is empty, as the integer `value` makes of it, converted to `T`.
    ///
    /// # Errors
    /// Returns [`ArgumentError::NoRoom`] where the room for the column or for its staged items is refused, and
    /// [`ArgumentError::EntryOutOfRange`] for the first item outside the range of `T`. Passes on, with a note, what
    /// the copy raises.
    fn stage<S, T>(
        &mut self,
        input: Input,
        view: &Bound<'_, PyMemoryView>,
        shape: Shape,
        column: &mut Vec<T>,
        value: impl Fn(S) -> i128,
    ) -> PyResult<()>
    where
        S: Pod,
        T: TryFrom<i128>,
    {
        let words = shape.bytes.div_ceil(size_of::<u64>());
        self.words.clear();
        self.words.try_reserve(words).map_err(|_| ArgumentError::NoRoom { input, length: shape.count })?;
        self.words.resize(words, 0);
        copy_bytes(input, view, shape, &mut bytemuck::cast_slice_mut::<u64, u8>(&mut self.words)[..shape.bytes])?;

        // A word's 8 bytes are a whole number of items of any width up to 8, so the cast cannot fail, and the buffer's
        // bytes are `count` items of `S`, which the words hold.
        let staged = &bytemuck::cast_slice::<u64, S>(&self.words)[..shape.count];
        reserve_entries(input, column, shape.count)?;
        convert(staged, column, value).map_err(|OutOfRange { position, value }| {
            ArgumentError::EntryOutOfRange { input, position, value: IntegerValue::Integer(value) }.into()
        })
    }
}

/// Copies the bytes of the buffer `view` shows, of `shape` and of items exactly of type `T`, into `column`, which is
/// empty and afterwards holds them.
///
/// # Errors
/// Returns [`ArgumentError::NoRoom`] where the room for the column is refused; passes on, with a note, what the copy
/// raises.
fn copy_exact<T: Held>(
    input: Input,
    view: &Bound<'_, PyMemoryView>,
    shape: Shape,
    column: &mut Vec<T>,
) -> PyResult<()> {
    reserve_entries(input, column, shape.count)?;
    column.resize(shape.count, T::default());

    copy_bytes(input, view, shape, bytemuck::cast_slice_mut::<T, u8>(column))
}

/// Copies the bytes of the buffer `view` shows, of `shape`, into `target`, which has as many, by the interpreter's
/// own copy: of a byte view of it where it is contiguous, else of the contiguous copy the interpreter makes of it.
/// No alignment is needed, so a buffer whose items are not aligned for their type is read as any other.
///
/// # Errors
/// Passes on, with a note that names `input`, what the byte view, the contiguous copy or the copy raises.
fn copy_bytes(input: Input, view: &Bound<'_, PyMemoryView>, shape: Shape, target: &mut [u8]) -> PyResult<()> {
    let py = view.py();
    let noted = |error| raised_reading(py, error, input);

    let bytes = match shape.contiguous {
        true => view.call_method1(intern!(py, "cast"), (intern!(py, "B"),)),
        false => view.call_method0(intern!(py, "tobytes")),
    }
    .map_err(noted)?;

    PyBuffer::<u8>::get(&bytes).and_then(|bytes| bytes.copy_to_slice(py, target)).map_err(noted)
}

/// Makes room in `column`, empty, for `length` entries of the column read from `input`, so that a memory refused is
/// raised and does not end the process.
///
/// # Errors
/// Returns [`ArgumentError::NoRoom`] where the memory is refused.
fn reserve_entries<T>(input: Input, column: &mut Vec<T>, length: usize) -> Result<(), ArgumentError> {
    column.clear();
    column.try_reserve(length).map_err(|_| ArgumentError::NoRoom { input, length })
}

/// Appends each of `staged` to `column`, as the integer `value` makes of it, converted to `T`, up to the first that
/// lies outside the range of `T`.
///
/// # Errors
/// Returns the position and value of the first item outside the range of `T`; `column` then holds the items before
/// it.
fn convert<S, T>(staged: &[S], column: &mut Vec<T>, value: impl Fn(S) -> i128) -> Result<(), OutOfRange>
where
    S: Copy,
    T: TryFrom<i128>,
{
    staged.iter().enumerate().try_for_each(|(position, &item)| {
        let value = value(item);
        column.push(T::try_from(value).map_err(|_| OutOfRange { position, value })?);

        Ok(())
    })
}

/// Reads `argument`, the object Python gave for `input`, through the sequence protocol into `column`: its length
/// first, then each entry by its position, converted from a Python integer.
///
/// # Errors
/// Returns [`ArgumentError::NotAColumn`] where `argument` is no sequence: it lacks a length or indexing, or is a
/// mapping. Returns [`ArgumentError::NoRoom`] where the room for its length is refused, and the error of the first
/// entry [`convert_entry`] refuses. Passes on, with a note, what its length or its indexing raises.
fn read_sequence<'py, T>(input: Input, argument: &Bound<'py, PyAny>, column: &mut Vec<T>) -> PyResult<()>
where
    T: FromPyObjectOwned<'py>,
{
    let py = argument.py();
    let noted = |error| raised_reading(py, error, input);
    let kind = argument.get_type();
    let sequence = kind.hasattr(intern!(py, "__len__")).map_err(noted)?
        && kind.hasattr(intern!(py, "__getitem__")).map_err(noted)?
        && !argument.is_instance_of::<PyMapping>();
    if !sequence {
        return Err(ArgumentError::NotAColumn { input, type_name: type_name(argument)? }.into());
    }

    let length = argument.len().map_err(noted)?;
    reserve_entries(input, column, length)?;
    for position in 0..length {
        let entry = argument.get_item(position).map_err(noted)?;
        column.push(convert_entry(input, position, &entry)?);
    }

    Ok(())
}

/// `entry`, at `position` of the object Python gave for `input`, as a `T`: an `int`, or an object whose type has
/// `__index__`, whose value is taken once and lies in the range of `T`.
///
/// # Errors
/// Returns [`ArgumentError::EntryNotAnInteger`] for an entry that is no `int` and whose type has no `__index__`, and
/// [`ArgumentError::EntryOutOfRange`], with the value taken, for one outside the range of `T`. Passes on, with a note
/// that names the argument, what its `__index__` raises.
fn convert_entry<'py, T>(input: Input, position: usize, entry: &Bound<'py, PyAny>) -> PyResult<T>
where
    T: FromPyObjectOwned<'py>,
{
    match integer::convert(entry).map_err(|error| raised_reading(entry.py(), error, input))? {
        Converted::Within(converted) => Ok(converted),
        Converted::Outside(value) => Err(ArgumentError::EntryOutOfRange { input, position, value }.into()),
        Converted::NotAnInteger => {
            Err(ArgumentError::EntryNotAnInteger { input, position, type_name: type_name(entry)? }.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use pyo3::types::PyDict;

    use super::*;

    /// Runs `test` with the interpreter embedded in this binary, attached.
    fn with_python(test: impl for<'py> FnOnce(Python<'py>) -> PyResult<()>) {
        Python::initialize();
        Python::attach(test).expect("the test's Python should run without an exception it does not expect");
    }

    /// The value of the Python expression `expression`.
    fn evaluate<'py>(py: Python<'py>, expression: &str) -> PyResult<Bound<'py, PyAny>> {
        let code = std::ffi::CString::new(expression).expect("an expression should hold no NUL");
        let globals = PyDict::new(py);
        globals.set_item("array", py.import("array")?.getattr("array")?)?;

        py.eval(&code, Some(&globals), None)
    }

    /// What `Staging::read` reads into a fresh column of `T` from the value of `expression`, given for `input`.
    fn read<'py, T: Held + FromPyObjectOwned<'py>>(
        py: Python<'py>,
        input: Input,
        expression: &str,
    ) -> PyResult<Vec<T>> {
        let mut column = vec![T::default(); 2];
        Staging::default().read(input, &evaluate(py, expression)?, &mut column)?;

        Ok(column)
    }

    /// The message of `error` and whether it is of the Python type `E`.
    fn message_of<E: pyo3::PyTypeInfo>(py: Python<'_>, error: &PyErr) -> (bool, String) {
        (error.is_instance_of::<E>(py), error.value(py).to_string())
    }

    #[test]
    fn test_convert_copies_items_within_range_by_hand() {
        let mut column: Vec<u64> = vec![];
        assert_eq!(convert(&[0_i8, 7, 127], &mut column, i128::from), Ok(()));
        assert_eq!(column, [0, 7, 127]);

        let mut column: Vec<i64> = vec![];
        assert_eq!(convert(&[u32::MAX, 0], &mut column, i128::from), Ok(()));
        assert_eq!(column, [4_294_967_295, 0]);

        let mut column: Vec<usize> = vec![];
        assert_eq!(convert(&[i64::MAX, 3], &mut column, i128::from), Ok(()));
        assert_eq!(column, [9_223_372_036_854_775_807, 3]);

        let mut column: Vec<u64> = vec![];
        assert_eq!(convert(&[0_u8, 1, 2, 255], &mut column, |byte: u8| i128::from(byte != 0)), Ok(()));
        assert_eq!(column, [0, 1, 1, 1], "a boolean byte is 0 or 1, as `bool` reads it");
    }

    #[test]
    fn test_convert_stops_at_the_first_item_out_of_range_by_hand() {
        let mut column: Vec<u64> = vec![];
        assert_eq!(convert(&[3_i64, -1, -2], &mut column, i128::from), Err(OutOfRange { position: 1, value: -1 }));
        assert_eq!(column, [3], "the items before the first out of range");

        let mut column: Vec<i64> = vec![];
        let past = u64::try_from(i64::MAX).expect("i64::MAX should be a u64") + 1;
        assert_eq!(convert(&[0, past], &mut column, i128::from), Err(OutOfRange { position: 1, value: 1 << 63 }));
    }

    #[test]
    fn test_reserve_entries_empties_the_column_and_refuses_a_length_past_any_allocation() {
        let mut column: Vec<u64> = vec![1, 2];
        assert_eq!(reserve_entries(Input::Costs, &mut column, 5), Ok(()));
        assert!(column.is_empty());
        assert!(column.capacity() >= 5);

        assert_eq!(
            reserve_entries(Input::Costs, &mut column, usize::MAX),
            Err(ArgumentError::NoRoom { input: Input::Costs, length: usize::MAX })
        );
    }

    #[test]
    fn test_try_reserve_makes_room_for_a_buffer_of_that_many_words() {
        let mut staging = Staging::default();
        assert_eq!(staging.try_reserve(5), Ok(()));

        assert!(staging.words.capacity() >= 5);
        assert!(staging.words.is_empty());
    }

    #[test]
    fn test_read_copies_buffers_of_the_column_s_type_and_converts_the_others_by_hand() {
        with_python(|py| {
            assert_eq!(read::<u64>(py, Input::Capacities, "array('Q', [3, 4, 2])")?, [3, 4, 2]);
            assert_eq!(read::<i64>(py, Input::Costs, "array('b', [5, -1, 1])")?, [5, -1, 1]);
            assert_eq!(read::<usize>(py, Input::Tails, "array('I', [0, 0, 1])")?, [0, 0, 1]);
            assert_eq!(read::<u64>(py, Input::Capacities, "memoryview(array('Q', [3, 9, 4, 9, 2]))[::2]")?, [3, 4, 2]);
            assert_eq!(read::<i64>(py, Input::Supplies, "memoryview(bytes([1, 0, 1])).cast('?')")?, [1, 0, 1]);
            assert_eq!(read::<u64>(py, Input::Capacities, "array('Q')")?, Vec::<u64>::new());

            Ok(())
        });
    }

    #[test]
    fn test_read_reads_sequences_entry_by_entry_by_hand() {
        with_python(|py| {
            assert_eq!(read::<u64>(py, Input::Capacities, "[3, 4, True]")?, [3, 4, 1]);
            assert_eq!(read::<i64>(py, Input::Costs, "(5, -1, 1)")?, [5, -1, 1]);
            assert_eq!(read::<usize>(py, Input::Tails, "range(3)")?, [0, 1, 2]);
            assert_eq!(read::<u64>(py, Input::Capacities, "[]")?, Vec::<u64>::new());

            Ok(())
        });
    }

    #[test]
    fn test_read_refuses_what_is_no_column_with_the_argument_s_name_by_hand() {
        with_python(|py| {
            let cases: [(&str, &str); 4] = [
                ("{1, 2}", "heads is of type set, neither a sequence nor a buffer of integers"),
                ("{1: 2}", "heads is of type dict, neither a sequence nor a buffer of integers"),
                ("(x for x in [1])", "heads is of type generator, neither a sequence nor a buffer of integers"),
                ("1.5", "heads is of type float, neither a sequence nor a buffer of integers"),
            ];
            for (expression, message) in cases {
                let error = read::<usize>(py, Input::Heads, expression).expect_err(expression);
                assert_eq!(message_of::<PyTypeError>(py, &error), (true, message.to_owned()), "{expression}");
            }

            Ok(())
        });
    }

    #[test]
    fn test_read_refuses_buffers_of_items_no_column_reads_by_hand() {
        with_python(|py| {
            let cases: [(&str, &str); 3] = [
                ("array('d', [1.0])", "costs holds items of format 'd', not integers"),
                ("memoryview(b'ab').cast('c')", "costs holds items of format 'c', not integers"),
                ("memoryview(bytes(8)).cast('B').cast('Q', (1, 1))", "costs has 2 dimensions, not 1"),
            ];
            for (expression, message) in cases {
                let error = read::<i64>(py, Input::Costs, expression).expect_err(expression);
                assert_eq!(error.value(py).to_string(), message, "{expression}");
            }

            Ok(())
        });
    }

    #[test]
    fn test_convert_entry_takes_the_index_once_and_names_what_it_refuses_by_hand() {
        with_python(|py| {
            let entry = |expression: &str| evaluate(py, expression);
            assert_eq!(convert_entry::<u64>(Input::Capacities, 0, &entry("7")?)?, 7);
            assert_eq!(convert_entry::<i64>(Input::Costs, 0, &entry("-(2**63)")?)?, i64::MIN);

            let error = convert_entry::<u64>(Input::Capacities, 2, &entry("-1")?).expect_err("below zero");
            let message =
                format!("capacities[2] is -1, outside the range of an unsigned 64-bit integer, from 0 to {}", u64::MAX);
            assert_eq!(message_of::<pyo3::exceptions::PyOverflowError>(py, &error), (true, message));

            let error = convert_entry::<i64>(Input::Costs, 1, &entry("10**5000")?).expect_err("too long to print");
            assert!(error.value(py).to_string().starts_with("costs[1] is 2**16609 or more, outside"));

            let error = convert_entry::<i64>(Input::Costs, 0, &entry("1.5")?).expect_err("no index");
            assert_eq!(error.value(py).to_string(), "costs[0] is of type float, not an integer");

            Ok(())
        });
    }
}
