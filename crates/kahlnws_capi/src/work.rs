// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a solve did, as plain data C reads: its pivots, counted exactly.

/// What one solve did, counted in pivots: exact counts of that solve, not samples or estimates. `kahlnws_solver_solve`
/// and `kahlnws_solver_resolve` write it, where asked, on `KAHLNWS_STATUS_OPTIMAL` and on `KAHLNWS_STATUS_INFEASIBLE`.
///
/// A pivot is an arc brought into the basis, a spanning tree, or flipped to its other bound. Either way the arc
/// violated its bound, and the most flow the cycle it closes with the tree carries is sent around that cycle: where a
/// link of the tree has the least room, that link leaves and the arc takes its place; where the arc's own capacity
/// less its lower bound is the least, no link leaves and the arc moves to its other bound, a flip. A flip sends that
/// whole difference, which is above zero, since an arc whose capacity equals its lower bound never enters.
///
/// Plain data. The counts `kahlnws_solver_solve` writes depend on the problem alone, its arc order included, and on
/// the library's version: the same problem gives the same work from a fresh solver and from one that solved others
/// before. Those `kahlnws_solver_resolve` writes depend on the problem and on the solves the solver made before it,
/// and are those of `kahlnws_solver_solve` where it does not start from the kept basis. In every `kahlnws_work` a
/// solve writes, `degenerate_pivots <= pivots` and `first_arc_pivots <= pivots`, and the pivots that sent flow are
/// `pivots - degenerate_pivots`. A solve that answers ran `pivots - first_arc_pivots + 1` block searches.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Work {
    /// The pivots the solve made, flips included. 0 where no arc of the problem ever violated its bound, as in a
    /// problem of no arc.
    pub pivots: u64,
    /// Of the pivots, those that sent no flow: the link that left had no room, so the flow is as it was and only the
    /// tree and the potentials changed. At most `pivots`; never a flip. 0 where every pivot sent flow.
    pub degenerate_pivots: u64,
    /// Of the pivots, those made by the arcs the solve tries before block search, chosen from the problem's
    /// structure, each where it violated its bound when its turn came. At most `pivots`. 0 where none was chosen or
    /// none violated, and in a resolve from the kept basis, which tries none; `pivots` where those arcs alone reached
    /// the outcome, optimal or infeasible.
    pub first_arc_pivots: u64,
}

impl From<kahlnws::Work> for Work {
    fn from(work: kahlnws::Work) -> Self {
        let kahlnws::Work { pivots, degenerate_pivots, first_arc_pivots } = work;

        Self { pivots, degenerate_pivots, first_arc_pivots }
    }
}

#[cfg(test)]
mod tests {
    use core::mem::{align_of, offset_of, size_of};

    use super::*;

    #[test]
    fn test_the_layout_c_reads_is_three_64_bit_counts_in_the_declared_order() {
        let offsets =
            [offset_of!(Work, pivots), offset_of!(Work, degenerate_pivots), offset_of!(Work, first_arc_pivots)];

        assert_eq!(offsets, [0, 8, 16]);
        assert_eq!((size_of::<Work>(), align_of::<Work>()), (24, 8));
    }

    #[test]
    fn test_the_work_of_the_library_is_carried_count_by_count() {
        let work = kahlnws::Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 2 };

        assert_eq!(Work::from(work), Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 2 });
    }
}
