// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! An arc restated for new bounds: its new width and what it carries above its new lower bound before any flow moves,
//! and where it then stands, in the tree or out of it at a bound, with the flow that moves.

use super::arc_state::ArcState;
use super::tree_arc_flow::beyond_width;

/// The new bounds of an arc, in the terms of a basis, which knows no lower bound: its new width, and how far its lower
/// bound falls. What the arc carries above its new lower bound is what it carries above its old one plus that fall:
/// restating it moves no flow, so no node's balance and no cost changes with it. The flow is the basis's own, read at
/// the arc's slot where it is restated, so a restatement cannot be made of another flow than the one the arc holds.
///
/// # Invariant
/// Only [`new`](Self::new) makes one: `lower_fall` is a lower bound below `2^64` less another, so it lies within
/// `−2^64 < lower_fall < 2^64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Restated {
    /// The arc's new width, its new capacity less its new lower bound, in units of flow.
    width: u64,
    /// The old lower bound less the new, in units of flow: below zero where the lower bound rises.
    lower_fall: i128,
}

/// An arc's numbers restated for new bounds, before any flow moves: its new width, and what it carries above its new
/// lower bound, which may lie outside `0..=width`.
///
/// # Invariant
/// Only [`Restated::of_flow`] makes one: `flow` is a flow below `2^64` plus a fall within `(−2^64, 2^64)`, so it lies
/// within `−2^64 < flow < 2^65`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RestatedArc {
    /// The arc's new width, in units of flow.
    width: u64,
    /// What it carries above its new lower bound, in units of flow.
    flow: i128,
}

/// Where an arc restated for new bounds stands, before the flow it moves is sent through the tree. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Restatement {
    /// The arc is a link of the tree: it holds the restated flow toward the root and takes it as a node's link takes
    /// what arrives, with nothing more arriving. It stays where that flow lies within its new width with room toward
    /// the root; otherwise its node hangs from the root with what its width cuts off.
    InTree,
    /// The arc is out of the tree and carries `flow`, nothing or its new width, standing where [`ArcState::at`] puts
    /// it; what it carries more than the restated flow moves from its head to its tail through the tree.
    OutOfTree {
        /// What it carries above its new lower bound, in units of flow.
        flow: u64,
    },
}

impl Restated {
    /// The new bounds of an arc whose lower bound was `lower_bound`: the lower bound `new_lower_bound` and the width
    /// `width`, in units of flow.
    pub(crate) fn new(lower_bound: u64, new_lower_bound: u64, width: u64) -> Self {
        // Each term is below `2^64`: the difference is exact in an `i128`.
        let lower_fall = i128::from(lower_bound) - i128::from(new_lower_bound);

        Self { width, lower_fall }
    }

    /// The arc that carries `flow` above its old lower bound, restated for these bounds: it carries `flow` plus the
    /// fall of the lower bound above the new one.
    pub(super) fn of_flow(self, flow: u64) -> RestatedArc {
        // A flow below `2^64` and a fall within `(−2^64, 2^64)` (`Restated`): the sum lies within `(−2^64, 2^65)`.
        RestatedArc { width: self.width, flow: i128::from(flow) + self.lower_fall }
    }
}

impl RestatedArc {
    /// The arc's new width, in units of flow.
    pub(super) fn width(self) -> u64 {
        self.width
    }

    /// What the arc carries above its new lower bound, in units of flow: within `−2^64 < flow < 2^65`.
    pub(super) fn flow(self) -> i128 {
        self.flow
    }
}

impl Restatement {
    /// Where an arc of a basis that stood at `state` with the width `width` stands restated as `restated`.
    ///
    /// It is in the tree where it is unpriced and has width: an arc out of the tree that is unpriced has none, and a
    /// tree arc has some, since a link of no width could carry nothing toward the root (`Basis`, Tree, Flow and Strong
    /// feasibility). Out of the tree it stays where it is restated to carry nothing or its whole new width; otherwise
    /// it goes to the bound its state names, carrying what [`ArcState::carried`] gives it of its new width: an arc at
    /// one of its bounds stays at that bound, and an arc of no width goes to its new lower bound. Either is a place an
    /// arc out of the tree may stand; where it then violates its bound, the next solve prices it.
    pub(super) fn of(state: ArcState, width: u64, restated: RestatedArc) -> Self {
        if state == ArcState::Unpriced && width > 0 {
            return Self::InTree;
        }

        let flow = match u64::try_from(restated.flow) {
            Ok(flow) if flow == 0 || flow == restated.width => flow,
            _ => state.carried(restated.width),
        };

        Self::OutOfTree { flow }
    }

    /// The flow this restatement of `restated` moves, in units of flow, the most it can raise the flow up the root
    /// links by: for a tree arc how far the restated flow lies outside its new width, which its node's root link takes;
    /// for an arc out of the tree how far what it carries lies from the restated flow, which the tree carries between
    /// its ends. Below `2^66`.
    pub(super) fn amount(self, restated: RestatedArc) -> u128 {
        match self {
            Self::InTree => beyond_width(restated.flow, restated.width),
            Self::OutOfTree { flow } => restated.flow.abs_diff(i128::from(flow)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basis::arc_state::tests::{IN_TREE, LOWER, UPPER};

    #[test]
    fn test_restated_carries_its_flow_less_the_lower_bound_change_by_hand() {
        // A tree arc carrying 2 above a lower bound of 0, raised to 3 with its capacity 4: one below its new bound.
        assert_eq!(Restated::new(0, 3, 1), Restated { width: 1, lower_fall: -3 });
        assert_eq!(Restated::new(0, 3, 1).of_flow(2), RestatedArc { width: 1, flow: -1 });
        // A full arc of width 2 above 1, the lower bound lowered to 0: it carries 3 above it, all of its new width.
        assert_eq!(Restated::new(1, 0, 3).of_flow(2), RestatedArc { width: 3, flow: 3 });

        // The ends of the range.
        let widest = Restated::new(u64::MAX, 0, u64::MAX).of_flow(u64::MAX);
        assert_eq!((widest.width(), widest.flow()), (u64::MAX, 2 * i128::from(u64::MAX)));
        assert_eq!(Restated::new(0, u64::MAX, 0).of_flow(0).flow(), -i128::from(u64::MAX));
    }

    #[test]
    fn test_a_tree_arc_stays_in_the_tree_and_moves_what_its_width_cuts_off_by_hand() {
        // Unpriced with width: in the tree, whatever it is restated as.
        for (flow, width, amount) in [(2, 1, 1), (-1, 0, 1), (1, 3, 0), (3, 3, 0), (-4, 2, 4), (9, 0, 9)] {
            let restated = RestatedArc { width, flow };
            assert_eq!(Restatement::of(IN_TREE, 4, restated), Restatement::InTree);
            assert_eq!(Restatement::InTree.amount(restated), amount, "{restated:?}");
        }
    }

    #[test]
    fn test_an_arc_at_its_lower_bound_stays_there_by_hand() {
        let at = |flow, width| {
            let restated = RestatedArc { width, flow };
            let restatement = Restatement::of(LOWER, 5, restated);
            (restatement, restatement.amount(restated))
        };
        let out = |flow| Restatement::OutOfTree { flow };

        assert_eq!(at(0, 7), (out(0), 0), "a capacity raised over an empty arc: nothing moves");
        assert_eq!(at(0, 2), (out(0), 0), "a capacity lowered over an empty arc: nothing moves");
        assert_eq!(at(-2, 3), (out(0), 2), "a lower bound raised by 2: its flow rises by 2");
        assert_eq!(at(3, 5), (out(0), 3), "a lower bound lowered by 3: its flow falls by 3");
        assert_eq!(at(3, 3), (out(3), 0), "lowered to nothing with its capacity: it stands at its new capacity");
        assert_eq!(at(0, 0), (out(0), 0), "its width taken away: no width, nothing moves");
    }

    #[test]
    fn test_an_arc_at_its_capacity_stays_there_by_hand() {
        let at = |flow, width| {
            let restated = RestatedArc { width, flow };
            let restatement = Restatement::of(UPPER, 2, restated);
            (restatement, restatement.amount(restated))
        };
        let out = |flow| Restatement::OutOfTree { flow };

        assert_eq!(at(2, 1), (out(1), 1), "a capacity lowered under the full arc: its flow falls by 1");
        assert_eq!(at(2, 3), (out(3), 1), "a capacity raised over the full arc: its flow rises by 1");
        assert_eq!(at(1, 1), (out(1), 0), "a lower bound raised under the full arc: nothing moves");
        assert_eq!(at(-1, 1), (out(1), 2), "both bounds raised past it: its flow rises by 2 to the new capacity");
        assert_eq!(at(2, 0), (out(0), 2), "switched off: its flow falls to nothing");
        assert_eq!(at(0, 4), (out(0), 0), "a lower bound raised to its flow: it stands at the lower bound");
    }

    #[test]
    fn test_an_arc_of_no_width_out_of_the_tree_by_hand() {
        let at = |flow, width| {
            let restated = RestatedArc { width, flow };
            let restatement = Restatement::of(IN_TREE, 0, restated);
            (restatement, restatement.amount(restated))
        };
        let out = |flow| Restatement::OutOfTree { flow };

        assert_eq!(at(1, 1), (out(1), 0), "its lower bound lowered by 1: it stands at its capacity");
        assert_eq!(at(0, 2), (out(0), 0), "its capacity raised by 2: it stands at its lower bound");
        assert_eq!(at(1, 3), (out(0), 1), "its lower bound lowered by 1, its capacity raised: to its new lower bound");
        assert_eq!(at(-2, 0), (out(0), 2), "both raised by 2: its flow rises by 2");
    }

    #[test]
    fn test_the_amount_reaches_the_ends_of_the_range_by_hand() {
        // A full arc of width `2^64 − 1` at a lower bound of `2^64 − 1`, restated for no lower bound and no width.
        let restated = Restated::new(u64::MAX, 0, 0).of_flow(u64::MAX);
        let restatement = Restatement::of(UPPER, u64::MAX, restated);
        assert_eq!((restatement, restatement.amount(restated)), (Restatement::OutOfTree { flow: 0 }, (1 << 65) - 2));

        let restated = Restated::new(0, u64::MAX, 0).of_flow(0);
        assert_eq!(Restatement::InTree.amount(restated), u128::from(u64::MAX));
    }
}
