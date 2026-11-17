// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// One solver kept for many solves: the network of `transport.c`, solved for three days whose shops ask for different
// amounts while the warehouses' supplies stay 20 and 30. The solver's room is reserved once, and every day is solved
// into the same buffer the program holds, so no solve allocates. The program prints each day's cost.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     cc -std=c99 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/reuse.c target/release/libkahlnws_capi.a -o reuse
//     ./reuse

#include <inttypes.h>
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
    DAY_COUNT = 3,
};

int main(void) {
    // The supplies of each day: the warehouses' first, then the shops' demands, negative.
    const int64_t supplies[DAY_COUNT][NODE_COUNT] = {
        {20, 30, -10, -25, -15},
        {20, 30, -20, -20, -10},
        {20, 30, -5, -15, -30},
    };
    const size_t tails[LANE_COUNT] = {0, 0, 0, 1, 1, 1};
    const size_t heads[LANE_COUNT] = {2, 3, 4, 2, 3, 4};
    const uint64_t capacities[LANE_COUNT] = {20, 20, 20, 20, 20, 20};
    const int64_t costs[LANE_COUNT] = {8, 6, 10, 9, 12, 13};
    kahlnws_problem problem = {.node_count = NODE_COUNT,
                               .arc_count = LANE_COUNT,
                               .tails = tails,
                               .heads = heads,
                               .capacities = capacities,
                               .costs = costs};

    kahlnws_solver *solver = kahlnws_solver_new();
    if (solver == NULL) {
        fprintf(stderr, "the memory for a solver was refused\n");
        return 1;
    }
    const kahlnws_status reserved = kahlnws_solver_reserve(solver, NODE_COUNT, LANE_COUNT);
    if (reserved != KAHLNWS_STATUS_OK) {
        fprintf(stderr, "the reservation answered %s\n", kahlnws_status_name(reserved));
        kahlnws_solver_free(solver);
        return 1;
    }

    // The buffer every day's flows are written to.
    uint64_t flows[LANE_COUNT];
    int exit_code = 0;
    for (size_t day = 0; day < DAY_COUNT && exit_code == 0; day++) {
        problem.supplies = supplies[day];
        kahlnws_int128 total_cost;
        const kahlnws_status status = kahlnws_solver_solve(solver, &problem, flows, NULL, &total_cost, NULL, NULL);

        if (status != KAHLNWS_STATUS_OPTIMAL) {
            fprintf(stderr, "day %zu: the solve answered %s\n", day + 1, kahlnws_status_name(status));
            exit_code = 1;
        } else {
            // The cost in decimal, whatever its size: 41 bytes hold every 128-bit value.
            char cost[41];
            kahlnws_int128_format(total_cost, cost, sizeof cost);
            printf("day %zu: cost %s\n", day + 1, cost);
        }
    }

    kahlnws_solver_free(solver);
    return exit_code;
}
