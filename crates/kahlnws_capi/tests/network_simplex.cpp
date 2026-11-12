// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// The C++ interface's solver, through the static library, in the mode the build chose: the depot through both levels;
// an infeasible problem, whose buffers stay as they were; every breach a test can reach, with its values and the
// library's message; buffers of the wrong length or sharing storage; moves and a moved-from solver; and, where
// exceptions are enabled, the throwing forms. What the header refuses at compile time is shown by static assertions.

#include "kahlnws.hpp"

#include <array>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <limits>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

#include "harness.hpp"

namespace {

/// Whether `Solver` has a member `solve` taking a problem: the throwing form, present only with exceptions.
template <class Solver, class = void>
struct has_solve : std::false_type {};

template <class Solver>
struct has_solve<Solver, decltype(static_cast<void>(std::declval<Solver&>().solve(kahlnws::problem())))>
    : std::true_type {};

/// Whether a `View` can be made from a braced list of three integers.
template <class View, class = void>
struct takes_braced_list : std::false_type {};

template <class View>
struct takes_braced_list<View, decltype(static_cast<void>(View({1, 2, 3})))> : std::true_type {};

static_assert(has_solve<kahlnws::network_simplex>::value == (KAHLNWS_EXCEPTIONS == 1),
              "the throwing forms exist exactly where exceptions are enabled");
static_assert(std::is_nothrow_default_constructible<kahlnws::network_simplex>::value, "a new solver allocates nothing");
static_assert(!std::is_copy_constructible<kahlnws::network_simplex>::value, "a solver is not copied");
static_assert(std::is_nothrow_move_constructible<kahlnws::network_simplex>::value, "a solver moves without throwing");
static_assert(std::is_nothrow_move_assignable<kahlnws::network_simplex>::value, "a solver moves without throwing");
static_assert(std::is_constructible<kahlnws::span<const std::size_t>, std::vector<std::size_t>&>::value,
              "a vector is viewed");
static_assert(!std::is_constructible<kahlnws::span<const std::size_t>, std::vector<std::size_t>&&>::value,
              "a temporary vector is not viewed");
static_assert(!std::is_constructible<kahlnws::span<std::uint64_t>, const std::vector<std::uint64_t>&>::value,
              "a const vector is not written to");
static_assert(!std::is_constructible<kahlnws::span<const std::int64_t>, std::vector<std::int32_t>&>::value,
              "a table of another type is not viewed");
static_assert(std::is_constructible<kahlnws::span<const int>, int (&)[3]>::value, "an array is viewed");
static_assert(!std::is_constructible<kahlnws::span<const int>, int (&&)[3]>::value, "a temporary array is not");
static_assert(!takes_braced_list<kahlnws::span<const int> >::value, "a braced list is not viewed");
static_assert(std::is_convertible<kahlnws::span<int>, kahlnws::span<const int> >::value, "const is added");
static_assert(!std::is_convertible<kahlnws::span<const int>, kahlnws::span<int> >::value, "const is not dropped");
static_assert(!std::is_constructible<kahlnws::span<const int>, int, std::size_t>::value, "a literal 0 is no pointer");
static_assert(std::is_default_constructible<kahlnws::answer<kahlnws::solution> >::value, "a default answer");

/// The tables of a problem, owned by the test: the depot unless changed.
struct tables {
    std::vector<std::size_t> tails = {0, 0, 1};
    std::vector<std::size_t> heads = {2, 1, 2};
    std::vector<std::uint64_t> lower_bounds;
    std::vector<std::uint64_t> capacities = {3, 4, 2};
    std::vector<std::int64_t> costs = {5, 1, 1};
    std::vector<std::int64_t> supplies = {4, 0, -4};

    /// The problem of the tables.
    kahlnws::problem problem() const {
        kahlnws::problem viewed;
        viewed.tails = tails;
        viewed.heads = heads;
        viewed.lower_bounds = lower_bounds;
        viewed.capacities = capacities;
        viewed.costs = costs;
        viewed.supplies = supplies;
        return viewed;
    }
};

/// The error a high-level solve of `problem` answers; a check fails where it answers none.
kahlnws::error refused(checks& checks, const tables& problem) {
    kahlnws::network_simplex solver;
    const kahlnws::result<kahlnws::answer<kahlnws::solution> > solved = solver.try_solve(problem.problem());
    checks.check(!solved.has_value(), "the problem should be refused");
    return solved ? kahlnws::error(kahlnws::breach::cost_total_beyond_range{0}) : solved.error();
}

void test_the_depot_through_both_levels(checks& checks) {
    const tables depot;
    kahlnws::network_simplex solver;
    const kahlnws::result<kahlnws::answer<kahlnws::solution> > solved = solver.try_solve(depot.problem());
    checks.check(solved.has_value(), "the depot should be solved");
    const kahlnws::solution* solution = solved ? solved->outcome.optimal() : nullptr;
    checks.check(solution != nullptr && solved->outcome.is_optimal(), "the depot should be optimal");
    if (solution != nullptr) {
        checks.check(solution->flows == std::vector<std::uint64_t>({2, 2, 2}), "the flows should be 2, 2, 2");
        checks.check(solution->potentials == std::vector<std::int64_t>({0, 1, 5}), "the potentials should be 0, 1, 5");
        checks.check(solution->total_cost == kahlnws::int128(14), "the cost should be 14");
        const kahlnws::work expected = {4, 1, 0};
        checks.check(solved->work == expected, "the depot should take four pivots, one degenerate");
    }

    checks.check(solver.try_reserve(3, 3).has_value(), "the room should be reserved");
    std::vector<std::uint64_t> flows(3, 99);
    std::int64_t potentials[3] = {7, 7, 7};
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > written =
        solver.try_solve_into(depot.problem(), flows, potentials);
    checks.check(written && written->outcome.optimal() != nullptr &&
                     written->outcome.optimal()->total_cost == kahlnws::int128(14),
                 "the low level should answer the cost 14");
    checks.check(flows == std::vector<std::uint64_t>({2, 2, 2}), "the flows should be written");
    checks.check(potentials[0] == 0 && potentials[1] == 1 && potentials[2] == 5, "the potentials should be written");
    checks.check(written && solved && written->work == solved->work, "both levels should count the same work");
}

void test_an_infeasible_problem_leaves_the_buffers(checks& checks) {
    tables narrow;
    narrow.capacities = {1, 1, 1};
    kahlnws::network_simplex solver;
    const kahlnws::result<kahlnws::answer<kahlnws::solution> > solved = solver.try_solve(narrow.problem());
    checks.check(solved && solved->outcome.is_infeasible() && solved->outcome.optimal() == nullptr,
                 "the narrow depot should be infeasible");

    std::vector<std::uint64_t> flows = {7, 8, 9};
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > written = solver.try_solve_into(narrow.problem(), flows);
    checks.check(written && written->outcome.is_infeasible(), "the low level should be infeasible too");
    checks.check(flows == std::vector<std::uint64_t>({7, 8, 9}), "an infeasible solve should leave the flows");
    checks.check(kahlnws::answer<kahlnws::optimum>().outcome.is_infeasible(), "a default answer is infeasible");
}

void test_every_reachable_breach_is_answered_with_its_values(checks& checks) {
    tables chain;
    chain.tails = {0, 1, 2};
    chain.heads = {1, 2, 3};
    chain.capacities = {5, 5, 5};
    chain.costs = {1, 1, 1};
    chain.supplies = {2, 0, 0, -2};

    tables problem = chain;
    problem.heads = {1, 2};
    kahlnws::error error = refused(checks, problem);
    const kahlnws::breach::length* length = error.refusal() ? error.refusal()->length() : nullptr;
    checks.check(length != nullptr && length->table == kahlnws::table::heads && length->length == 2 &&
                     length->expected == 3,
                 "heads of another length should be refused with their values");
    checks.check(std::strcmp(error.message(), "the heads have length 2 where the tails have length 3") == 0,
                 "a length refusal should have its message");

    problem = chain;
    problem.tails = {0, 1, 4};
    error = refused(checks, problem);
    const kahlnws::breach::tail_outside_nodes* tail = error.refusal() ? error.refusal()->tail_outside_nodes() : nullptr;
    checks.check(tail != nullptr && tail->arc == 2 && tail->tail == 4 && tail->node_count == 4, "a tail outside");
    checks.check(std::strcmp(error.message(), "arc 2 leaves node 4, not below the node count 4") == 0, "its message");
    checks.check(error.refusal() != nullptr && error.refusal()->unbalanced() == nullptr, "other kinds answer null");

    problem = chain;
    problem.heads = {9, 2, 3};
    error = refused(checks, problem);
    checks.check(error.refusal() != nullptr && error.refusal()->head_outside_nodes() != nullptr, "a head outside");

    problem = chain;
    problem.lower_bounds = {0, 6, 0};
    error = refused(checks, problem);
    const kahlnws::breach::lower_above_capacity* lower =
        error.refusal() ? error.refusal()->lower_above_capacity() : nullptr;
    checks.check(lower != nullptr && lower->arc == 1 && lower->lower_bound == 6 && lower->capacity == 5,
                 "a lower bound above its capacity");

    const kahlnws::result<std::int64_t> bound = kahlnws::try_cost_bound(4);
    checks.check(bound && *bound == ((std::numeric_limits<std::int64_t>::max)() / 2) / 4, "the bound of four nodes");
    problem = chain;
    problem.costs = {0, -*bound - 1, 0};
    error = refused(checks, problem);
    const kahlnws::breach::cost_beyond_bound* cost = error.refusal() ? error.refusal()->cost_beyond_bound() : nullptr;
    checks.check(cost != nullptr && cost->arc == 1 && cost->cost == -*bound - 1, "a cost beyond its bound");

    problem = tables();
    problem.tails = {0, 0, 0};
    problem.heads = {0, 0, 0};
    problem.capacities.assign(3, (std::numeric_limits<std::uint64_t>::max)());
    problem.costs.assign(3, *kahlnws::try_cost_bound(1));
    problem.supplies = {0};
    error = refused(checks, problem);
    const kahlnws::breach::cost_total_beyond_range* total =
        error.refusal() ? error.refusal()->cost_total_beyond_range() : nullptr;
    checks.check(total != nullptr && total->arc == 2, "a cost total beyond its range");

    problem = chain;
    problem.supplies = {2, 0, 0, -1};
    error = refused(checks, problem);
    const kahlnws::breach::unbalanced* unbalanced = error.refusal() ? error.refusal()->unbalanced() : nullptr;
    checks.check(unbalanced != nullptr && unbalanced->total == kahlnws::int128(1), "supplies off balance");
    checks.check(std::strcmp(error.message(), "the supplies add up to 1, not zero") == 0, "their message");

    problem = tables();
    problem.tails = {1};
    problem.heads = {0};
    problem.lower_bounds = {std::uint64_t(1) << 63};
    problem.capacities = {std::uint64_t(1) << 63};
    problem.costs = {0};
    problem.supplies = {(std::numeric_limits<std::int64_t>::max)(), (std::numeric_limits<std::int64_t>::min)() + 1};
    error = refused(checks, problem);
    const kahlnws::breach::net_supply_beyond_range* net =
        error.refusal() ? error.refusal()->net_supply_beyond_range() : nullptr;
    checks.check(net != nullptr && net->total == kahlnws::uint128((std::numeric_limits<std::uint64_t>::max)()),
                 "net supplies beyond their range");
    checks.check(error.kind() == kahlnws::error_kind::refused_problem, "a refusal is a refused problem");
}

void test_buffers_of_the_wrong_length_or_storage_are_refused(checks& checks) {
    tables depot;
    const kahlnws::problem problem = depot.problem();
    kahlnws::network_simplex solver;

    std::vector<std::uint64_t> short_flows = {5, 5};
    kahlnws::result<kahlnws::answer<kahlnws::optimum> > written = solver.try_solve_into(problem, short_flows);
    checks.check(!written && written.error().kind() == kahlnws::error_kind::invalid_argument &&
                     written.error().refusal() == nullptr,
                 "flows of the wrong length should be an invalid argument");
    checks.check(written ? false
                         : std::strcmp(written.error().message(), "called `network_simplex::try_solve_into` with 2 "
                                                                  "flows for a problem of 3 arcs") == 0,
                 "flows of the wrong length should have their message");

    std::vector<std::uint64_t> flows = {5, 5, 5};
    std::vector<std::int64_t> potentials = {6, 6};
    written = solver.try_solve_into(problem, flows, potentials);
    checks.check(!written && written.error().kind() == kahlnws::error_kind::invalid_argument,
                 "potentials of the wrong length should be an invalid argument");

    written = solver.try_solve_into(problem, depot.capacities);
    checks.check(!written && written.error().kind() == kahlnws::error_kind::invalid_argument,
                 "flows in the capacities' storage should be refused");
    written = solver.try_solve_into(problem, flows, depot.supplies);
    checks.check(!written && written.error().kind() == kahlnws::error_kind::invalid_argument,
                 "potentials in the supplies' storage should be refused");

    // Flows and potentials side by side in one block of storage touch but share no byte.
    std::vector<std::uint64_t> block(6, 0);
    std::int64_t* const after = reinterpret_cast<std::int64_t*>(block.data() + 3);
    written = solver.try_solve_into(problem, kahlnws::span<std::uint64_t>(block.data(), 3),
                                    kahlnws::span<std::int64_t>(after, 3));
    checks.check(written && written->outcome.is_optimal(), "adjacent buffers should be admitted");
    written = solver.try_solve_into(problem, kahlnws::span<std::uint64_t>(block.data(), 3),
                                    kahlnws::span<std::int64_t>(after - 1, 3));
    checks.check(!written, "buffers sharing one entry should be refused");
    checks.check(depot.capacities == std::vector<std::uint64_t>({3, 4, 2}), "the capacities should be left");
}

void test_moves_and_a_moved_from_solver(checks& checks) {
    const tables depot;
    const kahlnws::problem problem = depot.problem();
    kahlnws::network_simplex fresh;
    const kahlnws::answer<kahlnws::solution> expected = *fresh.try_solve(problem);

    kahlnws::network_simplex first;
    checks.check(first.try_reserve(3, 3).has_value(), "the room should be reserved");
    kahlnws::network_simplex second(std::move(first));
    checks.check(*second.try_solve(problem) == expected, "a solver moved into should solve as a fresh one");
    checks.check(*first.try_solve(problem) == expected, "a moved-from solver should solve as a fresh one");
    // A reference hides the self-move from the compiler's warning, which is what this checks.
    kahlnws::network_simplex& same = second;
    second = std::move(same);
    checks.check(*second.try_solve(problem) == expected, "a solver moved onto itself should be as it was");

    std::vector<kahlnws::network_simplex> solvers;
    for (std::size_t count = 0; count < 9; ++count) {
        solvers.push_back(kahlnws::network_simplex());
        checks.check(*solvers.back().try_solve(problem) == expected, "a solver in a growing vector should solve");
    }
    for (kahlnws::network_simplex& moved : solvers) {
        checks.check(*moved.try_solve(problem) == expected, "a solver moved by the vector should solve");
    }
}

void test_the_throwing_forms(checks& checks) {
#if KAHLNWS_EXCEPTIONS
    tables problem;
    kahlnws::network_simplex solver;
    solver.reserve(3, 3);
    checks.check(solver.solve(problem.problem()).outcome.is_optimal(), "solve should answer the depot");
    std::vector<std::uint64_t> flows(3);
    checks.check(solver.solve_into(problem.problem(), flows).outcome.is_optimal(), "solve_into should answer it");

    problem.supplies = {4, 0, -3};
    bool thrown = false;
    try {
        static_cast<void>(solver.solve(problem.problem()));
    } catch (const kahlnws::problem_error& error) {
        thrown = error.refusal().unbalanced() != nullptr &&
                 std::string(error.what()) == "the supplies add up to 1, not zero";
    }
    checks.check(thrown, "a refused problem should throw kahlnws::problem_error");

    thrown = false;
    try {
        std::vector<std::uint64_t> short_flows(2);
        static_cast<void>(solver.solve_into(tables().problem(), short_flows));
    } catch (const std::invalid_argument& error) {
        thrown = dynamic_cast<const kahlnws::problem_error*>(&error) == nullptr;
    }
    checks.check(thrown, "flows of the wrong length should throw std::invalid_argument");

    thrown = false;
    try {
        solver.reserve((std::numeric_limits<std::size_t>::max)(), 1);
    } catch (const std::bad_alloc&) {
        thrown = true;
    }
    checks.check(thrown, "a reservation past memory should throw std::bad_alloc");
    checks.check(kahlnws::cost_bound(1) == (std::numeric_limits<std::int64_t>::max)() / 2, "the throwing cost bound");
#else
    checks.check(true, "no throwing forms without exceptions");
#endif
}

void test_refused_memory_is_answered(checks& checks) {
    kahlnws::network_simplex solver;
    const kahlnws::result<void> reserved = solver.try_reserve((std::numeric_limits<std::size_t>::max)(), 1);
    checks.check(!reserved && reserved.error().kind() == kahlnws::error_kind::refused_memory,
                 "a reservation past memory should be refused memory");
    checks.check(solver.try_solve(tables().problem()).has_value(), "the solver should stay usable");
    checks.check(kahlnws::header_version() == KAHLNWS_VERSION && kahlnws::is_library_compatible(),
                 "the library should serve the header");
}

}  // namespace

int main() {
    checks checks("network_simplex");

    test_the_depot_through_both_levels(checks);
    test_an_infeasible_problem_leaves_the_buffers(checks);
    test_every_reachable_breach_is_answered_with_its_values(checks);
    test_buffers_of_the_wrong_length_or_storage_are_refused(checks);
    test_moves_and_a_moved_from_solver(checks);
    test_the_throwing_forms(checks);
    test_refused_memory_is_answered(checks);

    return checks.exit_code();
}
