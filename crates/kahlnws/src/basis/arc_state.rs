// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Where an arc stands in a basis: out of the tree at one of its bounds, or unpriced; and the bound an entering arc
//! leaves.

/// Where an arc stands: one of three states, an enum of one byte.
///
/// Its discriminant is chosen as its pricing sign, so that the sign is the byte read, widened: the arc violates its
/// bound exactly where the sign times its reduced cost is negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i8)]
pub(super) enum ArcState {
    /// Out of the tree, at its capacity: it violates where its reduced cost is positive.
    AtUpper = -1,
    /// In the tree, or of no width: no pivot moves it.
    Unpriced = 0,
    /// Out of the tree, carrying nothing above its lower bound: it violates where its reduced cost is negative.
    AtLower = 1,
}

/// The bound an entering arc leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Bound {
    /// It carries nothing above its lower bound: the pivot raises its flow.
    Lower,
    /// It carries its width: the pivot lowers its flow.
    Upper,
}

impl ArcState {
    /// Where an arc out of the tree of width `width` stands that carries `flow`: unpriced where it has no width, at its
    /// lower bound where it carries nothing, and at its capacity otherwise. An arc out of the tree carries nothing or
    /// its whole width, and is then where this puts it.
    pub(super) fn at(flow: u64, width: u64) -> Self {
        // An arc of no width carries its lower bound in every flow within the bounds: a constant of the problem,
        // already counted in the net supplies. No basis needs it: every node has its root link, so a spanning tree
        // exists without it, and in a strongly feasible tree it could not stand, since it can carry no flow toward the
        // root. At the end its reduced cost may have either sign: no step can move its flow, so it meets the
        // optimality conditions either way.
        match (flow, width) {
            (_, 0) => Self::Unpriced,
            (0, _) => Self::AtLower,
            _ => Self::AtUpper,
        }
    }

    /// What an arc out of the tree that stands here carries of its width `width`, in units of flow: its width where it
    /// stands at its capacity, nothing otherwise.
    pub(super) fn carried(self, width: u64) -> u64 {
        match self {
            Self::AtUpper => width,
            Self::Unpriced | Self::AtLower => 0,
        }
    }

    /// The violation of an arc that stands here at the reduced cost `reduced_cost`: the pricing sign times it.
    pub(super) fn violation(self, reduced_cost: i64) -> i64 {
        // The sign is the discriminant, so the match is the byte read, widened. A reduced cost is below `2^63` in
        // magnitude (`cost_bound`), so its negation is exact.
        let sign = match self {
            Self::AtUpper => -1,
            Self::Unpriced => 0,
            Self::AtLower => 1,
        };

        sign * reduced_cost
    }

    /// The bound an arc that stands here leaves when it enters; `None` where it is not priced.
    pub(super) fn bound(self) -> Option<Bound> {
        match self {
            Self::AtLower => Some(Bound::Lower),
            Self::AtUpper => Some(Bound::Upper),
            Self::Unpriced => None,
        }
    }
}

impl Bound {
    /// The other bound.
    pub(super) fn other(self) -> Self {
        match self {
            Self::Lower => Self::Upper,
            Self::Upper => Self::Lower,
        }
    }

    /// The state of an arc out of the tree at this bound.
    pub(super) fn state(self) -> ArcState {
        match self {
            Self::Lower => ArcState::AtLower,
            Self::Upper => ArcState::AtUpper,
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(in crate::basis) const LOWER: ArcState = ArcState::AtLower;
    pub(in crate::basis) const UPPER: ArcState = ArcState::AtUpper;
    pub(in crate::basis) const IN_TREE: ArcState = ArcState::Unpriced;

    #[test]
    fn test_the_states_and_bounds_by_hand() {
        assert_eq!((ArcState::at(0, 0), ArcState::at(0, 1), ArcState::at(0, u64::MAX)), (IN_TREE, LOWER, LOWER));
        assert_eq!([LOWER, IN_TREE, UPPER].map(|state| state.violation(-4)), [-4, 0, 4]);
        assert_eq!([LOWER, IN_TREE, UPPER].map(ArcState::bound), [Some(Bound::Lower), None, Some(Bound::Upper)]);
        assert_eq!((Bound::Lower.other(), Bound::Upper.other()), (Bound::Upper, Bound::Lower));
        assert_eq!((Bound::Lower.state(), Bound::Upper.state()), (LOWER, UPPER));
    }

    #[test]
    fn test_where_an_arc_stands_for_what_it_carries_by_hand() {
        // No width: unpriced, whatever it carries, which is nothing.
        assert_eq!(ArcState::at(0, 0), IN_TREE);
        // Nothing of a positive width: at its lower bound; its whole width: at its capacity.
        assert_eq!((ArcState::at(0, 3), ArcState::at(3, 3)), (LOWER, UPPER));
        assert_eq!((ArcState::at(0, u64::MAX), ArcState::at(u64::MAX, u64::MAX)), (LOWER, UPPER));
    }

    #[test]
    fn test_what_an_arc_carries_where_it_stands_by_hand() {
        assert_eq!([LOWER, IN_TREE, UPPER].map(|state| state.carried(5)), [0, 0, 5]);
        assert_eq!([LOWER, IN_TREE, UPPER].map(|state| state.carried(0)), [0, 0, 0]);
        assert_eq!(UPPER.carried(u64::MAX), u64::MAX);

        // What an arc carries puts it back where it stood, but for an arc whose width has gone, which is unpriced.
        for state in [LOWER, UPPER] {
            assert_eq!(ArcState::at(state.carried(4), 4), state);
            assert_eq!(ArcState::at(state.carried(0), 0), IN_TREE);
        }
        assert_eq!(ArcState::at(IN_TREE.carried(4), 4), LOWER, "a tree arc carries nothing out of the tree");
    }

    #[test]
    fn test_where_an_arc_stands_for_any_flow_by_hand() {
        // Every pair has an answer: no width is unpriced, nothing is the lower bound, anything else the capacity.
        assert_eq!((ArcState::at(7, 0), ArcState::at(u64::MAX, 0)), (IN_TREE, IN_TREE));
        assert_eq!((ArcState::at(0, 5), ArcState::at(2, 5), ArcState::at(9, 5)), (LOWER, UPPER, UPPER));
        assert_eq!(ArcState::at(u64::MAX, 3), UPPER);
    }
}
