// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// A network held over three days: the network of `transport.c`, for the three days of `reuse.c`, whose shops ask for
// different amounts while the warehouses' supplies stay 20 and 30. Day 1 is loaded into a network that held nothing
// and solved; each later day is loaded with that day's numbers, on the same lanes, and solved from the basis the
// network kept. The program prints each day's cost and the pivots its solve made, beside the pivots of a fresh solve
// of the same day by the solver, which keeps nothing from one day to the next, and checks each line: the costs worked
// by hand, the pivots the library's own counts, pinned.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     cc -std=c99 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/network.c target/release/libkahlnws_capi.a -o network
//     ./network

#include <inttypes.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

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

    // Each day's cost, worked by hand: a flow costs what warehouse 1 would pay to serve every shop alone, less what
    // each unit warehouse 0 sends in its place saves, 1, 6 and 3 at shops 0, 1 and 2. Beside it, the pivots of the
    // network's solve and of a fresh one, which are not worked by hand: they are the counts this library's solves make,
    // pinned so that the example shows a later day, started from the basis the day before ended on, taking far fewer.
    const char *const days[DAY_COUNT] = {
        // Warehouse 1 alone: 9 * 10 + 12 * 25 + 13 * 15 = 585; warehouse 0's 20 units save 6 each at shop 1: 465.
        "day 1: cost 465, pivots 6 (a fresh solve: 6)",
        // Warehouse 1 alone: 180 + 240 + 130 = 550; warehouse 0's 20 units save 6 each at shop 1: 430.
        "day 2: cost 430, pivots 1 (a fresh solve: 5)",
        // Warehouse 1 alone: 45 + 180 + 390 = 615; its lane carries 20 of shop 2's 30, so warehouse 0 sends the other
        // 10 there, saving 3 each, and its last 10 to shop 1, saving 6 each: 525.
        "day 3: cost 525, pivots 1 (a fresh solve: 7)",
    };

    // The network that keeps each day's basis, and the solver that solves each day from nothing kept.
    kahlnws_network *network = kahlnws_network_new();
    kahlnws_solver *fresh = kahlnws_solver_new();
    if (network == NULL || fresh == NULL) {
        fprintf(stderr, "the memory for a network or a solver was refused\n");
        kahlnws_network_free(network);
        kahlnws_solver_free(fresh);
        return 1;
    }

    uint64_t flows[LANE_COUNT];
    int exit_code = 0;
    for (size_t day = 0; day < DAY_COUNT && exit_code == 0; day++) {
        problem.supplies = supplies[day];
        kahlnws_int128 total_cost = {0, 0};
        kahlnws_work work = {0, 0, 0};
        kahlnws_work fresh_work = {0, 0, 0};
        const kahlnws_status loaded = kahlnws_network_load(network, &problem, NULL);
        const kahlnws_status status =
            loaded == KAHLNWS_STATUS_OK ? kahlnws_network_solve(network, &total_cost, &work) : loaded;
        const kahlnws_status fresh_status = kahlnws_solver_solve(fresh, &problem, flows, NULL, NULL, &fresh_work, NULL);
        if (status != KAHLNWS_STATUS_OPTIMAL || fresh_status != KAHLNWS_STATUS_OPTIMAL) {
            fprintf(stderr, "day %zu: the solves answered %s and %s\n", day + 1, kahlnws_status_name(status),
                    kahlnws_status_name(fresh_status));
            exit_code = 1;
            break;
        }

        // The cost in decimal, whatever its size: 41 bytes hold every 128-bit value.
        char cost[41];
        char line[128];
        kahlnws_int128_format(total_cost, cost, sizeof cost);
        snprintf(line, sizeof line, "day %zu: cost %s, pivots %" PRIu64 " (a fresh solve: %" PRIu64 ")", day + 1, cost,
                 work.pivots, fresh_work.pivots);
        printf("%s\n", line);
        if (strcmp(line, days[day]) != 0) {
            fprintf(stderr, "day %zu should print \"%s\"\n", day + 1, days[day]);
            exit_code = 1;
        }
    }

    kahlnws_network_free(network);
    kahlnws_solver_free(fresh);
    return exit_code;
}
