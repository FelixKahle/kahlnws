// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bounds of an arc as Python holds them: the named tuple `kahlnws.Bounds`, which a network's read of one arc's
//! bounds and its setter of both answer, and the pair that setter takes.

use pyo3::intern;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyInt, PyList, PySequence, PyType};

use crate::argument::{ArgumentError, FlowNumber, type_name};
use crate::integer;

/// The docstring of `kahlnws.Bounds`, which the class is given as its `__doc__` when it is made, and which the stubs
/// repeat.
const DOC: &str = "\
The least and the most flow of an arc, in units of flow: a named tuple of two `int`, the lower bound first.

What `Network.bounds` reads and `Network.set_bounds` answers, the bounds the arc had, in the order `set_bounds`
takes them: bounds read or answered and set back make the arc what it was, `network.set_bounds(arc, had)`.
`set_bounds` takes any pair of two integers, a plain tuple as well. A `Bounds` unpacks, compares and hashes as the
tuple it is, reads its two numbers by their names, and copies and pickles.

Args:
    lower_bound: The least flow of the arc.
    capacity: The most flow of the arc.

Example:
    >>> bounds = kahlnws.Bounds(lower_bound=0, capacity=3)
    >>> bounds.capacity, bounds == (0, 3)
    (3, True)";

/// The class `kahlnws.Bounds`, made the first time it is asked for and kept for the process.
static CLASS: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// The class `kahlnws.Bounds`: a `typing.NamedTuple` of the fields `lower_bound` and `capacity`, of the module
/// `kahlnws`, so that a `Bounds` pickles by that name.
///
/// # Errors
/// Passes on what Python raises while the class is made, which for these fields only a lack of memory makes.
pub(crate) fn class(py: Python<'_>) -> PyResult<&Bound<'_, PyType>> {
    let class = CLASS.get_or_try_init(py, || -> PyResult<Py<PyType>> {
        let int = py.get_type::<PyInt>();
        let fields = PyList::new(py, [("lower_bound", &int), ("capacity", &int)])?;
        let named_tuple = py.import(intern!(py, "typing"))?.getattr(intern!(py, "NamedTuple"))?;
        let class = named_tuple.call1(("Bounds", fields))?.cast_into::<PyType>()?;
        class.setattr(intern!(py, "__module__"), "kahlnws")?;
        class.setattr(intern!(py, "__doc__"), DOC)?;

        Ok(class.unbind())
    })?;

    Ok(class.bind(py))
}

/// `bounds` as a `kahlnws.Bounds`.
///
/// # Errors
/// Passes on what Python raises while the class or the tuple is made, which only a lack of memory makes.
pub(crate) fn to_python(py: Python<'_>, bounds: kahlnws::Bounds) -> PyResult<Bound<'_, PyAny>> {
    let kahlnws::Bounds { lower_bound, capacity } = bounds;

    class(py)?.call1((lower_bound, capacity))
}

/// The bounds `object`, the argument Python gave for `bounds`, holds: a sequence of two integers, the lower bound
/// first, such as a `Bounds` or a plain tuple, each converted as the number of that name is.
///
/// # Errors
/// Raises the `TypeError` of [`ArgumentError::BoundsNotAPair`] where `object` is no sequence, the `ValueError` of
/// [`ArgumentError::BoundsLength`] where it holds other than two items, and what [`integer::flow`] raises for an
/// item. Passes on what Python raises while the sequence is measured or indexed.
pub(crate) fn from_python(object: &Bound<'_, PyAny>) -> PyResult<kahlnws::Bounds> {
    let Ok(pair) = object.cast::<PySequence>() else {
        return Err(ArgumentError::BoundsNotAPair { type_name: type_name(object)? }.into());
    };
    let length = pair.len()?;
    if length != 2 {
        return Err(ArgumentError::BoundsLength { length }.into());
    }

    let lower_bound = integer::flow(FlowNumber::LowerBound, &pair.get_item(0)?)?;
    let capacity = integer::flow(FlowNumber::Capacity, &pair.get_item(1)?)?;
    Ok(kahlnws::Bounds { lower_bound, capacity })
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

    /// The value of the Python expression `expression`, which may name `Bounds`.
    fn evaluate<'py>(py: Python<'py>, expression: &str) -> PyResult<Bound<'py, PyAny>> {
        let code = std::ffi::CString::new(expression).expect("an expression should hold no NUL");
        let globals = PyDict::new(py);
        globals.set_item("Bounds", class(py)?)?;

        py.eval(&code, Some(&globals), None)
    }

    #[test]
    fn test_the_class_is_a_named_tuple_of_the_package_made_once_by_hand() {
        with_python(|py| {
            let made = class(py)?;
            assert!(made.is(class(py)?), "the same class every time");
            assert_eq!(made.getattr("__module__")?.extract::<String>()?, "kahlnws");
            assert_eq!(
                made.getattr("_fields")?.extract::<(String, String)>()?,
                ("lower_bound".to_owned(), "capacity".to_owned())
            );
            assert_eq!(made.getattr("__doc__")?.extract::<String>()?, DOC);

            let read = to_python(py, kahlnws::Bounds { lower_bound: 1, capacity: u64::MAX })?;
            assert!(read.is_instance(made)?);
            assert_eq!(read.repr()?.to_string(), format!("Bounds(lower_bound=1, capacity={})", u64::MAX));
            assert_eq!(read.getattr("capacity")?.extract::<u64>()?, u64::MAX);

            Ok(())
        });
    }

    #[test]
    fn test_from_python_takes_any_pair_of_two_integers_and_names_what_it_refuses_by_hand() {
        with_python(|py| {
            let taken = ["Bounds(1, 5)", "(1, 5)", "[1, 5]", "range(1, 6, 4)"]
                .map(|expression| evaluate(py, expression).and_then(|pair| from_python(&pair)));
            for bounds in taken {
                assert_eq!(bounds?, kahlnws::Bounds { lower_bound: 1, capacity: 5 });
            }

            let refused = ["3", "{1, 5}", "(1, 5, 7)", "()", "(-1, 5)", "(1, 'five')"].map(|expression| {
                evaluate(py, expression)
                    .and_then(|pair| from_python(&pair))
                    .map_err(|error| error.value(py).to_string())
            });
            assert_eq!(
                refused,
                [
                    Err("bounds is of type int, not a pair of integers".to_owned()),
                    Err("bounds is of type set, not a pair of integers".to_owned()),
                    Err("bounds has length 3, not 2".to_owned()),
                    Err("bounds has length 0, not 2".to_owned()),
                    Err(format!(
                        "lower_bound -1 is outside the range of an unsigned 64-bit integer, from 0 to {}",
                        u64::MAX
                    )),
                    Err("capacity is of type str, not an integer".to_owned()),
                ]
            );

            Ok(())
        });
    }
}
