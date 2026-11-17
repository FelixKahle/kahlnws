# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""A short search over a network held: each move changes a few numbers, the network is solved, and the move is kept
where the cost falls below the best so far and set back where it does not.

The network of the example `transport` for its first day: two warehouses hold 20 and 30 units, three shops ask for 10,
25 and 15, every lane carries up to 20, and a unit costs 8, 6 and 10 from warehouse 0 to the shops and 9, 12 and 13
from warehouse 1. The search tries five moves: lane 3 switched off by a capacity of zero; lane 1 made cheaper under a
contract to carry more than it can, a move of two changes whose second is refused; five units of stock moved from
warehouse 1 to warehouse 0; warehouse 0 set to hold 15 units; and lane 4 made cheaper. A move that is set back is
undone by its changes set back, each with the number its setter answered; a refused change changes nothing, so the
changes of a move made before it are set back the same way. Each move prints what it came to, and the end the cost of
the moves kept, with the stock of each warehouse read back from the network. The columns are plain lists.

Run it with `python crates/kahlnws_python/examples/search.py` from the repository root, in an environment where the
package is installed.
"""

from collections.abc import Callable
from typing import TypeAlias

import kahlnws

# A change made, ready to be set back: the call that makes the change back, with the number its setter answered.
SetBack: TypeAlias = Callable[[], object]
# A change of the problem a network holds: it makes itself on the network, and answers the call that sets it back.
Change: TypeAlias = Callable[[kahlnws.Network], SetBack]


def lane_cost(lane: int, cost: int) -> Change:
    """The change that makes `cost` the cost of a unit of flow on `lane`."""

    def change(network: kahlnws.Network) -> SetBack:
        had = network.set_cost(lane, cost)
        return lambda: network.set_cost(lane, had)

    return change


def lane_lower_bound(lane: int, lower_bound: int) -> Change:
    """The change that makes `lower_bound` the least flow of `lane`."""

    def change(network: kahlnws.Network) -> SetBack:
        had = network.set_lower_bound(lane, lower_bound)
        return lambda: network.set_lower_bound(lane, had)

    return change


def lane_capacity(lane: int, capacity: int) -> Change:
    """The change that makes `capacity` the most flow of `lane`: zero switches it off."""

    def change(network: kahlnws.Network) -> SetBack:
        had = network.set_capacity(lane, capacity)
        return lambda: network.set_capacity(lane, had)

    return change


def node_supply(node: int, supply: int, balancing_node: int) -> Change:
    """The change that makes `supply` the supply of `node`, balanced by `balancing_node`, whose supply moves the other
    way."""

    def change(network: kahlnws.Network) -> SetBack:
        had = network.set_supply(node, supply, balancing_node)
        return lambda: network.set_supply(node, had, balancing_node)

    return change


def moved_supply(from_node: int, to_node: int, amount: int) -> Change:
    """The change that takes `amount` units from the supply of `from_node` and gives them to `to_node`."""

    def change(network: kahlnws.Network) -> SetBack:
        network.move_supply(from_node, to_node, amount)
        return lambda: network.move_supply(to_node, from_node, amount)

    return change


def set_back(set_backs: list[SetBack]) -> None:
    """Make each change back of `set_backs`, in order."""
    for back in set_backs:
        back()


def make_move(network: kahlnws.Network, changes: list[Change]) -> list[SetBack]:
    """Make `changes` on `network` in order, and answer the calls that set them back, the last made first. Where one is
    refused, set back those made before it and raise the refusal: the network then holds the problem it held."""
    set_backs: list[SetBack] = []
    try:
        for change in changes:
            set_backs.insert(0, change(network))
    except kahlnws.ChangeError:
        set_back(set_backs)
        raise
    return set_backs


# Node w is warehouse w and node 2 + s shop s; lane 3 * w + s leads from warehouse w to shop s.
network = kahlnws.Network()
network.load(
    tails=[0, 0, 0, 1, 1, 1],
    heads=[2, 3, 4, 2, 3, 4],
    capacities=[20] * 6,
    costs=[8, 6, 10, 9, 12, 13],
    supplies=[20, 30, -10, -25, -15],
)

# By hand: a flow costs what warehouse 1 would pay to serve every shop alone, less what each unit warehouse 0 sends in
# its place saves, 1, 6 and 3 at shops 0, 1 and 2 while the costs are those loaded. Warehouse 1 alone would pay
# 9 * 10 + 12 * 25 + 13 * 15 = 585; warehouse 0's 20 units save 6 each at shop 1, whose lane from it takes all 20: 465.
best = network.solve().outcome
print(f"start: cost {best}")
assert best is not None and best == 585 - 6 * 20, best

moves: list[tuple[list[Change], str]] = [
    # Lane 3 off: warehouse 0 serves shop 0, saving 1 a unit, and has 10 units left for shop 1: 585 - 10 - 60.
    ([lane_capacity(3, 0)], "cost 515, set back"),
    # Lane 1 at 4 a unit for at least 25 units: it carries at most 20, so the cost goes back to 6.
    (
        [lane_cost(1, 4), lane_lower_bound(1, 25)],
        "refused (arc 1 has lower bound 25 above its capacity 20), set back",
    ),
    # Warehouse 0 holds 25: 20 to shop 1 save 6 each and 5 to shop 2 save 3 each: 585 - 120 - 15.
    ([moved_supply(1, 0, 5)], "cost 450, kept"),
    # Warehouse 0 to hold 15, warehouse 1 the other 35: all 15 to shop 1 save 6 each: 585 - 90.
    ([node_supply(0, 15, 1)], "cost 495, set back"),
    # Lane 4 at 7: warehouse 1 alone pays 460; 15 units to shop 2 save 3 each, the other 10 1 each: 460 - 55.
    ([lane_cost(4, 7)], "cost 405, kept"),
]

for number, (changes, hand_worked) in enumerate(moves, start=1):
    try:
        set_backs = make_move(network, changes)
    except kahlnws.ChangeError as refusal:
        came_to = f"refused ({refusal}), set back"
    else:
        cost = network.solve().outcome
        if cost is not None and cost < best:
            best, came_to = cost, f"cost {cost}, kept"
        else:
            set_back(set_backs)
            came_to = ("no flow" if cost is None else f"cost {cost}") + ", set back"

    print(f"move {number}: {came_to}")
    assert came_to == hand_worked, f"move {number} should come to {hand_worked!r}"

# The moves kept: warehouse 0 holds 25 and warehouse 1 25, lane 4 costs 7, every other number is as loaded.
end = network.solve().outcome
print(f"end: cost {end}, the warehouses hold {network.supply(0)} and {network.supply(1)}")
assert (end, network.supply(0), network.supply(1)) == (best, 25, 25)
