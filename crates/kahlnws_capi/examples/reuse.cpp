// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// One solver kept for many solves: the network of `transport.cpp`, solved for three days whose shops ask for different
// amounts while the warehouses' supplies stay 20 and 30. The solver's room is reserved once, and every day is solved
// into the same buffer the program holds, so no solve allocates. The program prints each day's cost. It uses the forms
// that answer errors as values, so it builds with exceptions enabled or not.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     c++ -std=c++11 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/reuse.cpp target/release/libkahlnws_capi.a -o reuse
//     ./reuse

#include <cstddef>
#include <cstdint>
#include <iostream>
#include <vector>

#include "kahlnws.hpp"

int main() {
    // The supplies of each day: the warehouses' first, then the shops' demands, negative.
    const std::vector<std::vector<std::int64_t> > days = {
        {20, 30, -10, -25, -15},
        {20, 30, -20, -20, -10},
        {20, 30, -5, -15, -30},
    };
    // Node w is warehouse w and node 2 + s shop s; lane 3 * w + s leads from warehouse w to shop s.
    const std::vector<std::size_t> tails = {0, 0, 0, 1, 1, 1};
    const std::vector<std::size_t> heads = {2, 3, 4, 2, 3, 4};
    const std::vector<std::uint64_t> capacities = {20, 20, 20, 20, 20, 20};
    const std::vector<std::int64_t> costs = {8, 6, 10, 9, 12, 13};

    kahlnws::problem problem;
    problem.tails = tails;
    problem.heads = heads;
    problem.capacities = capacities;
    problem.costs = costs;

    kahlnws::network_simplex solver;
    const kahlnws::result<void> reserved = solver.try_reserve(days[0].size(), tails.size());
    if (!reserved) {
        std::cerr << "the reservation failed: " << reserved.error().message() << '\n';
        return 1;
    }

    // The buffer every day's flows are written to.
    std::vector<std::uint64_t> flows(tails.size());
    for (std::size_t day = 0; day < days.size(); ++day) {
        problem.supplies = days[day];
        const kahlnws::result<kahlnws::answer<kahlnws::optimum>> written = solver.try_solve_into(problem, flows);
        if (!written) {
            std::cerr << "day " << day + 1 << ": the solve failed: " << written.error().message() << '\n';
            return 1;
        }
        const kahlnws::optimum* const optimum = written->outcome.optimal();
        if (optimum == nullptr) {
            std::cerr << "day " << day + 1 << ": no flow exists\n";
            return 1;
        }
        std::cout << "day " << day + 1 << ": cost " << optimum->total_cost << '\n';
    }
    return 0;
}
