#!/bin/sh
# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

# The checks of the Python package, run from anywhere: `crates/kahlnws_python/check.sh`. It builds the release wheel
# from the locked dependencies, checks that it carries the two licence texts of the workspace byte for byte, makes two
# fresh environments with uv under this crate's `.venv/` (one with NumPy and the tools, one with no more than pytest),
# installs the wheel into both, and runs the format check, the lint, the strict type check and the check of the stubs
# against the built module, then the tests in each environment: in the first every test, in the second every test
# that needs no NumPy and the one that proves it absent.
#
# It needs uv, a Rust toolchain and a Python interpreter of at least 3.11, which `KAHLNWS_PYTHON` names (`python3`
# unless set). The tools are pinned, so that a check means the same on every machine and every day; NumPy is the
# newest release for the interpreter from 3.12 on, and on 3.11 the lowest the package's extra promises.
set -eu

crate="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$crate/../.." && pwd)"
python="${KAHLNWS_PYTHON:-python3}"
work="$crate/.venv"
tools="pytest==9.2.0 mypy==2.4.1 ruff==0.17.4"

rm -rf "$work/wheels" "$work/numpy" "$work/plain"

uvx maturin==1.16.0 build --quiet --release --locked --manifest-path "$crate/Cargo.toml" --out "$work/wheels"
wheel="$(ls "$work"/wheels/kahlnws-*.whl)"
for licence in LICENSE-MIT LICENSE-APACHE; do
    unzip -p "$wheel" "kahlnws-*.dist-info/licenses/$licence" | cmp - "$root/$licence"
done

uv venv --quiet --python "$python" "$work/numpy"
# shellcheck disable=SC2086 # the tools are one word each
uv pip install --quiet --python "$work/numpy/bin/python" "$wheel[numpy]" \
    "numpy==2.5.4; python_version >= '3.12'" "numpy==2.4.6; python_version < '3.12'" $tools
uv venv --quiet --python "$python" "$work/plain"
uv pip install --quiet --python "$work/plain/bin/python" "$wheel" pytest==9.2.0

cd "$crate"
"$work/numpy/bin/python" -m ruff format --check
"$work/numpy/bin/python" -m ruff check
"$work/numpy/bin/python" -m mypy
"$work/numpy/bin/python" -m mypy.stubtest kahlnws
"$work/numpy/bin/python" -m pytest
"$work/plain/bin/python" -m pytest
