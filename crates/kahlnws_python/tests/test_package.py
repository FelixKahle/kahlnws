# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The package as a whole: its names, its version, `cost_bound`, the reservation, its documents and its stubs, and that
it never imports NumPy."""

import ast
import doctest
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
        "Breach",
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
    assert [kahlnws.NetworkSimplex.__module__, kahlnws.Solution.__module__, kahlnws.ProblemError.__module__] == [
        "kahlnws"
    ] * 3


def test_cost_bound_by_hand() -> None:
    assert kahlnws.cost_bound(0) == 2**62 - 1
    assert kahlnws.cost_bound(1) == 2**62 - 1
    assert kahlnws.cost_bound(1000) == 4_611_686_018_427_387
    assert kahlnws.cost_bound(2**64 - 1) == 0

    with pytest.raises(OverflowError):
        kahlnws.cost_bound(-1)
    with pytest.raises(OverflowError):
        kahlnws.cost_bound(2**64)
    with pytest.raises(TypeError):
        kahlnws.cost_bound(1.0)  # type: ignore[arg-type]  # a float is refused, which the test shows


def test_a_reservation_refused_raises_memory_error_and_keeps_the_solver_usable() -> None:
    # Counts whose room passes what an allocation may ask for: refused before any memory is asked for.
    solver = kahlnws.NetworkSimplex()

    with pytest.raises(MemoryError, match=r"^the room for 18446744073709551615 nodes and 0 arcs was refused$"):
        solver.reserve(2**64 - 1, 0)
    with pytest.raises(MemoryError, match=r"^the room for 0 nodes and 18446744073709551615 arcs was refused$"):
        solver.reserve(0, 2**64 - 1)
    with pytest.raises(OverflowError):
        solver.reserve(-1, 0)

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
    entries = ["solve", "solve_into", "resolve", "resolve_into"]
    assert all(f"NetworkSimplex.{entry}" in pairs for entry in entries)
    assert "Breach.NetSupplyBeyondRange.total" in pairs
    assert (PACKAGE / "py.typed").is_file()


def test_the_package_never_imports_numpy() -> None:
    # A fresh interpreter, in which importing NumPy fails: the package imports, reads every kind of plain column and
    # buffer, solves and resolves at both levels, raises, and NumPy is not among the modules loaded.
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
again = solver.resolve(**depot, costs=(5, 1, 1)).work.pivots
fewer = solver.resolve_into(**depot | {"supplies": [3, 0, -3]}, costs=[5, 1, 1], flows=flows).outcome
try:
    solver.solve(tails=[0], heads=[5], capacities=[1], costs=[1], supplies=[1, -1])
except kahlnws.ProblemError as error:
    breach = error.breach
assert (solution.flows, cost, again, fewer, flows.tolist()) == ([2, 2, 2], 14, 0, 9, [1, 2, 2])
assert breach == kahlnws.Breach.HeadOutsideNodes(arc=0, head=5, node_count=2)
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
