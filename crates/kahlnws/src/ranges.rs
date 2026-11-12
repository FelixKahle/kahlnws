// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The bounds a problem keeps so that the solve's arithmetic is exact, the argument that they are enough, and the
//! width an index is stored at.
//!
//! A problem of `n` nodes and `m` arcs, whose costs have the largest magnitude `C`, is admitted when
//!
//! 1. `n + 1` and `m + n` are at most [`INDEX_LIMIT`];
//! 2. every cost lies within plus or minus [`cost_bound`] of `n`, so that `n·C < 2^62`;
//! 3. every lower bound is at most its capacity;
//! 4. the magnitudes of the costs times the capacities add up to at most `i128::MAX` ([`cost_total_with`]);
//! 5. the supplies add up to zero;
//! 6. the supplies net of the lower bounds add up to at most [`NET_SUPPLY_LIMIT`] where they are positive.
//!
//! Within them every number the solve forms fits the type it is held in, so the solve adds and subtracts with no
//! check: each item below carries the argument for the numbers it bounds.

/// The largest magnitude a cost may have in a problem of `node_count` nodes: every cost of a problem a solve admits
/// lies within plus or minus it.
///
/// It is `⌊(2^62 − 1) / node_count⌋`, so `node_count` times it stays below `2^62`; for a problem of no node it is
/// `2^62 − 1`. Within it, every potential, reduced cost and path cost a solve forms fits an `i64`.
///
/// ```
/// use kahlnws::cost_bound;
///
/// assert_eq!(cost_bound(1), (1 << 62) - 1);
/// assert_eq!(cost_bound(1000), 4_611_686_018_427_387);
/// ```
pub const fn cost_bound(node_count: usize) -> i64 {
    // Why `n·C < 2^62` is enough, with `M = (n − 1)·C + 1` the root cost:
    //
    // - A potential is the one the tree implies: its component's top sits at zero below a root link that points up,
    //   or at `M` below one that points down, and a node below it is a path of at most `n − 1` arcs further. So every
    //   potential lies within `[−(n − 1)·C, M + (n − 1)·C]`, and `M + (n − 1)·C = 2(n − 1)·C + 1 < 2^63`.
    // - A reduced cost `cost + potential[tail] − potential[head]` of an arc whose ends hang in one component is its
    //   cost plus the cost of the tree path between its ends, at most `n − 1` arcs: within `±n·C`. Where the ends hang
    //   in two components, their paths from the two tops have at most `n − 2` arcs together and the tops differ by at
    //   most `M`: within `±(M + (n − 1)·C) = ±(2(n − 1)·C + 1)`. Both are below `2^63`.
    // - The partial sum `cost + potential[tail]` lies within `[−n·C, (2n − 1)·C + 1]`, and `(2n − 1)·C + 1 < 2^63`.
    // - A violation is a reduced cost times −1, 0 or 1; the shift of a pivot is a reduced cost or its negation, and
    //   the potentials after it are again the ones the tree implies.
    //
    // So the solve adds and subtracts costs and potentials in `i64` with no check.
    const BELOW: u64 = (1 << 62) - 1;

    #[expect(
        clippy::as_conversions,
        reason = "a `u64` holds every `usize` on the targets the crate builds for, which the assertion at the end of \
                  this file checks, and no lossless conversion is a `const fn`"
    )]
    let nodes = node_count as u64;

    // No node carries no arc, so any bound would do; the one of a single node is the largest.
    let bound = match BELOW.checked_div(nodes) {
        Some(bound) => bound,
        None => BELOW,
    };

    // At most `2^62 − 1`, so the signed value is the same number.
    bound.cast_signed()
}

/// The most the supplies net of the lower bounds may add up to where they are positive, in units of flow:
/// `2^64 − 2`. Within it no root link ever carries more than the positive total, and a root link never limits a
/// pivot in the direction it points.
// The star starts the flow up the root links and the flow down them at the positive total `T` each. The root
// conserves, so a pivot raises both totals or neither; a pivot that raises both runs up a supply link at cost zero
// and down a demand link at cost `M`, a cycle of cost at least `M − (n − 1)·C = 1`, which never enters. So no
// root-link flow passes `T`.
//
// A cycle through the root passes two root links. Running along both is the pivot that never enters; running against
// both blocks only at flows. Otherwise it runs against one, of flow `f₁`, and along the other, of flow `f₂`, which
// happens exactly when both point up or both point down, so `f₁ + f₂ ≤ T ≤ 2^64 − 2`. The step is at most `f₁`, so
// the room along the second is `u64::MAX − f₂ ≥ u64::MAX − (T − f₁) ≥ 1 + f₁ > step`. The inequality is strict, and
// must be: with a tie, the ratio test's non-strict choice on the second side would take out a root link that still
// carries flow. Hence no root-link flow overflows a `u64`, and a root link out of the tree carries nothing.
pub(crate) const NET_SUPPLY_LIMIT: u64 = u64::MAX - 1;

/// The most tree nodes and the most links a problem may have: `u32::MAX`, what a stored index counts. A problem of
/// `n` nodes and `m` arcs has `n + 1` tree nodes, the root among them, and `m + n` links, an arc's at its slot and a
/// root link per node.
// With `n + 1 ≤ u32::MAX` and `m + n ≤ u32::MAX`, every tree node, slot and link is below `u32::MAX` and fits the 32
// bits an index is stored at; so does the size of a subtree, at most `n + 1`.
pub(crate) const INDEX_LIMIT: usize = widen(u32::MAX);

/// The stored index `raw` at the native width, exactly.
pub(crate) const fn widen(raw: u32) -> usize {
    #[expect(
        clippy::as_conversions,
        reason = "a `usize` holds every `u32` on the targets the crate builds for, which the assertion at the end of \
                  this file checks, and std offers no lossless conversion"
    )]
    let index = raw as usize;

    index
}

/// `index` at the 32 bits a column stores an index at, exactly: the one narrowing of the cold passes, which write the
/// columns.
///
/// # Panics
/// Panics if `index` passes `u32::MAX`, which no node, slot or link of a problem within [`INDEX_LIMIT`] does.
#[track_caller]
pub(crate) fn narrow(index: usize) -> u32 {
    match u32::try_from(index) {
        Ok(raw) => raw,
        Err(_) => narrow_past_u32(index),
    }
}

/// Panics for an `index` past `u32::MAX`, as [`narrow`] documents.
///
/// # Panics
/// Always: that is its job.
#[cold]
#[inline(never)]
#[track_caller]
fn narrow_past_u32(index: usize) -> ! {
    panic!("called `narrow` with {index}, past u32::MAX")
}

/// Panics for a `root_cost` that passes `i64::MAX`, as [`root_cost`] documents.
///
/// # Panics
/// Always: that is its job.
#[cold]
#[inline(never)]
#[track_caller]
fn root_cost_past_i64_max(node_count: usize, largest_cost: u64) -> ! {
    panic!(
        "called `root_cost` with {node_count} nodes and a largest cost of {largest_cost}, whose root cost passes \
         i64::MAX"
    )
}

/// The cost of a root link that points down, `M = (n − 1)·C + 1`, in a problem of `node_count` nodes whose costs
/// have the largest magnitude `largest_cost`: above the magnitude of the cost of every path of at most `n − 1` arcs,
/// and at most `2^62` for costs within [`cost_bound`].
///
/// # Panics
/// Panics if it passes `i64::MAX`, which costs within [`cost_bound`] rule out.
#[track_caller]
pub(crate) fn root_cost(node_count: usize, largest_cost: u64) -> i64 {
    // `(n − 1)·C + 1 ≤ n·C < 2^62` where `C ≥ 1`, and it is 1 where `C = 0`.
    //
    // It is the least value with `M − (n − 1)·C ≥ 1`. So a cycle that runs up a supply link and down a demand link,
    // costing `M` less a path of at most `n − 1` arcs, never enters; and at the end a root link carries flow exactly
    // where no flow within the bounds meets the supplies, since a cycle that would route such a supply costs at most
    // `−M + (n − 1)·C = −1`.
    let root_cost = u64::try_from(node_count.saturating_sub(1))
        .ok()
        .and_then(|paths| paths.checked_mul(largest_cost))
        .and_then(|path_cost| path_cost.checked_add(1))
        .and_then(|cost| i64::try_from(cost).ok());

    match root_cost {
        Some(cost) => cost,
        None => root_cost_past_i64_max(node_count, largest_cost),
    }
}

/// `total` plus the magnitude of `cost` times `capacity`; `None` past `i128::MAX`. A running total of the arcs that
/// stays `Some` bounds the cost of every flow within their capacities, and of every partial sum of it, by
/// `i128::MAX` in magnitude.
pub(crate) fn cost_total_with(total: u128, cost: i64, capacity: u64) -> Option<u128> {
    // A cost's magnitude is at most `2^63` and a capacity below `2^64`, so the term is below `2^127` and fits a
    // `u128`; only the sum can pass. A flow's cost is a sum of `cost · flow` with `0 ≤ flow ≤ capacity`, each within
    // its term in magnitude, so every partial sum lies within the total.
    let term = u128::from(cost.unsigned_abs()) * u128::from(capacity);

    total.checked_add(term).filter(|&sum| sum <= i128::MAX.cast_unsigned())
}

// The two `as` conversions above are exact only where a `usize` has 32 to 64 bits.
const _: () = assert!(u32::BITS <= usize::BITS && usize::BITS <= u64::BITS, "a usize should have 32 to 64 bits");

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;

    use super::*;

    /// Two to the power `exponent`, in big integers.
    fn power_of_two(exponent: u32) -> BigInt {
        BigInt::from(1_u8) << exponent
    }

    /// Node counts across the range: the smallest, a few small ones, the largest a problem has, and beyond, on a
    /// `usize` of either width.
    const NODE_COUNTS: [usize; 9] = [1, 2, 3, 7, 1000, 1 << 20, INDEX_LIMIT - 1, usize::MAX / 2, usize::MAX];

    #[test]
    fn test_cost_bound_is_the_largest_cost_with_nodes_times_it_below_2_to_the_62() {
        let below = power_of_two(62) - 1;

        for node_count in NODE_COUNTS {
            let (nodes, bound) = (BigInt::from(node_count), BigInt::from(cost_bound(node_count)));
            assert!(bound >= BigInt::from(0_u8), "{node_count} nodes");
            assert!(&nodes * &bound <= below, "{node_count} nodes");
            assert!(&nodes * (&bound + 1) > below, "{node_count} nodes: one more passes");
        }

        assert_eq!(cost_bound(0), (1 << 62) - 1, "no node");
        assert_eq!(cost_bound(4), 1_152_921_504_606_846_975);
    }

    #[test]
    fn test_potentials_and_reduced_costs_within_the_cost_bound_fit_an_i64() {
        // The widest numbers the argument beside `cost_bound` names, recomputed in big integers at the bound itself.
        let (least, most) = (BigInt::from(i64::MIN), BigInt::from(i64::MAX));

        for node_count in NODE_COUNTS {
            let (n, c) = (BigInt::from(node_count), BigInt::from(cost_bound(node_count)));
            let (one, two) = (BigInt::from(1_u8), BigInt::from(2_u8));
            let root = (&n - &one) * &c + &one;
            let highest_potential = &root + (&n - &one) * &c;
            let lowest_potential = -((&n - &one) * &c);
            let across = &root + (&n - &one) * &c;
            let within = &n * &c;
            let partial_high = (&n * &two - &one) * &c + &one;
            let partial_low = -(&n * &c);

            for number in [&root, &highest_potential, &across, &within, &partial_high] {
                assert!(number <= &most && -number >= least, "{node_count} nodes: {number}");
            }
            assert!(lowest_potential >= least && partial_low >= least, "{node_count} nodes");
        }
    }

    #[test]
    fn test_root_cost_by_hand() {
        assert_eq!(root_cost(3, 5), 11);
        assert_eq!(root_cost(1, 7), 1, "one node has no path");
        assert_eq!(root_cost(0, 0), 1, "no node");
        assert_eq!(root_cost(4, 0), 1, "costs of zero");
    }

    #[test]
    fn test_root_cost_at_the_cost_bound_against_big_integers() {
        for node_count in NODE_COUNTS.into_iter().filter(|&nodes| nodes <= INDEX_LIMIT) {
            let largest = cost_bound(node_count).unsigned_abs();
            let expected = (BigInt::from(node_count) - 1) * BigInt::from(largest) + 1;

            assert_eq!(BigInt::from(root_cost(node_count, largest)), expected, "{node_count} nodes");
            assert!(expected <= power_of_two(62), "{node_count} nodes");
        }
    }

    #[test]
    fn test_root_cost_reaches_i64_max_exactly() {
        // Two nodes and a cost of `i64::MAX − 1`, far past the bound: `(2 − 1)·(2^63 − 2) + 1 = 2^63 − 1`.
        let largest = u64::try_from(BigInt::from(i64::MAX) - 1).expect("2^63 − 2 should fit a u64");

        assert_eq!(root_cost(2, largest), i64::MAX);
    }

    #[test]
    #[should_panic(
        expected = "called `root_cost` with 2 nodes and a largest cost of 9223372036854775807, whose root cost passes \
                    i64::MAX"
    )]
    fn test_root_cost_one_past_i64_max_panics() {
        let _ = root_cost(2, i64::MAX.unsigned_abs());
    }

    #[test]
    #[should_panic(expected = "called `root_cost` with 3 nodes and a largest cost of 18446744073709551615")]
    fn test_root_cost_past_u64_panics() {
        let _ = root_cost(3, u64::MAX);
    }

    #[test]
    fn test_cost_total_with_adds_the_magnitude_times_the_capacity_by_hand() {
        assert_eq!(cost_total_with(5, -3, 4), Some(17));
        assert_eq!(cost_total_with(5, 3, 4), Some(17));
        assert_eq!(cost_total_with(0, i64::MIN, 0), Some(0));
        assert_eq!(cost_total_with(0, i64::MIN, u64::MAX), Some((1 << 127) - (1 << 63)));
    }

    #[test]
    fn test_cost_total_with_stops_exactly_past_i128_max() {
        let max = i128::MAX.cast_unsigned();

        assert_eq!(cost_total_with(max - 3, 3, 1), Some(max));
        assert_eq!(cost_total_with(max - 2, -3, 1), None);
        assert_eq!(cost_total_with(max, 0, u64::MAX), Some(max));
        assert_eq!(cost_total_with(u128::MAX, 1, 1), None, "past a u128");
    }

    #[test]
    fn test_the_running_cost_total_at_its_edge_against_big_integers() {
        // Arcs of cost `±cost_bound(2)` at capacity `u64::MAX`, as many as fit under `i128::MAX`, then one whose
        // capacity brings the total to the edge, all recomputed in big integers; then that one a unit wider.
        let cost = cost_bound(2);
        let (max, unit) = (BigInt::from(i128::MAX), BigInt::from(cost));
        let full = BigInt::from(u64::MAX) * &unit;
        let whole = &max / &full;
        let count = usize::try_from(&whole).expect("a handful of full arcs should fit a usize");
        let rest = u64::try_from((&max - &whole * &full) / &unit).expect("the rest should lie below a full arc");
        let edge = &whole * &full + BigInt::from(rest) * &unit;
        assert!(edge <= max && &edge + &unit > max, "the last capacity should bring the total to the edge");

        let mut total = 0_u128;
        for at in 0..count {
            let signed = if at % 2 == 0 { cost } else { -cost };
            total = cost_total_with(total, signed, u64::MAX).expect("a full arc below the edge should fit");
        }

        assert_eq!(cost_total_with(total, cost, rest).map(BigInt::from), Some(edge));
        assert_eq!(cost_total_with(total, -cost, rest + 1), None);
    }

    #[test]
    fn test_narrow_keeps_every_index_up_to_the_limit_by_hand() {
        assert_eq!((narrow(0), narrow(7), narrow(INDEX_LIMIT)), (0, 7, u32::MAX));
    }

    // An index past `u32::MAX` exists only where a `usize` has 64 bits.
    #[cfg(target_pointer_width = "64")]
    #[test]
    #[should_panic(expected = "called `narrow` with 4294967296, past u32::MAX")]
    fn test_narrow_refuses_an_index_past_u32_max() {
        let _ = narrow(INDEX_LIMIT + 1);
    }

    #[test]
    fn test_the_limits_by_hand() {
        assert_eq!(BigInt::from(NET_SUPPLY_LIMIT), power_of_two(64) - 2);
        assert_eq!(BigInt::from(INDEX_LIMIT), power_of_two(32) - 1);
        assert_eq!(widen(7), 7);
        assert_eq!(BigInt::from(widen(u32::MAX)), power_of_two(32) - 1);
    }
}
