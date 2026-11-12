// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// The answers that are not an optimal flow. The network of `transport.c` with every lane cut to 5 units cannot carry
// what the warehouses supply: no flow exists, which is an outcome of the solve, answered as its own status. A lane
// into a fourth shop the problem does not have is a problem no solve admits: it is refused, and the refusal names the
// breach, as values and as the library's message.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     cc -std=c99 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/outcomes.c target/release/libkahlnws_capi.a -o outcomes
//     ./outcomes

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include "kahlnws.h"

// Node w is warehouse w and node WAREHOUSE_COUNT + s shop s; lane w * SHOP_COUNT + s leads from warehouse w to shop s.
enum {
    WAREHOUSE_COUNT = 2,
    SHOP_COUNT = 3,
    NODE_COUNT = WAREHOUSE_COUNT + SHOP_COUNT,
    LANE_COUNT = WAREHOUSE_COUNT * SHOP_COUNT,
};

// Solves `problem` with `solver` and prints what the solve answered; answers 0 where it answered an outcome or a
// refusal, and 1 where the call itself failed.
static int solve_and_print(kahlnws_solver *solver, const kahlnws_problem *problem) {
    uint64_t flows[LANE_COUNT];
    kahlnws_refusal refusal;
    const kahlnws_status status = kahlnws_solver_solve(solver, problem, flows, NULL, NULL, NULL, &refusal);

    switch (status) {
    case KAHLNWS_STATUS_OPTIMAL:
        printf("a flow exists\n");
        return 0;
    case KAHLNWS_STATUS_INFEASIBLE:
        printf("no flow exists\n");
        return 0;
    case KAHLNWS_STATUS_REFUSED: {
        // A message longer than the buffer would be cut; the length answered says how long it is.
        char message[256];
        const size_t length = kahlnws_refusal_message(&refusal, message, sizeof message);
        if (length >= sizeof message) {
            fprintf(stderr, "the message of %zu bytes was cut\n", length);
        }
        printf("refused: %s\n", message);
        // The same breach as values: here its kind is KAHLNWS_REFUSAL_KIND_HEAD_OUTSIDE_NODES, and refusal.arc,
        // refusal.node and refusal.node_count hold the lane, the node it enters and the problem's node count.
        return 0;
    }
    default:
        fprintf(stderr, "the solve answered %s\n", kahlnws_status_name(status));
        return 1;
    }
}

int main(void) {
    const int64_t supplies[NODE_COUNT] = {20, 30, -10, -25, -15};
    const size_t tails[LANE_COUNT] = {0, 0, 0, 1, 1, 1};
    const size_t heads[LANE_COUNT] = {2, 3, 4, 2, 3, 4};
    const int64_t costs[LANE_COUNT] = {8, 6, 10, 9, 12, 13};

    // Each warehouse can send at most 15 units in all, but warehouse 1 must send 30.
    const uint64_t narrow[LANE_COUNT] = {5, 5, 5, 5, 5, 5};
    const kahlnws_problem cut = {.node_count = NODE_COUNT,
                                 .arc_count = LANE_COUNT,
                                 .tails = tails,
                                 .heads = heads,
                                 .capacities = narrow,
                                 .costs = costs,
                                 .supplies = supplies};

    // The last lane enters node 5, a fourth shop, where the problem has nodes 0 to 4 alone.
    const size_t outside_heads[LANE_COUNT] = {2, 3, 4, 2, 3, 5};
    const uint64_t capacities[LANE_COUNT] = {20, 20, 20, 20, 20, 20};
    const kahlnws_problem outside = {.node_count = NODE_COUNT,
                                     .arc_count = LANE_COUNT,
                                     .tails = tails,
                                     .heads = outside_heads,
                                     .capacities = capacities,
                                     .costs = costs,
                                     .supplies = supplies};

    kahlnws_solver *solver = kahlnws_solver_new();
    if (solver == NULL) {
        fprintf(stderr, "the memory for a solver was refused\n");
        return 1;
    }

    int exit_code = solve_and_print(solver, &cut);
    if (exit_code == 0) {
        exit_code = solve_and_print(solver, &outside);
    }

    kahlnws_solver_free(solver);
    return exit_code;
}
