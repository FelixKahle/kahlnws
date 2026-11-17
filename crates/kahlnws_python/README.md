<h1 align="center">
  <img alt="kahlnws" width="880"
       src="https://raw.githubusercontent.com/FelixKahle/kahlnws/main/docs/img/kahlnws-light.svg">
</h1>

kahlnws is a primal network simplex for the min-cost flow problem on exact integers, written in Rust, as a Python
package. A problem is a set of nodes, each with a supply, and arcs between them, each with a lower bound, a capacity
and a cost per unit of flow; a solve finds a flow of least cost that meets every supply within the bounds, or answers
that no such flow exists.

- **Exact.** Every number is an integer and every sum a solve forms is exact; the cost is a Python `int`.
- **Certified.** An optimal answer comes with the potentials that prove it optimal.
- **No dependency, fully typed.** One wheel serves CPython 3.11 and later. Columns are sequences of `int` or buffers
  such as NumPy arrays; NumPy itself is never needed.

It offers two types, one contract each:

- **The solver**, `NetworkSimplex`, is a function: a problem in, an answer out. It keeps its room and nothing else, so
  its answer is a function of the problem alone. A caller who solves in a loop keeps one solver and has the flows
  written into a buffer of its own with `solve_into`.
- **A network**, `Network`, is a state: it holds one problem and a basis of it. Each change applies at once, and each
  setter answers the number it replaced, so it can be set back; a load hands it new numbers on the same arcs; and its
  solve starts from the basis it holds, so after a few changed numbers it often makes far fewer pivots than a solve from
  nothing kept. A network reads back the problem it holds, column by column or number by number, and copies itself, or
  into another network within its room, raising `MemoryError` where the memory for a copy is refused.

## Installing

```sh
pip install kahlnws
```

## Example

A depot (node 0) sends 4 units to a customer (node 2), directly or through a hub (node 1):

```python
import kahlnws

solver = kahlnws.NetworkSimplex()
answer = solver.solve(tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4])
assert answer.outcome is not None
assert answer.outcome.flows == [2, 2, 2] and answer.outcome.total_cost == 14
```

## Documentation

- [The Python guide][python-api]: installing, buffers, NumPy and the exceptions.
- [The problem and the guarantees][problem-and-guarantees]: the linear program, the certificate, the
  ranges and the refusals.
- [Benchmarks][benchmarks]: the three suites, and a comparison with two LP solvers.
- [The examples][examples]: six scripts on one small transport network.

The solver and a network are offered to Rust, C and C++ from [the repository](https://github.com/FelixKahle/kahlnws).

## Licence

Licensed under either of the Apache License, Version 2.0 or the MIT License, at your option.

[python-api]: https://github.com/FelixKahle/kahlnws/blob/main/docs/python-api.md
[problem-and-guarantees]: https://github.com/FelixKahle/kahlnws/blob/main/docs/problem-and-guarantees.md
[benchmarks]: https://github.com/FelixKahle/kahlnws/blob/main/docs/benchmarks.md
[examples]: https://github.com/FelixKahle/kahlnws/blob/main/crates/kahlnws_python/examples
