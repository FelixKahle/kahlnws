# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""How a column is read from what Python hands in, without NumPy: sequences, the standard library's buffers, and every
way an argument can be refused, each named by its argument."""

import ctypes
import re
import sys
from array import array
from collections.abc import Iterator, Sequence

import pytest

import kahlnws
from networks import DEPOT, DEPOT_COST, DEPOT_FLOWS


def solve_depot_with(**columns: object) -> kahlnws.Solution | None:
    """What a fresh solver answers for the depot with `columns` in place of its own, of any type."""
    arguments: dict[str, object] = {
        "tails": DEPOT.tails,
        "heads": DEPOT.heads,
        "capacities": DEPOT.capacities,
        "costs": DEPOT.costs,
        "supplies": DEPOT.supplies,
    }
    arguments.update(columns)

    # The columns are of any type here, so that the ones a solve refuses can be handed in.
    return kahlnws.NetworkSimplex().solve(**arguments).outcome  # type: ignore[arg-type]  # see above


@pytest.mark.parametrize("type_code", ["b", "B", "h", "H", "i", "I", "l", "L", "q", "Q"])
def test_buffers_of_every_integer_type_are_read(type_code: str) -> None:
    solution = solve_depot_with(
        tails=array(type_code, DEPOT.tails),
        heads=array(type_code, DEPOT.heads),
        capacities=array(type_code, DEPOT.capacities),
        costs=array(type_code, DEPOT.costs),
        lower_bounds=array(type_code, [0, 0, 0]),
    )

    assert solution is not None
    assert (solution.flows, solution.total_cost) == (DEPOT_FLOWS, DEPOT_COST)


def test_bytes_and_a_bytearray_are_read_as_columns_of_bytes() -> None:
    solution = solve_depot_with(tails=bytes(DEPOT.tails), heads=bytearray(DEPOT.heads))

    assert solution is not None
    assert solution.flows == DEPOT_FLOWS


def test_a_strided_view_of_the_column_s_type_is_copied_as_it_lies() -> None:
    # Every other entry of a longer column: 3, 4, 2 between the 99s.
    spread = memoryview(array("Q", [3, 99, 4, 99, 2]))[::2]

    solution = solve_depot_with(capacities=spread)

    assert solution is not None
    assert solution.flows == DEPOT_FLOWS


def test_integers_of_any_kind_with_an_index_are_read() -> None:
    class Index:
        """An object that is an integer by its `__index__` alone."""

        def __init__(self, value: int) -> None:
            self.value = value

        def __index__(self) -> int:
            return self.value

    solution = solve_depot_with(capacities=[Index(3), Index(4), Index(2)], costs=[5, True, 1])

    assert solution is not None
    assert solution.flows == DEPOT_FLOWS


def generated(entries: list[int]) -> Iterator[int]:
    """The entries of `entries`, from a generator, which has no length."""
    yield from entries


@pytest.mark.parametrize(
    ("column", "type_name"),
    [
        (1.5, "float"),
        (None, "NoneType"),
        ({0, 1}, "set"),
        ({0: 0, 1: 0, 2: 1}, "dict"),
        (generated([0, 0, 1]), "generator"),
        (iter([0, 0, 1]), "list_iterator"),
    ],
)
def test_an_argument_that_is_neither_a_sequence_nor_a_buffer_is_refused_with_its_type(
    column: object, type_name: str
) -> None:
    message = rf"^tails is of type {type_name}, neither a sequence nor a buffer of integers$"

    with pytest.raises(TypeError, match=message):
        solve_depot_with(tails=column)


@pytest.mark.parametrize(
    ("entry", "type_name"), [(1.0, "float"), ("1", "str"), (None, "NoneType"), (b"1", "bytes"), ([1], "list")]
)
def test_an_entry_that_is_no_integer_is_refused_with_its_position_and_type(entry: object, type_name: str) -> None:
    with pytest.raises(TypeError, match=rf"^heads\[1\] is of type {type_name}, not an integer$"):
        solve_depot_with(heads=[2, entry, 2])


def test_a_string_is_refused_entry_by_entry() -> None:
    with pytest.raises(TypeError, match=r"^tails\[0\] is of type str, not an integer$"):
        solve_depot_with(tails="001")


@pytest.mark.parametrize(
    ("column", "entries", "position", "written"),
    [
        ("tails", [0, -1, 1], 1, "-1"),
        ("heads", [2, 1, 2**64], 2, "18446744073709551616"),
        ("capacities", [-3, 4, 2], 0, "-3"),
        ("capacities", [3, 2**64, 2], 1, "18446744073709551616"),
        ("lower_bounds", [0, 0, -1], 2, "-1"),
        ("costs", [5, 2**63, 1], 1, "9223372036854775808"),
        ("costs", [5, 1, -(2**63) - 1], 2, "-9223372036854775809"),
        ("supplies", [2**100, 0, -4], 0, str(2**100)),
        ("supplies", [10**5000, 0, -4], 0, "2**16609 or more"),
        ("costs", [5, -(10**5000), 1], 1, "-(2**16609 or more)"),
    ],
)
def test_an_entry_outside_its_range_is_refused_with_its_position_and_value(
    column: str, entries: list[int], position: int, written: str
) -> None:
    with pytest.raises(
        OverflowError, match=rf"^{column}\[{position}\] is {re.escape(written)}, outside the range of an? "
    ):
        solve_depot_with(**{column: entries})


@pytest.mark.parametrize(
    ("column", "items", "position", "written"),
    [
        ("tails", array("q", [0, 0, -1]), 2, "-1"),
        ("capacities", array("b", [3, -4, 2]), 1, "-4"),
        ("lower_bounds", array("i", [-2, 0, 0]), 0, "-2"),
        ("costs", array("Q", [5, 2**63, 1]), 1, "9223372036854775808"),
        ("supplies", array("Q", [4, 0, 2**64 - 4]), 2, "18446744073709551612"),
    ],
)
def test_an_item_of_a_buffer_outside_its_range_is_refused_with_its_position_and_value(
    column: str, items: Sequence[int], position: int, written: str
) -> None:
    with pytest.raises(
        OverflowError, match=rf"^{column}\[{position}\] is {re.escape(written)}, outside the range of an? "
    ):
        solve_depot_with(**{column: items})


def test_the_range_named_is_the_column_s_own() -> None:
    with pytest.raises(OverflowError, match=r"outside the range of a node, from 0 to 18446744073709551615$"):
        solve_depot_with(tails=[0, 0, -1])
    with pytest.raises(OverflowError, match=r"outside the range of an unsigned 64-bit integer, from 0 to 1844674407"):
        solve_depot_with(capacities=[-1, 0, 0])
    with pytest.raises(OverflowError, match=r"of a signed 64-bit integer, from -9223372036854775808 to 9223372036"):
        solve_depot_with(costs=[2**63, 0, 0])


@pytest.mark.parametrize(
    ("column", "written"),
    [(array("f", [5, 1, 1]), "f"), (array("d", [5, 1, 1]), "d"), (memoryview(b"abc").cast("c"), "c")],
)
def test_a_buffer_of_floats_or_characters_is_refused_with_its_format(column: object, written: str) -> None:
    with pytest.raises(TypeError, match=rf"^costs holds items of format '{written}', not integers$"):
        solve_depot_with(costs=column)


def test_a_buffer_of_booleans_is_read_as_zeros_and_ones() -> None:
    lower_bounds = (ctypes.c_bool * 3)(False, True, False)

    solution = solve_depot_with(lower_bounds=lower_bounds)

    assert solution is not None
    assert solution.flows == [2, 2, 2]
    assert solve_depot_with(lower_bounds=memoryview(bytes([0, 2, 0])).cast("?")) == solution, "any byte but 0 is 1"


def test_integers_in_the_machine_s_byte_order_are_read_whether_the_format_names_it_or_not() -> None:
    native = getattr(ctypes.c_uint64, "__ctype_le__" if sys.byteorder == "little" else "__ctype_be__")
    capacities = (native * 3)(3, 4, 2)

    assert memoryview(capacities).format in {"<Q", ">Q"}
    solution = solve_depot_with(capacities=capacities)

    assert solution is not None
    assert solution.flows == DEPOT_FLOWS


def test_integers_in_the_other_byte_order_are_refused_with_their_format() -> None:
    foreign = getattr(ctypes.c_uint64, "__ctype_be__" if sys.byteorder == "little" else "__ctype_le__")
    capacities = (foreign * 3)(3, 4, 2)
    written = memoryview(capacities).format

    with pytest.raises(
        TypeError,
        match=rf"^capacities holds integers in the other byte order than the machine's, of format '\{written}'$",
    ):
        solve_depot_with(capacities=capacities)


def test_a_buffer_of_items_of_size_zero_is_refused() -> None:
    class Nothing(ctypes.Structure):
        """A structure of no fields, whose items have no bytes."""

        _fields_ = ()

    with pytest.raises(TypeError, match=r"^heads holds items of size zero$"):
        solve_depot_with(heads=(Nothing * 3)())


def test_a_buffer_whose_items_are_not_aligned_for_their_type_is_read() -> None:
    # Three 8-byte integers that start one byte into their memory: copied byte by byte, alignment does not matter.
    room = memoryview(bytearray(25))[1:].cast("Q")
    room[:] = array("Q", DEPOT.capacities)

    solution = solve_depot_with(capacities=room)

    assert solution is not None
    assert solution.flows == DEPOT_FLOWS


def test_a_buffer_of_two_dimensions_is_refused() -> None:
    square = memoryview(bytearray(4 * 8)).cast("q", (2, 2))

    with pytest.raises(ValueError, match=r"^supplies has 2 dimensions, not 1$"):
        solve_depot_with(supplies=square)


def test_the_first_column_that_cannot_be_read_is_answered_in_the_order_of_the_problem() -> None:
    # Tails, heads, lower bounds, capacities, costs, supplies: the heads before the costs, the lower bounds before the
    # capacities, whatever order the keywords come in.
    with pytest.raises(OverflowError, match=r"^heads\[0\]"):
        solve_depot_with(costs=[2**64, 1, 1], heads=[-1, 1, 2])
    with pytest.raises(OverflowError, match=r"^lower_bounds\[1\]"):
        solve_depot_with(capacities=[-1, 4, 2], lower_bounds=[0, -1, 0])


def test_the_value_of_an_entry_out_of_range_is_its_index_taken_once() -> None:
    class Index:
        """An integer by its `__index__`, which counts its calls, and whose `__str__` says something else."""

        calls = 0

        def __index__(self) -> int:
            Index.calls += 1
            return -5

        def __str__(self) -> str:
            return "7"

    with pytest.raises(OverflowError, match=r"^capacities\[1\] is -5, outside the range"):
        solve_depot_with(capacities=[3, Index(), 2])
    assert Index.calls == 1


@pytest.mark.parametrize("raised", [ZeroDivisionError, TypeError])
def test_an_exception_an_index_raises_is_passed_on_with_a_note_that_names_the_argument(raised: type[Exception]) -> None:
    class Broken:
        """An object whose `__index__` fails."""

        def __index__(self) -> int:
            raise raised("this entry cannot say what it is")

    with pytest.raises(raised, match="this entry cannot say what it is") as caught:
        solve_depot_with(costs=[5, Broken(), 1])
    assert caught.value.__notes__ == ["raised while the argument costs was read"]


def test_an_exception_a_sequence_raises_is_passed_on_with_a_note_that_names_the_argument() -> None:
    class Unmeasured:
        """A sequence whose length cannot be taken."""

        def __len__(self) -> int:
            raise LookupError("no length here")

        def __getitem__(self, position: int) -> int:
            return 0

    with pytest.raises(LookupError, match="no length here") as caught:
        solve_depot_with(supplies=Unmeasured())
    assert caught.value.__notes__ == ["raised while the argument supplies was read"]


def test_an_empty_buffer_of_integers_is_an_empty_column_and_one_of_floats_is_refused() -> None:
    # An exporter may point an empty buffer at memory not aligned for its items, as `array.array` does on some
    # versions: nothing is copied from it.
    solver = kahlnws.NetworkSimplex()
    empty = {"tails": array("Q"), "heads": array("q"), "capacities": array("Q"), "costs": array("q")}

    solution = solver.solve(**empty, supplies=array("q", [0, 0]), lower_bounds=array("B")).outcome
    assert solution is not None
    assert (solution.flows, len(solution.potentials), solution.total_cost) == ([], 2, 0)

    with pytest.raises(TypeError, match=r"^lower_bounds holds items of format 'd', not integers$"):
        solver.solve(**empty, supplies=[0, 0], lower_bounds=array("d"))
