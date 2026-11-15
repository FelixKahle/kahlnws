# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""`NetworkSimplex.resolve` and `NetworkSimplex.resolve_into`: a solve from the basis the solver kept where it serves,
told apart from a fresh solve by its work, and exactly a fresh solve where the basis cannot serve."""

from array import array

import pytest

import kahlnws
from kahlnws import Work
from networks import CUT, DEPOT, DEPOT_COST, DEPOT_FLOWS, DEPOT_POTENTIALS, Network

# The depot's work from a fresh solver, as `test_work` works it out: four pivots, one of them degenerate.
DEPOT_WORK = Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0)

# No pivot at all: what a resolve makes where the basis it starts from is already optimal.
NO_WORK = Work(0, 0, 0)

# The depot with one unit less to send. By hand: the hub's path carries its 2 units at 2 a unit and the direct arc the
# third at 5, a cost of 9. The depot's basis holds arcs 0 and 1 strictly within their bounds and arc 2 full; with one
# unit less, arc 0 carries 1 and the others stay, so the kept tree carries the optimum, its potentials 0, 1 and 5 as
# before, and no arc violates its bound.
FEWER = Network(DEPOT.tails, DEPOT.heads, DEPOT.capacities, DEPOT.costs, supplies=[3, 0, -3])

# The depot whose route through the hub costs 6 a unit, more than the direct arc: the direct arc fills with 3 units at 5
# and the hub's path carries the fourth at 6, a cost of 21.
DEARER = Network(DEPOT.tails, DEPOT.heads, DEPOT.capacities, costs=[5, 5, 1], supplies=DEPOT.supplies)

# Node 0 sends 2 units down a chain of four nodes: one node more than the depot, whose basis cannot serve it.
CHAIN = Network(tails=[0, 1, 2], heads=[1, 2, 3], capacities=[5, 5, 5], costs=[1, 1, 1], supplies=[2, 0, 0, -2])

# The depot's counts with other ends: arcs 1 and 2 swap theirs, so the depot's basis cannot serve it.
SWAPPED = Network(tails=[0, 1, 0], heads=[2, 2, 1], capacities=[3, 2, 4], costs=[5, 1, 1], supplies=DEPOT.supplies)

# Node 0 must send 2 units to node 1 over an arc of capacity 1; an arc back of capacity 5 does not help.
CUT_WITH_A_WAY_BACK = Network(tails=[0, 1], heads=[1, 0], capacities=[1, 5], costs=[1, 1], supplies=[2, -2])

# A problem every solve refuses: its supplies add up to 1.
UNBALANCED = Network(DEPOT.tails, DEPOT.heads, DEPOT.capacities, DEPOT.costs, supplies=[4, 0, -3])


def solved(network: Network) -> kahlnws.NetworkSimplex:
    """A solver that has solved `network`, and keeps the basis that solve ended on."""
    solver = kahlnws.NetworkSimplex()
    network.answer(solver)
    return solver


def fresh(network: Network) -> kahlnws.Answer[kahlnws.Solution]:
    """What a fresh solver answers for `network` by `solve`."""
    return network.answer(kahlnws.NetworkSimplex())


def test_the_problem_solved_last_again_makes_no_pivot_and_answers_its_optimum() -> None:
    solver = kahlnws.NetworkSimplex()
    assert DEPOT.answer(solver).work == DEPOT_WORK

    again = DEPOT.answer(solver, kahlnws.NetworkSimplex.resolve)

    assert again.outcome == kahlnws.Solution(DEPOT_FLOWS, DEPOT_POTENTIALS, DEPOT_COST)
    assert again.work == NO_WORK


def test_the_problem_solved_last_again_makes_no_pivot_through_the_low_level() -> None:
    solver = kahlnws.NetworkSimplex()
    flows, potentials = array("Q", [7, 7, 7]), array("q", [7, 7, 7])
    assert DEPOT.answer_into(solver, flows, potentials).work == DEPOT_WORK
    flows[:], potentials[:] = array("Q", [7, 7, 7]), array("q", [7, 7, 7])

    again = DEPOT.answer_into(solver, flows, potentials, kahlnws.NetworkSimplex.resolve_into)

    assert again == kahlnws.Answer(DEPOT_COST, NO_WORK)
    assert (flows.tolist(), potentials.tolist()) == (DEPOT_FLOWS, DEPOT_POTENTIALS)


def test_one_unit_less_is_sent_along_the_kept_tree_without_a_pivot_where_a_fresh_solve_makes_some() -> None:
    resolved = FEWER.answer(solved(DEPOT), kahlnws.NetworkSimplex.resolve)
    flows, potentials = array("Q", [0, 0, 0]), array("q", [0, 0, 0])
    resolved_into = FEWER.answer_into(solved(DEPOT), flows, potentials, kahlnws.NetworkSimplex.resolve_into)

    assert resolved.outcome == kahlnws.Solution([1, 2, 2], [0, 1, 5], 9)
    assert resolved_into.outcome == 9
    assert (flows.tolist(), potentials.tolist()) == ([1, 2, 2], [0, 1, 5])
    assert resolved.work == resolved_into.work == NO_WORK
    assert fresh(FEWER).work.pivots > 0


def test_dearer_costs_move_a_unit_over_in_fewer_pivots_than_a_fresh_solve() -> None:
    resolved = DEARER.answer(solved(DEPOT), kahlnws.NetworkSimplex.resolve)

    assert resolved.outcome is not None
    assert (resolved.outcome.flows, resolved.outcome.total_cost) == ([3, 1, 1], 21)
    DEARER.check(resolved.outcome.flows, resolved.outcome.potentials, resolved.outcome.total_cost)
    assert resolved.work.first_arc_pivots == 0, "a start from the kept basis tries no arc before block search"
    assert resolved.work.pivots < fresh(DEARER).work.pivots


@pytest.mark.parametrize(
    "network", [DEPOT, FEWER, CHAIN, CUT, CUT_WITH_A_WAY_BACK], ids=["depot", "fewer", "chain", "cut", "cut back"]
)
def test_a_resolve_of_a_solver_that_never_solved_answers_what_a_solve_answers(network: Network) -> None:
    assert network.answer(kahlnws.NetworkSimplex(), kahlnws.NetworkSimplex.resolve) == fresh(network)

    flows, potentials = array("Q", [7] * len(network.tails)), array("q", [7] * len(network.supplies))
    solved_flows, solved_potentials = array("Q", flows), array("q", potentials)
    resolved = network.answer_into(kahlnws.NetworkSimplex(), flows, potentials, kahlnws.NetworkSimplex.resolve_into)
    solved_into = network.answer_into(kahlnws.NetworkSimplex(), solved_flows, solved_potentials)

    assert resolved == solved_into
    assert (flows, potentials) == (solved_flows, solved_potentials)


@pytest.mark.parametrize("network", [CHAIN, SWAPPED, CUT], ids=["more nodes", "other ends", "fewer arcs"])
def test_a_resolve_of_other_counts_or_other_ends_answers_what_a_solve_answers(network: Network) -> None:
    resolved = network.answer(solved(DEPOT), kahlnws.NetworkSimplex.resolve)

    assert resolved == fresh(network)
    assert resolved.work.pivots > 0, "a fresh solve pivots here, so the work tells it from a start from a kept basis"


def test_a_refused_problem_is_refused_alike_by_both_entries_and_answers_no_work() -> None:
    for solver in [kahlnws.NetworkSimplex(), solved(DEPOT)]:
        with pytest.raises(kahlnws.ProblemError) as raised:
            UNBALANCED.answer(solver, kahlnws.NetworkSimplex.resolve)
        assert raised.value.breach == kahlnws.Breach.Unbalanced(total=1)

        flows = array("Q", [7, 7, 7])
        with pytest.raises(kahlnws.ProblemError, match=r"^the supplies add up to 1, not zero$"):
            UNBALANCED.answer_into(solver, flows, entry=kahlnws.NetworkSimplex.resolve_into)
        assert flows.tolist() == [7, 7, 7]


def test_a_call_that_raises_between_a_solve_and_a_resolve_keeps_the_basis() -> None:
    solver = solved(DEPOT)

    with pytest.raises(kahlnws.ProblemError):
        UNBALANCED.answer(solver, kahlnws.NetworkSimplex.resolve)
    with pytest.raises(OverflowError):
        Network(DEPOT.tails, DEPOT.heads, [3, -4, 2], DEPOT.costs, FEWER.supplies).answer(
            solver, kahlnws.NetworkSimplex.resolve
        )
    with pytest.raises(ValueError, match=r"^flows has length 2, not the arc count 3$"):
        FEWER.answer_into(solver, array("Q", [0, 0]), entry=kahlnws.NetworkSimplex.resolve_into)
    with pytest.raises(MemoryError):
        solver.reserve(2**64 - 1, 0)

    assert DEPOT.answer(solver, kahlnws.NetworkSimplex.resolve).work == NO_WORK


def test_an_infeasible_solve_keeps_its_basis_and_the_same_problem_again_makes_no_pivot() -> None:
    solver = kahlnws.NetworkSimplex()
    assert CUT_WITH_A_WAY_BACK.answer(solver).work == Work(1, 0, 0)

    again = CUT_WITH_A_WAY_BACK.answer(solver, kahlnws.NetworkSimplex.resolve)

    assert again == kahlnws.Answer(None, NO_WORK)


def test_a_solve_after_a_resolve_answers_as_a_fresh_one() -> None:
    solver = solved(DEPOT)
    assert DEARER.answer(solver, kahlnws.NetworkSimplex.resolve).work.pivots < DEPOT_WORK.pivots

    for network in [FEWER, DEPOT, DEARER]:
        assert network.answer(solver) == fresh(network)


def test_one_solver_resolves_day_after_day_to_the_optimum_of_each() -> None:
    # Every answer a resolve gives is optimal, whatever the solves before it: each is checked against its network.
    solver = kahlnws.NetworkSimplex()
    days = [DEPOT, FEWER, DEARER, CHAIN, CHAIN, DEPOT, CUT, FEWER, SWAPPED, DEPOT]

    for network in days:
        resolved, solution = network.answer(solver, kahlnws.NetworkSimplex.resolve), fresh(network).outcome
        if solution is None:
            assert resolved.outcome is None
        else:
            assert resolved.outcome is not None
            assert resolved.outcome.total_cost == solution.total_cost
            network.check(resolved.outcome.flows, resolved.outcome.potentials, resolved.outcome.total_cost)
