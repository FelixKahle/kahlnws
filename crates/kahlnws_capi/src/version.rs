// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The version of the library, from the version of the crate, and its encoding in one number.

/// The major version: what changes where the interface breaks.
pub(crate) const MAJOR: u32 = version_part(env!("CARGO_PKG_VERSION_MAJOR"));
/// The minor version.
pub(crate) const MINOR: u32 = version_part(env!("CARGO_PKG_VERSION_MINOR"));
/// The patch version.
pub(crate) const PATCH: u32 = version_part(env!("CARGO_PKG_VERSION_PATCH"));

/// The version in one number: `(major << 16) | (minor << 8) | patch`.
pub(crate) const VERSION: u32 = (MAJOR << 16) | (MINOR << 8) | PATCH;

// The encoding keeps each part apart only where the minor and the patch version fit 8 bits and the major one 16.
const _: () = assert!(MAJOR < 1 << 16 && MINOR < 1 << 8 && PATCH < 1 << 8, "a version part should fit its bits");

/// The number the decimal `digits` of a version part write.
///
/// # Panics
/// Panics, at compile time where evaluated in a constant, if `digits` is not a decimal number that fits a `u32`.
const fn version_part(digits: &str) -> u32 {
    match u32::from_str_radix(digits, 10) {
        Ok(part) => part,
        Err(_) => panic!("called `version_part` with a version part that is no decimal u32"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_version_is_the_crate_s_by_hand() {
        let parts = [MAJOR, MINOR, PATCH].map(|part| part.to_string()).join(".");

        assert_eq!(parts, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_the_encoding_keeps_each_part_apart() {
        assert_eq!((VERSION >> 16, (VERSION >> 8) & 0xFF, VERSION & 0xFF), (MAJOR, MINOR, PATCH));
    }

    #[test]
    fn test_version_part_reads_decimal_digits_by_hand() {
        assert_eq!((version_part("0"), version_part("17"), version_part("4294967295")), (0, 17, u32::MAX));
    }
}
