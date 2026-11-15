// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The cycle an arc closes with the tree: the walk from its two ends up to their apex, whose steps can keep the node
//! each side would have leave; the cycle that walk becomes, with the hangings of its two sides; and the cycle with the
//! node whose link leaves, which is what an exchange is made of.

use super::spanning_tree::{
    ApexClimb, ClimbStep, CycleEnds, Direction, HungNode, LentTree, Link, Side, SideHangingsIter, SpanningTree,
    TreeNode,
};

/// The walk of the cycle an arc closes with the tree, from both ends up toward their apex, before the apex is known: a
/// lending walk whose every step is a node strictly below the apex on one of the two sides, and can keep itself as the
/// node whose link that side would have leave.
///
/// It holds the climb, and with it the tree's borrow, from its making to [`into_cycle`](Self::into_cycle), which turns
/// it into the cycle with the node one side kept: so the tree cannot change in between, and the only leaving node a
/// cycle can have is one the walk of its own side yielded. Within a side the steps come in the order of its path from
/// its end up; how the two sides interleave is the climb's to choose, and changes nothing either side yields.
///
/// # Invariant
/// Only [`new_unchecked`](Self::new_unchecked) makes one, from the climb of the tree it is handed. `first_kept`
/// holds, where it holds a node, one the climb yielded on the first side, and `second_kept` one it yielded on the
/// second.
#[derive(Debug)]
pub(crate) struct CycleWalk<'t> {
    /// The climb of the two ends toward their apex, with the tree it climbs.
    climb: ApexClimb<'t>,
    /// The node the first side kept last; `None` while it kept none.
    first_kept: Option<HungNode>,
    /// The node the second side kept last; `None` while it kept none.
    second_kept: Option<HungNode>,
}

/// One node of a cycle as its walk found it: strictly below the apex, on its side, with the link that joins it to its
/// parent and which way that link runs. Only [`CycleWalk::step`] makes one.
#[derive(Debug)]
pub(crate) struct SideStep<'w> {
    /// The node, its side and how it hangs.
    step: ClimbStep<'w>,
    /// The node its side kept last, of the walk that lent the step.
    kept: &'w mut Option<HungNode>,
}

/// The cycle an arc closes with the tree its ends hold: its two ends, their apex, and the node whose link leaves.
///
/// # Invariant
/// Only [`CycleWalk::into_cycle`] makes one, from a walk and the ends its climb found: `leaving`, where it holds a
/// node, holds one that walk yielded on the side it names, in the tree `ends` holds.
#[derive(Debug)]
pub(crate) struct Cycle<'t> {
    /// The two ends and their apex, with the tree, borrowed from the walk to the exchange.
    ends: CycleEnds<'t>,
    /// The node whose link leaves, and its side; `None` where no link leaves.
    leaving: Option<Leaving>,
}

/// The node whose link leaves a cycle, with the side of the cycle its walk found it on. Plain data.
#[derive(Clone, Copy, Debug)]
struct Leaving {
    /// The node, strictly below the apex.
    node: HungNode,
    /// Its side.
    side: Side,
}

/// A cycle whose walk kept the node whose link leaves: what an exchange is made of.
///
/// # Invariant
/// Only [`Cycle::into_kept`] makes one, from a cycle and the node its walk kept: so, in the tree `ends` holds, the kept
/// node lies on the path from its side's end up to the apex, strictly below the apex, and the other end lies outside
/// the kept node's subtree.
#[derive(Debug)]
pub(super) struct KeptCycle<'t> {
    /// The two ends and their apex, with the tree.
    ends: CycleEnds<'t>,
    /// The kept node and its side.
    leaving: Leaving,
}

impl<'t> CycleWalk<'t> {
    /// The walk of the cycle an arc from `first` to `second` closes with `tree`, nothing kept yet. Its making reads the
    /// two ends' subtree sizes; the whole walk costs at most the two ends' depths.
    ///
    /// # Safety
    /// `first` and `second` are below the node count of `tree`: the ends for which
    /// [`SpanningTree::apex_climb`] answers a climb, as its doc states.
    pub(crate) unsafe fn new_unchecked(tree: &'t mut LentTree<'_>, first: usize, second: usize) -> Self {
        debug_assert!(
            first < tree.node_count() && second < tree.node_count(),
            "called `CycleWalk::new_unchecked` with ends {first} and {second}, which should both be below the node \
             count {}",
            tree.node_count()
        );
        // SAFETY: both ends are below the node count of `tree`, as the caller promises (`CycleWalk::new_unchecked`).
        let climb = unsafe { tree.tree_mut().apex_climb_unchecked(first, second) };

        Self { climb, first_kept: None, second_kept: None }
    }

    /// The next node strictly below the apex, on either side; `None` once the two sides have met at the apex, and
    /// ever after.
    pub(crate) fn step(&mut self) -> Option<SideStep<'_>> {
        let Self { climb, first_kept, second_kept } = self;
        let step = climb.step()?;
        let kept = match step.side {
            Side::First => first_kept,
            Side::Second => second_kept,
        };

        Some(SideStep { step, kept })
    }

    /// Whether the walk of `side` has kept a node so far.
    pub(crate) fn kept(&self, side: Side) -> bool {
        match side {
            Side::First => self.first_kept.is_some(),
            Side::Second => self.second_kept.is_some(),
        }
    }

    /// The cycle, the walk finished first where it was not, whose link leaves at the node `leaving`'s side kept last;
    /// none where `leaving` is `None` or that side kept no node.
    pub(crate) fn into_cycle(self, leaving: Option<Side>) -> Cycle<'t> {
        let Self { climb, first_kept, second_kept } = self;
        let leaving = leaving.and_then(|side| {
            let kept = match side {
                Side::First => first_kept,
                Side::Second => second_kept,
            };

            kept.map(|node| Leaving { node, side })
        });

        Cycle { ends: climb.into_ends(), leaving }
    }
}

impl<'w> SideStep<'w> {
    /// The link that joins the node to its parent, for as long as this step's borrow of its walk lasts.
    pub(crate) fn link(&self) -> Link<'w> {
        self.step.hanging.link
    }

    /// Which way the link that joins the node to its parent runs, seen from the node.
    pub(crate) fn direction(&self) -> Direction {
        self.step.hanging.direction
    }

    /// The side of the cycle the node lies on.
    pub(crate) fn side(&self) -> Side {
        self.step.side
    }

    /// Makes this node the one whose link its side would have leave, in place of any its side kept before.
    pub(crate) fn keep(self) {
        *self.kept = Some(self.step.node);
    }
}

impl<'t> Cycle<'t> {
    /// How each node of the first side hangs, from the first end up to the apex.
    pub(crate) fn first_side(&self) -> SideHangingsIter<'_> {
        self.ends.side(Side::First)
    }

    /// How each node of the second side hangs, from the second end up to the apex.
    pub(crate) fn second_side(&self) -> SideHangingsIter<'_> {
        self.ends.side(Side::Second)
    }

    /// The cycle with the node whose link leaves; `None` where no node was kept.
    pub(super) fn into_kept(self) -> Option<KeptCycle<'t>> {
        let Self { ends, leaving } = self;

        leaving.map(|leaving| KeptCycle { ends, leaving })
    }
}

impl<'t> KeptCycle<'t> {
    /// The side of the cycle the kept node lies on.
    pub(super) fn side(&self) -> Side {
        self.leaving.side
    }

    /// The kept node: the old top of the subtree that moves.
    pub(super) fn cut(&self) -> HungNode {
        self.leaving.node
    }

    /// The end of the kept node's side: the new top of the subtree that moves.
    pub(super) fn enter(&self) -> HungNode {
        self.ends.end(self.leaving.side)
    }

    /// The other end: the new parent of [`enter`](Self::enter).
    pub(super) fn attach(&self) -> TreeNode {
        let other = match self.leaving.side {
            Side::First => Side::Second,
            Side::Second => Side::First,
        };

        self.ends.end(other).node()
    }

    /// The apex of the cycle.
    pub(super) fn join(&self) -> TreeNode {
        self.ends.apex()
    }

    /// The tree, for the exchange to move.
    pub(super) fn into_tree(self) -> SpanningTree<'t> {
        self.ends.into_tree()
    }
}

#[cfg(test)]
pub(super) mod tests {
    use proptest::prelude::*;
    use proptest::sample::Index;

    use super::*;
    use crate::tree::plain_tree::{PlainTree, link_count_of, plain_trees, seven_nodes};
    use crate::tree::spanning_tree::TreeRoom;
    use crate::tree::spanning_tree::tests::{PlainStep, columns_of, plain_tree_of, step, tree_of};

    /// The link a node of a side hangs by and that link's direction, as plain numbers.
    type PlainHangingLink = (usize, Direction);

    /// The walk of the cycle of `first` and `second` in `tree`: the one place the tests make a walk from raw ends.
    ///
    /// # Panics
    /// Panics if `first` or `second` is not below the node count of `tree`.
    pub(crate) fn cycle_walk<'t>(tree: &'t mut LentTree<'_>, first: usize, second: usize) -> CycleWalk<'t> {
        let node_count = tree.node_count();
        assert!(first < node_count && second < node_count, "a test's ends should be below the node count");

        debug_assert!(first < node_count && second < node_count, "a test's ends should be below the node count");
        // SAFETY: both ends are below the node count of `tree`, as asserted just above.
        unsafe { CycleWalk::new_unchecked(tree, first, second) }
    }

    const FIRST: Side = Side::First;
    const SECOND: Side = Side::Second;
    const UP: Direction = Direction::Up;
    const DOWN: Direction = Direction::Down;

    /// Takes every step of `walk`, in a tree of `hung_nodes` nodes below the root, keeping nothing, and answers them as
    /// plain numbers.
    ///
    /// # Panics
    /// Panics if the walk takes more steps than the tree has nodes below the root.
    fn walked(walk: &mut CycleWalk<'_>, hung_nodes: usize) -> Vec<PlainStep> {
        let mut steps = Vec::new();
        while let Some(at) = walk.step() {
            assert!(steps.len() < hung_nodes, "a walk should take at most one step a node below the root");
            steps.push(step(at.side(), usize::from(at.step.node), usize::from(at.link()), at.direction()));
        }

        steps
    }

    /// The hangings `side` yields, as plain numbers.
    fn hanging_links(side: SideHangingsIter<'_>) -> Vec<PlainHangingLink> {
        side.map(|hanging| (usize::from(hanging.link), hanging.direction)).collect()
    }

    /// The steps of `side`, from `end` up to `apex` in `model`, from its parents and links alone.
    fn side_by_rule(model: &PlainTree, side: Side, end: usize, apex: usize) -> Vec<PlainStep> {
        let path = model.path_below(end, apex);

        path.into_iter().map(|node| step(side, node, model.hangings[node].0, model.hangings[node].1)).collect()
    }

    /// The steps of `side` among `steps`, in order.
    fn of_side(steps: &[PlainStep], side: Side) -> Vec<PlainStep> {
        steps.iter().filter(|at| at.side == side).copied().collect()
    }

    /// The steps of the walk of the cycle of `first` and `second` in `tree`, and the apex the cycle it becomes holds,
    /// as plain numbers.
    fn walk_of(tree: &mut LentTree<'_>, first: usize, second: usize) -> (Vec<PlainStep>, usize) {
        let hung_nodes = tree.node_count();
        let mut walk = cycle_walk(tree, first, second);
        let steps = walked(&mut walk, hung_nodes);

        (steps, usize::from(walk.into_cycle(None).ends.apex()))
    }

    /// The cycle of `first` and `second` in `tree` with `side` named, its walk having kept every node of `side` that
    /// `keeps` names.
    fn kept_of<'t>(tree: &'t mut LentTree<'_>, [first, second]: [usize; 2], side: Side, keeps: &[usize]) -> Cycle<'t> {
        let mut walk = cycle_walk(tree, first, second);
        while let Some(step) = walk.step() {
            if step.side() == side && keeps.contains(&usize::from(step.step.node)) {
                step.keep();
            }
        }

        walk.into_cycle(Some(side))
    }

    #[test]
    fn test_the_walk_yields_each_side_from_its_end_up_by_hand() {
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());

        // Two leaves under the apex 0: 3 and 4 have one node each, so the second end moves first, to 0, of five
        // nodes; then the first climbs 3, 2, 1 to 0.
        let steps = [step(SECOND, 4, 4, UP), step(FIRST, 3, 3, UP), step(FIRST, 2, 2, DOWN), step(FIRST, 1, 1, UP)];
        assert_eq!(walk_of(&mut tree, 3, 4), (steps.to_vec(), 0));

        // Leaves under different children of the root: the climbs alternate by size, and 5 and 2, of two nodes each,
        // let the second move.
        let steps = [
            step(FIRST, 6, 6, UP),
            step(SECOND, 2, 2, DOWN),
            step(FIRST, 5, 5, UP),
            step(SECOND, 1, 1, UP),
            step(SECOND, 0, 0, UP),
        ];
        assert_eq!(walk_of(&mut tree, 6, 2), (steps.to_vec(), 7));

        // An end next to the root: 5 hangs from it, so its side is that one node.
        let steps = [step(SECOND, 4, 4, UP), step(FIRST, 5, 5, UP), step(SECOND, 0, 0, UP)];
        assert_eq!(walk_of(&mut tree, 5, 4), (steps.to_vec(), 7));
    }

    #[test]
    fn test_an_end_above_the_other_is_the_apex_and_its_side_is_empty_by_hand() {
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());

        assert_eq!(walk_of(&mut tree, 1, 3), (vec![step(SECOND, 3, 3, UP), step(SECOND, 2, 2, DOWN)], 1));
        assert_eq!(walk_of(&mut tree, 3, 1), (vec![step(FIRST, 3, 3, UP), step(FIRST, 2, 2, DOWN)], 1));
    }

    #[test]
    fn test_a_loop_at_one_node_has_that_node_as_its_apex_and_no_step() {
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());
        let mut walk = cycle_walk(&mut tree, 2, 2);

        assert!(walk.step().is_none());
        let cycle = walk.into_cycle(Some(SECOND));
        assert_eq!(usize::from(cycle.ends.apex()), 2);
        assert!(cycle.first_side().next().is_none() && cycle.second_side().next().is_none());
        assert!(cycle.into_kept().is_none());
    }

    #[test]
    fn test_ends_whose_subtrees_are_of_one_size_meet_at_their_apex_by_hand() {
        // 5 and 0 hang from the root 6 with three nodes each; 1 and 2 are leaves under 0, 3 and 4 under 5.
        let model = PlainTree {
            parents: vec![6, 0, 0, 5, 5, 6, 6],
            hangings: (0..6).map(|node| (node, Direction::Up)).collect(),
        };
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &model);

        // Of two of one size the second moves; each side still yields its own path.
        assert_eq!(walk_of(&mut tree, 0, 5), (vec![step(SECOND, 5, 5, UP), step(FIRST, 0, 0, UP)], 6));
        assert_eq!(walk_of(&mut tree, 1, 2), (vec![step(SECOND, 2, 2, UP), step(FIRST, 1, 1, UP)], 0), "siblings");
        let steps = [step(SECOND, 1, 1, UP), step(FIRST, 3, 3, UP), step(SECOND, 0, 0, UP), step(FIRST, 5, 5, UP)];
        assert_eq!(walk_of(&mut tree, 3, 1), (steps.to_vec(), 6), "leaves of one size under different parents");
        assert_eq!(walk_of(&mut tree, 5, 3), (vec![step(SECOND, 3, 3, UP)], 5));
    }

    #[test]
    fn test_a_walk_ends_at_the_apex_and_stays_ended() {
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());
        let mut walk = cycle_walk(&mut tree, 4, 0);

        assert_eq!(walk.step().map(|step| usize::from(step.link())), Some(4));
        assert!(walk.step().is_none() && walk.step().is_none());
    }

    #[test]
    fn test_the_cycle_lends_the_hangings_of_both_sides_by_hand() {
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());
        let cycle = cycle_walk(&mut tree, 3, 6).into_cycle(None);

        assert_eq!(hanging_links(cycle.first_side()), [(3, UP), (2, DOWN), (1, UP), (0, UP)]);
        assert_eq!(hanging_links(cycle.second_side()), [(6, UP), (5, UP)]);
    }

    #[test]
    fn test_nothing_kept_is_no_kept_cycle() {
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());

        for leaving in [None, Some(FIRST), Some(SECOND)] {
            let mut walk = cycle_walk(&mut tree, 3, 6);
            walked(&mut walk, 7);
            assert!(!walk.kept(FIRST) && !walk.kept(SECOND), "{leaving:?}");
            assert!(walk.into_cycle(leaving).into_kept().is_none(), "{leaving:?}");
        }
    }

    #[test]
    fn test_each_side_keeps_its_last_node_and_the_side_named_leaves_by_hand() {
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());

        // The walk of 3 and 6 yields 6, 3, 5, 2, 1, 0; every node but 0 keeps itself.
        for (leaving, expected) in [(None, None), (Some(FIRST), Some((1, FIRST))), (Some(SECOND), Some((5, SECOND)))] {
            let mut walk = cycle_walk(&mut tree, 3, 6);
            while let Some(step) = walk.step() {
                if usize::from(step.step.node) != 0 {
                    step.keep();
                }
            }
            assert!(walk.kept(FIRST) && walk.kept(SECOND));
            let cycle = walk.into_cycle(leaving);
            let kept = cycle.leaving.map(|leaving| (usize::from(leaving.node), leaving.side));
            assert_eq!(kept, expected, "the last node kept on the side named, {leaving:?}");
        }

        // Only the second side keeps a node: the first side named leaves nothing.
        let mut walk = cycle_walk(&mut tree, 3, 6);
        walk.step().expect("6 lies below the apex").keep();
        assert!(!walk.kept(FIRST) && walk.kept(SECOND));
        assert!(walk.into_cycle(Some(FIRST)).into_kept().is_none());
    }

    #[test]
    fn test_a_walk_left_unfinished_finishes_into_its_cycle_by_hand() {
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());
        let mut walk = cycle_walk(&mut tree, 6, 2);
        walk.step().expect("6 lies below the apex").keep();

        let cycle = walk.into_cycle(Some(FIRST));
        assert_eq!(usize::from(cycle.ends.apex()), 7);
        assert_eq!(cycle.leaving.map(|leaving| usize::from(leaving.node)), Some(6));
    }

    #[test]
    fn test_a_kept_cycle_hangs_its_sides_end_from_the_other_end_by_hand() {
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());
        let plain = |kept: &KeptCycle<'_>| {
            [kept.enter().node(), kept.attach(), kept.cut().node(), kept.join()].map(usize::from)
        };

        for (side, cut, expected) in [(FIRST, 2, [3, 6, 2, 7]), (SECOND, 5, [6, 3, 5, 7])] {
            let kept = kept_of(&mut tree, [3, 6], side, &[cut]).into_kept().expect("the walk should keep its node");
            assert_eq!((plain(&kept), kept.side()), (expected, side));
            assert_eq!(columns_of(&kept.into_tree()).parents.len(), 8);
        }
    }

    proptest! {
        #[test]
        fn test_the_walk_its_apex_and_the_sides_agree_with_walks_to_the_root(
            model in plain_trees(),
            ends in (any::<Index>(), any::<Index>()),
        ) {
            let (first, second) = (ends.0.index(model.root()), ends.1.index(model.root()));
            let mut room = TreeRoom::new();
            let mut tree = tree_of(&mut room, &model, link_count_of(&model), &vec![0; link_count_of(&model)]);
            prop_assert_eq!(model.check(&columns_of(&tree), None), Ok(()));

            let apex = model.apex(first, second);
            let by_first = side_by_rule(&model, FIRST, first, apex);
            let by_second = side_by_rule(&model, SECOND, second, apex);
            let mut walk = cycle_walk(&mut tree, first, second);
            let steps = walked(&mut walk, model.root());
            prop_assert_eq!(of_side(&steps, FIRST), by_first.clone());
            prop_assert_eq!(of_side(&steps, SECOND), by_second.clone());

            let cycle = walk.into_cycle(None);
            prop_assert_eq!(usize::from(cycle.ends.apex()), apex);
            let plain = |path: &[PlainStep]| -> Vec<PlainHangingLink> {
                path.iter().map(|at| (at.link, at.direction)).collect()
            };
            prop_assert_eq!(hanging_links(cycle.first_side()), plain(&by_first));
            prop_assert_eq!(hanging_links(cycle.second_side()), plain(&by_second));
        }
    }
}
