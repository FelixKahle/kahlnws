# Benchmarks

For anyone who wants to measure kahlnws: what the three benchmark suites solve, how to run them, how to read what they
print, how to make a measurement that can be compared with another, what a network's kept state measured against a
fresh solve, and how kahlnws stands beside two LP solvers.

## What the suites measure

Three suites under `crates/kahlnws/benches/` solve the same networks: `cold.rs` times the solver's solves, which start
from the star; `warm.rs` a network's load of a changed network on the basis it holds, with the solve after it, beside
the solver's solve of the same network; and `changes.rs` a network's changes of a few numbers, with the solve after
them, beside a load of the whole changed network and the solver's solve. The networks every suite sweeps, and what a
row's measurement shares, are in `benches/rows/sweep.rs`; what the two suites that change a network share, the changes'
draws and the network and solver every iteration starts from, in `benches/rows/row_change.rs`.

Every network is drawn from seeded generators and is a function of its family and size alone, so every run of a
suite solves the same networks. A row of the cold suite is named `cold/<family>/<costs>/<size>`:

| Family | Sizes | Costs | Size means |
|---|---|---|---|
| `supply` | 20, 80, 300 | `tying`, `distinct` | the number of demand nodes; about 250, 1,000 and 4,000 arcs |
| `assignment` | 32, 128, 512 | `tying`, `distinct` | the number of demand nodes; about 250, 1,000 and 4,000 arcs |
| `grid` | 2, 5, 9, 18, 37, 74, 148, 296 | `tying` | the side $s$ of the square grid |

That is twenty rows.

- **The supply shape.** Chains of one to three holding nodes, each chain's first node holding a volume; each holding
  node draws to two demand nodes by twin arcs, two parallel arcs of the same capacity and cost, and writes off to a
  sink; each demand node can also be covered by a slack source at a high cost.
- **The assignment shape.** Sources, half as many as the demands, each feeding three demands by twin arcs and a
  surplus sink; each demand passes on to its own served node and to one later demand's, and each served node can be
  covered by a slack source at a high cost.
- **The grid.** An $s \times s$ grid of nodes, a source feeding the first node of every row and the last node of
  every row feeding a sink; every node feeds its right, lower and upper neighbour. One more arc joins the source to
  the sink at twenty times the side a unit. The grid has $s^2 + 2$ nodes and $3s^2 - s + 1$ arcs, from 11 arcs at
  side 2 to 262,553 at side 296.

At `tying` costs the costs are small, so many flows are optimal: the twin arcs of the supply and the assignment shape
tie, and the costs of the grid's arcs between its nodes take only the values 1 to 20. At `distinct` costs each cost
$c$ of arc $a$ becomes $c \cdot K + t_a$, with a tie-break $t_a$ from 1 to 4095 per arc and $K$ large enough that
an optimum at these costs is also optimal at the tying costs; the tie-breaks separate most twin arcs: they are drawn per
arc and can repeat. A row at tying costs stands beside the same network at distinct costs, with far fewer ties.

### Cold solves

Each row of `cold.rs` times one cold solve: one call of `solve_into` on one network, by a solver reserved for that
network, into a buffer of flows made once. Nothing is allocated inside the timing, and nothing is reset between
iterations, since a solve starts from the starting basis and reads nothing of the solve before. The work of an
iteration is therefore fixed by the network alone, and every sample runs the same number of iterations. The
throughput is counted in arcs.

Before a row is timed, its solve runs twice: both runs must answer the same flows, potentials and cost, and the answer
must pass the certificate (the flow within its bounds and meeting every supply, the potentials pricing it optimal, the
cost as stated). A row whose checks fail stops the suite.

### Loads on a kept basis

`warm.rs` takes each network of the cold suite, changes it, and solves the changed network twice: by a network that
holds the basis a solve of the network before the change ended on, loaded with the changed network and solved, and by
the solver from the star. Each network comes in these changes:

| Change | What changes |
|---|---|
| `same` | nothing: the network the kept basis ended on, again |
| `supplies` | three supply moves, each of one to three units between two nodes of one role |
| `arcs` | three capacities, each by one or two units, and the costs of three arcs, redrawn |
| `broad` | the capacity of about four arcs in fifteen and the cost of about as many, each by one or two units |
| `costs-hundredth` | the costs of a hundredth of the arcs, rounded up, redrawn |
| `costs-tenth` | the costs of a tenth of the arcs, rounded up, redrawn |
| `costs-all` | the costs of every arc, redrawn |

`same` is the floor of a load: its passes over the arcs and the nodes, and a solve with no pivot. `supplies` and
`arcs` move a few numbers, three of each, the same count on every network, so on the large networks they are small
changes by construction. A supply move takes an amount from one node and gives it to another of the same role, a
supplier's supply to another supplier or a demander's demand to another demander, and the giver keeps its role. The
grid has one supplier and one demander, so its rows have no `supplies` pair. `broad` and the three shares of the costs
scale with the network. A redrawn cost is the one the arc takes in the family's network with its costs drawn from
another stream: each arc's cost from the range of its role in the family, and a derived cost, such as the grid's slack
arc, by its rule; only arcs whose cost the redraw moves are chosen. Every change is made to the family's own network,
and the row's costs, tying or distinct, are applied after it, the distinct costs scaled by the multiplier of the row's
own network, so a change of a distinct row moves the costs it names, not only their tie-breaks, and no other cost
where it moves a capacity. Every change is drawn from one fixed seed, so every run makes the same changes. A change of
costs leaves a network's flow in place; moving supplies or capacities may leave none, and a pair whose network has no
flow times two answers that say so.

Each change gives a pair of rows, `warm/<family>/<costs>/<size>/<change>/load` and `.../solve`. The `load` row starts
each iteration from a network that loaded the network before the change, solved it and holds the basis that solve
ended on; the `solve` row from a solver that solved it, whose room holds it. Each is cloned outside the timing and its
room reserved, so that nothing is allocated inside it. The `load` row times one `Network::load` of the changed network
by that clone and the `Network::solve` after it; the `solve` row one `solve_into`, which starts from the star. The
problem handed in is made once, outside the timing, so every iteration does the same work.

Before a pair is timed, the change must have changed the network, unless it is `same`; each entry runs twice from two
clones and must answer alike, its work included; an optimal answer must pass the certificate; and the two must answer
the same outcome and cost. A load that cannot start from the kept basis starts from the star and answers exactly what
a load into a network that held nothing answers, its work included; so the load's work must differ from that one's,
which proves it started from the kept basis. On `same` the load's solve must make no pivot, since the kept basis is
the problem's own end. The suite then prints, for each pair, a line with the network's arcs, what the change moved
(the supplies and the units moved, the capacities and the costs changed), the outcome, and the pivots of each entry.

### Changes of a network

`changes.rs` takes each network of the cold suite, held by a network that solved it and holds its optimum, and changes
a few of its numbers by the network's setters. Each network comes in these changes:

| Change | What changes |
|---|---|
| `same` | nothing: a solve of the network as its last solve left it, the floor of a network's solve |
| `cost` | the cost of one arc, redrawn |
| `switch-off` | one arc that carries flow in the optimum held switched off, its capacity set to zero |
| `switch-on` | that arc switched on again, its capacity set back, on the network solved without it |
| `supply-pair` | one amount of one to three units moved between two nodes of one role |
| `costs-3`, `switch-offs-3`, `supply-pairs-3` | three of each, the same count on every network |

Costs and supply moves are drawn as the warm suite draws them; an arc switched off is drawn among the arcs of lower
bound zero that carry flow in the optimum held. The grid's rows have no supply pair. Every change is drawn from one
fixed seed, and a network may have no flow once an arc is switched off or a supply moved; its rows then time answers
that no flow exists.

Each change gives three rows, `changes/<family>/<costs>/<size>/<change>/network`, `.../load` and `.../solve`. The
`network` row times the calls that make the change, `set_cost`, `set_capacity` and `move_supply`, on a clone of the
network that held the optimum before it, and the solve after them. The `load` row times a load of the whole changed
network into the same clone, which starts from the basis it holds, and the solve after it. The `solve` row times the
solver's `solve_into` of the changed network, from the star, by a clone of a solver that solved the network before the
change. Every clone is made and reserved outside the timing, and the problem handed in is made once, so nothing is
allocated inside the timing.

Before the rows of a change are timed, the change must have changed the network, unless it is `same`; each entry runs
twice from two clones and must answer alike, its work included; an optimal answer must pass the certificate; and the
three entries must answer the same outcome and cost. Where nothing changed, neither the network nor the load may make
a pivot. The suite prints a line for each change with the network's arcs, what the change moved and in how many calls,
the outcome and each entry's pivots.

### The clock in the warm and changes suites

Each iteration of a warm or a changes row needs a clone of its own, so the clock is read around each one: a row's time
includes one reading of the clock, the same in every row of one change. Where a row's time is no more than a few
readings, as on the smallest networks and on the loads and changes that make no pivot, that addend compresses the
ratio between the rows, and the ratio understates the difference between them.

## Running the suites

From the root of the repository:

```sh
cargo bench -p kahlnws --bench cold
cargo bench -p kahlnws --bench warm
cargo bench -p kahlnws --bench changes
```

The benchmarks build with the `bench` profile, which is the release profile: fat link-time optimisation and one code
generation unit. Arguments after `--` go to the benchmark harness; a prefix of a row's name runs only the rows it
matches:

```sh
cargo bench -p kahlnws --bench cold -- cold/grid/
cargo bench -p kahlnws --bench warm -- warm/grid/tying/296/
cargo bench -p kahlnws --bench changes -- changes/supply/
```

Without timing, the suites are a smoke run: every row's checks, then every row once.

```sh
cargo test -p kahlnws --benches
```

## Reading the output

For each row the harness prints three numbers for the time of one solve, `[lower estimate upper]`: the estimate in
the middle with the bounds of its confidence interval, and the throughput in arcs per second likewise. Where an
earlier run of the same row is stored, it also prints the change against it, with its own interval, and says whether
the change is beyond noise. The harness keeps every row's measurements under `target/criterion/`.

The time of a row is the time of one whole call of that row, its checks, its reservation and, in the warm and the
changes suites, its clone excluded: a solve, a load and its solve, or a change and its solve. Rows of different
families or sizes are not comparable as times; the throughput in arcs puts them on one scale, though the work per arc
differs from family to family. The rows of one change compare directly: the same network, from the same state, timed
alike. They compare only within their own group: a warm row does not compare with a changes row or a cold row, not
even the `solve` row of `same` with the cold row of the same network, since a cold row repeats one solve on one solver
and times the handing in of the problem with it, which reads every arc's lower bound, while a warm or a changes row
times one call on a fresh clone with a reading of the clock around it.

## A fair measurement

A measurement says something only where it can be repeated and compared:

- Measure the release build the suite builds, never a debug build.
- Run on a quiet machine: no other heavy work, on mains power, the same power settings for every run, and no change
  to the machine between the runs compared.
- Compare runs of the same toolchain and the same suite. Save a baseline before a change and compare against it after:

  ```sh
  cargo bench -p kahlnws --bench cold -- --save-baseline before
  cargo bench -p kahlnws --bench cold -- --baseline before
  ```

- Measure the noise first: run the same build twice and compare the two runs. A difference between two builds counts
  only where it is larger than the difference between two runs of one build.
- Record what was measured: the machine and its processor, the operating system, the toolchain, the commit, the date,
  and the full output of the harness.
- Compare the work as well as the time. The same network solved by two versions may take different pivot paths;
  [the work](problem-and-guarantees.md#what-an-answer-holds) of a solve tells a change of path from a change of speed.

## A network beside the solver

The warm and the changes suites set a network's kept state beside a fresh start on the same changed network. This
measurement is of a selection of their rows.

### What is compared

- **Loads on a kept basis.** For each pair of the warm suite: the time of the network's load of the changed network
  and the solve after it, beside the time of the solver's solve of it from the star.
- **Changes of a network.** For each change of the changes suite: the time of the network's calls that make the
  change and the solve after them, beside the time of a load of the whole changed network into the same network and
  the solve after it; the time of the solver's solve of the changed network stands beside them.
- **The rows.** Every network of the supply and the assignment family, and the grid at its sides 2, 37 and 296: its
  smallest, a middle and its largest size, in every change. All 102 pairs of the warm suite at those networks were
  timed. Of the changes suite, every change of the supply family and of the assignment family at sizes 32 and 128
  and at size 512 with tying costs, and at size 512 with distinct costs the changes `same`, `cost`, `switch-off`,
  `switch-on` and `supply-pair`, were timed: 93 changes. The changes `costs-3`, `switch-offs-3` and
  `supply-pairs-3` of `assignment/distinct/512` and every change of the grid were not timed.
- **The time** of a row is the median of its samples at the suites' own settings. Every pair and every change passed
  its checks before it was timed.

### The measurement

Measured on 17 November 2026 on an Apple M3 Max with 14 cores and 36 GB of memory under macOS 27.2.1, on mains
power, with Rust 1.100.0, on the code of the commit that adds this record. One suite ran at a time, with no other
build, test or benchmark beside it. The confidence interval of every median spans less than 1% of it.

Loads on a kept basis. In the last column: how many times the time of the load and its solve the fresh solve took.

| Network | Change | Load and solve | Fresh solve | Ratio |
|---|---|---:|---:|---:|
| `tying/20` | `same` | 3.84 µs | 5.57 µs | 1.5x |
| `tying/20` | `supplies` | 3.87 µs | 5.63 µs | 1.5x |
| `tying/20` | `arcs` | 3.82 µs | 5.63 µs | 1.5x |
| `tying/20` | `broad` | 5.03 µs | 5.8 µs | 1.2x |
| `tying/20` | `costs-hundredth` | 4.01 µs | 5.58 µs | 1.4x |
| `tying/20` | `costs-tenth` | 4.17 µs | 6.03 µs | 1.4x |
| `tying/20` | `costs-all` | 6.36 µs | 5.67 µs | 0.9x |
| `distinct/20` | `same` | 3.85 µs | 5.92 µs | 1.5x |
| `distinct/20` | `supplies` | 4.58 µs | 5.56 µs | 1.2x |
| `distinct/20` | `arcs` | 3.84 µs | 5.85 µs | 1.5x |
| `distinct/20` | `broad` | 5.16 µs | 5.58 µs | 1.1x |
| `distinct/20` | `costs-hundredth` | 4.22 µs | 5.24 µs | 1.2x |
| `distinct/20` | `costs-tenth` | 4.71 µs | 5.77 µs | 1.2x |
| `distinct/20` | `costs-all` | 6.7 µs | 6.72 µs | 1.0x |
| `tying/80` | `same` | 13.8 µs | 42.7 µs | 3.1x |
| `tying/80` | `supplies` | 13.8 µs | 44.8 µs | 3.2x |
| `tying/80` | `arcs` | 13.9 µs | 42.6 µs | 3.1x |
| `tying/80` | `broad` | 18.8 µs | 40.9 µs | 2.2x |
| `tying/80` | `costs-hundredth` | 17.2 µs | 37.5 µs | 2.2x |
| `tying/80` | `costs-tenth` | 22.5 µs | 44.6 µs | 2.0x |
| `tying/80` | `costs-all` | 40.8 µs | 36.1 µs | 0.9x |
| `distinct/80` | `same` | 14 µs | 44.4 µs | 3.2x |
| `distinct/80` | `supplies` | 13.9 µs | 40.2 µs | 2.9x |
| `distinct/80` | `arcs` | 13.8 µs | 42 µs | 3.0x |
| `distinct/80` | `broad` | 19.5 µs | 42.4 µs | 2.2x |
| `distinct/80` | `costs-hundredth` | 19.5 µs | 38.8 µs | 2.0x |
| `distinct/80` | `costs-tenth` | 23.7 µs | 39.3 µs | 1.7x |
| `distinct/80` | `costs-all` | 39.7 µs | 43.7 µs | 1.1x |
| `tying/300` | `same` | 56.6 µs | 225 µs | 4.0x |
| `tying/300` | `supplies` | 56.4 µs | 217 µs | 3.9x |
| `tying/300` | `arcs` | 56.8 µs | 229 µs | 4.0x |
| `tying/300` | `broad` | 71.5 µs | 253 µs | 3.5x |
| `tying/300` | `costs-hundredth` | 59.5 µs | 218 µs | 3.7x |
| `tying/300` | `costs-tenth` | 112 µs | 221 µs | 2.0x |
| `tying/300` | `costs-all` | 259 µs | 212 µs | 0.8x |
| `distinct/300` | `same` | 56.6 µs | 221 µs | 3.9x |
| `distinct/300` | `supplies` | 56.8 µs | 213 µs | 3.8x |
| `distinct/300` | `arcs` | 56.5 µs | 220 µs | 3.9x |
| `distinct/300` | `broad` | 88.9 µs | 287 µs | 3.2x |
| `distinct/300` | `costs-hundredth` | 60.7 µs | 204 µs | 3.4x |
| `distinct/300` | `costs-tenth` | 101 µs | 240 µs | 2.4x |
| `distinct/300` | `costs-all` | 271 µs | 295 µs | 1.1x |

| Network | Change | Load and solve | Fresh solve | Ratio |
|---|---|---:|---:|---:|
| `tying/32` | `same` | 3.34 µs | 4.58 µs | 1.4x |
| `tying/32` | `supplies` | 3.56 µs | 4.46 µs | 1.3x |
| `tying/32` | `arcs` | 3.35 µs | 4.54 µs | 1.4x |
| `tying/32` | `broad` | 3.69 µs | 5.92 µs | 1.6x |
| `tying/32` | `costs-hundredth` | 3.54 µs | 4.82 µs | 1.4x |
| `tying/32` | `costs-tenth` | 4.11 µs | 4.67 µs | 1.1x |
| `tying/32` | `costs-all` | 5.66 µs | 4.7 µs | 0.8x |
| `distinct/32` | `same` | 3.34 µs | 3.87 µs | 1.2x |
| `distinct/32` | `supplies` | 3.57 µs | 3.84 µs | 1.1x |
| `distinct/32` | `arcs` | 3.32 µs | 3.82 µs | 1.2x |
| `distinct/32` | `broad` | 3.83 µs | 4.56 µs | 1.2x |
| `distinct/32` | `costs-hundredth` | 3.52 µs | 4.01 µs | 1.1x |
| `distinct/32` | `costs-tenth` | 4.49 µs | 4.11 µs | 0.9x |
| `distinct/32` | `costs-all` | 5.22 µs | 5.2 µs | 1.0x |
| `tying/128` | `same` | 13.5 µs | 32.8 µs | 2.4x |
| `tying/128` | `supplies` | 16.4 µs | 23.9 µs | 1.5x |
| `tying/128` | `arcs` | 13.5 µs | 32.5 µs | 2.4x |
| `tying/128` | `broad` | 18.1 µs | 25.9 µs | 1.4x |
| `tying/128` | `costs-hundredth` | 15.9 µs | 26.6 µs | 1.7x |
| `tying/128` | `costs-tenth` | 23.5 µs | 31.8 µs | 1.4x |
| `tying/128` | `costs-all` | 26.2 µs | 27.6 µs | 1.1x |
| `distinct/128` | `same` | 13.5 µs | 27.7 µs | 2.0x |
| `distinct/128` | `supplies` | 17.1 µs | 27.2 µs | 1.6x |
| `distinct/128` | `arcs` | 13.5 µs | 27.5 µs | 2.0x |
| `distinct/128` | `broad` | 18.5 µs | 27 µs | 1.5x |
| `distinct/128` | `costs-hundredth` | 17.5 µs | 26 µs | 1.5x |
| `distinct/128` | `costs-tenth` | 25.1 µs | 35.9 µs | 1.4x |
| `distinct/128` | `costs-all` | 32.6 µs | 25.7 µs | 0.8x |
| `tying/512` | `same` | 57.6 µs | 273 µs | 4.7x |
| `tying/512` | `supplies` | 58 µs | 264 µs | 4.6x |
| `tying/512` | `arcs` | 58 µs | 271 µs | 4.7x |
| `tying/512` | `broad` | 101 µs | 265 µs | 2.6x |
| `tying/512` | `costs-hundredth` | 75.2 µs | 284 µs | 3.8x |
| `tying/512` | `costs-tenth` | 141 µs | 257 µs | 1.8x |
| `tying/512` | `costs-all` | 279 µs | 287 µs | 1.0x |
| `distinct/512` | `same` | 57.7 µs | 310 µs | 5.4x |
| `distinct/512` | `supplies` | 67.4 µs | 304 µs | 4.5x |
| `distinct/512` | `arcs` | 57.3 µs | 314 µs | 5.5x |
| `distinct/512` | `broad` | 108 µs | 324 µs | 3.0x |
| `distinct/512` | `costs-hundredth` | 78.4 µs | 317 µs | 4.0x |
| `distinct/512` | `costs-tenth` | 161 µs | 313 µs | 1.9x |
| `distinct/512` | `costs-all` | 324 µs | 320 µs | 1.0x |

| Network | Change | Load and solve | Fresh solve | Ratio |
|---|---|---:|---:|---:|
| `tying/2` | `same` | 227 ns | 306 ns | 1.3x |
| `tying/2` | `arcs` | 246 ns | 288 ns | 1.2x |
| `tying/2` | `broad` | 232 ns | 313 ns | 1.3x |
| `tying/2` | `costs-hundredth` | 229 ns | 309 ns | 1.4x |
| `tying/2` | `costs-tenth` | 229 ns | 310 ns | 1.4x |
| `tying/2` | `costs-all` | 302 ns | 355 ns | 1.2x |
| `tying/37` | `same` | 61.2 µs | 2.27 ms | 37.1x |
| `tying/37` | `arcs` | 62.8 µs | 2.14 ms | 34.0x |
| `tying/37` | `broad` | 258 µs | 2.3 ms | 8.9x |
| `tying/37` | `costs-hundredth` | 90.8 µs | 2.13 ms | 23.5x |
| `tying/37` | `costs-tenth` | 213 µs | 2.29 ms | 10.7x |
| `tying/37` | `costs-all` | 541 µs | 2.29 ms | 4.2x |
| `tying/296` | `same` | 4.44 ms | 2.38 s | 537x |
| `tying/296` | `arcs` | 4.98 ms | 2.39 s | 480x |
| `tying/296` | `broad` | 274 ms | 2.41 s | 8.8x |
| `tying/296` | `costs-hundredth` | 44.9 ms | 2.4 s | 53.5x |
| `tying/296` | `costs-tenth` | 233 ms | 2.41 s | 10.3x |
| `tying/296` | `costs-all` | 1.11 s | 2.39 s | 2.2x |

Changes of a network. In the column `Ratio`: how many times the time of the change and its solve the load and its
solve took.

| Network | Change | Change and solve | Load and solve | Ratio | Fresh solve |
|---|---|---:|---:|---:|---:|
| `tying/20` | `same` | 34 ns | 3.61 µs | 106x | 5.62 µs |
| `tying/20` | `cost` | 47.9 ns | 3.62 µs | 75.4x | 5.64 µs |
| `tying/20` | `switch-off` | 1.95 µs | 5.19 µs | 2.7x | 5.48 µs |
| `tying/20` | `switch-on` | 1.88 µs | 5.24 µs | 2.8x | 5.61 µs |
| `tying/20` | `supply-pair` | 87.3 ns | 3.62 µs | 41.4x | 5.91 µs |
| `tying/20` | `costs-3` | 86 ns | 3.63 µs | 42.2x | 5.62 µs |
| `tying/20` | `switch-offs-3` | 2.39 µs | 5.28 µs | 2.2x | 8.28 µs |
| `tying/20` | `supply-pairs-3` | 1.13 µs | 3.63 µs | 3.2x | 5.9 µs |
| `distinct/20` | `same` | 34.2 ns | 3.63 µs | 106x | 6.11 µs |
| `distinct/20` | `cost` | 47.9 ns | 3.63 µs | 75.8x | 6.09 µs |
| `distinct/20` | `switch-off` | 505 ns | 3.77 µs | 7.5x | 6.05 µs |
| `distinct/20` | `switch-on` | 303 ns | 3.7 µs | 12.2x | 5.93 µs |
| `distinct/20` | `supply-pair` | 87.1 ns | 3.65 µs | 41.9x | 5.95 µs |
| `distinct/20` | `costs-3` | 85.8 ns | 3.64 µs | 42.4x | 5.9 µs |
| `distinct/20` | `switch-offs-3` | 3.32 µs | 6.13 µs | 1.8x | 5.95 µs |
| `distinct/20` | `supply-pairs-3` | 261 ns | 3.65 µs | 14.0x | 6.06 µs |
| `tying/80` | `same` | 35.6 ns | 13.2 µs | 371x | 44.2 µs |
| `tying/80` | `cost` | 64.3 ns | 13.5 µs | 210x | 44.7 µs |
| `tying/80` | `switch-off` | 5.66 µs | 17.5 µs | 3.1x | 44.2 µs |
| `tying/80` | `switch-on` | 1.63 µs | 14 µs | 8.6x | 44.1 µs |
| `tying/80` | `supply-pair` | 80.1 ns | 13.6 µs | 170x | 44.6 µs |
| `tying/80` | `costs-3` | 809 ns | 13.2 µs | 16.3x | 42.2 µs |
| `tying/80` | `switch-offs-3` | 8.06 µs | 20.2 µs | 2.5x | 43.6 µs |
| `tying/80` | `supply-pairs-3` | 3.64 µs | 15.6 µs | 4.3x | 44.9 µs |
| `distinct/80` | `same` | 35.7 ns | 13.2 µs | 368x | 45.9 µs |
| `distinct/80` | `cost` | 59.8 ns | 13.2 µs | 220x | 45.8 µs |
| `distinct/80` | `switch-off` | 3.3 µs | 15.4 µs | 4.7x | 43.8 µs |
| `distinct/80` | `switch-on` | 1.51 µs | 14.1 µs | 9.3x | 46.3 µs |
| `distinct/80` | `supply-pair` | 82.1 ns | 13.2 µs | 161x | 46 µs |
| `distinct/80` | `costs-3` | 1.03 µs | 13.4 µs | 13.1x | 41.9 µs |
| `distinct/80` | `switch-offs-3` | 8.33 µs | 19.3 µs | 2.3x | 37.9 µs |
| `distinct/80` | `supply-pairs-3` | 4.47 µs | 16.1 µs | 3.6x | 45.3 µs |
| `tying/300` | `same` | 45.4 ns | 54.2 µs | 1195x | 235 µs |
| `tying/300` | `cost` | 105 ns | 54 µs | 516x | 233 µs |
| `tying/300` | `switch-off` | 10.6 µs | 62.3 µs | 5.9x | 227 µs |
| `tying/300` | `switch-on` | 21.2 µs | 74.9 µs | 3.5x | 233 µs |
| `tying/300` | `supply-pair` | 97.1 ns | 54.2 µs | 559x | 234 µs |
| `tying/300` | `costs-3` | 261 ns | 53.9 µs | 207x | 234 µs |
| `tying/300` | `switch-offs-3` | 17 µs | 64.1 µs | 3.8x | 209 µs |
| `tying/300` | `supply-pairs-3` | 265 ns | 54.2 µs | 205x | 223 µs |
| `distinct/300` | `same` | 45.1 ns | 54.2 µs | 1202x | 229 µs |
| `distinct/300` | `cost` | 107 ns | 54.1 µs | 508x | 222 µs |
| `distinct/300` | `switch-off` | 19 µs | 66.3 µs | 3.5x | 225 µs |
| `distinct/300` | `switch-on` | 22.1 µs | 74.5 µs | 3.4x | 230 µs |
| `distinct/300` | `supply-pair` | 96.4 ns | 54.2 µs | 562x | 227 µs |
| `distinct/300` | `costs-3` | 271 ns | 54.1 µs | 200x | 228 µs |
| `distinct/300` | `switch-offs-3` | 25.5 µs | 72.5 µs | 2.8x | 245 µs |
| `distinct/300` | `supply-pairs-3` | 256 ns | 54.2 µs | 212x | 255 µs |

| Network | Change | Change and solve | Load and solve | Ratio | Fresh solve |
|---|---|---:|---:|---:|---:|
| `tying/32` | `same` | 34.1 ns | 3.61 µs | 106x | 4.73 µs |
| `tying/32` | `cost` | 47.1 ns | 3.15 µs | 67.0x | 4.74 µs |
| `tying/32` | `switch-off` | 816 ns | 4.09 µs | 5.0x | 4.85 µs |
| `tying/32` | `switch-on` | 690 ns | 3.7 µs | 5.4x | 4.72 µs |
| `tying/32` | `supply-pair` | 88.4 ns | 3.11 µs | 35.2x | 4.85 µs |
| `tying/32` | `costs-3` | 296 ns | 3.14 µs | 10.6x | 4.7 µs |
| `tying/32` | `switch-offs-3` | 3.97 µs | 6.53 µs | 1.6x | 5.31 µs |
| `tying/32` | `supply-pairs-3` | 757 ns | 3.39 µs | 4.5x | 4.57 µs |
| `distinct/32` | `same` | 33.9 ns | 3.13 µs | 92.2x | 4.01 µs |
| `distinct/32` | `cost` | 47 ns | 3.14 µs | 66.7x | 4 µs |
| `distinct/32` | `switch-off` | 808 ns | 3.61 µs | 4.5x | 4.62 µs |
| `distinct/32` | `switch-on` | 700 ns | 3.72 µs | 5.3x | 4.01 µs |
| `distinct/32` | `supply-pair` | 88.1 ns | 3.1 µs | 35.3x | 4.06 µs |
| `distinct/32` | `costs-3` | 292 ns | 3.13 µs | 10.7x | 4 µs |
| `distinct/32` | `switch-offs-3` | 3.6 µs | 6.1 µs | 1.7x | 5.24 µs |
| `distinct/32` | `supply-pairs-3` | 817 ns | 3.47 µs | 4.2x | 4.18 µs |
| `tying/128` | `same` | 35.4 ns | 13 µs | 368x | 33.9 µs |
| `tying/128` | `cost` | 59.2 ns | 12.8 µs | 216x | 27.2 µs |
| `tying/128` | `switch-off` | 819 ns | 12.8 µs | 15.6x | 33.2 µs |
| `tying/128` | `switch-on` | 832 ns | 13.2 µs | 15.8x | 33.8 µs |
| `tying/128` | `supply-pair` | 114 ns | 12.8 µs | 113x | 25.9 µs |
| `tying/128` | `costs-3` | 176 ns | 13 µs | 73.5x | 33.7 µs |
| `tying/128` | `switch-offs-3` | 8.32 µs | 19.4 µs | 2.3x | 34.5 µs |
| `tying/128` | `supply-pairs-3` | 240 ns | 12.9 µs | 53.7x | 26.9 µs |
| `distinct/128` | `same` | 34.8 ns | 12.8 µs | 369x | 28.5 µs |
| `distinct/128` | `cost` | 59.4 ns | 13 µs | 218x | 28.8 µs |
| `distinct/128` | `switch-off` | 884 ns | 13.4 µs | 15.1x | 28.5 µs |
| `distinct/128` | `switch-on` | 823 ns | 13.1 µs | 15.9x | 29.2 µs |
| `distinct/128` | `supply-pair` | 2.89 µs | 14.6 µs | 5.0x | 33.9 µs |
| `distinct/128` | `costs-3` | 821 ns | 13.5 µs | 16.4x | 28.6 µs |
| `distinct/128` | `switch-offs-3` | 3.18 µs | 15.2 µs | 4.8x | 28.8 µs |
| `distinct/128` | `supply-pairs-3` | 3.05 µs | 14.6 µs | 4.8x | 31 µs |
| `tying/512` | `same` | 48.3 ns | 55.4 µs | 1147x | 285 µs |
| `tying/512` | `cost` | 15 µs | 69.3 µs | 4.6x | 305 µs |
| `tying/512` | `switch-off` | 15.6 µs | 62.7 µs | 4.0x | 284 µs |
| `tying/512` | `switch-on` | 121 ns | 55.5 µs | 460x | 285 µs |
| `tying/512` | `supply-pair` | 100 ns | 56.2 µs | 560x | 268 µs |
| `tying/512` | `costs-3` | 9.44 µs | 61.8 µs | 6.5x | 284 µs |
| `tying/512` | `switch-offs-3` | 18.2 µs | 62.6 µs | 3.4x | 293 µs |
| `tying/512` | `supply-pairs-3` | 12 µs | 57.1 µs | 4.7x | 286 µs |
| `distinct/512` | `same` | 45.6 ns | 55.1 µs | 1209x | 320 µs |
| `distinct/512` | `cost` | 19.3 µs | 74.5 µs | 3.9x | 314 µs |
| `distinct/512` | `switch-off` | 38.5 µs | 86.6 µs | 2.2x | 328 µs |
| `distinct/512` | `switch-on` | 27.5 µs | 82.3 µs | 3.0x | 319 µs |
| `distinct/512` | `supply-pair` | 92.7 ns | 55.8 µs | 602x | 295 µs |

### What it shows

- A load of a network in which a few numbers changed (`same`, `supplies`, `arcs`) and its solve is faster than a
  fresh solve on every network, and the more so the larger the network: 1.1 to 1.5 times at the smallest sizes, 3.8
  to 5.5 times at the supply and the assignment shape's largest, about 35 times on the grid of side 37 and about
  500 times on the grid of side 296, where it takes 4 to 5 ms against 2.4 s.
- Where a share of the costs is redrawn the load pays less: with a tenth of them 1.3 to 2.4 times on the supply and
  the assignment shape from their middle size, about 10 times on the grids of side 37 and 296.
- Where every cost is redrawn (`costs-all`) the load is no faster than a fresh solve on the supply and the assignment
  shape: 0.8 to 1.1 times, slower on six of their twelve networks, by up to a fifth. On the grids it is still 4.2
  times faster at side 37 and 2.2 times at side 296.
- A change made by the network's calls and its solve is faster than a load of the whole changed network and its
  solve in every change timed. Where three arcs that carry flow are switched off on the smallest networks it is 1.6
  to 2.2 times faster, for a single arc switched off or on 2.2 to 16 times (460 times on `assignment/tying/512`),
  and for one cost or one supply move 35 to 600 times, the change and its solve taking 47 to 114 ns, save one supply
  move on `assignment/distinct/128`, 5 times, and one cost on `assignment/512`, 3.9 to 4.6 times.
- The time of `same` in the changes suite, 34 to 48 ns, comes near the reading of the clock the suite makes around
  each iteration, so its ratio understates the difference, as the clock in the warm and changes suites says.

One machine, one day and a selection of the rows: the numbers say what was measured there.

## Beside two LP solvers

A min-cost flow problem is a linear program, so a general LP solver solves it too. The script
`crates/kahlnws_python/benches/solvers.py` solves the same networks with kahlnws, with HiGHS and with Gurobi, and
`charts.py` beside it draws the charts below from the table it prints.

### What is compared

- **The networks.** Two families, drawn from seeded generators in the script itself, so every run solves the same
  networks; they are not the networks of the suite above. `grid/<side>` is the square grid of the suite's shape, with
  capacities from 1 to 100 and costs from 1 to 20. `transport/<side>` has as many sources as sinks and a lane from
  every source to every sink, with costs from 1 to 100 and lanes that carry whatever is sent.
- **kahlnws.** One `solve_into` through the Python package, by a solver with room reserved, into a buffer made once.
  The time includes the copy of the columns into the solver. It runs on one thread, in exact integers.
- **HiGHS**, at its defaults, and **Gurobi** twice: at its defaults, where it may use every core of the machine, and
  with its network simplex asked for (`NetworkAlg=1`). Each holds the network as a linear program over the flow
  conservation rows, built outside the timing, and is reset before every solve, so each solve is cold. Both work in
  floating point.
- **The time** of a row is the median of seven solves. Every solver's optimum is compared with the cost kahlnws
  answers before a row is printed.

### The measurement

Measured on 12 November 2026 on an Apple M3 Max with 14 cores and 36 GB of memory under macOS 27.2.1, with Python
3.13, kahlnws 0.1.0 built by Rust 1.99.1 as a release wheel, HiGHS 1.16.0 and Gurobi 14.0.0 under an academic
licence. In brackets: how many times the time of kahlnws a solver took.

| Row | Nodes | Arcs | kahlnws | HiGHS | Gurobi | Gurobi, network simplex |
|---|---:|---:|---:|---:|---:|---:|
| `grid/9` | 83 | 235 | 0.020 ms | 0.807 ms (41.3x) | 0.457 ms (23.4x) | 0.441 ms (22.6x) |
| `grid/18` | 326 | 955 | 0.232 ms | 5.226 ms (22.5x) | 6.381 ms (27.5x) | 4.283 ms (18.5x) |
| `grid/37` | 1,371 | 4,071 | 2.155 ms | 46.651 ms (21.6x) | 39.383 ms (18.3x) | 38.922 ms (18.1x) |
| `grid/74` | 5,478 | 16,355 | 22.987 ms | 471.714 ms (20.5x) | 52.530 ms (2.3x) | 420.910 ms (18.3x) |
| `grid/148` | 21,906 | 65,565 | 238.923 ms | 6229.743 ms (26.1x) | 204.945 ms (0.9x) | 5315.404 ms (22.2x) |
| `transport/16` | 32 | 256 | 0.007 ms | 0.420 ms (64.2x) | 0.185 ms (28.3x) | 0.167 ms (25.5x) |
| `transport/32` | 64 | 1,024 | 0.021 ms | 1.402 ms (65.2x) | 1.519 ms (70.6x) | 0.255 ms (11.9x) |
| `transport/64` | 128 | 4,096 | 0.100 ms | 6.413 ms (63.9x) | 4.330 ms (43.1x) | 0.620 ms (6.2x) |
| `transport/128` | 256 | 16,384 | 0.414 ms | 30.840 ms (74.5x) | 4.218 ms (10.2x) | 2.466 ms (6.0x) |
| `transport/256` | 512 | 65,536 | 2.303 ms | 153.862 ms (66.8x) | 24.205 ms (10.5x) | 17.630 ms (7.7x) |

<img src="img/solve-times-light.svg#gh-light-mode-only"
     alt="The time of one cold solve against the arcs of the network, per solver">
<img src="img/solve-times-dark.svg#gh-dark-mode-only"
     alt="The time of one cold solve against the arcs of the network, per solver">

<img src="img/slowdown-light.svg#gh-light-mode-only"
     alt="The time of each LP solver as a multiple of the time of kahlnws">
<img src="img/slowdown-dark.svg#gh-dark-mode-only"
     alt="The time of each LP solver as a multiple of the time of kahlnws">

### What it shows

- On every transport network kahlnws is the fastest: HiGHS takes 64 to 75 times as long, Gurobi's network simplex 6
  to 26 times, Gurobi at its defaults 10 to 71 times. Its time grows about as the arcs do.
- On the grids HiGHS and Gurobi's network simplex take 18 to 41 times as long at every size.
- On the grids the time of kahlnws grows faster than the arcs, about ten times for four times the arcs. Gurobi at
  its defaults, on fourteen cores, closes in as the grid grows: 2.3 times the time of kahlnws at 16,355 arcs, and
  0.9 times, which is faster, at 65,565 arcs. Past that size the comparison was not measured.

One machine, one day and two families: the numbers say what was measured there, not what holds for every network.

### Running it

In an environment that holds the package, NumPy, SciPy, `highspy`, and `gurobipy` with a licence; the charts need
Altair and `vl-convert-python`:

```sh
python crates/kahlnws_python/benches/solvers.py > table.md
python crates/kahlnws_python/benches/charts.py docs/img < table.md
```

A prefix of a row's name runs only the rows it matches, such as `solvers.py transport/`.
