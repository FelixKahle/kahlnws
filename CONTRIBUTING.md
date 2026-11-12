# Contributing to kahlnws

Contributions are welcome: reports of bugs, fixes, tests and documents. This page says how a change is made so that
it can be taken as it is.

## Building and the gates

[Building, testing and safety](docs/building-and-testing.md) lists what a build needs: Rust 1.99.1, a Python
interpreter of at least 3.11, a C and a C++ compiler, and uv for the Python checks. A change is ready when the gates
pass, run from the root of the repository:

```sh
./gates.sh
```

They run the format check, the lints with every warning an error, every test and doctest, the documentation build
and a smoke run of the benchmarks. A change to the Python package also passes `crates/kahlnws_python/check.sh`. A
change to the C interface that changes the header rewrites it with
`KAHLNWS_WRITE_HEADER=1 cargo build -p kahlnws_capi` and commits the result.

## The rules of the code

Every change follows [the rules of the code](docs/style.md), which state in full what the code of this repository
holds to. They are binding: a change follows them, and a departure from one is agreed beforehand. In short:

- **Correctness, then performance, then how it reads.** An invariant that is proven is not checked again, and a
  number in the code is derived, with its derivation beside it, or it is not there.
- **Wrong states are unrepresentable** where a type does it cheaply: ids are newtypes, settings are enums, and plain
  data has public fields.
- **Functions are split by logical step**, and each step is tested on its own in its module's tests.
- **`unsafe` only to skip a check an invariant proves.** Every block is preceded by a `debug_assert!` of its
  precondition and a `// SAFETY:` comment naming the invariant; every `unsafe fn` is named `_unchecked`, and every
  index accessor has a checked twin; the C boundary's helpers are the one exception to the naming.
- **`debug_assert!` against `assert!`.** A `debug_assert!` checks only a module's own invariants, or the precondition
  of `unsafe` code. What a caller from another module could break is made impossible by a type, or checked by an
  `assert!` in every build and documented under `# Panics`.
- **Views and named iterators, not copies; no allocation on a path that runs per iteration.**
- **Documentation states the guarantee**, in full words, with no history; lines stay within 120 characters.
- **Tests can fail.** A bug is fixed red first, property tests check against simple reference models, and there is
  no mutation testing.
- **A move is its own commit**, and so is a bug fix with its test.

## Changes and commit messages

A change does one thing. A bug fix is its own commit, with its test; a move is its own commit; a refactoring changes
no behaviour.

A commit message is plain prose saying what is now true, such as "a solve answers the work it did beside its
outcome". It carries no attribution or co-author lines and names no tool. Findings from a review are fixed in the
change, not argued away.

## Reporting a bug

A report says what was solved, what came back, and what was expected:

- **the problem**, as its six tables: `tails`, `heads`, `lower_bounds` (or none), `capacities`, `costs` and
  `supplies`, in a form that can be pasted into a test, and the language and interface it was solved through;
- **the answer**: the outcome with its flows, potentials and cost, or the refusal, and the work;
- **the expected answer**, and why: another solver's optimum cost, a flow of lower cost, a flow that shows the problem
  feasible, or the certificate condition the answer breaks;
- **the version** of kahlnws and of the toolchain, and the platform.

The smallest problem that shows the bug helps most. An answer can be checked against its own certificate first, as
[the certificate](docs/problem-and-guarantees.md#potentials-and-the-certificate) shows: a certified optimal answer of
another cost than expected points at the expectation, not the solver.

A bug is fixed as the rules say: first a test that fails because of it, in the module's tests or in `tests/`, then
the fix, in one commit.

## Documents

The documents follow the same rules as the documentation in the code: they state what there is, in full words, with
no history and no promise. Every line stays within 120 characters. No document names another solver library or an
author; an algorithm is named by its established name. Rust examples in the README and in the Rust guide run as
doctests, so a change that breaks one fails the gates.

## Licence

kahlnws is licensed under either of the Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE)) or the MIT
License ([`LICENSE-MIT`](LICENSE-MIT)), at your option. Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.
