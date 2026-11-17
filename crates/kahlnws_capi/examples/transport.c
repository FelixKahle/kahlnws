// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// The smallest useful call: two warehouses with supplies 20 and 30 serve three shops with demands 10, 25 and 15, over a
// lane from every warehouse to every shop that carries at most 20 units. One solve finds the cheapest way to ship; the
// program prints its cost and the flow on each lane.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     cc -std=c99 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/transport.c target/release/libkahlnws_capi.a -o transport
//     ./transport

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
};

int main(void) {
    // A supply is positive, a demand negative.
    const int64_t supplies[NODE_COUNT] = {20, 30, -10, -25, -15};
    const size_t tails[LANE_COUNT] = {0, 0, 0, 1, 1, 1};
    const size_t heads[LANE_COUNT] = {2, 3, 4, 2, 3, 4};
    const uint64_t capacities[LANE_COUNT] = {20, 20, 20, 20, 20, 20};
    const int64_t costs[LANE_COUNT] = {8, 6, 10, 9, 12, 13};
    // Every lower bound is zero, so `lower_bounds` stays null.
    const kahlnws_problem problem = {.node_count = NODE_COUNT,
                                     .arc_count = LANE_COUNT,
                                     .tails = tails,
                                     .heads = heads,
                                     .capacities = capacities,
                                     .costs = costs,
                                     .supplies = supplies};

    kahlnws_solver *solver = kahlnws_solver_new();
    if (solver == NULL) {
        fprintf(stderr, "the memory for a solver was refused\n");
        return 1;
    }

    uint64_t flows[LANE_COUNT];
    kahlnws_int128 total_cost;
    const kahlnws_status status = kahlnws_solver_solve(solver, &problem, flows, NULL, &total_cost, NULL, NULL);
    kahlnws_solver_free(solver);

    if (status != KAHLNWS_STATUS_OPTIMAL) {
        fprintf(stderr, "the solve answered %s\n", kahlnws_status_name(status));
        return 1;
    }
    // The cost in decimal, whatever its size: 41 bytes hold every 128-bit value.
    char cost[41];
    kahlnws_int128_format(total_cost, cost, sizeof cost);
    printf("cost: %s\n", cost);
    for (size_t warehouse = 0; warehouse < WAREHOUSE_COUNT; warehouse++) {
        for (size_t shop = 0; shop < SHOP_COUNT; shop++) {
            printf("warehouse %zu -> shop %zu: %" PRIu64 "\n", warehouse, shop, flows[warehouse * SHOP_COUNT + shop]);
        }
    }
    return 0;
}
