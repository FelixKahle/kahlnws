// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// One solver solving again: the network of `transport.c`, for the three days of `reuse.c`, whose shops ask for
// different amounts while the warehouses' supplies stay 20 and 30. Day 1 is solved with kahlnws_solver_solve; each
// later day with kahlnws_solver_resolve, which starts from the basis the solver kept from the day before. The program
// prints each day's cost and its pivots, beside the pivots a second solver, which solves every day afresh, makes.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     cc -std=c99 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/resolve.c target/release/libkahlnws_capi.a -o resolve
//     ./resolve

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

    // The solver that keeps each day's basis, and one that solves each day from the problem alone.
    kahlnws_solver *solver = kahlnws_solver_new();
    kahlnws_solver *fresh = kahlnws_solver_new();
    if (solver == NULL || fresh == NULL) {
        fprintf(stderr, "the memory for a solver was refused\n");
        kahlnws_solver_free(solver);
        kahlnws_solver_free(fresh);
        return 1;
    }

    uint64_t flows[LANE_COUNT];
    int exit_code = 0;
    for (size_t day = 0; day < DAY_COUNT && exit_code == 0; day++) {
        problem.supplies = supplies[day];
        kahlnws_int128 total_cost;
        kahlnws_work work;
        kahlnws_work fresh_work;
        const kahlnws_status status =
            day == 0 ? kahlnws_solver_solve(solver, &problem, flows, NULL, &total_cost, &work, NULL)
                     : kahlnws_solver_resolve(solver, &problem, flows, NULL, &total_cost, &work, NULL);
        const kahlnws_status fresh_status = kahlnws_solver_solve(fresh, &problem, flows, NULL, NULL, &fresh_work, NULL);

        // Every cost here is at least zero, so the total is too, and it fits 64 bits exactly where its high half is 0.
        if (status != KAHLNWS_STATUS_OPTIMAL || fresh_status != KAHLNWS_STATUS_OPTIMAL) {
            fprintf(stderr, "day %zu: the solves answered %s and %s\n", day + 1, kahlnws_status_name(status),
                    kahlnws_status_name(fresh_status));
            exit_code = 1;
        } else if (total_cost.high != 0) {
            fprintf(stderr, "day %zu: the cost passes 64 bits\n", day + 1);
            exit_code = 1;
        } else {
            printf("day %zu: cost %" PRIu64 ", pivots %" PRIu64 " (a fresh solve: %" PRIu64 ")\n", day + 1,
                   total_cost.low, work.pivots, fresh_work.pivots);
        }
    }

    kahlnws_solver_free(solver);
    kahlnws_solver_free(fresh);
    return exit_code;
}
