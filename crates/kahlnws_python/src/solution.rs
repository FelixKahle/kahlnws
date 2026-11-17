// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What `NetworkSimplex.solve` finds for a problem that has a flow, owned by Python.

use pyo3::PyTypeInfo;
use pyo3::basic::CompareOp;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple, PyType};

/// A flow of least cost, with the potentials that prove it optimal and its cost.
///
/// What `NetworkSimplex.solve` finds for a problem that has a flow. Each attribute answers the same object each time it
/// is read; the lists are the solution's own, and no solve writes them again.
///
/// Plain data: compared by value, and hashed by value as the tuple of its parts is, so a solution whose lists the
/// caller changes compares and hashes as changed; copied and pickled.
///
/// Args:
///     flows: The flow of each arc, any iterable of `int`, kept as a list.
///     potentials: The potential of each node, any iterable of `int`, kept as a list.
///     total_cost: The cost of the flow.
///
/// Raises:
///     TypeError: `flows` or `potentials` cannot be iterated.
///     OverflowError: `total_cost` lies outside `-2**127` to `2**127 - 1`.
///
/// Example:
///     >>> kahlnws.Solution([2, 2, 2], [0, 1, 5], 14)
///     Solution(flows=[2, 2, 2], potentials=[0, 1, 5], total_cost=14)
#[pyclass(module = "kahlnws", frozen)]
#[derive(Debug)]
pub(crate) struct Solution {
    /// The flow of each arc, in units of flow, arc `a` at `a`: a list of `int`, each from 0 to `2**64 - 1`.
    #[pyo3(get)]
    flows: Py<PyList>,
    /// Each node's potential, in units of cost, node `v` at `v`: a list of `int`, each from `-2**63` to `2**63 - 1`.
    /// With them, `costs[a] + potentials[tails[a]] - potentials[heads[a]]` is at least zero on every arc whose flow
    /// is below its capacity and at most zero on every arc whose flow is above its lower bound, which proves the flow
    /// optimal.
    #[pyo3(get)]
    potentials: Py<PyList>,
    /// The cost of the flow, in units of cost times units of flow: an exact `int` from `-2**127` to `2**127 - 1`.
    #[pyo3(get)]
    total_cost: i128,
}

impl Solution {
    /// The solution of the columns `flows` and `potentials` and of `total_cost`, its lists made from the columns.
    ///
    /// # Errors
    /// Passes on the `MemoryError` of a list or an `int` that cannot be made.
    pub(crate) fn of_columns(py: Python<'_>, flows: &[u64], potentials: &[i64], total_cost: i128) -> PyResult<Self> {
        let flows = PyList::new(py, flows)?.unbind();
        let potentials = PyList::new(py, potentials)?.unbind();

        Ok(Self { flows, potentials, total_cost })
    }

    /// The flows, the potentials and the cost as one tuple, which equality and the hash are taken of.
    ///
    /// # Errors
    /// Passes on the `MemoryError` of a tuple that cannot be made.
    fn parts<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let (flows, potentials) = (self.flows.bind(py).to_tuple(), self.potentials.bind(py).to_tuple());

        PyTuple::new(py, [flows.into_any(), potentials.into_any(), self.total_cost.into_pyobject(py)?.into_any()])
    }
}

#[pymethods]
impl Solution {
    #[new]
    #[pyo3(signature = (flows, potentials, total_cost))]
    fn new(flows: &Bound<'_, PyAny>, potentials: &Bound<'_, PyAny>, total_cost: i128) -> PyResult<Self> {
        let list = |items: &Bound<'_, PyAny>| -> PyResult<Py<PyList>> {
            Ok(PyList::type_object(items.py()).call1((items,))?.cast_into::<PyList>()?.unbind())
        };

        Ok(Self { flows: list(flows)?, potentials: list(potentials)?, total_cost })
    }

    #[classattr]
    const fn __match_args__() -> (&'static str, &'static str, &'static str) {
        ("flows", "potentials", "total_cost")
    }

    /// The solution as Python would make it: `Solution(flows=[2, 2, 2], potentials=[0, 1, 5], total_cost=14)`.
    ///
    /// # Errors
    /// Passes on what the `repr` of a list raises, which is only a `MemoryError`.
    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!(
            "Solution(flows={}, potentials={}, total_cost={})",
            self.flows.bind(py).repr()?,
            self.potentials.bind(py).repr()?,
            self.total_cost
        ))
    }

    /// Whether `other` is a solution of equal flows, potentials and cost; `NotImplemented` for another type.
    ///
    /// # Errors
    /// Passes on what the comparison of the lists raises.
    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: CompareOp) -> PyResult<Py<PyAny>> {
        let py = other.py();
        let Ok(other) = other.cast::<Self>() else {
            return Ok(py.NotImplemented());
        };

        let equal = self.parts(py)?.eq(other.get().parts(py)?)?;
        match op {
            CompareOp::Eq => Ok(equal.into_pyobject(py)?.to_owned().into_any().unbind()),
            CompareOp::Ne => Ok((!equal).into_pyobject(py)?.to_owned().into_any().unbind()),
            CompareOp::Lt | CompareOp::Le | CompareOp::Gt | CompareOp::Ge => Ok(py.NotImplemented()),
        }
    }

    /// The hash of the flows, the potentials and the cost, as a tuple of the three is hashed.
    ///
    /// # Errors
    /// Passes on what hashing an entry of a list raises.
    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        self.parts(py)?.hash()
    }

    /// The class and copies of the flows, the potentials and the cost, from which `copy` and `pickle` make the
    /// solution again.
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<(Bound<'py, PyType>, Bound<'py, PyTuple>)> {
        Ok((slf.as_any().get_type(), slf.get().parts(slf.py())?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_of_columns_makes_lists_of_the_columns_and_writes_itself_as_python_would_by_hand() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let solution = Solution::of_columns(py, &[2, 2, 2], &[0, 1, 5], -(1 << 100))?;

            assert_eq!(solution.flows.bind(py).extract::<Vec<u64>>()?, [2, 2, 2]);
            assert_eq!(solution.potentials.bind(py).extract::<Vec<i64>>()?, [0, 1, 5]);
            assert_eq!(
                solution.__repr__(py)?,
                "Solution(flows=[2, 2, 2], potentials=[0, 1, 5], total_cost=-1267650600228229401496703205376)"
            );

            let same = Solution::of_columns(py, &[2, 2, 2], &[0, 1, 5], -(1 << 100))?;
            assert!(solution.parts(py)?.eq(same.parts(py)?)?);
            assert_eq!(solution.__hash__(py)?, same.__hash__(py)?);

            Ok(())
        })
        .expect("a solution should be made");
    }
}
