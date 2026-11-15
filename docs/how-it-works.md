# How it works

For a reader who knows linear programming and wants to know what a solve does, without reading the code first. The
method is the primal network simplex with a strongly feasible spanning-tree basis, block search for pricing, and a
threaded tree; a resolve starts the same method from the basis the last solve ended on. The notation is that of [the
problem and the guarantees](problem-and-guarantees.md): $n$ nodes, $m$ arcs, costs $c$, lower bounds $l$, capacities
$u$, supplies $b$, potentials $\pi$.

## The problem the solve works on

Two transformations happen before the first pivot.

**Lower bounds are taken out.** Every arc starts at its lower bound, so the solve works on the flow above it,
$x'_{ij} = x_{ij} - l_{ij}$, within the arc's width $0 \le x'_{ij} \le u_{ij} - l_{ij}$. Each node's supply becomes its
net supply, what is left to route once every arc carries its lower bound:

```math
b'_i = b_i - \sum_{(i,j) \in A} l_{ij} + \sum_{(j,i) \in A} l_{ji} .
```

Here $b$ is `supplies` and $l$ is `lower_bounds`. When the flows are written out, each arc's lower bound is added
back.

**A root is added.** The network gets one more node, the root, and each node $v$ a root link that joins it to the
root. The basis is a spanning tree of these $n + 1$ nodes, and the root links make one exist for every problem.

## The starting basis

A solve starts from the star: every node hangs from the root by its own root link, and every arc is out of the tree
at its lower bound, carrying nothing above it. A resolve may start from the basis the last solve ended on instead,
made a basis of the new problem as [starting from the kept basis](#starting-from-the-kept-basis) describes.

- A node of net supply zero or more has a root link that points up, from the node to the root, at cost $0$, carrying
  its net supply.
- A node of negative net supply has a root link that points down, from the root to the node, at the root cost $M$,
  carrying its demand.

The root cost is the least value that a path of arcs can never undercut:

```math
M = (n - 1) \, C + 1 ,
```

where $C$ is the largest magnitude of a cost. A path between two nodes has at most $n - 1$ arcs, so its cost lies
strictly between $-M$ and $M$. Every root link has the capacity $2^{64} - 1$, and the range of the net supplies
ensures that none of them ever limits a pivot in the direction it points.

This start is feasible for the extended network, so the solve has a single phase. Routing a unit through the root
costs $M$, more than any path of arcs, so at the end a root link carries flow exactly where no flow of the problem
exists: that is how the solve decides infeasibility.

## The basis

A basis is a spanning tree of the $n + 1$ nodes, every arc out of the tree standing at one of its bounds, and a flow
that meets every net supply. The arcs out of the tree fix the flow on the tree links, since a tree has one way to
route what the arcs out of it leave.

The potentials are those the tree implies: the root's is zero, and every tree link is tight, its reduced cost zero.

```math
\bar c_{ij} = c_{ij} + \pi_i - \pi_j = 0 \quad \text{for every link } (i,j) \text{ of the tree.}
```

Here $c$ is `costs` and $\pi$ is `potentials`. An arc out of the tree violates its bound where moving it off that bound
would lower the cost: an arc at its lower bound with $\bar c_{ij} < 0$, or an arc at its capacity with
$\bar c_{ij} > 0$. The solve keeps each arc's state as its pricing sign $\sigma_{ij}$, $+1$ at the lower bound and
$-1$ at the capacity, so that one product decides:

```math
\text{the arc violates} \iff \sigma_{ij} \, \bar c_{ij} < 0 .
```

An arc in the tree, or an arc of no width, has the sign $0$ and never violates. Where no arc violates, the flow meets
the optimality conditions and is optimal.

The basis is strongly feasible: every tree link can carry more flow toward the root, a link that points up carries
less than its capacity, and a link that points down carries some flow. The leaving rule keeps it so, and that is what
makes the method end.

## A pivot

A pivot brings one violating arc into the basis.

1. **The cycle.** The entering arc and the tree path between its ends form a cycle. Flow is sent around it in the
   direction that moves the arc off its bound. The two ends climb toward their deepest common ancestor, the apex: at
   each step, the end whose subtree is the smaller moves to its parent.
2. **The ratio test.** On the same climb, each link's room in the direction of the cycle's flow is read: its capacity
   less its flow where the flow runs the way the link points, its flow otherwise. The step is the least room on the
   cycle, the entering arc's own width included.
3. **The leaving link.** Among the links of least room, the last one met when the cycle is walked from the apex in
   the direction of its flow leaves the tree; the entering arc is met in that walk between the two sides. This is the
   strongly feasible leaving rule. Where the last of least room met is the entering arc itself, no link leaves and
   the arc moves to its other bound: a flip.
4. **The flow update.** The step is sent around the cycle: along the entering arc, down one side of the cycle from
   the apex and up the other. A step of zero is a degenerate pivot; the flow stays as it was.
5. **The tree update.** The leaving link cuts a subtree off the tree. That subtree is hung under the far end of the
   entering arc, by the entering arc. Along the stem, the path from the entering arc's end up to the node whose link
   left, each node now hangs from the node that was below it; every other node keeps its parent.
6. **The potentials.** Only the moved subtree's potentials change, all by the same amount, the entering arc's reduced
   cost or its negation, so that the entering arc becomes tight. Nothing outside the subtree is touched.

A pivot costs the length of the cycle for steps 1 to 4, and the size of the moved subtree and the paths it touches for
steps 5 and 6. No step passes over the whole tree.

## Why the method ends

Costs and widths are fixed for the solve, so the potentials are a function of the tree. Each pivot is of one of three
kinds:

- A pivot that sends flow lowers the cost of the flow, root links counted at their costs, by the step times the
  magnitude of the entering arc's reduced cost: at least one, since both are whole numbers. The cost is an integer
  bounded below, so there are finitely many such pivots.
- A degenerate pivot has a leaving link of room zero. Strong feasibility gives every link on the side where the flow
  runs toward the root room above zero, and the entering arc has width above zero, so the leaving link lies on the
  other side, and the moved subtree's potentials rise strictly. Within a run of degenerate pivots the sum of the
  potentials rises at every pivot, so no tree repeats, and there are finitely many trees.
- A flip sends the arc's whole width, which is above zero: a pivot of the first kind.

An arc whose width is zero is never priced. It carries its lower bound in every flow within the bounds, it would only
ever enter to send nothing, and it meets the optimality conditions whatever its reduced cost. The argument uses no
property of the rule that chooses the entering arc, so it holds for every pivot of a solve.

## Starting from the kept basis

A solver keeps the basis its last run ended on, optimal or infeasible: the tree, with each node's parent, link and
direction and the thread, and where each arc out of the tree stood, at its lower bound, at its capacity, or unpriced.
None of the last problem's numbers is read again. A resolve starts from that basis where it can serve the new problem,
as [solving again](problem-and-guarantees.md#solving-again) states; among the conditions, every arc has the tail and the
head it had, so that every tree link joins the same two nodes as before. Then every number is read anew, and three
passes make the kept tree a strongly feasible basis of the new problem.

**The arcs out of the tree.** Each arc out of the tree keeps its bound: one that stood at its capacity carries its new
width, $u_{ij} - l_{ij}$, and any other carries nothing above its lower bound; an arc whose new width is zero is
unpriced. What each node then has left to route along the tree is its excess,

```math
e_i = b'_i - \sum_{(i,j) \in U} (u_{ij} - l_{ij}) + \sum_{(j,i) \in U} (u_{ji} - l_{ji}) ,
```

where $b'$ is the net supply and $U$ the set of arcs out of the tree that stand at their capacity. In the star every
arc stands at its lower bound, so there an excess is a net supply. The excesses must keep the range the net supplies
keep for the star: where they are positive they add up to at most $2^{64} - 2$.

**The tree's flows, from the leaves up.** The nodes are taken in the reverse of the thread's order, so that each comes
after its whole subtree, and each sends its excess, with what its children sent it, toward the root along its link. A
link of width $w$ carries

```math
g = \min\bigl(\max(\pm e, \, 0), \; w\bigr) ,
```

with $+e$ where the link points up from the node and $-e$ where it points down to it, and the parent's excess gains $g$
or $-g$ accordingly; the rest $r = e \mp g$ stays at the node. The link stays in the tree where nothing is left, $r =
0$, and where it can still carry more toward the root: $g < w$ pointing up, $g > 0$ pointing down. Otherwise the arc
leaves the tree at the bound its flow stands at, or unpriced where it has no width, and the node hangs from the root by
its own root link, which carries $\lvert r \rvert$, up where $r \ge 0$ and down where $r < 0$. A node that already hangs
from the root puts its whole excess on its root link the same way. A node that leaves its arc takes its subtree with it:
the subtree's run of the thread moves to the thread's end, where only nodes already passed stand, so the order of the
pass is undisturbed, and the subtree sizes and last successors are rebuilt in the same pass.

**The potentials, from the root down.** The nodes are taken in the thread's order, so that each comes after its
parent, and each takes the potential that makes its link tight: its parent's less the link's cost where the link
points up, plus it where it points down. An arc costs its cost, a root link zero up and $M$ down, with $M$ the root
cost of the new problem; the root's potential is zero.

### Why it is a basis

The result meets every condition the star meets:

- every node conserves its net supply, since it sends $\pm g$ to its parent and keeps $r$ on its root link; every arc
  carries a flow within its width, and an arc that leaves the tree stands at a bound, since it leaves only where the
  clamp met one or where its flow left it no room toward the root;
- the basis is strongly feasible: a link stays only with room toward the root, a root link that points down carries
  $\lvert r \rvert > 0$, and one that points up carries less than its capacity, by the bound below;
- every tree link is tight, so the potentials are those the tree implies.

The root links carry at most $P$, the sum of the positive excesses. Take the sum of the positive parts of the excesses
not yet sent and of the rests already placed: a node with $e \ge 0$ splits it into $r \ge 0$ and a transfer $g \ge 0$
that raises its parent's positive part by at most $g$, and a node with $e < 0$ only lowers its parent's excess. So the
sum never rises, and the flow up the root links is at most $P$; since the excesses add up to zero, so is the flow down
them. With $P \le 2^{64} - 2$ the argument that keeps the star's root links from limiting a pivot in the direction they
point holds as it does for the star, and every number of the passes fits the integer type it is held in.

From here the run is the run from the star. The pivot, the ratio test, the tree update, the pricing and [why the
method ends](#why-the-method-ends) rely only on a strongly feasible basis, not on how it was reached, so they are not
changed, and the outcome and the optimal cost are those of the problem. The star itself is the case of a tree in which
every node hangs from the root and every arc stands at its lower bound.

### What it costs

The start is a few passes over the arcs, which compare their tails and heads and write their new numbers and the
excesses, and a few over the nodes, each node at a constant cost whatever a pass moves: $O(m + n)$, as the star's is.
What follows depends on how far the new problem lies from the old one. Where only some numbers changed, many links
often still stand, and the run often makes far fewer pivots than a run from the star; where much changed, the kept
tree may lie no nearer an optimum than the star, and the run may make more. The same problem again, where the kept
basis serves, sends every link the flow it carried: nothing violates, and no pivot is made. Where only costs changed,
the start keeps the tree and the flows, and only the potentials move.

Where the kept basis cannot serve, the star is taken, and a resolve is a solve, its answer and its work included.

## Choosing the entering arc

### The order of the arcs

The pricing scans the arcs in a dealt order rather than in the order given. With the stride

```math
s = \max\left(1, \left\lceil \frac{m}{n} \right\rceil\right) ,
```

arc 0 goes to slot 0, the next arcs to slots $s, 2s, \dots$ below $m$, then to $1, 1 + s, \dots$, and so on. Problems
often list each node's arcs together, about $m / n$ of them a node; with the stride, consecutive slots hold arcs at
least that far apart, so a block of slots samples the whole network rather than one corner of it. A stride of one
keeps the given order.

### Block search

The slots are priced in blocks of

```math
B = \max\left(1, \left\lfloor \sqrt{m} \right\rfloor\right)
```

consecutive slots. A search starts at the slot where the last search stopped, wraps around once, and stops at the end
of the first block in which some arc violated; it enters the most violating arc it has priced, the first met among
equals. Where no arc violates, the search prices every slot once and the solve ends. A block of one slot would enter
the first violating arc, a block of all slots the most violating one; $\sqrt{m}$ is the geometric mean of 1 and
$m$.

### The first pivots

Before block search starts, the solve tries a few arcs chosen from the problem's structure, at most one per node:

- with exactly one node of positive net supply and one of negative, the arcs by which a search backwards from the
  demand node, over arcs wide enough to carry the whole supply, first reaches each node, until it reaches the supply
  node: entered in turn, a path of them routes the supply at once;
- otherwise, the cheapest arc that enters each demand node, the lowest arc among equals.

Only arcs of positive width that are not loops are chosen. Each chosen arc enters only where it violates when its turn
comes, so the choice changes the path a solve takes, never whether it ends optimal or infeasible, nor the optimal
cost. The work counts these pivots as `first_arc_pivots`. The arcs are chosen for the star; a start from a kept basis
tries none.

## The threaded tree

The tree is stored as columns indexed by node, the root last, so that the apex of a cycle is found and a subtree moved
at the cost of the paths they touch.

| Column | Per | What it holds | What it is for |
|---|---|---|---|
| parent | tree node | the node it hangs from; the root's is itself | the climb to the apex, the stem |
| parent link | node | its link: an arc's slot, or $m + v$ for node $v$'s root link | the ratio test, the flow update |
| direction | node | whether that link points up to the parent or down from it | the room of a link, the flow update |
| thread | tree node | the next node in a preorder of the tree | walking a subtree |
| reverse thread | tree node | the node before it in that preorder | cutting a subtree out |
| last successor | tree node | the last node of its subtree in the preorder | where a subtree's run ends |
| subtree size | tree node | the number of nodes in its subtree | choosing which end of a cycle climbs |
| potential | tree node | its potential; the root's is zero | pricing, and the shift of a moved subtree |

In the preorder each subtree is one contiguous run that starts at its top node and ends at its last successor. The
tree update cuts the moved subtree's run out of the thread, rethreads the stem, and splices the run in after its new
parent; the sizes change only on the paths from the two ends of the cycle up to the apex, and the last successors only
where a run ended where the subtree left or arrived.

## The layout of the columns

Every column the solve reads in a loop is laid out for the way that loop reads it.

- **The priced columns are in slot order.** For each slot the pricing reads the arc's two ends, stored together as
  two 32-bit indices, its cost, a 64-bit integer, and its state, one byte: 17 bytes, in three columns read in
  sequence. The only reads out of sequence are the two potentials of the arc's ends.
- **One index space for arcs and links.** The capacities and the flows are columns of $m + n$ entries: the arcs at
  their slots, then each node's root link. A tree node's parent link is then the index of its flow and its capacity,
  so the ratio test and the flow update read them without translating an index.
- **Widths, not two bounds.** Since the lower bounds are taken out, a link's room is its capacity less its flow, or
  its flow, depending on the direction the cycle's flow runs on it. Both are formed and one is selected, so the ratio
  test does not branch on a direction that is up as often as down.
- **One potential column.** The shift of a moved subtree is one read and one write per node.
- **Narrow indices.** Every index the tree and the arc columns store, of a node, a slot or a link, has 32 bits.
  The walks up the tree read one stored index per node, each read depending on the one before, so their speed is
  that of the cache the columns fit in, and a narrow index halves the bytes. The range checks keep every index below
  $2^{32}$.
- **Columns, not records.** Each quantity is its own column, so the one-byte state pads nothing and each loop reads
  only the columns it needs.

The caller's order is kept at the edge: the flows are written in arc order, read back from the slots, and the
potentials are answered in node order, since nodes are never renumbered.
