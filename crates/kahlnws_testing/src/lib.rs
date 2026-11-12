// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test infrastructure the tests and benchmarks of the kahlnws workspace share. A dev-dependency only.
//!
//! It speaks plain numbers and depends on no crate of the workspace, so the library's tests can use it without a
//! cycle; the few lines that turn a reference network into the library's problem live in those tests.
//!
//! - A min-cost network as plain data, [`MincostNetwork`], the one input type the models, the oracle and the
//!   generators share, with the [`fingerprint`](MincostNetwork::fingerprint) that pins a generated one and
//!   [`from_arcs`](MincostNetwork::from_arcs), the short form of a hand-written one.
//! - The brute force, [`brute_force_optimum`]: every integral flow of a tiny network tried, answering the least cost,
//!   or that no flow exists; it refuses a network of more than [`BRUTE_FORCE_FLOWS`] flows. A solve's answer, as an
//!   [`OptimalFlow`] or none, is checked against it by [`check_against_brute_force`], a failure named as a
//!   [`BruteForceBreach`].
//! - The certificate, [`check_certificate`]: in `i128`, the flow within its bounds and meeting every supply, the
//!   potentials pricing it optimal, and its cost recomputed; a failure named as a [`CertificateBreach`].
//! - The LP oracle, [`lp_optimum`]: the optimum cost by a linear-programming solver, for networks larger than the
//!   brute force enumerates, within the range of numbers it admits.
//! - The [`CountingAllocator`], read by [`allocations_on_this_thread`], for the tests that claim a call allocates
//!   nothing.
//! - Tiny-network strategies for the property tests: [`TinyMincostNetworks`], and [`tiny_networks`], the ones the
//!   solver's tests draw.
//! - Seeded networks larger than a brute force enumerates, a function of a seed of a few numbers:
//!   [`supply_shaped`], [`assignment_shaped`] and [`general`], their costs signed as [`Signs`] says.
//! - Networks of a size a benchmark names, drawn from seeded generators ([`ShapeDraws`]): [`supply_sized`] and
//!   [`assignment_sized`], whose optimum ties through twin arcs, the [`costed_grid`], and [`distinct_costs`] that
//!   make a network's costs distinct.
//! - The benchmark suite's networks: each [`Family`] at its sizes, drawn from the suite's seeds, and the
//!   [`suite_tie_breaks`] of its distinct rows.
//!
//! Every model is deliberately simple and shares no reasoning with the code it is checked against, and every one has
//! hand-worked tests of its own.

mod brute_force;
mod certificate;
mod counting_allocator;
mod lp_oracle;
mod mincost_network;
mod seeded_networks;
mod seeds;
mod sized_networks;
mod tiny_networks;

pub use brute_force::{
    BRUTE_FORCE_FLOWS, BruteForceBreach, OptimalFlow, brute_force_optimum, check_against_brute_force,
};
pub use certificate::{CertificateBreach, check_certificate};
pub use counting_allocator::{Allocations, CountingAllocator, allocations_on_this_thread};
pub use lp_oracle::lp_optimum;
pub use mincost_network::MincostNetwork;
pub use seeded_networks::{Signs, assignment_shaped, general, supply_shaped};
pub use seeds::{Family, suite_tie_breaks};
pub use sized_networks::{ShapeDraws, assignment_sized, costed_grid, distinct_costs, supply_sized};
pub use tiny_networks::{TinyBounds, TinyCosts, TinyMincostNetworks, TinySupplies, tiny_networks};
