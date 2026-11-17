# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""One solver, and one network, serves one call at a time: a second call on it, from the same thread or another, raises
instead of corrupting it, and so does a read of a network's optimum; the solver or the network answers as before
afterwards."""

import threading
from collections import UserList
from collections.abc import Callable

import pytest

import kahlnws
from networks import DEPOT, DEPOT_FLOWS

# How long the test of two threads waits for the other thread at most, in seconds: far more than a depot takes, and a
# bound, so that a regression that blocks fails the test instead of hanging the suite.
PATIENCE = 30.0


class Tails(UserList[int]):
    """The depot's tails as a sequence whose length runs `measured` first, inside the solver's call."""

    def __init__(self, measured: Callable[[], None]) -> None:
        super().__init__(DEPOT.tails)
        self.measured = measured

    def __len__(self) -> int:
        self.measured()
        return super().__len__()


def solve_depot(solver: kahlnws.NetworkSimplex, tails: Tails) -> kahlnws.Solution | None:
    """What `solver` finds for the depot read with `tails`."""
    return solver.solve(
        tails=tails, heads=DEPOT.heads, capacities=DEPOT.capacities, costs=DEPOT.costs, supplies=DEPOT.supplies
    ).outcome


def load_depot(network: kahlnws.Network, tails: Tails) -> None:
    """Loads the depot, read with `tails`, into `network`."""
    network.load(
        tails=tails, heads=DEPOT.heads, capacities=DEPOT.capacities, costs=DEPOT.costs, supplies=DEPOT.supplies
    )


def solved_flows(network: kahlnws.Network) -> list[int] | None:
    """The flows `network` solves to; `None` where no flow exists."""
    network.solve()
    optimum = network.optimum()
    return None if optimum is None else optimum.flows


def test_a_call_from_code_a_column_runs_while_it_is_read_raises() -> None:
    solver = kahlnws.NetworkSimplex()
    raised: list[BaseException] = []

    def measured() -> None:
        try:
            solver.reserve(1, 1)
        except RuntimeError as error:
            raised.append(error)

    solution = solve_depot(solver, Tails(measured))

    assert [str(error) for error in raised] == ["the solver is already in a call"]
    assert solution is not None
    assert solution.flows == DEPOT_FLOWS


def test_a_call_from_another_thread_while_one_runs_raises() -> None:
    # The one test that starts a thread: its call reads a column whose code waits until the call made here meanwhile
    # has raised. Every wait is bounded, and the thread is a daemon, so a regression fails here and hangs nothing.
    solver = kahlnws.NetworkSimplex()
    in_call, second_done = threading.Event(), threading.Event()
    answers: list[kahlnws.Solution | None] = []

    def measured() -> None:
        in_call.set()
        second_done.wait(PATIENCE)

    first = threading.Thread(target=lambda: answers.append(solve_depot(solver, Tails(measured))), daemon=True)
    first.start()
    try:
        assert in_call.wait(PATIENCE), "the other thread should reach its call"
        with pytest.raises(RuntimeError, match=r"^the solver is already in a call$"):
            DEPOT.solve(solver)
    finally:
        second_done.set()
        first.join(PATIENCE)
    assert not first.is_alive(), "the other thread's call should return"

    assert len(answers) == 1
    first_answer = answers[0]
    assert first_answer is not None
    assert first_answer.flows == DEPOT_FLOWS

    after = DEPOT.solve(solver)
    assert after is not None
    assert after.flows == DEPOT_FLOWS


def test_a_call_on_a_network_from_code_a_column_runs_while_it_is_read_raises() -> None:
    network = kahlnws.Network()
    raised: list[BaseException] = []

    def measured() -> None:
        try:
            network.reserve(1, 1)
        except RuntimeError as error:
            raised.append(error)

    load_depot(network, Tails(measured))

    assert [str(error) for error in raised] == ["the network is already in a call"]
    assert solved_flows(network) == DEPOT_FLOWS


def test_a_call_on_a_network_and_a_read_of_its_optimum_from_another_thread_while_one_runs_raise() -> None:
    # As for the solver: the other thread's load reads a column whose code waits until the calls made here meanwhile
    # have raised. Every wait is bounded, and the thread is a daemon.
    network = DEPOT.loaded()
    network.solve()
    optimum = network.optimum()
    assert optimum is not None
    in_call, second_done = threading.Event(), threading.Event()

    def measured() -> None:
        in_call.set()
        second_done.wait(PATIENCE)

    first = threading.Thread(target=lambda: load_depot(network, Tails(measured)), daemon=True)
    first.start()
    try:
        assert in_call.wait(PATIENCE), "the other thread should reach its call"
        with pytest.raises(RuntimeError, match=r"^the network is already in a call$"):
            network.solve()
        with pytest.raises(RuntimeError, match=r"^the network is already in a call$"):
            _ = optimum.flows
    finally:
        second_done.set()
        first.join(PATIENCE)
    assert not first.is_alive(), "the other thread's call should return"

    assert solved_flows(network) == DEPOT_FLOWS
    with pytest.raises(RuntimeError, match=r"^the network changed since this optimum was answered$"):
        _ = optimum.flows
