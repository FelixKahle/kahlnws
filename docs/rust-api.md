# The Rust interface by task

For a Rust programmer who uses the crate `kahlnws`. Each section answers one task with a complete program. Every
program on this page is compiled and run as a doctest of the crate, so it stays true. The contract behind the calls is
in [the problem and the guarantees](problem-and-guarantees.md).

The public surface is ten items:

| Item | What it is |
|---|---|
| `Problem` | the six borrowed tables of one problem |
| `NetworkSimplex` | the solver, with the room and the basis it keeps from one solve to the next |
| `Answer<T>` | what a solve answers: its `outcome` and its `work` |
| `Outcome<T>` | `Optimal(T)` or `Infeasible` |
| `Solution` | the owned optimum of `solve` and `resolve`: `flows`, `potentials`, `total_cost` |
| `Optimum<'s>` | the optimum of `solve_into` and `resolve_into`: `total_cost`, and `potentials` lent by the solver |
| `Work` | the counts of a solve: `pivots`, `degenerate_pivots`, `first_arc_pivots` |
| `ProblemError` | why a problem is refused, with the values of the breach |
| `Table` | which per-arc table a `ProblemError::Length` names |
| `cost_bound` | the largest magnitude a cost may have for a node count |

## Solve one problem

`solve` answers the flows, the potentials and the cost, owned by a `Solution`. The one `?` handles a refusal; the
`match` handles the two outcomes.

```rust
use kahlnws::{NetworkSimplex, Outcome, Problem, ProblemError};

fn main() -> Result<(), ProblemError> {
    // A depot (node 0) sends 4 units to a customer (node 2), directly or through a hub (node 1).
    let problem = Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    };

    match NetworkSimplex::new().solve(&problem)?.outcome {
        Outcome::Optimal(solution) => {
            assert_eq!(solution.flows, [2, 2, 2]);
            assert_eq!(solution.potentials, [0, 1, 5]);
            assert_eq!(solution.total_cost, 14);
        }
        Outcome::Infeasible => unreachable!("the two routes carry the 4 units"),
    }

    Ok(())
}
```

Where only the optimum matters, `Outcome::optimal` turns the outcome into an `Option`.

## Solve in a loop without allocating

A solver keeps its room from one solve to the next, and the room only grows. `try_reserve(node_count, arc_count)`
makes the room for problems of up to those counts at once; afterwards `solve_into` of such a problem allocates
nothing. `solve_into` writes the flows into a buffer the caller holds, one entry per arc, and lends the potentials
from the solver until its next use.

```rust
use kahlnws::{NetworkSimplex, Outcome, Problem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (tails, heads) = ([0, 0, 1], [2, 1, 2]);
    let (capacities, costs) = ([3, 4, 2], [5, 1, 1]);

    let mut solver = NetworkSimplex::new();
    solver.try_reserve(3, 3)?;
    let mut flows = vec![0; 3];

    // The customer asks for 1 to 5 units; only the supplies change from one solve to the next.
    for demand in 1..=5 {
        let supplies = [demand, 0, -demand];
        let problem = Problem {
            tails: &tails,
            heads: &heads,
            lower_bounds: None,
            capacities: &capacities,
            costs: &costs,
            supplies: &supplies,
        };

        let answer = solver.solve_into(&problem, &mut flows)?; // allocates nothing
        let Outcome::Optimal(optimum) = answer.outcome else {
            unreachable!("the two routes carry up to 5 units");
        };
        println!("{demand} units: flows {flows:?}, cost {}", optimum.total_cost);
    }

    Ok(())
}
```

`solve_into` panics if the buffer holds another number of entries than the problem has arcs; the buffer is compared
first, whether or not the problem would be refused. Where the outcome is infeasible, or the problem is refused, the
buffer is as it was.

## Solve again from the last basis

A solver keeps the basis its last solve ended on, optimal or infeasible. `resolve` and `resolve_into` take the
arguments of `solve` and `solve_into`, answer the same types and refuse the same problems, and start from that basis
where it can serve the problem, as [solving again](problem-and-guarantees.md#solving-again) states. Every number is
read anew, so supplies, bounds, capacities and costs may all differ. Where only some of them did, a resolve often
makes far fewer pivots than a solve; the same problem again, where the basis serves, makes none. Where it cannot
serve, as on a new solver or for a problem of other counts, a resolve is a solve, its answer and its work included, so
it is never wrong to call one.

```rust
use kahlnws::{NetworkSimplex, Outcome, Problem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Two warehouses (nodes 0 and 1) serve three shops (nodes 2 to 4); every lane carries up to 20 units.
    let (tails, heads) = ([0, 0, 0, 1, 1, 1], [2, 3, 4, 2, 3, 4]);
    let (capacities, costs) = ([20; 6], [8, 6, 10, 9, 12, 13]);
    let days = [[20, 30, -10, -25, -15], [20, 30, -20, -20, -10], [20, 30, -5, -15, -30]];

    let mut solver = NetworkSimplex::new();
    solver.try_reserve(5, 6)?;
    let mut flows = [0; 6];

    let mut answered = Vec::new();
    for (day, supplies) in days.iter().enumerate() {
        let problem = Problem {
            tails: &tails,
            heads: &heads,
            lower_bounds: None,
            capacities: &capacities,
            costs: &costs,
            supplies,
        };

        // The first day is solved; each later day starts from the basis the day before ended on.
        let answer = if day == 0 {
            solver.solve_into(&problem, &mut flows)?
        } else {
            solver.resolve_into(&problem, &mut flows)? // allocates nothing
        };
        let Outcome::Optimal(optimum) = answer.outcome else {
            unreachable!("the warehouses cover every day's demand");
        };
        let fresh = NetworkSimplex::new().solve(&problem)?.work.pivots;
        answered.push((optimum.total_cost, answer.work.pivots, fresh));
    }

    // Each day's cost, the pivots of this solver, and those of a fresh solve of the same day.
    assert_eq!(answered, [(465, 6, 6), (430, 1, 5), (525, 1, 7)]);

    Ok(())
}
```

The outcome and the cost of a resolve are those of the problem, and its potentials prove its flows optimal, as a
solve's do. Which optimal flow it answers among several, its potentials and its work depend on the solves before it
as well: they are a function of the problem and of those solves, the same every time. A resolve that starts from the
kept basis tries no arcs before block search, so its `first_arc_pivots` is zero. A refused call, or one that panics on
its buffer, keeps the basis kept before it; a solve that panics after it began to write the basis leaves none, and the
next resolve is then a solve.

`try_reserve` also makes the room a start from a kept basis reads, which a solve does not make: after it,
`resolve_into` of a problem within the reserved counts allocates nothing. Without a reservation, a resolve that
started from a kept basis makes that room, and a later `resolve_into` of a problem no larger allocates nothing.

## Check an optimal answer

An optimal answer carries its own proof: the potentials. A caller can check it without trusting the solver. The
check below works in `i128`, where none of its sums can overflow.

```rust
use kahlnws::{NetworkSimplex, Problem, Solution};

/// Whether `solution` is a flow of `problem` within its bounds that meets every supply, priced optimal by its
/// potentials, and of the cost it states.
fn certified(problem: &Problem<'_>, solution: &Solution) -> bool {
    let mut excess: Vec<i128> = problem.supplies.iter().map(|&supply| i128::from(supply)).collect();
    let mut cost = 0_i128;

    for arc in 0..problem.tails.len() {
        let (tail, head) = (problem.tails[arc], problem.heads[arc]);
        let lower = problem.lower_bounds.map_or(0, |lower_bounds| lower_bounds[arc]);
        let (capacity, flow) = (problem.capacities[arc], solution.flows[arc]);
        let reduced = i128::from(problem.costs[arc]) + i128::from(solution.potentials[tail])
            - i128::from(solution.potentials[head]);

        let within = lower <= flow && flow <= capacity;
        let priced = (flow == capacity || reduced >= 0) && (flow == lower || reduced <= 0);
        if !within || !priced {
            return false;
        }

        excess[tail] -= i128::from(flow);
        excess[head] += i128::from(flow);
        cost += i128::from(problem.costs[arc]) * i128::from(flow);
    }

    excess.iter().all(|&left| left == 0) && cost == solution.total_cost
}

fn main() {
    let problem = Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    };

    let answer = NetworkSimplex::new().solve(&problem).expect("the depot is admitted");
    let solution = answer.outcome.optimal().expect("the depot has a flow");
    assert!(certified(&problem, &solution));
}
```

The reduced cost of arc 2, from the hub to the customer, is `1 + 1 − 5 = −3`: negative, so it must carry its
capacity, which it does. Why the conditions prove the flow optimal is shown in
[the certificate](problem-and-guarantees.md#potentials-and-the-certificate).

## Handle an infeasible problem

A problem whose numbers lie in range but for which no flow exists is answered, not refused: its outcome is
`Outcome::Infeasible`, and its work is counted as for an optimal one.

```rust
use kahlnws::{NetworkSimplex, Outcome, Problem, ProblemError};

fn main() -> Result<(), ProblemError> {
    // The depot's three arcs narrowed to one unit each: at most 2 of the 4 units reach the customer.
    let problem = Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[1, 1, 1],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    };

    let mut solver = NetworkSimplex::new();
    let mut flows = [7; 3];
    let answer = solver.solve_into(&problem, &mut flows)?;
    assert_eq!(answer.outcome, Outcome::Infeasible);
    assert_eq!(flows, [7, 7, 7], "an infeasible solve writes no flow");
    assert_eq!(answer.work.pivots, 3);

    Ok(())
}
```

## Handle a refusal

A refused problem answers `Err(ProblemError)`. Each variant names one breach and holds its values, and its `Display`
gives the same as a message. Where a problem has several breaches, the first in the order the variants are declared
is answered, the breaches of single arcs taken arc by arc.

```rust
use kahlnws::{NetworkSimplex, Problem, ProblemError, Table};

fn main() {
    let mut solver = NetworkSimplex::new();
    let problem = Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -3],
    };

    // Two breaches: the heads are one short, and the supplies add up to 1. The length is looked for first.
    let error = solver.solve(&problem).expect_err("the problem has two breaches");
    assert_eq!(error, ProblemError::Length { table: Table::Heads, length: 2, expected: 3 });
    assert_eq!(error.to_string(), "the heads have length 2 where the tails have length 3");

    match solver.solve(&Problem { heads: &[2, 1, 2], ..problem }) {
        Err(ProblemError::Unbalanced { total }) => assert_eq!(total, 1),
        other => panic!("the supplies should not balance: {other:?}"),
    }
}
```

| Variant | Refused where |
|---|---|
| `Length { table, length, expected }` | a per-arc table holds another number of entries than the tails |
| `TooLarge { node_count, arc_count }` | `node_count + 1` or `arc_count + node_count` passes `u32::MAX` |
| `TailOutsideNodes { arc, tail, node_count }` | an arc leaves a node the problem does not have |
| `HeadOutsideNodes { arc, head, node_count }` | an arc enters a node the problem does not have |
| `LowerAboveCapacity { arc, lower_bound, capacity }` | an arc's lower bound passes its capacity |
| `CostBeyondBound { arc, cost, node_count }` | an arc's cost lies beyond plus or minus `cost_bound(node_count)` |
| `CostTotalBeyondRange { arc }` | the costs' magnitudes times the capacities, added in arc order, pass `i128::MAX` |
| `Unbalanced { total }` | the supplies do not add up to zero |
| `NetSupplyBeyondRange { total }` | the positive supplies net of the lower bounds add up past `2^64 − 2` |

[The ranges a solve admits](problem-and-guarantees.md#the-ranges-a-solve-admits) says why each range is there.

## Read the work

`Answer::work` counts the pivots of the solve, on an optimal and on an infeasible outcome alike. The number of block
searches the solve ran is not stored, since the counts imply it: `pivots - first_arc_pivots + 1`.

```rust
use kahlnws::{NetworkSimplex, Problem, ProblemError, Work};

fn main() -> Result<(), ProblemError> {
    let problem = Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    };

    let work = NetworkSimplex::new().solve(&problem)?.work;
    assert_eq!(work, Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 0 });

    let sent_flow = work.pivots - work.degenerate_pivots;
    let searches = work.pivots - work.first_arc_pivots + 1;
    assert_eq!((sent_flow, searches), (3, 5));

    Ok(())
}
```

[What an answer holds](problem-and-guarantees.md#what-an-answer-holds) defines each count.

## Give lower bounds

`lower_bounds` is `None` where every arc's lower bound is zero, and otherwise one entry per arc. The flows answered
include the lower bounds, and so does the cost.

```rust
use kahlnws::{NetworkSimplex, Problem, ProblemError};

fn main() -> Result<(), ProblemError> {
    // The depot's direct arc must carry at least 3 units, its capacity: the fourth unit goes through the hub.
    let problem = Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: Some(&[3, 0, 0]),
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    };

    let solution = NetworkSimplex::new().solve(&problem)?.outcome.optimal().expect("the bounds leave a flow");
    assert_eq!(solution.flows, [3, 1, 1]);
    assert_eq!(solution.total_cost, 17);

    Ok(())
}
```

## Choose costs within the bound

A cost may have a magnitude of at most `cost_bound(node_count)`, which is `⌊(2^62 − 1) / node_count⌋`, and
`2^62 − 1` for a problem of no node. Costs that come from fractional prices are scaled to integers before the solve;
the bound says how far they may be scaled.

```rust
use kahlnws::{NetworkSimplex, Problem, ProblemError, cost_bound};

fn main() {
    assert_eq!(cost_bound(3), 1_537_228_672_809_129_301);
    assert_eq!(cost_bound(1_000_000), 4_611_686_018_427);

    let too_dear = cost_bound(3) + 1;
    let problem = Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, too_dear],
        supplies: &[4, 0, -4],
    };

    let refusal = NetworkSimplex::new().solve(&problem).map(|_| ());
    assert_eq!(refusal, Err(ProblemError::CostBeyondBound { arc: 2, cost: too_dear, node_count: 3 }));
}
```

## Solve on several threads

The crate starts no thread and holds no global state. A solve takes its solver by `&mut`, so one solver serves one
solve at a time; distinct solvers may solve at once on distinct threads, also problems that borrow the same tables.

```rust
use std::thread;

use kahlnws::{NetworkSimplex, Problem};

fn main() {
    let (tails, heads, capacities, costs) = ([0, 0, 1], [2, 1, 2], [3, 4, 2], [5, 1, 1]);
    let demands = [[2, 0, -2], [4, 0, -4]];

    let costs_found: Vec<i128> = thread::scope(|scope| {
        let solves: Vec<_> = demands
            .iter()
            .map(|supplies| {
                scope.spawn(move || {
                    let problem = Problem {
                        tails: &tails,
                        heads: &heads,
                        lower_bounds: None,
                        capacities: &capacities,
                        costs: &costs,
                        supplies,
                    };
                    let answer = NetworkSimplex::new().solve(&problem).expect("the problem is admitted");
                    answer.outcome.optimal().expect("the problem has a flow").total_cost
                })
            })
            .collect();

        solves.into_iter().map(|solve| solve.join().expect("the solve does not panic")).collect()
    });

    assert_eq!(costs_found, [4, 14]);
}
```

## Examples

Five programs under `crates/kahlnws/examples/` solve one network, two warehouses that supply three shops, as a user
would. The examples of the other languages carry the same names and print the same lines. Each program asserts its
optimum, worked out by hand, and `cargo test` runs each as a test.

| Example | What it shows |
|---|---|
| `transport` | the smallest useful call: one solve, its cost and the flow on each lane |
| `reuse` | one solver for three days of demand, its room reserved once, each day solved into the same buffer |
| `resolve` | the days of `reuse`, each after the first resolved from the day before, beside a fresh solve's pivots |
| `outcomes` | a problem with no flow, answered as an outcome, and a refused problem, answered as an error |
| `potentials` | the potentials, each lane's reduced cost, the check of the optimality conditions, and the work |

From the root of the repository:

```sh
cargo run -p kahlnws --example transport
```
