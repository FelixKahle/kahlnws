#!/bin/sh
# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

# The gates a change passes before it is committed, run from the repository root: `./gates.sh`. Each cargo command
# covers the whole workspace, one command at a time. `KAHLNWS_JOBS` (4 unless set) bounds cargo's build jobs and the
# test threads, each on its own: it is cargo's own setting, so it bounds as well the jobs of the cargo that the tests
# of the C boundary start to build the C library, which runs beside the other test threads.
set -eu

export CARGO_BUILD_JOBS="${KAHLNWS_JOBS:-4}"

cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
RUSTFLAGS="-D warnings" cargo test --locked --workspace -- --test-threads="$CARGO_BUILD_JOBS"
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
# The bench smoke run: every benchmark once, untimed, after its checks.
cargo test --locked -p kahlnws --benches
