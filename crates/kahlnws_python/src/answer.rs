// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a solve answers in Python: what it found, beside the work it did to find it.

use pyo3::basic::CompareOp;
use pyo3::prelude::*;
use pyo3::types::{PyTuple, PyType};

/// What one solve did, counted in pivots: exact counts of that solve, not samples or estimates.
///
/// A pivot brings one arc that violates its bound into the basis and sends the most flow the cycle it closes with the
/// tree carries around that cycle. Where a link of the tree has the least room, that link leaves and the arc takes its
/// place; where the arc's own width is the least, no link leaves and the arc moves to its other bound, a flip, which
/// is a pivot too.
///
/// The counts of `solve` and `solve_into` depend on the problem as given, its arc order included, and on the version
/// of the package, as the answer does: the same problem gives the same work from a fresh solver and from one that
/// solved others before. Those of `resolve` and `resolve_into` depend on the problem and on the solves the solver made
/// before it, and are those of `solve` where the basis it kept cannot serve. A solve answers
/// `degenerate_pivots <= pivots` and `first_arc_pivots <= pivots`; the pivots that sent flow are
/// `pivots - degenerate_pivots`. The block search that chooses the other pivots runs `pivots - first_arc_pivots + 1`
/// times, once for each of them and once more to find that no arc violates its bound, so it is not counted apart.
///
/// Plain data: immutable, compared and hashed by value, matched by position in the order of its attributes, copied
/// and pickled.
///
/// Args:
///     pivots: The pivots a solve made.
///     degenerate_pivots: Of them, those that sent no flow.
///     first_arc_pivots: Of them, those made by the arcs tried before block search.
///
/// Raises:
///     OverflowError: A count is below zero or past `2**64 - 1`.
///
/// Example:
///     >>> kahlnws.Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0).pivots
///     4
#[pyclass(module = "kahlnws", frozen, eq, hash)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Work {
    /// The pivots the solve made, flips included: one for each arc brought into the basis. Zero where no arc ever
    /// violated its bound, as in a problem of no arc.
    #[pyo3(get)]
    pivots: u64,
    /// Of the pivots, those that sent no flow: the link that left had no room, so the flow stayed as it was and only
    /// the tree and the potentials changed. At most `pivots`; a flip is never one.
    #[pyo3(get)]
    degenerate_pivots: u64,
    /// Of the pivots, those made by the arcs the solve tries before block search, chosen from the problem's
    /// structure, each where it violated its bound when its turn came. At most `pivots`: 0 in a resolve from the basis
    /// the solver kept, which tries none, and `pivots` where those arcs alone reached the optimum.
    #[pyo3(get)]
    first_arc_pivots: u64,
}

/// What a solve answers: what it found, and the work it did to find it, as one value.
///
/// `NetworkSimplex.solve` and `NetworkSimplex.resolve` answer an `Answer[Solution]`: its `outcome` is the `Solution`,
/// or `None` where no flow meets the supplies. `NetworkSimplex.solve_into` and `NetworkSimplex.resolve_into` answer an
/// `Answer[int]`: its `outcome` is the cost of the flow written, or `None`. The work is answered either way. A refused
/// problem raises and answers nothing, since no solve ran.
///
/// Plain data: immutable, compared and hashed by value (its outcome and its work), matched by position, copied and
/// pickled.
///
/// Args:
///     outcome: What a solve found, or `None`.
///     work: The work it did.
///
/// Example:
///     >>> answer = kahlnws.Answer(14, kahlnws.Work(4, 1, 0))
///     >>> answer.outcome, answer.work.pivots
///     (14, 4)
#[pyclass(module = "kahlnws", frozen)]
#[derive(Debug)]
pub(crate) struct Answer {
    /// What the solve found: the `Solution` of `solve` and `resolve` or the cost of `solve_into` and `resolve_into`
    /// where a flow meets the supplies, `None` where none does.
    #[pyo3(get)]
    outcome: Py<PyAny>,
    /// The work the solve did, optimal or infeasible alike.
    #[pyo3(get)]
    work: Py<Work>,
}

impl From<kahlnws::Work> for Work {
    fn from(work: kahlnws::Work) -> Self {
        let kahlnws::Work { pivots, degenerate_pivots, first_arc_pivots } = work;

        Self { pivots, degenerate_pivots, first_arc_pivots }
    }
}

impl Answer {
    /// The answer of a solve that found `outcome`, `None` where the problem is infeasible, and did `work`.
    ///
    /// # Errors
    /// Passes on the `MemoryError` of an object that cannot be made.
    pub(crate) fn answered<'py>(
        py: Python<'py>,
        outcome: Option<impl IntoPyObject<'py>>,
        work: kahlnws::Work,
    ) -> PyResult<Self> {
        let outcome = outcome.into_pyobject(py).map_err(Into::into)?.into_any().unbind();
        let work = Py::new(py, Work::from(work))?;

        Ok(Self { outcome, work })
    }
}

#[pymethods]
impl Work {
    #[new]
    #[pyo3(signature = (pivots, degenerate_pivots, first_arc_pivots))]
    const fn new(pivots: u64, degenerate_pivots: u64, first_arc_pivots: u64) -> Self {
        Self { pivots, degenerate_pivots, first_arc_pivots }
    }

    #[classattr]
    const fn __match_args__() -> (&'static str, &'static str, &'static str) {
        ("pivots", "degenerate_pivots", "first_arc_pivots")
    }

    fn __repr__(&self) -> String {
        let Self { pivots, degenerate_pivots, first_arc_pivots } = self;

        format!("Work(pivots={pivots}, degenerate_pivots={degenerate_pivots}, first_arc_pivots={first_arc_pivots})")
    }

    /// The class and the counts, from which `copy` and `pickle` make the work again.
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> (Bound<'py, PyType>, (u64, u64, u64)) {
        let Self { pivots, degenerate_pivots, first_arc_pivots } = *slf.get();

        (slf.as_any().get_type(), (pivots, degenerate_pivots, first_arc_pivots))
    }
}

#[pymethods]
impl Answer {
    #[new]
    #[pyo3(signature = (outcome, work))]
    const fn new(outcome: Py<PyAny>, work: Py<Work>) -> Self {
        Self { outcome, work }
    }

    #[classattr]
    const fn __match_args__() -> (&'static str, &'static str) {
        ("outcome", "work")
    }

    /// The class `Answer` with its outcome's type, as `list[int]` is a list with its items' type: `Answer[int]`.
    ///
    /// # Errors
    /// Passes on what Python raises while the alias is made.
    #[classmethod]
    fn __class_getitem__<'py>(class: &Bound<'py, PyType>, item: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        class.py().import("types")?.getattr("GenericAlias")?.call1((class, item))
    }

    /// The answer as Python would make it: `Answer(outcome=14, work=Work(pivots=4, ...))`.
    ///
    /// # Errors
    /// Passes on what the `repr` of the outcome raises.
    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        Ok(format!("Answer(outcome={}, work={})", self.outcome.bind(py).repr()?, self.work.get().__repr__()))
    }

    /// Whether `other` is an answer of an equal outcome and an equal work; `NotImplemented` for another type.
    ///
    /// # Errors
    /// Passes on what the comparison of the outcomes raises.
    fn __richcmp__(&self, other: &Bound<'_, PyAny>, op: CompareOp) -> PyResult<Py<PyAny>> {
        let py = other.py();
        let Ok(other) = other.cast::<Self>() else {
            return Ok(py.NotImplemented());
        };
        let other = other.get();

        let equal = self.work.get() == other.work.get() && self.outcome.bind(py).eq(other.outcome.bind(py))?;
        match op {
            CompareOp::Eq => Ok(equal.into_pyobject(py)?.to_owned().into_any().unbind()),
            CompareOp::Ne => Ok((!equal).into_pyobject(py)?.to_owned().into_any().unbind()),
            CompareOp::Lt | CompareOp::Le | CompareOp::Gt | CompareOp::Ge => Ok(py.NotImplemented()),
        }
    }

    /// The hash of the outcome and the work together.
    ///
    /// # Errors
    /// Passes on the `TypeError` of an outcome that cannot be hashed.
    fn __hash__(&self, py: Python<'_>) -> PyResult<isize> {
        PyTuple::new(py, [self.outcome.bind(py).clone(), self.work.bind(py).clone().into_any()])?.hash()
    }

    /// The class and the outcome and the work, from which `copy` and `pickle` make the answer again.
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> (Bound<'py, PyType>, (Py<PyAny>, Py<Work>)) {
        let py = slf.py();
        let answer = slf.get();

        (slf.as_any().get_type(), (answer.outcome.clone_ref(py), answer.work.clone_ref(py)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_work_is_the_library_s_counts_and_writes_itself_as_python_makes_it_by_hand() {
        let work = Work::from(kahlnws::Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 });

        assert_eq!(work, Work::new(4, 1, 0));
        assert_eq!(work.__repr__(), "Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0)");
    }

    #[test]
    fn test_answered_holds_the_outcome_or_none_and_the_work_by_hand() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let work = kahlnws::Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 0 };

            let optimal = Answer::answered(py, Some(14_i128), work)?;
            assert_eq!(optimal.outcome.extract::<i128>(py)?, 14);
            assert_eq!(*optimal.work.get(), Work::new(1, 0, 0));
            assert_eq!(
                optimal.__repr__(py)?,
                "Answer(outcome=14, work=Work(pivots=1, degenerate_pivots=0, first_arc_pivots=0))"
            );

            let infeasible = Answer::answered(py, None::<i128>, work)?;
            assert!(infeasible.outcome.is_none(py));

            Ok(())
        })
        .expect("an answer should be made");
    }
}
