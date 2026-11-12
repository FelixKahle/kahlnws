# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The high level, `NetworkSimplex.solve`, handed plain Python values: what it answers and how one solver is reused."""

from array import array
from collections import UserList

import pytest

import kahlnws
from networks import CUT, DEPOT, DEPOT_COST, DEPOT_FLOWS, DEPOT_POTENTIALS, Network


def test_the_depot_by_hand() -> None:
    solution = DEPOT.solve(kahlnws.NetworkSimplex())

    assert solution is not None
    assert solution.flows == DEPOT_FLOWS
    assert solution.potentials == DEPOT_POTENTIALS
    assert solution.total_cost == DEPOT_COST
    assert type(solution.total_cost) is int
    assert all(type(flow) is int for flow in solution.flows)
    DEPOT.check(solution.flows, solution.potentials, solution.total_cost)


def test_the_depot_from_tuples_ranges_and_other_sequences() -> None:
    # A `UserList` is a sequence that is neither a list nor a tuple nor a buffer: read by its length and indexing.
    solver = kahlnws.NetworkSimplex()
    by_tuple = solver.solve(
        tails=tuple(DEPOT.tails),
        heads=tuple(DEPOT.heads),
        capacities=tuple(DEPOT.capacities),
        costs=tuple(DEPOT.costs),
        supplies=tuple(DEPOT.supplies),
    ).outcome
    by_protocol = solver.solve(
        tails=UserList(DEPOT.tails),
        heads=UserList(DEPOT.heads),
        capacities=range(3, 0, -1),
        costs=UserList(DEPOT.costs),
        supplies=UserList(DEPOT.supplies),
        lower_bounds=UserList([0, 0, 0]),
    ).outcome

    assert by_tuple is not None
    assert (by_tuple.flows, by_tuple.potentials, by_tuple.total_cost) == (DEPOT_FLOWS, DEPOT_POTENTIALS, 14)
    assert by_protocol is not None
    assert by_protocol.flows == [3, 1, 1], "capacities 3, 2 and 1 leave the direct arc to carry 3"


def test_the_depot_from_arrays_of_the_standard_library() -> None:
    solution = (
        kahlnws.NetworkSimplex()
        .solve(
            tails=array("Q", DEPOT.tails),
            heads=array("L", DEPOT.heads),
            capacities=array("Q", DEPOT.capacities),
            costs=array("q", DEPOT.costs),
            supplies=array("q", DEPOT.supplies),
            lower_bounds=array("Q", [0, 0, 0]),
        )
        .outcome
    )

    assert solution is not None
    assert (solution.flows, solution.total_cost) == (DEPOT_FLOWS, DEPOT_COST)


def test_the_solution_reads_the_same_lists_each_time_and_writes_itself_as_python_would() -> None:
    solution = DEPOT.solve(kahlnws.NetworkSimplex())

    assert solution is not None
    assert solution.flows is solution.flows
    assert solution.potentials is solution.potentials
    assert repr(solution) == "Solution(flows=[2, 2, 2], potentials=[0, 1, 5], total_cost=14)"


def test_an_infeasible_problem_answers_none() -> None:
    assert CUT.solve(kahlnws.NetworkSimplex()) is None


def test_lower_bounds_that_turn_a_supplier_into_a_demander() -> None:
    # Node 0 supplies 1 but must send at least 3 to node 1, which then sends 2 back over the arc of cost 1.
    network = Network(
        tails=[0, 1], heads=[1, 0], capacities=[5, 5], costs=[2, 1], supplies=[1, -1], lower_bounds=[3, 0]
    )
    solution = network.solve(kahlnws.NetworkSimplex())

    assert solution is not None
    assert (solution.flows, solution.total_cost) == ([3, 2], 8)
    network.check(solution.flows, solution.potentials, solution.total_cost)


def test_a_problem_of_no_node_and_one_of_nodes_without_arcs() -> None:
    solver = kahlnws.NetworkSimplex()

    nothing = solver.solve(tails=[], heads=[], capacities=[], costs=[], supplies=[]).outcome
    assert nothing is not None
    assert (nothing.flows, nothing.potentials, nothing.total_cost) == ([], [], 0)

    balanced = solver.solve(tails=[], heads=[], capacities=[], costs=[], supplies=[0, 0]).outcome
    assert balanced is not None
    assert (balanced.flows, len(balanced.potentials), balanced.total_cost) == ([], 2, 0)


def test_capacities_at_the_most_a_column_holds_and_supplies_at_both_ends() -> None:
    # Node 0 takes `2**63` from nodes 1 and 2, which supply `2**63 - 1` and 1, over arcs as wide as a column holds.
    widest = 2**64 - 1
    network = Network(
        tails=[1, 2, 2],
        heads=[0, 0, 1],
        capacities=[widest, widest, widest],
        costs=[3, -2, 1],
        supplies=[-(2**63), 2**63 - 1, 1],
    )
    solution = network.solve(kahlnws.NetworkSimplex())

    assert solution is not None
    assert solution.flows == [2**63 - 1, 1, 0]
    assert solution.total_cost == 3 * (2**63 - 1) - 2
    network.check(solution.flows, solution.potentials, solution.total_cost)


def test_costs_at_both_ends_of_the_cost_bound() -> None:
    # Five nodes in a row, node 0 sending 2 units to node 4 along the row or across, at the bound or its negation.
    bound = kahlnws.cost_bound(5)
    network = Network(
        tails=[0, 1, 2, 3, 0, 2, 4],
        heads=[1, 2, 3, 4, 4, 4, 0],
        capacities=[2, 2, 1, 2, 1, 2, 1],
        costs=[bound, bound, -bound, bound, bound, bound, -bound],
        supplies=[2, 0, 0, 0, -2],
    )
    solution = network.solve(kahlnws.NetworkSimplex())

    assert bound == (2**62 - 1) // 5
    assert solution is not None
    network.check(solution.flows, solution.potentials, solution.total_cost)
    assert max(abs(potential) for potential in solution.potentials) > bound


def test_the_cost_as_an_exact_integer_past_64_bits_at_both_signs() -> None:
    # On one node, loops of negative cost fill: their magnitudes times their capacities add up to exactly
    # `2**127 - 1`, which the flow costs negated; the same loops of positive cost stay empty.
    bound, widest, rest = kahlnws.cost_bound(1), 2**64 - 1, 2**63 - 1
    capacities = [widest, widest, widest, widest, rest]
    negative = [-bound, -bound, -1, -1, -1]
    loops = Network(tails=[0] * 5, heads=[0] * 5, capacities=capacities, costs=negative, supplies=[0])
    assert sum(cost * capacity for cost, capacity in zip(negative, capacities, strict=True)) == -(2**127 - 1)

    filled = loops.solve(kahlnws.NetworkSimplex())
    assert filled is not None
    assert filled.flows == capacities
    assert filled.total_cost == -(2**127 - 1)

    # Positive cost past 64 bits: a lower bound forces the flow through loops of positive cost.
    forced = Network(
        tails=[0, 0],
        heads=[0, 0],
        capacities=[widest, rest],
        costs=[bound, bound],
        supplies=[0],
        lower_bounds=[widest, rest],
    )
    paid = forced.solve(kahlnws.NetworkSimplex())
    assert paid is not None
    assert paid.total_cost == bound * (widest + rest)
    assert paid.total_cost > 2**64


def test_net_supplies_at_their_limit() -> None:
    # Node 0 supplies `2**63 - 1` and an arc into it of lower bound `2**63 - 1` adds as much: the positive net supplies
    # add up to `2**64 - 2`, which node 0 sends back over an arc as wide as a column holds.
    fixed = 2**63 - 1
    network = Network(
        tails=[1, 0],
        heads=[0, 1],
        capacities=[fixed, 2**64 - 1],
        costs=[0, 1],
        supplies=[fixed, -fixed],
        lower_bounds=[fixed, 0],
    )
    solution = network.solve(kahlnws.NetworkSimplex())

    assert solution is not None
    assert solution.flows == [fixed, 2**64 - 2]
    assert solution.total_cost == 2**64 - 2


def test_one_solver_answers_every_problem_as_a_fresh_one() -> None:
    larger = Network(
        tails=[0, 0, 1, 2, 3, 1],
        heads=[1, 2, 3, 3, 4, 4],
        capacities=[4, 4, 3, 3, 5, 1],
        costs=[2, 1, 1, 3, 1, 7],
        supplies=[6, 0, 0, 0, -6],
    )
    solver = kahlnws.NetworkSimplex()

    for network in [larger, DEPOT, CUT, DEPOT, larger, CUT, larger]:
        fresh = network.solve(kahlnws.NetworkSimplex())
        again = network.solve(solver)
        if fresh is None:
            assert again is None
        else:
            assert again is not None
            assert (again.flows, again.potentials, again.total_cost) == (
                fresh.flows,
                fresh.potentials,
                fresh.total_cost,
            )


def test_a_refused_and_an_infeasible_problem_leave_the_solver_answering_as_a_fresh_one() -> None:
    solver = kahlnws.NetworkSimplex()
    with pytest.raises(kahlnws.ProblemError):
        Network(tails=[0], heads=[9], capacities=[1], costs=[1], supplies=[0, 0]).solve(solver)
    assert CUT.solve(solver) is None

    solution = DEPOT.solve(solver)
    assert solution is not None
    assert (solution.flows, solution.potentials, solution.total_cost) == (DEPOT_FLOWS, DEPOT_POTENTIALS, DEPOT_COST)


def test_a_reserved_solver_answers_as_a_fresh_one_and_reserve_refuses_negative_counts() -> None:
    solver = kahlnws.NetworkSimplex()
    solver.reserve(3, 3)
    solver.reserve(node_count=1, arc_count=0)
    with pytest.raises(OverflowError):
        solver.reserve(-1, 3)

    reserved, fresh = DEPOT.answer(solver), DEPOT.answer(kahlnws.NetworkSimplex())
    assert reserved == fresh
    assert reserved.outcome is not None
    assert reserved.outcome.flows == DEPOT_FLOWS
