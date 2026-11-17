// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The walk of a tree from one node toward the root: each node lent in turn, keeping its link or hanging from the root
//! by its own root link with its subtree, which is a cut where it hung from another node and a turn where it hung
//! from the root the other way.

use super::rehang::Cut;
use super::spanning_tree::{
    Direction, Hanging, HungNode, LentTree, Link, SpanningTree, TreeNode, root_link_cost, tight_potential,
};

/// The walk of a tree from one node toward the root, one node a step: a lending walk whose every step is decided, its
/// node keeping its link or hanging from the root by its own root link the way it is told, with its subtree.
///
/// It lends the node it starts at, then the parent each node had before it was decided, so the nodes of the path from
/// its start up, and ends before the node it was told to stop before, or at the root. A cut moves only the subtree of
/// the node decided, which the walk has left, so the path above is the path it had. It owns the tree while it writes
/// it, and [`finish`](Self::finish) hands it back; every decision leaves the tree's invariant whole, so a walk may be
/// finished at any step. A walk dropped before it is finished drops the tree with it.
///
/// A node that stays costs O(1); one cut, the nodes of its subtree and its old ancestors; a turn, the nodes of its
/// subtree. It allocates nothing.
///
/// # Invariant
/// Only [`new`](Self::new) makes one, from a whole tree it takes. `next` is a tree node of `tree`, and between steps
/// the tree's invariant holds.
#[derive(Debug)]
pub(crate) struct RootwardWalk<'t> {
    /// The tree walked.
    tree: LentTree<'t>,
    /// The node the next step lends, unless it is the root or the node to stop before.
    next: TreeNode,
    /// The node the walk ends before, a raw node: where it is no node on the path, the walk ends at the root.
    stop: usize,
}

/// One node of a walk toward the root, not decided yet, lent until it is: the walk moves on only when it is decided,
/// and a step that is dropped undecided is lent again.
///
/// It holds the walk's tree for `'w`, a tree lent for `'t`: the two lifetimes, since the tree behind a `&mut` cannot
/// be named for less long than it was lent.
///
/// # Invariant
/// Only [`RootwardWalk::step`] makes one, of the walk's `next`, a hung node of `tree`: `parent` is its parent, read
/// before any change the decision makes.
#[derive(Debug)]
pub(crate) struct RootwardStep<'w, 't> {
    /// The walk's tree.
    tree: &'w mut SpanningTree<'t>,
    /// The node to decide.
    node: HungNode,
    /// The node's parent: where the walk goes on once the node is decided.
    parent: TreeNode,
    /// The walk's next node.
    next: &'w mut TreeNode,
}

impl<'t> RootwardWalk<'t> {
    /// The walk of `tree` from the tree node `from` toward the root, ending before the node `stop`, or at the root
    /// where `stop` is no node of the path from `from` up; nothing decided yet. The root as `from` makes a walk that
    /// lends nothing.
    ///
    /// # Panics
    /// Panics if `from` is not below the tree's `n + 1` tree nodes.
    #[track_caller]
    pub(crate) fn new(tree: LentTree<'t>, from: usize, stop: usize) -> Self {
        let tree_nodes = tree.node_count() + 1;
        let Some(next) = tree.tree_node(from) else {
            panic!("called `RootwardWalk::new` from node {from}, which should be below the {tree_nodes} tree nodes")
        };

        Self { tree, next, stop }
    }

    /// The tree, every decision made so far kept, its invariant whole.
    pub(crate) fn finish(self) -> LentTree<'t> {
        self.tree
    }

    /// The next node to decide; `None` at the node to stop before, at the root, and ever after.
    pub(crate) fn step(&mut self) -> Option<RootwardStep<'_, 't>> {
        let Self { tree, next, stop } = self;
        if usize::from(*next) == *stop {
            return None;
        }

        let tree = tree.tree_mut();
        // The root is the one tree node that is not hung.
        let columns = tree.columns_mut();
        let node = columns.hung(*next)?;
        let parent = columns.parent(*next);

        Some(RootwardStep { tree, node, parent, next })
    }
}

impl RootwardStep<'_, '_> {
    /// The link the node hangs by, and which way it runs seen from the node.
    pub(crate) fn hanging(&self) -> Hanging<'_> {
        self.tree.hanging(self.node)
    }

    /// The node's root link, which it hangs by from the root.
    pub(crate) fn root_link(&self) -> Link<'_> {
        self.tree.root_link(self.node)
    }

    /// Keeps the node on its link, under its parent.
    pub(crate) fn stay(self) {
        *self.next = self.parent;
    }

    /// Hangs the node from the root by its own root link, running `direction` seen from the node, with its subtree,
    /// every potential of which moves alike so that the root link is tight, under the cost `root_cost` of a root link
    /// that points down and zero for one that points up.
    ///
    /// Where the node hung from another node, that is a cut, and the subtree's run in the thread moves to just after
    /// the root. Where it hung from the root already, it keeps its place, and where it hung by its root link the way it
    /// is told, nothing changes; otherwise it turns to that link and that way. Either way the walk goes on at the
    /// node's parent before the decision, the root where the node hung from it.
    ///
    /// It answers the node, the top of the subtree, where the subtree's potentials moved, and `None` where the root
    /// link was tight already and none moved. Then every link with exactly one end in the subtree has changed its
    /// reduced cost by the move, and no other link has.
    pub(crate) fn hang_from_root(self, direction: Direction, root_cost: i64) -> Option<usize> {
        let Self { tree, node, parent, next } = self;
        let root_link = tree.root_link(node);
        let (root, top) = (tree.root(), node.node());

        // The potential that makes the root link tight below the root, less the node's: the move of the subtree.
        let columns = tree.columns_mut();
        let tight = tight_potential(columns.potential(root), root_link_cost(direction, root_cost), direction);
        let shift = tight.wrapping_sub(columns.potential(top));

        match tree.cut_ends(node) {
            Some(ends) => Cut::of(ends).apply(direction, shift),
            None => {
                let mut columns = tree.columns_mut();
                if columns.hanging(node) != (Hanging { link: root_link, direction }) {
                    columns.set_hanging(node, root_link, direction);
                    let size = columns.subtree_size(top);
                    columns.shift_run(top, size, shift);
                }
            }
        }

        *next = parent;
        // A node that keeps the root link it hung by, the way it ran, was tight below the root: its shift is zero.
        (shift != 0).then_some(usize::from(top))
    }
}

#[cfg(test)]
mod tests {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::sample::Index;
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    use super::*;
    use crate::tree::plain_tree::{Decision, PlainColumns, PlainTree, link_count_of, plain_trees, seven_nodes};
    use crate::tree::rehang::tests::{ExchangeRun, exchange_picks, run_exchanges};
    use crate::tree::spanning_tree::TreeRoom;
    use crate::tree::spanning_tree::tests::{columns_of, load, plain_tree_of, tree_of};

    const UP: Direction = Direction::Up;
    const DOWN: Direction = Direction::Down;

    /// The cost of a root link that points down in the hand cases.
    const ROOT_COST: i64 = 9;

    /// Walks `tree` from `from`, ending before `stop`, telling each node lent what `decide` answers for it, and answers
    /// the nodes lent, in order, with the tree. Each node told to hang from the root must answer itself exactly where
    /// its potential moved.
    ///
    /// # Panics
    /// Panics if the walk lends more nodes than the tree has below the root, or if a node answers otherwise.
    fn walked<'r>(
        tree: LentTree<'r>,
        from: usize,
        stop: usize,
        decide: impl Fn(usize) -> Decision,
    ) -> (Vec<usize>, LentTree<'r>) {
        let hung_nodes = tree.node_count();
        let mut walk = RootwardWalk::new(tree, from, stop);
        let mut lent = Vec::new();
        while let Some(step) = walk.step() {
            assert!(lent.len() < hung_nodes, "a walk should lend each node of its path once");
            let node = usize::from(step.node);
            lent.push(node);
            let before = step.tree.potentials()[node];
            match decide(node) {
                Decision::Stay => step.stay(),
                Decision::Root(direction) => {
                    let moved = step.hang_from_root(direction, ROOT_COST);
                    let after = walk.tree.potentials()[node];
                    assert_eq!(moved, (before != after).then_some(node), "node {node} moved from {before} to {after}");
                }
            }
        }

        (lent, walk.finish())
    }

    #[test]
    fn test_a_cut_of_a_node_below_a_top_moves_its_run_after_the_root_by_hand() {
        // Seven nodes at potential zero, the thread 7, 0, 1, …, 6. Node 1, under node 0, which is under the root, is
        // cut by its root link 8 pointing down: its subtree 1, 2, 3 follows the root and rises to 9; node 0 keeps 4.
        let mut room = TreeRoom::new();
        let tree = plain_tree_of(&mut room, &seven_nodes());
        let (lent, tree) = walked(tree, 1, 0, |_| Decision::Root(DOWN));
        assert_eq!(lent, [1], "the walk ends before node 0");

        let mut hangings = seven_nodes().hangings;
        hangings[1] = (8, DOWN);
        let expected = PlainColumns {
            parents: vec![7, 7, 1, 2, 0, 7, 5, 7],
            hangings,
            threads: vec![4, 2, 3, 0, 5, 6, 7, 1],
            reverse_threads: vec![3, 7, 1, 2, 0, 4, 5, 6],
            last_successors: vec![4, 3, 3, 3, 4, 6, 6, 6],
            subtree_sizes: vec![2, 3, 2, 1, 1, 2, 1, 8],
            potentials: vec![0, 9, 9, 9, 0, 0, 0, 0],
        };
        assert_eq!(columns_of(&tree), expected);
        assert_eq!(seven_nodes().hung_from_root(1, DOWN, 14).check(&expected, None), Ok(()));
    }

    #[test]
    fn test_a_cut_of_the_threads_last_node_ends_its_old_ancestors_runs_before_it_by_hand() {
        // Node 6, the thread's last, under node 5, which is under the root, cut pointing up: it follows the root, and
        // the runs of node 5 and of the root now end at node 5. No potential moves: up, its root link costs nothing.
        let mut room = TreeRoom::new();
        let tree = plain_tree_of(&mut room, &seven_nodes());
        let (lent, tree) = walked(tree, 6, 7, |node| if node == 6 { Decision::Root(UP) } else { Decision::Stay });
        assert_eq!(lent, [6, 5], "node 5 is lent too, and stays");

        let mut hangings = seven_nodes().hangings;
        hangings[6] = (13, UP);
        let expected = PlainColumns {
            parents: vec![7, 0, 1, 2, 0, 7, 7, 7],
            hangings,
            threads: vec![1, 2, 3, 4, 5, 7, 0, 6],
            reverse_threads: vec![6, 0, 1, 2, 3, 4, 7, 5],
            last_successors: vec![4, 3, 3, 3, 4, 5, 6, 5],
            subtree_sizes: vec![5, 3, 2, 1, 1, 1, 1, 8],
            potentials: vec![0; 8],
        };
        assert_eq!(columns_of(&tree), expected);
    }

    #[test]
    fn test_a_cut_inside_a_cut_and_a_turn_on_one_walk_by_hand() {
        // From node 3 up: node 3 cut pointing down, to potential 9; node 2 stays; node 1 cut pointing up, taking node
        // 2, at potential 0; node 0, under the root by link 0, turns to its root link 7 pointing down, its subtree 0, 4
        // rising by 9. The thread: 7, 3, 0, 1, 2, 4, 5, 6 after the first cut, then 7, 1, 2, 3, 0, 4, 5, 6.
        let decisions = [Decision::Root(DOWN), Decision::Root(UP), Decision::Stay, Decision::Root(DOWN)];
        let mut room = TreeRoom::new();
        let tree = plain_tree_of(&mut room, &seven_nodes());
        let (lent, tree) = walked(tree, 3, 7, |node| decisions[node]);
        assert_eq!(lent, [3, 2, 1, 0]);

        let expected = PlainColumns {
            parents: vec![7, 7, 1, 7, 0, 7, 5, 7],
            hangings: vec![(7, DOWN), (8, UP), (2, DOWN), (10, DOWN), (4, UP), (5, UP), (6, UP)],
            threads: vec![4, 2, 3, 0, 5, 6, 7, 1],
            reverse_threads: vec![3, 7, 1, 2, 0, 4, 5, 6],
            last_successors: vec![4, 2, 2, 3, 4, 6, 6, 6],
            subtree_sizes: vec![2, 2, 1, 1, 1, 2, 1, 8],
            potentials: vec![9, 0, 0, 9, 9, 0, 0, 0],
        };
        assert_eq!(columns_of(&tree), expected);

        // The model, each root link costing what its direction asks: 9 down, nothing up.
        let model = seven_nodes().hung_from_root(3, DOWN, 14).hung_from_root(1, UP, 14).hung_from_root(0, DOWN, 14);
        let mut costs = vec![0; 14];
        (costs[7], costs[10]) = (ROOT_COST, ROOT_COST);
        assert_eq!(model.check(&expected, Some(&costs)), Ok(()));
    }

    #[test]
    fn test_a_node_under_the_root_turns_or_keeps_its_place_and_ends_the_walk_by_hand() {
        // One node under the root by its root link 1 of 2 links, pointing up at potential 0: turned down it rises to
        // 9, turned up again it falls back; told the way it points, nothing moves.
        let one = PlainColumns {
            parents: vec![1, 1],
            hangings: vec![(1, UP)],
            threads: vec![1, 0],
            reverse_threads: vec![1, 0],
            last_successors: vec![0, 0],
            subtree_sizes: vec![1, 2],
            potentials: vec![0, 0],
        };
        let mut room = TreeRoom::new();
        let (lent, tree) = walked(load(&mut room, &one, 2), 0, 1, |_| Decision::Root(DOWN));
        let turned = PlainColumns { hangings: vec![(1, DOWN)], potentials: vec![9, 0], ..one.clone() };
        assert_eq!((lent, columns_of(&tree)), (vec![0], turned.clone()));

        let (_, tree) = walked(tree, 0, 1, |_| Decision::Root(DOWN));
        assert_eq!(columns_of(&tree), turned, "the way it points: nothing moves");
        let (_, tree) = walked(tree, 0, 1, |_| Decision::Root(UP));
        assert_eq!(columns_of(&tree), one);

        // From the root, or with the start to stop before, a walk lends nothing.
        let (lent, tree) = walked(tree, 1, 5, |_| Decision::Root(DOWN));
        let (none, tree) = walked(tree, 0, 0, |_| Decision::Root(DOWN));
        assert_eq!((lent, none, columns_of(&tree)), (vec![], vec![], one));
    }

    #[test]
    fn test_a_step_dropped_undecided_is_lent_again_and_a_walk_finishes_at_any_step_by_hand() {
        let mut room = TreeRoom::new();
        let mut walk = RootwardWalk::new(plain_tree_of(&mut room, &seven_nodes()), 3, 7);
        let first = walk.step().map(|step| usize::from(step.node));
        let again = walk.step().expect("the dropped node should be lent again");
        assert_eq!((first, usize::from(again.node)), (Some(3), 3));
        let Hanging { link, direction } = again.hanging();
        assert_eq!((usize::from(link), direction, usize::from(again.root_link())), (3, UP, 10));
        again.stay();

        // Finished after one decision, the tree is whole and unchanged.
        walk.step().expect("node 2 should be lent").stay();
        let tree = walk.finish();
        assert_eq!(seven_nodes().check(&columns_of(&tree), Some(&[0; 14])), Ok(()));
    }

    #[test]
    #[should_panic(expected = "called `RootwardWalk::new` from node 8, which should be below the 8 tree nodes")]
    fn test_a_walk_refuses_a_start_past_the_tree_nodes() {
        let mut room = TreeRoom::new();
        let _ = RootwardWalk::new(plain_tree_of(&mut room, &seven_nodes()), 8, 7);
    }

    /// One change of a property case's tree: an exchange, or a walk from the node drawn, ending before the node drawn,
    /// each node lent told the decision drawn for it.
    #[derive(Clone, Debug)]
    enum Change {
        /// An exchange.
        Exchange(crate::tree::rehang::tests::ExchangePick),
        /// A walk toward the root.
        Walk {
            /// The node it starts at, among the tree nodes.
            from: Index,
            /// The node it ends before, among the tree nodes and one past them.
            stop: Index,
            /// Per node: what it is told where it is lent.
            decisions: Vec<Decision>,
        },
    }

    /// What a property case reached, counted. Plain data.
    #[derive(Clone, Copy, Debug, Default)]
    struct Reached {
        /// Cuts of a node right after its parent in the thread.
        right_after_parent: usize,
        /// Cuts of a node whose subtree held the thread's last node.
        holding_last: usize,
        /// Cuts of a node whose parent hung from the root.
        below_top: usize,
        /// Cuts of a node whose parent is deeper, not a top.
        deeper: usize,
        /// Walks that cut two nodes or more.
        cut_twice: usize,
        /// Turns.
        turns: usize,
        /// Nodes under the root told the way they hung.
        kept_place: usize,
        /// Exchanges with the apex at the root, after a cut.
        exchanged_at_root_after_cut: usize,
        /// Turns of the one node of a tree of one node.
        turns_alone: usize,
        /// Cuts of the last child of a hung node, whose old ancestors' runs ended with its subtree.
        last_child_below_a_hung_node: usize,
        /// Nodes hung from the root, by a cut or a turn, whose potentials did not move.
        unmoved: usize,
    }

    /// Changes of one case: sixteen, alternating walks and exchanges at random.
    fn changes(nodes: usize) -> impl Strategy<Value = Vec<Change>> {
        let decisions = prop_oneof![
            2 => Just(Decision::Stay),
            1 => Just(Decision::Root(UP)),
            1 => Just(Decision::Root(DOWN)),
        ];
        let walk = (any::<Index>(), any::<Index>(), vec(decisions, nodes))
            .prop_map(|(from, stop, decisions)| Change::Walk { from, stop, decisions });
        let exchange = exchange_picks().prop_map(Change::Exchange);

        vec(prop_oneof![walk, exchange], 16)
    }

    /// Makes `run`'s changes, checking every column against the model after each exchange and after each decision of
    /// a walk; a root link costs its direction's cost, nothing up and [`ROOT_COST`] down. A node whose root link
    /// another node hangs by is told to stay. Answers what the run reached.
    fn run_changes(run: ExchangeRun<'_>, changes: &[Change]) -> Result<Reached, TestCaseError> {
        let mut run = run;
        let mut reached = Reached::default();
        let mut cut_before = false;
        for change in changes {
            match change {
                Change::Exchange(pick) => {
                    let kinds = run_exchanges(&mut run, &[*pick])?;
                    let at_root = kinds.iter().filter(|kind| kind.apex_at_root).count();
                    reached.exchanged_at_root_after_cut += if cut_before { at_root } else { 0 };
                }
                Change::Walk { from, stop, decisions } => {
                    let walked =
                        walk_checked(run, from.index(decisions.len() + 1), stop.index(decisions.len() + 2), decisions)?;
                    (run, cut_before) = (walked.0, cut_before || walked.1.cuts > 0);
                    let counts = walked.1;
                    reached.right_after_parent += counts.right_after_parent;
                    reached.holding_last += counts.holding_last;
                    reached.below_top += counts.below_top;
                    reached.deeper += counts.deeper;
                    reached.cut_twice += usize::from(counts.cuts >= 2);
                    reached.turns += counts.turns;
                    reached.kept_place += counts.kept_place;
                    reached.turns_alone += if decisions.len() == 1 { counts.turns } else { 0 };
                    reached.last_child_below_a_hung_node += counts.last_child_below_a_hung_node;
                    reached.unmoved += counts.unmoved - counts.kept_place;
                }
            }
        }

        Ok(reached)
    }

    /// What one walk did, counted. Plain data.
    #[derive(Clone, Copy, Debug, Default)]
    struct WalkCounts {
        /// Cuts.
        cuts: usize,
        /// Cuts of a node right after its parent in the thread.
        right_after_parent: usize,
        /// Cuts of a node whose subtree held the thread's last node.
        holding_last: usize,
        /// Cuts of a node whose parent hung from the root.
        below_top: usize,
        /// Cuts of a node whose parent is not a top.
        deeper: usize,
        /// Turns.
        turns: usize,
        /// Nodes under the root told the way they hung.
        kept_place: usize,
        /// Cuts of the last child of a hung node, whose old ancestors' runs ended with its subtree.
        last_child_below_a_hung_node: usize,
        /// Nodes hung from the root whose potentials did not move.
        unmoved: usize,
    }

    /// One walk of `run`'s tree from `from`, ending before `stop`, each node lent told `decisions[node]`, checked
    /// against the model after every decision; answers the run and what the walk did.
    fn walk_checked<'r>(
        run: ExchangeRun<'r>,
        from: usize,
        stop: usize,
        decisions: &[Decision],
    ) -> Result<(ExchangeRun<'r>, WalkCounts), TestCaseError> {
        let ExchangeRun { tree, mut model, mut costs } = run;
        let (root, link_count) = (model.root(), costs.len());
        let expected_path = if stop <= root && model.ancestors(from).contains(&stop) {
            model.path_below(from, stop)
        } else if from == root {
            vec![]
        } else {
            model.path_below(from, root)
        };

        let mut walk = RootwardWalk::new(tree, from, stop);
        let mut counts = WalkCounts::default();
        let mut lent = Vec::new();
        while let Some(step) = walk.step() {
            prop_assert!(lent.len() < root, "a walk should lend each node of its path once");
            let node = usize::from(step.node);
            lent.push(node);
            let root_link = link_count - root + node;
            let shared = (0..root).any(|other| other != node && model.hangings[other].0 == root_link);
            let decision = if shared { Decision::Stay } else { decisions[node] };
            let Decision::Root(direction) = decision else {
                step.stay();
                continue;
            };

            let columns = columns_of(step.tree);
            let parent = model.parents[node];
            if parent == root {
                let unchanged = model.hangings[node] == (root_link, direction);
                counts.kept_place += usize::from(unchanged);
                counts.turns += usize::from(!unchanged);
            } else {
                counts.cuts += 1;
                counts.right_after_parent += usize::from(columns.reverse_threads[node] == parent);
                counts.holding_last += usize::from(model.subtree(node).contains(&columns.reverse_threads[root]));
                counts.below_top += usize::from(model.parents[parent] == root);
                counts.deeper += usize::from(model.parents[parent] != root);
                let last_child = columns.last_successors[parent] == columns.last_successors[node];
                counts.last_child_below_a_hung_node += usize::from(last_child);
            }
            let moved = step.hang_from_root(direction, ROOT_COST);

            model = model.hung_from_root(node, direction, link_count);
            costs[root_link] = root_link_cost(direction, ROOT_COST);
            let after = columns_of(&walk.tree);
            model.check(&after, Some(&costs)).map_err(TestCaseError::fail)?;
            // The node answers itself exactly where the potentials of its subtree moved, each by the same amount.
            let shifted = columns.potentials[node] != after.potentials[node];
            prop_assert_eq!(moved, shifted.then_some(node));
            counts.unmoved += usize::from(!shifted);
        }
        prop_assert_eq!(lent, expected_path);

        Ok((ExchangeRun { tree: walk.finish(), model, costs }, counts))
    }

    /// Random trees of every shape, each link costing within ±100, every link tight.
    fn shaped_trees() -> impl Strategy<Value = (PlainTree, Vec<i64>, Vec<Change>)> {
        plain_trees().prop_flat_map(|model| {
            let link_count = link_count_of(&model);
            let nodes = model.root();
            (Just(model), vec(-100_i64..=100, link_count), changes(nodes))
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn test_random_walks_and_exchanges_agree_with_the_model_in_every_column(
            (model, costs, changes) in shaped_trees(),
        ) {
            let mut room = TreeRoom::new();
            let tree = tree_of(&mut room, &model, costs.len(), &costs);
            run_changes(ExchangeRun { tree, model, costs }, &changes)?;
        }
    }

    #[test]
    fn test_the_random_walks_reach_every_kind_of_cut_and_turn() {
        let mut runner = TestRunner::deterministic();
        let mut total = Reached::default();
        for _ in 0..128 {
            let (model, costs, changes) =
                shaped_trees().new_tree(&mut runner).expect("a case should be drawn").current();
            let mut room = TreeRoom::new();
            let tree = tree_of(&mut room, &model, costs.len(), &costs);
            let reached = run_changes(ExchangeRun { tree, model, costs }, &changes).expect("every change should agree");
            total.right_after_parent += reached.right_after_parent;
            total.holding_last += reached.holding_last;
            total.below_top += reached.below_top;
            total.deeper += reached.deeper;
            total.cut_twice += reached.cut_twice;
            total.turns += reached.turns;
            total.kept_place += reached.kept_place;
            total.exchanged_at_root_after_cut += reached.exchanged_at_root_after_cut;
            total.turns_alone += reached.turns_alone;
            total.last_child_below_a_hung_node += reached.last_child_below_a_hung_node;
            total.unmoved += reached.unmoved;
        }

        let counts = [
            total.right_after_parent,
            total.holding_last,
            total.below_top,
            total.deeper,
            total.cut_twice,
            total.turns,
            total.kept_place,
            total.exchanged_at_root_after_cut,
            total.turns_alone,
            total.last_child_below_a_hung_node,
            total.unmoved,
        ];
        assert!(counts.iter().all(|&count| count > 0), "{total:?}");
    }
}
