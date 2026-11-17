# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The examples under `examples/`: each runs in a fresh interpreter and prints what it shows, its optimal cost worked
out by hand. The one that needs NumPy is skipped where NumPy is not installed."""

import importlib.util
import subprocess
import sys
from pathlib import Path

import pytest

# The folder of the examples, beside the tests.
EXAMPLES = Path(__file__).parents[1] / "examples"

# The optimum of the transport network, worked out by hand: a lane from warehouse 1 costs 1, 6 and 3 more than the lane
# from warehouse 0 to shops 0, 1 and 2. Every unit sent from warehouse 1 would cost 9 * 10 + 12 * 25 + 13 * 15 = 585;
# each unit warehouse 0 sends instead saves the difference, and its 20 units save the most, 6 * 20, all to shop 1,
# whose lane carries 20. Warehouse 1 sends the rest, 10, 5 and 15, within its lanes.
TRANSPORT_COST = 585 - 6 * 20


def run(name: str) -> str:
    """What the example `name` prints, after asserting that it ran to its end."""
    finished = subprocess.run([sys.executable, EXAMPLES / f"{name}.py"], capture_output=True, text=True, check=False)

    assert finished.returncode == 0, finished.stderr
    return finished.stdout


def test_transport_prints_the_hand_worked_optimum_and_its_flow() -> None:
    assert run("transport").splitlines() == [
        f"cost: {TRANSPORT_COST}",
        "warehouse 0 -> shop 0: 0",
        "warehouse 0 -> shop 1: 20",
        "warehouse 0 -> shop 2: 0",
        "warehouse 1 -> shop 0: 10",
        "warehouse 1 -> shop 1: 5",
        "warehouse 1 -> shop 2: 15",
    ]


@pytest.mark.skipif(importlib.util.find_spec("numpy") is None, reason="the example needs NumPy")
def test_reuse_prints_the_hand_worked_optimum_of_each_day() -> None:
    # Everything from warehouse 1 would cost 585, 550 and 615. Days 1 and 2 send warehouse 0's 20 units to shop 1 and
    # save 120. On day 3 shop 2 asks for 30 where a lane carries 20, so warehouse 0 sends it at least 10 and shop 1 the
    # other 10, saving 3 * 10 + 6 * 10.
    assert run("reuse").splitlines() == [
        f"day 1: cost {TRANSPORT_COST}",
        f"day 2: cost {550 - 120}",
        f"day 3: cost {615 - 90}",
    ]


def test_network_prints_the_optimum_of_each_day_and_fewer_pivots_from_the_basis_it_kept() -> None:
    # The costs are those of `reuse`; the pivots are those the network's solve and a fresh solve report, which the
    # example checks.
    assert run("network").splitlines() == [
        f"day 1: cost {TRANSPORT_COST}, pivots 6 (a fresh solve: 6)",
        f"day 2: cost {550 - 120}, pivots 1 (a fresh solve: 5)",
        f"day 3: cost {615 - 90}, pivots 1 (a fresh solve: 7)",
    ]


def test_search_prints_what_each_move_came_to_worked_out_by_hand() -> None:
    # The example works out every cost by hand: a unit warehouse 0 sends saves 1, 6 and 3 at shops 0, 1 and 2 on what
    # warehouse 1 would pay alone, 585 while the costs are those loaded and 460 once lane 4 costs 7. Move 1 leaves
    # warehouse 0 10 units for shop 1; move 2 is refused at its second change, and its first is set back; move 3 gives
    # warehouse 0 five units more, which save 3 each at shop 2; move 4 leaves it 15, all for shop 1.
    assert run("search").splitlines() == [
        f"start: cost {TRANSPORT_COST}",
        f"move 1: cost {585 - 10 - 6 * 10}, set back",
        "move 2: refused (arc 1 has lower bound 25 above its capacity 20), set back",
        f"move 3: cost {TRANSPORT_COST - 3 * 5}, kept",
        f"move 4: cost {585 - 6 * 15}, set back",
        f"move 5: cost {460 - 3 * 15 - 10}, kept",
        f"end: cost {460 - 3 * 15 - 10}, the warehouses hold 25 and 25",
    ]


def test_outcomes_prints_that_no_flow_exists_and_the_refusal() -> None:
    # Warehouse 0's three lanes cut to 5 carry 15 of its 20 units; the last lane enters node 5 of five nodes.
    assert run("outcomes").splitlines() == [
        "no flow exists",
        "refused: arc 5 enters node 5, not below the node count 5",
    ]


def test_potentials_prints_potentials_that_prove_the_hand_worked_optimum() -> None:
    printed = run("potentials").splitlines()

    assert printed[0] == f"cost: {TRANSPORT_COST}"
    assert printed[1:6] == [
        "potential of warehouse 0: 3",
        "potential of warehouse 1: 0",
        "potential of shop 0: 9",
        "potential of shop 1: 12",
        "potential of shop 2: 13",
    ]
    # Each reduced cost by hand from those potentials: cost + potential of the warehouse - potential of the shop.
    assert printed[6:12] == [
        "reduced cost of warehouse 0 -> shop 0: 2",
        "reduced cost of warehouse 0 -> shop 1: -3",
        "reduced cost of warehouse 0 -> shop 2: 0",
        "reduced cost of warehouse 1 -> shop 0: 0",
        "reduced cost of warehouse 1 -> shop 1: 0",
        "reduced cost of warehouse 1 -> shop 2: 0",
    ]
    assert printed[12:] == [
        "the optimality conditions hold",
        "pivots: 6",
        "degenerate pivots: 1",
        "first arc pivots: 2",
    ]
