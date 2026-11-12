# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The smallest useful call: two warehouses supply three shops at the least cost.

Warehouses 0 and 1 hold 20 and 30 units; shops 0, 1 and 2 ask for 10, 25 and 15. A lane runs from every warehouse to
every shop and carries up to 20 units. One solve answers the cost of the cheapest delivery and the flow on each lane.
The columns are plain lists.

Run it with `python crates/kahlnws_python/examples/transport.py` from the repository root, in an environment where the
package is installed.
"""

import kahlnws

# Nodes 0 and 1 are the warehouses, nodes 2, 3 and 4 the shops; the arcs are the six lanes.
WAREHOUSES = 2
tails = [0, 0, 0, 1, 1, 1]
heads = [2, 3, 4, 2, 3, 4]

answer = kahlnws.NetworkSimplex().solve(
    tails=tails,
    heads=heads,
    capacities=[20] * 6,
    costs=[8, 6, 10, 9, 12, 13],
    supplies=[20, 30, -10, -25, -15],
)

solution = answer.outcome
if solution is None:
    print("no flow exists")
else:
    print(f"cost: {solution.total_cost}")
    for warehouse, shop, flow in zip(tails, heads, solution.flows, strict=True):
        print(f"warehouse {warehouse} -> shop {shop - WAREHOUSES}: {flow}")
