# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The answer of both levels and both entries of each: what a solve found beside the work it did, optimal and
infeasible alike, and the two values `Answer` and `Work` as Python handles them."""

from array import array

import pytest

import kahlnws
from kahlnws import Answer, Work
from networks import CUT, DEPOT, ENTRY_PAIRS, Network, Solve, SolveInto, named

# The entries of the two levels in pairs, each pair called on fresh solvers: a resolve on a solver that never solved
# answers what a solve answers, its work included.
PAIRS = pytest.mark.parametrize(("entry", "entry_into"), ENTRY_PAIRS, ids=named)

# Node 0 must send 2 units to node 1 over an arc of capacity 1; an arc back of capacity 5 does not help. One pivot
# fills the forward arc, and then no arc violates its bound while a root link still carries flow.
CUT_WITH_A_WAY_BACK = Network(tails=[0, 1], heads=[1, 0], capacities=[1, 5], costs=[1, 1], supplies=[2, -2])


def both_levels(
    network: Network,
    entry: Solve = kahlnws.NetworkSimplex.solve,
    entry_into: SolveInto = kahlnws.NetworkSimplex.solve_into,
) -> tuple[Answer[kahlnws.Solution], Answer[int]]:
    """What a fresh solver answers for `network` at the high level by `entry` and at the low level by `entry_into`."""
    high = network.answer(kahlnws.NetworkSimplex(), entry)
    low = network.answer_into(kahlnws.NetworkSimplex(), array("Q", [0] * len(network.tails)), entry=entry_into)
    return high, low


@PAIRS
def test_the_depot_answers_its_solution_and_its_work_by_hand(entry: Solve, entry_into: SolveInto) -> None:
    high, low = both_levels(DEPOT, entry, entry_into)

    assert high.outcome is not None
    assert high.outcome.total_cost == low.outcome == 14
    assert high.work == low.work == Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0)


@PAIRS
@pytest.mark.parametrize("network", [CUT_WITH_A_WAY_BACK, CUT])
def test_an_infeasible_problem_answers_none_and_its_work_by_hand(
    network: Network, entry: Solve, entry_into: SolveInto
) -> None:
    high, low = both_levels(network, entry, entry_into)

    assert high.outcome is None
    assert low.outcome is None
    assert high.work == low.work == Work(1, 0, 0)


@PAIRS
def test_a_problem_of_no_arc_does_no_work(entry: Solve, entry_into: SolveInto) -> None:
    for supplies in [[], [0, 0]]:
        high, low = both_levels(
            Network(tails=[], heads=[], capacities=[], costs=[], supplies=supplies), entry, entry_into
        )

        assert high.outcome is not None
        assert low.outcome == 0
        assert high.work == low.work == Work(0, 0, 0)


def test_one_solver_answers_the_work_of_a_fresh_one() -> None:
    solver = kahlnws.NetworkSimplex()

    for network in [DEPOT, CUT_WITH_A_WAY_BACK, DEPOT]:
        assert network.answer(solver).work == network.answer(kahlnws.NetworkSimplex()).work


def test_a_refused_problem_raises_answers_no_work_and_leaves_the_work_of_the_next_solve_as_it_would_be() -> None:
    solver = kahlnws.NetworkSimplex()

    with pytest.raises(kahlnws.ProblemError) as raised:
        solver.solve(tails=[0], heads=[1], capacities=[1], costs=[1], supplies=[1, 0])
    assert raised.value.breach == kahlnws.Breach.Unbalanced(total=1)
    assert not hasattr(raised.value, "work")

    assert DEPOT.answer(solver).work == Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0)


def test_work_is_plain_data_compared_hashed_and_matched_by_value() -> None:
    work = Work(4, 1, 0)

    assert (work.pivots, work.degenerate_pivots, work.first_arc_pivots) == (4, 1, 0)
    assert work == Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0)
    assert work != Work(4, 0, 1)
    assert len({work, Work(4, 1, 0), Work(0, 0, 0)}) == 2
    assert repr(work) == "Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0)"
    match work:
        case Work(pivots, degenerate, first):
            assert (pivots, degenerate, first) == (4, 1, 0)

    with pytest.raises(OverflowError):
        Work(-1, 0, 0)
    with pytest.raises(AttributeError):
        work.pivots = 5  # type: ignore[misc]  # a property cannot be set, which the test shows


def test_an_answer_writes_itself_matches_and_names_its_outcome_s_type() -> None:
    answer = DEPOT.answer(kahlnws.NetworkSimplex())

    assert repr(answer) == (
        "Answer(outcome=Solution(flows=[2, 2, 2], potentials=[0, 1, 5], total_cost=14), "
        "work=Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0))"
    )
    assert answer.work is answer.work
    match answer:
        case Answer(kahlnws.Solution() as solution, Work(pivots=pivots)):
            assert (solution.total_cost, pivots) == (14, 4)
        case other:
            pytest.fail(f"the depot should answer a solution, not {other!r}")

    assert repr(Answer[int]) == "kahlnws.Answer[int]"
