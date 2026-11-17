# Changelog

Every change that a user of kahlnws can notice is recorded here from the first release on. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

- A network, the state beside the solver, which stays a function that keeps its room and nothing else: `Network` in
  Rust, `kahlnws_network` in C, `kahlnws::network` in C++ and `kahlnws.Network` in Python. It holds one problem and a
  strongly feasible basis of it. A load hands it a whole problem and carries over the basis it holds where the arcs are
  the same and that basis serves, and otherwise starts as the solver does; setters change one arc's cost, capacity,
  lower bound or both bounds, and a move shifts supply between two nodes, each at once, a setter answering the number it
  replaced, and each refusing a number out of range with a `ChangeError` that changes nothing; its solve prices only the
  arcs the changes may have made violate, until it pivots, and answers the cost and the work, and the network then lends
  the optimum, which no later change can be read through. It reads back the problem it holds, copies within its room,
  and answers the outcome and the cost the solver answers for the problem as it stands, with potentials that prove them.
- `set_supply(node, supply, balancing_node)` on a network, in each language: it makes `supply` the supply of a node,
  the node that balances it moving by the same amount the other way, and answers the supply the node had; a node that
  balances itself is given only the supply it has. `set_supply` in Rust, `kahlnws_network_set_supply` in C,
  `try_set_supply` and `set_supply` in C++, `Network.set_supply` in Python.
- The reads of every table of the problem a network holds, in each language: a column, named as the field of a
  `Problem` (`tails`, `heads`, `costs`, `lower_bounds`, `capacities`, `supplies`), and a single read of one arc or
  node (`tail`, `head`, `cost`, `lower_bound`, `capacity`, `supply`), and the bounds of an arc as the pair
  `set_bounds` takes.
- Python: `kahlnws.Bounds`, a named tuple of an arc's lower bound and capacity, which `Network.bounds` reads and
  `Network.set_bounds` answers and takes. `Network.solve` answers an `Answer[int]`, the cost and the work, as
  `solve_into` does, and `Network.optimum()` answers the optimum held, a `NetworkOptimum`, or `None`.
- C: `kahlnws_int128_format` and `kahlnws_uint128_format` write a 128-bit value in decimal, NUL-ended, and answer its
  length; the C++ header prints its 128-bit values through them. `kahlnws_refusal` holds `supply`, the supply a node
  would have past its range, and `excess_total`, the flow a change would leave still to be sent, each in a field of
  its own.
- C: the statuses, numbered from 0 without gaps: a solve answers `KAHLNWS_STATUS_OPTIMAL` or
  `KAHLNWS_STATUS_INFEASIBLE`, every other call that did what it was asked `KAHLNWS_STATUS_OK`, and the other statuses
  say what stopped the call.
- C++: `try_set_bounds` and `set_bounds` take a `kahlnws::bounds`, as they answer one, and `kahlnws::span` has
  `first(count)`.
- Messages that name the argument a caller got wrong, with its value: in Python a number out of the range of its kind
  names the argument, the value and the range, and an arc or a node below zero is an `IndexError` as one not below
  its count is; in C++ an arc or a node not below its count is named with the count, as in "arc 42 is not below the
  arc count 8", and no message names a function the caller did not call.
- The examples `network`, a network held over three days, each later day loaded from the basis the day before ended
  on, and `search`, a short search whose moves change a few numbers and are solved and kept or set back, one of them
  refused at its second change and its first change set back, in each language.
- The benchmark suites `warm`, which times a network's load of a changed network on the basis it holds beside the
  solver's solve of the same network, and `changes`, which times a network's changes of a few numbers beside a load of
  the whole changed network and the solver's solve.
- The workspace names no Rust version: it builds with a current stable toolchain.
