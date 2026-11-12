# Copyright 2026 Felix Kahle
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The charts of the comparison `solvers.py` measures, drawn from the table it prints.

Two charts, each for a light and for a dark page: the time of one solve against the arcs of the network, and how many
times the time of kahlnws each LP solver took. Both have one panel per family and logarithmic axes, since the networks
grow by about four times from row to row. A solver keeps its colour and its mark in every panel.

Run with an environment that holds Altair and `vl-convert-python`, the table on the standard input:

    python crates/kahlnws_python/benches/solvers.py > table.md
    python crates/kahlnws_python/benches/charts.py docs/img < table.md

It writes `solve-times-light.svg`, `solve-times-dark.svg`, `slowdown-light.svg` and `slowdown-dark.svg` into the
folder it is given.
"""

import re
import sys
from dataclasses import dataclass
from pathlib import Path

import altair as alt

# The solvers in the order of the table's columns, kahlnws first.
SOLVERS = ("kahlnws", "HiGHS", "Gurobi", "Gurobi, network simplex")

# The mark of each solver, so that a solver is told by more than its colour.
SHAPES = ("circle", "square", "triangle-up", "diamond")

# What each family's panel is called.
FAMILIES = {"grid": "Grid", "transport": "Transport"}


@dataclass(frozen=True)
class Theme:
    """The colours of a chart on one kind of page."""

    name: str
    surface: str
    text: str
    muted: str
    grid: str
    # The colour of each solver, in the order of `SOLVERS`.
    series: tuple[str, str, str, str]


THEMES = (
    Theme("light", "#fcfcfb", "#0b0b0b", "#52514e", "#e4e3df", ("#2a78d6", "#eb6834", "#1baf7a", "#eda100")),
    Theme("dark", "#1a1a19", "#f0efec", "#c3c2b7", "#383835", ("#3987e5", "#d95926", "#199e70", "#c98500")),
)


def measurements(table: str) -> list[dict[str, object]]:
    """The measurements of the table `solvers.py` prints, one per row and solver: the family, the arcs, the solver, the
    milliseconds of one solve, and how many times the time of kahlnws that is."""
    found = []

    for line in table.splitlines():
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        row = re.fullmatch(r"`(\w+)/\d+`", cells[0])
        if row is None:
            continue

        arcs = int(cells[2].replace(",", ""))
        times = [float(cell.split(" ms")[0]) for cell in cells[3:]]

        for solver, milliseconds in zip(SOLVERS, times, strict=True):
            found.append(
                {
                    "family": FAMILIES[row[1]],
                    "arcs": arcs,
                    "solver": solver,
                    "milliseconds": milliseconds,
                    "slowdown": milliseconds / times[0],
                }
            )

    return found


def panels(
    rows: list[dict[str, object]],
    theme: Theme,
    *,
    measure: str,
    measure_title: str,
    ticks: list[float],
    title: str,
    subtitle: str,
) -> alt.TopLevelMixin:
    """A line of `measure` against the arcs per solver, one panel per family, in the colours of `theme`, with a grid
    line at each of `ticks`. A solver wears the same colour and mark whether or not the others are drawn."""
    drawn = [solver for solver in SOLVERS if any(row["solver"] == solver for row in rows)]
    colours = [theme.series[SOLVERS.index(solver)] for solver in drawn]
    shapes = [SHAPES[SOLVERS.index(solver)] for solver in drawn]
    solver = alt.Color(
        "solver:N",
        scale=alt.Scale(domain=drawn, range=colours),
        legend=alt.Legend(title=None, orient="top", symbolStrokeWidth=2, symbolSize=90),
    )
    base = alt.Chart(alt.Data(values=rows)).encode(
        x=alt.X(
            "arcs:Q",
            scale=alt.Scale(type="log", domain=[100, 100_000]),
            title="Arcs",
            axis=alt.Axis(format="~s", values=[100, 1_000, 10_000, 100_000]),
        ),
        y=alt.Y(
            f"{measure}:Q",
            scale=alt.Scale(type="log", domain=[ticks[0], ticks[-1]]),
            title=measure_title,
            axis=alt.Axis(format="~g", values=ticks),
        ),
        color=solver,
        shape=alt.Shape("solver:N", scale=alt.Scale(domain=drawn, range=shapes), legend=None),
    )
    lines = base.mark_line(strokeWidth=2) + base.mark_point(size=70, filled=True, opacity=1)

    return (
        lines.properties(width=330, height=260)
        .facet(column=alt.Column("family:N", title=None, header=alt.Header(labelFontSize=13, labelFontWeight=600)))
        .properties(title=alt.Title(title, subtitle=subtitle, anchor="start", fontSize=16, subtitleFontSize=12))
        .configure(background=theme.surface, font="Helvetica Neue, Helvetica, Arial, sans-serif")
        .configure_view(stroke=None)
        .configure_axis(
            gridColor=theme.grid,
            domainColor=theme.grid,
            tickColor=theme.grid,
            labelColor=theme.muted,
            titleColor=theme.muted,
            titleFontWeight="normal",
            labelFontSize=11,
            titleFontSize=12,
        )
        .configure_legend(labelColor=theme.text, labelFontSize=12, symbolType="circle")
        .configure_header(labelColor=theme.text)
        .configure_title(color=theme.text, subtitleColor=theme.muted, offset=12)
    )


def main() -> None:
    folder = Path(sys.argv[1])
    rows = measurements(sys.stdin.read())
    others = [row for row in rows if row["solver"] != SOLVERS[0]]

    for theme in THEMES:
        panels(
            rows,
            theme,
            measure="milliseconds",
            measure_title="Milliseconds per solve",
            ticks=[0.001, 0.01, 0.1, 1, 10, 100, 1_000, 10_000],
            title="The time of one cold solve",
            subtitle="Median of seven solves; both axes are logarithmic",
        ).save(folder / f"solve-times-{theme.name}.svg")
        panels(
            others,
            theme,
            measure="slowdown",
            measure_title="Times the time of kahlnws",
            ticks=[0.1, 1, 10, 100],
            title="The LP solvers beside kahlnws",
            subtitle="Time of one solve divided by the time of kahlnws on the same network; 1 is as fast as kahlnws",
        ).save(folder / f"slowdown-{theme.name}.svg")


if __name__ == "__main__":
    main()
