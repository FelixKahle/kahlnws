// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// The 128-bit carriers of the C++ interface: construction from every integer width without a silent change of value,
// the halves at both signs and the extremes, decimal text, order, hashing consistent with equality, the checked
// conversion to 64 bits, and, where the compiler has its own 128-bit integer, agreement with it on boundary and seeded
// values in the round trip, the order and the conversion to double.

#include "kahlnws.hpp"

#include <cstddef>
#include <cstdint>
#include <functional>
#include <limits>
#include <sstream>
#include <string>
#include <type_traits>

#include "harness.hpp"

namespace {

using kahlnws::int128;
using kahlnws::uint128;

const std::uint64_t all_ones = (std::numeric_limits<std::uint64_t>::max)();
const std::uint64_t high_bit = std::uint64_t(1) << 63;

static_assert(int128(all_ones) == int128::from_halves(all_ones, 0), "an unsigned 64-bit value keeps its value");
static_assert(int128(-1) == int128::from_halves(all_ones, -1), "-1 is all ones");
static_assert(int128(static_cast<signed char>(-2)) == int128::from_halves(all_ones - 1, -1), "a narrow negative");
static_assert(!std::is_constructible<uint128, int>::value, "a signed value is refused unsigned");
static_assert(std::is_constructible<uint128, unsigned>::value, "an unsigned value is taken");
static_assert(!std::is_convertible<int, int128>::value, "construction is explicit");
static_assert(int128::from_halves(high_bit, -1).fits_int64() && !int128::from_halves(high_bit, 0).fits_int64(),
              "-2^63 fits and 2^63 does not");

/// A signed value and its digits worked by hand, in increasing order.
struct signed_case {
    int128 value;
    const char* decimal;
};

const signed_case signed_cases[] = {
    {int128::from_halves(0, (std::numeric_limits<std::int64_t>::min)()), "-170141183460469231731687303715884105728"},
    {int128::from_halves(all_ones, -2), "-18446744073709551617"},
    {int128::from_halves(0, -1), "-18446744073709551616"},
    {int128::from_halves(high_bit - 1, -1), "-9223372036854775809"},
    {int128((std::numeric_limits<std::int64_t>::min)()), "-9223372036854775808"},
    {int128(-1), "-1"},
    {int128(), "0"},
    {int128(1000000000), "1000000000"},
    {int128(all_ones), "18446744073709551615"},
    {int128::from_halves(0, 1), "18446744073709551616"},
    {int128::from_halves(all_ones, (std::numeric_limits<std::int64_t>::max)()),
     "170141183460469231731687303715884105727"},
};

void test_the_signed_text_order_and_hash(checks& checks) {
    const std::hash<int128> hash;
    const std::size_t count = sizeof signed_cases / sizeof signed_cases[0];
    for (std::size_t first = 0; first < count; ++first) {
        const signed_case& left = signed_cases[first];
        checks.check(kahlnws::to_string(left.value) == left.decimal, left.decimal);
        std::ostringstream stream;
        stream << left.value;
        checks.check(stream.str() == left.decimal, "the stream output should be the decimal");
        checks.check(hash(left.value) == hash(int128::from_halves(left.value.low, left.value.high)), "equal hash");
        for (std::size_t second = 0; second < count; ++second) {
            const int128& right = signed_cases[second].value;
            checks.check((left.value < right) == (first < second) && (left.value == right) == (first == second) &&
                             (left.value >= right) == (first >= second),
                         "the order should be the cases' order");
        }
    }
    checks.check(hash(int128(5)) == std::hash<std::uint64_t>()(5), "a 64-bit value hashes as its low half");
}

void test_the_unsigned_text_and_conversions(checks& checks) {
    checks.check(kahlnws::to_string(uint128::from_halves(all_ones, all_ones)) ==
                     "340282366920938463463374607431768211455",
                 "2^128 - 1 in decimal");
    checks.check(kahlnws::to_string(uint128()) == "0", "zero in decimal");
    checks.check(uint128(all_ones).to_uint64() == all_ones && !uint128::from_halves(0, 1).fits_uint64(), "fits");
    const std::int64_t int64_max = (std::numeric_limits<std::int64_t>::max)();
    checks.check(int128(-14).to_int64() == -14 && int128(all_ones >> 1).to_int64() == int64_max,
                 "the 64-bit values come back");
    checks.check(int128::from_halves(0, 1).to_double() == 18446744073709551616.0, "2^64 as a double");
}

void test_agreement_with_the_native_type(checks& checks) {
#if defined(__SIZEOF_INT128__)
    typedef kahlnws::native_int128 native;
    std::uint64_t state = 7;
    for (int draw = 0; draw < 2000; ++draw) {
        // SplitMix64, seeded: two draws make the halves, and a third a shift that spreads the magnitudes.
        std::uint64_t halves[3];
        for (std::uint64_t& half : halves) {
            std::uint64_t mixed = (state += 0x9E3779B97F4A7C15u);
            mixed = (mixed ^ (mixed >> 30)) * 0xBF58476D1CE4E5B9u;
            mixed = (mixed ^ (mixed >> 27)) * 0x94D049BB133111EBu;
            half = mixed ^ (mixed >> 31);
        }
        const kahlnws::native_uint128 raw =
            ((static_cast<kahlnws::native_uint128>(halves[0]) << 64) | halves[1]) >> (halves[2] % 128);
        const native value = static_cast<native>(raw);
        const int128 carried(value);
        checks.check(carried.to_native() == value, "the native round trip");
        checks.check(carried.to_double() == static_cast<double>(value), "the double should be the native one");
        checks.check(uint128(raw).to_double() == static_cast<double>(raw), "the unsigned double too");
        checks.check(uint128(raw).to_native() == raw, "the unsigned round trip");
        const int128 negated(-value - 1);
        checks.check((carried < negated) == (value < -value - 1), "the order should be the native one");
    }
#else
    checks.check(true, "no native 128-bit integer");
#endif
}

}  // namespace

int main() {
    checks checks("int128");

    test_the_signed_text_order_and_hash(checks);
    test_the_unsigned_text_and_conversions(checks);
    test_agreement_with_the_native_type(checks);

    return checks.exit_code();
}
