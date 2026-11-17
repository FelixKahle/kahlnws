// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a solve of either front end answers: its outcome, optimal or infeasible, the optimum owned, and the work it
//! did. The optimum each front end lends is its own.

/// What a solve found: a flow of least cost, or that no flow exists.
///
/// What an optimal solve carries is the front end's own: a solver's answers an owned [`Solution`] or an
/// [`Optimum`](crate::Optimum) that lends the potentials, and a network's the total cost, its optimum lent by
/// [`Network::optimum`](crate::Network::optimum); either solve is optimal or infeasible alike, so the one type serves
/// both.
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

/// What an optimal solve found, owned: the flows, the potentials and the cost. Plain data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Solution {
    /// The flow of each arc, in units of flow, one per arc.
    pub flows: Vec<u64>,
    /// Each node's potential, in units of cost, node `v` at `v`: with them, `cost + potential[tail] − potential[head]`
    /// is at least zero on every arc whose flow is below its capacity and at most zero on every arc whose flow is above
    /// its lower bound, which proves the flow optimal.
    pub potentials: Vec<i64>,
    /// The cost of the flow, in units of cost times units of flow.
    #[doc(alias = "objective")]
    pub total_cost: i128,
}

/// What a solve answers: its outcome, and the work it did to reach it, as one value.
///
/// The outcome and the work travel together so that the counts are never read for another solve than the one they
/// count. Plain data. A refused solve answers no `Answer` at all, since nothing ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Answer<T> {
    /// What the solve found: an optimal flow as the front end answers it, a [`Solution`] or an
    /// [`Optimum`](crate::Optimum) of a solver, the total cost of a network, or that no flow exists.
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
/// The counts of a [`Network`](crate::Network)'s solve are a function of the calls made on that network, in order,
/// its loads among them, since a load starts from the basis the network held. In every `Work`,
/// `degenerate_pivots <= pivots` and `first_arc_pivots <= pivots`, and the pivots that sent flow are
/// `pivots - degenerate_pivots`. A solve that answers ran `pivots - first_arc_pivots + 1` block searches, and a
/// network's solve `pivots + 1` searches of its marked blocks.
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
    /// or none violated, and in a network's solve, which tries none; `pivots` where
    /// those arcs alone reached the outcome, optimal or infeasible.
    pub first_arc_pivots: u64,
}

impl Work {
    /// Counts one pivot more, which sent `sent` units of flow around its cycle: a degenerate one where it sent
    /// nothing. The pivots of the first arcs are not counted here.
    ///
    /// # Panics
    /// Panics if the pivots, or the degenerate ones, would pass `u64::MAX`, which no solve reaches.
    #[track_caller]
    pub(crate) fn count_pivot(&mut self, sent: u64) {
        // No run makes `2^64` pivots: at ten billion pivots a second, more than any machine makes, that would take
        // fifty-eight years. The check states the bound and never fires.
        self.pivots = self.pivots.strict_add(1);
        // `Work` is plain data and keeps no relation between its counts, so this sum is checked as the one above.
        self.degenerate_pivots = self.degenerate_pivots.strict_add(u64::from(sent == 0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_a_pivot_is_counted_and_as_degenerate_where_it_sent_nothing_by_hand() {
        let mut work = Work { pivots: 4, degenerate_pivots: 1, first_arc_pivots: 2 };

        work.count_pivot(3);
        assert_eq!(work, Work { pivots: 5, degenerate_pivots: 1, first_arc_pivots: 2 });
        work.count_pivot(0);
        assert_eq!(work, Work { pivots: 6, degenerate_pivots: 2, first_arc_pivots: 2 });
    }

    #[test]
    #[should_panic(expected = "attempt to add with overflow")]
    fn test_a_pivot_past_u64_max_is_refused() {
        let mut work = Work { pivots: u64::MAX, degenerate_pivots: 0, first_arc_pivots: 0 };

        work.count_pivot(1);
    }

    #[test]
    #[should_panic(expected = "attempt to add with overflow")]
    fn test_a_degenerate_pivot_past_u64_max_is_refused() {
        let mut work = Work { pivots: 0, degenerate_pivots: u64::MAX, first_arc_pivots: 0 };

        work.count_pivot(0);
    }

    #[test]
    fn test_optimal_answers_the_optimum_and_nothing_where_infeasible() {
        // A payload lent, as a front end lends its optimum: a borrowed slice.
        let potentials = [0, -3];
        assert_eq!(Outcome::Optimal(&potentials[..]).optimal(), Some(&[0, -3][..]));
        assert_eq!(Outcome::<&[i64]>::Infeasible.optimal(), None);

        let solution = Solution { flows: vec![2, 2], potentials: vec![0, -3], total_cost: 14 };
        let answered = Outcome::Optimal(solution).optimal().expect("an optimal outcome should hold its solution");
        assert_eq!((answered.flows, answered.potentials, answered.total_cost), (vec![2, 2], vec![0, -3], 14));
        assert_eq!(Outcome::<Solution>::Infeasible.optimal(), None);
    }
}
