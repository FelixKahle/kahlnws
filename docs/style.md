# The rules of the code

For a contributor: the rules every change to the Rust code of this repository follows. A change that departs from
one is agreed beforehand; existing code that breaks one is raised, not worked around.

## What comes first

1. **Correctness.** A wrong answer, a reachable panic, an unsound `unsafe` block or a broken promise in a document
   outranks everything, and nothing is traded for it.
2. **Performance.** The solver runs inside other people's loops. Columns are laid out for how they are read, loops
   stay tight, and no work is done that does not follow from the input.
3. **How it reads.** Everything else on this page.

The three rarely conflict, because an invariant that is proven is not checked again. Where a type, the owner of the
data or a construction proves a bound, the access is unchecked; the proof is stated once, where the invariant is
established, and the hot code relies on it.

- The mathematics and the established methods come first. A device no standard treatment has (a lookup, a special
  state, an extra phase) carries a stated reason why the standard one was not taken.
- The cleaner design decides, not a measurement. Measurements are a record, or locate a slowdown nobody can explain.
- A number in the code is derived, with its derivation in its documentation, or it is not there. No dispatch on
  problem size, no fitted constants.

## Design

- **Layers point down.** Crates and modules form a graph without cycles; a lower layer never reaches up.
- **Wrong states are unrepresentable** where a type does it cheaply: no `bool` that means "step X has run", a setting
  with alternatives is an enum, fields that exclude each other are enum variants. Where a type would cost more than
  the mistake it prevents, an assertion states the rule instead.
- **Ids are newtypes.** Every kind of id has its own type, a `usize` newtype, and a table keyed by an id is a
  `TiVec<Id, T>`, so one id cannot index another table. A narrower id carries a measured reason in its documentation,
  and its `From<usize>` is written by hand and panics, under `#[track_caller]` and `# Panics`, where a value does not
  fit. A quantity becomes a newtype where it carries an invariant,
  could be confused with another quantity of the same primitive, or has a unit.
- **One fact, one place.** A field exists only if no other field implies it.
- **Plain data stays plain.** A record without an invariant has public fields and no getters, setters or
  constructors that only set fields. A record with an invariant across its fields keeps them private behind one
  checked constructor.
- **Values with an invariant** are made by a panicking `new` and a refusing `try_new`; a literal is checked at compile
  time in a `const`, never with `.unwrap()`.
- **Functions are split by logical step**, not by length: a function that validates, computes and applies is three
  private functions named for what each guarantees, and the parent reads as the sequence of steps. A step is a pure
  function where it can be, and is tested on its own in its module's tests. Shared mutable state that several steps
  pass around becomes a struct whose methods are the steps.
- **Types fit exactly.** No field that one path ignores, no `Option` one algorithm sets and another never does, no
  error variant reused for another failure. A second algorithm brings its own types and its own folder.
- **Native widths by default.** Indices and counts are `usize`, signed quantities `i64`, unsigned ones `u64`. A
  narrower type needs a measured reason, recorded in its documentation, and never appears in a public signature.
- **Casts say what they are.** A lossless conversion is `From`; a lossy one is `try_from` with the failure handled,
  or a deliberate cast under the narrowest `#[expect(clippy::cast_..., reason = "...")]` with the bound that makes it
  exact stated beside it.
- **Loops, not recursion.** A walk over a tree is a loop over an explicit stack.
- **No allocation on a path that runs per iteration.** Repeated work reuses buffers its owner holds and clears; a
  function called in a loop takes `&mut` to a buffer instead of returning a fresh `Vec`. Cold code may allocate.
- **Views, not copies.** A structure's parts are reached through a borrowed view or a named iterator, never a
  copied-out `Vec` or a callback. Every type that implements `Iterator` is named `...Iter`, and implements
  `ExactSizeIterator`, `DoubleEndedIterator` and `FusedIterator` where they hold.
- **Idiomatic at the edge, compact inside.** The public surface uses `Option`, `Result`, enums and borrowed views;
  sentinels, narrow integers and column layouts stay behind a private boundary owned by one type.
- **One `?` at the call site.** An answer that is not a failure, such as "no flow exists", is a variant of the
  outcome; the `Err` is for a refused call.
- **Full names.** No `idx`, `tmp`, `cnt` or `pos`; no `Manager`, `Helper`, `Util`, `Data` or `Info`; no module named
  for a pattern. A module is one file, or one folder with a `mod.rs` that only maps it.
- **Visibility is as narrow as the tree allows**, and is never widened to make a call compile.
- **Attributes that force the compiler** (`#[inline(always)]`, `#[inline(never)]`, `#[cold]`) carry a comment saying
  why. A panic on a hot path is built in a `#[cold] #[inline(never)] #[track_caller]` function.
- **No macros that stamp out code**, no `#[non_exhaustive]`, and no generic parameter that no caller varies.
- **Library crates start no threads.** Randomness is seeded explicitly, and budgets are counted in work, not time.

## Assertions and `unsafe`

- **`unsafe` only to skip a check that an invariant proves**, such as a bounds check. No raw pointer in a stored
  type, no `transmute`, no uninitialised memory, no `unsafe impl Send` or `Sync`.
- Every `unsafe` block is directly preceded by a `debug_assert!` of its precondition and carries a `// SAFETY:`
  comment naming the invariant and the type that owns it. A block covers one read or one write, or the few accesses
  that rest on one proof; it never spans a loop.
- Every `unsafe fn` is named `_unchecked` and every `_unchecked` function is `unsafe`, but for the C boundary's
  helpers, which no invariant proves. It has a `# Safety` section and `debug_assert!`s its precondition. An index
  accessor comes as a pair, a checked `f` and an `f_unchecked` of the same signature; a property test shows that the
  two agree wherever the precondition holds and that the checked form refuses everywhere else.
- **Hot code calls the unchecked form wherever an invariant proves the precondition**: a bounds check on a hot path
  that an invariant already proves is a defect. Iterators and views are the first place for it.
- **`debug_assert!` has two uses only:** an invariant inside a module that nothing outside it can break, checked in
  a private function; and the precondition of `unsafe` code.
- **A condition a caller in another module could break** is made unrepresentable by a type, checked by an `assert!`
  in every build and documented under `# Panics`, or moved behind an `unsafe fn f_unchecked`. "No caller breaks it
  today" is not a reason for a `debug_assert!`.
- The C interface is the one place where `unsafe` reads the caller's pointers: in one private module whose helpers
  check null in every build and answer a status, and whose `# Safety` sections say what C must guarantee. Every entry
  runs under `catch_unwind`. The testing crate's counting allocator is the one `unsafe impl`.

## Messages

Messages are lower case, have no trailing full stop, and give the offending values.

- A `panic!` or `assert!` for a precondition names the call and the violated condition:
  ``called `NetworkSimplex::solve_into` with 2 flows, which should be the arc count 3``.
- An `expect` says what should hold, with "should": `the first worker should have filled the slot`.
- An overflow is not written by hand: the `strict_*` operations of std panic with `attempt to add with overflow`.
- An error's `Display` says what went wrong, with no type name in front, and gives its source through `source()`:
  `the supplies add up to 1, not zero`.
- `# Panics` describes the condition the panic message states, and `# Errors` names the variants returned.

A function never takes its caller's name, or any text that exists only to end up in a message, as a parameter.

## Documentation

- Every public item has a documentation comment; a private one has one where its name and type do not say what it
  guarantees. Every type implements `Debug`.
- The first sentence states what the item is or guarantees, not how it works. Then the contract in full, the cost
  where a caller would care, `# Panics`, `# Errors` and `# Safety` wherever they apply (private items included), and
  an example a user would type.
- A documentation comment holds no walk through the body, no implementation detail, no proof (the proof is a `//`
  comment beside the code it is about), no history and no claim the code does not keep.
- Within a type, every field is documented or none is; a field's documentation gives its unit and what its absence
  means. The invariant of a type is written once, on the type, and every `// SAFETY:` line names an invariant written
  there.
- A `//` comment says why, never what the next line does, and never tells history.
- Sources and documents name no design document, milestone, author, paper or other library; an algorithm's
  established name is fine.
- Lines stay within 120 characters, in code, comments, documents and tables.

## Lints

The workspace manifest sets the lints every crate inherits: among them `missing_docs`,
`missing_debug_implementations`, `unsafe_op_in_unsafe_fn`, `undocumented_unsafe_blocks`, `missing_safety_doc`,
`missing_panics_doc`, `missing_errors_doc`, `allow_attributes_without_reason` and the documentation lints. A lint is
never silenced with `#[allow]` to get a build through; a real false positive or a deliberate cast takes the narrowest
`#[expect(lint, reason = "...")]`. The only `#[allow]` is in test modules, for `clippy::missing_panics_doc`.

## Tests

- Test names start with `test_`. Every module ends with its own `#[cfg(test)] mod tests`, which tests its private
  steps directly; `tests/` covers the public surface. Shared test infrastructure lives in the testing crate, never
  under `src/`.
- Property tests check against deliberately simple reference models that share none of the code's reasoning, such
  as big integers in place of the overflow logic under test. Invariants that need no model, and hand-written expected
  outputs for small cases, come first.
- **A test must be able to fail.** A test that would pass with the code deleted, or a property whose generator never
  reaches the interesting case, is rewritten.
- **A bug is fixed red first and at once**: the test that fails because of it is written and seen to fail, then the
  fix makes it pass, in a commit of its own.
- No mutation testing. Benchmarks fix the amount of work, never the time. Every serialisable type has a round-trip
  test.

## Manifests and dependencies

- Edition 2024, resolver 3, `rust-version` pinned to the stable toolchain in use, `publish = false` unless a crate is
  meant for a registry.
- The standard library first, then an established crate that is widely used and has no open soundness issue, and code
  of one's own last; the error-prone parts (parsing, printing, rounding, exact arithmetic) are never hand-written.
- Every external dependency is declared once in `[workspace.dependencies]`, with a comment saying what it is for,
  `default-features = false` and only the features used. A dependency arrives in the change that first uses it and
  leaves once nothing does. Test, benchmark and compile-fail tools are dev-dependencies only.
- The licence is MIT OR Apache-2.0 for every crate. No dependency under a copyleft or non-commercial licence is added,
  not even for tests; the one exception is the writer of the C header, a build dependency of the C crate whose code is
  linked into nothing.

## The file header

Every source file starts with these two lines, before any documentation comment or attribute:

```rust
// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0
```

## Changes

- A move is only a move: files are moved with `git mv` in a commit of their own that changes no function body. A
  split of a file is a commit of its own and changes no behaviour. A bug fix is its own commit with its test.
- A commit message is plain prose saying what is now true. It carries no attribution or co-author lines and names no
  tool.
- Findings from a review are fixed, not argued away.
- A change is done when `./gates.sh` passes.
