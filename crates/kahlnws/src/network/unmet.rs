// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Why a call on a network's arc or node has nothing to work on: the one statement of the conditions every entry of a
//! network states before it reads or changes anything.

use core::fmt;

/// The condition a call on a network's arc or node did not meet, with the values that say so: what each entry of a
/// [`Network`](crate::Network) panics with, under its own name, where a caller handed it an arc or a node past the
/// counts, or a panic left the network half changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Unmet {
    /// A panic left the network half changed, and its columns may be half written.
    HalfChanged,
    /// The arc asked for is not below the arc count.
    PastTheArcs {
        /// The arc asked for.
        arc: usize,
        /// The number of arcs.
        arc_count: usize,
    },
    /// The node asked for is not below the node count.
    PastTheNodes {
        /// The node asked for.
        node: usize,
        /// The number of nodes.
        node_count: usize,
    },
    /// One of the two nodes asked for is not below the node count.
    PairPastTheNodes {
        /// The first node asked for.
        first: usize,
        /// The second node asked for.
        second: usize,
        /// The number of nodes.
        node_count: usize,
    },
}

impl fmt::Display for Unmet {
    /// The condition, as it follows the name of the call in a panic: "with arc 3, which should be below the arc count
    /// 3".
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::HalfChanged => formatter.write_str("on a network a panic left half changed"),
            Self::PastTheArcs { arc, arc_count } => {
                write!(formatter, "with arc {arc}, which should be below the arc count {arc_count}")
            }
            Self::PastTheNodes { node, node_count } => {
                write!(formatter, "with node {node}, which should be below the node count {node_count}")
            }
            Self::PairPastTheNodes { first, second, node_count } => write!(
                formatter,
                "with the nodes {first} and {second}, which should be below the node count {node_count}"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_each_condition_reads_after_the_name_with_its_values_by_hand() {
        assert_eq!(Unmet::HalfChanged.to_string(), "on a network a panic left half changed");
        let past = Unmet::PastTheArcs { arc: 3, arc_count: 2 };
        assert_eq!(past.to_string(), "with arc 3, which should be below the arc count 2");
        let past = Unmet::PastTheNodes { node: 5, node_count: 0 };
        assert_eq!(past.to_string(), "with node 5, which should be below the node count 0");
        let past = Unmet::PairPastTheNodes { first: 0, second: 3, node_count: 3 };
        assert_eq!(past.to_string(), "with the nodes 0 and 3, which should be below the node count 3");
    }
}
