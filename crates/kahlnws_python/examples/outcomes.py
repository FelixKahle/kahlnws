# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The answers that are no optimal flow: a problem with no flow, which is an outcome, and a refused problem, which is
an exception.

The network of the example `transport` is solved twice more. With every lane cut to 5 units, warehouse 0 can send 15
of its 20 units at most: the outcome is `None`, and the work is answered all the same. With the last lane drawn to node
5, which the five nodes do not have, the problem is refused before anything runs: `kahlnws.ProblemError`, a
`ValueError`, is raised, whose message and attribute `breach` name the breach.

Run it with `python crates/kahlnws_python/examples/outcomes.py` from the repository root, in an environment where the
package is installed.
"""

import kahlnws

solver = kahlnws.NetworkSimplex()

narrow = solver.solve(
    tails=[0, 0, 0, 1, 1, 1],
    heads=[2, 3, 4, 2, 3, 4],
    capacities=[5] * 6,
    costs=[8, 6, 10, 9, 12, 13],
    supplies=[20, 30, -10, -25, -15],
)
if narrow.outcome is None:
    print("no flow exists")
else:
    print(f"cost: {narrow.outcome.total_cost}")

try:
    solver.solve(
        tails=[0, 0, 0, 1, 1, 1],
        heads=[2, 3, 4, 2, 3, 5],
        capacities=[20] * 6,
        costs=[8, 6, 10, 9, 12, 13],
        supplies=[20, 30, -10, -25, -15],
    )
except kahlnws.ProblemError as error:
    print(f"refused: {error}")
else:
    print("solved")
