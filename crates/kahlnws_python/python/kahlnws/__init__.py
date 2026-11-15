# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""A primal network simplex for the min-cost flow problem on exact integers.

A problem is a set of nodes, each with a supply, and arcs between them, each with a lower bound, a capacity and a
cost of a unit of flow; the answer is a flow of least cost that meets every supply within the bounds, or the answer
that no such flow exists. Every number is an integer, and every sum a solve forms is exact.

A depot (node 0) sends 4 units to a customer (node 2), directly or through a hub (node 1). A solve answers what it
found, the `outcome`, beside the `work` it did:

>>> import kahlnws
>>> solver = kahlnws.NetworkSimplex()
>>> answer = solver.solve(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4])
>>> answer.outcome
Solution(flows=[2, 2, 2], potentials=[0, 1, 5], total_cost=14)
>>> answer.work
Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0)

Where no flow meets the supplies, the outcome is `None`, and the work is answered all the same:

>>> solver.solve(tails=[0], heads=[1], capacities=[1], costs=[1], supplies=[2, -2])
Answer(outcome=None, work=Work(pivots=1, degenerate_pivots=0, first_arc_pivots=0))

A caller who solves in a loop reserves the solver's room once and has the flows written into a buffer of its own: an
`array.array` of type code `"Q"`, or a NumPy array made once with `numpy.empty(arc_count, dtype=numpy.uint64)`. The
outcome is then the cost:

>>> from array import array
>>> solver.reserve(node_count=3, arc_count=3)
>>> flows = array("Q", [0, 0, 0])
>>> solver.solve_into(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4],
...                   flows=flows).outcome
14
>>> flows.tolist()
[2, 2, 2]

A solver keeps the basis its last solve ended on. `resolve` and `resolve_into` take the same arguments as `solve` and
`solve_into` and start from it where it serves the problem, the same arcs with other numbers; where it cannot, they
answer exactly what a solve answers. With one unit less to send, the kept basis carries the optimum at once:

>>> answer = solver.resolve_into(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
...                              supplies=[3, 0, -3], flows=flows)
>>> answer.outcome, answer.work.pivots, flows.tolist()
(9, 0, [1, 2, 2])

A problem no solve admits raises `ProblemError`, a `ValueError` whose attribute `breach` names the breach and holds
its values:

>>> try:
...     solver.solve(tails=[0], heads=[5], capacities=[1], costs=[1], supplies=[1, -1])
... except kahlnws.ProblemError as error:
...     print(error.breach)
Breach.HeadOutsideNodes(arc=0, head=5, node_count=2)

Every column is copied before the solve, which then runs with the interpreter released. A sequence of `int` (a list,
a tuple, a range: a length and indexing) is converted entry by entry; a buffer of one dimension (a NumPy array, an
`array.array`, a `memoryview`) is copied in one block where its items are exactly the column's type (unsigned
integers as wide as `size_t` for the tails and the heads, unsigned 64-bit for the lower bounds and the capacities,
signed 64-bit for the costs and the supplies) and converted with a range check otherwise. NumPy is never imported by
the package. The package needs the GIL: it has not been tested on free-threaded interpreters.
"""

from kahlnws._kahlnws import (
    Answer,
    Breach,
    NetworkSimplex,
    ProblemError,
    Solution,
    Table,
    Work,
    __version__,
    cost_bound,
)

__all__ = [
    "Answer",
    "Breach",
    "NetworkSimplex",
    "ProblemError",
    "Solution",
    "Table",
    "Work",
    "__version__",
    "cost_bound",
]
