// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// A short search over a network held: the network of `transport.c` for its first day, loaded once and solved; then
// five moves, each changing a few numbers, solved, and kept where the cost falls below the best so far or set back
// where it does not: lane 3 switched off by a capacity of zero; lane 1 made cheaper under a contract to carry more than
// it can, a move of two changes whose second is refused; five units of stock moved from warehouse 1 to warehouse 0;
// warehouse 0 set to hold 15 units; and lane 4 made cheaper. A move that is set back is undone by its changes set
// back, each with the number its setter wrote; a refused change changes nothing, so the changes of a move made before
// it are set back the same way. The program prints what each move came to, and the cost of the moves kept with the
// stock of each warehouse read back from the network, and checks each line against the one worked out by hand.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     cc -std=c99 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/search.c target/release/libkahlnws_capi.a -o search
//     ./search

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
    MOVE_COUNT = 5,
    MOST_CHANGES = 2,
    LINE_SIZE = 128,
};

/// Which setter a change calls.
typedef enum { CHANGE_COST, CHANGE_LOWER_BOUND, CHANGE_CAPACITY, CHANGE_SUPPLY, CHANGE_MOVED_SUPPLY } change_kind;

/// A change of the problem a network holds, by its setter: the lane or the nodes, and the number set.
typedef struct {
    change_kind kind;
    union {
        struct {
            size_t lane;
            int64_t cost;
        } cost;
        struct {
            size_t lane;
            uint64_t lower_bound;
        } lower_bound;
        struct {
            size_t lane;
            uint64_t capacity;
        } capacity;
        struct {
            size_t node;
            int64_t supply;
            size_t balancing_node;
        } supply;
        struct {
            size_t from_node;
            size_t to_node;
            uint64_t amount;
        } moved_supply;
    } as;
} change;

/// A move of the search: its changes, made in order, and what it comes to, worked out by hand.
typedef struct {
    change changes[MOST_CHANGES];
    size_t change_count;
    const char *hand_worked;
} move;

/// Makes `wanted` on `network` and writes the change that sets it back to `back`; answers the status of the setter,
/// with the breach of a refused change written to `refusal`.
static kahlnws_status make(kahlnws_network *network, change wanted, change *back, kahlnws_refusal *refusal) {
    *back = wanted;
    switch (wanted.kind) {
    case CHANGE_COST:
        return kahlnws_network_set_cost(network, wanted.as.cost.lane, wanted.as.cost.cost, &back->as.cost.cost,
                                        refusal);
    case CHANGE_LOWER_BOUND:
        return kahlnws_network_set_lower_bound(network, wanted.as.lower_bound.lane, wanted.as.lower_bound.lower_bound,
                                               &back->as.lower_bound.lower_bound, refusal);
    case CHANGE_CAPACITY:
        return kahlnws_network_set_capacity(network, wanted.as.capacity.lane, wanted.as.capacity.capacity,
                                            &back->as.capacity.capacity, refusal);
    case CHANGE_SUPPLY:
        return kahlnws_network_set_supply(network, wanted.as.supply.node, wanted.as.supply.supply,
                                          wanted.as.supply.balancing_node, &back->as.supply.supply, refusal);
    case CHANGE_MOVED_SUPPLY:
        back->as.moved_supply.from_node = wanted.as.moved_supply.to_node;
        back->as.moved_supply.to_node = wanted.as.moved_supply.from_node;
        return kahlnws_network_move_supply(network, wanted.as.moved_supply.from_node, wanted.as.moved_supply.to_node,
                                           wanted.as.moved_supply.amount, refusal);
    }
    return KAHLNWS_STATUS_INVALID_ARGUMENT;
}

/// Sets back the first `made` changes of a move, whose set-backs `backs` holds, the last made first; answers
/// KAHLNWS_STATUS_OK, or the status of the first set-back that was not made.
static kahlnws_status set_back(kahlnws_network *network, const change backs[], size_t made) {
    while (made > 0) {
        made--;
        change again;
        const kahlnws_status status = make(network, backs[made], &again, NULL);
        if (status != KAHLNWS_STATUS_OK) {
            return status;
        }
    }
    return KAHLNWS_STATUS_OK;
}

/// Makes the changes of `wanted` on `network` in order, writing the change that sets each back to `backs`. Where one
/// is refused, sets back those made before it, writes the breach to `refusal` and answers KAHLNWS_STATUS_REFUSED: the
/// network then holds the problem it held. Answers KAHLNWS_STATUS_OK where every change was made, and any other status
/// a change or a set-back answered.
static kahlnws_status make_move(kahlnws_network *network, const move *wanted, change backs[],
                                kahlnws_refusal *refusal) {
    for (size_t made = 0; made < wanted->change_count; made++) {
        const kahlnws_status status = make(network, wanted->changes[made], &backs[made], refusal);
        if (status != KAHLNWS_STATUS_OK) {
            const kahlnws_status undone = set_back(network, backs, made);
            return undone == KAHLNWS_STATUS_OK ? status : undone;
        }
    }
    return KAHLNWS_STATUS_OK;
}

/// Whether `left` is below `right`: their high halves compared as signed numbers, and where those are equal their low
/// halves, as the header says two values compare.
static int is_below(kahlnws_int128 left, kahlnws_int128 right) {
    return left.high < right.high || (left.high == right.high && left.low < right.low);
}

/// Tries `wanted` on `network` and writes what it came to to `line`: kept where its cost is below `best`, which then
/// becomes that cost, and set back otherwise. Answers KAHLNWS_STATUS_OK, or the status of the call that failed.
static kahlnws_status try_move(kahlnws_network *network, const move *wanted, kahlnws_int128 *best, char *line) {
    change backs[MOST_CHANGES];
    kahlnws_refusal refusal;
    const kahlnws_status made = make_move(network, wanted, backs, &refusal);
    if (made == KAHLNWS_STATUS_REFUSED) {
        char message[256];
        kahlnws_refusal_message(&refusal, message, sizeof message);
        snprintf(line, LINE_SIZE, "refused (%s), set back", message);
        return KAHLNWS_STATUS_OK;
    }
    if (made != KAHLNWS_STATUS_OK) {
        return made;
    }

    kahlnws_int128 cost = {0, 0};
    const kahlnws_status solved = kahlnws_network_solve(network, &cost, NULL);
    if (solved == KAHLNWS_STATUS_OPTIMAL && is_below(cost, *best)) {
        *best = cost;
        char text[41];
        kahlnws_int128_format(cost, text, sizeof text);
        snprintf(line, LINE_SIZE, "cost %s, kept", text);
        return KAHLNWS_STATUS_OK;
    }
    if (solved != KAHLNWS_STATUS_OPTIMAL && solved != KAHLNWS_STATUS_INFEASIBLE) {
        return solved;
    }

    const kahlnws_status undone = set_back(network, backs, wanted->change_count);
    if (solved == KAHLNWS_STATUS_INFEASIBLE) {
        snprintf(line, LINE_SIZE, "no flow, set back");
    } else {
        char text[41];
        kahlnws_int128_format(cost, text, sizeof text);
        snprintf(line, LINE_SIZE, "cost %s, set back", text);
    }
    return undone;
}

int main(void) {
    const size_t tails[LANE_COUNT] = {0, 0, 0, 1, 1, 1};
    const size_t heads[LANE_COUNT] = {2, 3, 4, 2, 3, 4};
    const uint64_t capacities[LANE_COUNT] = {20, 20, 20, 20, 20, 20};
    const int64_t costs[LANE_COUNT] = {8, 6, 10, 9, 12, 13};
    const int64_t supplies[NODE_COUNT] = {20, 30, -10, -25, -15};
    const kahlnws_problem problem = {.node_count = NODE_COUNT,
                                     .arc_count = LANE_COUNT,
                                     .tails = tails,
                                     .heads = heads,
                                     .capacities = capacities,
                                     .costs = costs,
                                     .supplies = supplies};

    // By hand: a flow costs what warehouse 1 would pay to serve every shop alone, less what each unit warehouse 0
    // sends in its place saves, 1, 6 and 3 at shops 0, 1 and 2 while the costs are those loaded. Warehouse 1 alone
    // would pay 9 * 10 + 12 * 25 + 13 * 15 = 585; warehouse 0's 20 units save 6 each at shop 1, whose lane from it
    // takes all 20: 465.
    const char *const start = "start: cost 465";
    const move moves[MOVE_COUNT] = {
        // Lane 3 off: warehouse 0 serves shop 0, saving 1 a unit, and has 10 units left for shop 1: 585 - 10 - 60.
        {{{.kind = CHANGE_CAPACITY, .as.capacity = {.lane = 3, .capacity = 0}}}, 1, "cost 515, set back"},
        // Lane 1 at 4 a unit for at least 25 units: it carries at most 20, so the cost goes back to 6.
        {{{.kind = CHANGE_COST, .as.cost = {.lane = 1, .cost = 4}},
          {.kind = CHANGE_LOWER_BOUND, .as.lower_bound = {.lane = 1, .lower_bound = 25}}},
         2,
         "refused (arc 1 has lower bound 25 above its capacity 20), set back"},
        // Warehouse 0 holds 25: 20 to shop 1 save 6 each and 5 to shop 2 save 3 each: 585 - 120 - 15.
        {{{.kind = CHANGE_MOVED_SUPPLY, .as.moved_supply = {.from_node = 1, .to_node = 0, .amount = 5}}},
         1,
         "cost 450, kept"},
        // Warehouse 0 to hold 15, warehouse 1 the other 35: all 15 to shop 1 save 6 each: 585 - 90.
        {{{.kind = CHANGE_SUPPLY, .as.supply = {.node = 0, .supply = 15, .balancing_node = 1}}},
         1,
         "cost 495, set back"},
        // Lane 4 at 7: warehouse 1 alone pays 460; 15 units to shop 2 save 3 each, the other 10 1 each: 460 - 55.
        {{{.kind = CHANGE_COST, .as.cost = {.lane = 4, .cost = 7}}}, 1, "cost 405, kept"},
    };
    // The moves kept: warehouse 0 holds 25 and warehouse 1 25, lane 4 costs 7, every other number is as loaded.
    const char *const end = "end: cost 405, the warehouses hold 25 and 25";

    kahlnws_network *network = kahlnws_network_new();
    if (network == NULL) {
        fprintf(stderr, "the memory for a network was refused\n");
        return 1;
    }

    char line[LINE_SIZE];
    char text[41];
    kahlnws_int128 best = {0, 0};
    if (kahlnws_network_load(network, &problem, NULL) != KAHLNWS_STATUS_OK ||
        kahlnws_network_solve(network, &best, NULL) != KAHLNWS_STATUS_OPTIMAL) {
        fprintf(stderr, "the first day should load and solve\n");
        kahlnws_network_free(network);
        return 1;
    }
    kahlnws_int128_format(best, text, sizeof text);
    snprintf(line, sizeof line, "start: cost %s", text);
    printf("%s\n", line);
    int exit_code = 0;
    if (strcmp(line, start) != 0) {
        fprintf(stderr, "the start should print \"%s\"\n", start);
        exit_code = 1;
    }

    for (size_t number = 0; number < MOVE_COUNT && exit_code == 0; number++) {
        char came_to[LINE_SIZE];
        const kahlnws_status status = try_move(network, &moves[number], &best, came_to);
        if (status != KAHLNWS_STATUS_OK) {
            fprintf(stderr, "move %zu: a call answered %s\n", number + 1, kahlnws_status_name(status));
            exit_code = 1;
            break;
        }
        printf("move %zu: %s\n", number + 1, came_to);
        if (strcmp(came_to, moves[number].hand_worked) != 0) {
            fprintf(stderr, "move %zu should come to \"%s\"\n", number + 1, moves[number].hand_worked);
            exit_code = 1;
        }
    }

    kahlnws_int128 cost = {0, 0};
    int64_t stock_0 = 0;
    int64_t stock_1 = 0;
    if (exit_code == 0 &&
        (kahlnws_network_solve(network, &cost, NULL) != KAHLNWS_STATUS_OPTIMAL ||
         kahlnws_network_supply(network, 0, &stock_0) != KAHLNWS_STATUS_OK ||
         kahlnws_network_supply(network, 1, &stock_1) != KAHLNWS_STATUS_OK)) {
        fprintf(stderr, "the moves kept should leave a flow\n");
        exit_code = 1;
    } else if (exit_code == 0) {
        kahlnws_int128_format(cost, text, sizeof text);
        snprintf(line, sizeof line, "end: cost %s, the warehouses hold %" PRId64 " and %" PRId64, text, stock_0,
                 stock_1);
        printf("%s\n", line);
        if (strcmp(line, end) != 0) {
            fprintf(stderr, "the end should print \"%s\"\n", end);
            exit_code = 1;
        }
    }

    kahlnws_network_free(network);
    return exit_code;
}
