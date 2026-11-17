# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The answer of both levels of the solver and of a network's solve: what a solve found beside the work it did, optimal
and infeasible alike, and the two values `Answer` and `Work` as Python handles them."""

from array import array

import pytest

import kahlnws
from kahlnws import Answer, Work
from networks import CUT, DEPOT, Network

# Node 0 must send 2 units to node 1 over an arc of capacity 1; an arc back of capacity 5 does not help. One pivot
# fills the forward arc, and then no arc violates its bound while a root link still carries flow.
CUT_WITH_A_WAY_BACK = Network(tails=[0, 1], heads=[1, 0], capacities=[1, 5], costs=[1, 1], supplies=[2, -2])


def both_levels(network: Network) -> tuple[Answer[kahlnws.Solution], Answer[int]]:
    """What a fresh solver answers for `network` by `solve` and by `solve_into`."""
    high = network.answer(kahlnws.NetworkSimplex())
    low = network.answer_into(kahlnws.NetworkSimplex(), array("Q", [0] * len(network.tails)))
    return high, low


def test_the_depot_answers_its_solution_and_its_work_by_hand() -> None:
    high, low = both_levels(DEPOT)

    assert high.outcome is not None
    assert high.outcome.total_cost == low.outcome == 14
    assert high.work == low.work == Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0)


@pytest.mark.parametrize("network", [CUT_WITH_A_WAY_BACK, CUT])
def test_an_infeasible_problem_answers_none_and_its_work_by_hand(network: Network) -> None:
    high, low = both_levels(network)

    assert high.outcome is None
    assert low.outcome is None
    assert high.work == low.work == Work(1, 0, 0)


def test_a_problem_of_no_arc_does_no_work() -> None:
    for supplies in [[], [0, 0]]:
        high, low = both_levels(Network(tails=[], heads=[], capacities=[], costs=[], supplies=supplies))

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


def test_a_network_answers_the_depot_s_optimum_and_its_work_by_hand_and_no_pivot_again() -> None:
    # A network's first solve starts from the star, as a solver's does, and makes the same four pivots on the depot,
    # one of them degenerate; it tries no arcs before block search. Nothing changed, its next solve makes none.
    network = DEPOT.loaded()

    answer = network.solve()
    assert (answer.outcome, answer.work) == (14, Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0))
    again = network.solve()

    assert again.work == Work(0, 0, 0)
    match again:
        case Answer(int() as total_cost, Work(pivots=pivots)):
            assert (total_cost, pivots) == (14, 0)
        case other:
            pytest.fail(f"the depot should answer a cost, not {other!r}")


def test_a_network_of_no_arc_and_one_never_loaded_do_no_work() -> None:
    for network in [kahlnws.Network(), Network(tails=[], heads=[], capacities=[], costs=[], supplies=[0, 0]).loaded()]:
        answer = network.solve()

        assert answer == Answer(0, Work(0, 0, 0))
