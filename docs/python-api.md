# The Python interface

For a Python programmer who uses the package `kahlnws`. The crate `kahlnws_python` builds it: one wheel for CPython 3.11
and later, fully typed, with no dependency. Columns are sequences of `int` or buffers such as NumPy arrays and
`array.array`, and the package never imports NumPy itself. Every example on this page runs as shown, which the package's
tests check. The contract of a solve is in [the problem and the guarantees](problem-and-guarantees.md).

The package offers two types, one contract each:

| | The solver, `kahlnws.NetworkSimplex` | A network, `kahlnws.Network` |
|---|---|---|
| What it is | a function: a problem in, an answer out | a state: one problem held, with a basis of it |
| What it keeps | its room, and nothing else | the problem, a basis of it, and its optimum once solved |
| Its calls | `reserve`, `solve`, `solve_into` | `reserve`, `load`, the setters, `solve`, `optimum`, reads, copies |
| An answer | a function of the problem alone | a function of the calls made on it |

Reach for the solver to solve one problem and have its answer. Reach for a network to hold a problem and solve it each
time it changes: a few numbers at a time, as a search tries a move and keeps it or sets it back, or all its numbers on
the same arcs, as each day of a plan brings new supplies and costs, loaded from the basis the network holds. Both
answer the same outcome and the same cost for the same problem.

## Building and installing

The wheel is built with maturin, from the root of the repository:

```sh
uvx maturin==1.16.0 build --release --locked --manifest-path crates/kahlnws_python/Cargo.toml --out dist
uv pip install dist/kahlnws-*.whl
```

The second line installs it into the environment uv finds, such as an activated virtual environment; `pip install`
serves as well. The package's extra `numpy` installs NumPy beside it, for callers who hold their columns as arrays.
The one wheel serves every CPython from 3.11 on, since it is built against the stable ABI. The package needs the GIL:
it has not been tested on free-threaded interpreters.

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
caller owns, and answers an `Answer[int]` whose `outcome` is the cost, or `None`. An `Answer[int]` holds the cost as
its outcome and the `Work` beside it, so `answer.outcome == 14` says the cost is 14, and the flows are read from the
buffer. The flows need a writable buffer of one dimension with one unsigned 64-bit integer per arc, the potentials one
with a signed 64-bit integer per node. Both are checked before the solve and written only where it finds a flow;
otherwise they hold what they held before.

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

## A problem held

The solver solves the problem each call hands it, from nothing kept. A program that holds a problem and changes it
hands it to a `kahlnws.Network` instead. The network copies the columns, keeps a basis of the problem, changes one
number or one pair of supplies at a time, at once and only where the change reaches, and loads new numbers on the same
arcs. Its `solve` answers an `Answer[int]`, as `solve_into` does: the cost, or `None`, and the work; the flows and the
potentials of the optimum it found are read through `optimum()`.

```python
import kahlnws

network = kahlnws.Network()
network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4])
first = network.solve()
assert (first.outcome, first.work.pivots) == (14, 4)

# The customer to take 5, balanced by the depot, which sends one more: the flow held carries the unit, so the solve
# makes no pivot. The call answers the customer's demand before, -4, which set back would undo it.
assert network.set_supply(2, -5, 0) == -4
moved = network.solve()
assert (moved.outcome, moved.work.pivots) == (19, 0)
optimum = network.optimum()
assert optimum is not None and optimum.flows == [3, 2, 2]

# The hub's lane switched off: the direct lane carries 3 of the 5 units, so no flow exists; set back, it is on again.
capacity = network.set_capacity(1, 0)
assert network.solve().outcome is None and network.optimum() is None
network.set_capacity(1, capacity)
assert network.solve().outcome == 19
optimum = network.optimum()
assert optimum is not None

# The problem held reads back as it stands, beside the optimum, which a read leaves readable.
assert (network.supplies(), network.bounds(1), network.cost(0)) == ([5, 0, -5], kahlnws.Bounds(0, 4), 5)
assert optimum.flows == [3, 2, 2]

# Both bounds of the direct lane at once, 1 to 2, and set back by what the setter answered.
had = network.set_bounds(0, (1, 2))
assert had == kahlnws.Bounds(lower_bound=0, capacity=3) and network.solve().outcome is None
network.set_bounds(0, had)
assert network.solve().outcome == 19

# A network kept as the state to return to, restored into the network the search changes, within its room.
best = network.copy()
network.set_cost(0, 9)
network.copy_from(best)
assert network.costs() == [5, 1, 1] and network.solve().work.pivots == 0
```

A network's arcs are fixed at the load, each with its tail and head: a change sets a number on an arc and never adds
or removes one. An arc is switched off by a capacity of zero, where its lower bound is zero.

A search tries a move of one or more changes, solves, and keeps the move where the cost falls or sets its changes back,
each with the value its setter answered. A refused change changes nothing, so where a later change of a move is
refused, what the changes before it made is the caller's to set back. The example `search.py` tells a whole search of
five moves on the transport network of the examples, whose lane 1 carries up to 20 units at 6 a unit; its move whose
second change is refused reads:

```python
import kahlnws

network = kahlnws.Network()
network.load(
    tails=[0, 0, 0, 1, 1, 1],
    heads=[2, 3, 4, 2, 3, 4],
    capacities=[20] * 6,
    costs=[8, 6, 10, 9, 12, 13],
    supplies=[20, 30, -10, -25, -15],
)
assert network.solve().outcome == 465

# A move of two changes, lane 1 at 4 a unit under a contract for at least 25 units: the second is refused, and the
# first is set back with the cost its setter answered.
had = network.set_cost(1, 4)
try:
    network.set_lower_bound(1, 25)
except kahlnws.ChangeError as refusal:
    assert str(refusal) == "arc 1 has lower bound 25 above its capacity 20"
    network.set_cost(1, had)
assert network.cost(1) == 6 and network.lower_bound(1) == 0 and network.solve().outcome == 465
```

| Call | What it does | Answers |
|---|---|---|
| `reserve(node_count, arc_count)` | room for problems of up to the counts given | `None` |
| `load(tails=..., heads=..., ...)` | holds a problem, from the basis held where it serves | `None` |
| `set_cost(arc, cost)` | one arc's cost | the cost it had |
| `set_capacity(arc, capacity)` | one arc's capacity; zero switches it off | the capacity it had |
| `set_lower_bound(arc, lower_bound)` | one arc's lower bound | the lower bound it had |
| `set_bounds(arc, bounds)` | both, for bounds that cross each other's old value | the `Bounds` it had |
| `set_supply(node, supply, balancing_node)` | one node's supply, balanced by another | the supply it had |
| `move_supply(from_node, to_node, amount)` | `amount` from the supply of one node to that of another | `None` |
| `solve()` | solves the problem as it stands | an `Answer[int]`: the cost, `None` without a flow |
| `optimum()` | the optimum the network holds | a `NetworkOptimum`, or `None` |
| `copy()` | a copy that changes apart from the network | a `Network` |
| `copy_from(source)` | makes this network a copy of `source`, within its own room | `None` |
| `tail(arc)`, `head(arc)`, `cost(arc)`, `supply(node)` | read one number of the problem held | an `int` |
| `lower_bound(arc)`, `capacity(arc)` | read one arc's bound, as `bounds(arc)` holds it | an `int` |
| `bounds(arc)` | read one arc's lower bound and capacity, as `set_bounds` takes them | a `Bounds` |
| `tails()`, `heads()`, `lower_bounds()`, `capacities()`, `costs()`, `supplies()` | read a column of it | a `list` |
| `tails_into(tails)` and the other five `_into` forms | write a column of it into the caller's buffer | `None` |

`node_count` and `arc_count` answer the counts held. A `NetworkOptimum` reads the optimum from its network: `total_cost`
as an `int`, `flow(arc)`, the properties `flows` and `potentials` as a new list at each read, and `flows_into(flows)`
and `potentials_into(potentials)` into buffers the caller owns, checked and written as `solve_into` checks and writes
its own. `copy()`, `copy.copy` and `copy.deepcopy` answer a copy that changes apart from the network: made before a
change, it is the earlier state itself. `copy_from(source)` makes an existing network such a copy within its own room,
so a search that keeps a network for each state it may return to restores one without allocating; it ends every optimum
the network it writes into answered before, as a load does.

The reads of the problem held answer it as it stands: the numbers loaded, with every change admitted since and none a
refused change named. They are named and shaped as the columns `load` takes, so the columns read back load the same
problem again (the lower bounds read back as zeros where the load gave none), and each column has a read of one number
named as the column in the singular, from `tail(arc)` to `supply(node)`; the `_into` forms take buffers of the column's
own type (unsigned integers as wide as `size_t` for the tails and heads, unsigned 64-bit for the lower bounds and
capacities, signed 64-bit for the costs and supplies) and are checked and written as `flows_into` is. A read changes
nothing, needs no optimum, and ends none: an optimum answered before it still reads. A caller keeps no copy of the
problem of its own.

A load is the state's entry for a whole problem, every number read anew, so supplies, bounds, capacities and costs may
all differ from those held. The solve after it answers the outcome and the cost the solver answers for the problem; the
flow among tied optima, the potentials and the work are the network's own. Where the problem has the node count, the arc
count and the tail and head at every arc of the problem held, the load starts that solve from the basis the network
holds, wherever that basis serves the new numbers, and otherwise from the start a fresh solve takes, every arc at its
lower bound. So where only some numbers changed, the solve often makes far fewer pivots than a fresh solve, and the same
problem loaded again makes none. [How it works](how-it-works.md#a-load-from-the-basis-held) says how a basis is carried
over to new numbers. A load that raises (a refused problem, a column that cannot be read, refused room) leaves the
network as it was, its basis included, and so does a refused `reserve`.

```python
import kahlnws

network, solver = kahlnws.Network(), kahlnws.NetworkSimplex()
depot = {"tails": [0, 0, 1], "heads": [2, 1, 2], "capacities": [3, 4, 2], "supplies": [4, 0, -4]}
network.load(**depot, costs=[5, 1, 1])
assert network.solve().work.pivots == 4

# The route through the hub now costs 6 a unit, more than the direct arc: one unit moves over, from the basis held.
network.load(**depot, costs=[5, 5, 1])
answer = network.solve()
optimum = network.optimum()
assert answer.outcome == 21 and optimum is not None and optimum.flows == [3, 1, 1]
assert answer.work.pivots < solver.solve(**depot, costs=[5, 5, 1]).work.pivots

# The same problem loaded again: the basis held is its optimum, so nothing moves.
network.load(**depot, costs=[5, 5, 1])
assert network.solve().work.pivots == 0
```

What each call guarantees:

- **A setter answers what it replaced**, so setting that number back, or moving the supply back with the nodes
  swapped, makes the problem the earlier one again; the next solve answers the earlier outcome and cost, with a flow of
  that cost, which among tied optima may be another than before. A capacity of zero switches an arc off where its
  lower bound is zero, and the capacity it had switches it on.
- **Both bounds of an arc are one `kahlnws.Bounds`**, a named tuple of `lower_bound` and `capacity`: `bounds(arc)`
  reads it and `set_bounds(arc, bounds)` answers the one it replaced, so a set-back reads
  `network.set_bounds(arc, had)`. `set_bounds` takes any pair of two integers, a plain tuple as well, and a `Bounds`
  unpacks, compares and pickles as the tuple it is.
- **A supply is set** by `set_supply(node, supply, balancing_node)`, the everyday call: it gives a node the supply it
  should have, positive where it sends flow and negative where it takes it; the balancing node moves by the same
  amount the other way, so the supplies still add up to zero; and the same call with the supply it answered sets both
  back. A node that balances itself is given only the supply it has, which changes nothing; any other supply for it
  raises `ValueError`, as an index not below its count raises `IndexError`.
- **A move of supply** is the pair move: `move_supply(from_node, to_node, amount)` lowers the supply of `from_node` by
  `amount` and raises the supply of `to_node` by `amount`, so a move of demand is written from the node that will
  demand more: a customer that needs two more units, sent by a depot, is `move_supply(customer, depot, 2)`.
- **A refused change, and a change that sets the number held, change nothing**, and keep the optimum the network held.
  Every other change, and a load, end it, and so does a solve that finds no flow: `optimum()` answers `None` until a
  solve finds one.
- **An optimum is lent**, as a borrow is in Rust: it reads its network only until the next call of `load`, a setter,
  `move_supply`, `copy_from` or `solve` on it, whatever that call answers or raises. A read after it raises
  `RuntimeError` ("the network changed since this optimum was answered") and never answers a number of an earlier state.
  Where the network still holds its optimum, after a refused change for one, `optimum()` answers it again. An optimum
  cannot be copied or pickled.
- **A change is refused** with `kahlnws.ChangeError`, a `ValueError` whose `breach` is a class of
  `kahlnws.ChangeBreach`: `LowerAboveCapacity`, `CostBeyondBound`, `CostTotalBeyondRange` (naming the arc changed),
  `SupplyBeyondRange` (a supply past a signed 64-bit integer) or `ExcessBeyondRange` (more than `2**64 - 2` units of
  flow still to be sent: a load leaves to be sent what its start does not route, a change adds what the arcs could not
  carry at once, and only a solve that finds a flow leaves none). A breach a problem could have reads alike, with
  its name, values and message, but is its own class: a problem can be refused for what no change makes, and a change
  for what no problem meets. An arc or a node below zero or not below its count raises `IndexError` ("arc -1 is not
  below the arc count 3"), a number outside its range `OverflowError`, which names the argument, its value and the range
  ("capacity -1 is outside the range of an unsigned 64-bit integer, from 0 to 18446744073709551615"), and an argument
  that is no integer `TypeError`; each changes nothing. An arc, a node or a number is an `int` or of a type with
  `__index__`, such as NumPy's integers.
- **A load refuses** what a solve refuses, with `ProblemError`. A refused load, and one that raises before, keep the
  problem the network held and the optimum it held.
- **A new network** holds the problem of no node: its solve answers the optimum of no flow at the cost zero, and no
  arc or node is below its counts.
- **The answers** are a function of the problem loaded and the calls made since: two networks given the same calls
  answer alike, their work included. The outcome and the cost are those of the problem as it stands, the potentials
  prove the flow optimal, and the flow among tied optima, the potentials and the work are the network's own; its solve
  tries no arcs first, so `work.first_arc_pivots` is 0. Where a `NetworkSimplex` admits the problem as it stands, it
  answers the same outcome and cost.
- **A network may hold numbers a solver refuses.** After moves of supply and changes of lower bounds every supply
  stays within a signed 64-bit integer, but the supplies net of the lower bounds may add up past the limit
  `Breach.NetSupplyBeyondRange` names, where the arcs carry the difference. The network still solves them exactly;
  the same numbers handed to `NetworkSimplex.solve` or to a load raise `ProblemError`.
- **After a `PanicException`**, which only a defect of the library raises, the network may be half changed: its
  changes, its solve and its reads of the problem raise it again, `optimum()` answers `None`, and a load mends it.

Allocation. A network allocates in `reserve`, in a load or a `copy_from` beyond its room, in `copy()`, and the lists
of the reads; after a reservation, no load, `copy_from`, change, solve or write into a buffer allocates room in the
network, and what a call still makes is its answer and, for a buffer, a `memoryview` and an export record. A copy's
changes and solves allocate nothing. `copy()` and `copy_from` raise `MemoryError` where the memory for the copy is
refused, `copy_from` with the network as it was.

What it costs, said plainly. A change costs what it moves: a cost that moves no potential, or a bound that leaves the
flow where it may stay, a few steps; a move of supply the flow held carries, a walk from the two nodes along that flow
to where their routes meet; one it cannot carry, the nodes whose potentials move, which can be most of the network. A
solve after changes that call for no pivot prices only the arcs those changes may have made worth using: none for a
move the flow held carried, the arcs near it for a cost. A solve that pivots prices at least one whole pass over the
arcs. And where a change leaves much flow still to be sent, so that the solve makes many pivots, a network's solve can
take longer than a load of the whole problem as it stands and the solve after it: changes pay where only a few numbers
change at a time, a load where many do.

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

What a call allocates: where the room holds the problem, `solve_into` allocates no room in the solver; it still makes
its answer, and a `memoryview` and an export record for each buffer it is handed. `solve` makes the lists of its
`Solution` besides. A network's load reads its columns as a solve does.

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
- `OverflowError`: an entry lies outside the range of its column, a number a network's change takes lies outside the
  range of its type, or a count given to `reserve` lies below zero or past the platform's `size_t`.
- `ValueError`: a buffer has other than one dimension, or an output is read-only, of another length than the
  problem's arcs or nodes, strided with items that cannot be written as they lie, or shares memory with the other
  output.
- `MemoryError`: the memory for the copy of a column or for the room of the problem is refused.
- `RuntimeError`: the solver or the network is already in a call.

A `ProblemError` is a `ValueError` too, so an `except ValueError` clause catches both; one for `ProblemError` first
separates them. A refused problem has no work, and may still have grown the solver's room.

A network adds four: a refused change raises `kahlnws.ChangeError`, a `ValueError` beside `ProblemError` and no kind of
it, whose `breach` is a class of `kahlnws.ChangeBreach`; an arc or a node below zero or not below its count raises
`IndexError`; a node given to `set_supply` as its own balancing node with a supply other than the one it has raises
`ValueError`; and a read of an optimum whose network was loaded, changed, copied into or solved since raises
`RuntimeError`.

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

A network's load and solve run with the interpreter released too, on the network's own copy of the problem, and its
changes hold the interpreter. A network serves one call at a time as a solver does, and a read of its optimum while it
is in a call raises `RuntimeError` as well. A network cannot be pickled; `copy()` copies it.

## Typing

The package ships its stubs and the marker `py.typed`, and passes a strict type check. `Answer` is generic in its
outcome: the solver's `solve` answers `Answer[Solution]`, and its `solve_into` and `Network.solve` answer `Answer[int]`;
`Network.optimum` answers `NetworkOptimum | None`. Columns are typed as sequences of integers or buffers, NumPy arrays
included on Python 3.11.

## Other calls

- `kahlnws.cost_bound(node_count)` answers the largest magnitude a cost may have in a problem of `node_count` nodes,
  `(2**62 - 1) // node_count`, and `2**62 - 1` for no node.
- `NetworkSimplex.reserve(node_count, arc_count)` makes the room for problems of up to those counts.
- `kahlnws.__version__` is the version of the package.

## Examples

Six scripts under `crates/kahlnws_python/examples/` solve one network, two warehouses that supply three shops, as a
user would. The examples of the other languages carry the same names and print the same lines.

| Example | What it shows |
|---|---|
| `transport.py` | the smallest useful call, with plain lists: one solve, its cost and the flow on each lane |
| `reuse.py` | one solver for three days of demand, its room reserved once, each day solved into NumPy arrays |
| `network.py` | the three days held by a network, each later day loaded onto the basis the day before ended on |
| `search.py` | a search on the first day held: five moves, one refused at its second change, each kept or set back |
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
stubs against the built module, and the tests in both, which run every example and compare every line it prints with the
lines the test lists, and run every example of this page. It needs uv, a Rust toolchain and a Python interpreter of at
least 3.11, which `KAHLNWS_PYTHON` names (`python3` where it is not set).
