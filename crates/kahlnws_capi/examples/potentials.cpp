// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// Everything an optimal answer holds beside the flow, on the network of `transport.cpp`: the potential of each node;
// the reduced cost of each lane computed from them, its cost plus the potential of its warehouse less that of its shop;
// the optimality conditions those prove, checked lane by lane (a lane with room left has a reduced cost of at least
// zero, and a lane that carries flow one of at most zero); and the work the solve did, counted in pivots. It uses the
// forms that answer errors as values, so it builds with exceptions enabled or not.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     c++ -std=c++11 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/potentials.cpp target/release/libkahlnws_capi.a -o potentials
//     ./potentials

#include <cstddef>
#include <cstdint>
#include <iostream>
#include <vector>

#include "kahlnws.hpp"

int main() {
    // Node w is warehouse w and node 2 + s shop s; lane 3 * w + s leads from warehouse w to shop s.
    const std::size_t warehouse_count = 2;
    const std::size_t shop_count = 3;

    const std::vector<std::int64_t> supplies = {20, 30, -10, -25, -15};
    const std::vector<std::size_t> tails = {0, 0, 0, 1, 1, 1};
    const std::vector<std::size_t> heads = {2, 3, 4, 2, 3, 4};
    const std::vector<std::uint64_t> capacities = {20, 20, 20, 20, 20, 20};
    const std::vector<std::int64_t> costs = {8, 6, 10, 9, 12, 13};

    kahlnws::problem problem;
    problem.tails = tails;
    problem.heads = heads;
    problem.capacities = capacities;
    problem.costs = costs;
    problem.supplies = supplies;

    kahlnws::network_simplex solver;
    const kahlnws::result<kahlnws::answer<kahlnws::solution>> solved = solver.try_solve(problem);
    if (!solved) {
        std::cerr << "the solve failed: " << solved.error().message() << '\n';
        return 1;
    }
    const kahlnws::solution* const solution = solved->outcome.optimal();
    if (solution == nullptr) {
        std::cerr << "no flow exists\n";
        return 1;
    }
    std::cout << "cost: " << solution->total_cost << '\n';

    const std::vector<std::int64_t>& potentials = solution->potentials;
    for (std::size_t warehouse = 0; warehouse < warehouse_count; ++warehouse) {
        std::cout << "potential of warehouse " << warehouse << ": " << potentials[warehouse] << '\n';
    }
    for (std::size_t shop = 0; shop < shop_count; ++shop) {
        std::cout << "potential of shop " << shop << ": " << potentials[warehouse_count + shop] << '\n';
    }

    bool conditions_hold = true;
    for (std::size_t warehouse = 0; warehouse < warehouse_count; ++warehouse) {
        for (std::size_t shop = 0; shop < shop_count; ++shop) {
            const std::size_t lane = warehouse * shop_count + shop;
            // A problem the solve admits keeps every potential and reduced cost, and this sum on its way, within 64
            // bits.
            const std::int64_t reduced_cost = costs[lane] + potentials[tails[lane]] - potentials[heads[lane]];
            std::cout << "reduced cost of warehouse " << warehouse << " -> shop " << shop << ": " << reduced_cost
                      << '\n';

            const std::uint64_t flow = solution->flows[lane];
            const bool has_room = flow < capacities[lane];
            const bool carries_flow = flow > 0;
            if ((has_room && reduced_cost < 0) || (carries_flow && reduced_cost > 0)) {
                conditions_hold = false;
            }
        }
    }
    std::cout << "the optimality conditions " << (conditions_hold ? "hold" : "fail") << '\n';

    std::cout << "pivots: " << solved->work.pivots << '\n';
    std::cout << "degenerate pivots: " << solved->work.degenerate_pivots << '\n';
    std::cout << "first arc pivots: " << solved->work.first_arc_pivots << '\n';
    return conditions_hold ? 0 : 1;
}
