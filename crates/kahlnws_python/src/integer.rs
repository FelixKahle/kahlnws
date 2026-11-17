// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! An integer Python gives for a number, an entry of a column or an argument of its own: an `int`, or an object whose
//! `__index__` makes one, taken once and checked against the range of the type it is held in.
//!
//! The rule is one, whatever the number: [`convert`] takes the integer and checks its range, and the column's entries,
//! a call's numbers and a call's arcs and nodes each say in their own words what it refused.

use pyo3::conversion::FromPyObjectOwned;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::PyInt;

use crate::argument::{ArgumentError, Count, FlowNumber, Index, IntegerValue, Number, type_name};

/// What an object Python gave for an integer converts to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Converted<T> {
    /// The integer, within the range of `T`.
    Within(T),
    /// The integer, outside the range of `T`.
    Outside(IntegerValue),
    /// No integer: the object is no `int`, and its type has no `__index__`.
    NotAnInteger,
}

/// `object` converted to a `T`: an `int`, or an object whose type has `__index__`, whose value is taken once.
///
/// # Errors
/// Passes on what the object's `__index__` raises, and the `TypeError` of one that answers no `int`.
pub(crate) fn convert<'py, T>(object: &Bound<'py, PyAny>) -> PyResult<Converted<T>>
where
    T: FromPyObjectOwned<'py>,
{
    let integer = match object.cast::<PyInt>() {
        Ok(integer) => integer.clone(),
        Err(_) if object.get_type().hasattr(intern!(object.py(), "__index__"))? => index_of(object)?,
        Err(_) => return Ok(Converted::NotAnInteger),
    };

    // The value is an `int` now, so the one way its conversion fails is a value outside the range of `T`.
    match integer.extract::<T>() {
        Ok(converted) => Ok(Converted::Within(converted)),
        Err(_) => Ok(Converted::Outside(value_of(&integer)?)),
    }
}

/// `object`, the argument Python gave for a cost, as the `i64` a cost is held in.
///
/// # Errors
/// As [`number`].
pub(crate) fn cost(object: &Bound<'_, PyAny>) -> PyResult<i64> {
    number(Number::Cost, object)
}

/// `object`, the argument Python gave for a supply, as the `i64` a supply is held in.
///
/// # Errors
/// As [`number`].
pub(crate) fn supply(object: &Bound<'_, PyAny>) -> PyResult<i64> {
    number(Number::Supply, object)
}

/// `object`, the argument Python gave for `flow`, as the `u64` a number of units of flow is held in.
///
/// # Errors
/// As [`number`].
pub(crate) fn flow(flow: FlowNumber, object: &Bound<'_, PyAny>) -> PyResult<u64> {
    number(Number::Flow(flow), object)
}

/// `object`, the argument Python gave for `count`, as the `usize` a count is held in.
///
/// # Errors
/// As [`number`].
pub(crate) fn count(count: Count, object: &Bound<'_, PyAny>) -> PyResult<usize> {
    number(Number::Count(count), object)
}

/// `object`, the argument Python gave for `number`, as a `T`, which is the type [`Number::entry`] names for it: the
/// four callers above each pair one kind of number with its type.
///
/// # Errors
/// Raises the `TypeError` of [`ArgumentError::NumberNotAnInteger`] for no integer, and the `OverflowError` of
/// [`ArgumentError::NumberOutOfRange`], with the value taken, for one outside the range of `T`. Passes on what the
/// object's `__index__` raises.
fn number<'py, T>(number: Number, object: &Bound<'py, PyAny>) -> PyResult<T>
where
    T: FromPyObjectOwned<'py>,
{
    match convert(object)? {
        Converted::Within(converted) => Ok(converted),
        Converted::Outside(value) => Err(ArgumentError::NumberOutOfRange { number, value }.into()),
        Converted::NotAnInteger => {
            Err(ArgumentError::NumberNotAnInteger { number, type_name: type_name(object)? }.into())
        }
    }
}

/// `object`, the argument Python gave for `index`, as the integer it is, of any size: whether it names an arc or a
/// node is the count's to say.
///
/// # Errors
/// Raises the `TypeError` of [`ArgumentError::IndexNotAnInteger`] for no integer. Passes on what the object's
/// `__index__` raises.
pub(crate) fn index(index: Index, object: &Bound<'_, PyAny>) -> PyResult<IntegerValue> {
    match convert::<i128>(object)? {
        Converted::Within(value) => Ok(IntegerValue::Integer(value)),
        Converted::Outside(value) => Ok(value),
        Converted::NotAnInteger => {
            Err(ArgumentError::IndexNotAnInteger { index, type_name: type_name(object)? }.into())
        }
    }
}

/// The `int` that `operator.index` makes of `entry`, which calls its `__index__` once.
///
/// # Errors
/// Passes on what `__index__` raises, and the `TypeError` of one that answers no `int`.
fn index_of<'py>(entry: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyInt>> {
    let py = entry.py();
    let index = py.import(intern!(py, "operator"))?.getattr(intern!(py, "index"))?.call1((entry,))?;

    Ok(index.cast_into::<PyInt>()?)
}

/// The value of `integer`, as a message gives it: in decimal where it fits 128 bits, else by its sign and its bits.
///
/// # Errors
/// Passes on what the comparison or `int.bit_length` raises, which for an `int` is nothing but a `MemoryError`.
fn value_of(integer: &Bound<'_, PyInt>) -> PyResult<IntegerValue> {
    if let Ok(value) = integer.extract::<i128>() {
        return Ok(IntegerValue::Integer(value));
    }

    let negative = integer.lt(0)?;
    let bits = integer.call_method0(intern!(integer.py(), "bit_length"))?.extract::<u64>()?;

    Ok(IntegerValue::Long { negative, bits })
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

        py.eval(&code, Some(&PyDict::new(py)), None)
    }

    #[test]
    fn test_convert_takes_an_int_or_an_index_and_checks_the_range_by_hand() {
        with_python(|py| {
            let converted = |expression: &str| -> PyResult<Converted<u64>> { convert(&evaluate(py, expression)?) };
            assert_eq!(converted("7")?, Converted::Within(7));
            assert_eq!(converted("True")?, Converted::Within(1));
            assert_eq!(converted("-1")?, Converted::Outside(IntegerValue::Integer(-1)));
            assert_eq!(converted("2**64")?, Converted::Outside(IntegerValue::Integer(1 << 64)));
            assert_eq!(converted("1.0")?, Converted::NotAnInteger);
            assert_eq!(converted("'7'")?, Converted::NotAnInteger);
            assert_eq!(convert::<i64>(&evaluate(py, "-(2**63)")?)?, Converted::Within(i64::MIN));

            let raising =
                py.eval(c"type('Raising', (), {'__index__': lambda self: 1 // 0})()", Some(&PyDict::new(py)), None)?;
            let error = convert::<u64>(&raising).expect_err("the index raises");
            assert!(error.is_instance_of::<pyo3::exceptions::PyZeroDivisionError>(py));

            Ok(())
        });
    }

    #[test]
    fn test_cost_flow_count_and_index_name_the_argument_they_refuse_by_hand() {
        with_python(|py| {
            assert_eq!(cost(&evaluate(py, "-5")?)?, -5);
            assert_eq!(supply(&evaluate(py, "-(2**63)")?)?, i64::MIN);
            let refused = supply(&evaluate(py, "2**63")?).expect_err("past i64");
            assert_eq!(
                refused.value(py).to_string(),
                format!(
                    "supply {} is outside the range of a signed 64-bit integer, from {} to {}",
                    1_u64 << 63,
                    i64::MIN,
                    i64::MAX
                )
            );
            assert_eq!(flow(FlowNumber::Amount, &evaluate(py, "2**64 - 1")?)?, u64::MAX);
            assert_eq!(count(Count::ArcCount, &evaluate(py, "7")?)?, 7);
            let refused = flow(FlowNumber::Capacity, &evaluate(py, "-1")?).expect_err("below zero");
            assert!(refused.is_instance_of::<pyo3::exceptions::PyOverflowError>(py));
            assert_eq!(
                refused.value(py).to_string(),
                format!("capacity -1 is outside the range of an unsigned 64-bit integer, from 0 to {}", u64::MAX)
            );
            let refused = flow(FlowNumber::Amount, &evaluate(py, "None")?).expect_err("no integer");
            assert!(refused.is_instance_of::<pyo3::exceptions::PyTypeError>(py));
            assert_eq!(refused.value(py).to_string(), "amount is of type NoneType, not an integer");
            let refused = count(Count::NodeCount, &evaluate(py, "-1")?).expect_err("below zero");
            assert!(refused.is_instance_of::<pyo3::exceptions::PyOverflowError>(py));
            assert_eq!(
                refused.value(py).to_string(),
                format!("node_count -1 is outside the range of a count, from 0 to {}", usize::MAX)
            );
            let refused = cost(&evaluate(py, "2**63")?).expect_err("past i64");
            assert_eq!(
                refused.value(py).to_string(),
                format!(
                    "cost {} is outside the range of a signed 64-bit integer, from {} to {}",
                    1_u64 << 63,
                    i64::MIN,
                    i64::MAX
                )
            );

            assert_eq!(index(Index::Arc, &evaluate(py, "-1")?)?, IntegerValue::Integer(-1));
            assert_eq!(index(Index::Arc, &evaluate(py, "2**70")?)?, IntegerValue::Integer(1 << 70));
            assert_eq!(
                index(Index::Node, &evaluate(py, "2**130")?)?,
                IntegerValue::Long { negative: false, bits: 131 }
            );
            let refused = index(Index::ToNode, &evaluate(py, "1.5")?).expect_err("no integer");
            assert_eq!(refused.value(py).to_string(), "to_node is of type float, not an integer");

            Ok(())
        });
    }

    #[test]
    fn test_value_of_by_hand() {
        with_python(|py| {
            let value =
                |expression: &str| -> PyResult<IntegerValue> { value_of(evaluate(py, expression)?.cast::<PyInt>()?) };
            assert_eq!(value("2**64")?, IntegerValue::Integer(1 << 64));
            assert_eq!(value("-(2**127)")?, IntegerValue::Integer(i128::MIN));
            assert_eq!(value("2**127")?, IntegerValue::Long { negative: false, bits: 128 });
            assert_eq!(value("-(2**200)")?, IntegerValue::Long { negative: true, bits: 201 });

            Ok(())
        });
    }
}
