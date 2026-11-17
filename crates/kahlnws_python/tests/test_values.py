# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The package's values as plain data: made from their parts, compared and hashed by value, copied and pickled; and
what cannot be made, copied or pickled, refused."""

import copy
import pickle
from collections.abc import Callable

import pytest

import kahlnws
from kahlnws import Answer, Bounds, Breach, ChangeBreach, Solution, Table, Work
from networks import DEPOT

# One value of every class that is plain data, every breach among them, each with values that differ from field to
# field.
VALUES: list[object] = [
    Work(4, 1, 0),
    Solution([2, 2, 2], [0, 1, 5], 14),
    Answer(Solution([2, 2, 2], [0, 1, 5], 14), Work(4, 1, 0)),
    Answer(-(2**100), Work(1, 0, 1)),
    Answer(None, Work(1, 0, 0)),
    Bounds(lower_bound=2, capacity=2**64 - 1),
    Table.HEADS,
    Table.LOWER_BOUNDS,
    Table.CAPACITIES,
    Table.COSTS,
    Breach.Length(Table.COSTS, 2, 3),
    Breach.TooLarge(4_294_967_295, 7),
    Breach.TailOutsideNodes(2, 9, 4),
    Breach.HeadOutsideNodes(3, 8, 5),
    Breach.LowerAboveCapacity(1, 6, 5),
    Breach.CostBeyondBound(1, -(2**62), 4),
    Breach.CostTotalBeyondRange(2),
    Breach.Unbalanced(-(2**127)),
    Breach.NetSupplyBeyondRange(2**128 - 1),
    ChangeBreach.LowerAboveCapacity(1, 6, 5),
    ChangeBreach.CostBeyondBound(2, 2**62, 3),
    ChangeBreach.CostTotalBeyondRange(4),
    ChangeBreach.SupplyBeyondRange(2, -(2**63) - 1),
    ChangeBreach.ExcessBeyondRange(2**64 - 1),
]


@pytest.mark.parametrize("value", VALUES, ids=repr)
def test_every_value_copies_and_pickles_to_an_equal_one_of_the_same_class(value: object) -> None:
    for again in [copy.copy(value), copy.deepcopy(value), pickle.loads(pickle.dumps(value))]:
        assert type(again) is type(value)
        assert again == value
        assert hash(again) == hash(value)
        assert repr(again) == repr(value)


def test_a_solution_and_an_answer_compare_and_hash_by_value() -> None:
    answer = DEPOT.answer(kahlnws.NetworkSimplex())
    solution = answer.outcome
    assert solution is not None

    assert solution == Solution(flows=[2, 2, 2], potentials=[0, 1, 5], total_cost=14)
    assert solution != Solution([2, 2, 2], [0, 1, 5], 15)
    assert solution != Solution([2, 2, 1], [0, 1, 5], 14)
    assert len({solution, Solution((2, 2, 2), (0, 1, 5), 14)}) == 1, "made from any iterables, kept as lists"
    assert answer == Answer(Solution([2, 2, 2], [0, 1, 5], 14), Work(4, 1, 0))
    assert answer != Answer(Solution([2, 2, 2], [0, 1, 5], 14), Work(4, 0, 0))
    assert answer != Answer(None, Work(4, 1, 0))
    assert len({answer, DEPOT.answer(kahlnws.NetworkSimplex())}) == 1


def test_a_solution_made_by_hand_keeps_its_parts_as_lists() -> None:
    solution = Solution(flows=(1, 2), potentials=range(3), total_cost=-(2**126))

    assert (solution.flows, solution.potentials, solution.total_cost) == ([1, 2], [0, 1, 2], -(2**126))
    with pytest.raises(TypeError):
        Solution(flows=1, potentials=[], total_cost=0)  # type: ignore[arg-type]  # an int cannot be iterated
    with pytest.raises(OverflowError):
        Solution(flows=[], potentials=[], total_cost=2**127)


def test_what_cannot_be_made_by_calling_its_class_says_so() -> None:
    with pytest.raises(TypeError):
        Table()
    with pytest.raises(TypeError):
        Breach()
    with pytest.raises(TypeError):
        ChangeBreach()
    with pytest.raises(TypeError):
        kahlnws.NetworkOptimum()


def test_a_problem_error_made_by_hand_has_no_breach_and_one_raised_by_a_solve_pickles_with_its_breach() -> None:
    assert kahlnws.ProblemError("made by hand").breach is None

    with pytest.raises(kahlnws.ProblemError) as raised:
        kahlnws.NetworkSimplex().solve(tails=[0], heads=[5], capacities=[1], costs=[1], supplies=[1, -1])
    again = pickle.loads(pickle.dumps(raised.value))

    assert type(again) is kahlnws.ProblemError
    assert str(again) == "arc 0 enters node 5, not below the node count 2"
    assert again.breach == Breach.HeadOutsideNodes(arc=0, head=5, node_count=2)


def test_a_solver_is_neither_copied_nor_pickled() -> None:
    solver = kahlnws.NetworkSimplex()

    with pytest.raises(TypeError):
        pickle.dumps(solver)
    with pytest.raises(TypeError):
        copy.deepcopy(solver)


def test_a_table_converts_to_its_position() -> None:
    assert [int(table) for table in [Table.HEADS, Table.LOWER_BOUNDS, Table.CAPACITIES, Table.COSTS]] == [0, 1, 2, 3]


def test_a_change_error_made_by_hand_has_no_breach_and_one_raised_by_a_change_pickles_with_its_breach() -> None:
    assert kahlnws.ChangeError("made by hand").breach is None

    with pytest.raises(kahlnws.ChangeError) as raised:
        DEPOT.loaded().set_lower_bound(0, 4)
    again = pickle.loads(pickle.dumps(raised.value))

    assert type(again) is kahlnws.ChangeError
    assert str(again) == "arc 0 has lower bound 4 above its capacity 3"
    assert again.breach == ChangeBreach.LowerAboveCapacity(arc=0, lower_bound=4, capacity=3)


def test_a_network_is_copied_but_not_pickled_and_its_optimum_neither() -> None:
    network = DEPOT.loaded()
    network.solve()
    optimum = network.optimum()

    with pytest.raises(TypeError):
        pickle.dumps(network)
    assert type(copy.deepcopy(network)) is kahlnws.Network
    refusals: list[Callable[[object], object]] = [pickle.dumps, copy.copy, copy.deepcopy]
    for refused in refusals:
        with pytest.raises(TypeError):
            refused(optimum)
