#!/bin/sh
# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

# The gates a change passes before it is committed, run from the repository root: `./gates.sh`. Each cargo command
# covers the whole workspace and builds on at most `KAHLNWS_JOBS` cores (4 unless set), one command at a time.
set -eu

jobs="${KAHLNWS_JOBS:-4}"

cargo fmt --all --check
cargo clippy -j "$jobs" --locked --workspace --all-targets -- -D warnings
RUSTFLAGS="-D warnings" cargo test -j "$jobs" --locked --workspace -- --test-threads="$jobs"
RUSTDOCFLAGS="-D warnings" cargo doc -j "$jobs" --locked --workspace --no-deps
# The bench smoke run: every benchmark once, untimed, after its checks.
cargo test -j "$jobs" --locked -p kahlnws --benches
