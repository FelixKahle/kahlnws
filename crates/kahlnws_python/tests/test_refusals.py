# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Every breach a solve refuses a problem for, raised as `ProblemError` with the library's message and the breach's
values, through both levels and both entries of each, and by a network's load; and the breaches of a change, as plain
data."""

from array import array

import pytest

import kahlnws
from kahlnws import Breach, ChangeBreach, Table
from networks import Network

# The bound of a cost on four nodes: `(2**62 - 1) // 4`.
BOUND = kahlnws.cost_bound(4)

# A balanced problem on four nodes, each breach below made by changing one or two of its columns.
BASE = Network(tails=[0, 1, 2], heads=[1, 2, 3], capacities=[5, 5, 5], costs=[1, 1, 1], supplies=[1, 0, 0, -1])

# Each refused network, the breach it is refused for, and the message: one per kind a problem of Python can have.
REFUSALS = [
    (
        Network(BASE.tails, [1, 2], BASE.capacities, BASE.costs, BASE.supplies),
        Breach.Length(Table.HEADS, 2, 3),
        "the heads have length 2 where the tails have length 3",
    ),
    (
        Network(BASE.tails, BASE.heads, BASE.capacities, BASE.costs, BASE.supplies, lower_bounds=[0, 0, 0, 0]),
        Breach.Length(table=Table.LOWER_BOUNDS, length=4, expected=3),
        "the lower bounds have length 4 where the tails have length 3",
    ),
    (
        Network(BASE.tails, BASE.heads, [5], BASE.costs, BASE.supplies),
        Breach.Length(table=Table.CAPACITIES, length=1, expected=3),
        "the capacities have length 1 where the tails have length 3",
    ),
    (
        Network(BASE.tails, BASE.heads, BASE.capacities, [], BASE.supplies),
        Breach.Length(table=Table.COSTS, length=0, expected=3),
        "the costs have length 0 where the tails have length 3",
    ),
    (
        Network([0, 9, 2], BASE.heads, BASE.capacities, BASE.costs, BASE.supplies),
        Breach.TailOutsideNodes(arc=1, tail=9, node_count=4),
        "arc 1 leaves node 9, not below the node count 4",
    ),
    (
        Network(BASE.tails, [1, 2, 4], BASE.capacities, BASE.costs, BASE.supplies),
        Breach.HeadOutsideNodes(arc=2, head=4, node_count=4),
        "arc 2 enters node 4, not below the node count 4",
    ),
    (
        Network(BASE.tails, BASE.heads, BASE.capacities, BASE.costs, BASE.supplies, lower_bounds=[0, 6, 0]),
        Breach.LowerAboveCapacity(arc=1, lower_bound=6, capacity=5),
        "arc 1 has lower bound 6 above its capacity 5",
    ),
    (
        Network(BASE.tails, BASE.heads, BASE.capacities, [1, 1, -BOUND - 1], BASE.supplies),
        Breach.CostBeyondBound(arc=2, cost=-BOUND - 1, node_count=4),
        f"arc 2 has cost {-BOUND - 1} beyond the bound ±{BOUND} for the node count 4",
    ),
    (
        # On one node the bound is `2**62 - 1`; loops at it of capacity `2**64 - 1` pass `2**127 - 1` at the third.
        Network([0] * 4, [0] * 4, [2**64 - 1] * 4, [kahlnws.cost_bound(1)] * 4, [0]),
        Breach.CostTotalBeyondRange(arc=2),
        f"the costs times the capacities, in magnitude, sum past {2**127 - 1} at arc 2",
    ),
    (
        Network(BASE.tails, BASE.heads, BASE.capacities, BASE.costs, [2**63 - 1, 2**63 - 1, 0, 0]),
        Breach.Unbalanced(total=2**64 - 2),
        "the supplies add up to 18446744073709551614, not zero",
    ),
    (
        Network(BASE.tails, BASE.heads, BASE.capacities, BASE.costs, [-(2**63), -(2**63), 0, 0]),
        Breach.Unbalanced(total=-(2**64)),
        "the supplies add up to -18446744073709551616, not zero",
    ),
    (
        # Node 0 supplies `2**63 - 1` and an arc into it of lower bound `2**63` adds as much and one more.
        Network([1, 0], [0, 1], [2**63, 2**64 - 1], [0, 1], [2**63 - 1, -(2**63) + 1], lower_bounds=[2**63, 0]),
        Breach.NetSupplyBeyondRange(total=2**64 - 1),
        "the positive supplies net of the lower bounds add up to 18446744073709551615, past 18446744073709551614",
    ),
]


@pytest.mark.parametrize(("network", "breach", "message"), REFUSALS)
def test_each_breach_is_raised_with_its_message_and_values(network: Network, breach: Breach, message: str) -> None:
    with pytest.raises(kahlnws.ProblemError) as raised:
        network.solve(kahlnws.NetworkSimplex())

    assert str(raised.value) == message
    assert raised.value.args == (message,)
    assert raised.value.breach == breach
    assert type(raised.value.breach) is type(breach)
    assert isinstance(raised.value, ValueError)


@pytest.mark.parametrize(("network", "breach", "message"), REFUSALS)
def test_each_breach_is_raised_alike_by_the_low_level(network: Network, breach: Breach, message: str) -> None:
    flows = array("Q", [0] * len(network.tails))

    with pytest.raises(kahlnws.ProblemError, match=f"^{message}$") as raised:
        network.answer_into(kahlnws.NetworkSimplex(), flows)

    assert raised.value.breach == breach
    assert flows.tolist() == [0] * len(network.tails)


def test_a_breach_matches_by_its_class_and_values() -> None:
    with pytest.raises(kahlnws.ProblemError) as raised:
        Network([0, 9, 2], BASE.heads, BASE.capacities, BASE.costs, BASE.supplies).solve(kahlnws.NetworkSimplex())

    match raised.value.breach:
        case Breach.TailOutsideNodes(arc=arc, tail=tail, node_count=node_count):
            assert (arc, tail, node_count) == (1, 9, 4)
        case other:
            pytest.fail(f"the breach should be a tail outside the nodes, not {other!r}")


def test_where_two_breaches_meet_the_first_in_the_order_is_answered() -> None:
    # The lowest arc with a breach comes first, whatever its kind; of one arc, the first kind; the balance last.
    lowest = Network([0, 9, 2], BASE.heads, BASE.capacities, [BOUND + 1, 1, 1], [5, 0, 0, 0])
    own_first = Network([0, 1, 9], [1, 2, 9], BASE.capacities, BASE.costs, BASE.supplies)

    with pytest.raises(kahlnws.ProblemError) as raised:
        lowest.solve(kahlnws.NetworkSimplex())
    assert raised.value.breach == Breach.CostBeyondBound(arc=0, cost=BOUND + 1, node_count=4)

    with pytest.raises(kahlnws.ProblemError) as raised:
        own_first.solve(kahlnws.NetworkSimplex())
    assert raised.value.breach == Breach.TailOutsideNodes(arc=2, tail=9, node_count=4)


def test_breaches_are_plain_data_compared_and_hashed_by_value() -> None:
    assert Breach.Unbalanced(3) == Breach.Unbalanced(total=3)
    assert Breach.Unbalanced(3) != Breach.Unbalanced(4)
    unbalanced: Breach = Breach.Unbalanced(3)
    assert unbalanced != Breach.CostTotalBeyondRange(3), "breaches of two kinds differ, whatever their values"
    assert len({Breach.TooLarge(1, 2), Breach.TooLarge(node_count=1, arc_count=2), Breach.TooLarge(2, 1)}) == 2
    assert isinstance(Breach.TooLarge(1, 2), Breach)
    assert Breach.TooLarge.__match_args__ == ("node_count", "arc_count")


def test_breaches_and_tables_write_themselves_as_python_makes_them() -> None:
    breach = Breach.Length(table=Table.LOWER_BOUNDS, length=4, expected=3)

    assert repr(breach) == "Breach.Length(table=Table.LOWER_BOUNDS, length=4, expected=3)"
    assert repr(Breach.NetSupplyBeyondRange(2**128 - 1)) == f"Breach.NetSupplyBeyondRange(total={2**128 - 1})"
    assert [repr(table) for table in [Table.HEADS, Table.LOWER_BOUNDS, Table.CAPACITIES, Table.COSTS]] == [
        "Table.HEADS",
        "Table.LOWER_BOUNDS",
        "Table.CAPACITIES",
        "Table.COSTS",
    ]
    assert Table.HEADS == Table.HEADS != Table.COSTS
    assert len({Table.HEADS, Table.HEADS, Table.COSTS}) == 2


@pytest.mark.parametrize(("network", "breach", "message"), REFUSALS)
def test_each_breach_is_raised_alike_by_a_network_s_load_which_keeps_what_the_network_held(
    network: Network, breach: Breach, message: str
) -> None:
    held = BASE.loaded()

    with pytest.raises(kahlnws.ProblemError, match=f"^{message}$") as raised:
        held.load(
            tails=network.tails,
            heads=network.heads,
            capacities=network.capacities,
            costs=network.costs,
            supplies=network.supplies,
            lower_bounds=network.lower_bounds,
        )

    assert raised.value.breach == breach
    assert (held.node_count, held.arc_count) == (4, 3)
    held.solve()
    optimum = held.optimum()
    assert optimum is not None
    assert (optimum.total_cost, optimum.flows) == (3, [1, 1, 1]), "the base, one unit down its chain"


def test_breaches_of_a_change_are_plain_data_compared_hashed_and_written_by_value() -> None:
    assert ChangeBreach.ExcessBeyondRange(3) == ChangeBreach.ExcessBeyondRange(total=3)
    assert ChangeBreach.ExcessBeyondRange(3) != ChangeBreach.ExcessBeyondRange(4)
    lower: object = ChangeBreach.LowerAboveCapacity(1, 6, 5)
    assert lower != Breach.LowerAboveCapacity(1, 6, 5), "a change's breach is not a problem's, whatever its values"
    assert len({ChangeBreach.SupplyBeyondRange(1, 2), ChangeBreach.SupplyBeyondRange(node=1, supply=2)}) == 1
    assert isinstance(lower, ChangeBreach)
    assert ChangeBreach.CostBeyondBound.__match_args__ == ("arc", "cost", "node_count")
    assert repr(ChangeBreach.SupplyBeyondRange(node=2, supply=-(2**63) - 1)) == (
        "ChangeBreach.SupplyBeyondRange(node=2, supply=-9223372036854775809)"
    )
    assert repr(ChangeBreach.CostTotalBeyondRange(arc=4)) == "ChangeBreach.CostTotalBeyondRange(arc=4)"
