# The C++ interface

For a C++ programmer who uses kahlnws. `crates/kahlnws_capi/include/kahlnws.hpp` is a header-only interface for C++11
and later over [the C interface](c-api.md): it includes `kahlnws.h`, and a program that includes it links the same
library, static or dynamic, as a C program does. It offers both levels of a solve, one whose answer owns its flows and
potentials and one that writes into buffers the caller holds. Errors come as values, and as exceptions where
exceptions are enabled. The contract of a solve is in [the problem and the guarantees](problem-and-guarantees.md).

The header documents every name in Doxygen form; this page is the tour.

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
| `span<T>` | A view of a run of `T` that owns nothing: a pointer and a count |
| `network_simplex` | The solver with the room it works in; movable, not copyable |
| `answer<T>` | What a solve answers: its `outcome` and the `work` it did |
| `outcome<T>` | Optimal with a `T`, or infeasible |
| `solution` | The flows, the potentials and the cost of an optimal flow, owned |
| `optimum` | The cost of an optimal flow whose columns went to the caller's buffers |
| `work` | The pivots of a solve, counted |
| `int128`, `uint128` | 128-bit values as two halves, to carry and to print |
| `result<T>` | A value, or the `error` that stood in its way |
| `error` | Why a call did not answer: its kind, its message and, for a refused problem, the `refusal` |

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

## Errors as values

Every call that can fail has a form whose name begins with `try_` and that never throws and is present in every build:
`try_reserve`, `try_solve`, `try_solve_into` and `try_cost_bound`. Each answers a `result`, which converts to `true`
where it holds its value. `error()` says why it does not:

| `error_kind` | When |
|---|---|
| `refused_problem` | The problem lies outside what a solve admits; `refusal()` holds the breach |
| `invalid_argument` | A buffer has the wrong length or shares storage with a table or another buffer |
| `refused_memory` | The memory for the solver's room was refused |
| `internal_error` | The library failed; the solver may be used again |

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
`cost_bound`. Each calls its `try_` twin and throws what that answered:

| Error | Exception |
|---|---|
| A refused problem | `kahlnws::problem_error`, a `std::invalid_argument` that holds the `refusal` |
| An argument the call does not admit | `std::invalid_argument` |
| Refused memory | `std::bad_alloc` |
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
halves, compare, hash, and print in decimal with `to_string` or a stream; they are not arithmetic types. Where the
compiler has a native 128-bit integer, `to_native()` converts to it.

```cpp
const std::string cost = kahlnws::to_string(solution->total_cost);
```

## Moving a solver

A `network_simplex` is movable and not copyable. A solver that was moved from is a solver with no room, the same as
a new one, and may be used again. A new solver holds no memory; its room is made by a reservation or by its first
solve.

## Threads

One thread at a time uses a solver. A solver may move from one thread to another between calls, and distinct
solvers may be used on distinct threads at once.

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

`crates/kahlnws_capi/examples/` holds four programs, the same four the other guides show in their languages, each
beside its C twin. Each solves a small transport network, two warehouses that serve three shops, with the `try_` forms,
so that it builds with exceptions enabled or not:

| Example | What it shows |
|---|---|
| `transport.cpp` | the smallest useful call: one owned answer, its cost and the flow on each lane |
| `reuse.cpp` | one solver for three days of demands, its room reserved once, each day solved into the same buffer |
| `outcomes.cpp` | an infeasible outcome, a refused problem as an error value and, with exceptions, as thrown |
| `potentials.cpp` | the potentials, each lane's reduced cost, the optimality conditions checked, the solve's work |

From the repository's root, an example builds and runs against the static library as any program does; on Linux the
libraries [the C guide](c-api.md#compiling-and-linking) names follow the library on the line:

```sh
cargo build --release -p kahlnws_capi
c++ -std=c++11 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
    crates/kahlnws_capi/examples/transport.cpp target/release/libkahlnws_capi.a -o transport
./transport
```

It prints `cost: 465` and then the flow on each lane, such as `warehouse 0 -> shop 1: 20`; built with
`-fno-exceptions` it prints the same. The crate's tests build every example in each mode and check that it prints what
its C twin prints.
