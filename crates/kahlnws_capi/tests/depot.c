// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// The depot solved from C through the static library: a depot (node 0) sends 4 units to a customer (node 2), directly
// or through a hub (node 1), with the work of the solve, and solved again by the same solver, with the same answer
// and the same work: a solve answers its problem alone. Then the same network with arcs too narrow, which is infeasible
// and has its work, and with a customer who asks for one unit more, which is refused; the version of the library
// against the header's; and the names of statuses. Every output is left without values before the call that writes it.
// The test `header.rs` builds and runs it; it exits with 0 where every result is the one worked by hand, and otherwise
// names each that is not.

#include <stdio.h>
#include <string.h>

#include "kahlnws.h"

/// Prints `what` and answers 1 where `holds` is false; answers 0 otherwise.
static int failed(int holds, const char *what) {
    if (!holds) {
        fprintf(stderr, "depot: %s\n", what);
    }
    return !holds;
}

/// 1 where `work` holds `pivots`, `degenerate_pivots` and `first_arc_pivots`, 0 otherwise.
static int counted(kahlnws_work work, uint64_t pivots, uint64_t degenerate_pivots, uint64_t first_arc_pivots) {
    return work.pivots == pivots && work.degenerate_pivots == degenerate_pivots &&
           work.first_arc_pivots == first_arc_pivots;
}

/// 1 where the status name of `status` is `expected`, 0 otherwise.
static int named(kahlnws_status status, const char *expected) {
    return strcmp(kahlnws_status_name(status), expected) == 0;
}

int main(void) {
    const size_t tails[] = {0, 0, 1};
    const size_t heads[] = {2, 1, 2};
    const uint64_t capacities[] = {3, 4, 2};
    const uint64_t narrow[] = {1, 1, 1};
    const int64_t costs[] = {5, 1, 1};
    const int64_t supplies[] = {4, 0, -4};
    const int64_t unbalanced[] = {4, 0, -3};
    const kahlnws_problem problem = {3, 3, tails, heads, NULL, capacities, costs, supplies};

    uint64_t flows[3];
    int64_t potentials[3];
    kahlnws_int128 total_cost;
    kahlnws_work work;
    kahlnws_refusal refusal;
    char message[64];
    int failures = 0;

    kahlnws_solver *solver = kahlnws_solver_new();
    if (failed(solver != NULL, "the solver should be made")) {
        return 1;
    }
    failures += failed(kahlnws_solver_reserve(solver, 3, 3) == KAHLNWS_STATUS_OK, "the room should be made");

    kahlnws_status status = kahlnws_solver_solve(solver, &problem, flows, potentials, &total_cost, &work, &refusal);
    failures += failed(status == KAHLNWS_STATUS_OPTIMAL, "the depot should be solved optimally");
    failures += failed(flows[0] == 2 && flows[1] == 2 && flows[2] == 2, "the flows should be 2, 2, 2");
    int potentials_hold = potentials[0] == 0 && potentials[1] == 1 && potentials[2] == 5;
    failures += failed(potentials_hold, "the potentials should be 0, 1, 5");
    failures += failed(total_cost.low == 14 && total_cost.high == 0, "the cost should be 14");
    failures += failed(counted(work, 4, 1, 0), "the depot should take four pivots, one of them degenerate");

    // The solver keeps room and nothing else: the same problem again makes the same work.
    status = kahlnws_solver_solve(solver, &problem, flows, potentials, &total_cost, &work, &refusal);
    failures += failed(status == KAHLNWS_STATUS_OPTIMAL, "the depot should be solved again optimally");
    failures += failed(total_cost.low == 14 && total_cost.high == 0, "the cost again should be 14");
    failures += failed(counted(work, 4, 1, 0), "the same problem again should take the same four pivots");

    kahlnws_problem infeasible = problem;
    infeasible.capacities = narrow;
    status = kahlnws_solver_solve(solver, &infeasible, flows, potentials, &total_cost, &work, &refusal);
    failures += failed(status == KAHLNWS_STATUS_INFEASIBLE, "the narrow depot should be infeasible");
    failures += failed(flows[0] == 2 && total_cost.low == 14, "an infeasible solve should leave the outputs");
    failures += failed(counted(work, 3, 1, 0), "the narrow depot should take three pivots, one degenerate");

    kahlnws_problem refused = problem;
    refused.supplies = unbalanced;
    status = kahlnws_solver_solve(solver, &refused, flows, potentials, &total_cost, &work, &refusal);
    failures += failed(status == KAHLNWS_STATUS_REFUSED, "the unbalanced depot should be refused");
    failures += failed(counted(work, 3, 1, 0), "a refused solve should leave the work");
    failures += failed(refusal.kind == KAHLNWS_REFUSAL_KIND_UNBALANCED, "the refusal should name the balance");
    failures += failed(refusal.supply_total.low == 1 && refusal.supply_total.high == 0, "the supplies should add to 1");

    const char *expected = "the supplies add up to 1, not zero";
    size_t length = kahlnws_refusal_message(&refusal, message, sizeof message);
    failures += failed(length == strlen(expected) && strcmp(message, expected) == 0, "the message should be the text");
    length = kahlnws_refusal_message(&refusal, message, 8);
    failures += failed(length == strlen(expected) && strcmp(message, "the sup") == 0, "a short buffer is cut");

    failures += failed(kahlnws_version() == KAHLNWS_VERSION, "the library should be the header's version");
    failures += failed(named(KAHLNWS_STATUS_INFEASIBLE, "infeasible"), "a status should have its name");
    failures += failed(named(99, "unknown status"), "an unknown status should be named as such");

    kahlnws_solver_free(solver);
    return failures == 0 ? 0 : 1;
}
