// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The seeded networks the benchmark suites share: each family at the sizes its rows sweep, every network a pure
//! function of its family and size, and the tie-breaks its distinct costs are made with.
//!
//! Each kind of number of a network is drawn from a substream split off the suites' seed by the network's family, its
//! size and the kind of number, never by the position of a draw, so no network depends on which others exist or are
//! drawn first. A split mixes one step under one domain with one round of SplitMix64, so a family, a size and a kind of
//! number never meet on one seed.

use rand::{Rng, SeedableRng};
use rand_xoshiro::{SplitMix64, Xoshiro256PlusPlus};

use crate::mincost_network::MincostNetwork;
use crate::sized_networks::{ShapeDraws, assignment_sized, costed_grid, supply_sized};

/// The seed every substream of the benchmark suites is split from.
const SUITE: Seed = Seed(0x6b61_686c_6172_6320);

/// The number of tie-breaks: they are `1..=4095`, below `2^12`.
const TIE_BREAK_MOST: u64 = (1 << 12) - 1;

/// A family of the benchmark suites' min-cost networks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    /// The supply shape, [`supply_sized`]; the size is the number of demands.
    Supply,
    /// The assignment shape, [`assignment_sized`]; the size is the number of demands.
    Assignment,
    /// The square grid with costs, [`costed_grid`]; the size is the side.
    Grid,
}

impl Family {
    /// Every family, in the order the rows report them.
    pub const ALL: [Self; 3] = [Self::Supply, Self::Assignment, Self::Grid];

    /// The sizes the rows sweep, ascending: the supply and the assignment shape at about 250, 1000 and 4000 arcs, the
    /// grid from about 16 to about 256,000 arcs, each side about twice the last.
    pub const fn sizes(self) -> &'static [usize] {
        match self {
            Self::Supply => &[20, 80, 300],
            Self::Assignment => &[32, 128, 512],
            Self::Grid => &[2, 5, 9, 18, 37, 74, 148, 296],
        }
    }

    /// The family's network of `size`, at its own costs: small ones, at which the supply and the assignment shape tie
    /// through their twin arcs and the grid by the few values its costs take.
    ///
    /// # Panics
    /// Panics if `size` is zero.
    pub fn network(self, size: usize) -> MincostNetwork {
        self.network_with_costs(size, self.seed(size).stream(Stream::Costs))
    }

    /// The family's network of `size` with every cost drawn by `costs` in place of the suite's costs stream: the nodes,
    /// arcs, capacities and supplies of [`network`](Self::network), and each arc's cost drawn from the range its role
    /// in the family draws from, or given by the rule a derived cost follows, such as the grid's slack arc.
    ///
    /// # Panics
    /// Panics if `size` is zero.
    pub fn network_with_costs(self, size: usize, mut costs: Xoshiro256PlusPlus) -> MincostNetwork {
        let seed = self.seed(size);
        match self {
            Self::Supply => supply_sized(size, &mut shape_draws(seed, costs)),
            Self::Assignment => assignment_sized(size, &mut shape_draws(seed, costs)),
            Self::Grid => costed_grid(size, &mut seed.stream(Stream::Capacities), &mut costs),
        }
    }

    /// The seed every stream of the family's network of `size` is split from.
    fn seed(self, size: usize) -> Seed {
        SUITE.split(self.part()).sized(size)
    }

    /// The part this family mixes into the suite's seed; it never changes once given.
    const fn part(self) -> u64 {
        match self {
            Self::Grid => 7,
            Self::Supply => 8,
            Self::Assignment => 9,
        }
    }
}

/// The tie-breaks of the arcs `0..arc_count` with which [`distinct_costs`](crate::distinct_costs) makes the costs of
/// the benchmark suites' distinct rows: arc `a`'s is `1 + splitmix64(a) mod 4095`, where `splitmix64(a)` is the first
/// output of SplitMix64 seeded with `a`. Each lies in `1..=4095`, and neighbouring arcs get spread values.
///
/// # Panics
/// Panics if an arc index does not fit 64 bits, which no index of a machine's memory passes.
pub fn suite_tie_breaks(arc_count: usize) -> Vec<u64> {
    (0..arc_count)
        .map(|arc| {
            let index = u64::try_from(arc).expect("an arc index should fit a u64");
            1 + SplitMix64::seed_from_u64(index).next_u64() % TIE_BREAK_MOST
        })
        .collect()
}

/// What a substream of a network's seed is drawn for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Stream {
    /// Which nodes and arcs a network has.
    Topology,
    /// The capacities of its arcs.
    Capacities,
    /// The costs of its arcs.
    Costs,
    /// The supplies of its nodes.
    Supplies,
}

impl Stream {
    /// The part this stream mixes into a network's seed; it never changes once given.
    const fn part(self) -> u64 {
        match self {
            Self::Topology => 1,
            Self::Capacities => 2,
            Self::Costs => 5,
            Self::Supplies => 6,
        }
    }
}

/// The generators of a network of the supply or the assignment shape from its `seed`, one substream per kind of
/// number, with `costs` drawing its costs.
fn shape_draws(seed: Seed, costs: Xoshiro256PlusPlus) -> ShapeDraws {
    ShapeDraws { topology: seed.stream(Stream::Topology), supplies: seed.stream(Stream::Supplies), costs }
}

/// The kind of step a seed is split by; each is mixed under its own domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Domain {
    /// A named part: a family or a stream.
    Part,
    /// A size.
    Size,
}

impl Domain {
    /// The number this domain mixes in before its step.
    const fn tag(self) -> u64 {
        match self {
            Self::Part => 1,
            Self::Size => 2,
        }
    }
}

/// The seed of one substream of the suite's randomness: two different paths of splits give independent streams for
/// any practical purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Seed(u64);

impl Seed {
    /// The seed of the substream `part` of this one.
    fn split(self, part: u64) -> Self {
        self.mixed(Domain::Part, part)
    }

    /// The seed of the substream of this one for a network of `size`.
    ///
    /// # Panics
    /// Panics if `size` does not fit 64 bits, which no network comes near.
    fn sized(self, size: usize) -> Self {
        self.mixed(Domain::Size, u64::try_from(size).expect("a size should fit a u64"))
    }

    /// This seed mixed first with `domain`, then with `step`, each by one round of SplitMix64: the same step under two
    /// domains starts from two different seeds.
    fn mixed(self, domain: Domain, step: u64) -> Self {
        let in_domain = SplitMix64::seed_from_u64(self.0 ^ domain.tag()).next_u64();

        Self(SplitMix64::seed_from_u64(in_domain ^ step).next_u64())
    }

    /// The generator of the substream `stream` of this seed.
    fn stream(self, stream: Stream) -> Xoshiro256PlusPlus {
        self.split(stream.part()).rng()
    }

    /// The generator of this substream: a `Xoshiro256PlusPlus` whose state SplitMix64 expands from the seed.
    fn rng(self) -> Xoshiro256PlusPlus {
        Xoshiro256PlusPlus::from_rng(&mut SplitMix64::seed_from_u64(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::distinct_costs;

    const ROOT: Seed = Seed(0x1234);

    #[test]
    fn test_splits_sizes_and_generators_by_hand() {
        // Worked out apart from the crate, one round of SplitMix64 per step: a part and a size of the same number land
        // on two seeds, and two parts split in either order on two more.
        assert_eq!(ROOT.split(1), Seed(0x94dd_8bc7_57d1_c37c));
        assert_eq!(ROOT.split(2), Seed(0x98e2_0fc3_c058_edb5));
        assert_eq!(ROOT.sized(1), Seed(0x97f3_563f_ee76_82e7));
        assert_eq!(ROOT.split(1).split(2), Seed(0x6cc5_f311_3ec7_e08f));
        assert_eq!(ROOT.split(2).split(1), Seed(0xec45_8426_a456_e14f));

        // The first draw of the generator SplitMix64 expands from the seed.
        assert_eq!(ROOT.rng().next_u64(), 0x90f0_4d8e_b6e8_6ab7);
        assert_eq!(ROOT.stream(Stream::Capacities).next_u64(), 0xf565_c2e6_1562_f776);
    }

    #[test]
    fn test_the_mixing_and_the_generator_draw_the_pinned_numbers() {
        // Pinned: a change of the mixing or of the generator changes every seeded network of the suite.
        assert_eq!(ROOT.split(Stream::Topology.part()).sized(20), Seed(0x82c4_596d_013d_f233));
        assert_eq!(ROOT.split(Stream::Capacities.part()).rng().next_u64(), 0xf565_c2e6_1562_f776);
    }

    #[test]
    fn test_suite_tie_breaks_are_fixed_never_zero_and_spread_neighbours() {
        let values = suite_tie_breaks(100);
        assert!(values.iter().all(|value| (1..=TIE_BREAK_MOST).contains(value)));

        // SplitMix64 seeded with 0, 1, 2, 3 gives 0xE220_A839_7B1D_CDAF, 0x910A_2DEC_8902_5CC1, ...; reduced modulo
        // 4095, plus one.
        assert_eq!(values[..4], [1466, 3426, 2336, 829]);
        assert_eq!(values[94], values[99], "arcs 94 and 99 share a tie-break");

        let distinct = values[..64].iter().collect::<std::collections::BTreeSet<_>>().len();
        assert_eq!(distinct, 64, "64 neighbouring arcs get 64 distinct tie-breaks");
        assert!(suite_tie_breaks(0).is_empty());
    }

    #[test]
    fn test_sizes_ascend() {
        for family in Family::ALL {
            assert!(family.sizes().is_sorted_by(|smaller, larger| smaller < larger), "{family:?}");
        }
    }

    /// The fingerprint of every network of the suite, family by family and size by size: at the family's own costs,
    /// and at distinct costs for the families with twins.
    const PINNED: [(Family, usize, u64, Option<u64>); 14] = [
        (Family::Supply, 20, 4_179_983_859_401_941_339, Some(10_646_520_056_145_623_867)),
        (Family::Supply, 80, 5_510_343_688_591_334_273, Some(12_574_195_511_716_258_520)),
        (Family::Supply, 300, 7_343_373_344_646_602_013, Some(2_673_744_708_060_354_400)),
        (Family::Assignment, 32, 265_230_422_617_474_092, Some(4_874_244_530_877_823_639)),
        (Family::Assignment, 128, 13_923_996_846_762_073_352, Some(8_236_525_002_290_329_221)),
        (Family::Assignment, 512, 10_050_576_579_825_798_319, Some(2_336_228_448_416_252_363)),
        (Family::Grid, 2, 4_622_014_571_610_203_285, None),
        (Family::Grid, 5, 2_418_293_544_934_957_370, None),
        (Family::Grid, 9, 18_313_080_846_264_427_529, None),
        (Family::Grid, 18, 1_985_779_509_518_992_569, None),
        (Family::Grid, 37, 13_662_593_574_593_445_984, None),
        (Family::Grid, 74, 14_754_299_361_623_493_597, None),
        (Family::Grid, 148, 17_979_819_941_737_903_278, None),
        (Family::Grid, 296, 6_838_927_011_223_234_410, None),
    ];

    #[test]
    fn test_the_suite_draws_the_pinned_networks() {
        // Pinned: a change of the seeds, of any generator's draws or of the tie-breaks changes these fingerprints, and
        // every benchmark row's network with them.
        let rows: Vec<(Family, usize)> =
            Family::ALL.iter().flat_map(|&family| family.sizes().iter().map(move |&size| (family, size))).collect();
        assert_eq!(rows, PINNED.map(|(family, size, _, _)| (family, size)), "the rows sweep the pinned sizes");

        for (family, size, own, distinct) in PINNED {
            let network = family.network(size);
            assert_eq!(network.fingerprint(), own, "{family:?} {size}");

            let tie_breaks = suite_tie_breaks(network.arc_count());
            let made_distinct = MincostNetwork { costs: distinct_costs(&network, &tie_breaks), ..network };
            if let Some(distinct) = distinct {
                assert_eq!(made_distinct.fingerprint(), distinct, "{family:?} {size} at distinct costs");
            }
        }
    }

    #[test]
    fn test_a_network_with_its_costs_drawn_anew_keeps_everything_else_and_each_role_s_range() {
        // The smallest network of each family, its costs drawn from another stream. Everything but the costs is the
        // suite's network; the costs moved; and each role keeps its range: twin arcs share one cost, the slack arcs of
        // the supply shape lie in 100..150 and of the assignment shape in 50..70, every other arc of the two shapes
        // below 12, and every grid arc in 1..=20 but the slack arc, whose cost the side gives, 20 a unit per row.
        for family in Family::ALL {
            let size = family.sizes()[0];
            let suite = family.network(size);
            let redrawn = family.network_with_costs(size, Xoshiro256PlusPlus::seed_from_u64(17));

            assert_eq!(MincostNetwork { costs: suite.costs.clone(), ..redrawn.clone() }, suite, "{family:?}");
            assert_ne!(redrawn.costs, suite.costs, "{family:?}");

            for arc in 0..suite.arc_count() {
                // Twins are made as two arcs in a row of the same ends and capacity; an arc to the sink beside a
                // write-off to it has the same ends, but another capacity.
                let twin = arc + 1 < suite.arc_count()
                    && (suite.tails[arc], suite.heads[arc], suite.capacities[arc])
                        == (suite.tails[arc + 1], suite.heads[arc + 1], suite.capacities[arc + 1]);
                if twin {
                    assert_eq!(redrawn.costs[arc], redrawn.costs[arc + 1], "{family:?} twins at arc {arc}");
                }
            }

            let slack = 20 * i64::try_from(size).expect("a size should fit an i64");
            let role_range = |network: &MincostNetwork, arc: usize| match family {
                Family::Supply if network.costs[arc] >= 100 => 100..150,
                Family::Assignment if network.costs[arc] >= 50 => 50..70,
                Family::Supply | Family::Assignment => 0..12,
                Family::Grid if arc + 1 == network.arc_count() => slack..slack + 1,
                Family::Grid => 1..21,
            };
            for arc in 0..suite.arc_count() {
                let range = role_range(&suite, arc);
                assert_eq!(role_range(&redrawn, arc), range, "{family:?} arc {arc} keeps its role");
                assert!(range.contains(&redrawn.costs[arc]), "{family:?} arc {arc}: {}", redrawn.costs[arc]);
            }
        }
    }
}
