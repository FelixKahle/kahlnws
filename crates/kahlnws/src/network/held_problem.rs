// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The problem a network holds, read in place: the one seam every read of a network's problem goes through.

use super::arc_ends::ArcEnds;
use super::bounds::Bounds;
use super::costs::Costs;
use super::dealt_columns::DealtColumns;
use super::flow_bounds::FlowBounds;
use super::unmet::Unmet;
use crate::basis::{Ends, KeptProblem};
use crate::slot_order::SlotOrder;

/// The numbers of the problem a network holds, read in place: each arc's ends, cost, lower bound and width by slot,
/// through the dealing of the arcs to the slots, and each node's supply by node. It lends the views of the problem's
/// tables, built from columns already dealt, so no view checks a length again, and reads nothing else of the network:
/// no basis, no flow, no mark.
///
/// # Invariant
/// Only [`new_unchecked`](Self::new_unchecked) and its checked twin `new` make one: the three dealt columns are of
/// one dealing, and the supplies hold one entry per node of it.
#[derive(Clone, Copy, Debug)]
pub(super) struct HeldProblem<'n> {
    /// The tail and the head of each arc, by slot.
    ends: DealtColumns<'n, Ends, 1>,
    /// The cost of each arc, by slot, in units of cost.
    costs: DealtColumns<'n, i64, 1>,
    /// The lower bound and the width of each arc, by slot, in units of flow.
    bounds: DealtColumns<'n, u64, 2>,
    /// The supplies, by node, in units of flow.
    supplies: &'n [i64],
}

impl<'n> HeldProblem<'n> {
    /// The problem of the columns `columns` and `lower_bounds`, by slot, and of the supplies `supplies`, by node, of
    /// the arcs and the nodes `order` deals; `None` where a column holds another number of entries than `order` has
    /// arcs, or the supplies another than it has nodes. The twin of [`new_unchecked`](Self::new_unchecked) its tests
    /// check it against.
    #[cfg(test)]
    pub(super) fn new(
        columns: KeptProblem<'n>,
        lower_bounds: &'n [u64],
        supplies: &'n [i64],
        order: SlotOrder,
    ) -> Option<Self> {
        if !whole(columns, lower_bounds, supplies, order) {
            return None;
        }

        debug_assert!(whole(columns, lower_bounds, supplies, order), "the counts should agree, as just compared");
        // SAFETY: every column holds one entry per arc and the supplies one per node, as `whole` just compared.
        Some(unsafe { Self::new_unchecked(columns, lower_bounds, supplies, order) })
    }

    /// The problem of the columns `columns` and `lower_bounds`, by slot, and of the supplies `supplies`, by node, of
    /// the arcs and the nodes `order` deals, as `new` makes it where it makes one, without comparing the lengths.
    ///
    /// # Safety
    /// The ends, the costs and the widths of `columns`, and `lower_bounds`, hold exactly `order.arc_count()` entries
    /// each, and `supplies` exactly `order.node_count()`: the columns `new` admits.
    pub(super) unsafe fn new_unchecked(
        columns: KeptProblem<'n>,
        lower_bounds: &'n [u64],
        supplies: &'n [i64],
        order: SlotOrder,
    ) -> Self {
        debug_assert!(
            whole(columns, lower_bounds, supplies, order),
            "called `HeldProblem::new_unchecked` with columns of other counts than {} arcs and {} nodes",
            order.arc_count(),
            order.node_count()
        );
        // SAFETY: every column holds `order.arc_count()` entries, the caller's promise.
        let (ends, costs, bounds) = unsafe {
            (
                DealtColumns::new_unchecked([columns.ends], order),
                DealtColumns::new_unchecked([columns.costs], order),
                DealtColumns::new_unchecked([lower_bounds, columns.widths], order),
            )
        };

        Self { ends, costs, bounds, supplies }
    }

    /// The tail of each arc.
    pub(super) fn tails(self) -> ArcEnds<'n> {
        ArcEnds::tails(self.ends)
    }

    /// The head of each arc.
    pub(super) fn heads(self) -> ArcEnds<'n> {
        ArcEnds::heads(self.ends)
    }

    /// The cost of each arc, in units of cost.
    pub(super) fn costs(self) -> Costs<'n> {
        Costs::new(self.costs)
    }

    /// The lower bound of each arc, in units of flow.
    pub(super) fn lower_bounds(self) -> FlowBounds<'n> {
        FlowBounds::lower_bounds(self.bounds)
    }

    /// The capacity of each arc, in units of flow.
    pub(super) fn capacities(self) -> FlowBounds<'n> {
        // An arc's lower bound and its width add up to its capacity within `u64` where a network lends them
        // (`Network`, Shape and Basis): no read of the view overflows.
        FlowBounds::capacities(self.bounds)
    }

    /// The supply of each node, in units of flow, node `v`'s at `v`.
    pub(super) fn supplies(self) -> &'n [i64] {
        self.supplies
    }

    /// The tail of `arc`; the refusal where `arc` is not below the arc count.
    ///
    /// # Errors
    /// [`Unmet::PastTheArcs`] where `arc` is not below the arc count.
    pub(super) fn tail(self, arc: usize) -> Result<usize, Unmet> {
        self.tails().get(arc).ok_or_else(|| self.past_the_arcs(arc))
    }

    /// The head of `arc`; the refusal where `arc` is not below the arc count.
    ///
    /// # Errors
    /// [`Unmet::PastTheArcs`] where `arc` is not below the arc count.
    pub(super) fn head(self, arc: usize) -> Result<usize, Unmet> {
        self.heads().get(arc).ok_or_else(|| self.past_the_arcs(arc))
    }

    /// The cost of `arc`, in units of cost; the refusal where `arc` is not below the arc count.
    ///
    /// # Errors
    /// [`Unmet::PastTheArcs`] where `arc` is not below the arc count.
    pub(super) fn cost(self, arc: usize) -> Result<i64, Unmet> {
        self.costs().get(arc).ok_or_else(|| self.past_the_arcs(arc))
    }

    /// The lower bound of `arc`, in units of flow; the refusal where `arc` is not below the arc count.
    ///
    /// # Errors
    /// [`Unmet::PastTheArcs`] where `arc` is not below the arc count.
    pub(super) fn lower_bound(self, arc: usize) -> Result<u64, Unmet> {
        self.lower_bounds().get(arc).ok_or_else(|| self.past_the_arcs(arc))
    }

    /// The capacity of `arc`, in units of flow; the refusal where `arc` is not below the arc count.
    ///
    /// # Errors
    /// [`Unmet::PastTheArcs`] where `arc` is not below the arc count.
    pub(super) fn capacity(self, arc: usize) -> Result<u64, Unmet> {
        self.capacities().get(arc).ok_or_else(|| self.past_the_arcs(arc))
    }

    /// The bounds of `arc`, its lower bound and its capacity in units of flow; the refusal where `arc` is not below
    /// the arc count.
    ///
    /// # Errors
    /// [`Unmet::PastTheArcs`] where `arc` is not below the arc count.
    pub(super) fn bounds(self, arc: usize) -> Result<Bounds, Unmet> {
        // An arc's lower bound and its width add up to its capacity within `u64` where a network lends them
        // (`Network`, Shape and Basis): the sum does not overflow.
        let [lower_bound, width] = self.bounds.get(arc).ok_or_else(|| self.past_the_arcs(arc))?;

        Ok(Bounds::of_width(lower_bound, width))
    }

    /// The supply of `node`, in units of flow; the refusal where `node` is not below the node count.
    ///
    /// # Errors
    /// [`Unmet::PastTheNodes`] where `node` is not below the node count.
    pub(super) fn supply(self, node: usize) -> Result<i64, Unmet> {
        let node_count = self.supplies.len();
        self.supplies.get(node).copied().ok_or(Unmet::PastTheNodes { node, node_count })
    }

    /// The refusal of a read of `arc`, which is not below the arc count.
    fn past_the_arcs(self, arc: usize) -> Unmet {
        Unmet::PastTheArcs { arc, arc_count: self.ends.len() }
    }
}

/// Whether the ends, the costs and the widths of `columns`, and `lower_bounds`, hold one entry per arc `order` deals,
/// and `supplies` one per node: the precondition of [`HeldProblem::new_unchecked`], stated once.
fn whole(columns: KeptProblem<'_>, lower_bounds: &[u64], supplies: &[i64], order: SlotOrder) -> bool {
    let arcs = [columns.ends.len(), columns.costs.len(), columns.widths.len(), lower_bounds.len()];

    arcs.iter().all(|&count| count == order.arc_count()) && supplies.len() == order.node_count()
}

#[cfg(test)]
mod tests {
    use kahlnws_testing::MincostNetwork;

    use super::*;
    use crate::basis::{KeptBasisRoom, kept_star};
    use crate::network::bounds::Bounds;
    use crate::problem::tests::problem_of;

    /// Four arcs on three nodes, arc `a` from node `a mod 3` to node `(a + 1) mod 3` at cost `a` and of width `a + 1`,
    /// dealt to the slots 0, 2, 1, 3 by two nodes a block, kept in `room`; answers the columns kept by slot and the
    /// dealing.
    fn four_arcs(room: &mut KeptBasisRoom) -> (KeptProblem<'_>, SlotOrder) {
        let arcs: Vec<(usize, usize, u64, u64, i64)> = (0..4_u8)
            .map(|arc| (usize::from(arc % 3), usize::from((arc + 1) % 3), 0, u64::from(arc) + 1, i64::from(arc)))
            .collect();
        kept_star(room, &problem_of(&MincostNetwork::from_arcs(&[0; 3], &arcs)));
        let columns = room.kept_problem().expect("the room should hold the basis just kept");

        (columns, SlotOrder::new(4, 3))
    }

    #[test]
    fn test_the_tables_of_four_arcs_read_by_arc_by_hand() {
        let mut room = KeptBasisRoom::new();
        let (columns, order) = four_arcs(&mut room);
        let lower_bounds = [0, 0, 0, 0];
        let problem = HeldProblem::new(columns, &lower_bounds, &[1, 0, -1], order).expect("the counts should agree");

        assert_eq!(problem.tails().iter().collect::<Vec<_>>(), [0, 1, 2, 0]);
        assert_eq!(problem.heads().iter().collect::<Vec<_>>(), [1, 2, 0, 1]);
        assert_eq!(problem.costs().iter().collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert_eq!(problem.capacities().iter().collect::<Vec<_>>(), [1, 2, 3, 4]);
        assert_eq!(problem.supplies(), [1, 0, -1]);
    }

    #[test]
    fn test_each_single_read_answers_its_arc_or_node_and_refuses_past_the_count_by_hand() {
        let mut room = KeptBasisRoom::new();
        let (columns, order) = four_arcs(&mut room);
        // By slot: arcs 1 and 2 are dealt to slots 2 and 1, so arc 1 has the lower bound 1 and arc 3 the bound 2.
        let lower_bounds = [0, 0, 1, 2];
        let problem = HeldProblem::new(columns, &lower_bounds, &[1, 0, -1], order).expect("the counts should agree");

        assert_eq!((problem.tail(3), problem.head(3), problem.cost(3)), (Ok(0), Ok(1), Ok(3)));
        assert_eq!((problem.lower_bound(3), problem.capacity(3), problem.supply(2)), (Ok(2), Ok(6), Ok(-1)));
        assert_eq!(
            (problem.bounds(3), problem.bounds(4)),
            (Ok(Bounds { lower_bound: 2, capacity: 6 }), Err(Unmet::PastTheArcs { arc: 4, arc_count: 4 }))
        );
        assert_eq!(problem.lower_bounds().iter().collect::<Vec<_>>(), [0, 1, 0, 2]);
        assert_eq!(problem.capacities().iter().collect::<Vec<_>>(), [1, 3, 3, 6]);

        let past = Unmet::PastTheArcs { arc: 4, arc_count: 4 };
        assert_eq!([problem.tail(4), problem.head(4)], [Err(past); 2]);
        assert_eq!((problem.cost(4), problem.lower_bound(4), problem.capacity(4)), (Err(past), Err(past), Err(past)));
        assert_eq!(problem.supply(3), Err(Unmet::PastTheNodes { node: 3, node_count: 3 }));
    }

    #[test]
    fn test_new_unchecked_reads_as_new_where_new_admits_the_columns_by_hand() {
        let mut room = KeptBasisRoom::new();
        let (columns, order) = four_arcs(&mut room);
        let (lower_bounds, supplies) = ([0, 0, 1, 2], [1, 0, -1]);
        let checked = HeldProblem::new(columns, &lower_bounds, &supplies, order).expect("the counts should agree");
        debug_assert!(whole(columns, &lower_bounds, &supplies, order), "the counts should agree, as `new` admitted");
        // SAFETY: four entries an arc column and three supplies, as `new` just admitted.
        let unchecked = unsafe { HeldProblem::new_unchecked(columns, &lower_bounds, &supplies, order) };

        assert_eq!((unchecked.tails(), unchecked.heads()), (checked.tails(), checked.heads()));
        assert_eq!((unchecked.costs(), unchecked.bounds(3)), (checked.costs(), checked.bounds(3)));
        assert_eq!((unchecked.lower_bounds(), unchecked.capacities()), (checked.lower_bounds(), checked.capacities()));
        assert_eq!((unchecked.supplies(), unchecked.supply(3)), (checked.supplies(), checked.supply(3)));
    }

    #[test]
    fn test_new_refuses_a_column_of_another_count_by_hand() {
        let mut room = KeptBasisRoom::new();
        let (columns, order) = four_arcs(&mut room);
        let supplies = [1, 0, -1];

        assert!(HeldProblem::new(columns, &[0, 0, 0], &supplies, order).is_none(), "three lower bounds for four arcs");
        assert!(HeldProblem::new(columns, &[0; 4], &supplies[..2], order).is_none(), "two supplies for three nodes");
        let costs = KeptProblem { costs: &columns.costs[..3], ..columns };
        assert!(HeldProblem::new(costs, &[0; 4], &supplies, order).is_none(), "three costs for four arcs");
        let none = KeptProblem { ends: &[], costs: &[], widths: &[] };
        assert!(HeldProblem::new(none, &[], &[], SlotOrder::new(0, 0)).is_some(), "the problem of no node");
    }
}
