// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// The smallest useful call: two warehouses with supplies 20 and 30 serve three shops with demands 10, 25 and 15, over a
// lane from every warehouse to every shop that carries at most 20 units. One solve finds the cheapest way to ship; the
// program prints its cost and the flow on each lane. It uses the forms that answer errors as values, so it builds with
// exceptions enabled or not.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     c++ -std=c++11 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/transport.cpp target/release/libkahlnws_capi.a -o transport
//     ./transport

#include <cstddef>
#include <cstdint>
#include <iostream>
#include <vector>

#include "kahlnws.hpp"

int main() {
    // Node w is warehouse w and node 2 + s shop s; lane 3 * w + s leads from warehouse w to shop s.
    const std::size_t warehouse_count = 2;
    const std::size_t shop_count = 3;

    // A supply is positive, a demand negative.
    const std::vector<std::int64_t> supplies = {20, 30, -10, -25, -15};
    const std::vector<std::size_t> tails = {0, 0, 0, 1, 1, 1};
    const std::vector<std::size_t> heads = {2, 3, 4, 2, 3, 4};
    const std::vector<std::uint64_t> capacities = {20, 20, 20, 20, 20, 20};
    const std::vector<std::int64_t> costs = {8, 6, 10, 9, 12, 13};

    // Every lower bound is zero, so `lower_bounds` stays empty.
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
    for (std::size_t warehouse = 0; warehouse < warehouse_count; ++warehouse) {
        for (std::size_t shop = 0; shop < shop_count; ++shop) {
            std::cout << "warehouse " << warehouse << " -> shop " << shop << ": "
                      << solution->flows[warehouse * shop_count + shop] << '\n';
        }
    }
    return 0;
}
