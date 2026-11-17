// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! How a tree arc stands once its node's excess is sent along it toward the root: what it carries, what its parent
//! gains, and whether it stays in the tree or leaves it for its node's root link.

use super::arc_state::ArcState;
use crate::starting_basis::{RootLink, root_link};
use crate::tree::Direction;

/// How a tree arc stands once its node's excess is sent along it toward the root. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TreeArcFlow {
    /// What the arc carries, in units of flow above its lower bound: within `0..=width`.
    pub(super) flow: u64,
    /// What the excess of the node's parent gains, in units of flow: the flow where the arc runs up from the node, its
    /// negation where it runs down to it.
    pub(super) to_parent: i128,
    /// Whether the arc stays in the tree.
    pub(super) standing: Standing,
}

/// Whether a tree arc stays in the tree once its node's excess is sent along it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Standing {
    /// It stays, carrying all of the node's excess, and can carry more toward the root.
    InTree,
    /// It leaves the tree at `state`, a bound, and its node hangs from the root by `root_link`, which carries what the
    /// arc could not.
    Leaves {
        /// Where the arc stands out of the tree.
        state: ArcState,
        /// The node's root link.
        root_link: RootLink,
    },
}

/// How a tree arc of width `width`, running `direction` seen from its node, stands once the node's excess `excess` is
/// sent along it toward the root: it carries as much of the excess as it can, `g`, the excess where the arc runs up
/// from the node and its negation where it runs down to it, clamped to `0..=width`; the parent gains `g` or `−g`. The
/// arc stays where it carries the whole excess and can carry more toward the root, less than its width running up and
/// more than nothing running down. Otherwise it leaves at the bound `g` stands at, and the node's root link takes the
/// rest of the excess, as [`root_link`] gives it.
///
/// # Panics
/// Panics if the rest of the excess passes `u64::MAX` in magnitude, which no excess of a start within the limit
/// leaves (`Reflow::flow_up`).
pub(super) fn resend(excess: i128, width: u64, direction: Direction) -> TreeArcFlow {
    // Within `±P` the negation is exact; past it the strict form states the bound.
    let (flow, to_parent, rest, room) = match direction {
        Direction::Up => {
            let flow = within_width(excess, width);
            (flow, i128::from(flow), excess - i128::from(flow), flow < width)
        }
        Direction::Down => {
            let flow = within_width(excess.strict_neg(), width);
            (flow, -i128::from(flow), excess + i128::from(flow), flow > 0)
        }
    };

    // A rest other than zero means the clamp met a bound; no room toward the root means the flow stands at the bound
    // beyond which the arc has none. Either way the arc carries nothing or its width, and stands where `at` puts it.
    let standing = if rest == 0 && room {
        Standing::InTree
    } else {
        Standing::Leaves { state: ArcState::at(flow, width), root_link: root_link(rest) }
    };

    TreeArcFlow { flow, to_parent, standing }
}

/// How far `flow` lies outside `0..=width`, in units of flow: what [`resend`] leaves on the root link of a node whose
/// arc of width `width` held `flow` toward the root, either way, where nothing more arrives at the node. Zero within.
pub(super) fn beyond_width(flow: i128, width: u64) -> u128 {
    flow.abs_diff(i128::from(within_width(flow, width)))
}

/// What an arc of width `width` carries of `demand`, in units of flow: nothing where it is zero or less, the demand
/// up to the width otherwise.
fn within_width(demand: i128, width: u64) -> u64 {
    match u64::try_from(demand) {
        Ok(demand) => demand.min(width),
        Err(_) if demand < 0 => 0,
        Err(_) => width,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::basis::arc_state::tests::{IN_TREE, LOWER, UPPER};
    use crate::ranges::NET_SUPPLY_LIMIT;
    use crate::tree::{DOWN, UP};

    /// A tree arc that stays, carrying `flow` and giving its parent `to_parent`, for a test to write out.
    fn stays(flow: u64, to_parent: i128) -> TreeArcFlow {
        TreeArcFlow { flow, to_parent, standing: Standing::InTree }
    }

    /// A tree arc that leaves at `state`, carrying `flow` and giving its parent `to_parent`, its node hanging from the
    /// root by a root link carrying `rest` the way `direction` says, for a test to write out.
    fn leaves(flow: u64, to_parent: i128, state: ArcState, rest: u64, direction: Direction) -> TreeArcFlow {
        TreeArcFlow {
            flow,
            to_parent,
            standing: Standing::Leaves { state, root_link: RootLink { flow: rest, direction } },
        }
    }

    #[test]
    fn test_resend_up_an_arc_on_every_case_by_hand() {
        // The node is the arc's tail and sends its excess up the arc of width 5, as much as fits.
        assert_eq!(resend(-3, 5, UP), leaves(0, 0, LOWER, 3, DOWN), "a demand: nothing up, the root link brings it");
        assert_eq!(resend(0, 5, UP), stays(0, 0), "nothing to send, room to send more");
        assert_eq!(resend(3, 5, UP), stays(3, 3), "within the width, room left");
        assert_eq!(resend(5, 5, UP), leaves(5, 5, UPPER, 0, UP), "exactly the width: no room left toward the root");
        assert_eq!(resend(8, 5, UP), leaves(5, 5, UPPER, 3, UP), "past the width: the rest up the root link");
    }

    #[test]
    fn test_resend_down_an_arc_on_every_case_by_hand() {
        // The node is the arc's head: the arc brings it the negation of its excess from the parent, as much as fits.
        assert_eq!(resend(-3, 5, DOWN), stays(3, -3), "within the width, room to bring less");
        assert_eq!(resend(0, 5, DOWN), leaves(0, 0, LOWER, 0, UP), "nothing brought: no room toward the root");
        assert_eq!(resend(3, 5, DOWN), leaves(0, 0, LOWER, 3, UP), "a supply: nothing down, the root link takes it");
        assert_eq!(resend(-5, 5, DOWN), stays(5, -5), "exactly the width: it can still bring less");
        assert_eq!(resend(-8, 5, DOWN), leaves(5, -5, UPPER, 3, DOWN), "past the width: the rest down the root link");
    }

    #[test]
    fn test_resend_along_an_arc_of_no_width_always_leaves_by_hand() {
        assert_eq!(resend(2, 0, UP), leaves(0, 0, IN_TREE, 2, UP));
        assert_eq!(resend(0, 0, UP), leaves(0, 0, IN_TREE, 0, UP));
        assert_eq!(resend(-2, 0, DOWN), leaves(0, 0, IN_TREE, 2, DOWN));
        assert_eq!(resend(0, 0, DOWN), leaves(0, 0, IN_TREE, 0, UP));
    }

    #[test]
    fn test_resend_at_the_limit_and_the_widest_width_by_hand() {
        let limit = i128::from(NET_SUPPLY_LIMIT);

        assert_eq!(resend(limit, u64::MAX, UP), stays(NET_SUPPLY_LIMIT, limit));
        assert_eq!(resend(-limit, u64::MAX, DOWN), stays(NET_SUPPLY_LIMIT, -limit));
        assert_eq!(resend(-limit, 1, UP), leaves(0, 0, LOWER, NET_SUPPLY_LIMIT, DOWN));
        assert_eq!(resend(limit, 1, UP), leaves(1, 1, UPPER, NET_SUPPLY_LIMIT - 1, UP));
    }

    #[test]
    fn test_within_width_clamps_by_hand() {
        assert_eq!([-7, 0, 3, 5, 9].map(|demand| within_width(demand, 5)), [0, 0, 3, 5, 5]);
        assert_eq!(within_width(i128::from(u64::MAX) + 1, u64::MAX), u64::MAX);
        assert_eq!(within_width(i128::MIN, 4), 0);
        assert_eq!(within_width(i128::MAX, 4), 4);
    }

    #[test]
    fn test_beyond_width_is_the_rest_resend_leaves_by_hand() {
        assert_eq!([-7, 0, 3, 5, 9].map(|flow| beyond_width(flow, 5)), [7, 0, 0, 0, 4]);
        assert_eq!((beyond_width(2, 0), beyond_width(-2, 0)), (2, 2), "an arc of no width holds nothing");
        assert_eq!(beyond_width(i128::MIN, 4), 1 << 127);
        assert_eq!(beyond_width(i128::MAX, 0), i128::MAX.cast_unsigned());

        // The rest of the arc held `flow` toward the root running up, and `−flow` running down: the same magnitude.
        for flow in [-7, 0, 3, 5, 9] {
            for (direction, held) in [(UP, flow), (DOWN, -flow)] {
                let rest = match resend(held, 5, direction).standing {
                    Standing::InTree => 0,
                    Standing::Leaves { root_link, .. } => u128::from(root_link.flow),
                };
                assert_eq!(rest, beyond_width(flow, 5), "{flow} {direction:?}");
            }
        }
    }
}
