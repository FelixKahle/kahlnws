# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Cold solves of seeded min-cost flow networks by kahlnws beside two general LP solvers, HiGHS and Gurobi.

Every network is a function of its family, its size and this file alone. Each solver holds its model of the network,
built once and outside the timing, and a row times the cold solve alone: kahlnws from its starting basis into buffers
made once, an LP solver after its model is reset, so that nothing of the solve before is kept. Every row runs the same
number of solves per solver and states their median. Before a row is printed, every solver's optimum is compared with
the cost kahlnws answers, and a difference stops the run.

HiGHS runs at its defaults. Gurobi runs twice: at its defaults, and with its network simplex asked for. The two LP
solvers work in floating point on the constraint matrix; kahlnws works on the arcs in exact integers.

Run with an environment that holds the package, NumPy, SciPy, `highspy` and `gurobipy` (with a licence):

    python crates/kahlnws_python/benches/solvers.py            # every row
    python crates/kahlnws_python/benches/solvers.py grid/      # the rows whose name starts so

It prints a table in Markdown: the time of one solve in milliseconds per solver, and how many times the time of
kahlnws each LP solver took.
"""

import statistics
import sys
import time
from collections.abc import Callable, Iterator
from dataclasses import dataclass
from typing import Protocol

import gurobipy
import highspy
import numpy as np
import numpy.typing as npt
import scipy.sparse

import kahlnws

# The solves a row times per solver. A count, not a duration: every row does the same work on every machine.
SOLVES_PER_ROW = 7

# The sides of the grids and of the transport networks, each about twice the one before.
GRID_SIDES = (9, 18, 37, 74, 148)
TRANSPORT_SIDES = (16, 32, 64, 128, 256)


@dataclass(frozen=True)
class Network:
    """A min-cost flow network as columns: arc `a` runs from `tails[a]` to `heads[a]`, carries at most
    `capacities[a]` units at `costs[a]` a unit, and node `v` supplies `supplies[v]` units. Every lower bound is zero."""

    tails: npt.NDArray[np.uintp]
    heads: npt.NDArray[np.uintp]
    capacities: npt.NDArray[np.uint64]
    costs: npt.NDArray[np.int64]
    supplies: npt.NDArray[np.int64]

    @property
    def node_count(self) -> int:
        return len(self.supplies)

    @property
    def arc_count(self) -> int:
        return len(self.tails)


def grid(side: int, seed: int) -> Network:
    """The `side` by `side` grid: a source feeds the first node of every row, every node feeds its right, lower and
    upper neighbour, and the last node of every row feeds a sink. Capacities are drawn from 1 to 100 and costs from 1
    to 20. The source supplies what its arcs can carry, and one more arc from the source to the sink carries as much
    at twenty times the side a unit, so a flow always exists."""
    generator = np.random.default_rng(seed)
    nodes = np.arange(side * side).reshape(side, side)
    source, sink = side * side, side * side + 1
    ends = [
        (np.full(side, source), nodes[:, 0]),
        (nodes[:, :-1].ravel(), nodes[:, 1:].ravel()),
        (nodes[:-1, :].ravel(), nodes[1:, :].ravel()),
        (nodes[1:, :].ravel(), nodes[:-1, :].ravel()),
        (nodes[:, -1], np.full(side, sink)),
    ]
    tails = np.concatenate([tail for tail, _ in ends])
    heads = np.concatenate([head for _, head in ends])
    capacities = generator.integers(1, 100, size=len(tails), endpoint=True)
    costs = generator.integers(1, 20, size=len(tails), endpoint=True)
    supply = int(capacities[:side].sum())
    supplies = np.zeros(side * side + 2, dtype=np.int64)
    supplies[source], supplies[sink] = supply, -supply

    return Network(
        tails=np.append(tails, source).astype(np.uintp),
        heads=np.append(heads, sink).astype(np.uintp),
        capacities=np.append(capacities, supply).astype(np.uint64),
        costs=np.append(costs, 20 * side).astype(np.int64),
        supplies=supplies,
    )


def transport(side: int, seed: int) -> Network:
    """The transport network of `side` sources and `side` sinks with a lane from every source to every sink. Each sink
    takes 50 to 150 units, the sources share out the same total at random, a lane carries whatever is sent, and its
    cost is drawn from 1 to 100."""
    generator = np.random.default_rng(seed)
    demands = generator.integers(50, 150, size=side, endpoint=True)
    total = int(demands.sum())
    sources = generator.multinomial(total, np.full(side, 1.0 / side))
    tails, heads = np.divmod(np.arange(side * side), side)

    return Network(
        tails=tails.astype(np.uintp),
        heads=(heads + side).astype(np.uintp),
        capacities=np.full(side * side, total, dtype=np.uint64),
        costs=generator.integers(1, 100, size=side * side, endpoint=True).astype(np.int64),
        supplies=np.concatenate([sources, -demands]).astype(np.int64),
    )


def rows() -> Iterator[tuple[str, Network]]:
    """Every row of the comparison, by name: the grids, then the transport networks, each by ascending size. The seed
    of a network is its side, so a row is the same network on every run."""
    for side in GRID_SIDES:
        yield f"grid/{side}", grid(side, seed=side)
    for side in TRANSPORT_SIDES:
        yield f"transport/{side}", transport(side, seed=side)


def flow_conservation(network: Network) -> scipy.sparse.csr_array:
    """The matrix of the flow conservation rows, one row per node and one column per arc: 1 where the arc leaves the
    node, -1 where it enters it. A self-loop's two entries cancel."""
    arcs = np.arange(network.arc_count)
    values = np.concatenate([np.ones(network.arc_count), -np.ones(network.arc_count)])
    entries = (values, (np.concatenate([network.tails, network.heads]), np.concatenate([arcs, arcs])))

    return scipy.sparse.csr_array(entries, shape=(network.node_count, network.arc_count))


class ColdSolve(Protocol):
    """One solver's model of one network, built once."""

    def solve(self) -> float:
        """Solve the network from scratch and answer the optimal cost."""
        ...


class KahlnwsSolve:
    """The network on kahlnws: a solver with room for it and the buffer its flows are written to."""

    def __init__(self, network: Network) -> None:
        self._network = network
        self._solver = kahlnws.NetworkSimplex()
        self._solver.reserve(node_count=network.node_count, arc_count=network.arc_count)
        self._flows = np.zeros(network.arc_count, dtype=np.uint64)

    def solve(self) -> float:
        network = self._network
        answer = self._solver.solve_into(
            tails=network.tails,
            heads=network.heads,
            capacities=network.capacities,
            costs=network.costs,
            supplies=network.supplies,
            flows=self._flows,
        )
        if answer.outcome is None:
            raise RuntimeError("the network should have a flow")

        return answer.outcome


class HighsSolve:
    """The network as a linear program on HiGHS at its defaults."""

    def __init__(self, network: Network) -> None:
        rows = flow_conservation(network)
        supplies = network.supplies.astype(np.float64)
        self._model = highspy.Highs()
        self._model.setOptionValue("output_flag", False)
        self._model.addVars(network.arc_count, np.zeros(network.arc_count), network.capacities.astype(np.float64))
        self._model.changeColsCost(
            network.arc_count, np.arange(network.arc_count, dtype=np.int32), network.costs.astype(np.float64)
        )
        self._model.addRows(network.node_count, supplies, supplies, rows.nnz, rows.indptr[:-1], rows.indices, rows.data)

    def solve(self) -> float:
        self._model.clearSolver()
        self._model.run()
        if self._model.getModelStatus() != highspy.HighsModelStatus.kOptimal:
            raise RuntimeError("the linear program should be solved to optimality")

        return float(self._model.getObjectiveValue())


class GurobiSolve:
    """The network as a linear program on Gurobi: at its defaults, or with its network simplex asked for."""

    def __init__(self, network: Network, environment: gurobipy.Env, *, network_simplex: bool) -> None:
        self._model = gurobipy.Model(env=environment)
        flows = self._model.addMVar(
            network.arc_count, ub=network.capacities.astype(np.float64), obj=network.costs.astype(np.float64)
        )
        self._model.addConstr(flow_conservation(network) @ flows == network.supplies.astype(np.float64))
        if network_simplex:
            self._model.Params.NetworkAlg = 1
        self._model.update()

    def solve(self) -> float:
        self._model.reset()
        self._model.optimize()
        if self._model.Status != gurobipy.GRB.OPTIMAL:
            raise RuntimeError("the linear program should be solved to optimality")

        return float(self._model.ObjVal)


def median_milliseconds(solve: Callable[[], float], expected_cost: float) -> float:
    """The median time of `SOLVES_PER_ROW` solves, in milliseconds, each of which must answer `expected_cost`."""
    times = []

    for _ in range(SOLVES_PER_ROW):
        started = time.perf_counter_ns()
        cost = solve()
        times.append(time.perf_counter_ns() - started)
        if abs(cost - expected_cost) > 1e-6 * max(1.0, abs(expected_cost)):
            raise RuntimeError(f"a solver answered the cost {cost} where kahlnws answered {expected_cost}")

    return statistics.median(times) / 1e6


def main() -> None:
    prefix = sys.argv[1] if len(sys.argv) > 1 else ""
    environment = gurobipy.Env(params={"OutputFlag": 0})

    print("| Row | Nodes | Arcs | kahlnws | HiGHS | Gurobi | Gurobi, network simplex |")
    print("|---|---:|---:|---:|---:|---:|---:|")

    for name, network in rows():
        if not name.startswith(prefix):
            continue

        ours = KahlnwsSolve(network)
        others: list[ColdSolve] = [
            HighsSolve(network),
            GurobiSolve(network, environment, network_simplex=False),
            GurobiSolve(network, environment, network_simplex=True),
        ]
        cost = ours.solve()
        our_time = median_milliseconds(ours.solve, cost)
        cells = [f"{our_time:.3f} ms"]

        for other in others:
            their_time = median_milliseconds(other.solve, cost)
            cells.append(f"{their_time:.3f} ms ({their_time / our_time:.1f}x)")

        print(f"| `{name}` | {network.node_count:,} | {network.arc_count:,} | {' | '.join(cells)} |", flush=True)


if __name__ == "__main__":
    main()
