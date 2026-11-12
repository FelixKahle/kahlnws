# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Everything an answer holds beyond the flow: the potentials that prove it optimal, and the work of the solve.

The network of the example `transport` is solved once more. Each lane's reduced cost is its cost plus the potential of
its warehouse less the potential of its shop. The flow is optimal because every lane below its capacity has a reduced
cost of at least zero and every lane that carries flow one of at most zero, which the example checks without trusting
the solver. The work counts the pivots the solve made. The columns are plain lists.

Run it with `python crates/kahlnws_python/examples/potentials.py` from the repository root, in an environment where
the package is installed.
"""

import kahlnws

WAREHOUSES = 2
tails = [0, 0, 0, 1, 1, 1]
heads = [2, 3, 4, 2, 3, 4]
capacities = [20] * 6
costs = [8, 6, 10, 9, 12, 13]

answer = kahlnws.NetworkSimplex().solve(
    tails=tails, heads=heads, capacities=capacities, costs=costs, supplies=[20, 30, -10, -25, -15]
)
solution = answer.outcome
if solution is None:
    print("no flow exists")
else:
    print(f"cost: {solution.total_cost}")

    potentials = solution.potentials
    for warehouse, potential in enumerate(potentials[:WAREHOUSES]):
        print(f"potential of warehouse {warehouse}: {potential}")
    for shop, potential in enumerate(potentials[WAREHOUSES:]):
        print(f"potential of shop {shop}: {potential}")

    optimal = True
    for warehouse, shop, capacity, cost, flow in zip(tails, heads, capacities, costs, solution.flows, strict=True):
        reduced_cost = cost + potentials[warehouse] - potentials[shop]
        print(f"reduced cost of warehouse {warehouse} -> shop {shop - WAREHOUSES}: {reduced_cost}")
        optimal &= (flow == capacity or reduced_cost >= 0) and (flow == 0 or reduced_cost <= 0)
    print("the optimality conditions hold" if optimal else "the optimality conditions fail")

print(f"pivots: {answer.work.pivots}")
print(f"degenerate pivots: {answer.work.degenerate_pivots}")
print(f"first arc pivots: {answer.work.first_arc_pivots}")
