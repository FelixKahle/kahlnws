// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// Everything an optimal answer holds beside the flow, on the network of `transport.c`: the potential of each node; the
// reduced cost of each lane computed from them, its cost plus the potential of its warehouse less that of its shop;
// the optimality conditions those prove, checked lane by lane (a lane with room left has a reduced cost of at least
// zero, and a lane that carries flow one of at most zero); and the work the solve did, counted in pivots.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     cc -std=c99 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/potentials.c target/release/libkahlnws_capi.a -o potentials
//     ./potentials

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
    const int64_t supplies[NODE_COUNT] = {20, 30, -10, -25, -15};
    const size_t tails[LANE_COUNT] = {0, 0, 0, 1, 1, 1};
    const size_t heads[LANE_COUNT] = {2, 3, 4, 2, 3, 4};
    const uint64_t capacities[LANE_COUNT] = {20, 20, 20, 20, 20, 20};
    const int64_t costs[LANE_COUNT] = {8, 6, 10, 9, 12, 13};
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
    int64_t potentials[NODE_COUNT];
    kahlnws_int128 total_cost;
    kahlnws_work work;
    const kahlnws_status status = kahlnws_solver_solve(solver, &problem, flows, potentials, &total_cost, &work, NULL);
    kahlnws_solver_free(solver);

    if (status != KAHLNWS_STATUS_OPTIMAL) {
        fprintf(stderr, "the solve answered %s\n", kahlnws_status_name(status));
        return 1;
    }
    // Every cost here is at least zero, so the total is too, and it fits 64 bits exactly where its high half is 0.
    if (total_cost.high != 0) {
        fprintf(stderr, "the cost passes 64 bits\n");
        return 1;
    }
    printf("cost: %" PRIu64 "\n", total_cost.low);

    for (size_t warehouse = 0; warehouse < WAREHOUSE_COUNT; warehouse++) {
        printf("potential of warehouse %zu: %" PRId64 "\n", warehouse, potentials[warehouse]);
    }
    for (size_t shop = 0; shop < SHOP_COUNT; shop++) {
        printf("potential of shop %zu: %" PRId64 "\n", shop, potentials[WAREHOUSE_COUNT + shop]);
    }

    int conditions_hold = 1;
    for (size_t warehouse = 0; warehouse < WAREHOUSE_COUNT; warehouse++) {
        for (size_t shop = 0; shop < SHOP_COUNT; shop++) {
            const size_t lane = warehouse * SHOP_COUNT + shop;
            // A problem the solve admits keeps every potential and reduced cost, and this sum on its way, within 64
            // bits.
            const int64_t reduced_cost = costs[lane] + potentials[tails[lane]] - potentials[heads[lane]];
            printf("reduced cost of warehouse %zu -> shop %zu: %" PRId64 "\n", warehouse, shop, reduced_cost);

            const int has_room = flows[lane] < capacities[lane];
            const int carries_flow = flows[lane] > 0;
            if ((has_room && reduced_cost < 0) || (carries_flow && reduced_cost > 0)) {
                conditions_hold = 0;
            }
        }
    }
    printf("the optimality conditions %s\n", conditions_hold ? "hold" : "fail");

    printf("pivots: %" PRIu64 "\n", work.pivots);
    printf("degenerate pivots: %" PRIu64 "\n", work.degenerate_pivots);
    printf("first arc pivots: %" PRIu64 "\n", work.first_arc_pivots);
    return conditions_hold ? 0 : 1;
}
