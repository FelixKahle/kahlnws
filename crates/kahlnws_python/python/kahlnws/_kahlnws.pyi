# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The extension module of the package `kahlnws`, which re-exports every name it holds."""

from collections.abc import Iterable, Sequence
from types import GenericAlias
from typing import ClassVar, Generic, NamedTuple, Protocol, Self, SupportsIndex, TypeAlias, TypeVar, final

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

# What a solve found where a flow meets the supplies: a `Solution` from `solve`, the cost from `solve_into` and from
# `Network.solve`.
_Found_co = TypeVar("_Found_co", covariant=True)

__all__ = [
    "Answer",
    "Bounds",
    "Breach",
    "ChangeBreach",
    "ChangeError",
    "Network",
    "NetworkOptimum",
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
    did. The solver keeps its room and nothing else: an answer, its work included, is a function of the problem alone,
    whatever the solver solved before. The room is kept from one solve to the next and only grows: a solve of a problem
    with no more nodes and no more arcs than the room was made for, by `reserve` or by an earlier solve, allocates no
    room, and a solve that is refused may still have grown it. A problem held and changed, or loaded again with new
    numbers on the same arcs, is a `Network`'s.

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
    def reserve(self, node_count: SupportsIndex, arc_count: SupportsIndex) -> None:
        """Make room for problems of up to `node_count` nodes and `arc_count` arcs.

        A solve of such a problem allocates no room afterwards. Room already held is kept.

        Args:
            node_count: The most nodes a problem will have.
            arc_count: The most arcs a problem will have.

        Raises:
            MemoryError: The memory is refused; the room held before is kept, and the solver may be used.
            OverflowError: A count is below zero or past the platform's `size_t`.
            TypeError: A count is no integer.
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

        What an exporter, a sequence or an entry's `__index__` raises is passed on, with a note that names the argument.

        Example:
            >>> solver = kahlnws.NetworkSimplex()
            >>> solver.solve(tails=[0, 1], heads=[1, 0], capacities=[1, 5], costs=[1, 1], supplies=[2, -2])
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

        What an exporter, a sequence or an entry's `__index__` raises is passed on, with a note that names the argument.

        Example:
            >>> from array import array
            >>> flows = array("Q", [0, 0, 0])
            >>> answer = kahlnws.NetworkSimplex().solve_into(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2],
            ...                                             costs=[5, 1, 1], supplies=[4, 0, -4], flows=flows)
            >>> answer.outcome, flows.tolist()
            (14, [2, 2, 2])
        """

@final
class Network:
    """A network: a state that holds one min-cost flow problem and a basis of it, which its changes, loads and solves
    carry on from.

    `load` hands it a problem, by the columns `NetworkSimplex.solve` takes, read and copied as a solve reads them;
    `set_cost` changes the cost of one arc of the problem held, `set_capacity`, `set_lower_bound` and `set_bounds` one
    arc's bounds, `set_supply` the supply of one node, balanced by another, and `move_supply` moves supply from one node
    to another, each at once and only where the change reaches; `solve` answers an `Answer[int]`, whose outcome is the
    cost of the optimum of the problem as it stands, or `None` where no flow within the bounds meets the supplies,
    beside the `Work` the solve did, and `optimum()` answers that optimum, which reads its flows and its potentials from
    the network. It serves a caller that holds a problem and changes it, a few numbers at a time as a search does that
    tries a move and keeps it or sets it back, or all its numbers on the same arcs by a load, and solves it each time.
    `tail`, `head`, `cost`, `lower_bound`, `capacity`, `bounds` and `supply` read one arc's or one node's number of the
    problem as it stands, and `tails()`, `heads()`, `costs()`, `lower_bounds()`, `capacities()` and `supplies()` a whole
    column, as a list or, by their `_into` forms, into a caller's buffer: a caller keeps no copy of the problem of its
    own. A read changes nothing, needs no optimum, and ends none.

    Each setter answers the number it replaced, so setting that number back, or moving the supply back, makes the
    problem the earlier one again; the next solve answers its outcome and its cost, with a flow of that cost, which
    among tied optima may be another than before. A capacity of zero switches an arc off where its lower bound is zero,
    and the capacity it answered switches it on. A change that is refused raises `ChangeError` and changes nothing, and
    so does one that raises before; a change that sets the number already held changes nothing either, and keeps the
    optimum the network held; every other change ends it, and `optimum()` answers `None` until the next solve that finds
    one. A network never loaded holds the problem of no node, whose optimum is no flow at the cost zero, and no arc or
    node a change can name. A load that is refused, or that raises before, leaves the network as it was.

    An optimum reads the network it came from, and only as long as the network is as it was when the optimum was
    answered, as a borrow does in Rust: the next call of `load`, a setter, `move_supply`, `copy_from` or `solve` on the
    network, whatever that call answers or raises, ends every optimum answered before it, and a read of one then
    raises `RuntimeError`. So no read answers a number of an earlier state; `optimum()` answers the optimum held again.

    The network keeps the basis its last solve ended on, and a change keeps it a basis of the problem as it then stands:
    a solve after a few changes makes the pivots they call for. A load of new numbers on the same arcs carries that
    basis over, where it serves, and the solve after it answers the outcome and the cost a `NetworkSimplex.solve` of
    the problem answers. A change costs what it moves, which is not always little. A solve after changes that call for
    no pivot prices only the arcs those changes may have made worth using: none for a move of supply the flow held
    carries, the arcs near it for a cost. A solve that pivots prices at least one whole pass over the arcs. Where a
    change leaves much flow still to be sent, so that the solve makes many pivots, it can take longer than a load of
    the whole problem as it stands and the solve after it. Its answers are a function of the problem loaded and
    of the calls made since, in order: two networks given the same calls answer alike, their work included. The outcome
    and the cost are those of the problem as it stands, the potentials prove the flow optimal, and the flow among tied
    optima, the potentials and the work are this network's own; its solve tries no arcs first, so
    `work.first_arc_pivots` is 0. Where a `NetworkSimplex` admits the problem as it stands, it answers the same outcome
    and cost. After moves of supply and changes of lower bounds a network may hold numbers a solver and a load refuse:
    every supply stays within a signed 64-bit integer, but the supplies net of the lower bounds may add up past the
    limit `Breach.NetSupplyBeyondRange` names, where the arcs carry the difference; the network still solves them
    exactly.

    The room is kept and only grows: after `reserve`, a load of a problem within the room allocates no room, and no
    change, solve, read into a buffer, `NetworkOptimum.flows_into` or `NetworkOptimum.potentials_into` allocates any.
    `copy()` answers a copy that changes apart from the network, the earlier state itself where it is made before a
    change, and `copy_from` makes a network a copy of another within its own room, allocating nothing where that room
    holds the other's counts; each raises `MemoryError` where the memory for the copy is refused.

    The load and the solve run with the interpreter released, on the network's own copy of the problem: other threads
    run meanwhile, and nothing they do reaches it. A running solve cannot be interrupted; a `KeyboardInterrupt` is
    raised once it returns. One network serves one call at a time: a call on a network that is already in a call, from
    another thread or from code a column or an output runs while it is read, raises `RuntimeError`, and so does a read
    of its optimum. After a `PanicException`, which only a defect of the library raises, the network may be half
    changed: its changes, its solve and its reads of the problem raise it again, its optimum is `None`, and a load
    mends it. A network cannot be pickled.

    Example:
        >>> import kahlnws
        >>> network = kahlnws.Network()
        >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
        ...              supplies=[4, 0, -4])
        >>> network.solve().outcome
        14
        >>> had = network.set_cost(2, 3)
        >>> had, network.solve().outcome
        (1, 18)
    """

    def __init__(self) -> None: ...
    def reserve(self, node_count: SupportsIndex, arc_count: SupportsIndex) -> None:
        """Make room for problems of up to `node_count` nodes and `arc_count` arcs.

        A load of such a problem allocates no room afterwards. Room already held is kept, and nothing else changes:
        the problem held and its optimum stay, and an optimum answered before still reads.

        Args:
            node_count: The most nodes a problem will have.
            arc_count: The most arcs a problem will have.

        Raises:
            MemoryError: The memory is refused; the room held before is kept, and the network may be used.
            OverflowError: A count is below zero or past the platform's `size_t`.
            TypeError: A count is no integer.
            RuntimeError: The network is already in a call.

        Example:
            >>> kahlnws.Network().reserve(node_count=1000, arc_count=4000)
        """

    def load(
        self,
        *,
        tails: _Column,
        heads: _Column,
        capacities: _Column,
        costs: _Column,
        supplies: _Column,
        lower_bounds: _Column | None = None,
    ) -> None:
        """Hold the problem from now on, in place of the problem held, with a basis of it to start the next solve from.

        The network then holds no optimum until that solve finds one.

        The basis is the one the network holds, carried over to the new numbers, where it serves: where the network
        holds a basis of a problem with the same node count, the same arc count and the same tail and head at every arc,
        and the supplies net of the lower bounds and of what that basis carries on its arcs out of the tree add up to at
        most `2**64 - 2` where they are positive. Otherwise, and where the network holds none, never loaded or left half
        changed by a panic, it is the star, every arc at its lower bound, as the first load starts from. Either way the
        next solve prices every arc.

        The solve after a load answers the same outcome and the same cost whichever basis the load started from, those
        a `NetworkSimplex.solve` of the same problem answers; the flow among tied optima, the potentials and the work
        may differ, each a function of the calls made on the network. Where only some numbers changed since the problem
        the network held, the solve often makes far fewer pivots than one from the star; the same problem loaded again,
        from the network's optimum, makes none. So a network loaded day after day with new numbers on the same arcs
        solves each day from the basis the day before ended on.

        Every column is copied first, as `NetworkSimplex.solve` copies it; the load then runs with the interpreter
        released and costs a few passes over the arcs and the nodes. It allocates only where the network's room is
        smaller than the problem: room made by `reserve` or by an earlier load of a problem no smaller. A load whose
        problem is refused, or that raises before, leaves the network as it was, its basis and its optimum included,
        and the next load starts from that basis where it serves; it may still have grown the room.

        Args:
            tails: The node each arc leaves, one per arc.
            heads: The node each arc enters, one per arc.
            capacities: The most flow of each arc, one per arc.
            costs: The cost of a unit of flow on each arc, of either sign, one per arc.
            supplies: The supply of each node: positive where it sends flow, negative where it takes it.
            lower_bounds: The least flow of each arc, one per arc; left out where every arc's is zero.

        Raises:
            ProblemError: The problem is one no solve admits; its `breach` says why. A `ValueError`.
            TypeError: A column is neither a sequence nor a buffer of integers, an entry is no integer, or a buffer's
                items are no integers in the machine's byte order.
            OverflowError: An entry lies outside the range of its column.
            ValueError: A buffer has other than one dimension.
            MemoryError: The memory for the copy of a column or for the room of the problem is refused.
            RuntimeError: The network is already in a call.

        What an exporter, a sequence or an entry's `__index__` raises is passed on, with a note that names the argument.

        Example:
            >>> network = kahlnws.Network()
            >>> depot = dict(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1])
            >>> network.load(**depot, supplies=[4, 0, -4])
            >>> network.optimum(), network.solve().work.pivots
            (None, 4)
            >>> network.load(**depot, supplies=[3, 0, -3])
            >>> answer = network.solve()
            >>> answer.outcome, answer.work.pivots
            (9, 0)
        """

    @property
    def node_count(self) -> int:
        """The number of nodes of the problem held: 0 on a network never loaded."""

    @property
    def arc_count(self) -> int:
        """The number of arcs of the problem held: 0 on a network never loaded."""

    def set_cost(self, arc: SupportsIndex, cost: SupportsIndex) -> int:
        """Make `cost` the cost of a unit of flow on `arc`, and answer the cost it had.

        The network then holds the problem with that cost, from which the next `solve` answers the optimum of the
        problem as it now stands. The cost the arc already has changes nothing, and keeps the optimum the network held;
        any other cost ends it. Setting the cost answered back makes the problem the earlier one again. It costs a few
        steps where the change moves no node's potential, and otherwise the nodes whose potentials it moves, which can
        be most of them; the next solve prices the arc, and where potentials moved, the arcs at the smaller of the two
        groups of nodes, those whose potentials moved and the rest, which can be half the network.

        Args:
            arc: The arc, below the arc count.
            cost: The cost of a unit of flow on it, of either sign.

        Returns:
            The cost the arc had, an `int`.

        Raises:
            ChangeError: The cost lies beyond plus or minus `cost_bound(node_count)`, or with it the costs times the
                capacities, in magnitude, would add up past `2**127 - 1`; its `breach` says which. A `ValueError`.
            IndexError: `arc` is below zero or not below the arc count.
            OverflowError: `cost` lies outside the range of a signed 64-bit integer; the message names it, its value and
                the range.
            TypeError: `arc` or `cost` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.set_cost(2, 3), network.solve().outcome
            (1, 18)
        """

    def set_capacity(self, arc: SupportsIndex, capacity: SupportsIndex) -> int:
        """Make `capacity` the most flow of `arc`, and answer the capacity it had.

        The network then holds the problem with that capacity, from which the next `solve` answers the optimum of the
        problem as it now stands. The capacity the arc already has changes nothing, and keeps the optimum the network
        held; any other ends it. Setting the capacity answered back makes the problem the earlier one again. A capacity
        of zero on an arc whose lower bound is zero switches the arc off: no flow can use it, and the capacity it
        answered, set back, switches it on. A capacity below the arc's lower bound is refused; where both bounds are to
        move past each other's old value, `set_bounds` moves them at once. It costs a few steps where the arc's flow can
        stay where it is; otherwise it costs what `move_supply` of the flow the change moves costs, which is at most the
        change of the capacity. Switching off an arc that carries flow always moves it. The next solve prices the arc,
        and what that move of supply asks it to.

        Args:
            arc: The arc, below the arc count.
            capacity: The most flow of the arc.

        Returns:
            The capacity the arc had, an `int`.

        Raises:
            ChangeError: The capacity lies below the arc's lower bound, with it the costs times the capacities, in
                magnitude, would add up past `2**127 - 1`, or the flow still to be sent would pass `2**64 - 2`; its
                `breach` says which. A `ValueError`.
            IndexError: `arc` is below zero or not below the arc count.
            OverflowError: `capacity` lies outside the range of an unsigned 64-bit integer; the message names it, its
                value and the range.
            TypeError: `arc` or `capacity` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        The capacity answered, set back directly after, is admitted wherever the flow still to be sent before the change
        and twice the change of the capacity add up to at most `2**64 - 2`: never refused for numbers below `2**62`.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> had = network.set_capacity(1, 0)
            >>> had, network.solve().outcome
            (4, None)
            >>> network.set_capacity(1, had), network.solve().outcome
            (0, 14)
        """

    def set_lower_bound(self, arc: SupportsIndex, lower_bound: SupportsIndex) -> int:
        """Make `lower_bound` the least flow of `arc`, and answer the lower bound it had.

        The network then holds the problem with that lower bound, from which the next `solve` answers the optimum of
        the problem as it now stands; every flow it answers includes its arc's lower bound. The lower bound the arc
        already has changes nothing, and keeps the optimum the network held; any other ends it. Setting the lower bound
        answered back makes the problem the earlier one again. A lower bound above the arc's capacity is refused; where
        both bounds are to move past each other's old value, `set_bounds` moves them at once. A lower bound changes no
        supply: the flow it forces leaves the arc's tail and enters its head, as every flow does. The supplies net of
        the lower bounds may so come to pass the limit `Breach.NetSupplyBeyondRange` names, where the arcs carry the
        excess: the network still solves them exactly, where a `NetworkSimplex` and a load refuse them. It costs a few
        steps where the arc's flow can stay where it is; otherwise it costs what `move_supply` of the flow the change
        moves costs, which is at most the change of the lower bound. The next solve prices the arc, and what that move
        of supply asks it to.

        Args:
            arc: The arc, below the arc count.
            lower_bound: The least flow of the arc.

        Returns:
            The lower bound the arc had, an `int`.

        Raises:
            ChangeError: The lower bound lies above the arc's capacity, or the flow still to be sent would pass
                `2**64 - 2`; its `breach` says which. A `ValueError`.
            IndexError: `arc` is below zero or not below the arc count.
            OverflowError: `lower_bound` lies outside the range of an unsigned 64-bit integer; the message names it, its
                value and the range.
            TypeError: `arc` or `lower_bound` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        The lower bound answered, set back directly after, is admitted wherever the flow still to be sent before the
        change and twice the change of the lower bound add up to at most `2**64 - 2`: never refused for numbers below
        `2**62`.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.set_lower_bound(0, 3)
            0
            >>> network.solve().outcome, network.optimum().flows
            (17, [3, 1, 1])
        """

    def set_bounds(self, arc: SupportsIndex, bounds: Sequence[SupportsIndex]) -> Bounds:
        """Make `bounds` the least and the most flow of `arc` at once, and answer the bounds it had.

        `set_capacity` and `set_lower_bound` check each bound against the other the arc holds, so where both bounds move
        past each other's old value, the order of two calls matters and one order is refused: bounds of 0 to 2 become 3
        to 4 by the capacity first, and 3 to 4 become 0 to 2 by the lower bound first. This sets both at once, with no
        order, and checks the pair alone. Otherwise it is the two in one: the bounds the arc already has change nothing
        and keep the optimum the network held, any others end it, and the bounds answered, set back, make the problem
        the earlier one again. It costs a few steps where the arc's flow can stay where it is; otherwise it costs what
        `move_supply` of the flow the change moves costs, which is at most the larger change of the two bounds. The
        next solve prices the arc, and what that move of supply asks it to.

        Args:
            arc: The arc, below the arc count.
            bounds: The least and the most flow of the arc, in that order: a `Bounds`, or any pair of two integers,
                such as a tuple.

        Returns:
            The bounds the arc had, a `Bounds`, which set back makes the arc what it was.

        Raises:
            ChangeError: The lower bound lies above the capacity, with the capacity the costs times the capacities, in
                magnitude, would add up past `2**127 - 1`, or the flow still to be sent would pass `2**64 - 2`; its
                `breach` says which. A `ValueError`.
            IndexError: `arc` is below zero or not below the arc count.
            OverflowError: A bound lies outside the range of an unsigned 64-bit integer; the message names it, its value
                and the range.
            TypeError: `arc` or a bound is no integer: neither an `int` nor of a type with `__index__`; or `bounds` is
                no sequence.
            ValueError: `bounds` holds other than two items.
            RuntimeError: The network is already in a call.

        The bounds answered, set back directly after, are admitted wherever the flow still to be sent before the change
        and twice the larger change of the two bounds add up to at most `2**64 - 2`: never refused for numbers below
        `2**62`.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> had = network.set_bounds(2, (3, 4))
            >>> had, network.solve().outcome
            (Bounds(lower_bound=0, capacity=2), 8)
            >>> network.set_bounds(2, had), network.solve().outcome
            (Bounds(lower_bound=3, capacity=4), 14)
        """

    def set_supply(self, node: SupportsIndex, supply: SupportsIndex, balancing_node: SupportsIndex) -> int:
        """Make `supply` the supply of `node`, in units of flow, a demand below zero, and answer the supply it had; the
        supply of `balancing_node` moves by the same amount the other way, so the supplies still add up to zero.

        A depot that is to send 2 units where it sent 4, balanced by the customer it sends to, is
        `set_supply(depot, 2, customer)`: the customer then takes 2 less. The same call with the supply it answered sets
        both nodes back. The network then holds the problem with those supplies, from which the next `solve` answers the
        optimum of the problem as it now stands. The supply `node` already has changes nothing, is never refused, and
        keeps the optimum the network held, as a move of nothing does; any other ends it. A node that balances itself is
        given only the supply it has, which changes nothing; any other supply for it raises `ValueError`, as an index
        not below its count raises `IndexError`.

        It is `move_supply` of the difference between the supply asked for and the one `node` has: from
        `balancing_node` to `node` where the supply rises, from `node` to `balancing_node` where it falls. It costs and
        marks what that move costs and marks.

        Args:
            node: The node whose supply is set, below the node count.
            supply: Its new supply: positive where it sends flow, negative where it takes it.
            balancing_node: The node whose supply moves the other way, below the node count.

        Returns:
            The supply `node` had, an `int`.

        Raises:
            ChangeError: The supply of `balancing_node` would leave the range of a signed 64-bit integer, naming it
                with the supply it would have, or the flow still to be sent would pass `2**64 - 2`; its `breach` says
                which. A `ValueError`.
            IndexError: A node is below zero or not below the node count; the first such names it.
            ValueError: `node` is `balancing_node` and `supply` is not the supply it has; the message gives the node
                and both supplies.
            OverflowError: `supply` lies outside the range of a signed 64-bit integer; the message names it, its value
                and the range.
            TypeError: A node or `supply` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        The supply answered, set back directly after, is admitted where the move it makes would be, as `move_supply`
        says.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> had = network.set_supply(2, -5, 0)  # the customer to take 5, balanced by the depot
            >>> had, network.supplies(), network.solve().outcome
            (-4, [5, 0, -5], 19)
            >>> network.set_supply(2, had, 0), network.supplies(), network.solve().outcome
            (-5, [4, 0, -4], 14)
        """

    def move_supply(self, from_node: SupportsIndex, to_node: SupportsIndex, amount: SupportsIndex) -> None:
        """Lower the supply of `from_node` by `amount` and raise the supply of `to_node` by `amount`, in units of flow,
        so the supplies still add up to zero; a move of demand is written from the node that will demand more, a
        customer that needs two more units, sent by a depot, being `move_supply(customer, depot, 2)`.

        A node that sends gains supply, one that receives gains demand, a supply below zero. A node is brought to a
        supply it should have, balanced by another node, by `set_supply`. A caller that holds new supplies whole pairs
        each node whose supply rises with one whose supply falls, and moves the lesser of their two remaining
        differences, until none is left.

        The network then holds the problem with those supplies, from which the next `solve` answers the optimum of the
        problem as it now stands. The same node twice, or an amount of zero, moves nothing: it changes nothing, is never
        refused, and keeps the optimum the network held; any other move ends it. Moving the amount back,
        `move_supply(to_node, from_node, amount)`, makes the problem the earlier one again. Every supply a network comes
        to hold lies within a signed 64-bit integer, but the supplies net of the lower bounds may pass the limit
        `Breach.NetSupplyBeyondRange` names, where the arcs carry the excess: the network still solves them exactly,
        where a `NetworkSimplex` and a load refuse them.

        It costs a walk from each of the two nodes along the flow the network routes, as far as their routes meet, where
        that flow can carry the amount; where it cannot, the nodes whose potentials move, up to most of the network, and
        where flow comes to be left to be sent, a read of each node that heads a part of the network. The next solve
        prices no arc for a move the routed flow carried, and otherwise the arcs at the smaller group of the nodes whose
        potentials moved and the rest.

        Args:
            from_node: The node whose supply falls, below the node count.
            to_node: The node whose supply rises, below the node count.
            amount: The units of flow moved.

        Raises:
            ChangeError: The supply of `from_node` would fall below `-2**63`, or that of `to_node` pass `2**63 - 1`, or
                the flow still to be sent would pass `2**64 - 2`; its `breach` says which. A `ValueError`.
            IndexError: A node is below zero or not below the node count; the first such names it.
            OverflowError: `amount` lies outside the range of an unsigned 64-bit integer; the message names it, its
                value and the range.
            TypeError: A node or `amount` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        The move back, made directly after, is admitted wherever the move left the flow still to be sent as it was or
        lower, always where the routed flow carried the move, and wherever that flow before the move and twice the
        amount add up to at most `2**64 - 2`: never refused for numbers below `2**62`. Other calls in between can leave
        that flow higher, a solve that finds no flow among them; a move back that is refused stays refused until a
        solve that finds a flow or other moves lower it, and a load of the earlier problem always undoes it.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.solve().outcome
            14
            >>> network.move_supply(2, 0, 1)
            >>> answer = network.solve()
            >>> answer.outcome, answer.work.pivots
            (19, 0)
            >>> network.move_supply(0, 1, 2)  # the hub takes 2 of the depot's units and sends them on at 1 a unit
            >>> network.supplies(), network.solve().outcome
            ([3, 2, -5], 17)
        """

    def solve(self) -> Answer[int]:
        """Solve the problem held, and answer the cost of the optimum it found with the work it did.

        It pivots from the basis the network holds: the one its last solve ended on and the changes since left, or the
        one a load started from. A network nothing changed since its last solve answers the same again and makes no
        pivot. It runs with the interpreter released, and allocates no room. A network never loaded answers the
        cost zero of the problem of no node with no work. Where it finds an optimum, the network holds it from then on,
        and `optimum()` answers it, with its flows and its potentials, until the next load, change or solve.

        Returns:
            An `Answer[int]`: its `outcome` is the cost of the optimum found, an exact `int`, or `None` where no flow
            within the bounds meets the supplies; its `work` is the `Work` the solve did either way.

        Raises:
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.solve()
            Answer(outcome=14, work=Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0))
            >>> network.solve().work.pivots
            0
        """

    def optimum(self) -> NetworkOptimum | None:
        """Answer the optimum the network holds, or `None`.

        The network holds an optimum where the last call that loaded, changed or solved it was a solve that found one,
        and on a network never loaded, whose optimum is that of the problem of no node; it holds none after a load,
        after a change, and after a solve that found no flow. A change that is refused, or that sets the number already
        held, keeps the optimum, which this answers again.

        Returns:
            The `NetworkOptimum`, which reads the network until its next load, change or solve, or `None`.

        Raises:
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.optimum().total_cost
            0
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.optimum() is None
            True
        """

    def copy(self) -> Network:
        """Answer a copy of the network, which changes apart from it.

        The copy holds the problem, the basis and the optimum the network holds, reads as it, and answers every call as
        it would, its work included: made before a change, it is the earlier state itself, which a search may keep
        instead of setting numbers back. It allocates the copy; the copy's changes and solves allocate nothing, and its
        first load allocates its own copy of the columns. `copy_from` makes an existing network a copy within its room.
        `copy.copy` and `copy.deepcopy` answer the same.

        Returns:
            A new `Network`.

        Raises:
            MemoryError: The memory for the copy is refused; the network is as it was.
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> before = network.copy()
            >>> network.set_cost(2, 3)
            1
            >>> network.solve().outcome, before.solve().outcome
            (18, 14)
        """

    def copy_from(self, source: Network) -> None:
        """Make this network a copy of `source`, reusing its own room, which changes apart from it.

        The network then holds the problem, the basis and the optimum `source` holds, as `source.copy()` would, and
        every optimum this network answered before reads nothing, as after a load. Where its room holds a problem of
        `source`'s counts, made by `reserve` or an earlier load or copy, it allocates nothing: a search that keeps a
        network for each state it may return to restores one into a network it keeps, without allocating. A network
        made a copy of itself is as it was.

        Args:
            source: The network copied.

        Raises:
            MemoryError: The memory for the room of `source`'s counts is refused; this network then holds what it held,
                its optimum included, and its room may have grown.
            TypeError: `source` is no `Network`.
            RuntimeError: A network is already in a call.

        Example:
            >>> network, best = kahlnws.Network(), kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.solve().outcome
            14
            >>> best.reserve(node_count=3, arc_count=3)
            >>> best.copy_from(network)
            >>> network.set_cost(2, 3)
            1
            >>> network.copy_from(best)
            >>> network.costs(), network.solve().work.pivots
            ([5, 1, 1], 0)
        """

    def tail(self, arc: SupportsIndex) -> int:
        """Answer the tail of `arc`, the node it leaves.

        A read of the problem held: it changes nothing, needs no optimum, and keeps every optimum answered before it
        readable. The tails are those the network was loaded with, which no change moves.

        Args:
            arc: The arc, below the arc count.

        Returns:
            The tail, an `int`.

        Raises:
            IndexError: `arc` is below zero or not below the arc count.
            TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.tail(2), network.head(2)
            (1, 2)
        """

    def head(self, arc: SupportsIndex) -> int:
        """Answer the head of `arc`, the node it enters.

        A read of the problem held, as `tail` is.

        Args:
            arc: The arc, below the arc count.

        Returns:
            The head, an `int`.

        Raises:
            IndexError: `arc` is below zero or not below the arc count.
            TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.head(0)
            2
        """

    def cost(self, arc: SupportsIndex) -> int:
        """Answer the cost of a unit of flow on `arc`, as the network holds it.

        A read of the problem held, as `tail` is: the cost loaded, or the one `set_cost` set since, and never one a
        refused change named.

        Args:
            arc: The arc, below the arc count.

        Returns:
            The cost, an `int`.

        Raises:
            IndexError: `arc` is below zero or not below the arc count.
            TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.set_cost(2, 3)
            1
            >>> network.cost(2)
            3
        """

    def bounds(self, arc: SupportsIndex) -> Bounds:
        """Answer the lower bound and the capacity of `arc`, as the network holds them.

        A read of the problem held, as `tail` is: the bounds loaded, or those the setters of bounds set since, and never
        those a refused change named. They are what `set_bounds` takes and answers, so bounds read and set back make the
        arc what it was.

        Args:
            arc: The arc, below the arc count.

        Returns:
            The lower bound and the capacity, a `Bounds`.

        Raises:
            IndexError: `arc` is below zero or not below the arc count.
            TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> read = network.bounds(0)
            >>> read, read.capacity
            (Bounds(lower_bound=0, capacity=3), 3)
            >>> network.set_capacity(0, 7), network.bounds(0).capacity
            (3, 7)
            >>> network.set_bounds(0, read), network.bounds(0)
            (Bounds(lower_bound=0, capacity=7), Bounds(lower_bound=0, capacity=3))
        """

    def lower_bound(self, arc: SupportsIndex) -> int:
        """Answer the least flow of `arc`, as the network holds it.

        A read of the problem held, as `bounds` is: the lower bound loaded, zero where the load gave none, or the one
        the setters of bounds set since, and never one a refused change named.

        Args:
            arc: The arc, below the arc count.

        Returns:
            The lower bound, an `int`.

        Raises:
            IndexError: `arc` is below zero or not below the arc count.
            TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.set_lower_bound(0, 1), network.lower_bound(0)
            (0, 1)
        """

    def capacity(self, arc: SupportsIndex) -> int:
        """Answer the most flow of `arc`, as the network holds it.

        A read of the problem held, as `bounds` is: the capacity loaded, or the one the setters of bounds set since,
        and never one a refused change named.

        Args:
            arc: The arc, below the arc count.

        Returns:
            The capacity, an `int`.

        Raises:
            IndexError: `arc` is below zero or not below the arc count.
            TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.set_capacity(1, 0), network.capacity(1)
            (4, 0)
        """

    def supply(self, node: SupportsIndex) -> int:
        """Answer the supply of `node`, as the network holds it.

        A read of the problem held, as `tail` is: the supply loaded, with each move of supply made since, and none a
        refused move named.

        Args:
            node: The node, below the node count.

        Returns:
            The supply, an `int`: positive where the node sends flow, negative where it takes it.

        Raises:
            IndexError: `node` is below zero or not below the node count.
            TypeError: `node` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.move_supply(2, 0, 1)
            >>> network.supply(0), network.supply(2)
            (5, -5)
        """

    def tails(self) -> list[int]:
        """Answer the tail of each arc, arc `a`'s at `a`, as a new list.

        A read of the problem held, as `tail` is. The six columns read back, `tails()`, `heads()`, `capacities()`,
        `costs()`, `supplies()` and `lower_bounds()`, are the columns `load` takes, under the same names.

        Returns:
            A list of `int`.

        Raises:
            MemoryError: The memory for the list is refused.
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.tails(), network.heads()
            ([0, 0, 1], [2, 1, 2])
        """

    def tails_into(self, tails: _Room) -> None:
        """Write the tail of each arc into a buffer the caller owns.

        The buffer is checked and written as `NetworkOptimum.flows_into` checks and writes its own. The network
        allocates nothing; what a call still makes is a `memoryview` and an export record for the buffer.

        Args:
            tails: A writable buffer of one dimension with one unsigned integer as wide as `size_t` per arc (64-bit on
                the platforms tested), such as `numpy.empty(arc_count, dtype=numpy.uintp)` or `array.array("Q", ...)`.

        Raises:
            TypeError: `tails` is no buffer, or its items are not exactly unsigned integers as wide as `size_t`.
            ValueError: `tails` has other than one dimension, is read-only, of another length than the arc count, or
                strided with items that cannot be written as they lie.
            RuntimeError: The network is already in a call.

        What an exporter raises is passed on, with a note that names the argument.

        Example:
            >>> from array import array
            >>> network, tails = kahlnws.Network(), array("Q", [0, 0, 0])
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.tails_into(tails)
            >>> tails.tolist()
            [0, 0, 1]
        """

    def heads(self) -> list[int]:
        """Answer the head of each arc, arc `a`'s at `a`, as a new list.

        A read of the problem held, as `tail` is.

        Returns:
            A list of `int`.

        Raises:
            MemoryError: The memory for the list is refused.
            RuntimeError: The network is already in a call.
        """

    def heads_into(self, heads: _Room) -> None:
        """Write the head of each arc into a buffer the caller owns, as `tails_into` writes the tails.

        Args:
            heads: A writable buffer of one dimension with one unsigned integer as wide as `size_t` per arc.

        Raises:
            TypeError: `heads` is no buffer, or its items are not exactly unsigned integers as wide as `size_t`.
            ValueError: `heads` has other than one dimension, is read-only, of another length than the arc count, or
                strided with items that cannot be written as they lie.
            RuntimeError: The network is already in a call.
        """

    def capacities(self) -> list[int]:
        """Answer the most flow of each arc, arc `a`'s at `a`, as a new list.

        A read of the problem held, as `tail` is: the capacities loaded, with those the setters of bounds set since.

        Returns:
            A list of `int`.

        Raises:
            MemoryError: The memory for the list is refused.
            RuntimeError: The network is already in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.set_bounds(1, (1, 5))
            Bounds(lower_bound=0, capacity=4)
            >>> network.lower_bounds(), network.capacities()
            ([0, 1, 0], [3, 5, 2])
        """

    def capacities_into(self, capacities: _Room) -> None:
        """Write the most flow of each arc into a buffer the caller owns, as `tails_into` writes the tails.

        Args:
            capacities: A writable buffer of one dimension with one unsigned 64-bit integer per arc.

        Raises:
            TypeError: `capacities` is no buffer, or its items are not exactly unsigned 64-bit integers.
            ValueError: `capacities` has other than one dimension, is read-only, of another length than the arc count,
                or strided with items that cannot be written as they lie.
            RuntimeError: The network is already in a call.
        """

    def costs(self) -> list[int]:
        """Answer the cost of a unit of flow on each arc, arc `a`'s at `a`, as a new list.

        A read of the problem held, as `cost` is.

        Returns:
            A list of `int`.

        Raises:
            MemoryError: The memory for the list is refused.
            RuntimeError: The network is already in a call.
        """

    def costs_into(self, costs: _Room) -> None:
        """Write the cost of each arc into a buffer the caller owns, as `tails_into` writes the tails.

        Args:
            costs: A writable buffer of one dimension with one signed 64-bit integer per arc.

        Raises:
            TypeError: `costs` is no buffer, or its items are not exactly signed 64-bit integers.
            ValueError: `costs` has other than one dimension, is read-only, of another length than the arc count, or
                strided with items that cannot be written as they lie.
            RuntimeError: The network is already in a call.
        """

    def supplies(self) -> list[int]:
        """Answer the supply of each node, node `v`'s at `v`, as a new list.

        A read of the problem held, as `supply` is. The supplies add up to zero.

        Returns:
            A list of `int`.

        Raises:
            MemoryError: The memory for the list is refused.
            RuntimeError: The network is already in a call.
        """

    def supplies_into(self, supplies: _Room) -> None:
        """Write the supply of each node into a buffer the caller owns, as `tails_into` writes the tails.

        Args:
            supplies: A writable buffer of one dimension with one signed 64-bit integer per node.

        Raises:
            TypeError: `supplies` is no buffer, or its items are not exactly signed 64-bit integers.
            ValueError: `supplies` has other than one dimension, is read-only, of another length than the node count,
                or strided with items that cannot be written as they lie.
            RuntimeError: The network is already in a call.
        """

    def lower_bounds(self) -> list[int]:
        """Answer the least flow of each arc, arc `a`'s at `a`, as a new list.

        A read of the problem held, as `tail` is: the lower bounds loaded, zero where the load gave none, with those
        the setters of bounds set since.

        Returns:
            A list of `int`.

        Raises:
            MemoryError: The memory for the list is refused.
            RuntimeError: The network is already in a call.
        """

    def lower_bounds_into(self, lower_bounds: _Room) -> None:
        """Write the least flow of each arc into a buffer the caller owns, as `tails_into` writes the tails.

        Args:
            lower_bounds: A writable buffer of one dimension with one unsigned 64-bit integer per arc.

        Raises:
            TypeError: `lower_bounds` is no buffer, or its items are not exactly unsigned 64-bit integers.
            ValueError: `lower_bounds` has other than one dimension, is read-only, of another length than the arc
                count, or strided with items that cannot be written as they lie.
            RuntimeError: The network is already in a call.
        """

    def __copy__(self) -> Network: ...
    def __deepcopy__(self, memo: object, /) -> Network: ...

@final
class NetworkOptimum:
    """The optimum a network answered: a flow of least cost for the problem it holds, its cost, and the potentials that
    prove it optimal, read from the network.

    What `Network.optimum` answers where the network holds an optimum, after a solve that found one. It holds no number
    of its own: each read reads the network, and the flows are copied out only where they are asked for. It reads only
    while the network is as it was when it was answered: after the next call of `load`, a setter, `move_supply`,
    `copy_from` or `solve` on the network, whatever that call answered or raised, every read raises `RuntimeError`, so
    none answers a number of an earlier state; `Network.optimum` answers the optimum held again. Every number is that
    of the problem with its lower bounds: each flow includes its arc's lower bound, and the cost counts it.

    `flows` and `potentials` answer a new list at each read; `flows_into` and `potentials_into` write into buffers the
    caller owns, made once and reused, and allocate nothing in the network. An optimum is not made by calling the
    class, and cannot be copied or pickled.

    Example:
        >>> network = kahlnws.Network()
        >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
        ...              supplies=[4, 0, -4])
        >>> network.solve().outcome
        14
        >>> optimum = network.optimum()
        >>> optimum.total_cost, optimum.flows, optimum.potentials
        (14, [2, 2, 2], [0, 1, 5])
        >>> network.set_cost(2, 3)
        1
        >>> optimum.flows
        Traceback (most recent call last):
        ...
        RuntimeError: the network changed since this optimum was answered
    """

    # An optimum is not made by calling the class: `Network.optimum` answers one.
    @property
    def total_cost(self) -> int:
        """The cost of the flow, in units of cost times units of flow: an exact `int` from `-2**127` to `2**127 - 1`.
        Read from the network, it raises `RuntimeError` where the network changed since the optimum was answered, or
        is in a call.
        """

    def flow(self, arc: SupportsIndex) -> int:
        """Answer the flow of `arc`, in units of flow, its lower bound included.

        Args:
            arc: The arc, below the arc count.

        Returns:
            The flow, an `int` from 0 to `2**64 - 1`.

        Raises:
            IndexError: `arc` is below zero or not below the arc count.
            TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
            RuntimeError: The network changed since the optimum was answered, or is in a call.

        Example:
            >>> network = kahlnws.Network()
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.solve().outcome
            14
            >>> network.optimum().flow(0)
            2
        """

    @property
    def flows(self) -> list[int]:
        """The flow of each arc, in units of flow, its lower bound included, arc `a` at `a`: a list of `int`, each from
        0 to `2**64 - 1`. Each read builds a new list, which the caller owns; `flows_into` writes into a buffer instead.
        Read from the network, it raises `RuntimeError` where the network changed since the optimum was answered, or is
        in a call, and `MemoryError` where the memory for the list is refused.
        """

    def flows_into(self, flows: _Room) -> None:
        """Write the flow of each arc, its lower bound included, into a buffer the caller owns.

        The buffer is checked before anything is written (its type, shape, length and writability) and written by one
        copy; where the call raises, it holds what it held before. The network allocates nothing; what a call still
        makes is a `memoryview` and an export record for the buffer.

        Args:
            flows: A writable buffer of one dimension with one unsigned 64-bit integer per arc, such as
                `numpy.empty(arc_count, dtype=numpy.uint64)` or `array.array("Q", ...)`.

        Raises:
            TypeError: `flows` is no buffer, or its items are not exactly unsigned 64-bit integers.
            ValueError: `flows` has other than one dimension, is read-only, of another length than the arc count, or
                strided with items that cannot be written as they lie.
            RuntimeError: The network changed since the optimum was answered, or is in a call.

        What an exporter raises is passed on, with a note that names the argument.

        Example:
            >>> from array import array
            >>> network, flows = kahlnws.Network(), array("Q", [0, 0, 0])
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.solve().outcome
            14
            >>> network.optimum().flows_into(flows)
            >>> flows.tolist()
            [2, 2, 2]
        """

    @property
    def potentials(self) -> list[int]:
        """Each node's potential, in units of cost, node `v` at `v`: a list of `int`, each from `-2**63` to `2**63 - 1`.
        With them, `costs[a] + potentials[tails[a]] - potentials[heads[a]]` is at least zero on every arc whose flow is
        below its capacity and at most zero on every arc whose flow is above its lower bound, which proves the flow
        optimal. Each read builds a new list, which the caller owns; `potentials_into` writes into a buffer instead.
        Read from the network, it raises `RuntimeError` where the network changed since the optimum was answered, or is
        in a call, and `MemoryError` where the memory for the list is refused.
        """

    def potentials_into(self, potentials: _Room) -> None:
        """Write each node's potential into a buffer the caller owns.

        The buffer is checked and written as `flows_into` checks and writes its own. The network allocates nothing;
        what a call still makes is a `memoryview` and an export record for the buffer.

        Args:
            potentials: A writable buffer of one dimension with one signed 64-bit integer per node, such as
                `numpy.empty(node_count, dtype=numpy.int64)` or `array.array("q", ...)`.

        Raises:
            TypeError: `potentials` is no buffer, or its items are not exactly signed 64-bit integers.
            ValueError: `potentials` has other than one dimension, is read-only, of another length than the node count,
                or strided with items that cannot be written as they lie.
            RuntimeError: The network changed since the optimum was answered, or is in a call.

        What an exporter raises is passed on, with a note that names the argument.

        Example:
            >>> from array import array
            >>> network, potentials = kahlnws.Network(), array("q", [0, 0, 0])
            >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
            ...              supplies=[4, 0, -4])
            >>> network.solve().outcome
            14
            >>> network.optimum().potentials_into(potentials)
            >>> potentials.tolist()
            [0, 1, 5]
        """

class Bounds(NamedTuple):
    """The least and the most flow of an arc, in units of flow: a named tuple of two `int`, the lower bound first.

    What `Network.bounds` reads and `Network.set_bounds` answers, the bounds the arc had, in the order `set_bounds`
    takes them: bounds read or answered and set back make the arc what it was, `network.set_bounds(arc, had)`.
    `set_bounds` takes any pair of two integers, a plain tuple as well. A `Bounds` unpacks, compares and hashes as the
    tuple it is, reads its two numbers by their names, and copies and pickles.

    Args:
        lower_bound: The least flow of the arc.
        capacity: The most flow of the arc.

    Example:
        >>> bounds = kahlnws.Bounds(lower_bound=0, capacity=3)
        >>> bounds.capacity, bounds == (0, 3)
        (3, True)
    """

    lower_bound: int
    capacity: int

@final
class Work:
    """What one solve did, counted in pivots: exact counts of that solve, not samples or estimates.

    A pivot brings one arc that violates its bound into the basis and sends the most flow the cycle it closes with the
    tree carries around that cycle. Where a link of the tree has the least room, that link leaves and the arc takes its
    place; where the arc's own width is the least, no link leaves and the arc moves to its other bound, a flip, which
    is a pivot too.

    The counts of `solve` and `solve_into` depend on the problem as given, its arc order included, and on the version
    of the package, as the answer does: the same problem gives the same work from a fresh solver and from one that
    solved others before. Those of `Network.solve` depend on the problems the network loaded and on the calls made on it
    since. A solve answers `degenerate_pivots <= pivots` and `first_arc_pivots <= pivots`; the pivots that sent flow are
    `pivots - degenerate_pivots`. The block search that chooses the other pivots runs `pivots - first_arc_pivots + 1`
    times, once for each of them and once more to find that no arc violates its bound, so it is not counted apart.

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
        structure, each where it violated its bound when its turn came. At most `pivots`: 0 in a network's solve, which
        tries none, and `pivots` where those arcs alone reached the optimum.
        """

@final
class Answer(Generic[_Found_co]):
    """What a solve answers: what it found, and the work it did to find it, as one value.

    `NetworkSimplex.solve` answers an `Answer[Solution]`: its `outcome` is the `Solution`, or `None` where no flow meets
    the supplies. `NetworkSimplex.solve_into` answers an `Answer[int]`: its `outcome` is the cost of the flow written,
    or `None`. `Network.solve` answers an `Answer[int]` too: its `outcome` is the cost of the optimum found, which
    `Network.optimum` then reads with its flows and its potentials, or `None`. An `Answer[int]` holds a number as its
    outcome, so `answer.outcome == 14` says the cost is 14. The work is answered either way. A refused problem raises
    and answers nothing, since no solve ran.

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
        """What the solve found: the `Solution` of `solve`, or the cost of `solve_into` and of `Network.solve`, where a
        flow meets the supplies; `None` where none does.
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

@disjoint_base
class ChangeBreach:
    """Why a change is no change a network admits: one class per kind of breach, whose attributes hold its values.

    The classes are `ChangeBreach.LowerAboveCapacity`, `ChangeBreach.CostBeyondBound`,
    `ChangeBreach.CostTotalBeyondRange`, `ChangeBreach.SupplyBeyondRange` and `ChangeBreach.ExcessBeyondRange`. Each is
    made from its values, by keyword or by position; `ChangeBreach` itself is not made by calling it.

    Plain data: immutable, compared and hashed by value, matched by position in the order of its attributes, copied
    and pickled.

    Example:
        >>> match kahlnws.ChangeBreach.SupplyBeyondRange(node=2, supply=-(2**63) - 1):
        ...     case kahlnws.ChangeBreach.SupplyBeyondRange(node, supply):
        ...         print(node, supply)
        2 -9223372036854775809
    """

    # `ChangeBreach` is not made by calling it; each of its classes below is.
    @final
    class LowerAboveCapacity(ChangeBreach):
        """The lower bound set on arc `arc` passes its capacity, either set with it or the one it holds."""

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
    class CostBeyondBound(ChangeBreach):
        """The cost set on arc `arc` lies beyond plus or minus `cost_bound(node_count)`."""

        __match_args__ = ("arc", "cost", "node_count")
        def __new__(cls, arc: int, cost: int, node_count: int) -> Self: ...
        @property
        def arc(self) -> int:
            """The arc."""

        @property
        def cost(self) -> int:
            """The cost set on it, in units of cost."""

        @property
        def node_count(self) -> int:
            """The number of nodes, which sets the bound."""

    @final
    class CostTotalBeyondRange(ChangeBreach):
        """With the cost or the capacity set on arc `arc`, the magnitudes of the costs times the capacities would add up
        past `2**127 - 1`.
        """

        __match_args__ = ("arc",)
        def __new__(cls, arc: int) -> Self: ...
        @property
        def arc(self) -> int:
            """The arc changed."""

    @final
    class SupplyBeyondRange(ChangeBreach):
        """A move of supply would take the supply of node `node` past the range of a signed 64-bit integer: below
        `-2**63` for the node it is taken from, above `2**63 - 1` for the node it is added to.
        """

        __match_args__ = ("node", "supply")
        def __new__(cls, node: int, supply: int) -> Self: ...
        @property
        def node(self) -> int:
            """The node, the one the amount is taken from where both would leave the range."""

        @property
        def supply(self) -> int:
            """The supply it would have, in units of flow."""

    @final
    class ExcessBeyondRange(ChangeBreach):
        """A change would leave more flow still to be sent than a network holds between solves: what the last load or a
        solve that found no flow left to be sent, with what changes since moved that the arcs could not carry at once,
        plus this change's amount, adds up to `total`, past `2**64 - 2`. A load leaves to be sent what its start does
        not route, up to the whole of the positive supplies net of the lower bounds where it starts from no basis, so a
        change right after a load of supplies near that limit can be refused. A solve that finds a flow leaves none.
        """

        __match_args__ = ("total",)
        def __new__(cls, total: int) -> Self: ...
        @property
        def total(self) -> int:
            """That sum, in units of flow."""

class ProblemError(ValueError):
    """A problem no solve admits, raised by `NetworkSimplex.solve` and `solve_into`, and by `Network.load`.

    The problem has a table of the wrong length, too many nodes or arcs, an arc whose numbers lie outside the range
    a solve admits, or supplies that do not balance. A problem with several breaches is refused with the first, in the
    order of the classes of `Breach`, where those about one arc are taken arc by arc: the lowest arc with any of them,
    and its first. A refused solve answers no work, and may still have grown the solver's room; a refused load leaves
    the network as it was, and may still have grown its room. A change of a network is refused by `ChangeError`
    instead.

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

class ChangeError(ValueError):
    """A change no network admits, raised by `Network.set_cost`, `set_capacity`, `set_lower_bound`, `set_bounds`,
    `set_supply` and `move_supply`.

    The number set lies outside the range a solve admits: a cost past the bound of the node count, a lower bound above
    its capacity, the costs times the capacities past their range, a supply past a signed 64-bit integer, or more than
    `2**64 - 2` units of flow still to be sent. A refused change changes nothing: the network holds the problem it held,
    and the optimum it held. A breach a problem could have too reads alike, with the same name, values and message as
    the class of `Breach`, but is a class of `ChangeBreach`: a problem can be refused for what no change makes, and a
    change for what no problem meets.

    Attributes:
        breach: The breach as a `ChangeBreach`, whose class names the kind and whose attributes hold the values; `None`
            on a `ChangeError` made by hand rather than raised by a change.

    Example:
        >>> network = kahlnws.Network()
        >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4])
        >>> try:
        ...     network.set_lower_bound(0, 4)
        ... except kahlnws.ChangeError as error:
        ...     print(error)
        ...     print(repr(error.breach))
        arc 0 has lower bound 4 above its capacity 3
        ChangeBreach.LowerAboveCapacity(arc=0, lower_bound=4, capacity=3)
    """

    breach: ChangeBreach | None

def cost_bound(node_count: SupportsIndex) -> int:
    """Answer the largest magnitude a cost may have in a problem of `node_count` nodes.

    Every cost of a problem a solve admits lies within plus or minus it. It is `(2**62 - 1) // node_count`, and
    `2**62 - 1` for a problem of no node.

    Args:
        node_count: The number of nodes of the problem.

    Returns:
        The bound, an `int`.

    Raises:
        OverflowError: `node_count` is below zero or past the platform's `size_t`.
        TypeError: `node_count` is no integer.

    Example:
        >>> kahlnws.cost_bound(1000)
        4611686018427387
    """
