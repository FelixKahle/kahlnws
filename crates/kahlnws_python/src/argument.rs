// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The arguments of a solve that hold columns of numbers, and every way one can be other than a solve reads it.

use core::fmt;

use pyo3::exceptions::{PyMemoryError, PyOverflowError, PyTypeError, PyValueError};
use pyo3::prelude::*;

/// An argument a solve reads a column of the problem from, named as Python names the parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Input {
    /// `tails`: the node each arc leaves.
    Tails,
    /// `heads`: the node each arc enters.
    Heads,
    /// `lower_bounds`: the least flow of each arc.
    LowerBounds,
    /// `capacities`: the most flow of each arc.
    Capacities,
    /// `costs`: the cost of a unit of flow on each arc.
    Costs,
    /// `supplies`: the supply of each node.
    Supplies,
}

/// An argument a solve writes a column of its answer to, named as Python names the parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Output {
    /// `flows`: the flow of each arc.
    Flows,
    /// `potentials`: the potential of each node.
    Potentials,
}

/// Any argument of a solve that holds a column, read or written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Argument {
    /// A column of the problem.
    Input(Input),
    /// A column of the answer.
    Output(Output),
}

/// The kind of integer an entry of a column is, which sets the range it must lie in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Entry {
    /// A node, held as a `usize`.
    Node,
    /// A number of units of flow, held as a `u64`.
    Unsigned,
    /// A number of units of cost or of flow of either sign, held as an `i64`.
    Signed,
}

impl Input {
    /// The kind of integer each entry of the column is.
    pub(crate) const fn entry(self) -> Entry {
        match self {
            Self::Tails | Self::Heads => Entry::Node,
            Self::LowerBounds | Self::Capacities => Entry::Unsigned,
            Self::Costs | Self::Supplies => Entry::Signed,
        }
    }
}

impl Output {
    /// The kind of integer each entry of the column is.
    pub(crate) const fn entry(self) -> Entry {
        match self {
            Self::Flows => Entry::Unsigned,
            Self::Potentials => Entry::Signed,
        }
    }

    /// What the problem has one entry of the column per: its arcs or its nodes, as their count is named.
    const fn counted(self) -> &'static str {
        match self {
            Self::Flows => "arc",
            Self::Potentials => "node",
        }
    }
}

impl fmt::Display for Input {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Tails => "tails",
            Self::Heads => "heads",
            Self::LowerBounds => "lower_bounds",
            Self::Capacities => "capacities",
            Self::Costs => "costs",
            Self::Supplies => "supplies",
        })
    }
}

impl fmt::Display for Output {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Flows => "flows",
            Self::Potentials => "potentials",
        })
    }
}

impl Entry {
    /// The integers a column of this kind holds, in the plural, without their range.
    const fn kind(self) -> &'static str {
        match self {
            Self::Node => "unsigned integers as wide as size_t",
            Self::Unsigned => "unsigned 64-bit integers",
            Self::Signed => "signed 64-bit integers",
        }
    }
}

impl fmt::Display for Argument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(input) => input.fmt(formatter),
            Self::Output(output) => output.fmt(formatter),
        }
    }
}

impl fmt::Display for Entry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Node => write!(formatter, "a node, from 0 to {}", usize::MAX),
            Self::Unsigned => write!(formatter, "an unsigned 64-bit integer, from 0 to {}", u64::MAX),
            Self::Signed => write!(formatter, "a signed 64-bit integer, from {} to {}", i64::MIN, i64::MAX),
        }
    }
}

/// An integer entry outside the range of its column, as the message gives it: in decimal where it fits 128 bits, else
/// by its sign and its length, which the decimal of a very long integer would pass Python's limit on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryValue {
    /// The entry, which fits 128 bits.
    Integer(i128),
    /// An entry past 128 bits: whether it is negative, and the number of bits of its magnitude.
    Long {
        /// Whether the entry is below zero.
        negative: bool,
        /// The bits of its magnitude.
        bits: u64,
    },
}

impl fmt::Display for EntryValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Integer(value) => write!(formatter, "{value}"),
            Self::Long { negative: false, bits } => write!(formatter, "an integer of {bits} bits"),
            Self::Long { negative: true, bits } => write!(formatter, "a negative integer of {bits} bits"),
        }
    }
}

/// Why an argument holds no column a solve reads, or no room a solve writes to. Plain data.
///
/// Each variant is raised in Python as the exception its doc names: [`TypeError`](PyTypeError) where the argument or
/// an entry is of the wrong kind, [`OverflowError`](PyOverflowError) where an integer lies outside the range of its
/// entry, as Python raises it where an `int` does not fit a C integer, [`ValueError`](PyValueError) where the shape,
/// the length or the memory of a room is wrong, and [`MemoryError`](PyMemoryError) where the memory for a copy is
/// refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ArgumentError {
    /// An input is neither a buffer nor a sequence. A `TypeError`.
    #[error("{input} is of type {type_name}, neither a sequence nor a buffer of integers")]
    NotAColumn {
        /// The argument.
        input: Input,
        /// The name of its type.
        type_name: String,
    },
    /// An entry of a sequence is no integer: its type has no `__index__`. A `TypeError`.
    #[error("{input}[{position}] is of type {type_name}, not an integer")]
    EntryNotAnInteger {
        /// The argument.
        input: Input,
        /// The entry's position in it, from 0.
        position: usize,
        /// The name of the entry's type.
        type_name: String,
    },
    /// An integer entry lies outside the range of its kind. An `OverflowError`.
    #[error("{input}[{position}] is {value}, outside the range of {entry}", entry = input.entry())]
    EntryOutOfRange {
        /// The argument.
        input: Input,
        /// The entry's position in it, from 0.
        position: usize,
        /// The entry.
        value: EntryValue,
    },
    /// The memory for the entries of an input is refused. A `MemoryError`.
    #[error("the room for the {length} entries of {input} was refused")]
    NoRoom {
        /// The argument.
        input: Input,
        /// Its number of entries.
        length: usize,
    },
    /// The items of an input buffer are no integers. A `TypeError`.
    #[error("{input} holds items of format '{format}', not integers")]
    ItemsNotIntegers {
        /// The argument.
        input: Input,
        /// The format the buffer states for its items, in the notation of the module `struct`.
        format: String,
    },
    /// The items of a buffer are integers in the other byte order than the machine's. A `TypeError`.
    #[error("{argument} holds integers in the other byte order than the machine's, of format '{format}'")]
    ForeignOrder {
        /// The argument.
        argument: Argument,
        /// The format the buffer states for its items, in the notation of the module `struct`.
        format: String,
    },
    /// The items of a buffer have no bytes. A `TypeError`.
    #[error("{argument} holds items of size zero")]
    EmptyItems {
        /// The argument.
        argument: Argument,
    },
    /// An input buffer has other than one dimension. A `ValueError`.
    #[error("{input} has {dimensions} dimensions, not 1")]
    InputDimensions {
        /// The argument.
        input: Input,
        /// Its number of dimensions.
        dimensions: usize,
    },
    /// An output is no buffer. A `TypeError`.
    #[error("{output} is of type {type_name}, not a writable buffer")]
    NotABuffer {
        /// The argument.
        output: Output,
        /// The name of its type.
        type_name: String,
    },
    /// The items of an output buffer are not exactly the integers the column holds. A `TypeError`.
    #[error("{output} holds items of format '{format}', not {kind}", kind = output.entry().kind())]
    OutputItems {
        /// The argument.
        output: Output,
        /// The format the buffer states for its items, in the notation of the module `struct`.
        format: String,
    },
    /// An output buffer has other than one dimension. A `ValueError`.
    #[error("{output} has {dimensions} dimensions, not 1")]
    OutputDimensions {
        /// The argument.
        output: Output,
        /// Its number of dimensions.
        dimensions: usize,
    },
    /// An output buffer cannot be written. A `ValueError`.
    #[error("{output} is read-only")]
    ReadOnly {
        /// The argument.
        output: Output,
    },
    /// An output buffer holds another number of entries than the problem has arcs or nodes. A `ValueError`.
    #[error("{output} has length {length}, not the {counted} count {expected}", counted = output.counted())]
    OutputLength {
        /// The argument.
        output: Output,
        /// Its number of entries.
        length: usize,
        /// The number of arcs, for the flows, or of nodes, for the potentials.
        expected: usize,
    },
    /// An output buffer is strided, and its items cannot be written as they lie: they are not aligned for their type,
    /// or their format names the byte order. A `ValueError`.
    #[error(
        "{output} is strided, with items not aligned for their type or a format that names the byte order: such a \
         buffer is written only where it is contiguous"
    )]
    StridedOutput {
        /// The argument.
        output: Output,
    },
    /// The two outputs share memory. A `ValueError`.
    #[error("flows and potentials share memory")]
    Overlap,
}

impl From<ArgumentError> for PyErr {
    fn from(error: ArgumentError) -> Self {
        let message = error.to_string();

        match error {
            ArgumentError::NotAColumn { .. }
            | ArgumentError::EntryNotAnInteger { .. }
            | ArgumentError::ItemsNotIntegers { .. }
            | ArgumentError::ForeignOrder { .. }
            | ArgumentError::EmptyItems { .. }
            | ArgumentError::NotABuffer { .. }
            | ArgumentError::OutputItems { .. } => PyTypeError::new_err(message),
            ArgumentError::EntryOutOfRange { .. } => PyOverflowError::new_err(message),
            ArgumentError::NoRoom { .. } => PyMemoryError::new_err(message),
            ArgumentError::InputDimensions { .. }
            | ArgumentError::OutputDimensions { .. }
            | ArgumentError::ReadOnly { .. }
            | ArgumentError::OutputLength { .. }
            | ArgumentError::StridedOutput { .. }
            | ArgumentError::Overlap => PyValueError::new_err(message),
        }
    }
}

/// `error`, raised while the object Python gave for `input` was read, with a note that names the argument.
pub(crate) fn raised_reading(py: Python<'_>, error: PyErr, input: Input) -> PyErr {
    noted(py, error, format!("raised while the argument {input} was read"))
}

/// `error`, raised while the object Python gave for `output` was checked or written, with a note that names the
/// argument.
pub(crate) fn raised_writing(py: Python<'_>, error: PyErr, output: Output) -> PyErr {
    noted(py, error, format!("raised while the argument {output} was checked or written"))
}

/// `error` with `note` added; the failure to add it in its place, which only a lack of memory makes.
fn noted(py: Python<'_>, error: PyErr, note: String) -> PyErr {
    match error.add_note(py, note) {
        Ok(()) => error,
        Err(failure) => failure,
    }
}

/// The name of the type of `object`, as Python's own messages give it: `float`, `numpy.ndarray`.
///
/// # Errors
/// Passes on an exception the type raises for its name, which a type with an ordinary name never does.
pub(crate) fn type_name(object: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(object.get_type().fully_qualified_name()?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_each_message_by_hand() {
        let cases = [
            (
                ArgumentError::NotAColumn { input: Input::Tails, type_name: "float".to_owned() },
                "tails is of type float, neither a sequence nor a buffer of integers".to_owned(),
            ),
            (
                ArgumentError::EntryNotAnInteger { input: Input::Costs, position: 3, type_name: "str".to_owned() },
                "costs[3] is of type str, not an integer".to_owned(),
            ),
            (
                ArgumentError::EntryOutOfRange {
                    input: Input::Capacities,
                    position: 2,
                    value: EntryValue::Integer(-1),
                },
                format!("capacities[2] is -1, outside the range of an unsigned 64-bit integer, from 0 to {}", u64::MAX),
            ),
            (
                ArgumentError::EntryOutOfRange { input: Input::Heads, position: 0, value: EntryValue::Integer(-7) },
                format!("heads[0] is -7, outside the range of a node, from 0 to {}", usize::MAX),
            ),
            (
                ArgumentError::EntryOutOfRange {
                    input: Input::Supplies,
                    position: 1,
                    value: EntryValue::Integer(9_223_372_036_854_775_808),
                },
                "supplies[1] is 9223372036854775808, outside the range of a signed 64-bit integer, from \
                 -9223372036854775808 to 9223372036854775807"
                    .to_owned(),
            ),
            (
                ArgumentError::NoRoom { input: Input::Tails, length: 7 },
                "the room for the 7 entries of tails was refused".to_owned(),
            ),
            (
                ArgumentError::ItemsNotIntegers { input: Input::LowerBounds, format: "d".to_owned() },
                "lower_bounds holds items of format 'd', not integers".to_owned(),
            ),
            (
                ArgumentError::InputDimensions { input: Input::Tails, dimensions: 2 },
                "tails has 2 dimensions, not 1".to_owned(),
            ),
            (
                ArgumentError::NotABuffer { output: Output::Flows, type_name: "list".to_owned() },
                "flows is of type list, not a writable buffer".to_owned(),
            ),
            (
                ArgumentError::OutputItems { output: Output::Potentials, format: "Q".to_owned() },
                "potentials holds items of format 'Q', not signed 64-bit integers".to_owned(),
            ),
            (
                ArgumentError::OutputDimensions { output: Output::Flows, dimensions: 0 },
                "flows has 0 dimensions, not 1".to_owned(),
            ),
            (
                ArgumentError::ForeignOrder { argument: Argument::Input(Input::Heads), format: ">q".to_owned() },
                "heads holds integers in the other byte order than the machine's, of format '>q'".to_owned(),
            ),
            (
                ArgumentError::EmptyItems { argument: Argument::Output(Output::Potentials) },
                "potentials holds items of size zero".to_owned(),
            ),
            (
                ArgumentError::EntryOutOfRange {
                    input: Input::Costs,
                    position: 4,
                    value: EntryValue::Long { negative: true, bits: 16_610 },
                },
                format!(
                    "costs[4] is a negative integer of 16610 bits, outside the range of a signed 64-bit integer, \
                     from {} to {}",
                    i64::MIN,
                    i64::MAX
                ),
            ),
            (
                ArgumentError::StridedOutput { output: Output::Flows },
                "flows is strided, with items not aligned for their type or a format that names the byte order: \
                 such a buffer is written only where it is contiguous"
                    .to_owned(),
            ),
            (ArgumentError::Overlap, "flows and potentials share memory".to_owned()),
            (ArgumentError::ReadOnly { output: Output::Flows }, "flows is read-only".to_owned()),
            (
                ArgumentError::OutputLength { output: Output::Flows, length: 2, expected: 3 },
                "flows has length 2, not the arc count 3".to_owned(),
            ),
            (
                ArgumentError::OutputLength { output: Output::Potentials, length: 4, expected: 3 },
                "potentials has length 4, not the node count 3".to_owned(),
            ),
        ];

        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn test_each_argument_is_named_as_its_python_parameter() {
        let inputs = [Input::Tails, Input::Heads, Input::LowerBounds, Input::Capacities, Input::Costs, Input::Supplies]
            .map(|input| input.to_string());
        let outputs = [Output::Flows, Output::Potentials].map(|output| output.to_string());

        assert_eq!(inputs, ["tails", "heads", "lower_bounds", "capacities", "costs", "supplies"]);
        assert_eq!(outputs, ["flows", "potentials"]);
    }

    #[test]
    fn test_each_column_holds_the_entry_of_its_table_in_the_library() {
        let inputs = [Input::Tails, Input::Heads, Input::LowerBounds, Input::Capacities, Input::Costs, Input::Supplies]
            .map(Input::entry);
        let outputs = [Output::Flows, Output::Potentials].map(Output::entry);

        assert_eq!(inputs, [Entry::Node, Entry::Node, Entry::Unsigned, Entry::Unsigned, Entry::Signed, Entry::Signed]);
        assert_eq!(outputs, [Entry::Unsigned, Entry::Signed]);
    }

    #[test]
    fn test_raised_reading_and_writing_keep_the_exception_and_add_a_note_naming_the_argument_by_hand() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let read = raised_reading(py, PyValueError::new_err("an exporter's own words"), Input::LowerBounds);
            let written = raised_writing(py, PyTypeError::new_err("another's"), Output::Potentials);

            assert!(read.is_instance_of::<PyValueError>(py));
            assert_eq!(read.value(py).to_string(), "an exporter's own words");
            assert_eq!(
                read.value(py).getattr("__notes__")?.extract::<Vec<String>>()?,
                ["raised while the argument lower_bounds was read"]
            );
            assert!(written.is_instance_of::<PyTypeError>(py));
            assert_eq!(
                written.value(py).getattr("__notes__")?.extract::<Vec<String>>()?,
                ["raised while the argument potentials was checked or written"]
            );

            Ok(())
        })
        .expect("the notes should be added");
    }

    #[test]
    fn test_each_error_is_raised_as_its_python_exception_by_hand() {
        Python::initialize();
        Python::attach(|py| {
            let raised = |error: ArgumentError| PyErr::from(error).get_type(py).name().map(|name| name.to_string());
            let input = Input::Tails;

            let cases = [
                (ArgumentError::NotAColumn { input, type_name: "set".to_owned() }, "TypeError"),
                (
                    ArgumentError::EntryOutOfRange { input, position: 0, value: EntryValue::Integer(-1) },
                    "OverflowError",
                ),
                (ArgumentError::NoRoom { input, length: 1 }, "MemoryError"),
                (ArgumentError::InputDimensions { input, dimensions: 2 }, "ValueError"),
                (ArgumentError::Overlap, "ValueError"),
                (ArgumentError::EmptyItems { argument: Argument::Input(input) }, "TypeError"),
            ];
            for (error, name) in cases {
                assert_eq!(raised(error.clone()).ok().as_deref(), Some(name), "{error:?}");
            }
        });
    }
}
