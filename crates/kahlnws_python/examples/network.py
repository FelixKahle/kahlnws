# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""A network held over three days: each day is loaded on the same lanes and solved from the basis the day before ended
on.

The network of the example `transport` is held for the three days of the example `reuse`: the warehouses hold 20 and
30 units every day, and the shops ask for (10, 25, 15), (20, 20, 10) and (5, 15, 30). Day 1 is loaded into a network
that held nothing and solved; each later day is loaded with that day's numbers and solved from the basis the network
kept. Each day prints its cost and the pivots its solve made, beside the pivots of a fresh solve of the same day by the
solver, which keeps nothing from one day to the next. The columns are plain lists. A day no flow serves ends the script
with an error.

Run it with `python crates/kahlnws_python/examples/network.py` from the repository root, in an environment where the
package is installed.
"""

import sys

import kahlnws

# Node w is warehouse w and node 2 + s shop s; lane 3 * w + s leads from warehouse w to shop s.
tails = [0, 0, 0, 1, 1, 1]
heads = [2, 3, 4, 2, 3, 4]
capacities = [20] * 6
costs = [8, 6, 10, 9, 12, 13]
demands = [(10, 25, 15), (20, 20, 10), (5, 15, 30)]

# Each day's cost, worked by hand: a flow costs what warehouse 1 would pay to serve every shop alone, less what each
# unit warehouse 0 sends in its place saves, 1, 6 and 3 at shops 0, 1 and 2. Beside it, the pivots of the network's
# solve and of a fresh one, which are not worked by hand: they are the counts this library's solves make, pinned so that
# the example shows a later day, started from the basis the day before ended on, taking far fewer.
days = [
    # Warehouse 1 alone: 9 * 10 + 12 * 25 + 13 * 15 = 585; warehouse 0's 20 units save 6 each at shop 1.
    (585 - 6 * 20, 6, 6),
    # Warehouse 1 alone: 180 + 240 + 130 = 550; warehouse 0's 20 units save 6 each at shop 1.
    (550 - 6 * 20, 1, 5),
    # Warehouse 1 alone: 45 + 180 + 390 = 615; its lane carries 20 of shop 2's 30, so warehouse 0 sends the other 10
    # there, saving 3 each, and its last 10 to shop 1, saving 6 each.
    (615 - 3 * 10 - 6 * 10, 1, 7),
]

# The network that keeps each day's basis, and the solver that solves each day from nothing kept.
network = kahlnws.Network()
fresh = kahlnws.NetworkSimplex()

for day, ((shop_0, shop_1, shop_2), expected) in enumerate(zip(demands, days, strict=True), start=1):
    supplies = [20, 30, -shop_0, -shop_1, -shop_2]
    problem = {"tails": tails, "heads": heads, "capacities": capacities, "costs": costs, "supplies": supplies}

    network.load(**problem)
    answer = network.solve()
    fresh_pivots = fresh.solve(**problem).work.pivots
    if answer.outcome is None:
        sys.exit(f"day {day}: no flow exists")
    cost, pivots = answer.outcome, answer.work.pivots
    print(f"day {day}: cost {cost}, pivots {pivots} (a fresh solve: {fresh_pivots})")
    assert (cost, pivots, fresh_pivots) == expected, f"day {day} should answer {expected}"
