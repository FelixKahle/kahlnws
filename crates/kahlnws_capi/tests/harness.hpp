// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// The checks the C++ test programs share: each failed check is named on standard error, and the program exits with
// 0 only where every check held.

#ifndef KAHLNWS_TESTS_HARNESS_HPP
#define KAHLNWS_TESTS_HARNESS_HPP

#include <cstdio>

/// The checks of one program, counting those that failed.
class checks {
public:
    /// The checks of the program `program`, named in every failure.
    explicit checks(const char* program) : program_(program), failures_(0) {}

    /// Names `what` where `holds` is false.
    void check(bool holds, const char* what) {
        if (!holds) {
            std::fprintf(stderr, "%s: %s\n", program_, what);
            ++failures_;
        }
    }

    /// The exit code: 0 where every check held, 1 otherwise.
    int exit_code() const { return failures_ == 0 ? 0 : 1; }

private:
    const char* program_;
    int failures_;
};

#endif  // KAHLNWS_TESTS_HARNESS_HPP
