// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The model the tree's tests check against: a rooted tree as each node's parent and the link it hangs by, with every
//! column a threaded tree derives recomputed from those alone by loops over explicit stacks.
//!
//! It knows two changes of a tree, each stated by its parent links: the stem reversed and the entering end hung under
//! the other end; and the prune, in which the nodes told to hang from the root do so by their root links. It knows
//! nothing of threads being cut and spliced, so it shares no reasoning with the tree's own updates.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::Index;

use super::spanning_tree::Direction;

/// A rooted tree on the nodes `0..=n`, the root `n` last, as each node's parent and the link it hangs by. Plain data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PlainTree {
    /// Per node: its parent; the root's is the root.
    pub(super) parents: Vec<usize>,
    /// Per node below the root, node `v` at `v`: the link it hangs by and which way that link runs.
    pub(super) hangings: Vec<(usize, Direction)>,
}

/// What a prune tells one node below the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Decision {
    /// It stays on its link, under its parent.
    Stay,
    /// It hangs from the root by its root link, running the way given, seen from the node.
    Root(Direction),
}

/// Every column of a threaded tree as plain numbers, as a test reads them out of the tree under test. Plain data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PlainColumns {
    /// Per node: its parent.
    pub(super) parents: Vec<usize>,
    /// Per node below the root: its link and which way it runs.
    pub(super) hangings: Vec<(usize, Direction)>,
    /// Per node: the node after it in the thread.
    pub(super) threads: Vec<usize>,
    /// Per node: the node before it in the thread.
    pub(super) reverse_threads: Vec<usize>,
    /// Per node: the last node of its subtree in the thread.
    pub(super) last_successors: Vec<usize>,
    /// Per node: the number of nodes of its subtree.
    pub(super) subtree_sizes: Vec<usize>,
    /// Per node: its potential, in units of cost.
    pub(super) potentials: Vec<i64>,
}

impl PlainTree {
    /// The star of `directions.len()` nodes: each node `v` under the root by the link `arc_count + v`, running the
    /// way `directions` gives.
    pub(super) fn star(directions: &[Direction], arc_count: usize) -> Self {
        let node_count = directions.len();
        let hangings = directions.iter().enumerate().map(|(node, &direction)| (arc_count + node, direction)).collect();

        Self { parents: vec![node_count; node_count + 1], hangings }
    }

    /// The root, the last node.
    pub(super) fn root(&self) -> usize {
        self.parents.len() - 1
    }

    /// The children of `node`, in ascending order.
    pub(super) fn children(&self, node: usize) -> Vec<usize> {
        (0..self.root()).filter(|&child| self.parents[child] == node).collect()
    }

    /// The nodes of the subtree of `node` in the preorder that visits each node's children in ascending order.
    ///
    /// # Panics
    /// Panics if the walk meets more nodes than the tree has, as parent links that run around a cycle make it.
    pub(super) fn subtree(&self, node: usize) -> Vec<usize> {
        let mut order = Vec::new();
        let mut stack = vec![node];
        while let Some(at) = stack.pop() {
            assert!(
                order.len() < self.parents.len(),
                "called `PlainTree::subtree` with {node}, whose walk passes the tree's {} nodes",
                self.parents.len()
            );
            order.push(at);
            stack.extend(self.children(at).into_iter().rev());
        }

        order
    }

    /// `node` and its ancestors, nearest first, the root last.
    ///
    /// # Panics
    /// Panics if the parent links lead `node` around a cycle that misses the root.
    pub(super) fn ancestors(&self, node: usize) -> Vec<usize> {
        let mut path = vec![node];
        let mut at = node;
        while at != self.root() {
            at = self.parents[at];
            assert!(
                path.len() <= self.parents.len(),
                "called `PlainTree::ancestors` with {node}, which misses the root"
            );
            path.push(at);
        }

        path
    }

    /// The deepest node that is an ancestor of both `first` and `second`, either of them included.
    ///
    /// # Panics
    /// Panics if the parent links lead either node around a cycle that misses the root.
    pub(super) fn apex(&self, first: usize, second: usize) -> usize {
        let above_second = self.ancestors(second);
        let ancestors = self.ancestors(first);

        ancestors.into_iter().find(|node| above_second.contains(node)).expect("the root should be above every node")
    }

    /// The nodes from `node` up to its ancestor `top`, `top` excluded, nearest `node` first.
    ///
    /// # Panics
    /// Panics if `top` is not an ancestor of `node`.
    pub(super) fn path_below(&self, node: usize, top: usize) -> Vec<usize> {
        let ancestors = self.ancestors(node);
        let length = ancestors.iter().position(|&at| at == top).expect("the top should be an ancestor of the node");

        ancestors[..length].to_vec()
    }

    /// The tree after the link of `cut` leaves and `entering`, running its direction seen from `enter`, enters:
    /// `cut` an ancestor of `enter` below the root, `attach` outside the subtree of `cut`. Along the stem, the path
    /// from `enter` up to `cut`, each node hangs from the node that was below it, by that node's link turned around;
    /// `enter` hangs from `attach` by `entering`. Every other node keeps its parent and its link.
    pub(super) fn exchanged(&self, enter: usize, attach: usize, cut: usize, entering: (usize, Direction)) -> Self {
        let mut tree = self.clone();
        let (mut below, mut hanging) = (attach, entering);
        for node in self.path_below(enter, self.parents[cut]) {
            tree.parents[node] = below;
            tree.hangings[node] = hanging;
            let (link, direction) = self.hangings[node];
            let turned = match direction {
                Direction::Up => Direction::Down,
                Direction::Down => Direction::Up,
            };
            hanging = (link, turned);
            below = node;
        }

        tree
    }

    /// The tree after a prune of a tree of `link_count` links that tells node `v` `decisions[v]`: a node told to hang
    /// from the root gets the root as its parent and its root link `link_count − n + v`, running the way it is told;
    /// every other node keeps its parent and its link.
    ///
    /// # Panics
    /// Panics if `decisions` holds another number of decisions than the tree has nodes below the root.
    pub(super) fn pruned(&self, decisions: &[Decision], link_count: usize) -> Self {
        let root = self.root();
        let given = decisions.len();
        assert!(given == root, "called `PlainTree::pruned` with {given} decisions for {root} nodes below the root");
        let mut tree = self.clone();

        for (node, &decision) in decisions.iter().enumerate() {
            if let Decision::Root(direction) = decision {
                tree.parents[node] = root;
                tree.hangings[node] = (link_count - root + node, direction);
            }
        }

        tree
    }

    /// Each node's potential where every link is tight under `costs`, one cost per link: the root at zero, a node
    /// whose link runs up at its parent's less the link's cost, one whose link runs down at its parent's plus it.
    pub(super) fn potentials(&self, costs: &[i64]) -> Vec<i64> {
        let mut potentials = vec![0; self.parents.len()];
        // The preorder puts each node after its parent, whose potential is then known.
        for node in self.subtree(self.root()).into_iter().skip(1) {
            let (link, direction) = self.hangings[node];
            let parent = potentials[self.parents[node]];
            potentials[node] = match direction {
                Direction::Up => parent - costs[link],
                Direction::Down => parent + costs[link],
            };
        }

        potentials
    }

    /// The columns of this tree threaded in its preorder of ascending children, with the potentials that make every
    /// link tight under `costs`.
    pub(super) fn columns(&self, costs: &[i64]) -> PlainColumns {
        let root = self.root();
        let order = self.subtree(root);
        let mut threads = vec![0; root + 1];
        let mut reverse_threads = vec![0; root + 1];
        for (at, &node) in order.iter().enumerate() {
            let next = order[(at + 1) % order.len()];
            threads[node] = next;
            reverse_threads[next] = node;
        }
        let last = |node: usize| *self.subtree(node).last().expect("a subtree should hold its top");

        PlainColumns {
            parents: self.parents.clone(),
            hangings: self.hangings.clone(),
            threads,
            reverse_threads,
            last_successors: (0..=root).map(last).collect(),
            subtree_sizes: (0..=root).map(|node| self.subtree(node).len()).collect(),
            potentials: self.potentials(costs),
        }
    }

    /// Checks `columns` against this tree: the same parents and links; a thread that runs from the root through every
    /// node once and back, in which each node's subtree is one contiguous run starting at it; the reverse thread its
    /// inverse; each node's last successor the end of its run and its subtree size the run's length; and, where
    /// `costs` are given, the potentials of [`potentials`](Self::potentials).
    ///
    /// # Errors
    /// Returns a message naming the first column that differs, and where.
    pub(super) fn check(&self, columns: &PlainColumns, costs: Option<&[i64]>) -> Result<(), String> {
        if columns.parents != self.parents {
            return Err(format!("the parents are {:?}, not {:?}", columns.parents, self.parents));
        }
        if columns.hangings != self.hangings {
            return Err(format!("the links are {:?}, not {:?}", columns.hangings, self.hangings));
        }

        let order = self.thread_order(columns)?;
        if let Some(node) = (0..order.len()).find(|&node| columns.reverse_threads[columns.threads[node]] != node) {
            return Err(format!("the reverse thread of {} is not {node}", columns.threads[node]));
        }

        for (at, &node) in order.iter().enumerate() {
            let mut members = self.subtree(node);
            let size = members.len();
            let mut run = order[at..at + size].to_vec();
            members.sort_unstable();
            run.sort_unstable();
            if run != members {
                return Err(format!("the run of {node} in the thread is not its subtree"));
            }
            if columns.subtree_sizes[node] != size {
                return Err(format!("{node} has subtree size {}, not {size}", columns.subtree_sizes[node]));
            }
            let last = order[at + size - 1];
            if columns.last_successors[node] != last {
                return Err(format!("{node} has last successor {}, not {last}", columns.last_successors[node]));
            }
        }

        match costs.map(|costs| self.potentials(costs)) {
            Some(potentials) if potentials != columns.potentials => {
                Err(format!("the potentials are {:?}, not {potentials:?}", columns.potentials))
            }
            _ => Ok(()),
        }
    }

    /// The nodes in the order the thread of `columns` visits them from the root.
    ///
    /// # Errors
    /// Returns a message where the thread meets a node twice or does not return to the root after every node.
    fn thread_order(&self, columns: &PlainColumns) -> Result<Vec<usize>, String> {
        let mut order = Vec::with_capacity(self.parents.len());
        let mut node = self.root();
        for _ in 0..self.parents.len() {
            if order.contains(&node) {
                return Err(format!("the thread meets {node} twice"));
            }
            order.push(node);
            node = columns.threads[node];
        }

        if node == self.root() { Ok(order) } else { Err(format!("the thread reaches {node} after every node")) }
    }
}

/// The tree of seven nodes under the root `7`: `0 <- 1 <- 2 <- 3` and `0 <- 4` under the root, and `5 <- 6` under the
/// root; node `v` hanging by link `v`, every link up but that of 2, which runs down. Its preorder runs 7, 0, 1, …, 6.
pub(super) fn seven_nodes() -> PlainTree {
    let mut hangings: Vec<_> = (0..7).map(|node| (node, Direction::Up)).collect();
    hangings[2].1 = Direction::Down;

    PlainTree { parents: vec![7, 0, 1, 2, 0, 7, 5, 7], hangings }
}

/// The links a tree of [`plain_trees`] has: more than its nodes hang by.
pub(super) fn link_count_of(model: &PlainTree) -> usize {
    2 * model.root() + 3
}

/// The most nodes below the root of a tree of [`plain_trees`].
const MOST_NODES: usize = 30;

/// The shape a tree of [`plain_trees`] is drawn in.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// Each node under a random earlier one or the root.
    Recursive,
    /// One path down from the root.
    Path,
    /// A path down from the root for half the nodes, the rest under random nodes of that path.
    Broom,
}

/// Random trees of up to [`MOST_NODES`] nodes below the root, of every [`Shape`], the nodes hung in a shuffled order,
/// node `v` by link `v + 3` running a random way.
pub(super) fn plain_trees() -> impl Strategy<Value = PlainTree> {
    let shapes = prop_oneof![Just(Shape::Recursive), Just(Shape::Path), Just(Shape::Broom)];
    (1..=MOST_NODES, shapes)
        .prop_flat_map(|(nodes, shape)| {
            let order = Just((0..nodes).collect::<Vec<_>>()).prop_shuffle();
            let directions = vec(prop_oneof![Just(Direction::Up), Just(Direction::Down)], nodes);
            (Just(shape), order, vec(any::<Index>(), nodes), directions)
        })
        .prop_map(|(shape, order, picks, directions)| {
            let root = order.len();
            let handle = root.div_ceil(2);
            let mut parents = vec![root; root + 1];
            for (at, (&node, pick)) in order.iter().zip(&picks).enumerate() {
                // Each node hangs from one hung before it, or from the root: a pick of `at` in `0..=at`.
                let earlier = match shape {
                    Shape::Recursive => pick.index(at + 1),
                    Shape::Path => at.wrapping_sub(1),
                    Shape::Broom if at < handle => at.wrapping_sub(1),
                    Shape::Broom => pick.index(handle),
                };
                parents[node] = order.get(earlier).filter(|_| earlier < at).copied().unwrap_or(root);
            }
            let hangings = directions.into_iter().enumerate().map(|(node, direction)| (node + 3, direction));

            PlainTree { parents, hangings: hangings.collect() }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nodes `0..5` under the root `5`: `3` and `2` under the root, `1` and `4` under `3`, `0` under `1`; node `v`
    /// hanging by link `v`, every link up.
    fn five_nodes() -> PlainTree {
        PlainTree { parents: vec![1, 3, 5, 5, 3, 5], hangings: (0..5).map(|node| (node, Direction::Up)).collect() }
    }

    /// The columns of `tree` threaded in its preorder of ascending children, written out by hand for five nodes.
    fn five_nodes_columns() -> PlainColumns {
        // The preorder: 5, 2, 3, 1, 0, 4.
        PlainColumns {
            parents: vec![1, 3, 5, 5, 3, 5],
            hangings: (0..5).map(|node| (node, Direction::Up)).collect(),
            threads: vec![4, 0, 3, 1, 5, 2],
            reverse_threads: vec![1, 3, 5, 2, 0, 4],
            last_successors: vec![0, 0, 2, 4, 4, 4],
            subtree_sizes: vec![1, 2, 1, 4, 1, 6],
            potentials: vec![0; 6],
        }
    }

    #[test]
    fn test_subtrees_ancestors_and_apex_by_hand() {
        let tree = five_nodes();

        assert_eq!(tree.subtree(5), [5, 2, 3, 1, 0, 4]);
        assert_eq!(tree.subtree(3), [3, 1, 0, 4]);
        assert_eq!(tree.children(3), [1, 4]);
        assert_eq!(tree.ancestors(0), [0, 1, 3, 5]);
        assert_eq!(tree.path_below(0, 3), [0, 1]);
        assert_eq!(tree.path_below(4, 4), Vec::<usize>::new());
        assert_eq!((tree.apex(0, 4), tree.apex(0, 2), tree.apex(1, 0), tree.apex(4, 4)), (3, 5, 1, 4));
    }

    #[test]
    #[should_panic(expected = "called `PlainTree::subtree` with 0, whose walk passes the tree's 6 nodes")]
    fn test_subtree_refuses_parents_that_run_around_a_cycle() {
        // 0 hangs from 1 and 1 from 0: the walk down from 0 meets the two of them in turn.
        let mut tree = five_nodes();
        tree.parents[1] = 0;

        let _ = tree.subtree(0);
    }

    #[test]
    fn test_the_star_hangs_every_node_from_the_root_by_its_root_link() {
        let star = PlainTree::star(&[Direction::Up, Direction::Down], 7);

        assert_eq!(star.parents, [2, 2, 2]);
        assert_eq!(star.hangings, [(7, Direction::Up), (8, Direction::Down)]);
    }

    #[test]
    fn test_an_exchange_reverses_the_stem_and_hangs_the_entering_end_by_hand() {
        // The link of 3 leaves and an arc from 0 to 2 enters, running up from 0: 0 under 2, 1 under 0, 3 under 1.
        let tree = five_nodes();
        let exchanged = tree.exchanged(0, 2, 3, (9, Direction::Up));

        assert_eq!(exchanged.parents, [2, 0, 5, 1, 3, 5]);
        assert_eq!(exchanged.hangings[0], (9, Direction::Up));
        assert_eq!(exchanged.hangings[1], (0, Direction::Down));
        assert_eq!(exchanged.hangings[3], (1, Direction::Down));
        assert_eq!((exchanged.hangings[2], exchanged.hangings[4]), ((2, Direction::Up), (4, Direction::Up)));

        // The leaving node is the entering end itself: only its parent and link change.
        let alone = tree.exchanged(4, 2, 4, (8, Direction::Down));
        assert_eq!(alone.parents, [1, 3, 5, 5, 2, 5]);
        assert_eq!(alone.hangings[4], (8, Direction::Down));
    }

    #[test]
    fn test_a_prune_hangs_the_nodes_told_from_the_root_by_their_root_links_by_hand() {
        // 0 under the root 3, 1 under 0, 2 under 1; six links, so the root links are 3, 4 and 5.
        let (up, down) = (Direction::Up, Direction::Down);
        let tree = PlainTree { parents: vec![3, 0, 1, 3], hangings: vec![(0, up), (1, up), (2, down)] };

        let pruned = tree.pruned(&[Decision::Stay, Decision::Root(down), Decision::Stay], 6);
        assert_eq!(pruned.parents, [3, 3, 1, 3], "2 goes with 1");
        assert_eq!(pruned.hangings, [(0, up), (4, down), (2, down)]);

        // A node under the root told to hang from it turns to its root link.
        let turned = tree.pruned(&[Decision::Root(down), Decision::Stay, Decision::Stay], 6);
        assert_eq!((turned.parents, turned.hangings[0]), (vec![3, 0, 1, 3], (3, down)));
        assert_eq!(tree.pruned(&[Decision::Stay; 3], 6), tree);
    }

    #[test]
    #[should_panic(expected = "called `PlainTree::pruned` with 1 decisions for 5 nodes below the root")]
    fn test_a_prune_refuses_too_few_decisions() {
        let _ = five_nodes().pruned(&[Decision::Stay], 10);
    }

    #[test]
    fn test_potentials_make_every_link_tight_by_hand() {
        // Costs per link 0 to 4: 2, -3, 5, 7, 1. Links up: a node sits at its parent's less the cost.
        let mut tree = five_nodes();
        tree.hangings[1].1 = Direction::Down;
        let costs = [2, -3, 5, 7, 1];

        // 3: 0 − 7 = −7; 1 (down): −7 + (−3) = −10; 0: −10 − 2 = −12; 4: −7 − 1 = −8; 2: −5.
        assert_eq!(tree.potentials(&costs), [-12, -10, -5, -7, -8, 0]);
    }

    #[test]
    fn test_check_accepts_the_columns_of_a_preorder_by_hand() {
        assert_eq!(five_nodes().check(&five_nodes_columns(), Some(&[0; 5])), Ok(()));

        // Another preorder, children of 3 in descending order: 5, 2, 3, 4, 1, 0.
        let other = PlainColumns {
            threads: vec![5, 0, 3, 4, 1, 2],
            reverse_threads: vec![1, 4, 5, 2, 3, 0],
            last_successors: vec![0, 0, 2, 0, 4, 0],
            ..five_nodes_columns()
        };
        assert_eq!(five_nodes().check(&other, None), Ok(()));
    }

    #[test]
    fn test_check_names_each_broken_column_by_hand() {
        let tree = five_nodes();
        let broken = |change: fn(&mut PlainColumns)| {
            let mut columns = five_nodes_columns();
            change(&mut columns);
            tree.check(&columns, Some(&[0; 5])).expect_err("a broken column should be refused")
        };

        let parents = "the parents are [3, 3, 5, 5, 3, 5], not [1, 3, 5, 5, 3, 5]";
        assert_eq!(broken(|columns| columns.parents[0] = 3), parents);
        assert!(broken(|columns| columns.hangings[2].1 = Direction::Down).starts_with("the links are"));
        assert_eq!(broken(|columns| columns.threads[1] = 1), "the thread meets 1 twice");
        assert_eq!(broken(|columns| columns.reverse_threads[0] = 4), "the reverse thread of 0 is not 1");
        assert_eq!(broken(|columns| columns.subtree_sizes[1] = 3), "1 has subtree size 3, not 2");
        assert_eq!(broken(|columns| columns.last_successors[3] = 0), "3 has last successor 0, not 4");
        assert!(broken(|columns| columns.potentials[2] = 1).starts_with("the potentials are"));
    }

    #[test]
    fn test_check_refuses_a_thread_that_is_no_preorder_by_hand() {
        // 5, 3, 2, 1, 0, 4 visits every node once, but 2 splits the run of 3.
        let columns = PlainColumns {
            threads: vec![4, 0, 1, 2, 5, 3],
            reverse_threads: vec![1, 2, 3, 5, 0, 4],
            ..five_nodes_columns()
        };

        assert_eq!(five_nodes().check(&columns, None), Err("the run of 3 in the thread is not its subtree".to_owned()));
    }

    #[test]
    fn test_check_refuses_a_thread_that_misses_the_root_by_hand() {
        // 5, 2, 3, 1, 0, 4 and on to 2 instead of the root.
        let mut columns = five_nodes_columns();
        columns.threads[4] = 2;

        assert_eq!(five_nodes().check(&columns, None), Err("the thread reaches 2 after every node".to_owned()));
    }
}
