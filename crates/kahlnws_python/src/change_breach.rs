// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Why a network refused a change, in Python's terms: the exception `ChangeError` and the breach it carries.

use pyo3::create_exception;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyTuple, PyType};

use crate::breach::with_breach;

create_exception!(
    kahlnws,
    ChangeError,
    PyValueError,
    "A change no network admits, raised by `Network.set_cost`, `set_capacity`, `set_lower_bound`, `set_bounds`,
`set_supply` and `move_supply`.

The number set lies outside the range a solve admits: a cost past the bound of the node count, a lower bound above
its capacity, the costs times the capacities past their range, a supply past a signed 64-bit integer, or more than
`2**64 - 2` units of flow still to be sent. A refused change changes nothing: the network holds the problem it held,
and the optimum it held. A breach a problem could have too reads alike, with the same name, values and message as
the class of `Breach`, but is a class of `ChangeBreach`: a problem can be refused for what no change makes, and a
change for what no problem meets.

Attributes:
    breach: The breach as a `ChangeBreach`, whose class names the kind and whose attributes hold the values; `None`
        on a `ChangeError` made by hand rather than raised by a change.

Example:
    >>> network = kahlnws.Network()
    >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4])
    >>> try:
    ...     network.set_lower_bound(0, 4)
    ... except kahlnws.ChangeError as error:
    ...     print(error)
    ...     print(repr(error.breach))
    arc 0 has lower bound 4 above its capacity 3
    ChangeBreach.LowerAboveCapacity(arc=0, lower_bound=4, capacity=3)"
);

/// Why a change is no change a network admits: one class per kind of breach, whose attributes hold its values.
///
/// The classes are `ChangeBreach.LowerAboveCapacity`, `ChangeBreach.CostBeyondBound`,
/// `ChangeBreach.CostTotalBeyondRange`, `ChangeBreach.SupplyBeyondRange` and `ChangeBreach.ExcessBeyondRange`. Each is
/// made from its values, by keyword or by position; `ChangeBreach` itself is not made by calling it.
///
/// Plain data: immutable, compared and hashed by value, matched by position in the order of its attributes, copied
/// and pickled.
///
/// Example:
///     >>> match kahlnws.ChangeBreach.SupplyBeyondRange(node=2, supply=-(2**63) - 1):
///     ...     case kahlnws.ChangeBreach.SupplyBeyondRange(node, supply):
///     ...         print(node, supply)
///     2 -9223372036854775809
#[pyclass(module = "kahlnws", frozen, eq, hash)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ChangeBreach {
    /// The lower bound set on arc `arc` passes its capacity, either set with it or the one it holds.
    LowerAboveCapacity {
        /// The arc.
        arc: usize,
        /// Its lower bound, in units of flow.
        lower_bound: u64,
        /// Its capacity, in units of flow.
        capacity: u64,
    },
    /// The cost set on arc `arc` lies beyond plus or minus `cost_bound(node_count)`.
    CostBeyondBound {
        /// The arc.
        arc: usize,
        /// The cost set on it, in units of cost.
        cost: i64,
        /// The number of nodes, which sets the bound.
        node_count: usize,
    },
    /// With the cost or the capacity set on arc `arc`, the magnitudes of the costs times the capacities would add up
    /// past `2**127 - 1`.
    CostTotalBeyondRange {
        /// The arc changed.
        arc: usize,
    },
    /// A move of supply would take the supply of node `node` past the range of a signed 64-bit integer: below
    /// `-2**63` for the node it is taken from, above `2**63 - 1` for the node it is added to.
    SupplyBeyondRange {
        /// The node, the one the amount is taken from where both would leave the range.
        node: usize,
        /// The supply it would have, in units of flow.
        supply: i128,
    },
    /// A change would leave more flow still to be sent than a network holds between solves: what the last load or a
    /// solve that found no flow left to be sent, with what changes since moved that the arcs could not carry at once,
    /// plus this change's amount, adds up to `total`, past `2**64 - 2`. A load leaves to be sent what its start does
    /// not route, up to the whole of the positive supplies net of the lower bounds where it starts from no basis, so a
    /// change right after a load of supplies near that limit can be refused. A solve that finds a flow leaves none.
    ExcessBeyondRange {
        /// That sum, in units of flow.
        total: u128,
    },
}

#[pymethods]
impl ChangeBreach {
    /// The breach's class and its values, from which `copy` and `pickle` make it again.
    ///
    /// # Errors
    /// Passes on the `MemoryError` of a tuple that cannot be made.
    fn __reduce__<'py>(slf: &Bound<'py, Self>) -> PyResult<(Bound<'py, PyType>, Bound<'py, PyTuple>)> {
        let py = slf.py();
        let values = match *slf.get() {
            Self::LowerAboveCapacity { arc, lower_bound, capacity } => {
                (arc, lower_bound, capacity).into_pyobject(py)?
            }
            Self::CostBeyondBound { arc, cost, node_count } => (arc, cost, node_count).into_pyobject(py)?,
            Self::CostTotalBeyondRange { arc } => (arc,).into_pyobject(py)?,
            Self::SupplyBeyondRange { node, supply } => (node, supply).into_pyobject(py)?,
            Self::ExcessBeyondRange { total } => (total,).into_pyobject(py)?,
        };

        Ok((slf.as_any().get_type(), values))
    }

    /// The breach as Python would make it: `ChangeBreach.SupplyBeyondRange(node=2, supply=-9223372036854775809)`.
    fn __repr__(&self) -> String {
        match *self {
            Self::LowerAboveCapacity { arc, lower_bound, capacity } => {
                format!("ChangeBreach.LowerAboveCapacity(arc={arc}, lower_bound={lower_bound}, capacity={capacity})")
            }
            Self::CostBeyondBound { arc, cost, node_count } => {
                format!("ChangeBreach.CostBeyondBound(arc={arc}, cost={cost}, node_count={node_count})")
            }
            Self::CostTotalBeyondRange { arc } => format!("ChangeBreach.CostTotalBeyondRange(arc={arc})"),
            Self::SupplyBeyondRange { node, supply } => {
                format!("ChangeBreach.SupplyBeyondRange(node={node}, supply={supply})")
            }
            Self::ExcessBeyondRange { total } => format!("ChangeBreach.ExcessBeyondRange(total={total})"),
        }
    }
}

impl From<kahlnws::ChangeError> for ChangeBreach {
    fn from(breach: kahlnws::ChangeError) -> Self {
        use kahlnws::ChangeError as Error;

        match breach {
            Error::LowerAboveCapacity { arc, lower_bound, capacity } => {
                Self::LowerAboveCapacity { arc, lower_bound, capacity }
            }
            Error::CostBeyondBound { arc, cost, node_count } => Self::CostBeyondBound { arc, cost, node_count },
            Error::CostTotalBeyondRange { arc } => Self::CostTotalBeyondRange { arc },
            Error::SupplyBeyondRange { node, supply } => Self::SupplyBeyondRange { node, supply },
            Error::ExcessBeyondRange { total } => Self::ExcessBeyondRange { total },
        }
    }
}

/// The exception that tells Python a network refused a change for `breach`: a [`ChangeError`] whose message is the
/// breach's text, and whose attribute `breach` holds the breach as a [`ChangeBreach`].
pub(crate) fn change_refusal(py: Python<'_>, breach: kahlnws::ChangeError) -> PyErr {
    with_breach(py, ChangeError::new_err(breach.to_string()), ChangeBreach::from(breach))
}

#[cfg(test)]
mod tests {
    use kahlnws::{ChangeError as Error, cost_bound};

    use super::*;

    /// Every breach of a change in the library with values that differ from field to field, the breach it becomes,
    /// and that breach as Python writes it.
    fn breaches() -> [(Error, ChangeBreach, String); 5] {
        let bound = cost_bound(3);

        [
            (
                Error::LowerAboveCapacity { arc: 1, lower_bound: 6, capacity: 5 },
                ChangeBreach::LowerAboveCapacity { arc: 1, lower_bound: 6, capacity: 5 },
                "ChangeBreach.LowerAboveCapacity(arc=1, lower_bound=6, capacity=5)".to_owned(),
            ),
            (
                Error::CostBeyondBound { arc: 2, cost: -bound - 1, node_count: 3 },
                ChangeBreach::CostBeyondBound { arc: 2, cost: -bound - 1, node_count: 3 },
                format!("ChangeBreach.CostBeyondBound(arc=2, cost={}, node_count=3)", -bound - 1),
            ),
            (
                Error::CostTotalBeyondRange { arc: 4 },
                ChangeBreach::CostTotalBeyondRange { arc: 4 },
                "ChangeBreach.CostTotalBeyondRange(arc=4)".to_owned(),
            ),
            (
                Error::SupplyBeyondRange { node: 2, supply: i128::from(i64::MIN) - 1 },
                ChangeBreach::SupplyBeyondRange { node: 2, supply: i128::from(i64::MIN) - 1 },
                "ChangeBreach.SupplyBeyondRange(node=2, supply=-9223372036854775809)".to_owned(),
            ),
            (
                Error::ExcessBeyondRange { total: u128::MAX },
                ChangeBreach::ExcessBeyondRange { total: u128::MAX },
                format!("ChangeBreach.ExcessBeyondRange(total={})", u128::MAX),
            ),
        ]
    }

    #[test]
    fn test_each_breach_of_a_change_by_hand() {
        for (error, breach, _) in breaches() {
            assert_eq!(ChangeBreach::from(error), breach, "{error:?}");
        }
    }

    #[test]
    fn test_each_breach_of_a_change_as_python_writes_it_by_hand() {
        for (_, breach, written) in breaches() {
            assert_eq!(breach.__repr__(), written);
        }
    }

    #[test]
    fn test_change_refusal_raises_change_error_with_the_library_s_message_and_the_breach_by_hand() {
        Python::initialize();
        Python::attach(|py| -> PyResult<()> {
            let error = change_refusal(py, Error::SupplyBeyondRange { node: 2, supply: i128::from(i64::MIN) - 1 });

            assert!(error.is_instance_of::<ChangeError>(py));
            assert!(error.is_instance_of::<PyValueError>(py));
            assert!(!error.is_instance_of::<crate::breach::ProblemError>(py), "a change is refused apart");
            assert_eq!(
                error.value(py).to_string(),
                "the supply of node 2 would be -9223372036854775809, outside the range of a signed 64-bit integer"
            );
            let breach = error.value(py).getattr("breach")?;
            assert_eq!(
                *breach.cast::<ChangeBreach>()?.get(),
                ChangeBreach::SupplyBeyondRange { node: 2, supply: -9_223_372_036_854_775_809 }
            );

            Ok(())
        })
        .expect("a refusal of a change should carry its breach");
    }
}
