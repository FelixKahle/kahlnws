# Changelog

Every change that a user of kahlnws can notice is recorded here from the first release on. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

- A solver keeps the basis its last solve ended on, and solves again from it: `NetworkSimplex::resolve` and
  `NetworkSimplex::resolve_into` in Rust, `kahlnws_solver_resolve` in C, `resolve`, `resolve_into` and their `try_`
  forms in C++, and `NetworkSimplex.resolve` and `NetworkSimplex.resolve_into` in Python. A resolve answers the
  outcome and the cost a solve answers, with potentials that prove them, and is a solve wherever the kept basis cannot
  serve the problem.
- The example `resolve` in each language, and the benchmark suite `warm`, which times a resolve beside a solve of the
  same network, again or with some of its numbers changed.
