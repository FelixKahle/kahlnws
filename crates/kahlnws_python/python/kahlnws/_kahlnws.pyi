# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The extension module of the package `kahlnws`, which re-exports every name it holds."""

from collections.abc import Iterable, Sequence
from types import GenericAlias
from typing import ClassVar, Generic, Protocol, Self, SupportsIndex, TypeAlias, TypeVar, final

from typing_extensions import Buffer, disjoint_base

class _ArrayExporter(Protocol):
    """An object NumPy makes an array of, such as a `numpy.ndarray`: named beside `Buffer` in every column and output
    because NumPy's stubs declare an array a buffer only from Python 3.12 on, so that an array type-checks on 3.11 too.
    At run time the buffer protocol alone decides how an argument is read or written."""

    def __array__(self) -> object: ...

# A column of a problem: a sequence of integers (a length and indexing), or a buffer of integers of one dimension.
_Column: TypeAlias = Sequence[SupportsIndex] | Buffer | _ArrayExporter

# A room a solve writes a column of its answer to: a writable buffer of one dimension.
_Room: TypeAlias = Buffer | _ArrayExporter

# What a solve found where a flow meets the supplies: a `Solution` from `solve`, the cost from `solve_into`.
_Found_co = TypeVar("_Found_co", covariant=True)

__all__ = [
    "Answer",
    "Breach",
    "NetworkSimplex",
    "ProblemError",
    "Solution",
    "Table",
    "Work",
    "__version__",
    "cost_bound",
]

__version__: str

@final
class NetworkSimplex:
    """A primal network simplex for the min-cost flow problem on exact integers, with the room it works in.

    A problem is arcs between nodes, given as columns by keyword: arc `a` leaves node `tails[a]`, enters node
    `heads[a]`, carries at least `lower_bounds[a]` and at most `capacities[a]` units of flow, and costs `costs[a]` per
    unit; node `v` has the supply `supplies[v]`, positive where it sends flow and negative where it takes it. A solve
    finds a flow of least cost that meets every supply within the bounds, or that no such flow exists. Every number is
    an integer, and every sum a solve forms is exact.

    Each column is a sequence of `int` (a list, a tuple, a range: anything with a length and indexing that is no
    mapping) or a buffer of integers of one dimension (a `numpy.ndarray`, an `array.array`, a `memoryview`), strided
    or not. Every column is copied before the solve starts. A buffer whose items are exactly the column's type is
    copied in one block: unsigned integers as wide as `size_t` for the tails and the heads (64-bit on the platforms
    tested), unsigned 64-bit for the lower bounds and the capacities, signed 64-bit for the costs and the supplies;
    with NumPy, `numpy.uintp`, `numpy.uint64` and `numpy.int64`. A buffer of other integers, such as tails of
    `numpy.int64`, NumPy's default, is converted item by item with a range check; a buffer of `bool` reads as 0 and 1,
    and a buffer of Python objects as the sequence it is. Integers in the other byte order than the machine's, floats,
    characters and items of no size are refused.

    `solve` answers an `Answer[Solution]`, whose outcome owns plain lists; `solve_into` writes into buffers the
    caller owns, made once and reused, and answers an `Answer[int]` with the cost. Both answer the `Work` the solve
    did. The room is kept from one solve to the next and only grows: a solve of a problem with no more nodes and no
    more arcs than the room was made for, by `reserve` or by an earlier solve, allocates no room, and a solve that is
    refused may still have grown it. Nothing of an earlier problem or its answer is kept.

    The solve runs with the interpreter released, on the solver's own copy of the columns: other threads run
    meanwhile, and nothing they do reaches the solve. A column that another thread writes, without the interpreter,
    while it is being copied is the caller's race: its values may be torn, and the answer is that of the columns as
    read. A running solve cannot be interrupted; a `KeyboardInterrupt` is raised once it returns. One solver serves one
    call at a time: a call on a solver that is already in a call, from another thread or from code a column runs while
    it is read, raises `RuntimeError`. A solver cannot be pickled or copied. The package needs the GIL: it has not been
    tested on free-threaded interpreters.

    Example:
        >>> import kahlnws
        >>> solver = kahlnws.NetworkSimplex()
        >>> answer = solver.solve(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
        ...                       supplies=[4, 0, -4])
        >>> answer.outcome.flows, answer.outcome.total_cost
        ([2, 2, 2], 14)
    """

    def __init__(self) -> None: ...
    def reserve(self, node_count: int, arc_count: int) -> None:
        """Make room for problems of up to `node_count` nodes and `arc_count` arcs.

        A solve of such a problem allocates no room afterwards. Room already held is kept.

        Args:
            node_count: The most nodes a problem will have.
            arc_count: The most arcs a problem will have.

        Raises:
            MemoryError: The memory is refused; the room held before is kept, and the solver may be used.
            OverflowError: A count is below zero or past the platform's `size_t`.
            RuntimeError: The solver is already in a call.

        Example:
            >>> kahlnws.NetworkSimplex().reserve(node_count=1000, arc_count=4000)
        """

    def solve(
        self,
        *,
        tails: _Column,
        heads: _Column,
        capacities: _Column,
        costs: _Column,
        supplies: _Column,
        lower_bounds: _Column | None = None,
    ) -> Answer[Solution]:
        """Solve the problem and answer what the solve found with the work it did.

        Args:
            tails: The node each arc leaves, one per arc.
            heads: The node each arc enters, one per arc.
            capacities: The most flow of each arc, one per arc.
            costs: The cost of a unit of flow on each arc, of either sign, one per arc.
            supplies: The supply of each node: positive where it sends flow, negative where it takes it.
            lower_bounds: The least flow of each arc, one per arc; left out where every arc's is zero.

        Returns:
            An `Answer[Solution]`: its `outcome` is the `Solution` (the flows, the potentials that prove them optimal,
            and the cost), or `None` where no flow within the bounds meets the supplies; its `work` is the `Work` the
            solve did either way.

        Raises:
            ProblemError: The problem is one no solve admits; its `breach` says why. A `ValueError`.
            TypeError: A column is neither a sequence nor a buffer of integers, an entry is no integer, or a buffer's
                items are no integers in the machine's byte order.
            OverflowError: An entry lies outside the range of its column.
            ValueError: A buffer has other than one dimension.
            MemoryError: The memory for the copy of a column or for the room of the problem is refused.
            RuntimeError: The solver is already in a call.

            What an exporter, a sequence or an entry's `__index__` raises is passed on, with a note that names the
            argument.

        Example:
            >>> solver = kahlnws.NetworkSimplex()
            >>> solver.solve(tails=[0], heads=[1], capacities=[1], costs=[1], supplies=[2, -2])
            Answer(outcome=None, work=Work(pivots=1, degenerate_pivots=0, first_arc_pivots=0))
        """

    def solve_into(
        self,
        *,
        tails: _Column,
        heads: _Column,
        capacities: _Column,
        costs: _Column,
        supplies: _Column,
        flows: _Room,
        lower_bounds: _Column | None = None,
        potentials: _Room | None = None,
    ) -> Answer[int]:
        """Solve the problem, write its flows, and its potentials where asked, into buffers the caller owns, and answer
        the cost with the work the solve did.

        Both outputs are checked before the solve (their type, shape, length, writability, and that they share no
        memory) and written only where it finds a flow, each by one copy; where the problem is infeasible or the call
        raises, both hold what they held before. Only an exporter that breaks the buffer protocol could make a copy
        fail after the checks, and then the flows may be written and the potentials not. The columns are copied before
        anything is written, so an output may also be one of them. Where the room holds the problem, the solver
        allocates nothing; what a call still makes is a `memoryview` and an export record for each buffer it is
        handed, and the answer.

        Args:
            tails: The node each arc leaves, one per arc.
            heads: The node each arc enters, one per arc.
            capacities: The most flow of each arc, one per arc.
            costs: The cost of a unit of flow on each arc, of either sign, one per arc.
            supplies: The supply of each node: positive where it sends flow, negative where it takes it.
            flows: A writable buffer of one dimension with one unsigned 64-bit integer per arc, such as
                `numpy.empty(arc_count, dtype=numpy.uint64)` or `array.array("Q", ...)`.
            lower_bounds: The least flow of each arc, one per arc; left out where every arc's is zero.
            potentials: A writable buffer of one dimension with one signed 64-bit integer per node, such as
                `numpy.empty(node_count, dtype=numpy.int64)`, sharing no memory with `flows`; left out where the
                potentials are not wanted.

        Returns:
            An `Answer[int]`: its `outcome` is the cost of the flow written, an exact `int`, or `None` where no flow
            within the bounds meets the supplies; its `work` is the `Work` the solve did either way.

        Raises:
            ProblemError: The problem is one no solve admits; its `breach` says why. A `ValueError`.
            TypeError: A column is neither a sequence nor a buffer of integers, an entry is no integer, a buffer's
                items are no integers in the machine's byte order, an output is no buffer, or its items are not
                exactly the integers written.
            OverflowError: An entry lies outside the range of its column.
            ValueError: A buffer has other than one dimension, an output is read-only, of another length than the
                problem's arcs or nodes, strided with items that cannot be written as they lie, or shares memory with
                the other.
            MemoryError: The memory for the copy of a column or for the room of the problem is refused.
            RuntimeError: The solver is already in a call.

            What an exporter, a sequence or an entry's `__index__` raises is passed on, with a note that names the
            argument.

        Example:
            >>> from array import array
            >>> flows = array("Q", [0, 0, 0])
            >>> answer = kahlnws.NetworkSimplex().solve_into(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2],
            ...                                             costs=[5, 1, 1], supplies=[4, 0, -4], flows=flows)
            >>> answer.outcome, flows.tolist()
            (14, [2, 2, 2])
        """

@final
class Work:
    """What one solve did, counted in pivots: exact counts of that solve, not samples or estimates.

    A pivot brings one arc that violates its bound into the basis and sends the most flow the cycle it closes with the
    tree carries around that cycle. Where a link of the tree has the least room, that link leaves and the arc takes its
    place; where the arc's own width is the least, no link leaves and the arc moves to its other bound, a flip, which
    is a pivot too.

    The counts depend on the problem as given, its arc order included, and on the version of the package, as the answer
    does: the same problem gives the same work from a fresh solver and from one that solved others before. A solve
    answers `degenerate_pivots <= pivots` and `first_arc_pivots <= pivots`; the pivots that sent flow are
    `pivots - degenerate_pivots`. The block search that chooses the other pivots runs
    `pivots - first_arc_pivots + 1` times, once for each of them and once more to find that no arc violates its bound,
    so it is not counted apart.

    Plain data: immutable, compared and hashed by value, matched by position in the order of its attributes, copied
    and pickled.

    Args:
        pivots: The pivots a solve made.
        degenerate_pivots: Of them, those that sent no flow.
        first_arc_pivots: Of them, those made by the arcs tried before block search.

    Raises:
        OverflowError: A count is below zero or past `2**64 - 1`.

    Example:
        >>> kahlnws.Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0).pivots
        4
    """

    __match_args__ = ("pivots", "degenerate_pivots", "first_arc_pivots")
    def __new__(cls, pivots: int, degenerate_pivots: int, first_arc_pivots: int) -> Self: ...
    @property
    def pivots(self) -> int:
        """The pivots the solve made, flips included: one for each arc brought into the basis. Zero where no arc ever
        violated its bound, as in a problem of no arc.
        """

    @property
    def degenerate_pivots(self) -> int:
        """Of the pivots, those that sent no flow: the link that left had no room, so the flow stayed as it was and only
        the tree and the potentials changed. At most `pivots`; a flip is never one.
        """

    @property
    def first_arc_pivots(self) -> int:
        """Of the pivots, those made by the arcs the solve tries before block search, chosen from the problem's
        structure, each where it violated its bound when its turn came. At most `pivots`, and `pivots` where those arcs
        alone reached the optimum.
        """

@final
class Answer(Generic[_Found_co]):
    """What a solve answers: what it found, and the work it did to find it, as one value.

    `NetworkSimplex.solve` answers an `Answer[Solution]`: its `outcome` is the `Solution`, or `None` where no flow
    meets the supplies. `NetworkSimplex.solve_into` answers an `Answer[int]`: its `outcome` is the cost of the flow it
    wrote, or `None`. The work is answered either way. A refused problem raises and answers nothing, since no solve
    ran.

    Plain data: immutable, compared and hashed by value (its outcome and its work), matched by position, copied and
    pickled.

    Args:
        outcome: What a solve found, or `None`.
        work: The work it did.

    Example:
        >>> answer = kahlnws.Answer(14, kahlnws.Work(4, 1, 0))
        >>> answer.outcome, answer.work.pivots
        (14, 4)
    """

    __match_args__ = ("outcome", "work")
    def __new__(cls, outcome: _Found_co | None, work: Work) -> Self: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __hash__(self) -> int: ...
    @classmethod
    def __class_getitem__(cls, item: object) -> GenericAlias:
        """The class `Answer` with its outcome's type, as `list[int]` is a list with its items' type: `Answer[int]`.

        # Errors
        Passes on what Python raises while the alias is made.
        """

    @property
    def outcome(self) -> _Found_co | None:
        """What the solve found: the `Solution` of `solve` or the cost of `solve_into` where a flow meets the supplies,
        `None` where none does.
        """

    @property
    def work(self) -> Work:
        """The work the solve did, optimal or infeasible alike."""

@final
class Solution:
    """A flow of least cost, with the potentials that prove it optimal and its cost.

    What `NetworkSimplex.solve` finds for a problem that has a flow. Each attribute answers the same object each time it
    is read; the lists are the solution's own, and no solve writes them again.

    Plain data: compared by value, and hashed by value as the tuple of its parts is, so a solution whose lists the
    caller changes compares and hashes as changed; copied and pickled.

    Args:
        flows: The flow of each arc, any iterable of `int`, kept as a list.
        potentials: The potential of each node, any iterable of `int`, kept as a list.
        total_cost: The cost of the flow.

    Raises:
        TypeError: `flows` or `potentials` cannot be iterated.
        OverflowError: `total_cost` lies outside `-2**127` to `2**127 - 1`.

    Example:
        >>> kahlnws.Solution([2, 2, 2], [0, 1, 5], 14)
        Solution(flows=[2, 2, 2], potentials=[0, 1, 5], total_cost=14)
    """

    __match_args__ = ("flows", "potentials", "total_cost")
    def __new__(cls, flows: Iterable[int], potentials: Iterable[int], total_cost: int) -> Self: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __hash__(self) -> int: ...
    @property
    def flows(self) -> list[int]:
        """The flow of each arc, in units of flow, arc `a` at `a`: a list of `int`, each from 0 to `2**64 - 1`."""

    @property
    def potentials(self) -> list[int]:
        """Each node's potential, in units of cost, node `v` at `v`: a list of `int`, each from `-2**63` to `2**63 - 1`.
        With them, `costs[a] + potentials[tails[a]] - potentials[heads[a]]` is at least zero on every arc whose flow
        is below its capacity and at most zero on every arc whose flow is above its lower bound, which proves the flow
        optimal.
        """

    @property
    def total_cost(self) -> int:
        """The cost of the flow, in units of cost times units of flow: an exact `int` from `-2**127` to `2**127 - 1`."""

@final
class Table:
    """One of the per-arc tables of a problem whose length is compared with the tails', as `Breach.Length` names it.

    Its members are `Table.HEADS`, `Table.LOWER_BOUNDS`, `Table.CAPACITIES` and `Table.COSTS`; they compare and hash by
    value, convert to their `int` in that order from 0, and are copied and pickled. A table is not made by calling the
    class.
    """

    # A table is not made by calling the class: its members are the four below.
    def __int__(self) -> int: ...
    HEADS: ClassVar[Table]
    LOWER_BOUNDS: ClassVar[Table]
    CAPACITIES: ClassVar[Table]
    COSTS: ClassVar[Table]

@disjoint_base
class Breach:
    """Why a problem is no problem a solve admits: one class per kind of breach, whose attributes hold its values.

    The classes are `Breach.Length`, `Breach.TooLarge`, `Breach.TailOutsideNodes`, `Breach.HeadOutsideNodes`,
    `Breach.LowerAboveCapacity`, `Breach.CostBeyondBound`, `Breach.CostTotalBeyondRange`, `Breach.Unbalanced` and
    `Breach.NetSupplyBeyondRange`, in the order in which a problem's breaches are looked for. Each is made from its
    values, by keyword or by position; `Breach` itself is not made by calling it.

    Plain data: immutable, compared and hashed by value, matched by position in the order of its attributes, copied
    and pickled.

    Example:
        >>> match kahlnws.Breach.TailOutsideNodes(arc=2, tail=9, node_count=4):
        ...     case kahlnws.Breach.TailOutsideNodes(arc, tail):
        ...         print(arc, tail)
        2 9
    """

    # `Breach` is not made by calling it; each of its classes below is.
    @final
    class Length(Breach):
        """A per-arc table holds another number of entries than the tails: `table` has `length` entries where the tails
        have `expected`.
        """

        __match_args__ = ("table", "length", "expected")
        def __new__(cls, table: Table, length: int, expected: int) -> Self: ...
        @property
        def table(self) -> Table:
            """The table."""

        @property
        def length(self) -> int:
            """Its number of entries."""

        @property
        def expected(self) -> int:
            """The number of tails, one per arc."""

    @final
    class TooLarge(Breach):
        """The problem has more nodes or arcs than a solve counts: `node_count + 1` or `arc_count + node_count` passes
        `2**32 - 1`.
        """

        __match_args__ = ("node_count", "arc_count")
        def __new__(cls, node_count: int, arc_count: int) -> Self: ...
        @property
        def node_count(self) -> int:
            """The number of nodes, one per supply."""

        @property
        def arc_count(self) -> int:
            """The number of arcs, one per tail."""

    @final
    class TailOutsideNodes(Breach):
        """Arc `arc` leaves the node `tail`, which the problem of `node_count` nodes does not have."""

        __match_args__ = ("arc", "tail", "node_count")
        def __new__(cls, arc: int, tail: int, node_count: int) -> Self: ...
        @property
        def arc(self) -> int:
            """The arc."""

        @property
        def tail(self) -> int:
            """Its tail."""

        @property
        def node_count(self) -> int:
            """The number of nodes."""

    @final
    class HeadOutsideNodes(Breach):
        """Arc `arc` enters the node `head`, which the problem of `node_count` nodes does not have."""

        __match_args__ = ("arc", "head", "node_count")
        def __new__(cls, arc: int, head: int, node_count: int) -> Self: ...
        @property
        def arc(self) -> int:
            """The arc."""

        @property
        def head(self) -> int:
            """Its head."""

        @property
        def node_count(self) -> int:
            """The number of nodes."""

    @final
    class LowerAboveCapacity(Breach):
        """The lower bound of arc `arc` passes its capacity."""

        __match_args__ = ("arc", "lower_bound", "capacity")
        def __new__(cls, arc: int, lower_bound: int, capacity: int) -> Self: ...
        @property
        def arc(self) -> int:
            """The arc."""

        @property
        def lower_bound(self) -> int:
            """Its lower bound, in units of flow."""

        @property
        def capacity(self) -> int:
            """Its capacity, in units of flow."""

    @final
    class CostBeyondBound(Breach):
        """The cost of arc `arc` lies beyond plus or minus `cost_bound(node_count)`."""

        __match_args__ = ("arc", "cost", "node_count")
        def __new__(cls, arc: int, cost: int, node_count: int) -> Self: ...
        @property
        def arc(self) -> int:
            """The arc."""

        @property
        def cost(self) -> int:
            """Its cost, in units of cost."""

        @property
        def node_count(self) -> int:
            """The number of nodes, which sets the bound."""

    @final
    class CostTotalBeyondRange(Breach):
        """The magnitudes of the costs times the capacities, added up in arc order, pass `2**127 - 1` at arc `arc`."""

        __match_args__ = ("arc",)
        def __new__(cls, arc: int) -> Self: ...
        @property
        def arc(self) -> int:
            """The arc at which the sum passes `2**127 - 1`."""

    @final
    class Unbalanced(Breach):
        """The supplies add up to `total`, not zero."""

        __match_args__ = ("total",)
        def __new__(cls, total: int) -> Self: ...
        @property
        def total(self) -> int:
            """Their sum, in units of flow."""

    @final
    class NetSupplyBeyondRange(Breach):
        """The supplies net of the lower bounds add up to `total` where they are positive, past `2**64 - 2`. A node's
        net supply is its supply, less the lower bounds of the arcs that leave it, plus those of the arcs that enter
        it.
        """

        __match_args__ = ("total",)
        def __new__(cls, total: int) -> Self: ...
        @property
        def total(self) -> int:
            """Their sum, in units of flow."""

class ProblemError(ValueError):
    """A problem no solve admits, raised by `NetworkSimplex.solve` and `NetworkSimplex.solve_into`.

    The problem has a table of the wrong length, too many nodes or arcs, an arc whose numbers lie outside the range
    a solve admits, or supplies that do not balance. A problem with several breaches is refused with the first, in the
    order of the classes of `Breach`, where those about one arc are taken arc by arc: the lowest arc with any of them,
    and its first. A refused solve answers no work, and may still have grown the solver's room.

    Attributes:
        breach: The breach as a `Breach`, whose class names the kind and whose attributes hold the values; `None` on a
            `ProblemError` made by hand rather than raised by a solve.

    Example:
        >>> try:
        ...     kahlnws.NetworkSimplex().solve(tails=[0], heads=[5], capacities=[1], costs=[1], supplies=[1, -1])
        ... except kahlnws.ProblemError as error:
        ...     print(error)
        ...     print(repr(error.breach))
        arc 0 enters node 5, not below the node count 2
        Breach.HeadOutsideNodes(arc=0, head=5, node_count=2)
    """

    breach: Breach | None

def cost_bound(node_count: int) -> int:
    """Answer the largest magnitude a cost may have in a problem of `node_count` nodes.

    Every cost of a problem a solve admits lies within plus or minus it. It is `(2**62 - 1) // node_count`, and
    `2**62 - 1` for a problem of no node.

    Args:
        node_count: The number of nodes of the problem.

    Returns:
        The bound, an `int`.

    Raises:
        OverflowError: `node_count` is below zero or past the platform's `size_t`.

    Example:
        >>> kahlnws.cost_bound(1000)
        4611686018427387
    """
