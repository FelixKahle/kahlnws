// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Why a solve refused a problem, in Python's terms: the exception `ProblemError` and the breach it carries.

use pyo3::create_exception;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyTuple, PyType};

create_exception!(
    kahlnws,
    ProblemError,
    PyValueError,
    "A problem no solve admits, raised by `NetworkSimplex.solve` and `NetworkSimplex.solve_into`.

The problem has a table of the wrong length, too many nodes or arcs, an arc whose numbers lie outside the range
a solve admits, or supplies that do not balance. A problem with several breaches is refused with the first, in the
order of the classes of `Breach`, where those about one arc are taken arc by arc: the lowest arc with any of them,
and its first. A refused solve answers no work, and may still have grown the solver's room.

Attributes:
    breach: The breach as a `Breach`, whose class names the kind and whose attributes hold the values; `None` on a
        `ProblemError` made by hand rather than raised by a solve.

Example:
    >>> try:
    ...     kahlnws.NetworkSimplex().solve(tails=[0], heads=[5], capacities=[1], costs=[1], supplies=[1, -1])
    ... except kahlnws.ProblemError as error:
    ...     print(error)
    ...     print(repr(error.breach))
    arc 0 enters node 5, not below the node count 2
    Breach.HeadOutsideNodes(arc=0, head=5, node_count=2)"
);

/// One of the per-arc tables of a problem whose length is compared with the tails', as `Breach.Length` names it.
///
/// Its members are `Table.HEADS`, `Table.LOWER_BOUNDS`, `Table.CAPACITIES` and `Table.COSTS`; they compare and hash by
/// value, convert to their `int` in that order from 0, and are copied and pickled. A table is not made by calling the
/// class.
#[pyclass(module = "kahlnws", frozen, eq, hash, from_py_object, rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Table {
    /// The heads.
    Heads,
    /// The lower bounds, where the problem gives them.
    LowerBounds,
    /// The capacities.
    Capacities,
    /// The costs.
    Costs,
}

/// Why a problem is no problem a solve admits: one class per kind of breach, whose attributes hold its values.
///
/// The classes are `Breach.Length`, `Breach.TooLarge`, `Breach.TailOutsideNodes`, `Breach.HeadOutsideNodes`,
/// `Breach.LowerAboveCapacity`, `Breach.CostBeyondBound`, `Breach.CostTotalBeyondRange`, `Breach.Unbalanced` and
/// `Breach.NetSupplyBeyondRange`, in the order in which a problem's breaches are looked for. Each is made from its
/// values, by keyword or by position; `Breach` itself is not made by calling it.
///
/// Plain data: immutable, compared and hashed by value, matched by position in the order of its attributes, copied
/// and pickled.
///
/// Example:
///     >>> match kahlnws.Breach.TailOutsideNodes(arc=2, tail=9, node_count=4):
///     ...     case kahlnws.Breach.TailOutsideNodes(arc, tail):
///     ...         print(arc, tail)
///     2 9
#[pyclass(module = "kahlnws", frozen, eq, hash)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Breach {
    /// A per-arc table holds another number of entries than the tails: `table` has `length` entries where the tails
    /// have `expected`.
    Length {
        /// The table.
        table: Table,
        /// Its number of entries.
        length: usize,
        /// The number of tails, one per arc.
        expected: usize,
    },
    /// The problem has more nodes or arcs than a solve counts: `node_count + 1` or `arc_count + node_count` passes
    /// `2**32 - 1`.
    TooLarge {
        /// The number of nodes, one per supply.
        node_count: usize,
        /// The number of arcs, one per tail.
        arc_count: usize,
    },
    /// Arc `arc` leaves the node `tail`, which the problem of `node_count` nodes does not have.
    TailOutsideNodes {
        /// The arc.
        arc: usize,
        /// Its tail.
        tail: usize,
        /// The number of nodes.
        node_count: usize,
    },
    /// Arc `arc` enters the node `head`, which the problem of `node_count` nodes does not have.
    HeadOutsideNodes {
        /// The arc.
        arc: usize,
        /// Its head.
        head: usize,
        /// The number of nodes.
        node_count: usize,
    },
    /// The lower bound of arc `arc` passes its capacity.
    LowerAboveCapacity {
        /// The arc.
        arc: usize,
        /// Its lower bound, in units of flow.
        lower_bound: u64,
        /// Its capacity, in units of flow.
        capacity: u64,
    },
    /// The cost of arc `arc` lies beyond plus or minus `cost_bound(node_count)`.
    CostBeyondBound {
        /// The arc.
        arc: usize,
        /// Its cost, in units of cost.
        cost: i64,
        /// The number of nodes, which sets the bound.
        node_count: usize,
    },
    /// The magnitudes of the costs times the capacities, added up in arc order, pass `2**127 - 1` at arc `arc`.
    CostTotalBeyondRange {
        /// The arc at which the sum passes `2**127 - 1`.
        arc: usize,
    },
    /// The supplies add up to `total`, not zero.
    Unbalanced {
        /// Their sum, in units of flow.
        total: i128,
    },
    /// The supplies net of the lower bounds add up to `total` where they are positive, past `2**64 - 2`. A node's
    /// net supply is its supply, less the lower bounds of the arcs that leave it, plus those of the arcs that enter
    /// it.
    NetSupplyBeyondRange {
        /// Their sum, in units of flow.
        total: u128,
    },
}

#[pymethods]
impl Table {
    /// `getattr` and the class and member name, from which `copy` and `pickle` find the member again.
    ///
    /// # Errors
    /// Passes on what Python raises while `getattr` is looked up.
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyTuple>> {
        let getattr = slf.py().import("builtins")?.getattr("getattr")?;

        (getattr, (slf.as_any().get_type(), slf.get().name())).into_pyobject(slf.py())
    }
}

#[pymethods]
impl Breach {
    /// The breach's class and its values, from which `copy` and `pickle` make it again.
    ///
    /// # Errors
    /// Passes on the `MemoryError` of a tuple that cannot be made.
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<(Bound<'py, PyType>, Bound<'py, PyTuple>)> {
        let py = slf.py();
        let values = match *slf.get() {
            Self::Length { table, length, expected } => {
                (table.into_pyobject(py)?.into_any(), length, expected).into_pyobject(py)?
            }
            Self::TooLarge { node_count, arc_count } => (node_count, arc_count).into_pyobject(py)?,
            Self::TailOutsideNodes { arc, tail, node_count } => (arc, tail, node_count).into_pyobject(py)?,
            Self::HeadOutsideNodes { arc, head, node_count } => (arc, head, node_count).into_pyobject(py)?,
            Self::LowerAboveCapacity { arc, lower_bound, capacity } => {
                (arc, lower_bound, capacity).into_pyobject(py)?
            }
            Self::CostBeyondBound { arc, cost, node_count } => (arc, cost, node_count).into_pyobject(py)?,
            Self::CostTotalBeyondRange { arc } => (arc,).into_pyobject(py)?,
            Self::Unbalanced { total } => (total,).into_pyobject(py)?,
            Self::NetSupplyBeyondRange { total } => (total,).into_pyobject(py)?,
        };

        Ok((slf.as_any().get_type(), values))
    }

    /// The breach as Python would make it: `Breach.TailOutsideNodes(arc=2, tail=9, node_count=4)`.
    fn __repr__(&self) -> String {
        match *self {
            Self::Length { table, length, expected } => {
                format!("Breach.Length(table=Table.{}, length={length}, expected={expected})", table.name())
            }
            Self::TooLarge { node_count, arc_count } => {
                format!("Breach.TooLarge(node_count={node_count}, arc_count={arc_count})")
            }
            Self::TailOutsideNodes { arc, tail, node_count } => {
                format!("Breach.TailOutsideNodes(arc={arc}, tail={tail}, node_count={node_count})")
            }
            Self::HeadOutsideNodes { arc, head, node_count } => {
                format!("Breach.HeadOutsideNodes(arc={arc}, head={head}, node_count={node_count})")
            }
            Self::LowerAboveCapacity { arc, lower_bound, capacity } => {
                format!("Breach.LowerAboveCapacity(arc={arc}, lower_bound={lower_bound}, capacity={capacity})")
            }
            Self::CostBeyondBound { arc, cost, node_count } => {
                format!("Breach.CostBeyondBound(arc={arc}, cost={cost}, node_count={node_count})")
            }
            Self::CostTotalBeyondRange { arc } => format!("Breach.CostTotalBeyondRange(arc={arc})"),
            Self::Unbalanced { total } => format!("Breach.Unbalanced(total={total})"),
            Self::NetSupplyBeyondRange { total } => format!("Breach.NetSupplyBeyondRange(total={total})"),
        }
    }
}

impl Table {
    /// The member's name in the class, as `rename_all` makes it and the default `repr` writes it after `Table.`.
    const fn name(self) -> &'static str {
        match self {
            Self::Heads => "HEADS",
            Self::LowerBounds => "LOWER_BOUNDS",
            Self::Capacities => "CAPACITIES",
            Self::Costs => "COSTS",
        }
    }
}

impl From<kahlnws::Table> for Table {
    fn from(table: kahlnws::Table) -> Self {
        match table {
            kahlnws::Table::Heads => Self::Heads,
            kahlnws::Table::LowerBounds => Self::LowerBounds,
            kahlnws::Table::Capacities => Self::Capacities,
            kahlnws::Table::Costs => Self::Costs,
        }
    }
}

impl From<kahlnws::ProblemError> for Breach {
    fn from(breach: kahlnws::ProblemError) -> Self {
        use kahlnws::ProblemError as Error;

        match breach {
            Error::Length { table, length, expected } => Self::Length { table: table.into(), length, expected },
            Error::TooLarge { node_count, arc_count } => Self::TooLarge { node_count, arc_count },
            Error::TailOutsideNodes { arc, tail, node_count } => Self::TailOutsideNodes { arc, tail, node_count },
            Error::HeadOutsideNodes { arc, head, node_count } => Self::HeadOutsideNodes { arc, head, node_count },
            Error::LowerAboveCapacity { arc, lower_bound, capacity } => {
                Self::LowerAboveCapacity { arc, lower_bound, capacity }
            }
            Error::CostBeyondBound { arc, cost, node_count } => Self::CostBeyondBound { arc, cost, node_count },
            Error::CostTotalBeyondRange { arc } => Self::CostTotalBeyondRange { arc },
            Error::Unbalanced { total } => Self::Unbalanced { total },
            Error::NetSupplyBeyondRange { total } => Self::NetSupplyBeyondRange { total },
        }
    }
}

/// The exception that tells Python a solve refused its problem for `breach`: a [`ProblemError`] whose message is the
/// breach's text, and whose attribute `breach` holds the breach as a [`Breach`].
pub(crate) fn refusal(py: Python<'_>, breach: kahlnws::ProblemError) -> PyErr {
    let exception = ProblemError::new_err(breach.to_string());
    let attached =
        Breach::from(breach).into_pyobject(py).and_then(|breach| exception.value(py).setattr("breach", breach));

    // Making a small object or setting an attribute of a fresh exception fails only where memory runs out; then that
    // failure is raised in its place.
    match attached {
        Ok(()) => exception,
        Err(failure) => failure,
    }
}

#[cfg(test)]
mod tests {
    use kahlnws::{ProblemError as Error, cost_bound};

    use super::*;

    /// Every breach of the library with values that differ from field to field, the breach it becomes, and that
    /// breach as Python writes it.
    fn breaches() -> [(Error, Breach, String); 9] {
        let bound = cost_bound(4);

        [
            (
                Error::Length { table: kahlnws::Table::LowerBounds, length: 2, expected: 3 },
                Breach::Length { table: Table::LowerBounds, length: 2, expected: 3 },
                "Breach.Length(table=Table.LOWER_BOUNDS, length=2, expected=3)".to_owned(),
            ),
            (
                Error::TooLarge { node_count: 4_294_967_295, arc_count: 7 },
                Breach::TooLarge { node_count: 4_294_967_295, arc_count: 7 },
                "Breach.TooLarge(node_count=4294967295, arc_count=7)".to_owned(),
            ),
            (
                Error::TailOutsideNodes { arc: 2, tail: 9, node_count: 4 },
                Breach::TailOutsideNodes { arc: 2, tail: 9, node_count: 4 },
                "Breach.TailOutsideNodes(arc=2, tail=9, node_count=4)".to_owned(),
            ),
            (
                Error::HeadOutsideNodes { arc: 3, head: 8, node_count: 5 },
                Breach::HeadOutsideNodes { arc: 3, head: 8, node_count: 5 },
                "Breach.HeadOutsideNodes(arc=3, head=8, node_count=5)".to_owned(),
            ),
            (
                Error::LowerAboveCapacity { arc: 1, lower_bound: 6, capacity: 5 },
                Breach::LowerAboveCapacity { arc: 1, lower_bound: 6, capacity: 5 },
                "Breach.LowerAboveCapacity(arc=1, lower_bound=6, capacity=5)".to_owned(),
            ),
            (
                Error::CostBeyondBound { arc: 1, cost: -bound - 1, node_count: 4 },
                Breach::CostBeyondBound { arc: 1, cost: -bound - 1, node_count: 4 },
                format!("Breach.CostBeyondBound(arc=1, cost={}, node_count=4)", -bound - 1),
            ),
            (
                Error::CostTotalBeyondRange { arc: 2 },
                Breach::CostTotalBeyondRange { arc: 2 },
                "Breach.CostTotalBeyondRange(arc=2)".to_owned(),
            ),
            (
                Error::Unbalanced { total: i128::MIN },
                Breach::Unbalanced { total: i128::MIN },
                format!("Breach.Unbalanced(total={})", i128::MIN),
            ),
            (
                Error::NetSupplyBeyondRange { total: u128::MAX },
                Breach::NetSupplyBeyondRange { total: u128::MAX },
                format!("Breach.NetSupplyBeyondRange(total={})", u128::MAX),
            ),
        ]
    }

    #[test]
    fn test_each_breach_by_hand() {
        for (error, breach, _) in breaches() {
            assert_eq!(Breach::from(error), breach, "{error:?}");
        }
    }

    #[test]
    fn test_each_breach_as_python_writes_it_by_hand() {
        for (_, breach, written) in breaches() {
            assert_eq!(breach.__repr__(), written);
        }
    }

    #[test]
    fn test_each_table_by_hand() {
        let tables =
            [kahlnws::Table::Heads, kahlnws::Table::LowerBounds, kahlnws::Table::Capacities, kahlnws::Table::Costs]
                .map(Table::from);

        assert_eq!(tables, [Table::Heads, Table::LowerBounds, Table::Capacities, Table::Costs]);
        assert_eq!(tables.map(Table::name), ["HEADS", "LOWER_BOUNDS", "CAPACITIES", "COSTS"]);
    }

    #[test]
    fn test_refusal_raises_problem_error_with_the_library_s_message_and_the_breach_by_hand() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let error = refusal(py, Error::TailOutsideNodes { arc: 2, tail: 9, node_count: 4 });

            assert!(error.is_instance_of::<ProblemError>(py));
            assert!(error.is_instance_of::<PyValueError>(py));
            assert_eq!(error.value(py).to_string(), "arc 2 leaves node 9, not below the node count 4");
            let breach = error.value(py).getattr("breach")?;
            assert_eq!(*breach.cast::<Breach>()?.get(), Breach::TailOutsideNodes { arc: 2, tail: 9, node_count: 4 });

            Ok(())
        })
        .expect("a refusal should carry its breach");
    }
}
