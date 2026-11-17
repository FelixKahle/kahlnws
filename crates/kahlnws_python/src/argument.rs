// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The arguments of a solve that hold columns of numbers, the arguments of a network's calls that hold one number or
//! name an arc or a node, and every way one can be other than a call reads it.

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

/// An argument a solve or a network writes a column to, named as Python names the parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Output {
    /// `flows`: the flow of each arc.
    Flows,
    /// `potentials`: the potential of each node.
    Potentials,
    /// A column of the problem a network holds, read back, named as the argument a load reads it from.
    Problem(Input),
}

/// Any argument of a solve that holds a column, read or written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Argument {
    /// A column of the problem.
    Input(Input),
    /// A column of the answer.
    Output(Output),
}

/// A number a call takes on its own, outside any column: a cost, a number of units of flow, or a count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Number {
    /// `cost`: the cost of a unit of flow on an arc.
    Cost,
    /// `supply`: the supply of a node, a demand below zero.
    Supply,
    /// A number of units of flow.
    Flow(FlowNumber),
    /// A count of nodes or of arcs.
    Count(Count),
}

/// A number of units of flow a call of a network takes, named as Python names the parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlowNumber {
    /// `lower_bound`: the least flow of an arc.
    LowerBound,
    /// `capacity`: the most flow of an arc.
    Capacity,
    /// `amount`: the units of flow a move of supply moves.
    Amount,
}

/// A count a call takes, named as Python names the parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Count {
    /// `node_count`: a number of nodes.
    NodeCount,
    /// `arc_count`: a number of arcs.
    ArcCount,
}

/// An argument of a network's call that names an arc or a node, named as Python names the parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Index {
    /// `arc`: an arc.
    Arc,
    /// `node`: a node.
    Node,
    /// `from_node`: the node a move of supply takes from.
    FromNode,
    /// `to_node`: the node a move of supply adds to.
    ToNode,
    /// `balancing_node`: the node whose supply moves the other way where a supply is set.
    BalancingNode,
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
    /// A count of nodes or of arcs, held as a `usize`.
    Count,
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
            Self::Problem(input) => input.entry(),
        }
    }

    /// What the problem has one entry of the column per: its arcs or its nodes, as their count is named.
    const fn counted(self) -> &'static str {
        match self {
            Self::Flows
            | Self::Problem(Input::Tails | Input::Heads | Input::LowerBounds | Input::Capacities | Input::Costs) => {
                "arc"
            }
            Self::Potentials | Self::Problem(Input::Supplies) => "node",
        }
    }
}

impl Number {
    /// The kind of integer the number is.
    pub(crate) const fn entry(self) -> Entry {
        match self {
            Self::Cost | Self::Supply => Entry::Signed,
            Self::Flow(_) => Entry::Unsigned,
            Self::Count(_) => Entry::Count,
        }
    }
}

impl fmt::Display for Number {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Cost => "cost",
            Self::Supply => "supply",
            Self::Flow(FlowNumber::LowerBound) => "lower_bound",
            Self::Flow(FlowNumber::Capacity) => "capacity",
            Self::Flow(FlowNumber::Amount) => "amount",
            Self::Count(Count::NodeCount) => "node_count",
            Self::Count(Count::ArcCount) => "arc_count",
        })
    }
}

impl fmt::Display for Index {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Arc => "arc",
            Self::Node => "node",
            Self::FromNode => "from_node",
            Self::ToNode => "to_node",
            Self::BalancingNode => "balancing_node",
        })
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
        match self {
            Self::Flows => formatter.write_str("flows"),
            Self::Potentials => formatter.write_str("potentials"),
            Self::Problem(input) => input.fmt(formatter),
        }
    }
}

impl Entry {
    /// The integers a column of this kind holds, in the plural, without their range.
    const fn kind(self) -> &'static str {
        match self {
            Self::Node => "unsigned integers as wide as size_t",
            Self::Unsigned => "unsigned 64-bit integers",
            Self::Signed => "signed 64-bit integers",
            Self::Count => "counts as wide as size_t",
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
            Self::Count => write!(formatter, "a count, from 0 to {}", usize::MAX),
        }
    }
}

/// An integer Python gave, of any size, as a message gives it: in decimal where it fits 128 bits, else by the power of
/// two its magnitude reaches, as `2**200 or more` or `-(2**200 or more)`, since the decimal of a very long integer
/// would pass Python's limit on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IntegerValue {
    /// The entry, which fits 128 bits.
    Integer(i128),
    /// An entry past 128 bits: whether it is negative, and the number of bits of its magnitude, at least 128.
    Long {
        /// Whether the entry is below zero.
        negative: bool,
        /// The bits of its magnitude.
        bits: u64,
    },
}

impl IntegerValue {
    /// The integer as a position in a table: `None` where it is below zero or past a `usize`.
    pub(crate) fn position(self) -> Option<usize> {
        match self {
            Self::Integer(value) => usize::try_from(value).ok(),
            Self::Long { .. } => None,
        }
    }
}

impl fmt::Display for IntegerValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Integer(value) => write!(formatter, "{value}"),
            // A magnitude of `bits` bits is at least `2**(bits - 1)`.
            Self::Long { negative: false, bits } => write!(formatter, "2**{} or more", bits.strict_sub(1)),
            Self::Long { negative: true, bits } => write!(formatter, "-(2**{} or more)", bits.strict_sub(1)),
        }
    }
}

/// Why an argument holds no column a solve reads, no room a solve writes to, or no number a network's call takes. Plain
/// data.
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
        value: IntegerValue,
    },
    /// A number a call takes is no integer: its type has no `__index__`. A `TypeError`.
    #[error("{number} is of type {type_name}, not an integer")]
    NumberNotAnInteger {
        /// The argument.
        number: Number,
        /// The name of its type.
        type_name: String,
    },
    /// A number a call takes lies outside the range of its kind. An `OverflowError`.
    #[error("{number} {value} is outside the range of {entry}", entry = number.entry())]
    NumberOutOfRange {
        /// The argument.
        number: Number,
        /// Its value.
        value: IntegerValue,
    },
    /// An argument that names an arc or a node is no integer: its type has no `__index__`. A `TypeError`.
    #[error("{index} is of type {type_name}, not an integer")]
    IndexNotAnInteger {
        /// The argument.
        index: Index,
        /// The name of its type.
        type_name: String,
    },
    /// The bounds a call takes are no sequence. A `TypeError`.
    #[error("bounds is of type {type_name}, not a pair of integers")]
    BoundsNotAPair {
        /// The name of its type.
        type_name: String,
    },
    /// The bounds a call takes hold other than two items. A `ValueError`.
    #[error("bounds has length {length}, not 2")]
    BoundsLength {
        /// Their number of items.
        length: usize,
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
    #[error("{output} holds {length} entries for a problem of {expected} {counted}s", counted = output.counted())]
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
            | ArgumentError::OutputItems { .. }
            | ArgumentError::NumberNotAnInteger { .. }
            | ArgumentError::IndexNotAnInteger { .. }
            | ArgumentError::BoundsNotAPair { .. } => PyTypeError::new_err(message),
            ArgumentError::EntryOutOfRange { .. } | ArgumentError::NumberOutOfRange { .. } => {
                PyOverflowError::new_err(message)
            }
            ArgumentError::NoRoom { .. } => PyMemoryError::new_err(message),
            ArgumentError::InputDimensions { .. }
            | ArgumentError::OutputDimensions { .. }
            | ArgumentError::ReadOnly { .. }
            | ArgumentError::OutputLength { .. }
            | ArgumentError::StridedOutput { .. }
            | ArgumentError::BoundsLength { .. }
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
                    value: IntegerValue::Integer(-1),
                },
                format!("capacities[2] is -1, outside the range of an unsigned 64-bit integer, from 0 to {}", u64::MAX),
            ),
            (
                ArgumentError::EntryOutOfRange { input: Input::Heads, position: 0, value: IntegerValue::Integer(-7) },
                format!("heads[0] is -7, outside the range of a node, from 0 to {}", usize::MAX),
            ),
            (
                ArgumentError::EntryOutOfRange {
                    input: Input::Supplies,
                    position: 1,
                    value: IntegerValue::Integer(9_223_372_036_854_775_808),
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
                    value: IntegerValue::Long { negative: true, bits: 16_610 },
                },
                format!(
                    "costs[4] is -(2**16609 or more), outside the range of a signed 64-bit integer, \
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
                ArgumentError::BoundsNotAPair { type_name: "set".to_owned() },
                "bounds is of type set, not a pair of integers".to_owned(),
            ),
            (ArgumentError::BoundsLength { length: 3 }, "bounds has length 3, not 2".to_owned()),
            (
                ArgumentError::NumberOutOfRange {
                    number: Number::Flow(FlowNumber::Capacity),
                    value: IntegerValue::Integer(-1),
                },
                format!("capacity -1 is outside the range of an unsigned 64-bit integer, from 0 to {}", u64::MAX),
            ),
            (
                ArgumentError::NumberOutOfRange { number: Number::Cost, value: IntegerValue::Integer(1 << 63) },
                "cost 9223372036854775808 is outside the range of a signed 64-bit integer, from -9223372036854775808 \
                 to 9223372036854775807"
                    .to_owned(),
            ),
            (
                ArgumentError::NumberOutOfRange {
                    number: Number::Flow(FlowNumber::Amount),
                    value: IntegerValue::Long { negative: false, bits: 130 },
                },
                format!(
                    "amount 2**129 or more is outside the range of an unsigned 64-bit integer, from 0 to {}",
                    u64::MAX
                ),
            ),
            (
                ArgumentError::NumberOutOfRange {
                    number: Number::Count(Count::ArcCount),
                    value: IntegerValue::Integer(-1),
                },
                format!("arc_count -1 is outside the range of a count, from 0 to {}", usize::MAX),
            ),
            (
                ArgumentError::NumberNotAnInteger {
                    number: Number::Flow(FlowNumber::LowerBound),
                    type_name: "float".to_owned(),
                },
                "lower_bound is of type float, not an integer".to_owned(),
            ),
            (
                ArgumentError::IndexNotAnInteger { index: Index::FromNode, type_name: "str".to_owned() },
                "from_node is of type str, not an integer".to_owned(),
            ),
            (
                ArgumentError::OutputLength { output: Output::Flows, length: 2, expected: 3 },
                "flows holds 2 entries for a problem of 3 arcs".to_owned(),
            ),
            (
                ArgumentError::OutputLength { output: Output::Potentials, length: 4, expected: 3 },
                "potentials holds 4 entries for a problem of 3 nodes".to_owned(),
            ),
            (
                ArgumentError::OutputLength { output: Output::Problem(Input::Supplies), length: 2, expected: 3 },
                "supplies holds 2 entries for a problem of 3 nodes".to_owned(),
            ),
            (
                ArgumentError::OutputLength { output: Output::Problem(Input::Capacities), length: 2, expected: 3 },
                "capacities holds 2 entries for a problem of 3 arcs".to_owned(),
            ),
            (
                ArgumentError::OutputItems { output: Output::Problem(Input::Tails), format: "q".to_owned() },
                "tails holds items of format 'q', not unsigned integers as wide as size_t".to_owned(),
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
        assert_eq!(Output::Problem(Input::LowerBounds).to_string(), "lower_bounds");
    }

    #[test]
    fn test_each_number_and_index_is_named_as_its_python_parameter_and_has_the_kind_of_its_setter() {
        let numbers = [
            Number::Cost,
            Number::Supply,
            Number::Flow(FlowNumber::LowerBound),
            Number::Flow(FlowNumber::Capacity),
            Number::Flow(FlowNumber::Amount),
            Number::Count(Count::NodeCount),
            Number::Count(Count::ArcCount),
        ];
        let indices = [Index::Arc, Index::Node, Index::FromNode, Index::ToNode, Index::BalancingNode]
            .map(|index| index.to_string());

        let names = ["cost", "supply", "lower_bound", "capacity", "amount", "node_count", "arc_count"];
        assert_eq!(numbers.map(|number| number.to_string()), names);
        let (signed, unsigned, count) = (Entry::Signed, Entry::Unsigned, Entry::Count);
        assert_eq!(numbers.map(Number::entry), [signed, signed, unsigned, unsigned, unsigned, count, count]);
        assert_eq!(indices, ["arc", "node", "from_node", "to_node", "balancing_node"]);
    }

    #[test]
    fn test_an_integer_is_a_position_where_it_is_at_least_zero_and_fits_a_usize_by_hand() {
        let positions = [
            IntegerValue::Integer(0),
            IntegerValue::Integer(7),
            IntegerValue::Integer(-1),
            IntegerValue::Integer(i128::MIN),
            IntegerValue::Long { negative: false, bits: 200 },
        ]
        .map(IntegerValue::position);
        let past = i128::try_from(usize::MAX).expect("a usize should fit 128 bits") + 1;

        assert_eq!(positions, [Some(0), Some(7), None, None, None]);
        assert_eq!(IntegerValue::Integer(past - 1).position(), Some(usize::MAX));
        assert_eq!(IntegerValue::Integer(past).position(), None);
    }

    #[test]
    fn test_each_column_holds_the_entry_of_its_table_in_the_library() {
        let inputs = [Input::Tails, Input::Heads, Input::LowerBounds, Input::Capacities, Input::Costs, Input::Supplies]
            .map(Input::entry);
        let outputs = [Output::Flows, Output::Potentials].map(Output::entry);
        let read_back =
            [Input::Tails, Input::Heads, Input::LowerBounds, Input::Capacities, Input::Costs, Input::Supplies]
                .map(|input| Output::Problem(input).entry());

        assert_eq!(inputs, [Entry::Node, Entry::Node, Entry::Unsigned, Entry::Unsigned, Entry::Signed, Entry::Signed]);
        assert_eq!(outputs, [Entry::Unsigned, Entry::Signed]);
        assert_eq!(read_back, inputs, "a column read back holds what it is loaded from");
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
                    ArgumentError::EntryOutOfRange { input, position: 0, value: IntegerValue::Integer(-1) },
                    "OverflowError",
                ),
                (ArgumentError::NoRoom { input, length: 1 }, "MemoryError"),
                (ArgumentError::InputDimensions { input, dimensions: 2 }, "ValueError"),
                (ArgumentError::Overlap, "ValueError"),
                (ArgumentError::EmptyItems { argument: Argument::Input(input) }, "TypeError"),
                (
                    ArgumentError::NumberOutOfRange { number: Number::Cost, value: IntegerValue::Integer(1 << 63) },
                    "OverflowError",
                ),
                (ArgumentError::NumberNotAnInteger { number: Number::Cost, type_name: "str".to_owned() }, "TypeError"),
                (ArgumentError::IndexNotAnInteger { index: Index::Arc, type_name: "str".to_owned() }, "TypeError"),
                (ArgumentError::BoundsNotAPair { type_name: "int".to_owned() }, "TypeError"),
                (ArgumentError::BoundsLength { length: 1 }, "ValueError"),
            ];
            for (error, name) in cases {
                assert_eq!(raised(error.clone()).ok().as_deref(), Some(name), "{error:?}");
            }
        });
    }
}
