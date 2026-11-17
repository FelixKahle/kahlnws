# The C++ interface

For a C++ programmer who uses kahlnws. `crates/kahlnws_capi/include/kahlnws.hpp` is a header-only interface for C++11
and later over [the C interface](c-api.md): it includes `kahlnws.h`, and a program that includes it links the same
library, static or dynamic, as a C program does. It offers both levels of a solve, one whose answer owns its flows and
potentials and one that writes into buffers the caller holds, and a network that holds one problem through many small
changes. Errors come as values, and as exceptions where exceptions are enabled. The contract of a solve is in
[the problem and the guarantees](problem-and-guarantees.md).

The header documents every name in Doxygen form; this page is the tour.

Two types, one contract each:

| | The solver, `network_simplex` | A network, `network` |
|---|---|---|
| What it is | a function: a problem in, an answer out | a state: one problem and a basis of it, held |
| What it keeps | room, and nothing else | the problem, its basis and its optimum |
| What it answers | a function of the problem alone | a function of the problem loaded and the calls made since |
| Reach for it | to answer one problem | to hold a problem, change it, load new numbers on its arcs, and solve it |

The solver is `try_solve` and `try_solve_into`; a network is [a problem held](#a-problem-held).

## Including the header and linking

Both headers lie in `crates/kahlnws_capi/include/`. A program includes `kahlnws.hpp` alone and links the library the
crate `kahlnws_capi` builds, as [the C guide](c-api.md) describes:

```sh
cargo build --release -p kahlnws_capi
c++ -std=c++11 -Icrates/kahlnws_capi/include program.cpp target/release/libkahlnws_capi.a -o program
```

Everything is in the namespace `kahlnws`, named as the standard library names things. A program that links the
dynamic library on Windows defines `KAHLNWS_SHARED` before the include.

## The types

| Type | What it is |
|---|---|
| `problem` | The six tables of a problem, as views: plain data with public fields |
| `span<T>` | A view of a run of `T` that owns nothing: a pointer and a count; `first(count)` cuts it shorter |
| `network_simplex` | The solver with the room it works in; movable, not copyable |
| `network` | A network: one problem held with a basis of it, changed, loaded and solved; copied by `try_clone` |
| `bounds` | The lower bound and the capacity of an arc of a network, as `try_set_bounds` takes and answers them |
| `answer<T>` | What a solve answers: its `outcome` and the `work` it did |
| `outcome<T>` | Optimal with a `T`, or infeasible |
| `solution` | The flows, the potentials and the cost of an optimal flow, owned |
| `optimum` | The cost of an optimal flow, whose columns went to the caller's buffers or stay in the network |
| `work` | The pivots of a solve, counted |
| `int128`, `uint128` | 128-bit values as two halves, to carry and to print |
| `result<T>` | A value, or the `error` that stood in its way |
| `error` | Why a call did not answer: its kind, its message and the `refusal` or the `change_refusal` |
| `change_refusal` | Why a network refused a change: its kind and the values of its `change_breach` |

A `problem` is filled by name, so two tables of one type are not swapped unseen:

```cpp
kahlnws::problem problem;
problem.tails = tails;            // the node each arc leaves
problem.heads = heads;            // the node each arc enters
problem.capacities = capacities;  // the most flow of each arc
problem.costs = costs;            // the cost of a unit of flow on each arc
problem.supplies = supplies;      // the supply of each node
// problem.lower_bounds stays empty where every lower bound is zero
```

Each field is a `span` of constant entries and takes a `std::vector`, a `std::array`, a C array, or a pointer with a
count. The tables must outlive every solve of the problem; a temporary container does not compile.

## Solving with an owned answer

`try_solve` answers a `result` that holds an `answer<solution>`. The solution owns its columns as vectors.

```cpp
kahlnws::network_simplex solver;
const kahlnws::result<kahlnws::answer<kahlnws::solution>> solved = solver.try_solve(problem);
if (!solved) {
    std::fprintf(stderr, "%s\n", solved.error().message());
    return 1;
}
if (const kahlnws::solution* solution = solved->outcome.optimal()) {
    // solution->flows, solution->potentials, solution->total_cost
} else {
    // no flow within the bounds meets the supplies
}
// solved->work.pivots, solved->work.degenerate_pivots, solved->work.first_arc_pivots
```

`outcome::optimal()` answers a pointer to the optimum, null where the problem is infeasible; `is_optimal()` and
`is_infeasible()` ask the same question.

## Solving into the caller's buffers

`try_solve_into` writes the flows, and the potentials where a buffer is given for them, into storage the caller
holds, and answers an `answer<optimum>` with the cost. After a reservation that covers the problem, a solve that
answers allocates nothing.

```cpp
if (!solver.try_reserve(supplies.size(), tails.size())) {
    return 1;  // the memory was refused
}
std::vector<std::uint64_t> flows(tails.size());
std::vector<std::int64_t> potentials(supplies.size());

const kahlnws::result<kahlnws::answer<kahlnws::optimum>> written =
    solver.try_solve_into(problem, flows, potentials);
if (written && written->outcome.is_optimal()) {
    // flows and potentials are written; written->outcome.optimal()->total_cost is the cost
}
```

The flows hold one entry per arc and the potentials one per node. A buffer of another length, or one that shares
storage with a table or with the other buffer, is an error of the kind `invalid_argument`, and nothing is written.
Where the problem is infeasible, or refused, or an argument is not admitted, the buffers hold what they held before;
an internal error leaves them unspecified.

## A problem held

The solver answers each problem from nothing kept. A program that solves one problem many times, with a few numbers
changed between, as a search does that tries a move and keeps it or sets it back, or as a plan does that loads each
day's numbers on the same arcs, hands the problem to a `kahlnws::network` instead. The network copies the tables and
holds a basis of them; each change keeps that basis a basis of the problem as it then stands. It changes one number or
one pair at a time, at once and only where the change reaches, and its solve starts from that basis.

**Loading new numbers on the same arcs.** `try_load` is the network's entry for a whole problem, every number read
anew, so supplies, bounds, capacities and costs may all differ from those held. The solve after it answers the outcome
and the cost a `network_simplex` answers for the problem; the flow among tied optima, the potentials and the work are
the network's own. Where the problem has the node count, the arc count and the tail and head at every arc of the
problem held, the load starts that solve from the basis the network holds, wherever that basis serves the new
numbers, and otherwise from the start a fresh solve takes, every arc at its lower bound. So where only some numbers
changed, the solve often makes far fewer pivots than a fresh solve, and the same problem loaded again makes none.
[How it works](how-it-works.md#a-load-from-the-basis-held) says how a basis is carried over to new numbers. A refused
load leaves the network as it was, its basis included.

```cpp
for (std::size_t day = 0; day < days.size(); ++day) {
    problem.supplies = days[day];                                   // the same arcs, new numbers
    if (!network.try_load(problem)) {                               // from the basis held where it serves
        return 1;
    }
    const kahlnws::result<kahlnws::answer<kahlnws::optimum>> answered = network.try_solve();
    // answered->work.pivots: often few, none for the same day again
}
```

A network's arcs are fixed at the load, each with its tail and head: a change sets a number on an arc and never adds
or removes one. An arc is switched off by a capacity of zero, where its lower bound is zero.

**Changing a few numbers.** A search tries a move of one or more changes, solves, and keeps the move where the cost
falls or sets its changes back, each with the value its setter answered. A refused change changes nothing, so where a
later change of a move is refused, what the changes before it made is the caller's to set back. The calls of such a
search look so on the transport network of the examples, whose lane 1 carries up to 20 units at 6 a unit; the example
`search.cpp` tells a whole search of five moves:

```cpp
kahlnws::network network;
if (!network.try_load(problem)) {
    return 1;  // refused, as a solve would refuse it, or memory refused
}

// A move of two changes: lane 1 at 4 a unit, under a contract to carry at least 25 units.
const kahlnws::result<std::int64_t> cost = network.try_set_cost(1, 4);               // the 6 it had
const kahlnws::result<std::uint64_t> lower_bound = network.try_set_lower_bound(1, 25);
if (cost && !lower_bound) {                    // refused_change: above the capacity 20; the cost is set back
    static_cast<void>(network.try_set_cost(1, *cost));
}

// Another move of two changes: lane 3 switched off, and warehouse 0 to hold 25 units, balanced by warehouse 1.
const kahlnws::result<std::uint64_t> capacity = network.try_set_capacity(3, 0);      // the 20 it had
const kahlnws::result<std::int64_t> supply = network.try_set_supply(0, 25, 1);       // the 20 it had
const kahlnws::result<kahlnws::answer<kahlnws::optimum>> answered = network.try_solve();
if (answered && !better(*answered)) {                                                // set the numbers back
    static_cast<void>(network.try_set_supply(0, *supply, 1));
    static_cast<void>(network.try_set_capacity(3, *capacity));
}
const kahlnws::result<std::uint64_t> carried = network.try_flow(1);                  // or error_kind::no_optimum
```

| Form | What it does | Answers |
|---|---|---|
| `try_reserve` | room for problems of up to the counts given | nothing, or refused memory |
| `try_load` | holds a problem, its tables copied, with no optimum yet | nothing, a refused problem, or refused memory |
| `try_set_cost` | one arc's cost | the cost it had, or a refused change |
| `try_set_capacity` | one arc's capacity; zero switches it off | the capacity it had, or a refused change |
| `try_set_lower_bound` | one arc's lower bound | the lower bound it had, or a refused change |
| `try_set_bounds` | both as one `bounds`, for bounds crossing the old ones | the `bounds` it had, or a refused change |
| `try_set_supply` | one node's supply, balanced by another | the supply it had, or a refused change |
| `try_move_supply` | lowers one node's supply by `amount`, raises another's | nothing, or a refused change |
| `try_solve` | solves the problem as it stands | an `answer<optimum>`: the cost or infeasible, and the work |
| `try_total_cost`, `try_flow` | read the optimum held | the value, or `no_optimum` |
| `try_flows`, `try_potentials` | read the optimum held, owned | a vector, or `no_optimum` |
| `try_flows_into`, `try_potentials_into` | write the optimum held into a buffer | nothing, or `no_optimum` |
| `try_tail`, `try_head`, `try_cost`, `try_supply` | read one arc or node of the problem | the value |
| `try_lower_bound`, `try_capacity`, `try_bounds` | read one arc's bounds, apart or as the pair | the value |
| `try_tails`, `try_heads`, `try_costs`, `try_supplies` | read a column of the problem, owned | a vector |
| `try_lower_bounds`, `try_capacities` | read a column of the problem, owned | a vector |
| the same with `_into` | write a column of the problem held into a buffer | nothing |
| `try_clone` | a new network that is a copy | the copy, or refused memory |
| `try_clone_from` | makes this network a copy of another, reusing its room | nothing, or refused memory |

`node_count()` and `arc_count()` answer the counts held; an arc or a node not below its count is an `invalid_argument`,
and so is a buffer of the wrong length; its message names the number and the count, such as "arc 42 is not below the arc
count 8", and no function. A setter answers what it replaced in the shape it takes, so `network.set_bounds(arc, had)`
sets back the `bounds` `had` that `network.set_bounds(arc, wider)` answered. A copy holds the problem, the basis and the
optimum, solves from that basis and changes apart from its source: one made before a change is the earlier state itself.
With exceptions enabled, the copy constructor and the copy assignment copy too, the assignment through `try_clone_from`.
The reads of the problem answer it as it stands after the load and every change since; they need no optimum and end
none. A supply is positive where a node sends and negative where it takes. `try_set_supply` is the everyday call: it
gives a node the supply it should have, the balancing node moves by the same amount the other way, so the supplies
still add up to zero, and the same call with the supply it answered sets both back. A node that balances itself is
given only the supply it has, which changes nothing; any other supply for it answers an `invalid_argument`, as an index
not below its count does, its message giving the node and both supplies. `try_move_supply` is the pair move: it lowers
the supply of `from_node` by `amount` and raises the supply of `to_node` by `amount`, so a move of demand is written
from the node that will demand more.

What each form guarantees:

- **A setter answers what it replaced**, so setting that number back, or moving the supply back, makes the problem
  the earlier one again; the next solve answers the earlier outcome and cost, with a flow of that cost, which among
  tied optima may be another than before. A capacity of zero switches an arc off where its lower bound is zero, and
  the capacity it had switches it on.
- **A refused change, and a change that sets the number held, change nothing**, and keep the optimum the network held.
  Every other change, a load, and a solve that finds no flow end it: the reads answer `error_kind::no_optimum` until a
  solve finds one, and never a number of an earlier state. Nothing borrows the network, so every read checks.
- **A change is refused** as `error_kind::refused_change`, its `change_refusal()` naming the breach: a cost past the
  bound, a lower bound above its capacity, the costs times the capacities past their range, a supply past
  `std::int64_t`, or more than 2^64 - 2 units of flow still to be sent. A load leaves to be sent what its start does
  not route, which from no basis held is the whole of the positive supplies net of the lower bounds; a change adds
  what the arcs could not carry at once; only a solve that finds a flow leaves none.
  A breach a problem could have reads alike, with its message, but is its own type: a problem can be refused for what
  no change makes, and a change for what no problem meets.
- **A load refuses** what a solve refuses, as `refused_problem`; it and refused memory keep what the network held.
- **A new network**, and one moved from, holds the problem of no node: its solve answers the optimum of no flow at
  the cost zero, and it makes no handle until a reservation, a load or a solve needs one.
- **The answers** are a function of the problem loaded and the calls made since: two networks given the same calls
  answer alike, their work included. The outcome and the cost are those of the problem as it stands, the potentials
  prove the flow optimal, and the flow among tied optima, the potentials and the work are the network's own; its solve
  tries no arcs first, so `work.first_arc_pivots` is 0. Where a `network_simplex` admits the problem as it stands, it
  answers the same outcome and cost. After moves of supply and changes of lower bounds a network may hold numbers a
  solver and a load refuse, its supplies net of the lower bounds past the limit `breach::net_supply_beyond_range`
  names, where the arcs carry the difference; the network still solves them exactly.
- **After an internal error**, which only a defect of the library makes, the network may be half changed. An arc or a
  node not below the counts held before is still answered as `invalid_argument` first; beyond that its changes, its
  solve and its reads of the problem answer `internal_error`, its reads of the optimum `no_optimum`, its counts those
  held before, a copy is as half changed, and `try_load` mends it.

Allocation. A network allocates in `try_reserve`, in `try_load` and `try_clone_from` beyond its room, in `try_clone`,
and the vectors of the owned reads; after a reservation, no load, copy into it, change, solve or `_into` read
allocates, and a copy's changes and solves allocate nothing. Refused memory, for a handle or a room, is answered as
`refused_memory` in every form, the network then holding what it held.

What it costs, said plainly. A change costs what it moves: a cost that moves no potential, or a bound that leaves the
flow where it may stay, a few steps; a move of supply the flow held carries, a walk from the two nodes along that flow
to where their routes meet; one it cannot carry, the nodes whose potentials move, which can be most of the network. A
solve that makes no pivot prices only the arcs the changes may have made worth using: none for a move the flow held
carried, the arcs near it for a cost. A solve that pivots prices at least one whole pass over the arcs. And where a
change leaves much flow still to be sent, so that the solve makes many pivots, a network's solve can take longer than a
load of the whole problem as it stands and the solve after it: changes pay where only a few numbers change at a time, a
load where many do.

## Errors as values

Every call that can fail has a form whose name begins with `try_` and that never throws and is present in every build:
`try_reserve`, `try_solve`, `try_solve_into` and `try_cost_bound`, and each form of a network. Each answers a
`result`, which converts to `true` where it holds its value. `error()` says why it does not:

| `error_kind` | When |
|---|---|
| `refused_problem` | The problem lies outside what a solve admits; `refusal()` holds the breach |
| `invalid_argument` | A buffer has the wrong length or shares storage, or an arc or a node is not below its count |
| `refused_memory` | The memory for the room was refused |
| `internal_error` | The library failed; the solver or network may be used again |
| `refused_change` | A change lies outside what a network admits; `change_refusal()` holds the breach |
| `no_optimum` | A network holds no optimum to read |

A node given to `try_set_supply` as its own balancing node with a supply other than the one it has is an
`invalid_argument` too, as an index not below its count is.

A refusal is a value to branch on. Its `kind()` names the breach, and one accessor per kind answers a pointer to the
breach's values, null for another kind:

```cpp
const kahlnws::result<kahlnws::answer<kahlnws::solution>> refused = solver.try_solve(problem);
if (!refused && refused.error().kind() == kahlnws::error_kind::refused_problem) {
    const kahlnws::refusal& refusal = *refused.error().refusal();
    if (const kahlnws::breach::unbalanced* unbalanced = refusal.unbalanced()) {
        // unbalanced->total is what the supplies add up to
    }
    std::fprintf(stderr, "refused (%s): %s\n", kahlnws::name(refusal.kind()), refused.error().message());
}
```

The breaches and their conditions are those of [the problem and the guarantees](problem-and-guarantees.md). An
error's message is held inside the error, so building one allocates nothing.

## Errors as exceptions

Where exceptions are enabled, the plain names are the throwing forms: `reserve`, `solve`, `solve_into` and
`cost_bound`, and a network's forms without `try_`. Each calls its `try_` twin and throws what that answered:

| Error | Exception |
|---|---|
| A refused problem | `kahlnws::problem_error`, a `std::invalid_argument` that holds the `refusal` |
| A refused change | `kahlnws::change_error`, a `std::invalid_argument` that holds the `change_refusal` |
| An argument the call does not admit | `std::invalid_argument` |
| Refused memory | `std::bad_alloc` |
| No optimum held | `std::logic_error` |
| An internal error | `kahlnws::internal_error`, a `std::runtime_error` |

```cpp
try {
    const kahlnws::answer<kahlnws::solution> answer = solver.solve(problem);
    // answer.outcome, answer.work
} catch (const kahlnws::problem_error& error) {
    std::fprintf(stderr, "%s\n", error.what());  // error.refusal() holds the breach
}
```

Under `-fno-exceptions`, or where the program defines `KAHLNWS_NO_EXCEPTIONS` before the include, no throwing form
and no exception class exists: a call of `solve` does not compile, and the `try_` forms are the interface. The
macro `KAHLNWS_EXCEPTIONS` is 1 or 0 accordingly. The names of each mode lie in an inline namespace of their own, so
translation units of both modes may be linked into one program; a value of the header's types is not passed between
units of different modes.

## 128-bit values

The cost of a flow and the totals a refusal reports are 128-bit. `int128` and `uint128` carry such a value as two
halves, compare, hash, and print in decimal with `to_string` or a stream, which write what the C interface's
`kahlnws_int128_format` and `kahlnws_uint128_format` write; they are not arithmetic types. Where the compiler has a
native 128-bit integer, `to_native()` converts to it.

```cpp
const std::string cost = kahlnws::to_string(solution->total_cost);
```

## Moving a solver

A `network_simplex` is movable and not copyable. A solver moved into takes the room of the one it was moved from; a
solver that was moved from is a solver with no room, the same as a new one, and may be used again. A new solver holds no
memory; its room is made by a reservation or by its first solve. A `network` moves alike, a moved-from network being a
new one; `try_clone` and `try_clone_from` copy it in every build, and its copy constructor and copy assignment only
where exceptions are enabled.

## Threads

One thread at a time uses a solver or a network. Either may move from one thread to another between calls, and
distinct ones may be used on distinct threads at once.

## A complete program

```cpp
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <vector>

#include "kahlnws.hpp"

int main() {
    const std::vector<std::size_t> tails = {0, 0, 1};
    const std::vector<std::size_t> heads = {2, 1, 2};
    const std::vector<std::uint64_t> capacities = {3, 4, 2};
    const std::vector<std::int64_t> costs = {5, 1, 1};
    const std::vector<std::int64_t> supplies = {4, 0, -4};

    kahlnws::problem problem;
    problem.tails = tails;
    problem.heads = heads;
    problem.capacities = capacities;
    problem.costs = costs;
    problem.supplies = supplies;

    kahlnws::network_simplex solver;
    const kahlnws::result<kahlnws::answer<kahlnws::solution>> solved = solver.try_solve(problem);
    if (!solved) {
        std::fprintf(stderr, "%s\n", solved.error().message());
        return 1;
    }

    if (const kahlnws::solution* solution = solved->outcome.optimal()) {
        std::printf("cost %s, %llu pivots\n", kahlnws::to_string(solution->total_cost).c_str(),
                    static_cast<unsigned long long>(solved->work.pivots));
    } else {
        std::printf("no flow meets the supplies\n");
    }
    return 0;
}
```

It prints `cost 14, 4 pivots`, with exceptions enabled or not.

## Examples

`crates/kahlnws_capi/examples/` holds six programs, which the Rust and C guides show in their languages, each beside
its C twin. Each solves a small transport network, two warehouses that serve three shops, with the `try_` forms,
so that it builds with exceptions enabled or not:

| Example | What it shows |
|---|---|
| `transport.cpp` | the smallest useful call: one owned answer, its cost and the flow on each lane |
| `reuse.cpp` | one solver for three days of demands, its room reserved once, each day solved into the same buffer |
| `outcomes.cpp` | an infeasible outcome, a refused problem as an error value and, with exceptions, as thrown |
| `potentials.cpp` | the potentials, each lane's reduced cost, the optimality conditions checked, the solve's work |
| `network.cpp` | the days of `reuse.cpp` on a network, each loaded and solved from the basis kept, beside a solve |
| `search.cpp` | a search on the first day held: five moves, one refused at its second change, each kept or set back |

From the repository's root, an example builds and runs against the static library as any program does; on Linux the
libraries [the C guide](c-api.md#compiling-and-linking) names follow the library on the line:

```sh
cargo build --release -p kahlnws_capi
c++ -std=c++11 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
    crates/kahlnws_capi/examples/transport.cpp target/release/libkahlnws_capi.a -o transport
./transport
```

It prints `cost: 465` and then the flow on each lane, such as `warehouse 0 -> shop 1: 20`; built with
`-fno-exceptions` it prints the same. The crate's tests build every example in each mode and check every line it
prints.
