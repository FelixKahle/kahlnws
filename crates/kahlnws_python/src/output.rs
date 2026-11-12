// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The buffers the caller lends a solve to write the columns of its answer to, checked before anything is written.

use pyo3::buffer::{Element, PyBuffer, PyUntypedBuffer};
use pyo3::exceptions::PyTypeError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::PyMemoryView;

use crate::argument::{Argument, ArgumentError, Output, raised_writing, type_name};
use crate::column::{Held, dimensions_of};
use crate::items::Items;

/// A buffer the caller lends for one column of the answer, checked to take it.
///
/// # Invariant
/// Only [`OutputBuffer::new`] makes one: the buffer has one dimension, is writable, holds items exactly of type `T`
/// in the machine's byte order, and holds the number of entries it was checked for. While it is held, the object
/// that exported it cannot be resized.
#[derive(Debug)]
pub(crate) enum OutputBuffer<T> {
    /// A buffer of no entries, for a column of none: nothing is written to it.
    Empty,
    /// A contiguous buffer, held as a view of its bytes, which the column's bytes are copied to whatever the
    /// alignment of its items and however its format names the machine's byte order.
    Contiguous {
        /// The argument it was given for.
        output: Output,
        /// The export of its bytes.
        bytes: PyBuffer<u8>,
    },
    /// A strided buffer whose items are aligned for `T` and whose format names the machine's byte order implicitly,
    /// held as a view of its items, which the column is copied to item by item.
    Strided {
        /// The argument it was given for.
        output: Output,
        /// The export of its items.
        items: PyBuffer<T>,
    },
}

/// The bytes of memory a buffer's items cover, from its lowest byte to one past its highest; empty for a buffer of no
/// item. Addresses are compared as numbers, with room for any stride.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Extent {
    /// The address of its lowest byte.
    start: i128,
    /// One past the address of its highest byte; `start` where it covers none.
    end: i128,
}

/// The two buffers of one call of `solve_into`, checked to take its answer: the flows always, the potentials where
/// asked for.
///
/// # Invariant
/// Only [`Outputs::new`] makes one: each buffer is checked as [`OutputBuffer::new`] checks it, and the two share no
/// byte of memory, so writing one leaves the other as it is.
#[derive(Debug)]
pub(crate) struct Outputs {
    /// Where each arc's flow goes.
    flows: OutputBuffer<u64>,
    /// Where each node's potential goes; `None` where the caller asked for none.
    potentials: Option<OutputBuffer<i64>>,
}

impl Outputs {
    /// The buffers of `flows`, for `arc_count` entries, and of `potentials`, for `node_count`, each checked, and
    /// checked to share no memory.
    ///
    /// # Errors
    /// Returns the errors of [`OutputBuffer::new`], the flows' first, and [`ArgumentError::Overlap`] where the two
    /// share memory.
    pub(crate) fn new(
        flows: &Bound<'_, PyAny>,
        potentials: Option<&Bound<'_, PyAny>>,
        arc_count: usize,
        node_count: usize,
    ) -> PyResult<Self> {
        let (flows, flows_extent) = OutputBuffer::new(Output::Flows, flows, arc_count)?;
        let Some(potentials) = potentials else {
            return Ok(Self { flows, potentials: None });
        };

        let (potentials, potentials_extent) = OutputBuffer::new(Output::Potentials, potentials, node_count)?;
        if flows_extent.overlaps(potentials_extent) {
            return Err(ArgumentError::Overlap.into());
        }

        Ok(Self { flows, potentials: Some(potentials) })
    }

    /// Writes `flows` and `potentials` to the buffers, by the interpreter's own copies; the potentials only where a
    /// buffer was given for them.
    ///
    /// # Errors
    /// Passes on, with a note that names the argument, what a copy raises, which an exporter that keeps the buffer
    /// protocol never makes it do once its buffer is checked: the flows may then be written and the potentials not.
    pub(crate) fn write(&self, py: Python<'_>, flows: &[u64], potentials: &[i64]) -> PyResult<()> {
        self.flows.write(py, flows)?;
        match &self.potentials {
            Some(buffer) => buffer.write(py, potentials),
            None => Ok(()),
        }
    }
}

impl<T: Held + Element> OutputBuffer<T> {
    /// The buffer of `argument`, the object Python gave for `output`, checked to take a column of `length` entries,
    /// with the memory it covers.
    ///
    /// # Errors
    /// Returns the [`ArgumentError`], as a Python exception, of an argument that exports no buffer, or whose buffer
    /// has other than one dimension, holds other items than `T` in the machine's byte order, is read-only, holds
    /// another number of entries than `length`, or is strided with items that cannot be written as they lie, in that
    /// order. Passes on, with a note that names the argument, what Python raises while the buffer is exported.
    fn new(output: Output, argument: &Bound<'_, PyAny>, length: usize) -> PyResult<(Self, Extent)> {
        let py = argument.py();
        let noted = |error| raised_writing(py, error, output);
        let view = match PyMemoryView::from(argument) {
            Ok(view) => view,
            Err(error) if error.is_instance_of::<PyTypeError>(py) => {
                return Err(ArgumentError::NotABuffer { output, type_name: type_name(argument)? }.into());
            }
            Err(error) => return Err(noted(error)),
        };
        let dimensions = dimensions_of(&view).map_err(noted)?;
        if dimensions != 1 {
            return Err(ArgumentError::OutputDimensions { output, dimensions }.into());
        }
        let buffer = PyUntypedBuffer::get(view.as_any()).map_err(noted)?;

        check::<T>(output, &buffer, length)?;
        let extent = Extent::of(&buffer);
        if length == 0 {
            return Ok((Self::Empty, extent));
        }

        let room = if buffer.is_c_contiguous() {
            let bytes = view.call_method1(intern!(py, "cast"), (intern!(py, "B"),)).map_err(noted)?;
            Self::Contiguous { output, bytes: PyBuffer::<u8>::get(&bytes).map_err(noted)? }
        } else {
            let items = PyBuffer::<T>::get(view.as_any()).map_err(|_| ArgumentError::StridedOutput { output })?;
            Self::Strided { output, items }
        };

        Ok((room, extent))
    }

    /// Writes `column` to the buffer, entry `i` to item `i`, by the interpreter's own copy; nothing to an empty one.
    ///
    /// # Errors
    /// Passes on, with a note that names the argument, what the copy raises: the `BufferError` of a `column` of
    /// another length than the buffer was checked for, which no caller in this crate hands it.
    fn write(&self, py: Python<'_>, column: &[T]) -> PyResult<()> {
        match self {
            Self::Empty => Ok(()),
            Self::Contiguous { output, bytes } => bytes
                .copy_from_slice(py, bytemuck::cast_slice::<T, u8>(column))
                .map_err(|error| raised_writing(py, error, *output)),
            Self::Strided { output, items } => {
                items.copy_from_slice(py, column).map_err(|error| raised_writing(py, error, *output))
            }
        }
    }
}

/// Checks that `buffer`, of one dimension and given for `output`, takes a column of `length` entries of `T`: items
/// exactly `T` in the machine's byte order, writable, of that length.
///
/// # Errors
/// Returns the first breach, in that order, as an [`ArgumentError`].
fn check<T: Held>(output: Output, buffer: &PyUntypedBuffer, length: usize) -> Result<(), ArgumentError> {
    let format = || buffer.format().to_string_lossy().into_owned();
    match Items::of(buffer.format(), buffer.item_size()) {
        items if items == T::ITEMS => {}
        Items::ForeignOrder => {
            return Err(ArgumentError::ForeignOrder { argument: Argument::Output(output), format: format() });
        }
        Items::Empty => return Err(ArgumentError::EmptyItems { argument: Argument::Output(output) }),
        _ => return Err(ArgumentError::OutputItems { output, format: format() }),
    }

    if buffer.readonly() {
        return Err(ArgumentError::ReadOnly { output });
    }
    match buffer.item_count() {
        count if count == length => Ok(()),
        count => Err(ArgumentError::OutputLength { output, length: count, expected: length }),
    }
}

impl Extent {
    /// The memory the items of `buffer`, of one dimension, cover.
    fn of(buffer: &PyUntypedBuffer) -> Self {
        let start = wide(buffer.buf_ptr().addr());
        let (count, stride, size) = (buffer.item_count(), buffer.strides()[0], buffer.item_size());

        Self::spanned(start, count, stride, size)
    }

    /// The memory `count` items of `size` bytes cover, the first at `start` and each next `stride` bytes on.
    fn spanned(start: i128, count: usize, stride: isize, size: usize) -> Self {
        let Some(last) = count.checked_sub(1) else {
            return Self { start, end: start };
        };

        // From the first item to the last, whichever way the stride runs; the widths fit `i128` with room to spare.
        let reach = wide(stride) * wide(last);
        let size = wide(size);
        Self { start: start + reach.min(0), end: start + reach.max(0) + size }
    }

    /// Whether the two cover a byte in common; an empty extent covers none.
    const fn overlaps(self, other: Self) -> bool {
        self.start < self.end && other.start < other.end && self.start < other.end && other.start < self.end
    }
}

/// `number`, an address, a count or a stride, as an `i128`, which holds every `usize` and `isize` of the platforms
/// the binding builds for, whose pointers have at most 64 bits.
///
/// # Panics
/// Panics if `number` does not fit an `i128`, which no `usize` or `isize` of 64 bits or fewer does.
fn wide<N: TryInto<i128>>(number: N) -> i128 {
    number.try_into().ok().expect("a usize or an isize should fit an i128")
}

#[cfg(test)]
mod tests {
    use pyo3::exceptions::PyValueError;
    use pyo3::types::PyDict;

    use super::*;

    /// Runs `test` with the interpreter embedded in this binary, attached.
    fn with_python(test: impl for<'py> FnOnce(Python<'py>) -> PyResult<()>) {
        Python::initialize();
        Python::attach(test).expect("the test's Python should run without an exception it does not expect");
    }

    /// The value of the Python expression `expression`, which may name `array` and `memoryview`.
    fn evaluate<'py>(py: Python<'py>, expression: &str) -> PyResult<Bound<'py, PyAny>> {
        let code = std::ffi::CString::new(expression).expect("an expression should hold no NUL");
        let globals = PyDict::new(py);
        globals.set_item("array", py.import("array")?.getattr("array")?)?;

        py.eval(&code, Some(&globals), None)
    }

    #[test]
    fn test_extents_by_hand() {
        assert_eq!(Extent::spanned(100, 3, 8, 8), Extent { start: 100, end: 124 });
        assert_eq!(Extent::spanned(100, 3, 16, 8), Extent { start: 100, end: 140 });
        assert_eq!(Extent::spanned(100, 3, -8, 8), Extent { start: 84, end: 108 });
        assert_eq!(Extent::spanned(100, 0, 8, 8), Extent { start: 100, end: 100 });
        assert_eq!(Extent::spanned(100, 5, 0, 8), Extent { start: 100, end: 108 });
    }

    #[test]
    fn test_overlaps_by_hand() {
        let at = |start: i128, end: i128| Extent { start, end };

        assert!(at(0, 24).overlaps(at(16, 40)));
        assert!(at(16, 40).overlaps(at(0, 24)));
        assert!(at(0, 24).overlaps(at(8, 16)), "one inside the other");
        assert!(!at(0, 24).overlaps(at(24, 48)), "one just after the other");
        assert!(!at(0, 0).overlaps(at(0, 24)), "an empty extent covers nothing");
    }

    #[test]
    fn test_a_contiguous_buffer_is_written_whatever_its_alignment_by_hand() {
        with_python(|py| {
            let room = evaluate(py, "memoryview(bytearray(25))[1:].cast('Q')")?;
            let (buffer, _) = OutputBuffer::<u64>::new(Output::Flows, &room, 3)?;
            assert!(matches!(buffer, OutputBuffer::Contiguous { .. }));

            buffer.write(py, &[2, 3, 5])?;
            assert_eq!(room.call_method0("tolist")?.extract::<Vec<u64>>()?, [2, 3, 5]);

            Ok(())
        });
    }

    #[test]
    fn test_a_strided_buffer_is_written_item_by_item_by_hand() {
        with_python(|py| {
            let whole = evaluate(py, "array('q', [9, 9, 9, 9, 9])")?;
            let room =
                evaluate(py, "memoryview")?.call1((&whole,))?.get_item(pyo3::types::PySlice::new(py, 0, 5, 2))?;
            let (buffer, _) = OutputBuffer::<i64>::new(Output::Potentials, &room, 3)?;
            assert!(matches!(buffer, OutputBuffer::Strided { .. }));

            buffer.write(py, &[-1, 0, 1])?;
            assert_eq!(whole.call_method0("tolist")?.extract::<Vec<i64>>()?, [-1, 9, 0, 9, 1]);

            Ok(())
        });
    }

    #[test]
    fn test_what_cannot_take_a_column_is_refused_in_order_by_hand() {
        with_python(|py| {
            let cases: [(&str, usize, &str); 5] = [
                ("[0, 0]", 2, "flows is of type list, not a writable buffer"),
                ("array('q', [0, 0])", 2, "flows holds items of format 'q', not unsigned 64-bit integers"),
                ("memoryview(bytes(16)).cast('Q')", 2, "flows is read-only"),
                ("array('Q', [0])", 2, "flows has length 1, not the arc count 2"),
                ("memoryview(bytearray(41))[1:].cast('Q')[::2]", 3, "flows is strided"),
            ];
            for (expression, length, message) in cases {
                let error =
                    OutputBuffer::<u64>::new(Output::Flows, &evaluate(py, expression)?, length).expect_err(expression);
                assert!(error.value(py).to_string().starts_with(message), "{expression}: {error}");
            }

            Ok(())
        });
    }

    #[test]
    fn test_outputs_that_share_memory_are_refused_and_others_are_written_by_hand() {
        with_python(|py| {
            let shared = evaluate(py, "memoryview(bytearray(48))")?;
            let flows = shared.get_item(pyo3::types::PySlice::new(py, 0, 24, 1))?.call_method1("cast", ("Q",))?;
            let potentials = shared.get_item(pyo3::types::PySlice::new(py, 16, 40, 1))?.call_method1("cast", ("q",))?;
            let error = Outputs::new(&flows, Some(&potentials), 3, 3).expect_err("the two share bytes 16 to 24");
            assert!(error.is_instance_of::<PyValueError>(py));
            assert_eq!(error.value(py).to_string(), "flows and potentials share memory");

            let apart = shared.get_item(pyo3::types::PySlice::new(py, 24, 48, 1))?.call_method1("cast", ("q",))?;
            let outputs = Outputs::new(&flows, Some(&apart), 3, 3)?;
            outputs.write(py, &[1, 2, 3], &[-1, -2, -3])?;
            assert_eq!(flows.call_method0("tolist")?.extract::<Vec<u64>>()?, [1, 2, 3]);
            assert_eq!(apart.call_method0("tolist")?.extract::<Vec<i64>>()?, [-1, -2, -3]);

            Ok(())
        });
    }
}
