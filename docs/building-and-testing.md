# Building, testing and safety

For a contributor, or anyone who wants to know what has been checked and how: what a build needs, what the gates
run, which kinds of tests there are, how the `unsafe` code is kept sound, and how the tests run under Miri.

## What a build needs

- **Rust 1.99.1**, the `rust-version` of the workspace manifest, with `rustfmt` and `clippy`: every crate, and the
  gates.
- **A Python interpreter of at least 3.11**, `python3` on the path or the one `PYO3_PYTHON` names: the gates build the
  Python crate with the rest of the workspace, and its tests embed the interpreter.
- **A C and a C++ compiler**, `cc` and `c++` or those `CC` and `CXX` name: the tests of the C and C++ headers.
- **uv**: the checks of the Python package.
- **A nightly toolchain with Miri**: running the tests under Miri, apart from the gates.

The linear-programming oracle of the tests is built from bundled sources with cmake, which the testing crate's build
needs too.

Where a C or C++ compiler is not found, a test that needs it says so on standard error and passes; it never passes
without saying.

## The workspace

- `crates/kahlnws`: the library, which depends on no other crate of the workspace.
- `crates/kahlnws_capi`: the C interface, built as a static and a dynamic library, with the headers `kahlnws.h` and
  `kahlnws.hpp`.
- `crates/kahlnws_python`: the Python package, built with maturin.
- `crates/kahlnws_testing`: the models, generators and allocator the tests and the benchmarks share.

Every dependency points down: the C and the Python crate depend on the library; the testing crate depends on none of
the workspace's crates and is a dev-dependency of the other three.

## The gates

A change is ready when the gates pass, run from the root of the repository:

```sh
./gates.sh
```

The steps run in this order; the first four cover the whole workspace, and each builds on at most `KAHLNWS_JOBS`
cores, 4 unless the variable is set:

1. `cargo fmt --all --check`: every source is formatted as `rustfmt` writes it.
2. `cargo clippy --locked --workspace --all-targets -- -D warnings`: no lint fires, in the code, the tests or the
   benchmarks.
3. `RUSTFLAGS="-D warnings" cargo test --locked --workspace`: every test passes, every doctest included, and nothing
   compiles with a warning.
4. `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps`: the documentation builds, every link in it
   resolving.
5. `cargo test --locked -p kahlnws --benches`: every benchmark's checks pass, and each runs once, untimed.

The checks of the Python package run apart, with `crates/kahlnws_python/check.sh`; [the Python guide](python-api.md)
lists what it runs.

On GitHub the same checks run for every pull request into `main`, from `.github/workflows/ci.yml`: the format check,
the lints, the tests with the benchmarks' smoke run on Linux and on macOS, the documentation build and the checks of
the Python package, each as a job of its own. They run once per change, before it is merged, and not again on the
push the merge makes.

## The tests

Each kind of test checks something the others cannot.

- **Unit tests of each step.** Every module ends with its own tests, which check its private steps directly against
  cases worked by hand: the dealing of the arcs, the net supplies, the ratio test, each step of the tree update.
- **The basis invariant after every pivot.** A test-only check verifies the whole invariant of the basis (the shape of
  its columns, the tree, the potentials, the flows and strong feasibility) after the starting basis and after every
  pivot, on hand-worked networks pivot by pivot and on tiny random networks under four rules for the entering arc.
- **The tree against a model.** After random exchanges, every column of the threaded tree is compared with a plain
  model that recomputes it from the parent links alone.
- **Property tests against simple models.** Tiny random networks are solved and compared with a brute force that
  tries every integral flow; larger seeded networks with the optimum cost of a linear-programming solver. Neither model
  shares the solver's reasoning.
- **The certificate.** Every optimal answer of the public tests is checked in 128 bits: the flow within its bounds
  and meeting every supply, the potentials pricing it optimal, and the cost as stated.
- **The ranges.** Problems at both ends of every range, and one past each, are solved or refused, with the bounds
  recomputed in big integers apart from the arithmetic under test.
- **Allocation.** A counting allocator shows that a solve into the caller's buffer allocates nothing after a
  reservation, nor from the second solve on for a problem no larger than the first, and that the C library's reserved
  solve allocates nothing.
- **The C and C++ programs.** The header is compiled as C99, C11, C17, C++11 and C++17 with every warning an error,
  and a C program and two C++ programs are built against the static library and run; the C++ header is compiled in
  every standard from C++11 on.
- **The Python checks.** The package's tests run with and without NumPy, beside a strict type check and a check of
  the stubs against the built module.
- **The examples.** Every example in the Rust documentation, the README's and the Rust guide's included, runs as a
  doctest. The example programs of the crate `kahlnws` run as tests, each asserting its optimum worked out by hand,
  the Python package's tests run its example scripts and compare what they print, and the tests of the C boundary
  build the C and the C++ example programs, the C++ ones with and without exceptions, run them and compare what
  they print.

Every test is named `test_`, and every test can fail: none compares the code with itself.

## How the `unsafe` code is kept sound

The library uses `unsafe` for one purpose only: to skip a bounds check that an invariant already proves. It holds no
raw pointer, no `transmute` and no uninitialised memory. The rules, stated in full in
[the rules of the code](style.md#assertions-and-unsafe):

- Every `unsafe` block is preceded by a `debug_assert!` of the bound it relies on, and carries a `// SAFETY:` comment
  that names the invariant and the type that owns it. A block covers one read or one write, or the few accesses that
  rest on the same proof.
- Where a proof is shared, it moves into a type: an index that only its owner makes, so that holding one proves the
  bound. The tree's nodes and links, an arc's ends and a checked problem are such types.
- An `unsafe fn` is named `_unchecked`, states its precondition under `# Safety` and checks it with a
  `debug_assert!`. The tree's accessors have a checked twin with the same signature, and a property test shows that
  the two agree wherever the precondition holds. Two crate-internal steps of a pivot, `CycleWalk::new_unchecked` and
  `Exchange::apply_unchecked`, have no checked twin: the pivot hands them the ends and the slot of an arc of the
  checked problem, which the arc columns bound.
- The tests run with debug assertions on, so every stated bound is checked in every test.

The C interface needs `unsafe` that no invariant can replace: it reads the caller's pointers. All of it lives in the
crate's private module `boundary.rs`, whose helpers check every pointer for null and alignment in every build, answer
a status, and state under `# Safety` what C must guarantee. Every entry runs under `catch_unwind`, so no panic crosses
into C. The Python crate forbids `unsafe` code. The testing crate's counting allocator is the one `unsafe impl` of the
workspace.

## Miri

Miri runs the tests in an interpreter that reports undefined behaviour. It needs a nightly toolchain:

```sh
rustup toolchain install nightly --component miri
MIRIFLAGS=-Zmiri-disable-isolation PROPTEST_CASES=2 cargo +nightly miri test -p kahlnws -- \
    --skip test_the_seeded_shapes_against_the_oracle \
    --skip test_the_benchmark_networks_of_a_few_thousand_arcs_against_the_oracle \
    --skip test_one_solver_answers_every_problem_as_a_fresh_one \
    --skip test_a_refused_buffer_leaves_the_solver_answering_as_a_fresh_one \
    --skip test_every_exchange_reachable_from_the_star_keeps_every_column \
    --skip test_the_random_exchanges_reach_every_kind_of_exchange
cargo +nightly miri test -p kahlnws_testing counting_allocator -- --skip test_a_request_the_system_refuses_counts_too
```

`MIRIFLAGS=-Zmiri-disable-isolation` lets the property tests read the working directory, where proptest keeps the
failures it persists; Miri's isolation refuses that read. `PROPTEST_CASES` cuts the property tests that take the
default number of cases. The tests left out cannot run under
Miri, for these reasons:

- the four public tests that reach the linear-programming oracle: the oracle is foreign code, which Miri does not
  interpret;
- the allocator test that asks for $2^{62}$ bytes: Miri cannot refuse an allocation;
- the exhaustive exchange test and the exchange coverage run: each runs too long in an interpreter; the same exchange
  code runs in the random exchange tests.

Run so, with debug assertions on, Miri reports no undefined behaviour in any test of the library or of the counting
allocator.
