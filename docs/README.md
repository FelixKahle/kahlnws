# The documents of kahlnws

Each page answers one question for one reader. The first two hold for every language; the guides show each
interface at work.

- [The problem and the guarantees](problem-and-guarantees.md), for every user: the problem as a linear program,
  feasibility, the certificate of an optimal answer, the ranges and the refusals, the answer and the work, solving
  again, room, determinism and threads.
- [How it works](how-it-works.md), for a reader who knows linear programming: the algorithm in its established terms,
  its start from the star or from a kept basis, and the layout of its data.
- [The Rust interface](rust-api.md), for a Rust programmer: each task with a complete program.
- [The C interface](c-api.md), for a C programmer: building and linking, ownership, statuses, refusals, 128-bit
  values and a complete program.
- [The C++ interface](cpp-api.md), for a C++ programmer: the header-only interface over the C one.
- [The Python interface](python-api.md), for a Python programmer: installing, columns and buffers, NumPy, the
  exceptions and threads.
- [Benchmarks](benchmarks.md), for anyone who measures: the two suites, how to run them and read them, a fair
  measurement, and a comparison with two LP solvers.
- [Building, testing and safety](building-and-testing.md), for a contributor: the gates, the kinds of tests, the
  `unsafe` code and Miri.
- [The rules of the code](style.md), for a contributor: what every change to the code holds to.
- [Releasing](releasing.md), for a maintainer: what a release publishes, its one-time setup and its steps.

The Rust items carry their own reference documentation, which `cargo doc --open -p kahlnws` builds; the C header
`crates/kahlnws_capi/include/kahlnws.h` and the Python package's docstrings document every name of theirs. How changes
are made is in [`CONTRIBUTING.md`](../CONTRIBUTING.md).
