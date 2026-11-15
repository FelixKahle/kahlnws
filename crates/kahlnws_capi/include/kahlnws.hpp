// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

#ifndef KAHLNWS_HPP_3D75A16F_358D_4952_81EB_3C4A49B5DE0C
#define KAHLNWS_HPP_3D75A16F_358D_4952_81EB_3C4A49B5DE0C

/// @file kahlnws.hpp
/// @brief The C++ interface of kahlnws: a primal network simplex for the min-cost flow problem on exact integers.
///
/// A problem is a set of nodes, each with a supply, and arcs between them, each with a lower bound, a capacity and a
/// cost of a unit of flow. A solve finds a flow of least cost that meets every supply within the bounds, or that no
/// such flow exists, and answers the work it did either way.
///
/// @par Requirements
/// C++11 or later, and the C header kahlnws.h beside this one, which it includes; this header includes nothing else
/// of the library. A program that includes it links the library libkahlnws_capi, static or dynamic, as a C program
/// does. A program that links the dynamic library on Windows defines @c KAHLNWS_SHARED before including this header,
/// as the C header asks.
///
/// @par Two levels
/// A kahlnws::problem views the caller's tables, arc a at position a of every per-arc table and node v at position v
/// of the supplies; it copies nothing. A kahlnws::network_simplex solves it at one of two levels:
/// - the high level, @c try_solve and @c solve, answers an owned kahlnws::solution: the flows, the potentials and the
///   cost of an optimal flow, or that the problem is infeasible, beside the kahlnws::work of the solve;
/// - the low level, @c try_solve_into and @c solve_into, writes the flows, and the potentials where asked, into
///   buffers the caller owns, and answers a kahlnws::optimum, the cost, or that the problem is infeasible, beside the
///   work.
///
/// @par Solving again
/// A solver keeps the basis its last solve ended on, optimal or infeasible. @c try_resolve and @c resolve, and
/// @c try_resolve_into and @c resolve_into, beside the two levels above and with the same arguments and answers, start
/// from it where it serves: where the problem has the node and arc counts of that solve and every arc its tail and its
/// head, and the supplies the basis leaves stay within range. Then, where only some numbers changed, they often make
/// far fewer pivots, and the same problem again, where the kept basis serves, makes none. Where the basis cannot serve,
/// a resolve answers exactly as a solve does. Every form answers the same outcome, the same cost, and potentials that
/// prove the flow optimal; only a solve answers the flows among tied optima, the potentials and the work as a function
/// of the problem alone, while a resolve answers a function of the problem and of the solves before it.
///
/// @par Errors as values, and as exceptions
/// An infeasible problem is an answer, not an error. Every operation that can fail has a form named @c try_..., in
/// every build, that never throws and answers a kahlnws::result: the value, or a kahlnws::error, which says whether
/// the problem was refused (and why, as a kahlnws::refusal), an argument was not admitted, memory was refused, or the
/// library failed. Where exceptions are enabled, each also has a throwing form under the plain name, which calls the
/// @c try_ form and throws what kahlnws::result::value() throws: kahlnws::problem_error for a refused problem,
/// @c std::invalid_argument for an argument not admitted, @c std::bad_alloc for refused memory, and
/// kahlnws::internal_error for a failure of the library. Nothing else in this header throws.
///
/// @par Exceptions enabled or not
/// The throwing forms and the exception classes exist only where @c KAHLNWS_EXCEPTIONS is 1: where the compiler
/// has exceptions enabled (it defines @c __cpp_exceptions, @c __EXCEPTIONS or @c _CPPUNWIND) and the program has not
/// defined @c KAHLNWS_NO_EXCEPTIONS before including this header. Otherwise no @c throw, no @c try and no exception
/// class is compiled, and a call of a throwing form does not compile. Everything else is the same in both modes, and
/// lies in an inline namespace of the mode's name, @c kahlnws::exceptions or @c kahlnws::no_exceptions: translation
/// units of different modes may be linked into one program, each with its own copy of the interface, but a function
/// of one mode that takes a type of this header cannot be called from the other: the call fails to link rather than
/// break the one-definition rule. Translation units of different standards share one copy, which differs between
/// standards only in attributes that make warnings and in the constructor of a kahlnws::span from a @c std::span.
///
/// @par Allocation
/// The @c try_ forms of the low level, @c try_solve_into and @c try_resolve_into, allocate nothing where the solver's
/// room holds the problem, made by @c try_reserve or by an earlier solve, and nothing on any error. The high level
/// allocates the vectors of the solution. Where exceptions are disabled and memory for those vectors is refused, the
/// standard library acts as it does without exceptions, which is to end the program on the common implementations;
/// the low level, with the caller's buffers, is the path that needs no allocation. A kahlnws::error holds its
/// message, and making one allocates nothing; a throwing form allocates the message of the exception it throws.
///
/// @par Preconditions
/// A precondition this header states is checked in every build, and a call that breaks it ends the program with
/// @c std::abort after a message on standard error, never with a wrong answer; the one exception is
/// kahlnws::span::operator[], which, as the standard containers do, checks its index with @c assert alone.
///
/// @par Threads
/// A kahlnws::network_simplex is used by one thread at a time: no two of its calls run at once. It may move from one
/// thread to another between calls, and distinct solvers may be used on distinct threads at once, also to solve the
/// same problem, whose tables a solve only reads. Every other function may be called from any thread at any time.
///
/// @par Ownership and lifetimes
/// A kahlnws::network_simplex owns its solver and room, and frees them when destroyed. A kahlnws::span and a
/// kahlnws::problem own nothing: what they view must outlive every use of them and must not move, so a
/// @c std::vector viewed must not grow meanwhile. Every answer is a value that owns what it holds.
///
/// @par Numbers
/// Node and arc counts and node numbers are @c std::size_t; flows, lower bounds, capacities and the counts of a
/// kahlnws::work @c std::uint64_t; costs, supplies and potentials @c std::int64_t; the cost of a flow a
/// kahlnws::int128, exact.
///
/// @par Example
/// A depot (node 0) sends 4 units to a customer (node 2), directly or through a hub (node 1); this compiles in both
/// modes:
/// @code{.cpp}
/// #include <cstddef>
/// #include <cstdint>
/// #include <cstdio>
/// #include <vector>
///
/// #include "kahlnws.hpp"
///
/// int main() {
///     const std::vector<std::size_t> tails = {0, 0, 1};
///     const std::vector<std::size_t> heads = {2, 1, 2};
///     const std::vector<std::uint64_t> capacities = {3, 4, 2};
///     const std::vector<std::int64_t> costs = {5, 1, 1};
///     const std::vector<std::int64_t> supplies = {4, 0, -4};
///
///     kahlnws::problem problem;
///     problem.tails = tails;
///     problem.heads = heads;
///     problem.capacities = capacities;
///     problem.costs = costs;
///     problem.supplies = supplies;
///
///     kahlnws::network_simplex solver;
///     const kahlnws::result<kahlnws::answer<kahlnws::solution>> solved = solver.try_solve(problem);
///     if (!solved) {
///         std::fprintf(stderr, "%s\n", solved.error().message());
///         return 1;
///     }
///     if (const kahlnws::solution* solution = solved->outcome.optimal()) {
///         std::printf("cost %s\n", kahlnws::to_string(solution->total_cost).c_str());  // 14; flows 2, 2, 2
///     }
///     return 0;
/// }
/// @endcode

#include <cassert>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <functional>
#include <iosfwd>
#include <limits>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

#include "kahlnws.h"

#if defined(_MSVC_LANG) && _MSVC_LANG > __cplusplus
#define KAHLNWS_HPP_STANDARD _MSVC_LANG
#else
#define KAHLNWS_HPP_STANDARD __cplusplus
#endif

/// @brief 1 where the throwing forms and the exception classes are compiled, 0 where they are not.
///
/// The header defines it; a program reads it and does not define it. It is 1 where the compiler has exceptions
/// enabled, as @c __cpp_exceptions, @c __EXCEPTIONS (older GCC) or @c _CPPUNWIND (MSVC) shows, and the program has not
/// defined @c KAHLNWS_NO_EXCEPTIONS before including this header.
#if !defined(KAHLNWS_NO_EXCEPTIONS) && (defined(__cpp_exceptions) || defined(__EXCEPTIONS) || defined(_CPPUNWIND))
#define KAHLNWS_EXCEPTIONS 1
#else
#define KAHLNWS_EXCEPTIONS 0
#endif

#if KAHLNWS_EXCEPTIONS
#include <new>
#include <stdexcept>
#endif

// The standard attribute from C++17 on. Before it, Clang's own, which a cast to void silences as the standard one;
// GCC's own cannot be silenced so, so GCC gets none before C++17.
#if KAHLNWS_HPP_STANDARD >= 201703L
#define KAHLNWS_HPP_NODISCARD [[nodiscard]]
#elif defined(__clang__)
#define KAHLNWS_HPP_NODISCARD __attribute__((warn_unused_result))
#else
#define KAHLNWS_HPP_NODISCARD
#endif

// A std::span is a view, so a kahlnws::span may be made from a temporary one; it exists from C++20 on.
#if KAHLNWS_HPP_STANDARD >= 202002L
#include <version>
#if defined(__cpp_lib_span)
#include <span>
#endif
#endif

/// @brief The C++ interface of kahlnws.
namespace kahlnws {

#if KAHLNWS_EXCEPTIONS
inline namespace exceptions {
#else
inline namespace no_exceptions {
#endif

class network_simplex;
template <class T>
class span;
template <class T>
class result;

#if defined(__SIZEOF_INT128__)
/// @brief The compiler's signed 128-bit integer; declared only where the compiler defines @c __SIZEOF_INT128__.
__extension__ typedef __int128 native_int128;
/// @brief The compiler's unsigned 128-bit integer; declared only where the compiler defines @c __SIZEOF_INT128__.
__extension__ typedef unsigned __int128 native_uint128;
#endif

/// What only this header uses: nothing in it is part of the interface, and any of it may change.
namespace detail {

/// Ends the program after writing `condition`, and `cause` after it, to standard error: a precondition was broken.
[[noreturn]] inline void precondition_failed(const char* condition, const char* cause = "") noexcept {
    std::fprintf(stderr, "%s%s\n", condition, cause);
    std::abort();
}

/// Writes the magnitude `high * 2^64 + low` in decimal, after a minus sign where `negative`, to the characters
/// before `end`, and answers the first; 2^128 - 1 has 39 digits, so 40 characters before `end` always suffice.
inline char* write_decimal(char* end, std::uint64_t high, std::uint64_t low, bool negative) noexcept {
    // The magnitude as four 32-bit limbs, the most significant first, divided by 10^9 until nothing is left: each
    // division leaves a remainder below 10^9, so the remainder shifted by 32 bits and a limb stay below 2^62.
    const std::uint64_t billion = 1000000000u;
    std::uint64_t limbs[4] = {high >> 32, high & 0xFFFFFFFFu, low >> 32, low & 0xFFFFFFFFu};
    char* first = end;
    bool more = true;
    while (more) {
        std::uint64_t remainder = 0;
        for (std::uint64_t& limb : limbs) {
            const std::uint64_t current = (remainder << 32) | limb;
            limb = current / billion;
            remainder = current % billion;
        }
        more = (limbs[0] | limbs[1] | limbs[2] | limbs[3]) != 0;

        // A group with more groups above it is written with all nine of its digits, its leading zeros included; the
        // most significant group without them, and as one zero where the whole magnitude is zero.
        std::size_t written = 0;
        do {
            --first;
            *first = static_cast<char>('0' + remainder % 10);
            remainder /= 10;
            ++written;
        } while (more ? written < 9 : remainder != 0);
    }
    if (negative) {
        --first;
        *first = '-';
    }
    return first;
}

/// The magnitude `high * 2^64 + low` as the nearest double, ties to even, where a 64-bit integer converts to the
/// nearest double as on every IEEE 754 platform in its default rounding mode.
inline double nearest_double(std::uint64_t high, std::uint64_t low) noexcept {
    if (high == 0) {
        return static_cast<double>(low);
    }
    // The 64 bits from the highest set bit down, and below them a sticky bit for whatever is left: a nonzero rest
    // lies below bit 10, the lowest rounding bit of a 53-bit significand, so it decides only a tie, as it must.
    unsigned shift = 0;
    std::uint64_t top = high;
    while ((top >> 63) == 0) {
        top <<= 1;
        ++shift;
    }
    const std::uint64_t rest = shift == 0 ? low : low << shift;
    if (shift != 0) {
        top |= low >> (64 - shift);
    }
    top |= rest != 0 ? 1u : 0u;

    // The value is `top * 2^(64 - shift)`, with `64 - shift` from 1 to 64; multiplying by a power of two is exact.
    const double scale = static_cast<double>(std::uint64_t(1) << (63 - shift)) * 2.0;
    return static_cast<double>(top) * scale;
}

/// Whether `value` is below zero, for a signed type.
template <class Integer>
constexpr bool is_negative(Integer value, std::true_type) noexcept {
    return value < 0;
}

/// Whether `value` is below zero, for an unsigned type: never.
template <class Integer>
constexpr bool is_negative(Integer, std::false_type) noexcept {
    return false;
}

/// Whether `Integer` is an integer type of at most 64 bits other than `bool`.
template <class Integer>
struct is_narrow_integer
    : std::integral_constant<bool, std::is_integral<Integer>::value && !std::is_same<Integer, bool>::value &&
                                       sizeof(Integer) <= sizeof(std::uint64_t)> {};

/// `void`, for any well-formed types: a partial specialisation that names it applies where they are well-formed.
template <class...>
struct void_type {
    typedef void type;
};

/// Whether a pointer to `Stored` may be viewed as a pointer to `Element`: the same type, with `const` only added.
template <class Stored, class Element>
struct is_viewable_as
    : std::integral_constant<bool,
                             std::is_same<typename std::remove_const<Stored>::type,
                                          typename std::remove_const<Element>::type>::value &&
                                 (std::is_const<Element>::value || !std::is_const<Stored>::value)> {};

/// Whether `View` views storage it does not own, so that a temporary of it may be viewed in turn.
template <class View>
struct is_borrowed : std::false_type {};

#if defined(__cpp_lib_span)
template <class Element, std::size_t Extent>
struct is_borrowed<std::span<Element, Extent> > : std::true_type {};
#endif

/// Whether `Viewed` is a kahlnws::span, which is copied or converted, never viewed as a range.
template <class Viewed>
struct is_span : std::false_type {};

template <class Element>
struct is_span<span<Element> > : std::true_type {};

/// Whether `Range`, as a forwarding reference names it, gives contiguous elements by `data()` and an integer
/// `size()` that may be viewed as elements of type `Element`, and outlives the expression it is named in.
template <class Range, class Element, class = void>
struct is_viewable_range : std::false_type {};

template <class Range, class Element>
struct is_viewable_range<Range,
                         Element,
                         typename void_type<decltype(std::declval<Range&>().data()),
                                            decltype(std::declval<Range&>().size())>::type>
    : std::integral_constant<
          bool,
          !is_span<typename std::remove_cv<typename std::remove_reference<Range>::type>::type>::value &&
              (std::is_lvalue_reference<Range>::value ||
               is_borrowed<typename std::remove_cv<typename std::remove_reference<Range>::type>::type>::value) &&
              std::is_pointer<decltype(std::declval<Range&>().data())>::value &&
              is_viewable_as<typename std::remove_pointer<decltype(std::declval<Range&>().data())>::type,
                             Element>::value &&
              std::is_integral<decltype(std::declval<Range&>().size())>::value> {};

/// The most elements of type `Element` an object holds: PTRDIFF_MAX bytes of them.
template <class Element>
constexpr std::size_t max_count() noexcept {
    return static_cast<std::size_t>((std::numeric_limits<std::ptrdiff_t>::max)()) / sizeof(Element);
}

/// `first`, where it may start a view of `count` elements: not null unless `count` is zero, and `count` elements
/// within PTRDIFF_MAX bytes. Ends the program otherwise.
template <class Element>
constexpr Element* view_start(Element* first, std::size_t count) noexcept {
    return (first != nullptr || count == 0) && count <= max_count<Element>()
               ? first
               : (precondition_failed("called `span::span` with a null pointer and a count above zero, or a count "
                                      "past PTRDIFF_MAX bytes"),
                  first);
}

/// `size` as a count, for an unsigned size: where it fits a `std::size_t`. Ends the program otherwise.
template <class Size>
constexpr std::size_t count_of(Size size, std::false_type) noexcept {
    return size <= (std::numeric_limits<std::size_t>::max)()
               ? static_cast<std::size_t>(size)
               : (precondition_failed("called `span::span` with a range whose size passes SIZE_MAX"), 0);
}

/// `size` as a count, for a signed size: where it is not negative and fits a `std::size_t`. Ends the program
/// otherwise.
template <class Size>
constexpr std::size_t count_of(Size size, std::true_type) noexcept {
    return size >= 0 ? count_of(static_cast<typename std::make_unsigned<Size>::type>(size), std::false_type())
                     : (precondition_failed("called `span::span` with a range of negative size"), 0);
}

/// Whether the byte ranges from `first` and from `second`, of `first_bytes` and `second_bytes` bytes, share a byte;
/// an empty range shares none. Exact where pointers order as the addresses of a flat address space, as they do on
/// every platform this header is built for: `std::less` orders any two pointers, also into distinct objects.
inline bool overlap(const void* first, std::size_t first_bytes, const void* second, std::size_t second_bytes) noexcept {
    if (first_bytes == 0 || second_bytes == 0) {
        return false;
    }
    const unsigned char* const first_begin = static_cast<const unsigned char*>(first);
    const unsigned char* const second_begin = static_cast<const unsigned char*>(second);
    const std::less<const unsigned char*> before;
    return before(first_begin, second_begin + second_bytes) && before(second_begin, first_begin + first_bytes);
}

/// A message being written into a buffer of fixed capacity: always ended by a NUL, and cut where it is full.
class message_writer {
public:
    /// An empty message in the `capacity` characters from `buffer`, at least one.
    message_writer(char* buffer, std::size_t capacity) noexcept : buffer_(buffer), capacity_(capacity), length_(0) {
        buffer_[0] = '\0';
    }

    /// Appends `text`.
    void write(const char* text) noexcept {
        for (; *text != '\0' && length_ + 1 < capacity_; ++text) {
            buffer_[length_] = *text;
            ++length_;
        }
        buffer_[length_] = '\0';
    }

    /// Appends `number` in decimal.
    void write(std::uint64_t number) noexcept {
        char digits[41];
        digits[40] = '\0';
        write(write_decimal(digits + 40, 0, number, false));
    }

private:
    char* buffer_;
    std::size_t capacity_;
    /// The characters written, below `capacity_`; the NUL follows them.
    std::size_t length_;
};

}  // namespace detail

/// @brief A signed 128-bit integer as its two 64-bit halves: the value is `high * 2^64 + low`.
///
/// The cost of a flow, and the sum of supplies that do not balance. A carrier of exact values, not an arithmetic
/// type: it compares, hashes, converts and prints, and does nothing else. Plain data: every pair of halves is a
/// value, and every value has exactly one pair; @c low is the value modulo 2^64, and @c high the value divided by 2^64
/// and rounded down, so they are the low and the high 64 bits of the value's two's complement.
class int128 {
public:
    /// @brief The low 64 bits: the value modulo 2^64.
    std::uint64_t low;
    /// @brief The high 64 bits, signed: the value divided by 2^64, rounded down.
    std::int64_t high;

    /// @brief Zero.
    constexpr int128() noexcept : low(0), high(0) {}

    /// @brief The value of an integer of at most 64 bits, signed or unsigned, exactly.
    /// @tparam Integer An integer type of at most 64 bits other than @c bool.
    /// @param value The value.
    template <class Integer, typename std::enable_if<detail::is_narrow_integer<Integer>::value, int>::type = 0>
    constexpr explicit int128(Integer value) noexcept
        : low(static_cast<std::uint64_t>(value)),
          high(detail::is_negative(value, std::is_signed<Integer>()) ? -1 : 0) {}

#if defined(__SIZEOF_INT128__)
    /// @brief The value of the compiler's own signed 128-bit integer, exactly; only where it has one.
    /// @tparam Native kahlnws::native_int128 and no other type, so that no other integer converts on its way in.
    /// @param value The value.
    template <class Native, typename std::enable_if<std::is_same<Native, native_int128>::value, int>::type = 0>
    constexpr explicit int128(Native value) noexcept
        // `value` less its low half is a multiple of 2^64 at least -2^127, so the division is exact and in range.
        : low(static_cast<std::uint64_t>(value)),
          high(static_cast<std::int64_t>((value - static_cast<native_int128>(static_cast<std::uint64_t>(value))) /
                                         (static_cast<native_int128>(1) << 64))) {}

    /// @brief The value as the compiler's own signed 128-bit integer, exactly; only where it has one.
    constexpr native_int128 to_native() const noexcept {
        // `high * 2^64` lies from -2^127 to 2^127 - 2^64, so neither the product nor the sum overflows.
        return static_cast<native_int128>(high) * (static_cast<native_int128>(1) << 64) +
               static_cast<native_int128>(low);
    }
#endif

    /// @brief The value of the halves `high * 2^64 + low`, given in the order they are laid out, the low one first.
    /// @param low_half The low 64 bits.
    /// @param high_half The high 64 bits.
    static constexpr int128 from_halves(std::uint64_t low_half, std::int64_t high_half) noexcept {
        return int128(low_half, high_half, 0);
    }

    /// @brief Whether the value lies within the range of @c std::int64_t.
    constexpr bool fits_int64() const noexcept {
        return (high == 0 && low >> 63 == 0) || (high == -1 && low >> 63 == 1);
    }

    /// @brief The value as a @c std::int64_t, exactly.
    /// @pre fits_int64(); checked in every build.
    std::int64_t to_int64() const noexcept {
        if (!fits_int64()) {
            detail::precondition_failed("called `int128::to_int64` on a value that does not fit 64 bits");
        }
        // Where `high` is -1, the value is `low - 2^64`, which is `-(2^64 - 1 - low) - 1`, and `~low` is below 2^63.
        return high == 0 ? static_cast<std::int64_t>(low) : -static_cast<std::int64_t>(~low) - 1;
    }

    /// @brief The value as the nearest double, ties to even.
    /// @note Exact where a 64-bit integer converts to the nearest double, as on every IEEE 754 platform in its
    /// default rounding mode.
    double to_double() const noexcept {
        const double magnitude = detail::nearest_double(magnitude_high(), magnitude_low());
        return high < 0 ? -magnitude : magnitude;
    }

    /// @brief The high half of the value's magnitude, for the conversions to text and to double.
    constexpr std::uint64_t magnitude_high() const noexcept {
        return high < 0 ? ~static_cast<std::uint64_t>(high) + (low == 0 ? 1u : 0u) : static_cast<std::uint64_t>(high);
    }

    /// @brief The low half of the value's magnitude, for the conversions to text and to double.
    constexpr std::uint64_t magnitude_low() const noexcept { return high < 0 ? ~low + 1u : low; }

private:
    /// The value of the halves; the third parameter only sets it apart from the public constructors.
    constexpr int128(std::uint64_t low_half, std::int64_t high_half, int) noexcept : low(low_half), high(high_half) {}
};

/// @brief An unsigned 128-bit integer as its two 64-bit halves: the value is `high * 2^64 + low`.
///
/// The sum of the positive supplies net of the lower bounds that passes its limit. A carrier of exact values, not an
/// arithmetic type, as kahlnws::int128. Plain data: every pair of halves is a value, and every value has exactly one
/// pair; @c low is the value modulo 2^64 and @c high the value divided by 2^64, rounded down.
class uint128 {
public:
    /// @brief The low 64 bits: the value modulo 2^64.
    std::uint64_t low;
    /// @brief The high 64 bits: the value divided by 2^64, rounded down.
    std::uint64_t high;

    /// @brief Zero.
    constexpr uint128() noexcept : low(0), high(0) {}

    /// @brief The value of an unsigned integer of at most 64 bits, exactly.
    /// @tparam Integer An unsigned integer type of at most 64 bits other than @c bool.
    /// @param value The value.
    template <class Integer,
              typename std::enable_if<detail::is_narrow_integer<Integer>::value && std::is_unsigned<Integer>::value,
                                      int>::type = 0>
    constexpr explicit uint128(Integer value) noexcept : low(value), high(0) {}

    /// @brief Refused: a signed integer could be negative, which no unsigned value is.
    template <class Integer,
              typename std::enable_if<detail::is_narrow_integer<Integer>::value && std::is_signed<Integer>::value,
                                      int>::type = 0>
    explicit uint128(Integer value) = delete;

#if defined(__SIZEOF_INT128__)
    /// @brief The value of the compiler's own unsigned 128-bit integer, exactly; only where it has one.
    /// @tparam Native kahlnws::native_uint128 and no other type, so that no other integer converts on its way in.
    /// @param value The value.
    template <class Native, typename std::enable_if<std::is_same<Native, native_uint128>::value, int>::type = 0>
    constexpr explicit uint128(Native value) noexcept
        : low(static_cast<std::uint64_t>(value)), high(static_cast<std::uint64_t>(value >> 64)) {}

    /// @brief The value as the compiler's own unsigned 128-bit integer, exactly; only where it has one.
    constexpr native_uint128 to_native() const noexcept {
        return (static_cast<native_uint128>(high) << 64) | static_cast<native_uint128>(low);
    }
#endif

    /// @brief The value of the halves `high * 2^64 + low`, given in the order they are laid out, the low one first.
    /// @param low_half The low 64 bits.
    /// @param high_half The high 64 bits.
    static constexpr uint128 from_halves(std::uint64_t low_half, std::uint64_t high_half) noexcept {
        return uint128(low_half, high_half, 0);
    }

    /// @brief Whether the value lies within the range of @c std::uint64_t.
    constexpr bool fits_uint64() const noexcept { return high == 0; }

    /// @brief The value as a @c std::uint64_t, exactly.
    /// @pre fits_uint64(); checked in every build.
    std::uint64_t to_uint64() const noexcept {
        if (!fits_uint64()) {
            detail::precondition_failed("called `uint128::to_uint64` on a value that does not fit 64 bits");
        }
        return low;
    }

    /// @brief The value as the nearest double, ties to even.
    /// @note Exact where a 64-bit integer converts to the nearest double, as on every IEEE 754 platform in its
    /// default rounding mode.
    double to_double() const noexcept { return detail::nearest_double(high, low); }

private:
    /// The value of the halves; the third parameter only sets it apart from the public constructors.
    constexpr uint128(std::uint64_t low_half, std::uint64_t high_half, int) noexcept : low(low_half), high(high_half) {}
};

/// @brief Whether @p left and @p right are the same value.
/// @param left A value.
/// @param right Another value.
constexpr bool operator==(const int128& left, const int128& right) noexcept {
    return left.low == right.low && left.high == right.high;
}

/// @brief Whether @p left and @p right are different values.
/// @param left A value.
/// @param right Another value.
constexpr bool operator!=(const int128& left, const int128& right) noexcept { return !(left == right); }

/// @brief Whether @p left is less than @p right: their high halves compared, and where equal their low halves.
/// @param left A value.
/// @param right Another value.
constexpr bool operator<(const int128& left, const int128& right) noexcept {
    return left.high < right.high || (left.high == right.high && left.low < right.low);
}

/// @brief Whether @p left is greater than @p right.
/// @param left A value.
/// @param right Another value.
constexpr bool operator>(const int128& left, const int128& right) noexcept { return right < left; }

/// @brief Whether @p left is at most @p right.
/// @param left A value.
/// @param right Another value.
constexpr bool operator<=(const int128& left, const int128& right) noexcept { return !(right < left); }

/// @brief Whether @p left is at least @p right.
/// @param left A value.
/// @param right Another value.
constexpr bool operator>=(const int128& left, const int128& right) noexcept { return !(left < right); }

/// @brief Whether @p left and @p right are the same value.
/// @param left A value.
/// @param right Another value.
constexpr bool operator==(const uint128& left, const uint128& right) noexcept {
    return left.low == right.low && left.high == right.high;
}

/// @brief Whether @p left and @p right are different values.
/// @param left A value.
/// @param right Another value.
constexpr bool operator!=(const uint128& left, const uint128& right) noexcept { return !(left == right); }

/// @brief Whether @p left is less than @p right: their high halves compared, and where equal their low halves.
/// @param left A value.
/// @param right Another value.
constexpr bool operator<(const uint128& left, const uint128& right) noexcept {
    return left.high < right.high || (left.high == right.high && left.low < right.low);
}

/// @brief Whether @p left is greater than @p right.
/// @param left A value.
/// @param right Another value.
constexpr bool operator>(const uint128& left, const uint128& right) noexcept { return right < left; }

/// @brief Whether @p left is at most @p right.
/// @param left A value.
/// @param right Another value.
constexpr bool operator<=(const uint128& left, const uint128& right) noexcept { return !(right < left); }

/// @brief Whether @p left is at least @p right.
/// @param left A value.
/// @param right Another value.
constexpr bool operator>=(const uint128& left, const uint128& right) noexcept { return !(left < right); }

/// @brief The value in decimal, exactly, with a minus sign where it is negative: such as "-18446744073709551616".
/// @param value The value.
/// @note Allocates the string.
inline std::string to_string(const int128& value) {
    char digits[41];
    char* const end = digits + sizeof digits;
    return std::string(detail::write_decimal(end, value.magnitude_high(), value.magnitude_low(), value.high < 0), end);
}

/// @brief The value in decimal, exactly: such as "18446744073709551616".
/// @param value The value.
/// @note Allocates the string.
inline std::string to_string(const uint128& value) {
    char digits[41];
    char* const end = digits + sizeof digits;
    return std::string(detail::write_decimal(end, value.high, value.low, false), end);
}

/// @brief Writes @p value to @p stream in decimal, as kahlnws::to_string gives it.
/// @tparam Char The stream's character type.
/// @tparam Traits The stream's character traits.
/// @param[in,out] stream The stream; the program includes @c <ostream> to use it.
/// @param value The value.
/// @return @p stream.
template <class Char, class Traits>
std::basic_ostream<Char, Traits>& operator<<(std::basic_ostream<Char, Traits>& stream, const int128& value) {
    char digits[41];
    digits[40] = '\0';
    return stream << detail::write_decimal(digits + 40, value.magnitude_high(), value.magnitude_low(), value.high < 0);
}

/// @brief Writes @p value to @p stream in decimal, as kahlnws::to_string gives it.
/// @tparam Char The stream's character type.
/// @tparam Traits The stream's character traits.
/// @param[in,out] stream The stream; the program includes @c <ostream> to use it.
/// @param value The value.
/// @return @p stream.
template <class Char, class Traits>
std::basic_ostream<Char, Traits>& operator<<(std::basic_ostream<Char, Traits>& stream, const uint128& value) {
    char digits[41];
    digits[40] = '\0';
    return stream << detail::write_decimal(digits + 40, value.high, value.low, false);
}

/// @brief A view of @c size() elements of type @p T stored contiguously elsewhere: a table of a kahlnws::problem, or a
/// buffer kahlnws::network_simplex::try_solve_into or try_resolve_into writes to.
///
/// Its invariant: the pointer is null only where the size is zero, and the size counts elements within PTRDIFF_MAX
/// bytes of one object. Every constructor keeps it; copying a span copies the view, never the elements.
///
/// It is made from
/// - a lvalue container whose @c data() gives a pointer to its contiguous elements and whose @c size() gives their
///   number, such as a @c std::vector or a @c std::array, implicitly, so that a container is passed where a span is
///   asked for;
/// - a C array, implicitly;
/// - another kahlnws::span, with @c const added or not, and, from C++20 on, a @c std::span: views, also temporary;
/// - a pointer and a count.
///
/// A temporary container, a temporary array and a braced list are refused at compile time, since the view would
/// outlive them. A temporary view of another kind is refused too, since the header cannot tell it from a container:
/// name it first, or pass its pointer and its count.
///
/// The element type matches exactly, with @c const added at most: a table of @c std::int32_t is not viewed as
/// @c std::int64_t. Types that are the same on one platform are distinct on another: @c std::uint64_t and
/// @c unsigned @c long are one type where @c long has 64 bits and @c std::uint64_t is defined as it, and two types
/// elsewhere, so a portable program keeps its tables in vectors of @c std::size_t, @c std::uint64_t and
/// @c std::int64_t, the types the problem's tables name.
///
/// @warning What a span views must outlive it and must not move: a @c std::vector that grows after the span is made
/// may move its elements away from it.
/// @tparam T The element type: @c const for a table that is only read, plain for a buffer that is written.
template <class T>
class span {
public:
    /// @brief The element type, @c const where the span only reads.
    typedef T element_type;
    /// @brief The element type without @c const.
    typedef typename std::remove_cv<T>::type value_type;
    /// @brief The type of a size and an index.
    typedef std::size_t size_type;
    /// @brief The type of a difference of two iterators.
    typedef std::ptrdiff_t difference_type;
    /// @brief A pointer to an element.
    typedef T* pointer;
    /// @brief A pointer to an element that only reads.
    typedef const T* const_pointer;
    /// @brief A reference to an element.
    typedef T& reference;
    /// @brief A reference to an element that only reads.
    typedef const T& const_reference;
    /// @brief An iterator over the elements, a pointer.
    typedef T* iterator;

    /// @brief An empty view.
    constexpr span() noexcept : data_(nullptr), size_(0) {}

    /// @brief A view of the @p count elements from @p first.
    /// @tparam Pointer A pointer to @p T, or to @p T without @c const; a null pointer constant is refused at compile
    /// time, so an empty view is made by the default constructor.
    /// @param first The first element.
    /// @param count The number of elements.
    /// @pre @p first is not null, or @p count is zero; checked in every build.
    /// @pre @p count elements lie within PTRDIFF_MAX bytes; checked in every build.
    /// @pre The @p count elements from @p first lie within one object.
    template <class Pointer,
              typename std::enable_if<std::is_pointer<Pointer>::value &&
                                          detail::is_viewable_as<typename std::remove_pointer<Pointer>::type, T>::value,
                                      int>::type = 0>
    constexpr span(Pointer first, size_type count) noexcept
        : data_(detail::view_start<T>(first, count)), size_(count) {}

    /// @brief A view of the elements of @p range.
    /// @tparam Range A lvalue container, or from C++20 on a @c std::span of any kind, whose elements may be viewed as
    /// @p T.
    /// @param[in] range The range; it must outlive the view.
    /// @pre The size of @p range is not negative and fits a @c std::size_t; checked in every build.
    template <class Range, typename std::enable_if<detail::is_viewable_range<Range, T>::value, int>::type = 0>
    constexpr span(Range&& range) noexcept(noexcept(std::declval<Range&>().data()) &&
                                           noexcept(std::declval<Range&>().size()))
        : span(range.data(),
               detail::count_of(range.size(), std::is_signed<decltype(std::declval<Range&>().size())>())) {}

    /// @brief A view of the C array @p array.
    ///
    /// The array's type is deduced from a lvalue alone, so a temporary array and a braced list, which would end
    /// before the view of them is used, match no constructor.
    /// @tparam Array An array type whose elements may be viewed as @p T.
    /// @param[in] array The array; it must outlive the view.
    template <class Array,
              typename std::enable_if<std::is_array<Array>::value && std::extent<Array>::value != 0 &&
                                          detail::is_viewable_as<typename std::remove_extent<Array>::type, T>::value,
                                      int>::type = 0>
    constexpr span(Array& array) noexcept : data_(array), size_(std::extent<Array>::value) {}

    /// @brief A view of what @p other views, with @c const added.
    /// @tparam U @p T without @c const.
    /// @param other The view.
    template <class U,
              typename std::enable_if<!std::is_same<U, T>::value && detail::is_viewable_as<U, T>::value, int>::type = 0>
    constexpr span(const span<U>& other) noexcept : data_(other.data()), size_(other.size()) {}

    /// @brief The first element viewed; null where the view is empty and was made without a pointer.
    constexpr pointer data() const noexcept { return data_; }

    /// @brief The number of elements viewed.
    constexpr size_type size() const noexcept { return size_; }

    /// @brief Whether no element is viewed.
    constexpr bool empty() const noexcept { return size_ == 0; }

    /// @brief The first element viewed, to iterate from.
    constexpr iterator begin() const noexcept { return data_; }

    /// @brief Past the last element viewed, to iterate to.
    constexpr iterator end() const noexcept { return data_ + size_; }

    /// @brief The element at @p index.
    /// @param index The position of the element.
    /// @pre @p index is below size(); checked by @c assert alone, where @c NDEBUG is not defined.
    constexpr reference operator[](size_type index) const noexcept { return assert(index < size_), data_[index]; }

private:
    pointer data_;
    size_type size_;
};

/// @brief A per-arc table of a kahlnws::problem whose length is compared with the tails'.
enum class table {
    /// @brief The node each arc enters.
    heads,
    /// @brief The least flow of each arc.
    lower_bounds,
    /// @brief The most flow of each arc.
    capacities,
    /// @brief The cost of a unit of flow on each arc.
    costs,
};

/// @brief One min-cost flow problem, viewed: arcs 0 to `tails.size() - 1` between nodes 0 to `supplies.size() - 1`.
///
/// Plain data, the six tables in the order of the C interface: arc a is entry a of every per-arc table, and node v
/// entry v of the supplies. It owns none of them. Whether the tables agree in length, and whether their numbers lie in
/// the range a solve admits, is the solve's to check: one that does not is refused with a kahlnws::refusal naming the
/// breach. Self-loops and parallel arcs are allowed.
///
/// A problem is filled in by name, so that two tables of the same type cannot be swapped unseen:
/// @code{.cpp}
/// kahlnws::problem problem;  // C++11 on
/// problem.tails = tails;
/// problem.heads = heads;
/// problem.capacities = capacities;
/// problem.costs = costs;
/// problem.supplies = supplies;
///
/// const kahlnws::problem same = {.tails = tails, .heads = heads, .capacities = capacities, .costs = costs,
///                                .supplies = supplies};  // C++20 on
/// @endcode
/// @warning The tables must outlive every solve of the problem and must not change while one runs; a temporary
/// container is refused at compile time.
struct problem {
    /// @brief The node each arc leaves, one per arc: its length is the arc count.
    span<const std::size_t> tails;
    /// @brief The node each arc enters, one per arc.
    span<const std::size_t> heads;
    /// @brief The least flow of each arc, in units of flow, one per arc; empty, as by default, where every arc's is
    /// zero, which with no arc is the same.
    span<const std::uint64_t> lower_bounds;
    /// @brief The most flow of each arc, in units of flow, one per arc.
    span<const std::uint64_t> capacities;
    /// @brief The cost of a unit of flow on each arc, in units of cost, of either sign, one per arc.
    span<const std::int64_t> costs;
    /// @brief The supply of each node, in units of flow, positive where it sends flow and negative where it takes
    /// flow: its length is the node count.
    span<const std::int64_t> supplies;
};

/// @brief What one solve did, counted in pivots: exact counts of that solve, not samples or estimates.
///
/// A pivot is an arc brought into the basis, a spanning tree, or flipped to its other bound. Either way the arc
/// violated its bound, and the most flow the cycle it closes with the tree carries is sent around that cycle: where a
/// link of the tree has the least room, that link leaves and the arc takes its place; where the arc's own capacity
/// less its lower bound is the least, no link leaves and the arc moves to its other bound, a flip.
///
/// Plain data, an aggregate: `kahlnws::work counted = {};` is all zero. The counts of a solve depend on the problem
/// alone, its arc order included, and on the library's version; those of a resolve on the problem and on the solves the
/// solver made before it, and are those of a solve where it does not start from the kept basis. In every work a solve
/// or a resolve answers, `degenerate_pivots <= pivots` and `first_arc_pivots <= pivots`, and the pivots that sent flow
/// are `pivots - degenerate_pivots`.
struct work {
    /// @brief The pivots the solve made, flips included; 0 where no arc ever violated its bound.
    std::uint64_t pivots;
    /// @brief Of the pivots, those that sent no flow, so that only the tree and the potentials changed; never a flip.
    std::uint64_t degenerate_pivots;
    /// @brief Of the pivots, those made by the arcs the solve tries before block search, chosen from the problem's
    /// structure; 0 in a resolve from the kept basis, which tries none; @c pivots where those arcs alone reached the
    /// outcome.
    std::uint64_t first_arc_pivots;
};

/// @brief Whether @p left and @p right hold the same three counts.
/// @param left A work.
/// @param right Another work.
constexpr bool operator==(const work& left, const work& right) noexcept {
    return left.pivots == right.pivots && left.degenerate_pivots == right.degenerate_pivots &&
           left.first_arc_pivots == right.first_arc_pivots;
}

/// @brief Whether @p left and @p right differ in a count.
/// @param left A work.
/// @param right Another work.
constexpr bool operator!=(const work& left, const work& right) noexcept { return !(left == right); }

/// @brief What an optimal kahlnws::network_simplex::try_solve_into or try_resolve_into answers beside the flows, and
/// the potentials where asked, that it wrote to the caller's buffers. Plain data.
struct optimum {
    /// @brief The cost of the flow, in units of cost times units of flow.
    int128 total_cost;
};

/// @brief Whether @p left and @p right hold the same cost.
/// @param left An optimum.
/// @param right Another optimum.
constexpr bool operator==(const optimum& left, const optimum& right) noexcept {
    return left.total_cost == right.total_cost;
}

/// @brief Whether @p left and @p right hold different costs.
/// @param left An optimum.
/// @param right Another optimum.
constexpr bool operator!=(const optimum& left, const optimum& right) noexcept { return !(left == right); }

/// @brief What an optimal kahlnws::network_simplex::try_solve or try_resolve answers, owned: the flows, the potentials
/// and the cost.
///
/// Plain data. With the potentials, `costs[a] + potentials[tails[a]] - potentials[heads[a]]` is at least zero on every
/// arc a whose flow is below its capacity and at most zero on every arc whose flow is above its lower bound, which
/// proves the flow optimal.
struct solution {
    /// @brief The flow of each arc, in units of flow, one per arc.
    std::vector<std::uint64_t> flows;
    /// @brief The potential of each node, in units of cost, one per node.
    std::vector<std::int64_t> potentials;
    /// @brief The cost of the flow, in units of cost times units of flow.
    int128 total_cost;
};

/// @brief Whether @p left and @p right hold the same flows, potentials and cost.
/// @param left A solution.
/// @param right Another solution.
inline bool operator==(const solution& left, const solution& right) noexcept {
    return left.flows == right.flows && left.potentials == right.potentials && left.total_cost == right.total_cost;
}

/// @brief Whether @p left and @p right differ in a flow, a potential or the cost.
/// @param left A solution.
/// @param right Another solution.
inline bool operator!=(const solution& left, const solution& right) noexcept { return !(left == right); }

/// @brief What a solve found: an optimal flow, with what the solve answers about it, or that no flow within the bounds
/// meets the supplies.
///
/// A value that compares by value. The one accessor of the optimum answers it or null, so it cannot be read from an
/// infeasible outcome:
/// @code{.cpp}
/// if (const kahlnws::solution* solution = answer.outcome.optimal()) { /* optimal */ } else { /* infeasible */ }
/// @endcode
/// @tparam T kahlnws::solution for the high level, which owns its answer, and kahlnws::optimum for the low level,
/// which wrote the flows to the caller's buffers.
template <class T>
class outcome {
public:
    /// @brief What an optimal outcome holds.
    typedef T value_type;

    /// @brief An infeasible outcome.
    outcome() : state_(state::infeasible), optimum_() {}

    /// @brief An optimal outcome holding @p optimum.
    /// @param optimum What the solve answers about the optimal flow.
    static outcome make_optimal(T optimum) noexcept { return outcome(std::move(optimum)); }

    /// @brief The optimum, to read or to move from; null where the outcome is infeasible.
    T* optimal() noexcept { return state_ == state::optimal ? &optimum_ : nullptr; }

    /// @brief The optimum; null where the outcome is infeasible.
    const T* optimal() const noexcept { return state_ == state::optimal ? &optimum_ : nullptr; }

    /// @brief Whether a flow of least cost was found.
    bool is_optimal() const noexcept { return state_ == state::optimal; }

    /// @brief Whether no flow within the bounds meets the supplies.
    bool is_infeasible() const noexcept { return state_ == state::infeasible; }

private:
    enum class state { optimal, infeasible };

    explicit outcome(T&& optimum) noexcept : state_(state::optimal), optimum_(std::move(optimum)) {}

    state state_;
    /// The optimum where `state_` is optimal; a value-initialised `T`, read by nothing, where it is infeasible.
    T optimum_;
};

/// @brief Whether @p left and @p right are the same outcome: both infeasible, or both optimal with equal optima.
/// @tparam T What an optimal outcome holds.
/// @param left An outcome.
/// @param right Another outcome.
template <class T>
bool operator==(const outcome<T>& left, const outcome<T>& right) noexcept {
    const T* const left_optimum = left.optimal();
    const T* const right_optimum = right.optimal();
    return left_optimum == nullptr || right_optimum == nullptr ? left_optimum == right_optimum
                                                               : *left_optimum == *right_optimum;
}

/// @brief Whether @p left and @p right are different outcomes.
/// @tparam T What an optimal outcome holds.
/// @param left An outcome.
/// @param right Another outcome.
template <class T>
bool operator!=(const outcome<T>& left, const outcome<T>& right) noexcept {
    return !(left == right);
}

/// @brief What a solve answers: its outcome and the work it did to reach it, as one value, so that the counts are
/// never read for another solve than the one they count.
///
/// A value that compares by value, with public members. A default answer is infeasible with zero work.
/// @tparam T kahlnws::solution or kahlnws::optimum, as for kahlnws::outcome.
template <class T>
struct answer {
    /// @brief What the solve found.
    kahlnws::outcome<T> outcome;
    /// @brief The work the solve did, optimal or infeasible alike.
    kahlnws::work work;

    /// @brief An infeasible answer with zero work.
    answer() : outcome(), work() {}

    /// @brief The answer of @p found and @p done.
    /// @param found What the solve found.
    /// @param done The work the solve did.
    answer(kahlnws::outcome<T> found, const kahlnws::work& done) noexcept : outcome(std::move(found)), work(done) {}
};

/// @brief Whether @p left and @p right hold the same outcome and the same work.
/// @tparam T What an optimal outcome holds.
/// @param left An answer.
/// @param right Another answer.
template <class T>
bool operator==(const answer<T>& left, const answer<T>& right) noexcept {
    return left.outcome == right.outcome && left.work == right.work;
}

/// @brief Whether @p left and @p right differ in their outcome or their work.
/// @tparam T What an optimal outcome holds.
/// @param left An answer.
/// @param right Another answer.
template <class T>
bool operator!=(const answer<T>& left, const answer<T>& right) noexcept {
    return !(left == right);
}

/// @brief Which breach a kahlnws::refusal names, in the order the breaches are looked for.
enum class refusal_kind {
    /// @brief A per-arc table holds another number of entries than the tails; values in kahlnws::breach::length.
    length,
    /// @brief More nodes or arcs than a solve counts: nodes + 1 or arcs + nodes passes 2^32 - 1.
    too_large,
    /// @brief An arc leaves a node the problem does not have.
    tail_outside_nodes,
    /// @brief An arc enters a node the problem does not have.
    head_outside_nodes,
    /// @brief An arc's lower bound passes its capacity.
    lower_above_capacity,
    /// @brief An arc's cost lies beyond plus or minus kahlnws::cost_bound of the node count.
    cost_beyond_bound,
    /// @brief The magnitudes of the costs times the capacities, added up in arc order, pass 2^127 - 1.
    cost_total_beyond_range,
    /// @brief The supplies do not add up to zero.
    unbalanced,
    /// @brief The supplies net of the lower bounds add up past 2^64 - 2 where they are positive; a node's net supply
    /// is its supply, less the lower bounds of the arcs that leave it, plus those of the arcs that enter it.
    net_supply_beyond_range,
};

/// @brief The values of each breach a kahlnws::refusal names, one plain struct per kahlnws::refusal_kind.
namespace breach {

/// @brief The values of a breach of kind kahlnws::refusal_kind::length.
struct length {
    /// @brief The first table, in the order of kahlnws::table, of another length than the tails.
    kahlnws::table table;
    /// @brief Its number of entries.
    std::size_t length;
    /// @brief The tails' number of entries.
    std::size_t expected;
};

/// @brief The values of a breach of kind kahlnws::refusal_kind::too_large.
struct too_large {
    /// @brief The number of nodes.
    std::size_t node_count;
    /// @brief The number of arcs.
    std::size_t arc_count;
};

/// @brief The values of a breach of kind kahlnws::refusal_kind::tail_outside_nodes.
struct tail_outside_nodes {
    /// @brief The arc.
    std::size_t arc;
    /// @brief Its tail.
    std::size_t tail;
    /// @brief The number of nodes.
    std::size_t node_count;
};

/// @brief The values of a breach of kind kahlnws::refusal_kind::head_outside_nodes.
struct head_outside_nodes {
    /// @brief The arc.
    std::size_t arc;
    /// @brief Its head.
    std::size_t head;
    /// @brief The number of nodes.
    std::size_t node_count;
};

/// @brief The values of a breach of kind kahlnws::refusal_kind::lower_above_capacity.
struct lower_above_capacity {
    /// @brief The arc.
    std::size_t arc;
    /// @brief Its lower bound, in units of flow.
    std::uint64_t lower_bound;
    /// @brief Its capacity, in units of flow.
    std::uint64_t capacity;
};

/// @brief The values of a breach of kind kahlnws::refusal_kind::cost_beyond_bound.
struct cost_beyond_bound {
    /// @brief The arc.
    std::size_t arc;
    /// @brief Its cost, in units of cost.
    std::int64_t cost;
    /// @brief The number of nodes, which sets the bound.
    std::size_t node_count;
};

/// @brief The values of a breach of kind kahlnws::refusal_kind::cost_total_beyond_range.
struct cost_total_beyond_range {
    /// @brief The arc at which the sum passes 2^127 - 1.
    std::size_t arc;
};

/// @brief The values of a breach of kind kahlnws::refusal_kind::unbalanced.
struct unbalanced {
    /// @brief The sum of the supplies, in units of flow.
    int128 total;
};

/// @brief The values of a breach of kind kahlnws::refusal_kind::net_supply_beyond_range.
struct net_supply_beyond_range {
    /// @brief The sum of the positive supplies net of the lower bounds, in units of flow.
    uint128 total;
};

}  // namespace breach

/// @brief Why a solve refused a problem: the breach, by its kind, and the values that make it one.
///
/// A problem with several breaches is refused with the first in the order of kahlnws::refusal_kind, where the kinds
/// about one arc, from tail_outside_nodes to cost_total_beyond_range, are taken arc by arc: the lowest arc with any of
/// them, and its first. A value, made from the values of one breach; the accessor of its kind answers them, and every
/// other accessor null:
/// @code{.cpp}
/// if (const kahlnws::breach::unbalanced* unbalanced = refusal.unbalanced()) {
///     std::cerr << "the supplies add up to " << unbalanced->total << "\n";
/// }
/// @endcode
class refusal {
public:
    /// @brief A refusal for tables of different lengths. @param values The values of the breach.
    constexpr refusal(const breach::length& values) noexcept : kind_(refusal_kind::length), length_(values) {}
    /// @brief A refusal for too many nodes or arcs. @param values The values of the breach.
    constexpr refusal(const breach::too_large& values) noexcept
        : kind_(refusal_kind::too_large), too_large_(values) {}
    /// @brief A refusal for a tail outside the nodes. @param values The values of the breach.
    constexpr refusal(const breach::tail_outside_nodes& values) noexcept
        : kind_(refusal_kind::tail_outside_nodes), tail_outside_nodes_(values) {}
    /// @brief A refusal for a head outside the nodes. @param values The values of the breach.
    constexpr refusal(const breach::head_outside_nodes& values) noexcept
        : kind_(refusal_kind::head_outside_nodes), head_outside_nodes_(values) {}
    /// @brief A refusal for a lower bound above its capacity. @param values The values of the breach.
    constexpr refusal(const breach::lower_above_capacity& values) noexcept
        : kind_(refusal_kind::lower_above_capacity), lower_above_capacity_(values) {}
    /// @brief A refusal for a cost beyond its bound. @param values The values of the breach.
    constexpr refusal(const breach::cost_beyond_bound& values) noexcept
        : kind_(refusal_kind::cost_beyond_bound), cost_beyond_bound_(values) {}
    /// @brief A refusal for costs times capacities beyond range. @param values The values of the breach.
    constexpr refusal(const breach::cost_total_beyond_range& values) noexcept
        : kind_(refusal_kind::cost_total_beyond_range), cost_total_beyond_range_(values) {}
    /// @brief A refusal for supplies that do not balance. @param values The values of the breach.
    constexpr refusal(const breach::unbalanced& values) noexcept
        : kind_(refusal_kind::unbalanced), unbalanced_(values) {}
    /// @brief A refusal for net supplies beyond range. @param values The values of the breach.
    constexpr refusal(const breach::net_supply_beyond_range& values) noexcept
        : kind_(refusal_kind::net_supply_beyond_range), net_supply_beyond_range_(values) {}

    /// @brief Which breach it names.
    constexpr refusal_kind kind() const noexcept { return kind_; }

    /// @brief The values of a breach of kind length; null for another kind.
    const breach::length* length() const noexcept { return kind_ == refusal_kind::length ? &length_ : nullptr; }

    /// @brief The values of a breach of kind too_large; null for another kind.
    const breach::too_large* too_large() const noexcept {
        return kind_ == refusal_kind::too_large ? &too_large_ : nullptr;
    }

    /// @brief The values of a breach of kind tail_outside_nodes; null for another kind.
    const breach::tail_outside_nodes* tail_outside_nodes() const noexcept {
        return kind_ == refusal_kind::tail_outside_nodes ? &tail_outside_nodes_ : nullptr;
    }

    /// @brief The values of a breach of kind head_outside_nodes; null for another kind.
    const breach::head_outside_nodes* head_outside_nodes() const noexcept {
        return kind_ == refusal_kind::head_outside_nodes ? &head_outside_nodes_ : nullptr;
    }

    /// @brief The values of a breach of kind lower_above_capacity; null for another kind.
    const breach::lower_above_capacity* lower_above_capacity() const noexcept {
        return kind_ == refusal_kind::lower_above_capacity ? &lower_above_capacity_ : nullptr;
    }

    /// @brief The values of a breach of kind cost_beyond_bound; null for another kind.
    const breach::cost_beyond_bound* cost_beyond_bound() const noexcept {
        return kind_ == refusal_kind::cost_beyond_bound ? &cost_beyond_bound_ : nullptr;
    }

    /// @brief The values of a breach of kind cost_total_beyond_range; null for another kind.
    const breach::cost_total_beyond_range* cost_total_beyond_range() const noexcept {
        return kind_ == refusal_kind::cost_total_beyond_range ? &cost_total_beyond_range_ : nullptr;
    }

    /// @brief The values of a breach of kind unbalanced; null for another kind.
    const breach::unbalanced* unbalanced() const noexcept {
        return kind_ == refusal_kind::unbalanced ? &unbalanced_ : nullptr;
    }

    /// @brief The values of a breach of kind net_supply_beyond_range; null for another kind.
    const breach::net_supply_beyond_range* net_supply_beyond_range() const noexcept {
        return kind_ == refusal_kind::net_supply_beyond_range ? &net_supply_beyond_range_ : nullptr;
    }

private:
    refusal_kind kind_;
    /// The values of the breach `kind_` names: the one member of that name.
    union {
        breach::length length_;
        breach::too_large too_large_;
        breach::tail_outside_nodes tail_outside_nodes_;
        breach::head_outside_nodes head_outside_nodes_;
        breach::lower_above_capacity lower_above_capacity_;
        breach::cost_beyond_bound cost_beyond_bound_;
        breach::cost_total_beyond_range cost_total_beyond_range_;
        breach::unbalanced unbalanced_;
        breach::net_supply_beyond_range net_supply_beyond_range_;
    };
};

/// @brief What failed in a call that answered a kahlnws::error.
enum class error_kind {
    /// @brief The problem lies outside what a solve admits; kahlnws::error::refusal() names the breach.
    refused_problem,
    /// @brief An argument the call does not admit: a buffer of the wrong length, or one that shares storage with a
    /// table of the problem or with the other buffer.
    invalid_argument,
    /// @brief The memory the call needed was refused.
    refused_memory,
    /// @brief The library failed: a defect of it, or a library built from other sources than this header.
    internal_error,
};

/// @brief The name of @p kind for a person to read: "refused problem", "invalid argument", "refused memory" or
/// "internal error".
/// @param kind The kind.
/// @return A static string, or "unknown error kind" for a number no kind has.
inline const char* name(error_kind kind) noexcept {
    switch (kind) {
    case error_kind::refused_problem:
        return "refused problem";
    case error_kind::invalid_argument:
        return "invalid argument";
    case error_kind::refused_memory:
        return "refused memory";
    case error_kind::internal_error:
        return "internal error";
    }
    return "unknown error kind";
}

/// @brief The name of @p kind for a person to read, its enumerator in words: such as "tail outside nodes".
/// @param kind The kind.
/// @return A static string, or "unknown refusal kind" for a number no kind has.
inline const char* name(refusal_kind kind) noexcept {
    switch (kind) {
    case refusal_kind::length:
        return "length";
    case refusal_kind::too_large:
        return "too large";
    case refusal_kind::tail_outside_nodes:
        return "tail outside nodes";
    case refusal_kind::head_outside_nodes:
        return "head outside nodes";
    case refusal_kind::lower_above_capacity:
        return "lower above capacity";
    case refusal_kind::cost_beyond_bound:
        return "cost beyond bound";
    case refusal_kind::cost_total_beyond_range:
        return "cost total beyond range";
    case refusal_kind::unbalanced:
        return "unbalanced";
    case refusal_kind::net_supply_beyond_range:
        return "net supply beyond range";
    }
    return "unknown refusal kind";
}

/// @brief The name of @p which as a message names it: "heads", "lower bounds", "capacities" or "costs".
/// @param which The table.
/// @return A static string, or "unknown table" for a number no table has.
inline const char* name(table which) noexcept {
    switch (which) {
    case table::heads:
        return "heads";
    case table::lower_bounds:
        return "lower bounds";
    case table::capacities:
        return "capacities";
    case table::costs:
        return "costs";
    }
    return "unknown table";
}

/// @brief Why a call failed: its kind, the breach of a refused problem, and a message for a person to read.
///
/// A value, copied freely, that holds its message: making one allocates nothing. The message follows the library's
/// form: lower case, no full stop, the offending values; for a refused problem it is the library's text of the breach.
class error {
public:
    /// @brief The error of a problem refused for @p breach, with the library's message of it.
    /// @param breach The breach.
    explicit error(const kahlnws::refusal& breach) noexcept : kind_(error_kind::refused_problem), refusal_(breach) {
        detail::message_writer writer(message_, sizeof message_);
        if (const breach::length* values = breach.length()) {
            writer.write("the ");
            writer.write(name(values->table));
            writer.write(" have length ");
            writer.write(values->length);
            writer.write(" where the tails have length ");
            writer.write(values->expected);
        } else {
            const kahlnws_refusal laid_out = c_refusal(breach);
            kahlnws_refusal_message(&laid_out, message_, sizeof message_);
        }
    }

    /// @brief What failed.
    error_kind kind() const noexcept { return kind_; }

    /// @brief The breach of a refused problem; null for another kind.
    const kahlnws::refusal* refusal() const noexcept {
        return kind_ == error_kind::refused_problem ? &refusal_ : nullptr;
    }

    /// @brief The message for a person to read, a NUL-terminated string the error holds.
    const char* message() const noexcept { return message_; }

private:
    friend class network_simplex;
    friend result<std::int64_t> try_cost_bound(std::size_t) noexcept;

    /// A placeholder for the refusal of an error of another kind.
    struct no_refusal {};

    /// The error of `kind`, a kind other than refused_problem, with the message of `pieces` one after the other.
    template <class... Pieces>
    explicit error(error_kind kind, const Pieces&... pieces) noexcept : kind_(kind), no_refusal_() {
        detail::message_writer writer(message_, sizeof message_);
        const int written[] = {0, (writer.write(pieces), 0)...};
        static_cast<void>(written);
    }

    /// The error of `status`, a status the call does not answer with a value.
    static error of_status(kahlnws_status status) noexcept {
        switch (status) {
        case KAHLNWS_STATUS_NO_ROOM:
            return refused_memory();
        case KAHLNWS_STATUS_INTERNAL_ERROR:
            return internal();
        }
        // Every other status is one the header rules out or the call answers with a value: only a defect of the
        // library, or a library built from other sources than the header, answers it here.
        return error(error_kind::internal_error, "the library answered the status ", std::uint64_t(status), " (",
                     kahlnws_status_name(status), "), which the call cannot answer");
    }

    /// The error of the memory the call needed being refused.
    static error refused_memory() noexcept {
        return error(error_kind::refused_memory, "the memory the solver needed was refused");
    }

    /// The error of an internal error of the library caught.
    static error internal() noexcept {
        return error(error_kind::internal_error, "an internal error of the library was caught");
    }

    /// The error of the refusal the library wrote: the refused problem of its breach, or an internal error where it
    /// names a kind this header does not know, which only a library built from other sources gives.
    static error of_refusal(const kahlnws_refusal& written) noexcept {
        switch (written.kind) {
        case KAHLNWS_REFUSAL_KIND_TOO_LARGE:
            return error(breach::too_large{written.node_count, written.arc_count});
        case KAHLNWS_REFUSAL_KIND_TAIL_OUTSIDE_NODES:
            return error(breach::tail_outside_nodes{written.arc, written.node, written.node_count});
        case KAHLNWS_REFUSAL_KIND_HEAD_OUTSIDE_NODES:
            return error(breach::head_outside_nodes{written.arc, written.node, written.node_count});
        case KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY:
            return error(breach::lower_above_capacity{written.arc, written.lower_bound, written.capacity});
        case KAHLNWS_REFUSAL_KIND_COST_BEYOND_BOUND:
            return error(breach::cost_beyond_bound{written.arc, written.cost, written.node_count});
        case KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE:
            return error(breach::cost_total_beyond_range{written.arc});
        case KAHLNWS_REFUSAL_KIND_UNBALANCED:
            return error(breach::unbalanced{int128::from_halves(written.supply_total.low, written.supply_total.high)});
        case KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE:
            return error(breach::net_supply_beyond_range{
                uint128::from_halves(written.net_supply_total.low, written.net_supply_total.high)});
        }
        return error(error_kind::internal_error, "the library refused the problem with the breach kind ",
                     std::uint64_t(written.kind), ", which this header does not know");
    }

    /// The refusal as the C interface lays it out, for its message; a refusal of kind length has no such form and
    /// is laid out as one that names no breach.
    static kahlnws_refusal c_refusal(const kahlnws::refusal& breach) noexcept {
        kahlnws_refusal laid_out = kahlnws_refusal();
        if (const breach::too_large* values = breach.too_large()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_TOO_LARGE;
            laid_out.node_count = values->node_count;
            laid_out.arc_count = values->arc_count;
        } else if (const breach::tail_outside_nodes* tail = breach.tail_outside_nodes()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_TAIL_OUTSIDE_NODES;
            laid_out.arc = tail->arc;
            laid_out.node = tail->tail;
            laid_out.node_count = tail->node_count;
        } else if (const breach::head_outside_nodes* head = breach.head_outside_nodes()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_HEAD_OUTSIDE_NODES;
            laid_out.arc = head->arc;
            laid_out.node = head->head;
            laid_out.node_count = head->node_count;
        } else if (const breach::lower_above_capacity* lower = breach.lower_above_capacity()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY;
            laid_out.arc = lower->arc;
            laid_out.lower_bound = lower->lower_bound;
            laid_out.capacity = lower->capacity;
        } else if (const breach::cost_beyond_bound* cost = breach.cost_beyond_bound()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_COST_BEYOND_BOUND;
            laid_out.arc = cost->arc;
            laid_out.cost = cost->cost;
            laid_out.node_count = cost->node_count;
        } else if (const breach::cost_total_beyond_range* total = breach.cost_total_beyond_range()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE;
            laid_out.arc = total->arc;
        } else if (const breach::unbalanced* unbalanced = breach.unbalanced()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_UNBALANCED;
            laid_out.supply_total.low = unbalanced->total.low;
            laid_out.supply_total.high = unbalanced->total.high;
        } else if (const breach::net_supply_beyond_range* net = breach.net_supply_beyond_range()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_NET_SUPPLY_BEYOND_RANGE;
            laid_out.net_supply_total.low = net->total.low;
            laid_out.net_supply_total.high = net->total.high;
        }
        return laid_out;
    }

    error_kind kind_;
    /// The breach where `kind_` is refused_problem; the placeholder otherwise.
    union {
        no_refusal no_refusal_;
        kahlnws::refusal refusal_;
    };
    /// The message, NUL-terminated. The longest message the header writes, a too_large refusal with both counts at
    /// 2^64 - 1, has 146 bytes; 160 holds it with its NUL, and a longer one would be cut, never overrun.
    char message_[160];
};

#if KAHLNWS_EXCEPTIONS

/// @brief The exception of a refused problem: what() is the library's message of the breach, and refusal() the
/// breach.
///
/// Thrown by the throwing forms where the @c try_ form answers an error of kind error_kind::refused_problem. Only where
/// @c KAHLNWS_EXCEPTIONS is 1.
/// @note It has no key function, so every shared object that uses it holds its own copy of its type information: an
/// exception thrown in one shared object is caught by this type in another only where the platform merges those
/// copies or compares them by name, as the common platforms do for types of default visibility. Catching it as
/// @c std::invalid_argument holds everywhere.
class problem_error : public std::invalid_argument {
public:
    /// @brief The exception of a problem refused for @p breach.
    /// @param breach The breach.
    /// @throws std::bad_alloc Where the memory for the message is refused.
    explicit problem_error(const kahlnws::refusal& breach)
        : std::invalid_argument(error(breach).message()), refusal_(breach) {}

    /// @brief The breach the problem was refused for.
    const kahlnws::refusal& refusal() const noexcept { return refusal_; }

private:
    kahlnws::refusal refusal_;
};

/// @brief The exception of a failure of the library: a defect of it, or a library built from other sources than this
/// header. what() says which.
///
/// Thrown by the throwing forms where the @c try_ form answers an error of kind error_kind::internal_error. The solver
/// that threw it may be used again or destroyed; the buffers a solve was writing hold unspecified values. Only where
/// @c KAHLNWS_EXCEPTIONS is 1.
/// @note Its type information is held per shared object, as kahlnws::problem_error's; catching it as
/// @c std::runtime_error holds everywhere.
class internal_error : public std::runtime_error {
public:
    /// @brief An internal error described by @p what.
    /// @param what The description.
    /// @throws std::bad_alloc Where the memory for the description is refused.
    explicit internal_error(const std::string& what) : std::runtime_error(what) {}

    /// @brief An internal error described by @p what.
    /// @param what The description, a NUL-terminated string.
    /// @throws std::bad_alloc Where the memory for the description is refused.
    explicit internal_error(const char* what) : std::runtime_error(what) {}
};

namespace detail {

/// Throws the exception of `failure`: the one rule from an error to its exception.
[[noreturn]] inline void raise(const error& failure) {
    switch (failure.kind()) {
    case error_kind::refused_problem:
        throw problem_error(*failure.refusal());
    case error_kind::invalid_argument:
        throw std::invalid_argument(failure.message());
    case error_kind::refused_memory:
        throw std::bad_alloc();
    case error_kind::internal_error:
        break;
    }
    throw internal_error(failure.message());
}

}  // namespace detail

#endif

/// @brief The value of a call that succeeded, or the kahlnws::error of one that failed.
///
/// The answer of every @c try_ form, in the shape of the standard library's @c std::expected: test it with
/// has_value() or as a @c bool, then read the value with @c * or @c ->, or the error with error(). Where the language
/// allows, discarding one makes the compiler warn: from C++17 on everywhere, before it with Clang.
///
/// A wrong access never answers garbage: value() on an error throws, where @c KAHLNWS_EXCEPTIONS is 1, the exception
/// the throwing forms throw for that error; and ends the program otherwise. @c *, @c -> on an error and error() on a
/// value end the program in every build.
/// @tparam T The value; @c void for a call that answers nothing but success.
template <class T>
class KAHLNWS_HPP_NODISCARD result {
    static_assert(std::is_nothrow_move_constructible<T>::value, "a result holds a value that moves without throwing");

public:
    /// @brief The value of a call that succeeded.
    typedef T value_type;
    /// @brief The error of a call that failed.
    typedef kahlnws::error error_type;

    /// @brief A result holding a copy of @p held.
    /// @param held The value.
    result(const T& held) : state_(state::value) { new (&value_) T(held); }

    /// @brief A result holding @p held, moved.
    /// @param held The value.
    result(T&& held) noexcept : state_(state::value) { new (&value_) T(std::move(held)); }

    /// @brief A result holding @p failure.
    /// @param failure The error.
    result(const kahlnws::error& failure) noexcept : state_(state::error) { new (&error_) kahlnws::error(failure); }

    /// @brief A copy of @p other.
    /// @param other The result.
    result(const result& other) : state_(other.state_) {
        if (state_ == state::value) {
            new (&value_) T(other.value_);
        } else {
            new (&error_) kahlnws::error(other.error_);
        }
    }

    /// @brief What @p other held, moved.
    /// @param other The result.
    result(result&& other) noexcept : state_(other.state_) {
        if (state_ == state::value) {
            new (&value_) T(std::move(other.value_));
        } else {
            new (&error_) kahlnws::error(other.error_);
        }
    }

    /// @brief A copy of @p other; where the copy fails, this result is as it was.
    /// @param other The result.
    /// @return This result.
    result& operator=(const result& other) {
        if (this != &other) {
            result copy(other);
            *this = std::move(copy);
        }
        return *this;
    }

    /// @brief What @p other held, moved.
    /// @param other The result.
    /// @return This result.
    result& operator=(result&& other) noexcept {
        if (this != &other) {
            destroy();
            new (this) result(std::move(other));
        }
        return *this;
    }

    /// @brief Destroys what the result holds.
    ~result() { destroy(); }

    /// @brief Whether the call succeeded and the result holds its value.
    bool has_value() const noexcept { return state_ == state::value; }

    /// @brief Whether the call succeeded, as has_value().
    explicit operator bool() const noexcept { return has_value(); }

    /// @brief The value.
    /// @pre has_value(): where @c KAHLNWS_EXCEPTIONS is 1, a result holding an error throws its exception; otherwise
    /// the program ends.
    T& value() & {
        require_value();
        return value_;
    }

    /// @copydoc value()
    const T& value() const& {
        require_value();
        return value_;
    }

    /// @copydoc value()
    T&& value() && {
        require_value();
        return std::move(value_);
    }

    /// @brief The value.
    /// @pre has_value(); checked in every build.
    T& operator*() & noexcept { return checked_value(); }

    /// @copydoc operator*()
    const T& operator*() const& noexcept { return const_cast<result*>(this)->checked_value(); }

    /// @copydoc operator*()
    T&& operator*() && noexcept { return std::move(checked_value()); }

    /// @brief The value, to reach a member of.
    /// @pre has_value(); checked in every build.
    T* operator->() noexcept { return &checked_value(); }

    /// @copydoc operator->()
    const T* operator->() const noexcept { return &const_cast<result*>(this)->checked_value(); }

    /// @brief The error.
    /// @pre !has_value(); checked in every build.
    const kahlnws::error& error() const noexcept {
        if (state_ != state::error) {
            detail::precondition_failed("called `result::error` on a result that holds a value");
        }
        return error_;
    }

private:
    enum class state { value, error };

    /// Ends the program, or throws the error's exception where exceptions are enabled, unless a value is held.
    void require_value() const {
        if (state_ != state::value) {
#if KAHLNWS_EXCEPTIONS
            detail::raise(error_);
#else
            detail::precondition_failed("called `result::value` on a result that holds an error: ", error_.message());
#endif
        }
    }

    /// The value; ends the program unless one is held.
    T& checked_value() noexcept {
        if (state_ != state::value) {
            detail::precondition_failed("called `result::operator*` on a result that holds an error: ",
                                        error_.message());
        }
        return value_;
    }

    void destroy() noexcept {
        if (state_ == state::value) {
            value_.~T();
        } else {
            error_.~error();
        }
    }

    state state_;
    /// What the call answered: the member `state_` names.
    union {
        T value_;
        kahlnws::error error_;
    };
};

/// @brief The success of a call that answers nothing else, or the kahlnws::error of one that failed.
///
/// As kahlnws::result, without a value: a default result is a success.
template <>
class KAHLNWS_HPP_NODISCARD result<void> {
public:
    /// @brief Nothing: the call answers only its success.
    typedef void value_type;
    /// @brief The error of a call that failed.
    typedef kahlnws::error error_type;

    /// @brief A success.
    constexpr result() noexcept : state_(state::value), no_error_() {}

    /// @brief A result holding @p failure.
    /// @param failure The error.
    result(const kahlnws::error& failure) noexcept : state_(state::error), error_(failure) {}

    /// @brief Whether the call succeeded.
    bool has_value() const noexcept { return state_ == state::value; }

    /// @brief Whether the call succeeded, as has_value().
    explicit operator bool() const noexcept { return has_value(); }

    /// @brief Nothing, where the call succeeded.
    /// @pre has_value(): where @c KAHLNWS_EXCEPTIONS is 1, a result holding an error throws its exception; otherwise
    /// the program ends.
    void value() const {
        if (state_ != state::value) {
#if KAHLNWS_EXCEPTIONS
            detail::raise(error_);
#else
            detail::precondition_failed("called `result::value` on a result that holds an error: ", error_.message());
#endif
        }
    }

    /// @brief The error.
    /// @pre !has_value(); checked in every build.
    const kahlnws::error& error() const noexcept {
        if (state_ != state::error) {
            detail::precondition_failed("called `result::error` on a result that holds a value");
        }
        return error_;
    }

private:
    enum class state { value, error };

    /// A placeholder for the error of a success.
    struct no_error {};

    state state_;
    /// The error where `state_` is error; the placeholder otherwise.
    union {
        no_error no_error_;
        kahlnws::error error_;
    };
};

/// @brief A primal network simplex with the room it works in: the solver of a kahlnws::problem.
///
/// It finds a flow of least cost, or that no flow within the bounds meets the supplies, in exact integer arithmetic.
/// The room is kept from one solve to the next and only grows: a solve of a problem with no more nodes and no more
/// arcs than the room holds, made by try_reserve() or by an earlier solve, allocates nothing in the library.
///
/// The solver also keeps the basis its last solve ended on, by any form, optimal or infeasible, which only
/// try_resolve() and try_resolve_into() and their throwing forms read: what try_solve() and try_solve_into() answer
/// depends on the problem alone. A resolve starts from that basis where the problem has the node and arc counts of
/// that solve and every arc its tail and its head, and otherwise answers exactly as a solve does.
///
/// An owning handle, movable and never copied. A new solver holds no room and no handle of the library: the first call
/// that needs one makes it, and a handle refused is the error error_kind::refused_memory. A moved-from solver is in
/// the same state as a new one. The handle and the room are freed with the solver.
///
/// Where a call fails, the solver is as usable as before, and what a later call answers is as it would have been: only
/// its room may have grown, and the basis kept stays kept, but after an internal error, which may leave none. The
/// buffers of try_solve_into and try_resolve_into are as they were, except after an internal error.
class network_simplex {
public:
    /// @brief A solver that holds no room yet; it allocates nothing.
    network_simplex() noexcept : handle_(nullptr) {}

    /// @brief A solver that takes @p other's handle and room; @p other is left as a new solver.
    /// @param other The solver.
    network_simplex(network_simplex&& other) noexcept : handle_(other.handle_) { other.handle_ = nullptr; }

    /// @brief Frees this solver's handle and room and takes @p other's; @p other is left as a new solver.
    /// @param other The solver; moving a solver onto itself leaves it as it was.
    /// @return This solver.
    network_simplex& operator=(network_simplex&& other) noexcept {
        network_simplex taken(std::move(other));
        swap(taken);
        return *this;
    }

    network_simplex(const network_simplex&) = delete;
    network_simplex& operator=(const network_simplex&) = delete;

    /// @brief Frees the handle and the room.
    ~network_simplex() { kahlnws_solver_free(handle_); }

    /// @brief Exchanges the handles and rooms of this solver and @p other.
    /// @param[in,out] other The solver.
    void swap(network_simplex& other) noexcept { std::swap(handle_, other.handle_); }

    /// @brief Exchanges the handles and rooms of @p first and @p second.
    /// @param[in,out] first A solver.
    /// @param[in,out] second Another solver.
    friend void swap(network_simplex& first, network_simplex& second) noexcept { first.swap(second); }

    /// @brief Makes room for problems of up to @p node_count nodes and @p arc_count arcs: a solve or a resolve of such
    /// a problem allocates nothing in the library afterwards, and try_solve_into and try_resolve_into nothing at all.
    /// Room already held is kept, and so is the basis kept.
    /// @param node_count The most nodes.
    /// @param arc_count The most arcs.
    /// @return A success, or an error of kind refused_memory, or internal_error.
    /// @post On an error, the room held before is kept; the basis kept is kept either way.
    /// @see reserve
    KAHLNWS_HPP_NODISCARD result<void> try_reserve(std::size_t node_count, std::size_t arc_count) noexcept {
        kahlnws_solver* const solver = handle();
        if (solver == nullptr) {
            return error::refused_memory();
        }
        const kahlnws_status status = kahlnws_solver_reserve(solver, node_count, arc_count);
        if (status != KAHLNWS_STATUS_RESERVED) {
            return error::of_status(status);
        }
        return result<void>();
    }

    /// @brief Solves @p problem and answers the flows, the potentials and the cost of an optimal flow, owned, or that
    /// no flow exists; either way beside the work of the solve.
    /// @param problem The problem.
    /// @return The answer, or an error of kind refused_problem, refused_memory or internal_error.
    /// @post On an error other than an internal error, the basis kept is kept.
    /// @note Allocates the flows and the potentials, one entry per arc and per node, and the room the problem needs
    /// beyond the room held. Where exceptions are disabled and the memory for the vectors is refused, the standard
    /// library ends the program as it does without exceptions.
    /// @see solve
    KAHLNWS_HPP_NODISCARD result<answer<solution>> try_solve(const problem& problem) noexcept {
        return owned(entry::solve, problem);
    }

    /// @brief Solves @p problem, writes the flow of each arc to @p flows, arc a at position a, and answers the cost of
    /// an optimal flow, or that no flow exists; either way beside the work of the solve.
    /// @param problem The problem.
    /// @param[out] flows One entry per arc; what it held before does not matter.
    /// @return The answer, or an error of kind invalid_argument, refused_problem, refused_memory or internal_error.
    /// @pre @p flows holds one entry per arc, and shares no storage with a table of @p problem; answered as an error
    /// of kind invalid_argument, before anything runs.
    /// @post Where the outcome is infeasible, or the call fails other than with an internal error, @p flows is as it
    /// was, and on such a failure the basis kept is kept.
    /// @note Allocates nothing where the room holds the problem, and nothing on an error.
    /// @see solve_into
    KAHLNWS_HPP_NODISCARD result<answer<optimum>> try_solve_into(const problem& problem,
                                                                span<std::uint64_t> flows) noexcept {
        return into(entry::solve, problem, flows);
    }

    /// @brief Solves @p problem as try_solve_into(problem, flows) does, and of an optimal flow writes each node's
    /// potential to @p potentials too, node v at position v.
    ///
    /// With the potentials, `costs[a] + potentials[tails[a]] - potentials[heads[a]]` is at least zero on every arc a
    /// whose flow is below its capacity and at most zero on every arc whose flow is above its lower bound, which
    /// proves the flow optimal.
    /// @param problem The problem.
    /// @param[out] flows One entry per arc; what it held before does not matter.
    /// @param[out] potentials One entry per node; what it held before does not matter.
    /// @return The answer, or an error of kind invalid_argument, refused_problem, refused_memory or internal_error.
    /// @pre @p flows holds one entry per arc, and shares no storage with a table of @p problem; answered as an error
    /// of kind invalid_argument, before anything runs.
    /// @pre @p potentials holds one entry per node, and shares no storage with a table of @p problem or with
    /// @p flows; answered as an error of kind invalid_argument, before anything runs.
    /// @post Where the outcome is infeasible, or the call fails other than with an internal error, @p flows and
    /// @p potentials are as they were, and on such a failure the basis kept is kept.
    /// @note Allocates nothing where the room holds the problem, and nothing on an error.
    /// @see solve_into
    KAHLNWS_HPP_NODISCARD result<answer<optimum>> try_solve_into(const problem& problem,
                                                                span<std::uint64_t> flows,
                                                                span<std::int64_t> potentials) noexcept {
        return into(entry::solve, problem, flows, potentials);
    }

    /// @brief Solves @p problem from the basis the solver's last solve ended on, where it serves, and answers as
    /// try_solve() does: the flows, the potentials and the cost of an optimal flow, owned, or that no flow exists;
    /// either way beside the work of the resolve.
    ///
    /// The basis serves where the solver holds one: the basis its last solve ended on, by any form, optimal or
    /// infeasible, which a move carries with the solver, a new or moved-from solver holds none, and an internal error
    /// may leave none; where @p problem has that solve's node and arc counts and every arc the tail and the head it
    /// had; and where the flows that basis puts on its arcs out of the tree keep the supplies net of them within the
    /// limit breach::net_supply_beyond_range names. Its tree is kept and every number is read anew. Where the basis
    /// cannot serve, the resolve answers exactly what try_solve() answers, its work included.
    ///
    /// Where the basis serves and only some numbers changed, it often makes far fewer pivots than a solve, and the
    /// same problem again makes none. The outcome and the cost are those of try_solve(), and the potentials prove the
    /// flows optimal; of a problem with several optimal flows it may answer another, and other potentials, a function
    /// of the problem and of the solves before it. A resolve from the kept basis tries no arcs before block search,
    /// so its work.first_arc_pivots is 0. The solver keeps the basis the resolve ended on.
    /// @param problem The problem.
    /// @return The answer, or an error of kind refused_problem, refused_memory or internal_error.
    /// @post On an error other than an internal error, the basis kept is kept.
    /// @note Allocates the flows and the potentials, one entry per arc and per node, and the room the problem needs
    /// beyond the room held. Where exceptions are disabled and the memory for the vectors is refused, the standard
    /// library ends the program as it does without exceptions.
    /// @see resolve
    KAHLNWS_HPP_NODISCARD result<answer<solution>> try_resolve(const problem& problem) noexcept {
        return owned(entry::resolve, problem);
    }

    /// @brief Solves @p problem from the basis the solver's last solve ended on, where it serves, writes the flow of
    /// each arc to @p flows, arc a at position a, and answers as try_solve_into(problem, flows) does.
    ///
    /// The basis serves, and what is answered, as try_resolve() states.
    /// @param problem The problem.
    /// @param[out] flows One entry per arc; what it held before does not matter.
    /// @return The answer, or an error of kind invalid_argument, refused_problem, refused_memory or internal_error.
    /// @pre @p flows holds one entry per arc, and shares no storage with a table of @p problem; answered as an error
    /// of kind invalid_argument, before anything runs.
    /// @post Where the outcome is infeasible, or the call fails other than with an internal error, @p flows is as it
    /// was, and on such a failure the basis kept is kept.
    /// @note Allocates nothing where the room holds the problem, and nothing on an error.
    /// @see resolve_into
    KAHLNWS_HPP_NODISCARD result<answer<optimum>> try_resolve_into(const problem& problem,
                                                                  span<std::uint64_t> flows) noexcept {
        return into(entry::resolve, problem, flows);
    }

    /// @brief Solves @p problem as try_resolve_into(problem, flows) does, and of an optimal flow writes each node's
    /// potential to @p potentials too, node v at position v.
    ///
    /// With the potentials, `costs[a] + potentials[tails[a]] - potentials[heads[a]]` is at least zero on every arc a
    /// whose flow is below its capacity and at most zero on every arc whose flow is above its lower bound, which
    /// proves the flow optimal.
    /// @param problem The problem.
    /// @param[out] flows One entry per arc; what it held before does not matter.
    /// @param[out] potentials One entry per node; what it held before does not matter.
    /// @return The answer, or an error of kind invalid_argument, refused_problem, refused_memory or internal_error.
    /// @pre @p flows holds one entry per arc, and shares no storage with a table of @p problem; answered as an error
    /// of kind invalid_argument, before anything runs.
    /// @pre @p potentials holds one entry per node, and shares no storage with a table of @p problem or with
    /// @p flows; answered as an error of kind invalid_argument, before anything runs.
    /// @post Where the outcome is infeasible, or the call fails other than with an internal error, @p flows and
    /// @p potentials are as they were, and on such a failure the basis kept is kept.
    /// @note Allocates nothing where the room holds the problem, and nothing on an error.
    /// @see resolve_into
    KAHLNWS_HPP_NODISCARD result<answer<optimum>> try_resolve_into(const problem& problem,
                                                                  span<std::uint64_t> flows,
                                                                  span<std::int64_t> potentials) noexcept {
        return into(entry::resolve, problem, flows, potentials);
    }

#if KAHLNWS_EXCEPTIONS
    /// @brief Makes room as try_reserve() does, and throws where it fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param node_count The most nodes.
    /// @param arc_count The most arcs.
    /// @throws std::bad_alloc Where the memory is refused; the room held before is kept.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_reserve
    void reserve(std::size_t node_count, std::size_t arc_count) { try_reserve(node_count, arc_count).value(); }

    /// @brief Solves @p problem as try_solve() does, and throws where it fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param problem The problem.
    /// @return The answer.
    /// @throws kahlnws::problem_error Where the problem lies outside what a solve admits.
    /// @throws std::bad_alloc Where memory is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_solve
    KAHLNWS_HPP_NODISCARD answer<solution> solve(const problem& problem) { return try_solve(problem).value(); }

    /// @brief Solves @p problem into @p flows as try_solve_into() does, and throws where it fails; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @param problem The problem.
    /// @param[out] flows One entry per arc.
    /// @return The answer.
    /// @throws std::invalid_argument Where @p flows does not hold one entry per arc or shares storage with a table.
    /// @throws kahlnws::problem_error Where the problem lies outside what a solve admits.
    /// @throws std::bad_alloc Where memory is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_solve_into
    KAHLNWS_HPP_NODISCARD answer<optimum> solve_into(const problem& problem, span<std::uint64_t> flows) {
        return try_solve_into(problem, flows).value();
    }

    /// @brief Solves @p problem into @p flows and @p potentials as try_solve_into() does, and throws where it fails;
    /// only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param problem The problem.
    /// @param[out] flows One entry per arc.
    /// @param[out] potentials One entry per node.
    /// @return The answer.
    /// @throws std::invalid_argument Where a buffer does not hold one entry per arc or node, or shares storage with a
    /// table or the other buffer.
    /// @throws kahlnws::problem_error Where the problem lies outside what a solve admits.
    /// @throws std::bad_alloc Where memory is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_solve_into
    KAHLNWS_HPP_NODISCARD answer<optimum> solve_into(const problem& problem,
                                                    span<std::uint64_t> flows,
                                                    span<std::int64_t> potentials) {
        return try_solve_into(problem, flows, potentials).value();
    }

    /// @brief Solves @p problem from the kept basis as try_resolve() does, and throws where it fails; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @param problem The problem.
    /// @return The answer.
    /// @throws kahlnws::problem_error Where the problem lies outside what a solve admits.
    /// @throws std::bad_alloc Where memory is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_resolve
    KAHLNWS_HPP_NODISCARD answer<solution> resolve(const problem& problem) { return try_resolve(problem).value(); }

    /// @brief Solves @p problem into @p flows from the kept basis as try_resolve_into() does, and throws where it
    /// fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param problem The problem.
    /// @param[out] flows One entry per arc.
    /// @return The answer.
    /// @throws std::invalid_argument Where @p flows does not hold one entry per arc or shares storage with a table.
    /// @throws kahlnws::problem_error Where the problem lies outside what a solve admits.
    /// @throws std::bad_alloc Where memory is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_resolve_into
    KAHLNWS_HPP_NODISCARD answer<optimum> resolve_into(const problem& problem, span<std::uint64_t> flows) {
        return try_resolve_into(problem, flows).value();
    }

    /// @brief Solves @p problem into @p flows and @p potentials from the kept basis as try_resolve_into() does, and
    /// throws where it fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param problem The problem.
    /// @param[out] flows One entry per arc.
    /// @param[out] potentials One entry per node.
    /// @return The answer.
    /// @throws std::invalid_argument Where a buffer does not hold one entry per arc or node, or shares storage with a
    /// table or the other buffer.
    /// @throws kahlnws::problem_error Where the problem lies outside what a solve admits.
    /// @throws std::bad_alloc Where memory is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_resolve_into
    KAHLNWS_HPP_NODISCARD answer<optimum> resolve_into(const problem& problem,
                                                      span<std::uint64_t> flows,
                                                      span<std::int64_t> potentials) {
        return try_resolve_into(problem, flows, potentials).value();
    }
#endif

private:
    /// The entry of the library a solve calls: kahlnws_solver_solve, from the problem alone, or
    /// kahlnws_solver_resolve, from the basis the last solve ended on where it serves.
    enum class entry { solve, resolve };

    /// A function of the C interface that solves, as kahlnws_solver_solve and kahlnws_solver_resolve are declared.
    typedef kahlnws_status (*c_solve)(kahlnws_solver*, const kahlnws_problem*, std::uint64_t*, std::int64_t*,
                                      kahlnws_int128*, kahlnws_work*, kahlnws_refusal*);

    /// The function of the C interface `which` calls.
    static c_solve c_function(entry which) noexcept {
        switch (which) {
        case entry::solve:
            return kahlnws_solver_solve;
        case entry::resolve:
            return kahlnws_solver_resolve;
        }
        // Every entry is named above; a value no enumerator has is never made, and is called as a solve.
        return kahlnws_solver_solve;
    }

    /// The name an error's message gives the low-level form of `which`.
    static const char* into_name(entry which) noexcept {
        switch (which) {
        case entry::solve:
            return "network_simplex::try_solve_into";
        case entry::resolve:
            return "network_simplex::try_resolve_into";
        }
        // Every entry is named above; a value no enumerator has is never made, and is named as a solve.
        return "network_simplex::try_solve_into";
    }

    /// Solves `problem` by `which` into vectors of its own, and answers them owned: the body of try_solve() and
    /// try_resolve().
    result<answer<solution>> owned(entry which, const problem& problem) noexcept {
        result<solution> columns = allocated(problem.tails.size(), problem.supplies.size());
        if (!columns) {
            return columns.error();
        }
        const result<answer<optimum>> found_into = into(which, problem, columns->flows, columns->potentials);
        if (!found_into) {
            return found_into.error();
        }
        const optimum* const found = found_into->outcome.optimal();
        if (found == nullptr) {
            return answer<solution>(outcome<solution>(), found_into->work);
        }
        columns->total_cost = found->total_cost;
        return answer<solution>(outcome<solution>::make_optimal(std::move(*columns)), found_into->work);
    }

    /// Solves `problem` by `which` into `flows` where it is admitted: the body of try_solve_into() and
    /// try_resolve_into() without potentials.
    result<answer<optimum>> into(entry which, const problem& problem, span<std::uint64_t> flows) noexcept {
        const result<void> admitted = admitted_flows(which, problem, flows);
        if (!admitted) {
            return admitted.error();
        }
        return run(which, problem, flows.data(), nullptr);
    }

    /// Solves `problem` by `which` into `flows` and `potentials` where both are admitted: the body of
    /// try_solve_into() and try_resolve_into() with potentials.
    result<answer<optimum>> into(entry which,
                                 const problem& problem,
                                 span<std::uint64_t> flows,
                                 span<std::int64_t> potentials) noexcept {
        const result<void> admitted_first = admitted_flows(which, problem, flows);
        if (!admitted_first) {
            return admitted_first.error();
        }
        const result<void> admitted_second = admitted_potentials(which, problem, flows, potentials);
        if (!admitted_second) {
            return admitted_second.error();
        }
        return run(which, problem, flows.data(), potentials.data());
    }

    /// The handle, made on first need; null where the library could not make one, which it answers only where the
    /// memory for it is refused.
    kahlnws_solver* handle() noexcept {
        if (handle_ == nullptr) {
            handle_ = kahlnws_solver_new();
        }
        return handle_;
    }

    /// The vectors of a solution of `arc_count` flows and `node_count` potentials, or the error of the memory refused.
    static result<solution> allocated(std::size_t arc_count, std::size_t node_count) noexcept {
        solution owned;
        if (arc_count > owned.flows.max_size() || node_count > owned.potentials.max_size()) {
            return error::refused_memory();
        }
#if KAHLNWS_EXCEPTIONS
        try {
            owned.flows.resize(arc_count);
            owned.potentials.resize(node_count);
        } catch (const std::bad_alloc&) {
            return error::refused_memory();
        }
#else
        owned.flows.resize(arc_count);
        owned.potentials.resize(node_count);
#endif
        return result<solution>(std::move(owned));
    }

    /// Whether the `bytes` bytes from `data` share a byte with a table of `problem`.
    static bool overlaps_tables(const problem& problem, const void* data, std::size_t bytes) noexcept {
        return detail::overlap(data, bytes, problem.tails.data(), problem.tails.size() * sizeof(std::size_t)) ||
               detail::overlap(data, bytes, problem.heads.data(), problem.heads.size() * sizeof(std::size_t)) ||
               detail::overlap(data, bytes, problem.lower_bounds.data(),
                               problem.lower_bounds.size() * sizeof(std::uint64_t)) ||
               detail::overlap(data, bytes, problem.capacities.data(),
                               problem.capacities.size() * sizeof(std::uint64_t)) ||
               detail::overlap(data, bytes, problem.costs.data(), problem.costs.size() * sizeof(std::int64_t)) ||
               detail::overlap(data, bytes, problem.supplies.data(), problem.supplies.size() * sizeof(std::int64_t));
    }

    /// A success where `flows` holds one entry per arc of `problem` and shares no storage with a table of it; the
    /// error names the low-level form of `which`.
    static result<void> admitted_flows(entry which, const problem& problem, span<std::uint64_t> flows) noexcept {
        if (flows.size() != problem.tails.size()) {
            return error(error_kind::invalid_argument, "called `", into_name(which), "` with ",
                         std::uint64_t(flows.size()), " flows for a problem of ", std::uint64_t(problem.tails.size()),
                         " arcs");
        }
        if (overlaps_tables(problem, flows.data(), flows.size() * sizeof(std::uint64_t))) {
            return error(error_kind::invalid_argument, "called `", into_name(which),
                         "` with flows that share storage with a table of the problem");
        }
        return result<void>();
    }

    /// A success where `potentials` holds one entry per node of `problem` and shares no storage with a table of it or
    /// with `flows`; the error names the low-level form of `which`.
    static result<void> admitted_potentials(entry which,
                                            const problem& problem,
                                            span<std::uint64_t> flows,
                                            span<std::int64_t> potentials) noexcept {
        if (potentials.size() != problem.supplies.size()) {
            return error(error_kind::invalid_argument, "called `", into_name(which), "` with ",
                         std::uint64_t(potentials.size()), " potentials for a problem of ",
                         std::uint64_t(problem.supplies.size()), " nodes");
        }
        const std::size_t bytes = potentials.size() * sizeof(std::int64_t);
        if (overlaps_tables(problem, potentials.data(), bytes) ||
            detail::overlap(potentials.data(), bytes, flows.data(), flows.size() * sizeof(std::uint64_t))) {
            return error(error_kind::invalid_argument, "called `", into_name(which),
                         "` with potentials that share storage with a table of the problem or with the flows");
        }
        return result<void>();
    }

    /// The tables of `problem` as the C interface takes them, where every per-arc table holds as many entries as the
    /// tails, or empty lower bounds; the refusal of the first, in the order of kahlnws::table, that does not.
    static result<kahlnws_problem> laid_out(const problem& problem) noexcept {
        const std::size_t arc_count = problem.tails.size();
        const std::size_t lengths[] = {problem.heads.size(),
                                       problem.lower_bounds.empty() ? arc_count : problem.lower_bounds.size(),
                                       problem.capacities.size(), problem.costs.size()};
        const table tables[] = {table::heads, table::lower_bounds, table::capacities, table::costs};
        for (std::size_t position = 0; position < 4; ++position) {
            if (lengths[position] != arc_count) {
                return error(refusal(breach::length{tables[position], lengths[position], arc_count}));
            }
        }
        const kahlnws_problem tables_laid_out = {
            problem.supplies.size(),
            arc_count,
            problem.tails.data(),
            problem.heads.data(),
            problem.lower_bounds.empty() ? nullptr : problem.lower_bounds.data(),
            problem.capacities.data(),
            problem.costs.data(),
            problem.supplies.data()};
        return tables_laid_out;
    }

    /// Solves `problem` by `which` into `flows`, one entry per arc, and `potentials`, one per node or null, which
    /// overlap nothing else of the call: the one call of the library's solve, and the answer it maps to.
    result<answer<optimum>> run(entry which,
                                const problem& problem,
                                std::uint64_t* flows,
                                std::int64_t* potentials) noexcept {
        const result<kahlnws_problem> tables = laid_out(problem);
        if (!tables) {
            return tables.error();
        }
        kahlnws_solver* const solver = handle();
        if (solver == nullptr) {
            return error::refused_memory();
        }
        kahlnws_int128 total_cost = {0, 0};
        kahlnws_work counts = {0, 0, 0};
        kahlnws_refusal written = kahlnws_refusal();
        const kahlnws_status status =
            c_function(which)(solver, &*tables, flows, potentials, &total_cost, &counts, &written);

        const work done = {counts.pivots, counts.degenerate_pivots, counts.first_arc_pivots};
        switch (status) {
        case KAHLNWS_STATUS_OPTIMAL: {
            const optimum found = {int128::from_halves(total_cost.low, total_cost.high)};
            return answer<optimum>(outcome<optimum>::make_optimal(found), done);
        }
        case KAHLNWS_STATUS_INFEASIBLE:
            return answer<optimum>(outcome<optimum>(), done);
        case KAHLNWS_STATUS_REFUSED:
            return error::of_refusal(written);
        }
        return error::of_status(status);
    }

    /// The library's solver; null where none was made yet.
    kahlnws_solver* handle_;
};

/// @brief The largest magnitude a cost may have in a problem of @p node_count nodes: every cost of a problem a solve
/// admits lies within plus or minus it. It is (2^62 - 1) / node_count, rounded down, and 2^62 - 1 for no node.
/// @param node_count The number of nodes.
/// @return The bound, or an error of kind internal_error, which only a defect of the library answers.
/// @see cost_bound
KAHLNWS_HPP_NODISCARD inline result<std::int64_t> try_cost_bound(std::size_t node_count) noexcept {
    const std::int64_t bound = kahlnws_cost_bound(node_count);
    if (bound < 0) {
        return error::internal();
    }
    return bound;
}

#if KAHLNWS_EXCEPTIONS
/// @brief The largest magnitude a cost may have in a problem of @p node_count nodes, as try_cost_bound() answers it;
/// only where @c KAHLNWS_EXCEPTIONS is 1.
/// @param node_count The number of nodes.
/// @return The bound.
/// @throws kahlnws::internal_error Where the library fails.
/// @see try_cost_bound
KAHLNWS_HPP_NODISCARD inline std::int64_t cost_bound(std::size_t node_count) {
    return try_cost_bound(node_count).value();
}
#endif

/// @brief The version of this header, as (major << 16) | (minor << 8) | patch, the minor and the patch each below 256.
constexpr std::uint32_t header_version() noexcept { return KAHLNWS_VERSION; }

/// @brief The version of the library linked, encoded as header_version() is.
inline std::uint32_t library_version() noexcept { return kahlnws_version(); }

/// @brief Whether a library of the version @p library serves a program built against a header of the version
/// @p header: from the first release on, the same major version and a minor version at least the header's; while the
/// major version is 0, the same minor version too.
/// @param header A header's version, encoded as header_version() is.
/// @param library A library's version, encoded alike.
/// @note Until the first release the interface may change while the version stays, so a header and a library must
/// be built from the same sources, which no comparison of versions can check.
constexpr bool is_compatible(std::uint32_t header, std::uint32_t library) noexcept {
    return (header >> 16) == (library >> 16) &&
           ((header >> 16) == 0 ? ((header >> 8) & 0xFFu) == ((library >> 8) & 0xFFu)
                                : ((header >> 8) & 0xFFu) <= ((library >> 8) & 0xFFu));
}

/// @brief Whether the library linked serves this header, as `is_compatible(header_version(), library_version())`
/// answers; the note of is_compatible() holds.
inline bool is_library_compatible() noexcept { return is_compatible(header_version(), library_version()); }

}  // inline namespace
}  // namespace kahlnws

/// @brief The hashes of the header's value types, consistent with their equality.
namespace std {

/// @brief The hash of a kahlnws::int128: a value that fits 64 bits hashes as its low half does.
template <>
struct hash<kahlnws::int128> {
    /// @brief The hash of @p value.
    /// @param value The value.
    std::size_t operator()(const kahlnws::int128& value) const noexcept {
        // Where the value fits 64 bits, `high` is the sign of `low` spread over 64 bits, and the difference is zero.
        const std::uint64_t spread_sign = value.low >> 63 == 0 ? 0 : ~std::uint64_t(0);
        const std::hash<std::uint64_t> halves;
        return halves(value.low) ^ halves(static_cast<std::uint64_t>(value.high) - spread_sign);
    }
};

/// @brief The hash of a kahlnws::uint128: a value that fits 64 bits hashes as its low half does.
template <>
struct hash<kahlnws::uint128> {
    /// @brief The hash of @p value.
    /// @param value The value.
    std::size_t operator()(const kahlnws::uint128& value) const noexcept {
        const std::hash<std::uint64_t> halves;
        return halves(value.low) ^ halves(value.high);
    }
};

}  // namespace std

#undef KAHLNWS_HPP_NODISCARD
#undef KAHLNWS_HPP_STANDARD

#endif  // KAHLNWS_HPP_3D75A16F_358D_4952_81EB_3C4A49B5DE0C
