// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// One solver solving again: the network of `transport.cpp`, for the three days of `reuse.cpp`, whose shops ask for
// different amounts while the warehouses' supplies stay 20 and 30. Day 1 is solved with try_solve; each later day with
// try_resolve, which starts from the basis the solver kept from the day before. The program prints each day's cost and
// its pivots, beside the pivots a second solver, which solves every day afresh, makes. It uses the forms that answer
// errors as values, so it builds with exceptions enabled or not.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     c++ -std=c++11 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/resolve.cpp target/release/libkahlnws_capi.a -o resolve
//     ./resolve

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

    // The solver that keeps each day's basis, and one that solves each day from the problem alone.
    kahlnws::network_simplex solver;
    kahlnws::network_simplex fresh;

    for (std::size_t day = 0; day < days.size(); ++day) {
        problem.supplies = days[day];
        const kahlnws::result<kahlnws::answer<kahlnws::solution>> answered =
            day == 0 ? solver.try_solve(problem) : solver.try_resolve(problem);
        const kahlnws::result<kahlnws::answer<kahlnws::solution>> fresh_answered = fresh.try_solve(problem);
        if (!answered || !fresh_answered) {
            const kahlnws::error& error = !answered ? answered.error() : fresh_answered.error();
            std::cerr << "day " << day + 1 << ": the solve failed: " << error.message() << '\n';
            return 1;
        }
        const kahlnws::solution* const solution = answered->outcome.optimal();
        if (solution == nullptr) {
            std::cerr << "day " << day + 1 << ": no flow exists\n";
            return 1;
        }
        std::cout << "day " << day + 1 << ": cost " << solution->total_cost << ", pivots " << answered->work.pivots
                  << " (a fresh solve: " << fresh_answered->work.pivots << ")\n";
    }
    return 0;
}
