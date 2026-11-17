# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Many tiny problems drawn from a seeded generator, each solved through both levels and compared with a brute force
that tries every flow: whether a flow exists, its least cost, and the potentials' proof of the flow answered; and the
work, the same at both levels and within its relations. Problems on the same arcs are loaded in turn into one network,
each loaded again after its solve; and each problem is held by a network, changed at random, solved after each change,
and set back."""

import dataclasses
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

    return numbered(generator, node_count, tails, heads)


def numbered(generator: random.Random, node_count: int, tails: list[int], heads: list[int]) -> Network:
    """A tiny problem on `node_count` nodes and the arcs of `tails` and `heads`, its numbers drawn as `draw` draws
    them."""
    arc_count = len(tails)
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


def answered(network: Network, solver: kahlnws.NetworkSimplex, best: int | None) -> tuple[kahlnws.Work, kahlnws.Work]:
    """The work `solver` answers for the balanced `network` by `solve` and then by `solve_into`, its columns handed in
    as buffers, after asserting that each answers the least cost `best`, or `None`, that the high level's potentials
    prove its flow, and that the low level writes the same columns."""
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

    for work in [high.work, low.work]:
        assert work.degenerate_pivots <= work.pivots, network
        assert work.first_arc_pivots <= work.pivots, network

    if best is None:
        assert solution is None, network
        assert total_cost is None, network
    else:
        assert solution is not None, network
        assert solution.total_cost == total_cost == best, network
        assert (flows.tolist(), potentials.tolist()) == (solution.flows, solution.potentials), network
        network.check(solution.flows, solution.potentials, solution.total_cost)

    return high.work, low.work


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

        work, work_into = answered(network, solver, best)
        assert work_into == work, network
        pivoted += work.pivots > 0
        degenerate += work.degenerate_pivots > 0
        feasible += best is not None

    # The draw reaches every answer often, refusals among them, and pivots of both kinds.
    assert PROBLEMS // 4 < feasible < PROBLEMS * 3 // 4, feasible
    assert PROBLEMS // 20 < refused < PROBLEMS // 5, refused
    assert pivoted > PROBLEMS // 4, pivoted
    assert degenerate > 0, degenerate


def test_tiny_problems_on_the_same_arcs_loaded_in_turn_into_one_network_against_the_brute_force() -> None:
    # Each problem drawn is followed by one on its arcs with numbers drawn anew, from a generator of its own, so the
    # problems drawn are those above. One network loads each in turn and solves it, and then loads it again: the same
    # problem again, from the basis the solve ended on, which makes no pivot.
    generator, renumbering = random.Random(20261112), random.Random(20261115)
    network = kahlnws.Network()
    loaded = kept = 0

    for _ in range(PROBLEMS):
        drawn = draw(generator)
        renumbered = numbered(renumbering, len(drawn.supplies), drawn.tails, drawn.heads)

        for problem in [drawn, renumbered]:
            total = sum(problem.supplies)
            if total != 0:
                with pytest.raises(kahlnws.ProblemError) as raised:
                    network.load(**dataclasses.asdict(problem))
                assert raised.value.breach == kahlnws.Breach.Unbalanced(total=total), problem
                continue

            network.load(**dataclasses.asdict(problem))
            answer = network.solve()
            best = least_cost(problem)
            if best is None:
                assert answer.outcome is None, problem
            else:
                optimum = network.optimum()
                assert optimum is not None, problem
                assert answer.outcome == optimum.total_cost == best, problem
                problem.check(optimum.flows, optimum.potentials, optimum.total_cost)
            work = answer.work

            network.load(**dataclasses.asdict(problem))
            assert network.solve().work == kahlnws.Work(0, 0, 0), problem
            loaded += 1
            # A load that cannot use the basis held answers exactly what a fresh network answers, its work included:
            # a work that differs proves a start from the basis kept.
            fresh = problem.loaded()
            kept += work != fresh.solve().work

    # Most problems are loaded, and many of them onto the basis kept.
    assert loaded > PROBLEMS, loaded
    assert kept > PROBLEMS // 10, kept


# The changes made to each network drawn, and the most a capacity, a lower bound and an amount of supply drawn for one:
# with at most 4 arcs, each of at most 4 flows to try, the brute force tries at most `4**4 = 256` flows a problem.
CHANGES = 6
MOST_CAPACITY = 3
MOST_LOWER_BOUND = 2
MOST_AMOUNT = 2


def bounds_set(kind: str, network: kahlnws.Network, arc: int, lower_bound: int, capacity: int) -> object:
    """What the setter of `kind` answers on `network` for `arc`: `set_capacity` for a capacity, `set_lower_bound` for a
    lower bound, `set_bounds` for both, each handed its own numbers of `lower_bound` and `capacity`."""
    match kind:
        case "capacity":
            return network.set_capacity(arc, capacity)
        case "lower bound":
            return network.set_lower_bound(arc, lower_bound)
        case _:
            return network.set_bounds(arc, kahlnws.Bounds(lower_bound, capacity))


def change_at_random(generator: random.Random, network: kahlnws.Network, problem: Network) -> tuple[Network, object]:
    """Makes one change drawn from `generator` on `network`, which holds `problem`, after asserting that the setter
    answers the number it replaced or refuses as it should; answers the problem the network then holds and what
    `set_back` sets the change back by."""
    lower_bounds = list(problem.lower_bounds or [0] * len(problem.tails))
    capacities, costs, supplies = list(problem.capacities), list(problem.costs), list(problem.supplies)
    kind = generator.choice(["cost", "capacity", "lower bound", "bounds", "supply"] if problem.tails else ["supply"])
    arc = generator.randrange(len(problem.tails)) if problem.tails else 0

    if kind == "supply":
        from_node, to_node = generator.randrange(len(supplies)), generator.randrange(len(supplies))
        amount = generator.randint(0, MOST_AMOUNT)
        network.move_supply(from_node, to_node, amount)
        supplies[from_node] -= amount
        supplies[to_node] += amount
        back: object = ("supply", to_node, from_node, amount)
    elif kind == "cost":
        cost = generator.randint(-3, 3)
        assert network.set_cost(arc, cost) == costs[arc], problem
        back, costs[arc] = ("cost", arc, costs[arc]), cost
    else:
        # A single setter keeps the other bound the arc holds.
        lower_bound = lower_bounds[arc] if kind == "capacity" else generator.randint(0, MOST_LOWER_BOUND)
        capacity = capacities[arc] if kind == "lower bound" else generator.randint(0, MOST_CAPACITY)
        if lower_bound > capacity:
            with pytest.raises(kahlnws.ChangeError) as raised:
                bounds_set(kind, network, arc, lower_bound, capacity)
            assert raised.value.breach == kahlnws.ChangeBreach.LowerAboveCapacity(arc, lower_bound, capacity), problem
            return problem, ("refused",)

        had = {"capacity": capacities[arc], "lower bound": lower_bounds[arc]}.get(
            kind, (lower_bounds[arc], capacities[arc])
        )
        assert bounds_set(kind, network, arc, lower_bound, capacity) == had, problem
        back = ("bounds", arc, lower_bounds[arc], capacities[arc])
        lower_bounds[arc], capacities[arc] = lower_bound, capacity

    return Network(problem.tails, problem.heads, capacities, costs, supplies, lower_bounds=lower_bounds), back


def set_back(network: kahlnws.Network, back: object) -> None:
    """Sets back on `network` the change `change_at_random` answered `back` for."""
    match back:
        case ("supply", int(from_node), int(to_node), int(amount)):
            network.move_supply(from_node, to_node, amount)
        case ("cost", int(arc), int(cost)):
            network.set_cost(arc, cost)
        case ("bounds", int(arc), int(lower_bound), int(capacity)):
            network.set_bounds(arc, (lower_bound, capacity))
        case ("refused",):
            pass
        case other:
            pytest.fail(f"no change sets back {other!r}")


def test_tiny_networks_changed_at_random_and_set_back_against_the_brute_force() -> None:
    # Each balanced problem drawn is loaded into a network, solved, and changed `CHANGES` times, each change drawn from
    # a generator of its own and solved, against the brute force and a fresh solve of the problem as it then stands;
    # then every change is set back, last first, and the network solves to the first cost again.
    generator, changing = random.Random(20261112), random.Random(20261116)
    loaded = changed = refused = kept = 0

    for _ in range(PROBLEMS):
        problem = draw(generator)
        if sum(problem.supplies) != 0:
            continue
        network = problem.loaded()
        first_cost = network.solve().outcome
        assert first_cost == least_cost(problem), problem
        loaded += 1

        backs: list[object] = []
        for _ in range(CHANGES):
            problem, back = change_at_random(changing, network, problem)
            backs.append(back)
            refused += back == ("refused",)

            answer, best = network.solve(), least_cost(problem)
            fresh = problem.solve(kahlnws.NetworkSimplex())
            if best is None:
                assert answer.outcome is None, problem
                assert fresh is None, problem
                continue
            optimum = network.optimum()
            assert optimum is not None, problem
            assert fresh is not None, problem
            assert answer.outcome == optimum.total_cost == fresh.total_cost == best, problem
            problem.check(optimum.flows, optimum.potentials, optimum.total_cost)
            changed += 1
            # A network loaded with the problem as it stands solves it from the star; one that makes no pivot where
            # that one makes some started from the basis the change left.
            kept += answer.work.pivots == 0 < problem.loaded().solve().work.pivots

        for back in reversed(backs):
            set_back(network, back)
        assert network.solve().outcome == first_cost, problem

    # The draw reaches feasible changes often, refusals among the changes, and solves from the basis a change left.
    assert loaded > PROBLEMS * 3 // 4, loaded
    assert changed > loaded * CHANGES // 4, changed
    assert refused > 0, refused
    assert kept > loaded // 4, kept
