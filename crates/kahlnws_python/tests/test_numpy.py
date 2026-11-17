# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""NumPy arrays in and out, through both levels of the solver and through a network: the item types read as
they are, the ones converted, the ones refused. Skipped where NumPy is not installed."""

import dataclasses
from typing import TypeAlias

import pytest

pytest.importorskip("numpy")

import numpy
import numpy.typing

import kahlnws
from networks import CUT, DEPOT, DEPOT_COST, DEPOT_FLOWS, DEPOT_POTENTIALS

# An array of any item type, as the tests hand them in.
Array: TypeAlias = numpy.typing.NDArray[numpy.generic]


def depot_arrays(index_type: numpy.typing.DTypeLike, number_type: numpy.typing.DTypeLike) -> list[Array]:
    """The depot's tails, heads, capacities, costs and supplies as arrays: the ends of `index_type`, the rest of
    `number_type`."""
    return [
        numpy.array(DEPOT.tails, dtype=index_type),
        numpy.array(DEPOT.heads, dtype=index_type),
        numpy.array(DEPOT.capacities, dtype=number_type),
        numpy.array(DEPOT.costs, dtype=number_type),
        numpy.array(DEPOT.supplies, dtype=numpy.int64),
    ]


# The item types of the ends and of the other per-arc columns the depot is handed in as: the exact ones, NumPy's
# default, and narrower ones of either sign.
INTEGER_TYPES = pytest.mark.parametrize(
    ("index_type", "number_type"),
    [
        (numpy.uint64, numpy.uint64),
        (numpy.int64, numpy.int64),
        (numpy.intp, numpy.int64),
        (numpy.int32, numpy.int32),
        (numpy.int16, numpy.int8),
        (numpy.uint8, numpy.uint16),
        (numpy.uint32, numpy.int64),
    ],
)


@INTEGER_TYPES
def test_the_depot_from_arrays_of_every_integer_type_through_both_levels(
    index_type: numpy.typing.DTypeLike, number_type: numpy.typing.DTypeLike
) -> None:
    tails, heads, capacities, costs, supplies = depot_arrays(index_type, number_type)
    solver = kahlnws.NetworkSimplex()

    solution = solver.solve(
        tails=tails,
        heads=heads,
        capacities=capacities,
        costs=costs,
        supplies=supplies,
        lower_bounds=numpy.zeros(3, dtype=number_type),
    ).outcome
    assert solution is not None
    assert (solution.flows, solution.potentials, solution.total_cost) == (DEPOT_FLOWS, DEPOT_POTENTIALS, DEPOT_COST)

    flows, potentials = numpy.zeros(3, dtype=numpy.uint64), numpy.zeros(3, dtype=numpy.int64)
    total_cost = solver.solve_into(
        tails=tails,
        heads=heads,
        capacities=capacities,
        costs=costs,
        supplies=supplies,
        flows=flows,
        potentials=potentials,
    ).outcome
    assert total_cost == DEPOT_COST
    assert flows.tolist() == DEPOT_FLOWS
    assert potentials.tolist() == DEPOT_POTENTIALS


def test_numpy_integers_in_a_list_are_read() -> None:
    solution = (
        kahlnws.NetworkSimplex()
        .solve(
            tails=[numpy.uint64(0), numpy.int8(0), numpy.int64(1)],
            heads=DEPOT.heads,
            capacities=DEPOT.capacities,
            costs=DEPOT.costs,
            supplies=DEPOT.supplies,
        )
        .outcome
    )

    assert solution is not None
    assert solution.flows == DEPOT_FLOWS


def test_strided_arrays_are_read_and_written() -> None:
    capacities = numpy.array([3, 0, 4, 0, 2], dtype=numpy.uint64)[::2]
    costs = numpy.array([1, 1, 5], dtype=numpy.int64)[::-1]
    flows_room = numpy.full(6, 9, dtype=numpy.uint64)

    total_cost = (
        kahlnws.NetworkSimplex()
        .solve_into(
            tails=DEPOT.tails,
            heads=DEPOT.heads,
            capacities=capacities,
            costs=costs,
            supplies=DEPOT.supplies,
            flows=flows_room[::2],
        )
        .outcome
    )

    assert total_cost == DEPOT_COST
    assert flows_room.tolist() == [2, 9, 2, 9, 2, 9], "only every other entry is written"


def test_the_arrays_handed_in_are_neither_kept_nor_changed() -> None:
    tails, heads, capacities, costs, supplies = depot_arrays(numpy.uint64, numpy.int64)
    solver = kahlnws.NetworkSimplex()

    solution = solver.solve(tails=tails, heads=heads, capacities=capacities, costs=costs, supplies=supplies).outcome
    capacities[:] = 0
    tails, heads, capacities, costs, supplies = depot_arrays(numpy.uint64, numpy.int64)
    after = solver.solve(tails=tails, heads=heads, capacities=capacities, costs=costs, supplies=supplies).outcome

    assert solution is not None and after is not None
    assert solution.flows == after.flows == DEPOT_FLOWS
    assert tails.tolist() == DEPOT.tails


def test_an_infeasible_problem_leaves_the_arrays_as_they_were() -> None:
    flows, potentials = numpy.full(1, 7, dtype=numpy.uint64), numpy.full(2, 8, dtype=numpy.int64)

    total_cost = (
        kahlnws.NetworkSimplex()
        .solve_into(
            tails=numpy.array(CUT.tails),
            heads=numpy.array(CUT.heads),
            capacities=numpy.array(CUT.capacities),
            costs=numpy.array(CUT.costs),
            supplies=numpy.array(CUT.supplies),
            flows=flows,
            potentials=potentials,
        )
        .outcome
    )

    assert total_cost is None
    assert (flows.tolist(), potentials.tolist()) == ([7], [8, 8])


def test_negative_tails_of_numpy_s_default_integer_are_refused_with_their_position() -> None:
    tails = numpy.array([0, -3, 1])

    with pytest.raises(OverflowError, match=r"^tails\[1\] is -3, outside the range of a node, from 0 to "):
        kahlnws.NetworkSimplex().solve(
            tails=tails, heads=DEPOT.heads, capacities=DEPOT.capacities, costs=DEPOT.costs, supplies=DEPOT.supplies
        )


def test_unsigned_items_past_the_signed_range_are_refused() -> None:
    supplies = numpy.array([4, 0, 2**63], dtype=numpy.uint64)

    with pytest.raises(OverflowError, match=r"^supplies\[2\] is 9223372036854775808, outside the range of a signed"):
        kahlnws.NetworkSimplex().solve(
            tails=DEPOT.tails, heads=DEPOT.heads, capacities=DEPOT.capacities, costs=DEPOT.costs, supplies=supplies
        )


def solve_depot_with(**columns: Array) -> kahlnws.Solution | None:
    """What a fresh solver finds for the depot with `columns`, arrays, in place of its own."""
    arguments: dict[str, object] = {
        "tails": DEPOT.tails,
        "heads": DEPOT.heads,
        "capacities": DEPOT.capacities,
        "costs": DEPOT.costs,
        "supplies": DEPOT.supplies,
    }
    arguments.update(columns)

    # The columns are arrays of any item type here, so that the ones a solve refuses can be handed in.
    return kahlnws.NetworkSimplex().solve(**arguments).outcome  # type: ignore[arg-type]  # see above


@pytest.mark.parametrize(("dtype", "written"), [(numpy.float64, "d"), (numpy.float32, "f"), (numpy.float16, "e")])
def test_arrays_of_floats_are_refused_with_their_format(dtype: numpy.typing.DTypeLike, written: str) -> None:
    with pytest.raises(TypeError, match=rf"^capacities holds items of format '{written}', not integers$"):
        solve_depot_with(capacities=numpy.array([3, 4, 2], dtype=dtype))


def test_an_array_in_the_other_byte_order_is_refused_and_one_in_the_machine_s_is_read() -> None:
    other, own = (">", "<") if numpy.little_endian else ("<", ">")
    foreign = numpy.array([3, 4, 2], dtype=numpy.dtype(f"{other}u8"))

    with pytest.raises(TypeError, match=r"^capacities holds integers in the other byte order than the machine's"):
        solve_depot_with(capacities=foreign)
    solution = solve_depot_with(capacities=numpy.array([3, 4, 2], dtype=numpy.dtype(f"{own}u8")))
    assert solution is not None
    assert solution.flows == DEPOT_FLOWS


def test_arrays_of_booleans_read_as_zeros_and_ones_and_arrays_of_objects_as_sequences() -> None:
    by_flags = solve_depot_with(lower_bounds=numpy.array([False, True, False]))
    by_objects = solve_depot_with(capacities=numpy.array([3, 4, numpy.uint8(2)], dtype=object))

    assert by_flags is not None
    assert by_flags.flows == [2, 2, 2]
    assert by_objects is not None
    assert by_objects.flows == DEPOT_FLOWS
    with pytest.raises(TypeError, match=r"^costs\[1\] is of type str, not an integer$"):
        solve_depot_with(costs=numpy.array([5, "1", 1], dtype=object))


def test_an_array_of_items_of_size_zero_is_refused() -> None:
    with pytest.raises(TypeError, match=r"^heads holds items of size zero$"):
        solve_depot_with(heads=numpy.zeros(3, dtype="V0"))


@pytest.mark.parametrize(("shape", "dimensions"), [((), 0), ((3, 1), 2), ((1, 3, 1), 3)])
def test_arrays_of_other_than_one_dimension_are_refused(shape: tuple[int, ...], dimensions: int) -> None:
    with pytest.raises(ValueError, match=rf"^heads has {dimensions} dimensions, not 1$"):
        solve_depot_with(heads=numpy.zeros(shape, dtype=numpy.uint64))


@pytest.mark.parametrize("dtype", [numpy.uint64, numpy.int32])
def test_an_array_whose_memory_is_not_aligned_for_its_items_is_read_and_written(dtype: numpy.typing.DTypeLike) -> None:
    # Three integers that start one byte into their memory: their bytes are copied, so alignment does not matter.
    tails = numpy.frombuffer(bytearray(1) + numpy.array(DEPOT.tails, dtype=dtype).tobytes(), dtype=dtype, offset=1)
    flows = numpy.frombuffer(bytearray(25), dtype=numpy.uint64, count=3, offset=1)

    answer = kahlnws.NetworkSimplex().solve_into(
        tails=tails,
        heads=DEPOT.heads,
        capacities=DEPOT.capacities,
        costs=DEPOT.costs,
        supplies=DEPOT.supplies,
        flows=flows,
    )

    assert answer.outcome == DEPOT_COST
    assert flows.tolist() == DEPOT_FLOWS


@pytest.mark.parametrize(
    ("flows", "potentials", "raised", "message"),
    [
        (
            numpy.zeros(3, dtype=numpy.int64),
            None,
            TypeError,
            r"^flows holds items of format '[lq]', not unsigned 64-bit integers$",
        ),
        (
            numpy.zeros(3, dtype=numpy.uint32),
            None,
            TypeError,
            r"^flows holds items of format 'I', not unsigned 64-bit integers$",
        ),
        (
            numpy.zeros(3, dtype=numpy.uint64),
            numpy.zeros(3, dtype=numpy.float64),
            TypeError,
            r"^potentials holds items of format 'd', not signed 64-bit integers$",
        ),
        (numpy.zeros((3, 1), dtype=numpy.uint64), None, ValueError, r"^flows has 2 dimensions, not 1$"),
        (numpy.zeros(4, dtype=numpy.uint64), None, ValueError, r"^flows holds 4 entries for a problem of 3 arcs$"),
    ],
)
def test_output_arrays_that_cannot_take_the_answer_are_refused(
    flows: Array, potentials: Array | None, raised: type[Exception], message: str
) -> None:
    with pytest.raises(raised, match=message):
        kahlnws.NetworkSimplex().solve_into(
            tails=DEPOT.tails,
            heads=DEPOT.heads,
            capacities=DEPOT.capacities,
            costs=DEPOT.costs,
            supplies=DEPOT.supplies,
            flows=flows,
            potentials=potentials,
        )


def test_a_read_only_array_is_refused() -> None:
    flows = numpy.zeros(3, dtype=numpy.uint64)
    flows.flags.writeable = False

    with pytest.raises(ValueError, match=r"^flows is read-only$"):
        kahlnws.NetworkSimplex().solve_into(
            tails=DEPOT.tails,
            heads=DEPOT.heads,
            capacities=DEPOT.capacities,
            costs=DEPOT.costs,
            supplies=DEPOT.supplies,
            flows=flows,
        )


def test_a_column_whose_copy_cannot_be_held_raises_memory_error_and_ends_nothing() -> None:
    # A view of one integer that claims `2**59` of them, `2**62` bytes: more than any address space holds, so the
    # allocator refuses the copy at once, and the solver stays usable.
    claimed = numpy.broadcast_to(numpy.uint64(0), (2**59,))
    solver = kahlnws.NetworkSimplex()

    with pytest.raises(MemoryError, match=r"^the room for the 576460752303423488 entries of capacities was refused$"):
        solver.solve(
            tails=DEPOT.tails, heads=DEPOT.heads, capacities=claimed, costs=DEPOT.costs, supplies=DEPOT.supplies
        )

    solution = DEPOT.solve(solver)
    assert solution is not None
    assert solution.flows == DEPOT_FLOWS


@INTEGER_TYPES
def test_a_network_loads_the_depot_from_arrays_of_every_integer_type_and_writes_its_optimum_into_arrays(
    index_type: numpy.typing.DTypeLike, number_type: numpy.typing.DTypeLike
) -> None:
    tails, heads, capacities, costs, supplies = depot_arrays(index_type, number_type)
    network = kahlnws.Network()

    network.load(
        tails=tails,
        heads=heads,
        capacities=capacities,
        costs=costs,
        supplies=supplies,
        lower_bounds=numpy.zeros(3, dtype=number_type),
    )
    assert network.solve().outcome == DEPOT_COST
    optimum = network.optimum()
    assert optimum is not None
    flows, potentials = numpy.zeros(3, dtype=numpy.uint64), numpy.zeros(3, dtype=numpy.int64)
    optimum.flows_into(flows)
    optimum.potentials_into(potentials)

    assert optimum.total_cost == DEPOT_COST
    assert (flows.tolist(), potentials.tolist()) == (DEPOT_FLOWS, DEPOT_POTENTIALS)


def test_a_network_s_optimum_writes_strided_arrays_and_refuses_arrays_that_cannot_take_it() -> None:
    network = DEPOT.loaded()
    network.solve()
    held = network.optimum()
    assert held is not None
    every_other = numpy.full(6, 7, dtype=numpy.uint64)

    held.flows_into(every_other[::2])
    assert every_other.tolist() == [2, 7, 2, 7, 2, 7]

    with pytest.raises(TypeError, match=r"^flows holds items of format '.+', not unsigned 64-bit integers$"):
        held.flows_into(numpy.zeros(3, dtype=numpy.int64))
    with pytest.raises(ValueError, match=r"^potentials has 2 dimensions, not 1$"):
        held.potentials_into(numpy.zeros((3, 1), dtype=numpy.int64))
    read_only = numpy.zeros(3, dtype=numpy.int64)
    read_only.flags.writeable = False
    with pytest.raises(ValueError, match=r"^potentials is read-only$"):
        held.potentials_into(read_only)


def test_a_network_writes_the_problem_it_holds_into_arrays_of_the_columns_types() -> None:
    network = dataclasses.replace(DEPOT, lower_bounds=[1, 0, 0]).loaded()
    tails, heads = numpy.zeros(3, dtype=numpy.uintp), numpy.zeros(3, dtype=numpy.uintp)
    lower_bounds, capacities = numpy.zeros(3, dtype=numpy.uint64), numpy.zeros(3, dtype=numpy.uint64)
    costs, supplies = numpy.zeros(3, dtype=numpy.int64), numpy.zeros(3, dtype=numpy.int64)

    network.tails_into(tails)
    network.heads_into(heads)
    network.lower_bounds_into(lower_bounds)
    network.capacities_into(capacities)
    network.costs_into(costs)
    network.supplies_into(supplies)

    written = [column.tolist() for column in [tails, heads, lower_bounds, capacities, costs, supplies]]
    assert written == [DEPOT.tails, DEPOT.heads, [1, 0, 0], DEPOT.capacities, DEPOT.costs, DEPOT.supplies]
    with pytest.raises(TypeError, match=r"^tails holds items of format '.+', not unsigned integers as wide as size_t$"):
        network.tails_into(numpy.zeros(3, dtype=numpy.int64))
