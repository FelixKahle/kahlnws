# The problem and the guarantees

For anyone who calls kahlnws, from any of its languages: what problem a solve takes, what it answers, and what it
promises about both. The names in `code` are the Rust ones; [the names in each language](#names-in-each-language)
are listed at the end. The C++ interface states the same contract in its own terms, in [its guide](cpp-api.md).

## The min-cost flow problem

A problem has $n$ nodes, numbered $0$ to $n - 1$, and $m$ arcs, numbered $0$ to $m - 1$. Arc $(i, j)$ leaves its tail
$i$ and enters its head $j$; it has a lower bound $l_{ij}$, a capacity $u_{ij}$ and a cost $c_{ij}$ per unit of flow.
Node $i$ has a supply $b_i$: positive where the node sends flow, negative where it takes flow, zero where it only
passes flow on. A flow $x$ assigns each arc an amount, and the problem is the linear program

```math
\begin{aligned}
\text{minimise} \quad & \sum_{(i,j) \in A} c_{ij} \, x_{ij} \\
\text{subject to} \quad & \sum_{(i,j) \in A} x_{ij} - \sum_{(j,i) \in A} x_{ji} = b_i
  && \text{for every node } i, \\
& l_{ij} \le x_{ij} \le u_{ij} && \text{for every arc } (i,j) \in A.
\end{aligned}
```

The costs $c$ are the column `costs`, the bounds $l$ and $u$ are `lower_bounds` and `capacities`, the supplies $b$
are `supplies`, and the answer $x$ is `flows`; the first sum runs over the arcs that leave node $i$, the second over
those that enter it.

The problem is given as columns, one table per quantity: arc $a$ is entry $a$ of `tails`, `heads`, `lower_bounds`,
`capacities` and `costs`, and node $v$ is entry $v$ of `supplies`. The index is the identity: nodes are never
renumbered, and every answer is in the same order. `lower_bounds` may be left out, which means zero on every arc.

| Column | Entries | Rust | C | Python |
|---|---|---|---|---|
| `tails` | one per arc | `&[usize]` | `const size_t *` | `int` each |
| `heads` | one per arc | `&[usize]` | `const size_t *` | `int` each |
| `lower_bounds` | one per arc, or none | `Option<&[u64]>` | `const uint64_t *`, or null | `int` each, or `None` |
| `capacities` | one per arc | `&[u64]` | `const uint64_t *` | `int` each |
| `costs` | one per arc | `&[i64]` | `const int64_t *` | `int` each |
| `supplies` | one per node | `&[i64]` | `const int64_t *` | `int` each |
| `flows` (answer) | one per arc | `u64` each | `uint64_t` each | `int` each |
| `potentials` (answer) | one per node | `i64` each | `int64_t` each | `int` each |
| `total_cost` (answer) | one | `i128` | `kahlnws_int128` | `int` |

Every number is an integer, and so is every flow answered. Since the data are integers, an optimal flow in integers
exists wherever an optimal flow exists at all, and the solve finds one.

### Parallel arcs and loops

An arc is an entry of the tables, not a pair of nodes: two arcs with the same tail and head are two variables of the
program, each with its own bounds and cost. A loop, an arc whose tail is its head, is allowed too. It appears on both
sides of its node's equation, so it changes no balance, and its flow is decided by its cost alone: an optimal flow
fills a loop of negative cost to its capacity and leaves a loop of positive cost at its lower bound.

## Feasibility

The supplies must add up to zero, since every unit sent is taken somewhere:

```math
\sum_{i} b_i = 0 .
```

A problem whose `supplies` do not is refused as unbalanced, before any solve. A balanced problem may still have no
flow: the capacities may be too small to carry the supplies where they are wanted, or the lower bounds may force more
flow into a node than it can pass on. Such a problem is answered as infeasible, which is an outcome and not a refusal.
No flow exists exactly where some set $S$ of nodes has more supply than the arcs leaving it can carry away, once the
arcs entering it bring in their lower bounds:

```math
\sum_{i \in S} b_i \;>\; \sum_{\substack{(i,j) \in A \\ i \in S,\; j \notin S}} u_{ij}
  \;-\; \sum_{\substack{(j,i) \in A \\ j \notin S,\; i \in S}} l_{ji} .
```

Here $b$ is `supplies`, $u$ is `capacities` and $l$ is `lower_bounds`. The solve does not search for such a set; it
decides infeasibility from where its own flow ends, and the decision is exact.

## Potentials and the certificate

Beside the flow, an optimal solve answers a potential $\pi_i$ for each node. With them, the reduced cost of an arc is

```math
\bar c_{ij} = c_{ij} + \pi_i - \pi_j ,
```

the arc's `costs` entry plus the `potentials` entry of its tail less that of its head. The answer meets the
optimality conditions

```math
x_{ij} < u_{ij} \implies \bar c_{ij} \ge 0 ,
\qquad
x_{ij} > l_{ij} \implies \bar c_{ij} \le 0 ,
```

so no arc with room left would lower the cost by carrying more, and no arc above its lower bound would lower it by
carrying less. An arc whose lower bound equals its capacity meets both conditions whatever its reduced cost.

These conditions prove the flow optimal. For any flow $y$ that meets the supplies,

```math
\sum_{(i,j) \in A} c_{ij} \, y_{ij} = \sum_{(i,j) \in A} \bar c_{ij} \, y_{ij} - \sum_{i} \pi_i \, b_i .
```

The last sum is the same for every such flow. The first is smallest where every arc of positive reduced cost carries
its lower bound and every arc of negative reduced cost its capacity, which is what the conditions say of $x$. So no
flow costs less than $x$. In the usual sign convention of linear programming, $-\pi_i$ is the dual value of node
$i$'s equation.

A caller can check an optimal answer without trusting the solver, in four steps over the arcs and nodes, in an integer
type wide enough for the sums (128 bits, or Python's `int`):

1. every flow lies within its bounds: $l_{ij} \le x_{ij} \le u_{ij}$;
2. every node meets its supply: the flow out less the flow in is $b_i$;
3. every arc meets the optimality conditions above;
4. the cost $\sum c_{ij} x_{ij}$ is `total_cost`.

```python
def certified(tails, heads, lower_bounds, capacities, costs, supplies, flows, potentials, total_cost):
    excess = list(supplies)
    for tail, head, lower, capacity, cost, flow in zip(tails, heads, lower_bounds, capacities, costs, flows):
        reduced = cost + potentials[tail] - potentials[head]
        if not lower <= flow <= capacity:
            return False
        if (flow < capacity and reduced < 0) or (flow > lower and reduced > 0):
            return False
        excess[tail] -= flow
        excess[head] += flow
    return all(left == 0 for left in excess) and total_cost == sum(c * x for c, x in zip(costs, flows))


assert certified([0, 0, 1], [2, 1, 2], [0, 0, 0], [3, 4, 2], [5, 1, 1], [4, 0, -4], [2, 2, 2], [0, 1, 5], 14)
```

The potentials are one certificate among many: any potentials that meet the conditions prove the same flow optimal,
and where several flows are optimal, the solve answers one of them.

## The ranges a solve admits

Before a solve starts, every number of the problem is checked against six ranges. Within them, every number the solve
forms fits the integer type it is held in, so the solve is exact without a check inside its loops; outside them, the
problem is refused. With $n$ nodes, $m$ arcs and $C$ the largest magnitude of a cost:

| Range | Why |
|---|---|
| $n + 1 \le 2^{32} - 1$ and $m + n \le 2^{32} - 1$ | the solve stores every node and arc index in 32 bits |
| $\lvert c_{ij} \rvert \le$ `cost_bound(n)` for every arc | every potential and reduced cost fits 64 bits |
| $l_{ij} \le u_{ij}$ for every arc | every width $u_{ij} - l_{ij}$ is not negative |
| $\sum \lvert c_{ij} \rvert \, u_{ij} \le 2^{127} - 1$ | the cost of every flow, and each partial sum, fits 128 bits |
| $\sum_i b_i = 0$ | a flow can exist at all |
| the positive net supplies add up to at most $2^{64} - 2$ | every flow the solve moves fits 64 bits |

The cost bound depends on the node count:

```math
\texttt{cost\_bound}(n) = \left\lfloor \frac{2^{62} - 1}{n} \right\rfloor ,
\qquad \texttt{cost\_bound}(0) = 2^{62} - 1 ,
```

so that $n \cdot C < 2^{62}$. For example, `cost_bound(1000)` is 4,611,686,018,427,387 and `cost_bound(1000000)` is
4,611,686,018,427. The net supply of a node is its supply less the lower bounds of the arcs that leave it, plus those
of the arcs that enter it: what is left to route once every arc carries its lower bound.

Within these ranges, the numbers the solve forms are bounded as follows, with $M = (n - 1) C + 1$ the cost of the
links by which the starting basis routes a demand ([how it works](how-it-works.md#the-starting-basis)):

- $M \le 2^{62}$, and every potential lies within $[-(n - 1) C, \; M + (n - 1) C]$;
- every reduced cost lies within $\pm \max(nC, \; 2(n - 1) C + 1)$, below $2^{63}$ in magnitude;
- every arc's flow lies within its bounds, and the flow of every link that joins a node to the root of the basis
  within the positive net supply total, below $2^{64}$;
- the supplies are added up in 128 bits, the net supplies are formed in 128 bits, and the cost of the flow is summed
  in 128 bits within the bound of the fourth range.

## The refusals

A problem outside the ranges is refused, and no pivot is made. The refusal names the breach and holds the values that
make it one. Where a problem has several breaches, the first in the order of this table is answered, the five about one
arc taken arc by arc: the lowest arc that has any of them, and of that arc the first.

| Breach | Refused where | Values |
|---|---|---|
| `Length` | a per-arc table holds another number of entries than `tails` | the table, its length, the arc count |
| `TooLarge` | $n + 1$ or $m + n$ passes $2^{32} - 1$ | the node count, the arc count |
| `TailOutsideNodes` | an arc's tail is not below $n$ | the arc, its tail, the node count |
| `HeadOutsideNodes` | an arc's head is not below $n$ | the arc, its head, the node count |
| `LowerAboveCapacity` | an arc's lower bound passes its capacity | the arc, its lower bound, its capacity |
| `CostBeyondBound` | an arc's cost lies beyond $\pm$ `cost_bound(n)` | the arc, its cost, the node count |
| `CostTotalBeyondRange` | $\sum \lvert c \rvert u$, added in arc order, passes $2^{127} - 1$ | the arc it passes at |
| `Unbalanced` | the supplies do not add up to zero | their sum |
| `NetSupplyBeyondRange` | the positive net supplies add up past $2^{64} - 2$ | their sum |

Every language gives the same message for a breach, such as `the supplies add up to 1, not zero`, and the same
values as data. In C the counts are given with the tables, so a length is not a refusal there.

## What an answer holds

A solve that is not refused answers two things as one value: its outcome and its work.

The outcome is either optimal or infeasible. An optimal outcome holds the flows, the potentials and the cost of the
flow, $\sum c_{ij} x_{ij}$, exact in 128 bits. An infeasible outcome holds nothing more: no flows are written and no
potentials given, and a buffer the caller passed for them is left as it was.

The work counts the pivots of the solve. A pivot takes one arc that violates the optimality conditions and sends the
most flow around the cycle the arc closes with the current spanning tree; [how it works](how-it-works.md#a-pivot)
describes it. Where a link of the tree has the least room on the cycle, that link leaves the tree and the arc takes its
place; where the arc's own width is the least, no link leaves and the arc moves to its other bound, which is called a
flip.

| Count | What it counts |
|---|---|
| `pivots` | every pivot, flips included; zero where no arc ever violated the conditions, as in a problem of no arc |
| `degenerate_pivots` | the pivots that sent no flow, because the leaving link had no room; never a flip |
| `first_arc_pivots` | the pivots made by the arcs tried before block search, each made where the arc violated |

In every work answered, `degenerate_pivots` and `first_arc_pivots` are each at most `pivots`. Two more numbers follow
from the three and are not stored:

- the pivots that sent flow are `pivots - degenerate_pivots`;
- the block searches the solve ran are `pivots - first_arc_pivots + 1`: one for each pivot they chose, and one more
  that found no arc violating the conditions.

The counts are exact counts of that solve, not samples or estimates. A refused problem has no work, since no pivot is
made.

## Room and allocation

A solver owns the room it works in: the columns a solve reads and writes. The room is kept from
one solve to the next, never shrinks, and is freed with the solver. A new solver holds no room.

The room for a problem is made by a reservation for at least its node and arc counts, or by an earlier solve of a
problem at least as large that answered, optimal or infeasible. A solve of a problem within the room allocates nothing
in the library. What each interface allocates beside that:

| Interface | Reservation | Memory refused | Solve that allocates nothing within the room |
|---|---|---|---|
| Rust | `try_reserve(nodes, arcs)` | answers a `TryReserveError` | `solve_into` |
| C | `kahlnws_solver_reserve` | answers `KAHLNWS_STATUS_NO_ROOM` | `kahlnws_solver_solve` |
| Python | `reserve(nodes, arcs)` | raises `MemoryError` | `solve_into`, but for its answer and each buffer's view |

In Rust, `solve` allocates the flows and the potentials it answers. In Python, each call makes its answer and a
`memoryview` with an export record for each buffer it is handed; `solve` makes lists besides.

In Rust, a refused solve leaves the room short of the problem, though it may have grown one column. In C and Python the
room is grown to the problem's counts before the problem is checked, so a refused problem may still have grown it.

## Determinism

The answer of a solve, its work included, is a function of the problem as given, its arc order included, and of the
version of kahlnws. The same problem gives the same answer from a new solver and from one that solved other problems
before: nothing of an earlier problem is kept but the room. Where several flows are optimal, which one is answered is
not specified beyond that; reordering the arcs may change it, and its cost stays the same.

## Threads

The library starts no thread and keeps no global state. A solver serves one call at a time: in Rust a solve takes it
by `&mut`; in C no two calls on the same solver may run at once, and in Python a second call raises
`RuntimeError`. A solver may move between threads
from one call to the next, and distinct solvers may solve at once on distinct threads, also problems that read the
same tables. The Python package runs a solve with the interpreter released, so other Python threads run meanwhile.

## Names in each language

| Concept | Rust | C | Python |
|---|---|---|---|
| solver | `NetworkSimplex` | `kahlnws_solver` | `kahlnws.NetworkSimplex` |
| problem | `Problem` | `kahlnws_problem` | the keyword arguments of a solve |
| answer | `Answer<T>` | the status and the outputs | `kahlnws.Answer` |
| optimum, owned | `Solution` | none | `kahlnws.Solution` |
| optimum, written | `Optimum` | the outputs on `KAHLNWS_STATUS_OPTIMAL` | the cost, an `int` |
| infeasible | `Outcome::Infeasible` | `KAHLNWS_STATUS_INFEASIBLE` | `outcome` is `None` |
| work | `Work` | `kahlnws_work` | `kahlnws.Work` |
| refusal | `ProblemError` | `KAHLNWS_STATUS_REFUSED` and `kahlnws_refusal` | `kahlnws.ProblemError` |
| breach | a variant of `ProblemError` | `kahlnws_refusal_kind` | a class of `kahlnws.Breach` |
| cost bound | `cost_bound` | `kahlnws_cost_bound` | `kahlnws.cost_bound` |
