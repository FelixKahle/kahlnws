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
/// @par Two contracts
/// The solver, a kahlnws::network_simplex, is a function: a problem in, an answer out. It keeps room and nothing else,
/// so what it answers is a function of the problem alone. A network, a kahlnws::network, is a state: it holds one
/// problem and a basis of it, which its changes, its loads of new numbers on the same arcs and its solves carry on
/// from. Reach for the solver to answer one problem; for a network to hold a problem, change it, load new numbers on
/// the same arcs, and solve it again and again.
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
/// @par A problem held
/// A kahlnws::network holds one problem, which @c try_load copies, carrying over the basis it holds where a load of
/// new numbers on the same arcs lets it, and a flow it keeps optimal as the problem's numbers change: @c try_set_cost,
/// @c try_set_capacity, @c try_set_lower_bound, @c try_set_bounds and @c try_set_supply each change one number or one
/// pair, at once, and answer what they replaced, and @c try_move_supply moves supply from one node to another;
/// @c try_solve solves the problem as it then stands; @c try_total_cost, @c try_flow, @c try_flows,
/// @c try_flows_into, @c try_potentials and @c try_potentials_into read the optimum, and answer an error of kind
/// kahlnws::error_kind::no_optimum where the network holds none, since nothing can borrow it. @c try_tail,
/// @c try_head, @c try_cost, @c try_lower_bound, @c try_capacity, @c try_bounds and @c try_supply, and the columns
/// @c try_tails, @c try_heads, @c try_costs, @c try_lower_bounds, @c try_capacities and @c try_supplies with their
/// @c _into forms, read the problem as it stands, which needs no optimum and ends none. It serves a search that tries
/// a move, keeps it or sets its numbers back: a change costs what it moves, and a solve after it the pivots it calls
/// for; a solve that makes no pivot prices only the arcs the changes may have made worth using, while a change that
/// leaves much flow still to be sent can cost more than a load of the whole problem as it stands and the solve after
/// it. A copy, by @c try_clone, @c try_clone_from or the copy forms, is the state itself.
///
/// @par Errors as values, and as exceptions
/// An infeasible problem is an answer, not an error. Every operation that can fail has a form named @c try_..., in
/// every build, that never throws and answers a kahlnws::result: the value, or a kahlnws::error, which says whether
/// the problem was refused (and why, as a kahlnws::refusal), a change of a network was refused (and why, as a
/// kahlnws::change_refusal), an argument was not admitted, memory was refused, a network held no optimum to read, or
/// the library failed. Where exceptions are enabled, each also has a throwing form under the plain name, which calls
/// the @c try_ form and throws what kahlnws::result::value() throws: kahlnws::problem_error for a refused problem,
/// kahlnws::change_error for a refused change, @c std::invalid_argument for an argument not admitted,
/// @c std::bad_alloc for refused memory, @c std::logic_error for no optimum held, and kahlnws::internal_error for a
/// failure of the library. Beyond them, only what builds a string may throw, and only @c std::bad_alloc where the
/// memory for it is refused: kahlnws::to_string and the constructors of the exception classes; and the stream
/// operators throw what their stream throws.
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
/// The @c try_ forms of the low level, @c try_solve_into, allocate nothing where the solver's room holds the problem,
/// made by @c try_reserve or by an earlier solve, and nothing on any error. The high level allocates the vectors of the
/// solution. Where exceptions are disabled and memory for those vectors is refused, the standard library acts as it
/// does without exceptions, which is to end the program on the common implementations; the low level, with the caller's
/// buffers, is the path that needs no allocation. A kahlnws::network allocates in @c try_reserve, in @c try_load and
/// @c try_clone_from beyond its room, and in @c try_clone; its changes, its solve and every @c _into read allocate
/// nothing, the owned reads their vectors. A kahlnws::error holds its message, and making one allocates nothing; a
/// throwing form allocates the message of the exception it throws.
///
/// @par Preconditions
/// A precondition this header states is checked in every build, and a call that breaks it ends the program with
/// @c std::abort after a message on standard error, never with a wrong answer; the one exception is
/// kahlnws::span::operator[], which, as the standard containers do, checks its index with @c assert alone.
///
/// @par Threads
/// A kahlnws::network_simplex or a kahlnws::network is used by one thread at a time: no two of its calls run at once.
/// It may move from one thread to another between calls, and distinct ones may be used on distinct threads at once,
/// also to solve or load the same problem, whose tables they only read. Every other function may be called from any
/// thread at any time.
///
/// @par Ownership and lifetimes
/// A kahlnws::network_simplex owns its solver and room, and frees them when destroyed; a kahlnws::network likewise owns
/// its network, with the problem it copied. A kahlnws::span and a kahlnws::problem own nothing: what they view must
/// outlive every use of them and must not move, so a @c std::vector viewed must not grow meanwhile. Every answer is a
/// value that owns what it holds.
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
class network;
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

    /// Appends `number` in decimal, as the library writes it.
    void write(std::uint64_t number) noexcept {
        char digits[21];
        const kahlnws_uint128 value = {number, 0};
        kahlnws_uint128_format(value, digits, sizeof digits);
        write(digits);
    }

    /// Appends `number` in decimal, its sign first where it is negative, as the library writes it.
    void write(std::int64_t number) noexcept {
        char digits[21];
        const kahlnws_int128 value = {static_cast<std::uint64_t>(number), number < 0 ? -1 : 0};
        kahlnws_int128_format(value, digits, sizeof digits);
        write(digits);
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
/// The cost of a flow, the sum of supplies that do not balance, and a supply past its range. A carrier of exact
/// values, not an arithmetic type: it compares, hashes, converts and prints, and does nothing else. Plain data: every
/// pair of halves is a value, and every value has exactly one pair; @c low is the value modulo 2^64, and @c high the
/// value divided by 2^64 and rounded down, so they are the low and the high 64 bits of the value's two's complement.
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

    /// @brief The high half of the value's magnitude, for the conversion to double.
    constexpr std::uint64_t magnitude_high() const noexcept {
        return high < 0 ? ~static_cast<std::uint64_t>(high) + (low == 0 ? 1u : 0u) : static_cast<std::uint64_t>(high);
    }

    /// @brief The low half of the value's magnitude, for the conversion to double.
    constexpr std::uint64_t magnitude_low() const noexcept { return high < 0 ? ~low + 1u : low; }

private:
    /// The value of the halves; the third parameter only sets it apart from the public constructors.
    constexpr int128(std::uint64_t low_half, std::int64_t high_half, int) noexcept : low(low_half), high(high_half) {}
};

/// @brief An unsigned 128-bit integer as its two 64-bit halves: the value is `high * 2^64 + low`.
///
/// The sum of the positive supplies net of the lower bounds that passes its limit, and the flow still to be sent that
/// would pass its own. A carrier of exact values, not an arithmetic type, as kahlnws::int128. Plain data: every pair
/// of halves is a value, and every value has exactly one pair; @c low is the value modulo 2^64 and @c high the value
/// divided by 2^64, rounded down.
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

/// @brief The value in decimal, exactly, with a minus sign where it is negative: such as "-18446744073709551616", as
/// kahlnws_int128_format writes it.
/// @param value The value.
/// @throws std::bad_alloc Where the memory for the string is refused, and exceptions are enabled.
inline std::string to_string(const int128& value) {
    char digits[41];
    const kahlnws_int128 halves = {value.low, value.high};
    return std::string(digits, kahlnws_int128_format(halves, digits, sizeof digits));
}

/// @brief The value in decimal, exactly: such as "18446744073709551616", as kahlnws_uint128_format writes it.
/// @param value The value.
/// @throws std::bad_alloc Where the memory for the string is refused, and exceptions are enabled.
inline std::string to_string(const uint128& value) {
    char digits[40];
    const kahlnws_uint128 halves = {value.low, value.high};
    return std::string(digits, kahlnws_uint128_format(halves, digits, sizeof digits));
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
    const kahlnws_int128 halves = {value.low, value.high};
    kahlnws_int128_format(halves, digits, sizeof digits);
    return stream << digits;
}

/// @brief Writes @p value to @p stream in decimal, as kahlnws::to_string gives it.
/// @tparam Char The stream's character type.
/// @tparam Traits The stream's character traits.
/// @param[in,out] stream The stream; the program includes @c <ostream> to use it.
/// @param value The value.
/// @return @p stream.
template <class Char, class Traits>
std::basic_ostream<Char, Traits>& operator<<(std::basic_ostream<Char, Traits>& stream, const uint128& value) {
    char digits[40];
    const kahlnws_uint128 halves = {value.low, value.high};
    kahlnws_uint128_format(halves, digits, sizeof digits);
    return stream << digits;
}

/// @brief A view of @c size() elements of type @p T stored contiguously elsewhere: a table of a kahlnws::problem, or a
/// buffer kahlnws::network_simplex::try_solve_into writes to.
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

    /// @brief A view of the first @p count elements, as the standard's @c std::span::first: a problem's tables or a
    /// solve's buffers cut to one problem from storage that holds the largest.
    /// @param count The number of elements.
    /// @pre @p count is at most size(); checked in every build.
    constexpr span first(size_type count) const noexcept {
        return count <= size_ ? span(data_, count) : (count_above_size(count, size_), span());
    }

private:
    /// Ends the program after writing to standard error that first() was called with @p count above @p size.
    [[noreturn]] static void count_above_size(size_type count, size_type size) noexcept {
        std::fprintf(stderr, "called `span::first` with a count of %zu, above the size %zu\n", count, size);
        std::abort();
    }

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
/// alone, its arc order included, and on the library's version; those of a network's solve on the problems it loaded
/// and the calls made on it since. In every work a solve answers, `degenerate_pivots <= pivots` and `first_arc_pivots
/// <= pivots`, and the pivots that sent flow are `pivots - degenerate_pivots`.
struct work {
    /// @brief The pivots the solve made, flips included; 0 where no arc ever violated its bound.
    std::uint64_t pivots;
    /// @brief Of the pivots, those that sent no flow, so that only the tree and the potentials changed; never a flip.
    std::uint64_t degenerate_pivots;
    /// @brief Of the pivots, those made by the arcs the solve tries before block search, chosen from the problem's
    /// structure; 0 in a network's solve, which tries none; @c pivots where those arcs alone reached the outcome.
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

/// @brief What an optimal kahlnws::network_simplex::try_solve_into answers beside the flows, and
/// the potentials where asked, that it wrote to the caller's buffers; and what an optimal kahlnws::network::try_solve
/// answers of the optimum the network then holds. Plain data.
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

/// @brief What an optimal kahlnws::network_simplex::try_solve answers, owned: the flows, the potentials and the cost.
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

/// @brief Which breach a kahlnws::change_refusal names: why a kahlnws::network refused a change.
enum class change_refusal_kind {
    /// @brief The lower bound set on an arc passes its capacity, either set with it or the one it holds.
    lower_above_capacity,
    /// @brief The cost set on an arc lies beyond plus or minus kahlnws::cost_bound of the node count.
    cost_beyond_bound,
    /// @brief With the cost or the capacity set on an arc, the magnitudes of the costs times the capacities would add
    /// up past 2^127 - 1.
    cost_total_beyond_range,
    /// @brief A move of supply would take a node's supply past the range of a @c std::int64_t.
    supply_beyond_range,
    /// @brief A change would leave more flow still to be sent than a network holds between solves: what the last load
    /// or a solve that found no flow left to be sent, with what changes since moved that the arcs could not carry at
    /// once, plus this change's amount, adds up past 2^64 - 2. A load leaves to be sent what its start does not route,
    /// up to the whole of the positive supplies net of the lower bounds where it starts from no basis, so a change
    /// right after a load of supplies near that limit can be refused. A solve that finds a flow leaves none.
    excess_beyond_range,
};

/// @brief The values of each breach a kahlnws::change_refusal names, one plain struct per
/// kahlnws::change_refusal_kind.
namespace change_breach {

/// @brief The values of a breach of kind kahlnws::change_refusal_kind::lower_above_capacity.
struct lower_above_capacity {
    /// @brief The arc.
    std::size_t arc;
    /// @brief Its lower bound, in units of flow.
    std::uint64_t lower_bound;
    /// @brief Its capacity, in units of flow.
    std::uint64_t capacity;
};

/// @brief The values of a breach of kind kahlnws::change_refusal_kind::cost_beyond_bound.
struct cost_beyond_bound {
    /// @brief The arc.
    std::size_t arc;
    /// @brief The cost set on it, in units of cost.
    std::int64_t cost;
    /// @brief The number of nodes, which sets the bound.
    std::size_t node_count;
};

/// @brief The values of a breach of kind kahlnws::change_refusal_kind::cost_total_beyond_range.
struct cost_total_beyond_range {
    /// @brief The arc whose change would take the sum past 2^127 - 1.
    std::size_t arc;
};

/// @brief The values of a breach of kind kahlnws::change_refusal_kind::supply_beyond_range.
struct supply_beyond_range {
    /// @brief The node, the one the amount is taken from where both would leave the range.
    std::size_t node;
    /// @brief The supply it would have, in units of flow.
    int128 supply;
};

/// @brief The values of a breach of kind kahlnws::change_refusal_kind::excess_beyond_range.
struct excess_beyond_range {
    /// @brief The flow still to be sent and the flow the change moves, added up, in units of flow.
    uint128 total;
};

}  // namespace change_breach

/// @brief Why a kahlnws::network refused a change: the breach, by its kind, and the values that make it one.
///
/// A refused change changes nothing: the network holds the problem it held, and an optimum it held. A breach refused
/// for the reason a kahlnws::refusal would refuse a problem has that breach's name and values, and its message; the two
/// types stay apart, since a problem can be refused for what no change can make, and a change for what no problem
/// meets. A value, made from the values of one breach; the accessor of its kind answers them, and every other accessor
/// null:
/// @code{.cpp}
/// if (const kahlnws::change_breach::supply_beyond_range* supply = refusal.supply_beyond_range()) {
///     std::cerr << "node " << supply->node << " would have the supply " << supply->supply << "\n";
/// }
/// @endcode
class change_refusal {
public:
    /// @brief A refusal for a lower bound above its capacity. @param values The values of the breach.
    constexpr change_refusal(const change_breach::lower_above_capacity& values) noexcept
        : kind_(change_refusal_kind::lower_above_capacity), lower_above_capacity_(values) {}
    /// @brief A refusal for a cost beyond its bound. @param values The values of the breach.
    constexpr change_refusal(const change_breach::cost_beyond_bound& values) noexcept
        : kind_(change_refusal_kind::cost_beyond_bound), cost_beyond_bound_(values) {}
    /// @brief A refusal for costs times capacities beyond range. @param values The values of the breach.
    constexpr change_refusal(const change_breach::cost_total_beyond_range& values) noexcept
        : kind_(change_refusal_kind::cost_total_beyond_range), cost_total_beyond_range_(values) {}
    /// @brief A refusal for a supply beyond its range. @param values The values of the breach.
    constexpr change_refusal(const change_breach::supply_beyond_range& values) noexcept
        : kind_(change_refusal_kind::supply_beyond_range), supply_beyond_range_(values) {}
    /// @brief A refusal for more flow still to be sent than a network holds. @param values The values of the breach.
    constexpr change_refusal(const change_breach::excess_beyond_range& values) noexcept
        : kind_(change_refusal_kind::excess_beyond_range), excess_beyond_range_(values) {}

    /// @brief Which breach it names.
    constexpr change_refusal_kind kind() const noexcept { return kind_; }

    /// @brief The values of a breach of kind lower_above_capacity; null for another kind.
    const change_breach::lower_above_capacity* lower_above_capacity() const noexcept {
        return kind_ == change_refusal_kind::lower_above_capacity ? &lower_above_capacity_ : nullptr;
    }

    /// @brief The values of a breach of kind cost_beyond_bound; null for another kind.
    const change_breach::cost_beyond_bound* cost_beyond_bound() const noexcept {
        return kind_ == change_refusal_kind::cost_beyond_bound ? &cost_beyond_bound_ : nullptr;
    }

    /// @brief The values of a breach of kind cost_total_beyond_range; null for another kind.
    const change_breach::cost_total_beyond_range* cost_total_beyond_range() const noexcept {
        return kind_ == change_refusal_kind::cost_total_beyond_range ? &cost_total_beyond_range_ : nullptr;
    }

    /// @brief The values of a breach of kind supply_beyond_range; null for another kind.
    const change_breach::supply_beyond_range* supply_beyond_range() const noexcept {
        return kind_ == change_refusal_kind::supply_beyond_range ? &supply_beyond_range_ : nullptr;
    }

    /// @brief The values of a breach of kind excess_beyond_range; null for another kind.
    const change_breach::excess_beyond_range* excess_beyond_range() const noexcept {
        return kind_ == change_refusal_kind::excess_beyond_range ? &excess_beyond_range_ : nullptr;
    }

private:
    change_refusal_kind kind_;
    /// The values of the breach `kind_` names: the one member of that name.
    union {
        change_breach::lower_above_capacity lower_above_capacity_;
        change_breach::cost_beyond_bound cost_beyond_bound_;
        change_breach::cost_total_beyond_range cost_total_beyond_range_;
        change_breach::supply_beyond_range supply_beyond_range_;
        change_breach::excess_beyond_range excess_beyond_range_;
    };
};

/// @brief What failed in a call that answered a kahlnws::error.
enum class error_kind {
    /// @brief The problem lies outside what a solve admits; kahlnws::error::refusal() names the breach.
    refused_problem,
    /// @brief An argument the call does not admit: a buffer of the wrong length, one that shares storage with a table
    /// of the problem or with the other buffer, an arc or a node of a network not below its count, or a node given to
    /// kahlnws::network::try_set_supply as its own balancing node with a supply other than the one it has.
    invalid_argument,
    /// @brief The memory the call needed was refused.
    refused_memory,
    /// @brief The library failed: a defect of it, or a library built from other sources than this header.
    internal_error,
    /// @brief A change of a kahlnws::network lies outside what it admits, and changed nothing;
    /// kahlnws::error::change_refusal() names the breach.
    refused_change,
    /// @brief A read of a kahlnws::network's optimum found none: the network was loaded or changed since its last
    /// solve, or that solve found no flow.
    no_optimum,
};

/// @brief The name of @p kind for a person to read: "refused problem", "invalid argument", "refused memory",
/// "internal error", "refused change" or "no optimum".
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
    case error_kind::refused_change:
        return "refused change";
    case error_kind::no_optimum:
        return "no optimum";
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

/// @brief The name of @p kind for a person to read, its enumerator in words: such as "supply beyond range".
/// @param kind The kind.
/// @return A static string, or "unknown change refusal kind" for a number no kind has.
inline const char* name(change_refusal_kind kind) noexcept {
    switch (kind) {
    case change_refusal_kind::lower_above_capacity:
        return "lower above capacity";
    case change_refusal_kind::cost_beyond_bound:
        return "cost beyond bound";
    case change_refusal_kind::cost_total_beyond_range:
        return "cost total beyond range";
    case change_refusal_kind::supply_beyond_range:
        return "supply beyond range";
    case change_refusal_kind::excess_beyond_range:
        return "excess beyond range";
    }
    return "unknown change refusal kind";
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

namespace detail {

inline result<answer<optimum> > answered(kahlnws_status status,
                                         const kahlnws_int128& total_cost,
                                         const kahlnws_work& counts) noexcept;

}  // namespace detail

/// @brief Why a call failed: its kind, the breach of a refused problem or of a refused change, and a message for a
/// person to read.
///
/// A value, copied freely, that holds its message: making one allocates nothing. The message follows the library's
/// form: lower case, no full stop, the offending values; for a refused problem or change it is the library's text of
/// the breach.
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
            writer.write(std::uint64_t(values->length));
            writer.write(" where the tails have length ");
            writer.write(std::uint64_t(values->expected));
        } else {
            const kahlnws_refusal laid_out = c_refusal(breach);
            kahlnws_refusal_message(&laid_out, message_, sizeof message_);
        }
    }

    /// @brief The error of a change of a kahlnws::network refused for @p breach, with the library's message of it.
    /// @param breach The breach.
    explicit error(const kahlnws::change_refusal& breach) noexcept
        : kind_(error_kind::refused_change), change_refusal_(breach) {
        const kahlnws_refusal laid_out = c_refusal(breach);
        kahlnws_refusal_message(&laid_out, message_, sizeof message_);
    }

    /// @brief What failed.
    error_kind kind() const noexcept { return kind_; }

    /// @brief The breach of a refused problem; null for another kind.
    const kahlnws::refusal* refusal() const noexcept {
        return kind_ == error_kind::refused_problem ? &refusal_ : nullptr;
    }

    /// @brief The breach of a refused change; null for another kind.
    const kahlnws::change_refusal* change_refusal() const noexcept {
        return kind_ == error_kind::refused_change ? &change_refusal_ : nullptr;
    }

    /// @brief The message for a person to read, a NUL-terminated string the error holds.
    const char* message() const noexcept { return message_; }

private:
    friend class network_simplex;
    friend class network;
    friend result<std::int64_t> try_cost_bound(std::size_t) noexcept;
    friend result<answer<optimum> > detail::answered(kahlnws_status,
                                                     const kahlnws_int128&,
                                                     const kahlnws_work&) noexcept;

    /// A placeholder for the refusal of an error of another kind.
    struct no_refusal {};

    /// The error of `kind`, a kind other than refused_problem and refused_change, with the message of `pieces` one
    /// after the other.
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
        return error(error_kind::refused_memory, "the memory the call needed was refused");
    }

    /// The error of an internal error of the library caught.
    static error internal() noexcept {
        return error(error_kind::internal_error, "an internal error of the library was caught");
    }

    /// The error of a read of a network's optimum that found none.
    static error no_optimum() noexcept {
        return error(error_kind::no_optimum,
                     "the network holds no optimum: it was loaded or changed since its last solve, or that solve found "
                     "no flow");
    }

    /// The error of the refusal of a change the library wrote: the refused change of its breach, or an internal error
    /// where it names a kind no change is refused with, which only a library built from other sources gives.
    static error of_change_refusal(const kahlnws_refusal& written) noexcept {
        switch (written.kind) {
        case KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY:
            return error(change_breach::lower_above_capacity{written.arc, written.lower_bound, written.capacity});
        case KAHLNWS_REFUSAL_KIND_COST_BEYOND_BOUND:
            return error(change_breach::cost_beyond_bound{written.arc, written.cost, written.node_count});
        case KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE:
            return error(change_breach::cost_total_beyond_range{written.arc});
        case KAHLNWS_REFUSAL_KIND_SUPPLY_BEYOND_RANGE:
            return error(change_breach::supply_beyond_range{
                written.node, int128::from_halves(written.supply.low, written.supply.high)});
        case KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE:
            return error(change_breach::excess_beyond_range{
                uint128::from_halves(written.excess_total.low, written.excess_total.high)});
        }
        return error(error_kind::internal_error, "the library refused the change with the breach kind ",
                     std::uint64_t(written.kind), ", which no change is refused with");
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

    /// The refusal of a change as the C interface lays it out, for its message.
    static kahlnws_refusal c_refusal(const kahlnws::change_refusal& breach) noexcept {
        kahlnws_refusal laid_out = kahlnws_refusal();
        if (const change_breach::lower_above_capacity* lower = breach.lower_above_capacity()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_LOWER_ABOVE_CAPACITY;
            laid_out.arc = lower->arc;
            laid_out.lower_bound = lower->lower_bound;
            laid_out.capacity = lower->capacity;
        } else if (const change_breach::cost_beyond_bound* cost = breach.cost_beyond_bound()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_COST_BEYOND_BOUND;
            laid_out.arc = cost->arc;
            laid_out.cost = cost->cost;
            laid_out.node_count = cost->node_count;
        } else if (const change_breach::cost_total_beyond_range* total = breach.cost_total_beyond_range()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_COST_TOTAL_BEYOND_RANGE;
            laid_out.arc = total->arc;
        } else if (const change_breach::supply_beyond_range* supply = breach.supply_beyond_range()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_SUPPLY_BEYOND_RANGE;
            laid_out.node = supply->node;
            laid_out.supply.low = supply->supply.low;
            laid_out.supply.high = supply->supply.high;
        } else if (const change_breach::excess_beyond_range* excess = breach.excess_beyond_range()) {
            laid_out.kind = KAHLNWS_REFUSAL_KIND_EXCESS_BEYOND_RANGE;
            laid_out.excess_total.low = excess->total.low;
            laid_out.excess_total.high = excess->total.high;
        }
        return laid_out;
    }

    error_kind kind_;
    /// The breach where `kind_` is refused_problem or refused_change; the placeholder otherwise.
    union {
        no_refusal no_refusal_;
        kahlnws::refusal refusal_;
        kahlnws::change_refusal change_refusal_;
    };
    /// The message, NUL-terminated. The longest message an error holds, the library's of a problem too large with both
    /// counts at 2^64 - 1, has 147 bytes; 160 holds it with its NUL, and a longer one would be cut, never overrun.
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

/// @brief The exception of a refused change of a kahlnws::network: what() is the library's message of the breach, and
/// refusal() the breach.
///
/// Thrown by the throwing forms where the @c try_ form answers an error of kind error_kind::refused_change. The
/// network is as it was. Only where @c KAHLNWS_EXCEPTIONS is 1.
/// @note Its type information is held per shared object, as kahlnws::problem_error's; catching it as
/// @c std::invalid_argument holds everywhere.
class change_error : public std::invalid_argument {
public:
    /// @brief The exception of a change refused for @p breach.
    /// @param breach The breach.
    /// @throws std::bad_alloc Where the memory for the message is refused.
    explicit change_error(const kahlnws::change_refusal& breach)
        : std::invalid_argument(error(breach).message()), refusal_(breach) {}

    /// @brief The breach the change was refused for.
    const kahlnws::change_refusal& refusal() const noexcept { return refusal_; }

private:
    kahlnws::change_refusal refusal_;
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
    case error_kind::refused_change:
        throw change_error(*failure.change_refusal());
    case error_kind::no_optimum:
        throw std::logic_error(failure.message());
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

namespace detail {

/// The answer of a solve the library answered with `status`, writing `total_cost` and `counts`: the optimum or
/// infeasibility it found, beside its work; the error of any other status.
inline result<answer<optimum> > answered(kahlnws_status status,
                                         const kahlnws_int128& total_cost,
                                         const kahlnws_work& counts) noexcept {
    const work done = {counts.pivots, counts.degenerate_pivots, counts.first_arc_pivots};
    switch (status) {
    case KAHLNWS_STATUS_OPTIMAL: {
        const optimum found = {int128::from_halves(total_cost.low, total_cost.high)};
        return answer<optimum>(outcome<optimum>::make_optimal(found), done);
    }
    case KAHLNWS_STATUS_INFEASIBLE:
        return answer<optimum>(outcome<optimum>(), done);
    }
    return error::of_status(status);
}

/// The tables of `problem` as the C interface takes them, where every per-arc table holds as many entries as the
/// tails, or empty lower bounds; the refusal of the first, in the order of kahlnws::table, that does not.
inline result<kahlnws_problem> laid_out(const problem& problem) noexcept {
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

/// Makes `entries` hold `count` entries; false where the memory for them is refused. Where exceptions are disabled,
/// refused memory ends the program as the standard library ends it.
template <class Entry>
bool resized(std::vector<Entry>& entries, std::size_t count) noexcept {
    if (count > entries.max_size()) {
        return false;
    }
#if KAHLNWS_EXCEPTIONS
    try {
        entries.resize(count);
    } catch (const std::bad_alloc&) {
        return false;
    }
#else
    entries.resize(count);
#endif
    return true;
}

}  // namespace detail

/// @brief A primal network simplex with the room it works in: the solver of a kahlnws::problem.
///
/// It finds a flow of least cost, or that no flow within the bounds meets the supplies, in exact integer arithmetic.
/// The room is kept from one solve to the next and only grows: a solve of a problem with no more nodes and no more
/// arcs than the room holds, made by try_reserve() or by an earlier solve, allocates nothing in the library.
///
/// It keeps its room and nothing else: what a solve answers is a function of its problem alone, and nothing of an
/// earlier problem is read again. To hold a problem and carry a basis from one solve to the next, use a
/// kahlnws::network.
///
/// An owning handle, movable and never copied. A new solver holds no room and no handle of the library: the first call
/// that needs one makes it, and a handle refused is the error error_kind::refused_memory. A moved-from solver is in
/// the same state as a new one. The handle and the room are freed with the solver.
///
/// Where a call fails, the solver is as usable as before, and what a later call answers is as it would have been: only
/// its room may have grown. The buffers of try_solve_into are as they were, except after an internal error.
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

    /// @brief Makes room for problems of up to @p node_count nodes and @p arc_count arcs: a solve of such a problem
    /// allocates nothing in the library afterwards, and try_solve_into nothing at all. Room already held is kept.
    /// @param node_count The most nodes.
    /// @param arc_count The most arcs.
    /// @return A success, or an error of kind refused_memory, or internal_error.
    /// @post On an error, the room held before is kept.
    /// @see reserve
    KAHLNWS_HPP_NODISCARD result<void> try_reserve(std::size_t node_count, std::size_t arc_count) noexcept {
        kahlnws_solver* const solver = handle();
        if (solver == nullptr) {
            return error::refused_memory();
        }
        const kahlnws_status status = kahlnws_solver_reserve(solver, node_count, arc_count);
        if (status != KAHLNWS_STATUS_OK) {
            return error::of_status(status);
        }
        return result<void>();
    }

    /// @brief Solves @p problem and answers the flows, the potentials and the cost of an optimal flow, owned, or that
    /// no flow exists; either way beside the work of the solve.
    /// @param problem The problem.
    /// @return The answer, or an error of kind refused_problem, refused_memory or internal_error.
    /// @note Allocates the flows and the potentials, one entry per arc and per node, and the room the problem needs
    /// beyond the room held. Where exceptions are disabled and the memory for the vectors is refused, the standard
    /// library ends the program as it does without exceptions.
    /// @see solve
    KAHLNWS_HPP_NODISCARD result<answer<solution>> try_solve(const problem& problem) noexcept {
        return owned(problem);
    }

    /// @brief Solves @p problem, writes the flow of each arc to @p flows, arc a at position a, and answers the cost of
    /// an optimal flow, or that no flow exists; either way beside the work of the solve.
    /// @param problem The problem.
    /// @param[out] flows One entry per arc; what it held before does not matter.
    /// @return The answer, or an error of kind invalid_argument, refused_problem, refused_memory or internal_error.
    /// @pre @p flows holds one entry per arc, and shares no storage with a table of @p problem; answered as an error
    /// of kind invalid_argument, before anything runs.
    /// @post Where the outcome is infeasible, or the call fails other than with an internal error, @p flows is as it
    /// was.
    /// @note Allocates nothing where the room holds the problem, and nothing on an error.
    /// @see solve_into
    KAHLNWS_HPP_NODISCARD result<answer<optimum>> try_solve_into(const problem& problem,
                                                                span<std::uint64_t> flows) noexcept {
        return into(problem, flows);
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
    /// @p potentials are as they were.
    /// @note Allocates nothing where the room holds the problem, and nothing on an error.
    /// @see solve_into
    KAHLNWS_HPP_NODISCARD result<answer<optimum>> try_solve_into(const problem& problem,
                                                                span<std::uint64_t> flows,
                                                                span<std::int64_t> potentials) noexcept {
        return into(problem, flows, potentials);
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
#endif

private:
    /// Solves `problem` into vectors of its own, and answers them owned: the body of try_solve().
    result<answer<solution>> owned(const problem& problem) noexcept {
        result<solution> columns = allocated(problem.tails.size(), problem.supplies.size());
        if (!columns) {
            return columns.error();
        }
        const result<answer<optimum>> found_into = into(problem, columns->flows, columns->potentials);
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

    /// Solves `problem` into `flows` where it is admitted: the body of try_solve_into() without potentials.
    result<answer<optimum>> into(const problem& problem, span<std::uint64_t> flows) noexcept {
        const result<void> admitted = admitted_flows(problem, flows);
        if (!admitted) {
            return admitted.error();
        }
        return run(problem, flows.data(), nullptr);
    }

    /// Solves `problem` into `flows` and `potentials` where both are admitted: the body of try_solve_into() with
    /// potentials.
    result<answer<optimum>> into(const problem& problem,
                                 span<std::uint64_t> flows,
                                 span<std::int64_t> potentials) noexcept {
        const result<void> admitted_first = admitted_flows(problem, flows);
        if (!admitted_first) {
            return admitted_first.error();
        }
        const result<void> admitted_second = admitted_potentials(problem, flows, potentials);
        if (!admitted_second) {
            return admitted_second.error();
        }
        return run(problem, flows.data(), potentials.data());
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
        if (!detail::resized(owned.flows, arc_count) || !detail::resized(owned.potentials, node_count)) {
            return error::refused_memory();
        }
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

    /// A success where `flows` holds one entry per arc of `problem` and shares no storage with a table of it.
    static result<void> admitted_flows(const problem& problem, span<std::uint64_t> flows) noexcept {
        if (flows.size() != problem.tails.size()) {
            return error(error_kind::invalid_argument, std::uint64_t(flows.size()), " entries for a problem of ",
                         std::uint64_t(problem.tails.size()), " arcs");
        }
        if (overlaps_tables(problem, flows.data(), flows.size() * sizeof(std::uint64_t))) {
            return error(error_kind::invalid_argument, "the flows share storage with a table of the problem");
        }
        return result<void>();
    }

    /// A success where `potentials` holds one entry per node of `problem` and shares no storage with a table of it or
    /// with `flows`.
    static result<void> admitted_potentials(const problem& problem,
                                            span<std::uint64_t> flows,
                                            span<std::int64_t> potentials) noexcept {
        if (potentials.size() != problem.supplies.size()) {
            return error(error_kind::invalid_argument, std::uint64_t(potentials.size()),
                         " entries for a problem of ", std::uint64_t(problem.supplies.size()), " nodes");
        }
        const std::size_t bytes = potentials.size() * sizeof(std::int64_t);
        if (overlaps_tables(problem, potentials.data(), bytes) ||
            detail::overlap(potentials.data(), bytes, flows.data(), flows.size() * sizeof(std::uint64_t))) {
            return error(error_kind::invalid_argument,
                         "the potentials share storage with a table of the problem or with the flows");
        }
        return result<void>();
    }

    /// Solves `problem` into `flows`, one entry per arc, and `potentials`, one per node or null, which overlap nothing
    /// else of the call: the one call of the library's solve, and the answer it maps to.
    result<answer<optimum>> run(const problem& problem, std::uint64_t* flows, std::int64_t* potentials) noexcept {
        const result<kahlnws_problem> tables = detail::laid_out(problem);
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
            kahlnws_solver_solve(solver, &*tables, flows, potentials, &total_cost, &counts, &written);

        if (status == KAHLNWS_STATUS_REFUSED) {
            return error::of_refusal(written);
        }
        return detail::answered(status, total_cost, counts);
    }

    /// The library's solver; null where none was made yet.
    kahlnws_solver* handle_;
};

/// @brief The least and the most flow of an arc of a kahlnws::network, in units of flow: what network::try_set_bounds
/// takes and answers, and network::try_bounds reads. Plain data, an aggregate: `kahlnws::bounds{1, 5}`.
struct bounds {
    /// @brief The least flow of the arc, in units of flow.
    std::uint64_t lower_bound;
    /// @brief The most flow of the arc, in units of flow.
    std::uint64_t capacity;
};

/// @brief Whether @p left and @p right hold the same two bounds.
/// @param left Bounds.
/// @param right Other bounds.
constexpr bool operator==(const bounds& left, const bounds& right) noexcept {
    return left.lower_bound == right.lower_bound && left.capacity == right.capacity;
}

/// @brief Whether @p left and @p right differ in a bound.
/// @param left Bounds.
/// @param right Other bounds.
constexpr bool operator!=(const bounds& left, const bounds& right) noexcept { return !(left == right); }

/// @brief A network: a state that holds one min-cost flow problem and a basis of it, which its changes, loads and
/// solves carry on from.
///
/// try_load() hands it a kahlnws::problem, whose tables it copies; try_set_cost() changes the cost of one arc of the
/// problem held, try_set_capacity(), try_set_lower_bound() and try_set_bounds() one arc's bounds, try_set_supply() the
/// supply of one node, balanced by another, and try_move_supply() moves supply from one node to another, each at once
/// and only where the change reaches; try_solve() solves the problem as it then stands, and try_total_cost(),
/// try_flow(), try_flows() and try_potentials() read the optimum it found. try_tail(), try_head(), try_cost(),
/// try_lower_bound(), try_capacity(), try_bounds(), try_supply() and their columns read the problem as it stands, after
/// the load and every change since; a read of the problem needs no optimum and ends none.
///
/// Beside the solver, a kahlnws::network_simplex, which answers the problem each call hands it from nothing kept: a
/// network holds its problem and a basis of it; a change costs what it moves, a load of new numbers on the same arcs
/// carries over the basis held, and a solve after either makes the pivots they call for. A solve that makes no pivot
/// prices only the arcs the changes may have made worth using, none for a move of supply the flow held carries; a
/// solve that pivots prices at least one whole pass over the arcs. Where a change leaves much flow still to be sent,
/// so that the solve makes many pivots, a network's solve can take longer than a load of the whole problem as it
/// stands and the solve after it.
///
/// A network never loaded holds the problem of no node and no arc: its solve answers the optimum of no flow at the cost
/// zero, and no arc or node is below its counts. Each setter answers what it replaced, so setting that number back,
/// or moving the supply back, makes the problem the earlier one again: the next solve answers its outcome and its cost,
/// with a flow of that cost, which among tied optima may be another than before. A capacity of zero on an arc whose
/// lower bound is zero switches it off, and the capacity it answered switches it on again. A change that is refused,
/// or that sets the number already held, changes nothing, and keeps the optimum the network held; every other change
/// ends it, as do a load and a solve that finds no flow: a read then answers an error of kind error_kind::no_optimum
/// until a solve finds one, and never a number of an earlier state.
///
/// Its answers are a function of the problem loaded and of the calls made since, in order: two networks given the same
/// calls answer alike, their work included. The outcome and the cost are those of the problem as it stands, the
/// potentials prove the flow optimal, and the flow among tied optima, the potentials and the work are the network's
/// own. Where a kahlnws::network_simplex admits the problem as it stands, it answers the same outcome and cost. After
/// moves of supply and changes of lower bounds a network may hold numbers a solver and a load refuse: every supply
/// stays within @c std::int64_t, but the supplies net of the lower bounds may pass the limit
/// breach::net_supply_beyond_range names, where the arcs carry the difference; the network still solves them exactly.
///
/// An owning handle, movable in every build and copied by its copy constructor and copy assignment only where
/// exceptions are enabled; try_clone() and try_clone_from() copy in every build. A copy holds the problem, the basis
/// and the optimum, solves from that basis, and changes apart from the network it was made from: a copy made before a
/// change is the earlier state itself. A new network holds no handle of the library: try_reserve(), try_load(),
/// try_solve() and try_clone_from() make one when they need it, and a handle refused is the error
/// error_kind::refused_memory. A moved-from network is in the same state as a new one. The handle and the room are
/// freed with the network.
///
/// Where a change fails, the network is as it was: nothing changed, the optimum held is kept. Where a load or a copy
/// into it fails, the network holds what it held, its optimum included. After an internal error, which only a defect
/// of the library makes, the network may be half changed: an arc or a node not below the counts held before is still
/// answered as error_kind::invalid_argument first; beyond that, every change, try_solve() and every read of the problem
/// answer error_kind::internal_error, the reads of the optimum error_kind::no_optimum, node_count() and arc_count() the
/// counts held before, a copy is as half changed, and try_load() mends it.
class network {
public:
    /// @brief A network never loaded, which holds the problem of no node; it allocates nothing.
    network() noexcept : handle_(nullptr) {}

    /// @brief A network that takes @p other's handle, problem and room; @p other is left as a new network.
    /// @param other The network.
    network(network&& other) noexcept : handle_(other.handle_) { other.handle_ = nullptr; }

    /// @brief Frees this network's handle and room and takes @p other's; @p other is left as a new network.
    /// @param other The network; moving a network onto itself leaves it as it was.
    /// @return This network.
    network& operator=(network&& other) noexcept {
        network taken(std::move(other));
        swap(taken);
        return *this;
    }

#if KAHLNWS_EXCEPTIONS
    /// @brief A copy of @p other, as try_clone() makes it; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param other The network.
    /// @throws std::bad_alloc Where the memory for the copy's handle or its room is refused.
    network(const network& other) : network(other.try_clone().value()) {}

    /// @brief Makes this network a copy of @p other, reusing its room, as try_clone_from() does; where the copy fails,
    /// this network is as it was. Only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param other The network; assigning a network to itself leaves it as it was.
    /// @return This network.
    /// @throws std::bad_alloc Where the memory for this network's handle or for the room the copy needs is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    network& operator=(const network& other) {
        try_clone_from(other).value();
        return *this;
    }
#else
    network(const network&) = delete;
    network& operator=(const network&) = delete;
#endif

    /// @brief Frees the handle and the room.
    ~network() { kahlnws_network_free(handle_); }

    /// @brief Exchanges the handles, problems and rooms of this network and @p other.
    /// @param[in,out] other The network.
    void swap(network& other) noexcept { std::swap(handle_, other.handle_); }

    /// @brief Exchanges the handles, problems and rooms of @p first and @p second.
    /// @param[in,out] first A network.
    /// @param[in,out] second Another network.
    friend void swap(network& first, network& second) noexcept { first.swap(second); }

    /// @brief A copy of this network, which holds its problem, its basis and its optimum and changes apart from it.
    ///
    /// The copy answers every call as this network would, its work included. It allocates a handle and room of its
    /// own, as large as the problem held, and its changes and solves allocate nothing; a network half changed copies as
    /// half changed.
    /// @return The copy, or an error of kind refused_memory, where the memory for the copy's handle or for its room is
    /// refused.
    /// @see network(const network&)
    KAHLNWS_HPP_NODISCARD result<network> try_clone() const noexcept {
        if (handle_ == nullptr) {
            return network();
        }
        kahlnws_network* const copy = kahlnws_network_clone(handle_);
        if (copy == nullptr) {
            return error::refused_memory();
        }
        return network(copy);
    }

    /// @brief Makes this network a copy of @p source, which holds its problem, its basis and its optimum and changes
    /// apart from it, reusing this network's room.
    ///
    /// Where the room holds a problem of the counts of the problem @p source holds, by try_reserve() or an earlier load
    /// or copy, it allocates nothing: a search may keep a network for each state it may return to and copy into it.
    /// A copy of a network that holds no handle, a new or moved-from one, makes this network one that holds none,
    /// freeing its room; a copy of itself changes nothing.
    /// @param source The network copied.
    /// @return A success, or an error of kind refused_memory or internal_error.
    /// @post On refused memory this network holds what it held, its optimum included; its room may have grown.
    /// @see operator=(const network&)
    KAHLNWS_HPP_NODISCARD result<void> try_clone_from(const network& source) noexcept {
        if (this == &source) {
            return result<void>();
        }
        if (source.handle_ == nullptr) {
            network emptied;
            swap(emptied);
            return result<void>();
        }
        kahlnws_network* const held = handle();
        if (held == nullptr) {
            return error::refused_memory();
        }
        const kahlnws_status status = kahlnws_network_copy(held, source.handle_);
        if (status != KAHLNWS_STATUS_OK) {
            return error::of_status(status);
        }
        return result<void>();
    }

    /// @brief The number of nodes of the problem held: 0 for a network never loaded. Every node a change names is
    /// below it.
    std::size_t node_count() const noexcept { return kahlnws_network_node_count(handle_); }

    /// @brief The number of arcs of the problem held: 0 for a network never loaded. Every arc a change or a read names
    /// is below it, and the flows are as many.
    std::size_t arc_count() const noexcept { return kahlnws_network_arc_count(handle_); }

    /// @brief Makes room for problems of up to @p node_count nodes and @p arc_count arcs: a load of such a problem
    /// allocates nothing in the library afterwards. Room already held is kept, and so are the problem held and its
    /// optimum.
    /// @param node_count The most nodes.
    /// @param arc_count The most arcs.
    /// @return A success, or an error of kind refused_memory, or internal_error.
    /// @post On an error, the room held before is kept; the problem and its optimum are kept either way.
    /// @see reserve
    KAHLNWS_HPP_NODISCARD result<void> try_reserve(std::size_t node_count, std::size_t arc_count) noexcept {
        kahlnws_network* const held = handle();
        if (held == nullptr) {
            return error::refused_memory();
        }
        const kahlnws_status status = kahlnws_network_reserve(held, node_count, arc_count);
        if (status != KAHLNWS_STATUS_OK) {
            return error::of_status(status);
        }
        return result<void>();
    }

    /// @brief Makes @p problem the problem the network holds, in place of the one it held, with no optimum answered
    /// yet, and a basis of it to start the next try_solve() from.
    ///
    /// The basis is the one the network holds, carried over to the new numbers, where it serves: where the network
    /// holds a basis of a problem with the same node count, the same arc count and the same tail and head at every arc,
    /// and the supplies net of the lower bounds and of what that basis carries on its arcs out of the tree add up to at
    /// most 2^64 - 2 where they are positive. Otherwise, and where the network holds none, it is the star, every arc at
    /// its lower bound, as the first load starts from; either way the next solve prices every arc. The solve after a
    /// load answers the same outcome and the same cost
    /// whichever basis the load started from; the flow among tied optima, the potentials and the work may differ, each
    /// a deterministic function of the calls made on the network. Where only some numbers changed, the solve often
    /// makes far fewer pivots than one from the star; the same problem loaded again, from the network's optimum, makes
    /// none.
    ///
    /// The problem's tables are copied: the caller may change or free them afterwards. It costs a few passes over the
    /// arcs and the nodes, and refuses what kahlnws::network_simplex::try_solve refuses, with the same breach.
    /// @param problem The problem.
    /// @return A success, or an error of kind refused_problem, refused_memory or internal_error.
    /// @post On an error other than an internal error, the network is as it was, its basis and its optimum included,
    /// and the next load starts from that basis where it serves.
    /// @note Allocates the room the problem needs beyond the room held, and nothing on an error.
    /// @see load
    KAHLNWS_HPP_NODISCARD result<void> try_load(const problem& problem) noexcept {
        const result<kahlnws_problem> tables = detail::laid_out(problem);
        if (!tables) {
            return tables.error();
        }
        kahlnws_network* const held = handle();
        if (held == nullptr) {
            return error::refused_memory();
        }
        kahlnws_refusal written = kahlnws_refusal();
        const kahlnws_status status = kahlnws_network_load(held, &*tables, &written);
        switch (status) {
        case KAHLNWS_STATUS_OK:
            return result<void>();
        case KAHLNWS_STATUS_REFUSED:
            return error::of_refusal(written);
        }
        return error::of_status(status);
    }

    /// @brief Makes @p cost the cost of a unit of flow on @p arc, in units of cost, and answers the cost it had.
    ///
    /// The cost the arc already has changes nothing, and keeps an optimum the network held; any other ends it. Setting
    /// the cost answered back makes the problem the earlier one again. It costs a few steps where the change moves no
    /// node's potential, and otherwise the nodes whose potentials it moves, which can be most of them; the next solve
    /// prices the arc, and where potentials moved, the arcs at the smaller of the two groups of nodes, those whose
    /// potentials moved and the rest. It allocates nothing.
    /// @param arc The arc, below arc_count().
    /// @param cost Its new cost, in units of cost.
    /// @return The cost it had, or an error of kind invalid_argument, refused_change or internal_error.
    /// @pre @p arc is below arc_count(); answered as an error of kind invalid_argument, nothing changed.
    /// @post On a refusal, change_breach::cost_beyond_bound where @p cost lies beyond plus or minus
    /// kahlnws::cost_bound of the node count, else change_breach::cost_total_beyond_range, the network is as it was.
    /// @see set_cost
    KAHLNWS_HPP_NODISCARD result<std::int64_t> try_set_cost(std::size_t arc, std::int64_t cost) noexcept {
        std::int64_t previous = 0;
        kahlnws_refusal written = kahlnws_refusal();
        const kahlnws_status status = kahlnws_network_set_cost(handle_, arc, cost, &previous, &written);
        const result<void> changed = changed_arc(status, written, arc);
        if (!changed) {
            return changed.error();
        }
        return previous;
    }

    /// @brief Makes @p capacity the most flow of @p arc, in units of flow, and answers the capacity it had.
    ///
    /// The capacity the arc already has changes nothing, and keeps an optimum the network held; any other ends it.
    /// Setting the capacity answered back makes the problem the earlier one again. A capacity of zero on an arc whose
    /// lower bound is zero switches the arc off, and the capacity it answered switches it on. Where both bounds are to
    /// move past each other's old value, try_set_bounds() moves them at once. It costs a few steps where the arc's
    /// flow can stay where it is, and otherwise what try_move_supply() of the flow it moves costs, at most the change
    /// of the capacity. It allocates nothing.
    /// @param arc The arc, below arc_count().
    /// @param capacity Its new capacity, in units of flow.
    /// @return The capacity it had, or an error of kind invalid_argument, refused_change or internal_error.
    /// @pre @p arc is below arc_count(); answered as an error of kind invalid_argument, nothing changed.
    /// @post On a refusal, change_breach::lower_above_capacity where @p capacity is below the arc's lower bound, else
    /// change_breach::cost_total_beyond_range, else change_breach::excess_beyond_range, the network is as it was. The
    /// capacity answered, set back directly after, is admitted wherever the flow still to be sent before the change
    /// and twice the change of the capacity add up to at most 2^64 - 2: never refused below 2^62.
    /// @see set_capacity
    KAHLNWS_HPP_NODISCARD result<std::uint64_t> try_set_capacity(std::size_t arc, std::uint64_t capacity) noexcept {
        std::uint64_t previous = 0;
        kahlnws_refusal written = kahlnws_refusal();
        const kahlnws_status status = kahlnws_network_set_capacity(handle_, arc, capacity, &previous, &written);
        const result<void> changed = changed_arc(status, written, arc);
        if (!changed) {
            return changed.error();
        }
        return previous;
    }

    /// @brief Makes @p lower_bound the least flow of @p arc, in units of flow, and answers the lower bound it had.
    ///
    /// The lower bound the arc already has changes nothing, and keeps an optimum the network held; any other ends it.
    /// Setting the lower bound answered back makes the problem the earlier one again; every flow answered includes its
    /// arc's lower bound. Where both bounds are to move past each other's old value, try_set_bounds() moves them at
    /// once. A lower bound changes no supply: the supplies net of the lower bounds may so come to pass the limit
    /// breach::net_supply_beyond_range names, where the arcs carry the difference, and the network still solves them
    /// exactly, where a kahlnws::network_simplex refuses them. It costs a few steps where the arc's flow can stay where
    /// it is, and otherwise what try_move_supply() of the flow it moves costs, at most the change of the lower bound.
    /// It allocates nothing.
    /// @param arc The arc, below arc_count().
    /// @param lower_bound Its new lower bound, in units of flow.
    /// @return The lower bound it had, or an error of kind invalid_argument, refused_change or internal_error.
    /// @pre @p arc is below arc_count(); answered as an error of kind invalid_argument, nothing changed.
    /// @post On a refusal, change_breach::lower_above_capacity where @p lower_bound is above the arc's capacity, else
    /// change_breach::excess_beyond_range, the network is as it was. The lower bound answered, set back directly
    /// after, is admitted wherever the flow still to be sent before the change and twice the change of the lower
    /// bound add up to at most 2^64 - 2: never refused below 2^62.
    /// @see set_lower_bound
    KAHLNWS_HPP_NODISCARD result<std::uint64_t> try_set_lower_bound(std::size_t arc,
                                                                    std::uint64_t lower_bound) noexcept {
        std::uint64_t previous = 0;
        kahlnws_refusal written = kahlnws_refusal();
        const kahlnws_status status = kahlnws_network_set_lower_bound(handle_, arc, lower_bound, &previous, &written);
        const result<void> changed = changed_arc(status, written, arc);
        if (!changed) {
            return changed.error();
        }
        return previous;
    }

    /// @brief Makes @p arc_bounds the least and the most flow of @p arc at once, in units of flow, and answers the
    /// bounds it had.
    ///
    /// The single setters check each bound against the other the arc holds, so where both move past each other's old
    /// value, one order of two calls is refused: bounds of 0 to 2 become 3 to 4 by the capacity first, and 3 to 4
    /// become 0 to 2 by the lower bound first. This sets both at once and checks the pair alone; otherwise it is
    /// try_set_capacity() and try_set_lower_bound() in one, its cost at most what try_move_supply() of the larger
    /// change of the two bounds costs. The bounds it answers are what it takes, so `set_bounds(arc, had)` sets them
    /// back. It allocates nothing.
    /// @param arc The arc, below arc_count().
    /// @param arc_bounds Its new lower bound and capacity, in units of flow.
    /// @return The bounds it had, or an error of kind invalid_argument, refused_change or internal_error.
    /// @pre @p arc is below arc_count(); answered as an error of kind invalid_argument, nothing changed.
    /// @post On a refusal, change_breach::lower_above_capacity where the lower bound is above the capacity, else
    /// change_breach::cost_total_beyond_range, else change_breach::excess_beyond_range, the network is as it was. The
    /// bounds answered, set back directly after, are admitted wherever the flow still to be sent before the change
    /// and twice the larger change of the two bounds add up to at most 2^64 - 2: never refused below 2^62.
    /// @see set_bounds
    KAHLNWS_HPP_NODISCARD result<kahlnws::bounds> try_set_bounds(std::size_t arc,
                                                                 const kahlnws::bounds& arc_bounds) noexcept {
        kahlnws::bounds previous = {0, 0};
        kahlnws_refusal written = kahlnws_refusal();
        const kahlnws_status status =
            kahlnws_network_set_bounds(handle_, arc, arc_bounds.lower_bound, arc_bounds.capacity,
                                       &previous.lower_bound, &previous.capacity, &written);
        const result<void> changed = changed_arc(status, written, arc);
        if (!changed) {
            return changed.error();
        }
        return previous;
    }

    /// @brief Makes @p supply the supply of @p node, in units of flow, a demand below zero, and answers the supply it
    /// had; the supply of @p balancing_node moves by the same amount the other way, so the supplies still add up to
    /// zero.
    ///
    /// A depot that is to send 2 units where it sent 4, balanced by the customer it sends to, is
    /// `try_set_supply(depot, 2, customer)`: the customer then takes 2 less. The same call with the supply it answered
    /// sets both nodes back. The supply @p node already has changes nothing, is never refused, and keeps an optimum the
    /// network held, as a move of nothing does; any other ends it. A node that balances itself is given only the supply
    /// it has, which changes nothing; any other supply for it answers an error of kind invalid_argument, as an index
    /// not below its count does. It is try_move_supply() of the difference between the supply asked for and the one
    /// @p node has: from @p balancing_node to @p node where the supply rises, from @p node to @p balancing_node where
    /// it falls. It costs what that move costs, and allocates nothing.
    /// @param node The node whose supply is set, below node_count().
    /// @param supply Its new supply, in units of flow: positive where it sends, negative where it takes.
    /// @param balancing_node The node whose supply moves the other way, below node_count().
    /// @return The supply @p node had, or an error of kind invalid_argument, refused_change or internal_error.
    /// @pre @p node and @p balancing_node are below node_count(), and where they are one node, @p supply is the supply
    /// it has; answered as an error of kind invalid_argument, nothing changed.
    /// @post On a refusal, change_breach::supply_beyond_range where the supply of @p balancing_node would leave the
    /// range of a @c std::int64_t, naming it with the supply it would have, else change_breach::excess_beyond_range,
    /// the network is as it was. The supply answered, set back directly after, is admitted where the move it makes
    /// would be, as try_move_supply() says.
    /// @see set_supply
    KAHLNWS_HPP_NODISCARD result<std::int64_t> try_set_supply(std::size_t node,
                                                              std::int64_t supply,
                                                              std::size_t balancing_node) noexcept {
        std::int64_t previous = 0;
        kahlnws_refusal written = kahlnws_refusal();
        const kahlnws_status status =
            kahlnws_network_set_supply(handle_, node, supply, balancing_node, &previous, &written);
        if (status == KAHLNWS_STATUS_INVALID_ARGUMENT || status == KAHLNWS_STATUS_NULL_POINTER) {
            return supply_not_set(node, supply, balancing_node);
        }
        const result<void> changed_supply = changed(status, written);
        if (!changed_supply) {
            return changed_supply.error();
        }
        return previous;
    }

    /// @brief Lowers the supply of @p from_node by @p amount and raises the supply of @p to_node by @p amount, in units
    /// of flow, so the supplies still add up to zero; a move of demand is written from the node that will demand more,
    /// a customer that needs two more units, sent by a depot, being `try_move_supply(customer, depot, 2)`.
    ///
    /// A node that sends gains supply, one that receives gains demand, a supply below zero. A node is brought to a
    /// supply it should have, balanced by another node, by try_set_supply(). The supplies a network holds
    /// stay within @c std::int64_t; their net supplies may pass the limit breach::net_supply_beyond_range names, and
    /// the network still solves them exactly. The same node twice, or an amount of zero, changes nothing, is
    /// never refused and keeps an optimum the network held; any other move ends it. The move back,
    /// `try_move_supply(to_node, from_node, amount)`, makes the problem the earlier one again; called directly after,
    /// it is admitted wherever the move left the flow still to be sent as it was or lower, always where the routed flow
    /// carried the move, and wherever that flow before the move and twice the amount add up to at most 2^64 - 2. It
    /// costs a walk from each of the two nodes along the flow the network routes, as far as their routes meet, where
    /// that flow can carry the amount, and then the next solve prices no arc for it; more where it cannot, up to most
    /// of the network. It allocates nothing.
    /// @param from_node The node whose supply falls, below node_count().
    /// @param to_node The node whose supply rises, below node_count().
    /// @param amount The units of flow moved.
    /// @return A success, or an error of kind invalid_argument, refused_change or internal_error.
    /// @pre @p from_node and @p to_node are below node_count(); answered as an error of kind invalid_argument, nothing
    /// changed.
    /// @post On a refusal, change_breach::supply_beyond_range where a supply would leave the range of a
    /// @c std::int64_t, naming @p from_node first, else change_breach::excess_beyond_range, the network is as it was.
    /// @see move_supply
    KAHLNWS_HPP_NODISCARD result<void> try_move_supply(std::size_t from_node,
                                                       std::size_t to_node,
                                                       std::uint64_t amount) noexcept {
        kahlnws_refusal written = kahlnws_refusal();
        const kahlnws_status status = kahlnws_network_move_supply(handle_, from_node, to_node, amount, &written);
        if (status == KAHLNWS_STATUS_INVALID_ARGUMENT || status == KAHLNWS_STATUS_NULL_POINTER) {
            return node_outside(from_node < node_count() ? to_node : from_node);
        }
        return changed(status, written);
    }

    /// @brief Solves the problem held and answers the cost of an optimal flow, or that no flow exists; either way
    /// beside the work of the solve. The network keeps the optimum for the reads.
    ///
    /// It pivots from the basis the network holds: the one its last solve ended on and the changes since left, or the
    /// one a load started from. So a network nothing changed since its last solve answers the same again and makes no
    /// pivot; a solve after changes that call for no pivot prices only the arcs those changes may have made worth
    /// using; and a solve that pivots prices at least one whole pass over the arcs. It tries no arcs first, so its
    /// work.first_arc_pivots is 0. A network never loaded answers the optimum of no flow at the cost zero, with no
    /// work. The cost is that of the problem with its lower bounds.
    /// @return The answer, or an error of kind refused_memory, for a new network's handle, or internal_error.
    /// @note Allocates nothing but a new network's handle.
    /// @see solve
    KAHLNWS_HPP_NODISCARD result<answer<optimum> > try_solve() noexcept {
        kahlnws_network* const held = handle();
        if (held == nullptr) {
            return error::refused_memory();
        }
        kahlnws_int128 total_cost = {0, 0};
        kahlnws_work counts = {0, 0, 0};
        const kahlnws_status status = kahlnws_network_solve(held, &total_cost, &counts);
        return detail::answered(status, total_cost, counts);
    }

    /// @brief The cost of the optimum the network holds, in units of cost times units of flow.
    ///
    /// A network holds an optimum where the last call that loaded, changed or solved it was a solve that found one,
    /// and on a network never loaded, whose optimum is that of no node; a change that was refused, or that set the
    /// number already held, is no change. It reads, and costs a few steps.
    /// @return The cost, or an error of kind no_optimum or internal_error.
    /// @see total_cost
    KAHLNWS_HPP_NODISCARD result<int128> try_total_cost() const noexcept {
        kahlnws_int128 total_cost = {0, 0};
        const result<void> read = read_of(kahlnws_network_total_cost(handle_, &total_cost));
        if (!read) {
            return read.error();
        }
        return int128::from_halves(total_cost.low, total_cost.high);
    }

    /// @brief The flow of @p arc in the optimum the network holds, in units of flow, its lower bound included.
    ///
    /// A network holds an optimum as try_total_cost() says. It reads one flow, and costs a few steps.
    /// @param arc The arc, below arc_count().
    /// @return The flow, or an error of kind invalid_argument, no_optimum or internal_error.
    /// @pre @p arc is below arc_count(); answered as an error of kind invalid_argument.
    /// @see flow
    KAHLNWS_HPP_NODISCARD result<std::uint64_t> try_flow(std::size_t arc) const noexcept {
        std::uint64_t flow = 0;
        const kahlnws_status status = kahlnws_network_flow(handle_, arc, &flow);
        if (status == KAHLNWS_STATUS_INVALID_ARGUMENT || status == KAHLNWS_STATUS_NULL_POINTER) {
            return arc_outside(arc);
        }
        const result<void> read = read_of(status);
        if (!read) {
            return read.error();
        }
        return flow;
    }

    /// @brief The flow of every arc in the optimum the network holds, owned, arc a at position a, in units of flow,
    /// each with its arc's lower bound.
    ///
    /// A network holds an optimum as try_total_cost() says.
    /// @return The flows, or an error of kind refused_memory, no_optimum or internal_error.
    /// @note Allocates the flows, one entry per arc. Where exceptions are disabled and the memory for the vector is
    /// refused, the standard library ends the program as it does without exceptions.
    /// @see flows
    KAHLNWS_HPP_NODISCARD result<std::vector<std::uint64_t> > try_flows() const noexcept {
        std::vector<std::uint64_t> flows;
        if (!detail::resized(flows, arc_count())) {
            return error::refused_memory();
        }
        const result<void> read = try_flows_into(flows);
        if (!read) {
            return read.error();
        }
        return result<std::vector<std::uint64_t> >(std::move(flows));
    }

    /// @brief Writes the flow of every arc in the optimum the network holds to @p flows, arc a at position a, in units
    /// of flow, each with its arc's lower bound.
    ///
    /// A network holds an optimum as try_total_cost() says.
    /// @param[out] flows One entry per arc; what it held before does not matter.
    /// @return A success, or an error of kind invalid_argument, no_optimum or internal_error.
    /// @pre @p flows holds one entry per arc; answered as an error of kind invalid_argument.
    /// @post On an error, @p flows is as it was.
    /// @note Allocates nothing.
    /// @see flows_into
    KAHLNWS_HPP_NODISCARD result<void> try_flows_into(span<std::uint64_t> flows) const noexcept {
        const std::size_t arcs = arc_count();
        if (flows.size() != arcs) {
            return error(error_kind::invalid_argument, std::uint64_t(flows.size()), " entries for a network of ",
                         std::uint64_t(arcs), " arcs");
        }
        return read_of(kahlnws_network_flows(handle_, flows.data()));
    }

    /// @brief Each node's potential in the optimum the network holds, owned, node v at position v, in units of cost.
    ///
    /// With the potentials, `cost + potentials[tail] - potentials[head]` is at least zero on every arc whose flow is
    /// below its capacity and at most zero on every arc whose flow is above its lower bound, which proves the flow
    /// optimal. A network holds an optimum as try_total_cost() says.
    /// @return The potentials, or an error of kind refused_memory, no_optimum or internal_error.
    /// @note Allocates the potentials, one entry per node. Where exceptions are disabled and the memory for the vector
    /// is refused, the standard library ends the program as it does without exceptions.
    /// @see potentials
    KAHLNWS_HPP_NODISCARD result<std::vector<std::int64_t> > try_potentials() const noexcept {
        std::vector<std::int64_t> potentials;
        if (!detail::resized(potentials, node_count())) {
            return error::refused_memory();
        }
        const result<void> read = try_potentials_into(potentials);
        if (!read) {
            return read.error();
        }
        return result<std::vector<std::int64_t> >(std::move(potentials));
    }

    /// @brief Writes each node's potential in the optimum the network holds to @p potentials, node v at position v, in
    /// units of cost.
    ///
    /// The potentials prove the flow optimal, as try_potentials() says. A network holds an optimum as try_total_cost()
    /// says.
    /// @param[out] potentials One entry per node; what it held before does not matter.
    /// @return A success, or an error of kind invalid_argument, no_optimum or internal_error.
    /// @pre @p potentials holds one entry per node; answered as an error of kind invalid_argument.
    /// @post On an error, @p potentials is as it was.
    /// @note Allocates nothing.
    /// @see potentials_into
    KAHLNWS_HPP_NODISCARD result<void> try_potentials_into(span<std::int64_t> potentials) const noexcept {
        const std::size_t nodes = node_count();
        if (potentials.size() != nodes) {
            return error(error_kind::invalid_argument, std::uint64_t(potentials.size()),
                         " entries for a network of ", std::uint64_t(nodes), " nodes");
        }
        return read_of(kahlnws_network_potentials(handle_, potentials.data()));
    }

    /// @brief The node @p arc leaves, in the problem the network holds.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
    /// @param arc The arc, below arc_count().
    /// @return The tail, or an error of kind invalid_argument or internal_error.
    /// @pre @p arc is below arc_count(); answered as an error of kind invalid_argument.
    /// @see tail
    KAHLNWS_HPP_NODISCARD result<std::size_t> try_tail(std::size_t arc) const noexcept {
        return read_arc(kahlnws_network_tail, arc);
    }

    /// @brief The node @p arc enters, in the problem the network holds.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
    /// @param arc The arc, below arc_count().
    /// @return The head, or an error of kind invalid_argument or internal_error.
    /// @pre @p arc is below arc_count(); answered as an error of kind invalid_argument.
    /// @see head
    KAHLNWS_HPP_NODISCARD result<std::size_t> try_head(std::size_t arc) const noexcept {
        return read_arc(kahlnws_network_head, arc);
    }

    /// @brief The cost of a unit of flow on @p arc, in units of cost, in the problem the network holds.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
    /// @param arc The arc, below arc_count().
    /// @return The cost, or an error of kind invalid_argument or internal_error.
    /// @pre @p arc is below arc_count(); answered as an error of kind invalid_argument.
    /// @see cost
    KAHLNWS_HPP_NODISCARD result<std::int64_t> try_cost(std::size_t arc) const noexcept {
        return read_arc(kahlnws_network_cost, arc);
    }

    /// @brief The least flow of @p arc, in units of flow, in the problem the network holds.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
    /// @param arc The arc, below arc_count().
    /// @return The lower bound, or an error of kind invalid_argument or internal_error.
    /// @pre @p arc is below arc_count(); answered as an error of kind invalid_argument.
    /// @see lower_bound
    KAHLNWS_HPP_NODISCARD result<std::uint64_t> try_lower_bound(std::size_t arc) const noexcept {
        return read_arc(kahlnws_network_lower_bound, arc);
    }

    /// @brief The most flow of @p arc, in units of flow, in the problem the network holds.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
    /// @param arc The arc, below arc_count().
    /// @return The capacity, or an error of kind invalid_argument or internal_error.
    /// @pre @p arc is below arc_count(); answered as an error of kind invalid_argument.
    /// @see capacity
    KAHLNWS_HPP_NODISCARD result<std::uint64_t> try_capacity(std::size_t arc) const noexcept {
        return read_arc(kahlnws_network_capacity, arc);
    }

    /// @brief The lower bound and the capacity of @p arc, in units of flow, in the problem the network holds: the
    /// bounds try_set_bounds() takes, so that bounds read and set back make the arc what it was.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded no arc is below its count. It costs a few steps.
    /// @param arc The arc, below arc_count().
    /// @return The bounds, or an error of kind invalid_argument or internal_error.
    /// @pre @p arc is below arc_count(); answered as an error of kind invalid_argument.
    /// @see bounds
    KAHLNWS_HPP_NODISCARD result<kahlnws::bounds> try_bounds(std::size_t arc) const noexcept {
        kahlnws::bounds read = {0, 0};
        const kahlnws_status status = kahlnws_network_bounds(handle_, arc, &read.lower_bound, &read.capacity);
        const result<void> answered = arc_read(status, arc);
        if (!answered) {
            return answered.error();
        }
        return read;
    }

    /// @brief The supply of @p node, in units of flow, in the problem the network holds: positive where it sends,
    /// negative where it takes.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded no node is below its count. It costs a few steps.
    /// @param node The node, below node_count().
    /// @return The supply, or an error of kind invalid_argument or internal_error.
    /// @pre @p node is below node_count(); answered as an error of kind invalid_argument.
    /// @see supply
    KAHLNWS_HPP_NODISCARD result<std::int64_t> try_supply(std::size_t node) const noexcept {
        std::int64_t supply = 0;
        const kahlnws_status status = kahlnws_network_supply(handle_, node, &supply);
        if (status == KAHLNWS_STATUS_INVALID_ARGUMENT || status == KAHLNWS_STATUS_NULL_POINTER) {
            return node_outside(node);
        }
        const result<void> answered = problem_read(status);
        if (!answered) {
            return answered.error();
        }
        return supply;
    }

    /// @brief The node each arc leaves, in the problem the network holds, owned, arc a at position a.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded the vector is empty.
    /// @return The entries, or an error of kind refused_memory or internal_error.
    /// @note Allocates the entries, one per arc. Where exceptions are disabled and the memory for the vector is
    /// refused, the standard library ends the program as it does without exceptions.
    /// @see tails
    KAHLNWS_HPP_NODISCARD result<std::vector<std::size_t> > try_tails() const noexcept {
        return owned_column(kahlnws_network_tails, indexed_by::arc);
    }

    /// @brief Writes the node each arc leaves, in the problem the network holds, to @p tails, arc a at position a.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded nothing is written.
    /// @param[out] tails One entry per arc; what it held before does not matter.
    /// @return A success, or an error of kind invalid_argument or internal_error.
    /// @pre @p tails holds one entry per arc; answered as an error of kind invalid_argument.
    /// @post On an error, @p tails is as it was.
    /// @note Allocates nothing.
    /// @see tails_into
    KAHLNWS_HPP_NODISCARD result<void> try_tails_into(span<std::size_t> tails) const noexcept {
        return read_column(kahlnws_network_tails, tails, indexed_by::arc);
    }

    /// @brief The node each arc enters, in the problem the network holds, owned, arc a at position a.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded the vector is empty.
    /// @return The entries, or an error of kind refused_memory or internal_error.
    /// @note Allocates the entries, one per arc. Where exceptions are disabled and the memory for the vector is
    /// refused, the standard library ends the program as it does without exceptions.
    /// @see heads
    KAHLNWS_HPP_NODISCARD result<std::vector<std::size_t> > try_heads() const noexcept {
        return owned_column(kahlnws_network_heads, indexed_by::arc);
    }

    /// @brief Writes the node each arc enters, in the problem the network holds, to @p heads, arc a at position a.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded nothing is written.
    /// @param[out] heads One entry per arc; what it held before does not matter.
    /// @return A success, or an error of kind invalid_argument or internal_error.
    /// @pre @p heads holds one entry per arc; answered as an error of kind invalid_argument.
    /// @post On an error, @p heads is as it was.
    /// @note Allocates nothing.
    /// @see heads_into
    KAHLNWS_HPP_NODISCARD result<void> try_heads_into(span<std::size_t> heads) const noexcept {
        return read_column(kahlnws_network_heads, heads, indexed_by::arc);
    }

    /// @brief The cost of a unit of flow on each arc, in units of cost, in the problem the network holds, owned, arc a
    /// at position a.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded the vector is empty.
    /// @return The entries, or an error of kind refused_memory or internal_error.
    /// @note Allocates the entries, one per arc. Where exceptions are disabled and the memory for the vector is
    /// refused, the standard library ends the program as it does without exceptions.
    /// @see costs
    KAHLNWS_HPP_NODISCARD result<std::vector<std::int64_t> > try_costs() const noexcept {
        return owned_column(kahlnws_network_costs, indexed_by::arc);
    }

    /// @brief Writes the cost of a unit of flow on each arc, in units of cost, in the problem the network holds, to @p
    /// costs, arc a at position a.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded nothing is written.
    /// @param[out] costs One entry per arc; what it held before does not matter.
    /// @return A success, or an error of kind invalid_argument or internal_error.
    /// @pre @p costs holds one entry per arc; answered as an error of kind invalid_argument.
    /// @post On an error, @p costs is as it was.
    /// @note Allocates nothing.
    /// @see costs_into
    KAHLNWS_HPP_NODISCARD result<void> try_costs_into(span<std::int64_t> costs) const noexcept {
        return read_column(kahlnws_network_costs, costs, indexed_by::arc);
    }

    /// @brief The lower bound of each arc, in units of flow, in the problem the network holds, owned, arc a at position
    /// a.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded the vector is empty.
    /// @return The entries, or an error of kind refused_memory or internal_error.
    /// @note Allocates the entries, one per arc. Where exceptions are disabled and the memory for the vector is
    /// refused, the standard library ends the program as it does without exceptions.
    /// @see lower_bounds
    KAHLNWS_HPP_NODISCARD result<std::vector<std::uint64_t> > try_lower_bounds() const noexcept {
        return owned_column(kahlnws_network_lower_bounds, indexed_by::arc);
    }

    /// @brief Writes the lower bound of each arc, in units of flow, in the problem the network holds, to @p
    /// lower_bounds, arc a at position a.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded nothing is written.
    /// @param[out] lower_bounds One entry per arc; what it held before does not matter.
    /// @return A success, or an error of kind invalid_argument or internal_error.
    /// @pre @p lower_bounds holds one entry per arc; answered as an error of kind invalid_argument.
    /// @post On an error, @p lower_bounds is as it was.
    /// @note Allocates nothing.
    /// @see lower_bounds_into
    KAHLNWS_HPP_NODISCARD result<void> try_lower_bounds_into(span<std::uint64_t> lower_bounds) const noexcept {
        return read_column(kahlnws_network_lower_bounds, lower_bounds, indexed_by::arc);
    }

    /// @brief The capacity of each arc, in units of flow, in the problem the network holds, owned, arc a at position a.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded the vector is empty.
    /// @return The entries, or an error of kind refused_memory or internal_error.
    /// @note Allocates the entries, one per arc. Where exceptions are disabled and the memory for the vector is
    /// refused, the standard library ends the program as it does without exceptions.
    /// @see capacities
    KAHLNWS_HPP_NODISCARD result<std::vector<std::uint64_t> > try_capacities() const noexcept {
        return owned_column(kahlnws_network_capacities, indexed_by::arc);
    }

    /// @brief Writes the capacity of each arc, in units of flow, in the problem the network holds, to @p capacities,
    /// arc a at position a.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded nothing is written.
    /// @param[out] capacities One entry per arc; what it held before does not matter.
    /// @return A success, or an error of kind invalid_argument or internal_error.
    /// @pre @p capacities holds one entry per arc; answered as an error of kind invalid_argument.
    /// @post On an error, @p capacities is as it was.
    /// @note Allocates nothing.
    /// @see capacities_into
    KAHLNWS_HPP_NODISCARD result<void> try_capacities_into(span<std::uint64_t> capacities) const noexcept {
        return read_column(kahlnws_network_capacities, capacities, indexed_by::arc);
    }

    /// @brief The supply of each node, in units of flow, in the problem the network holds, owned, node v at position v.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded the vector is empty.
    /// @return The entries, or an error of kind refused_memory or internal_error.
    /// @note Allocates the entries, one per node. Where exceptions are disabled and the memory for the vector is
    /// refused, the standard library ends the program as it does without exceptions.
    /// @see supplies
    KAHLNWS_HPP_NODISCARD result<std::vector<std::int64_t> > try_supplies() const noexcept {
        return owned_column(kahlnws_network_supplies, indexed_by::node);
    }

    /// @brief Writes the supply of each node, in units of flow, in the problem the network holds, to @p supplies, node
    /// v at position v.
    ///
    /// It reads the problem as it stands, after the load and every change since: it changes nothing, keeps an optimum
    /// the network holds and needs none. On a network never loaded nothing is written.
    /// @param[out] supplies One entry per node; what it held before does not matter.
    /// @return A success, or an error of kind invalid_argument or internal_error.
    /// @pre @p supplies holds one entry per node; answered as an error of kind invalid_argument.
    /// @post On an error, @p supplies is as it was.
    /// @note Allocates nothing.
    /// @see supplies_into
    KAHLNWS_HPP_NODISCARD result<void> try_supplies_into(span<std::int64_t> supplies) const noexcept {
        return read_column(kahlnws_network_supplies, supplies, indexed_by::node);
    }

#if KAHLNWS_EXCEPTIONS
    /// @brief Makes room as try_reserve() does, and throws where it fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param node_count The most nodes.
    /// @param arc_count The most arcs.
    /// @throws std::bad_alloc Where the memory is refused; the room held before is kept.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_reserve
    void reserve(std::size_t node_count, std::size_t arc_count) { try_reserve(node_count, arc_count).value(); }

    /// @brief Holds @p problem as try_load() does, and throws where it fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param problem The problem.
    /// @throws kahlnws::problem_error Where the problem lies outside what a solve admits.
    /// @throws std::bad_alloc Where memory is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_load
    void load(const problem& problem) { try_load(problem).value(); }

    /// @brief Sets the cost of @p arc as try_set_cost() does, and throws where it fails; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @param arc The arc, below arc_count().
    /// @param cost Its new cost, in units of cost.
    /// @return The cost it had.
    /// @throws std::invalid_argument Where @p arc is not below arc_count().
    /// @throws kahlnws::change_error Where the change lies outside what the network admits.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_set_cost
    std::int64_t set_cost(std::size_t arc, std::int64_t cost) { return try_set_cost(arc, cost).value(); }

    /// @brief Sets the capacity of @p arc as try_set_capacity() does, and throws where it fails; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @param arc The arc, below arc_count().
    /// @param capacity Its new capacity, in units of flow.
    /// @return The capacity it had.
    /// @throws std::invalid_argument Where @p arc is not below arc_count().
    /// @throws kahlnws::change_error Where the change lies outside what the network admits.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_set_capacity
    std::uint64_t set_capacity(std::size_t arc, std::uint64_t capacity) {
        return try_set_capacity(arc, capacity).value();
    }

    /// @brief Sets the lower bound of @p arc as try_set_lower_bound() does, and throws where it fails; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @param arc The arc, below arc_count().
    /// @param lower_bound Its new lower bound, in units of flow.
    /// @return The lower bound it had.
    /// @throws std::invalid_argument Where @p arc is not below arc_count().
    /// @throws kahlnws::change_error Where the change lies outside what the network admits.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_set_lower_bound
    std::uint64_t set_lower_bound(std::size_t arc, std::uint64_t lower_bound) {
        return try_set_lower_bound(arc, lower_bound).value();
    }

    /// @brief Sets both bounds of @p arc as try_set_bounds() does, and throws where it fails; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @param arc The arc, below arc_count().
    /// @param arc_bounds Its new lower bound and capacity, in units of flow.
    /// @return The bounds it had.
    /// @throws std::invalid_argument Where @p arc is not below arc_count().
    /// @throws kahlnws::change_error Where the change lies outside what the network admits.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_set_bounds
    kahlnws::bounds set_bounds(std::size_t arc, const kahlnws::bounds& arc_bounds) {
        return try_set_bounds(arc, arc_bounds).value();
    }

    /// @brief Sets the supply of @p node, balanced by @p balancing_node, as try_set_supply() does, and throws where it
    /// fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param node The node whose supply is set, below node_count().
    /// @param supply Its new supply, in units of flow: positive where it sends, negative where it takes.
    /// @param balancing_node The node whose supply moves the other way, below node_count().
    /// @return The supply @p node had.
    /// @throws std::invalid_argument Where a node is not below node_count(), or @p node is @p balancing_node and
    /// @p supply is not the supply it has.
    /// @throws kahlnws::change_error Where the change lies outside what the network admits.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_set_supply
    std::int64_t set_supply(std::size_t node, std::int64_t supply, std::size_t balancing_node) {
        return try_set_supply(node, supply, balancing_node).value();
    }

    /// @brief Moves supply as try_move_supply() does, and throws where it fails; only where @c KAHLNWS_EXCEPTIONS is
    /// 1.
    /// @param from_node The node whose supply falls, below node_count().
    /// @param to_node The node whose supply rises, below node_count().
    /// @param amount The units of flow moved.
    /// @throws std::invalid_argument Where a node is not below node_count().
    /// @throws kahlnws::change_error Where the change lies outside what the network admits.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_move_supply
    void move_supply(std::size_t from_node, std::size_t to_node, std::uint64_t amount) {
        try_move_supply(from_node, to_node, amount).value();
    }

    /// @brief Solves the problem held as try_solve() does, and throws where it fails; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @return The answer.
    /// @throws std::bad_alloc Where the memory for a new network's handle is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_solve
    answer<optimum> solve() { return try_solve().value(); }

    /// @brief The cost of the optimum held, as try_total_cost() answers it; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @return The cost.
    /// @throws std::logic_error Where the network holds no optimum.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_total_cost
    KAHLNWS_HPP_NODISCARD int128 total_cost() const { return try_total_cost().value(); }

    /// @brief The flow of @p arc in the optimum held, as try_flow() answers it; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param arc The arc, below arc_count().
    /// @return The flow.
    /// @throws std::invalid_argument Where @p arc is not below arc_count().
    /// @throws std::logic_error Where the network holds no optimum.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_flow
    KAHLNWS_HPP_NODISCARD std::uint64_t flow(std::size_t arc) const { return try_flow(arc).value(); }

    /// @brief The flows of the optimum held, owned, as try_flows() answers them; only where @c KAHLNWS_EXCEPTIONS is
    /// 1.
    /// @return The flows.
    /// @throws std::bad_alloc Where the memory for the flows is refused.
    /// @throws std::logic_error Where the network holds no optimum.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_flows
    KAHLNWS_HPP_NODISCARD std::vector<std::uint64_t> flows() const { return try_flows().value(); }

    /// @brief Writes the flows of the optimum held to @p flows as try_flows_into() does, and throws where it fails;
    /// only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param[out] flows One entry per arc.
    /// @throws std::invalid_argument Where @p flows does not hold one entry per arc.
    /// @throws std::logic_error Where the network holds no optimum.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_flows_into
    void flows_into(span<std::uint64_t> flows) const { try_flows_into(flows).value(); }

    /// @brief The potentials of the optimum held, owned, as try_potentials() answers them; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @return The potentials.
    /// @throws std::bad_alloc Where the memory for the potentials is refused.
    /// @throws std::logic_error Where the network holds no optimum.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_potentials
    KAHLNWS_HPP_NODISCARD std::vector<std::int64_t> potentials() const { return try_potentials().value(); }

    /// @brief Writes the potentials of the optimum held to @p potentials as try_potentials_into() does, and throws
    /// where it fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param[out] potentials One entry per node.
    /// @throws std::invalid_argument Where @p potentials does not hold one entry per node.
    /// @throws std::logic_error Where the network holds no optimum.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_potentials_into
    void potentials_into(span<std::int64_t> potentials) const { try_potentials_into(potentials).value(); }

    /// @brief The node @p arc leaves, as try_tail() answers it; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @param arc The arc, below arc_count().
    /// @return The tail.
    /// @throws std::invalid_argument Where @p arc is not below arc_count().
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_tail
    KAHLNWS_HPP_NODISCARD std::size_t tail(std::size_t arc) const { return try_tail(arc).value(); }

    /// @brief The node @p arc enters, as try_head() answers it; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @param arc The arc, below arc_count().
    /// @return The head.
    /// @throws std::invalid_argument Where @p arc is not below arc_count().
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_head
    KAHLNWS_HPP_NODISCARD std::size_t head(std::size_t arc) const { return try_head(arc).value(); }

    /// @brief The cost of a unit of flow on @p arc, in units of cost, as try_cost() answers it; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @param arc The arc, below arc_count().
    /// @return The cost.
    /// @throws std::invalid_argument Where @p arc is not below arc_count().
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_cost
    KAHLNWS_HPP_NODISCARD std::int64_t cost(std::size_t arc) const { return try_cost(arc).value(); }

    /// @brief The least flow of @p arc, in units of flow, as try_lower_bound() answers it; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @param arc The arc, below arc_count().
    /// @return The lower bound.
    /// @throws std::invalid_argument Where @p arc is not below arc_count().
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_lower_bound
    KAHLNWS_HPP_NODISCARD std::uint64_t lower_bound(std::size_t arc) const { return try_lower_bound(arc).value(); }

    /// @brief The most flow of @p arc, in units of flow, as try_capacity() answers it; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @param arc The arc, below arc_count().
    /// @return The capacity.
    /// @throws std::invalid_argument Where @p arc is not below arc_count().
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_capacity
    KAHLNWS_HPP_NODISCARD std::uint64_t capacity(std::size_t arc) const { return try_capacity(arc).value(); }

    /// @brief The bounds of @p arc, as try_bounds() answers them; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param arc The arc, below arc_count().
    /// @return The bounds.
    /// @throws std::invalid_argument Where @p arc is not below arc_count().
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_bounds
    KAHLNWS_HPP_NODISCARD kahlnws::bounds bounds(std::size_t arc) const { return try_bounds(arc).value(); }

    /// @brief The supply of @p node, as try_supply() answers it; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param node The node, below node_count().
    /// @return The supply.
    /// @throws std::invalid_argument Where @p node is not below node_count().
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_supply
    KAHLNWS_HPP_NODISCARD std::int64_t supply(std::size_t node) const { return try_supply(node).value(); }

    /// @brief The node each arc leaves, owned, as try_tails() answers them; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @return The entries.
    /// @throws std::bad_alloc Where the memory for the entries is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_tails
    KAHLNWS_HPP_NODISCARD std::vector<std::size_t> tails() const { return try_tails().value(); }

    /// @brief Writes the node each arc leaves to @p tails as try_tails_into() does, and throws where it fails; only
    /// where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param[out] tails One entry per arc.
    /// @throws std::invalid_argument Where @p tails does not hold one entry per arc.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_tails_into
    void tails_into(span<std::size_t> tails) const { try_tails_into(tails).value(); }

    /// @brief The node each arc enters, owned, as try_heads() answers them; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @return The entries.
    /// @throws std::bad_alloc Where the memory for the entries is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_heads
    KAHLNWS_HPP_NODISCARD std::vector<std::size_t> heads() const { return try_heads().value(); }

    /// @brief Writes the node each arc enters to @p heads as try_heads_into() does, and throws where it fails; only
    /// where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param[out] heads One entry per arc.
    /// @throws std::invalid_argument Where @p heads does not hold one entry per arc.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_heads_into
    void heads_into(span<std::size_t> heads) const { try_heads_into(heads).value(); }

    /// @brief The cost of a unit of flow on each arc, in units of cost, owned, as try_costs() answers them; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @return The entries.
    /// @throws std::bad_alloc Where the memory for the entries is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_costs
    KAHLNWS_HPP_NODISCARD std::vector<std::int64_t> costs() const { return try_costs().value(); }

    /// @brief Writes the cost of a unit of flow on each arc, in units of cost to @p costs as try_costs_into() does, and
    /// throws where it fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param[out] costs One entry per arc.
    /// @throws std::invalid_argument Where @p costs does not hold one entry per arc.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_costs_into
    void costs_into(span<std::int64_t> costs) const { try_costs_into(costs).value(); }

    /// @brief The lower bound of each arc, in units of flow, owned, as try_lower_bounds() answers them; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @return The entries.
    /// @throws std::bad_alloc Where the memory for the entries is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_lower_bounds
    KAHLNWS_HPP_NODISCARD std::vector<std::uint64_t> lower_bounds() const { return try_lower_bounds().value(); }

    /// @brief Writes the lower bound of each arc, in units of flow to @p lower_bounds as try_lower_bounds_into() does,
    /// and throws where it fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param[out] lower_bounds One entry per arc.
    /// @throws std::invalid_argument Where @p lower_bounds does not hold one entry per arc.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_lower_bounds_into
    void lower_bounds_into(span<std::uint64_t> lower_bounds) const { try_lower_bounds_into(lower_bounds).value(); }

    /// @brief The capacity of each arc, in units of flow, owned, as try_capacities() answers them; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @return The entries.
    /// @throws std::bad_alloc Where the memory for the entries is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_capacities
    KAHLNWS_HPP_NODISCARD std::vector<std::uint64_t> capacities() const { return try_capacities().value(); }

    /// @brief Writes the capacity of each arc, in units of flow to @p capacities as try_capacities_into() does, and
    /// throws where it fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param[out] capacities One entry per arc.
    /// @throws std::invalid_argument Where @p capacities does not hold one entry per arc.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_capacities_into
    void capacities_into(span<std::uint64_t> capacities) const { try_capacities_into(capacities).value(); }

    /// @brief The supply of each node, in units of flow, owned, as try_supplies() answers them; only where
    /// @c KAHLNWS_EXCEPTIONS is 1.
    /// @return The entries.
    /// @throws std::bad_alloc Where the memory for the entries is refused.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_supplies
    KAHLNWS_HPP_NODISCARD std::vector<std::int64_t> supplies() const { return try_supplies().value(); }

    /// @brief Writes the supply of each node, in units of flow to @p supplies as try_supplies_into() does, and throws
    /// where it fails; only where @c KAHLNWS_EXCEPTIONS is 1.
    /// @param[out] supplies One entry per node.
    /// @throws std::invalid_argument Where @p supplies does not hold one entry per node.
    /// @throws kahlnws::internal_error Where the library fails.
    /// @see try_supplies_into
    void supplies_into(span<std::int64_t> supplies) const { try_supplies_into(supplies).value(); }
#endif

private:
    /// A network that owns `held`, a handle of the library.
    explicit network(kahlnws_network* held) noexcept : handle_(held) {}

    /// The handle, made on first need; null where the library could not make one, which it answers only where the
    /// memory for it is refused.
    kahlnws_network* handle() noexcept {
        if (handle_ == nullptr) {
            handle_ = kahlnws_network_new();
        }
        return handle_;
    }

    /// The error of `arc` not below the arc count.
    error arc_outside(std::size_t arc) const noexcept {
        return error(error_kind::invalid_argument, "arc ", std::uint64_t(arc), " is not below the arc count ",
                     std::uint64_t(arc_count()));
    }

    /// The error of `node` not below the node count.
    error node_outside(std::size_t node) const noexcept {
        return error(error_kind::invalid_argument, "node ", std::uint64_t(node), " is not below the node count ",
                     std::uint64_t(node_count()));
    }

    /// The error of a supply set for `node` that the library answered as an invalid argument, or as a null pointer on
    /// a network that holds no handle, which has no node: `node` or `balancing_node` not below the node count, the
    /// first such named, or else `node` given as its own balancing node with another `supply` than the one it has.
    error supply_not_set(std::size_t node, std::int64_t supply, std::size_t balancing_node) const noexcept {
        if (node >= node_count() || balancing_node >= node_count()) {
            return node_outside(node < node_count() ? balancing_node : node);
        }
        std::int64_t held = 0;
        const kahlnws_status status = kahlnws_network_supply(handle_, node, &held);
        if (status != KAHLNWS_STATUS_OK) {
            return error::of_status(status);
        }
        return error(error_kind::invalid_argument, "node ", std::uint64_t(node),
                     " is its own balancing node, whose supply ", held, " should then stay, not become ", supply);
    }

    /// The answer of the change of `arc` that the library answered with `status`, writing `written`: as changed()
    /// says, but for an arc not below the arc count, which the library answers as an invalid argument, and as a null
    /// pointer on a network that holds no handle, which has no arc.
    result<void> changed_arc(kahlnws_status status, const kahlnws_refusal& written, std::size_t arc) const noexcept {
        if (status == KAHLNWS_STATUS_INVALID_ARGUMENT || status == KAHLNWS_STATUS_NULL_POINTER) {
            return arc_outside(arc);
        }
        return changed(status, written);
    }

    /// The answer of a change the library answered with `status`, writing `written`: a success where it changed the
    /// network or found nothing to change, the error of the refused change, or the error of any other status.
    static result<void> changed(kahlnws_status status, const kahlnws_refusal& written) noexcept {
        switch (status) {
        case KAHLNWS_STATUS_OK:
            return result<void>();
        case KAHLNWS_STATUS_REFUSED:
            return error::of_change_refusal(written);
        }
        return error::of_status(status);
    }

    /// The answer of a read of the optimum the library answered with `status`: a success where it wrote what was read,
    /// and where the network holds no handle, which the library answers as a null pointer: a network never loaded,
    /// whose optimum is no flow at the cost zero, with nothing to write; the error of no optimum, or of any other
    /// status.
    static result<void> read_of(kahlnws_status status) noexcept {
        switch (status) {
        case KAHLNWS_STATUS_OK:
        case KAHLNWS_STATUS_NULL_POINTER:
            return result<void>();
        case KAHLNWS_STATUS_NO_OPTIMUM:
            return error::no_optimum();
        }
        return error::of_status(status);
    }

    /// What a column of the problem held is indexed by: one entry per arc, or one per node.
    enum class indexed_by { arc, node };

    /// The entries of a column indexed by `entry`: the arc count or the node count.
    std::size_t count_of(indexed_by entry) const noexcept {
        return entry == indexed_by::node ? node_count() : arc_count();
    }

    /// The answer of a read of the problem the library answered with `status`: a success where it wrote what was
    /// read, and where the network holds no handle, which the library answers as a null pointer: a network never
    /// loaded, whose columns hold nothing to write; the error of any other status.
    static result<void> problem_read(kahlnws_status status) noexcept {
        switch (status) {
        case KAHLNWS_STATUS_OK:
        case KAHLNWS_STATUS_NULL_POINTER:
            return result<void>();
        }
        return error::of_status(status);
    }

    /// The answer of a read of `arc` that the library answered with `status`: as problem_read() says, but for an arc
    /// not below the arc count, which the library answers as an invalid argument, and as a null pointer on a network
    /// that holds no handle, which has no arc.
    result<void> arc_read(kahlnws_status status, std::size_t arc) const noexcept {
        if (status == KAHLNWS_STATUS_INVALID_ARGUMENT || status == KAHLNWS_STATUS_NULL_POINTER) {
            return arc_outside(arc);
        }
        return problem_read(status);
    }

    /// The number of `arc` the C read `read` writes: the body of the reads of one arc's number.
    template <class Number>
    result<Number> read_arc(kahlnws_status (*read)(const kahlnws_network*, std::size_t, Number*),
                            std::size_t arc) const noexcept {
        Number number = Number();
        const result<void> answered = arc_read(read(handle_, arc, &number), arc);
        if (!answered) {
            return answered.error();
        }
        return number;
    }

    /// Writes a column of one entry per arc or node, `entry` says which, into `entries` by the C read `read`, where
    /// `entries` holds as many: the body of the _into forms of the problem's columns.
    template <class Entry>
    result<void> read_column(kahlnws_status (*read)(const kahlnws_network*, Entry*),
                             span<Entry> entries,
                             indexed_by entry) const noexcept {
        const std::size_t count = count_of(entry);
        if (entries.size() != count) {
            return error(error_kind::invalid_argument, std::uint64_t(entries.size()), " entries for a network of ",
                         std::uint64_t(count), entry == indexed_by::node ? " nodes" : " arcs");
        }
        return problem_read(read(handle_, entries.data()));
    }

    /// A column of one entry per arc or node, `entry` says which, owned, read by the C read `read`: the body of the
    /// owned forms of the problem's columns.
    template <class Entry>
    result<std::vector<Entry> > owned_column(kahlnws_status (*read)(const kahlnws_network*, Entry*),
                                             indexed_by entry) const noexcept {
        std::vector<Entry> entries;
        if (!detail::resized(entries, count_of(entry))) {
            return error::refused_memory();
        }
        const result<void> answered = read_column(read, span<Entry>(entries), entry);
        if (!answered) {
            return answered.error();
        }
        return result<std::vector<Entry> >(std::move(entries));
    }

    /// The library's network; null where none was made yet, which holds the problem of no node.
    kahlnws_network* handle_;
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
