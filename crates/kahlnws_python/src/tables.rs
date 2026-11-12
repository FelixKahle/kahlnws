// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The solver's own copy of a problem, read from the Python arguments that hold its columns.

use std::collections::TryReserveError;

use kahlnws::Problem;
use pyo3::prelude::*;

use crate::argument::Input;
use crate::column::Staging;

/// The arguments of a solve that hold the problem, as Python gave them. Plain data.
#[derive(Debug)]
pub(crate) struct Arguments<'py> {
    /// The node each arc leaves.
    pub(crate) tails: Bound<'py, PyAny>,
    /// The node each arc enters.
    pub(crate) heads: Bound<'py, PyAny>,
    /// The least flow of each arc; `None` where every arc's is zero.
    pub(crate) lower_bounds: Option<Bound<'py, PyAny>>,
    /// The most flow of each arc.
    pub(crate) capacities: Bound<'py, PyAny>,
    /// The cost of a unit of flow on each arc.
    pub(crate) costs: Bound<'py, PyAny>,
    /// The supply of each node.
    pub(crate) supplies: Bound<'py, PyAny>,
}

/// The columns of the problem a solver was last handed, each its own copy, and the room a buffer of other items than
/// its column's is staged in.
///
/// Each column is kept from one solve to the next and only grows; its contents mean nothing between solves.
#[derive(Debug, Default)]
pub(crate) struct Tables {
    /// The node each arc leaves, one per arc.
    tails: Vec<usize>,
    /// The node each arc enters, one per arc.
    heads: Vec<usize>,
    /// The least flow of each arc, in units of flow, one per arc, where the problem gives them.
    lower_bounds: Vec<u64>,
    /// The most flow of each arc, in units of flow, one per arc.
    capacities: Vec<u64>,
    /// The cost of a unit of flow on each arc, in units of cost, one per arc.
    costs: Vec<i64>,
    /// The supply of each node, in units of flow, one per node.
    supplies: Vec<i64>,
    /// The room a buffer is staged in before it is converted.
    staging: Staging,
}

impl Tables {
    /// Makes room for problems of up to `node_count` nodes and `arc_count` arcs: reading one allocates nothing
    /// afterwards. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    pub(crate) fn try_reserve(&mut self, node_count: usize, arc_count: usize) -> Result<(), TryReserveError> {
        reserve_column(&mut self.tails, arc_count)?;
        reserve_column(&mut self.heads, arc_count)?;
        reserve_column(&mut self.lower_bounds, arc_count)?;
        reserve_column(&mut self.capacities, arc_count)?;
        reserve_column(&mut self.costs, arc_count)?;
        reserve_column(&mut self.supplies, node_count)?;

        self.staging.try_reserve(arc_count.max(node_count))
    }

    /// Reads the problem `arguments` hold, each column into its own table, and answers it, borrowed from the tables.
    /// The columns are read in the order of the problem's fields, the lower bounds after the heads.
    ///
    /// The tables' lengths are not compared: a column of another length than the tails is the solve's to refuse.
    ///
    /// # Errors
    /// Returns the error of the first column that cannot be read, as [`Staging::read`] gives it.
    pub(crate) fn read(&mut self, arguments: &Arguments<'_>) -> PyResult<Problem<'_>> {
        let Self { tails, heads, lower_bounds, capacities, costs, supplies, staging } = self;

        staging.read(Input::Tails, &arguments.tails, tails)?;
        staging.read(Input::Heads, &arguments.heads, heads)?;
        let lower_bounds = match &arguments.lower_bounds {
            Some(argument) => {
                staging.read(Input::LowerBounds, argument, lower_bounds)?;
                Some(lower_bounds.as_slice())
            }
            None => None,
        };
        staging.read(Input::Capacities, &arguments.capacities, capacities)?;
        staging.read(Input::Costs, &arguments.costs, costs)?;
        staging.read(Input::Supplies, &arguments.supplies, supplies)?;

        Ok(Problem { tails, heads, lower_bounds, capacities, costs, supplies })
    }
}

/// Makes room in `column`, emptied, for `entries` entries.
///
/// # Errors
/// Returns the `TryReserveError` of a refused reservation.
fn reserve_column<T>(column: &mut Vec<T>, entries: usize) -> Result<(), TryReserveError> {
    column.clear();
    column.try_reserve(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_reserve_makes_room_for_each_column_by_its_count() {
        let mut tables = Tables::default();
        assert_eq!(tables.try_reserve(3, 7), Ok(()));

        let per_arc = [
            tables.tails.capacity(),
            tables.heads.capacity(),
            tables.lower_bounds.capacity(),
            tables.capacities.capacity(),
            tables.costs.capacity(),
        ];
        assert!(per_arc.iter().all(|&capacity| capacity >= 7), "{per_arc:?}");
        assert!(tables.supplies.capacity() >= 3);
    }

    #[test]
    fn test_reserve_column_keeps_the_room_already_held() {
        let mut column: Vec<u64> = Vec::with_capacity(10);
        column.extend([1, 2, 3]);
        assert_eq!(reserve_column(&mut column, 4), Ok(()));

        assert!(column.is_empty());
        assert!(column.capacity() >= 10);
    }
}
