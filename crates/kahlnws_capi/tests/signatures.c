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
static kahlnws_status (*const solver_resolve)(kahlnws_solver *, const kahlnws_problem *, uint64_t *, int64_t *,
                                              kahlnws_int128 *, kahlnws_work *, kahlnws_refusal *) =
    kahlnws_solver_resolve;
static size_t (*const refusal_message)(const kahlnws_refusal *, char *, size_t) = kahlnws_refusal_message;
static const char *(*const status_name)(kahlnws_status) = kahlnws_status_name;
static int64_t (*const cost_bound)(size_t) = kahlnws_cost_bound;
static uint32_t (*const version)(void) = kahlnws_version;

/// 1 for every status the header names, 0 for any other number.
static int is_status(kahlnws_status status) {
    switch (status) {
    case KAHLNWS_STATUS_OPTIMAL:
    case KAHLNWS_STATUS_INFEASIBLE:
    case KAHLNWS_STATUS_RESERVED:
    case KAHLNWS_STATUS_REFUSED:
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
        return 1;
    default:
        return 0;
    }
}

int main(void) {
    int entries = solver_new != 0 && solver_free != 0 && solver_reserve != 0 && solver_solve != 0 &&
                  solver_resolve != 0 && refusal_message != 0 && status_name != 0 && cost_bound != 0 && version != 0;
    int versions = KAHLNWS_VERSION == version();

    return entries && versions && is_status(KAHLNWS_STATUS_OPTIMAL) && is_refusal_kind(KAHLNWS_REFUSAL_KIND_UNBALANCED)
               ? 0
               : 1;
}
