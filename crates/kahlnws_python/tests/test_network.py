# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""`Network`: a problem held, changed one number or one pair of supplies at a time, and solved again. Each change and
the solve after it against a fresh solve of the changed problem, each refusal with nothing changed, an optimum that
reads nothing once its network changed, the copy, an index past its count, what a call that raises keeps, and the
reads of the problem held."""

import copy
import dataclasses
import re
from array import array
from collections.abc import Callable
from typing import Any, TypeAlias

import pytest

import kahlnws
from kahlnws import ChangeBreach, Work
from networks import CUT, DEPOT, DEPOT_COST, DEPOT_FLOWS, DEPOT_POTENTIALS, Network

# The depot's work in a network, from the star its load leaves: four pivots, one of them degenerate, and none of the
# arcs a solver tries before block search.
DEPOT_WORK = Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0)

# No pivot: what a solve makes where nothing changed, or where the changes left the flow optimal.
NO_WORK = Work(0, 0, 0)

# The bound of a cost on the depot's three nodes: `(2**62 - 1) // 3`.
BOUND = kahlnws.cost_bound(3)

# The depot with a dear second direct lane, arc 3, at 9 a unit for up to 5: switched off, arc 1 sends nothing into the
# hub, so 3 units go directly at 5 and the fourth by the dear lane at 9, a cost of 24.
DEPOT_WITH_A_DEAR_LANE = Network(
    tails=[0, 0, 1, 0], heads=[2, 1, 2, 2], capacities=[3, 4, 2, 5], costs=[5, 1, 1, 9], supplies=[4, 0, -4]
)

# One node and three loops of the widest capacity, at cost zero: a cost at the bound on all three passes the range of
# the costs times the capacities, two of them do not.
WIDEST_LOOPS = Network(tails=[0, 0, 0], heads=[0, 0, 0], capacities=[2**64 - 1] * 3, costs=[0, 0, 0], supplies=[0])

# Four nodes and no arc: a move of supply between two of them goes up and down their root links.
FOUR_NODES = Network(tails=[], heads=[], capacities=[], costs=[], supplies=[0, 0, 0, 0])

# A problem every load refuses: its supplies add up to 1.
UNBALANCED = dataclasses.replace(DEPOT, supplies=[4, 0, -3])

# A change of a network, and the change that sets it back, each called on the network.
Change: TypeAlias = Callable[[kahlnws.Network], object]


def solved(network: Network) -> kahlnws.Network:
    """A network that holds `network` and has solved it."""
    held = network.loaded()
    held.solve()
    return held


def answered(network: kahlnws.Network) -> tuple[int, list[int], list[int]] | None:
    """The cost, the flows and the potentials of what `network` solves to; `None` where no flow exists. The cost is
    the one the solve answers, and the one its optimum reads."""
    total_cost = network.solve().outcome
    optimum = network.optimum()
    if total_cost is None:
        assert optimum is None
        return None
    assert optimum is not None
    assert optimum.total_cost == total_cost
    return total_cost, optimum.flows, optimum.potentials


def assert_answers_as_a_fresh_solve(network: kahlnws.Network, problem: Network, cost: int | None) -> None:
    """Asserts that `network` solves to the hand-worked `cost`, or to no flow, as a fresh solve of `problem` does, and
    that the potentials it answers prove its flows optimal."""
    found, fresh = answered(network), problem.solve(kahlnws.NetworkSimplex())

    if cost is None:
        assert found is None, problem
        assert fresh is None, problem
    else:
        assert found is not None, problem
        assert fresh is not None, problem
        assert found[0] == fresh.total_cost == cost, problem
        problem.check(found[1], found[2], found[0])


def test_a_network_never_loaded_holds_the_problem_of_no_node() -> None:
    network = kahlnws.Network()
    assert (network.node_count, network.arc_count) == (0, 0)

    optimum = network.optimum()
    assert optimum is not None
    assert (optimum.total_cost, optimum.flows, optimum.potentials) == (0, [], [])
    assert network.solve() == kahlnws.Answer(0, NO_WORK)

    with pytest.raises(IndexError, match=r"^arc 0 is not below the arc count 0$"):
        network.set_cost(0, 1)
    with pytest.raises(IndexError, match=r"^node 0 is not below the node count 0$"):
        network.move_supply(0, 0, 1)
    with pytest.raises(IndexError, match=r"^node 0 is not below the node count 0$"):
        network.set_supply(0, 0, 0)


def test_the_depot_loaded_and_solved_and_solved_again_by_hand() -> None:
    network = DEPOT.loaded()
    assert (network.node_count, network.arc_count, network.optimum()) == (3, 3, None)

    answer = network.solve()
    assert answer == kahlnws.Answer(DEPOT_COST, DEPOT_WORK)
    assert type(answer.outcome) is int
    optimum = network.optimum()
    assert optimum is not None
    assert (optimum.total_cost, optimum.flows, optimum.potentials) == (DEPOT_COST, DEPOT_FLOWS, DEPOT_POTENTIALS)
    assert [optimum.flow(arc) for arc in range(3)] == DEPOT_FLOWS

    assert network.solve() == kahlnws.Answer(DEPOT_COST, NO_WORK)
    held = network.optimum()
    assert held is not None
    assert (held.flows, held.potentials) == (DEPOT_FLOWS, DEPOT_POTENTIALS)


def test_each_read_of_the_flows_or_the_potentials_builds_a_new_list_and_a_stale_one_raises() -> None:
    network = solved(DEPOT)
    optimum = network.optimum()
    assert optimum is not None

    flows, potentials = optimum.flows, optimum.potentials
    assert (flows, potentials) == (DEPOT_FLOWS, DEPOT_POTENTIALS)
    assert optimum.flows is not flows and optimum.potentials is not potentials, "each read is a new list"
    flows.append(9)
    potentials.clear()
    assert (optimum.flows, optimum.potentials) == (DEPOT_FLOWS, DEPOT_POTENTIALS), "a list read changes nothing held"

    network.solve()
    with pytest.raises(RuntimeError, match=r"^the network changed since this optimum was answered$"):
        _ = optimum.flows
    with pytest.raises(RuntimeError, match=r"^the network changed since this optimum was answered$"):
        _ = optimum.potentials


# Each change of the depot, the problem it makes, what the setter answers, and the optimal cost of that problem by
# hand, `None` where no flow exists:
# - the second leg through the hub at 3: 2 units at 4 a unit through it and 2 at 5 directly, 18;
# - the direct arc narrowed to 1: it and the hub's 2 carry 3 of the 4 units, no flow;
# - the direct arc's lower bound raised to 3: 3 units directly and 1 through the hub, 17;
# - the second leg's bounds set to 3 to 4 at once: every unit through the hub, 8;
# - one more unit for the customer, sent by the depot: 3 directly and 2 through the hub, 19;
# - two units of the depot's supply moved to the hub: the hub sends them on at 1, the depot its other 2 directly, 12;
# - the depot set to send 2, balanced by the customer: both through the hub, at 2 a unit, 4;
# - the customer set to take 5, balanced by the depot, or the depot set to send 5, balanced by the customer: 19.
CHANGES: list[tuple[str, Change, Network, object, int | None]] = [
    ("cost", lambda network: network.set_cost(2, 3), dataclasses.replace(DEPOT, costs=[5, 1, 3]), 1, 18),
    ("capacity", lambda network: network.set_capacity(0, 1), dataclasses.replace(DEPOT, capacities=[1, 4, 2]), 3, None),
    (
        "lower bound",
        lambda network: network.set_lower_bound(0, 3),
        dataclasses.replace(DEPOT, lower_bounds=[3, 0, 0]),
        0,
        17,
    ),
    (
        "bounds",
        lambda network: network.set_bounds(2, (3, 4)),
        dataclasses.replace(DEPOT, capacities=[3, 4, 4], lower_bounds=[0, 0, 3]),
        (0, 2),
        8,
    ),
    ("demand", lambda network: network.move_supply(2, 0, 1), dataclasses.replace(DEPOT, supplies=[5, 0, -5]), None, 19),
    ("supply", lambda network: network.move_supply(0, 1, 2), dataclasses.replace(DEPOT, supplies=[2, 2, -4]), None, 12),
    ("set supply", lambda network: network.set_supply(0, 2, 2), dataclasses.replace(DEPOT, supplies=[2, 0, -2]), 4, 4),
    (
        "set demand",
        lambda network: network.set_supply(2, -5, 0),
        dataclasses.replace(DEPOT, supplies=[5, 0, -5]),
        -4,
        19,
    ),
    (
        "set balancing",
        lambda network: network.set_supply(0, 5, 2),
        dataclasses.replace(DEPOT, supplies=[5, 0, -5]),
        4,
        19,
    ),
]

# The change that sets each of `CHANGES` back, by the number its setter answered or the move with the nodes swapped.
SET_BACKS: list[Change] = [
    lambda network: network.set_cost(2, 1),
    lambda network: network.set_capacity(0, 3),
    lambda network: network.set_lower_bound(0, 0),
    lambda network: network.set_bounds(2, kahlnws.Bounds(0, 2)),
    lambda network: network.move_supply(0, 2, 1),
    lambda network: network.move_supply(1, 0, 2),
    lambda network: network.set_supply(0, 4, 2),
    lambda network: network.set_supply(2, -4, 0),
    lambda network: network.set_supply(0, 4, 2),
]


@pytest.mark.parametrize(
    ("change", "problem", "replaced", "cost", "set_back"),
    [(*case[1:], set_back) for case, set_back in zip(CHANGES, SET_BACKS, strict=True)],
    ids=[case[0] for case in CHANGES],
)
def test_each_change_and_its_set_back_answer_what_a_fresh_solve_of_each_problem_answers(
    change: Change, problem: Network, replaced: object, cost: int | None, set_back: Change
) -> None:
    network = solved(DEPOT)

    assert change(network) == replaced
    assert network.optimum() is None, "a change ends the optimum held"
    assert_answers_as_a_fresh_solve(network, problem, cost)

    set_back(network)
    assert_answers_as_a_fresh_solve(network, DEPOT, DEPOT_COST)


def test_a_supply_set_back_by_the_supply_it_answered_restores_both_nodes_and_the_cost() -> None:
    network = solved(DEPOT)

    had = network.set_supply(2, -5, 0)
    assert (had, network.supplies(), network.solve().outcome) == (-4, [5, 0, -5], 19)
    assert (network.set_supply(2, had, 0), network.supplies(), network.solve().outcome) == (-5, [4, 0, -4], DEPOT_COST)


def test_a_node_that_changes_sides_by_hand() -> None:
    # The hub is to send 2, balanced by the depot, which sends 2 less; then to take 1, balanced by the customer, which
    # takes 3 less. The depot sends 1 to the hub and 1 on to the customer: 2 + 1 = 3.
    network = solved(DEPOT)

    assert (network.set_supply(1, 2, 0), network.set_supply(1, -1, 2)) == (0, 2)
    assert network.supplies() == [2, -1, -1]
    assert_answers_as_a_fresh_solve(network, dataclasses.replace(DEPOT, supplies=[2, -1, -1]), 3)


def test_a_node_that_balances_itself_and_is_to_change_raises_value_error_and_changes_nothing() -> None:
    network = solved(DEPOT)
    refused: list[tuple[Change, str]] = [
        (
            lambda network: network.set_supply(0, 5, 0),
            "node 0 is its own balancing node, whose supply 4 should then stay, not become 5",
        ),
        (
            lambda network: network.set_supply(2, -(2**63), 2),
            f"node 2 is its own balancing node, whose supply -4 should then stay, not become {-(2**63)}",
        ),
    ]

    for change, message in refused:
        with pytest.raises(ValueError, match=f"^{message}$") as raised:
            change(network)
        assert type(raised.value) is ValueError, "neither a refused change nor an index past its count"

    assert network.set_supply(1, 0, 1) == 0, "the supply it has changes nothing"
    assert read_back(network) == with_lower_bounds(DEPOT)
    optimum = network.optimum()
    assert optimum is not None, "nothing changed"
    assert network.solve().work == NO_WORK


def test_an_arc_switched_off_and_on_again_by_the_capacity_it_answered() -> None:
    network = solved(DEPOT_WITH_A_DEAR_LANE)

    capacity = network.set_capacity(1, 0)
    assert capacity == 4
    off = dataclasses.replace(DEPOT_WITH_A_DEAR_LANE, capacities=[3, 0, 2, 5])
    assert_answers_as_a_fresh_solve(network, off, 24)
    optimum = network.optimum()
    assert optimum is not None
    assert optimum.flows == [3, 0, 0, 1]

    assert network.set_capacity(1, capacity) == 0
    assert_answers_as_a_fresh_solve(network, DEPOT_WITH_A_DEAR_LANE, DEPOT_COST)


def test_bounds_that_cross_are_set_at_once_or_in_the_one_order_the_single_setters_admit() -> None:
    network = solved(DEPOT)
    crossed = dataclasses.replace(DEPOT, capacities=[3, 4, 4], lower_bounds=[0, 0, 3])

    with pytest.raises(kahlnws.ChangeError) as raised:
        network.set_lower_bound(2, 3)
    assert raised.value.breach == ChangeBreach.LowerAboveCapacity(arc=2, lower_bound=3, capacity=2)
    had = network.set_bounds(2, (3, 4))
    assert had == kahlnws.Bounds(lower_bound=0, capacity=2)
    assert type(had) is kahlnws.Bounds
    assert_answers_as_a_fresh_solve(network, crossed, 8)

    assert network.set_bounds(2, had) == (3, 4), "a set-back takes what the setter answered"
    assert (network.set_capacity(2, 4), network.set_lower_bound(2, 3)) == (2, 0), "the capacity first"
    assert_answers_as_a_fresh_solve(network, crossed, 8)


def test_bounds_are_read_and_answered_as_a_named_pair_and_set_from_any_pair_of_two_integers() -> None:
    network = solved(DEPOT)

    read = network.bounds(1)
    assert (read, read.lower_bound, read.capacity, type(read)) == ((0, 4), 0, 4, kahlnws.Bounds)
    assert repr(read) == "Bounds(lower_bound=0, capacity=4)"
    assert network.set_bounds(1, kahlnws.Bounds(capacity=5, lower_bound=1)) == read
    assert network.set_bounds(1, [2, 6]) == (1, 5)
    assert network.bounds(1) == (2, 6)

    refused: list[tuple[Change, type[Exception], str]] = [
        (
            lambda network: network.set_bounds(1, unchecked(4)),
            TypeError,
            "bounds is of type int, not a pair of integers",
        ),
        (
            lambda network: network.set_bounds(1, unchecked({0: 1})),
            TypeError,
            "bounds is of type dict, not a pair of integers",
        ),
        (lambda network: network.set_bounds(1, unchecked((0, 1, 2))), ValueError, "bounds has length 3, not 2"),
        (lambda network: network.set_bounds(1, unchecked((5,))), ValueError, "bounds has length 1, not 2"),
        (
            lambda network: network.set_bounds(1, unchecked((0, 1.5))),
            TypeError,
            "capacity is of type float, not an integer",
        ),
        (
            lambda network: network.set_bounds(1, (-1, 4)),
            OverflowError,
            f"lower_bound -1 is outside the range of an unsigned 64-bit integer, from 0 to {2**64 - 1}",
        ),
    ]
    for change, raised, message in refused:
        with pytest.raises(raised, match=f"^{message}$"):
            change(network)
    assert network.bounds(1) == (2, 6), "a refused pair changes nothing"


def test_a_change_to_the_number_held_changes_nothing_and_keeps_the_optimum() -> None:
    network = solved(DEPOT)
    unchanged: list[tuple[Change, object]] = [
        (lambda network: network.set_cost(0, 5), 5),
        (lambda network: network.set_capacity(0, 3), 3),
        (lambda network: network.set_lower_bound(0, 0), 0),
        (lambda network: network.set_bounds(2, [0, 2]), (0, 2)),
        (lambda network: network.move_supply(1, 1, 5), None),
        (lambda network: network.move_supply(0, 2, 0), None),
        (lambda network: network.set_supply(0, 4, 2), 4),
        (lambda network: network.set_supply(1, 0, 1), 0),
    ]

    for change, answered_number in unchanged:
        assert change(network) == answered_number
        optimum = network.optimum()
        assert optimum is not None, "no number changed"
        assert (optimum.total_cost, optimum.flows) == (DEPOT_COST, DEPOT_FLOWS)

    assert network.solve().work == NO_WORK


# Each refused change of the depot, the breach it is refused for, and the message.
REFUSED: list[tuple[Change, ChangeBreach, str]] = [
    (
        lambda network: network.set_cost(1, BOUND + 1),
        ChangeBreach.CostBeyondBound(arc=1, cost=BOUND + 1, node_count=3),
        f"arc 1 has cost {BOUND + 1} beyond the bound ±{BOUND} for the node count 3",
    ),
    (
        lambda network: network.set_cost(0, -BOUND - 1),
        ChangeBreach.CostBeyondBound(0, -BOUND - 1, 3),
        f"arc 0 has cost {-BOUND - 1} beyond the bound ±{BOUND} for the node count 3",
    ),
    (
        lambda network: network.set_lower_bound(0, 4),
        ChangeBreach.LowerAboveCapacity(arc=0, lower_bound=4, capacity=3),
        "arc 0 has lower bound 4 above its capacity 3",
    ),
    (
        lambda network: network.set_bounds(2, (3, 2)),
        ChangeBreach.LowerAboveCapacity(2, 3, 2),
        "arc 2 has lower bound 3 above its capacity 2",
    ),
    (
        lambda network: network.move_supply(2, 1, 2**64 - 1),
        ChangeBreach.SupplyBeyondRange(node=2, supply=-4 - (2**64 - 1)),
        "the supply of node 2 would be -18446744073709551619, outside the range of a signed 64-bit integer",
    ),
    (
        lambda network: network.move_supply(1, 0, 2**63),
        ChangeBreach.SupplyBeyondRange(node=0, supply=4 + 2**63),
        "the supply of node 0 would be 9223372036854775812, outside the range of a signed 64-bit integer",
    ),
    (
        lambda network: network.set_supply(2, -(2**63), 0),
        ChangeBreach.SupplyBeyondRange(node=0, supply=2**63),
        "the supply of node 0 would be 9223372036854775808, outside the range of a signed 64-bit integer",
    ),
]


@pytest.mark.parametrize(("change", "breach", "message"), REFUSED)
def test_each_refused_change_raises_its_breach_and_message_and_changes_nothing(
    change: Change, breach: ChangeBreach, message: str
) -> None:
    network = solved(DEPOT)

    with pytest.raises(kahlnws.ChangeError) as raised:
        change(network)

    assert (str(raised.value), raised.value.args) == (message, (message,))
    assert raised.value.breach == breach
    assert type(raised.value.breach) is type(breach)
    assert isinstance(raised.value, ValueError)
    assert not isinstance(raised.value, kahlnws.ProblemError)
    optimum = network.optimum()
    assert optimum is not None, "a refused change keeps the optimum"
    assert (optimum.total_cost, optimum.flows, optimum.potentials) == (DEPOT_COST, DEPOT_FLOWS, DEPOT_POTENTIALS)
    assert network.solve().work == NO_WORK


def test_a_cost_or_a_capacity_that_takes_the_cost_total_past_its_range_is_refused_at_the_arc_changed() -> None:
    bound = kahlnws.cost_bound(1)
    network = solved(WIDEST_LOOPS)
    message = f"the costs times the capacities, in magnitude, sum past {2**127 - 1} at arc 2"

    assert (network.set_cost(0, bound), network.set_cost(1, -bound)) == (0, 0)
    with pytest.raises(kahlnws.ChangeError, match=f"^{message}$") as raised:
        network.set_cost(2, bound)
    assert raised.value.breach == ChangeBreach.CostTotalBeyondRange(arc=2)

    assert network.set_capacity(2, 0) == 2**64 - 1
    assert network.set_cost(2, bound) == 0, "with no capacity, the cost adds nothing"
    with pytest.raises(kahlnws.ChangeError, match=f"^{message}$") as raised:
        network.set_capacity(2, 2**64 - 1)
    assert raised.value.breach == ChangeBreach.CostTotalBeyondRange(2)
    # The loop of negative cost carries all it can, and moves no supply.
    changed = dataclasses.replace(WIDEST_LOOPS, capacities=[2**64 - 1, 2**64 - 1, 0], costs=[bound, -bound, bound])
    assert_answers_as_a_fresh_solve(network, changed, -bound * (2**64 - 1))


def test_a_move_past_what_the_root_links_admit_is_refused_with_the_sum_and_changes_nothing() -> None:
    network = solved(FOUR_NODES)
    most = 2**63 - 1

    # No arc carries either move, so each goes up one root link and down another: the flow up the root links reaches
    # 2 * (2**63 - 1) = 2**64 - 2, the most admitted, exactly.
    network.move_supply(0, 1, most)
    network.move_supply(2, 3, most)
    before = network.copy()
    with pytest.raises(kahlnws.ChangeError) as raised:
        network.move_supply(2, 0, 1)

    assert raised.value.breach == ChangeBreach.ExcessBeyondRange(total=2**64 - 1)
    # The supply of node 0 set one lower, balanced by node 2, moves the same unit the other way: as refused.
    with pytest.raises(kahlnws.ChangeError) as set_raised:
        network.set_supply(0, -(2**63), 2)
    assert set_raised.value.breach == raised.value.breach
    assert str(raised.value) == (
        "this change would leave 18446744073709551615 units of flow still to be sent, past "
        "18446744073709551614; a solve that finds a flow leaves none"
    )
    # Nothing changed: with no arc, the supplies moved have no way, and the network solves as its copy made before the
    # refused move does, to no flow, with the same work.
    answer, copied = network.solve(), before.solve()
    assert answer.outcome is None
    assert copied == answer


def reads(optimum: kahlnws.NetworkOptimum, flows: "array[int]", potentials: "array[int]") -> list[Callable[[], object]]:
    """Every read of `optimum`, the writes into `flows` and `potentials` among them."""
    return [
        lambda: optimum.total_cost,
        lambda: optimum.flow(0),
        lambda: optimum.flows,
        lambda: optimum.flows_into(flows),
        lambda: optimum.potentials,
        lambda: optimum.potentials_into(potentials),
    ]


# Each call that ends the optimum answered before it: a change, a change to the number held, a solve, a load.
ENDS: list[Change] = [
    lambda network: network.set_cost(2, 3),
    lambda network: network.set_cost(2, 1),
    lambda network: network.solve(),
    lambda network: network.load(**dataclasses.asdict(DEPOT)),
]


@pytest.mark.parametrize("end", ENDS, ids=["change", "no change", "solve", "load"])
def test_an_optimum_reads_nothing_once_its_network_was_loaded_changed_or_solved(end: Change) -> None:
    network = solved(DEPOT)
    optimum = network.optimum()
    assert optimum is not None
    flows, potentials = array("Q", [7, 7, 7]), array("q", [7, 7, 7])
    assert optimum.total_cost == DEPOT_COST, "it reads before the call"

    end(network)

    for read in reads(optimum, flows, potentials):
        with pytest.raises(RuntimeError, match=r"^the network changed since this optimum was answered$"):
            read()
    assert (flows.tolist(), potentials.tolist()) == ([7, 7, 7], [7, 7, 7]), "a stale read writes nothing"


def test_a_refused_change_ends_the_optima_answered_before_it_while_the_network_keeps_its_optimum() -> None:
    network = solved(DEPOT)
    optimum = network.optimum()
    assert optimum is not None

    with pytest.raises(kahlnws.ChangeError):
        network.set_lower_bound(0, 4)
    with pytest.raises(IndexError):
        network.set_cost(3, 1)

    with pytest.raises(RuntimeError):
        _ = optimum.flows
    held = network.optimum()
    assert held is not None
    assert (held.total_cost, held.flows) == (DEPOT_COST, DEPOT_FLOWS)


def test_an_optimum_reads_the_network_until_the_next_load_change_or_solve() -> None:
    network = DEPOT.loaded()
    network.solve()
    solved_optimum, held = network.optimum(), network.optimum()
    assert solved_optimum is not None
    assert held is not None

    network.reserve(node_count=10, arc_count=10)
    assert (network.node_count, network.arc_count) == (3, 3)
    for optimum in [solved_optimum, held]:
        assert (optimum.total_cost, optimum.flows, optimum.potentials) == (
            DEPOT_COST,
            DEPOT_FLOWS,
            DEPOT_POTENTIALS,
        )


def test_a_copy_is_the_earlier_state_and_changes_apart_from_the_network() -> None:
    network = solved(DEPOT)
    copies = [network.copy(), copy.copy(network), copy.deepcopy(network)]

    network.set_cost(2, 3)
    network.move_supply(0, 1, 2)
    assert network.solve().outcome is not None

    for earlier in copies:
        assert type(earlier) is kahlnws.Network
        assert (earlier.node_count, earlier.arc_count) == (3, 3)
        optimum = earlier.optimum()
        assert optimum is not None, "the copy holds the optimum the network held"
        assert (optimum.total_cost, optimum.flows, optimum.potentials) == (
            DEPOT_COST,
            DEPOT_FLOWS,
            DEPOT_POTENTIALS,
        )
        assert earlier.solve().work == NO_WORK

        earlier.set_capacity(0, 1)
        assert earlier.solve().outcome is None
    # The hub sends its 2 units on at 3 a unit; the depot its 2 directly at 5, the hub's lane being full.
    assert_answers_as_a_fresh_solve(network, dataclasses.replace(DEPOT, costs=[5, 1, 3], supplies=[2, 2, -4]), 16)


def test_an_arc_or_a_node_not_below_its_count_raises_index_error_and_changes_nothing() -> None:
    network = solved(DEPOT)
    outside: list[tuple[Change, str]] = [
        (lambda network: network.set_cost(3, 1), "arc 3 is not below the arc count 3"),
        (lambda network: network.set_capacity(3, 1), "arc 3 is not below the arc count 3"),
        (lambda network: network.set_lower_bound(2**64 - 1, 0), f"arc {2**64 - 1} is not below the arc count 3"),
        (lambda network: network.set_bounds(3, (0, 1)), "arc 3 is not below the arc count 3"),
        (lambda network: network.move_supply(3, 0, 1), "node 3 is not below the node count 3"),
        (lambda network: network.move_supply(0, 4, 1), "node 4 is not below the node count 3"),
        (lambda network: network.move_supply(5, 4, 0), "node 5 is not below the node count 3"),
        (lambda network: network.set_cost(-1, 1), "arc -1 is not below the arc count 3"),
        (lambda network: network.set_capacity(2**64, 1), f"arc {2**64} is not below the arc count 3"),
        (
            lambda network: network.set_lower_bound(-(2**200), 0),
            "arc -(2**200 or more) is not below the arc count 3",
        ),
        (lambda network: network.set_bounds(-3, (0, 1)), "arc -3 is not below the arc count 3"),
        (lambda network: network.move_supply(-2, 0, 1), "node -2 is not below the node count 3"),
        (lambda network: network.move_supply(0, -1, 1), "node -1 is not below the node count 3"),
        (lambda network: network.set_supply(3, 1, 0), "node 3 is not below the node count 3"),
        (lambda network: network.set_supply(0, 1, 4), "node 4 is not below the node count 3"),
        (lambda network: network.set_supply(-1, 1, 0), "node -1 is not below the node count 3"),
        (lambda network: network.set_supply(5, 1, 5), "node 5 is not below the node count 3"),
    ]

    for change, message in outside:
        with pytest.raises(IndexError, match=f"^{re.escape(message)}$"):
            change(network)

    optimum = network.optimum()
    assert optimum is not None, "nothing changed"
    assert (optimum.total_cost, optimum.flows) == (DEPOT_COST, DEPOT_FLOWS)
    with pytest.raises(IndexError, match=r"^arc 3 is not below the arc count 3$"):
        optimum.flow(3)
    with pytest.raises(IndexError, match=r"^arc -1 is not below the arc count 3$"):
        optimum.flow(-1)
    assert network.solve().work == NO_WORK


# A number of a change outside the range of its type, and the message: the argument, the value and the range.
UNSIGNED = f"an unsigned 64-bit integer, from 0 to {2**64 - 1}"
SIGNED = f"a signed 64-bit integer, from {-(2**63)} to {2**63 - 1}"
OUT_OF_RANGE: list[tuple[Change, str]] = [
    (lambda network: network.set_capacity(0, -1), f"capacity -1 is outside the range of {UNSIGNED}"),
    (lambda network: network.set_cost(0, 2**63), f"cost {2**63} is outside the range of {SIGNED}"),
    (lambda network: network.set_cost(0, -(2**63) - 1), f"cost {-(2**63) - 1} is outside the range of {SIGNED}"),
    (lambda network: network.set_lower_bound(0, 2**64), f"lower_bound {2**64} is outside the range of {UNSIGNED}"),
    (lambda network: network.move_supply(2, 0, 2**64), f"amount {2**64} is outside the range of {UNSIGNED}"),
    (lambda network: network.move_supply(2, 0, -1), f"amount -1 is outside the range of {UNSIGNED}"),
    (lambda network: network.set_supply(0, 2**63, 1), f"supply {2**63} is outside the range of {SIGNED}"),
    (
        lambda network: network.set_cost(0, 2**200),
        f"cost 2**200 or more is outside the range of {SIGNED}",
    ),
]


@pytest.mark.parametrize(("change", "message"), OUT_OF_RANGE)
def test_a_number_outside_its_range_raises_overflow_error_naming_it_and_changes_nothing(
    change: Change, message: str
) -> None:
    network = solved(DEPOT)

    with pytest.raises(OverflowError, match=f"^{re.escape(message)}$"):
        change(network)

    assert read_back(network) == with_lower_bounds(DEPOT)
    assert network.optimum() is not None, "nothing changed"
    assert network.solve().work == NO_WORK


def unchecked(value: object) -> Any:
    """`value`, of a type the stubs refuse where a number is taken, which the test hands in to show it raises."""
    return value


class Two:
    """An integer of its own type, which `operator.index` takes: as NumPy's integers are."""

    def __index__(self) -> int:
        return 2


def test_an_argument_that_is_no_integer_raises_type_error_and_one_with_an_index_is_taken() -> None:
    network = solved(DEPOT)
    refused: list[tuple[Change, str]] = [
        (lambda network: network.set_cost(unchecked(1.0), 1), "arc is of type float, not an integer"),
        (lambda network: network.set_cost(0, unchecked("5")), "cost is of type str, not an integer"),
        (lambda network: network.move_supply(unchecked(None), 0, 1), "from_node is of type NoneType, not an integer"),
        (lambda network: network.move_supply(0, unchecked(1.5), 1), "to_node is of type float, not an integer"),
        (lambda network: network.move_supply(0, 1, unchecked(1.0)), "amount is of type float, not an integer"),
        (lambda network: network.supply(unchecked(0.0)), "node is of type float, not an integer"),
        (lambda network: network.set_supply(unchecked(None), 1, 0), "node is of type NoneType, not an integer"),
        (lambda network: network.set_supply(0, unchecked(1.0), 1), "supply is of type float, not an integer"),
        (lambda network: network.set_supply(0, 1, unchecked("2")), "balancing_node is of type str, not an integer"),
    ]

    for change, message in refused:
        with pytest.raises(TypeError, match=f"^{message}$"):
            change(network)
    assert read_back(network) == with_lower_bounds(DEPOT)

    assert network.set_cost(Two(), Two()) == 1
    assert (network.cost(Two()), network.tail(True)) == (2, 0)
    assert_answers_as_a_fresh_solve(network, dataclasses.replace(DEPOT, costs=[5, 1, 2]), 16)


def test_a_load_that_raises_keeps_the_problem_held_and_its_optimum() -> None:
    network = solved(DEPOT)

    with pytest.raises(kahlnws.ProblemError) as raised:
        network.load(**dataclasses.asdict(UNBALANCED))
    assert raised.value.breach == kahlnws.Breach.Unbalanced(total=1)
    with pytest.raises(OverflowError):
        network.load(**dataclasses.asdict(dataclasses.replace(DEPOT, capacities=[3, -4, 2])))
    with pytest.raises(TypeError):
        network.load(**(dataclasses.asdict(DEPOT) | {"costs": 5}))
    with pytest.raises(MemoryError, match=r"^the room for 18446744073709551615 nodes and 0 arcs was refused$"):
        network.reserve(2**64 - 1, 0)
    with pytest.raises(OverflowError, match=r"^node_count -1 is outside the range of a count, from 0 to "):
        network.reserve(-1, 0)
    with pytest.raises(TypeError, match=r"^arc_count is of type str, not an integer$"):
        network.reserve(0, "3")  # type: ignore[arg-type]  # a string is refused, which the test shows

    assert (network.node_count, network.arc_count) == (3, 3)
    optimum = network.optimum()
    assert optimum is not None
    assert (optimum.total_cost, optimum.flows) == (DEPOT_COST, DEPOT_FLOWS)
    assert network.solve().work == NO_WORK


@pytest.mark.parametrize(
    "problem",
    [
        DEPOT,
        CUT,
        dataclasses.replace(DEPOT, lower_bounds=[3, 0, 0]),
        Network(tails=[0, 1, 2], heads=[1, 2, 3], capacities=[5, 5, 5], costs=[1, 1, 1], supplies=[2, 0, 0, -2]),
        Network(tails=[], heads=[], capacities=[], costs=[], supplies=[0, 0]),
    ],
    ids=["depot", "cut", "lower bounds", "chain", "no arc"],
)
def test_one_network_loaded_with_problem_after_problem_answers_what_a_fresh_solve_answers(problem: Network) -> None:
    network = solved(DEPOT)
    network.load(**dataclasses.asdict(problem))
    assert (network.node_count, network.arc_count, network.optimum()) == (
        len(problem.supplies),
        len(problem.tails),
        None,
    )

    fresh = problem.solve(kahlnws.NetworkSimplex())
    assert_answers_as_a_fresh_solve(network, problem, None if fresh is None else fresh.total_cost)


def test_the_flows_and_the_potentials_are_written_into_buffers_and_what_cannot_take_them_is_refused() -> None:
    optimum = solved(DEPOT).optimum()
    assert optimum is not None
    flows, potentials = array("Q", [0, 0, 0]), array("q", [0, 0, 0])

    optimum.flows_into(flows)
    optimum.potentials_into(memoryview(potentials))
    assert (flows.tolist(), potentials.tolist()) == (DEPOT_FLOWS, DEPOT_POTENTIALS)

    refused: list[tuple[Callable[[], None], type[Exception], str]] = [
        (
            lambda: optimum.flows_into([0, 0, 0]),  # type: ignore[arg-type]  # a list is refused, which the test shows
            TypeError,
            "flows is of type list, not a writable buffer",
        ),
        (
            lambda: optimum.flows_into(array("q", [0, 0, 0])),
            TypeError,
            "flows holds items of format 'q', not unsigned 64-bit integers",
        ),
        (lambda: optimum.flows_into(array("Q", [0, 0])), ValueError, "flows holds 2 entries for a problem of 3 arcs"),
        (lambda: optimum.flows_into(memoryview(bytes(24)).cast("Q")), ValueError, "flows is read-only"),
        (
            lambda: optimum.potentials_into(array("Q", [0, 0, 0])),
            TypeError,
            "potentials holds items of format 'Q', not signed 64-bit integers",
        ),
        (
            lambda: optimum.potentials_into(array("q", [0] * 4)),
            ValueError,
            "potentials holds 4 entries for a problem of 3 nodes",
        ),
    ]
    for write, raised, message in refused:
        with pytest.raises(raised, match=f"^{message}$"):
            write()
    assert (flows.tolist(), potentials.tolist()) == (DEPOT_FLOWS, DEPOT_POTENTIALS)


def test_an_infeasible_solve_answers_none_and_holds_no_optimum_until_one_is_found() -> None:
    network = solved(DEPOT)

    network.set_capacity(0, 1)
    answer = network.solve()
    assert answer.outcome is None
    assert network.optimum() is None
    assert network.solve().outcome is None, "the same problem again"

    network.set_capacity(0, 3)
    assert_answers_as_a_fresh_solve(network, DEPOT, DEPOT_COST)


def read_back(network: kahlnws.Network) -> Network:
    """The problem `network` holds, read back column by column, as a problem a load takes."""
    return Network(
        tails=network.tails(),
        heads=network.heads(),
        capacities=network.capacities(),
        costs=network.costs(),
        supplies=network.supplies(),
        lower_bounds=network.lower_bounds(),
    )


def read_one_by_one(network: kahlnws.Network) -> Network:
    """The problem `network` holds, read back one arc and one node at a time."""
    arcs, nodes = range(network.arc_count), range(network.node_count)
    bounds = [network.bounds(arc) for arc in arcs]
    problem = Network(
        tails=[network.tail(arc) for arc in arcs],
        heads=[network.head(arc) for arc in arcs],
        capacities=[network.capacity(arc) for arc in arcs],
        costs=[network.cost(arc) for arc in arcs],
        supplies=[network.supply(node) for node in nodes],
        lower_bounds=[network.lower_bound(arc) for arc in arcs],
    )
    assert bounds == list(zip(problem.lower_bounds or [], problem.capacities, strict=True)), "the pair and its two"
    return problem


def with_lower_bounds(problem: Network) -> Network:
    """`problem` with its lower bounds given, zero where it gives none: as a network reads it back."""
    return dataclasses.replace(problem, lower_bounds=problem.lower_bounds or [0] * len(problem.tails))


def test_a_loaded_network_reads_back_the_problem_it_was_loaded_with() -> None:
    bounded = dataclasses.replace(DEPOT_WITH_A_DEAR_LANE, lower_bounds=[1, 0, 0, 2])
    network = bounded.loaded()

    assert read_back(network) == bounded
    assert read_one_by_one(network) == bounded
    assert read_back(DEPOT.loaded()) == with_lower_bounds(DEPOT)


@pytest.mark.parametrize(
    ("change", "problem"),
    [(case[1], case[2]) for case in CHANGES],
    ids=[case[0] for case in CHANGES],
)
def test_each_change_reads_back_as_the_changed_problem(change: Change, problem: Network) -> None:
    network = solved(DEPOT)

    change(network)

    assert read_back(network) == with_lower_bounds(problem)
    assert read_one_by_one(network) == with_lower_bounds(problem)


@pytest.mark.parametrize(("change", "breach", "message"), REFUSED)
def test_a_refused_change_reads_back_the_numbers_held_before(
    change: Change, breach: ChangeBreach, message: str
) -> None:
    network = solved(DEPOT)

    with pytest.raises(kahlnws.ChangeError, match=f"^{message}$"):
        change(network)

    assert read_back(network) == with_lower_bounds(DEPOT)


def test_a_network_never_loaded_reads_the_problem_of_no_node() -> None:
    network = kahlnws.Network()

    assert read_back(network) == Network(tails=[], heads=[], capacities=[], costs=[], supplies=[], lower_bounds=[])
    network.tails_into(array("Q"))
    network.supplies_into(array("q"))
    with pytest.raises(IndexError, match=r"^arc 0 is not below the arc count 0$"):
        network.bounds(0)
    with pytest.raises(IndexError, match=r"^node 0 is not below the node count 0$"):
        network.supply(0)


def test_a_read_needs_no_optimum_ends_none_and_leaves_one_readable() -> None:
    network = DEPOT.loaded()
    assert network.optimum() is None
    assert network.costs() == DEPOT.costs, "a read needs no optimum"

    assert network.solve().outcome == DEPOT_COST
    optimum = network.optimum()
    assert optimum is not None
    read_back(network)
    read_one_by_one(network)
    network.costs_into(array("q", [0, 0, 0]))

    assert (optimum.total_cost, optimum.flows, optimum.potentials) == (DEPOT_COST, DEPOT_FLOWS, DEPOT_POTENTIALS)
    assert network.solve().work == NO_WORK


def test_an_index_past_its_count_is_refused_by_every_read_of_one() -> None:
    network = DEPOT.loaded()
    reads: list[tuple[Callable[[], object], str]] = [
        (lambda: network.tail(3), "arc 3 is not below the arc count 3"),
        (lambda: network.head(3), "arc 3 is not below the arc count 3"),
        (lambda: network.cost(2**64 - 1), f"arc {2**64 - 1} is not below the arc count 3"),
        (lambda: network.bounds(3), "arc 3 is not below the arc count 3"),
        (lambda: network.lower_bound(3), "arc 3 is not below the arc count 3"),
        (lambda: network.capacity(-1), "arc -1 is not below the arc count 3"),
        (lambda: network.supply(3), "node 3 is not below the node count 3"),
        (lambda: network.tail(-1), "arc -1 is not below the arc count 3"),
        (lambda: network.head(-3), "arc -3 is not below the arc count 3"),
        (lambda: network.cost(-1), "arc -1 is not below the arc count 3"),
        (lambda: network.bounds(-(2**64)), f"arc {-(2**64)} is not below the arc count 3"),
        (lambda: network.supply(-1), "node -1 is not below the node count 3"),
    ]

    for read, message in reads:
        with pytest.raises(IndexError, match=f"^{message}$"):
            read()


def test_each_column_is_written_into_a_buffer_and_what_cannot_take_it_is_refused() -> None:
    bounded = dataclasses.replace(DEPOT, lower_bounds=[1, 0, 0])
    network = bounded.loaded()
    nodes_of_arcs = [array("Q", [9, 9, 9]) for _ in range(2)]
    numbers_of_arcs = [array("Q", [9, 9, 9]) for _ in range(2)]
    costs, supplies = array("q", [9, 9, 9]), array("q", [9, 9, 9])

    network.tails_into(nodes_of_arcs[0])
    network.heads_into(memoryview(nodes_of_arcs[1]))
    network.lower_bounds_into(numbers_of_arcs[0])
    network.capacities_into(numbers_of_arcs[1])
    network.costs_into(costs)
    network.supplies_into(supplies)

    written = [column.tolist() for column in [*nodes_of_arcs, *numbers_of_arcs, costs, supplies]]
    assert written == [bounded.tails, bounded.heads, [1, 0, 0], bounded.capacities, bounded.costs, bounded.supplies]

    refused: list[tuple[Callable[[], None], type[Exception], str]] = [
        (
            lambda: network.costs_into(array("Q", [0, 0, 0])),
            TypeError,
            "costs holds items of format 'Q', not signed 64-bit integers",
        ),
        (
            lambda: network.tails_into(array("q", [0, 0, 0])),
            TypeError,
            "tails holds items of format 'q', not unsigned integers as wide as size_t",
        ),
        (
            lambda: network.supplies_into(array("q", [0, 0])),
            ValueError,
            "supplies holds 2 entries for a problem of 3 nodes",
        ),
        (
            lambda: network.capacities_into(array("Q", [0] * 4)),
            ValueError,
            "capacities holds 4 entries for a problem of 3 arcs",
        ),
        (lambda: network.lower_bounds_into(memoryview(bytes(24)).cast("Q")), ValueError, "lower_bounds is read-only"),
        (
            lambda: network.heads_into([0, 0, 0]),  # type: ignore[arg-type]  # a list is refused, which the test shows
            TypeError,
            "heads is of type list, not a writable buffer",
        ),
    ]
    for write, raised, message in refused:
        with pytest.raises(raised, match=f"^{message}$"):
            write()
    assert costs.tolist() == bounded.costs, "a refused write writes nothing"


def test_a_copy_reads_as_its_source_and_a_copy_into_a_network_holds_what_its_source_holds() -> None:
    network = solved(DEPOT)
    network.set_cost(2, 3)
    network.move_supply(0, 1, 2)
    changed = read_back(network)

    copied = network.copy()
    assert read_back(copied) == changed

    into = solved(DEPOT_WITH_A_DEAR_LANE)
    into.reserve(node_count=3, arc_count=4)
    stale = into.optimum()
    assert stale is not None
    into.copy_from(network)

    assert read_back(into) == changed
    assert (into.node_count, into.arc_count, into.optimum()) == (3, 3, None), "the source held no optimum"
    with pytest.raises(RuntimeError, match=r"^the network changed since this optimum was answered$"):
        _ = stale.flows
    # The hub sends its 2 units on at 3 a unit; the depot its 2 directly at 5.
    assert_answers_as_a_fresh_solve(into, changed, 16)
    assert read_back(network) == changed, "the source is as it was"


def test_a_copy_into_a_network_starts_from_the_source_s_basis_and_changes_apart_from_it() -> None:
    network = solved(DEPOT)
    restored = kahlnws.Network()

    restored.copy_from(network)
    held = restored.optimum()
    assert held is not None
    assert (held.total_cost, held.flows) == (DEPOT_COST, DEPOT_FLOWS)
    assert restored.solve().work == NO_WORK, "a copy that started from the star would pivot"

    restored.set_capacity(0, 1)
    assert restored.solve().outcome is None
    optimum = network.optimum()
    assert optimum is not None, "the source keeps its optimum"
    assert optimum.total_cost == DEPOT_COST

    # A copy into itself is as it was; the optimum answered before it reads nothing, as after any change.
    before = network.optimum()
    network.copy_from(network)
    assert read_back(network) == with_lower_bounds(DEPOT)
    assert network.solve().work == NO_WORK
    assert before is not None
    with pytest.raises(RuntimeError):
        _ = before.flows
    with pytest.raises(TypeError):
        network.copy_from(DEPOT)  # type: ignore[arg-type]  # a problem held as lists is no network, which it shows


def test_a_node_is_brought_to_a_supply_it_should_have_by_set_supply_as_by_moving_the_difference() -> None:
    by_setter, by_moves = solved(DEPOT), solved(DEPOT)

    # The customer, node 2, is to take 3 units, balanced by the depot: its supply rises from -4 by 1, and the depot's
    # falls from 4 to 3. The depot is then to send 2, balanced by the hub: its supply falls from 3 by 1, and the hub's
    # rises to 1. Each move written by hand goes from the node whose supply falls to the one whose supply rises.
    assert by_setter.set_supply(2, -3, 0) == -4
    assert by_setter.set_supply(0, 2, 1) == 3
    by_moves.move_supply(0, 2, 1)
    by_moves.move_supply(0, 1, 1)

    assert by_setter.supplies() == by_moves.supplies() == [2, 1, -3]
    # The hub sends its unit by arc 2, at 1; the depot one by the hub, at 2, and one directly, at 5.
    assert by_moves.solve().outcome == 8
    assert_answers_as_a_fresh_solve(by_setter, dataclasses.replace(DEPOT, supplies=[2, 1, -3]), 8)


# The depot with one unit less to send. By hand: the hub's path carries its 2 units at 2 a unit and the direct arc the
# third at 5, a cost of 9. The depot's optimal basis holds arcs 0 and 1 strictly within their bounds and arc 2 full;
# with one unit less, arc 0 carries 1 and the others stay, so the basis kept carries the optimum, and no arc violates.
FEWER = dataclasses.replace(DEPOT, supplies=[3, 0, -3])

# The depot whose route through the hub costs 6 a unit, more than the direct arc: the direct arc fills with 3 units at 5
# and the hub's path carries the fourth at 6, a cost of 21.
DEARER = dataclasses.replace(DEPOT, costs=[5, 5, 1])

# Node 0 sends 2 units down a chain of four nodes: one node more than the depot, whose basis cannot serve it.
CHAIN = Network(tails=[0, 1, 2], heads=[1, 2, 3], capacities=[5, 5, 5], costs=[1, 1, 1], supplies=[2, 0, 0, -2])

# The depot's counts with other ends: arcs 1 and 2 swap theirs, so the depot's basis cannot serve it.
SWAPPED = Network(tails=[0, 1, 0], heads=[2, 2, 1], capacities=[3, 2, 4], costs=[5, 1, 1], supplies=DEPOT.supplies)

# Node 0 must send 2 units to node 1 over an arc of capacity 1; an arc back of capacity 5 does not help.
CUT_WITH_A_WAY_BACK = Network(tails=[0, 1], heads=[1, 0], capacities=[1, 5], costs=[1, 1], supplies=[2, -2])


def solution_and_work(network: kahlnws.Network) -> tuple[tuple[int, list[int], list[int]] | None, Work]:
    """What `network` solves to, as plain values (the cost, the flows and the potentials, or `None`), and the work."""
    answer = network.solve()
    optimum = network.optimum()
    found = None if optimum is None else (optimum.total_cost, optimum.flows, optimum.potentials)
    assert (None if found is None else found[0]) == answer.outcome
    return found, answer.work


def loaded_into(held: kahlnws.Network, problem: Network) -> kahlnws.Network:
    """`held`, loaded with `problem`."""
    held.load(**dataclasses.asdict(problem))
    return held


def test_the_problem_loaded_last_loaded_again_makes_no_pivot_where_a_fresh_network_makes_some() -> None:
    network = solved(DEPOT)

    again = solution_and_work(loaded_into(network, DEPOT))

    assert again == ((DEPOT_COST, DEPOT_FLOWS, DEPOT_POTENTIALS), NO_WORK)
    assert solution_and_work(DEPOT.loaded())[1] == DEPOT_WORK


def test_one_unit_less_is_loaded_onto_the_basis_kept_and_needs_no_pivot() -> None:
    found, work = solution_and_work(loaded_into(solved(DEPOT), FEWER))

    assert found is not None
    assert (found[0], found[1], work) == (9, [1, 2, 2], NO_WORK)
    FEWER.check(found[1], found[2], found[0])
    assert solution_and_work(FEWER.loaded())[1].pivots > 0, "a fresh network starts from the star and pivots"


def test_dearer_costs_on_the_same_arcs_start_from_the_basis_kept_and_answer_as_a_fresh_solve() -> None:
    network = loaded_into(solved(DEPOT), DEARER)

    found, work = solution_and_work(network)
    fresh = DEARER.solve(kahlnws.NetworkSimplex())
    assert found is not None
    assert fresh is not None
    assert (found[0], found[1]) == (fresh.total_cost, [3, 1, 1]) == (21, [3, 1, 1])
    DEARER.check(found[1], found[2], found[0])
    assert work.pivots < solution_and_work(DEARER.loaded())[1].pivots, "fewer pivots than from the star"


@pytest.mark.parametrize(
    "problem", [CHAIN, SWAPPED, CUT, FEWER], ids=["more nodes", "other ends", "fewer arcs", "same arcs"]
)
def test_a_load_answers_what_a_fresh_solve_answers_and_of_other_counts_or_ends_what_a_fresh_network_answers(
    problem: Network,
) -> None:
    found, work = solution_and_work(loaded_into(solved(DEPOT), problem))
    fresh_network = solution_and_work(problem.loaded())
    fresh = problem.solve(kahlnws.NetworkSimplex())

    assert (None if found is None else found[0]) == (None if fresh is None else fresh.total_cost)
    if problem.tails != DEPOT.tails or problem.heads != DEPOT.heads:
        assert (found, work) == fresh_network, "the basis held cannot serve: the load starts from the star"
        assert work.pivots > 0


def test_a_call_that_raises_between_two_loads_keeps_the_basis() -> None:
    network = solved(DEPOT)

    with pytest.raises(kahlnws.ProblemError):
        network.load(**dataclasses.asdict(UNBALANCED))
    with pytest.raises(OverflowError):
        network.load(**dataclasses.asdict(dataclasses.replace(FEWER, capacities=[3, -4, 2])))
    with pytest.raises(MemoryError):
        network.reserve(2**64 - 1, 0)

    assert solution_and_work(loaded_into(network, FEWER))[1] == NO_WORK, "the next load starts from the basis kept"


def test_an_infeasible_basis_is_kept_and_the_same_problem_loaded_again_makes_no_pivot() -> None:
    network = CUT_WITH_A_WAY_BACK.loaded()
    found, work = solution_and_work(network)
    assert found is None
    assert work.pivots > 0

    assert solution_and_work(loaded_into(network, CUT_WITH_A_WAY_BACK)) == (None, NO_WORK)


def test_one_network_loaded_day_after_day_answers_the_optimum_of_each() -> None:
    # Every answer a load leads to is optimal, whatever the loads before it: each is checked against its problem.
    network = kahlnws.Network()
    days = [DEPOT, FEWER, DEARER, CHAIN, CHAIN, DEPOT, CUT, FEWER, SWAPPED, DEPOT]

    for problem in days:
        found, _ = solution_and_work(loaded_into(network, problem))
        fresh = problem.solve(kahlnws.NetworkSimplex())
        if fresh is None:
            assert found is None, problem
        else:
            assert found is not None, problem
            assert found[0] == fresh.total_cost, problem
            problem.check(found[1], found[2], found[0])
