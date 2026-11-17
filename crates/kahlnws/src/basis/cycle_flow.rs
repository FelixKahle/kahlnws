// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What a pivot's cycle does to the links it runs through: the room of a link, its flow once the step is sent, and the
//! entering arc as the link of the end whose subtree an exchange moves.

use core::hint::select_unpredictable;

use super::arc_state::Bound;
use crate::tree::{Direction, Side};

/// The entering arc as the link of the end inside the subtree an exchange moves: which way it runs from that end, and
/// the shift that makes it tight there. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EnteringLink {
    /// Which way the entering arc runs, seen from that end.
    pub(super) direction: Direction,
    /// What every potential of the moved subtree gains, in units of cost.
    pub(super) shift: i64,
}

/// The entering arc, of reduced cost `reduced_cost`, that left `bound`, as the link of the end of the cycle's `side`
/// once an exchange moves the subtree holding it: which way it runs seen from that end, and the shift that makes it
/// tight.
pub(super) fn entering_link(bound: Bound, side: Side, reduced_cost: i64) -> EnteringLink {
    // The first end is the tail at the lower bound and the head at the capacity, and the moved subtree holds the end
    // of the leaving link's side: so it holds the tail exactly at the first side from the lower bound and at the
    // second from the capacity. The arc is tight once `cost + π(tail) − π(head)` is zero: a shift `δ` of the subtree
    // adds `δ` to it where the subtree holds the tail and subtracts `δ` where it holds the head.
    match (bound, side) {
        (Bound::Lower, Side::First) | (Bound::Upper, Side::Second) => {
            EnteringLink { direction: Direction::Up, shift: -reduced_cost }
        }
        (Bound::Lower, Side::Second) | (Bound::Upper, Side::First) => {
            EnteringLink { direction: Direction::Down, shift: reduced_cost }
        }
    }
}

/// The room of a link of capacity `capacity` that carries `flow` and runs `direction` seen from the node below it,
/// where the cycle's flow runs `flow_runs` along the tree there: its capacity less its flow where the two agree, its
/// flow where they do not.
pub(super) fn room(capacity: u64, flow: u64, direction: Direction, flow_runs: Direction) -> u64 {
    // Both are formed and one is selected. A flow lies within `0..=capacity` (`Basis`), so neither underflows; which
    // way a link runs is data, one way as often as the other, so a branch on it would be mispredicted half the time.
    select_unpredictable(direction == flow_runs, capacity - flow, flow)
}

/// The flow of a link that carries `flow` and runs `direction` seen from the node below it, once `step` is sent along
/// the cycle where its flow runs `flow_runs` along the tree there: `step` more where the two agree, `step` less where
/// they do not.
pub(super) fn sent(flow: u64, step: u64, direction: Direction, flow_runs: Direction) -> u64 {
    // The step is at most the link's room (`least_room`), so neither passes the link's bounds.
    if direction == flow_runs { flow + step } else { flow - step }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{DOWN, UP};

    #[test]
    fn test_room_on_all_its_cases_by_hand() {
        assert_eq!(room(5, 2, UP, UP), 3, "along a link up: what is left of its capacity");
        assert_eq!(room(5, 2, DOWN, DOWN), 3, "along a link down");
        assert_eq!(room(5, 2, UP, DOWN), 2, "against a link up: its flow");
        assert_eq!(room(5, 2, DOWN, UP), 2, "against a link down");
        assert_eq!((room(5, 5, UP, UP), room(5, 0, DOWN, UP)), (0, 0), "at a bound");
        assert_eq!(room(u64::MAX, 7, UP, UP), u64::MAX - 7, "a root link the way it points");
        assert_eq!(room(u64::MAX, 7, DOWN, UP), 7, "a root link against the way it points");
    }

    #[test]
    fn test_sent_on_all_its_cases_by_hand() {
        assert_eq!(sent(2, 3, UP, UP), 5);
        assert_eq!(sent(2, 3, DOWN, DOWN), 5);
        assert_eq!(sent(4, 3, UP, DOWN), 1);
        assert_eq!(sent(4, 3, DOWN, UP), 1);
        assert_eq!(sent(4, 0, DOWN, UP), 4, "a step of zero");
    }

    #[test]
    fn test_entering_link_on_all_its_cases_by_hand() {
        // From its lower bound the first end is the tail; from its capacity the head.
        let link = |direction: Direction, shift: i64| EnteringLink { direction, shift };
        assert_eq!(entering_link(Bound::Lower, Side::First, -7), link(UP, 7), "the tail moves");
        assert_eq!(entering_link(Bound::Lower, Side::Second, -7), link(DOWN, -7), "the head moves");
        assert_eq!(entering_link(Bound::Upper, Side::First, 7), link(DOWN, 7), "the head moves");
        assert_eq!(entering_link(Bound::Upper, Side::Second, 7), link(UP, -7), "the tail moves");
    }
}
