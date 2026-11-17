// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// The declarations of the header as a C or C++ program sees them. Each entry is taken as a pointer of the exact
// function type the interface promises, and each constant as a case label, so this file compiles only where the
// header declares every entry with that type and every constant as an integer constant. The test `header.rs`
// compiles it, without linking, in every language standard the header promises.

#include "kahlnws.h"

static kahlnws_solver *(*const solver_new)(void) = kahlnws_solver_new;
static void (*const solver_free)(kahlnws_solver *) = kahlnws_solver_free;
static kahlnws_status (*const solver_reserve)(kahlnws_solver *, size_t, size_t) = kahlnws_solver_reserve;
static kahlnws_status (*const solver_solve)(kahlnws_solver *, const kahlnws_problem *, uint64_t *, int64_t *,
                                            kahlnws_int128 *, kahlnws_work *, kahlnws_refusal *) = kahlnws_solver_solve;
static kahlnws_network *(*const network_new)(void) = kahlnws_network_new;
static kahlnws_network *(*const network_clone)(const kahlnws_network *) = kahlnws_network_clone;
static kahlnws_status (*const network_copy)(kahlnws_network *, const kahlnws_network *) = kahlnws_network_copy;
static void (*const network_free)(kahlnws_network *) = kahlnws_network_free;
static kahlnws_status (*const network_reserve)(kahlnws_network *, size_t, size_t) = kahlnws_network_reserve;
static kahlnws_status (*const network_load)(kahlnws_network *, const kahlnws_problem *, kahlnws_refusal *) =
    kahlnws_network_load;
static size_t (*const network_node_count)(const kahlnws_network *) = kahlnws_network_node_count;
static size_t (*const network_arc_count)(const kahlnws_network *) = kahlnws_network_arc_count;
static kahlnws_status (*const network_set_cost)(kahlnws_network *, size_t, int64_t, int64_t *, kahlnws_refusal *) =
    kahlnws_network_set_cost;
static kahlnws_status (*const network_set_capacity)(kahlnws_network *, size_t, uint64_t, uint64_t *,
                                                    kahlnws_refusal *) = kahlnws_network_set_capacity;
static kahlnws_status (*const network_set_lower_bound)(kahlnws_network *, size_t, uint64_t, uint64_t *,
                                                       kahlnws_refusal *) = kahlnws_network_set_lower_bound;
static kahlnws_status (*const network_set_bounds)(kahlnws_network *, size_t, uint64_t, uint64_t, uint64_t *,
                                                  uint64_t *, kahlnws_refusal *) = kahlnws_network_set_bounds;
static kahlnws_status (*const network_set_supply)(kahlnws_network *, size_t, int64_t, size_t, int64_t *,
                                                  kahlnws_refusal *) = kahlnws_network_set_supply;
static kahlnws_status (*const network_move_supply)(kahlnws_network *, size_t, size_t, uint64_t, kahlnws_refusal *) =
    kahlnws_network_move_supply;
static kahlnws_status (*const network_solve)(kahlnws_network *, kahlnws_int128 *, kahlnws_work *) =
    kahlnws_network_solve;
static kahlnws_status (*const network_total_cost)(const kahlnws_network *, kahlnws_int128 *) =
    kahlnws_network_total_cost;
static kahlnws_status (*const network_flow)(const kahlnws_network *, size_t, uint64_t *) = kahlnws_network_flow;
static kahlnws_status (*const network_flows)(const kahlnws_network *, uint64_t *) = kahlnws_network_flows;
static kahlnws_status (*const network_potentials)(const kahlnws_network *, int64_t *) = kahlnws_network_potentials;
static kahlnws_status (*const network_tail)(const kahlnws_network *, size_t, size_t *) = kahlnws_network_tail;
static kahlnws_status (*const network_head)(const kahlnws_network *, size_t, size_t *) = kahlnws_network_head;
static kahlnws_status (*const network_cost)(const kahlnws_network *, size_t, int64_t *) = kahlnws_network_cost;
static kahlnws_status (*const network_lower_bound)(const kahlnws_network *, size_t, uint64_t *) =
    kahlnws_network_lower_bound;
static kahlnws_status (*const network_capacity)(const kahlnws_network *, size_t, uint64_t *) =
    kahlnws_network_capacity;
static kahlnws_status (*const network_bounds)(const kahlnws_network *, size_t, uint64_t *, uint64_t *) =
    kahlnws_network_bounds;
static kahlnws_status (*const network_supply)(const kahlnws_network *, size_t, int64_t *) = kahlnws_network_supply;
static kahlnws_status (*const network_tails)(const kahlnws_network *, size_t *) = kahlnws_network_tails;
static kahlnws_status (*const network_heads)(const kahlnws_network *, size_t *) = kahlnws_network_heads;
static kahlnws_status (*const network_costs)(const kahlnws_network *, int64_t *) = kahlnws_network_costs;
static kahlnws_status (*const network_lower_bounds)(const kahlnws_network *, uint64_t *) = kahlnws_network_lower_bounds;
static kahlnws_status (*const network_capacities)(const kahlnws_network *, uint64_t *) = kahlnws_network_capacities;
static kahlnws_status (*const network_supplies)(const kahlnws_network *, int64_t *) = kahlnws_network_supplies;
static size_t (*const refusal_message)(const kahlnws_refusal *, char *, size_t) = kahlnws_refusal_message;
static size_t (*const int128_format)(kahlnws_int128, char *, size_t) = kahlnws_int128_format;
static size_t (*const uint128_format)(kahlnws_uint128, char *, size_t) = kahlnws_uint128_format;
static const char *(*const status_name)(kahlnws_status) = kahlnws_status_name;
static int64_t (*const cost_bound)(size_t) = kahlnws_cost_bound;
static uint32_t (*const version)(void) = kahlnws_version;

/// 1 for every status the header names, 0 for any other number.
static int is_status(kahlnws_status status) {
    switch (status) {
    case KAHLNWS_STATUS_OPTIMAL:
    case KAHLNWS_STATUS_INFEASIBLE:
    case KAHLNWS_STATUS_OK:
    case KAHLNWS_STATUS_REFUSED:
    case KAHLNWS_STATUS_NO_OPTIMUM:
    case KAHLNWS_STATUS_NULL_POINTER:
    case KAHLNWS_STATUS_INVALID_ARGUMENT:
    case KAHLNWS_STATUS_NO_ROOM:
    case KAHLNWS_STATUS_INTERNAL_ERROR:
        return 1;
    default:
        return 0;
    }
}

/// 1 for every refusal kind the header names, 0 for any other number.
static int is_refusal_kind(kahlnws_refusal_kind kind) {
    switch (kind) {
    case KAHLNWS_REFUSAL_KIND_TOO_LARGE:
    case KAHLNWS_REFUSAL_KIND_TAIL_OUTSIDE_NODES:
    case KAHLNWS_REFUSAL_KIND_HEAD_OUTSIDE_NODES:
    case KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY:
    case KAHLNWS_REFUSAL_KIND_COST_BEYOND_BOUND:
    case KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE:
    case KAHLNWS_REFUSAL_KIND_UNBALANCED:
    case KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE:
    case KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE:
    case KAHLNWS_REFUSAL_KIND_SUPPLY_BEYOND_RANGE:
        return 1;
    default:
        return 0;
    }
}

int main(void) {
    int entries = solver_new != 0 && solver_free != 0 && solver_reserve != 0 && solver_solve != 0 &&
                  refusal_message != 0 && int128_format != 0 && uint128_format != 0 && status_name != 0 &&
                  cost_bound != 0 && version != 0;
    int network_entries = network_new != 0 && network_clone != 0 && network_free != 0 && network_reserve != 0 &&
                          network_load != 0 && network_node_count != 0 && network_arc_count != 0 &&
                          network_set_cost != 0 && network_set_capacity != 0 && network_set_lower_bound != 0 &&
                          network_set_bounds != 0 && network_set_supply != 0 && network_move_supply != 0 &&
                          network_solve != 0 && network_total_cost != 0 && network_flow != 0 &&
                          network_flows != 0 && network_potentials != 0 && network_copy != 0;
    int problem_reads = network_tail != 0 && network_head != 0 && network_cost != 0 && network_lower_bound != 0 &&
                        network_capacity != 0 && network_bounds != 0 && network_supply != 0 && network_tails != 0 &&
                        network_heads != 0 && network_costs != 0 && network_lower_bounds != 0 &&
                        network_capacities != 0 && network_supplies != 0;
    int versions = KAHLNWS_VERSION == version();

    return entries && network_entries && problem_reads && versions && is_status(KAHLNWS_STATUS_OPTIMAL) &&
                   is_refusal_kind(KAHLNWS_REFUSAL_KIND_UNBALANCED)
               ? 0
               : 1;
}
