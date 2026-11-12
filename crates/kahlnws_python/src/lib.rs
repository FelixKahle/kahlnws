// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The Python binding of kahlnws: the extension module `kahlnws._kahlnws`, which the Python package `kahlnws`
//! re-exports, and whose names the stubs `python/kahlnws/_kahlnws.pyi` declare.
//!
//! Every Python object a call is handed is read into memory the binding owns before the solve starts: a sequence
//! entry by entry while the interpreter is attached, a buffer by the interpreter's own copy while its export is held.
//! The solve then runs with the interpreter released on the binding's copy alone, and an answer is written back to a
//! caller's buffer by the interpreter's copy again. No reference to memory Python owns is ever formed, so no other
//! thread, whatever it writes meanwhile, can make the solve read a changing value.
//!
//! Docstrings. The Python docstrings of the classes, methods and functions are written once, here, as their doc
//! comments, in the form a Python reader of `help()` expects (a summary line, `Args:`, `Returns:`, `Raises:`, an
//! example that runs as a doctest); the binding library makes them `__doc__`. The stubs repeat them for editors, and a
//! test of the package fails where a stub's docstring differs from `__doc__` by a word.

// No `unsafe` is written in this crate: Python's memory is read and written only through the interpreter's own
// copies. The binding library's macros expand to `unsafe` code of their own, which this lint does not reach.
#![forbid(unsafe_code)]

mod answer;
mod argument;
mod breach;
mod column;
mod items;
mod output;
mod solution;
mod solver;
mod tables;

use pyo3::prelude::*;

// The unit tests' binary counts the allocations of each thread, for the test of what a reserved solve allocates.
#[cfg(test)]
#[global_allocator]
static ALLOCATOR: kahlnws_testing::CountingAllocator = kahlnws_testing::CountingAllocator;

/// Answer the largest magnitude a cost may have in a problem of `node_count` nodes.
///
/// Every cost of a problem a solve admits lies within plus or minus it. It is `(2**62 - 1) // node_count`, and
/// `2**62 - 1` for a problem of no node.
///
/// Args:
///     node_count: The number of nodes of the problem.
///
/// Returns:
///     The bound, an `int`.
///
/// Raises:
///     OverflowError: `node_count` is below zero or past the platform's `size_t`.
///
/// Example:
///     >>> kahlnws.cost_bound(1000)
///     4611686018427387
#[pyfunction]
#[pyo3(signature = (node_count))]
fn cost_bound(node_count: usize) -> i64 {
    kahlnws::cost_bound(node_count)
}

/// The extension module of the package `kahlnws`, which re-exports every name it holds.
// The module needs the GIL: it has not been tested on a free-threaded interpreter, which then runs it with the GIL.
#[pymodule(gil_used = true)]
mod _kahlnws {
    use pyo3::prelude::*;

    #[pymodule_export]
    use crate::answer::{Answer, Work};
    #[pymodule_export]
    use crate::breach::{Breach, ProblemError, Table};
    #[pymodule_export]
    use crate::cost_bound;
    #[pymodule_export]
    use crate::solution::Solution;
    #[pymodule_export]
    use crate::solver::NetworkSimplex;

    /// The version of the package, which is the workspace's.
    #[pymodule_export]
    #[expect(non_upper_case_globals, reason = "the name Python gives the version of a package")]
    const __version__: &str = env!("CARGO_PKG_VERSION");

    /// Gives `ProblemError` the attribute `breach` at `None`, so that one made by hand answers it too.
    ///
    /// # Errors
    /// Passes on what Python raises while the attribute is set.
    #[pymodule_init]
    fn init(py: Python<'_>) -> PyResult<()> {
        py.get_type::<ProblemError>().setattr("breach", py.None())
    }
}
