// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The prune of a tree: the walk of its hung nodes from the leaves up, each told to stay on its link or to hang from
//! the root by its own root link, a node that leaves its link taking its subtree with it.

use super::spanning_tree::{Direction, Hanging, HungNode, LentTree, Link, TreeColumnsMut, TreeNode};

/// The walk of a tree's hung nodes from the leaves up, each after its whole subtree: a lending walk whose every step is
/// decided, its node staying on its link or hanging from the root by its own root link the way it is told. A node that
/// leaves its link takes its subtree with it, which moves, in its own order, to the end of the thread.
///
/// It lends the hung nodes in the reverse of the order the thread visited them when the walk was made, each once
/// it is decided. It owns the tree while it writes it, and [`finish`](Self::finish) hands it back only once every
/// node is decided, with the tree's invariant whole again: the subtree sizes and last successors are rebuilt as it
/// goes, and the root's last successor is set when it is finished. A walk dropped before that drops the tree with it,
/// so no tree is left half pruned. A decision costs a constant number of reads and writes, whatever it moves, so the
/// whole walk costs O(n); it allocates nothing.
///
/// # Invariant
/// Only [`new`](Self::new) makes one, from a whole tree it takes. Let a node be decided once the step that lent it was
/// decided:
/// - `next` is the root once every hung node is decided, and otherwise the last node of the thread not decided;
/// - the nodes not decided keep their parents and links, and stand at the front of the thread, after the root, in the
///   order the thread had when the walk was made; every node after `next` in the thread is decided;
/// - each decided node's subtree, in the tree as decided so far, is one run of the thread that starts at it, its
///   subtree size the run's length and its last successor the run's last node.
#[derive(Debug)]
pub(crate) struct PruneWalk<'t> {
    /// The tree pruned.
    tree: LentTree<'t>,
    /// The node the next step lends: the last node of the thread not decided, the root once every node is.
    next: TreeNode,
}

/// One node of a prune, not decided yet, lent until it is: the walk moves on only when it is decided, and a step that
/// is dropped undecided is lent again. Its subtree size and last successor are already those of its subtree as
/// decided below it.
///
/// # Invariant
/// Only [`PruneWalk::step`] makes one, of the walk's `next`, a hung node: `previous` is the node before `node` in the
/// thread, read before any change the decision makes.
#[derive(Debug)]
pub(crate) struct PruneStep<'w> {
    /// The columns of the walk's tree.
    columns: TreeColumnsMut<'w>,
    /// The node to decide.
    node: HungNode,
    /// The node's root link.
    root_link: Link<'w>,
    /// The tree's root.
    root: TreeNode,
    /// The node before `node` in the thread: where the walk goes on once `node` is decided.
    previous: TreeNode,
    /// The walk's next node.
    next: &'w mut TreeNode,
}

impl<'t> PruneWalk<'t> {
    /// The prune of `tree`, nothing decided yet: its first step lends the thread's last node.
    pub(crate) fn new(mut tree: LentTree<'t>) -> Self {
        let root = tree.root();
        let next = tree.tree_mut().columns_mut().reverse_thread(root);

        Self { tree, next }
    }

    /// The tree pruned, every node decided, its invariant whole: the root's last successor set to the thread's last
    /// node.
    ///
    /// # Panics
    /// Panics if a node is still to be decided: where [`step`](Self::step) would lend one.
    #[track_caller]
    pub(crate) fn finish(self) -> LentTree<'t> {
        let Self { mut tree, next } = self;
        let root = tree.root();
        assert!(next == root, "called `PruneWalk::finish` with {next:?} still to be decided");

        let mut columns = tree.tree_mut().columns_mut();
        let last = columns.reverse_thread(root);
        columns.set_last_successor(root, last);

        tree
    }

    /// The next node to decide, whose subtree has been decided; `None` once every hung node has been, and ever after.
    pub(crate) fn step(&mut self) -> Option<PruneStep<'_>> {
        // Why the walk holds. Every node after `next` in the current thread has been decided, and a splice moves only a
        // run of decided nodes, to the end of the thread: so the nodes up to `next` stand at the front in their old
        // order, and the node before `next`, read before its decision changes anything, is the next node to decide.
        // A decided node's new run is contiguous and starts at the node. After the run of a node that stays under a
        // parent `p` comes either the run of the next sibling that stayed, the runs of the siblings cut in between
        // having left, or a node whose parent is not `p`: one outside `p`'s old run, the root, or the top of a run
        // moved to the end, whose parent was just made the root. So the first child that stays, met from behind, finds
        // no sibling after its run and sets `p`'s columns, and every later one adds to them; a node none of whose
        // children stayed is followed by a node whose parent it is not, and makes its own columns. Neither read depends
        // on anything a dropped step wrote, so a step lent again writes the same.
        let root = self.tree.root();
        let Self { tree, next } = self;

        if *next == root {
            return None;
        }

        let columns = tree.tree_mut().columns_mut();
        debug_assert!(columns.hung(*next).is_some(), "the walk's next node should be a hung node");
        // SAFETY: `next` is a tree node, below `n + 1` (`TreeNode`), and not the root, `n`, as tested just above
        // (`PruneWalk`).
        let node = unsafe { columns.hung_unchecked(*next) };
        let root_link = tree.root_link(node);

        Some(PruneStep::new(tree.tree_mut().columns_mut(), node, root_link, root, next))
    }
}

impl<'w> PruneStep<'w> {
    /// The step of `node`, the walk's `next`, in the tree of `columns` and `root`, hanging from the root by
    /// `root_link` where it is told to: `node`'s subtree size and last successor are those of its subtree as decided,
    /// and the node before it in the thread is read.
    fn new(
        mut columns: TreeColumnsMut<'w>,
        node: HungNode,
        root_link: Link<'w>,
        root: TreeNode,
        next: &'w mut TreeNode,
    ) -> Self {
        let top = node.node();
        let previous = columns.reverse_thread(top);

        // A child that stayed comes right after its parent; where none did, the node makes a subtree of its own.
        if columns.parent(columns.thread(top)) != top {
            columns.set_subtree_size(top, 1);
            columns.set_last_successor(top, top);
        }

        Self { columns, node, root_link, root, previous, next }
    }

    /// The node, below the tree's `n`.
    pub(crate) fn node(&self) -> usize {
        usize::from(self.node)
    }

    /// The node's parent, below the tree's `n + 1`: the root, `n`, where the node hangs from the root.
    pub(crate) fn parent(&self) -> usize {
        usize::from(self.columns.parent(self.node.node()))
    }

    /// The link the node hangs by, and which way it runs seen from the node.
    pub(crate) fn hanging(&self) -> Hanging<'_> {
        self.columns.hanging(self.node)
    }

    /// The node's root link, which it hangs by where it is told to hang from the root.
    pub(crate) fn root_link(&self) -> Link<'_> {
        self.root_link
    }

    /// Keeps the node on its link, under its parent: the parent's subtree gains the node's.
    pub(crate) fn stay(mut self) {
        let top = self.node.node();
        let parent = self.columns.parent(top);

        // The root's subtree is the whole tree, of a size that never changes, and its last successor is set at the end.
        if parent != self.root {
            let (size, last) = (self.columns.subtree_size(top), self.columns.last_successor(top));
            let after = self.columns.thread(last);

            if self.columns.parent(after) == parent {
                let gained = self.columns.subtree_size(parent) + size;
                self.columns.set_subtree_size(parent, gained);
            } else {
                self.columns.set_subtree_size(parent, 1 + size);
                self.columns.set_last_successor(parent, last);
            }
        }

        *self.next = self.previous;
    }

    /// Hangs the node from the root by its root link, running `direction` seen from the node, with its subtree, which
    /// moves to the end of the thread where the node did not hang from the root already.
    pub(crate) fn hang_from_root(mut self, direction: Direction) {
        let top = self.node.node();
        let moves = self.columns.parent(top) != self.root;
        self.columns.set_parent(top, self.root);
        self.columns.set_hanging(self.node, self.root_link, direction);

        // A run that ends the thread already stays where it is.
        let last = self.columns.last_successor(top);
        let after = self.columns.thread(last);
        if moves && after != self.root {
            let end = self.columns.reverse_thread(self.root);
            self.columns.join_thread(self.previous, after);
            self.columns.join_thread(end, top);
            self.columns.join_thread(last, self.root);
        }

        *self.next = self.previous;
    }
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;

    use super::*;
    use crate::tree::plain_tree::{Decision, PlainColumns, PlainTree, link_count_of, plain_trees, seven_nodes};
    use crate::tree::rehang::tests::{ExchangeRun, exchange_picks, run_exchanges};
    use crate::tree::spanning_tree::TreeRoom;
    use crate::tree::spanning_tree::tests::{columns_of, load, plain_tree_of, tree_of};
    use crate::tree::tighten::TightenWalk;

    const UP: Direction = Direction::Up;
    const DOWN: Direction = Direction::Down;
    const STAY: Decision = Decision::Stay;

    /// Prunes `tree`, telling node `v` `decisions[v]`, and answers the nodes the walk lent, in order, with the tree
    /// the finished walk hands back.
    ///
    /// # Panics
    /// Panics if the walk lends more nodes than the tree has below the root.
    fn prune<'r>(tree: LentTree<'r>, decisions: &[Decision]) -> (Vec<usize>, LentTree<'r>) {
        let hung_nodes = tree.node_count();
        let mut walk = PruneWalk::new(tree);
        let mut lent = Vec::new();

        while let Some(step) = walk.step() {
            assert!(lent.len() < hung_nodes, "a prune should lend each node below the root once");
            lent.push(step.node());
            match decisions[step.node()] {
                Decision::Stay => step.stay(),
                Decision::Root(direction) => step.hang_from_root(direction),
            }
        }

        (lent, walk.finish())
    }

    /// The tree of seven nodes, its nodes 0 and 5, which hang from the root, by their root links 7 and 12 of the 14
    /// links, running up. Its thread runs 7, 0, 1, …, 6.
    fn seven_rooted() -> PlainTree {
        let mut model = seven_nodes();
        model.hangings[0] = (7, UP);
        model.hangings[5] = (12, UP);

        model
    }

    /// The columns of [`seven_rooted`], written out by hand, every potential zero.
    fn seven_rooted_columns() -> PlainColumns {
        PlainColumns {
            parents: vec![7, 0, 1, 2, 0, 7, 5, 7],
            hangings: vec![(7, UP), (1, UP), (2, DOWN), (3, UP), (4, UP), (12, UP), (6, UP)],
            threads: vec![1, 2, 3, 4, 5, 6, 7, 0],
            reverse_threads: vec![7, 0, 1, 2, 3, 4, 5, 6],
            last_successors: vec![4, 3, 3, 3, 4, 6, 6, 6],
            subtree_sizes: vec![5, 3, 2, 1, 1, 2, 1, 8],
            potentials: vec![0; 8],
        }
    }

    /// The seven nodes in a room, as [`seven_rooted`], pruned by `decisions`: the nodes lent and the columns left.
    fn pruned_seven(decisions: [Decision; 7]) -> (Vec<usize>, PlainColumns) {
        let mut room = TreeRoom::new();
        let tree = plain_tree_of(&mut room, &seven_rooted());
        assert_eq!(columns_of(&tree), seven_rooted_columns(), "the hand columns should be the model's");
        let (lent, tree) = prune(tree, &decisions);

        (lent, columns_of(&tree))
    }

    #[test]
    fn test_a_prune_that_cuts_nothing_leaves_every_column_by_hand() {
        let (lent, columns) = pruned_seven([STAY; 7]);

        assert_eq!(lent, [6, 5, 4, 3, 2, 1, 0], "the thread backwards");
        assert_eq!(columns, seven_rooted_columns());
    }

    #[test]
    fn test_a_prune_that_cuts_every_arc_hangs_each_node_from_the_root_by_hand() {
        let cut = Decision::Root(DOWN);
        let (lent, columns) = pruned_seven([STAY, cut, cut, cut, cut, STAY, cut]);

        // 6 ends the thread and stays there; 4, 3, 2 and 1 move to the end in turn: 7, 0, 5, 6, 4, 3, 2, 1.
        let expected = PlainColumns {
            parents: vec![7; 8],
            hangings: vec![(7, UP), (8, DOWN), (9, DOWN), (10, DOWN), (11, DOWN), (12, UP), (13, DOWN)],
            threads: vec![5, 7, 1, 2, 3, 6, 4, 0],
            reverse_threads: vec![7, 2, 3, 4, 6, 0, 5, 1],
            last_successors: vec![0, 1, 2, 3, 4, 5, 6, 1],
            subtree_sizes: vec![1, 1, 1, 1, 1, 1, 1, 8],
            potentials: vec![0; 8],
        };
        assert_eq!((lent, columns), (vec![6, 5, 4, 3, 2, 1, 0], expected));
    }

    #[test]
    fn test_a_cut_inside_a_cut_moves_the_inner_run_first_by_hand() {
        let (lent, columns) = pruned_seven([STAY, Decision::Root(UP), STAY, Decision::Root(DOWN), STAY, STAY, STAY]);

        // 3 is passed first and moves to the end, after 6; then 1 takes 1, 2 after it: 7, 0, 4, 5, 6, 3, 1, 2.
        let expected = PlainColumns {
            parents: vec![7, 7, 1, 7, 0, 7, 5, 7],
            hangings: vec![(7, UP), (8, UP), (2, DOWN), (10, DOWN), (4, UP), (12, UP), (6, UP)],
            threads: vec![4, 2, 7, 1, 5, 6, 3, 0],
            reverse_threads: vec![7, 3, 1, 6, 0, 4, 5, 2],
            last_successors: vec![4, 2, 2, 3, 4, 6, 6, 2],
            subtree_sizes: vec![2, 2, 1, 1, 1, 2, 1, 8],
            potentials: vec![0; 8],
        };
        assert_eq!((lent, columns), (vec![6, 5, 4, 3, 2, 1, 0], expected));
    }

    #[test]
    fn test_a_cut_whose_run_ends_the_thread_stays_in_place_by_hand() {
        let (_, columns) = pruned_seven([STAY, STAY, STAY, STAY, STAY, STAY, Decision::Root(UP)]);

        // 6 hangs from the root by link 13 where it was; 5 is left with a subtree of its own.
        let expected = PlainColumns {
            parents: vec![7, 0, 1, 2, 0, 7, 7, 7],
            hangings: vec![(7, UP), (1, UP), (2, DOWN), (3, UP), (4, UP), (12, UP), (13, UP)],
            last_successors: vec![4, 3, 3, 3, 4, 5, 6, 6],
            subtree_sizes: vec![5, 3, 2, 1, 1, 1, 1, 8],
            ..seven_rooted_columns()
        };
        assert_eq!(columns, expected);
    }

    #[test]
    fn test_a_cut_of_the_first_child_leaves_the_next_to_head_its_parent_by_hand() {
        let (_, columns) = pruned_seven([STAY, Decision::Root(DOWN), STAY, STAY, STAY, STAY, STAY]);

        // 1 takes 1, 2, 3 to the end: 7, 0, 4, 5, 6, 1, 2, 3; 0 keeps 4 alone, and the thread ends at 3.
        let expected = PlainColumns {
            parents: vec![7, 7, 1, 2, 0, 7, 5, 7],
            hangings: vec![(7, UP), (8, DOWN), (2, DOWN), (3, UP), (4, UP), (12, UP), (6, UP)],
            threads: vec![4, 2, 3, 7, 5, 6, 1, 0],
            reverse_threads: vec![7, 6, 1, 2, 0, 4, 5, 3],
            last_successors: vec![4, 3, 3, 3, 4, 6, 6, 3],
            subtree_sizes: vec![2, 3, 2, 1, 1, 2, 1, 8],
            potentials: vec![0; 8],
        };
        assert_eq!(columns, expected);
    }

    #[test]
    fn test_a_node_under_the_root_told_another_way_turns_and_moves_nothing_by_hand() {
        let (_, columns) = pruned_seven([Decision::Root(DOWN), STAY, STAY, STAY, STAY, STAY, STAY]);

        let mut expected = seven_rooted_columns();
        expected.hangings[0] = (7, DOWN);
        assert_eq!(columns, expected);
    }

    #[test]
    fn test_a_prune_of_no_node_and_of_one_by_hand() {
        let mut room = TreeRoom::new();
        let alone = PlainColumns {
            parents: vec![0],
            hangings: vec![],
            threads: vec![0],
            reverse_threads: vec![0],
            last_successors: vec![0],
            subtree_sizes: vec![1],
            potentials: vec![0],
        };
        let (lent, tree) = prune(load(&mut room, &alone, 2), &[]);
        assert_eq!((lent, columns_of(&tree)), (vec![], alone));

        // One node under the root by its root link 1 of 2 links, turned down.
        let one = PlainColumns {
            parents: vec![1, 1],
            hangings: vec![(1, UP)],
            threads: vec![1, 0],
            reverse_threads: vec![1, 0],
            last_successors: vec![0, 0],
            subtree_sizes: vec![1, 2],
            potentials: vec![0, 0],
        };
        let (lent, tree) = prune(load(&mut room, &one, 2), &[Decision::Root(DOWN)]);
        assert_eq!((lent, columns_of(&tree)), (vec![0], PlainColumns { hangings: vec![(1, DOWN)], ..one.clone() }));

        let (lent, tree) = prune(load(&mut room, &one, 2), &[STAY]);
        assert_eq!((lent, columns_of(&tree)), (vec![0], one));
    }

    #[test]
    fn test_a_step_dropped_undecided_is_lent_again_and_changes_nothing_by_hand() {
        // A walk that drops its first step, then a step of 3, the node it would cut, and of 2 below it, each the first
        // time it is lent, goes on as one that dropped none.
        let decisions = [STAY, Decision::Root(UP), STAY, Decision::Root(DOWN), STAY, STAY, STAY];
        let mut room = TreeRoom::new();
        let mut walk = PruneWalk::new(plain_tree_of(&mut room, &seven_rooted()));

        let first = walk.step().expect("the thread's last node should be lent").node();
        let again = walk.step().expect("the dropped node should be lent again");
        assert_eq!((first, again.node()), (6, 6));
        let Hanging { link, direction } = again.hanging();
        assert_eq!((usize::from(link), direction, usize::from(again.root_link())), (6, UP, 13));

        let mut dropped = Vec::new();
        for _ in 0..2 * decisions.len() {
            let Some(step) = walk.step() else { break };
            let node = step.node();
            if (node == 3 || node == 2) && !dropped.contains(&node) {
                dropped.push(node);
                continue;
            }
            match decisions[node] {
                Decision::Stay => step.stay(),
                Decision::Root(direction) => step.hang_from_root(direction),
            }
        }
        assert!(walk.step().is_none(), "every node should be decided within twice the nodes");

        let tree = walk.finish();
        assert_eq!(seven_rooted().pruned(&decisions, 14).check(&columns_of(&tree), None), Ok(()));
        assert_eq!(columns_of(&tree).threads, [4, 2, 7, 1, 5, 6, 3, 0], "the thread runs 7, 0, 4, 5, 6, 3, 1, 2");
    }

    #[test]
    fn test_a_finished_prune_stays_finished() {
        let mut room = TreeRoom::new();
        let mut walk = PruneWalk::new(plain_tree_of(&mut room, &seven_rooted()));
        for _ in 0..7 {
            walk.step().expect("each of the seven nodes should be lent").stay();
        }
        assert!(walk.step().is_none() && walk.step().is_none());

        assert_eq!(columns_of(&walk.finish()), seven_rooted_columns());
    }

    #[test]
    #[should_panic(expected = "called `PruneWalk::finish` with TreeNode 5 still to be decided")]
    fn test_a_prune_finished_before_every_node_is_decided_panics() {
        let mut room = TreeRoom::new();
        let mut walk = PruneWalk::new(plain_tree_of(&mut room, &seven_rooted()));
        walk.step().expect("node 6 should be lent").stay();

        let _ = walk.finish();
    }

    /// What a prune tells one node: to stay, or to hang from the root either way.
    fn decisions() -> impl Strategy<Value = Decision> {
        prop_oneof![2 => Just(STAY), 1 => Just(Decision::Root(UP)), 1 => Just(Decision::Root(DOWN))]
    }

    /// Prunes the tree of `run`, telling node `v` `decisions[v]`, checks every column against the model's prune, and
    /// makes every link tight again under the run's costs, so that exchanges can follow.
    ///
    /// # Errors
    /// Returns the model's message where a column differs.
    fn prune_run<'r>(run: ExchangeRun<'r>, decisions: &[Decision]) -> Result<ExchangeRun<'r>, TestCaseError> {
        let ExchangeRun { tree, model, costs } = run;
        let (_, tree) = prune(tree, decisions);
        let model = model.pruned(decisions, costs.len());
        model.check(&columns_of(&tree), None).map_err(TestCaseError::fail)?;

        let mut walk = TightenWalk::new(tree);
        for _ in 0..decisions.len() {
            let Some(step) = walk.step() else { break };
            let cost = costs[usize::from(step.hanging().link)];
            step.tighten(cost);
        }
        let tree = walk.finish();
        model.check(&columns_of(&tree), Some(&costs)).map_err(TestCaseError::fail)?;

        Ok(ExchangeRun { tree, model, costs })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn test_a_prune_between_exchanges_and_of_a_pruned_tree_agrees_with_the_model(
            (directions, first, second) in vec(prop_oneof![Just(UP), Just(DOWN)], 2..=24).prop_flat_map(|directions| {
                let nodes = directions.len();
                (Just(directions), vec(decisions(), nodes), vec(decisions(), nodes))
            }),
            arc_costs in vec(-100_i64..=100, 0..=12),
            picks in vec(exchange_picks(), 3 * 16),
        ) {
            // Exchanges leave a thread whose children come in any order; a prune, more exchanges, then a prune of the
            // pruned tree, each checked against the model in every column.
            let mut room = TreeRoom::new();
            let mut run = ExchangeRun::star(&mut room, &directions, &arc_costs, 1_000);
            run_exchanges(&mut run, &picks[..16])?;
            let mut run = prune_run(run, &first)?;
            run_exchanges(&mut run, &picks[16..32])?;
            let run = prune_run(run, &second)?;
            let mut run = prune_run(run, &first)?;
            run_exchanges(&mut run, &picks[32..])?;
        }
    }

    proptest! {
        #[test]
        fn test_a_prune_agrees_with_the_model_in_every_column(
            (model, decisions) in plain_trees().prop_flat_map(|model| {
                let nodes = model.root();
                (Just(model), vec(decisions(), nodes))
            }),
            costs in vec(-50_i64..=50, 64),
        ) {
            let link_count = link_count_of(&model);
            let mut room = TreeRoom::new();
            let tree = tree_of(&mut room, &model, link_count, &costs[..link_count]);
            let before = columns_of(&tree);
            let thread = model.subtree(model.root());

            let (lent, tree) = prune(tree, &decisions);
            prop_assert_eq!(lent, thread.into_iter().skip(1).rev().collect::<Vec<_>>());

            let columns = columns_of(&tree);
            prop_assert_eq!(model.pruned(&decisions, link_count).check(&columns, None), Ok(()));
            prop_assert_eq!(columns.potentials, before.potentials, "a prune moves no potential");
        }
    }
}
