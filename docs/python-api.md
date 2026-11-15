# The Python interface

For a Python programmer who uses the package `kahlnws`. The crate `kahlnws_python` builds it: one wheel for CPython
3.11 and later, fully typed, with no dependency. Columns are sequences of `int` or buffers such as NumPy arrays and
`array.array`, and the package never imports NumPy itself. Every example on this page runs as shown. The contract of a
solve is in [the problem and the guarantees](problem-and-guarantees.md).

## Building and installing

The wheel is built with maturin, from the root of the repository:

```sh
uvx maturin==1.16.0 build --release --locked --manifest-path crates/kahlnws_python/Cargo.toml --out dist
uv pip install dist/kahlnws-*.whl
```

The second line installs it into the environment uv finds, such as an activated virtual environment; `pip install`
serves as well. The package's extra `numpy` installs NumPy beside it, for callers who hold their columns as arrays.
The one wheel serves every CPython from 3.11 on, since it is built
against the stable ABI. The package needs the GIL: it has not been tested on free-threaded interpreters.

## Solving with an owned answer

`NetworkSimplex.solve` takes every column by keyword and answers an `Answer[Solution]`. Its `outcome` is a `Solution`,
with the flows and the potentials as lists of `int` and the cost as an exact `int`, or `None` where no flow meets the
supplies. Its `work` is the `Work` of the solve either way.

```python
import kahlnws

solver = kahlnws.NetworkSimplex()

# A depot (node 0) sends 4 units to a customer (node 2), directly or through a hub (node 1).
answer = solver.solve(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4])
assert answer.outcome == kahlnws.Solution(flows=[2, 2, 2], potentials=[0, 1, 5], total_cost=14)
assert answer.work == kahlnws.Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0)

# The same arcs narrowed to one unit each carry 2 of the 4 units at most.
narrow = solver.solve(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[1, 1, 1], costs=[5, 1, 1], supplies=[4, 0, -4])
assert narrow.outcome is None
assert narrow.work.pivots == 3

# Lower bounds are one more keyword: the direct arc must carry 3 units.
bounded = solver.solve(
    tails=[0, 0, 1], heads=[2, 1, 2], lower_bounds=[3, 0, 0], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4]
)
assert bounded.outcome is not None
assert bounded.outcome.flows == [3, 1, 1] and bounded.outcome.total_cost == 17
```

`Answer`, `Solution` and `Work` are plain values: they compare and hash by value, match by position in `match`
statements, copy and pickle.

## Solving into buffers

`NetworkSimplex.solve_into` writes the flows, and the potentials where a buffer is given for them, into buffers the
caller owns, and answers an `Answer[int]` whose `outcome` is the cost, or `None`. The flows need a writable buffer of
one dimension with one unsigned 64-bit integer per arc, the potentials one with a signed 64-bit integer per node. Both
are checked before the solve and written only where it finds a flow; otherwise they hold what they held before.

With NumPy, the buffers are made once and the answer arrives in them without a conversion per entry:

```python
import numpy

import kahlnws

tails = numpy.array([0, 0, 1], dtype=numpy.uintp)
heads = numpy.array([2, 1, 2], dtype=numpy.uintp)
capacities = numpy.array([3, 4, 2], dtype=numpy.uint64)
costs = numpy.array([5, 1, 1], dtype=numpy.int64)

solver = kahlnws.NetworkSimplex()
solver.reserve(node_count=3, arc_count=3)
flows = numpy.empty(3, dtype=numpy.uint64)
potentials = numpy.empty(3, dtype=numpy.int64)

for demand in range(1, 6):
    supplies = numpy.array([demand, 0, -demand], dtype=numpy.int64)
    answer = solver.solve_into(
        tails=tails, heads=heads, capacities=capacities, costs=costs, supplies=supplies,
        flows=flows, potentials=potentials,
    )
    assert answer.outcome is not None
    print(demand, flows.tolist(), answer.outcome)
```

Without NumPy, `array.array` serves the same purpose, type code `"Q"` for the flows and `"q"` for the potentials:

```python
from array import array

import kahlnws

flows = array("Q", [0, 0, 0])
potentials = array("q", [0, 0, 0])
answer = kahlnws.NetworkSimplex().solve_into(
    tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4],
    flows=flows, potentials=potentials,
)
assert answer.outcome == 14
assert flows.tolist() == [2, 2, 2] and potentials.tolist() == [0, 1, 5]
```

## Solving again

A solver keeps the basis its last solve ended on, optimal or infeasible. `NetworkSimplex.resolve` and
`NetworkSimplex.resolve_into` take the same arguments as `solve` and `solve_into`, answer the same types and raise the
same exceptions, and start from that basis where it serves: where the problem has the node and arc counts of that solve
and every arc the tail and the head it had, and the supplies the basis leaves stay within the range a solve admits. The
tree is kept and every number is read anew, so supplies, bounds, capacities and costs may all have changed. Where only
some of them did, a resolve often makes far fewer pivots than a solve, and the same problem again, where the kept basis
serves, makes none. Where the basis cannot serve, as on a new solver or for a problem of other counts, a resolve answers
exactly what `solve` answers, its work included; so it is never wrong to call it.

```python
import kahlnws

solver = kahlnws.NetworkSimplex()
depot = {"tails": [0, 0, 1], "heads": [2, 1, 2], "capacities": [3, 4, 2], "supplies": [4, 0, -4]}
solved = solver.solve(**depot, costs=[5, 1, 1])
assert solved.work.pivots == 4

# The route through the hub now costs 6 a unit, more than the direct arc: one unit moves over.
resolved = solver.resolve(**depot, costs=[5, 5, 1])
assert resolved.outcome is not None
assert resolved.outcome.flows == [3, 1, 1] and resolved.outcome.total_cost == 21
assert resolved.work.pivots < solved.work.pivots

# The same problem again: the kept basis serves and is its optimum, so nothing moves.
assert solver.resolve(**depot, costs=[5, 5, 1]).work.pivots == 0
```

What each entry guarantees:

| | `solve`, `solve_into` | `resolve`, `resolve_into` |
|---|---|---|
| outcome and cost | those of the problem | the same |
| potentials | prove the flows optimal | prove the flows optimal |
| flows among tied optima, potentials, work | a function of the problem | of the problem and the solves before it |
| `work.first_arc_pivots` | as the problem's structure gives | 0 where it starts from the kept basis |

Every entry keeps the basis its solve ended on, and a call that raises before a solve runs (a refused problem, a column
or an output that cannot be used, a refused room, `reserve` included) keeps the basis kept before. All four grow the
room alike, so where it holds the problem `resolve_into` allocates no room in the solver either.

## How columns are read

Every column is copied into the solver's own memory before the solve starts, and the solve then runs on that copy
with the interpreter released. A column is either a sequence of `int` (a list, a tuple, a `range`: anything with a
length and indexing that is no mapping) or a buffer of integers of one dimension (a NumPy array, an `array.array`, a
`memoryview`), strided or not.

A buffer whose items are exactly the column's type is copied in one block; any other buffer of integers, and every
sequence, is converted entry by entry with a range check.

| Column | Copied in one block from | With NumPy |
|---|---|---|
| `tails`, `heads` | unsigned integers as wide as `size_t` (64 bits on the platforms tested) | `numpy.uintp` |
| `lower_bounds`, `capacities` | unsigned 64-bit integers | `numpy.uint64` |
| `costs`, `supplies` | signed 64-bit integers | `numpy.int64` |

So `numpy.int64` tails, NumPy's default integer type, are accepted and converted. A buffer of `bool` reads as 0 and 1.
Floats, characters, and integers in the other byte order than the machine's are refused.

What a call allocates: where the room holds the problem, `solve_into` and `resolve_into` allocate no room in the
solver; they still make their answer, and a `memoryview` and an export record for each buffer they are handed. `solve`
and `resolve` make the lists of their `Solution` besides.

## Refusals and other exceptions

A problem outside the ranges a solve admits raises `kahlnws.ProblemError`, a `ValueError` whose message is the
breach's and whose attribute `breach` holds it as a value. Each kind of breach is a class of `kahlnws.Breach`, with
its values as attributes, so a `match` statement takes it apart:

```python
import kahlnws

solver = kahlnws.NetworkSimplex()
try:
    solver.solve(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -3])
except kahlnws.ProblemError as error:
    assert str(error) == "the supplies add up to 1, not zero"
    match error.breach:
        case kahlnws.Breach.Unbalanced(total):
            assert total == 1
        case kahlnws.Breach.Length(table, length, expected):
            print(f"the {table} have {length} entries, not {expected}")
        case other:
            print(other)
```

The classes, in the order in which a problem's breaches are looked for, are `Breach.Length`, `Breach.TooLarge`,
`Breach.TailOutsideNodes`, `Breach.HeadOutsideNodes`, `Breach.LowerAboveCapacity`, `Breach.CostBeyondBound`,
`Breach.CostTotalBeyondRange`, `Breach.Unbalanced` and `Breach.NetSupplyBeyondRange`; `Breach.Length` names its
table as a member of `kahlnws.Table`. [The refusals](problem-and-guarantees.md#the-refusals) gives the condition of
each.

Other mistakes raise the built-in exceptions:

- `TypeError`: a column is neither a sequence nor a buffer of integers, an entry is no integer, a buffer's items
  are no integers in the machine's byte order, or an output is no buffer or its items are not exactly the integers
  written.
- `OverflowError`: an entry lies outside the range of its column, or a count given to `reserve` lies below zero or
  past the platform's `size_t`.
- `ValueError`: a buffer has other than one dimension, or an output is read-only, of another length than the
  problem's arcs or nodes, strided with items that cannot be written as they lie, or shares memory with the other
  output.
- `MemoryError`: the memory for the copy of a column or for the room of the problem is refused.
- `RuntimeError`: the solver is already in a call.

A `ProblemError` is a `ValueError` too, so an `except ValueError` clause catches both; one for `ProblemError` first
separates them. A refused problem has no work, keeps the basis the solver kept, and may still have grown the solver's
room.

## Threads and the GIL

A solve runs with the interpreter released, so other Python threads run meanwhile, and nothing they do reaches the
solve, which works on its own copy of the columns. A solver serves one call at a time: a second call on a solver that
is already in one raises `RuntimeError`. Distinct solvers solve at once on distinct threads:

```python
from concurrent.futures import ThreadPoolExecutor

import kahlnws


def cheapest(demand: int) -> int | None:
    answer = kahlnws.NetworkSimplex().solve(
        tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[demand, 0, -demand]
    )
    return None if answer.outcome is None else answer.outcome.total_cost


with ThreadPoolExecutor(max_workers=2) as pool:
    assert list(pool.map(cheapest, [2, 4, 6])) == [4, 14, None]
```

A running solve cannot be interrupted; a `KeyboardInterrupt` is raised once it returns. A solver cannot be pickled
or copied.

## Typing

The package ships its stubs and the marker `py.typed`, and passes a strict type check. `Answer` is generic in its
outcome: `solve` and `resolve` answer `Answer[Solution]`, `solve_into` and `resolve_into` answer `Answer[int]`. Columns
are typed as sequences of integers or buffers, NumPy arrays included on Python 3.11.

## Other calls

- `kahlnws.cost_bound(node_count)` answers the largest magnitude a cost may have in a problem of `node_count` nodes,
  `(2**62 - 1) // node_count`, and `2**62 - 1` for no node.
- `NetworkSimplex.reserve(node_count, arc_count)` makes the room for problems of up to those counts.
- `kahlnws.__version__` is the version of the package.

## Examples

Five scripts under `crates/kahlnws_python/examples/` solve one network, two warehouses that supply three shops, as a
user would. The examples of the other languages carry the same names and print the same lines.

| Example | What it shows |
|---|---|
| `transport.py` | the smallest useful call, with plain lists: one solve, its cost and the flow on each lane |
| `reuse.py` | one solver for three days of demand, its room reserved once, each day solved into NumPy arrays |
| `resolve.py` | the three days again, each later day resolved from the day before, its pivots beside a fresh solve's |
| `outcomes.py` | a problem with no flow, whose outcome is `None`, and a refused problem, raised as `ProblemError` |
| `potentials.py` | the potentials, each lane's reduced cost, the check of the optimality conditions, and the work |

`reuse.py` needs NumPy; the others need the package alone. In an environment where it is installed, from the root of
the repository:

```sh
python crates/kahlnws_python/examples/transport.py
```

## Checking the package

`crates/kahlnws_python/check.sh` builds the wheel, installs it into two fresh uv environments, one with NumPy and the
tools and one with no more than pytest, and runs the format check, the lint, the strict type check, the check of the
stubs against the built module, and the tests in both, which run every example and compare what it prints with its
optimum worked out by hand. It needs uv, a Rust toolchain and a Python interpreter of at least 3.11, which
`KAHLNWS_PYTHON` names (`python3` where it is not set).
