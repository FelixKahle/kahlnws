<h1 align="center">
  <img alt="kahlnws" src="docs/img/kahlnws-light.svg#gh-light-mode-only" width="880">
  <img alt="kahlnws" src="docs/img/kahlnws-dark.svg#gh-dark-mode-only" width="880">
</h1>

kahlnws is a primal network simplex for the min-cost flow problem on exact integers, written in Rust. A problem is a
set of nodes, each with a supply, and arcs between them, each with a lower bound, a capacity and a cost per unit of
flow; a solve finds a flow of least cost that meets every supply within the bounds, or answers that no such flow
exists. It is offered to Rust, C, C++ and Python as two types, one contract each:

- **The solver** is a function: a problem in, an answer out. It keeps its room and nothing else, so its answer is a
  function of the problem alone. Reach for it to solve one problem.
- **A network** is a state: it holds one problem and a basis of it. Change a number, set a node's supply or move
  supply between two nodes, load new numbers on the same arcs, and solve it from the basis it holds; read the problem
  back; copy it. Reach for it to solve a problem each time it changes, as a search or a plan from day to day does.

## What it guarantees

- **Exact integer arithmetic.** Every number of a problem is checked before a solve starts. Within the ranges a solve
  admits, every sum it forms fits the integer type it is held in, and the cost of the flow is answered in 128 bits. A
  problem outside those ranges is refused with the breach named and its values.
- **An optimum that proves itself.** An optimal solve finds the flow together with a potential per node that
  certifies it optimal; any caller can check the certificate. Where no flow meets the supplies, the solve says so,
  without a certificate.
- **The work counted.** Every solve, optimal or infeasible, answers how many pivots it made, how many of them sent no
  flow, and how many were made by the arcs tried before block search.
- **No allocation within reserved room.** The solver and a network keep their room from one call to the next. Once
  room is reserved, a solve into the caller's buffers allocates nothing in Rust, C and C++, nor does a network's load,
  change or solve; in Python a call makes only its answer and a view of each buffer.
- **Work that follows the change.** A network's change costs what it reaches in the network, and its solve prices only
  the arcs the changes may have made violate, until it pivots. After a load of new numbers on the same arcs, its solve
  often makes far fewer pivots than one from nothing kept. Either way a network answers the outcome and the cost the
  solver answers for the problem as it stands, with potentials that prove it.
- **One small interface in four languages.** Rust, C, C++ and Python share the same problem, answer, work and
  refusals.

## The problem

```math
\min \sum_{(i,j) \in A} c_{ij} x_{ij}
\quad \text{subject to} \quad
\sum_{(i,j) \in A} x_{ij} - \sum_{(j,i) \in A} x_{ji} = b_i \;\; \text{for } i \in N,
\qquad
l_{ij} \le x_{ij} \le u_{ij} \;\; \text{for } (i,j) \in A
```

Here $N$ is the set of nodes and $A$ the set of arcs. The costs $c$ are `costs`, the supplies $b$ are `supplies`
(positive where a node sends flow, negative where it takes flow), the bounds $l$ and $u$ are `lower_bounds` and
`capacities`, and the answer $x$ is `flows`.

## Rust

A depot (node 0) sends 4 units to a customer (node 2), directly or through a hub (node 1):

```rust
use kahlnws::{Answer, NetworkSimplex, Outcome, Problem, ProblemError};

fn main() -> Result<(), ProblemError> {
    let problem = Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    };

    let mut solver = NetworkSimplex::new();
    let Answer { outcome, work } = solver.solve(&problem)?;
    match outcome {
        Outcome::Optimal(solution) => {
            assert_eq!(solution.flows, [2, 2, 2]);
            assert_eq!(solution.total_cost, 14);
        }
        Outcome::Infeasible => unreachable!("the depot's arcs carry the customer's demand"),
    }
    println!("{} pivots, {} of them degenerate", work.pivots, work.degenerate_pivots);

    Ok(())
}
```

A caller who solves in a loop reserves the solver's room once and has the flows written into a buffer of its own; a
solve of a problem within the room allocates nothing:

```rust
use kahlnws::{NetworkSimplex, Outcome, Problem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let problem = Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    };

    let mut solver = NetworkSimplex::new();
    solver.try_reserve(problem.supplies.len(), problem.tails.len())?;
    let mut flows = vec![0; problem.tails.len()];

    let outcome = solver.solve_into(&problem, &mut flows)?.outcome; // allocates nothing
    if let Outcome::Optimal(optimum) = outcome {
        assert_eq!(flows, [2, 2, 2]);
        assert_eq!(optimum.total_cost, 14);
        assert_eq!(optimum.potentials, [0, 1, 5]);
    }

    Ok(())
}
```

A network holds the problem: a load of new numbers on the same arcs starts the next solve from the basis the network
holds where that basis serves, and a setter changes one number at a time, answering the number it replaced so that a
search can set it back. Its solve answers the cost and the work; `optimum()` then lends the flows and the potentials:

```rust
use kahlnws::{Network, Problem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let problem = Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    };

    let mut network = Network::new();
    network.load(&problem)?;
    assert_eq!(network.solve().work.pivots, 4);

    // One unit less, loaded from the optimum the network holds: no pivot.
    network.load(&Problem { supplies: &[3, 0, -3], ..problem })?;
    let answer = network.solve();
    assert_eq!((answer.outcome.optimal(), answer.work.pivots), (Some(9), 0));

    // The leg from the hub costs 3, then is set back.
    let had = network.set_cost(2, 3)?;
    assert_eq!(network.solve().outcome.optimal(), Some(13));
    network.set_cost(2, had)?;

    Ok(())
}
```

The tasks of the Rust API, one by one, are in [the Rust guide](docs/rust-api.md).

## C

The crate `kahlnws_capi` builds a static and a dynamic library with the header `kahlnws.h`, for C99 and later. Every
call that reserves, loads, changes, reads or solves answers a status: a solve `KAHLNWS_STATUS_OPTIMAL` or
`KAHLNWS_STATUS_INFEASIBLE`, every other call that did what it was asked `KAHLNWS_STATUS_OK`, and the rest say what
stopped the call. No call aborts or unwinds into the caller. The heart of a program, as a fragment:

```c
const kahlnws_problem problem = {.node_count = 3,
                                 .arc_count = 3,
                                 .tails = tails,
                                 .heads = heads,
                                 .capacities = capacities,
                                 .costs = costs,
                                 .supplies = supplies};
kahlnws_solver *solver = kahlnws_solver_new();
kahlnws_status status = kahlnws_solver_solve(solver, &problem, flows, potentials, &total_cost, &work, &refusal);
/* KAHLNWS_STATUS_OPTIMAL: flows, potentials, total_cost and work are written */
kahlnws_solver_free(solver);
```

Building, linking and a complete program are in [the C guide](docs/c-api.md).

## C++

`crates/kahlnws_capi/include/kahlnws.hpp` is a header-only C++11 interface over the C one, linked against the same
library and named as the standard library names things. Errors come as values, and as exceptions where exceptions
are enabled. As a fragment:

```cpp
kahlnws::problem problem;
problem.tails = tails;
problem.heads = heads;
problem.capacities = capacities;
problem.costs = costs;
problem.supplies = supplies;

kahlnws::network_simplex solver;
const kahlnws::result<kahlnws::answer<kahlnws::solution>> solved = solver.try_solve(problem);
if (solved && solved->outcome.is_optimal()) {
    const kahlnws::solution& solution = *solved->outcome.optimal();  // flows, potentials, total_cost
}
```

Both levels, the errors and a complete program are in [the C++ guide](docs/cpp-api.md).

## Python

The crate `kahlnws_python` builds the package `kahlnws` for CPython 3.11 and later: one wheel, fully typed, with no
dependency. Columns are sequences of `int` or buffers such as NumPy arrays.

```python
import kahlnws

solver = kahlnws.NetworkSimplex()
answer = solver.solve(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4])
assert answer.outcome is not None
assert answer.outcome.flows == [2, 2, 2] and answer.outcome.total_cost == 14
```

Installing, buffers, NumPy and the exceptions are in [the Python guide](docs/python-api.md).

## Examples

Six programs in each language show the interface at work on one small transport network, and print the same lines
in every language:

| Example | Shows |
|---|---|
| `transport` | the smallest call: a problem in, the optimal cost and the flow of each lane out |
| `reuse` | one solver kept, its room reserved once, solving day after day into the caller's buffers |
| `network` | the days of `reuse` held by a network, each later day loaded from the basis the day before ended on |
| `search` | a search on the first day held: five moves, one refused at its second change, each kept or set back |
| `outcomes` | the answers that are no optimal flow: a problem without a flow, and a problem that is refused |
| `potentials` | the potentials, the reduced costs they give, the optimality conditions, and the work counted |

They are in [`crates/kahlnws/examples/`](crates/kahlnws/examples) for Rust,
[`crates/kahlnws_capi/examples/`](crates/kahlnws_capi/examples) for C and C++ and
[`crates/kahlnws_python/examples/`](crates/kahlnws_python/examples) for Python; each guide says how to run them, and
the tests run every one.

## Documentation

- [The problem and the guarantees](docs/problem-and-guarantees.md): the linear program, feasibility, the certificate,
  the ranges and the refusals, the answer and the work, and what the solver and a network each guarantee.
- [How it works](docs/how-it-works.md): the algorithm, a network's load from the basis it holds and its changes, and
  the layout of its data.
- The guides for [Rust](docs/rust-api.md), [C](docs/c-api.md), [C++](docs/cpp-api.md) and
  [Python](docs/python-api.md).
- [Benchmarks](docs/benchmarks.md): the three suites, of the solver's solves, of a network's loads from the basis it
  holds and of a network's changes, how to run them, and a comparison with two LP solvers.
- [Building, testing and safety](docs/building-and-testing.md): the gates, the tests, the `unsafe` code and Miri.
- [The rules of the code](docs/style.md): what every change to the code holds to.
- [Releasing](docs/releasing.md): what a release publishes and how one is made.

[The index of the documents](docs/README.md) says who each page is for.

## Building and testing

The workspace builds with a current stable Rust toolchain and a Python interpreter of at least 3.11 on the path; its
tests also need a C and a C++ compiler and cmake.
[What a build needs](docs/building-and-testing.md#what-a-build-needs) says what each one serves. From the root of the
repository:

```sh
cargo build --release
./gates.sh
```

`./gates.sh` runs the format check, the lints, the tests and doctests of every crate of the workspace, the
documentation build and a smoke run of the benchmarks. The Python package's own checks run apart, with
`crates/kahlnws_python/check.sh`. [Building, testing and safety](docs/building-and-testing.md) has the details, and
[`CONTRIBUTING.md`](CONTRIBUTING.md) the rules for changes.

## Licence

Licensed under either of the Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE)) or the MIT License
([`LICENSE-MIT`](LICENSE-MIT)), at your option.
