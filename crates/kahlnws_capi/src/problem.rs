// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The problem as C gives it: its two counts and a pointer to each of its tables; and its tables once lent.

/// One min-cost flow problem as C lays it out: `arc_count` arcs between `node_count` nodes, each table given by a
/// pointer to its first entry. Arc `a` is entry `a` of every per-arc table and node `v` entry `v` of the supplies.
///
/// Plain data the caller fills in. A call reads it and the tables it points to while the call runs, changes none of
/// them, and keeps nothing of them afterwards. Self-loops and parallel arcs are allowed.
///
/// What each pointer must point to:
///
/// - `tails`, `heads`, `capacities` and `costs`: `arc_count` entries each;
/// - `lower_bounds`: null, where every arc's lower bound is zero, or `arc_count` entries;
/// - `supplies`: `node_count` entries.
///
/// A table of no entries needs no storage: where its count is zero, a pointer may be null and is not read. Every
/// other pointer is non-null, aligned for its type, and points to that many initialised entries within one object,
/// which nothing writes while a call reads them; several calls may read the same tables at once. Whether the numbers
/// lie in the range a solve admits is the solve's to check; one that does not is refused with its breach named.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Problem {
    /// The number of nodes: the entries of `supplies`.
    pub node_count: usize,
    /// The number of arcs: the entries of each per-arc table.
    pub arc_count: usize,
    /// The node each arc leaves, one per arc.
    pub tails: *const usize,
    /// The node each arc enters, one per arc.
    pub heads: *const usize,
    /// The least flow of each arc, in units of flow, one per arc; null where every arc's is zero.
    pub lower_bounds: *const u64,
    /// The most flow of each arc, in units of flow, one per arc.
    pub capacities: *const u64,
    /// The cost of a unit of flow on each arc, in units of cost, of either sign, one per arc.
    pub costs: *const i64,
    /// The supply of each node, in units of flow, one per node: positive where it sends flow, negative where it takes
    /// flow. They add up to zero in a problem a solve admits.
    pub supplies: *const i64,
}

/// The tables of a problem C gave, lent for one call as the library's problem.
///
/// # Invariant
/// Only [`LentProblem::new`] makes one: every per-arc table holds one entry per tail, as one arc count gives them, so
/// neither a solve nor a load refuses it as `ProblemError::Length`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LentProblem<'a> {
    /// The problem, its tables borrowed from C.
    problem: kahlnws::Problem<'a>,
}

impl<'a> LentProblem<'a> {
    /// The tables of `problem`, lent.
    ///
    /// # Panics
    /// Panics if a per-arc table of `problem` holds another number of entries than its tails.
    #[track_caller]
    pub(crate) fn new(problem: kahlnws::Problem<'a>) -> Self {
        let arc_count = problem.tails.len();
        let lower_bounds = problem.lower_bounds.map_or(arc_count, <[u64]>::len);
        let tables = [problem.heads.len(), lower_bounds, problem.capacities.len(), problem.costs.len()];
        assert!(
            tables.iter().all(|&length| length == arc_count),
            "called `LentProblem::new` with per-arc tables of lengths {tables:?}, which should each be the arc count \
             {arc_count}"
        );

        Self { problem }
    }

    /// The problem, as the library takes it.
    pub(crate) fn problem(&self) -> &kahlnws::Problem<'a> {
        &self.problem
    }

    /// The number of nodes of the problem.
    pub(crate) fn node_count(&self) -> usize {
        self.problem.supplies.len()
    }

    /// The number of arcs of the problem.
    pub(crate) fn arc_count(&self) -> usize {
        self.problem.tails.len()
    }
}

#[cfg(test)]
mod tests {
    use core::mem::{offset_of, size_of};

    use super::*;

    /// The depot: node 0 sends 4 units to node 2, directly or through node 1.
    fn depot() -> kahlnws::Problem<'static> {
        kahlnws::Problem {
            tails: &[0, 0, 1],
            heads: &[2, 1, 2],
            lower_bounds: None,
            capacities: &[3, 4, 2],
            costs: &[5, 1, 1],
            supplies: &[4, 0, -4],
        }
    }

    #[test]
    fn test_a_lent_problem_answers_its_counts_and_its_tables() {
        let lent = LentProblem::new(depot());

        assert_eq!((lent.node_count(), lent.arc_count()), (3, 3));
        assert_eq!(lent.problem().costs, &[5, 1, 1]);
        assert_eq!(LentProblem::new(kahlnws::Problem { lower_bounds: Some(&[0, 1, 0]), ..depot() }).arc_count(), 3);
    }

    #[test]
    #[should_panic(expected = "called `LentProblem::new` with per-arc tables of lengths [3, 3, 2, 3], which should \
                               each be the arc count 3")]
    fn test_a_problem_with_a_table_of_another_length_is_not_lent() {
        let _ = LentProblem::new(kahlnws::Problem { capacities: &[3, 4], ..depot() });
    }

    #[test]
    #[should_panic(expected = "called `LentProblem::new` with per-arc tables of lengths [3, 2, 3, 3], which should \
                               each be the arc count 3")]
    fn test_a_problem_with_lower_bounds_of_another_length_is_not_lent() {
        let _ = LentProblem::new(kahlnws::Problem { lower_bounds: Some(&[0, 0]), ..depot() });
    }

    #[test]
    fn test_the_layout_c_reads_is_eight_words_in_the_declared_order() {
        let word = size_of::<usize>();
        let offsets = [
            offset_of!(Problem, node_count),
            offset_of!(Problem, arc_count),
            offset_of!(Problem, tails),
            offset_of!(Problem, heads),
            offset_of!(Problem, lower_bounds),
            offset_of!(Problem, capacities),
            offset_of!(Problem, costs),
            offset_of!(Problem, supplies),
        ];

        assert_eq!(offsets, [0, 1, 2, 3, 4, 5, 6, 7].map(|field| field * word));
        assert_eq!(size_of::<Problem>(), 8 * word);
    }
}
