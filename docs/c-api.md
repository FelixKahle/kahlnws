# The C interface

For a C programmer who links kahlnws into a program. The crate `kahlnws_capi` builds the solver as a static and a
dynamic library, and its header `crates/kahlnws_capi/include/kahlnws.h` declares the interface for C99 and later; a
C++ compiler from C++11 on reads it as well. The header documents every declaration; this page shows how the pieces
fit. The contract of a solve is in [the problem and the guarantees](problem-and-guarantees.md).

Two handles, one contract each:

| | The solver, `kahlnws_solver` | A network, `kahlnws_network` |
|---|---|---|
| What it is | a function: a problem in, an answer out | a state: one problem and a basis of it, held |
| What it keeps | room, and nothing else | the problem, its basis and its optimum |
| What it answers | a function of the problem alone | a function of the problem loaded and the calls made since |
| Reach for it | to answer one problem | to hold a problem, change it, load new numbers on its arcs, and solve it |

The solver is `kahlnws_solver_solve`; a network is [a problem held](#a-problem-held).

## Building the library

```sh
cargo build --release -p kahlnws_capi
```

The build writes both libraries to `target/release/`:

| Library | File on macOS |
|---|---|
| static | `libkahlnws_capi.a` |
| dynamic | `libkahlnws_capi.dylib` |

Other platforms name the files by their own conventions. The libraries were built and linked on macOS on Apple
silicon only; Linux and Windows have not been tested.

## Compiling and linking

Against the static library, a program links the system libraries the Rust standard library needs beside it. `rustc`
names them for the target it builds for:

```sh
cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs
```

On macOS on Apple silicon it prints `-lSystem -lc -lm`, which the C compiler links by default, so a program builds
there with the static library alone; elsewhere the printed libraries follow it on the line:

```sh
cc -std=c99 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include depot.c \
    target/release/libkahlnws_capi.a -o depot
```

Against the dynamic library:

```sh
cc -std=c99 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include depot.c \
    -L target/release -lkahlnws_capi -o depot
```

On macOS the dynamic library's install name is the absolute path of the file `rustc` wrote in cargo's build directory,
not of its copy in `target/release/`, so a program linked against it loads it from there, and only while that file
exists. Where in the build directory it lies is cargo's own affair and differs between cargo's versions:
`otool -D target/release/libkahlnws_capi.dylib` prints it. A program that ships the library elsewhere sets another
install name, for example with `install_name_tool -id @rpath/libkahlnws_capi.dylib`, and adds the matching run path
to the program. On Windows, a program that links the dynamic library defines `KAHLNWS_SHARED` before it includes the
header, so that every function is imported from it.

## The header

The header is written by the crate's build from the library's sources: the overview, the version macros, and every
declaration with its documentation. It is committed, and a test fails where the committed header differs from the one
the build writes. After a change to the interface, this rewrites it:

```sh
KAHLNWS_WRITE_HEADER=1 cargo build -p kahlnws_capi
```

Every name the header declares starts with `kahlnws_` or `KAHLNWS_`.

## A complete program

The depot of the other guides: node 0 sends 4 units to node 2, directly or through node 1.

```c
#include <inttypes.h>
#include <stdio.h>

#include "kahlnws.h"

int main(void) {
    /* A depot (node 0) sends 4 units to a customer (node 2), directly or through a hub (node 1). */
    const size_t tails[] = {0, 0, 1};
    const size_t heads[] = {2, 1, 2};
    const uint64_t capacities[] = {3, 4, 2};
    const int64_t costs[] = {5, 1, 1};
    const int64_t supplies[] = {4, 0, -4};
    /* Every lower bound is zero, so `lower_bounds` is left out and stays null. */
    const kahlnws_problem problem = {.node_count = 3,
                                     .arc_count = 3,
                                     .tails = tails,
                                     .heads = heads,
                                     .capacities = capacities,
                                     .costs = costs,
                                     .supplies = supplies};

    uint64_t flows[3];
    int64_t potentials[3];
    kahlnws_int128 total_cost;
    kahlnws_work work;
    kahlnws_refusal refusal;

    kahlnws_solver *solver = kahlnws_solver_new();
    if (solver == NULL) {
        return 1;
    }

    kahlnws_status status = kahlnws_solver_solve(solver, &problem, flows, potentials, &total_cost, &work, &refusal);
    switch (status) {
    case KAHLNWS_STATUS_OPTIMAL: {
        char cost[41]; /* 41 bytes hold every 128-bit value in decimal */
        kahlnws_int128_format(total_cost, cost, sizeof cost);
        printf("flows %" PRIu64 " %" PRIu64 " %" PRIu64 ", cost %s, %" PRIu64 " pivots\n", flows[0], flows[1],
               flows[2], cost, work.pivots);
        break;
    }
    case KAHLNWS_STATUS_INFEASIBLE:
        printf("no flow meets the supplies, after %" PRIu64 " pivots\n", work.pivots);
        break;
    case KAHLNWS_STATUS_REFUSED: {
        char message[256];
        kahlnws_refusal_message(&refusal, message, sizeof message);
        printf("refused: %s\n", message);
        break;
    }
    default:
        printf("%s\n", kahlnws_status_name(status));
        break;
    }

    kahlnws_solver_free(solver);
    return status == KAHLNWS_STATUS_OPTIMAL ? 0 : 1;
}
```

It prints `flows 2 2 2, cost 14, 4 pivots`. The repository's test program `crates/kahlnws_capi/tests/depot.c` goes
further: an infeasible problem, a refused one with its message, the version and the names of statuses.

## Examples

`crates/kahlnws_capi/examples/` holds six programs, which the Rust and C++ guides show in their languages. Each solves
a small transport network, two warehouses that serve three shops, checks every status a call answers and frees its
solver or network on every path:

| Example | What it shows |
|---|---|
| `transport.c` | the smallest useful call: one solve, its cost and the flow on each lane |
| `reuse.c` | one solver for three days of demands, its room reserved once, each day solved into the same buffer |
| `outcomes.c` | the answers that are not an optimal flow: no flow exists, and a refused problem with its message |
| `potentials.c` | the potentials, each lane's reduced cost, the optimality conditions checked, the solve's work |
| `network.c` | the days of `reuse.c` on a network, each loaded and solved from the basis kept, beside fresh pivots |
| `search.c` | a search on the first day held: five moves, one refused at its second change, each kept or set back |

From the repository's root, an example builds and runs against the static library as any program does; on Linux the
libraries of [compiling and linking](#compiling-and-linking) follow the library on the line:

```sh
cargo build --release -p kahlnws_capi
cc -std=c99 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
    crates/kahlnws_capi/examples/transport.c target/release/libkahlnws_capi.a -o transport
./transport
```

It prints `cost: 465` and then the flow on each lane, such as `warehouse 0 -> shop 1: 20`. The crate's tests build
every example and check every line it prints.

## Ownership and lifetimes

The library owns only the solvers and the networks it makes. `kahlnws_solver_new` answers a new solver, or null where
the memory for it is refused; `kahlnws_solver_free` frees it with all its room, and accepts null. `kahlnws_network_new`
and `kahlnws_network_clone` answer a network, or null where the memory for it is refused, and `kahlnws_network_free`
frees it likewise. Every other pointer belongs to the caller: a call reads or writes through it while it runs and keeps
none of it afterwards.

A `kahlnws_problem` is plain data the caller fills in: the two counts and a pointer to each table. `lower_bounds` is
null where every arc's lower bound is zero. A table of no entries needs no storage, and its pointer may be null. The
outputs of a solve:

| Output | Storage | Written on |
|---|---|---|
| `flows` | `arc_count` entries; required unless `arc_count` is zero | `KAHLNWS_STATUS_OPTIMAL` |
| `potentials` | `node_count` entries, or null | `KAHLNWS_STATUS_OPTIMAL` |
| `total_cost` | one value, or null | `KAHLNWS_STATUS_OPTIMAL` |
| `work` | one value, or null | `KAHLNWS_STATUS_OPTIMAL` and `KAHLNWS_STATUS_INFEASIBLE` |
| `refusal` | one value, or null | `KAHLNWS_STATUS_REFUSED` |

On every other status each output is as it was, but after an internal error, which leaves them unspecified. An output
overlaps no other output and none of the problem's tables.

The room of a solver only grows. `kahlnws_solver_reserve(solver, node_count, arc_count)` makes it for problems of up
to those counts; a solve of such a problem, or of one no larger than an earlier solve's, allocates nothing. A solve
grows the room to the problem's counts before it checks the problem, so a refused problem may have grown it, and a
problem too large for memory answers `KAHLNWS_STATUS_NO_ROOM` before it could be refused as too large. The room is
all a solver keeps: what a solve answers, its work included, is a function of its problem alone.

## A problem held

The solver answers each problem from nothing kept. A program that solves one problem many times, with a few numbers
changed between, as a search does that tries a move and keeps it or sets it back, or as a plan does that loads each
day's numbers on the same arcs, hands the problem to a `kahlnws_network` instead. The network copies it and holds a
basis of it; each change keeps that basis a basis of the problem as it then stands. It changes one number or one pair
at a time, at once and only where the change reaches, and its solve starts from that basis.

**Loading new numbers on the same arcs.** `kahlnws_network_load` is the network's entry for a whole problem, every
number read anew, so supplies, bounds, capacities and costs may all differ from those held. The solve after it answers
the status and the cost a solver answers for the problem; the flow among tied optima, the potentials and the work are
the network's own. Where the problem has the node count, the arc count and the tail and head at every arc of the
problem held, the load starts that solve from the basis the network holds, wherever that basis serves the new
numbers, and otherwise from the start a fresh solve takes, every arc at its lower bound. So where only some numbers
changed, the solve often makes far fewer pivots than a fresh solve, and the same problem loaded again makes none.
[How it works](how-it-works.md#a-load-from-the-basis-held) says how a basis is carried over to new numbers. A refused
load leaves the network as it was, its basis included.

```c
for (size_t day = 0; day < days; day++) {
    problem.supplies = supplies[day];                                    /* the same arcs, new numbers */
    status = kahlnws_network_load(network, &problem, &refusal);          /* from the basis held where it serves */
    status = kahlnws_network_solve(network, &total_cost, &work);         /* often few pivots, none for the same day */
}
```

A network's arcs are fixed at the load, each with its tail and head: a change sets a number on an arc and never adds
or removes one. An arc is switched off by a capacity of zero, where its lower bound is zero.

**Changing a few numbers.** A search tries a move of one or more changes, solves, and keeps the move where the cost
falls or sets its changes back, each with the number its setter wrote. A refused change changes nothing, so where a
later change of a move is refused, what the changes before it made is the caller's to set back. The calls of such a
search look so on the transport network of the examples, whose lane 1 carries up to 20 units at 6 a unit; the example
`search.c` tells a whole search of five moves:

```c
kahlnws_network *network = kahlnws_network_new();
kahlnws_status status = kahlnws_network_load(network, &problem, &refusal);     /* KAHLNWS_STATUS_OK */

/* A move of two changes: lane 1 at 4 a unit, under a contract to carry at least 25 units. */
int64_t cost;
status = kahlnws_network_set_cost(network, 1, 4, &cost, &refusal);             /* OK: cost holds the 6 it had */
status = kahlnws_network_set_lower_bound(network, 1, 25, NULL, &refusal);      /* REFUSED: above the capacity 20 */
if (status == KAHLNWS_STATUS_REFUSED) {                                         /* the first change is set back */
    status = kahlnws_network_set_cost(network, 1, cost, NULL, &refusal);
}

/* Another move of two changes: lane 3 switched off, and warehouse 0 to hold 25 units, balanced by warehouse 1. */
uint64_t capacity;
int64_t supply;
status = kahlnws_network_set_capacity(network, 3, 0, &capacity, &refusal);      /* OK: capacity holds 20 */
status = kahlnws_network_set_supply(network, 0, 25, 1, &supply, &refusal);      /* OK: supply holds 20 */
status = kahlnws_network_solve(network, &total_cost, &work);                    /* OPTIMAL or INFEASIBLE */
if (!better) {                                                                   /* set the numbers back */
    status = kahlnws_network_set_supply(network, 0, supply, 1, NULL, &refusal);
    status = kahlnws_network_set_capacity(network, 3, capacity, NULL, &refusal);
}
status = kahlnws_network_flow(network, 1, &flow);                                /* OK or NO_OPTIMUM */
kahlnws_network_free(network);
```

The calls, each of which answers a status, as [the statuses](#statuses) say:

| Call | What it does |
|---|---|
| `kahlnws_network_reserve` | room for problems of up to the counts given |
| `kahlnws_network_load` | holds a problem, copied, with no optimum yet |
| `kahlnws_network_set_cost` | one arc's cost; writes the cost it had |
| `kahlnws_network_set_capacity` | one arc's capacity; zero switches it off |
| `kahlnws_network_set_lower_bound` | one arc's lower bound |
| `kahlnws_network_set_bounds` | both at once, for bounds that cross each other's old value |
| `kahlnws_network_set_supply` | one node's supply, balanced by another; writes the supply it had |
| `kahlnws_network_move_supply` | lowers one node's supply by `amount`, raises another's |
| `kahlnws_network_solve` | solves the problem as it stands; writes the cost and the work |
| `kahlnws_network_total_cost`, `_flow`, `_flows`, `_potentials` | read the optimum held |
| `kahlnws_network_tail`, `_head`, `_cost`, `_lower_bound`, `_capacity`, `_bounds`, `_supply` | read one number |
| `kahlnws_network_tails`, `_heads`, `_costs`, `_lower_bounds`, `_capacities`, `_supplies` | read a column |
| `kahlnws_network_copy` | makes one network a copy of another, reusing its room |

`kahlnws_network_node_count` and `kahlnws_network_arc_count` answer the counts held, 0 for null; `kahlnws_network_clone`
answers a new network that is a copy, or null where memory for it is refused. A copy holds the problem, the basis and
the optimum, solves from that basis and changes apart from its source: one made before a change is the earlier state
itself. The reads of the problem answer it as it stands after the load and every change since; they need no optimum and
end none, so a caller keeps no copy of its numbers of its own. A supply is positive where a node sends and negative
where it takes. `kahlnws_network_set_supply` is the everyday call: it gives a node the supply it should have, the
balancing node moves by the same amount the other way, so the supplies still add up to zero, and the same call with the
supply it wrote sets both back. A node that balances itself is given only the supply it has, which changes nothing; any
other supply for it answers `KAHLNWS_STATUS_INVALID_ARGUMENT`, as an index not below its count does.
`kahlnws_network_move_supply` is the pair move: it lowers the supply of `from_node` by `amount` and raises the supply of
`to_node` by `amount`, so a move of demand is written from the node that will demand more. Every pointer argument
answers `KAHLNWS_STATUS_NULL_POINTER` or `KAHLNWS_STATUS_INVALID_ARGUMENT` as for a solver, and an arc or a node not
below its count answers `KAHLNWS_STATUS_INVALID_ARGUMENT`; the outputs `previous` and `refusal` of the changes, and the
cost and work of the solve, may be null.

What each call guarantees:

- **A setter writes what it replaced**, so setting that number back, or moving the supply back, makes the problem
  the earlier one again; the next solve answers the earlier status and cost, with a flow of that cost, which among tied
  optima may be another than before. A capacity of zero switches an arc off where its lower bound is zero, and the
  capacity it had switches it on.
- **A refused change, and a change that sets the number held, change nothing**, and keep the optimum the network held.
  Every other change, a load, and a solve that finds no flow end it: the reads answer `KAHLNWS_STATUS_NO_OPTIMUM`
  until a solve finds one, and never write a number of an earlier state. Nothing in C can borrow the network, so every
  read checks.
- **A change is refused** with a breach in the same `kahlnws_refusal` a problem's refusal fills: a cost past the bound,
  a lower bound above its capacity, the costs times the capacities past their range, a supply past `int64_t`, or more
  than $2^{64} - 2$ units of flow still to be sent; each setter's doc says in which order it refuses. A load leaves to
  be sent what its start does not route, which from no basis held is the whole of the positive supplies net of the
  lower bounds; a change adds what the arcs could not carry at once; only a solve that finds a flow leaves none. So a
  change right after a load of supplies near that limit can be refused. A move back right after a move is admitted
  wherever the move left no more flow to be sent than before, always where the flow held carried it, and for every
  amount and flow below $2^{62}$.
- **A load refuses** what a solve refuses; it and a refused memory keep what the network held.
- **A network never loaded** holds the problem of no node: its solve answers the optimum of no flow at the cost zero.
- **The answers** are a function of the problem loaded and the calls made since: two networks given the same calls
  answer alike, their work included. The status and the cost are those of the problem as it stands, the potentials
  prove the flow optimal, and the flow among tied optima, the potentials and the work are the network's own; its solve
  tries no arcs first, so `first_arc_pivots` is 0. Where a solver admits the problem as it stands, it answers the same
  status and cost. After moves of supply and changes of lower bounds a network may hold numbers a solver and a load
  refuse, its supplies net of the lower bounds past the limit `KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE` names,
  where the arcs carry the difference; the network still solves them exactly.
- **After `KAHLNWS_STATUS_INTERNAL_ERROR`**, which only a defect of the library answers, the network may be half
  changed. An arc or a node not below the counts held before is still answered as `KAHLNWS_STATUS_INVALID_ARGUMENT`
  first; beyond that its changes, its solve and its reads of the problem answer `KAHLNWS_STATUS_INTERNAL_ERROR`, its
  reads of the optimum `KAHLNWS_STATUS_NO_OPTIMUM`, its counts those held before, a reservation makes room, a copy is
  as half changed, and `kahlnws_network_load` mends it.

Allocation. Only `kahlnws_network_reserve`, `kahlnws_network_load` and `kahlnws_network_copy` allocate, the load and
the copy not for a problem within the room held; after a reservation, no load, copy, change, solve or read allocates.
A load grows the room before it checks the problem, as a solve does, and a copy before it copies; each answers refused
memory as `KAHLNWS_STATUS_NO_ROOM`, holding what it held. `kahlnws_network_clone` allocates a new network, and answers
null where memory for it is refused; a copy's changes and solves allocate nothing.

What it costs, said plainly. A change costs what it moves: a cost that moves no potential, or a bound that leaves the
flow where it may stay, a few steps; a move of supply the flow held carries, a walk from the two nodes along that flow
to where their routes meet; one it cannot carry, the nodes whose potentials move, which can be most of the network. A
solve that makes no pivot prices only the arcs the changes may have made worth using: none for a move the flow held
carried, the arcs near it for a cost. A solve that pivots prices at least one whole pass over the arcs. And where a
change leaves much flow still to be sent, so that the solve makes many pivots, a network's solve can take longer than a
load of the whole problem as it stands and the solve after it: changes pay where only a few numbers change at a time, a
load where many do. A read of the problem costs a few steps for one arc or node and one step an entry for a column.

## Statuses

Every call that reserves, solves, loads, copies, changes or reads answers a `kahlnws_status`, a `uint32_t` whose
numbers are fixed for good. A solve answers `KAHLNWS_STATUS_OPTIMAL` or `KAHLNWS_STATUS_INFEASIBLE`, every other call
that did what it was asked `KAHLNWS_STATUS_OK`, and the rest say what stopped the call. `kahlnws_status_name` names
each one, and `"unknown status"` any other number.

| Status | Number | Answered by | Meaning |
|---|---|---|---|
| `KAHLNWS_STATUS_OPTIMAL` | 0 | a solve | a flow of least cost is found and written |
| `KAHLNWS_STATUS_INFEASIBLE` | 1 | a solve | no flow meets the supplies; only the work is written |
| `KAHLNWS_STATUS_OK` | 2 | every other call | it did what it was asked |
| `KAHLNWS_STATUS_REFUSED` | 3 | a solver's solve, a load, a change | not admitted; only the refusal is written |
| `KAHLNWS_STATUS_NO_OPTIMUM` | 4 | a read of a network's optimum | none is held; nothing is written |
| `KAHLNWS_STATUS_NULL_POINTER` | 5 | every call | a pointer the call needs is null; nothing is written |
| `KAHLNWS_STATUS_INVALID_ARGUMENT` | 6 | every call | a misaligned pointer, a count or an index too large |
| `KAHLNWS_STATUS_NO_ROOM` | 7 | a reservation, a solver's solve, a load, a copy | the memory for the room was refused |
| `KAHLNWS_STATUS_INTERNAL_ERROR` | 8 | every call | a defect of the library was caught |

A solve is `kahlnws_solver_solve` or `kahlnws_network_solve`, and a read of a network's optimum
`kahlnws_network_total_cost`, `_flow`, `_flows` or `_potentials`; each entry's doc in the header lists what it answers.
A count of entries past `PTRDIFF_MAX` bytes, an arc or a node not below its count, and a node given to
`kahlnws_network_set_supply` as its own balancing node with a supply other than the one it has, are a call written
wrongly, as a misaligned pointer is: each answers `KAHLNWS_STATUS_INVALID_ARGUMENT` and changes nothing.

No entry aborts or unwinds into the caller.

Every entry that answers a status or a handle is declared `KAHLNWS_NODISCARD`, so one whose answer is dropped warns. A
cast to `void` silences that warning under Clang and from C23 on, but not under GCC before C23, whose
`warn_unused_result` no cast silences: there a status deliberately not checked is kept in a variable that is then
cast, `kahlnws_status ignored = kahlnws_network_set_cost(network, arc, cost, NULL, NULL); (void) ignored;`.

## The refusal

A refused problem, or a change a network refused, is described by a `kahlnws_refusal`: its `kind` and the values that
make the breach one. Each kind sets the fields the table names, and every other field is zero. Where a problem has
several breaches, the one of the lowest kind is answered, the kinds about one arc taken arc by arc. A change refused
for the reason a problem would be has the problem's kind, 4, 5 or 6, with the arc changed in `arc`; kinds 9 and 10 only
a change meets.

| Kind | Number | Fields set |
|---|---|---|
| `KAHLNWS_REFUSAL_KIND_TOO_LARGE` | 1 | `node_count`, `arc_count` |
| `KAHLNWS_REFUSAL_KIND_TAIL_OUTSIDE_NODES` | 2 | `arc`, `node` (its tail), `node_count` |
| `KAHLNWS_REFUSAL_KIND_HEAD_OUTSIDE_NODES` | 3 | `arc`, `node` (its head), `node_count` |
| `KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY` | 4 | `arc`, `lower_bound`, `capacity` |
| `KAHLNWS_REFUSAL_KIND_COST_BEYOND_BOUND` | 5 | `arc`, `cost`, `node_count` |
| `KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE` | 6 | `arc` |
| `KAHLNWS_REFUSAL_KIND_UNBALANCED` | 7 | `supply_total` |
| `KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE` | 8 | `net_supply_total` |
| `KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE` | 9 | `excess_total`: the flow the change would leave to be sent |
| `KAHLNWS_REFUSAL_KIND_SUPPLY_BEYOND_RANGE` | 10 | `node`, `supply`: the supply it would have |

Zero names no breach, so a zeroed `kahlnws_refusal` names none. [The refusals](problem-and-guarantees.md#the-refusals)
gives the condition of each.

`kahlnws_refusal_message(refusal, buffer, capacity)` writes the breach as text, the same message every language
gives, in UTF-8 ending in a NUL, and answers the length of the whole message without the NUL. A call with a null
buffer and a capacity of zero answers the length to make room for; a buffer too short receives as much as ends on a
character, and its NUL.

## 128-bit values

The cost of a flow, the sum of unbalanced supplies and a supply past its range are signed 128-bit integers, which C
has no standard type for. They come as a `kahlnws_int128` of two halves, and the value is

```math
v = \mathit{high} \cdot 2^{64} + \mathit{low} ,
```

with `low`, a `uint64_t`, the value modulo $2^{64}$ and `high`, an `int64_t`, the value divided by $2^{64}$ and
rounded down: the low and the high 64 bits of its two's complement. The value fits an `int64_t` exactly where `high` is
0 and `low` is below $2^{63}$, or `high` is -1 and `low` is at least $2^{63}$. Two values compare as their `high`
halves, and where those are equal as their `low` halves, unsigned. On a compiler with a 128-bit integer:

```c
__int128 value = (__int128) (((unsigned __int128) (uint64_t) total_cost.high << 64) | total_cost.low);
```

The positive net supply total and the excess total of a refusal are a `kahlnws_uint128`, both halves unsigned.

`kahlnws_int128_format(value, buffer, capacity)` writes a value in decimal as C has no `printf` conversion for it, and
`kahlnws_uint128_format` an unsigned one, shaped as `kahlnws_refusal_message`: ASCII ending in a NUL, the length of the
whole text answered, and a buffer too short cut, its NUL kept. 41 bytes hold every value.

```c
char cost[41];
kahlnws_int128_format(total_cost, cost, sizeof cost);
printf("cost %s\n", cost);
```

## The work

A `kahlnws_work` holds the three counts of a solve as `uint64_t`: `pivots`, `degenerate_pivots` and
`first_arc_pivots`. It is written, where the pointer is not null, on an optimal and on an infeasible answer, by
`kahlnws_solver_solve` and by `kahlnws_network_solve` alike.
[What an answer holds](problem-and-guarantees.md#what-an-answer-holds) defines each count.

## The version

`KAHLNWS_VERSION_MAJOR`, `KAHLNWS_VERSION_MINOR` and `KAHLNWS_VERSION_PATCH` give the version of the header, and
`KAHLNWS_VERSION` all three as `(major << 16) | (minor << 8) | patch`; `kahlnws_version()` answers the version of the
library linked in the same encoding. From the first release on, a program built against a header works with a library
of the same major version whose minor version is at least the header's, and while the major version is 0, only with a
library of the same minor version. Until the first release the interface may change while the version stays, so a
header and a library must be built from the same sources, which no comparison of versions can check.

## Threads

A solver or a network is used by one thread at a time: no two calls that take the same one run at once. It may move
from one thread to another between calls, and distinct ones may be used on distinct threads at once, also to solve or
load the same problem tables. The entries that take neither (`kahlnws_solver_new`, `kahlnws_network_new`,
`kahlnws_refusal_message`, `kahlnws_int128_format`, `kahlnws_uint128_format`, `kahlnws_status_name`,
`kahlnws_cost_bound`, `kahlnws_version`) may be called from any thread at any time.
