# How it works

For a reader who knows linear programming and wants to know what a solve does, without reading the code first. The
method is the primal network simplex with a strongly feasible spanning-tree basis, block search for pricing, and a
threaded tree. The solver runs it from the star; a network runs the same method from the basis it holds, through loads
and changes. The notation is that of [the problem and the guarantees](problem-and-guarantees.md): $n$ nodes, $m$ arcs,
costs $c$, lower bounds $l$, capacities $u$, supplies $b$, potentials $\pi$.

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

The solver's solve starts from the star, always: every node hangs from the root by its own root link, and every arc
is out of the tree at its lower bound, carrying nothing above it. A network's load starts from the basis the network
holds where it serves, made a basis of the new problem as [a load from the basis held](#a-load-from-the-basis-held)
describes, and from the star otherwise.

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
cost. The work counts these pivots as `first_arc_pivots`. The arcs are chosen for the star of the solver; a network's
solve tries none.

## A network

A network holds one problem and, between any two calls, a strongly feasible basis of it, in the columns a solve pivots
on. A load makes a basis of the problem it hands in, and a change makes the basis held a basis of the problem as changed
before it returns, by the same rules a basis is made by: an arc out of the tree stands at a bound, a tree link carries
what it can toward the root, a node whose link cannot carry what arrives hangs from the root by its own root link, and
every potential is the one the tree implies. Its solve is then the run from that basis: the pivot, the ratio test and
[why the method ends](#why-the-method-ends) rely only on a strongly feasible basis, so they are not changed, and the
outcome and the optimal cost are those of the problem as it stands.

The solver and a network make a basis in the same room: the arc and link columns, the tree's columns, and each node's
net supply, which a start reads. The solver's room holds no basis between solves: each solve writes its start anew,
the star, from the problem it is handed, and nothing of a basis outlives it. A network's room holds the same columns,
in which it keeps the basis its last call left, and beside them each node's excess over that basis, which only a load
from it reads. It knows whether its columns hold a whole basis: every call that writes them records first that they
hold none, and records the basis again with its last write, so a call that stopped half way leaves a network that
holds none, and the next load starts from the star.

### The root cost of a network

A network's root links cost the root cost of the widest costs its node count admits, whatever its own costs are:

```math
M_N = (n - 1) \, B + 1 , \qquad B = \texttt{cost\_bound}(n) = \left\lfloor \frac{2^{62} - 1}{n} \right\rfloor .
```

Every cost a load or a change may set lies within $\pm B$, so $M_N$ is at least the root cost $(n - 1) C + 1$ of every
problem the network can come to hold, and no cost change asks for more. A change of the root cost would move the
potential of every node under a root link that points down; with $M_N$ fixed, no cost change moves it. Since $n B <
2^{62}$, the bounds of [the ranges](problem-and-guarantees.md#the-ranges-a-solve-admits) hold with $B$ in place of $C$:
every potential lies within $[-(n - 1) B, \; 2 (n - 1) B + 1]$, and every reduced cost and every partial sum a pivot
forms fits 64 bits. So a network's solve may pivot otherwise than the solver's solve of the same problem, and its
potentials may be other ones, but its outcome and its cost are the same.

### A load from the basis held

A load hands a network a whole problem. The network holds the basis its last call left, optimal or infeasible: the
tree, with each node's parent, link and direction and the thread, and where each arc out of the tree stands, at its
lower bound, at its capacity, or unpriced. The load starts from that basis where it serves the new problem: where the
problem has the node count and the arc count of the problem held and every arc the tail and the head it had, so that
every tree link joins the same two nodes as before, and where the excesses below keep their range. None of the old
numbers is read again; every number is read anew, and three passes make the kept tree a strongly feasible basis of the
new problem.

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
points up, plus it where it points down. An arc costs its cost, a root link zero up and $M_N$ down, the network's
[root cost](#the-root-cost-of-a-network); the root's potential is zero.

#### Why it is a basis

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

#### What it costs

The load is a few passes over the arcs, which compare their tails and heads and write their new numbers and the
excesses, and a few over the nodes, each node at a constant cost whatever a pass moves: $O(m + n)$, as the star's is.
What follows depends on how far the new problem lies from the old one. Where only some numbers changed, many links
often still stand, and the run often makes far fewer pivots than a run from the star; where much changed, the kept
tree may lie no nearer an optimum than the star, and the run may make more. The same problem again, where the kept
basis serves, sends every link the flow it carried: nothing violates, and no pivot is made. Where only costs changed,
the start keeps the tree and the flows, and only the potentials move.

Where the basis held cannot serve, or the network holds none, never loaded or left half changed by a panic, the load
starts from the star, at the network's root cost. Either way the load then writes the supplies and the lower bounds as
loaded, computes the [kept sums](#the-kept-sums) anew for the problem and the basis as they stand, and marks every
block, so that the solve after it prices every arc. That solve answers the same outcome and the same cost whichever
start the load took; the flow among tied optima, the potentials and the work may differ.

### A cost

A cost changed from $c_{ij}$ to $c_{ij} + \delta$ moves no flow and no state. Where the arc is out of the tree, only
its own reduced cost changes, by $\delta$, and the change costs a few steps. Where it is a link of the tree, it hangs
a node $v$ from its parent, and the link must be tight again: every potential of the subtree $T(v)$ moves by the same
amount,

```math
\pi_k \leftarrow \pi_k - \delta \quad (v = i,\ \text{the link points up from } v),
\qquad
\pi_k \leftarrow \pi_k + \delta \quad (v = j,\ \text{it points down to } v),
\qquad k \in T(v) ,
```

and no other. A link inside $T(v)$ has both ends moved alike and stays tight; the root lies outside it and stays at
zero. So the arc's own reduced cost changes, and those of the arcs with exactly one end in $T(v)$; no other does. It
costs the nodes of $T(v)$: for an arc near the root, most of the tree.

### A pair of supplies

`move_supply(s, t, a)` lowers $b_s$ by $a$ and raises $b_t$ by $a$: node $t$ has $a$ more to send toward the root, and
node $s$ has $a$ less. Each is sent from its node up the tree path to the apex, the deepest node the two paths share,
the amount $+a$ from $t$ and $-a$ from $s$, and what the two paths hand the apex is sent on from it toward the root.
`set_supply` is the move of the difference: node $v$'s supply set to $b$ against node $w$ is
`move_supply(w, v, b - b_v)` where $b > b_v$, and `move_supply(v, w, b_v - b)` where $b < b_v$.

At each node $v$ on a path, what arrives, $\Delta$, is taken by $v$'s link. With $f$ what the link sends toward the root
before, its flow above the lower bound where it points up from $v$ and the negation of that flow where it points down
to $v$, the node has $e = f + \Delta$ to send, and a link of width $w$ carries

```math
g = \min\bigl(\max(\pm e, \, 0), \; w\bigr) ,
```

with $+e$ where the link points up and $-e$ where it points down, as in [a load from the basis
held](#a-load-from-the-basis-held). The link stays where it carries the whole of $e$ and can still carry more
toward the root: $g < w$ pointing up, $g > 0$ pointing down. The parent then receives the change of what the link
sends toward the root, $\pm g - f$, and the walk goes on there; it ends where that change is zero. Where the tree
carries the amount, every link on both paths stays, the two paths hand the apex $+a$ and $-a$, and nothing goes on
from it: the move costs the two paths, and no potential moves.

### An arc's bounds

The basis knows an arc by its width $w = u - l$ and its flow above the lower bound $x' = x - l$. New bounds $l^*$ and
$u^*$ are restated without moving any flow: the arc keeps its flow $x$, so it carries

```math
\tilde x' = x' + (l - l^*) \quad \text{above its new lower bound, within the new width } w^* = u^* - l^* ,
```

which may lie outside $[0, w^*]$. Every node's net supply moves with the arc's lower bound, by as much as the flow above
it, so every node still conserves. Then:

- An arc out of the tree whose restated flow is $0$ or $w^*$ stays there. Any other goes to the bound its state names,
  carrying $0$ at its lower bound or $w^*$ at its capacity ($0$ where it had no width); what it then carries more than
  $\tilde x'$ leaves its head and arrives at its tail through the tree, as a pair of supplies is sent.
- A tree arc takes $\tilde x'$ as its node's link takes what arrives, with nothing more arriving: it stays where
  $\tilde x'$ lies within its new width with room toward the root, and is otherwise clamped, its node cut to the root
  as the next section says, and what its parent then has to send more is sent on toward the root.

Where the arc's flow can stay where it is, the change costs a few steps; otherwise it costs what a move of supply of
the flow it moves costs, a flow of at most the change of the bound.

### A link that cannot carry what arrives

Where $e$ lies outside what the link can carry, or the link could then carry no more toward the root, the link
leaves the tree at the bound its flow $g$ stands at, and its node hangs from the root by its own root link, which
carries the rest $r = e \mp g$: $\lvert r \rvert$, up where $r \ge 0$ and down where $r < 0$. This is the cut: the
node takes its subtree with it, and every potential of the subtree moves by the same amount, so that the root link is
tight, the node's potential $0$ under a root link that points up and $M_N$ under one that points down. The parent
receives the change of what the link sends toward the root, and the walk goes on there. A cut costs the nodes of the
subtree, whose potentials move, and the path from it to the root.

A node that hangs from the root takes what arrives on its root link, which then carries $\lvert e \rvert$, pointing
up where $e \ge 0$ and down where $e < 0$, and the walk ends there. Where the sign of $e$ is other than the direction
the link pointed, the root link turns: a part of the network comes to be short of flow, so that the root sends it
some, or over it, so that it sends some to the root. The turn moves every potential of the node's subtree by $M_N$,
down where the link turns up and up where it turns down, and costs every node of that subtree, which can be most of
the tree.

The root links carry flow after a load, which leaves on them what its start does not route, where a change could not be
carried by the tree since, and after a solve that found none; a solve that finds a flow leaves none on them. The root
conserves, so as much flows up the root links as down them. At the end of a solve, as at the end of [any
run](#the-starting-basis), the problem has a flow exactly where the root links carry none.

### The marks

A solve prices only the blocks of slots that a change, a load or a pivot marked: the blocks of $B$ consecutive slots
that [block search](#block-search) prices, one bit a block. Beside the bits a network keeps, for each node, the blocks
of the arcs that have an end at it.

What makes this enough is what an arc's violation, $\sigma_{ij} \bar c_{ij}$, reads: its own cost, its own state, and
the difference $\pi_i - \pi_j$ of its ends' potentials. Potentials only ever move a whole subtree at a time, all by
the same amount, so an arc's violation changes only with its own numbers or where exactly one of its ends lies in a
subtree whose potentials moved. Such an arc has one end on each side of the cut above that subtree, so it has an end
on the side with fewer nodes. Each change therefore marks:

- for a cost, the arc's block, and for a tree arc the arcs at every node of the smaller side of the cut above $T(v)$;
- for bounds, the arc's block, and what the flow it moves marks;
- for a move of supply, the arcs at every node of the smaller side of each cut whose potentials moved, below each link
  that could not carry the amount and under each root link that turned; where the tree carries the amount, nothing.

An arc that leaves the tree in a cut joins a node of a moved subtree to the rest, so it is marked with that side, or
it stands tight where no potential moved, and a tight arc does not violate. So after each change every arc that
violates lies in a marked block.

A solve searches the marked blocks in order, from the first and after each pivot from the block after the one it
entered from, enters the most violating arc of the first block that holds one, the first met among equals, and clears
every block it prices without finding one. A pivot moves the potentials of a subtree, and the arcs at either side of
it lie, under the [dealt order](#the-order-of-the-arcs), in nearly every block once that side holds about as many arc
ends as there are blocks; so each pivot marks every block, at the cost of a word for 64 of them, and the search goes
on as block search does. A solve ends where no block is marked, which proves that no arc violates. So a solve after
changes that call for no pivot prices only what they marked, and a solve that pivots prices at least one whole pass
over the arcs at its end.

### The kept sums

A network keeps two sums, so that a change is checked and a solve answers its cost without a pass over the arcs.

The kept cost $K$ is the cost of the basis's flow with the root links counted, a root link that points down at $M_N$
a unit and one that points up at nothing, kept modulo $2^{128}$:

```math
K \equiv \sum_{(i,j) \in A} c_{ij} \, x_{ij} + M_N \sum_{v \,:\, \text{link down}} x_{rv} \pmod{2^{128}} ,
```

where $x$ includes the lower bounds and $x_{rv}$ is the flow down node $v$'s root link. A change of a cost moves it
by $\delta \, x_{ij}$; a pivot lowers it by the flow it sends times the magnitude of the entering arc's violation; a
change that moves flow moves it by each link's cost after less its cost before. Where the root links carry nothing, the
cost of every flow within the bounds lies within $\pm \sum \lvert c_{ij} \rvert u_{ij} \le 2^{127} - 1$, so the
congruence is an equality and $K$ is the optimum's `total_cost`, answered at once.

The excess $E$ is the flow up the root links. A change that moves flow raises it by at most the flow it moves, and a
pivot never raises it, since a cycle up a root link that points up and down one that points down costs at least
$M_N - (n - 1) C \ge 1$ and never enters. A change is admitted only where

```math
E + a \le 2^{64} - 2 ,
```

with $a$ the flow it moves, which keeps every root link's flow within the limit
[the starting basis](#the-starting-basis) needs; the end of a solve sets $E$ to the flow up the root links as it
stands. Beside the two, the network keeps the cost total $\sum \lvert c_{ij} \rvert u_{ij}$, against which a cost or
a capacity is checked.

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

