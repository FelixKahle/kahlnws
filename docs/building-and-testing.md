# Building, testing and safety

For a contributor, or anyone who wants to know what has been checked and how: what a build needs, what the gates
run, which kinds of tests there are, how the `unsafe` code is kept sound, and how the tests run under Miri.

## What a build needs

- **A current stable Rust toolchain**, with `rustfmt` and `clippy`: every crate, and the gates. The workspace names
  no Rust version.
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

The steps run in this order; the first four cover the whole workspace. `KAHLNWS_JOBS`, 4 unless the variable is set,
bounds cargo's build jobs and the test threads, each on its own; the cargo that the tests of the C boundary start to
build the C library keeps the same bound on its jobs, which run beside the other test threads:

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
  pivot, on hand-worked networks pivot by pivot and on tiny random networks under four rules for the entering arc;
  after a network's load from the basis it holds, on hand-worked changes of one network and on tiny random pairs of
  networks, whose second answer the brute force checks; and after every load, change and solve of a network in random
  sequences of calls.
- **The tree against a model.** After random exchanges, and after the two walks that make a kept tree a tree of a new
  problem, given random decisions and costs, every column of the threaded tree is compared with a plain model that
  recomputes it from the parent links alone.
- **Property tests against simple models.** Tiny random networks are solved and compared with a brute force that
  tries every integral flow; larger seeded networks with the optimum cost of a linear-programming solver. Neither model
  shares the solver's reasoning.
- **The certificate.** Every optimal answer of the public tests is checked in 128 bits: the flow within its bounds
  and meeting every supply, the potentials pricing it optimal, and the cost as stated.
- **The ranges.** Problems at both ends of every range, and one past each, are solved or refused, with the bounds
  recomputed in big integers apart from the arithmetic under test.
- **One solver, many problems.** One solver answers every problem as a fresh one does, its work included, whatever
  it solved or refused before.
- **A load from the basis held.** Sequences of seeded networks, each with a few numbers moved from the one before, are
  loaded into one network and answer the outcome and the cost of a fresh solve, with a certificate; where the basis
  held cannot serve, for other counts, other ends or past the limit of the excesses, a load answers exactly what a
  load into a new network answers, its work included. The same problem loaded again makes no pivot, and a refused
  load keeps the basis.
- **A network's changes.** Hand-worked networks are loaded, changed by every setter and by moves of supply, solved
  and set back, with each change's refusals at the ends of their ranges; random sequences of changes are solved and
  checked against a solve of the problem as it stands; tiny networks are checked against the brute force and seeded ones
  against the solver and the linear-programming oracle, by outcome, cost and certificate; two networks given the same
  calls answer alike, their work included; and a copy changes apart from its source.
- **Allocation.** A counting allocator shows that a solve into the caller's buffer allocates nothing after a
  reservation, nor from the second solve on for a problem no larger than the first; that a network's load, from the
  basis it holds or from the star, its changes, solve, reads and copies into a network with room allocate nothing
  after a reservation, nor its loads after a first load of a problem no smaller; and that the C library's reserved
  solve and its network's calls allocate nothing.
- **The C and C++ programs.** The header is compiled as C99, C11, C17, C++11 and C++17 with every warning an error,
  and two C programs and three C++ programs are built against the static library and run; the C++ header is compiled in
  every standard from C++11 on.
- **The Python checks.** The package's tests run with and without NumPy, beside a strict type check and a check of
  the stubs against the built module.
- **The examples.** Every example in the Rust documentation, the README's and the Rust guide's included, runs as a
  doctest. The example programs of the crate `kahlnws` run as tests, each asserting its optimum worked out by hand,
  the Python package's tests run its example scripts and every example of the Python guide and compare every line
  the scripts print, and the tests of the C boundary build the C and the C++ example programs and the complete
  programs of the C and the C++ guide, the C++ ones with and without exceptions, run them and compare every line they
  print.

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

Miri runs the tests in an interpreter that reports undefined behaviour: an out-of-bounds read or write, a read of
memory that holds no value, a reference that breaks the aliasing rules. Here it checks the `unsafe` code of the library
and of the C interface, whose bounds the tests otherwise only assert. It needs a nightly toolchain with the Miri
component; the tests run with debug assertions on, as in every test build.

The library's tests run module by module, the public tests and the C interface's tests file by file, each a command
of its own:

```sh
rustup toolchain install nightly --component miri
export MIRIFLAGS=-Zmiri-disable-isolation PROPTEST_CASES=2
cargo +nightly miri test -p kahlnws --lib tree:: -- --test-threads=1 \
    --skip test_every_exchange_reachable_from_the_star_keeps_every_column \
    --skip test_the_random_exchanges_reach_every_kind_of_exchange \
    --skip test_the_random_walks_reach_every_kind_of_cut_and_turn
cargo +nightly miri test -p kahlnws --lib basis:: -- --test-threads=1 \
    --skip test_the_random_routes_reach_every_way_a_route_goes
cargo +nightly miri test -p kahlnws --lib network_simplex:: -- --test-threads=1
cargo +nightly miri test -p kahlnws --lib network:: -- --test-threads=1 \
    --skip test_the_random_calls_reach_every_kind_of_change
cargo +nightly miri test -p kahlnws --lib block_scan:: -- --test-threads=1
cargo +nightly miri test -p kahlnws --lib problem:: -- --test-threads=1
cargo +nightly miri test -p kahlnws --lib ranges:: -- --test-threads=1
cargo +nightly miri test -p kahlnws --lib slot_order:: -- --test-threads=1
cargo +nightly miri test -p kahlnws --lib outcome:: -- --test-threads=1
cargo +nightly miri test -p kahlnws --lib room:: -- --test-threads=1
cargo +nightly miri test -p kahlnws --test allocation -- --test-threads=1
cargo +nightly miri test -p kahlnws --test network -- --test-threads=1 \
    --skip test_the_seeded_shapes_against_a_solve_and_the_oracle
cargo +nightly miri test -p kahlnws --test solve -- --test-threads=1 \
    --skip test_the_seeded_shapes_against_the_oracle \
    --skip test_the_benchmark_networks_of_a_few_thousand_arcs_against_the_oracle \
    --skip test_one_solver_answers_every_problem_as_a_fresh_one \
    --skip test_a_refused_buffer_leaves_the_solver_answering_as_a_fresh_one
cargo +nightly miri test -p kahlnws_capi --lib -- --test-threads=1
cargo +nightly miri test -p kahlnws_capi --test entries -- --test-threads=1
cargo +nightly miri test -p kahlnws_capi --test allocation -- --test-threads=1
cargo +nightly miri test -p kahlnws_capi --test header_text -- --test-threads=1
cargo +nightly miri test -p kahlnws_testing --lib counting_allocator:: -- --test-threads=1 \
    --skip test_a_request_the_system_refuses_counts_too
```

The library's filters together cover every module of `--lib`. A filter matches every test whose path contains it, so
`basis::` runs the tests of `starting_basis` too, and `problem::` runs those of `network::held_problem`, which
`network::` runs as well. `MIRIFLAGS=-Zmiri-disable-isolation` lets the property tests read the working directory,
where proptest keeps the failures it persists; Miri's isolation refuses that read. `PROPTEST_CASES=2` sets the number
of cases of every property test. `--test-threads=1` runs the tests one after another: Miri interleaves the threads of
tests run side by side, which makes each of them many times slower.

The tests left out cannot run under Miri, for these reasons:

- the public tests that reach the linear-programming oracle, directly or through the networks they choose by it: the
  oracle is foreign code, which Miri cannot call;
- the allocator test that asks for $2^{62}$ bytes: Miri stops with resource exhaustion where the system refuses;
- the exhaustive exchange test and the coverage runs of the random exchanges, the random walks, the random routes and
  the random calls of a network: each runs for more than ten minutes in an interpreter, and the code each reaches runs
  in the property tests of the same exchanges, walks, routes and calls;
- the tests of the C interface that build and run C and C++ programs and check the header (`--test header`): they
  start compilers and run foreign code.

The doctests and the example programs are not run under Miri.

The commands take about forty minutes when four run at a time, each with its own `--target-dir`. The public network
tests alone take that long, the longest of them, the coverage run of the changes and solves of a network, about
fifteen minutes.

On 17 November 2026, with `rustc 1.102.0-nightly (3ef0c22cd 2026-11-11)` and its Miri, the commands above ran 816
tests: 504 of the library's own, 124 of its public tests, 181 of the C interface and 7 of the counting allocator. Miri
reported no undefined behaviour in any of them. They reach every `unsafe` block of the library and of the C interface's
Rust code, and the four `unsafe` blocks of the counting allocator.
