# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""In an environment without NumPy, which `check.sh` makes besides the one with it: the package is installed, imports
and solves by every entry. Skipped where NumPy is installed."""

import importlib.util
from array import array

import pytest

import kahlnws
from networks import DEPOT, DEPOT_COST, DEPOT_FLOWS, ENTRY_PAIRS, Solve, SolveInto, named

pytestmark = pytest.mark.skipif(importlib.util.find_spec("numpy") is not None, reason="NumPy is installed here")


@pytest.mark.parametrize(("entry", "entry_into"), ENTRY_PAIRS, ids=named)
def test_numpy_is_absent_and_both_levels_solve_from_plain_values(entry: Solve, entry_into: SolveInto) -> None:
    assert importlib.util.find_spec("numpy") is None

    solver = kahlnws.NetworkSimplex()
    solution = DEPOT.solve(solver, entry)
    flows = array("Q", [0, 0, 0])
    total_cost = DEPOT.answer_into(solver, flows, entry=entry_into).outcome

    assert solution is not None
    assert (solution.flows, solution.total_cost) == (DEPOT_FLOWS, DEPOT_COST)
    assert (flows.tolist(), total_cost) == (DEPOT_FLOWS, DEPOT_COST)
