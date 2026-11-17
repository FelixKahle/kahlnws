// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

#ifndef KAHLNWS_H_0F9A793A_BC8A_421B_8E0E_B5B356E26F84
#define KAHLNWS_H_0F9A793A_BC8A_421B_8E0E_B5B356E26F84

// Written by the build from the library's sources: edit those, not this file.

#include <stddef.h>
#include <stdint.h>

/**
 * @file kahlnws.h
 * @brief The C interface of kahlnws: a primal network simplex for the min-cost flow problem on exact integers.
 *
 * The header include/kahlnws.h declares it, and the library libkahlnws_capi, static or dynamic, provides it. The
 * header is read by a C compiler from C99 on and by a C++ compiler from C++11 on.
 *
 * Two handles, one contract each. The solver, kahlnws_solver, is a function: a problem in, an answer out. It keeps
 * room and nothing else, so what a solve answers is a function of its problem alone. A network, kahlnws_network, is
 * a state: it holds one problem and a basis of it, which its changes, its loads of new numbers on the same arcs and
 * its solves carry on from. Reach for the solver to answer one problem; for a network to hold a problem, change it,
 * load new numbers on the same arcs, and solve it again and again.
 *
 * Use. A program makes a solver with kahlnws_solver_new and, where it solves in a loop, reserves its room once
 * with kahlnws_solver_reserve. It fills in a kahlnws_problem with the two counts and a pointer to each of its
 * tables, and calls kahlnws_solver_solve with storage of its own for the flows and, where it wants them, the
 * potentials, the cost, the work and the refusal. It reads the kahlnws_status the call answers, which
 * kahlnws_status_name names; where the solve ran, optimal or infeasible, it may read the pivots it made from the
 * kahlnws_work; on a refusal, it reads the breach from the kahlnws_refusal, or its text from
 * kahlnws_refusal_message. It frees the solver with kahlnws_solver_free.
 *
 * A problem held. A program that changes a few numbers of one problem many times, as a search does that tries a move,
 * keeps it or sets it back, makes a network with kahlnws_network_new and hands it the problem once with
 * kahlnws_network_load; a later load of new numbers on the same arcs carries over the basis the network holds, so
 * a network that holds a problem over days carries its basis from day to day. Each kahlnws_network_set_cost,
 * kahlnws_network_set_capacity, kahlnws_network_set_lower_bound, kahlnws_network_set_bounds and
 * kahlnws_network_set_supply changes one number of the problem held, or one pair, at once, and writes the number it
 * replaced where the caller asks for it; kahlnws_network_move_supply moves supply from one node to another and
 * writes no number. Each answers KAHLNWS_STATUS_OK where the change is made, KAHLNWS_STATUS_REFUSED with the
 * breach where the number lies outside the ranges a network keeps, and KAHLNWS_STATUS_INVALID_ARGUMENT for an arc
 * or a node not below its count. kahlnws_network_solve solves the problem as it then stands, and
 * kahlnws_network_total_cost, kahlnws_network_flow, kahlnws_network_flows and kahlnws_network_potentials read
 * its optimum, each answering KAHLNWS_STATUS_NO_OPTIMUM where the network holds none, since nothing in C can borrow
 * it. The problem as it stands is read back by kahlnws_network_tail, _head, _cost, _lower_bound, _capacity,
 * _bounds and _supply, one arc or node, and kahlnws_network_tails, _heads, _costs, _lower_bounds,
 * _capacities and _supplies, every one into a buffer; a read of the problem needs no optimum and ends none. A
 * change costs what it moves, and a solve after it the pivots it calls for: a solve that makes no pivot prices only
 * the arcs the changes may have made worth using, while a change that leaves much flow still to be sent can cost
 * more than a load of the whole problem as it stands and the solve after it. Setting a number back, or moving a
 * supply back, undoes a change; kahlnws_network_clone copies a network whole into a new one, and
 * kahlnws_network_copy into one that exists, reusing its room. It frees the network with kahlnws_network_free.
 *
 * Ownership. The library owns only the solvers and the networks it makes, from kahlnws_solver_new to
 * kahlnws_solver_free and from kahlnws_network_new or kahlnws_network_clone to kahlnws_network_free. Every
 * other pointer belongs to the caller: a call reads or writes through it while it runs, and keeps none.
 *
 * Pointers. Each entry states what each of its pointers must point to. An input is initialised, and nothing writes
 * it while the call runs. An output needs writable storage for as many entries as the call states, which nothing
 * else reads or writes while the call runs and which overlaps no other argument; what it held before does not
 * matter. An array lies within one object. A table of no entries needs no storage: where its count is zero, its
 * pointer is not read and may be null. A pointer the call needs that is null is answered as
 * KAHLNWS_STATUS_NULL_POINTER; a pointer not aligned for its type, or a count of entries that passes PTRDIFF_MAX
 * bytes, as KAHLNWS_STATUS_INVALID_ARGUMENT. Either way the call writes nothing.
 *
 * Answers. Every entry answers, and none aborts or unwinds into the caller. Memory refused for the room of a solver
 * or of a network is answered as KAHLNWS_STATUS_NO_ROOM, or as null by an entry that answers a handle, and an
 * internal error of the library, a defect of it, as KAHLNWS_STATUS_INTERNAL_ERROR.
 *
 * Threads. A solver or a network is used by one thread at a time: no two calls that take the same one run at once.
 * It may move from one thread to another between calls, and distinct ones may be used on distinct threads at once,
 * also to solve or load the same problem tables. The entries that take neither may be called from any thread at any
 * time.
 *
 * Numbers. Node and arc counts and node numbers are size_t; flows, lower bounds, capacities and the counts of a
 * kahlnws_work uint64_t; costs, supplies and potentials int64_t. A 128-bit integer, such as the cost of a
 * flow, comes as its two 64-bit halves, kahlnws_int128 or kahlnws_uint128: the value is high * 2^64 + low,
 * with low the value modulo 2^64. kahlnws_int128_format and kahlnws_uint128_format write one in decimal.
 *
 * Version. KAHLNWS_VERSION_MAJOR, KAHLNWS_VERSION_MINOR and KAHLNWS_VERSION_PATCH give the version of the
 * header, and KAHLNWS_VERSION all three in one number, encoded as kahlnws_version answers the version of the
 * library linked. From the first release on, a program built against a header works with a library of the same
 * major version whose minor version is at least the header's, and while the major version is 0, only with a library
 * of the same minor version. Until that release the interface may change while the version stays, so a header and a
 * library must be built from the same sources, which comparing KAHLNWS_VERSION with kahlnws_version cannot
 * check.
 */

/**
 * @def KAHLNWS_API
 * @brief Marks every function of the library, and imports it from the dynamic library on Windows.
 *
 * Define KAHLNWS_SHARED before including this header where the program links the dynamic library on Windows: then
 * KAHLNWS_API imports each function from it. Elsewhere, and for the static library, KAHLNWS_API is empty.
 */
#if defined(_WIN32) && defined(KAHLNWS_SHARED)
#define KAHLNWS_API __declspec(dllimport)
#else
#define KAHLNWS_API
#endif

/**
 * @def KAHLNWS_NODISCARD
 * @brief Marks a function whose answer, a status or a handle, must not be ignored.
 *
 * It is the standard attribute under C23 and C++17, the compiler's own under GCC and Clang, nothing otherwise. It
 * follows KAHLNWS_API, after which a standard attribute cannot stand, so it takes the standard form only where
 * KAHLNWS_API is empty.
 */
#if !(defined(_WIN32) && defined(KAHLNWS_SHARED)) && \
    ((defined(__cplusplus) && __cplusplus >= 201703L) || \
     (!defined(__cplusplus) && defined(__STDC_VERSION__) && __STDC_VERSION__ >= 202311L))
#define KAHLNWS_NODISCARD [[nodiscard]]
#elif defined(__GNUC__) || defined(__clang__)
#define KAHLNWS_NODISCARD __attribute__((warn_unused_result))
#else
#define KAHLNWS_NODISCARD
#endif

/** @brief The major version of the header. */
#define KAHLNWS_VERSION_MAJOR 0
/** @brief The minor version of the header. */
#define KAHLNWS_VERSION_MINOR 1
/** @brief The patch version of the header. */
#define KAHLNWS_VERSION_PATCH 0
/** @brief The version of the header in one number, encoded as kahlnws_version answers the library's. */
#define KAHLNWS_VERSION 0x000100u

/**
 * @brief A network: a state that holds one min-cost flow problem and a basis of it, which its changes, loads and solves
 * carry on from.
 *
 * Opaque: C holds it only by the pointer kahlnws_network_new or kahlnws_network_clone answers, passes that pointer
 * to the other entries, and gives it back to kahlnws_network_free. kahlnws_network_load hands it a problem, which
 * it copies; kahlnws_network_set_cost, kahlnws_network_set_capacity, kahlnws_network_set_lower_bound and
 * kahlnws_network_set_bounds change one arc of the problem held, kahlnws_network_set_supply sets the supply of one
 * node, balanced by another, and kahlnws_network_move_supply moves supply from one node to another, each at once and
 * only where the change reaches; kahlnws_network_solve solves the problem as it then stands, and the reads
 * kahlnws_network_total_cost, kahlnws_network_flow, kahlnws_network_flows and kahlnws_network_potentials read
 * the optimum it found. The reads kahlnws_network_tail, _head, _cost, _lower_bound, _capacity, _bounds,
 * _supply and their columns read the problem as it stands, which needs no optimum and ends none.
 * kahlnws_network_copy makes one network a copy of another.
 *
 * Beside the solver. The solver answers the problem each call hands it, from nothing kept. A network holds its
 * problem and a basis of it: a change costs what it moves, a load of new numbers on the same arcs carries over the
 * basis held, and a solve after either makes the pivots they call for. A solve that makes no pivot prices only the
 * arcs the changes may have made worth using, none for a move of supply the flow held carries; a solve that pivots
 * prices at least one whole pass over the arcs. Where a change leaves much flow still to be sent, so that the solve
 * makes many pivots, a network's solve can take longer than a load of the whole problem as it stands and the solve
 * after it.
 *
 * A network never loaded holds the problem of no node and no arc: its solve answers the optimum of no flow at the cost
 * zero, and no arc or node is below its counts. Each setter answers the number it replaced, so setting that number
 * back, or moving the supply back, makes the problem the earlier one again: the next solve answers its status and its
 * cost, with a flow of that cost, which among tied optima may be another than before. A capacity of zero on an arc
 * whose lower bound is zero switches it off, and the capacity it answered switches it on again. A change that is
 * refused, or that sets the number already held, changes nothing, and keeps the optimum the network held; every
 * other change ends it, as do a load and a solve that finds no flow: the reads then answer
 * KAHLNWS_STATUS_NO_OPTIMUM until a solve finds one, and never a number of an earlier state.
 *
 * Its answers are a function of the problem loaded and of the calls made since, in order: two networks given the
 * same calls answer alike, their work included. The status and the cost are those of the problem as it stands, the
 * potentials prove the flow optimal, and the flow among tied optima, the potentials and the work are the network's
 * own. Where a solver admits the problem as it stands, it answers the same status and cost. After moves of supply
 * and changes of lower bounds a network may hold numbers a solver and a load refuse: every supply stays within
 * int64_t, but the supplies net of the lower bounds may pass the limit
 * KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE names, where the arcs carry the difference; the network still solves
 * them exactly.
 *
 * After a call that answered KAHLNWS_STATUS_INTERNAL_ERROR, which only a defect of the library makes, the network
 * may be half changed. An arc or a node not below the counts of the problem held before is still answered as
 * KAHLNWS_STATUS_INVALID_ARGUMENT first; beyond that, every change, kahlnws_network_solve and every read of the
 * problem answer KAHLNWS_STATUS_INTERNAL_ERROR, the reads of the optimum KAHLNWS_STATUS_NO_OPTIMUM, the counts
 * are those of the problem held before, kahlnws_network_reserve makes room, a copy is as half changed, and
 * kahlnws_network_load mends it.
 *
 * Room. Only kahlnws_network_reserve, kahlnws_network_load and kahlnws_network_copy allocate, the load and the
 * copy not for a problem within the room held. kahlnws_network_clone allocates the copy, whose changes and solves
 * allocate nothing.
 *
 * One thread at a time uses a network: no two calls that take the same network may run at once. A network may move
 * from one thread to another between calls, and distinct networks may be used on distinct threads at once.
 */
typedef struct kahlnws_network kahlnws_network;

/**
 * @brief A solver: a primal network simplex with the room it works in, kept from one solve to the next.
 *
 * Opaque: C holds it only by the pointer kahlnws_solver_new answers, passes that pointer to the other entries, and
 * gives it back to kahlnws_solver_free. The room only grows, by kahlnws_solver_reserve or by a solve of a
 * problem larger than the room held; a solve of a problem within the room allocates nothing.
 *
 * It keeps its room and nothing else: what a solve answers is a function of its problem alone, and nothing of an
 * earlier problem is read again.
 *
 * One thread at a time uses a solver: no two calls that take the same solver may run at once. A solver may move
 * from one thread to another between calls, and distinct solvers may be used on distinct threads at once.
 */
typedef struct kahlnws_solver kahlnws_solver;

/**
 * @brief How a call went, by a number fixed for good: one of the KAHLNWS_STATUS_ constants.
 *
 * A solve answers KAHLNWS_STATUS_OPTIMAL or KAHLNWS_STATUS_INFEASIBLE, and every other call that did what it was
 * asked KAHLNWS_STATUS_OK; every other status says what stopped the call. Each constant names the calls that answer
 * it, and each entry lists what it can answer. An arc or a node of a network not below its count, and a node given as
 * its own balancing node with a supply other than the one it has, are KAHLNWS_STATUS_INVALID_ARGUMENT, as a pointer
 * no call can use is: a call written wrongly.
 *
 * The numbers are part of the binary interface: none is ever changed or given to another status. Plain data: a
 * caller may hold any number, and kahlnws_status_name names every one.
 */
typedef uint32_t kahlnws_status;
enum {
  /**
   * @brief kahlnws_solver_solve or kahlnws_network_solve found a flow of least cost and wrote what was asked of it.
   */
  KAHLNWS_STATUS_OPTIMAL = 0,
  /**
   * @brief kahlnws_solver_solve or kahlnws_network_solve found that no flow within the bounds meets the supplies,
   * and wrote nothing but the work of the solve.
   */
  KAHLNWS_STATUS_INFEASIBLE = 1,
  /**
   * @brief Any call that is not a solve did what it was asked: kahlnws_solver_reserve and kahlnws_network_reserve
   * made the room, kahlnws_network_load holds the problem, a change of a network was made or set the number already
   * held, kahlnws_network_copy made the copy, and a read of a network wrote what it reads.
   */
  KAHLNWS_STATUS_OK = 2,
  /**
   * @brief kahlnws_solver_solve or kahlnws_network_load refused the problem, which lies outside what a solve
   * admits, or a change of a network refused its number, which lies outside what the network admits; nothing was
   * written but the refusal that names the breach.
   */
  KAHLNWS_STATUS_REFUSED = 3,
  /**
   * @brief kahlnws_network_total_cost, kahlnws_network_flow, kahlnws_network_flows or
   * kahlnws_network_potentials found no optimum: the network was loaded or changed since its last solve, or that
   * solve found no flow. Nothing was written.
   */
  KAHLNWS_STATUS_NO_OPTIMUM = 4,
  /**
   * @brief Any call: a pointer it needs is null, and nothing was written.
   */
  KAHLNWS_STATUS_NULL_POINTER = 5,
  /**
   * @brief Any call: an argument no call can use, a pointer not aligned for its type, a count of entries that no object
   * can hold, its bytes past PTRDIFF_MAX, an arc or a node of a network not below its count, or a node given to
   * kahlnws_network_set_supply as its own balancing node with a supply other than the one it has. Nothing was
   * written.
   */
  KAHLNWS_STATUS_INVALID_ARGUMENT = 6,
  /**
   * @brief kahlnws_solver_reserve, kahlnws_solver_solve, kahlnws_network_reserve, kahlnws_network_load or
   * kahlnws_network_copy: the memory for the room of the solver or of the network was refused, and nothing was
   * written.
   */
  KAHLNWS_STATUS_NO_ROOM = 7,
  /**
   * @brief Any call: an internal error of the library was caught, a defect of it, and answered here. What the call's
   * outputs hold is then unspecified; the solver or the network may be used again or freed, as its entries say.
   */
  KAHLNWS_STATUS_INTERNAL_ERROR = 8,
};

/**
 * @brief One min-cost flow problem as C lays it out: arc_count arcs between node_count nodes, each table given by a
 * pointer to its first entry. Arc a is entry a of every per-arc table and node v entry v of the supplies.
 *
 * Plain data the caller fills in. A call reads it and the tables it points to while the call runs, changes none of
 * them, and keeps nothing of them afterwards. Self-loops and parallel arcs are allowed.
 *
 * What each pointer must point to:
 *
 * - tails, heads, capacities and costs: arc_count entries each;
 * - lower_bounds: null, where every arc's lower bound is zero, or arc_count entries;
 * - supplies: node_count entries.
 *
 * A table of no entries needs no storage: where its count is zero, a pointer may be null and is not read. Every
 * other pointer is non-null, aligned for its type, and points to that many initialised entries within one object,
 * which nothing writes while a call reads them; several calls may read the same tables at once. Whether the numbers
 * lie in the range a solve admits is the solve's to check; one that does not is refused with its breach named.
 */
typedef struct kahlnws_problem {
  /**
   * @brief The number of nodes: the entries of supplies.
   */
  size_t node_count;
  /**
   * @brief The number of arcs: the entries of each per-arc table.
   */
  size_t arc_count;
  /**
   * @brief The node each arc leaves, one per arc.
   */
  const size_t *tails;
  /**
   * @brief The node each arc enters, one per arc.
   */
  const size_t *heads;
  /**
   * @brief The least flow of each arc, in units of flow, one per arc; null where every arc's is zero.
   */
  const uint64_t *lower_bounds;
  /**
   * @brief The most flow of each arc, in units of flow, one per arc.
   */
  const uint64_t *capacities;
  /**
   * @brief The cost of a unit of flow on each arc, in units of cost, of either sign, one per arc.
   */
  const int64_t *costs;
  /**
   * @brief The supply of each node, in units of flow, one per node: positive where it sends flow, negative where it
   * takes flow. They add up to zero in a problem a solve admits.
   */
  const int64_t *supplies;
} kahlnws_problem;

/**
 * @brief A signed 128-bit integer as its two 64-bit halves: the value is high * 2^64 + low. The cost of a flow, the
 * sum of supplies that do not balance, and a supply past its range.
 *
 * low is the value modulo 2^64, from 0 to 2^64 - 1, and high the value divided by 2^64 and rounded down,
 * from -2^63 to 2^63 - 1: the low and the high 64 bits of the value's two's complement. Every value has exactly
 * one pair of halves, so:
 *
 * - a value fits an int64_t exactly where high is 0 and low is below 2^63 (the value is low), or high is
 *   -1 and low is at least 2^63 (the value is low - 2^64);
 * - two values compare as their high halves compare, signed, and where those are equal as their low halves,
 *   unsigned;
 * - on a compiler with a 128-bit integer, the value is
 *   (__int128) (((unsigned __int128) (uint64_t) high << 64) | low).
 */
typedef struct kahlnws_int128 {
  /**
   * @brief The low 64 bits: the value modulo 2^64.
   */
  uint64_t low;
  /**
   * @brief The high 64 bits, signed: the value divided by 2^64, rounded down.
   */
  int64_t high;
} kahlnws_int128;

/**
 * @brief What one solve did, counted in pivots: exact counts of that solve, not samples or estimates.
 * kahlnws_solver_solve and kahlnws_network_solve write it, where asked, on KAHLNWS_STATUS_OPTIMAL and on
 * KAHLNWS_STATUS_INFEASIBLE.
 *
 * A pivot is an arc brought into the basis, a spanning tree, or flipped to its other bound. Either way the arc
 * violated its bound, and the most flow the cycle it closes with the tree carries is sent around that cycle: where a
 * link of the tree has the least room, that link leaves and the arc takes its place; where the arc's own capacity
 * less its lower bound is the least, no link leaves and the arc moves to its other bound, a flip. A flip sends that
 * whole difference, which is above zero, since an arc whose capacity equals its lower bound never enters.
 *
 * Plain data. The counts kahlnws_solver_solve writes depend on the problem alone, its arc order included, and on the
 * library's version: the same problem gives the same work from a fresh solver and from one that solved others before.
 * Those kahlnws_network_solve writes depend on the problems the network loaded and on the calls made on it since. In
 * every kahlnws_work a solve writes, degenerate_pivots <= pivots and first_arc_pivots <= pivots, and the pivots
 * that sent flow are pivots - degenerate_pivots. A solve of a solver that answers ran
 * pivots - first_arc_pivots + 1 block searches.
 */
typedef struct kahlnws_work {
  /**
   * @brief The pivots the solve made, flips included. 0 where no arc of the problem ever violated its bound, as in a
   * problem of no arc.
   */
  uint64_t pivots;
  /**
   * @brief Of the pivots, those that sent no flow: the link that left had no room, so the flow is as it was and only
   * the tree and the potentials changed. At most pivots; never a flip. 0 where every pivot sent flow.
   */
  uint64_t degenerate_pivots;
  /**
   * @brief Of the pivots, those made by the arcs the solve tries before block search, chosen from the problem's
   * structure, each where it violated its bound when its turn came. At most pivots. 0 where none was chosen or none
   * violated, and in a network's solve, which tries none; pivots where those arcs alone reached the outcome, optimal
   * or infeasible.
   */
  uint64_t first_arc_pivots;
} kahlnws_work;

/**
 * @brief Which breach a refusal names, by a number fixed for good: one of the KAHLNWS_REFUSAL_KIND_ constants.
 *
 * A problem a solve or a load refuses is refused with a kind from KAHLNWS_REFUSAL_KIND_TOO_LARGE to
 * KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE. A change a network refuses is refused with
 * KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY, KAHLNWS_REFUSAL_KIND_COST_BEYOND_BOUND or
 * KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE, for the reason a problem would be, or with
 * KAHLNWS_REFUSAL_KIND_SUPPLY_BEYOND_RANGE or KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE, which only a change meets.
 *
 * The numbers are part of the binary interface: none is ever changed or given to another breach. A number no
 * constant has, zero among them, names no breach; a zeroed kahlnws_refusal names none. Plain data: a caller may
 * hold any number.
 */
typedef uint32_t kahlnws_refusal_kind;
enum {
  /**
   * @brief The problem has more nodes or arcs than a solve counts: node_count + 1 or arc_count + node_count passes
   * 2^32 - 1. Sets node_count and arc_count.
   */
  KAHLNWS_REFUSAL_KIND_TOO_LARGE = 1,
  /**
   * @brief An arc leaves a node the problem does not have. Sets arc, node (its tail) and node_count.
   */
  KAHLNWS_REFUSAL_KIND_TAIL_OUTSIDE_NODES = 2,
  /**
   * @brief An arc enters a node the problem does not have. Sets arc, node (its head) and node_count.
   */
  KAHLNWS_REFUSAL_KIND_HEAD_OUTSIDE_NODES = 3,
  /**
   * @brief An arc's lower bound passes its capacity, in a problem or as a change of a network would set them. Sets
   * arc, lower_bound and capacity.
   */
  KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY = 4,
  /**
   * @brief An arc's cost lies beyond plus or minus kahlnws_cost_bound(node_count), in a problem or as a change of a
   * network would set it. Sets arc, cost and node_count.
   */
  KAHLNWS_REFUSAL_KIND_COST_BEYOND_BOUND = 5,
  /**
   * @brief The magnitudes of the costs times the capacities pass 2^127 - 1: added up in arc order in a problem, or
   * with the cost or the capacity a change of a network would set. Sets arc: in a problem the arc at which the sum
   * passes, for a change the arc changed.
   */
  KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE = 6,
  /**
   * @brief The supplies do not add up to zero. Sets supply_total.
   */
  KAHLNWS_REFUSAL_KIND_UNBALANCED = 7,
  /**
   * @brief The supplies net of the lower bounds add up past 2^64 - 2 where they are positive; a node's net supply is
   * its supply, less the lower bounds of the arcs that leave it, plus those of the arcs that enter it. Sets
   * net_supply_total.
   */
  KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE = 8,
  /**
   * @brief A change of a network would leave more flow still to be sent than a network holds between solves: what the
   * last load or a solve that found no flow left to be sent, with what changes since moved that the arcs could not
   * carry at once, plus this change's amount, adds up past 2^64 - 2. A load leaves to be sent what its start does not
   * route, up to the whole of the positive supplies net of the lower bounds where it starts from no basis, so a change
   * right after a load of supplies near that limit can be refused. A solve that finds a flow leaves none. No solve of a
   * solver answers it. Sets excess_total to that sum.
   */
  KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE = 9,
  /**
   * @brief A move of supply of a network would take a node's supply past the range of an int64_t: below its least
   * value for the node the amount is taken from, above its greatest for the node it is added to. Sets node, the node
   * the amount is taken from where both would leave the range, and supply to the supply it would have.
   */
  KAHLNWS_REFUSAL_KIND_SUPPLY_BEYOND_RANGE = 10,
};

/**
 * @brief An unsigned 128-bit integer as its two 64-bit halves: the value is high * 2^64 + low. The sum of the
 * positive supplies net of the lower bounds that passes its limit, and the flow still to be sent that would pass its
 * own.
 *
 * low is the value modulo 2^64 and high the value divided by 2^64, rounded down, both from 0 to 2^64 - 1.
 * A value fits a uint64_t exactly where high is 0, and two values compare as their high halves and, where those
 * are equal, as their low halves.
 */
typedef struct kahlnws_uint128 {
  /**
   * @brief The low 64 bits: the value modulo 2^64.
   */
  uint64_t low;
  /**
   * @brief The high 64 bits: the value divided by 2^64, rounded down.
   */
  uint64_t high;
} kahlnws_uint128;

/**
 * @brief Why a solve or a load refused a problem, or a network a change: the breach, by its kind, and the values that
 * make it one.
 *
 * A problem with several breaches is refused with the first in the order of the kinds' numbers, where the kinds
 * about one arc, from KAHLNWS_REFUSAL_KIND_TAIL_OUTSIDE_NODES to KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE,
 * are taken arc by arc: the lowest arc with any of them, and its first. Each change of a network says in which order
 * it refuses.
 *
 * Plain data. Each kind sets the fields its constant names; every other field is zero. kahlnws_refusal_message
 * writes the same breach as text.
 */
typedef struct kahlnws_refusal {
  /**
   * @brief Which breach: one of the KAHLNWS_REFUSAL_KIND_ constants.
   */
  kahlnws_refusal_kind kind;
  /**
   * @brief The arc the breach is at; zero where the kind sets no arc.
   */
  size_t arc;
  /**
   * @brief The node outside the problem's nodes, the arc's tail or its head; for a supply beyond its range, the node
   * whose supply would leave it; zero where the kind sets no node.
   */
  size_t node;
  /**
   * @brief The problem's number of nodes; zero where the kind does not set it.
   */
  size_t node_count;
  /**
   * @brief The problem's number of arcs; zero where the kind does not set it.
   */
  size_t arc_count;
  /**
   * @brief The arc's lower bound, in units of flow; zero where the kind does not set it.
   */
  uint64_t lower_bound;
  /**
   * @brief The arc's capacity, in units of flow; zero where the kind does not set it.
   */
  uint64_t capacity;
  /**
   * @brief The arc's cost, in units of cost; zero where the kind does not set it.
   */
  int64_t cost;
  /**
   * @brief The supply the node named by node would have, in units of flow; zero where the kind does not set it.
   */
  struct kahlnws_int128 supply;
  /**
   * @brief The sum of the supplies, in units of flow; zero where the kind does not set it.
   */
  struct kahlnws_int128 supply_total;
  /**
   * @brief The sum of the positive supplies net of the lower bounds, in units of flow; zero where the kind does not set
   * it.
   */
  struct kahlnws_uint128 net_supply_total;
  /**
   * @brief The flow still to be sent and the flow the change moves, added up, in units of flow; zero where the kind
   * does not set it.
   */
  struct kahlnws_uint128 excess_total;
} kahlnws_refusal;

#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

/**
 * @brief A new solver that holds no room yet, for the other entries to use and kahlnws_solver_free to free.
 *
 * It allocates the solver itself, and no room. Safe to call from any thread.
 *
 * @return The solver, or null where the memory for it is refused; nothing else answers null, since nothing in it can
 * fail otherwise, and an internal error would be answered as null too.
 */
KAHLNWS_API KAHLNWS_NODISCARD struct kahlnws_solver *kahlnws_solver_new(void);

/**
 * @brief Frees solver with all its room.
 *
 * Null is accepted and frees nothing, and so is a pointer not aligned for a solver, which kahlnws_solver_new
 * never answers. After the call the solver is freed: no call may use the pointer again. A solver may be freed on
 * any thread.
 *
 * @param[in] solver the solver to free, or null.
 *
 * @pre solver is null, or a pointer kahlnws_solver_new answered that has not been freed since and that no other
 * call uses meanwhile.
 */
KAHLNWS_API void kahlnws_solver_free(struct kahlnws_solver *solver);

/**
 * @brief Makes room in solver for problems of up to node_count nodes and arc_count arcs.
 *
 * A solve of such a problem allocates nothing afterwards. Room already held is kept.
 *
 * @param[in,out] solver the solver whose room is made.
 * @param[in] node_count the most nodes a problem will have.
 * @param[in] arc_count the most arcs a problem will have.
 *
 * @retval KAHLNWS_STATUS_OK the room is made;
 * @retval KAHLNWS_STATUS_NO_ROOM the memory was refused; the room held before is kept, and the solver may be used;
 * @retval KAHLNWS_STATUS_NULL_POINTER solver is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT solver is not aligned for a solver;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught.
 *
 * @pre solver is null, or a pointer kahlnws_solver_new answered that has not been freed since and that no other
 * call uses meanwhile: one thread at a time uses a solver.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_solver_reserve(struct kahlnws_solver *solver,
                                      size_t node_count,
                                      size_t arc_count);

/**
 * @brief Solves problem with solver: finds a flow of least cost, or that no flow within the bounds meets the
 * supplies, in exact integers.
 *
 * On every status but KAHLNWS_STATUS_OPTIMAL the flows, the potentials and the cost are as they were; the work is
 * written on KAHLNWS_STATUS_OPTIMAL and KAHLNWS_STATUS_INFEASIBLE alone, and the refusal on
 * KAHLNWS_STATUS_REFUSED alone; an internal error leaves the outputs unspecified.
 *
 * Room. Before the problem is checked, the solver's room is grown to the problem's counts and kept, so that refused
 * memory is answered rather than ending the process. So a refused problem may still have grown the room, and a
 * problem too large for memory answers KAHLNWS_STATUS_NO_ROOM before it could be refused as too large. Where the
 * room already holds the problem, by kahlnws_solver_reserve or an earlier solve, the call allocates nothing.
 *
 * The solver keeps its room and nothing else: what the call answers is a function of problem alone, and nothing of
 * an earlier problem is read again.
 *
 * @param[in,out] solver the solver, whose room the solve uses and may grow.
 * @param[in] problem the problem to solve.
 * @param[out] flows where the flow of each arc is written, arc a at a: storage for problem->arc_count entries,
 * which may be null where that count is zero.
 * @param[out] potentials where each node's potential is written, node v at v: storage for problem->node_count
 * entries, or null where the potentials are not wanted.
 * @param[out] total_cost where the cost of the flow is written, or null where it is not wanted.
 * @param[out] work where the work of the solve is written, or null where it is not wanted.
 * @param[out] refusal where the breach of a refused problem is written, or null where it is not wanted.
 *
 * @retval KAHLNWS_STATUS_OPTIMAL the flow of each arc is written to flows, arc a at a; where potentials is not
 * null, each node's potential to it, node v at v; where total_cost is not null, the cost of the flow to it; where
 * work is not null, the work of the solve to it. With the potentials, cost + potential[tail] - potential[head] is
 * at least zero on every arc whose flow is below its capacity and at most zero on every arc whose flow is above its
 * lower bound, which proves the flow optimal;
 * @retval KAHLNWS_STATUS_INFEASIBLE no flow within the bounds meets the supplies; where work is not null, the work of
 * the solve is written to it;
 * @retval KAHLNWS_STATUS_REFUSED the problem lies outside what a solve admits; where refusal is not null, the breach
 * is written to it;
 * @retval KAHLNWS_STATUS_NULL_POINTER solver, problem or flows is null, or a table of the problem other than its
 * lower bounds is null while its count is above zero;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer the call reads or writes is not aligned for its type, or a count of
 * entries passes PTRDIFF_MAX bytes;
 * @retval KAHLNWS_STATUS_NO_ROOM the memory for the room the problem needs was refused;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught.
 *
 * @pre solver is null, or a pointer kahlnws_solver_new answered that has not been freed since and that no other
 * call uses meanwhile: one thread at a time uses a solver;
 * @pre problem is null or points to a kahlnws_problem whose every pointer points to what its doc states;
 * @pre flows points to writable storage for problem->arc_count entries, and may be null where that count is zero;
 * @pre potentials is null or points to writable storage for problem->node_count entries;
 * @pre total_cost, work and refusal are each null or point to writable storage for one value;
 * @pre an array lies within one object; what an output held before does not matter;
 * @pre no output overlaps another output, the problem or its tables; nothing writes the problem or its tables, and
 * nothing else reads or writes the outputs, while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_solver_solve(struct kahlnws_solver *solver,
                                    const struct kahlnws_problem *problem,
                                    uint64_t *flows,
                                    int64_t *potentials,
                                    struct kahlnws_int128 *total_cost,
                                    struct kahlnws_work *work,
                                    struct kahlnws_refusal *refusal);

/**
 * @brief A new network, never loaded, for the other entries to use and kahlnws_network_free to free.
 *
 * It holds the problem of no node and no arc. It allocates the network itself, and no room. Safe to call from any
 * thread.
 *
 * @return The network, or null where the memory for it is refused; nothing else answers null, since nothing in it can
 * fail otherwise, and an internal error would be answered as null too.
 */
KAHLNWS_API KAHLNWS_NODISCARD struct kahlnws_network *kahlnws_network_new(void);

/**
 * @brief A copy of network, for the other entries to use and kahlnws_network_free to free: it holds the problem,
 * the basis and the optimum network holds, and changes apart from it.
 *
 * A copy made before a change is the earlier state itself: a search may copy a network, change and solve the copy,
 * and keep whichever it wants. The copy answers every call as network would, its work included. It allocates its
 * own handle and room, as large as the problem held, and its changes and solves allocate nothing; a network half
 * changed copies as half changed. network is read, never changed. kahlnws_network_copy copies into a network
 * that already exists, reusing its room.
 *
 * @param[in] network the network to copy.
 *
 * @return The copy, or null where network is null or not aligned for a network, or where the memory for the copy's
 * handle or for its room is refused; nothing else answers null, and an internal error would be answered as null too.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network.
 */
KAHLNWS_API KAHLNWS_NODISCARD struct kahlnws_network *kahlnws_network_clone(const struct kahlnws_network *network);

/**
 * @brief Makes destination a copy of source: it holds the problem, the basis and the optimum source holds, and
 * changes apart from it, as a copy kahlnws_network_clone makes does.
 *
 * It reuses the room destination holds: where that room holds a problem of the counts of the problem source holds,
 * by kahlnws_network_reserve or an earlier load or copy, the call allocates nothing. So a search may keep a network
 * for each state it may return to and copy into it without allocating. A network half changed copies as half
 * changed. source is read, never changed; the same network as both is a copy of itself, which changes nothing.
 *
 * @param[in,out] destination the network made a copy, whatever it held before.
 * @param[in] source the network copied.
 *
 * @retval KAHLNWS_STATUS_OK destination is a copy of source;
 * @retval KAHLNWS_STATUS_NO_ROOM the memory for the room the copy needs was refused; destination holds what it held,
 * its optimum included, and its room may have grown;
 * @retval KAHLNWS_STATUS_NULL_POINTER destination or source is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT destination or source is not aligned for a network;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught; destination may be half changed.
 *
 * @pre destination and source are each null, or a pointer kahlnws_network_new or kahlnws_network_clone answered
 * that has not been freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre they are the same pointer, or point to two different networks.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_copy(struct kahlnws_network *destination,
                                    const struct kahlnws_network *source);

/**
 * @brief Frees network with all its room.
 *
 * Null is accepted and frees nothing, and so is a pointer not aligned for a network, which kahlnws_network_new and
 * kahlnws_network_clone never answer. After the call the network is freed: no call may use the pointer again. A
 * network may be freed on any thread.
 *
 * @param[in] network the network to free, or null.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile.
 */
KAHLNWS_API void kahlnws_network_free(struct kahlnws_network *network);

/**
 * @brief Makes room in network for problems of up to node_count nodes and arc_count arcs.
 *
 * A load of such a problem allocates nothing afterwards. Room already held is kept, and nothing else changes: the
 * problem held, and its optimum, stay.
 *
 * @param[in,out] network the network whose room is made.
 * @param[in] node_count the most nodes a problem will have.
 * @param[in] arc_count the most arcs a problem will have.
 *
 * @retval KAHLNWS_STATUS_OK the room is made;
 * @retval KAHLNWS_STATUS_NO_ROOM the memory was refused; the room held before is kept, and the network may be used;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT network is not aligned for a network;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_reserve(struct kahlnws_network *network,
                                       size_t node_count,
                                       size_t arc_count);

/**
 * @brief Makes problem the problem network holds, in place of the one it held, with no optimum answered yet, and a
 * basis of it to start the next kahlnws_network_solve from.
 *
 * The basis is the one the network holds, carried over to the new numbers, where it serves: where the network holds
 * a basis of a problem with the same node count, the same arc count and the same tail and head at every arc, and the
 * supplies net of the lower bounds and of what that basis carries on its arcs out of the tree add up to at most
 * 2^64 - 2 where they are positive. Otherwise, and where the network holds none, never loaded or left half changed,
 * it is the star, every arc at its lower bound, as the first load starts from. Either way the next solve prices every
 * arc.
 *
 * The solve after a load answers the same status and the same cost whichever basis the load started from; the flow
 * among tied optima, the potentials and the work may differ, each a function of the calls made on the network, which
 * is deterministic. Where only some numbers changed since the problem the network held, the solve often makes far
 * fewer pivots than one from the star; the same problem loaded again, from the network's optimum, makes none. So a
 * network that loads new numbers on the same arcs each day carries its basis from day to day.
 *
 * The problem's tables are copied: the caller may change or free them after the call. The load refuses what
 * kahlnws_solver_solve refuses, with the same breach; a refused load, and refused memory, leave the network as it
 * was, its basis and its optimum included, and the next load starts from that basis where it serves. A load mends a
 * network half changed. It costs a few passes over the arcs and the nodes.
 *
 * Room. Before the problem is checked, the network's room is grown to the problem's counts and kept, so that refused
 * memory is answered rather than ending the process; where the room already holds the problem, by
 * kahlnws_network_reserve or an earlier load, the call allocates nothing.
 *
 * @param[in,out] network the network, whose room the load uses and may grow.
 * @param[in] problem the problem to hold.
 * @param[out] refusal where the breach of a refused problem is written, or null where it is not wanted.
 *
 * @retval KAHLNWS_STATUS_OK the network holds the problem;
 * @retval KAHLNWS_STATUS_REFUSED the problem lies outside what a solve admits; where refusal is not null, the breach
 * is written to it;
 * @retval KAHLNWS_STATUS_NULL_POINTER network or problem is null, or a table of the problem other than its lower
 * bounds is null while its count is above zero;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer the call reads or writes is not aligned for its type, or a count of
 * entries passes PTRDIFF_MAX bytes;
 * @retval KAHLNWS_STATUS_NO_ROOM the memory for the room the problem needs was refused;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre problem is null or points to a kahlnws_problem whose every pointer points to what its doc states;
 * @pre refusal is null or points to writable storage for one value, whatever it held before;
 * @pre the refusal overlaps neither the problem nor its tables; nothing writes the problem or its tables, and nothing
 * else reads or writes the refusal, while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_load(struct kahlnws_network *network,
                                    const struct kahlnws_problem *problem,
                                    struct kahlnws_refusal *refusal);

/**
 * @brief The number of nodes of the problem network holds: 0 for a network never loaded.
 *
 * Every node a change or a read names is below it. It reads, and never fails.
 *
 * @param[in] network the network, or null.
 *
 * @return The number of nodes; 0 where network is null or not aligned for a network.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network.
 */
KAHLNWS_API size_t kahlnws_network_node_count(const struct kahlnws_network *network);

/**
 * @brief The number of arcs of the problem network holds: 0 for a network never loaded.
 *
 * Every arc a change or a read names is below it, and the flows kahlnws_network_flows writes are as many. It reads,
 * and never fails.
 *
 * @param[in] network the network, or null.
 *
 * @return The number of arcs; 0 where network is null or not aligned for a network.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network.
 */
KAHLNWS_API size_t kahlnws_network_arc_count(const struct kahlnws_network *network);

/**
 * @brief Makes cost the cost of a unit of flow on arc of the problem network holds, in units of cost.
 *
 * The network then holds the problem with that cost, with a basis of it, from which the next kahlnws_network_solve
 * answers the optimum of the problem as it now stands. The cost the arc already has changes nothing, and keeps an
 * optimum the network held; any other cost ends it. Setting the cost written to previous back makes the problem
 * the earlier one again.
 *
 * It costs a few steps where the change moves no node's potential, and otherwise the nodes whose potentials it
 * moves, which can be most of them; the next solve prices the arc, and where potentials moved, the arcs at the
 * smaller of the two groups of nodes, those whose potentials moved and the rest, which can be half the network. It
 * allocates nothing.
 *
 * @param[in,out] network the network.
 * @param[in] arc the arc, below kahlnws_network_arc_count(network).
 * @param[in] cost its new cost, in units of cost.
 * @param[out] previous where the cost the arc had is written, or null where it is not wanted.
 * @param[out] refusal where the breach of a refused change is written, or null where it is not wanted.
 *
 * @retval KAHLNWS_STATUS_OK the arc has the cost; where previous is not null, the cost it had is written to it;
 * @retval KAHLNWS_STATUS_REFUSED nothing changed; where refusal is not null, the breach is written to it, of kind
 * KAHLNWS_REFUSAL_KIND_COST_BEYOND_BOUND where cost lies beyond plus or minus kahlnws_cost_bound of the node
 * count, else KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE where with it the magnitudes of the costs times the
 * capacities would add up past 2^127 - 1;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT arc is not below the arc count, which on a network never loaded no arc is,
 * or a pointer is not aligned for its type; nothing changed;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre previous and refusal are each null or point to writable storage for one value, whatever it held before;
 * @pre neither overlaps the other, and nothing else reads or writes them while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_set_cost(struct kahlnws_network *network,
                                        size_t arc,
                                        int64_t cost,
                                        int64_t *previous,
                                        struct kahlnws_refusal *refusal);

/**
 * @brief Makes capacity the most flow of arc of the problem network holds, in units of flow.
 *
 * The network then holds the problem with that capacity, with a basis of it, from which the next
 * kahlnws_network_solve answers the optimum of the problem as it now stands. The capacity the arc already has
 * changes nothing, and keeps an optimum the network held; any other ends it. Setting the capacity written to
 * previous back makes the problem the earlier one again. A capacity of zero on an arc whose lower bound is zero
 * switches the arc off, and the capacity it had, set back, switches it on: a caller may build every arc it may ever
 * need up front and switch each on and off. Where both bounds are to move past each other's old value,
 * kahlnws_network_set_bounds moves them at once.
 *
 * It costs a few steps where the arc's flow can stay where it is, and otherwise what kahlnws_network_move_supply
 * of the flow the change moves costs, that flow being at most the change of the capacity; switching off an arc that
 * carries flow always moves it. The next solve prices the arc, and what that move of supply asks it to. It allocates
 * nothing.
 *
 * @param[in,out] network the network.
 * @param[in] arc the arc, below kahlnws_network_arc_count(network).
 * @param[in] capacity its new capacity, in units of flow.
 * @param[out] previous where the capacity the arc had is written, or null where it is not wanted.
 * @param[out] refusal where the breach of a refused change is written, or null where it is not wanted.
 *
 * @retval KAHLNWS_STATUS_OK the arc has the capacity; where previous is not null, the capacity it had is written to
 * it;
 * @retval KAHLNWS_STATUS_REFUSED nothing changed; where refusal is not null, the breach is written to it, of kind
 * KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY where capacity is below the arc's lower bound, else
 * KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE where with it the magnitudes of the costs times the capacities would
 * add up past 2^127 - 1, else KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE where the flow still to be sent and the flow
 * this change moves add up past 2^64 - 2. The capacity written to previous, set back directly after, is admitted
 * wherever the flow still to be sent after the change and the change of the capacity add up to at most 2^64 - 2, so
 * wherever that flow before the change and twice the change of the capacity do: never refused for capacities and a flow
 * below 2^62;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT arc is not below the arc count, which on a network never loaded no arc is,
 * or a pointer is not aligned for its type; nothing changed;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre previous and refusal are each null or point to writable storage for one value, whatever it held before;
 * @pre neither overlaps the other, and nothing else reads or writes them while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_set_capacity(struct kahlnws_network *network,
                                            size_t arc,
                                            uint64_t capacity,
                                            uint64_t *previous,
                                            struct kahlnws_refusal *refusal);

/**
 * @brief Makes lower_bound the least flow of arc of the problem network holds, in units of flow.
 *
 * The network then holds the problem with that lower bound, with a basis of it, from which the next
 * kahlnws_network_solve answers the optimum of the problem as it now stands; every flow it answers includes its
 * arc's lower bound. The lower bound the arc already has changes nothing, and keeps an optimum the network held; any
 * other ends it. Setting the lower bound written to previous back makes the problem the earlier one again. Where
 * both bounds are to move past each other's old value, kahlnws_network_set_bounds moves them at once. A lower bound
 * changes no supply: the supplies net of the lower bounds may so come to pass the limit
 * KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE names, where the arcs carry the excess, and the network still solves
 * them exactly, where a solver refuses them.
 *
 * It costs a few steps where the arc's flow can stay where it is, and otherwise what kahlnws_network_move_supply
 * of the flow the change moves costs, that flow being at most the change of the lower bound; an arc at its old lower
 * bound always moves by the change. The next solve prices the arc, and what that move of supply asks it to. It
 * allocates nothing.
 *
 * @param[in,out] network the network.
 * @param[in] arc the arc, below kahlnws_network_arc_count(network).
 * @param[in] lower_bound its new lower bound, in units of flow.
 * @param[out] previous where the lower bound the arc had is written, or null where it is not wanted.
 * @param[out] refusal where the breach of a refused change is written, or null where it is not wanted.
 *
 * @retval KAHLNWS_STATUS_OK the arc has the lower bound; where previous is not null, the lower bound it had is
 * written to it;
 * @retval KAHLNWS_STATUS_REFUSED nothing changed; where refusal is not null, the breach is written to it, of kind
 * KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY where lower_bound is above the arc's capacity, else
 * KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE where the flow still to be sent and the flow this change moves add up past
 * 2^64 - 2. The lower bound written to previous, set back directly after, is admitted wherever the flow still to be
 * sent after the change and the change of the lower bound add up to at most 2^64 - 2, so wherever that flow before
 * the change and twice the change of the lower bound do: never refused for bounds and a flow below 2^62;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT arc is not below the arc count, which on a network never loaded no arc is,
 * or a pointer is not aligned for its type; nothing changed;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre previous and refusal are each null or point to writable storage for one value, whatever it held before;
 * @pre neither overlaps the other, and nothing else reads or writes them while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_set_lower_bound(struct kahlnws_network *network,
                                               size_t arc,
                                               uint64_t lower_bound,
                                               uint64_t *previous,
                                               struct kahlnws_refusal *refusal);

/**
 * @brief Makes lower_bound and capacity the least and the most flow of arc of the problem network holds at
 * once, in units of flow.
 *
 * The single setters check each bound against the other the arc holds, so where both bounds move past each other's
 * old value, the order of two calls matters and one order is refused: bounds of 0 to 2 become 3 to 4 by the capacity
 * first, and 3 to 4 become 0 to 2 by the lower bound first. This sets both at once, with no order, and checks the
 * pair alone. Otherwise it is kahlnws_network_set_capacity and kahlnws_network_set_lower_bound in one: the
 * network then holds the problem with those bounds, with a basis of it; the bounds the arc already has change nothing
 * and keep an optimum the network held; any others end it; the bounds written to previous_lower_bound and
 * previous_capacity, set back, make the problem the earlier one again.
 *
 * It costs a few steps where the arc's flow can stay where it is, and otherwise what kahlnws_network_move_supply of
 * the flow the change moves costs, that flow being at most the larger change of the two bounds. The next solve
 * prices the arc, and what that move of supply asks it to. It allocates nothing.
 *
 * @param[in,out] network the network.
 * @param[in] arc the arc, below kahlnws_network_arc_count(network).
 * @param[in] lower_bound its new lower bound, in units of flow.
 * @param[in] capacity its new capacity, in units of flow.
 * @param[out] previous_lower_bound where the lower bound the arc had is written, or null where it is not wanted.
 * @param[out] previous_capacity where the capacity the arc had is written, or null where it is not wanted.
 * @param[out] refusal where the breach of a refused change is written, or null where it is not wanted.
 *
 * @retval KAHLNWS_STATUS_OK the arc has the bounds; where previous_lower_bound and previous_capacity are not null,
 * the lower bound and the capacity it had are written to them;
 * @retval KAHLNWS_STATUS_REFUSED nothing changed; where refusal is not null, the breach is written to it, of kind
 * KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY where lower_bound is above capacity, else
 * KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE where with that capacity the magnitudes of the costs times the
 * capacities would add up past 2^127 - 1, else KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE where the flow still to be
 * sent and the flow this change moves add up past 2^64 - 2. The bounds written, set back directly after, are admitted
 * wherever the flow still to be sent after the change and the larger change of the two bounds add up to at most 2^64 -
 * 2, so wherever that flow before the change and twice the larger change do: never refused for bounds and a flow below
 * 2^62;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT arc is not below the arc count, which on a network never loaded no arc is,
 * or a pointer is not aligned for its type; nothing changed;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre previous_lower_bound, previous_capacity and refusal are each null or point to writable storage for one
 * value, whatever it held before;
 * @pre none overlaps another, and nothing else reads or writes them while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_set_bounds(struct kahlnws_network *network,
                                          size_t arc,
                                          uint64_t lower_bound,
                                          uint64_t capacity,
                                          uint64_t *previous_lower_bound,
                                          uint64_t *previous_capacity,
                                          struct kahlnws_refusal *refusal);

/**
 * @brief Makes supply the supply of node in the problem network holds, in units of flow, a demand below zero; the
 * supply of balancing_node moves by the same amount the other way, so the supplies still add up to zero.
 *
 * A depot that is to send 2 units where it sent 4, balanced by the customer it sends to, is
 * kahlnws_network_set_supply(network, depot, 2, customer, &had, NULL): the customer then takes 2 less. The same call
 * with the supply written to previous sets both nodes back. The network then holds the problem with those supplies,
 * with a basis of it, from which the next kahlnws_network_solve answers the optimum of the problem as it now stands.
 * The supply node already has changes nothing, is never refused, and keeps an optimum the network held, as a move of
 * nothing does; any other ends it. A node that balances itself is given only the supply it has, which changes nothing;
 * any other supply for it answers KAHLNWS_STATUS_INVALID_ARGUMENT, as an index not below its count does.
 *
 * It is kahlnws_network_move_supply of the difference between the supply asked for and the one node has: from
 * balancing_node to node where the supply rises, from node to balancing_node where it falls. It costs and
 * marks what that move costs and marks, and allocates nothing.
 *
 * @param[in,out] network the network.
 * @param[in] node the node whose supply is set, below kahlnws_network_node_count(network).
 * @param[in] supply its new supply, in units of flow: positive where it sends, negative where it takes.
 * @param[in] balancing_node the node whose supply moves the other way, below kahlnws_network_node_count(network).
 * @param[out] previous where the supply node had is written, or null where it is not wanted.
 * @param[out] refusal where the breach of a refused change is written, or null where it is not wanted.
 *
 * @retval KAHLNWS_STATUS_OK node has the supply, and balancing_node the difference the other way; where previous
 * is not null, the supply node had is written to it;
 * @retval KAHLNWS_STATUS_REFUSED nothing changed; where refusal is not null, the breach is written to it, of kind
 * KAHLNWS_REFUSAL_KIND_SUPPLY_BEYOND_RANGE where the supply of balancing_node would leave the range of int64_t,
 * naming it with the supply it would have; else KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE where the flow still to be
 * sent and the difference add up past 2^64 - 2. The supply written to previous, set back directly after, is
 * admitted where the move it makes would be, as kahlnws_network_move_supply says;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT node or balancing_node is not below the node count, which on a network
 * never loaded no node is; or node is balancing_node and supply is not the supply it has; or a pointer is not
 * aligned for its type; nothing changed;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre previous and refusal are each null or point to writable storage for one value, whatever it held before;
 * @pre neither overlaps the other, and nothing else reads or writes them while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_set_supply(struct kahlnws_network *network,
                                          size_t node,
                                          int64_t supply,
                                          size_t balancing_node,
                                          int64_t *previous,
                                          struct kahlnws_refusal *refusal);

/**
 * @brief Lowers the supply of from_node by amount and raises the supply of to_node by amount, in units of flow,
 * in the problem network holds, so the supplies still add up to zero; a move of demand is written from the node that
 * will demand more, a customer that needs two more units, sent by a depot, being kahlnws_network_move_supply(network,
 * customer, depot, 2, NULL).
 *
 * A node that sends gains supply, one that receives gains demand, a supply below zero. A node is brought to a supply
 * it should have, balanced by another node, by kahlnws_network_set_supply. The network then holds the problem with
 * those supplies, with a basis of it, from which the next kahlnws_network_solve answers the optimum of the problem
 * as it now stands. The same node twice, or an amount of zero, moves nothing: it changes nothing, is never refused,
 * and keeps an optimum the network held; any other move ends it. The move back,
 * kahlnws_network_move_supply(network, to_node, from_node, amount, NULL), makes the problem the earlier one again.
 * Called directly after the move, it is admitted wherever the move left the flow still to be sent as it was or lower,
 * always where the routed flow carried the move, and wherever that flow before the move and twice the amount add up
 * to at most 2^64 - 2: so never for amounts and a flow below 2^62. Other calls in between can leave that flow
 * higher, a solve that finds no flow among them, which does not lower it. A move back that is refused stays refused
 * until the flow still to be sent falls, by a solve that finds a flow or by other moves; a kahlnws_network_load of
 * the earlier problem always undoes it. A caller that holds new supplies whole pairs each node whose supply rises with
 * one whose supply falls and moves the lesser of their two remaining differences, until none is left: every supply on
 * the way lies between its old and its new value. The supplies a network comes to hold always lie within int64_t;
 * their net supplies may pass the limit KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE names, where the arcs carry the
 * difference, and the network still solves them exactly, where a solver refuses them.
 *
 * It costs a walk from each of the two nodes along the flow the network routes, as far as their routes meet, where
 * that flow can carry the amount; where it cannot, the nodes whose potentials move, up to most of the network, and
 * where flow comes to be left to be sent, a read of each node that heads a part of the network. The next solve prices
 * no arc for a move the routed flow carried, and otherwise the arcs at the smaller group of the nodes whose
 * potentials moved and the rest. It allocates nothing.
 *
 * @param[in,out] network the network.
 * @param[in] from_node the node whose supply falls, below kahlnws_network_node_count(network).
 * @param[in] to_node the node whose supply rises, below kahlnws_network_node_count(network).
 * @param[in] amount the units of flow moved.
 * @param[out] refusal where the breach of a refused change is written, or null where it is not wanted.
 *
 * @retval KAHLNWS_STATUS_OK the supplies are moved, or nothing was to move;
 * @retval KAHLNWS_STATUS_REFUSED nothing changed; where refusal is not null, the breach is written to it, of kind
 * KAHLNWS_REFUSAL_KIND_SUPPLY_BEYOND_RANGE where the supply of from_node less amount falls below the least
 * int64_t, or the supply of to_node plus amount passes the greatest, naming the first of the two in that order;
 * else KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE where the flow still to be sent and amount add up past 2^64 - 2;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT from_node or to_node is not below the node count, which on a network
 * never loaded no node is, or refusal is not aligned for its type; nothing changed;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre refusal is null or points to writable storage for one value, whatever it held before, that nothing else reads
 * or writes while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_move_supply(struct kahlnws_network *network,
                                           size_t from_node,
                                           size_t to_node,
                                           uint64_t amount,
                                           struct kahlnws_refusal *refusal);

/**
 * @brief Solves the problem network holds: finds a flow of least cost for the problem as it stands, or that no flow
 * within the bounds meets the supplies, in exact integers, and keeps it for the reads.
 *
 * It pivots from the basis the network holds: the one its last solve ended on and the changes since left, or the one
 * a load started from. A network nothing changed since its last solve answers the same again, prices no arc and makes
 * no pivot; a solve after changes that call for no pivot prices only the arcs those changes may have made worth
 * using; and a solve that pivots prices at least one whole pass over the arcs. Where a change leaves much flow still
 * to be sent and the solve makes many pivots, it can take longer than a load of the whole problem as it stands and
 * the solve after it. It tries no arcs first, so the first_arc_pivots it writes are 0. A network never loaded
 * answers the optimum of no flow at the cost zero, with no work. It allocates nothing.
 *
 * After KAHLNWS_STATUS_OPTIMAL the reads answer that optimum until the next load or change that changes the
 * problem; after KAHLNWS_STATUS_INFEASIBLE they answer KAHLNWS_STATUS_NO_OPTIMUM. The total cost and the
 * potentials are those of the problem with its lower bounds, and every flow includes its arc's lower bound.
 *
 * @param[in,out] network the network.
 * @param[out] total_cost where the cost of the flow is written, or null where it is not wanted.
 * @param[out] work where the work of the solve is written, or null where it is not wanted.
 *
 * @retval KAHLNWS_STATUS_OPTIMAL a flow of least cost is found; where total_cost is not null, its cost is written to
 * it, and where work is not null, the work of the solve;
 * @retval KAHLNWS_STATUS_INFEASIBLE no flow within the bounds meets the supplies; where work is not null, the work of
 * the solve is written to it;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer is not aligned for its type;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre total_cost and work are each null or point to writable storage for one value, whatever it held before;
 * @pre neither overlaps the other, and nothing else reads or writes them while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_solve(struct kahlnws_network *network,
                                     struct kahlnws_int128 *total_cost,
                                     struct kahlnws_work *work);

/**
 * @brief Writes the cost of the optimum network holds, in units of cost times units of flow.
 *
 * A network holds an optimum where the last call that loaded, changed or solved it was a kahlnws_network_solve
 * that answered KAHLNWS_STATUS_OPTIMAL, and on a network never loaded, whose optimum is that of no node; a change
 * that was refused, or that set the number already held, is no change. It reads, changes nothing, and costs a few
 * steps.
 *
 * @param[in] network the network.
 * @param[out] total_cost where the cost is written.
 *
 * @retval KAHLNWS_STATUS_OK the cost is written to total_cost;
 * @retval KAHLNWS_STATUS_NO_OPTIMUM the network holds no optimum; nothing is written;
 * @retval KAHLNWS_STATUS_NULL_POINTER network or total_cost is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer is not aligned for its type;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre total_cost is null or points to writable storage for one value, whatever it held before, that nothing else
 * reads or writes while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_total_cost(const struct kahlnws_network *network,
                                          struct kahlnws_int128 *total_cost);

/**
 * @brief Writes the flow of arc in the optimum network holds, in units of flow, its lower bound included.
 *
 * A network holds an optimum as kahlnws_network_total_cost says. It reads one flow, changes nothing, and costs a
 * few steps.
 *
 * @param[in] network the network.
 * @param[in] arc the arc, below kahlnws_network_arc_count(network).
 * @param[out] flow where the flow is written.
 *
 * @retval KAHLNWS_STATUS_OK the flow is written to flow;
 * @retval KAHLNWS_STATUS_NO_OPTIMUM the network holds no optimum; nothing is written;
 * @retval KAHLNWS_STATUS_NULL_POINTER network or flow is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT arc is not below the arc count, or a pointer is not aligned for its type;
 * nothing is written;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre flow is null or points to writable storage for one value, whatever it held before, that nothing else reads or
 * writes while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_flow(const struct kahlnws_network *network,
                                    size_t arc,
                                    uint64_t *flow);

/**
 * @brief Writes the flow of every arc in the optimum network holds, arc a at a, in units of flow, each with its
 * arc's lower bound.
 *
 * A network holds an optimum as kahlnws_network_total_cost says. It reads every flow once, changes nothing, and
 * allocates nothing.
 *
 * @param[in] network the network.
 * @param[out] flows where the flows are written: storage for kahlnws_network_arc_count(network) entries, which may be
 * null where that count is zero.
 *
 * @retval KAHLNWS_STATUS_OK the flow of each arc is written to flows;
 * @retval KAHLNWS_STATUS_NO_OPTIMUM the network holds no optimum; nothing is written;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null, or flows is null while the arc count is above zero;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer is not aligned for its type;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre flows points to writable storage for kahlnws_network_arc_count(network) entries within one object, whatever
 * it held before, and may be null where that count is zero; nothing else reads or writes it while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_flows(const struct kahlnws_network *network,
                                     uint64_t *flows);

/**
 * @brief Writes each node's potential in the optimum network holds, node v at v, in units of cost.
 *
 * With the potentials, cost + potential[tail] - potential[head] is at least zero on every arc whose flow is below
 * its capacity and at most zero on every arc whose flow is above its lower bound, which proves the flow optimal. A
 * network holds an optimum as kahlnws_network_total_cost says. It reads every potential once, changes nothing,
 * and allocates nothing.
 *
 * @param[in] network the network.
 * @param[out] potentials where the potentials are written: storage for kahlnws_network_node_count(network) entries,
 * which may be null where that count is zero.
 *
 * @retval KAHLNWS_STATUS_OK each node's potential is written to potentials;
 * @retval KAHLNWS_STATUS_NO_OPTIMUM the network holds no optimum; nothing is written;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null, or potentials is null while the node count is above zero;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer is not aligned for its type;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre potentials points to writable storage for kahlnws_network_node_count(network) entries within one object,
 * whatever it held before, and may be null where that count is zero; nothing else reads or writes it while the call
 * runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_potentials(const struct kahlnws_network *network,
                                          int64_t *potentials);

/**
 * @brief Writes the node arc leaves in the problem network holds.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
 *
 * @param[in] network the network.
 * @param[in] arc the arc, below kahlnws_network_arc_count(network).
 * @param[out] tail where its tail is written.
 *
 * @retval KAHLNWS_STATUS_OK its tail is written to tail;
 * @retval KAHLNWS_STATUS_NULL_POINTER network or tail is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT arc is not below the arc count, or a pointer is not aligned for its type;
 * nothing is written;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre tail is null or points to writable storage for one value, whatever it held before, that nothing else reads or
 * writes while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_tail(const struct kahlnws_network *network,
                                    size_t arc,
                                    size_t *tail);

/**
 * @brief Writes the node arc enters in the problem network holds.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
 *
 * @param[in] network the network.
 * @param[in] arc the arc, below kahlnws_network_arc_count(network).
 * @param[out] head where its head is written.
 *
 * @retval KAHLNWS_STATUS_OK its head is written to head;
 * @retval KAHLNWS_STATUS_NULL_POINTER network or head is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT arc is not below the arc count, or a pointer is not aligned for its type;
 * nothing is written;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre head is null or points to writable storage for one value, whatever it held before, that nothing else reads or
 * writes while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_head(const struct kahlnws_network *network,
                                    size_t arc,
                                    size_t *head);

/**
 * @brief Writes the cost of a unit of flow on arc, in units of cost, in the problem network holds.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
 *
 * @param[in] network the network.
 * @param[in] arc the arc, below kahlnws_network_arc_count(network).
 * @param[out] cost where its cost is written.
 *
 * @retval KAHLNWS_STATUS_OK its cost is written to cost;
 * @retval KAHLNWS_STATUS_NULL_POINTER network or cost is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT arc is not below the arc count, or a pointer is not aligned for its type;
 * nothing is written;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre cost is null or points to writable storage for one value, whatever it held before, that nothing else reads or
 * writes while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_cost(const struct kahlnws_network *network,
                                    size_t arc,
                                    int64_t *cost);

/**
 * @brief Writes the least flow of arc, in units of flow, in the problem network holds.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
 *
 * @param[in] network the network.
 * @param[in] arc the arc, below kahlnws_network_arc_count(network).
 * @param[out] lower_bound where its lower bound is written.
 *
 * @retval KAHLNWS_STATUS_OK its lower bound is written to lower_bound;
 * @retval KAHLNWS_STATUS_NULL_POINTER network or lower_bound is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT arc is not below the arc count, or a pointer is not aligned for its type;
 * nothing is written;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre lower_bound is null or points to writable storage for one value, whatever it held before, that nothing else
 * reads or writes while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_lower_bound(const struct kahlnws_network *network,
                                           size_t arc,
                                           uint64_t *lower_bound);

/**
 * @brief Writes the most flow of arc, in units of flow, in the problem network holds.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
 *
 * @param[in] network the network.
 * @param[in] arc the arc, below kahlnws_network_arc_count(network).
 * @param[out] capacity where its capacity is written.
 *
 * @retval KAHLNWS_STATUS_OK its capacity is written to capacity;
 * @retval KAHLNWS_STATUS_NULL_POINTER network or capacity is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT arc is not below the arc count, or a pointer is not aligned for its type;
 * nothing is written;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre capacity is null or points to writable storage for one value, whatever it held before, that nothing else reads
 * or writes while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_capacity(const struct kahlnws_network *network,
                                        size_t arc,
                                        uint64_t *capacity);

/**
 * @brief Writes the lower bound and the capacity of arc, in units of flow, in the problem network holds.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
 *
 * @param[in] network the network.
 * @param[in] arc the arc, below kahlnws_network_arc_count(network).
 * @param[out] lower_bound where its lower bound is written.
 * @param[out] capacity where its capacity is written.
 *
 * @retval KAHLNWS_STATUS_OK its lower bound is written to lower_bound and its capacity to capacity;
 * @retval KAHLNWS_STATUS_NULL_POINTER network, lower_bound or capacity is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT arc is not below the arc count, or a pointer is not aligned for its type;
 * nothing is written;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre lower_bound and capacity are each null or point to writable storage for one value, whatever it held before;
 * @pre neither overlaps the other, and nothing else reads or writes them while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_bounds(const struct kahlnws_network *network,
                                      size_t arc,
                                      uint64_t *lower_bound,
                                      uint64_t *capacity);

/**
 * @brief Writes the supply of node in the problem network holds, in units of flow: positive where it sends,
 * negative where it takes.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no node is below its count. It costs a few steps.
 *
 * @param[in] network the network.
 * @param[in] node the node, below kahlnws_network_node_count(network).
 * @param[out] supply where its supply is written.
 *
 * @retval KAHLNWS_STATUS_OK its supply is written to supply;
 * @retval KAHLNWS_STATUS_NULL_POINTER network or supply is null;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT node is not below the node count, or a pointer is not aligned for its type;
 * nothing is written;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre supply is null or points to writable storage for one value, whatever it held before, that nothing else reads
 * or writes while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_supply(const struct kahlnws_network *network,
                                      size_t node,
                                      int64_t *supply);

/**
 * @brief Writes the node each arc leaves in the problem network holds, arc a at a.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no arc is held, and nothing is written. It reads every entry
 * once, and allocates nothing.
 *
 * @param[in] network the network.
 * @param[out] tails where they are written: storage for kahlnws_network_arc_count(network) entries, which may be null
 * where that count is zero.
 *
 * @retval KAHLNWS_STATUS_OK they are written to tails;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null, or tails is null while the arc count is above zero;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer is not aligned for its type;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre tails points to writable storage for kahlnws_network_arc_count(network) entries within one object, whatever
 * it held before, and may be null where that count is zero; nothing else reads or writes it while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_tails(const struct kahlnws_network *network,
                                     size_t *tails);

/**
 * @brief Writes the node each arc enters in the problem network holds, arc a at a.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no arc is held, and nothing is written. It reads every entry
 * once, and allocates nothing.
 *
 * @param[in] network the network.
 * @param[out] heads where they are written: storage for kahlnws_network_arc_count(network) entries, which may be null
 * where that count is zero.
 *
 * @retval KAHLNWS_STATUS_OK they are written to heads;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null, or heads is null while the arc count is above zero;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer is not aligned for its type;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre heads points to writable storage for kahlnws_network_arc_count(network) entries within one object, whatever
 * it held before, and may be null where that count is zero; nothing else reads or writes it while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_heads(const struct kahlnws_network *network,
                                     size_t *heads);

/**
 * @brief Writes the cost of a unit of flow on each arc, in units of cost, in the problem network holds, arc a at
 * a.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no arc is held, and nothing is written. It reads every entry
 * once, and allocates nothing.
 *
 * @param[in] network the network.
 * @param[out] costs where they are written: storage for kahlnws_network_arc_count(network) entries, which may be null
 * where that count is zero.
 *
 * @retval KAHLNWS_STATUS_OK they are written to costs;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null, or costs is null while the arc count is above zero;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer is not aligned for its type;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre costs points to writable storage for kahlnws_network_arc_count(network) entries within one object, whatever
 * it held before, and may be null where that count is zero; nothing else reads or writes it while the call runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_costs(const struct kahlnws_network *network,
                                     int64_t *costs);

/**
 * @brief Writes the lower bound of each arc, in units of flow, in the problem network holds, arc a at a.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no arc is held, and nothing is written. It reads every entry
 * once, and allocates nothing.
 *
 * @param[in] network the network.
 * @param[out] lower_bounds where they are written: storage for kahlnws_network_arc_count(network) entries, which may
 * be null where that count is zero.
 *
 * @retval KAHLNWS_STATUS_OK they are written to lower_bounds;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null, or lower_bounds is null while the arc count is above zero;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer is not aligned for its type;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre lower_bounds points to writable storage for kahlnws_network_arc_count(network) entries within one object,
 * whatever it held before, and may be null where that count is zero; nothing else reads or writes it while the call
 * runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_lower_bounds(const struct kahlnws_network *network,
                                            uint64_t *lower_bounds);

/**
 * @brief Writes the capacity of each arc, in units of flow, in the problem network holds, arc a at a.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no arc is held, and nothing is written. It reads every entry
 * once, and allocates nothing.
 *
 * @param[in] network the network.
 * @param[out] capacities where they are written: storage for kahlnws_network_arc_count(network) entries, which may be
 * null where that count is zero.
 *
 * @retval KAHLNWS_STATUS_OK they are written to capacities;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null, or capacities is null while the arc count is above zero;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer is not aligned for its type;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre capacities points to writable storage for kahlnws_network_arc_count(network) entries within one object,
 * whatever it held before, and may be null where that count is zero; nothing else reads or writes it while the call
 * runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_capacities(const struct kahlnws_network *network,
                                          uint64_t *capacities);

/**
 * @brief Writes the supply of each node, in units of flow, in the problem network holds, node v at v.
 *
 * It reads the problem as it stands, after the load and every change since; it changes nothing, keeps an optimum the
 * network holds and needs none. On a network never loaded no node is held, and nothing is written. It reads every
 * entry once, and allocates nothing.
 *
 * @param[in] network the network.
 * @param[out] supplies where they are written: storage for kahlnws_network_node_count(network) entries, which may be
 * null where that count is zero.
 *
 * @retval KAHLNWS_STATUS_OK they are written to supplies;
 * @retval KAHLNWS_STATUS_NULL_POINTER network is null, or supplies is null while the node count is above zero;
 * @retval KAHLNWS_STATUS_INVALID_ARGUMENT a pointer is not aligned for its type;
 * @retval KAHLNWS_STATUS_INTERNAL_ERROR an internal error was caught, or the network is half changed.
 *
 * @pre network is null, or a pointer kahlnws_network_new or kahlnws_network_clone answered that has not been
 * freed since and that no other call uses meanwhile: one thread at a time uses a network;
 * @pre supplies points to writable storage for kahlnws_network_node_count(network) entries within one object,
 * whatever it held before, and may be null where that count is zero; nothing else reads or writes it while the call
 * runs.
 */
KAHLNWS_API
KAHLNWS_NODISCARD
kahlnws_status kahlnws_network_supplies(const struct kahlnws_network *network,
                                        int64_t *supplies);

/**
 * @brief Writes the message of the breach refusal names to buffer, as UTF-8 ending in a NUL.
 *
 * The message is the text kahlnws gives the breach, with its values. A buffer of at least the length answered plus
 * one receives it whole; a shorter one receives as much of it as ends on a character and leaves a byte for the NUL;
 * a capacity of zero, or a null buffer whatever capacity is, receives nothing, so a first call with neither tells
 * the length to make room for. Where refusal is null, not aligned for a refusal, or of a kind that names no breach,
 * the message is empty and the answer 0 (a breach's message is never empty); whenever the buffer has a byte, it ends
 * in a NUL. Where buffer is not null and capacity passes PTRDIFF_MAX, nothing is written and the answer is 0.
 * It allocates nothing, and may be called from any thread.
 *
 * @param[in] refusal the refusal whose message is written, or null.
 * @param[out] buffer where the message is written: storage for capacity bytes, or null.
 * @param[in] capacity the number of bytes buffer holds.
 *
 * @return The length of the whole message in bytes, the NUL not counted; 0 where there is no message.
 *
 * @pre refusal is null or points to an initialised kahlnws_refusal;
 * @pre buffer is null, or capacity is zero or passes PTRDIFF_MAX, or buffer points to writable storage for
 * capacity bytes within one object, whatever it held before;
 * @pre the buffer overlaps nothing of the refusal; nothing writes the refusal, and nothing else reads or writes the
 * buffer, while the call runs.
 */
KAHLNWS_API size_t kahlnws_refusal_message(const struct kahlnws_refusal *refusal, char *buffer, size_t capacity);

/**
 * @brief Writes value in decimal to buffer, with a minus sign where it is negative, as ASCII ending in a NUL.
 *
 * The text is the value exact, every digit of it, such as "-18446744073709551616": the form C has no printf
 * conversion for. A buffer of at least the length answered plus one receives it whole, and 41 bytes hold every value;
 * a shorter one receives as much of it as leaves a byte for the NUL; a capacity of zero, or a null buffer whatever
 * capacity is, receives nothing, so a first call with neither tells the length to make room for. Where buffer is
 * not null and capacity passes PTRDIFF_MAX, nothing is written and the answer is 0. It allocates nothing, and may
 * be called from any thread.
 *
 * @param[in] value the value written.
 * @param[out] buffer where the text is written: storage for capacity bytes, or null.
 * @param[in] capacity the number of bytes buffer holds.
 *
 * @return The length of the whole text in bytes, the NUL not counted, from 1 to 40; 0 only where buffer is not null
 * and capacity passes PTRDIFF_MAX, or for an internal error, which nothing in it can raise.
 *
 * @pre buffer is null, or capacity is zero or passes PTRDIFF_MAX, or buffer points to writable storage for
 * capacity bytes within one object, whatever it held before, that nothing else reads or writes while the call runs.
 */
KAHLNWS_API size_t kahlnws_int128_format(struct kahlnws_int128 value, char *buffer, size_t capacity);

/**
 * @brief Writes value in decimal to buffer, as ASCII ending in a NUL.
 *
 * It writes as kahlnws_int128_format does, for an unsigned value, such as "18446744073709551616": 40 bytes hold
 * every value.
 *
 * @param[in] value the value written.
 * @param[out] buffer where the text is written: storage for capacity bytes, or null.
 * @param[in] capacity the number of bytes buffer holds.
 *
 * @return The length of the whole text in bytes, the NUL not counted, from 1 to 39; 0 only where buffer is not null
 * and capacity passes PTRDIFF_MAX, or for an internal error, which nothing in it can raise.
 *
 * @pre buffer is null, or capacity is zero or passes PTRDIFF_MAX, or buffer points to writable storage for
 * capacity bytes within one object, whatever it held before, that nothing else reads or writes while the call runs.
 */
KAHLNWS_API size_t kahlnws_uint128_format(struct kahlnws_uint128 value, char *buffer, size_t capacity);

/**
 * @brief The name of status for a person to read.
 *
 * Safe to call from any thread.
 *
 * @param[in] status the status to name.
 *
 * @return A static NUL-terminated string the caller does not free, such as "optimal" or "null pointer"; "unknown
 * status" for a number no status has.
 */
KAHLNWS_API const char *kahlnws_status_name(kahlnws_status status);

/**
 * @brief The largest magnitude a cost may have in a problem of node_count nodes: every cost of a problem a solve
 * admits lies within plus or minus it.
 *
 * Safe to call from any thread.
 *
 * @param[in] node_count the number of nodes of the problem.
 *
 * @return (2^62 - 1) / node_count, rounded down, so node_count times it stays below 2^62; for a problem of no
 * node, 2^62 - 1. It is never negative: -1 would answer an internal error, which nothing in it can raise.
 */
KAHLNWS_API int64_t kahlnws_cost_bound(size_t node_count);

/**
 * @brief The version of the library linked.
 *
 * Compare it with KAHLNWS_VERSION, the same number made of the version of the header. Safe to call from any
 * thread.
 *
 * @return (major << 16) | (minor << 8) | patch, where the minor and the patch version are each below 256.
 */
KAHLNWS_API uint32_t kahlnws_version(void);

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* KAHLNWS_H_0F9A793A_BC8A_421B_8E0E_B5B356E26F84 */
