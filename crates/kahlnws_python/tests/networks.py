# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Small networks the tests solve, held as plain lists, the entries that solve them, and the check of an answer against
its network."""

from array import array
from collections.abc import Callable
from dataclasses import dataclass
from typing import TypeAlias

import kahlnws

# An entry of the high level, called on a solver with the columns by keyword.
Solve: TypeAlias = Callable[..., kahlnws.Answer[kahlnws.Solution]]

# An entry of the low level, called on a solver with the columns and the outputs by keyword.
SolveInto: TypeAlias = Callable[..., kahlnws.Answer[int]]

# The two entries of each level: the one that starts from the problem alone, and the one that starts from the basis the
# solver kept where it serves. A test of what both entries of a level share runs once for each.
HIGH_LEVEL: list[Solve] = [kahlnws.NetworkSimplex.solve, kahlnws.NetworkSimplex.resolve]
LOW_LEVEL: list[SolveInto] = [kahlnws.NetworkSimplex.solve_into, kahlnws.NetworkSimplex.resolve_into]

# The entries of the two levels in pairs, by the problem alone and from the basis kept, for a test of both levels.
ENTRY_PAIRS: list[tuple[Solve, SolveInto]] = list(zip(HIGH_LEVEL, LOW_LEVEL, strict=True))


def named(entry: Callable[..., object]) -> str:
    """The name of `entry`, which a test run once for each entry is told apart by."""
    return entry.__name__


@dataclass(frozen=True)
class Network:
    """One min-cost flow problem as plain lists: arc `a` at position `a` of every per-arc list, node `v` at `v`."""

    tails: list[int]
    heads: list[int]
    capacities: list[int]
    costs: list[int]
    supplies: list[int]
    lower_bounds: list[int] | None = None

    def answer(
        self, solver: kahlnws.NetworkSimplex, entry: Solve = kahlnws.NetworkSimplex.solve
    ) -> kahlnws.Answer[kahlnws.Solution]:
        """What `solver` answers by `entry` for the network handed in as lists."""
        return entry(
            solver,
            tails=self.tails,
            heads=self.heads,
            capacities=self.capacities,
            costs=self.costs,
            supplies=self.supplies,
            lower_bounds=self.lower_bounds,
        )

    def solve(
        self, solver: kahlnws.NetworkSimplex, entry: Solve = kahlnws.NetworkSimplex.solve
    ) -> kahlnws.Solution | None:
        """The solution `solver` finds by `entry` for the network handed in as lists; `None` where it is infeasible."""
        return self.answer(solver, entry).outcome

    def answer_into(
        self,
        solver: kahlnws.NetworkSimplex,
        flows: "array[int]",
        potentials: "array[int] | None" = None,
        entry: SolveInto = kahlnws.NetworkSimplex.solve_into,
    ) -> kahlnws.Answer[int]:
        """What `solver` answers by `entry` for the network handed in as lists, written into `flows` and
        `potentials`."""
        return entry(
            solver,
            tails=self.tails,
            heads=self.heads,
            capacities=self.capacities,
            costs=self.costs,
            supplies=self.supplies,
            flows=flows,
            lower_bounds=self.lower_bounds,
            potentials=potentials,
        )

    def check(self, flows: list[int], potentials: list[int], total_cost: int) -> None:
        """Asserts that `flows` is a flow of the network of cost `total_cost` and that `potentials` prove it optimal.

        The flow keeps every bound and meets every supply; the cost is the sum of each flow times its cost; and the
        reduced cost of every arc below its capacity is at least zero, of every arc above its lower bound at most zero.
        """
        assert len(flows) == len(self.tails)
        assert len(potentials) == len(self.supplies)
        arcs = [self.arc(position, flow) for position, flow in enumerate(flows)]

        excess = list(self.supplies)
        for arc in arcs:
            assert arc.lower_bound <= arc.flow <= arc.capacity
            excess[arc.tail] -= arc.flow
            excess[arc.head] += arc.flow
        assert excess == [0] * len(self.supplies), "every supply is met"

        assert total_cost == sum(arc.flow * arc.cost for arc in arcs)

        for arc in arcs:
            reduced = arc.cost + potentials[arc.tail] - potentials[arc.head]
            assert arc.flow == arc.capacity or reduced >= 0, (
                "an arc below its capacity costs no less than the path back"
            )
            assert arc.flow == arc.lower_bound or reduced <= 0, (
                "an arc above its lower bound costs no more than the way back"
            )

    def arc(self, position: int, flow: int) -> "Arc":
        """The arc at `position` of the network, carrying `flow`."""
        lower_bound = 0 if self.lower_bounds is None else self.lower_bounds[position]
        return Arc(
            self.tails[position],
            self.heads[position],
            lower_bound,
            self.capacities[position],
            self.costs[position],
            flow,
        )


@dataclass(frozen=True)
class Arc:
    """One arc of a network with the flow it carries: its two ends, its bounds, its cost."""

    tail: int
    head: int
    lower_bound: int
    capacity: int
    cost: int
    flow: int


# A depot (node 0) sends 4 units to a customer (node 2), directly or through a hub (node 1).
DEPOT = Network(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4])

# The depot's flows, cost and potentials, worked by hand: the hub's path carries its 2 units at cost 2 each, the direct
# arc the other 2 at cost 5. Arcs 0 and 1 lie strictly within their bounds, so their reduced costs are zero, which fixes
# the potentials up to a shift; the solver puts node 0 at zero.
DEPOT_FLOWS = [2, 2, 2]
DEPOT_COST = 14
DEPOT_POTENTIALS = [0, 1, 5]

# Node 0 sends 2 units to node 1 over one arc of capacity 1: no flow meets the supplies.
CUT = Network(tails=[0], heads=[1], capacities=[1], costs=[1], supplies=[2, -2])
