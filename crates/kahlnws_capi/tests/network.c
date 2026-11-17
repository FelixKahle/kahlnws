// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// The depot held by a network, from C through the static library: a depot (node 0) sends 4 units to a customer
// (node 2), directly or through a hub (node 1). The network loads it, solves it with its work and reads its optimum;
// solves it again with no pivot; changes a cost, which ends the optimum, solves the changed problem and sets the cost
// back; switches the hub's arc off, which leaves no flow, and on again; refuses a lower bound above a capacity with
// its message, changing nothing; and copies itself, the copy changing apart from it. Every output is left without
// values before the call that writes it. The test `header.rs` builds and runs it; it exits with 0 where every result
// is the one worked by hand, and otherwise names each that is not.

#include <stdio.h>
#include <string.h>

#include "kahlnws.h"

/// Prints `what` and answers 1 where `holds` is false; answers 0 otherwise.
static int failed(int holds, const char *what) {
    if (!holds) {
        fprintf(stderr, "network: %s\n", what);
    }
    return !holds;
}

/// 1 where `cost` is the value `expected`, at least zero and below 2^63, 0 otherwise.
static int costs(kahlnws_int128 cost, uint64_t expected) {
    return cost.high == 0 && cost.low == expected;
}

/// 1 where `network` answers `status` to a solve and, where it is optimal, the cost `expected`; 0 otherwise. The work
/// goes to `work`.
static int solves_to(kahlnws_network *network, kahlnws_status status, uint64_t expected, kahlnws_work *work) {
    kahlnws_int128 total_cost;
    const kahlnws_status solved = kahlnws_network_solve(network, &total_cost, work);
    return solved == status && (status != KAHLNWS_STATUS_OPTIMAL || costs(total_cost, expected));
}

int main(void) {
    const size_t tails[] = {0, 0, 1};
    const size_t heads[] = {2, 1, 2};
    const uint64_t capacities[] = {3, 4, 2};
    const int64_t costs_of_arcs[] = {5, 1, 1};
    const int64_t supplies[] = {4, 0, -4};
    const kahlnws_problem problem = {3, 3, tails, heads, NULL, capacities, costs_of_arcs, supplies};

    uint64_t flows[3];
    int64_t potentials[3];
    kahlnws_int128 total_cost;
    kahlnws_work work;
    kahlnws_refusal refusal;
    int64_t previous_cost;
    uint64_t previous_capacity;
    uint64_t flow;
    char message[64];
    int failures = 0;

    kahlnws_network *network = kahlnws_network_new();
    if (failed(network != NULL, "the network should be made")) {
        return 1;
    }
    failures += failed(kahlnws_network_reserve(network, 3, 3) == KAHLNWS_STATUS_OK, "the room should be made");
    failures += failed(kahlnws_network_load(network, &problem, &refusal) == KAHLNWS_STATUS_OK, "it should load");
    failures += failed(kahlnws_network_node_count(network) == 3 && kahlnws_network_arc_count(network) == 3,
                       "the network should hold 3 nodes and 3 arcs");
    failures += failed(kahlnws_network_total_cost(network, &total_cost) == KAHLNWS_STATUS_NO_OPTIMUM,
                       "a network loaded and not solved should hold no optimum");

    // From the star, with no arcs tried first: four pivots, one of them degenerate, as a solver's solve of the depot.
    failures += failed(solves_to(network, KAHLNWS_STATUS_OPTIMAL, 14, &work), "the depot should be solved at 14");
    failures += failed(work.pivots == 4 && work.degenerate_pivots == 1 && work.first_arc_pivots == 0,
                       "the depot should take four pivots, one of them degenerate");
    kahlnws_status status = kahlnws_network_flows(network, flows);
    failures += failed(status == KAHLNWS_STATUS_OK && flows[0] == 2 && flows[1] == 2 && flows[2] == 2,
                       "the flows should be 2, 2, 2");
    status = kahlnws_network_potentials(network, potentials);
    failures += failed(status == KAHLNWS_STATUS_OK && potentials[0] == 0 && potentials[1] == 1 &&
                           potentials[2] == 5,
                       "the potentials should be 0, 1, 5");
    status = kahlnws_network_flow(network, 1, &flow);
    failures += failed(status == KAHLNWS_STATUS_OK && flow == 2, "arc 1 should carry 2");
    failures += failed(kahlnws_network_flow(network, 3, &flow) == KAHLNWS_STATUS_INVALID_ARGUMENT,
                       "arc 3 is not below the arc count");

    // Nothing changed: the same optimum, with no pivot.
    failures += failed(solves_to(network, KAHLNWS_STATUS_OPTIMAL, 14, &work) && work.pivots == 0,
                       "a solve with nothing changed should take no pivot");

    // The leg through the hub costs 1 + 3 = 4 a unit: 2 units by it and 2 direct at 5, 18.
    status = kahlnws_network_set_cost(network, 2, 3, &previous_cost, &refusal);
    failures += failed(status == KAHLNWS_STATUS_OK && previous_cost == 1, "arc 2 should have cost 1");
    failures += failed(kahlnws_network_total_cost(network, &total_cost) == KAHLNWS_STATUS_NO_OPTIMUM,
                       "a change should end the optimum");
    failures += failed(solves_to(network, KAHLNWS_STATUS_OPTIMAL, 18, &work), "the changed depot should cost 18");
    status = kahlnws_network_set_cost(network, 2, previous_cost, NULL, NULL);
    failures += failed(status == KAHLNWS_STATUS_OK, "the cost should be set back");
    failures += failed(solves_to(network, KAHLNWS_STATUS_OPTIMAL, 14, &work), "set back, the depot should cost 14");

    // The hub's arc off: the hub is cut off, and only 3 of the 4 units have a way.
    status = kahlnws_network_set_capacity(network, 1, 0, &previous_capacity, &refusal);
    failures += failed(status == KAHLNWS_STATUS_OK && previous_capacity == 4, "arc 1 should have capacity 4");
    failures += failed(solves_to(network, KAHLNWS_STATUS_INFEASIBLE, 0, &work), "no flow should exist");
    failures += failed(kahlnws_network_flows(network, flows) == KAHLNWS_STATUS_NO_OPTIMUM && flows[0] == 2,
                       "a network that found no flow should hold no optimum, and write nothing");
    status = kahlnws_network_set_capacity(network, 1, previous_capacity, NULL, NULL);
    failures += failed(status == KAHLNWS_STATUS_OK, "the arc should be switched on");
    failures += failed(solves_to(network, KAHLNWS_STATUS_OPTIMAL, 14, &work), "on again, the depot should cost 14");

    // A lower bound above the capacity is refused, and changes nothing.
    status = kahlnws_network_set_lower_bound(network, 0, 4, NULL, &refusal);
    failures += failed(status == KAHLNWS_STATUS_REFUSED, "a lower bound above the capacity should be refused");
    failures += failed(refusal.kind == KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY && refusal.arc == 0 &&
                           refusal.lower_bound == 4 && refusal.capacity == 3,
                       "the refusal should name arc 0, its lower bound 4 and its capacity 3");
    const char *expected = "arc 0 has lower bound 4 above its capacity 3";
    size_t length = kahlnws_refusal_message(&refusal, message, sizeof message);
    failures += failed(length == strlen(expected) && strcmp(message, expected) == 0, "the message should be the text");
    status = kahlnws_network_total_cost(network, &total_cost);
    failures += failed(status == KAHLNWS_STATUS_OK && costs(total_cost, 14), "a refusal should keep the optimum");

    // A copy holds the optimum, and changes apart from the network.
    kahlnws_network *copy = kahlnws_network_clone(network);
    failures += failed(copy != NULL, "the copy should be made");
    if (copy != NULL) {
        status = kahlnws_network_move_supply(copy, 2, 0, 1, &refusal);
        failures += failed(status == KAHLNWS_STATUS_OK, "the copy's customer should take one more unit");
        // The tree's arc 0 carries the unit, at 5: 19, with no pivot.
        failures += failed(solves_to(copy, KAHLNWS_STATUS_OPTIMAL, 19, &work) && work.pivots == 0,
                           "the copy should cost 19, with no pivot");
        status = kahlnws_network_total_cost(network, &total_cost);
        failures += failed(status == KAHLNWS_STATUS_OK && costs(total_cost, 14), "the network should stay at 14");
    }

    kahlnws_network_free(copy);
    kahlnws_network_free(network);
    return failures == 0 ? 0 : 1;
}
