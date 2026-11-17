// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// A network held over three days: the network of `transport.cpp`, for the three days of `reuse.cpp`, whose shops ask
// for different amounts while the warehouses' supplies stay 20 and 30. Day 1 is loaded into a network that held
// nothing and solved; each later day is loaded with that day's numbers, on the same lanes, and solved from the basis
// the network kept. The program prints each day's cost and the pivots its solve made, beside the pivots of a fresh
// solve of the same day by the solver, which keeps nothing from one day to the next, and checks each line: the costs
// worked by hand, the pivots the library's own counts, pinned. It uses the forms that answer errors as values, so it
// builds with exceptions enabled or not.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     c++ -std=c++11 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/network.cpp target/release/libkahlnws_capi.a -o network
//     ./network

#include <cstddef>
#include <cstdint>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

#include "kahlnws.hpp"

int main() {
    // The supplies of each day: the warehouses' first, then the shops' demands, negative.
    const std::vector<std::vector<std::int64_t> > supplies = {
        {20, 30, -10, -25, -15},
        {20, 30, -20, -20, -10},
        {20, 30, -5, -15, -30},
    };
    // Node w is warehouse w and node 2 + s shop s; lane 3 * w + s leads from warehouse w to shop s.
    const std::vector<std::size_t> tails = {0, 0, 0, 1, 1, 1};
    const std::vector<std::size_t> heads = {2, 3, 4, 2, 3, 4};
    const std::vector<std::uint64_t> capacities = {20, 20, 20, 20, 20, 20};
    const std::vector<std::int64_t> costs = {8, 6, 10, 9, 12, 13};

    // Each day's cost, worked by hand: a flow costs what warehouse 1 would pay to serve every shop alone, less what
    // each unit warehouse 0 sends in its place saves, 1, 6 and 3 at shops 0, 1 and 2. Beside it, the pivots of the
    // network's solve and of a fresh one, which are not worked by hand: they are the counts this library's solves make,
    // pinned so that the example shows a later day, started from the basis the day before ended on, taking far fewer.
    const std::vector<std::string> days = {
        // Warehouse 1 alone: 9 * 10 + 12 * 25 + 13 * 15 = 585; warehouse 0's 20 units save 6 each at shop 1: 465.
        "day 1: cost 465, pivots 6 (a fresh solve: 6)",
        // Warehouse 1 alone: 180 + 240 + 130 = 550; warehouse 0's 20 units save 6 each at shop 1: 430.
        "day 2: cost 430, pivots 1 (a fresh solve: 5)",
        // Warehouse 1 alone: 45 + 180 + 390 = 615; its lane carries 20 of shop 2's 30, so warehouse 0 sends the other
        // 10 there, saving 3 each, and its last 10 to shop 1, saving 6 each: 525.
        "day 3: cost 525, pivots 1 (a fresh solve: 7)",
    };

    kahlnws::problem problem;
    problem.tails = tails;
    problem.heads = heads;
    problem.capacities = capacities;
    problem.costs = costs;

    // The network that keeps each day's basis, and the solver that solves each day from nothing kept.
    kahlnws::network network;
    kahlnws::network_simplex fresh;

    for (std::size_t day = 0; day < days.size(); ++day) {
        problem.supplies = supplies[day];
        const kahlnws::result<void> loaded = network.try_load(problem);
        if (!loaded) {
            std::cerr << "day " << day + 1 << ": the load failed: " << loaded.error().message() << '\n';
            return 1;
        }
        const kahlnws::result<kahlnws::answer<kahlnws::optimum> > answered = network.try_solve();
        const kahlnws::result<kahlnws::answer<kahlnws::solution> > fresh_answered = fresh.try_solve(problem);
        if (!answered || !fresh_answered) {
            const kahlnws::error& error = !answered ? answered.error() : fresh_answered.error();
            std::cerr << "day " << day + 1 << ": the solve failed: " << error.message() << '\n';
            return 1;
        }
        const kahlnws::optimum* const optimum = answered->outcome.optimal();
        if (optimum == nullptr) {
            std::cerr << "day " << day + 1 << ": no flow exists\n";
            return 1;
        }

        std::ostringstream line;
        line << "day " << day + 1 << ": cost " << optimum->total_cost << ", pivots " << answered->work.pivots
             << " (a fresh solve: " << fresh_answered->work.pivots << ")";
        std::cout << line.str() << '\n';
        if (line.str() != days[day]) {
            std::cerr << "day " << day + 1 << " should print \"" << days[day] << "\"\n";
            return 1;
        }
    }
    return 0;
}
