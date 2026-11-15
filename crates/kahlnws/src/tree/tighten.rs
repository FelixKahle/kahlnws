// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The tighten of a tree: the walk of its hung nodes from the root down, each told the cost of its link and given the
//! potential that makes that link tight.

use super::spanning_tree::{Hanging, HungNode, LentTree, TreeColumnsMut, TreeNode, tight_potential};

/// The walk of a tree's hung nodes from the root down, each after its parent: a lending walk whose every step takes
/// the cost of its node's link and makes the link tight. Once every node has been told, every link is tight under the
/// costs told and the root is at potential zero.
///
/// It lends the hung nodes in the order of the thread, each once it is told, and changes no column but the potentials.
/// It owns the tree while it writes them, and [`finish`](Self::finish) hands it back only once every node is told; a
/// walk dropped before that drops the tree with it, so no tree is left with potentials half tight. It costs O(n) and
/// allocates nothing.
///
/// # Invariant
/// Only [`new`](Self::new) makes one, from a tree it takes, setting the root's potential to zero. `next` is the root
/// once every hung node is told, and otherwise the first node of the thread not told; every node before `next` in the
/// thread has the potential that makes its link tight below its parent under the cost it was told.
#[derive(Debug)]
pub(crate) struct TightenWalk<'t> {
    /// The tree tightened.
    tree: LentTree<'t>,
    /// The node the next step lends: the first node of the thread not told, the root once every node is.
    next: TreeNode,
}

/// One node of a tighten, not told yet, lent until it is: the walk moves on only when it is told, and a step that is
/// dropped untold is lent again. Its parent's potential is already tight.
///
/// # Invariant
/// Only [`TightenWalk::step`] makes one, of the walk's `next`, a hung node, every node before it in the thread told.
#[derive(Debug)]
pub(crate) struct TightenStep<'w> {
    /// The columns of the walk's tree.
    columns: TreeColumnsMut<'w>,
    /// The node to tell.
    node: HungNode,
    /// The walk's next node.
    next: &'w mut TreeNode,
}

impl<'t> TightenWalk<'t> {
    /// The tighten of `tree`, the root set at potential zero and no node told yet: its first step lends the thread's
    /// first node after the root.
    pub(crate) fn new(mut tree: LentTree<'t>) -> Self {
        let root = tree.root();

        let mut columns = tree.tree_mut().columns_mut();
        columns.set_potential(root, 0);
        let next = columns.thread(root);

        Self { tree, next }
    }

    /// The tree tightened, every node told, every link tight under the cost it was told.
    ///
    /// # Panics
    /// Panics if a node is still to be told: where [`step`](Self::step) would lend one.
    #[track_caller]
    pub(crate) fn finish(self) -> LentTree<'t> {
        let Self { tree, next } = self;
        assert!(next == tree.root(), "called `TightenWalk::finish` with {next:?} still to be told");

        tree
    }

    /// The next node to tell, whose parent has been told; `None` once every hung node has been, and ever after.
    pub(crate) fn step(&mut self) -> Option<TightenStep<'_>> {
        // The thread is a preorder, so a node's parent comes before it: its potential is tight when the node is lent.
        let root = self.tree.root();
        if self.next == root {
            return None;
        }

        let columns = self.tree.tree_mut().columns_mut();
        debug_assert!(columns.hung(self.next).is_some(), "the walk's next node should be a hung node");
        // SAFETY: `next` is a tree node, below `n + 1` (`TreeNode`), and not the root, `n`, as tested just above
        // (`TightenWalk`).
        let node = unsafe { columns.hung_unchecked(self.next) };

        Some(TightenStep { columns, node, next: &mut self.next })
    }
}

impl TightenStep<'_> {
    /// The link the node hangs by, and which way it runs seen from the node.
    pub(crate) fn hanging(&self) -> Hanging<'_> {
        self.columns.hanging(self.node)
    }

    /// Gives the node the potential that makes its link, of cost `cost` in units of cost, tight below its parent:
    /// wrapping, exact where the potentials and costs lie within the range a basis keeps them in.
    pub(crate) fn tighten(mut self, cost: i64) {
        let node = self.node.node();
        let Hanging { direction, .. } = self.columns.hanging(self.node);
        let parent = self.columns.potential(self.columns.parent(node));

        self.columns.set_potential(node, tight_potential(parent, cost, direction));
        *self.next = self.columns.thread(node);
    }
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;

    use super::*;
    use crate::tree::plain_tree::{PlainColumns, link_count_of, plain_trees, seven_nodes};
    use crate::tree::spanning_tree::TreeRoom;
    use crate::tree::spanning_tree::tests::{columns_of, load};

    /// Tightens `tree`, telling each node the cost of its link from `costs`, one per link, and answers the nodes the
    /// walk lent, in order, with the tree the finished walk hands back.
    ///
    /// # Panics
    /// Panics if the walk lends more nodes than the tree has below the root.
    fn tighten<'r>(tree: LentTree<'r>, costs: &[i64]) -> (Vec<usize>, LentTree<'r>) {
        let hung_nodes = tree.node_count();
        let mut walk = TightenWalk::new(tree);
        let mut lent = Vec::new();

        while let Some(step) = walk.step() {
            assert!(lent.len() < hung_nodes, "a tighten should lend each node below the root once");
            lent.push(usize::from(step.node));
            let cost = costs[usize::from(step.hanging().link)];
            step.tighten(cost);
        }

        (lent, walk.finish())
    }

    /// `columns` with every potential at `potential`: a tree whose potentials no link makes tight.
    fn at_potential(columns: PlainColumns, potential: i64) -> PlainColumns {
        let count = columns.potentials.len();

        PlainColumns { potentials: vec![potential; count], ..columns }
    }

    #[test]
    fn test_a_tighten_makes_every_link_of_the_seven_nodes_tight_by_hand() {
        // Links 0 to 6 cost 2, −3, 5, 7, 1, 4, −6; all run up but 2's. The root at 0; 0 at 0 − 2; 1 at −2 + 3; 2 below
        // a link down at 1 + 5; 3 at 6 − 7; 4 at −2 − 1; 5 at 0 − 4; 6 at −4 + 6.
        let model = seven_nodes();
        let mut costs = vec![0_i64; 14];
        costs[..7].copy_from_slice(&[2, -3, 5, 7, 1, 4, -6]);
        let columns = at_potential(model.columns(&costs), 99);
        let mut room = TreeRoom::new();
        let (lent, tree) = tighten(load(&mut room, &columns, 14), &costs);
        assert_eq!(lent, [0, 1, 2, 3, 4, 5, 6], "the thread from the root");
        let expected = PlainColumns { potentials: vec![-2, 1, 6, -1, -3, -4, 2, 0], ..columns };
        assert_eq!(columns_of(&tree), expected);
    }

    #[test]
    fn test_a_tighten_of_no_node_puts_the_root_at_zero_by_hand() {
        let alone = PlainColumns {
            parents: vec![0],
            hangings: vec![],
            threads: vec![0],
            reverse_threads: vec![0],
            last_successors: vec![0],
            subtree_sizes: vec![1],
            potentials: vec![17],
        };
        let mut room = TreeRoom::new();
        let (lent, tree) = tighten(load(&mut room, &alone, 3), &[]);
        assert_eq!(lent, Vec::<usize>::new());
        assert_eq!(columns_of(&tree).potentials, [0]);
    }

    #[test]
    fn test_a_step_dropped_untold_is_lent_again_by_hand() {
        // Node 0 is lent, dropped, lent again and told the cost 3 of its link up: `0 − 3`. Every later node is told
        // the cost 0, so it takes its parent's potential: 1 to 4 below 0 at −3, 5 and 6 below the root at 0.
        let model = seven_nodes();
        let mut room = TreeRoom::new();
        let mut walk = TightenWalk::new(load(&mut room, &at_potential(model.columns(&[0; 14]), 5), 14));

        let first = walk.step().map(|step| usize::from(step.node));
        let again = walk.step().expect("the dropped node should be lent again");
        assert_eq!((first, usize::from(again.node)), (Some(0), 0));
        again.tighten(3);

        let mut lent = Vec::new();
        for _ in 0..model.root() {
            let Some(step) = walk.step() else { break };
            lent.push(usize::from(step.node));
            step.tighten(0);
        }
        assert_eq!(lent, [1, 2, 3, 4, 5, 6]);
        assert_eq!(columns_of(&walk.finish()).potentials, [-3, -3, -3, -3, -3, 0, 0, 0]);
    }

    #[test]
    #[should_panic(expected = "called `TightenWalk::finish` with TreeNode 1 still to be told")]
    fn test_a_tighten_finished_before_every_node_is_told_panics() {
        let mut room = TreeRoom::new();
        let mut walk = TightenWalk::new(load(&mut room, &seven_nodes().columns(&[0; 14]), 14));
        walk.step().expect("node 0 should be lent").tighten(3);

        let _ = walk.finish();
    }

    proptest! {
        #[test]
        fn test_a_tighten_agrees_with_the_model_and_changes_no_other_column(
            model in plain_trees(),
            costs in vec(-1_000_i64..=1_000, 64),
            stale in any::<i64>(),
        ) {
            let link_count = link_count_of(&model);
            let costs = &costs[..link_count];
            let columns = at_potential(model.columns(costs), stale);
            let mut room = TreeRoom::new();
            let (lent, tree) = tighten(load(&mut room, &columns, link_count), costs);
            prop_assert_eq!(lent, model.subtree(model.root()).into_iter().skip(1).collect::<Vec<_>>());

            let tightened = columns_of(&tree);
            prop_assert_eq!(tightened.potentials[model.root()], 0);
            prop_assert_eq!(model.check(&tightened, Some(costs)), Ok(()));
            prop_assert_eq!(tightened, PlainColumns { potentials: model.potentials(costs), ..columns });
        }
    }
}
