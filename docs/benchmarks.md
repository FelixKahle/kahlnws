# Benchmarks

For anyone who wants to measure kahlnws: what the two benchmark suites solve, how to run them, how to read what they
print, how to make a measurement that can be compared with another, and how kahlnws stands beside two LP solvers.

## What the suites measure

Two suites under `crates/kahlnws/benches/` solve the same networks: `cold.rs` times solves from the starting basis,
and `warm.rs` a resolve from a kept basis beside a solve of the same network.

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

### Solves from a kept basis

`warm.rs` takes each network of the cold suite, changes it, and solves the changed network twice: by a resolve from
the basis a solve of the network before the change ended on, and by a solve. Each network comes in these changes:

| Change | What changes |
|---|---|
| `same` | nothing: the network the kept basis ended on, again |
| `supplies` | three supply moves, each of one to three units between two nodes of one role |
| `arcs` | three capacities, each by one or two units, and the costs of three arcs, redrawn |
| `broad` | the capacity of about four arcs in fifteen and the cost of about as many, each by one or two units |
| `costs-hundredth` | the costs of a hundredth of the arcs, rounded up, redrawn |
| `costs-tenth` | the costs of a tenth of the arcs, rounded up, redrawn |
| `costs-all` | the costs of every arc, redrawn |

`same` is the floor of a resolve: its passes over the arcs and the nodes, and no pivot. `supplies` and `arcs` move a
few numbers, three of each, the same count on every network, so on the large networks they are small changes by
construction. A supply move takes an amount from one node and gives it to another of the same role, a supplier's
supply to another supplier or a demander's demand to another demander, and the giver keeps its role. The grid has one
supplier and one demander, so its rows have no `supplies` pair. `broad` and the three shares of the costs scale with
the network. A redrawn cost is the one the arc takes in the family's network with its costs drawn from another
stream: each arc's cost from the range of its role in the family, and a derived cost, such as the grid's slack arc,
by its rule; only arcs whose cost the redraw moves are chosen. Every change is made to the family's own network, and
the row's costs, tying or distinct, are applied after it, so a change of a distinct row moves costs and not only their
tie-breaks. Every change is drawn from one fixed seed, so every run makes the same changes. A change of costs leaves a
network's flow in place; moving supplies or capacities may leave none, and a pair whose network has no flow times two
answers that say so.

Each change gives a pair of rows, `warm/<family>/<costs>/<size>/<change>/resolve` and `.../solve`. Both rows start
each iteration from the same solver: one that solved the network before the change and kept the basis it ended on,
cloned outside the timing and its room reserved, so that nothing is allocated inside it. The `resolve` row times one
`resolve_into` of the changed network by that clone, which starts from the kept basis; the `solve` row one
`solve_into`, which starts from the star. The problem handed in is made once, outside the timing. So every iteration
does the same work, and the two rows of a pair solve the same network from the same solver.

Each iteration needs a clone of its own, so the clock is read around each one: a row's time includes one reading of
the clock, the same in both rows of a pair. Where a row's time is no more than a few readings, as on the smallest
networks and on the resolves that make no pivot, that addend compresses the ratio of the pair, and the ratio
understates the difference between the two.

Before a pair is timed, the change must have changed the network, unless it is `same`; each entry runs twice from two
clones and must answer alike, its work included; an optimal answer must pass the certificate; and the two must answer
the same outcome and cost. The resolve must try no arc before block search, as a start from the star does wherever a
solve's first arcs pivot; and since a resolve that cannot start from the kept basis answers exactly a solve's work,
one whose work differs from the solve's started from it. Where the two works are equal and neither tried an arc
first, the work cannot tell the starts apart, and the printed line says so. On `same` the resolve must make no pivot,
since the kept basis is the problem's own end; a network whose kept basis could not serve it would stop the suite
there. The suite then prints, for each pair, a line with the network's arcs, what the change moved (the supplies and
the units moved, the capacities and the costs changed), the outcome, and the pivots of each entry.

## Running the suites

From the root of the repository:

```sh
cargo bench -p kahlnws --bench cold
cargo bench -p kahlnws --bench warm
```

The benchmarks build with the `bench` profile, which is the release profile: fat link-time optimisation and one code
generation unit. Arguments after `--` go to the benchmark harness; a prefix of a row's name runs only the rows it
matches:

```sh
cargo bench -p kahlnws --bench cold -- cold/grid/
cargo bench -p kahlnws --bench warm -- warm/grid/tying/296/
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

The time of a row is the time of one whole solve of that network, its checks, its reservation and, in the warm
suite, its clone excluded. Rows of different families or sizes are not comparable as times; the throughput in arcs
puts them on one scale, though the work per arc differs from family to family. The two rows of a warm pair compare
directly: the same network, solved from the same solver, timed alike. A warm row does not compare with a cold row,
not even the `solve` row of `same` with the cold row of the same network: a cold row repeats one solve on one solver
and times the handing in of the problem with it, which reads every arc's lower bound, while a warm row times one solve
on a fresh clone with a reading of the clock around it.

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
