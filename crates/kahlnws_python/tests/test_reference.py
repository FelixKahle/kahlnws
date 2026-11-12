# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Many tiny problems drawn from a seeded generator, each solved through both levels and compared with a brute force
that tries every flow: whether a flow exists, its least cost, and the potentials' proof of the flow answered; and the
work, the same at both levels and within its relations."""

import itertools
import random
from array import array

import pytest

import kahlnws
from networks import Network

# The number of problems drawn, and their bounds: at most 3 nodes and 4 arcs, each arc with at most 3 flows to try, so
# the brute force tries at most `3**4 = 81` flows a problem.
PROBLEMS = 400
MOST_NODES = 3
MOST_ARCS = 4
WIDEST = 2


def draw(generator: random.Random) -> Network:
    """A tiny problem: loops and parallel arcs allowed, lower bounds given or not, supplies that balance or not."""
    node_count = generator.randint(1, MOST_NODES)
    arc_count = generator.randint(0, MOST_ARCS)
    tails = [generator.randrange(node_count) for _ in range(arc_count)]
    heads = [generator.randrange(node_count) for _ in range(arc_count)]
    lower_bounds = [generator.randint(0, 1) for _ in range(arc_count)]
    capacities = [lower_bound + generator.randint(0, WIDEST) for lower_bound in lower_bounds]
    costs = [generator.randint(-3, 3) for _ in range(arc_count)]

    # One problem in ten is off balance by one unit, which every solve refuses.
    supplies = [generator.randint(-2, 2) for _ in range(node_count - 1)]
    supplies.append(-sum(supplies) + (1 if generator.random() < 0.1 else 0))

    given = generator.random() < 0.5 or any(lower_bounds)
    return Network(tails, heads, capacities, costs, supplies, lower_bounds=lower_bounds if given else None)


def least_cost(network: Network) -> int | None:
    """The least cost of a flow of `network`, found by trying every flow within the bounds; `None` where none meets the
    supplies."""
    lower_bounds = network.lower_bounds or [0] * len(network.tails)
    ranges = [range(lower_bound, capacity + 1) for lower_bound, capacity in zip(lower_bounds, network.capacities)]
    best: int | None = None

    for flows in itertools.product(*ranges):
        excess = list(network.supplies)
        for tail, head, flow in zip(network.tails, network.heads, flows):
            excess[tail] -= flow
            excess[head] += flow
        if any(excess):
            continue

        cost = sum(flow * cost for flow, cost in zip(flows, network.costs))
        best = cost if best is None else min(best, cost)

    return best


def test_tiny_problems_through_both_levels_against_the_brute_force() -> None:
    generator = random.Random(20261112)
    solver = kahlnws.NetworkSimplex()
    feasible = pivoted = degenerate = refused = 0

    for _ in range(PROBLEMS):
        network = draw(generator)
        best = least_cost(network)

        total = sum(network.supplies)
        if total != 0:
            refused += 1
            with pytest.raises(kahlnws.ProblemError) as raised:
                network.answer(solver)
            assert raised.value.breach == kahlnws.Breach.Unbalanced(total=total), network
            continue

        high = network.answer(solver)
        solution = high.outcome
        flows, potentials = array("Q", [0] * len(network.tails)), array("q", [0] * len(network.supplies))
        low = solver.solve_into(
            tails=array("Q", network.tails),
            heads=array("Q", network.heads),
            capacities=array("Q", network.capacities),
            costs=array("q", network.costs),
            supplies=array("q", network.supplies),
            flows=flows,
            lower_bounds=None if network.lower_bounds is None else array("Q", network.lower_bounds),
            potentials=potentials,
        )
        total_cost = low.outcome

        work = high.work
        assert low.work == work, network
        assert work.degenerate_pivots <= work.pivots, network
        assert work.first_arc_pivots <= work.pivots, network
        pivoted += work.pivots > 0
        degenerate += work.degenerate_pivots > 0

        if best is None:
            assert solution is None, network
            assert total_cost is None, network
            continue

        feasible += 1
        assert solution is not None, network
        assert solution.total_cost == total_cost == best, network
        assert (flows.tolist(), potentials.tolist()) == (solution.flows, solution.potentials), network
        network.check(solution.flows, solution.potentials, solution.total_cost)

    # The draw reaches every answer often, refusals among them, and pivots of both kinds.
    assert PROBLEMS // 4 < feasible < PROBLEMS * 3 // 4, feasible
    assert PROBLEMS // 20 < refused < PROBLEMS // 5, refused
    assert pivoted > PROBLEMS // 4, pivoted
    assert degenerate > 0, degenerate
