// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// The answers that are not an optimal flow. The network of `transport.cpp` with every lane cut to 5 units cannot carry
// what the warehouses supply: no flow exists, which is an outcome of the solve, not an error. A lane into a fourth shop
// the problem does not have is a problem no solve admits: the solve answers an error that holds the refusal, which
// names the breach as values and as the library's message. Where exceptions are enabled, the throwing form is shown
// too, which throws the same refusal; the rest builds with exceptions enabled or not.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     c++ -std=c++11 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/outcomes.cpp target/release/libkahlnws_capi.a -o outcomes
//     ./outcomes

#include <cstddef>
#include <cstdint>
#include <iostream>
#include <vector>

#include "kahlnws.hpp"

int main() {
    // Node w is warehouse w and node 2 + s shop s; lane 3 * w + s leads from warehouse w to shop s.
    const std::vector<std::int64_t> supplies = {20, 30, -10, -25, -15};
    const std::vector<std::size_t> tails = {0, 0, 0, 1, 1, 1};
    const std::vector<std::size_t> heads = {2, 3, 4, 2, 3, 4};
    const std::vector<std::uint64_t> capacities = {20, 20, 20, 20, 20, 20};
    const std::vector<std::int64_t> costs = {8, 6, 10, 9, 12, 13};
    kahlnws::network_simplex solver;

    // Each warehouse can send at most 15 units in all, but warehouse 1 must send 30.
    const std::vector<std::uint64_t> narrow = {5, 5, 5, 5, 5, 5};
    kahlnws::problem cut;
    cut.tails = tails;
    cut.heads = heads;
    cut.capacities = narrow;
    cut.costs = costs;
    cut.supplies = supplies;

    const kahlnws::result<kahlnws::answer<kahlnws::solution>> cut_solved = solver.try_solve(cut);
    if (!cut_solved) {
        std::cerr << "the solve failed: " << cut_solved.error().message() << '\n';
        return 1;
    }
    if (cut_solved->outcome.is_infeasible()) {
        std::cout << "no flow exists\n";
    }

    // The last lane enters node 5, a fourth shop, where the problem has nodes 0 to 4 alone.
    const std::vector<std::size_t> outside_heads = {2, 3, 4, 2, 3, 5};
    kahlnws::problem outside;
    outside.tails = tails;
    outside.heads = outside_heads;
    outside.capacities = capacities;
    outside.costs = costs;
    outside.supplies = supplies;

    const kahlnws::result<kahlnws::answer<kahlnws::solution>> outside_solved = solver.try_solve(outside);
    if (outside_solved) {
        std::cerr << "the problem was not refused\n";
        return 1;
    }
    const kahlnws::error& error = outside_solved.error();
    if (error.kind() != kahlnws::error_kind::refused_problem) {
        std::cerr << "the solve failed: " << error.message() << '\n';
        return 1;
    }
    // The same breach as values: here error.refusal()->head_outside_nodes() answers the lane, the node it enters and
    // the problem's node count, and the accessor of every other kind answers null.
    std::cout << "refused: " << error.message() << '\n';

#if KAHLNWS_EXCEPTIONS
    // The throwing form of the same solve, which exists only where exceptions are enabled.
    try {
        const kahlnws::answer<kahlnws::solution> answer = solver.solve(outside);
        std::cerr << "the problem was not refused, after " << answer.work.pivots << " pivots\n";
        return 1;
    } catch (const kahlnws::problem_error& thrown) {
        std::cout << "thrown: " << thrown.what() << '\n';
    }
#endif
    return 0;
}
