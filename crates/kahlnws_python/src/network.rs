// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The network Python holds: a problem kept by the library's network, its load, its changes and its solve, and the
//! optimum it answers, which reads the network for as long as no call has changed it since.

use std::collections::TryReserveError;

use pyo3::buffer::Element;
use pyo3::exceptions::{PyIndexError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::answer::Answer;
use crate::argument::{Count, FlowNumber, Index, Input, IntegerValue, Output};
use crate::bounds;
use crate::breach::refusal;
use crate::change_breach::change_refusal;
use crate::column::Held;
use crate::integer;
use crate::output::OutputBuffer;
use crate::tables::{Arguments, Tables, no_room, reserve_column};

/// A network: a state that holds one min-cost flow problem and a basis of it, which its changes, loads and solves
/// carry on from.
///
/// `load` hands it a problem, by the columns `NetworkSimplex.solve` takes, read and copied as a solve reads them;
/// `set_cost` changes the cost of one arc of the problem held, `set_capacity`, `set_lower_bound` and `set_bounds` one
/// arc's bounds, `set_supply` the supply of one node, balanced by another, and `move_supply` moves supply from one node
/// to another, each at once and only where the change reaches; `solve` answers an `Answer[int]`, whose outcome is the
/// cost of the optimum of the problem as it stands, or `None` where no flow within the bounds meets the supplies,
/// beside the `Work` the solve did, and `optimum()` answers that optimum, which reads its flows and its potentials from
/// the network. It serves a caller that holds a problem and changes it, a few numbers at a time as a search does that
/// tries a move and keeps it or sets it back, or all its numbers on the same arcs by a load, and solves it each time.
/// `tail`, `head`, `cost`, `lower_bound`, `capacity`, `bounds` and `supply` read one arc's or one node's number of the
/// problem as it stands, and `tails()`, `heads()`, `costs()`, `lower_bounds()`, `capacities()` and `supplies()` a whole
/// column, as a list or, by their `_into` forms, into a caller's buffer: a caller keeps no copy of the problem of its
/// own. A read changes nothing, needs no optimum, and ends none.
///
/// Each setter answers the number it replaced, so setting that number back, or moving the supply back, makes the
/// problem the earlier one again; the next solve answers its outcome and its cost, with a flow of that cost, which
/// among tied optima may be another than before. A capacity of zero switches an arc off where its lower bound is zero,
/// and the capacity it answered switches it on. A change that is refused raises `ChangeError` and changes nothing, and
/// so does one that raises before; a change that sets the number already held changes nothing either, and keeps the
/// optimum the network held; every other change ends it, and `optimum()` answers `None` until the next solve that finds
/// one. A network never loaded holds the problem of no node, whose optimum is no flow at the cost zero, and no arc or
/// node a change can name. A load that is refused, or that raises before, leaves the network as it was.
///
/// An optimum reads the network it came from, and only as long as the network is as it was when the optimum was
/// answered, as a borrow does in Rust: the next call of `load`, a setter, `move_supply`, `copy_from` or `solve` on the
/// network, whatever that call answers or raises, ends every optimum answered before it, and a read of one then
/// raises `RuntimeError`. So no read answers a number of an earlier state; `optimum()` answers the optimum held again.
///
/// The network keeps the basis its last solve ended on, and a change keeps it a basis of the problem as it then stands:
/// a solve after a few changes makes the pivots they call for. A load of new numbers on the same arcs carries that
/// basis over, where it serves, and the solve after it answers the outcome and the cost a `NetworkSimplex.solve` of
/// the problem answers. A change costs what it moves, which is not always little. A solve after changes that call for
/// no pivot prices only the arcs those changes may have made worth using: none for a move of supply the flow held
/// carries, the arcs near it for a cost. A solve that pivots prices at least one whole pass over the arcs. Where a
/// change leaves much flow still to be sent, so that the solve makes many pivots, it can take longer than a load of
/// the whole problem as it stands and the solve after it. Its answers are a function of the problem loaded and
/// of the calls made since, in order: two networks given the same calls answer alike, their work included. The outcome
/// and the cost are those of the problem as it stands, the potentials prove the flow optimal, and the flow among tied
/// optima, the potentials and the work are this network's own; its solve tries no arcs first, so
/// `work.first_arc_pivots` is 0. Where a `NetworkSimplex` admits the problem as it stands, it answers the same outcome
/// and cost. After moves of supply and changes of lower bounds a network may hold numbers a solver and a load refuse:
/// every supply stays within a signed 64-bit integer, but the supplies net of the lower bounds may add up past the
/// limit `Breach.NetSupplyBeyondRange` names, where the arcs carry the difference; the network still solves them
/// exactly.
///
/// The room is kept and only grows: after `reserve`, a load of a problem within the room allocates no room, and no
/// change, solve, read into a buffer, `NetworkOptimum.flows_into` or `NetworkOptimum.potentials_into` allocates any.
/// `copy()` answers a copy that changes apart from the network, the earlier state itself where it is made before a
/// change, and `copy_from` makes a network a copy of another within its own room, allocating nothing where that room
/// holds the other's counts; each raises `MemoryError` where the memory for the copy is refused.
///
/// The load and the solve run with the interpreter released, on the network's own copy of the problem: other threads
/// run meanwhile, and nothing they do reaches it. A running solve cannot be interrupted; a `KeyboardInterrupt` is
/// raised once it returns. One network serves one call at a time: a call on a network that is already in a call, from
/// another thread or from code a column or an output runs while it is read, raises `RuntimeError`, and so does a read
/// of its optimum. After a `PanicException`, which only a defect of the library raises, the network may be half
/// changed: its changes, its solve and its reads of the problem raise it again, its optimum is `None`, and a load
/// mends it. A network cannot be pickled.
///
/// Example:
///     >>> import kahlnws
///     >>> network = kahlnws.Network()
///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
///     ...              supplies=[4, 0, -4])
///     >>> network.solve().outcome
///     14
///     >>> had = network.set_cost(2, 3)
///     >>> had, network.solve().outcome
///     (1, 18)
#[pyclass(module = "kahlnws")]
#[derive(Debug, Default)]
pub(crate) struct Network {
    /// The library's network, with the rooms the columns it answers are written out of.
    held: HeldNetwork,
    /// The network's copy of the problem a load is handed.
    tables: Tables,
    /// The number of calls of `load`, a setter, `move_supply`, `copy_from` and `solve` this network has taken, refused
    /// or not: an optimum answered at another count may hold the numbers of an earlier state, and reads nothing.
    changes: u64,
}

/// The library's network, with the rooms the columns it answers are written out of.
#[derive(Debug, Default)]
struct HeldNetwork {
    /// The network, with its room.
    network: kahlnws::Network,
    /// The columns the network answers, copied out for a write into a caller's buffer.
    rooms: ColumnRooms,
}

/// One room for each type of a per-arc column a network writes into a caller's buffer by way of a copy of its own.
///
/// Each has room for every arc of the problem held wherever a load or a copy made the network hold it, so that a write
/// allocates nothing; its contents mean nothing between writes. The supplies and the potentials are written as the
/// network holds them, and need none.
#[derive(Debug, Default)]
struct ColumnRooms {
    /// Per arc: a flow, a lower bound or a capacity, in units of flow.
    unsigned: Vec<u64>,
    /// Per arc: a tail or a head.
    nodes: Vec<usize>,
    /// Per arc: a cost, in units of cost.
    signed: Vec<i64>,
}

/// The optimum a network answered: a flow of least cost for the problem it holds, its cost, and the potentials that
/// prove it optimal, read from the network.
///
/// What `Network.optimum` answers where the network holds an optimum, after a solve that found one. It holds no number
/// of its own: each read reads the network, and the flows are copied out only where they are asked for. It reads only
/// while the network is as it was when it was answered: after the next call of `load`, a setter, `move_supply`,
/// `copy_from` or `solve` on the network, whatever that call answered or raised, every read raises `RuntimeError`, so
/// none answers a number of an earlier state; `Network.optimum` answers the optimum held again. Every number is that
/// of the problem with its lower bounds: each flow includes its arc's lower bound, and the cost counts it.
///
/// `flows` and `potentials` answer a new list at each read; `flows_into` and `potentials_into` write into buffers the
/// caller owns, made once and reused, and allocate nothing in the network. An optimum is not made by calling the
/// class, and cannot be copied or pickled.
///
/// Example:
///     >>> network = kahlnws.Network()
///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
///     ...              supplies=[4, 0, -4])
///     >>> network.solve().outcome
///     14
///     >>> optimum = network.optimum()
///     >>> optimum.total_cost, optimum.flows, optimum.potentials
///     (14, [2, 2, 2], [0, 1, 5])
///     >>> network.set_cost(2, 3)
///     1
///     >>> optimum.flows
///     Traceback (most recent call last):
///     ...
///     RuntimeError: the network changed since this optimum was answered
#[pyclass(module = "kahlnws", frozen)]
#[derive(Debug)]
pub(crate) struct NetworkOptimum {
    /// The network it reads.
    network: Py<Network>,
    /// The network's count of changes when it was answered.
    changes: u64,
}

/// An arc or a node a call names, not below the count of the problem the network holds, or below zero. Raised as
/// `IndexError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
enum NotBelowCount {
    /// An arc, not below the arc count.
    #[error("arc {arc} is not below the arc count {arc_count}")]
    Arc {
        /// The arc, as the call was given it.
        arc: IntegerValue,
        /// The number of arcs.
        arc_count: usize,
    },
    /// A node, not below the node count.
    #[error("node {node} is not below the node count {node_count}")]
    Node {
        /// The node, as the call was given it.
        node: IntegerValue,
        /// The number of nodes.
        node_count: usize,
    },
}

#[pymethods]
impl Network {
    /// Make a network that holds the problem of no node and no room yet.
    #[new]
    fn new() -> Self {
        Self::default()
    }

    /// Make room for problems of up to `node_count` nodes and `arc_count` arcs.
    ///
    /// A load of such a problem allocates no room afterwards. Room already held is kept, and nothing else changes:
    /// the problem held and its optimum stay, and an optimum answered before still reads.
    ///
    /// Args:
    ///     node_count: The most nodes a problem will have.
    ///     arc_count: The most arcs a problem will have.
    ///
    /// Raises:
    ///     MemoryError: The memory is refused; the room held before is kept, and the network may be used.
    ///     OverflowError: A count is below zero or past the platform's `size_t`.
    ///     TypeError: A count is no integer.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> kahlnws.Network().reserve(node_count=1000, arc_count=4000)
    #[pyo3(signature = (node_count, arc_count))]
    fn reserve(slf: &Bound<'_, Self>, node_count: &Bound<'_, PyAny>, arc_count: &Bound<'_, PyAny>) -> PyResult<()> {
        let node_count = integer::count(Count::NodeCount, node_count)?;
        let arc_count = integer::count(Count::ArcCount, arc_count)?;
        let mut this = borrow(slf)?;
        let this = &mut *this;

        this.held
            .try_reserve(node_count, arc_count)
            .and_then(|()| this.tables.try_reserve(node_count, arc_count))
            .map_err(|_| no_room(node_count, arc_count))
    }

    /// Hold the problem from now on, in place of the problem held, with a basis of it to start the next solve from.
    ///
    /// The network then holds no optimum until that solve finds one.
    ///
    /// The basis is the one the network holds, carried over to the new numbers, where it serves: where the network
    /// holds a basis of a problem with the same node count, the same arc count and the same tail and head at every arc,
    /// and the supplies net of the lower bounds and of what that basis carries on its arcs out of the tree add up to at
    /// most `2**64 - 2` where they are positive. Otherwise, and where the network holds none, never loaded or left half
    /// changed by a panic, it is the star, every arc at its lower bound, as the first load starts from. Either way the
    /// next solve prices every arc.
    ///
    /// The solve after a load answers the same outcome and the same cost whichever basis the load started from, those
    /// a `NetworkSimplex.solve` of the same problem answers; the flow among tied optima, the potentials and the work
    /// may differ, each a function of the calls made on the network. Where only some numbers changed since the problem
    /// the network held, the solve often makes far fewer pivots than one from the star; the same problem loaded again,
    /// from the network's optimum, makes none. So a network loaded day after day with new numbers on the same arcs
    /// solves each day from the basis the day before ended on.
    ///
    /// Every column is copied first, as `NetworkSimplex.solve` copies it; the load then runs with the interpreter
    /// released and costs a few passes over the arcs and the nodes. It allocates only where the network's room is
    /// smaller than the problem: room made by `reserve` or by an earlier load of a problem no smaller. A load whose
    /// problem is refused, or that raises before, leaves the network as it was, its basis and its optimum included,
    /// and the next load starts from that basis where it serves; it may still have grown the room.
    ///
    /// Args:
    ///     tails: The node each arc leaves, one per arc.
    ///     heads: The node each arc enters, one per arc.
    ///     capacities: The most flow of each arc, one per arc.
    ///     costs: The cost of a unit of flow on each arc, of either sign, one per arc.
    ///     supplies: The supply of each node: positive where it sends flow, negative where it takes it.
    ///     lower_bounds: The least flow of each arc, one per arc; left out where every arc's is zero.
    ///
    /// Raises:
    ///     ProblemError: The problem is one no solve admits; its `breach` says why. A `ValueError`.
    ///     TypeError: A column is neither a sequence nor a buffer of integers, an entry is no integer, or a buffer's
    ///         items are no integers in the machine's byte order.
    ///     OverflowError: An entry lies outside the range of its column.
    ///     ValueError: A buffer has other than one dimension.
    ///     MemoryError: The memory for the copy of a column or for the room of the problem is refused.
    ///     RuntimeError: The network is already in a call.
    ///
    /// What an exporter, a sequence or an entry's `__index__` raises is passed on, with a note that names the argument.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> depot = dict(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1])
    ///     >>> network.load(**depot, supplies=[4, 0, -4])
    ///     >>> network.optimum(), network.solve().work.pivots
    ///     (None, 4)
    ///     >>> network.load(**depot, supplies=[3, 0, -3])
    ///     >>> answer = network.solve()
    ///     >>> answer.outcome, answer.work.pivots
    ///     (9, 0)
    #[pyo3(signature = (*, tails, heads, capacities, costs, supplies, lower_bounds = None))]
    fn load<'py>(
        slf: &Bound<'py, Self>,
        tails: Bound<'py, PyAny>,
        heads: Bound<'py, PyAny>,
        capacities: Bound<'py, PyAny>,
        costs: Bound<'py, PyAny>,
        supplies: Bound<'py, PyAny>,
        lower_bounds: Option<Bound<'py, PyAny>>,
    ) -> PyResult<()> {
        let arguments = Arguments { tails, heads, lower_bounds, capacities, costs, supplies };
        let mut this = changing(slf)?;
        let this = &mut *this;

        let problem = this.tables.read(&arguments)?;
        this.held.load(slf.py(), &problem)
    }

    /// The number of nodes of the problem held: 0 on a network never loaded.
    #[getter]
    fn node_count(slf: &Bound<'_, Self>) -> PyResult<usize> {
        Ok(borrow(slf)?.held.network.node_count())
    }

    /// The number of arcs of the problem held: 0 on a network never loaded.
    #[getter]
    fn arc_count(slf: &Bound<'_, Self>) -> PyResult<usize> {
        Ok(borrow(slf)?.held.network.arc_count())
    }

    /// Make `cost` the cost of a unit of flow on `arc`, and answer the cost it had.
    ///
    /// The network then holds the problem with that cost, from which the next `solve` answers the optimum of the
    /// problem as it now stands. The cost the arc already has changes nothing, and keeps the optimum the network held;
    /// any other cost ends it. Setting the cost answered back makes the problem the earlier one again. It costs a few
    /// steps where the change moves no node's potential, and otherwise the nodes whose potentials it moves, which can
    /// be most of them; the next solve prices the arc, and where potentials moved, the arcs at the smaller of the two
    /// groups of nodes, those whose potentials moved and the rest, which can be half the network.
    ///
    /// Args:
    ///     arc: The arc, below the arc count.
    ///     cost: The cost of a unit of flow on it, of either sign.
    ///
    /// Returns:
    ///     The cost the arc had, an `int`.
    ///
    /// Raises:
    ///     ChangeError: The cost lies beyond plus or minus `cost_bound(node_count)`, or with it the costs times the
    ///         capacities, in magnitude, would add up past `2**127 - 1`; its `breach` says which. A `ValueError`.
    ///     IndexError: `arc` is below zero or not below the arc count.
    ///     OverflowError: `cost` lies outside the range of a signed 64-bit integer; the message names it, its value and
    ///         the range.
    ///     TypeError: `arc` or `cost` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.set_cost(2, 3), network.solve().outcome
    ///     (1, 18)
    #[pyo3(signature = (arc, cost))]
    fn set_cost(slf: &Bound<'_, Self>, arc: &Bound<'_, PyAny>, cost: &Bound<'_, PyAny>) -> PyResult<i64> {
        let mut this = changing(slf)?;
        let arc = this.arc(arc)?;
        let cost = integer::cost(cost)?;

        this.held.network.set_cost(arc, cost).map_err(|breach| change_refusal(slf.py(), breach))
    }

    /// Make `capacity` the most flow of `arc`, and answer the capacity it had.
    ///
    /// The network then holds the problem with that capacity, from which the next `solve` answers the optimum of the
    /// problem as it now stands. The capacity the arc already has changes nothing, and keeps the optimum the network
    /// held; any other ends it. Setting the capacity answered back makes the problem the earlier one again. A capacity
    /// of zero on an arc whose lower bound is zero switches the arc off: no flow can use it, and the capacity it
    /// answered, set back, switches it on. A capacity below the arc's lower bound is refused; where both bounds are to
    /// move past each other's old value, `set_bounds` moves them at once. It costs a few steps where the arc's flow can
    /// stay where it is; otherwise it costs what `move_supply` of the flow the change moves costs, which is at most the
    /// change of the capacity. Switching off an arc that carries flow always moves it. The next solve prices the arc,
    /// and what that move of supply asks it to.
    ///
    /// Args:
    ///     arc: The arc, below the arc count.
    ///     capacity: The most flow of the arc.
    ///
    /// Returns:
    ///     The capacity the arc had, an `int`.
    ///
    /// Raises:
    ///     ChangeError: The capacity lies below the arc's lower bound, with it the costs times the capacities, in
    ///         magnitude, would add up past `2**127 - 1`, or the flow still to be sent would pass `2**64 - 2`; its
    ///         `breach` says which. A `ValueError`.
    ///     IndexError: `arc` is below zero or not below the arc count.
    ///     OverflowError: `capacity` lies outside the range of an unsigned 64-bit integer; the message names it, its
    ///         value and the range.
    ///     TypeError: `arc` or `capacity` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// The capacity answered, set back directly after, is admitted wherever the flow still to be sent before the change
    /// and twice the change of the capacity add up to at most `2**64 - 2`: never refused for numbers below `2**62`.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> had = network.set_capacity(1, 0)
    ///     >>> had, network.solve().outcome
    ///     (4, None)
    ///     >>> network.set_capacity(1, had), network.solve().outcome
    ///     (0, 14)
    #[pyo3(signature = (arc, capacity))]
    fn set_capacity(slf: &Bound<'_, Self>, arc: &Bound<'_, PyAny>, capacity: &Bound<'_, PyAny>) -> PyResult<u64> {
        let mut this = changing(slf)?;
        let arc = this.arc(arc)?;
        let capacity = integer::flow(FlowNumber::Capacity, capacity)?;

        this.held.network.set_capacity(arc, capacity).map_err(|breach| change_refusal(slf.py(), breach))
    }

    /// Make `lower_bound` the least flow of `arc`, and answer the lower bound it had.
    ///
    /// The network then holds the problem with that lower bound, from which the next `solve` answers the optimum of
    /// the problem as it now stands; every flow it answers includes its arc's lower bound. The lower bound the arc
    /// already has changes nothing, and keeps the optimum the network held; any other ends it. Setting the lower bound
    /// answered back makes the problem the earlier one again. A lower bound above the arc's capacity is refused; where
    /// both bounds are to move past each other's old value, `set_bounds` moves them at once. A lower bound changes no
    /// supply: the flow it forces leaves the arc's tail and enters its head, as every flow does. The supplies net of
    /// the lower bounds may so come to pass the limit `Breach.NetSupplyBeyondRange` names, where the arcs carry the
    /// excess: the network still solves them exactly, where a `NetworkSimplex` and a load refuse them. It costs a few
    /// steps where the arc's flow can stay where it is; otherwise it costs what `move_supply` of the flow the change
    /// moves costs, which is at most the change of the lower bound. The next solve prices the arc, and what that move
    /// of supply asks it to.
    ///
    /// Args:
    ///     arc: The arc, below the arc count.
    ///     lower_bound: The least flow of the arc.
    ///
    /// Returns:
    ///     The lower bound the arc had, an `int`.
    ///
    /// Raises:
    ///     ChangeError: The lower bound lies above the arc's capacity, or the flow still to be sent would pass
    ///         `2**64 - 2`; its `breach` says which. A `ValueError`.
    ///     IndexError: `arc` is below zero or not below the arc count.
    ///     OverflowError: `lower_bound` lies outside the range of an unsigned 64-bit integer; the message names it, its
    ///         value and the range.
    ///     TypeError: `arc` or `lower_bound` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// The lower bound answered, set back directly after, is admitted wherever the flow still to be sent before the
    /// change and twice the change of the lower bound add up to at most `2**64 - 2`: never refused for numbers below
    /// `2**62`.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.set_lower_bound(0, 3)
    ///     0
    ///     >>> network.solve().outcome, network.optimum().flows
    ///     (17, [3, 1, 1])
    #[pyo3(signature = (arc, lower_bound))]
    fn set_lower_bound(slf: &Bound<'_, Self>, arc: &Bound<'_, PyAny>, lower_bound: &Bound<'_, PyAny>) -> PyResult<u64> {
        let mut this = changing(slf)?;
        let arc = this.arc(arc)?;
        let lower_bound = integer::flow(FlowNumber::LowerBound, lower_bound)?;

        this.held.network.set_lower_bound(arc, lower_bound).map_err(|breach| change_refusal(slf.py(), breach))
    }

    /// Make `bounds` the least and the most flow of `arc` at once, and answer the bounds it had.
    ///
    /// `set_capacity` and `set_lower_bound` check each bound against the other the arc holds, so where both bounds move
    /// past each other's old value, the order of two calls matters and one order is refused: bounds of 0 to 2 become 3
    /// to 4 by the capacity first, and 3 to 4 become 0 to 2 by the lower bound first. This sets both at once, with no
    /// order, and checks the pair alone. Otherwise it is the two in one: the bounds the arc already has change nothing
    /// and keep the optimum the network held, any others end it, and the bounds answered, set back, make the problem
    /// the earlier one again. It costs a few steps where the arc's flow can stay where it is; otherwise it costs what
    /// `move_supply` of the flow the change moves costs, which is at most the larger change of the two bounds. The
    /// next solve prices the arc, and what that move of supply asks it to.
    ///
    /// Args:
    ///     arc: The arc, below the arc count.
    ///     bounds: The least and the most flow of the arc, in that order: a `Bounds`, or any pair of two integers,
    ///         such as a tuple.
    ///
    /// Returns:
    ///     The bounds the arc had, a `Bounds`, which set back makes the arc what it was.
    ///
    /// Raises:
    ///     ChangeError: The lower bound lies above the capacity, with the capacity the costs times the capacities, in
    ///         magnitude, would add up past `2**127 - 1`, or the flow still to be sent would pass `2**64 - 2`; its
    ///         `breach` says which. A `ValueError`.
    ///     IndexError: `arc` is below zero or not below the arc count.
    ///     OverflowError: A bound lies outside the range of an unsigned 64-bit integer; the message names it, its value
    ///         and the range.
    ///     TypeError: `arc` or a bound is no integer: neither an `int` nor of a type with `__index__`; or `bounds` is
    ///         no sequence.
    ///     ValueError: `bounds` holds other than two items.
    ///     RuntimeError: The network is already in a call.
    ///
    /// The bounds answered, set back directly after, are admitted wherever the flow still to be sent before the change
    /// and twice the larger change of the two bounds add up to at most `2**64 - 2`: never refused for numbers below
    /// `2**62`.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> had = network.set_bounds(2, (3, 4))
    ///     >>> had, network.solve().outcome
    ///     (Bounds(lower_bound=0, capacity=2), 8)
    ///     >>> network.set_bounds(2, had), network.solve().outcome
    ///     (Bounds(lower_bound=3, capacity=4), 14)
    #[pyo3(signature = (arc, bounds))]
    fn set_bounds<'py>(
        slf: &Bound<'py, Self>,
        arc: &Bound<'_, PyAny>,
        bounds: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let py = slf.py();
        let mut this = changing(slf)?;
        let arc = this.arc(arc)?;
        let bounds = bounds::from_python(bounds)?;

        let had = this.held.network.set_bounds(arc, bounds).map_err(|breach| change_refusal(py, breach))?;
        bounds::to_python(py, had)
    }

    /// Make `supply` the supply of `node`, in units of flow, a demand below zero, and answer the supply it had; the
    /// supply of `balancing_node` moves by the same amount the other way, so the supplies still add up to zero.
    ///
    /// A depot that is to send 2 units where it sent 4, balanced by the customer it sends to, is
    /// `set_supply(depot, 2, customer)`: the customer then takes 2 less. The same call with the supply it answered sets
    /// both nodes back. The network then holds the problem with those supplies, from which the next `solve` answers the
    /// optimum of the problem as it now stands. The supply `node` already has changes nothing, is never refused, and
    /// keeps the optimum the network held, as a move of nothing does; any other ends it. A node that balances itself is
    /// given only the supply it has, which changes nothing; any other supply for it raises `ValueError`, as an index
    /// not below its count raises `IndexError`.
    ///
    /// It is `move_supply` of the difference between the supply asked for and the one `node` has: from
    /// `balancing_node` to `node` where the supply rises, from `node` to `balancing_node` where it falls. It costs and
    /// marks what that move costs and marks.
    ///
    /// Args:
    ///     node: The node whose supply is set, below the node count.
    ///     supply: Its new supply: positive where it sends flow, negative where it takes it.
    ///     balancing_node: The node whose supply moves the other way, below the node count.
    ///
    /// Returns:
    ///     The supply `node` had, an `int`.
    ///
    /// Raises:
    ///     ChangeError: The supply of `balancing_node` would leave the range of a signed 64-bit integer, naming it
    ///         with the supply it would have, or the flow still to be sent would pass `2**64 - 2`; its `breach` says
    ///         which. A `ValueError`.
    ///     IndexError: A node is below zero or not below the node count; the first such names it.
    ///     ValueError: `node` is `balancing_node` and `supply` is not the supply it has; the message gives the node
    ///         and both supplies.
    ///     OverflowError: `supply` lies outside the range of a signed 64-bit integer; the message names it, its value
    ///         and the range.
    ///     TypeError: A node or `supply` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// The supply answered, set back directly after, is admitted where the move it makes would be, as `move_supply`
    /// says.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> had = network.set_supply(2, -5, 0)  # the customer to take 5, balanced by the depot
    ///     >>> had, network.supplies(), network.solve().outcome
    ///     (-4, [5, 0, -5], 19)
    ///     >>> network.set_supply(2, had, 0), network.supplies(), network.solve().outcome
    ///     (-5, [4, 0, -4], 14)
    #[pyo3(signature = (node, supply, balancing_node))]
    fn set_supply(
        slf: &Bound<'_, Self>,
        node: &Bound<'_, PyAny>,
        supply: &Bound<'_, PyAny>,
        balancing_node: &Bound<'_, PyAny>,
    ) -> PyResult<i64> {
        let mut this = changing(slf)?;
        let node = this.node(Index::Node, node)?;
        let supply = integer::supply(supply)?;
        let balancing_node = this.node(Index::BalancingNode, balancing_node)?;
        balanced_by_another(&this.held.network, node, supply, balancing_node)?;

        this.held.network.set_supply(node, supply, balancing_node).map_err(|breach| change_refusal(slf.py(), breach))
    }

    /// Lower the supply of `from_node` by `amount` and raise the supply of `to_node` by `amount`, in units of flow, so
    /// the supplies still add up to zero; a move of demand is written from the node that will demand more, a customer
    /// that needs two more units, sent by a depot, being `move_supply(customer, depot, 2)`.
    ///
    /// A node that sends gains supply, one that receives gains demand, a supply below zero. A node is brought to a
    /// supply it should have, balanced by another node, by `set_supply`. A caller that holds new supplies whole pairs
    /// each node whose supply rises with one whose supply falls, and moves the lesser of their two remaining
    /// differences, until none is left.
    ///
    /// The network then holds the problem with those supplies, from which the next `solve` answers the optimum of the
    /// problem as it now stands. The same node twice, or an amount of zero, moves nothing: it changes nothing, is never
    /// refused, and keeps the optimum the network held; any other move ends it. Moving the amount back,
    /// `move_supply(to_node, from_node, amount)`, makes the problem the earlier one again. Every supply a network comes
    /// to hold lies within a signed 64-bit integer, but the supplies net of the lower bounds may pass the limit
    /// `Breach.NetSupplyBeyondRange` names, where the arcs carry the excess: the network still solves them exactly,
    /// where a `NetworkSimplex` and a load refuse them.
    ///
    /// It costs a walk from each of the two nodes along the flow the network routes, as far as their routes meet, where
    /// that flow can carry the amount; where it cannot, the nodes whose potentials move, up to most of the network, and
    /// where flow comes to be left to be sent, a read of each node that heads a part of the network. The next solve
    /// prices no arc for a move the routed flow carried, and otherwise the arcs at the smaller group of the nodes whose
    /// potentials moved and the rest.
    ///
    /// Args:
    ///     from_node: The node whose supply falls, below the node count.
    ///     to_node: The node whose supply rises, below the node count.
    ///     amount: The units of flow moved.
    ///
    /// Raises:
    ///     ChangeError: The supply of `from_node` would fall below `-2**63`, or that of `to_node` pass `2**63 - 1`, or
    ///         the flow still to be sent would pass `2**64 - 2`; its `breach` says which. A `ValueError`.
    ///     IndexError: A node is below zero or not below the node count; the first such names it.
    ///     OverflowError: `amount` lies outside the range of an unsigned 64-bit integer; the message names it, its
    ///         value and the range.
    ///     TypeError: A node or `amount` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// The move back, made directly after, is admitted wherever the move left the flow still to be sent as it was or
    /// lower, always where the routed flow carried the move, and wherever that flow before the move and twice the
    /// amount add up to at most `2**64 - 2`: never refused for numbers below `2**62`. Other calls in between can leave
    /// that flow higher, a solve that finds no flow among them; a move back that is refused stays refused until a
    /// solve that finds a flow or other moves lower it, and a load of the earlier problem always undoes it.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.solve().outcome
    ///     14
    ///     >>> network.move_supply(2, 0, 1)
    ///     >>> answer = network.solve()
    ///     >>> answer.outcome, answer.work.pivots
    ///     (19, 0)
    ///     >>> network.move_supply(0, 1, 2)  # the hub takes 2 of the depot's units and sends them on at 1 a unit
    ///     >>> network.supplies(), network.solve().outcome
    ///     ([3, 2, -5], 17)
    #[pyo3(signature = (from_node, to_node, amount))]
    fn move_supply(
        slf: &Bound<'_, Self>,
        from_node: &Bound<'_, PyAny>,
        to_node: &Bound<'_, PyAny>,
        amount: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let mut this = changing(slf)?;
        let from_node = this.node(Index::FromNode, from_node)?;
        let to_node = this.node(Index::ToNode, to_node)?;
        let amount = integer::flow(FlowNumber::Amount, amount)?;

        this.held.network.move_supply(from_node, to_node, amount).map_err(|breach| change_refusal(slf.py(), breach))
    }

    /// Solve the problem held, and answer the cost of the optimum it found with the work it did.
    ///
    /// It pivots from the basis the network holds: the one its last solve ended on and the changes since left, or the
    /// one a load started from. A network nothing changed since its last solve answers the same again and makes no
    /// pivot. It runs with the interpreter released, and allocates no room. A network never loaded answers the
    /// cost zero of the problem of no node with no work. Where it finds an optimum, the network holds it from then on,
    /// and `optimum()` answers it, with its flows and its potentials, until the next load, change or solve.
    ///
    /// Returns:
    ///     An `Answer[int]`: its `outcome` is the cost of the optimum found, an exact `int`, or `None` where no flow
    ///     within the bounds meets the supplies; its `work` is the `Work` the solve did either way.
    ///
    /// Raises:
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.solve()
    ///     Answer(outcome=14, work=Work(pivots=4, degenerate_pivots=1, first_arc_pivots=0))
    ///     >>> network.solve().work.pivots
    ///     0
    fn solve(slf: &Bound<'_, Self>) -> PyResult<Answer> {
        let py = slf.py();
        let mut this = changing(slf)?;

        let kahlnws::Answer { outcome, work } = this.held.solve(py);
        Answer::answered(py, outcome.optimal(), work)
    }

    /// Answer the optimum the network holds, or `None`.
    ///
    /// The network holds an optimum where the last call that loaded, changed or solved it was a solve that found one,
    /// and on a network never loaded, whose optimum is that of the problem of no node; it holds none after a load,
    /// after a change, and after a solve that found no flow. A change that is refused, or that sets the number already
    /// held, keeps the optimum, which this answers again.
    ///
    /// Returns:
    ///     The `NetworkOptimum`, which reads the network until its next load, change or solve, or `None`.
    ///
    /// Raises:
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.optimum().total_cost
    ///     0
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.optimum() is None
    ///     True
    fn optimum(slf: &Bound<'_, Self>) -> PyResult<Option<NetworkOptimum>> {
        let this = borrow(slf)?;
        let changes = this.changes;

        Ok(this.held.network.optimum().map(|_| NetworkOptimum { network: slf.clone().unbind(), changes }))
    }

    /// Answer a copy of the network, which changes apart from it.
    ///
    /// The copy holds the problem, the basis and the optimum the network holds, reads as it, and answers every call as
    /// it would, its work included: made before a change, it is the earlier state itself, which a search may keep
    /// instead of setting numbers back. It allocates the copy; the copy's changes and solves allocate nothing, and its
    /// first load allocates its own copy of the columns. `copy_from` makes an existing network a copy within its room.
    /// `copy.copy` and `copy.deepcopy` answer the same.
    ///
    /// Returns:
    ///     A new `Network`.
    ///
    /// Raises:
    ///     MemoryError: The memory for the copy is refused; the network is as it was.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> before = network.copy()
    ///     >>> network.set_cost(2, 3)
    ///     1
    ///     >>> network.solve().outcome, before.solve().outcome
    ///     (18, 14)
    fn copy(slf: &Bound<'_, Self>) -> PyResult<Self> {
        let this = borrow(slf)?;

        Ok(Self { held: this.held.copy()?, tables: Tables::default(), changes: 0 })
    }

    /// Make this network a copy of `source`, reusing its own room, which changes apart from it.
    ///
    /// The network then holds the problem, the basis and the optimum `source` holds, as `source.copy()` would, and
    /// every optimum this network answered before reads nothing, as after a load. Where its room holds a problem of
    /// `source`'s counts, made by `reserve` or an earlier load or copy, it allocates nothing: a search that keeps a
    /// network for each state it may return to restores one into a network it keeps, without allocating. A network
    /// made a copy of itself is as it was.
    ///
    /// Args:
    ///     source: The network copied.
    ///
    /// Raises:
    ///     MemoryError: The memory for the room of `source`'s counts is refused; this network then holds what it held,
    ///         its optimum included, and its room may have grown.
    ///     TypeError: `source` is no `Network`.
    ///     RuntimeError: A network is already in a call.
    ///
    /// Example:
    ///     >>> network, best = kahlnws.Network(), kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.solve().outcome
    ///     14
    ///     >>> best.reserve(node_count=3, arc_count=3)
    ///     >>> best.copy_from(network)
    ///     >>> network.set_cost(2, 3)
    ///     1
    ///     >>> network.copy_from(best)
    ///     >>> network.costs(), network.solve().work.pivots
    ///     ([5, 1, 1], 0)
    #[pyo3(signature = (source))]
    fn copy_from(slf: &Bound<'_, Self>, source: &Bound<'_, Self>) -> PyResult<()> {
        let mut this = changing(slf)?;
        if slf.is(source) {
            return Ok(());
        }

        let source = borrow(source)?;
        this.held.copy_from(&source.held)
    }

    /// Answer the tail of `arc`, the node it leaves.
    ///
    /// A read of the problem held: it changes nothing, needs no optimum, and keeps every optimum answered before it
    /// readable. The tails are those the network was loaded with, which no change moves.
    ///
    /// Args:
    ///     arc: The arc, below the arc count.
    ///
    /// Returns:
    ///     The tail, an `int`.
    ///
    /// Raises:
    ///     IndexError: `arc` is below zero or not below the arc count.
    ///     TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.tail(2), network.head(2)
    ///     (1, 2)
    #[pyo3(signature = (arc))]
    fn tail(slf: &Bound<'_, Self>, arc: &Bound<'_, PyAny>) -> PyResult<usize> {
        let this = borrow(slf)?;
        let arc = this.arc(arc)?;

        Ok(this.held.network.tail(arc))
    }

    /// Answer the head of `arc`, the node it enters.
    ///
    /// A read of the problem held, as `tail` is.
    ///
    /// Args:
    ///     arc: The arc, below the arc count.
    ///
    /// Returns:
    ///     The head, an `int`.
    ///
    /// Raises:
    ///     IndexError: `arc` is below zero or not below the arc count.
    ///     TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.head(0)
    ///     2
    #[pyo3(signature = (arc))]
    fn head(slf: &Bound<'_, Self>, arc: &Bound<'_, PyAny>) -> PyResult<usize> {
        let this = borrow(slf)?;
        let arc = this.arc(arc)?;

        Ok(this.held.network.head(arc))
    }

    /// Answer the cost of a unit of flow on `arc`, as the network holds it.
    ///
    /// A read of the problem held, as `tail` is: the cost loaded, or the one `set_cost` set since, and never one a
    /// refused change named.
    ///
    /// Args:
    ///     arc: The arc, below the arc count.
    ///
    /// Returns:
    ///     The cost, an `int`.
    ///
    /// Raises:
    ///     IndexError: `arc` is below zero or not below the arc count.
    ///     TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.set_cost(2, 3)
    ///     1
    ///     >>> network.cost(2)
    ///     3
    #[pyo3(signature = (arc))]
    fn cost(slf: &Bound<'_, Self>, arc: &Bound<'_, PyAny>) -> PyResult<i64> {
        let this = borrow(slf)?;
        let arc = this.arc(arc)?;

        Ok(this.held.network.cost(arc))
    }

    /// Answer the lower bound and the capacity of `arc`, as the network holds them.
    ///
    /// A read of the problem held, as `tail` is: the bounds loaded, or those the setters of bounds set since, and never
    /// those a refused change named. They are what `set_bounds` takes and answers, so bounds read and set back make the
    /// arc what it was.
    ///
    /// Args:
    ///     arc: The arc, below the arc count.
    ///
    /// Returns:
    ///     The lower bound and the capacity, a `Bounds`.
    ///
    /// Raises:
    ///     IndexError: `arc` is below zero or not below the arc count.
    ///     TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> read = network.bounds(0)
    ///     >>> read, read.capacity
    ///     (Bounds(lower_bound=0, capacity=3), 3)
    ///     >>> network.set_capacity(0, 7), network.bounds(0).capacity
    ///     (3, 7)
    ///     >>> network.set_bounds(0, read), network.bounds(0)
    ///     (Bounds(lower_bound=0, capacity=7), Bounds(lower_bound=0, capacity=3))
    #[pyo3(signature = (arc))]
    fn bounds<'py>(slf: &Bound<'py, Self>, arc: &Bound<'_, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        bounds::to_python(slf.py(), Self::arc_bounds(slf, arc)?)
    }

    /// Answer the least flow of `arc`, as the network holds it.
    ///
    /// A read of the problem held, as `bounds` is: the lower bound loaded, zero where the load gave none, or the one
    /// the setters of bounds set since, and never one a refused change named.
    ///
    /// Args:
    ///     arc: The arc, below the arc count.
    ///
    /// Returns:
    ///     The lower bound, an `int`.
    ///
    /// Raises:
    ///     IndexError: `arc` is below zero or not below the arc count.
    ///     TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.set_lower_bound(0, 1), network.lower_bound(0)
    ///     (0, 1)
    #[pyo3(signature = (arc))]
    fn lower_bound(slf: &Bound<'_, Self>, arc: &Bound<'_, PyAny>) -> PyResult<u64> {
        Ok(Self::arc_bounds(slf, arc)?.lower_bound)
    }

    /// Answer the most flow of `arc`, as the network holds it.
    ///
    /// A read of the problem held, as `bounds` is: the capacity loaded, or the one the setters of bounds set since,
    /// and never one a refused change named.
    ///
    /// Args:
    ///     arc: The arc, below the arc count.
    ///
    /// Returns:
    ///     The capacity, an `int`.
    ///
    /// Raises:
    ///     IndexError: `arc` is below zero or not below the arc count.
    ///     TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.set_capacity(1, 0), network.capacity(1)
    ///     (4, 0)
    #[pyo3(signature = (arc))]
    fn capacity(slf: &Bound<'_, Self>, arc: &Bound<'_, PyAny>) -> PyResult<u64> {
        Ok(Self::arc_bounds(slf, arc)?.capacity)
    }

    /// Answer the supply of `node`, as the network holds it.
    ///
    /// A read of the problem held, as `tail` is: the supply loaded, with each move of supply made since, and none a
    /// refused move named.
    ///
    /// Args:
    ///     node: The node, below the node count.
    ///
    /// Returns:
    ///     The supply, an `int`: positive where the node sends flow, negative where it takes it.
    ///
    /// Raises:
    ///     IndexError: `node` is below zero or not below the node count.
    ///     TypeError: `node` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.move_supply(2, 0, 1)
    ///     >>> network.supply(0), network.supply(2)
    ///     (5, -5)
    #[pyo3(signature = (node))]
    fn supply(slf: &Bound<'_, Self>, node: &Bound<'_, PyAny>) -> PyResult<i64> {
        let this = borrow(slf)?;
        let node = this.node(Index::Node, node)?;

        Ok(this.held.network.supply(node))
    }

    /// Answer the tail of each arc, arc `a`'s at `a`, as a new list.
    ///
    /// A read of the problem held, as `tail` is. The six columns read back, `tails()`, `heads()`, `capacities()`,
    /// `costs()`, `supplies()` and `lower_bounds()`, are the columns `load` takes, under the same names.
    ///
    /// Returns:
    ///     A list of `int`.
    ///
    /// Raises:
    ///     MemoryError: The memory for the list is refused.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.tails(), network.heads()
    ///     ([0, 0, 1], [2, 1, 2])
    fn tails<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyList>> {
        borrow(slf)?.held.column(slf.py(), Input::Tails)
    }

    /// Write the tail of each arc into a buffer the caller owns.
    ///
    /// The buffer is checked and written as `NetworkOptimum.flows_into` checks and writes its own. The network
    /// allocates nothing; what a call still makes is a `memoryview` and an export record for the buffer.
    ///
    /// Args:
    ///     tails: A writable buffer of one dimension with one unsigned integer as wide as `size_t` per arc (64-bit on
    ///         the platforms tested), such as `numpy.empty(arc_count, dtype=numpy.uintp)` or `array.array("Q", ...)`.
    ///
    /// Raises:
    ///     TypeError: `tails` is no buffer, or its items are not exactly unsigned integers as wide as `size_t`.
    ///     ValueError: `tails` has other than one dimension, is read-only, of another length than the arc count, or
    ///         strided with items that cannot be written as they lie.
    ///     RuntimeError: The network is already in a call.
    ///
    /// What an exporter raises is passed on, with a note that names the argument.
    ///
    /// Example:
    ///     >>> from array import array
    ///     >>> network, tails = kahlnws.Network(), array("Q", [0, 0, 0])
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.tails_into(tails)
    ///     >>> tails.tolist()
    ///     [0, 0, 1]
    #[pyo3(signature = (tails))]
    fn tails_into(slf: &Bound<'_, Self>, tails: &Bound<'_, PyAny>) -> PyResult<()> {
        borrow(slf)?.held.write_column(slf.py(), Input::Tails, tails)
    }

    /// Answer the head of each arc, arc `a`'s at `a`, as a new list.
    ///
    /// A read of the problem held, as `tail` is.
    ///
    /// Returns:
    ///     A list of `int`.
    ///
    /// Raises:
    ///     MemoryError: The memory for the list is refused.
    ///     RuntimeError: The network is already in a call.
    fn heads<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyList>> {
        borrow(slf)?.held.column(slf.py(), Input::Heads)
    }

    /// Write the head of each arc into a buffer the caller owns, as `tails_into` writes the tails.
    ///
    /// Args:
    ///     heads: A writable buffer of one dimension with one unsigned integer as wide as `size_t` per arc.
    ///
    /// Raises:
    ///     TypeError: `heads` is no buffer, or its items are not exactly unsigned integers as wide as `size_t`.
    ///     ValueError: `heads` has other than one dimension, is read-only, of another length than the arc count, or
    ///         strided with items that cannot be written as they lie.
    ///     RuntimeError: The network is already in a call.
    #[pyo3(signature = (heads))]
    fn heads_into(slf: &Bound<'_, Self>, heads: &Bound<'_, PyAny>) -> PyResult<()> {
        borrow(slf)?.held.write_column(slf.py(), Input::Heads, heads)
    }

    /// Answer the most flow of each arc, arc `a`'s at `a`, as a new list.
    ///
    /// A read of the problem held, as `tail` is: the capacities loaded, with those the setters of bounds set since.
    ///
    /// Returns:
    ///     A list of `int`.
    ///
    /// Raises:
    ///     MemoryError: The memory for the list is refused.
    ///     RuntimeError: The network is already in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.set_bounds(1, (1, 5))
    ///     Bounds(lower_bound=0, capacity=4)
    ///     >>> network.lower_bounds(), network.capacities()
    ///     ([0, 1, 0], [3, 5, 2])
    fn capacities<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyList>> {
        borrow(slf)?.held.column(slf.py(), Input::Capacities)
    }

    /// Write the most flow of each arc into a buffer the caller owns, as `tails_into` writes the tails.
    ///
    /// Args:
    ///     capacities: A writable buffer of one dimension with one unsigned 64-bit integer per arc.
    ///
    /// Raises:
    ///     TypeError: `capacities` is no buffer, or its items are not exactly unsigned 64-bit integers.
    ///     ValueError: `capacities` has other than one dimension, is read-only, of another length than the arc count,
    ///         or strided with items that cannot be written as they lie.
    ///     RuntimeError: The network is already in a call.
    #[pyo3(signature = (capacities))]
    fn capacities_into(slf: &Bound<'_, Self>, capacities: &Bound<'_, PyAny>) -> PyResult<()> {
        borrow(slf)?.held.write_column(slf.py(), Input::Capacities, capacities)
    }

    /// Answer the cost of a unit of flow on each arc, arc `a`'s at `a`, as a new list.
    ///
    /// A read of the problem held, as `cost` is.
    ///
    /// Returns:
    ///     A list of `int`.
    ///
    /// Raises:
    ///     MemoryError: The memory for the list is refused.
    ///     RuntimeError: The network is already in a call.
    fn costs<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyList>> {
        borrow(slf)?.held.column(slf.py(), Input::Costs)
    }

    /// Write the cost of each arc into a buffer the caller owns, as `tails_into` writes the tails.
    ///
    /// Args:
    ///     costs: A writable buffer of one dimension with one signed 64-bit integer per arc.
    ///
    /// Raises:
    ///     TypeError: `costs` is no buffer, or its items are not exactly signed 64-bit integers.
    ///     ValueError: `costs` has other than one dimension, is read-only, of another length than the arc count, or
    ///         strided with items that cannot be written as they lie.
    ///     RuntimeError: The network is already in a call.
    #[pyo3(signature = (costs))]
    fn costs_into(slf: &Bound<'_, Self>, costs: &Bound<'_, PyAny>) -> PyResult<()> {
        borrow(slf)?.held.write_column(slf.py(), Input::Costs, costs)
    }

    /// Answer the supply of each node, node `v`'s at `v`, as a new list.
    ///
    /// A read of the problem held, as `supply` is. The supplies add up to zero.
    ///
    /// Returns:
    ///     A list of `int`.
    ///
    /// Raises:
    ///     MemoryError: The memory for the list is refused.
    ///     RuntimeError: The network is already in a call.
    fn supplies<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyList>> {
        borrow(slf)?.held.column(slf.py(), Input::Supplies)
    }

    /// Write the supply of each node into a buffer the caller owns, as `tails_into` writes the tails.
    ///
    /// Args:
    ///     supplies: A writable buffer of one dimension with one signed 64-bit integer per node.
    ///
    /// Raises:
    ///     TypeError: `supplies` is no buffer, or its items are not exactly signed 64-bit integers.
    ///     ValueError: `supplies` has other than one dimension, is read-only, of another length than the node count,
    ///         or strided with items that cannot be written as they lie.
    ///     RuntimeError: The network is already in a call.
    #[pyo3(signature = (supplies))]
    fn supplies_into(slf: &Bound<'_, Self>, supplies: &Bound<'_, PyAny>) -> PyResult<()> {
        borrow(slf)?.held.write_column(slf.py(), Input::Supplies, supplies)
    }

    /// Answer the least flow of each arc, arc `a`'s at `a`, as a new list.
    ///
    /// A read of the problem held, as `tail` is: the lower bounds loaded, zero where the load gave none, with those
    /// the setters of bounds set since.
    ///
    /// Returns:
    ///     A list of `int`.
    ///
    /// Raises:
    ///     MemoryError: The memory for the list is refused.
    ///     RuntimeError: The network is already in a call.
    fn lower_bounds<'py>(slf: &Bound<'py, Self>) -> PyResult<Bound<'py, PyList>> {
        borrow(slf)?.held.column(slf.py(), Input::LowerBounds)
    }

    /// Write the least flow of each arc into a buffer the caller owns, as `tails_into` writes the tails.
    ///
    /// Args:
    ///     lower_bounds: A writable buffer of one dimension with one unsigned 64-bit integer per arc.
    ///
    /// Raises:
    ///     TypeError: `lower_bounds` is no buffer, or its items are not exactly unsigned 64-bit integers.
    ///     ValueError: `lower_bounds` has other than one dimension, is read-only, of another length than the arc
    ///         count, or strided with items that cannot be written as they lie.
    ///     RuntimeError: The network is already in a call.
    #[pyo3(signature = (lower_bounds))]
    fn lower_bounds_into(slf: &Bound<'_, Self>, lower_bounds: &Bound<'_, PyAny>) -> PyResult<()> {
        borrow(slf)?.held.write_column(slf.py(), Input::LowerBounds, lower_bounds)
    }

    /// The copy `copy()` answers, for the module `copy`.
    ///
    /// # Errors
    /// As `copy()`.
    fn __copy__(slf: &Bound<'_, Self>) -> PyResult<Self> {
        Self::copy(slf)
    }

    /// The copy `copy()` answers, for the module `copy`: the network holds no Python object, so a deep copy is that.
    ///
    /// # Errors
    /// As `copy()`.
    #[pyo3(signature = (_memo, /))]
    fn __deepcopy__(slf: &Bound<'_, Self>, _memo: &Bound<'_, PyAny>) -> PyResult<Self> {
        Self::copy(slf)
    }
}

impl Network {
    /// The bounds of the arc `object`, the argument Python gave for an arc, names, as the network holds them: what
    /// `bounds`, `lower_bound` and `capacity` read.
    ///
    /// # Errors
    /// Raises `RuntimeError` where the network is already in a call, the `TypeError` of an argument that is no
    /// integer, and `IndexError` where it is below zero or not below the arc count. Passes on what the argument's
    /// `__index__` raises.
    fn arc_bounds(slf: &Bound<'_, Self>, object: &Bound<'_, PyAny>) -> PyResult<kahlnws::Bounds> {
        let this = borrow(slf)?;
        let arc = this.arc(object)?;

        // `arc` is below the arc count, as `arc` just checked: the read does not panic.
        Ok(this.held.network.bounds(arc))
    }

    /// The arc `object`, the argument Python gave for an arc, names in the problem held.
    ///
    /// # Errors
    /// Raises the `TypeError` of an argument that is no integer, and `IndexError` where it is below zero or not below
    /// the arc count. Passes on what the argument's `__index__` raises.
    fn arc(&self, object: &Bound<'_, PyAny>) -> PyResult<usize> {
        let arc = integer::index(Index::Arc, object)?;
        let arc_count = self.held.network.arc_count();

        below(arc, arc_count).ok_or_else(|| NotBelowCount::Arc { arc, arc_count }.into())
    }

    /// The node `object`, the argument Python gave for `index`, names in the problem held.
    ///
    /// # Errors
    /// Raises the `TypeError` of an argument that is no integer, and `IndexError` where it is below zero or not below
    /// the node count. Passes on what the argument's `__index__` raises.
    fn node(&self, index: Index, object: &Bound<'_, PyAny>) -> PyResult<usize> {
        let node = integer::index(index, object)?;
        let node_count = self.held.network.node_count();

        below(node, node_count).ok_or_else(|| NotBelowCount::Node { node, node_count }.into())
    }
}

/// What the flow of an arc [`Network::arc`] answered finds in the optimum of the problem held: a flow for every arc
/// below the arc count.
const HELD_ARC: &str = "an arc below the arc count should have a flow in the optimum";

/// `value` as a position below `count`: `None` where it is below zero or not below `count`.
fn below(value: IntegerValue, count: usize) -> Option<usize> {
    value.position().filter(|&position| position < count)
}

/// The network `slf` holds, borrowed for one call.
///
/// # Errors
/// Raises `RuntimeError` where the network is already in a call, on another thread or further up this one.
fn borrow<'py>(slf: &Bound<'py, Network>) -> PyResult<PyRefMut<'py, Network>> {
    slf.try_borrow_mut().map_err(|_| PyRuntimeError::new_err("the network is already in a call"))
}

/// The network `slf` holds, borrowed for a call that loads, changes or solves it, with its count of changes risen:
/// every optimum answered before the call reads nothing from now on, whatever the call answers or raises.
///
/// # Errors
/// Raises `RuntimeError` where the network is already in a call, as [`borrow`] does; the count is then as it was.
fn changing<'py>(slf: &Bound<'py, Network>) -> PyResult<PyRefMut<'py, Network>> {
    let mut this = borrow(slf)?;
    // A count of 2^64 calls is not reached: at one call a nanosecond, it takes over five centuries.
    this.changes = this.changes.strict_add(1);

    Ok(this)
}

/// Writes `column` into `buffer`, the object Python gave for `output`, checked first to take `length` entries, by way
/// of `room`.
///
/// # Errors
/// Raises what [`OutputBuffer::new`] raises for a buffer that cannot take the column, and what the write raises.
fn write_through<T: Held + Element>(
    py: Python<'_>,
    output: Output,
    buffer: &Bound<'_, PyAny>,
    length: usize,
    room: &mut Vec<T>,
    column: impl IntoIterator<Item = T>,
) -> PyResult<()> {
    let (checked, _) = OutputBuffer::new(output, buffer, length)?;

    // The room holds an entry for every arc of the problem held, made by its load or its copy: no allocation.
    room.clear();
    room.extend(column);
    checked.write(py, room)
}

impl ColumnRooms {
    /// Makes room in each, emptied, for `arc_count` arcs. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    fn try_reserve(&mut self, arc_count: usize) -> Result<(), TryReserveError> {
        reserve_column(&mut self.unsigned, arc_count)?;
        reserve_column(&mut self.nodes, arc_count)?;
        reserve_column(&mut self.signed, arc_count)
    }
}

impl HeldNetwork {
    /// Makes room for problems of up to `node_count` nodes and `arc_count` arcs. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    fn try_reserve(&mut self, node_count: usize, arc_count: usize) -> Result<(), TryReserveError> {
        self.network.try_reserve(node_count, arc_count)?;
        self.rooms.try_reserve(arc_count)
    }

    /// Loads `problem` with the interpreter released, the room for it made first. A load that raises keeps the problem
    /// held.
    ///
    /// The problem is the network's own copy, so no other thread can reach what the load reads while the interpreter
    /// is released.
    ///
    /// # Errors
    /// Raises `MemoryError` where the room for the problem is refused, and `ProblemError` for a problem no solve
    /// admits.
    fn load(&mut self, py: Python<'_>, problem: &kahlnws::Problem<'_>) -> PyResult<()> {
        // The room is made before the load, by a reservation that can be refused: a load that grew its own room could
        // only end the process where the memory runs out.
        let (node_count, arc_count) = (problem.supplies.len(), problem.tails.len());
        self.try_reserve(node_count, arc_count).map_err(|_| no_room(node_count, arc_count))?;

        let network = &mut self.network;
        py.detach(|| network.load(problem)).map_err(|breach| refusal(py, breach))
    }

    /// Solves the problem held with the interpreter released, and answers its total cost, or that it is infeasible,
    /// with the work the solve did.
    fn solve(&mut self, py: Python<'_>) -> kahlnws::Answer<i128> {
        let network = &mut self.network;

        py.detach(|| network.solve())
    }

    /// A copy of the network, with its rooms, made as [`copy_from`](Self::copy_from) makes it into a network never
    /// loaded.
    ///
    /// # Errors
    /// Raises `MemoryError` where the memory for the copy is refused.
    fn copy(&self) -> PyResult<Self> {
        let mut copy = Self::default();
        copy.copy_from(self)?;

        Ok(copy)
    }

    /// Makes this network a copy of `source`, with rooms for its counts, reusing the room it holds.
    ///
    /// # Errors
    /// Raises `MemoryError` where the memory for the room of `source`'s counts is refused; the library's network then
    /// holds what it held.
    fn copy_from(&mut self, source: &Self) -> PyResult<()> {
        let (node_count, arc_count) = (source.network.node_count(), source.network.arc_count());

        self.rooms
            .try_reserve(arc_count)
            .and_then(|()| self.network.try_clone_from(&source.network))
            .map_err(|_| no_room(node_count, arc_count))
    }

    /// The column `input` names of the problem held, as a new list.
    ///
    /// # Errors
    /// Raises the `MemoryError` of a list that cannot be made.
    ///
    /// # Panics
    /// Panics if the network is half changed, as the library's reads do.
    fn column<'py>(&self, py: Python<'py>, input: Input) -> PyResult<Bound<'py, PyList>> {
        let network = &self.network;

        match input {
            Input::Tails => PyList::new(py, network.tails()),
            Input::Heads => PyList::new(py, network.heads()),
            Input::LowerBounds => PyList::new(py, network.lower_bounds()),
            Input::Capacities => PyList::new(py, network.capacities()),
            Input::Costs => PyList::new(py, network.costs()),
            Input::Supplies => PyList::new(py, network.supplies()),
        }
    }

    /// Writes the column `input` names of the problem held into `buffer`, checked first to take it.
    ///
    /// # Errors
    /// Raises what [`write_through`] raises.
    ///
    /// # Panics
    /// Panics if the network is half changed, as the library's reads do.
    fn write_column(&mut self, py: Python<'_>, input: Input, buffer: &Bound<'_, PyAny>) -> PyResult<()> {
        let Self { network, rooms } = self;
        let (output, arc_count) = (Output::Problem(input), network.arc_count());

        match input {
            Input::Tails => write_through(py, output, buffer, arc_count, &mut rooms.nodes, network.tails()),
            Input::Heads => write_through(py, output, buffer, arc_count, &mut rooms.nodes, network.heads()),
            Input::LowerBounds => {
                write_through(py, output, buffer, arc_count, &mut rooms.unsigned, network.lower_bounds())
            }
            Input::Capacities => {
                write_through(py, output, buffer, arc_count, &mut rooms.unsigned, network.capacities())
            }
            Input::Costs => write_through(py, output, buffer, arc_count, &mut rooms.signed, network.costs()),
            Input::Supplies => {
                let supplies = network.supplies();
                let (checked, _) = OutputBuffer::new(output, buffer, supplies.len())?;
                checked.write(py, supplies)
            }
        }
    }

    /// Writes the flows of the optimum held into `buffer`, checked first to take them.
    ///
    /// # Errors
    /// Raises what [`write_through`] raises.
    ///
    /// # Panics
    /// As [`optimum_of`].
    fn write_flows(&mut self, py: Python<'_>, buffer: &Bound<'_, PyAny>) -> PyResult<()> {
        let Self { network, rooms } = self;
        let flows = optimum_of(network).flows;

        write_through(py, Output::Flows, buffer, flows.len(), &mut rooms.unsigned, flows)
    }
}

/// The optimum `network` holds, which an optimum that reads it reads.
///
/// # Panics
/// Panics if the network holds none, which no caller in this module asks for: an optimum that reads the network was
/// answered where it held one, and checks before it reads that no load, change or solve came since, which alone end
/// the optimum a network holds.
fn optimum_of(network: &kahlnws::Network) -> kahlnws::NetworkOptimum<'_> {
    network.optimum().expect("the network should hold the optimum it answered, unchanged since")
}

#[pymethods]
impl NetworkOptimum {
    /// The cost of the flow, in units of cost times units of flow: an exact `int` from `-2**127` to `2**127 - 1`.
    /// Read from the network, it raises `RuntimeError` where the network changed since the optimum was answered, or
    /// is in a call.
    #[getter]
    fn total_cost(&self, py: Python<'_>) -> PyResult<i128> {
        Ok(optimum_of(&self.lender(py)?.held.network).total_cost)
    }

    /// Answer the flow of `arc`, in units of flow, its lower bound included.
    ///
    /// Args:
    ///     arc: The arc, below the arc count.
    ///
    /// Returns:
    ///     The flow, an `int` from 0 to `2**64 - 1`.
    ///
    /// Raises:
    ///     IndexError: `arc` is below zero or not below the arc count.
    ///     TypeError: `arc` is no integer: neither an `int` nor of a type with `__index__`.
    ///     RuntimeError: The network changed since the optimum was answered, or is in a call.
    ///
    /// Example:
    ///     >>> network = kahlnws.Network()
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.solve().outcome
    ///     14
    ///     >>> network.optimum().flow(0)
    ///     2
    #[pyo3(signature = (arc))]
    fn flow(&self, py: Python<'_>, arc: &Bound<'_, PyAny>) -> PyResult<u64> {
        let lender = self.lender(py)?;
        let arc = lender.arc(arc)?;

        Ok(optimum_of(&lender.held.network).flows.get(arc).expect(HELD_ARC))
    }

    /// The flow of each arc, in units of flow, its lower bound included, arc `a` at `a`: a list of `int`, each from
    /// 0 to `2**64 - 1`. Each read builds a new list, which the caller owns; `flows_into` writes into a buffer instead.
    /// Read from the network, it raises `RuntimeError` where the network changed since the optimum was answered, or is
    /// in a call, and `MemoryError` where the memory for the list is refused.
    #[getter]
    fn flows<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        PyList::new(py, optimum_of(&self.lender(py)?.held.network).flows)
    }

    /// Write the flow of each arc, its lower bound included, into a buffer the caller owns.
    ///
    /// The buffer is checked before anything is written (its type, shape, length and writability) and written by one
    /// copy; where the call raises, it holds what it held before. The network allocates nothing; what a call still
    /// makes is a `memoryview` and an export record for the buffer.
    ///
    /// Args:
    ///     flows: A writable buffer of one dimension with one unsigned 64-bit integer per arc, such as
    ///         `numpy.empty(arc_count, dtype=numpy.uint64)` or `array.array("Q", ...)`.
    ///
    /// Raises:
    ///     TypeError: `flows` is no buffer, or its items are not exactly unsigned 64-bit integers.
    ///     ValueError: `flows` has other than one dimension, is read-only, of another length than the arc count, or
    ///         strided with items that cannot be written as they lie.
    ///     RuntimeError: The network changed since the optimum was answered, or is in a call.
    ///
    /// What an exporter raises is passed on, with a note that names the argument.
    ///
    /// Example:
    ///     >>> from array import array
    ///     >>> network, flows = kahlnws.Network(), array("Q", [0, 0, 0])
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.solve().outcome
    ///     14
    ///     >>> network.optimum().flows_into(flows)
    ///     >>> flows.tolist()
    ///     [2, 2, 2]
    #[pyo3(signature = (flows))]
    fn flows_into(&self, py: Python<'_>, flows: &Bound<'_, PyAny>) -> PyResult<()> {
        self.lender(py)?.held.write_flows(py, flows)
    }

    /// Each node's potential, in units of cost, node `v` at `v`: a list of `int`, each from `-2**63` to `2**63 - 1`.
    /// With them, `costs[a] + potentials[tails[a]] - potentials[heads[a]]` is at least zero on every arc whose flow is
    /// below its capacity and at most zero on every arc whose flow is above its lower bound, which proves the flow
    /// optimal. Each read builds a new list, which the caller owns; `potentials_into` writes into a buffer instead.
    /// Read from the network, it raises `RuntimeError` where the network changed since the optimum was answered, or is
    /// in a call, and `MemoryError` where the memory for the list is refused.
    #[getter]
    fn potentials<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        PyList::new(py, optimum_of(&self.lender(py)?.held.network).potentials)
    }

    /// Write each node's potential into a buffer the caller owns.
    ///
    /// The buffer is checked and written as `flows_into` checks and writes its own. The network allocates nothing;
    /// what a call still makes is a `memoryview` and an export record for the buffer.
    ///
    /// Args:
    ///     potentials: A writable buffer of one dimension with one signed 64-bit integer per node, such as
    ///         `numpy.empty(node_count, dtype=numpy.int64)` or `array.array("q", ...)`.
    ///
    /// Raises:
    ///     TypeError: `potentials` is no buffer, or its items are not exactly signed 64-bit integers.
    ///     ValueError: `potentials` has other than one dimension, is read-only, of another length than the node count,
    ///         or strided with items that cannot be written as they lie.
    ///     RuntimeError: The network changed since the optimum was answered, or is in a call.
    ///
    /// What an exporter raises is passed on, with a note that names the argument.
    ///
    /// Example:
    ///     >>> from array import array
    ///     >>> network, potentials = kahlnws.Network(), array("q", [0, 0, 0])
    ///     >>> network.load(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1],
    ///     ...              supplies=[4, 0, -4])
    ///     >>> network.solve().outcome
    ///     14
    ///     >>> network.optimum().potentials_into(potentials)
    ///     >>> potentials.tolist()
    ///     [0, 1, 5]
    #[pyo3(signature = (potentials))]
    fn potentials_into(&self, py: Python<'_>, potentials: &Bound<'_, PyAny>) -> PyResult<()> {
        let lender = self.lender(py)?;
        let node_count = lender.held.network.node_count();

        let (buffer, _) = OutputBuffer::new(Output::Potentials, potentials, node_count)?;
        buffer.write(py, optimum_of(&lender.held.network).potentials)
    }
}

impl NetworkOptimum {
    /// The network this optimum reads, borrowed for one read, where no load, change or solve came since the optimum
    /// was answered: the network then holds that optimum.
    ///
    /// # Errors
    /// Raises `RuntimeError` where the network is already in a call, or where it changed since.
    fn lender<'py>(&self, py: Python<'py>) -> PyResult<PyRefMut<'py, Network>> {
        let lender = borrow(self.network.bind(py))?;
        if lender.changes != self.changes {
            return Err(PyRuntimeError::new_err("the network changed since this optimum was answered"));
        }

        Ok(lender)
    }
}

/// A node a supply is set for that is its own balancing node and whose supply is to change: a change no other node
/// balances, which the call names wrongly. Raised as `ValueError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("node {node} is its own balancing node, whose supply {held} should then stay, not become {supply}")]
struct BalancedByItself {
    /// The node.
    node: usize,
    /// The supply it has.
    held: i64,
    /// The supply asked for.
    supply: i64,
}

/// Checks that `supply` for `node`, balanced by `balancing_node`, both below the node count of `network`, is
/// balanced by another node or changes nothing: a node that balances itself is given only the supply it has.
///
/// # Errors
/// [`BalancedByItself`] where `node` is `balancing_node` and `supply` is not the supply it has.
///
/// # Panics
/// Panics if the network is half changed and `node` is `balancing_node`, as the read of its supply does.
fn balanced_by_another(
    network: &kahlnws::Network,
    node: usize,
    supply: i64,
    balancing_node: usize,
) -> Result<(), BalancedByItself> {
    if node != balancing_node {
        return Ok(());
    }
    let held = network.supply(node);

    if held == supply { Ok(()) } else { Err(BalancedByItself { node, held, supply }) }
}

impl From<BalancedByItself> for PyErr {
    fn from(error: BalancedByItself) -> Self {
        PyValueError::new_err(error.to_string())
    }
}

impl From<NotBelowCount> for PyErr {
    fn from(error: NotBelowCount) -> Self {
        PyIndexError::new_err(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use kahlnws_testing::{Allocations, allocations_on_this_thread};
    use pyo3::types::{PyDict, PyInt};

    use super::*;

    /// Runs `test` with the interpreter embedded in this binary, attached.
    fn with_python(test: impl for<'py> FnOnce(Python<'py>) -> PyResult<()>) {
        Python::initialize();
        Python::attach(test).expect("the test's Python should run without an exception it does not expect");
    }

    /// The value of the Python expression `expression`, which may name `array`.
    fn evaluate<'py>(py: Python<'py>, expression: &str) -> PyResult<Bound<'py, PyAny>> {
        let code = std::ffi::CString::new(expression).expect("an expression should hold no NUL");
        let globals = PyDict::new(py);
        globals.set_item("array", py.import("array")?.getattr("array")?)?;

        py.eval(&code, Some(&globals), None)
    }

    /// Each of `values` as a Python `int`, made before a call whose allocations a test counts.
    fn integers<const N: usize>(py: Python<'_>, values: [i64; N]) -> [Bound<'_, PyAny>; N] {
        values.map(|value| value.into_pyobject(py).expect("an int should be made").into_any())
    }

    /// A network of `py` that holds the depot of `supplies`: node 0 sends to node 2, directly at 5 a unit for up to 3
    /// units, or through node 1 at 2 a unit for up to 2.
    fn depot<'py>(py: Python<'py>, supplies: &str) -> PyResult<Bound<'py, Network>> {
        let network = Bound::new(py, Network::new())?;
        load_depot(&network, supplies)?;

        Ok(network)
    }

    /// Loads into `network` the depot of `supplies`, and answers what the load allocates on the Rust heap of this
    /// thread.
    fn load_depot(network: &Bound<'_, Network>, supplies: &str) -> PyResult<Allocations> {
        let py = network.py();
        let [tails, heads, capacities, costs, supplies] =
            ["[0, 0, 1]", "[2, 1, 2]", "[3, 4, 2]", "[5, 1, 1]", supplies].map(|column| evaluate(py, column));
        let (tails, heads, capacities, costs, supplies) = (tails?, heads?, capacities?, costs?, supplies?);

        let before = allocations_on_this_thread();
        Network::load(network, tails, heads, capacities, costs, supplies, None)?;
        Ok(allocations_on_this_thread() - before)
    }

    /// Loads into `network` one node with a loop of cost 1 and capacity 1, of other counts than the depot's, and
    /// answers what the load allocates on the Rust heap of this thread.
    fn load_a_loop(network: &Bound<'_, Network>) -> PyResult<Allocations> {
        let py = network.py();
        let [tails, heads, capacities, costs, supplies] = ["[0]", "[0]", "[1]", "[1]", "[0]"].map(|c| evaluate(py, c));
        let (tails, heads, capacities, costs, supplies) = (tails?, heads?, capacities?, costs?, supplies?);

        let before = allocations_on_this_thread();
        Network::load(network, tails, heads, capacities, costs, supplies, None)?;
        Ok(allocations_on_this_thread() - before)
    }

    /// The cost the answer of a solve holds, `None` where it is infeasible, and its pivots.
    fn cost_and_pivots(answer: &Bound<'_, PyAny>) -> PyResult<(Option<i128>, u64)> {
        Ok((answer.getattr("outcome")?.extract()?, answer.getattr("work")?.getattr("pivots")?.extract()?))
    }

    /// What `solve` answers on `network`, as a Python object, and what it allocates on the Rust heap of this thread.
    fn solved<'py>(network: &Bound<'py, Network>) -> PyResult<(Bound<'py, PyAny>, Allocations)> {
        let before = allocations_on_this_thread();
        let answer = Network::solve(network)?;
        let after = allocations_on_this_thread();

        Ok((Bound::new(network.py(), answer)?.into_any(), after - before))
    }

    #[test]
    fn test_an_arc_or_a_node_not_below_its_count_says_so_and_raises_index_error_by_hand() {
        let arc = NotBelowCount::Arc { arc: IntegerValue::Integer(3), arc_count: 3 };
        let node = NotBelowCount::Node { node: IntegerValue::Integer(7), node_count: 0 };
        let negative = NotBelowCount::Arc { arc: IntegerValue::Integer(-1), arc_count: 3 };
        assert_eq!(arc.to_string(), "arc 3 is not below the arc count 3");
        assert_eq!(node.to_string(), "node 7 is not below the node count 0");
        assert_eq!(negative.to_string(), "arc -1 is not below the arc count 3");

        with_python(|py| {
            assert!(PyErr::from(arc).is_instance_of::<PyIndexError>(py));
            Ok(())
        });
    }

    #[test]
    fn test_a_node_that_balances_itself_changes_nothing_or_says_so_and_raises_value_error_by_hand() {
        with_python(|py| {
            let network = depot(py, "[4, 0, -4]")?;
            let held = network.borrow();

            assert_eq!(balanced_by_another(&held.held.network, 0, 7, 2), Ok(()), "another node balances it");
            assert_eq!(balanced_by_another(&held.held.network, 1, 0, 1), Ok(()), "the supply it has");
            let refused = balanced_by_another(&held.held.network, 0, 5, 0);
            assert_eq!(refused, Err(BalancedByItself { node: 0, held: 4, supply: 5 }));
            let error = refused.expect_err("node 0 is to change");
            assert_eq!(
                error.to_string(),
                "node 0 is its own balancing node, whose supply 4 should then stay, not become 5"
            );
            assert!(PyErr::from(error).is_instance_of::<PyValueError>(py));

            Ok(())
        });
    }

    #[test]
    fn test_below_answers_a_position_at_least_zero_and_below_the_count_by_hand() {
        let below_three = [0, 2, 3, -1].map(|value| below(IntegerValue::Integer(value), 3));
        let long = below(IntegerValue::Long { negative: false, bits: 200 }, usize::MAX);

        assert_eq!(below_three, [Some(0), Some(2), None, None]);
        assert_eq!(long, None);
        assert_eq!(below(IntegerValue::Integer(0), 0), None, "nothing is below a count of zero");
    }

    #[test]
    fn test_arc_and_node_name_the_index_given_where_it_is_not_below_its_count_by_hand() {
        with_python(|py| {
            let network = depot(py, "[4, 0, -4]")?;
            let held = network.borrow();

            assert_eq!(held.arc(&evaluate(py, "2")?)?, 2);
            assert_eq!(held.node(Index::ToNode, &evaluate(py, "0")?)?, 0);
            let refused = [
                held.arc(&evaluate(py, "3")?),
                held.arc(&evaluate(py, "-1")?),
                held.node(Index::FromNode, &evaluate(py, "2**64")?),
                held.node(Index::Node, &evaluate(py, "-(2**200)")?),
            ];
            let messages = refused.map(|refused| refused.map_err(|error| error.value(py).to_string()));
            assert_eq!(
                messages,
                [
                    Err("arc 3 is not below the arc count 3".to_owned()),
                    Err("arc -1 is not below the arc count 3".to_owned()),
                    Err(format!("node {} is not below the node count 3", 1_u128 << 64)),
                    Err("node -(2**200 or more) is not below the node count 3".to_owned()),
                ]
            );
            let error = held.node(Index::FromNode, &evaluate(py, "1.5")?).expect_err("no integer");
            assert_eq!(error.value(py).to_string(), "from_node is of type float, not an integer");

            Ok(())
        });
    }

    #[test]
    fn test_a_reserved_network_loads_from_the_basis_held_or_from_the_star_and_allocates_nothing_either_way() {
        // The depot's first load follows a load of other counts, so it starts from the star and its solve makes 4
        // pivots. With one unit less on the same arcs the optimum held still serves: the load starts from it, and the
        // solve makes none, where a start from the star makes as many as a fresh network. After a loop of other counts
        // the depot starts from the star again, its work a fresh network's.
        with_python(|py| {
            let network = Bound::new(py, Network::new())?;
            Network::reserve(&network, PyInt::new(py, 3).as_any(), PyInt::new(py, 3).as_any())?;
            // The first load and solve make the classes of the answer and the optimum, once for the process.
            load_a_loop(&network)?;
            solved(&network)?;

            assert_eq!(load_depot(&network, "[4, 0, -4]")?.blocks, 0);
            let (answer, allocated) = solved(&network)?;
            assert_eq!((cost_and_pivots(&answer)?, allocated.blocks), ((Some(14), 4), 0));

            assert_eq!(load_depot(&network, "[3, 0, -3]")?.blocks, 0);
            let (answer, allocated) = solved(&network)?;
            assert_eq!((cost_and_pivots(&answer)?, allocated.blocks), ((Some(9), 0), 0), "from the basis held");

            let fresh = Bound::new(py, Network::solve(&depot(py, "[3, 0, -3]")?)?)?.into_any();
            let fresh_pivots = cost_and_pivots(&fresh)?.1;
            assert!(fresh_pivots > 0, "a fresh network starts from the star and pivots");
            assert_eq!(load_a_loop(&network)?.blocks, 0);
            assert_eq!(load_depot(&network, "[3, 0, -3]")?.blocks, 0);
            let (answer, allocated) = solved(&network)?;
            assert_eq!((cost_and_pivots(&answer)?, allocated.blocks), ((Some(9), fresh_pivots), 0), "from the star");

            Ok(())
        });
    }

    #[test]
    fn test_a_reserved_network_loads_changes_and_solves_with_no_allocation_and_keeps_its_basis() {
        // The interpreter is embedded in this binary. What Python allocates (the columns, the answer, its work, the
        // optimum) goes through Python's own allocator and is not counted: the count is the network's and the
        // binding's alone. The depot's first solve starts from the star and makes 4 pivots; one more unit for node 2,
        // sent by node 0, goes along the tree, directly at 5 a unit, which the optimum's basis then still is, so the
        // solve after it makes none, where a start from the star would make some again.
        with_python(|py| {
            let network = Bound::new(py, Network::new())?;
            Network::reserve(&network, PyInt::new(py, 3).as_any(), PyInt::new(py, 3).as_any())?;
            // The first load and solve make the classes of the answer and the optimum, once for the process; every call
            // after them is measured.
            Network::load(
                &network,
                evaluate(py, "[0]")?,
                evaluate(py, "[0]")?,
                evaluate(py, "[1]")?,
                evaluate(py, "[1]")?,
                evaluate(py, "[0]")?,
                None,
            )?;
            solved(&network)?;

            let columns = ["[0, 0, 1]", "[2, 1, 2]", "[3, 4, 2]", "[5, 1, 1]", "[4, 0, -4]"].map(|c| evaluate(py, c));
            let [tails, heads, capacities, costs, supplies] = columns;
            let before = allocations_on_this_thread();
            Network::load(&network, tails?, heads?, capacities?, costs?, supplies?, None)?;
            let loaded = allocations_on_this_thread() - before;
            assert_eq!(loaded.blocks, 0, "{loaded}");

            let (answer, allocated) = solved(&network)?;
            assert_eq!(cost_and_pivots(&answer)?, (Some(14), 4));
            assert_eq!(allocated.blocks, 0, "{allocated}");

            let [zero, one, two, three] = integers(py, [0, 1, 2, 3]);
            let pair = (0, 2).into_pyobject(py)?.into_any();
            // The class `Bounds` the setter answers is made once for the process, as the module's import makes it.
            crate::bounds::class(py)?;
            let before = allocations_on_this_thread();
            Network::move_supply(&network, &two, &zero, &one)?;
            let moved = allocations_on_this_thread() - before;
            let (answer, allocated) = solved(&network)?;
            assert_eq!(cost_and_pivots(&answer)?, (Some(19), 0), "the network did not start again from the star");
            assert_eq!((moved.blocks, allocated.blocks), (0, 0), "{moved}, {allocated}");

            // Each write into a buffer makes the two export records the buffer protocol's binding keeps on the Rust
            // heap, of the buffer's `memoryview` and of the view of its bytes, and nothing else.
            let optimum = Bound::new(py, Network::optimum(&network)?.expect("the solve found an optimum"))?;
            let (flows, potentials) = (evaluate(py, "array('Q', [0, 0, 0])")?, evaluate(py, "array('q', [0, 0, 0])")?);
            let before = allocations_on_this_thread();
            optimum.call_method1("flows_into", (&flows,))?;
            let written = allocations_on_this_thread() - before;
            assert_eq!(written.blocks, 2, "{written}");
            let before = allocations_on_this_thread();
            optimum.call_method1("potentials_into", (&potentials,))?;
            let written = allocations_on_this_thread() - before;
            assert_eq!(written.blocks, 2, "{written}");
            assert_eq!(flows.call_method0("tolist")?.extract::<Vec<u64>>()?, [3, 2, 2]);
            assert_eq!(potentials.call_method0("tolist")?.extract::<Vec<i64>>()?, [0, 1, 5]);

            let [four, minus_three, minus_four] = integers(py, [4, -3, -4]);
            let before = allocations_on_this_thread();
            Network::set_cost(&network, &two, &three)?;
            Network::set_bounds(&network, &zero, &pair)?;
            Network::move_supply(&network, &zero, &two, &one)?;
            Network::set_supply(&network, &two, &minus_three, &zero)?;
            Network::set_supply(&network, &two, &minus_four, &zero)?;
            Network::set_supply(&network, &zero, &four, &zero)?;
            let (answer, allocated) = solved(&network)?;
            let changed = allocations_on_this_thread() - before;
            assert_eq!(cost_and_pivots(&answer)?.0, Some(18), "2 units at 4 and 2 at 5");
            assert_eq!((changed.blocks, allocated.blocks), (0, 0), "{changed}");

            Ok(())
        });
    }

    #[test]
    fn test_every_load_change_and_solve_raises_the_count_and_ends_the_optima_answered_before() {
        with_python(|py| {
            let network = depot(py, "[4, 0, -4]")?;
            assert_eq!(network.borrow().changes, 1, "the load");

            let answer = Bound::new(py, Network::solve(&network)?)?.into_any();
            assert_eq!(cost_and_pivots(&answer)?.0, Some(14));
            let first = Bound::new(py, Network::optimum(&network)?.expect("the solve found an optimum"))?;
            assert_eq!(first.getattr("total_cost")?.extract::<i128>()?, 14);
            Network::reserve(&network, PyInt::new(py, 9).as_any(), PyInt::new(py, 9).as_any())?;
            let again = Network::optimum(&network)?.expect("a reservation keeps the optimum");
            assert_eq!(again.changes, 2, "a reservation and a read change nothing");
            assert_eq!(first.getattr("total_cost")?.extract::<i128>()?, 14, "an optimum still reads");

            // A refused change changes nothing in the network, which holds its optimum, but it is a call that could
            // have: the optimum answered before it reads nothing.
            let [zero, four] = integers(py, [0, 4]);
            let refused =
                Network::set_lower_bound(&network, &zero, &four).expect_err("the lower bound passes the capacity 3");
            assert!(refused.is_instance_of::<crate::change_breach::ChangeError>(py));
            let stale = first.getattr("total_cost").expect_err("the count rose");
            assert!(stale.is_instance_of::<PyRuntimeError>(py));
            assert_eq!(stale.value(py).to_string(), "the network changed since this optimum was answered");

            let held = Bound::new(py, Network::optimum(&network)?.expect("a refused change keeps the optimum"))?;
            assert_eq!((held.get().changes, held.getattr("total_cost")?.extract::<i128>()?), (3, 14));
            let [nine, one] = integers(py, [9, 1]);
            Network::set_cost(&network, &nine, &one).expect_err("arc 9 is past the count");
            Network::solve(&network)?;
            assert_eq!(network.borrow().changes, 5, "a change refused at its index and a solve");
            assert!(held.getattr("flows").is_err());

            Ok(())
        });
    }

    #[test]
    fn test_a_call_on_a_network_in_a_call_and_a_read_of_its_optimum_raise_our_runtime_error() {
        with_python(|py| {
            let network = depot(py, "[4, 0, -4]")?;
            Network::solve(&network)?;
            let optimum = Bound::new(py, Network::optimum(&network)?.expect("the solve found an optimum"))?;
            let held = network.borrow_mut();

            let error = Network::reserve(&network, PyInt::new(py, 1).as_any(), PyInt::new(py, 1).as_any())
                .expect_err("the network is borrowed");
            assert!(error.is_instance_of::<PyRuntimeError>(py));
            assert_eq!(error.value(py).to_string(), "the network is already in a call");
            let read = optimum.getattr("potentials").expect_err("the network is borrowed");
            assert_eq!(read.value(py).to_string(), "the network is already in a call");

            drop(held);
            assert_eq!(optimum.getattr("potentials")?.extract::<Vec<i64>>()?, [0, 1, 5]);

            Ok(())
        });
    }

    #[test]
    fn test_reads_of_the_problem_end_no_optimum_and_a_reserved_network_reads_and_is_copied_into_with_no_allocation() {
        // As in the test above, each write into a buffer makes the two export records of the buffer's views and
        // nothing else; a copy into a network whose room holds the source's counts allocates nothing.
        with_python(|py| {
            let network = depot(py, "[4, 0, -4]")?;
            Network::solve(&network)?;
            let optimum = Bound::new(py, Network::optimum(&network)?.expect("the solve found an optimum"))?;
            let changes = network.borrow().changes;

            let lists =
                [Input::Tails, Input::Heads, Input::LowerBounds, Input::Capacities, Input::Costs, Input::Supplies]
                    .map(|input| network.borrow().held.column(py, input).and_then(|list| list.extract::<Vec<i128>>()));
            let lists = lists.into_iter().collect::<PyResult<Vec<_>>>()?;
            assert_eq!(
                lists,
                [vec![0, 0, 1], vec![2, 1, 2], vec![0, 0, 0], vec![3, 4, 2], vec![5, 1, 1], vec![4, 0, -4]]
            );
            let [zero, one, two] = integers(py, [0, 1, 2]);
            let singles = (
                Network::tail(&network, &two)?,
                Network::head(&network, &two)?,
                Network::cost(&network, &zero)?,
                Network::bounds(&network, &one)?.extract::<(u64, u64)>()?,
                Network::supply(&network, &two)?,
            );
            assert_eq!(singles, (1, 2, 5, (0, 4), -4));
            assert_eq!(network.borrow().changes, changes, "a read is no change");
            assert_eq!(optimum.getattr("total_cost")?.extract::<i128>()?, 14, "the optimum still reads");

            let columns = [
                (Input::Tails, "array('Q', [9, 9, 9])"),
                (Input::LowerBounds, "array('Q', [9, 9, 9])"),
                (Input::Costs, "array('q', [9, 9, 9])"),
                (Input::Supplies, "array('q', [9, 9, 9])"),
            ];
            for (input, room) in columns {
                let room = evaluate(py, room)?;
                let before = allocations_on_this_thread();
                network.borrow_mut().held.write_column(py, input, &room)?;
                let written = allocations_on_this_thread() - before;
                assert_eq!(written.blocks, 2, "{input}: {written}");
            }

            let copy = Bound::new(py, Network::new())?;
            Network::reserve(&copy, PyInt::new(py, 3).as_any(), PyInt::new(py, 3).as_any())?;
            let before = allocations_on_this_thread();
            Network::copy_from(&copy, &network)?;
            let copied = allocations_on_this_thread() - before;
            assert_eq!(copied.blocks, 0, "{copied}");
            assert_eq!(copy.borrow().changes, 1, "a copy into a network changes it");
            let (answer, allocated) = solved(&copy)?;
            assert_eq!((cost_and_pivots(&answer)?, allocated.blocks), ((Some(14), 0), 0), "the copied basis");

            Ok(())
        });
    }

    #[test]
    fn test_a_copy_holds_what_the_network_held_and_changes_apart_from_it() {
        with_python(|py| {
            let network = depot(py, "[4, 0, -4]")?;
            Network::solve(&network)?;
            let copy = Bound::new(py, Network::copy(&network)?)?;

            assert!(copy.borrow().held.rooms.unsigned.capacity() >= 3, "room for the flows of its optimum");
            assert!(Network::optimum(&copy)?.is_some(), "the copy holds the optimum");
            let [two, three] = integers(py, [2, 3]);
            Network::set_cost(&copy, &two, &three)?;
            let copied = Bound::new(py, Network::solve(&copy)?)?.into_any();
            let original = Bound::new(py, Network::solve(&network)?)?.into_any();
            assert_eq!(cost_and_pivots(&copied)?.0, Some(18), "2 units at 4 and 2 at 5");
            assert_eq!(cost_and_pivots(&original)?, (Some(14), 0), "the network itself is as it was");

            Ok(())
        });
    }
}
