// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a solve answers: its outcome, optimal or infeasible, the optimum lent or owned, and the work it did.

/// What a solve found: a flow of least cost, or that no flow exists.
///
/// The optimum of an optimal solve comes lent, as an [`Optimum`] beside the flows written to the caller's buffer, or
/// owned, as a [`Solution`]; either solve is optimal or infeasible alike, so the one type serves the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Outcome<T> {
    /// A flow of least cost, with what the solve found about it.
    Optimal(T),
    /// No flow within the bounds meets the supplies.
    Infeasible,
}

impl<T> Outcome<T> {
    /// What an optimal solve found; `None` where the problem is infeasible.
    pub fn optimal(self) -> Option<T> {
        match self {
            Self::Optimal(optimum) => Some(optimum),
            Self::Infeasible => None,
        }
    }
}

/// What an optimal solve found beside the flows it wrote to the caller's buffer, the potentials lent by the solver.
/// Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Optimum<'s> {
    /// The cost of the flow, in units of cost times units of flow.
    pub total_cost: i128,
    /// Each node's potential, in units of cost, one per node: with them, `cost + potential[tail] − potential[head]`
    /// is at least zero on every arc whose flow is below its capacity and at most zero on every arc whose flow is
    /// above its lower bound, which proves the flow optimal.
    pub potentials: &'s [i64],
}

/// What an optimal solve found, owned: the flows, the potentials and the cost. Plain data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Solution {
    /// The flow of each arc, in units of flow, one per arc.
    pub flows: Vec<u64>,
    /// Each node's potential, as [`Optimum::potentials`].
    pub potentials: Vec<i64>,
    /// The cost of the flow, in units of cost times units of flow.
    pub total_cost: i128,
}

/// What a solve answers: its outcome, and the work it did to reach it, as one value.
///
/// The outcome and the work travel together so that the counts are never read for another solve than the one they
/// count. Plain data. A refused solve answers no `Answer` at all, since nothing ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Answer<T> {
    /// What the solve found: an optimal flow, lent as an [`Optimum`] or owned as a [`Solution`], or that no flow
    /// exists.
    pub outcome: Outcome<T>,
    /// The work the solve did, optimal or infeasible alike.
    pub work: Work,
}

/// What one solve did, counted in pivots: exact counts of that solve, not samples or estimates. Plain data.
///
/// A pivot is an arc brought into the basis, or flipped to its other bound. Either way the arc violated its bound,
/// and the most flow the cycle it closes with the tree carries is sent around that cycle: where a link of the tree has
/// the least room, that link leaves and the arc takes its place; where the arc's own width is the least, no link
/// leaves and the arc moves to its other bound, a flip. A flip sends the arc's whole width, which is above zero, since
/// an arc of no width never enters.
///
/// The counts of a solve are a function of the problem as given, its arc order included, and of the crate's version, as
/// the outcome is: the same problem gives the same work from a fresh solver and from one that solved others before.
/// The counts of a resolve are a function of the problem and of the solves the solver made before it, and are those of
/// a solve where it does not start from the kept basis. In every `Work`, `degenerate_pivots <= pivots` and
/// `first_arc_pivots <= pivots`, and the pivots that sent flow are `pivots - degenerate_pivots`. A solve or a resolve
/// that answers ran `pivots - first_arc_pivots + 1` block searches.
///
/// ```
/// use kahlnws::{Answer, NetworkSimplex, Outcome, Problem, ProblemError, Work};
///
/// fn main() -> Result<(), ProblemError> {
///     // Node 0 must send 2 units to node 1, but its one arc carries 1: the arc fills, and no more can move.
///     let problem = Problem {
///         tails: &[0],
///         heads: &[1],
///         lower_bounds: None,
///         capacities: &[1],
///         costs: &[1],
///         supplies: &[2, -2],
///     };
///
///     let Answer { outcome, work } = NetworkSimplex::new().solve(&problem)?;
///     assert_eq!(outcome, Outcome::Infeasible);
///     assert_eq!(work, Work { pivots: 1, degenerate_pivots: 0, first_arc_pivots: 0 });
///
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Work {
    /// The pivots the solve made, flips included. Zero where no arc of the problem ever violated its bound, as in a
    /// problem of no arc.
    pub pivots: u64,
    /// Of the pivots, those that sent no flow: the link that left had no room, so the flow is as it was and only the
    /// tree and the potentials changed. At most `pivots`; never a flip. Zero where every pivot sent flow.
    pub degenerate_pivots: u64,
    /// Of the pivots, those made by the arcs the solve tries before block search, chosen from the problem's
    /// structure, each where it violated its bound when its turn came. At most `pivots`. Zero where none was chosen
    /// or none violated, and in a resolve from the kept basis, which tries none; `pivots` where those arcs alone
    /// reached the outcome, optimal or infeasible.
    pub first_arc_pivots: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_optimal_answers_the_optimum_and_nothing_where_infeasible() {
        let potentials = [0, -3];
        let optimum = Optimum { total_cost: 14, potentials: &potentials };
        assert_eq!(Outcome::Optimal(optimum).optimal(), Some(Optimum { total_cost: 14, potentials: &[0, -3] }));
        assert_eq!(Outcome::<Optimum<'_>>::Infeasible.optimal(), None);

        let solution = Solution { flows: vec![2, 2], potentials: vec![0, -3], total_cost: 14 };
        let answered = Outcome::Optimal(solution).optimal().expect("an optimal outcome should hold its solution");
        assert_eq!((answered.flows, answered.potentials, answered.total_cost), (vec![2, 2], vec![0, -3], 14));
        assert_eq!(Outcome::<Solution>::Infeasible.optimal(), None);
    }
}
