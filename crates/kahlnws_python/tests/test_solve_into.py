# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The low level, `NetworkSimplex.solve_into`: the buffers it writes, the ones it refuses, and what it leaves alone."""

import ctypes
import sys
from array import array

import pytest

import kahlnws
from networks import CUT, DEPOT, DEPOT_COST, DEPOT_FLOWS, DEPOT_POTENTIALS


def test_the_depot_into_arrays_of_the_standard_library() -> None:
    solver = kahlnws.NetworkSimplex()
    solver.reserve(3, 3)
    flows, potentials = array("Q", [7, 7, 7]), array("q", [7, 7, 7])

    total_cost = solver.solve_into(
        tails=DEPOT.tails,
        heads=DEPOT.heads,
        capacities=DEPOT.capacities,
        costs=DEPOT.costs,
        supplies=DEPOT.supplies,
        flows=flows,
        potentials=potentials,
    ).outcome

    assert total_cost == DEPOT_COST
    assert type(total_cost) is int
    assert flows.tolist() == DEPOT_FLOWS
    assert potentials.tolist() == DEPOT_POTENTIALS


def test_the_flows_alone_and_through_a_memoryview_and_a_bytearray() -> None:
    solver = kahlnws.NetworkSimplex()
    room = bytearray(3 * 8)
    flows = memoryview(room).cast("Q")

    total_cost = solver.solve_into(
        tails=DEPOT.tails,
        heads=DEPOT.heads,
        capacities=DEPOT.capacities,
        costs=DEPOT.costs,
        supplies=DEPOT.supplies,
        flows=flows,
    ).outcome

    assert total_cost == DEPOT_COST
    assert flows.tolist() == DEPOT_FLOWS
    assert array("Q", bytes(room)).tolist() == DEPOT_FLOWS, "written into the memory the view shows"


def test_one_solver_writes_problem_after_problem() -> None:
    solver = kahlnws.NetworkSimplex()
    flows = array("Q", [0, 0, 0])

    for _ in range(3):
        flows[:] = array("Q", [0, 0, 0])
        assert (
            solver.solve_into(
                tails=DEPOT.tails,
                heads=DEPOT.heads,
                capacities=DEPOT.capacities,
                costs=DEPOT.costs,
                supplies=DEPOT.supplies,
                flows=flows,
            ).outcome
            == 14
        )
        assert flows.tolist() == DEPOT_FLOWS


def test_a_buffer_that_is_also_a_column_is_read_before_it_is_written() -> None:
    # The capacities' own buffer receives the flows: the solve reads the capacities 3, 4, 2 before it writes 2, 2, 2.
    shared = array("Q", DEPOT.capacities)

    total_cost = (
        kahlnws.NetworkSimplex()
        .solve_into(
            tails=DEPOT.tails,
            heads=DEPOT.heads,
            capacities=shared,
            costs=DEPOT.costs,
            supplies=DEPOT.supplies,
            flows=shared,
        )
        .outcome
    )

    assert total_cost == DEPOT_COST
    assert shared.tolist() == DEPOT_FLOWS


def test_an_infeasible_problem_answers_none_and_leaves_the_buffers_as_they_were() -> None:
    flows, potentials = array("Q", [7]), array("q", [8, 9])

    total_cost = (
        kahlnws.NetworkSimplex()
        .solve_into(
            tails=CUT.tails,
            heads=CUT.heads,
            capacities=CUT.capacities,
            costs=CUT.costs,
            supplies=CUT.supplies,
            flows=flows,
            potentials=potentials,
        )
        .outcome
    )

    assert total_cost is None
    assert (flows.tolist(), potentials.tolist()) == ([7], [8, 9])


def test_a_refused_problem_leaves_the_buffers_as_they_were() -> None:
    flows, potentials = array("Q", [7, 7, 7]), array("q", [8, 8, 8])

    with pytest.raises(kahlnws.ProblemError):
        kahlnws.NetworkSimplex().solve_into(
            tails=DEPOT.tails,
            heads=DEPOT.heads,
            capacities=DEPOT.capacities,
            costs=DEPOT.costs,
            supplies=[4, 0, -3],
            flows=flows,
            potentials=potentials,
        )

    assert (flows.tolist(), potentials.tolist()) == ([7, 7, 7], [8, 8, 8])


def test_a_column_that_cannot_be_read_leaves_the_buffers_as_they_were() -> None:
    flows = array("Q", [7, 7, 7])

    with pytest.raises(OverflowError):
        kahlnws.NetworkSimplex().solve_into(
            tails=DEPOT.tails,
            heads=DEPOT.heads,
            capacities=[3, -4, 2],
            costs=DEPOT.costs,
            supplies=DEPOT.supplies,
            flows=flows,
        )

    assert flows.tolist() == [7, 7, 7]


def solve_depot_into(flows: object, potentials: object = None) -> int | None:
    """What a fresh solver answers for the depot written into `flows` and `potentials`, of any type."""
    # The outputs are of any type here, so that the ones a solve refuses can be handed in.
    rooms: dict[str, object] = {"flows": flows, "potentials": potentials}
    return (
        kahlnws.NetworkSimplex()
        .solve_into(
            tails=DEPOT.tails,
            heads=DEPOT.heads,
            capacities=DEPOT.capacities,
            costs=DEPOT.costs,
            supplies=DEPOT.supplies,
            **rooms,  # type: ignore[arg-type]  # see above
        )
        .outcome
    )


def test_an_output_that_is_no_buffer_is_refused_with_its_type() -> None:
    with pytest.raises(TypeError, match=r"^flows is of type list, not a writable buffer$"):
        solve_depot_into([0, 0, 0])
    with pytest.raises(TypeError, match=r"^potentials is of type tuple, not a writable buffer$"):
        solve_depot_into(array("Q", [0, 0, 0]), (0, 0, 0))


@pytest.mark.parametrize("type_code", ["q", "B", "I", "d"])
def test_flows_of_other_items_than_unsigned_64_bit_are_refused(type_code: str) -> None:
    flows = array(type_code, [0, 0, 0])

    with pytest.raises(TypeError, match=rf"^flows holds items of format '{type_code}', not unsigned 64-bit integers$"):
        solve_depot_into(flows)

    assert flows.tolist() == [0, 0, 0]


def test_potentials_of_other_items_than_signed_64_bit_are_refused() -> None:
    flows, potentials = array("Q", [7, 7, 7]), array("Q", [0, 0, 0])

    with pytest.raises(TypeError, match=r"^potentials holds items of format 'Q', not signed 64-bit integers$"):
        solve_depot_into(flows, potentials)

    assert flows.tolist() == [7, 7, 7], "the outputs are checked before the solve writes either"


def test_a_read_only_output_is_refused() -> None:
    read_only = memoryview(bytes(3 * 8)).cast("Q")

    with pytest.raises(ValueError, match=r"^flows is read-only$"):
        solve_depot_into(read_only)


def test_an_output_of_another_length_is_refused_with_both_lengths() -> None:
    with pytest.raises(ValueError, match=r"^flows has length 2, not the arc count 3$"):
        solve_depot_into(array("Q", [0, 0]))
    with pytest.raises(ValueError, match=r"^potentials has length 4, not the node count 3$"):
        solve_depot_into(array("Q", [0, 0, 0]), array("q", [0, 0, 0, 0]))


def test_an_output_of_two_dimensions_is_refused() -> None:
    square = memoryview(bytearray(4 * 8)).cast("Q", (2, 2))

    with pytest.raises(ValueError, match=r"^flows has 2 dimensions, not 1$"):
        solve_depot_into(square)


def test_an_output_of_the_wrong_length_is_refused_before_a_breach_of_the_problem() -> None:
    # The supplies do not balance, and the flows are one short: the buffer is answered first, as the library does.
    with pytest.raises(ValueError, match=r"^flows has length 1, not the arc count 2$"):
        kahlnws.NetworkSimplex().solve_into(
            tails=[0, 0], heads=[1, 1], capacities=[1, 1], costs=[1, 1], supplies=[1, 0], flows=array("Q", [0])
        )


def test_an_empty_output_takes_the_flows_of_a_problem_of_no_arc() -> None:
    # An exporter may point an empty buffer at memory not aligned for its items, as `array.array` does on some
    # versions: nothing is written to it, so it is not refused for that.
    flows, potentials = array("Q"), array("q", [9])
    problem = {"tails": array("Q"), "heads": [], "capacities": (), "costs": [], "supplies": [0]}

    answer = kahlnws.NetworkSimplex().solve_into(**problem, flows=flows, potentials=potentials)
    solution = kahlnws.NetworkSimplex().solve(**problem).outcome

    assert answer.outcome == 0
    assert solution is not None
    assert (flows.tolist(), potentials.tolist()) == ([], solution.potentials)


def test_outputs_whose_format_names_the_machine_s_byte_order_or_whose_items_are_not_aligned_are_written() -> None:
    native = getattr(ctypes.c_uint64, "__ctype_le__" if sys.byteorder == "little" else "__ctype_be__")
    flows = (native * 3)()
    potentials = memoryview(bytearray(25))[1:].cast("q")

    assert solve_depot_into(flows, potentials) == DEPOT_COST
    assert (list(flows), potentials.tolist()) == (DEPOT_FLOWS, DEPOT_POTENTIALS)


def test_a_strided_output_whose_items_cannot_be_written_as_they_lie_is_refused() -> None:
    misaligned = memoryview(bytearray(41))[1:].cast("Q")[::2]

    with pytest.raises(ValueError, match=r"^flows is strided, with items not aligned for their type"):
        solve_depot_into(misaligned)
    assert misaligned.tolist() == [0, 0, 0]


def test_outputs_that_share_memory_are_refused_before_anything_is_written() -> None:
    shared = memoryview(bytearray(48))

    with pytest.raises(ValueError, match=r"^flows and potentials share memory$"):
        solve_depot_into(shared[:24].cast("Q"), shared[16:40].cast("q"))
    assert bytes(shared) == bytes(48)

    assert solve_depot_into(shared[:24].cast("Q"), shared[24:].cast("q")) == DEPOT_COST, "side by side, they do not"


def test_every_output_is_checked_before_either_is_written() -> None:
    flows, potentials = array("Q", [7, 7, 7]), memoryview(bytes(24)).cast("q")

    with pytest.raises(ValueError, match=r"^potentials is read-only$"):
        solve_depot_into(flows, potentials)
    assert flows.tolist() == [7, 7, 7]


def test_an_empty_output_of_other_items_is_refused() -> None:
    with pytest.raises(TypeError, match=r"^flows holds items of format 'd', not unsigned 64-bit integers$"):
        kahlnws.NetworkSimplex().solve_into(tails=[], heads=[], capacities=[], costs=[], supplies=[0], flows=array("d"))
