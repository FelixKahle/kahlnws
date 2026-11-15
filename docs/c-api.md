# The C interface

For a C programmer who links kahlnws into a program. The crate `kahlnws_capi` builds the solver as a static and a
dynamic library, and its header `crates/kahlnws_capi/include/kahlnws.h` declares the interface for C99 and later; a
C++ compiler from C++11 on reads it as well. The header documents every declaration; this page shows how the pieces
fit. The contract of a solve is in [the problem and the guarantees](problem-and-guarantees.md).

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
    const kahlnws_problem problem = {3, 3, tails, heads, NULL, capacities, costs, supplies};

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
    case KAHLNWS_STATUS_OPTIMAL:
        /* The cost fits an int64_t here: its high half is 0 and its low half below 2^63. */
        printf("flows %" PRIu64 " %" PRIu64 " %" PRIu64 ", cost %" PRIu64 ", %" PRIu64 " pivots\n", flows[0],
               flows[1], flows[2], total_cost.low, work.pivots);
        break;
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

`crates/kahlnws_capi/examples/` holds five programs, the same five the other guides show in their languages. Each
solves a small transport network, two warehouses that serve three shops, checks every status a call answers and frees
its solver on every path:

| Example | What it shows |
|---|---|
| `transport.c` | the smallest useful call: one solve, its cost and the flow on each lane |
| `reuse.c` | one solver for three days of demands, its room reserved once, each day solved into the same buffer |
| `resolve.c` | the days of `reuse.c`, each after the first resolved from the day before, beside fresh pivots |
| `outcomes.c` | the answers that are not an optimal flow: no flow exists, and a refused problem with its message |
| `potentials.c` | the potentials, each lane's reduced cost, the optimality conditions checked, the solve's work |

From the repository's root, an example builds and runs against the static library as any program does; on Linux the
libraries of [compiling and linking](#compiling-and-linking) follow the library on the line:

```sh
cargo build --release -p kahlnws_capi
cc -std=c99 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
    crates/kahlnws_capi/examples/transport.c target/release/libkahlnws_capi.a -o transport
./transport
```

It prints `cost: 465` and then the flow on each lane, such as `warehouse 0 -> shop 1: 20`. The crate's tests build
every example and check what it prints.

## Ownership and lifetimes

The library owns only the solvers it makes. `kahlnws_solver_new` answers a new solver, or null where the memory for
it is refused; `kahlnws_solver_free` frees it with all its room, and accepts null. Every other pointer belongs to the
caller: a call reads or writes through it while it runs and keeps none of it afterwards.

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
problem too large for memory answers `KAHLNWS_STATUS_NO_ROOM` before it could be refused as too large.

## Solving again

A solver keeps the basis its last solve ended on, optimal or infeasible. `kahlnws_solver_resolve` takes the same
arguments as `kahlnws_solver_solve`, answers the same statuses and writes the same outputs on each, and starts from that
basis where it serves: where the problem has the node and arc counts of that solve and every arc the tail and the head
it had, and the supplies the basis leaves stay within the range a solve admits. The tree is kept and every number is
read anew, so supplies, bounds, capacities and costs may all have changed. Where only some of them did, a resolve often
makes far fewer pivots than a solve, and the same problem again, where the kept basis serves, makes none. Where the
basis cannot serve, as on a new solver or for a problem of other counts, a resolve answers and writes exactly what
`kahlnws_solver_solve` does, its work included; so it is never wrong to call it.

```c
kahlnws_status status = kahlnws_solver_solve(solver, &problem, flows, NULL, &total_cost, &work, NULL);
/* ... the program changes some supplies or costs of the same arcs ... */
status = kahlnws_solver_resolve(solver, &problem, flows, NULL, &total_cost, &work, NULL);
```

What each entry guarantees:

| | `kahlnws_solver_solve` | `kahlnws_solver_resolve` |
|---|---|---|
| status and cost | those of the problem | the same |
| potentials | prove the flows optimal | prove the flows optimal |
| flows among tied optima, potentials, work | a function of the problem | of the problem and the solves before it |
| `first_arc_pivots` | as the problem's structure gives | 0 where it starts from the kept basis |

Each entry keeps the basis an optimal or infeasible answer ended on. A call that answers any other status keeps the
basis kept before, but `KAHLNWS_STATUS_INTERNAL_ERROR`, which may leave none. Both entries grow the room alike, so
after `kahlnws_solver_reserve` a resolve allocates nothing either.

## Statuses

Every call that reserves or solves answers a `kahlnws_status`, a `uint32_t` whose numbers are fixed for good.
`kahlnws_status_name` names each one, and `"unknown status"` any other number.

| Status | Number | Meaning |
|---|---|---|
| `KAHLNWS_STATUS_OPTIMAL` | 0 | the solve found a flow of least cost and wrote it |
| `KAHLNWS_STATUS_INFEASIBLE` | 1 | no flow within the bounds meets the supplies; only the work is written |
| `KAHLNWS_STATUS_RESERVED` | 2 | `kahlnws_solver_reserve` made the room |
| `KAHLNWS_STATUS_REFUSED` | 3 | the problem lies outside the ranges a solve admits; only the refusal is written |
| `KAHLNWS_STATUS_NULL_POINTER` | 4 | a pointer the call needs is null; nothing is written |
| `KAHLNWS_STATUS_INVALID_ARGUMENT` | 5 | a pointer is not aligned for its type, or a count passes `PTRDIFF_MAX` bytes |
| `KAHLNWS_STATUS_NO_ROOM` | 6 | the memory for the solver's room was refused; nothing is written |
| `KAHLNWS_STATUS_INTERNAL_ERROR` | 7 | a defect of the library was caught; the solver may be used again or freed |

No entry aborts or unwinds into the caller.

## The refusal

A refused problem is described by a `kahlnws_refusal`: its `kind` and the values that make the breach one. Each kind
sets the fields the table names, and every other field is zero. Where a problem has several breaches, the one of the
lowest kind is answered, the kinds about one arc taken arc by arc.

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

Zero names no breach, so a zeroed `kahlnws_refusal` names none. [The refusals](problem-and-guarantees.md#the-refusals)
gives the condition of each.

`kahlnws_refusal_message(refusal, buffer, capacity)` writes the breach as text, the same message every language
gives, in UTF-8 ending in a NUL, and answers the length of the whole message without the NUL. A call with a null
buffer and a capacity of zero answers the length to make room for; a buffer too short receives as much as ends on a
character, and its NUL.

## 128-bit values

The cost of a flow and the sum of unbalanced supplies are signed 128-bit integers, which C has no standard type for.
They come as a `kahlnws_int128` of two halves, and the value is

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

The positive net supply total of a refusal is a `kahlnws_uint128`, both halves unsigned.

## The work

A `kahlnws_work` holds the three counts of a solve as `uint64_t`: `pivots`, `degenerate_pivots` and
`first_arc_pivots`. It is written, where the pointer is not null, on an optimal and on an infeasible answer.
[What an answer holds](problem-and-guarantees.md#what-an-answer-holds) defines each count.

## The version

`KAHLNWS_VERSION_MAJOR`, `KAHLNWS_VERSION_MINOR` and `KAHLNWS_VERSION_PATCH` give the version of the header, and
`KAHLNWS_VERSION` all three as `(major << 16) | (minor << 8) | patch`; `kahlnws_version()` answers the version of the
library linked in the same encoding. From the first release on, a program built against a header works with a library
of the same major version whose minor version is at least the header's, and while the major version is 0, only with a
library of the same minor version. Until the first release the interface may change while the version stays, so a
header and a library must be built from the same sources, which no comparison of versions can check.

## Threads

A solver is used by one thread at a time: no two calls that take the same solver run at once. It may move from one
thread to another between calls, and distinct solvers may be used on distinct threads at once, also to solve the same
problem tables. The entries that take no solver (`kahlnws_solver_new`, `kahlnws_refusal_message`,
`kahlnws_status_name`, `kahlnws_cost_bound`, `kahlnws_version`) may be called from any thread at any time.
