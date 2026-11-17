// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// The C++ interface's network, through the static library, in the mode the build chose: a network never loaded and its
// optimum of no node; the depot loaded, solved with its work and read whole and in part, and solved again with no
// pivot; each change, and a solve after it, against a fresh solve of the changed problem; each change set back; each
// refusal of a change and of a load with its values and the library's message, nothing changed; an arc or a node not
// below its count; a supply set by hand, set back, refused, and given to a node that balances itself; buffers of the
// wrong length; every read where the network holds no optimum; a load from the basis held, and from the star for other
// counts or other ends; copies, moves and a moved-from network; and, where exceptions are enabled, the throwing forms
// and the copy constructor. What the header refuses at compile time is shown by static assertions.

#include "kahlnws.hpp"

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

/// Whether `Network` has a member `load` taking a problem: the throwing form, present only with exceptions.
template <class Network, class = void>
struct has_load : std::false_type {};

template <class Network>
struct has_load<Network, decltype(static_cast<void>(std::declval<Network&>().load(kahlnws::problem())))>
    : std::true_type {};

static_assert(has_load<kahlnws::network>::value == (KAHLNWS_EXCEPTIONS == 1),
              "the throwing forms exist exactly where exceptions are enabled");
static_assert(std::is_copy_constructible<kahlnws::network>::value == (KAHLNWS_EXCEPTIONS == 1),
              "a network is copied by its constructor exactly where exceptions are enabled");
static_assert(std::is_copy_assignable<kahlnws::network>::value == (KAHLNWS_EXCEPTIONS == 1),
              "a network is copied by assignment exactly where exceptions are enabled");
static_assert(std::is_nothrow_default_constructible<kahlnws::network>::value, "a new network allocates nothing");
static_assert(std::is_nothrow_move_constructible<kahlnws::network>::value, "a network moves without throwing");
static_assert(std::is_nothrow_move_assignable<kahlnws::network>::value, "a network moves without throwing");

/// The tables of a problem, owned by the test: the depot unless changed. A depot (node 0) sends 4 units to a customer
/// (node 2), directly by arc 0 (capacity 3, cost 5) or through a hub (node 1) by arcs 1 and 2 (capacity 4 and 2, cost
/// 1 each): two units each way, at 14.
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

/// A network that holds `problem`, solved; a check fails where it does not load or solve to an optimum.
kahlnws::network solved(checks& checks, const tables& problem) {
    kahlnws::network network;
    checks.check(network.try_load(problem.problem()).has_value(), "the problem should be loaded");
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > answered = network.try_solve();
    checks.check(answered && answered->outcome.is_optimal(), "the problem should be solved to an optimum");
    return network;
}

/// Whether `potentials` prove `flows` optimal for `problem`: on every arc, `cost + potential[tail] - potential[head]`
/// at least zero where the flow is below the capacity, and at most zero where it is above the lower bound.
bool certifies(const tables& problem,
               const std::vector<std::uint64_t>& flows,
               const std::vector<std::int64_t>& potentials) {
    for (std::size_t arc = 0; arc < problem.tails.size(); ++arc) {
        const std::int64_t reduced =
            problem.costs[arc] + potentials[problem.tails[arc]] - potentials[problem.heads[arc]];
        const std::uint64_t lower_bound = problem.lower_bounds.empty() ? 0 : problem.lower_bounds[arc];
        if ((flows[arc] != problem.capacities[arc] && reduced < 0) || (flows[arc] != lower_bound && reduced > 0)) {
            return false;
        }
    }
    return true;
}

/// Whether `network`, solved now, answers what a fresh solver answers for `problem`, the problem it holds: the same
/// cost and flows, potentials that prove them optimal, and no arc tried first. Every problem it is given has one
/// optimal flow.
bool answers_as_a_fresh_solve(kahlnws::network& network, const tables& problem) {
    kahlnws::network_simplex fresh;
    const kahlnws::result<kahlnws::answer<kahlnws::solution> > expected = fresh.try_solve(problem.problem());
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > answered = network.try_solve();
    const kahlnws::result<std::vector<std::uint64_t> > flows = network.try_flows();
    const kahlnws::result<std::vector<std::int64_t> > potentials = network.try_potentials();
    if (!expected || !answered || !flows || !potentials) {
        return false;
    }
    const kahlnws::solution* const solution = expected->outcome.optimal();
    const kahlnws::optimum* const optimum = answered->outcome.optimal();
    return solution != nullptr && optimum != nullptr && optimum->total_cost == solution->total_cost &&
           *flows == solution->flows && certifies(problem, *flows, *potentials) &&
           answered->work.first_arc_pivots == 0;
}

/// Whether `error` is a refused change of `kind` whose message is `message`, and names no refused problem.
bool refused_change(const kahlnws::error& error, kahlnws::change_refusal_kind kind, const char* message) {
    return error.kind() == kahlnws::error_kind::refused_change && error.refusal() == nullptr &&
           error.change_refusal() != nullptr && error.change_refusal()->kind() == kind &&
           std::strcmp(error.message(), message) == 0;
}

/// Whether `network` holds the optimum of the depot, 14, and a solve of it makes no pivot: nothing changed.
bool holds_the_depot_unchanged(kahlnws::network& network) {
    const kahlnws::result<kahlnws::int128> cost = network.try_total_cost();
    const kahlnws::work none = {0, 0, 0};
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > again = network.try_solve();
    return cost && *cost == kahlnws::int128(14) && again && again->work == none;
}

void test_a_new_network_holds_the_problem_of_no_node(checks& checks) {
    kahlnws::network network;
    checks.check(network.node_count() == 0 && network.arc_count() == 0, "a new network should have no node and no arc");
    const kahlnws::result<kahlnws::int128> cost = network.try_total_cost();
    checks.check(cost && *cost == kahlnws::int128(0), "its optimum should cost zero");
    const kahlnws::result<std::vector<std::uint64_t> > flows = network.try_flows();
    const kahlnws::result<std::vector<std::int64_t> > potentials = network.try_potentials();
    checks.check(flows && flows->empty() && potentials && potentials->empty(), "it should have no flow or potential");

    const kahlnws::result<std::uint64_t> flow = network.try_flow(0);
    checks.check(!flow && flow.error().kind() == kahlnws::error_kind::invalid_argument &&
                     std::strcmp(flow.error().message(), "arc 0 is not below the arc count 0") == 0,
                 "no arc should be below the count 0");
    const kahlnws::result<std::int64_t> cost_set = network.try_set_cost(0, 1);
    checks.check(!cost_set && cost_set.error().kind() == kahlnws::error_kind::invalid_argument,
                 "a change of no arc should be an invalid argument");
    const kahlnws::result<void> moved = network.try_move_supply(0, 0, 1);
    checks.check(!moved && std::strcmp(moved.error().message(), "node 0 is not below the node count 0") == 0,
                 "a move between no nodes should be an invalid argument");

    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > answered = network.try_solve();
    const kahlnws::work none = {0, 0, 0};
    checks.check(answered && answered->outcome.optimal() != nullptr &&
                     answered->outcome.optimal()->total_cost == kahlnws::int128(0) && answered->work == none,
                 "its solve should answer the optimum of no node with no work");
}

void test_the_depot_is_loaded_solved_and_read(checks& checks) {
    const tables depot;
    kahlnws::network network;
    checks.check(network.try_reserve(3, 3).has_value(), "the room should be reserved");
    checks.check(network.try_load(depot.problem()).has_value(), "the depot should be loaded");
    checks.check(network.node_count() == 3 && network.arc_count() == 3, "it should hold 3 nodes and 3 arcs");
    const kahlnws::result<kahlnws::int128> unsolved = network.try_total_cost();
    checks.check(!unsolved && unsolved.error().kind() == kahlnws::error_kind::no_optimum &&
                     std::strcmp(unsolved.error().message(),
                                 "the network holds no optimum: it was loaded or changed since its last solve, or that "
                                 "solve found no flow") == 0,
                 "a network loaded and not solved should hold no optimum");

    // From the star, with no arcs tried first: four pivots, one of them degenerate, as a solver's solve of the depot.
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > answered = network.try_solve();
    const kahlnws::work expected = {4, 1, 0};
    checks.check(answered && answered->outcome.optimal() != nullptr &&
                     answered->outcome.optimal()->total_cost == kahlnws::int128(14) && answered->work == expected,
                 "the depot should cost 14, in four pivots, one degenerate");
    const kahlnws::result<std::vector<std::uint64_t> > flows = network.try_flows();
    checks.check(flows && *flows == std::vector<std::uint64_t>({2, 2, 2}), "the flows should be 2, 2, 2");
    const kahlnws::result<std::vector<std::int64_t> > potentials = network.try_potentials();
    checks.check(potentials && *potentials == std::vector<std::int64_t>({0, 1, 5}), "the potentials should be 0, 1, 5");
    const kahlnws::result<std::uint64_t> flow = network.try_flow(1);
    checks.check(flow && *flow == 2, "arc 1 should carry 2");

    std::vector<std::uint64_t> flows_written(3, 9);
    std::int64_t potentials_written[3] = {9, 9, 9};
    checks.check(network.try_flows_into(flows_written) && network.try_potentials_into(potentials_written),
                 "the low level should write the optimum");
    checks.check(flows_written == std::vector<std::uint64_t>({2, 2, 2}) && potentials_written[2] == 5,
                 "the written optimum should be the owned one");

    // Nothing changed: the same optimum, with no pivot.
    checks.check(holds_the_depot_unchanged(network), "a solve with nothing changed should take no pivot");
}

void test_each_change_and_a_solve_after_it_answer_a_fresh_solve(checks& checks) {
    // A cost of 3 on arc 2 makes the leg through the hub cost 4: 2 units by it and 2 direct, 18.
    kahlnws::network network = solved(checks, tables());
    tables changed;
    changed.costs = {5, 1, 3};
    const kahlnws::result<std::int64_t> cost = network.try_set_cost(2, 3);
    checks.check(cost && *cost == 1, "arc 2 should have cost 1");
    checks.check(network.try_total_cost().error().kind() == kahlnws::error_kind::no_optimum, "a change ends it");
    checks.check(answers_as_a_fresh_solve(network, changed), "a cost changed should solve as a fresh solve");

    // A lower bound of 3 on arc 0: 3 direct and 1 by the hub, 17.
    network = solved(checks, tables());
    changed = tables();
    changed.lower_bounds = {3, 0, 0};
    const kahlnws::result<std::uint64_t> lower = network.try_set_lower_bound(0, 3);
    checks.check(lower && *lower == 0, "arc 0 should have lower bound 0");
    checks.check(answers_as_a_fresh_solve(network, changed), "a lower bound changed should solve as a fresh solve");

    // Arc 0 widened to 4: still 2 by the hub, at 2 a unit, and 2 direct, 14.
    network = solved(checks, tables());
    changed = tables();
    changed.capacities = {4, 4, 2};
    const kahlnws::result<std::uint64_t> capacity = network.try_set_capacity(0, 4);
    checks.check(capacity && *capacity == 3, "arc 0 should have capacity 3");
    checks.check(answers_as_a_fresh_solve(network, changed), "a capacity changed should solve as a fresh solve");

    // Arc 2 bounded to 3 to 4: every unit by the hub, 8.
    network = solved(checks, tables());
    changed = tables();
    changed.lower_bounds = {0, 0, 3};
    changed.capacities = {3, 4, 4};
    const kahlnws::result<kahlnws::bounds> bounds = network.try_set_bounds(2, kahlnws::bounds{3, 4});
    const kahlnws::bounds had = {0, 2};
    checks.check(bounds && *bounds == had, "arc 2 should have had the bounds 0 to 2");
    checks.check(answers_as_a_fresh_solve(network, changed), "bounds changed should solve as a fresh solve");

    // The customer's one more unit from the depot goes direct: 3 direct and 2 by the hub, 19.
    network = solved(checks, tables());
    changed = tables();
    changed.supplies = {5, 0, -5};
    checks.check(network.try_move_supply(2, 0, 1).has_value(), "the customer should take one more unit");
    checks.check(answers_as_a_fresh_solve(network, changed), "supplies moved should solve as a fresh solve");
}

void test_a_change_set_back_answers_the_earlier_optimum(checks& checks) {
    const tables depot;
    kahlnws::network network = solved(checks, depot);

    const kahlnws::result<std::int64_t> cost = network.try_set_cost(2, 3);
    checks.check(cost && network.try_set_cost(2, *cost), "a cost should be set back");
    checks.check(answers_as_a_fresh_solve(network, depot), "a cost set back should answer the depot");

    const kahlnws::result<std::uint64_t> capacity = network.try_set_capacity(1, 0);
    checks.check(capacity && *capacity == 4, "arc 1 should be switched off");
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > off = network.try_solve();
    checks.check(off && off->outcome.is_infeasible(), "with the hub cut off, no flow should exist");
    checks.check(network.try_flows().error().kind() == kahlnws::error_kind::no_optimum, "and no optimum is held");
    checks.check(network.try_set_capacity(1, *capacity).has_value(), "arc 1 should be switched on again");
    checks.check(answers_as_a_fresh_solve(network, depot), "on again, the depot");

    const kahlnws::bounds three_to_four = {3, 4};
    const kahlnws::result<kahlnws::bounds> bounds = network.try_set_bounds(2, three_to_four);
    checks.check(bounds && network.try_set_bounds(2, *bounds), "bounds set back by what they answered");
    checks.check(network.try_move_supply(2, 0, 1) && network.try_move_supply(0, 2, 1), "a move and the move back");
    checks.check(answers_as_a_fresh_solve(network, depot), "bounds and supplies set back should answer the depot");

    const kahlnws::result<std::int64_t> same = network.try_set_cost(0, 5);
    checks.check(same && *same == 5 && holds_the_depot_unchanged(network), "the cost held should keep the optimum");
}

void test_each_refusal_is_answered_with_its_values_and_changes_nothing(checks& checks) {
    const kahlnws::result<std::int64_t> bound = kahlnws::try_cost_bound(3);
    kahlnws::network network = solved(checks, tables());

    const kahlnws::result<std::int64_t> cost = network.try_set_cost(1, *bound + 1);
    const std::string cost_message = "arc 1 has cost " + std::to_string(*bound + 1) + " beyond the bound \xC2\xB1" +
                                     std::to_string(*bound) + " for the node count 3";
    checks.check(!cost && refused_change(cost.error(), kahlnws::change_refusal_kind::cost_beyond_bound,
                                         cost_message.c_str()),
                 "a cost beyond its bound should be refused with its message");
    const kahlnws::change_breach::cost_beyond_bound* beyond =
        cost ? nullptr : cost.error().change_refusal()->cost_beyond_bound();
    checks.check(beyond != nullptr && beyond->arc == 1 && beyond->cost == *bound + 1 && beyond->node_count == 3,
                 "a cost beyond its bound should carry its values");
    checks.check(!cost && cost.error().change_refusal()->supply_beyond_range() == nullptr, "other kinds answer null");

    const kahlnws::result<std::uint64_t> lower = network.try_set_lower_bound(0, 4);
    checks.check(!lower && refused_change(lower.error(), kahlnws::change_refusal_kind::lower_above_capacity,
                                          "arc 0 has lower bound 4 above its capacity 3"),
                 "a lower bound above its capacity should be refused");
    const kahlnws::change_breach::lower_above_capacity* above =
        lower ? nullptr : lower.error().change_refusal()->lower_above_capacity();
    checks.check(above != nullptr && above->arc == 0 && above->lower_bound == 4 && above->capacity == 3,
                 "a lower bound above its capacity should carry its values");

    const kahlnws::bounds crossing = {3, 2};
    const kahlnws::result<kahlnws::bounds> crossed = network.try_set_bounds(2, crossing);
    checks.check(!crossed && refused_change(crossed.error(), kahlnws::change_refusal_kind::lower_above_capacity,
                                            "arc 2 has lower bound 3 above its capacity 2"),
                 "bounds that cross should be refused");

    const kahlnws::result<void> moved = network.try_move_supply(2, 1, (std::numeric_limits<std::uint64_t>::max)());
    checks.check(!moved && refused_change(moved.error(), kahlnws::change_refusal_kind::supply_beyond_range,
                                          "the supply of node 2 would be -18446744073709551619, outside the range "
                                          "of a signed 64-bit integer"),
                 "a supply beyond its range should be refused");
    const kahlnws::change_breach::supply_beyond_range* supply =
        moved ? nullptr : moved.error().change_refusal()->supply_beyond_range();
    checks.check(supply != nullptr && supply->node == 2 &&
                     supply->supply ==
                         kahlnws::int128::from_halves((std::numeric_limits<std::uint64_t>::max)() - 2, -2),
                 "a supply beyond its range should carry the supply it would have");
    checks.check(holds_the_depot_unchanged(network), "a refused change should change nothing");

    // Three loops of the widest capacity on one node: three costs at the bound pass 2^127 - 1, two do not.
    tables loops;
    loops.tails = {0, 0, 0};
    loops.heads = {0, 0, 0};
    loops.capacities.assign(3, (std::numeric_limits<std::uint64_t>::max)());
    loops.costs = {0, 0, 0};
    loops.supplies = {0};
    kahlnws::network widest = solved(checks, loops);
    const std::int64_t widest_bound = *kahlnws::try_cost_bound(1);
    checks.check(widest.try_set_cost(0, widest_bound) && widest.try_set_cost(1, -widest_bound), "two costs fit");
    const kahlnws::result<std::int64_t> total = widest.try_set_cost(2, widest_bound);
    const kahlnws::change_breach::cost_total_beyond_range* past =
        total ? nullptr : total.error().change_refusal()->cost_total_beyond_range();
    checks.check(past != nullptr && past->arc == 2, "a cost total past its range should name the arc changed");

    // Four nodes and no arc: each move goes up one root link and down another, 2^64 - 2 in all, then one more unit.
    tables four;
    four.tails.clear();
    four.heads.clear();
    four.capacities.clear();
    four.costs.clear();
    four.supplies = {0, 0, 0, 0};
    kahlnws::network stranded = solved(checks, four);
    const std::uint64_t most = (std::numeric_limits<std::uint64_t>::max)() >> 1;
    checks.check(stranded.try_move_supply(0, 1, most) && stranded.try_move_supply(2, 3, most), "the limit is admitted");
    const kahlnws::result<void> excess = stranded.try_move_supply(2, 0, 1);
    checks.check(!excess && refused_change(excess.error(), kahlnws::change_refusal_kind::excess_beyond_range,
                                           "this change would leave 18446744073709551615 units of flow still to be "
                                           "sent, past 18446744073709551614; a solve that finds a flow leaves none"),
                 "a move past the flow still to be sent a network holds should be refused");
    const kahlnws::change_breach::excess_beyond_range* sum =
        excess ? nullptr : excess.error().change_refusal()->excess_beyond_range();
    checks.check(sum != nullptr && sum->total == kahlnws::uint128((std::numeric_limits<std::uint64_t>::max)()),
                 "the refusal should carry the sum");
    const char* const excess_name = kahlnws::name(kahlnws::change_refusal_kind::excess_beyond_range);
    checks.check(std::strcmp(excess_name, "excess beyond range") == 0 &&
                     std::strcmp(kahlnws::name(kahlnws::error_kind::refused_change), "refused change") == 0 &&
                     std::strcmp(kahlnws::name(kahlnws::error_kind::no_optimum), "no optimum") == 0,
                 "the new kinds should have their names");
}

/// Whether `network` holds the supplies `expected` and, solved now, finds an optimum that costs `cost`.
bool holds_supplies_and_solves_to(kahlnws::network& network, const std::vector<std::int64_t>& expected, int cost) {
    const kahlnws::result<std::vector<std::int64_t> > supplies = network.try_supplies();
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > answered = network.try_solve();
    return supplies && *supplies == expected && answered && answered->outcome.optimal() != nullptr &&
           answered->outcome.optimal()->total_cost == kahlnws::int128(cost);
}

void test_a_supply_set_by_hand(checks& checks) {
    // The depot is to send 2: the customer, balancing it, takes 2. Both go through the hub, at 2 a unit: 4.
    kahlnws::network network = solved(checks, tables());
    const kahlnws::result<std::int64_t> falls = network.try_set_supply(0, 2, 2);
    checks.check(falls && *falls == 4, "the depot should have supplied 4");
    checks.check(network.try_total_cost().error().kind() == kahlnws::error_kind::no_optimum, "a change ends it");
    checks.check(holds_supplies_and_solves_to(network, {2, 0, -2}, 4), "a supplier that falls should cost 4");

    // The customer is to take 5, as the node set and as the balancing node alike: 2 through the hub, 3 directly, 19.
    // The answer set back restores both nodes and the cost.
    network = solved(checks, tables());
    const kahlnws::result<std::int64_t> demand = network.try_set_supply(2, -5, 0);
    checks.check(demand && *demand == -4, "the customer should have taken 4");
    checks.check(holds_supplies_and_solves_to(network, {5, 0, -5}, 19), "a demand that rises should cost 19");
    const kahlnws::result<std::int64_t> back = network.try_set_supply(2, *demand, 0);
    checks.check(back && *back == -5 && holds_supplies_and_solves_to(network, {4, 0, -4}, 14),
                 "the supply answered, set back, should restore both nodes and the cost");
    network = solved(checks, tables());
    const kahlnws::result<std::int64_t> balancing = network.try_set_supply(0, 5, 2);
    checks.check(balancing && *balancing == 4 && holds_supplies_and_solves_to(network, {5, 0, -5}, 19),
                 "a demand that rises as the balancing node should cost 19");

    // The hub is to send 2, balanced by the depot, which sends 2 less; then to take 1, balanced by the customer, which
    // takes 3 less. The depot sends 1 to the hub and 1 on to the customer: 2 + 1 = 3.
    network = solved(checks, tables());
    const kahlnws::result<std::int64_t> sends = network.try_set_supply(1, 2, 0);
    const kahlnws::result<std::int64_t> takes = network.try_set_supply(1, -1, 2);
    checks.check(sends && *sends == 0 && takes && *takes == 2 && holds_supplies_and_solves_to(network, {2, -1, -1}, 3),
                 "a node that changes sides should cost 3");

    // A supply set to what it is, and a node that balances itself given the supply it has: nothing changes.
    network = solved(checks, tables());
    const kahlnws::result<std::int64_t> same = network.try_set_supply(0, 4, 2);
    const kahlnws::result<std::int64_t> itself = network.try_set_supply(1, 0, 1);
    checks.check(same && *same == 4 && itself && *itself == 0 && holds_the_depot_unchanged(network),
                 "a supply set to what it is should keep the optimum and make no pivot");

    // The customer is to take 2^63: the depot would send 2^63, past the greatest std::int64_t, and is named.
    const kahlnws::result<std::int64_t> past = network.try_set_supply(2, (std::numeric_limits<std::int64_t>::min)(), 0);
    checks.check(!past && refused_change(past.error(), kahlnws::change_refusal_kind::supply_beyond_range,
                                         "the supply of node 0 would be 9223372036854775808, outside the range "
                                         "of a signed 64-bit integer"),
                 "a supply whose balancing node would leave its range should be refused");
    const kahlnws::change_breach::supply_beyond_range* beyond =
        past ? nullptr : past.error().change_refusal()->supply_beyond_range();
    checks.check(beyond != nullptr && beyond->node == 0 &&
                     beyond->supply == kahlnws::int128::from_halves(std::uint64_t(1) << 63, 0),
                 "the refusal should name the balancing node with the supply it would have");
    checks.check(holds_the_depot_unchanged(network), "a refused supply should change nothing");
}

void test_a_supply_of_a_node_that_balances_itself_or_not_below_the_count_is_refused(checks& checks) {
    kahlnws::network network = solved(checks, tables());

    const kahlnws::result<std::int64_t> itself = network.try_set_supply(0, 5, 0);
    checks.check(!itself && itself.error().kind() == kahlnws::error_kind::invalid_argument &&
                     std::strcmp(itself.error().message(),
                                 "node 0 is its own balancing node, whose supply 4 should then stay, not become 5") ==
                         0,
                 "a node that balances itself and is to change should be an invalid argument with both supplies");
    const kahlnws::result<std::int64_t> lowest =
        network.try_set_supply(2, (std::numeric_limits<std::int64_t>::min)(), 2);
    checks.check(!lowest && std::strcmp(lowest.error().message(),
                                        "node 2 is its own balancing node, whose supply -4 should then stay, not "
                                        "become -9223372036854775808") == 0,
                 "a negative supply asked for should be written with its sign");

    const kahlnws::result<std::int64_t> node = network.try_set_supply(3, 1, 0);
    checks.check(!node && node.error().kind() == kahlnws::error_kind::invalid_argument &&
                     std::strcmp(node.error().message(), "node 3 is not below the node count 3") == 0,
                 "a node not below the count should be an invalid argument that names it");
    const kahlnws::result<std::int64_t> balancing = network.try_set_supply(0, 1, 4);
    checks.check(!balancing && std::strcmp(balancing.error().message(), "node 4 is not below the node count 3") == 0,
                 "a balancing node not below the count should be named");
    const kahlnws::result<std::int64_t> both = network.try_set_supply(5, 1, 5);
    checks.check(!both && std::strcmp(both.error().message(), "node 5 is not below the node count 3") == 0,
                 "one node twice, not below the count, should be named as not below it");
    checks.check(holds_the_depot_unchanged(network), "an argument refused should change nothing");

    kahlnws::network never_loaded;
    const kahlnws::result<std::int64_t> none = never_loaded.try_set_supply(0, 0, 0);
    checks.check(!none && std::strcmp(none.error().message(), "node 0 is not below the node count 0") == 0,
                 "a network never loaded should have no node");
}

void test_a_refused_load_keeps_what_the_network_held(checks& checks) {
    kahlnws::network network = solved(checks, tables());

    tables unbalanced;
    unbalanced.supplies = {4, 0, -3};
    const kahlnws::result<void> refused = network.try_load(unbalanced.problem());
    checks.check(!refused && refused.error().kind() == kahlnws::error_kind::refused_problem &&
                     refused.error().refusal() != nullptr && refused.error().refusal()->unbalanced() != nullptr &&
                     refused.error().change_refusal() == nullptr &&
                     std::strcmp(refused.error().message(), "the supplies add up to 1, not zero") == 0,
                 "a load of supplies that do not balance should be refused as a solve refuses them");

    tables short_heads;
    short_heads.heads = {2, 1};
    const kahlnws::result<void> length = network.try_load(short_heads.problem());
    checks.check(!length && length.error().refusal() != nullptr && length.error().refusal()->length() != nullptr,
                 "a load of heads of another length should be refused");
    checks.check(holds_the_depot_unchanged(network), "a refused load should keep the problem and its optimum");
}

void test_buffers_of_the_wrong_length_and_arcs_outside_are_refused(checks& checks) {
    kahlnws::network network = solved(checks, tables());

    std::vector<std::uint64_t> short_flows = {7, 7};
    const kahlnws::result<void> flows = network.try_flows_into(short_flows);
    checks.check(!flows && flows.error().kind() == kahlnws::error_kind::invalid_argument &&
                     std::strcmp(flows.error().message(), "2 entries for a network of 3 arcs") == 0,
                 "flows of the wrong length should be an invalid argument with their message");
    std::vector<std::int64_t> long_potentials = {7, 7, 7, 7};
    const kahlnws::result<void> potentials = network.try_potentials_into(long_potentials);
    checks.check(!potentials && std::strcmp(potentials.error().message(), "4 entries for a network of 3 nodes") == 0,
                 "potentials of the wrong length should be an invalid argument with their message");
    checks.check(short_flows == std::vector<std::uint64_t>({7, 7}) &&
                     long_potentials == std::vector<std::int64_t>({7, 7, 7, 7}),
                 "buffers refused should be left");

    const kahlnws::result<std::uint64_t> capacity = network.try_set_capacity(3, 1);
    checks.check(!capacity && std::strcmp(capacity.error().message(), "arc 3 is not below the arc count 3") == 0,
                 "an arc not below the arc count should be an invalid argument with its message");
    const kahlnws::result<void> moved = network.try_move_supply(0, 3, 1);
    checks.check(!moved && moved.error().kind() == kahlnws::error_kind::invalid_argument &&
                     std::strcmp(moved.error().message(), "node 3 is not below the node count 3") == 0,
                 "a node not below the node count should be an invalid argument that names it");
    const kahlnws::result<void> moved_from = network.try_move_supply(4, 0, 1);
    checks.check(!moved_from && std::strcmp(moved_from.error().message(), "node 4 is not below the node count 3") == 0,
                 "a move from a node not below the node count should name that node");
    const kahlnws::bounds zero_to_one = {0, 1};
    checks.check(!network.try_set_lower_bound(3, 0) && !network.try_set_bounds(3, zero_to_one) && !network.try_flow(3),
                 "every arc not below the count should be refused");
    checks.check(holds_the_depot_unchanged(network), "an argument refused should change nothing");
}

void test_every_read_answers_no_optimum_where_none_is_held(checks& checks) {
    kahlnws::network network;
    checks.check(network.try_load(tables().problem()).has_value(), "the depot should be loaded");
    std::vector<std::uint64_t> flows(3, 7);
    std::vector<std::int64_t> potentials(3, 7);

    for (int state = 0; state < 2; ++state) {
        checks.check(network.try_total_cost().error().kind() == kahlnws::error_kind::no_optimum, "no cost");
        checks.check(network.try_flow(0).error().kind() == kahlnws::error_kind::no_optimum, "no flow");
        checks.check(network.try_flows().error().kind() == kahlnws::error_kind::no_optimum, "no flows");
        checks.check(network.try_potentials().error().kind() == kahlnws::error_kind::no_optimum, "no potentials");
        checks.check(!network.try_flows_into(flows) && !network.try_potentials_into(potentials), "nothing written");
        checks.check(flows == std::vector<std::uint64_t>(3, 7) && potentials == std::vector<std::int64_t>(3, 7),
                     "a read with no optimum should leave the buffers");
        // The second round: solved, then changed.
        checks.check(network.try_solve() && network.try_set_cost(0, 4), "solved, then changed");
    }
}

void test_copies_moves_and_a_moved_from_network(checks& checks) {
    kahlnws::network network = solved(checks, tables());

    kahlnws::result<kahlnws::network> cloned = network.try_clone();
    checks.check(cloned.has_value(), "the copy should be made");
    if (cloned) {
        kahlnws::network copy = std::move(*cloned);
        const kahlnws::result<kahlnws::int128> copied = copy.try_total_cost();
        checks.check(copied && *copied == kahlnws::int128(14), "the copy should hold the optimum");
        checks.check(holds_the_depot_unchanged(copy), "the copy should solve from the copied basis, with no pivot");
        checks.check(copy.try_set_cost(2, 3).has_value(), "the copy should change");
        tables changed;
        changed.costs = {5, 1, 3};
        checks.check(answers_as_a_fresh_solve(copy, changed), "the copy should solve its own problem");
        checks.check(holds_the_depot_unchanged(network), "the network should stay as it was");
    }

    kahlnws::network moved(std::move(network));
    checks.check(holds_the_depot_unchanged(moved), "a network moved into should hold what it held");
    const kahlnws::result<kahlnws::int128> moved_from = network.try_total_cost();
    checks.check(network.node_count() == 0 && moved_from && *moved_from == kahlnws::int128(0),
                 "a moved-from network should be a new one");
    const kahlnws::result<kahlnws::network> empty_copy = network.try_clone();
    checks.check(empty_copy && empty_copy->node_count() == 0, "a copy of a new network should be a new one");
    // A reference hides the self-move from the compiler's warning, which is what this checks.
    kahlnws::network& same = moved;
    moved = std::move(same);
    checks.check(holds_the_depot_unchanged(moved), "a network moved onto itself should be as it was");

    std::vector<kahlnws::network> networks;
    for (std::size_t count = 0; count < 9; ++count) {
        networks.push_back(solved(checks, tables()));
    }
    for (kahlnws::network& kept : networks) {
        checks.check(holds_the_depot_unchanged(kept), "a network moved by the vector should keep its optimum");
    }
}

/// Whether `network` reads back `problem`, the problem it should hold, by every read of the problem: one arc or node
/// at a time, every column owned, and every column into a buffer.
bool reads_back(const kahlnws::network& network, const tables& problem) {
    const std::size_t arcs = problem.tails.size();
    std::vector<std::uint64_t> lower_bounds = problem.lower_bounds;
    if (lower_bounds.empty()) {
        lower_bounds.assign(arcs, 0);
    }
    const kahlnws::result<std::vector<std::size_t> > tails = network.try_tails();
    const kahlnws::result<std::vector<std::size_t> > heads = network.try_heads();
    const kahlnws::result<std::vector<std::int64_t> > costs = network.try_costs();
    const kahlnws::result<std::vector<std::uint64_t> > lower = network.try_lower_bounds();
    const kahlnws::result<std::vector<std::uint64_t> > capacities = network.try_capacities();
    const kahlnws::result<std::vector<std::int64_t> > supplies = network.try_supplies();
    if (!tails || !heads || !costs || !lower || !capacities || !supplies || *tails != problem.tails ||
        *heads != problem.heads || *costs != problem.costs || *lower != lower_bounds ||
        *capacities != problem.capacities || *supplies != problem.supplies) {
        return false;
    }

    std::vector<std::size_t> ends(arcs, 7);
    std::vector<std::int64_t> numbers(problem.supplies.size(), 7);
    std::vector<std::uint64_t> bounds(arcs, 7);
    if (!network.try_heads_into(ends) || ends != problem.heads || !network.try_supplies_into(numbers) ||
        numbers != problem.supplies || !network.try_capacities_into(bounds) || bounds != problem.capacities ||
        !network.try_lower_bounds_into(bounds) || bounds != lower_bounds || !network.try_tails_into(ends) ||
        ends != problem.tails) {
        return false;
    }
    std::vector<std::int64_t> arc_costs(arcs, 7);
    if (!network.try_costs_into(arc_costs) || arc_costs != problem.costs) {
        return false;
    }

    for (std::size_t arc = 0; arc < arcs; ++arc) {
        const kahlnws::result<std::size_t> tail = network.try_tail(arc);
        const kahlnws::result<std::size_t> head = network.try_head(arc);
        const kahlnws::result<std::int64_t> cost = network.try_cost(arc);
        const kahlnws::result<kahlnws::bounds> both = network.try_bounds(arc);
        const kahlnws::result<std::uint64_t> lower_bound = network.try_lower_bound(arc);
        const kahlnws::result<std::uint64_t> capacity = network.try_capacity(arc);
        const kahlnws::bounds expected = {lower_bounds[arc], problem.capacities[arc]};
        if (!tail || !head || !cost || !both || !lower_bound || !capacity || *tail != problem.tails[arc] ||
            *head != problem.heads[arc] || *cost != problem.costs[arc] || *both != expected ||
            *lower_bound != expected.lower_bound || *capacity != expected.capacity) {
            return false;
        }
    }
    for (std::size_t node = 0; node < problem.supplies.size(); ++node) {
        const kahlnws::result<std::int64_t> supply = network.try_supply(node);
        if (!supply || *supply != problem.supplies[node]) {
            return false;
        }
    }
    return true;
}

void test_the_reads_of_the_problem_answer_it_as_it_stands_and_need_no_optimum(checks& checks) {
    tables problem;
    problem.lower_bounds = {1, 0, 0};
    kahlnws::network network;
    checks.check(network.try_load(problem.problem()).has_value(), "the problem should be loaded");
    checks.check(reads_back(network, problem), "a network loaded should read its problem back");
    checks.check(!network.try_total_cost(), "a read of the problem needs no optimum and makes none");

    const kahlnws::bounds one_to_three = {1, 3};
    checks.check(network.try_solve().has_value() && network.try_set_cost(2, 3) &&
                     network.try_set_bounds(2, one_to_three) && network.try_set_capacity(0, 4) &&
                     network.try_set_lower_bound(1, 1) && network.try_move_supply(2, 0, 1),
                 "every kind of change should be made");
    problem.costs = {5, 1, 3};
    problem.lower_bounds = {1, 1, 1};
    problem.capacities = {4, 4, 3};
    problem.supplies = {5, 0, -5};
    checks.check(reads_back(network, problem), "the reads should answer every change");
    checks.check(!network.try_set_cost(0, (std::numeric_limits<std::int64_t>::max)()) &&
                     !network.try_move_supply(2, 1, (std::numeric_limits<std::uint64_t>::max)()),
                 "the changes should be refused");
    checks.check(reads_back(network, problem), "a refused change should read as before");

    kahlnws::network solved_depot = solved(checks, tables());
    checks.check(reads_back(solved_depot, tables()), "a read should answer beside an optimum");
    checks.check(holds_the_depot_unchanged(solved_depot), "and keep it");

    kahlnws::network copy;
    checks.check(copy.try_clone_from(network).has_value() && reads_back(copy, problem), "a copy reads as its source");

    const kahlnws::network never_loaded;
    tables none;
    none.tails.clear();
    none.heads.clear();
    none.capacities.clear();
    none.costs.clear();
    none.supplies.clear();
    checks.check(reads_back(never_loaded, none), "a network never loaded should read no number");
}

void test_a_read_of_the_problem_past_its_count_or_of_the_wrong_length_is_refused(checks& checks) {
    const kahlnws::network network = solved(checks, tables());
    const kahlnws::result<std::size_t> tail = network.try_tail(3);
    checks.check(!tail && tail.error().kind() == kahlnws::error_kind::invalid_argument &&
                     std::strcmp(tail.error().message(), "arc 3 is not below the arc count 3") == 0,
                 "an arc past the count should be an invalid argument with its message");
    const kahlnws::result<std::int64_t> supply = network.try_supply(3);
    checks.check(!supply && std::strcmp(supply.error().message(), "node 3 is not below the node count 3") == 0,
                 "a node past the count should be an invalid argument with its message");
    checks.check(!network.try_head(3) && !network.try_cost(9) && !network.try_bounds(3) &&
                     !network.try_lower_bound(3) && !network.try_capacity(3),
                 "every arc past the count");
    const kahlnws::network never_loaded;
    checks.check(!never_loaded.try_tail(0) && !never_loaded.try_supply(0), "a network never loaded has no arc or node");

    std::vector<std::uint64_t> short_capacities = {7, 7};
    const kahlnws::result<void> capacities = network.try_capacities_into(short_capacities);
    checks.check(!capacities && std::strcmp(capacities.error().message(), "2 entries for a network of 3 arcs") == 0,
                 "a column of the wrong length should be an invalid argument with its message");
    std::vector<std::int64_t> long_supplies(4, 7);
    const kahlnws::result<void> supplies = network.try_supplies_into(long_supplies);
    checks.check(!supplies && std::strcmp(supplies.error().message(), "4 entries for a network of 3 nodes") == 0,
                 "supplies of the wrong length should be an invalid argument with their message");
    checks.check(short_capacities == std::vector<std::uint64_t>({7, 7}) &&
                     long_supplies == std::vector<std::int64_t>(4, 7),
                 "buffers refused should be left");
}

void test_a_copy_into_a_network_reuses_it_and_solves_from_the_copied_basis(checks& checks) {
    kahlnws::network source = solved(checks, tables());
    tables chain;
    chain.tails = {0, 1, 2};
    chain.heads = {1, 2, 3};
    chain.capacities = {5, 5, 5};
    chain.costs = {1, 1, 1};
    chain.supplies = {2, 0, 0, -2};
    kahlnws::network destination = solved(checks, chain);

    checks.check(destination.try_clone_from(source).has_value(), "the copy into a network should be made");
    checks.check(holds_the_depot_unchanged(destination), "the copy should solve from the copied basis");
    checks.check(destination.try_set_cost(2, 3) && holds_the_depot_unchanged(source), "the copy changes apart");
    checks.check(source.try_clone_from(source).has_value() && holds_the_depot_unchanged(source),
                 "a copy of itself should change nothing");
    const kahlnws::network never_loaded;
    checks.check(destination.try_clone_from(never_loaded) && destination.node_count() == 0,
                 "a copy of a network that holds no handle should hold the problem of no node");
}

/// Whether a solve of `network` answers the depot's optimum, 14, with no pivot.
bool solves_the_depot_with_no_pivot(kahlnws::network& network) {
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > answered = network.try_solve();
    const kahlnws::work none = {0, 0, 0};
    return answered && answered->outcome.optimal() != nullptr &&
           answered->outcome.optimal()->total_cost == kahlnws::int128(14) && answered->work == none;
}

void test_a_load_starts_from_the_basis_held_where_it_serves(checks& checks) {
    const kahlnws::work none = {0, 0, 0};
    kahlnws::network network = solved(checks, tables());
    checks.check(network.try_load(tables().problem()).has_value(), "the depot should be loaded again");
    checks.check(!network.try_total_cost(), "a load should end the optimum");
    checks.check(solves_the_depot_with_no_pivot(network), "the same problem loaded again should make no pivot");

    // One unit less: arc 2 stays full and arc 0 carries 1, at 5 + 2 + 2 = 9, which the kept basis carries.
    tables fewer;
    fewer.supplies = {3, 0, -3};
    checks.check(network.try_load(fewer.problem()).has_value(), "one unit less should be loaded");
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > kept = network.try_solve();
    checks.check(kept && kept->outcome.optimal() != nullptr &&
                     kept->outcome.optimal()->total_cost == kahlnws::int128(9) && kept->work == none,
                 "one unit less should cost 9 with no pivot");
    kahlnws::network fresh;
    checks.check(fresh.try_load(fewer.problem()).has_value(), "a fresh network should load one unit less");
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > from_the_star = fresh.try_solve();
    checks.check(from_the_star && from_the_star->work != none, "a fresh network starts from the star and pivots");

    // Other counts answer as a fresh network, its work included.
    tables chain;
    chain.tails = {0, 1, 2};
    chain.heads = {1, 2, 3};
    chain.capacities = {5, 5, 5};
    chain.costs = {1, 1, 1};
    chain.supplies = {2, 0, 0, -2};
    kahlnws::network chain_fresh;
    checks.check(network.try_load(chain.problem()) && chain_fresh.try_load(chain.problem()), "the chain loads");
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > other = network.try_solve();
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > from_star = chain_fresh.try_solve();
    checks.check(other && from_star && *other == *from_star && other->work != none,
                 "a load of other counts should answer as a fresh network");

    // Other ends on the same counts answer as a fresh network too, from the depot's optimum: the depot with arc 0
    // leaving the hub. All 4 units go to the hub by arc 1, at 4; from there 2 by arc 2, at 2, and 2 by arc 0, at 10:
    // 16, the one optimum.
    tables moved;
    moved.tails = {1, 0, 1};
    kahlnws::network moved_held = solved(checks, tables());
    kahlnws::network moved_fresh;
    checks.check(moved_held.try_load(moved.problem()) && moved_fresh.try_load(moved.problem()),
                 "the depot with arc 0 moved loads");
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > moved_answer = moved_held.try_solve();
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > moved_from_star = moved_fresh.try_solve();
    checks.check(moved_answer && moved_answer->outcome.optimal() != nullptr &&
                     moved_answer->outcome.optimal()->total_cost == kahlnws::int128(16),
                 "the depot with arc 0 moved should cost 16");
    checks.check(moved_answer && moved_from_star && *moved_answer == *moved_from_star,
                 "a load of other ends should answer as a fresh network, its work included");

    // A refused load between two loads keeps the basis.
    kahlnws::network kept_network = solved(checks, tables());
    tables unbalanced;
    unbalanced.supplies = {4, 0, -3};
    checks.check(!kept_network.try_load(unbalanced.problem()), "the unbalanced depot should be refused");
    checks.check(kept_network.try_load(tables().problem()) && solves_the_depot_with_no_pivot(kept_network),
                 "the next load should start from the basis kept, with no pivot");
}

void test_the_throwing_forms(checks& checks) {
#if KAHLNWS_EXCEPTIONS
    const tables depot;
    kahlnws::network network;
    network.reserve(3, 3);
    network.load(depot.problem());
    checks.check(network.solve().outcome.optimal()->total_cost == kahlnws::int128(14), "solve should answer 14");
    checks.check(network.total_cost() == kahlnws::int128(14) && network.flow(0) == 2, "the reads should answer");
    checks.check(network.flows() == std::vector<std::uint64_t>({2, 2, 2}), "flows should answer");
    checks.check(network.potentials() == std::vector<std::int64_t>({0, 1, 5}), "potentials should answer");
    std::vector<std::uint64_t> flows(3);
    std::vector<std::int64_t> potentials(3);
    network.flows_into(flows);
    network.potentials_into(potentials);
    checks.check(flows[2] == 2 && potentials[1] == 1, "the low-level reads should write");

    kahlnws::network copy(network);
    checks.check(copy.total_cost() == kahlnws::int128(14), "the copy constructor should copy the optimum");
    checks.check(copy.set_cost(2, 3) == 1 && copy.solve().outcome.optimal()->total_cost == kahlnws::int128(18),
                 "the copy should change apart");
    copy = network;
    checks.check(copy.total_cost() == kahlnws::int128(14), "the copy assignment should copy the optimum");
    const kahlnws::work none = {0, 0, 0};
    checks.check(copy.solve().work == none, "the copy assigned should solve from the copied basis");
    // A reference hides the self-assignment from the compiler's warning, which is what this checks.
    kahlnws::network& itself = copy;
    copy = itself;
    checks.check(copy.total_cost() == kahlnws::int128(14), "a network assigned to itself should be as it was");
    checks.check(copy.tail(2) == 1 && copy.head(2) == 2 && copy.cost(0) == 5 && copy.supply(2) == -4 &&
                     copy.bounds(0) == kahlnws::bounds{0, 3} && copy.lower_bound(0) == 0 && copy.capacity(0) == 3,
                 "the throwing reads should answer one arc or node");
    checks.check(copy.tails() == std::vector<std::size_t>({0, 0, 1}) &&
                     copy.capacities() == std::vector<std::uint64_t>({3, 4, 2}) &&
                     copy.supplies() == std::vector<std::int64_t>({4, 0, -4}) &&
                     copy.lower_bounds() == std::vector<std::uint64_t>(3, 0) &&
                     copy.heads() == std::vector<std::size_t>({2, 1, 2}) &&
                     copy.costs() == std::vector<std::int64_t>({5, 1, 1}),
                 "the throwing reads should answer every column");
    std::vector<std::size_t> ends(3);
    copy.tails_into(ends);
    copy.heads_into(ends);
    std::vector<std::int64_t> numbers(3);
    copy.costs_into(numbers);
    copy.supplies_into(numbers);
    std::vector<std::uint64_t> bounds(3);
    copy.lower_bounds_into(bounds);
    copy.capacities_into(bounds);
    checks.check(ends[0] == 2 && numbers[0] == 4 && bounds[1] == 4, "the throwing reads into buffers should write");

    bool read_thrown = false;
    try {
        static_cast<void>(copy.tail(3));
    } catch (const std::invalid_argument& error) {
        read_thrown = std::string(error.what()) == "arc 3 is not below the arc count 3";
    }
    checks.check(read_thrown, "a read past the count should throw std::invalid_argument");

    const kahlnws::bounds expected = {0, 2};
    const kahlnws::bounds had = network.set_bounds(2, expected);
    checks.check(had == expected && network.set_capacity(0, 3) == 3 && network.set_lower_bound(0, 0) == 0,
                 "the bounds held should be answered, and change nothing");
    const kahlnws::bounds wider = {0, 5};
    checks.check(network.set_bounds(2, wider) == expected && network.set_bounds(2, had) == wider &&
                     network.bounds(2) == expected,
                 "the bounds answered should set the arc back by the form that set it");
    checks.check(network.solve().outcome.optimal()->total_cost == kahlnws::int128(14),
                 "set back, the depot should cost 14 again");
    network.move_supply(1, 1, 5);
    checks.check(network.set_supply(0, 4, 2) == 4 && network.total_cost() == kahlnws::int128(14),
                 "changes of nothing should keep the optimum");

    bool thrown = false;
    try {
        static_cast<void>(network.set_cost(0, (std::numeric_limits<std::int64_t>::max)()));
    } catch (const kahlnws::change_error& error) {
        thrown = error.refusal().cost_beyond_bound() != nullptr &&
                 std::string(error.what()).compare(0, 15, "arc 0 has cost ") == 0;
    }
    checks.check(thrown, "a refused change should throw kahlnws::change_error");

    thrown = false;
    try {
        network.move_supply(0, 2, 1);
        static_cast<void>(network.total_cost());
    } catch (const std::logic_error& error) {
        thrown = dynamic_cast<const std::invalid_argument*>(&error) == nullptr;
    }
    checks.check(thrown, "a read with no optimum should throw std::logic_error");

    thrown = false;
    try {
        static_cast<void>(network.set_capacity(9, 1));
    } catch (const std::invalid_argument& error) {
        thrown = dynamic_cast<const kahlnws::change_error*>(&error) == nullptr &&
                 std::string(error.what()) == "arc 9 is not below the arc count 3";
    }
    checks.check(thrown, "an arc outside should throw std::invalid_argument");

    thrown = false;
    try {
        static_cast<void>(network.set_supply(1, 3, 1));
    } catch (const std::invalid_argument& error) {
        thrown = dynamic_cast<const kahlnws::change_error*>(&error) == nullptr &&
                 std::string(error.what()) == "node 1 is its own balancing node, whose supply 0 should then stay, not "
                                              "become 3";
    }
    checks.check(thrown, "a node that balances itself and is to change should throw std::invalid_argument");

    thrown = false;
    try {
        static_cast<void>(network.set_supply(2, (std::numeric_limits<std::int64_t>::min)(), 0));
    } catch (const kahlnws::change_error& error) {
        thrown = error.refusal().supply_beyond_range() != nullptr;
    }
    checks.check(thrown, "a refused supply should throw kahlnws::change_error");
    checks.check(network.set_supply(2, -4, 0) == -3 && network.set_supply(2, -3, 0) == -4,
                 "the throwing form should answer the supply the node had");

    thrown = false;
    try {
        tables unbalanced;
        unbalanced.supplies = {4, 0, -3};
        network.load(unbalanced.problem());
    } catch (const kahlnws::problem_error& error) {
        thrown = error.refusal().unbalanced() != nullptr;
    }
    checks.check(thrown, "a refused load should throw kahlnws::problem_error");

    thrown = false;
    try {
        network.reserve((std::numeric_limits<std::size_t>::max)(), 1);
    } catch (const std::bad_alloc&) {
        thrown = true;
    }
    checks.check(thrown, "a reservation past memory should throw std::bad_alloc");
#else
    checks.check(true, "no throwing forms without exceptions");
#endif
}

void test_refused_memory_is_answered(checks& checks) {
    kahlnws::network network = solved(checks, tables());
    const kahlnws::result<void> reserved = network.try_reserve((std::numeric_limits<std::size_t>::max)(), 1);
    checks.check(!reserved && reserved.error().kind() == kahlnws::error_kind::refused_memory,
                 "a reservation past memory should be refused memory");
    checks.check(holds_the_depot_unchanged(network), "the network should stay as it was");
}

}  // namespace

int main() {
    checks checks("network");

    test_a_new_network_holds_the_problem_of_no_node(checks);
    test_the_depot_is_loaded_solved_and_read(checks);
    test_each_change_and_a_solve_after_it_answer_a_fresh_solve(checks);
    test_a_change_set_back_answers_the_earlier_optimum(checks);
    test_each_refusal_is_answered_with_its_values_and_changes_nothing(checks);
    test_a_supply_set_by_hand(checks);
    test_a_supply_of_a_node_that_balances_itself_or_not_below_the_count_is_refused(checks);
    test_a_refused_load_keeps_what_the_network_held(checks);
    test_buffers_of_the_wrong_length_and_arcs_outside_are_refused(checks);
    test_every_read_answers_no_optimum_where_none_is_held(checks);
    test_copies_moves_and_a_moved_from_network(checks);
    test_the_reads_of_the_problem_answer_it_as_it_stands_and_need_no_optimum(checks);
    test_a_read_of_the_problem_past_its_count_or_of_the_wrong_length_is_refused(checks);
    test_a_copy_into_a_network_reuses_it_and_solves_from_the_copied_basis(checks);
    test_a_load_starts_from_the_basis_held_where_it_serves(checks);
    test_the_throwing_forms(checks);
    test_refused_memory_is_answered(checks);

    return checks.exit_code();
}
