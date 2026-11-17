# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The package as a whole: its names, its version, `cost_bound`, the reservation, its documents and its stubs, and that
it never imports NumPy."""

import ast
import doctest
import importlib.util
import inspect
import subprocess
import sys
from pathlib import Path

import pytest

import kahlnws
from kahlnws import _kahlnws

# The folder the package was installed to, with its stubs and its marker of types.
PACKAGE = Path(kahlnws.__file__).parent


def test_the_package_exports_its_names_and_the_version_of_the_workspace() -> None:
    assert sorted(kahlnws.__all__) == [
        "Answer",
        "Bounds",
        "Breach",
        "ChangeBreach",
        "ChangeError",
        "Network",
        "NetworkOptimum",
        "NetworkSimplex",
        "ProblemError",
        "Solution",
        "Table",
        "Work",
        "__version__",
        "cost_bound",
    ]
    assert kahlnws.__version__ == "0.1.0"
    assert all(getattr(kahlnws, name) is getattr(_kahlnws, name) for name in kahlnws.__all__)
    assert issubclass(kahlnws.ProblemError, ValueError)
    assert issubclass(kahlnws.ChangeError, ValueError)
    assert not issubclass(kahlnws.ChangeError, kahlnws.ProblemError), "a change is refused apart from a problem"
    classes = [kahlnws.NetworkSimplex, kahlnws.Network, kahlnws.NetworkOptimum, kahlnws.Solution, kahlnws.Bounds]
    exceptions = [kahlnws.ProblemError, kahlnws.ChangeError]
    assert [owner.__module__ for owner in [*classes, *exceptions]] == ["kahlnws"] * 7
    assert issubclass(kahlnws.Bounds, tuple)
    assert kahlnws.Bounds._fields == ("lower_bound", "capacity")


def test_cost_bound_by_hand() -> None:
    assert kahlnws.cost_bound(0) == 2**62 - 1
    assert kahlnws.cost_bound(1) == 2**62 - 1
    assert kahlnws.cost_bound(1000) == 4_611_686_018_427_387
    assert kahlnws.cost_bound(2**64 - 1) == 0

    with pytest.raises(OverflowError, match=rf"^node_count -1 is outside the range of a count, from 0 to {2**64 - 1}$"):
        kahlnws.cost_bound(-1)
    with pytest.raises(OverflowError, match=rf"^node_count {2**64} is outside the range of a count, from 0 to "):
        kahlnws.cost_bound(2**64)
    with pytest.raises(TypeError, match=r"^node_count is of type float, not an integer$"):
        kahlnws.cost_bound(1.0)  # type: ignore[arg-type]  # a float is refused, which the test shows


def test_a_reservation_refused_raises_memory_error_and_keeps_the_solver_usable() -> None:
    # Counts whose room passes what an allocation may ask for: refused before any memory is asked for.
    solver = kahlnws.NetworkSimplex()

    with pytest.raises(MemoryError, match=r"^the room for 18446744073709551615 nodes and 0 arcs was refused$"):
        solver.reserve(2**64 - 1, 0)
    with pytest.raises(MemoryError, match=r"^the room for 0 nodes and 18446744073709551615 arcs was refused$"):
        solver.reserve(0, 2**64 - 1)
    with pytest.raises(OverflowError, match=r"^node_count -1 is outside the range of a count, from 0 to "):
        solver.reserve(-1, 0)
    with pytest.raises(OverflowError, match=rf"^arc_count {2**64} is outside the range of a count, from 0 to "):
        solver.reserve(0, 2**64)
    with pytest.raises(TypeError, match=r"^arc_count is of type float, not an integer$"):
        solver.reserve(0, 1.5)  # type: ignore[arg-type]  # a float is refused, which the test shows

    solution = solver.solve(
        tails=[0, 0, 1], heads=[2, 1, 2], capacities=[3, 4, 2], costs=[5, 1, 1], supplies=[4, 0, -4]
    ).outcome
    assert solution is not None
    assert solution.total_cost == 14


def test_the_examples_of_every_docstring_run() -> None:
    # The package's own document, and every docstring of the extension's classes, methods and functions, which the
    # package's namespace holds; their examples name the package `kahlnws`.
    failed, attempted = doctest.testmod(kahlnws, extraglobs={"kahlnws": kahlnws}, raise_on_error=False)

    assert attempted >= 12
    assert failed == 0


def documented(node: ast.ClassDef | ast.Module, owner: object, prefix: str) -> dict[str, tuple[str, str | None]]:
    """For each public class, function, method and property the stubs declare in `node` with a docstring, keyed by its
    dotted name from `prefix`: the stubs' docstring and the docstring of what the runtime `owner` holds under that
    name."""
    found: dict[str, tuple[str, str | None]] = {}

    for child in node.body:
        # A private name of the stubs, such as a protocol that only types an argument, is held by no module.
        if not isinstance(child, ast.ClassDef | ast.FunctionDef) or child.name.startswith("_"):
            continue
        name, stub = f"{prefix}{child.name}", ast.get_docstring(child)
        held = getattr(owner, child.name)
        if stub is not None:
            found[name] = (stub, inspect.getdoc(held))
        if isinstance(child, ast.ClassDef):
            found.update(documented(child, held, f"{name}."))

    return found


def words(text: str | None) -> str | None:
    """`text` with every run of white space one space: so the two docstrings compare whatever their line breaks."""
    return None if text is None else " ".join(text.split())


def test_the_stubs_declare_every_name_with_the_docstring_the_module_gives_it() -> None:
    stubs = ast.parse((PACKAGE / "_kahlnws.pyi").read_text())
    pairs = documented(stubs, _kahlnws, "")

    for name, (stub, runtime) in pairs.items():
        assert words(stub) == words(runtime), name

    declared = {name for name in pairs if "." not in name}
    assert declared == {name for name in kahlnws.__all__ if name != "__version__"}
    assert words(ast.get_docstring(stubs)) == words(_kahlnws.__doc__)
    entries = ["solve", "solve_into"]
    assert all(f"NetworkSimplex.{entry}" in pairs for entry in entries)
    calls = ["reserve", "load", "node_count", "arc_count", "set_cost", "set_capacity", "set_lower_bound", "set_bounds"]
    calls += ["set_supply", "move_supply", "solve", "optimum", "copy", "copy_from", "tail", "head", "cost", "bounds"]
    calls += ["supply", "lower_bound", "capacity"]
    columns = ["tails", "heads", "lower_bounds", "capacities", "costs", "supplies"]
    calls += [*columns, *(f"{column}_into" for column in columns)]
    assert all(f"Network.{call}" in pairs for call in calls)
    reads = ["total_cost", "flow", "flows", "flows_into", "potentials", "potentials_into"]
    assert all(f"NetworkOptimum.{read}" in pairs for read in reads)
    assert "Breach.NetSupplyBeyondRange.total" in pairs
    assert "ChangeBreach.ExcessBeyondRange.total" in pairs
    assert (PACKAGE / "py.typed").is_file()


def test_the_package_never_imports_numpy() -> None:
    # A fresh interpreter, in which importing NumPy fails: the package imports, reads every kind of plain column and
    # buffer, solves at both levels, holds a network, changes and solves it and writes its optimum, raises,
    # and NumPy is not among the modules loaded.
    program = """
import sys
from array import array

class NoNumPy:
    def find_spec(self, name, path=None, target=None):
        if name == "numpy" or name.startswith("numpy."):
            raise ImportError("numpy is blocked")
        return None

sys.meta_path.insert(0, NoNumPy())

import kahlnws

solver = kahlnws.NetworkSimplex()
depot = {"tails": [0, 0, 1], "heads": (2, 1, 2), "capacities": array("Q", [3, 4, 2]), "supplies": [4, 0, -4]}
answer = solver.solve(**depot, costs=memoryview(array("q", [5, 1, 1])))
solution = answer.outcome
flows = array("Q", [0, 0, 0])
cost = solver.solve_into(**depot, costs=[5, 1, 1], flows=flows).outcome
fewer = solver.solve_into(**depot | {"supplies": [3, 0, -3]}, costs=[5, 1, 1], flows=flows).outcome
network = kahlnws.Network()
network.load(**depot, costs=array("q", [5, 1, 1]))
network.move_supply(2, 0, 1)
cost_held = network.solve().outcome
held = network.optimum()
potentials = array("q", [0, 0, 0])
held.potentials_into(potentials)
read = (cost_held, held.total_cost, held.flows, potentials.tolist())
try:
    network.set_lower_bound(0, 4)
except kahlnws.ChangeError as error:
    change = error.breach
try:
    solver.solve(tails=[0], heads=[5], capacities=[1], costs=[1], supplies=[1, -1])
except kahlnws.ProblemError as error:
    breach = error.breach
assert (solution.flows, cost, fewer, flows.tolist()) == ([2, 2, 2], 14, 9, [1, 2, 2])
assert breach == kahlnws.Breach.HeadOutsideNodes(arc=0, head=5, node_count=2)
assert read == (19, 19, [3, 2, 2], [0, 1, 5])
assert change == kahlnws.ChangeBreach.LowerAboveCapacity(arc=0, lower_bound=4, capacity=3)
assert not any(name == "numpy" or name.startswith("numpy.") for name in sys.modules), "numpy was imported"
print("solved without numpy")
"""
    finished = subprocess.run([sys.executable, "-c", program], capture_output=True, text=True, check=False)

    assert finished.returncode == 0, finished.stderr
    assert finished.stdout == "solved without numpy\n"


@pytest.mark.parametrize("folders_up", [3, 1], ids=["workspace", "package"])
def test_the_example_of_the_workspace_s_and_of_the_package_s_readme_runs(folders_up: int) -> None:
    readme = (Path(__file__).parents[folders_up] / "README.md").read_text()
    examples = readme.split("```python\n")[1:]

    assert len(examples) == 1
    finished = subprocess.run([sys.executable, "-c", examples[0].split("```")[0]], capture_output=True, check=False)
    assert finished.returncode == 0, finished.stderr


# The Python guide of the workspace, whose every example is promised to run as shown.
GUIDE = Path(__file__).parents[3] / "docs" / "python-api.md"

# The examples of the guide, in the order they stand: the text of each block of Python code.
GUIDE_EXAMPLES = [block.split("```")[0] for block in GUIDE.read_text().split("```python\n")[1:]]


def test_the_guide_holds_its_examples() -> None:
    # The page's eight examples: the solver's owned answer, its buffers as NumPy arrays and as `array.array`, three of
    # a network, the refusals and the threads.
    assert len(GUIDE_EXAMPLES) == 8
    assert sum("import numpy" in example for example in GUIDE_EXAMPLES) == 1


@pytest.mark.parametrize("example", GUIDE_EXAMPLES, ids=[f"example {number}" for number in range(1, 9)])
def test_every_example_of_the_guide_runs_as_shown(example: str) -> None:
    if "import numpy" in example and importlib.util.find_spec("numpy") is None:
        pytest.skip("the example needs NumPy")

    finished = subprocess.run([sys.executable, "-c", example], capture_output=True, text=True, check=False)

    assert finished.returncode == 0, finished.stderr
