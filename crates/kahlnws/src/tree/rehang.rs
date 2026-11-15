// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The exchange of a leaving link for an entering one: the subtree the leaving link cuts off is hung from the entering
//! arc's other end, in four steps that each leave one more part of the tree right, and its potentials are shifted.

use super::cycle::Cycle;
use super::spanning_tree::{Direction, HungNode, Link, Side, SpanningTree, TreeNode};

/// The exchange of the link of a cycle's kept node for an entering one, in the tree the cycle held: its nodes, with
/// what its steps read of the tree before the move, while it was whole.
///
/// Let `S` be the subtree of `cut`. The move hangs `S` from `attach` by the entering link, with `enter` as its new top.
/// Along the stem, the path from `enter` up to `cut`, the parent relation is reversed, each stem node hanging by the
/// link of the node that was below it; every other node keeps its parent and its link. Each step repairs one more
/// derived column where the move changed it, so the move touches the nodes of `S`, the paths from `attach` and from
/// `cut`'s old parent up to `join`, and above `join` only the ancestors whose run ended where `S` left or arrived.
///
/// # Invariant
/// Only [`Exchange::of`] makes one, from a cycle whose walk kept a node, and it holds that cycle's borrow of the tree,
/// as the cycle found it: `cut` is the kept node and `side` its side; `enter` is the end of that side and `attach` the
/// other end, which lies outside `S`; `join` is the apex. So the stem, from `enter` up to `cut`, lies in `S`, strictly
/// below the apex: no stem node is the root. After step one of the move, each stem node above `enter` has as its
/// parent the stem node that was below it, and `enter` has `attach`. `old_parent`, `before_cut`, `old_last` and
/// `moved_size` were read from the tree before any step of the move.
#[derive(Debug)]
pub(crate) struct Exchange<'t> {
    /// The tree.
    tree: SpanningTree<'t>,
    /// The side of the cycle `cut` was found on, whose end `S` holds.
    side: Side,
    /// The top of `S` after the move.
    enter: HungNode,
    /// The new parent of `enter`.
    attach: TreeNode,
    /// The top of `S` before the move.
    cut: HungNode,
    /// The apex of the cycle, the deepest common ancestor of `enter` and `attach`.
    join: TreeNode,
    /// `cut`'s parent before the move.
    old_parent: TreeNode,
    /// The node before `cut` in the thread before the move.
    before_cut: TreeNode,
    /// The last node of `S` in the thread before the move.
    old_last: TreeNode,
    /// The number of nodes of `S`.
    moved_size: u32,
}

/// Which way a walk of the size repair changes the sizes it passes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SizeChange {
    /// The ancestors of `attach` below the apex gain the moved subtree.
    Gain,
    /// The ancestors of `cut`'s old parent below the apex lose it.
    Lose,
}

impl<'t> Exchange<'t> {
    /// The exchange of `cycle`, in which the link of the node its walk kept leaves and that node's side's end hangs
    /// from the other end; `None` where the walk kept no node, so that no link leaves.
    pub(crate) fn of(cycle: Cycle<'t>) -> Option<Self> {
        let kept = cycle.into_kept()?;
        let (side, enter, attach, cut, join) = (kept.side(), kept.enter(), kept.attach(), kept.cut(), kept.join());
        let mut tree = kept.into_tree();

        let columns = tree.columns_mut();
        let top = cut.node();
        let (old_parent, before_cut) = (columns.parent(top), columns.reverse_thread(top));
        let (old_last, moved_size) = (columns.last_successor(top), columns.subtree_size(top));

        Some(Self { tree, side, enter, attach, cut, join, old_parent, before_cut, old_last, moved_size })
    }

    /// The link that leaves, for as long as this exchange is borrowed.
    pub(crate) fn leaving_link(&self) -> Link<'_> {
        self.tree.hanging(self.cut).link
    }

    /// The side of the cycle whose end the moved subtree holds.
    pub(crate) fn side(&self) -> Side {
        self.side
    }

    /// Hangs the subtree of the leaving node from the entering arc's other end by the link `entering`, which runs
    /// `direction` seen from the end inside the subtree, and adds `shift` to the potential of every node of that
    /// subtree, wrapping. The tree keeps its invariant.
    ///
    /// Afterwards the subtree's nodes hang as before but along the path from the entering end up to the leaving node,
    /// where each hangs from the node that was below it, by that node's link turned around; the entering end hangs by
    /// `entering`. No potential outside the subtree changes. It touches the nodes of the subtree, the paths from the
    /// entering arc's other end and from the leaving node's old parent up to the apex, and above the apex only the
    /// ancestors whose subtree's run in the thread ended where the subtree left or arrived. It allocates nothing.
    ///
    /// # Safety
    /// `entering` is below the tree's link count: a raw link for which [`SpanningTree::link`] answers a link, as its
    /// doc states.
    pub(crate) unsafe fn apply_unchecked(self, entering: usize, direction: Direction, shift: i64) {
        debug_assert!(
            entering < self.tree.link_count(),
            "called `Exchange::apply_unchecked` with link {entering}, which should be below the link count {}",
            self.tree.link_count()
        );
        // SAFETY: the link is below the tree's link count, as the caller promises (`Exchange::apply_unchecked`).
        let entering = unsafe { self.tree.link_unchecked(entering) };

        self.move_subtree(entering, direction, shift);
    }

    /// The move of [`apply_unchecked`](Self::apply_unchecked), by the entering link `entering`, in its five steps.
    fn move_subtree(mut self, entering: Link<'_>, direction: Direction, shift: i64) {
        let end = if self.enter == self.cut { self.move_block() } else { self.rethread_stem() };
        self.reverse_stem(end, entering, direction);
        self.repair_last_successors(end);
        self.repair_subtree_sizes();
        self.shift_potentials(shift);
    }

    /// Step one for a stem of the single node `cut`, which is `enter`. Answers the last node of `S`, which is
    /// unchanged.
    ///
    /// Afterwards `cut` hangs from `attach`, the run of `S` in the thread, in its old order, follows `attach`, the
    /// thread runs through every node once in a preorder of the new tree, and the reverse thread is its inverse;
    /// nothing else changed.
    fn move_block(&mut self) -> TreeNode {
        let (attach, cut, before_cut, old_last) = (self.attach, self.cut.node(), self.before_cut, self.old_last);
        let mut columns = self.tree.columns_mut();

        columns.set_parent(cut, attach);
        // Where `S` follows `attach` already, the splice would write the thread as it stands.
        if before_cut != attach {
            let after_block = columns.thread(old_last);
            columns.join_thread(before_cut, after_block);
            let after_attach = columns.thread(attach);
            columns.join_thread(attach, cut);
            columns.join_thread(old_last, after_attach);
        }

        old_last
    }

    /// Step one for a stem of two nodes or more. Answers the last node of `S` in the new thread.
    ///
    /// Afterwards every stem node hangs from the stem node that was below it and `enter` from `attach`; `S` runs in
    /// the thread just after `attach`, as `enter` with its old subtree, then each next stem node with its old subtree
    /// less the branch before it; the thread runs through every node once in a preorder of the new tree, and the
    /// reverse thread is its inverse; nothing else changed.
    fn rethread_stem(&mut self) -> TreeNode {
        let (enter, attach, cut) = (self.enter.node(), self.attach, self.cut.node());
        let (before_cut, old_last, node_count) = (self.before_cut, self.old_last, self.tree.node_count());
        let mut columns = self.tree.columns_mut();

        // Where the thread goes on after `S`: after its old run if `S` stays there, else after `attach`.
        let continuation = if before_cut == attach { columns.thread(old_last) } else { columns.thread(attach) };
        let (mut stem, mut new_parent) = (enter, attach);
        let mut last = columns.last_successor(enter);
        let mut after = columns.thread(last);
        let mut before = columns.reverse_thread(enter);
        columns.join_thread(attach, enter);

        // Each stem node's block, its old subtree less the branch threaded before it, is cut out of the thread, from
        // the node before it (`before`) to its end, and appended after the block before. Every reverse link the walk
        // reads is read before the walk writes it: `before` for `enter` above, ahead of the join that writes it; and
        // for each next stem node in the loop, ahead of the join that makes it follow the block before. Until then
        // no join of the walk has written it. Every segment cut so far lies inside the current stem node's run, so in
        // the thread as modified so far a parent still precedes its child's run; a stem node is therefore never the
        // node that followed a block of its descendants, the only reverse links the other joins write.
        //
        // `block_end` takes the node that preceded the branch before the branch was cut out, held in `before`: by the
        // time it is asked, the reverse thread at the branch's top already names the block it now follows, and a
        // fresh read would answer that instead.
        //
        // `S` holds at most `n − 1` hung nodes, since the other end is a hung node outside it; the stem lies in `S`,
        // and each step climbs one stem node short of `cut`: at most `n − 2` steps, as a path of all `n` nodes entered
        // at its foot, cut below its top and hung from that top takes.
        let mut steps = 0_usize;
        while stem != cut {
            steps += 1;
            debug_assert!(steps + 1 < node_count, "the stem should end at the leaving node within n − 2 steps");
            let next = columns.parent(stem);
            let before_next = columns.reverse_thread(next);
            columns.join_thread(last, next);
            columns.join_thread(before, after);
            columns.set_parent(stem, new_parent);

            new_parent = stem;
            stem = next;
            let (stem_last, branch_last) = (columns.last_successor(stem), columns.last_successor(new_parent));
            last = block_end(stem_last, branch_last, before);
            after = columns.thread(last);
            before = before_next;
        }

        columns.set_parent(cut, new_parent);
        columns.join_thread(last, continuation);
        if before_cut != attach {
            columns.join_thread(before_cut, after);
        }

        last
    }

    /// Step two: every stem node above `enter` hangs by the link of the node that was below it, turned around, and
    /// `enter` by `entering`, running `direction`.
    ///
    /// Afterwards links and directions are those of the new tree, and subtree sizes and last successors agree with it
    /// on every node of `S`, whose new last node in the thread is `end`.
    fn reverse_stem(&mut self, end: TreeNode, entering: Link<'_>, direction: Direction) {
        let (enter, cut, moved_size, node_count) = (self.enter, self.cut, self.moved_size, self.tree.node_count());
        let mut columns = self.tree.columns_mut();

        // The walk goes down the stem from `cut` along the new parents, each the stem node that was below: never the
        // root. The sizes it reads are the old ones: a node's is overwritten when it is left. Like step one's walk, it
        // takes at most `n − 2` steps.
        let mut size = 0;
        let mut node = cut;
        let mut steps = 0_usize;
        while node != enter {
            steps += 1;
            debug_assert!(
                steps + 1 < node_count,
                "the walk down the stem should end at the entering end within n − 2 steps"
            );
            let parent = columns.parent(node.node());
            debug_assert!(columns.hung(parent).is_some(), "the walk down the stem should never meet the root");
            // SAFETY: after step one each stem node above `enter` has as its parent the stem node that was below it,
            // and no stem node is the root (`Exchange`).
            let below = unsafe { columns.hung_unchecked(parent) };
            columns.hang_by_turned_link_of(node, below);

            // `below` was a child of `node`: the difference of their old sizes is what stays under `node`, and the
            // sizes of the stem nodes passed are what now hangs below it.
            size += columns.subtree_size(node.node()) - columns.subtree_size(below.node());
            columns.set_subtree_size(node.node(), size);
            columns.set_last_successor(node.node(), end);
            node = below;
        }

        columns.set_hanging(enter, entering, direction);
        // Where `enter` is `cut`, its run and its size are what they were.
        if enter != cut {
            columns.set_subtree_size(enter.node(), moved_size);
            columns.set_last_successor(enter.node(), end);
        }
    }

    /// Step three: the ancestors of `attach` whose run ended at `attach` now end where `S` ends, and the ancestors of
    /// `cut`'s old parent whose run ended with `S` end where [`old_side_end`] says.
    ///
    /// Afterwards last successors agree with the new tree everywhere.
    fn repair_last_successors(&mut self, end: TreeNode) {
        let (attach, join) = (self.attach, self.join);
        // Where the run of `join` ended at `attach`, the walk from `attach` repairs `join` and the ancestors above it,
        // and the walk from `cut`'s old parent stops short of `join`.
        let limit = (self.tree.columns_mut().last_successor(join) == attach).then_some(join);

        self.replace_last_successors(attach, None, attach, end);
        if let Some(new_end) = old_side_end(self.before_cut, join, attach, end, self.old_last) {
            self.replace_last_successors(self.old_parent, limit, self.old_last, new_end);
        }
    }

    /// Makes `new` the last successor of `from` and of its ancestors in turn, each whose last successor is `old`,
    /// stopping at the first that is not, or at `limit`.
    fn replace_last_successors(&mut self, from: TreeNode, limit: Option<TreeNode>, old: TreeNode, new: TreeNode) {
        let node_count = self.tree.node_count();
        let mut columns = self.tree.columns_mut();

        // `new` is never `old`: the first walk replaces `attach` by the end of `S`, which lies inside `S`, the second
        // `S`'s old end by a node outside `S` or by its new end, which differs. So at the root, its own parent, the
        // walk finds the value it just wrote and stops. It writes `from` and its ancestors, the root included, each
        // once. Both `attach` and `cut`'s old parent lie outside `S`, which holds a hung node at least, so their paths
        // up hold at most `n − 1` hung nodes and the root: at most `n` writes, as `attach` at the foot of a path of the
        // other `n − 1` nodes, ending the runs of all its ancestors, takes.
        let mut node = from;
        let mut steps = 0_usize;
        while Some(node) != limit && columns.last_successor(node) == old {
            steps += 1;
            debug_assert!(steps <= node_count, "the walk up the last successors should end within n steps");
            columns.set_last_successor(node, new);
            node = columns.parent(node);
        }
    }

    /// Step four: the ancestors of `attach` below `join` gain the nodes of `S`, the ancestors of `cut`'s old parent
    /// below `join` lose them.
    ///
    /// Afterwards subtree sizes agree with the new tree everywhere.
    fn repair_subtree_sizes(&mut self) {
        self.change_sizes_up_to_join(self.attach, SizeChange::Gain);
        self.change_sizes_up_to_join(self.old_parent, SizeChange::Lose);
    }

    /// Changes the subtree size of `from` and of each ancestor below `join` by the size of `S`, as `change` says.
    fn change_sizes_up_to_join(&mut self, from: TreeNode, change: SizeChange) {
        let (join, moved_size, node_count) = (self.join, self.moved_size, self.tree.node_count());
        let mut columns = self.tree.columns_mut();

        // `join` is an ancestor of `attach` and of `cut`'s old parent (`Exchange`), so the walk meets it, having
        // passed only hung nodes strictly below it and outside `S`: at most `n − 1`, as a path of the other `n − 1`
        // nodes under the root reaches.
        let mut node = from;
        let mut steps = 0_usize;
        while node != join {
            steps += 1;
            debug_assert!(steps < node_count, "the walk up the sizes should meet the apex within n − 1 steps");
            // The parent first: the next step's read waits on it, not on this step's write.
            let parent = columns.parent(node);
            columns.set_subtree_size(node, change.applied(columns.subtree_size(node), moved_size));
            node = parent;
        }
    }

    /// Step five: adds `shift` to the potential of every node of `S`, wrapping, and of no other node.
    ///
    /// Afterwards the subtree of `enter` is shifted by `shift`; nothing else changed.
    fn shift_potentials(&mut self, shift: i64) {
        let (enter, moved_size) = (self.enter.node(), self.moved_size);
        let mut columns = self.tree.columns_mut();

        // After steps one and two, `S` is the run of `moved_size` nodes the thread makes from `enter`.
        let mut node = enter;
        for _ in 0..moved_size {
            columns.shift_potential(node, shift);
            node = columns.thread(node);
        }
    }
}

impl SizeChange {
    /// The size `held` after gaining or losing `moved` nodes.
    fn applied(self, held: u32, moved: u32) -> u32 {
        // A gaining ancestor of `attach` lies outside `S`, so its size and `S`'s add up to at most the `n + 1` tree
        // nodes; a losing ancestor of `cut`'s old parent held `S` and itself, so its size is above `moved`.
        match self {
            Self::Gain => held + moved,
            Self::Lose => held - moved,
        }
    }
}

/// The end of a stem node's block once the branch below it is threaded ahead of it: the stem node's subtree ends at
/// `stem_last`, the branch's at `branch_last`, and `before_branch` preceded the branch in the thread. Where both end at
/// one node, the branch ended the stem node's subtree, so its block ends just before the branch; else where its
/// subtree did.
fn block_end(stem_last: TreeNode, branch_last: TreeNode, before_branch: TreeNode) -> TreeNode {
    if stem_last == branch_last { before_branch } else { stem_last }
}

/// Where the runs of `cut`'s old ancestors that ended with `S`, at `old_last`, end now: just before where `S` was,
/// at `before_cut`, where `S` left its place; at `end`, where `S` now ends, where it stayed in place and changed its
/// end; `None` where nothing changes. `S` left its place unless the node before it was `join` or `attach`.
fn old_side_end(
    before_cut: TreeNode,
    join: TreeNode,
    attach: TreeNode,
    end: TreeNode,
    old_last: TreeNode,
) -> Option<TreeNode> {
    if before_cut != join && before_cut != attach {
        Some(before_cut)
    } else if end != old_last {
        Some(end)
    } else {
        None
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::collections::{BTreeSet, HashSet, VecDeque};

    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::sample::Index;
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    use super::*;
    use crate::tree::cycle::tests::cycle_walk;
    use crate::tree::plain_tree::{PlainColumns, PlainTree, link_count_of, plain_trees, seven_nodes};
    use crate::tree::spanning_tree::tests::{columns_of, load, plain_tree_of, tree_node, tree_of};
    use crate::tree::spanning_tree::{LentTree, TreeRoom};

    /// The nodes of `moving` as plain numbers: `enter`, `attach`, `cut`, `join`, `cut`'s old parent, the node before
    /// it in the thread, and the last node of `S`.
    fn nodes_of(moving: &Exchange<'_>) -> [usize; 7] {
        let Exchange { enter, attach, cut, join, old_parent, before_cut, old_last, .. } = *moving;

        [enter.node(), attach, cut.node(), join, old_parent, before_cut, old_last].map(usize::from)
    }

    /// The exchange of the cycle of `first` and `second` in `tree` in which the link of `cut` leaves, `cut` kept by the
    /// walk of `side`.
    ///
    /// # Panics
    /// Panics if that walk does not find `cut`.
    fn exchange_keeping<'t>(
        tree: &'t mut LentTree<'_>,
        [first, second]: [usize; 2],
        side: Side,
        cut: usize,
    ) -> Exchange<'t> {
        // A side yields its path from its end up, so `cut` is the step of its side at the place it has on that path.
        let parents = columns_of(tree).parents;
        let mut place = 0;
        let mut node = if side == Side::First { first } else { second };
        while node != cut {
            assert!(place < parents.len(), "`cut` should lie on the path from the end of its side up");
            (node, place) = (parents[node], place + 1);
        }

        let mut walk = cycle_walk(tree, first, second);
        let mut walked = 0;
        while let Some(step) = walk.step() {
            if step.side() == side {
                if walked == place {
                    step.keep();
                }
                walked += 1;
            }
        }

        Exchange::of(walk.into_cycle(Some(side))).expect("the walk should find the leaving node on its side")
    }

    /// Applies `exchange` by the entering link `entering`, running `direction`, with the shift `shift`: the one place
    /// the tests apply an exchange from a raw link.
    ///
    /// # Panics
    /// Panics if `entering` is not below the tree's link count.
    fn apply_exchange(exchange: Exchange<'_>, entering: usize, direction: Direction, shift: i64) {
        let link_count = exchange.tree.link_count();
        assert!(entering < link_count, "a test's entering link should be below the link count {link_count}");

        debug_assert!(entering < link_count, "a test's entering link should be below the link count");
        // SAFETY: the link is below the tree's link count, as asserted just above.
        unsafe { exchange.apply_unchecked(entering, direction, shift) };
    }

    /// The model of `parents` under its last node, node `v` hanging up by link `v`.
    fn hung_up(parents: Vec<usize>) -> PlainTree {
        let hangings = (0..parents.len() - 1).map(|node| (node, Direction::Up)).collect();

        PlainTree { parents, hangings }
    }

    #[test]
    fn test_each_step_of_a_stem_of_three_nodes_by_hand() {
        let (up, down) = (Direction::Up, Direction::Down);
        let model = seven_nodes();
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &model);
        let before = columns_of(&tree);
        assert_eq!(before.threads, [1, 2, 3, 4, 5, 6, 7, 0], "the thread runs 7, 0, 1, …, 6");
        let entering = tree.link(13).expect("link 13 lies below the link count");

        // An arc between 3 and 6 enters and the link of 1 leaves: `S` is 1, 2, 3, hung from 6 by the arc at 3.
        let mut moving = exchange_keeping(&mut tree, [3, 6], Side::First, 1);
        assert_eq!((usize::from(moving.leaving_link()), moving.side()), (1, Side::First), "the link of 1, not 3");
        assert_eq!((nodes_of(&moving), moving.moved_size), ([3, 6, 1, 7, 0, 0, 3], 3));

        let end = moving.rethread_stem();
        assert_eq!(usize::from(end), 1);
        let mut expected = PlainColumns {
            parents: vec![7, 2, 3, 6, 0, 7, 5, 7],
            threads: vec![4, 7, 1, 2, 5, 6, 3, 0],
            reverse_threads: vec![7, 2, 3, 6, 0, 4, 5, 1],
            ..before.clone()
        };
        assert_eq!(columns_of(&moving.tree), expected, "the thread runs 7, 0, 4, 5, 6, 3, 2, 1");

        moving.reverse_stem(end, entering, up);
        expected.hangings[1] = (2, up);
        expected.hangings[2] = (3, down);
        expected.hangings[3] = (13, up);
        expected.subtree_sizes[1..4].copy_from_slice(&[1, 2, 3]);
        expected.last_successors[1..4].copy_from_slice(&[1, 1, 1]);
        assert_eq!(columns_of(&moving.tree), expected);

        moving.repair_last_successors(end);
        expected.last_successors = vec![4, 1, 1, 1, 4, 1, 1, 1];
        assert_eq!(columns_of(&moving.tree), expected);

        moving.repair_subtree_sizes();
        expected.subtree_sizes = vec![2, 1, 2, 3, 1, 5, 4, 8];
        assert_eq!(columns_of(&moving.tree), expected);

        moving.shift_potentials(5);
        expected.potentials = vec![0, 5, 5, 5, 0, 0, 0, 0];
        assert_eq!(columns_of(&tree), expected);
        assert_eq!(model.exchanged(3, 6, 1, (13, up)).check(&expected, None), Ok(()));
    }

    #[test]
    fn test_each_step_of_a_leaving_entering_end_by_hand() {
        let model = seven_nodes();
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &model);
        let before = columns_of(&tree);
        let entering = tree.link(12).expect("link 12 lies below the link count");

        // An arc between 4 and 2 enters and the link of 4 itself leaves: 4 moves between 2 and 3 in the thread.
        let mut moving = exchange_keeping(&mut tree, [4, 2], Side::First, 4);
        assert_eq!(nodes_of(&moving), [4, 2, 4, 0, 0, 3, 4]);

        let end = moving.move_block();
        assert_eq!(usize::from(end), 4);
        let mut expected = PlainColumns {
            parents: vec![7, 0, 1, 2, 2, 7, 5, 7],
            threads: vec![1, 2, 4, 5, 3, 6, 7, 0],
            reverse_threads: vec![7, 0, 1, 4, 2, 3, 5, 6],
            ..before.clone()
        };
        assert_eq!(columns_of(&moving.tree), expected, "the thread runs 7, 0, 1, 2, 4, 3, 5, 6");

        moving.reverse_stem(end, entering, Direction::Down);
        expected.hangings[4] = (12, Direction::Down);
        assert_eq!(columns_of(&moving.tree), expected);

        moving.repair_last_successors(end);
        expected.last_successors = vec![3, 3, 3, 3, 4, 6, 6, 6];
        assert_eq!(columns_of(&moving.tree), expected);

        moving.repair_subtree_sizes();
        expected.subtree_sizes = vec![5, 4, 3, 1, 1, 2, 1, 8];
        assert_eq!(columns_of(&moving.tree), expected);

        moving.shift_potentials(-2);
        expected.potentials[4] = -2;
        assert_eq!(columns_of(&tree), expected);
        assert_eq!(model.exchanged(4, 2, 4, (12, Direction::Down)).check(&expected, None), Ok(()));
    }

    #[test]
    fn test_a_leaving_entering_end_that_follows_its_new_parent_keeps_its_place_by_hand() {
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());
        let before = columns_of(&tree);
        let entering = tree.link(11).expect("link 11 lies below the link count");

        // An arc between 4 and 3 enters and the link of 4 leaves: 4 follows 3 in the thread already.
        let mut moving = exchange_keeping(&mut tree, [4, 3], Side::First, 4);
        assert_eq!(nodes_of(&moving), [4, 3, 4, 0, 0, 3, 4]);

        let end = moving.move_block();
        let mut expected = PlainColumns { parents: vec![7, 0, 1, 2, 3, 7, 5, 7], ..before.clone() };
        assert_eq!((usize::from(end), columns_of(&moving.tree)), (4, expected.clone()));

        moving.reverse_stem(end, entering, Direction::Up);
        moving.repair_last_successors(end);
        expected.hangings[4] = (11, Direction::Up);
        expected.last_successors = vec![4, 4, 4, 4, 4, 6, 6, 6];
        assert_eq!(columns_of(&moving.tree), expected);

        moving.repair_subtree_sizes();
        expected.subtree_sizes = vec![5, 4, 3, 2, 1, 2, 1, 8];
        assert_eq!(columns_of(&tree), expected);
    }

    #[test]
    fn test_each_step_of_a_stem_on_the_second_side_that_stays_in_place_by_hand() {
        // Nodes 0 to 4 under the root 5: `0 <- 1 <- 2` and `0 <- 3 <- 4`; the thread runs 5, 0, 1, 2, 3, 4, so the
        // subtree of 3 holds the thread's last node and follows 2, its new parent's end.
        let model = hung_up(vec![5, 0, 1, 0, 3, 5]);
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &model);
        let before = columns_of(&tree);
        let entering = tree.link(9).expect("link 9 lies below the link count");

        // An arc between 2 and 4 enters, the link of 3 leaves, found on the second side: 4 under 2, 3 under 4.
        let mut moving = exchange_keeping(&mut tree, [2, 4], Side::Second, 3);
        assert_eq!(nodes_of(&moving), [4, 2, 3, 0, 0, 2, 4]);

        let end = moving.rethread_stem();
        assert_eq!(usize::from(end), 3);
        let mut expected = PlainColumns {
            parents: vec![5, 0, 1, 4, 2, 5],
            threads: vec![1, 2, 4, 5, 3, 0],
            reverse_threads: vec![5, 0, 1, 4, 2, 3],
            ..before.clone()
        };
        assert_eq!(columns_of(&moving.tree), expected, "the thread runs 5, 0, 1, 2, 4, 3");

        moving.reverse_stem(end, entering, Direction::Down);
        expected.hangings[3] = (4, Direction::Down);
        expected.hangings[4] = (9, Direction::Down);
        expected.subtree_sizes = vec![5, 2, 1, 1, 2, 6];
        expected.last_successors = vec![4, 2, 2, 3, 3, 4];
        assert_eq!(columns_of(&moving.tree), expected);

        // The old side's walk passes the apex and ends at the root, its own parent.
        moving.repair_last_successors(end);
        expected.last_successors = vec![3; 6];
        assert_eq!(columns_of(&moving.tree), expected);

        moving.repair_subtree_sizes();
        expected.subtree_sizes = vec![5, 4, 3, 1, 2, 6];
        assert_eq!(columns_of(&tree), expected);
        assert_eq!(model.exchanged(4, 2, 3, (9, Direction::Down)).check(&expected, None), Ok(()));
    }

    #[test]
    fn test_a_stem_whose_old_place_followed_the_apex_leaves_it_by_hand() {
        // Nodes 0 to 4 under the root 5: `0 <- 1 <- 2` and `0 <- 3` under the root, 4 under the root; the thread runs
        // 5, 0, 1, 2, 3, 4. An arc between 2 and 3 enters and the link of 1 leaves: `S` is 1, 2, which followed the
        // apex 0, and is spliced out of its place to follow 3, as 3, 2, 1.
        let model = hung_up(vec![5, 0, 1, 0, 5, 5]);
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &model);
        let entering = tree.link(8).expect("link 8 lies below the link count");

        let mut moving = exchange_keeping(&mut tree, [2, 3], Side::First, 1);
        assert_eq!(nodes_of(&moving), [2, 3, 1, 0, 0, 0, 2]);
        let end = moving.rethread_stem();
        assert_eq!(usize::from(end), 1);
        assert_eq!(columns_of(&moving.tree).threads, [3, 4, 1, 2, 5, 0], "the thread runs 5, 0, 3, 2, 1, 4");

        moving.reverse_stem(end, entering, Direction::Up);
        moving.repair_last_successors(end);
        moving.repair_subtree_sizes();
        let columns = columns_of(&tree);
        assert_eq!(columns.last_successors, [1, 1, 1, 1, 4, 4]);
        assert_eq!(columns.subtree_sizes, [4, 1, 2, 3, 1, 6]);
        assert_eq!(model.exchanged(2, 3, 1, (8, Direction::Up)).check(&columns, None), Ok(()));
    }

    #[test]
    fn test_the_old_sides_walk_stops_at_the_apex_the_new_side_repaired_by_hand() {
        // Nodes 0 to 3 under the root 4: 1 and 2 under 0, 3 under 1; the thread runs 4, 0, 1, 3, 2, so the run of 0
        // ends at 2. An arc between 3 and 2 enters and the link of 3 leaves: 3 moves to the end, after 2.
        let model = hung_up(vec![4, 0, 0, 1, 4]);
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &model);
        let entering = tree.link(7).expect("link 7 lies below the link count");

        let mut moving = exchange_keeping(&mut tree, [3, 2], Side::First, 3);
        assert_eq!(nodes_of(&moving), [3, 2, 3, 0, 1, 1, 3]);
        let end = moving.move_block();
        moving.reverse_stem(end, entering, Direction::Up);
        moving.repair_last_successors(end);

        // The new side's walk ends 2, 0 and the root at 3; the old side's walk ends 1 at itself and stops at 0, whose
        // run, ending at 3 again, is no longer its old run.
        assert_eq!(columns_of(&moving.tree).last_successors, [3, 1, 3, 3, 3]);
        moving.repair_subtree_sizes();
        let exchanged = model.exchanged(3, 2, 3, (7, Direction::Up));
        assert_eq!(exchanged.check(&columns_of(&tree), None), Ok(()));
    }

    #[test]
    fn test_a_path_turned_upside_down_by_hand() {
        // A path 0 <- 1 <- … <- 5 under the root 6; an arc between 0 and 5 enters and the link of 1 leaves, found on
        // the second side: the stem of five nodes reverses under 0.
        let model = hung_up(vec![6, 0, 1, 2, 3, 4, 6]);
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &model);

        apply_exchange(exchange_keeping(&mut tree, [0, 5], Side::Second, 1), 11, Direction::Down, 0);

        let columns = columns_of(&tree);
        assert_eq!(columns.parents, [6, 2, 3, 4, 5, 0, 6]);
        assert_eq!(columns.threads, [5, 6, 1, 2, 3, 4, 0], "the thread runs 6, 0, 5, 4, 3, 2, 1");
        assert_eq!(columns.subtree_sizes, [6, 1, 2, 3, 4, 5, 7]);
        assert_eq!(model.exchanged(5, 0, 1, (11, Direction::Down)).check(&columns, None), Ok(()));
    }

    #[test]
    fn test_an_exchange_in_the_star_makes_the_entering_arc_tight_by_hand() {
        let (up, down) = (Direction::Up, Direction::Down);
        let directions = [up, down, up, up];
        let mut room = TreeRoom::new();
        let mut tree = room.star(4, 2, directions.into_iter(), 10);
        // Arc 0 costs 7; the root links 2 to 5 cost 0 where they point up and the root cost 10 where down.
        let costs = [7, 0, 0, 10, 0, 0];

        // Arc 0 runs from 1 to 3 and enters; the root link of 1 leaves: 1 hangs from 3, and its potential falls from
        // 10 to `0 − 7`, which makes the arc tight.
        apply_exchange(exchange_keeping(&mut tree, [1, 3], Side::First, 1), 0, up, -17);

        let columns = columns_of(&tree);
        assert_eq!(columns.threads, [2, 4, 3, 1, 0], "the thread runs 4, 0, 2, 3, 1");
        assert_eq!(columns.potentials, [0, -7, 0, 0, 0]);
        let model = PlainTree::star(&directions, 2).exchanged(1, 3, 1, (0, up));
        assert_eq!(model.check(&columns, Some(&costs)), Ok(()));
    }

    #[test]
    fn test_a_stem_nodes_block_ends_before_the_branch_that_ended_its_subtree_by_hand() {
        let node = tree_node;

        // Node 1's subtree ends at 3, as does that of its child 2: once 2 is threaded ahead, 1's block ends at 1.
        assert_eq!(block_end(node(3), node(3), node(1)), node(1));
        // Node 0's subtree ends at 4, the branch 1's at 3: 0's block keeps its end.
        assert_eq!(block_end(node(4), node(3), node(0)), node(4));
    }

    #[test]
    fn test_the_old_sides_end_follows_where_the_subtree_went_by_hand() {
        let node = tree_node;

        // `S` left its place: the old ancestors end just before it.
        assert_eq!(old_side_end(node(0), node(7), node(6), node(1), node(3)), Some(node(0)));
        // `S` stayed after `attach` or `join` and its end changed: they end where it ends now.
        assert_eq!(old_side_end(node(2), node(0), node(2), node(3), node(4)), Some(node(3)));
        assert_eq!(old_side_end(node(0), node(0), node(2), node(3), node(4)), Some(node(3)));
        // `S` stayed and kept its end: nothing changes.
        assert_eq!(old_side_end(node(3), node(0), node(3), node(4), node(4)), None);
    }

    #[test]
    fn test_a_size_gains_or_loses_the_moved_nodes_by_hand() {
        assert_eq!((SizeChange::Gain.applied(3, 2), SizeChange::Lose.applied(5, 3)), (5, 2));
        let edges = (SizeChange::Gain.applied(0, u32::MAX), SizeChange::Lose.applied(u32::MAX, u32::MAX));
        assert_eq!(edges, (u32::MAX, 0));
    }

    /// What a pick aims the leaving node and the new parent at, so that the special cases come up often.
    #[derive(Clone, Copy, Debug)]
    enum Aim {
        /// Any leaving node, entering end and new parent.
        Any,
        /// The leaving node is the entering end.
        StemOfOne,
        /// The new parent is the node before the leaving node in the thread, where that is not the root.
        Adjacent,
        /// The leaving node is a leaf.
        Leaf,
        /// The leaving node is an ancestor of the thread's last node.
        ThreadsLast,
    }

    /// The choices of one exchange, each read against the tree it is made on.
    #[derive(Clone, Copy, Debug)]
    pub(in crate::tree) struct ExchangePick {
        /// What the pick aims at.
        aim: Aim,
        /// The leaving node, among those the aim allows.
        cut: Index,
        /// The entering end, among the nodes of the leaving node's subtree.
        enter: Index,
        /// The new parent, among the nodes outside that subtree.
        attach: Index,
        /// The side of the cycle the leaving node is found on.
        side: Side,
        /// The entering link, among the tree's links.
        link: Index,
        /// Which way the entering link runs, seen from the entering end.
        direction: Direction,
    }

    /// What one exchange was. Plain data.
    #[derive(Clone, Copy, Debug)]
    pub(in crate::tree) struct Kind {
        /// The side of the cycle the leaving node lay on.
        side: Side,
        /// The number of stem nodes.
        stem: usize,
        /// Whether the moved subtree followed its new parent in the thread.
        adjacent: bool,
        /// Whether the moved subtree held the thread's last node.
        holds_last: bool,
        /// Whether the apex was the root.
        apex_at_root: bool,
    }

    /// A loaded tree with its model and the cost of each link, which exchange after exchange on that one tree keep
    /// agreeing.
    #[derive(Debug)]
    pub(in crate::tree) struct ExchangeRun<'r> {
        /// The tree under test, loaded once for the whole run.
        pub(in crate::tree) tree: LentTree<'r>,
        /// Its model.
        pub(in crate::tree) model: PlainTree,
        /// Per link: its cost, under which every tree link is tight.
        pub(in crate::tree) costs: Vec<i64>,
    }

    impl<'r> ExchangeRun<'r> {
        /// The run of `model` loaded into `room`, each link costing what `costs` gives.
        fn shaped(room: &'r mut TreeRoom, model: PlainTree, costs: Vec<i64>) -> Self {
            Self { tree: tree_of(room, &model, costs.len(), &costs), model, costs }
        }

        /// The run of the star `room` makes of `directions` after arcs costing `arc_costs`, a root link pointing down
        /// costing `root_cost`.
        pub(in crate::tree) fn star(
            room: &'r mut TreeRoom,
            directions: &[Direction],
            arc_costs: &[i64],
            root_cost: i64,
        ) -> Self {
            let tree = room.star(directions.len(), arc_costs.len(), directions.iter().copied(), root_cost);
            let root_links = directions.iter().map(|direction| match direction {
                Direction::Up => 0,
                Direction::Down => root_cost,
            });
            let costs = arc_costs.iter().copied().chain(root_links).collect();

            Self { tree, model: PlainTree::star(directions, arc_costs.len()), costs }
        }

        /// Makes the exchange `pick` aims at on the run's tree, with the shift that makes the entering link tight, and
        /// checks every column against the model's exchange; answers what the exchange was, or `None` where every
        /// node outside the leaving node's subtree is the root, so that no arc can enter.
        ///
        /// # Errors
        /// Returns the model's message where a column differs.
        fn exchange(&mut self, pick: ExchangePick) -> Result<Option<Kind>, String> {
            let columns = columns_of(&self.tree);
            let root = self.model.root();
            let thread_last = columns.reverse_threads[root];
            let candidates: Vec<usize> = match pick.aim {
                Aim::Leaf => (0..root).filter(|&node| columns.subtree_sizes[node] == 1).collect(),
                Aim::ThreadsLast => self.model.path_below(thread_last, root),
                Aim::Any | Aim::StemOfOne | Aim::Adjacent => (0..root).collect(),
            };
            let cut = candidates[pick.cut.index(candidates.len())];
            let moved = self.model.subtree(cut);
            let outside: Vec<usize> = (0..root).filter(|node| !moved.contains(node)).collect();
            if outside.is_empty() {
                return Ok(None);
            }

            let enter = if let Aim::StemOfOne = pick.aim { cut } else { moved[pick.enter.index(moved.len())] };
            let before_cut = columns.reverse_threads[cut];
            let attach = match pick.aim {
                Aim::Adjacent if before_cut != root => before_cut,
                _ => outside[pick.attach.index(outside.len())],
            };
            let link = pick.link.index(self.costs.len());
            let tight = match pick.direction {
                Direction::Up => columns.potentials[attach] - self.costs[link],
                Direction::Down => columns.potentials[attach] + self.costs[link],
            };
            let shift = tight - columns.potentials[enter];
            let kind = Kind {
                side: pick.side,
                stem: self.model.path_below(enter, self.model.parents[cut]).len(),
                adjacent: before_cut == attach,
                holds_last: moved.contains(&thread_last),
                apex_at_root: self.model.apex(enter, attach) == root,
            };

            let ends = match pick.side {
                Side::First => [enter, attach],
                Side::Second => [attach, enter],
            };
            apply_exchange(exchange_keeping(&mut self.tree, ends, pick.side, cut), link, pick.direction, shift);
            self.model = self.model.exchanged(enter, attach, cut, (link, pick.direction));

            self.model.check(&columns_of(&self.tree), Some(&self.costs)).map(|()| Some(kind))
        }
    }

    /// The exchanges one property case makes, each checked against the model.
    const EXCHANGES: usize = 48;

    /// One exchange a test aims at, drawn.
    pub(in crate::tree) fn exchange_picks() -> impl Strategy<Value = ExchangePick> {
        let aims = prop_oneof![
            4 => Just(Aim::Any),
            1 => Just(Aim::StemOfOne),
            2 => Just(Aim::Adjacent),
            1 => Just(Aim::Leaf),
            1 => Just(Aim::ThreadsLast),
        ];
        let sides = prop_oneof![Just(Side::First), Just(Side::Second)];
        let directions = prop_oneof![Just(Direction::Up), Just(Direction::Down)];
        (aims, any::<[Index; 4]>(), sides, directions).prop_map(|(aim, [cut, enter, attach, link], side, direction)| {
            ExchangePick { aim, cut, enter, attach, side, link, direction }
        })
    }

    /// Random trees of every shape, each link costing within ±100.
    fn shaped_trees() -> impl Strategy<Value = (PlainTree, Vec<i64>)> {
        plain_trees().prop_flat_map(|model| {
            let link_count = link_count_of(&model);
            (Just(model), vec(-100_i64..=100, link_count))
        })
    }

    /// A room a star of 64 nodes and 80 arcs, larger than any tree the runs make, was written into and left behind.
    fn room_left_by_a_larger_star() -> TreeRoom {
        let mut room = TreeRoom::new();
        let _ = room.star(64, 80, [Direction::Down, Direction::Up].into_iter().cycle().take(64), 9);

        room
    }

    /// A fresh room, or one a larger star left behind where `left_behind` says so.
    fn room(left_behind: bool) -> TreeRoom {
        if left_behind { room_left_by_a_larger_star() } else { TreeRoom::new() }
    }

    /// Runs `picks` on `run`, one exchange after another on its one tree, failing at the first exchange whose tree
    /// the model does not rebuild.
    pub(in crate::tree) fn run_exchanges(
        run: &mut ExchangeRun<'_>,
        picks: &[ExchangePick],
    ) -> Result<Vec<Kind>, TestCaseError> {
        let mut kinds = Vec::with_capacity(picks.len());
        for (number, &pick) in picks.iter().enumerate() {
            let kind =
                run.exchange(pick).map_err(|breach| TestCaseError::fail(format!("exchange {number}: {breach}")))?;
            kinds.extend(kind);
        }

        Ok(kinds)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn test_random_exchanges_agree_with_the_model_in_every_column(
            (model, costs) in shaped_trees(),
            left_behind in any::<bool>(),
            picks in vec(exchange_picks(), EXCHANGES),
        ) {
            let mut room = room(left_behind);
            let mut run = ExchangeRun::shaped(&mut room, model, costs);
            prop_assert_eq!(run.model.check(&columns_of(&run.tree), Some(&run.costs)), Ok(()));

            run_exchanges(&mut run, &picks)?;
        }

        #[test]
        fn test_random_exchanges_from_the_star_agree_with_the_model_in_every_column(
            directions in vec(prop_oneof![Just(Direction::Up), Just(Direction::Down)], 2..=24),
            arc_costs in vec(-100_i64..=100, 0..=12),
            left_behind in any::<bool>(),
            picks in vec(exchange_picks(), EXCHANGES),
        ) {
            let mut room = room(left_behind);
            let mut run = ExchangeRun::star(&mut room, &directions, &arc_costs, 1_000);
            prop_assert_eq!(run.model.check(&columns_of(&run.tree), Some(&run.costs)), Ok(()));

            run_exchanges(&mut run, &picks)?;
        }
    }

    #[test]
    fn test_the_random_exchanges_reach_every_kind_of_exchange() {
        let mut runner = TestRunner::deterministic();
        let cases = (shaped_trees(), vec(exchange_picks(), EXCHANGES));
        let mut kinds = Vec::new();
        for _ in 0..64 {
            let ((model, costs), picks) = cases.new_tree(&mut runner).expect("a case should be drawn").current();
            let mut room = TreeRoom::new();
            let mut run = ExchangeRun::shaped(&mut room, model, costs);
            kinds.extend(run_exchanges(&mut run, &picks).expect("every exchange should agree with the model"));
        }

        let count = |reached: fn(&Kind) -> bool| kinds.iter().filter(|kind| reached(kind)).count();
        assert!(count(|kind| kind.side == Side::First) > 0 && count(|kind| kind.side == Side::Second) > 0);
        let stems: BTreeSet<usize> = kinds.iter().map(|kind| kind.stem).collect();
        assert!((1..=10).all(|stem| stems.contains(&stem)), "the stems reached: {stems:?}");
        assert!(count(|kind| kind.adjacent && kind.stem == 1) > 0, "a block that stays in place");
        assert!(count(|kind| kind.adjacent && kind.stem > 1) > 0, "a stem whose subtree stays in place");
        assert!(count(|kind| kind.holds_last) > 0, "a subtree that holds the thread's last node");
        assert!(count(|kind| kind.apex_at_root) > 0 && count(|kind| !kind.apex_at_root) > 0);
    }

    /// Whether `top` is `node` or one of its ancestors under `parents`, the root last.
    fn holds(parents: &[usize], top: usize, node: usize) -> bool {
        let root = parents.len() - 1;
        let mut at = node;
        for _ in 0..parents.len() {
            if at == top {
                return true;
            }
            if at == root {
                return false;
            }
            at = parents[at];
        }

        false
    }

    /// Checks `after`, the columns after the move of `enter`'s side up to `cut` under `attach` by `link`, shifted by
    /// `shift`, against `before`, from the parent links alone.
    ///
    /// # Errors
    /// Returns which column differs.
    fn check_from_parents(
        before: &PlainColumns,
        after: &PlainColumns,
        [enter, attach, cut]: [usize; 3],
        entering: (usize, Direction),
        shift: i64,
    ) -> Result<(), String> {
        let count = before.parents.len();
        let root = count - 1;

        // The stem reversed and the entering end under the other end, each stem node's link carried up, turned.
        let (mut parents, mut hangings) = (before.parents.clone(), before.hangings.clone());
        let (mut node, mut below, mut hanging) = (enter, attach, entering);
        // The stem is a path up the tree from `enter` to `cut`: at most one step a node below the root.
        for steps in 0.. {
            assert!(steps < root, "the stem from {enter} should reach {cut} within the {root} nodes below the root");
            let (link, direction) = before.hangings[node];
            (parents[node], hangings[node]) = (below, hanging);
            hanging = (link, if direction == Direction::Up { Direction::Down } else { Direction::Up });
            if node == cut {
                break;
            }
            (below, node) = (node, before.parents[node]);
        }
        if (&after.parents, &after.hangings) != (&parents, &hangings) {
            return Err("the parents or links".to_owned());
        }

        let mut order = vec![root];
        while order.len() < count && after.threads[order[order.len() - 1]] != root {
            order.push(after.threads[order[order.len() - 1]]);
        }
        let mut seen = order.clone();
        seen.sort_unstable();
        if seen != (0..count).collect::<Vec<_>>() || after.threads[order[count - 1]] != root {
            return Err("the thread".to_owned());
        }
        if (0..count).any(|node| after.reverse_threads[after.threads[node]] != node) {
            return Err("the reverse thread".to_owned());
        }
        for (at, &top) in order.iter().enumerate() {
            let size = (0..count).filter(|&node| holds(&parents, top, node)).count();
            let run_holds = order[at..at + size].iter().all(|&node| holds(&parents, top, node));
            if after.subtree_sizes[top] != size || !run_holds || after.last_successors[top] != order[at + size - 1] {
                return Err(format!("the run of {top}"));
            }
        }

        let moved = |node: usize| holds(&before.parents, cut, node);
        let shifted = (0..count).map(|node| before.potentials[node] + if moved(node) { shift } else { 0 });
        if !after.potentials.iter().copied().eq(shifted) {
            return Err("the potentials".to_owned());
        }

        Ok(())
    }

    /// The most nodes below the root of the trees the exhaustive check reaches from the star.
    const EXHAUSTIVE_NODES: usize = 5;

    /// The most threaded trees the exhaustive check can meet: a tree on the five nodes and the root is one of the
    /// `6^4` labelled trees on six nodes, and its thread visits the five nodes after the root in one of `5!` orders.
    const THREADED_TREES: usize = 6_usize.pow(4) * (2 * 3 * 4 * 5);

    #[test]
    fn test_every_exchange_reachable_from_the_star_keeps_every_column() {
        let (states, exchanges) = {
            // The room first holds a larger tree, so every star and load below writes over what that one left.
            let mut room = room_left_by_a_larger_star();
            let directions = [Direction::Up, Direction::Down].into_iter().cycle().take(EXHAUSTIVE_NODES);
            let start = columns_of(&room.star(EXHAUSTIVE_NODES, 1, directions, 3));
            let link_count = EXHAUSTIVE_NODES + 1;
            let root = EXHAUSTIVE_NODES;

            let mut seen = HashSet::from([(start.parents.clone(), start.threads.clone())]);
            let mut queue = VecDeque::from([start]);
            let mut exchanges = 0_usize;
            while let Some(state) = queue.pop_front() {
                for cut in 0..root {
                    let moved: Vec<usize> = (0..root).filter(|&node| holds(&state.parents, cut, node)).collect();
                    for &enter in &moved {
                        let attaches = (0..root).filter(|node| !moved.contains(node));
                        for (attach, side) in
                            attaches.flat_map(|attach| [(attach, Side::First), (attach, Side::Second)])
                        {
                            let ends = if side == Side::First { [enter, attach] } else { [attach, enter] };
                            let direction = if attach % 2 == 0 { Direction::Up } else { Direction::Down };
                            let entering = (cut % link_count, direction);

                            // Every state starts many exchanges and an exchange has no undo, so each one starts from
                            // its state written into the room again.
                            let mut tree = load(&mut room, &state, link_count);
                            apply_exchange(exchange_keeping(&mut tree, ends, side, cut), entering.0, direction, 1);
                            let after = columns_of(&tree);
                            let checked = check_from_parents(&state, &after, [enter, attach, cut], entering, 1);
                            checked.unwrap_or_else(|column| panic!("{column} after {enter}, {attach}, {cut}"));

                            exchanges += 1;
                            if seen.insert((after.parents.clone(), after.threads.clone())) {
                                assert!(seen.len() <= THREADED_TREES, "the check should meet no more trees than exist");
                                queue.push_back(after);
                            }
                        }
                    }
                }
            }

            (seen.len(), exchanges)
        };

        // Every threaded tree on five nodes below the root that the exchanges reach, and every exchange of each, its
        // leaving node found on either side.
        assert_eq!((states, exchanges), (2_721, 109_160));
    }
}
