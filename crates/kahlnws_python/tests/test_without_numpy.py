# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""In an environment without NumPy, which `check.sh` makes besides the one with it: the package is installed, imports
and solves. Skipped where NumPy is installed."""

import importlib.util
from array import array

import pytest

import kahlnws
from networks import DEPOT, DEPOT_COST, DEPOT_FLOWS

pytestmark = pytest.mark.skipif(importlib.util.find_spec("numpy") is not None, reason="NumPy is installed here")


def test_numpy_is_absent_and_both_levels_solve_from_plain_values() -> None:
    assert importlib.util.find_spec("numpy") is None

    solver = kahlnws.NetworkSimplex()
    solution = DEPOT.solve(solver)
    flows = array("Q", [0, 0, 0])
    total_cost = solver.solve_into(
        tails=DEPOT.tails,
        heads=DEPOT.heads,
        capacities=DEPOT.capacities,
        costs=DEPOT.costs,
        supplies=DEPOT.supplies,
        flows=flows,
    ).outcome

    assert solution is not None
    assert (solution.flows, solution.total_cost) == (DEPOT_FLOWS, DEPOT_COST)
    assert (flows.tolist(), total_cost) == (DEPOT_FLOWS, DEPOT_COST)
