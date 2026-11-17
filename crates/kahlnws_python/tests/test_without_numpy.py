# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""In an environment without NumPy, which `check.sh` makes besides the one with it: the package is installed, imports
and solves at both levels, and a network loads, solves and writes its optimum. Skipped where NumPy is installed."""

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
    total_cost = DEPOT.answer_into(solver, flows).outcome

    assert solution is not None
    assert (solution.flows, solution.total_cost) == (DEPOT_FLOWS, DEPOT_COST)
    assert (flows.tolist(), total_cost) == (DEPOT_FLOWS, DEPOT_COST)


def test_numpy_is_absent_and_a_network_loads_solves_and_writes_from_plain_values() -> None:
    assert importlib.util.find_spec("numpy") is None

    network = DEPOT.loaded()
    assert network.solve().outcome == DEPOT_COST
    optimum = network.optimum()
    assert optimum is not None
    flows, potentials = array("Q", [0, 0, 0]), array("q", [0, 0, 0])
    optimum.flows_into(flows)
    optimum.potentials_into(potentials)

    assert (optimum.flows, optimum.total_cost) == (DEPOT_FLOWS, DEPOT_COST)
    assert (flows.tolist(), potentials.tolist()) == (DEPOT_FLOWS, [0, 1, 5])

    tails, costs = array("Q", [0, 0, 0]), array("q", [0, 0, 0])
    network.tails_into(tails)
    network.costs_into(costs)
    assert (tails.tolist(), costs.tolist(), network.supplies()) == (DEPOT.tails, DEPOT.costs, DEPOT.supplies)
