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
 * Use. A program makes a solver with kahlnws_solver_new and, where it solves in a loop, reserves its room once
 * with kahlnws_solver_reserve. It fills in a kahlnws_problem with the two counts and a pointer to each of its
 * tables, and calls kahlnws_solver_solve with storage of its own for the flows and, where it wants them, the
 * potentials, the cost, the work and the refusal. It reads the kahlnws_status the call answers, which
 * kahlnws_status_name names; where the solve ran, optimal or infeasible, it may read the pivots it made from the
 * kahlnws_work; on a refusal, it reads the breach from the kahlnws_refusal, or its text from
 * kahlnws_refusal_message. It frees the solver with kahlnws_solver_free.
 *
 * Solving again. A solver keeps the basis its last solve ended on, optimal or infeasible. A program that solves a
 * problem again after some of its numbers changed calls kahlnws_solver_resolve, with the same arguments as
 * kahlnws_solver_solve: it starts from that basis where it serves, where the problem has the node and arc counts of
 * that solve and every arc its tail and its head and the supplies the basis leaves stay within range. Where only some
 * numbers changed it then often makes far fewer pivots, and the same problem again, where the kept basis serves, makes
 * none. Where the basis cannot serve, it answers exactly as kahlnws_solver_solve does. Both entries answer the same
 * status, the same cost, and potentials that prove the flow optimal; only kahlnws_solver_solve answers the flows
 * among tied optima, the potentials and the work as a function of the problem alone, while kahlnws_solver_resolve
 * answers a function of the problem and of the solves before it.
 *
 * Ownership. The library owns only the solvers it makes, from kahlnws_solver_new to kahlnws_solver_free. Every
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
 * Answers. Every entry answers, and none aborts or unwinds into the caller. Memory refused for a solver's room is
 * answered as KAHLNWS_STATUS_NO_ROOM, and an internal error of the library, a defect of it, as
 * KAHLNWS_STATUS_INTERNAL_ERROR.
 *
 * Threads. A solver is used by one thread at a time: no two calls that take the same solver run at once. It may
 * move from one thread to another between calls, and distinct solvers may be used on distinct threads at once, also
 * to solve the same problem tables. The entries that take no solver may be called from any thread at any time.
 *
 * Numbers. Node and arc counts and node numbers are size_t; flows, lower bounds, capacities and the counts of a
 * kahlnws_work uint64_t; costs, supplies and potentials int64_t. A 128-bit integer, such as the cost of a
 * flow, comes as its two 64-bit halves, kahlnws_int128 or kahlnws_uint128: the value is high * 2^64 + low,
 * with low the value modulo 2^64.
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
 * @brief Marks a function whose answer, a status or a solver, must not be ignored.
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
 * @brief A solver: a primal network simplex with the room it works in, kept from one solve to the next.
 *
 * Opaque: C holds it only by the pointer kahlnws_solver_new answers, passes that pointer to the other entries, and
 * gives it back to kahlnws_solver_free. The room only grows, by kahlnws_solver_reserve or by a solve of a
 * problem larger than the room held; a solve of a problem within the room allocates nothing.
 *
 * It also keeps the basis its last solve ended on, by either entry, optimal or infeasible, which only
 * kahlnws_solver_resolve reads: what kahlnws_solver_solve finds depends on its problem alone.
 *
 * One thread at a time uses a solver: no two calls that take the same solver may run at once. A solver may move
 * from one thread to another between calls, and distinct solvers may be used on distinct threads at once.
 */
typedef struct kahlnws_solver kahlnws_solver;

/**
 * @brief How a call went, by a number fixed for good: one of the KAHLNWS_STATUS_ constants.
 *
 * kahlnws_solver_reserve, kahlnws_solver_solve and kahlnws_solver_resolve answer one; each says which it can
 * answer.
 *
 * The numbers are part of the binary interface: none is ever changed or given to another status. Plain data: a
 * caller may hold any number, and kahlnws_status_name names every one.
 */
typedef uint32_t kahlnws_status;
enum {
  /**
   * @brief A solve found a flow of least cost and wrote it.
   */
  KAHLNWS_STATUS_OPTIMAL = 0,
  /**
   * @brief A solve found that no flow within the bounds meets the supplies, and wrote nothing but the work of the
   * solve.
   */
  KAHLNWS_STATUS_INFEASIBLE = 1,
  /**
   * @brief kahlnws_solver_reserve made the room it was asked for.
   */
  KAHLNWS_STATUS_RESERVED = 2,
  /**
   * @brief A solve refused the problem, which lies outside what a solve admits, and wrote nothing but the refusal that
   * names the breach.
   */
  KAHLNWS_STATUS_REFUSED = 3,
  /**
   * @brief A pointer the call needs is null, and nothing was written.
   */
  KAHLNWS_STATUS_NULL_POINTER = 4,
  /**
   * @brief An argument no call can use: a pointer not aligned for its type, or a count of entries that no object can
   * hold, its bytes past PTRDIFF_MAX. Nothing was written.
   */
  KAHLNWS_STATUS_INVALID_ARGUMENT = 5,
  /**
   * @brief The memory for the solver's room was refused, and nothing was written.
   */
  KAHLNWS_STATUS_NO_ROOM = 6,
  /**
   * @brief An internal error of the library was caught, a defect of it, and answered here. What the call's outputs hold
   * is then unspecified; the solver may be used again or freed.
   */
  KAHLNWS_STATUS_INTERNAL_ERROR = 7,
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
 * @brief A signed 128-bit integer as its two 64-bit halves: the value is high * 2^64 + low. The cost of a flow, and
 * the sum of supplies that do not balance.
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
 * kahlnws_solver_solve and kahlnws_solver_resolve write it, where asked, on KAHLNWS_STATUS_OPTIMAL and on
 * KAHLNWS_STATUS_INFEASIBLE.
 *
 * A pivot is an arc brought into the basis, a spanning tree, or flipped to its other bound. Either way the arc
 * violated its bound, and the most flow the cycle it closes with the tree carries is sent around that cycle: where a
 * link of the tree has the least room, that link leaves and the arc takes its place; where the arc's own capacity
 * less its lower bound is the least, no link leaves and the arc moves to its other bound, a flip. A flip sends that
 * whole difference, which is above zero, since an arc whose capacity equals its lower bound never enters.
 *
 * Plain data. The counts kahlnws_solver_solve writes depend on the problem alone, its arc order included, and on
 * the library's version: the same problem gives the same work from a fresh solver and from one that solved others
 * before. Those kahlnws_solver_resolve writes depend on the problem and on the solves the solver made before it,
 * and are those of kahlnws_solver_solve where it does not start from the kept basis. In every kahlnws_work a
 * solve writes, degenerate_pivots <= pivots and first_arc_pivots <= pivots, and the pivots that sent flow are
 * pivots - degenerate_pivots. A solve that answers ran pivots - first_arc_pivots + 1 block searches.
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
   * violated, and in a resolve from the kept basis, which tries none; pivots where those arcs alone reached the
   * outcome, optimal or infeasible.
   */
  uint64_t first_arc_pivots;
} kahlnws_work;

/**
 * @brief Which breach a refusal names, by a number fixed for good: one of the KAHLNWS_REFUSAL_KIND_ constants.
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
   * @brief An arc's lower bound passes its capacity. Sets arc, lower_bound and capacity.
   */
  KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY = 4,
  /**
   * @brief An arc's cost lies beyond plus or minus kahlnws_cost_bound(node_count). Sets arc, cost and
   * node_count.
   */
  KAHLNWS_REFUSAL_KIND_COST_BEYOND_BOUND = 5,
  /**
   * @brief The magnitudes of the costs times the capacities, added up in arc order, pass 2^127 - 1. Sets arc, the
   * arc at which the sum passes.
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
};

/**
 * @brief An unsigned 128-bit integer as its two 64-bit halves: the value is high * 2^64 + low. The sum of the
 * positive supplies net of the lower bounds that passes its limit.
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
 * @brief Why a solve refused a problem: the breach, by its kind, and the values that make it one.
 *
 * A problem with several breaches is refused with the first in the order of the kinds' numbers, where the kinds
 * about one arc, from KAHLNWS_REFUSAL_KIND_TAIL_OUTSIDE_NODES to KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE,
 * are taken arc by arc: the lowest arc with any of them, and its first.
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
   * @brief The node outside the problem's nodes, the arc's tail or its head; zero where the kind sets no node.
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
   * @brief The sum of the supplies, in units of flow; zero where the kind does not set it.
   */
  struct kahlnws_int128 supply_total;
  /**
   * @brief The sum of the positive supplies net of the lower bounds, in units of flow; zero where the kind does not set
   * it.
   */
  struct kahlnws_uint128 net_supply_total;
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
 * @retval KAHLNWS_STATUS_RESERVED the room is made;
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
 * The solver keeps the basis an optimal or infeasible solve ended on, for a later kahlnws_solver_resolve; a call
 * that answers another status keeps the basis kept before, but an internal error, which may leave none.
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
 * @brief Solves problem with solver from the basis the solver's last solve ended on, where that basis serves:
 * answers as kahlnws_solver_solve does, the same status and the same cost, with potentials that prove the flow
 * optimal.
 *
 * The basis serves where a solve of solver, by either entry, optimal or infeasible, has ended since the solver was
 * made and since any call of it that answered KAHLNWS_STATUS_INTERNAL_ERROR, which may leave none; where problem
 * has that solve's node and arc counts and every arc the tail and the head it had; and where the flows that basis
 * puts on its arcs out of the tree keep the supplies net of them within the limit
 * KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE names. Its tree is kept and every number is read anew from
 * problem. Where the basis cannot serve, the call answers and writes exactly what kahlnws_solver_solve does, its
 * work included. So any problem is solved, after any call.
 *
 * Where the basis serves and only some numbers changed, it often makes far fewer pivots than a solve, and the same
 * problem again makes none. Of a problem with several optimal flows it may write another than kahlnws_solver_solve
 * writes, and other potentials: its flows, potentials and work are a function of the problem and of the solves before
 * it, deterministic still. A resolve from the kept basis tries no arcs before block search, so the first_arc_pivots
 * it writes are 0.
 *
 * On every status but KAHLNWS_STATUS_OPTIMAL the flows, the potentials and the cost are as they were; the work is
 * written on KAHLNWS_STATUS_OPTIMAL and KAHLNWS_STATUS_INFEASIBLE alone, and the refusal on
 * KAHLNWS_STATUS_REFUSED alone; an internal error leaves the outputs unspecified.
 *
 * Room. As in kahlnws_solver_solve, the solver's room is grown to the problem's counts and kept before the problem
 * is checked; where the room already holds the problem, by kahlnws_solver_reserve or an earlier solve, the call
 * allocates nothing.
 *
 * The solver keeps the basis an optimal or infeasible resolve ended on, as a solve does; a call that answers another
 * status keeps the basis kept before, but an internal error, which may leave none.
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
kahlnws_status kahlnws_solver_resolve(struct kahlnws_solver *solver,
                                      const struct kahlnws_problem *problem,
                                      uint64_t *flows,
                                      int64_t *potentials,
                                      struct kahlnws_int128 *total_cost,
                                      struct kahlnws_work *work,
                                      struct kahlnws_refusal *refusal);

/**
 * @brief Writes the message of the breach refusal names to buffer, as UTF-8 ending in a NUL.
 *
 * The message is the text kahlnws gives the breach, with its values. A buffer of at least the length answered plus
 * one receives it whole; a shorter one receives as much of it as ends on a character and leaves a byte for the NUL;
 * a capacity of zero, or a null buffer, receives nothing, so a first call with neither tells the length to make
 * room for. Where refusal is null, not aligned for a refusal, or of a kind that names no breach, the message is
 * empty and the answer 0 (a breach's message is never empty); whenever the buffer has a byte, it ends in a NUL.
 * Where capacity passes PTRDIFF_MAX, nothing is written and the answer is 0. It allocates nothing, and may be
 * called from any thread.
 *
 * @param[in] refusal the refusal whose message is written, or null.
 * @param[out] buffer where the message is written: storage for capacity bytes, or null.
 * @param[in] capacity the number of bytes buffer holds.
 *
 * @return The length of the whole message in bytes, the NUL not counted; 0 where there is no message.
 *
 * @pre refusal is null or points to an initialised kahlnws_refusal;
 * @pre buffer is null, or capacity is zero, or buffer points to writable storage for capacity bytes within one
 * object, whatever it held before;
 * @pre the buffer overlaps nothing of the refusal; nothing writes the refusal, and nothing else reads or writes the
 * buffer, while the call runs.
 */
KAHLNWS_API size_t kahlnws_refusal_message(const struct kahlnws_refusal *refusal, char *buffer, size_t capacity);

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
