# The Rust interface by task

For a Rust programmer who uses the crate `kahlnws`. Each section answers one task with a complete program. Every
program on this page is compiled and run as a doctest of the crate, so it stays true. The contract behind the calls is
in [the problem and the guarantees](problem-and-guarantees.md).

The crate offers two types, one contract each:

| | The solver, `NetworkSimplex` | A network, `Network` |
|---|---|---|
| What it is | a function: a problem in, an answer out | a state: one problem held, with a basis of it |
| What it keeps | its room, and nothing else | the problem, a basis of it, and its optimum once solved |
| Its calls | `try_reserve`, `solve`, `solve_into` | `try_reserve`, `load`, setters, `solve`, `optimum`, reads, copies |
| An answer | a function of the problem alone | a function of the problem loaded and the calls made since |

Reach for the solver to solve one problem and have its answer. Reach for a network to hold a problem and solve it each
time it changes: a few numbers at a time, as a search tries a move and keeps it or sets it back, or all its numbers on
the same arcs, as each day of a plan brings new supplies and costs, loaded from the basis the network holds. Both
answer the same outcome and the same cost for the same problem.

The public surface is twenty-two items. Ten serve the solver, and a network's solve answers the same `Answer`,
`Outcome` and `Work`:

| Item | What it is |
|---|---|
| `Problem` | the six borrowed tables of one problem |
| `NetworkSimplex` | the solver, with the room it keeps from one solve to the next |
| `Answer<T>` | what a solve answers: its `outcome` and its `work` |
| `Outcome<T>` | `Optimal(T)` or `Infeasible` |
| `Solution` | the owned optimum of `solve`: `flows`, `potentials`, `total_cost` |
| `Optimum<'s>` | the optimum of `solve_into`: `total_cost`, and `potentials` lent by the solver |
| `Work` | the counts of a solve: `pivots`, `degenerate_pivots`, `first_arc_pivots` |
| `ProblemError` | why a problem is refused, with the values of the breach |
| `Table` | which per-arc table a `ProblemError::Length` names |
| `cost_bound` | the largest magnitude a cost may have for a node count |

Twelve serve [a network](#hold-a-problem-in-a-network):

| Item | What it is |
|---|---|
| `Network` | one problem held with a basis of it, loaded, changed number by number, solved, read and copied |
| `NetworkOptimum<'n>` | the optimum a network lends: `total_cost`, `flows` and `potentials`, read from the network |
| `Flows<'n>`, `FlowsIter<'n>` | the flow of each arc of that optimum, read where asked for, and the iterator over it |
| `Bounds` | an arc's `lower_bound` and `capacity`, set and answered as a pair |
| `ChangeError` | why a change of a network is refused, with the values of the breach |
| `ArcEnds<'n>`, `ArcEndsIter<'n>` | the tails or the heads of the arcs a network holds, and the iterator over them |
| `Costs<'n>`, `CostsIter<'n>` | the costs of the arcs a network holds, and the iterator over them |
| `FlowBounds<'n>`, `FlowBoundsIter<'n>` | the lower bounds or the capacities of the arcs, and the iterator over them |

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

## Hold a problem in a network

A network holds one problem, copied by `load` into the layout it is solved in, and a strongly feasible basis of it.
Each change is made at once and keeps the basis a basis of the problem as it then stands; `solve` pivots from that
basis. [How it works](how-it-works.md#a-network) says how, and
[the guarantees](problem-and-guarantees.md#a-network) state what a network promises.

### Load and solve

`load` refuses what the solver's `solve` refuses, with the same `ProblemError`, and a network that refuses a load
keeps what it held, its basis and its optimum included. `solve` takes no problem and answers no error: an `Answer`
whose outcome is the cost, and `optimum()` then lends the optimum, which borrows the network.

```rust
use kahlnws::{Network, Problem, ProblemError, Solution};

fn main() -> Result<(), ProblemError> {
    // A depot (node 0) sends 4 units to a customer (node 2), directly or through a hub (node 1).
    let mut network = Network::new();
    network.load(&Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    })?;

    let answer = network.solve();
    assert_eq!((answer.outcome.optimal(), answer.work.pivots), (Some(14), 4));
    let optimum = network.optimum().expect("the two routes carry the 4 units");
    assert_eq!(optimum.total_cost, 14);
    assert_eq!(optimum.flows.get(2), Some(2));
    assert_eq!(optimum.flows.iter().collect::<Vec<u64>>(), [2, 2, 2]);

    // Arc 2, from the hub to the customer, is full: its reduced cost is at most zero.
    assert!(1 + optimum.potentials[1] - optimum.potentials[2] <= 0);

    // Each arc's flow beside its tail and head, the views zipped.
    for ((tail, head), flow) in network.tails().iter().zip(network.heads()).zip(&optimum.flows) {
        println!("{tail} -> {head}: {flow}");
    }

    // An owned copy outlives the borrow. Nothing changed since, so solving once more makes no pivot.
    let solution = Solution::from(optimum);
    assert_eq!(network.solve().work.pivots, 0);
    assert_eq!(solution.flows, [2, 2, 2]);

    Ok(())
}
```

The cost is in the answer of `solve`, so a caller that needs no more reads nothing else. `flows` is a view:
`get(arc)` reads one arc's flow, its lower bound included, `iter()` every arc's in arc order, and `&optimum.flows`
iterates as `iter()` does, so a loop zips it with the views `tails()` and `heads()`. The optimum `optimum()` lends
borrows the network, and every change takes the network by `&mut`, so no optimum can be read past a change: such a
program does not compile. A caller that needs a flow or a potential after a change reads it before, or copies the
optimum into an owned `Solution` by `Solution::from`. `optimum()` lends the optimum again later, and answers `None`
where a load or a change came after the last solve, or where that solve found no flow. A network never loaded holds
the problem of no node, whose optimum is no flow at the cost zero.

A network's arcs are fixed at the load, each with its tail and head: a change sets a number on an arc and never adds
or removes one. An arc is switched off by a capacity of zero, where its lower bound is zero.

### Load new numbers on the same arcs

A load hands the network a whole problem, every number read anew, so supplies, bounds, capacities and costs may all
differ from those held. The solve after it answers the outcome and the cost the solver answers for the problem; the flow
among tied optima, the potentials and the work are the network's own. Where the problem has the node count, the arc
count and the tail and head at every arc of the problem held, the load starts that solve from the basis the network
holds, wherever that basis serves the new numbers, and otherwise from the start a fresh solve takes, every arc at its
lower bound. So where only some numbers changed, the solve often makes far fewer pivots than a fresh solve, and the same
problem loaded again makes none. [How it works](how-it-works.md#a-load-from-the-basis-held) says how a basis is carried
over to new numbers. The example `network` holds three days so:

```rust
use kahlnws::{Network, NetworkSimplex, Problem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Two warehouses (nodes 0 and 1) serve three shops (nodes 2 to 4); every lane carries up to 20 units.
    let (tails, heads) = ([0, 0, 0, 1, 1, 1], [2, 3, 4, 2, 3, 4]);
    let (capacities, costs) = ([20; 6], [8, 6, 10, 9, 12, 13]);
    let days = [[20, 30, -10, -25, -15], [20, 30, -20, -20, -10], [20, 30, -5, -15, -30]];

    let mut network = Network::new();
    network.try_reserve(5, 6)?;

    let mut answered = Vec::new();
    for supplies in &days {
        let problem = Problem {
            tails: &tails,
            heads: &heads,
            lower_bounds: None,
            capacities: &capacities,
            costs: &costs,
            supplies,
        };

        // The first day starts as a fresh solve does; each later day from the basis the day before ended on.
        network.load(&problem)?; // allocates nothing
        let answer = network.solve();
        let pivots = answer.work.pivots;
        let cost = answer.outcome.optimal().expect("the warehouses cover every day's demand");
        let fresh = NetworkSimplex::new().solve(&problem)?.work.pivots;
        answered.push((cost, pivots, fresh));
    }

    // Each day's cost, the pivots of the network, and those of the solver's solve of the same day.
    assert_eq!(answered, [(465, 6, 6), (430, 1, 5), (525, 1, 7)]);

    Ok(())
}
```

A load costs a few passes over the arcs and the nodes, from either start, and marks every arc for the next solve to
price. A load that is refused leaves the network as it was, its basis included, so the next load starts from that
basis where it serves.

### Change one number

| Call | What it changes | Answers |
|---|---|---|
| `set_cost(arc, cost)` | the cost of a unit of flow on `arc` | the cost it had |
| `set_capacity(arc, capacity)` | the most flow of `arc`; zero switches it off, at lower bound 0 | the capacity it had |
| `set_lower_bound(arc, lower_bound)` | the least flow of `arc` | the lower bound it had |
| `set_bounds(arc, bounds)` | both bounds of `arc` at once | the `Bounds` it had |
| `set_supply(node, supply, balancing_node)` | the supply of `node`, balanced by `balancing_node` | the supply it had |
| `move_supply(from_node, to_node, amount)` | `amount` units of supply, from one node to another | `()` |

`set_supply` is the everyday call for a supply. It makes `supply` the supply of `node`, a demand below zero, and
answers the supply it had; the supply of `balancing_node` moves by the same amount the other way, so the supplies
still add up to zero. A depot that is to send 2 units where it sent 4, balanced by the customer it sends to, is
`set_supply(depot, 2, customer)`: the customer then takes 2 less. The same call with the supply it answered sets both
nodes back. A node that balances itself is given only the supply it has, which changes nothing; any other supply for
it panics, as an index not below its count does.

`move_supply` is the pair move. It lowers the supply of `from_node` by `amount` and raises the supply of `to_node` by
`amount`, so the supplies still add up to zero; a move of demand is written from the node that will demand more, a
customer that needs two more units, sent by a depot, being `move_supply(customer, depot, 2)`. Moving the amount back,
with the two nodes swapped, sets both nodes back.

Each change is made at once: the network then holds the problem with the number changed, and its next solve finds
the optimum of that problem. A change that sets the number already held changes nothing and keeps the optimum; any
other change ends it, also where the flow held stays optimal. `set_capacity` and `set_lower_bound` check the bound
they set against the other bound the arc holds, so bounds that move past each other's old value are set at once by
`set_bounds`.

```rust
use kahlnws::{Bounds, ChangeError, Network, Problem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut network = Network::new();
    network.load(&Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    })?;
    network.solve();

    // The leg from the hub costs 3: 2 units at 4 and 2 at 5. The setter answers the cost it replaced.
    let had = network.set_cost(2, 3)?;
    assert_eq!((had, network.optimum()), (1, None));
    assert_eq!(network.solve().outcome.optimal(), Some(18));
    network.set_cost(2, had)?;

    // The customer demands one unit more, which the depot sends: 3 directly at 5 and 2 through the hub at 2.
    network.move_supply(2, 0, 1)?;
    assert_eq!(network.supplies(), [5, 0, -5]);
    assert_eq!(network.solve().outcome.optimal(), Some(19));

    // The hub's lane switched off: only 3 of the 5 units fit, so no flow exists. Its capacity set back switches it on.
    let capacity = network.set_capacity(1, 0)?;
    assert!(network.solve().outcome.optimal().is_none());
    network.set_capacity(1, capacity)?;

    // Arc 2 from `0..=2` to `3..=4`: the lower bound alone is refused, past the capacity held; both at once are set.
    let refused = ChangeError::LowerAboveCapacity { arc: 2, lower_bound: 3, capacity: 2 };
    assert_eq!(network.set_lower_bound(2, 3), Err(refused));
    network.set_bounds(2, Bounds { lower_bound: 3, capacity: 4 })?;
    // 4 units through the hub at 2 and 1 directly at 5.
    assert_eq!(network.solve().outcome.optimal(), Some(13));

    // A cost past the bound is refused, and changes nothing: the optimum is still held.
    let refused = ChangeError::CostBeyondBound { arc: 0, cost: i64::MAX, node_count: 3 };
    assert_eq!(network.set_cost(0, i64::MAX), Err(refused));
    assert!(network.optimum().is_some());

    Ok(())
}
```

A refused change answers a `ChangeError` and changes nothing: the network keeps the problem and the optimum it held.
Each variant names one breach and holds its values, and a setter checks in the order the variants are listed:

| Variant | Refused where |
|---|---|
| `LowerAboveCapacity { arc, lower_bound, capacity }` | a bound set leaves the lower bound above the capacity |
| `CostBeyondBound { arc, cost, node_count }` | a cost set lies beyond plus or minus `cost_bound(node_count)` |
| `CostTotalBeyondRange { arc }` | a cost or capacity set takes the costs times the capacities past `i128::MAX` |
| `SupplyBeyondRange { node, supply }` | a supply set or moved leaves `i64`, the node taken from named first |
| `ExcessBeyondRange { total }` | a move or a bound would leave more than `2^64 − 2` units of flow still to be sent |

`set_cost` refuses with the second and the third, `set_capacity` and `set_bounds` with the first, the third and the
last, `set_lower_bound` with the first and the last, and `set_supply` and `move_supply` with the last two. The first
three refuse what a load refuses under the same name, and `CostTotalBeyondRange` names the arc changed. A load leaves to
be sent what its start does not route, which from no basis held is the whole of the positive supplies net of the lower
bounds, so a change right after a load of supplies near that limit can be refused; a change adds what the arcs could not
carry at once; only a solve that finds a flow leaves none. [How it
works](how-it-works.md#a-link-that-cannot-carry-what-arrives) says where that flow waits. An arc or a node not below its
count panics, before anything changes, and so does `set_supply` for a node that balances itself with another supply than
it has.

### Read the problem back

`tails()`, `heads()`, `costs()`, `lower_bounds()` and `capacities()` lend views of the arcs the network holds, arc
`a`'s number at `a`, each read where it is asked for: `get(arc)`, `iter()` and `len()`, with `get_unchecked` beside
`get`. `supplies()` lends the supplies, node `v`'s at `v`. One number is read by the name of its table in the
singular: `tail(arc)`, `head(arc)`, `cost(arc)`, `lower_bound(arc)`, `capacity(arc)` and `supply(node)`, and the pair
`set_bounds` takes by `bounds(arc)`, each panicking on an index not below its count, as its setter does. They answer
the problem as it stands: as loaded, with every change admitted since and none a refused change named. A read takes
the network by `&`, so it is made beside an optimum, changes nothing and ends no optimum; a caller keeps no copy of
the problem of its own.

```rust
use kahlnws::{Bounds, Network, Problem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut network = Network::new();
    network.load(&Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    })?;
    network.set_capacity(1, 3)?;
    network.solve();

    // Read beside the optimum, which a read leaves lent.
    let optimum = network.optimum().expect("the routes carry the 4 units");
    assert_eq!(network.heads().iter().collect::<Vec<usize>>(), [2, 1, 2]);
    assert_eq!(network.costs().get(0), Some(5));
    assert_eq!(network.bounds(1), Bounds { lower_bound: 0, capacity: 3 });
    assert_eq!((network.supplies(), optimum.total_cost), (&[4, 0, -4][..], 14));

    Ok(())
}
```

### Copy, and return to a state

`try_clone()` copies a network into a new one, and `try_clone_from(&source)` makes an existing network a copy of
another within the room it holds; `clone` and `clone_from` copy the same way, but end the process where the memory
for the copy is refused, as a `Vec`'s do. A copy holds the problem, the basis and the optimum of its source, answers
every call as its source would, its work included, and changes apart from it. A search that keeps a state to return
to keeps it in a second network, its room reserved once, and restores it by `try_clone_from`, which allocates nothing
where the room holds the other network's counts:

```rust
use kahlnws::{Network, Problem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut network = Network::new();
    network.load(&Problem {
        tails: &[0, 0, 1],
        heads: &[2, 1, 2],
        lower_bounds: None,
        capacities: &[3, 4, 2],
        costs: &[5, 1, 1],
        supplies: &[4, 0, -4],
    })?;
    network.solve();

    // The state to return to, in room made once.
    let mut saved = Network::new();
    saved.try_reserve(network.node_count(), network.arc_count())?;
    saved.try_clone_from(&network)?;

    // The search moves away from it, by as many changes as it likes.
    network.set_cost(0, 1)?;
    network.move_supply(2, 0, 1)?;
    network.solve();

    // Back at once: the saved problem, its optimum and its basis, from which a solve makes no pivot.
    network.try_clone_from(&saved)?;
    assert_eq!(network.optimum().map(|optimum| optimum.total_cost), Some(14));
    assert_eq!(network.solve().work.pivots, 0);

    Ok(())
}
```

### Set the numbers back

A network holds the problem as it stands and a basis of it, and never the changes that led there: the crate keeps no
history and has no undo. A change is undone by the change back, which its answer puts at hand: the number a setter
answered set again, the supply `set_supply` answered set with the same balancing node, or the amount moved back
with the two nodes swapped. The problem is then the earlier one, and the next solve answers its outcome and its cost,
with a flow of that cost, which among tied optima may be another than before: the basis is where the changes left it,
not the earlier one. Setting a few numbers back costs what those changes back cost; a copy costs a pass over the whole
network, whatever changed. A capacity of zero and the capacity it answered switch an arc of lower bound zero off and on
so, and a caller may build every arc it may ever need up front.

A cost set back directly after its change is never refused. A bound or a supply set back directly after is refused
only with `ExcessBeyondRange`, and admitted wherever the flow still to be sent before the change and twice the flow
the change moved add up to at most `2^64 − 2`: never refused for amounts and flows below `2^62`. Other calls in
between, among them a solve that finds no flow, can leave that flow higher; a set-back refused stays refused until a
solve that finds a flow or other changes lower it, and a load of the earlier problem always restores it.

A search tries a move, solves, and keeps the move where the cost falls or sets it back where it does not. A move may
make several changes. A refused change changes nothing, so where a later change of a move is refused, what the changes
before it made is the caller's to set back, each with the number its setter answered. This is a shorter form of the
story the example `search` tells, on the transport network of the examples: four moves where the example makes five,
and the stock of warehouse 0 set by `set_supply` where the example moves it by `move_supply`:

```rust
use kahlnws::{ChangeError, Network, Problem};

/// A change of the problem a network holds: a lane's cost, lower bound or capacity, or a node's supply against another.
#[derive(Clone, Copy)]
enum Change {
    Cost(usize, i64),
    LowerBound(usize, u64),
    Capacity(usize, u64),
    Supply(usize, i64, usize),
}

impl Change {
    /// Makes the change on `network`, and answers the change that sets it back.
    fn make(self, network: &mut Network) -> Result<Self, ChangeError> {
        Ok(match self {
            Self::Cost(lane, cost) => Self::Cost(lane, network.set_cost(lane, cost)?),
            Self::LowerBound(lane, lower_bound) => {
                Self::LowerBound(lane, network.set_lower_bound(lane, lower_bound)?)
            }
            Self::Capacity(lane, capacity) => Self::Capacity(lane, network.set_capacity(lane, capacity)?),
            Self::Supply(node, supply, balancing_node) => {
                Self::Supply(node, network.set_supply(node, supply, balancing_node)?, balancing_node)
            }
        })
    }
}

/// Makes the changes of a move in order, and answers the changes that set them back, the last made first. Where one
/// is refused, it sets back those made before it and answers the refusal.
fn make_move(network: &mut Network, changes: &[Change]) -> Result<Vec<Change>, ChangeError> {
    let mut set_backs = Vec::new();
    for change in changes {
        match change.make(network) {
            Ok(set_back) => set_backs.insert(0, set_back),
            Err(refusal) => {
                for set_back in set_backs {
                    set_back.make(network)?;
                }
                return Err(refusal);
            }
        }
    }

    Ok(set_backs)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Two warehouses (nodes 0 and 1) serve three shops (nodes 2 to 4); lane 3 * w + s leads from warehouse w to
    // shop s.
    let mut network = Network::new();
    network.load(&Problem {
        tails: &[0, 0, 0, 1, 1, 1],
        heads: &[2, 3, 4, 2, 3, 4],
        lower_bounds: None,
        capacities: &[20; 6],
        costs: &[8, 6, 10, 9, 12, 13],
        supplies: &[20, 30, -10, -25, -15],
    })?;
    let mut best = network.solve().outcome.optimal().expect("the warehouses cover the demand");
    assert_eq!(best, 465);

    // Lane 3 off costs 50 more; lane 1 at 4 for at least 25 units is refused, as it carries 20; warehouse 0 holding
    // 25 units saves 15; lane 4 at 7 saves 45 more.
    let moves: [&[Change]; 4] = [
        &[Change::Capacity(3, 0)],
        &[Change::Cost(1, 4), Change::LowerBound(1, 25)],
        &[Change::Supply(0, 25, 1)],
        &[Change::Cost(4, 7)],
    ];
    for changes in moves {
        // A refused move is set back by `make_move`, and leaves nothing to solve.
        let Ok(set_backs) = make_move(&mut network, changes) else { continue };
        match network.solve().outcome.optimal() {
            Some(cost) if cost < best => best = cost,
            _ => {
                for set_back in set_backs {
                    set_back.make(&mut network)?;
                }
            }
        }
    }

    assert_eq!(best, 405);
    assert_eq!((network.capacity(3), network.cost(1), network.supply(0), network.cost(4)), (20, 6, 25, 7));
    assert_eq!(network.solve().outcome.optimal(), Some(best));

    Ok(())
}
```

### What each call costs

A network pays where only a few numbers change at a time. In the caller's terms:

- **A load** costs a few passes over the arcs and the nodes, from either start, and the solve after it prices every
  arc.
- **A cost** costs a few steps where it moves no node's potential, and otherwise the nodes whose potentials it moves,
  which can be most of the network.
- **A bound** costs a few steps where the arc's flow can stay where it is; otherwise what a move of supply of the flow
  it moves costs, a flow of at most the change of the bound.
- **A move of supply** that the flow held can carry costs a walk from the two nodes along that flow to where their
  routes meet. Where it cannot, it costs the nodes whose potentials move, which can be most of the network.
- **A solve** prices first the arcs a load or a change may have made worth using. With nothing changed it prices no
  arc and makes no pivot. After changes that call for no pivot it prices only those arcs: none for a move the flow
  held carried, the arcs near it for a cost, and for a change that moved potentials the arcs at the smaller of the two
  groups of nodes, those whose potentials moved and the rest, which can be half the network. A solve that pivots
  prices at least one whole pass over the arcs, as a solver's solve does. Where a change leaves much flow still to be
  sent, so that the solve makes many pivots, the change and the solve can cost more than a load of the whole problem
  as it stands and the solve after it.
- **A read** of one number costs a few steps; **a copy** a few passes over the arcs and the nodes; `Solution::from` a
  pass over them, and it allocates.

### Room and allocation

A network keeps its room from one load to the next, and the room only grows. `try_reserve(node_count, arc_count)`
makes the room for problems of up to those counts at once; a load of such a problem allocates nothing afterwards, and
without a reservation a load allocates only where the room is smaller than the problem. Changes, solves and reads
never allocate. `try_clone_from` allocates nothing where the room holds the counts of the network it copies, and the
changes and solves of a copy allocate nothing. `try_reserve`, `try_clone` and `try_clone_from` answer a refused memory
as a `TryReserveError`, `try_clone_from` with the network as it was; `load`, `clone` and `clone_from` end the process
there, as a `Vec` that grows does. The doc of `Network` states how many bytes it holds an arc and a node.

### What a network answers

The outcome and the cost are those of the problem as it stands, and where the solver admits that problem it answers
the same outcome and cost; the potentials prove the flow optimal, as [checking an optimal
answer](#check-an-optimal-answer) does. The flow among tied optima, the potentials and the work are the network's own:
a function of the problem loaded and of the calls made since, in order, so two networks given the same calls answer
alike, their work included. They may differ from what the solver answers for the same problem: a network may pivot
otherwise than the solver, and its solve tries no arcs first, so its `first_arc_pivots` is zero.

Moves of supply and lower bounds keep every supply within `i64` and the flow still to be sent within `2^64 − 2`, but
the supplies net of the lower bounds may come to add up past `2^64 − 2` where they are positive, where the arcs carry
the difference. A solve and a load refuse such a problem with `ProblemError::NetSupplyBeyondRange`; the network holds
it and solves it exactly.

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
solve at a time, as a network serves one load, change or solve at a time; distinct solvers and distinct networks may
work at once on distinct threads, also on problems that borrow the same tables.

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

Six programs under `crates/kahlnws/examples/` solve one network, two warehouses that supply three shops, as a user
would. The examples of the other languages carry the same names and print the same lines. Each program asserts its
optimum, worked out by hand, and `cargo test` runs each as a test.

| Example | What it shows |
|---|---|
| `transport` | the smallest useful call: one solve, its cost and the flow on each lane |
| `reuse` | one solver for three days of demand, its room reserved once, each day solved into the same buffer |
| `network` | the days of `reuse` held by a network, each later day loaded from the basis the day before ended on |
| `search` | a search on the first day held: five moves, one refused at its second change, each kept or set back |
| `outcomes` | a problem with no flow, answered as an outcome, and a refused problem, answered as an error |
| `potentials` | the potentials, each lane's reduced cost, the check of the optimality conditions, and the work |

From the root of the repository:

```sh
cargo run -p kahlnws --example transport
```
