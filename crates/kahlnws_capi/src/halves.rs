// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A 128-bit integer as C reads it: its low and its high 64 bits, since standard C has no 128-bit integer.

/// A signed 128-bit integer as its two 64-bit halves: the value is `high * 2^64 + low`. The cost of a flow, and the
/// sum of supplies that do not balance.
///
/// `low` is the value modulo `2^64`, from 0 to `2^64 - 1`, and `high` the value divided by `2^64` and rounded down,
/// from `-2^63` to `2^63 - 1`: the low and the high 64 bits of the value's two's complement. Every value has exactly
/// one pair of halves, so:
///
/// - a value fits an `int64_t` exactly where `high` is 0 and `low` is below `2^63` (the value is `low`), or `high` is
///   -1 and `low` is at least `2^63` (the value is `low - 2^64`);
/// - two values compare as their `high` halves compare, signed, and where those are equal as their `low` halves,
///   unsigned;
/// - on a compiler with a 128-bit integer, the value is
///   `(__int128) (((unsigned __int128) (uint64_t) high << 64) | low)`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Int128 {
    /// The low 64 bits: the value modulo `2^64`.
    pub low: u64,
    /// The high 64 bits, signed: the value divided by `2^64`, rounded down.
    pub high: i64,
}

/// An unsigned 128-bit integer as its two 64-bit halves: the value is `high * 2^64 + low`. The sum of the positive
/// supplies net of the lower bounds that passes its limit.
///
/// `low` is the value modulo `2^64` and `high` the value divided by `2^64`, rounded down, both from 0 to `2^64 - 1`.
/// A value fits a `uint64_t` exactly where `high` is 0, and two values compare as their `high` halves and, where those
/// are equal, as their `low` halves.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct UInt128 {
    /// The low 64 bits: the value modulo `2^64`.
    pub low: u64,
    /// The high 64 bits: the value divided by `2^64`, rounded down.
    pub high: u64,
}

impl From<i128> for Int128 {
    fn from(value: i128) -> Self {
        // The two's complement's high bits, read as signed, are the value divided by `2^64` and rounded down.
        let bits = value.cast_unsigned();

        Self { low: low_half(bits), high: high_half(bits).cast_signed() }
    }
}

impl From<Int128> for i128 {
    fn from(halves: Int128) -> Self {
        // The high half moved up is a multiple of `2^64`, the low half below it: the `or` adds them.
        (Self::from(halves.high) << 64) | Self::from(halves.low)
    }
}

impl From<u128> for UInt128 {
    fn from(value: u128) -> Self {
        Self { low: low_half(value), high: high_half(value) }
    }
}

impl From<UInt128> for u128 {
    fn from(halves: UInt128) -> Self {
        (Self::from(halves.high) << 64) | Self::from(halves.low)
    }
}

/// The low 64 bits of `bits`: `bits` modulo `2^64`.
fn low_half(bits: u128) -> u64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the low half is `bits` modulo 2^64, which is exactly what the cast keeps"
    )]
    let low = bits as u64;

    low
}

/// The high 64 bits of `bits`: `bits` divided by `2^64`, rounded down.
fn high_half(bits: u128) -> u64 {
    // Below `2^64`, so its low half is all of it.
    low_half(bits >> 64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `high · 2^64 + low` in checked arithmetic, apart from the bit operations under test; `None` past an `i128`.
    fn signed_value(halves: Int128) -> Option<i128> {
        i128::from(halves.high).checked_mul(1 << 64)?.checked_add(i128::from(halves.low))
    }

    /// `high · 2^64 + low` in checked arithmetic; `None` past a `u128`.
    fn unsigned_value(halves: UInt128) -> Option<u128> {
        u128::from(halves.high).checked_mul(1 << 64)?.checked_add(u128::from(halves.low))
    }

    /// Signed values at both signs and the extremes: zero, one either side of it, either side of `2^63` and `2^64`,
    /// and the ends of an `i128`.
    const SIGNED: [i128; 14] = [
        0,
        1,
        -1,
        14,
        -14,
        (1 << 63) - 1,
        -(1 << 63),
        1 << 63,
        (1 << 64) - 1,
        1 << 64,
        -(1 << 64),
        -(1 << 64) - 1,
        i128::MAX,
        i128::MIN,
    ];

    #[test]
    fn test_signed_halves_by_hand() {
        assert_eq!(Int128::from(0), Int128 { low: 0, high: 0 });
        assert_eq!(Int128::from(14), Int128 { low: 14, high: 0 });
        assert_eq!(Int128::from(-1), Int128 { low: u64::MAX, high: -1 });
        assert_eq!(Int128::from(-14), Int128 { low: u64::MAX - 13, high: -1 });
        assert_eq!(Int128::from(1 << 64), Int128 { low: 0, high: 1 });
        assert_eq!(Int128::from(-(1 << 64)), Int128 { low: 0, high: -1 });
        assert_eq!(Int128::from(-(1 << 64) - 1), Int128 { low: u64::MAX, high: -2 });
        assert_eq!(Int128::from(i128::MAX), Int128 { low: u64::MAX, high: i64::MAX });
        assert_eq!(Int128::from(i128::MIN), Int128 { low: 0, high: i64::MIN });
    }

    #[test]
    fn test_signed_halves_keep_the_value_by_the_stated_rule_and_back() {
        for value in SIGNED {
            let halves = Int128::from(value);

            assert_eq!(signed_value(halves), Some(value), "{value}");
            assert_eq!(i128::from(halves), value, "{value}");
        }
    }

    #[test]
    fn test_signed_halves_compare_as_their_values() {
        for (first, second) in SIGNED.into_iter().flat_map(|first| SIGNED.map(|second| (first, second))) {
            let (left, right) = (Int128::from(first), Int128::from(second));

            assert_eq!((left.high, left.low).cmp(&(right.high, right.low)), first.cmp(&second), "{first} {second}");
        }
    }

    #[test]
    fn test_signed_halves_fit_an_int64_exactly_where_the_rule_says() {
        for value in SIGNED {
            let halves = Int128::from(value);
            let by_rule = match halves.high {
                0 => i64::try_from(halves.low).ok(),
                -1 => (halves.low >= 1 << 63).then(|| halves.low.cast_signed()),
                _ => None,
            };

            assert_eq!(by_rule, i64::try_from(value).ok(), "{value}");
        }
    }

    #[test]
    fn test_unsigned_halves_by_hand_and_back() {
        let cases = [
            (0, UInt128 { low: 0, high: 0 }),
            (u128::from(u64::MAX), UInt128 { low: u64::MAX, high: 0 }),
            (1 << 64, UInt128 { low: 0, high: 1 }),
            ((1 << 64) + 5, UInt128 { low: 5, high: 1 }),
            (u128::MAX, UInt128 { low: u64::MAX, high: u64::MAX }),
        ];

        for (value, halves) in cases {
            assert_eq!(UInt128::from(value), halves, "{value}");
            assert_eq!(unsigned_value(halves), Some(value), "{value}");
            assert_eq!(u128::from(halves), value, "{value}");
        }
    }
}
