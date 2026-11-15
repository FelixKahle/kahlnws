// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The threaded spanning tree with each node's potential: the room it is kept in, with whether it holds a whole tree
//! between runs, the loaded tree with its invariant, its proven indices, the climb of a cycle's two ends to their
//! apex with the tree it climbs, the ends it finds and the walk of a side, the rules of a root link's cost and of a
//! tight potential, and the view through which the tree's walks read and write its columns.

use core::iter;
use core::iter::FusedIterator;
use core::marker::PhantomData;
use core::mem;
use core::ops::Deref;

use std::collections::TryReserveError;

use crate::ranges::{INDEX_LIMIT, narrow, widen};
use crate::room::{refill, reserve_total};

/// A node of a loaded spanning tree, below its `n + 1` tree nodes: the index every per-tree-node column of that tree
/// is read and written at without a check.
///
/// What the type proves: the index was made by this file for a tree of `n` nodes, and lies below `n + 1`. This file
/// makes one only as a node of the star [`TreeRoom::star`] writes, the root among them; as the root of a tree,
/// [`SpanningTree::root`]; as an end of a climb, which
/// [`SpanningTree::apex_climb`] checked against the node count or [`SpanningTree::apex_climb_unchecked`] was promised
/// below it; or as a value read back from the tree's parent, thread, reverse-thread or last-successor column, which
/// hold nothing else.
///
/// What it does not prove, and remains the `tree` folder's own discipline: that a node is used only with the tree it
/// was made for. Nodes never leave the folder, and inside it they are held only in values that hold that tree's borrow
/// beside them: the climb of a cycle and its steps, the ends of a cycle and the walk of a side, the walk of a cycle, a
/// cycle, a kept cycle, an exchange, a prune or a tighten and their steps. Outside this file, the only functions that
/// take a node beside a tree are the reads and writes of [`TreeColumnsMut`] and [`SpanningTree::hanging`]; their
/// callers, the exchange and the two walks, hand them their own tree with the nodes they hold with it, or read back
/// from it.
///
/// Stored at 32 bits, as [`SpanningTree`] says why.
#[derive(Clone, Copy, PartialEq, Eq, derive_more::Debug)]
#[debug("TreeNode {_0}")]
pub(super) struct TreeNode(u32);

/// A node of a loaded spanning tree that hangs from a parent by a link, below its `n` hung nodes, so not the root: the
/// index the tree's link and direction columns are read and written at without a check.
///
/// What the type proves: the index was made by this file for a tree of `n` nodes, and lies below `n`. This file makes
/// one only as an end of a climb, checked or promised as [`TreeNode`] says; as a node the climb of a cycle
/// ([`ApexClimb`]) or the walk of a side ([`SideHangingsIter`]) leaves strictly below the apex; or where
/// [`TreeColumnsMut::hung`] finds a tree node below `n`, or [`TreeColumnsMut::hung_unchecked`] was promised one. It
/// is held under the discipline [`TreeNode`] states.
#[derive(Clone, Copy, PartialEq, Eq, derive_more::Debug)]
#[debug("HungNode {}", _0.0)]
pub(super) struct HungNode(TreeNode);

/// A link of the basis, below the link count `m + n` of the tree it was read from: an arc's slot `s` at `s`, node
/// `v`'s root link at `m + v`.
///
/// What its lifetime proves: the link lies below the link count of the tree it was read from, for as long as that
/// tree is borrowed; so no link outlives that borrow, and none survives a later [`TreeRoom::star`] or
/// [`TreeRoom::resume`]. What it does not prove: the lifetime is covariant, so it does not tell one tree's links from
/// another's, and a link of one tree can be handed to another borrowed as long. Which columns a link may index is its
/// user's invariant. Only this file makes one: a link the tree holds, read back; a hung node's root link
/// ([`SpanningTree::root_link`]); or a raw link [`SpanningTree::link`] checked against the link count or
/// [`SpanningTree::link_unchecked`] was promised below it. It converts to a `usize`; nothing converts to it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, derive_more::Debug)]
#[debug("Link {_0}")]
pub(crate) struct Link<'t>(u32, PhantomData<&'t ()>);

/// Which way the link that joins a node to its parent runs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Direction {
    /// The node is the link's tail: the link runs up, toward the parent.
    Up,
    /// The node is the link's head: the link runs down, from the parent.
    Down,
}

/// One of the two sides of a cycle below its apex.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Side {
    /// The path from the end flow leaves the entering arc from up to the apex.
    First,
    /// The path from the end flow enters the entering arc's far side at up to the apex.
    Second,
}

/// The room a spanning tree is kept in from one solve to the next: its columns, holding whatever the last tree left,
/// and whether that is a whole tree.
///
/// It only grows. Nothing is read from it but through the [`LentTree`] that [`star`](Self::star) writes into it and
/// lends, or that [`resume`](Self::resume) lends again from the columns a kept tree left, and through
/// [`kept`](Self::kept). Its column fields are the tree's columns, each described here once; the tree and the view
/// that lend them refer to these descriptions.
///
/// # Invariant
/// `kept` guards the columns: where it is [`Kept::Tree`], they are those of a tree whose invariant held when its
/// lending ended ([`LentTree::keep`]), of the `n` hung nodes the directions hold and the link count it names, and
/// nothing has written them since. `star` and `resume` take it out before the tree can be written, and only
/// `LentTree::keep`, of the one tree the room lent, puts it in.
#[derive(Clone, Debug, Default)]
pub(crate) struct TreeRoom {
    /// Per tree node: its parent; the root's is the root.
    parent: Vec<TreeNode>,
    /// Per hung node: the link it hangs by.
    parent_link: Vec<u32>,
    /// Per hung node: which way that link runs, seen from the node.
    direction: Vec<Direction>,
    /// Per tree node: the node after it in the preorder; the last node's is the root.
    thread: Vec<TreeNode>,
    /// Per tree node: the node before it in the preorder; the root's is the last node.
    reverse_thread: Vec<TreeNode>,
    /// Per tree node: the last node of its subtree in the preorder.
    last_successor: Vec<TreeNode>,
    /// Per tree node: the number of nodes of its subtree, itself included.
    subtree_size: Vec<u32>,
    /// Per tree node: its potential, in units of cost; the root's is zero.
    potential: Vec<i64>,
    /// Whether the columns hold a whole tree, kept when its run ended.
    kept: Kept,
}

/// What a tree room holds between runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Kept {
    /// No whole tree: before the first tree is kept, and from the lending of a tree until it is kept.
    #[default]
    Nothing,
    /// The tree a run ended on, of `link_count` links, `m + n`.
    Tree {
        /// The number of links, at most `u32::MAX`.
        link_count: usize,
    },
}

/// The counts of the tree a room keeps, which [`TreeRoom::resume`] lends again. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeptCounts {
    /// The number of hung nodes, `n`.
    pub(crate) node_count: usize,
    /// The number of links, `m + n`.
    pub(crate) link_count: usize,
}

/// A rooted spanning tree on the tree nodes `0..=n`, the root `n` last, with its preorder threading and each node's
/// potential, loaded into the room it borrows for `'r`.
///
/// Each node below the root hangs from its parent by a link, an arc's or its own root link, running up or down. The
/// parent links, each node's link and its direction are the tree; the thread, its reverse, the last successors and the
/// subtree sizes are derived from them and stored, so that a cycle's apex is found and a subtree moved at the cost of
/// the paths they touch, never a pass over the tree. What a link carries and costs is not the tree's.
///
/// Every index is stored at 32 bits, not at the native width: the walks up the tree read one stored index a node, each
/// a dependent load, so their speed is that of the cache the columns fit in. The measurement behind it is on
/// [`Slot`](crate::slot_order::Slot).
///
/// # Invariant
/// The columns describe one tree of `n` hung nodes and `link_count` links:
/// - `parent`, `thread`, `reverse_thread`, `last_successor`, `subtree_size` and `potential` hold `n + 1` entries,
///   one per tree node; `parent_link` and `direction` hold `n`, one per hung node;
/// - every stored [`TreeNode`] lies below `n + 1` and every stored link below `link_count`; `n + 1` and `link_count`
///   are at most `u32::MAX`;
/// - `parent` leads from every node but the root to the root without a cycle; the root's parent is itself;
/// - each hung node's `parent_link` is the link it hangs by, below `link_count`, and `direction` the way it runs;
/// - `thread` visits every node once, from the root and back to it, in an order in which each node's subtree is one
///   contiguous run that starts at the node; `reverse_thread` is its inverse;
/// - `last_successor[v]` is the last node of `v`'s run and `subtree_size[v]` its length.
///
/// [`TreeRoom::star`] establishes it, and [`TreeRoom::resume`] lends a tree its room's marker says it held when it was
/// kept; an exchange keeps it; a prune keeps it once it is finished, and a tighten, once finished, makes every link
/// tight under the costs it was told: a walk owns the [`LentTree`] while it writes, and hands it back only finished.
/// Every
/// unchecked access rests on it as a whole: the reads and writes at a [`TreeNode`] or a [`HungNode`] on the first two
/// items; the climb's making of a hung node strictly below the apex on the subtree sizes (a proper ancestor's is
/// larger, the root's the largest); the stem walk's and the walks' making of a hung node on the parents and the thread,
/// which lead to the root without a cycle and visit it once.
#[derive(Debug)]
pub(crate) struct SpanningTree<'r> {
    /// The room's columns, lent for `'r`.
    columns: TreeColumnsMut<'r>,
    /// The number of links, `m + n`, which every stored link is below.
    link_count: usize,
}

/// The tree a room lends, with the room's marker: the one value that can end the lending by keeping the tree whole in
/// its room.
///
/// It reads as the [`SpanningTree`] it holds. A climb, a cycle, an exchange and a walk's step hold a [`SpanningTree`],
/// which has no marker and cannot be kept.
///
/// What the type proves: only [`TreeRoom::star`] and [`TreeRoom::resume`] make one, from the columns of the room whose
/// marker `kept` is, the marker taken out when it was made; no second lending of that room exists while it is out,
/// since each takes the room by `&mut`. Outside this folder its tree is only read, through `Deref`, which lends no
/// `&mut`, and only moved by the folder's own walks: a cycle walk borrows the lent tree, and a prune or a tighten owns
/// it and hands it back only finished, so a walk dropped half way drops the tree with the marker still out.
///
/// What remains discipline. Inside this folder: no tree is put in another's place through
/// [`tree_mut`](Self::tree_mut), by a swap or a replace of two lent trees, and no column is written through it outside
/// an exchange or a walk before the tree is kept. In the basis's file: the lent trees of two bases are not swapped,
/// which would pair one room's link columns with another room's tree, and a basis whose pivot unwound inside the crate
/// is not kept afterwards.
///
/// # Invariant
/// `tree` is lent from the columns of the room whose marker `kept` is, and the marker was taken out when it was made.
#[derive(Debug)]
pub(crate) struct LentTree<'r> {
    /// The tree lent.
    tree: SpanningTree<'r>,
    /// The room's marker, which [`keep`](Self::keep) sets.
    kept: &'r mut Kept,
}

/// The columns of a room, lent for one tree or one walk that reads or writes them: each field is the room's column of
/// the same name, as [`TreeRoom`] describes it.
///
/// # Invariant
/// Only this file makes one: as the columns a star has just written ([`TreeRoom::star`]), as the columns of a kept tree
/// lent again ([`TreeRoom::resume`]), or by reborrowing another.
/// The per-tree-node slices hold `n + 1` entries and the per-hung-node slices `n`, `n` being the length of
/// `direction`, and every stored [`TreeNode`] lies below `n + 1`. Every write takes a [`TreeNode`] or a link, so every
/// value stored is one this file made. That the nodes and links it is handed were made for, or read from, this same
/// tree is the folder's discipline [`TreeNode`] states.
#[derive(Debug)]
pub(super) struct TreeColumnsMut<'t> {
    parent: &'t mut [TreeNode],
    parent_link: &'t mut [u32],
    direction: &'t mut [Direction],
    thread: &'t mut [TreeNode],
    reverse_thread: &'t mut [TreeNode],
    last_successor: &'t mut [TreeNode],
    subtree_size: &'t mut [u32],
    potential: &'t mut [i64],
}

/// The climb of the two ends of a cycle toward their apex in the loaded tree it holds, one node a step: the end whose
/// subtree is the smaller, the second of two equal ones, moves to its parent.
///
/// It holds the tree's borrow with the climb, so the nodes it climbs are read only in the tree they were made for, and
/// the tree cannot change until the climb has become the ends it finds.
///
/// # Invariant
/// Only [`SpanningTree::apex_climb`] and [`SpanningTree::apex_climb_unchecked`] make one, from two ends below the node
/// count of the tree it holds. `first` and `second` are hung nodes of `tree`; `at_first` lies on the path from `first`
/// up to their deepest common ancestor, the apex, and `at_second` on the path from `second`; `first_size` and
/// `second_size` are their subtree sizes. The two climbing nodes are equal exactly at the apex. It is neither `Clone`
/// nor `Copy`, so it is climbed once.
#[derive(Debug)]
pub(super) struct ApexClimb<'t> {
    /// The tree climbed, borrowed from the climb's making to its end.
    tree: SpanningTree<'t>,
    /// The end flow leaves the entering arc from.
    first: HungNode,
    /// The end flow enters the entering arc's far side at.
    second: HungNode,
    /// How far the first side has climbed: the next node of its path, or the apex.
    at_first: TreeNode,
    /// The number of nodes of the subtree of `at_first`.
    first_size: u32,
    /// How far the second side has climbed: the next node of its path, or the apex.
    at_second: TreeNode,
    /// The number of nodes of the subtree of `at_second`.
    second_size: u32,
    /// The steps climbed so far, which debug builds bound by the tree's hung nodes.
    #[cfg(debug_assertions)]
    steps: usize,
}

/// One step of an [`ApexClimb`]: the node the climb left, strictly below the apex, the side of the cycle it lies on,
/// and how it hangs, read for the ratio test. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ClimbStep<'w> {
    /// The node.
    pub(super) node: HungNode,
    /// The side the node lies on.
    pub(super) side: Side,
    /// The link that joins the node to its parent, and which way it runs.
    pub(super) hanging: Hanging<'w>,
}

/// The two ends of a cycle in the loaded tree it holds, with their apex.
///
/// # Invariant
/// Only [`ApexClimb::into_ends`] makes one, from the climb of the tree it holds: `first` and `second` are hung nodes
/// of `tree`, and `apex` is their deepest common ancestor in it. It is neither `Clone` nor `Copy`, so it is held by one
/// cycle only.
#[derive(Debug)]
pub(super) struct CycleEnds<'t> {
    /// The tree the ends were found in, borrowed until the cycle is dropped or exchanged.
    tree: SpanningTree<'t>,
    /// The end flow leaves the entering arc from.
    first: HungNode,
    /// The end flow enters the entering arc's far side at.
    second: HungNode,
    /// The deepest common ancestor of the two ends.
    apex: TreeNode,
}

/// How each hung node of one side of a cycle hangs, from the side's end up to the apex, the apex excluded, nearest the
/// end first: the link that joins it to its parent and which way that link runs.
///
/// # Invariant
/// Only [`CycleEnds::side`] makes one, from the ends of a cycle and the columns of the tree they hold: `next` lies on
/// the path from the side's end up to `apex`, which is an ancestor of every node of that path.
#[derive(Clone, Debug)]
pub(crate) struct SideHangingsIter<'c> {
    /// The tree's parents.
    parents: Parents<'c>,
    /// The tree's links and directions.
    hangings: Hangings<'c>,
    /// The next node of the path up: the apex once the side is walked.
    next: TreeNode,
    /// The apex, where the walk ends.
    apex: TreeNode,
}

/// The link a hung node hangs by and which way it runs, seen from the node. Plain data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Hanging<'c> {
    /// The link.
    pub(crate) link: Link<'c>,
    /// Which way it runs, seen from the node.
    pub(crate) direction: Direction,
}

/// The parent column of one loaded tree, lent for one walk.
///
/// # Invariant
/// Only this file makes one, from the parent column of one [`SpanningTree`], which holds `n + 1` entries, every one a
/// [`TreeNode`] of that tree. That the nodes it is read at were made for this same tree is the folder's discipline
/// [`TreeNode`] states.
#[derive(Clone, Copy, Debug)]
struct Parents<'c>(&'c [TreeNode]);

/// The subtree sizes of one loaded tree, lent for one walk that reads them.
///
/// # Invariant
/// Only this file makes one, from the subtree-size column of one [`SpanningTree`], which holds `n + 1` entries. That
/// the nodes it is read at were made for this same tree is the folder's discipline [`TreeNode`] states.
#[derive(Clone, Copy, Debug)]
struct Sizes<'c>(&'c [u32]);

/// The link and direction columns of one loaded tree, lent for one walk that reads them.
///
/// # Invariant
/// Only this file makes one, from the link and direction columns of one [`SpanningTree`], which hold `n` entries each,
/// every link below the tree's link count. That the nodes it is read at were made for this same tree is the folder's
/// discipline [`TreeNode`] states.
#[derive(Clone, Copy, Debug)]
struct Hangings<'c> {
    /// Per hung node: the link it hangs by.
    links: &'c [u32],
    /// Per hung node: which way that link runs.
    directions: &'c [Direction],
}

impl TreeRoom {
    /// A room that holds nothing yet; it allocates nothing.
    pub(crate) const fn new() -> Self {
        Self {
            parent: Vec::new(),
            parent_link: Vec::new(),
            direction: Vec::new(),
            thread: Vec::new(),
            reverse_thread: Vec::new(),
            last_successor: Vec::new(),
            subtree_size: Vec::new(),
            potential: Vec::new(),
            kept: Kept::Nothing,
        }
    }

    /// Makes room for the tree of a problem of up to `node_count` nodes: a star of such a problem, and every exchange
    /// in it, allocates nothing afterwards. Room already held is kept.
    ///
    /// # Errors
    /// Returns the `TryReserveError` of the first refused reservation.
    pub(crate) fn try_reserve(&mut self, node_count: usize) -> Result<(), TryReserveError> {
        let tree_nodes = node_count.saturating_add(1);

        reserve_total(&mut self.parent, tree_nodes)?;
        reserve_total(&mut self.parent_link, node_count)?;
        reserve_total(&mut self.direction, node_count)?;
        reserve_total(&mut self.thread, tree_nodes)?;
        reserve_total(&mut self.reverse_thread, tree_nodes)?;
        reserve_total(&mut self.last_successor, tree_nodes)?;
        reserve_total(&mut self.subtree_size, tree_nodes)?;
        reserve_total(&mut self.potential, tree_nodes)
    }

    /// The star of `node_count` nodes and `arc_count` arcs, written into this room and lent from it: each node `v` a
    /// leaf under the root, hanging by its root link `arc_count + v` the way `directions` gives it, at potential zero
    /// below a link that points up and `root_cost` below one that points down; the root at potential zero; the thread
    /// running root, `0`, `1`, …, `n − 1`. Nothing of an earlier tree is kept but the room: a tree the room kept is
    /// taken out before the first write.
    ///
    /// # Panics
    /// Panics if `node_count + 1` or `arc_count + node_count` passes `u32::MAX`, or if `directions` yields another
    /// number of directions than `node_count`. On a target whose `usize` has 32 bits it can also panic while it
    /// refills a column, where the column's room passes `isize::MAX` bytes: the potentials take `8·(n + 1)`.
    #[track_caller]
    pub(crate) fn star(
        &mut self,
        node_count: usize,
        arc_count: usize,
        directions: impl Iterator<Item = Direction>,
        root_cost: i64,
    ) -> LentTree<'_> {
        let within = |count: Option<usize>| count.is_some_and(|count| count <= INDEX_LIMIT);
        assert!(
            within(node_count.checked_add(1)) && within(arc_count.checked_add(node_count)),
            "called `TreeRoom::star` with {node_count} nodes and {arc_count} arcs, whose tree nodes or links pass \
             {INDEX_LIMIT}"
        );

        // The room keeps no tree from its first write on; the directions are counted where they were written, never by
        // what the iterator says of its length.
        self.kept = Kept::Nothing;
        refill(&mut self.direction, directions);
        let given = self.direction.len();
        assert!(given == node_count, "called `TreeRoom::star` with {given} directions for {node_count} nodes");

        self.hang_star(arc_count, root_cost)
    }

    /// Writes every column but the directions as the star of the `n` directions the room holds: node `v` hanging by
    /// the link `arc_count + v`, at potential `root_cost` where its link points down and zero where it points up; and
    /// lends the tree. `n + 1` and `arc_count + n` are at most `u32::MAX`.
    fn hang_star(&mut self, arc_count: usize, root_cost: i64) -> LentTree<'_> {
        let node_count = self.direction.len();
        // Every index narrowed below is at most `n + 1` or below `arc_count + n`, both at most `u32::MAX` as `star`
        // asserted: no narrowing can fail.
        let tree_node = |index: usize| TreeNode(narrow(index));
        let root = tree_node(node_count);

        // The thread runs root, 0, 1, …, n − 1 and back to the root: node `v` is followed by `v + 1` and the root by 0;
        // the root's last successor is the node before it, `n − 1`, or the root itself where it is alone.
        refill(&mut self.parent, iter::repeat_n(root, node_count + 1));
        refill(&mut self.thread, (1..=node_count).chain(iter::once(0)).map(tree_node));
        refill(&mut self.reverse_thread, iter::once(node_count).chain(0..node_count).map(tree_node));
        let root_last = node_count.checked_sub(1).unwrap_or(node_count);
        refill(&mut self.last_successor, (0..node_count).chain(iter::once(root_last)).map(tree_node));
        refill(&mut self.subtree_size, iter::repeat_n(1, node_count).chain(iter::once(narrow(node_count + 1))));
        refill(&mut self.parent_link, (0..node_count).map(|node| narrow(root_link_of(arc_count, node))));

        // Each potential makes its node's root link tight below the root at zero.
        let potential = |&direction: &Direction| tight_potential(0, root_link_cost(direction, root_cost), direction);
        refill(&mut self.potential, self.direction.iter().map(potential).chain(iter::once(0)));

        self.lend(arc_count + node_count)
    }

    /// The counts of the tree this room keeps, which [`resume`](Self::resume) lends again; `None` where it keeps
    /// none. It reads, and changes nothing.
    pub(crate) fn kept(&self) -> Option<KeptCounts> {
        match self.kept {
            Kept::Tree { link_count } => Some(KeptCounts { node_count: self.direction.len(), link_count }),
            Kept::Nothing => None,
        }
    }

    /// The tree this room keeps, lent again from the columns as its run left them, with its potentials; the room
    /// keeps no tree until it is kept again. It costs nothing more.
    ///
    /// # Panics
    /// Panics if the room keeps no tree: where [`kept`](Self::kept) answers `None`.
    #[track_caller]
    pub(crate) fn resume(&mut self) -> LentTree<'_> {
        let Kept::Tree { link_count } = mem::take(&mut self.kept) else { resume_of_nothing_kept() };

        self.lend(link_count)
    }

    /// The tree of `link_count` links whose columns the room holds, lent as they stand, beside the room's marker, which
    /// its caller has taken out.
    fn lend(&mut self, link_count: usize) -> LentTree<'_> {
        let columns = TreeColumnsMut {
            parent: &mut self.parent,
            parent_link: &mut self.parent_link,
            direction: &mut self.direction,
            thread: &mut self.thread,
            reverse_thread: &mut self.reverse_thread,
            last_successor: &mut self.last_successor,
            subtree_size: &mut self.subtree_size,
            potential: &mut self.potential,
        };

        LentTree { tree: SpanningTree { columns, link_count }, kept: &mut self.kept }
    }
}

impl<'r> SpanningTree<'r> {
    /// The number of nodes below the root, `n`.
    pub(crate) fn node_count(&self) -> usize {
        self.columns.node_count()
    }

    /// The number of links, `m + n`, which every link of the tree is below.
    pub(crate) fn link_count(&self) -> usize {
        self.link_count
    }

    /// Every tree node's potential, in units of cost, node `v` at `v`, the root's last.
    pub(crate) fn potentials(&self) -> &[i64] {
        self.columns.potential
    }

    /// The root, the tree node after the `n` hung nodes.
    pub(super) fn root(&self) -> TreeNode {
        // The tree's `n + 1` tree nodes are at most `u32::MAX` (`SpanningTree`), so `n` is stored exactly.
        TreeNode(stored(self.node_count()))
    }

    /// The root link of `node`, the link it hangs by where it hangs from the root: link `m + v` for node `v`.
    pub(super) fn root_link(&self, node: HungNode) -> Link<'r> {
        // A hung node is below `n`, so its root link is below `m + n`, the link count, at most `u32::MAX`
        // (`SpanningTree`): it is stored exactly.
        let raw = root_link_of(self.link_count - self.node_count(), usize::from(node));

        Link(stored(raw), PhantomData)
    }

    /// The climb of the cycle an arc from `first` to `second` closes with the tree, from both ends up toward their
    /// apex, holding the tree's borrow until it becomes the ends it finds; `None` where `first` or `second` is not
    /// below the node count. Its making reads the two ends' subtree sizes, and the whole climb costs at most the two
    /// ends' depths.
    pub(super) fn apex_climb(&mut self, first: usize, second: usize) -> Option<ApexClimb<'_>> {
        let (first, second) = (self.hung_node(first)?, self.hung_node(second)?);

        Some(ApexClimb::new(self.reborrow(), first, second))
    }

    /// The climb of the cycle from `first` to `second`, as [`apex_climb`](Self::apex_climb) makes it where it answers
    /// one.
    ///
    /// # Safety
    /// `first` and `second` are below the node count: the ends for which [`apex_climb`](Self::apex_climb) answers
    /// `Some`.
    pub(super) unsafe fn apex_climb_unchecked(&mut self, first: usize, second: usize) -> ApexClimb<'_> {
        debug_assert!(
            self.apex_climb(first, second).is_some(),
            "called `SpanningTree::apex_climb_unchecked` with ends {first} and {second}, which should both be below \
             the node count {}",
            self.node_count()
        );
        let (first, second) = (HungNode(TreeNode(stored(first))), HungNode(TreeNode(stored(second))));

        ApexClimb::new(self.reborrow(), first, second)
    }

    /// The link `raw`, where it is below the link count; `None` past it.
    pub(super) fn link(&self, raw: usize) -> Option<Link<'r>> {
        (raw < self.link_count).then(|| Link(stored(raw), PhantomData))
    }

    /// The link `raw`, as [`link`](Self::link) answers it where it answers one.
    ///
    /// # Safety
    /// `raw` is below the link count: a raw link for which [`link`](Self::link) answers `Some`.
    pub(super) unsafe fn link_unchecked(&self, raw: usize) -> Link<'r> {
        debug_assert!(
            self.link(raw).is_some(),
            "called `SpanningTree::link_unchecked` with link {raw}, which should be below the link count {}",
            self.link_count
        );

        Link(stored(raw), PhantomData)
    }

    /// The link `node` hangs by, and which way it runs seen from `node`.
    pub(super) fn hanging(&self, node: HungNode) -> Hanging<'_> {
        self.columns.hanging(node)
    }

    /// The tree's columns, lent for one walk that writes them.
    pub(super) fn columns_mut(&mut self) -> TreeColumnsMut<'_> {
        self.columns.reborrow()
    }

    /// The same tree, borrowed for less long: what a climb holds while it moves the tree. It has no marker: only the
    /// [`LentTree`] it was borrowed from can be kept.
    fn reborrow(&mut self) -> SpanningTree<'_> {
        SpanningTree { columns: self.columns.reborrow(), link_count: self.link_count }
    }

    /// The hung node `raw`, where it is below the node count; `None` at the root and past it.
    fn hung_node(&self, raw: usize) -> Option<HungNode> {
        (raw < self.node_count()).then(|| HungNode(TreeNode(stored(raw))))
    }
}

impl<'r> LentTree<'r> {
    /// Ends the lending: the room keeps the tree whole for [`TreeRoom::resume`], and every tree node's potential, as
    /// [`SpanningTree::potentials`], is lent for as long as the room is: what a run hands over when it is over.
    pub(crate) fn keep(self) -> &'r [i64] {
        let Self { tree, kept } = self;
        *kept = Kept::Tree { link_count: tree.link_count };

        tree.columns.potential
    }

    /// The tree lent, to be moved by a walk of the folder.
    pub(super) fn tree_mut(&mut self) -> &mut SpanningTree<'r> {
        &mut self.tree
    }
}

impl<'r> Deref for LentTree<'r> {
    type Target = SpanningTree<'r>;

    fn deref(&self) -> &SpanningTree<'r> {
        &self.tree
    }
}

impl From<TreeNode> for usize {
    fn from(node: TreeNode) -> Self {
        widen(node.0)
    }
}

impl From<HungNode> for usize {
    fn from(node: HungNode) -> Self {
        usize::from(node.0)
    }
}

impl From<Link<'_>> for usize {
    fn from(link: Link<'_>) -> Self {
        widen(link.0)
    }
}

impl HungNode {
    /// The same node, as a node of the tree.
    pub(super) fn node(self) -> TreeNode {
        self.0
    }
}

impl Direction {
    /// The direction the same link has seen from its other end.
    const fn reversed(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

impl<'t> ApexClimb<'t> {
    /// The climb of `first` and `second`, hung nodes of `tree`, toward their apex, not yet begun: each end with its
    /// subtree size.
    fn new(tree: SpanningTree<'t>, first: HungNode, second: HungNode) -> Self {
        let (first_size, second_size) = (tree.columns.subtree_size(first.0), tree.columns.subtree_size(second.0));

        Self {
            tree,
            first,
            second,
            at_first: first.0,
            first_size,
            at_second: second.0,
            second_size,
            #[cfg(debug_assertions)]
            steps: 0,
        }
    }

    /// The next node strictly below the apex, on either side, with its side and how it hangs; `None` once the two
    /// sides meet at the apex, and ever after. Within each side the nodes come in the order of its path from its end
    /// up.
    pub(super) fn step(&mut self) -> Option<ClimbStep<'_>> {
        // A proper ancestor's subtree holds its descendant's and itself, so it is strictly larger: the node of the
        // smaller subtree, or the second of two equal ones, is never a proper ancestor of the other, so it is never the
        // apex, and its parent lies on its path to the apex. Which of two equal ones moves changes only how the two
        // sides' steps interleave, never which nodes either side yields or in which order: each side yields its own
        // path. The root is an ancestor of every node, so a node strictly below the apex is hung. Each size is read
        // once, when its node is reached.
        if self.at_first == self.at_second {
            return None;
        }

        // Every step leaves a distinct hung node strictly below the apex, the two sides' paths meeting only there: at
        // most `n` steps, as two ends at the feet of two paths that make up the whole tree take.
        #[cfg(debug_assertions)]
        {
            self.steps += 1;
            debug_assert!(
                self.steps <= self.tree.node_count(),
                "the climb should reach the apex within the tree's hung nodes"
            );
        }

        let columns = &self.tree.columns;
        if self.first_size < self.second_size {
            let node = HungNode(self.at_first);
            self.at_first = columns.parent(node.0);
            self.first_size = columns.subtree_size(self.at_first);

            Some(ClimbStep { node, side: Side::First, hanging: columns.hanging(node) })
        } else {
            let node = HungNode(self.at_second);
            self.at_second = columns.parent(node.0);
            self.second_size = columns.subtree_size(self.at_second);

            Some(ClimbStep { node, side: Side::Second, hanging: columns.hanging(node) })
        }
    }

    /// The two ends and their apex, with the tree, the climb finished first where it was not.
    pub(super) fn into_ends(mut self) -> CycleEnds<'t> {
        while self.step().is_some() {}
        let Self { tree, first, second, at_first, .. } = self;

        CycleEnds { tree, first, second, apex: at_first }
    }
}

impl<'t> CycleEnds<'t> {
    /// The end of `side`: the first end or the second.
    pub(super) fn end(&self, side: Side) -> HungNode {
        match side {
            Side::First => self.first,
            Side::Second => self.second,
        }
    }

    /// The deepest common ancestor of the two ends.
    pub(super) fn apex(&self) -> TreeNode {
        self.apex
    }

    /// How each node of `side` hangs, from its end up to the apex.
    pub(super) fn side(&self, side: Side) -> SideHangingsIter<'_> {
        let columns = &self.tree.columns;
        let hangings = Hangings { links: columns.parent_link, directions: columns.direction };

        SideHangingsIter { parents: Parents(columns.parent), hangings, next: self.end(side).0, apex: self.apex }
    }

    /// The tree, for the exchange to move.
    pub(super) fn into_tree(self) -> SpanningTree<'t> {
        self.tree
    }
}

impl<'c> Iterator for SideHangingsIter<'c> {
    type Item = Hanging<'c>;

    fn next(&mut self) -> Option<Hanging<'c>> {
        if self.next == self.apex {
            return None;
        }

        // The root is an ancestor of every node, so a walk up from a hung end that stops at the deepest common
        // ancestor of the ends meets the root only as that ancestor: every node it yields before it is hung.
        let node = HungNode(self.next);
        self.next = self.parents.parent(self.next);

        Some(self.hangings.of(node))
    }
}

impl FusedIterator for SideHangingsIter<'_> {}

impl Parents<'_> {
    /// The parent of `node`; the root's is the root.
    fn parent(self, node: TreeNode) -> TreeNode {
        let index = usize::from(node);
        debug_assert!(index < self.0.len(), "a tree node should be below the tree's n + 1");
        // SAFETY: a `TreeNode` is below the `n + 1` entries this slice holds (`TreeNode`, `Parents`).
        unsafe { *self.0.get_unchecked(index) }
    }
}

impl Sizes<'_> {
    /// The number of nodes of the subtree of `node`.
    fn of(self, node: TreeNode) -> u32 {
        let index = usize::from(node);
        debug_assert!(index < self.0.len(), "a tree node should be below the tree's n + 1");
        // SAFETY: a `TreeNode` is below the `n + 1` entries this slice holds (`TreeNode`, `Sizes`).
        unsafe { *self.0.get_unchecked(index) }
    }
}

impl<'c> Hangings<'c> {
    /// The link `node` hangs by, and which way it runs seen from `node`.
    fn of(self, node: HungNode) -> Hanging<'c> {
        let index = usize::from(node);
        debug_assert!(
            index < self.links.len() && index < self.directions.len(),
            "a hung node should be below the tree's n, the length of both columns"
        );
        // SAFETY: a `HungNode` is below the `n` entries each of these slices holds (`HungNode`, `Hangings`).
        let (link, direction) = unsafe { (*self.links.get_unchecked(index), *self.directions.get_unchecked(index)) };

        Hanging { link: Link(link, PhantomData), direction }
    }
}

impl TreeColumnsMut<'_> {
    /// The same columns, borrowed for less long.
    fn reborrow(&mut self) -> TreeColumnsMut<'_> {
        TreeColumnsMut {
            parent: self.parent,
            parent_link: self.parent_link,
            direction: self.direction,
            thread: self.thread,
            reverse_thread: self.reverse_thread,
            last_successor: self.last_successor,
            subtree_size: self.subtree_size,
            potential: self.potential,
        }
    }

    /// The number of hung nodes, `n`.
    fn node_count(&self) -> usize {
        self.direction.len()
    }

    /// `node` as a hung node, where it is below the tree's `n`; `None` at the root.
    pub(super) fn hung(&self, node: TreeNode) -> Option<HungNode> {
        (usize::from(node) < self.node_count()).then_some(HungNode(node))
    }

    /// `node` as a hung node, as [`hung`](Self::hung) answers it where it answers one.
    ///
    /// # Safety
    /// `node` is not the root: it is below the tree's `n`, where [`hung`](Self::hung) answers `Some`.
    pub(super) unsafe fn hung_unchecked(&self, node: TreeNode) -> HungNode {
        debug_assert!(
            self.hung(node).is_some(),
            "called `TreeColumnsMut::hung_unchecked` with {node:?}, which should be below the tree's {} hung nodes",
            self.node_count()
        );

        HungNode(node)
    }

    /// The parent of `node`; the root's is the root.
    pub(super) fn parent(&self, node: TreeNode) -> TreeNode {
        Parents(self.parent).parent(node)
    }

    /// The node after `node` in the thread.
    pub(super) fn thread(&self, node: TreeNode) -> TreeNode {
        let index = usize::from(node);
        debug_assert!(index < self.thread.len(), "a tree node should be below the tree's n + 1");
        // SAFETY: a `TreeNode` is below the `n + 1` entries this slice holds (`TreeNode`, `TreeColumnsMut`).
        unsafe { *self.thread.get_unchecked(index) }
    }

    /// The node before `node` in the thread.
    pub(super) fn reverse_thread(&self, node: TreeNode) -> TreeNode {
        let index = usize::from(node);
        debug_assert!(index < self.reverse_thread.len(), "a tree node should be below the tree's n + 1");
        // SAFETY: a `TreeNode` is below the `n + 1` entries this slice holds (`TreeNode`, `TreeColumnsMut`).
        unsafe { *self.reverse_thread.get_unchecked(index) }
    }

    /// The last node of the subtree of `node` in the thread.
    pub(super) fn last_successor(&self, node: TreeNode) -> TreeNode {
        let index = usize::from(node);
        debug_assert!(index < self.last_successor.len(), "a tree node should be below the tree's n + 1");
        // SAFETY: a `TreeNode` is below the `n + 1` entries this slice holds (`TreeNode`, `TreeColumnsMut`).
        unsafe { *self.last_successor.get_unchecked(index) }
    }

    /// The number of nodes of the subtree of `node`, itself included.
    pub(super) fn subtree_size(&self, node: TreeNode) -> u32 {
        Sizes(self.subtree_size).of(node)
    }

    /// Makes `parent` the parent of `node`.
    pub(super) fn set_parent(&mut self, node: TreeNode, parent: TreeNode) {
        let index = usize::from(node);
        debug_assert!(index < self.parent.len(), "a tree node should be below the tree's n + 1");
        // SAFETY: a `TreeNode` is below the `n + 1` entries this slice holds (`TreeNode`, `TreeColumnsMut`).
        unsafe { *self.parent.get_unchecked_mut(index) = parent };
    }

    /// Makes `node` hang by `link`, running `direction` seen from `node`.
    pub(super) fn set_hanging(&mut self, node: HungNode, link: Link<'_>, direction: Direction) {
        self.write_hanging(node, link.0, direction);
    }

    /// Makes `node` hang by the link `below` hangs by, turned around: the link of the node below `node` once the
    /// parent relation between the two is reversed.
    pub(super) fn hang_by_turned_link_of(&mut self, node: HungNode, below: HungNode) {
        let Hanging { link, direction } = self.hanging(below);
        let raw = link.0;

        self.write_hanging(node, raw, direction.reversed());
    }

    /// Makes `after` follow `before` in the thread, both links written.
    pub(super) fn join_thread(&mut self, before: TreeNode, after: TreeNode) {
        let (before_index, after_index) = (usize::from(before), usize::from(after));
        debug_assert!(
            before_index < self.thread.len() && after_index < self.reverse_thread.len(),
            "both tree nodes should be below the tree's n + 1, the length of both thread columns"
        );
        // SAFETY: a `TreeNode` is below the `n + 1` entries each of these slices holds (`TreeNode`, `TreeColumnsMut`).
        unsafe {
            *self.thread.get_unchecked_mut(before_index) = after;
            *self.reverse_thread.get_unchecked_mut(after_index) = before;
        }
    }

    /// Makes `last` the last node of the subtree of `node` in the thread.
    pub(super) fn set_last_successor(&mut self, node: TreeNode, last: TreeNode) {
        let index = usize::from(node);
        debug_assert!(index < self.last_successor.len(), "a tree node should be below the tree's n + 1");
        // SAFETY: a `TreeNode` is below the `n + 1` entries this slice holds (`TreeNode`, `TreeColumnsMut`).
        unsafe { *self.last_successor.get_unchecked_mut(index) = last };
    }

    /// Makes `size` the number of nodes of the subtree of `node`.
    pub(super) fn set_subtree_size(&mut self, node: TreeNode, size: u32) {
        let index = usize::from(node);
        debug_assert!(index < self.subtree_size.len(), "a tree node should be below the tree's n + 1");
        // SAFETY: a `TreeNode` is below the `n + 1` entries this slice holds (`TreeNode`, `TreeColumnsMut`).
        unsafe { *self.subtree_size.get_unchecked_mut(index) = size };
    }

    /// The potential of `node`, in units of cost.
    pub(super) fn potential(&self, node: TreeNode) -> i64 {
        let index = usize::from(node);
        debug_assert!(index < self.potential.len(), "a tree node should be below the tree's n + 1");
        // SAFETY: a `TreeNode` is below the `n + 1` entries this slice holds (`TreeNode`, `TreeColumnsMut`).
        unsafe { *self.potential.get_unchecked(index) }
    }

    /// Makes `potential` the potential of `node`, in units of cost.
    pub(super) fn set_potential(&mut self, node: TreeNode, potential: i64) {
        let index = usize::from(node);
        debug_assert!(index < self.potential.len(), "a tree node should be below the tree's n + 1");
        // SAFETY: a `TreeNode` is below the `n + 1` entries this slice holds (`TreeNode`, `TreeColumnsMut`).
        unsafe { *self.potential.get_unchecked_mut(index) = potential };
    }

    /// Adds `shift` to the potential of `node`, wrapping.
    pub(super) fn shift_potential(&mut self, node: TreeNode, shift: i64) {
        let index = usize::from(node);
        debug_assert!(index < self.potential.len(), "a tree node should be below the tree's n + 1");
        // SAFETY: a `TreeNode` is below the `n + 1` entries this slice holds (`TreeNode`, `TreeColumnsMut`).
        let potential = unsafe { self.potential.get_unchecked_mut(index) };
        // The shift comes from the module that prices the arcs, and the tree cannot know the range it keeps its
        // potentials in; within that range the sum is exact, and wrapping is that sum with no panic on the walk.
        *potential = potential.wrapping_add(shift);
    }

    /// The link `node` hangs by, and which way it runs seen from `node`.
    pub(super) fn hanging(&self, node: HungNode) -> Hanging<'_> {
        Hangings { links: self.parent_link, directions: self.direction }.of(node)
    }

    /// Makes `node` hang by the stored link `link`, running `direction`.
    fn write_hanging(&mut self, node: HungNode, link: u32, direction: Direction) {
        let index = usize::from(node);
        debug_assert!(
            index < self.parent_link.len() && index < self.direction.len(),
            "a hung node should be below the tree's n, the length of both columns"
        );
        // SAFETY: a `HungNode` is below the `n` entries each of these slices holds (`HungNode`, `TreeColumnsMut`).
        unsafe {
            *self.parent_link.get_unchecked_mut(index) = link;
            *self.direction.get_unchecked_mut(index) = direction;
        }
    }
}

/// The cost of a root link that runs `direction` seen from its node, in units of cost: zero where it runs up, into the
/// root, and `root_cost` where it runs down, out of it.
pub(crate) const fn root_link_cost(direction: Direction, root_cost: i64) -> i64 {
    match direction {
        Direction::Up => 0,
        Direction::Down => root_cost,
    }
}

/// The potential, in units of cost, that makes a link of cost `cost`, running `direction` seen from the node below it,
/// tight below a parent at potential `parent`: the parent's less the cost where the link runs up, plus the cost where
/// it runs down, wrapping.
pub(super) fn tight_potential(parent: i64, cost: i64, direction: Direction) -> i64 {
    // A link is tight where its cost plus its tail's potential less its head's is zero; the node below is the tail of
    // a link that runs up and the head of one that runs down. The tree cannot know the range its potentials are kept
    // in; within it the sum is exact, and wrapping is that sum with no panic on the walk.
    match direction {
        Direction::Up => parent.wrapping_sub(cost),
        Direction::Down => parent.wrapping_add(cost),
    }
}

/// Panics for a resume of a room that keeps no tree, as [`TreeRoom::resume`] documents.
///
/// # Panics
/// Always: that is its job.
#[cold]
#[inline(never)]
#[track_caller]
fn resume_of_nothing_kept() -> ! {
    panic!("called `TreeRoom::resume` on a room that keeps no tree")
}

/// The link node `node` hangs by from the root, in a tree of `arc_count` arcs: its root link, `arc_count + node`.
fn root_link_of(arc_count: usize, node: usize) -> usize {
    arc_count + node
}

/// `index` at the 32 bits the tree stores an index at, on the pivot's path: exact for every index this file hands it,
/// each compared with, or promised below, the node count or the link count of a tree, both at most `u32::MAX`
/// ([`SpanningTree`]). The cold passes narrow by [`narrow`], which checks.
fn stored(index: usize) -> u32 {
    debug_assert!(index <= INDEX_LIMIT, "an index the tree stores should be at most u32::MAX");
    #[expect(clippy::cast_possible_truncation, reason = "the index is below a count of the tree, at most u32::MAX")]
    let raw = index as u32;

    raw
}

#[cfg(test)]
pub(super) mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use itertools::izip;
    use proptest::prelude::*;
    use proptest::sample::Index;

    use super::*;
    use crate::tree::plain_tree::{PlainColumns, PlainTree, link_count_of, plain_trees, seven_nodes};

    /// The way one hung node hangs in a loaded tree, as plain numbers: what the tests of the modules built on the
    /// tree read of it. Plain data.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct PlainHanging {
        /// The node's parent, the root being the node count.
        pub(crate) parent: usize,
        /// The link it hangs by.
        pub(crate) link: usize,
        /// Which way that link runs, seen from the node.
        pub(crate) direction: Direction,
    }

    /// One step of a climb or a walk as plain numbers. Plain data.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(in crate::tree) struct PlainStep {
        /// The side of the cycle the node lies on.
        pub(in crate::tree) side: Side,
        /// The node.
        pub(in crate::tree) node: usize,
        /// The link it hangs by.
        pub(in crate::tree) link: usize,
        /// Which way that link runs, seen from the node.
        pub(in crate::tree) direction: Direction,
    }

    /// The step on `side` at `node`, hanging by `link` the way `direction` says, for a test to write out.
    pub(in crate::tree) fn step(side: Side, node: usize, link: usize, direction: Direction) -> PlainStep {
        PlainStep { side, node, link, direction }
    }

    /// How each hung node of `tree` hangs, node `v` at `v`.
    pub(crate) fn hangings_of(tree: &SpanningTree<'_>) -> Vec<PlainHanging> {
        let columns = &tree.columns;

        izip!(columns.parent.iter(), columns.parent_link.iter(), columns.direction.iter())
            .map(|(&parent, &link, &direction)| PlainHanging {
                parent: usize::from(parent),
                link: widen(link),
                direction,
            })
            .collect()
    }

    /// Checks the columns of `tree` that its parents imply, the thread, its reverse, the last successors and the
    /// subtree sizes, against what the model recomputes from the parents and links alone: what the tests of the
    /// modules built on the tree check after every change they make to it.
    ///
    /// # Errors
    /// Returns a message where a node's parents do not lead to the root, or naming the first column that differs.
    pub(crate) fn check_threading(tree: &SpanningTree<'_>) -> Result<(), String> {
        let columns = columns_of(tree);
        let root = columns.parents.len() - 1;

        // The model's walks end only on parents without a cycle, so that is checked first, each walk at most `n`
        // steps.
        for node in 0..root {
            let mut above = node;
            for _ in 0..root {
                if above != root {
                    above = columns.parents[above];
                }
            }
            if above != root {
                return Err(format!("node {node} does not lead to the root"));
            }
        }

        PlainTree { parents: columns.parents.clone(), hangings: columns.hangings.clone() }.check(&columns, None)
    }

    /// Every column of `tree` read out as plain numbers.
    ///
    /// # Panics
    /// Panics if the link and direction columns differ in length.
    pub(in crate::tree) fn columns_of(tree: &SpanningTree<'_>) -> PlainColumns {
        let columns = &tree.columns;
        assert_eq!(columns.parent_link.len(), columns.direction.len(), "one link and one direction a hung node");
        let nodes = |column: &[TreeNode]| column.iter().map(|&node| usize::from(node)).collect();
        let hangings = columns.parent_link.iter().zip(columns.direction.iter());

        PlainColumns {
            parents: nodes(columns.parent),
            hangings: hangings.map(|(&link, &direction)| (widen(link), direction)).collect(),
            threads: nodes(columns.thread),
            reverse_threads: nodes(columns.reverse_thread),
            last_successors: nodes(columns.last_successor),
            subtree_sizes: columns.subtree_size.iter().map(|&size| widen(size)).collect(),
            potentials: columns.potential.to_vec(),
        }
    }

    /// The tree whose columns are `columns`, with `link_count` links, written into `room` as they are, in the room it
    /// holds: a test's tree in any state.
    pub(in crate::tree) fn load<'r>(room: &'r mut TreeRoom, columns: &PlainColumns, link_count: usize) -> LentTree<'r> {
        let node = |index: &usize| TreeNode(u32::try_from(*index).expect("a test's node should fit a u32"));
        let raw = |index: usize| u32::try_from(index).expect("a test's index should fit a u32");
        room.kept = Kept::Nothing;
        refill(&mut room.parent, columns.parents.iter().map(node));
        refill(&mut room.parent_link, columns.hangings.iter().map(|&(link, _)| raw(link)));
        refill(&mut room.direction, columns.hangings.iter().map(|&(_, direction)| direction));
        refill(&mut room.thread, columns.threads.iter().map(node));
        refill(&mut room.reverse_thread, columns.reverse_threads.iter().map(node));
        refill(&mut room.last_successor, columns.last_successors.iter().map(node));
        refill(&mut room.subtree_size, columns.subtree_sizes.iter().map(|&size| raw(size)));
        refill(&mut room.potential, columns.potentials.iter().copied());

        room.lend(link_count)
    }

    /// The tree of `model` with `link_count` links, threaded in the model's preorder, every link tight under `costs`.
    pub(in crate::tree) fn tree_of<'r>(
        room: &'r mut TreeRoom,
        model: &PlainTree,
        link_count: usize,
        costs: &[i64],
    ) -> LentTree<'r> {
        load(room, &model.columns(costs), link_count)
    }

    /// The tree of `model` with twice as many links as nodes, every potential zero.
    pub(in crate::tree) fn plain_tree_of<'r>(room: &'r mut TreeRoom, model: &PlainTree) -> LentTree<'r> {
        let link_count = 2 * model.root();

        tree_of(room, model, link_count, &vec![0; link_count])
    }

    /// The tree node `index`, for a test that hands it to a pure step.
    pub(in crate::tree) fn tree_node(index: u32) -> TreeNode {
        TreeNode(index)
    }

    /// `step` as plain numbers.
    pub(in crate::tree) fn plain_step(climb_step: &ClimbStep<'_>) -> PlainStep {
        step(
            climb_step.side,
            usize::from(climb_step.node),
            usize::from(climb_step.hanging.link),
            climb_step.hanging.direction,
        )
    }

    /// Every step of `climb`, in a tree of `hung_nodes` nodes below the root, as plain numbers, then the two ends and
    /// the apex it finds.
    ///
    /// # Panics
    /// Panics if the climb takes more steps than the tree has nodes below the root.
    fn climbed(mut climb: ApexClimb<'_>, hung_nodes: usize) -> (Vec<PlainStep>, [usize; 3]) {
        let mut steps = Vec::new();
        while let Some(climb_step) = climb.step() {
            assert!(steps.len() < hung_nodes, "a climb should take at most one step a node below the root");
            steps.push(plain_step(&climb_step));
        }
        let ends = climb.into_ends();

        (steps, [usize::from(ends.end(Side::First)), usize::from(ends.end(Side::Second)), usize::from(ends.apex())])
    }

    /// The hangings `side` yields, as plain numbers.
    fn plain_side(side: SideHangingsIter<'_>) -> Vec<(usize, Direction)> {
        side.map(|hanging| (usize::from(hanging.link), hanging.direction)).collect()
    }

    /// The cost of each of `link_count` links that makes the star of `directions` tight: zero for an arc and a root
    /// link that points up, `root_cost` for one that points down.
    fn star_costs(directions: &[Direction], link_count: usize, root_cost: i64) -> Vec<i64> {
        let arc_count = link_count - directions.len();
        let root_links = directions.iter().map(|direction| match direction {
            Direction::Up => 0,
            Direction::Down => root_cost,
        });

        iter::repeat_n(0, arc_count).chain(root_links).collect()
    }

    #[test]
    fn test_the_star_by_hand() {
        let (up, down) = (Direction::Up, Direction::Down);
        let mut room = TreeRoom::new();
        let tree = room.star(3, 4, [up, down, down].into_iter(), 9);

        let expected = PlainColumns {
            parents: vec![3, 3, 3, 3],
            hangings: vec![(4, up), (5, down), (6, down)],
            threads: vec![1, 2, 3, 0],
            reverse_threads: vec![3, 0, 1, 2],
            last_successors: vec![0, 1, 2, 2],
            subtree_sizes: vec![1, 1, 1, 4],
            potentials: vec![0, 9, 9, 0],
        };
        assert_eq!(columns_of(&tree), expected);
        assert_eq!((tree.node_count(), tree.link_count(), tree.potentials()), (3, 7, &[0, 9, 9, 0][..]));
        assert_eq!(tree.keep(), [0, 9, 9, 0]);
    }

    #[test]
    fn test_a_star_of_one_node_and_of_none_by_hand() {
        let mut room = TreeRoom::default();
        let one = room.star(1, 0, iter::once(Direction::Down), 1);
        let expected = PlainColumns {
            parents: vec![1, 1],
            hangings: vec![(0, Direction::Down)],
            threads: vec![1, 0],
            reverse_threads: vec![1, 0],
            last_successors: vec![0, 0],
            subtree_sizes: vec![1, 2],
            potentials: vec![1, 0],
        };
        assert_eq!(columns_of(&one), expected);

        let none = room.star(0, 5, iter::empty(), 1);
        let alone = PlainColumns {
            parents: vec![0],
            hangings: vec![],
            threads: vec![0],
            reverse_threads: vec![0],
            last_successors: vec![0],
            subtree_sizes: vec![1],
            potentials: vec![0],
        };
        assert_eq!((columns_of(&none), none.node_count(), none.link_count()), (alone, 0, 5));
    }

    #[test]
    fn test_every_star_agrees_with_the_model() {
        let (up, down) = (Direction::Up, Direction::Down);
        let mut room = TreeRoom::new();
        for directions in [vec![], vec![up], vec![down, down], vec![up, down, up, up, down, down, up]] {
            for arc_count in [0, 1, 12] {
                let tree = room.star(directions.len(), arc_count, directions.iter().copied(), 17);
                let costs = star_costs(&directions, tree.link_count(), 17);
                let model = PlainTree::star(&directions, arc_count);

                assert_eq!(model.check(&columns_of(&tree), Some(&costs)), Ok(()), "{directions:?}, {arc_count} arcs");
            }
        }
    }

    #[test]
    fn test_check_threading_accepts_a_star_and_names_a_broken_tree_by_hand() {
        let (up, down) = (Direction::Up, Direction::Down);
        let mut room = TreeRoom::new();
        let star = columns_of(&room.star(3, 4, [up, down, up].into_iter(), 9));
        assert_eq!(check_threading(&load(&mut room, &star, 7)), Ok(()));

        // Node 0 under node 1 and node 1 under node 0: neither leads to the root.
        let cycle = PlainColumns { parents: vec![1, 0, 3, 3], ..star.clone() };
        assert_eq!(check_threading(&load(&mut room, &cycle, 7)), Err("node 0 does not lead to the root".to_owned()));

        let sizes = PlainColumns { subtree_sizes: vec![1, 2, 1, 4], ..star };
        assert_eq!(check_threading(&load(&mut room, &sizes, 7)), Err("1 has subtree size 2, not 1".to_owned()));
    }

    #[test]
    fn test_a_room_stars_a_smaller_tree_in_the_room_it_holds() {
        let mut room = TreeRoom::new();
        let _ = room.star(6, 3, iter::repeat_n(Direction::Up, 6), 5);
        let held = room.parent.capacity();

        let directions = [Direction::Down, Direction::Up];
        let tree = room.star(2, 1, directions.into_iter(), 4);
        let model = PlainTree::star(&directions, 1);
        assert_eq!(model.check(&columns_of(&tree), Some(&star_costs(&directions, 3, 4))), Ok(()));
        assert_eq!(room.parent.capacity(), held);
    }

    #[test]
    fn test_try_reserve_makes_room_for_a_star() {
        let mut room = TreeRoom::new();
        assert_eq!(room.parent.capacity(), 0, "a new room holds nothing");
        room.try_reserve(9).expect("room for nine nodes should be granted");

        let capacities = |room: &TreeRoom| {
            [room.parent.capacity(), room.thread.capacity(), room.reverse_thread.capacity()]
                .into_iter()
                .chain([room.last_successor.capacity(), room.subtree_size.capacity(), room.potential.capacity()])
                .min()
        };
        assert!(capacities(&room) >= Some(10));
        assert!(room.parent_link.capacity() >= 9 && room.direction.capacity() >= 9);

        let _ = room.star(9, 4, iter::repeat_n(Direction::Up, 9), 1);
        room.try_reserve(2).expect("a smaller reservation should be granted");
        assert!(capacities(&room) >= Some(10), "room already held is kept");

        assert!(room.try_reserve(usize::MAX).is_err());
    }

    #[test]
    #[should_panic(expected = "called `TreeRoom::star` with 4294967295 nodes and 0 arcs, whose tree nodes or links \
                               pass 4294967295")]
    fn test_a_star_refuses_tree_nodes_past_the_index_limit() {
        let _ = TreeRoom::new().star(INDEX_LIMIT, 0, iter::repeat_n(Direction::Up, INDEX_LIMIT), 1);
    }

    #[test]
    #[should_panic(expected = "called `TreeRoom::star` with 1 nodes and 4294967295 arcs, whose tree nodes or links \
                               pass 4294967295")]
    fn test_a_star_refuses_links_past_the_index_limit() {
        let _ = TreeRoom::new().star(1, INDEX_LIMIT, iter::once(Direction::Up), 1);
    }

    #[test]
    fn test_a_star_accepts_links_at_the_index_limit() {
        let mut room = TreeRoom::new();
        let tree = room.star(1, INDEX_LIMIT - 1, iter::once(Direction::Up), 1);

        assert_eq!(tree.link_count(), INDEX_LIMIT);
        assert_eq!(columns_of(&tree).hangings, [(INDEX_LIMIT - 1, Direction::Up)]);
    }

    #[test]
    #[should_panic(expected = "called `TreeRoom::star` with 2 directions for 3 nodes")]
    fn test_a_star_refuses_fewer_directions_than_nodes() {
        let _ = TreeRoom::new().star(3, 0, [Direction::Up, Direction::Up].into_iter(), 1);
    }

    #[test]
    fn test_a_room_whose_star_panicked_stars_again_as_a_fresh_one() {
        let directions = [Direction::Up, Direction::Down, Direction::Down];
        let mut fresh = TreeRoom::new();
        let expected = columns_of(&fresh.star(3, 2, directions.into_iter(), 7));

        let mut room = TreeRoom::new();
        let _ = room.star(5, 1, iter::repeat_n(Direction::Up, 5), 3);
        let miscounted = catch_unwind(AssertUnwindSafe(|| {
            let _ = room.star(3, 0, iter::repeat_n(Direction::Down, 4), 1);
        }));
        assert!(miscounted.is_err(), "four directions for three nodes");
        assert_eq!(columns_of(&room.star(3, 2, directions.into_iter(), 7)), expected);

        let panicked = catch_unwind(AssertUnwindSafe(|| {
            let directions = (0..3).map(|at| if at < 2 { Direction::Up } else { panic!("the third direction") });
            let _ = room.star(3, 0, directions, 1);
        }));
        assert!(panicked.is_err());
        assert_eq!(columns_of(&room.star(3, 2, directions.into_iter(), 7)), expected);
    }

    #[test]
    fn test_hung_nodes_and_links_are_made_exactly_below_their_counts() {
        let mut room = TreeRoom::new();
        let mut tree = room.star(3, 2, iter::repeat_n(Direction::Up, 3), 1);

        assert_eq!(tree.hung_node(2).map(usize::from), Some(2));
        assert_eq!((tree.hung_node(3), tree.hung_node(usize::MAX)), (None, None), "the root and past it");
        assert_eq!(tree.link(4).map(usize::from), Some(4));
        assert_eq!((tree.link(5), tree.link(usize::MAX)), (None, None));
        assert!(
            tree.tree_mut().apex_climb(3, 0).is_none() && tree.tree_mut().apex_climb(0, usize::MAX).is_none(),
            "an end past the nodes"
        );

        let columns = tree.tree_mut().columns_mut();
        assert_eq!(columns.hung(TreeNode(2)).map(usize::from), Some(2));
        assert_eq!(columns.hung(TreeNode(3)), None, "the root");
    }

    #[test]
    fn test_the_views_read_and_write_each_column_by_hand() {
        let mut room = TreeRoom::new();
        let mut tree = room.star(2, 3, [Direction::Up, Direction::Down].into_iter(), 6);
        let (zero, one, root) = (TreeNode(0), TreeNode(1), TreeNode(2));
        let entering = tree.link(1).expect("link 1 lies below the link count");

        let mut columns = tree.tree_mut().columns_mut();
        let hung_one = columns.hung(one).expect("1 lies below the root");
        let hung_zero = columns.hung(zero).expect("0 lies below the root");
        columns.set_parent(one, zero);
        columns.set_hanging(hung_one, entering, Direction::Up);
        columns.hang_by_turned_link_of(hung_zero, hung_one);
        columns.join_thread(one, root);
        columns.join_thread(root, zero);
        columns.set_last_successor(zero, one);
        columns.set_subtree_size(zero, 2);
        columns.shift_potential(one, -10);
        assert_eq!((columns.parent(one), columns.thread(root), columns.reverse_thread(zero)), (zero, zero, root));
        assert_eq!((columns.last_successor(zero), columns.subtree_size(zero), columns.subtree_size(root)), (one, 2, 3));

        let columns = columns_of(&tree);
        assert_eq!(columns.hangings, [(1, Direction::Down), (1, Direction::Up)]);
        assert_eq!((columns.reverse_threads[2], columns.potentials), (1, vec![0, -4, 0]));
        let Hanging { link, direction } = tree.hanging(hung_one);
        assert_eq!((usize::from(link), direction), (1, Direction::Up));
    }

    #[test]
    fn test_the_views_read_and_write_a_potential_by_hand() {
        let mut room = TreeRoom::new();
        let mut tree = room.star(2, 3, [Direction::Up, Direction::Down].into_iter(), 6);

        let mut columns = tree.tree_mut().columns_mut();
        assert_eq!([0, 1, 2].map(|node| columns.potential(TreeNode(node))), [0, 6, 0]);
        columns.set_potential(TreeNode(0), -9);
        columns.set_potential(TreeNode(2), 4);
        assert_eq!([0, 1, 2].map(|node| columns.potential(TreeNode(node))), [-9, 6, 4]);
        assert_eq!(tree.potentials(), [-9, 6, 4]);
    }

    #[test]
    fn test_the_root_and_each_hung_nodes_root_link_by_hand() {
        let mut room = TreeRoom::new();
        let tree = room.star(3, 4, iter::repeat_n(Direction::Up, 3), 1);
        assert_eq!(usize::from(tree.root()), 3);
        assert_eq!([0, 1, 2].map(|node| usize::from(tree.root_link(HungNode(TreeNode(node))))), [4, 5, 6]);

        // Seven nodes and fourteen links: the root links are links 7 to 13, whatever the nodes hang by.
        let tree = plain_tree_of(&mut room, &seven_nodes());
        assert_eq!((usize::from(tree.root()), tree.link_count()), (7, 14));
        assert_eq!([0, 3, 6].map(|node| usize::from(tree.root_link(HungNode(TreeNode(node))))), [7, 10, 13]);

        let alone = room.star(0, 2, iter::empty(), 1);
        assert_eq!(usize::from(alone.root()), 0);
    }

    #[test]
    fn test_a_kept_tree_is_lent_again_as_its_run_left_it_by_hand() {
        let model = seven_nodes();
        let costs: Vec<i64> = (0..14).map(|link| 3 * link - 7).collect();
        let mut room = TreeRoom::new();
        let tree = tree_of(&mut room, &model, 14, &costs);
        let before = columns_of(&tree);
        assert!(before.potentials.iter().any(|&potential| potential != 0), "potentials a reload would lose");

        assert_eq!(tree.keep(), before.potentials.as_slice());
        assert_eq!(room.kept(), Some(KeptCounts { node_count: 7, link_count: 14 }));

        let resumed = room.resume();
        assert_eq!((resumed.node_count(), resumed.link_count()), (7, 14));
        assert_eq!(columns_of(&resumed), before);
    }

    #[test]
    fn test_a_kept_star_is_lent_again_as_it_was_left_by_hand() {
        let mut room = TreeRoom::new();
        let star = room.star(3, 4, [Direction::Up, Direction::Down, Direction::Down].into_iter(), 9);
        let before = columns_of(&star);
        let _ = star.keep();
        assert_eq!(room.kept(), Some(KeptCounts { node_count: 3, link_count: 7 }));

        assert_eq!(columns_of(&room.resume()), before);
    }

    #[test]
    fn test_a_room_keeps_a_tree_only_from_its_keep_to_its_next_lending() {
        let mut room = TreeRoom::new();
        assert_eq!(room.kept(), None, "a new room keeps nothing");

        let _ = room.star(3, 4, iter::repeat_n(Direction::Up, 3), 1);
        assert_eq!(room.kept(), None, "a tree lent and dropped is not kept");

        let _ = room.star(3, 4, iter::repeat_n(Direction::Up, 3), 1).keep();
        let copy = room.clone();
        let _ = room.star(2, 1, iter::repeat_n(Direction::Up, 2), 1);
        assert_eq!(room.kept(), None, "a star after a keep takes it out");
        assert_eq!(copy.kept(), Some(KeptCounts { node_count: 3, link_count: 7 }), "a copy keeps what was kept");

        let _ = room.star(2, 1, iter::repeat_n(Direction::Up, 2), 1).keep();
        let _ = room.resume();
        assert_eq!(room.kept(), None, "a resume takes it out until the next keep");
        let _ = room.star(2, 1, iter::repeat_n(Direction::Down, 2), 1).keep();
        assert_eq!(room.kept(), Some(KeptCounts { node_count: 2, link_count: 3 }));
    }

    #[test]
    #[should_panic(expected = "called `TreeRoom::resume` on a room that keeps no tree")]
    fn test_resume_refuses_a_room_that_keeps_no_tree() {
        let mut room = TreeRoom::new();
        let _ = room.star(3, 4, iter::repeat_n(Direction::Up, 3), 1).keep();
        let _ = room.resume();

        let _ = room.resume();
    }

    #[test]
    fn test_the_climb_moves_the_smaller_subtree_and_ends_at_the_apex_by_hand() {
        let (first, second, up, down) = (Side::First, Side::Second, Direction::Up, Direction::Down);
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());

        // 3 and 4 have one node each: of two equal ones the second moves, to 0 of five nodes; then 3, of one node, 2,
        // of two, and 1, of three, each smaller than 0's five, climb to 0, where the two sides meet.
        let mut climb = tree.tree_mut().apex_climb(3, 4).expect("3 and 4 lie below the node count");
        assert_eq!(climb.step().as_ref().map(plain_step), Some(step(second, 4, 4, up)));
        assert_eq!(climb.step().as_ref().map(plain_step), Some(step(first, 3, 3, up)));
        assert_eq!(climb.step().as_ref().map(plain_step), Some(step(first, 2, 2, down)));
        assert_eq!(climb.step().as_ref().map(plain_step), Some(step(first, 1, 1, up)));
        assert!(climb.step().is_none(), "the sides meet at the apex");
        assert!(climb.step().is_none(), "and the climb stays ended");
        let ends = climb.into_ends();
        assert_eq!([ends.end(first), ends.end(second)].map(usize::from), [3, 4]);
        assert_eq!(usize::from(ends.apex()), 0);
    }

    #[test]
    fn test_a_climb_left_unfinished_finishes_into_its_ends_by_hand() {
        let (first, second, up, down) = (Side::First, Side::Second, Direction::Up, Direction::Down);
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());

        // 6, of one node, moves first, to 5, of two; then the second end 2, of two as well, would move.
        let mut climb = tree.tree_mut().apex_climb(6, 2).expect("6 and 2 lie below the node count");
        assert_eq!(climb.step().as_ref().map(plain_step), Some(step(first, 6, 6, up)));
        let ends = climb.into_ends();
        assert_eq!((usize::from(ends.end(first)), usize::from(ends.end(second)), usize::from(ends.apex())), (6, 2, 7));

        // The walk of each side comes from the tree the ends hold, whatever the climb walked of it.
        assert_eq!(plain_side(ends.side(first)), [(6, up), (5, up)]);
        assert_eq!(plain_side(ends.side(second)), [(2, down), (1, up), (0, up)]);
    }

    #[test]
    fn test_an_end_above_the_other_and_a_loop_by_hand() {
        let (first, second, up, down) = (Side::First, Side::Second, Direction::Up, Direction::Down);
        let mut room = TreeRoom::new();
        let mut tree = plain_tree_of(&mut room, &seven_nodes());
        let nodes = tree.node_count();

        // 1 is an ancestor of 3: of the subtree sizes 3 and 1 the second moves, to 2, of two, then to 1 itself.
        let climb = tree.tree_mut().apex_climb(1, 3).expect("1 and 3 lie below the node count");
        assert_eq!(climbed(climb, nodes), (vec![step(second, 3, 3, up), step(second, 2, 2, down)], [1, 3, 1]));

        // A loop at 2: the apex is 2, and the climb takes no step.
        let climb = tree.tree_mut().apex_climb(2, 2).expect("2 lies below the node count");
        assert_eq!(climbed(climb, nodes), (vec![], [2, 2, 2]));
        let ends = tree.tree_mut().apex_climb(2, 2).expect("2 lies below the node count").into_ends();
        assert!(ends.side(first).next().is_none() && ends.side(second).next().is_none());
    }

    proptest! {
        #[test]
        fn test_apex_climb_and_its_unchecked_twin_agree(
            model in plain_trees(),
            ends in (any::<Index>(), any::<Index>()),
        ) {
            // The ends range over the hung nodes, the root and one past it, so that the checked form refuses some.
            let (first, second) = (ends.0.index(model.root() + 2), ends.1.index(model.root() + 2));
            let mut room = TreeRoom::new();
            let mut tree = tree_of(&mut room, &model, link_count_of(&model), &vec![0; link_count_of(&model)]);
            let nodes = tree.node_count();

            let checked = tree.tree_mut().apex_climb(first, second).map(|climb| climbed(climb, nodes));
            if first < nodes && second < nodes {
                debug_assert!(first < nodes && second < nodes, "both ends should be below the node count");
                // SAFETY: both ends are below the node count, as the branch just tested.
                let unchecked = climbed(unsafe { tree.tree_mut().apex_climb_unchecked(first, second) }, nodes);
                prop_assert_eq!(checked, Some(unchecked));
            } else {
                prop_assert_eq!(checked, None);
            }
        }

        #[test]
        fn test_link_and_its_unchecked_twin_agree(model in plain_trees(), raw in any::<Index>()) {
            let link_count = link_count_of(&model);
            let raw = raw.index(link_count + 3);
            let mut room = TreeRoom::new();
            let tree = tree_of(&mut room, &model, link_count, &vec![0; link_count]);

            let checked = tree.link(raw).map(usize::from);
            if raw < link_count {
                debug_assert!(raw < link_count, "the link should be below the link count");
                // SAFETY: the link is below the link count, as the branch just tested.
                let unchecked = usize::from(unsafe { tree.link_unchecked(raw) });
                prop_assert_eq!(checked, Some(unchecked));
                prop_assert_eq!(unchecked, raw);
            } else {
                prop_assert_eq!(checked, None);
            }
        }

        #[test]
        fn test_hung_and_its_unchecked_twin_agree(model in plain_trees()) {
            let mut room = TreeRoom::new();
            let mut tree = tree_of(&mut room, &model, link_count_of(&model), &vec![0; link_count_of(&model)]);
            let nodes = tree.node_count();
            let columns = tree.tree_mut().columns_mut();

            // Every tree node, the root last: the checked form refuses exactly the root.
            for index in 0..=nodes {
                let node = tree_node(u32::try_from(index).expect("a test's node should fit a u32"));
                let checked = columns.hung(node);
                if index < nodes {
                    debug_assert!(index < nodes, "the node should be below the tree's hung nodes");
                    // SAFETY: the node is below the tree's hung nodes, as the branch just tested.
                    let unchecked = unsafe { columns.hung_unchecked(node) };
                    prop_assert_eq!(checked, Some(unchecked));
                    prop_assert_eq!(usize::from(unchecked), index);
                } else {
                    prop_assert_eq!(checked, None);
                }
            }
        }
    }

    #[test]
    fn test_the_ids_convert_and_name_themselves() {
        let link = Link(9, PhantomData);

        assert_eq!((usize::from(TreeNode(3)), usize::from(HungNode(TreeNode(2))), usize::from(link)), (3, 2, 9));
        let names = format!("{:?} {:?} {link:?}", TreeNode(3), HungNode(TreeNode(2)));
        assert_eq!(names, "TreeNode 3 HungNode 2 Link 9");
    }

    #[test]
    fn test_a_direction_reversed_is_the_other() {
        assert_eq!(Direction::Up.reversed(), Direction::Down);
        assert_eq!(Direction::Down.reversed(), Direction::Up);
    }

    #[test]
    fn test_a_root_link_costs_nothing_up_and_the_root_cost_down_by_hand() {
        assert_eq!(root_link_cost(Direction::Up, 11), 0);
        assert_eq!(root_link_cost(Direction::Down, 11), 11);
        assert_eq!(const { root_link_cost(Direction::Down, i64::MAX) }, i64::MAX);
    }

    #[test]
    fn test_a_tight_potential_takes_the_cost_off_up_and_adds_it_down_by_hand() {
        // Up: the node is the tail, `7 + π − 10 = 0` at `π = 3`. Down: the node is the head, `7 + 10 − π = 0` at 17.
        assert_eq!(tight_potential(10, 7, Direction::Up), 3);
        assert_eq!(tight_potential(10, 7, Direction::Down), 17);
        assert_eq!(tight_potential(-4, -6, Direction::Up), 2);
        assert_eq!(tight_potential(-4, -6, Direction::Down), -10);

        // Below the root at zero, a root link makes its node's potential zero up and the root cost down.
        for (direction, potential) in [(Direction::Up, 0), (Direction::Down, 11)] {
            assert_eq!(tight_potential(0, root_link_cost(direction, 11), direction), potential);
        }

        assert_eq!(tight_potential(i64::MIN, 1, Direction::Up), i64::MAX, "wrapping");
    }

    #[test]
    fn test_a_nodes_root_link_follows_the_arcs_by_hand() {
        assert_eq!((root_link_of(4, 0), root_link_of(4, 2), root_link_of(0, 3)), (4, 6, 3));
    }

    #[test]
    fn test_stored_keeps_every_index_up_to_the_limit_by_hand() {
        assert_eq!((stored(0), stored(7), stored(INDEX_LIMIT)), (0, 7, u32::MAX));
    }
}
