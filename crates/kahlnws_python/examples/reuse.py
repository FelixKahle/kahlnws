# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""One solver kept for many solves: its room reserved once, each answer written into NumPy arrays made once.

The network of the example `transport` is solved for three days. The warehouses hold 20 and 30 units every day; the
shops ask for (10, 25, 15) on day 1, (20, 20, 10) on day 2 and (5, 15, 30) on day 3. The columns are NumPy arrays of
the types a solve copies in one block, and the flows and the potentials arrive in arrays the loop reuses.

It needs NumPy, which the package's extra installs: `pip install "kahlnws[numpy]"`. Run it with
`python crates/kahlnws_python/examples/reuse.py` from the repository root.
"""

import numpy

import kahlnws

tails = numpy.array([0, 0, 0, 1, 1, 1], dtype=numpy.uintp)
heads = numpy.array([2, 3, 4, 2, 3, 4], dtype=numpy.uintp)
capacities = numpy.full(6, 20, dtype=numpy.uint64)
costs = numpy.array([8, 6, 10, 9, 12, 13], dtype=numpy.int64)
demands = [(10, 25, 15), (20, 20, 10), (5, 15, 30)]

solver = kahlnws.NetworkSimplex()
solver.reserve(node_count=5, arc_count=6)
flows = numpy.empty(6, dtype=numpy.uint64)
potentials = numpy.empty(5, dtype=numpy.int64)

for day, (shop_0, shop_1, shop_2) in enumerate(demands, start=1):
    supplies = numpy.array([20, 30, -shop_0, -shop_1, -shop_2], dtype=numpy.int64)
    answer = solver.solve_into(
        tails=tails,
        heads=heads,
        capacities=capacities,
        costs=costs,
        supplies=supplies,
        flows=flows,
        potentials=potentials,
    )
    if answer.outcome is None:
        print(f"day {day}: no flow exists")
    else:
        print(f"day {day}: cost {answer.outcome}")
