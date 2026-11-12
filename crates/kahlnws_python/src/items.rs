// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What the items of a buffer are, read from the format and the item size its export states.
//!
//! The check is the binding's own: the typed views of the buffer protocol's binding library read a byte order given
//! explicitly the wrong way round on a little-endian machine (they take `>` for native and refuse `<`), so they are
//! not asked. Here a format names the native byte order implicitly (no prefix, `@`) or explicitly (`=`, and `<` or
//! `>`/`!` on a machine of that order); either way it is read, and the other order is refused.

use core::ffi::{CStr, c_long};

/// The width of an integer item, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Width {
    /// One byte.
    One,
    /// Two bytes.
    Two,
    /// Four bytes.
    Four,
    /// Eight bytes.
    Eight,
}

/// What the items of a buffer are, as far as a column is read from them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Items {
    /// Integers in the machine's byte order, signed or not, of a width.
    Integers {
        /// Whether they are signed.
        signed: bool,
        /// Their width.
        width: Width,
    },
    /// Booleans, one byte each, read as 0 and 1 as a list of `bool` is.
    Booleans,
    /// References to Python objects, read through the sequence protocol as a list is.
    Objects,
    /// Integers in the other byte order than the machine's.
    ForeignOrder,
    /// Items of no bytes.
    Empty,
    /// Anything else: floats, characters, structures, or a format whose item size is not the size its export states.
    Other,
}

/// The byte order a format prefix names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Order {
    /// The machine's, with the sizes of its C types: no prefix, or `@`.
    NativeSizes,
    /// The machine's, with the standard sizes of the module `struct`: `=`.
    Native,
    /// Least significant byte first, standard sizes: `<`.
    Little,
    /// Most significant byte first, standard sizes: `>` or `!`.
    Big,
}

impl Width {
    /// The width of `bytes` bytes; `None` for a width no integer item has.
    pub(crate) const fn of(bytes: usize) -> Option<Self> {
        match bytes {
            1 => Some(Self::One),
            2 => Some(Self::Two),
            4 => Some(Self::Four),
            8 => Some(Self::Eight),
            _ => None,
        }
    }
}

impl Items {
    /// What the items of a buffer are whose export states `format`, in the notation of the module `struct`, and
    /// items of `item_size` bytes.
    pub(crate) fn of(format: &CStr, item_size: usize) -> Self {
        if item_size == 0 {
            return Self::Empty;
        }

        let (order, code) = match format.to_bytes() {
            [code] => (Order::NativeSizes, *code),
            [b'@', code] => (Order::NativeSizes, *code),
            [b'=', code] => (Order::Native, *code),
            [b'<', code] => (Order::Little, *code),
            [b'>' | b'!', code] => (Order::Big, *code),
            _ => return Self::Other,
        };

        let items = match (code, order) {
            (b'O', Order::NativeSizes) => return checked(Self::Objects, size_of::<usize>(), item_size),
            (b'?', _) => return checked(Self::Booleans, 1, item_size),
            (b'b' | b'B' | b'h' | b'H' | b'i' | b'I' | b'q' | b'Q', _) => standard_size(code),
            (b'l' | b'L', Order::NativeSizes) => size_of::<c_long>(),
            (b'l' | b'L', _) => 4,
            (b'n' | b'N', Order::NativeSizes) => size_of::<usize>(),
            _ => return Self::Other,
        };

        let Some(width) = Width::of(items).filter(|_| items == item_size) else {
            return Self::Other;
        };
        if !order.is_native() {
            return Self::ForeignOrder;
        }

        Self::Integers { signed: code.is_ascii_lowercase(), width }
    }
}

impl Order {
    /// Whether this is the machine's byte order.
    const fn is_native(self) -> bool {
        match self {
            Self::NativeSizes | Self::Native => true,
            Self::Little => cfg!(target_endian = "little"),
            Self::Big => cfg!(target_endian = "big"),
        }
    }
}

/// The size in bytes of an integer of type `code` at the standard sizes, which the native sizes share for these
/// codes on every platform the binding builds for.
const fn standard_size(code: u8) -> usize {
    match code {
        b'b' | b'B' => 1,
        b'h' | b'H' => 2,
        b'i' | b'I' => 4,
        _ => 8,
    }
}

/// `items`, where its export states the `expected` item size; [`Items::Other`] where it states another.
const fn checked(items: Items, expected: usize, item_size: usize) -> Items {
    if item_size == expected { items } else { Items::Other }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The integers of `bytes` bytes, signed or not.
    fn integers(signed: bool, bytes: usize) -> Items {
        Items::Integers { signed, width: Width::of(bytes).expect("the test's widths should be integer widths") }
    }

    #[test]
    fn test_integers_in_the_native_order_explicit_or_not_by_hand() {
        let cases = [
            (c"q", 8, integers(true, 8)),
            (c"@Q", 8, integers(false, 8)),
            (c"=q", 8, integers(true, 8)),
            (c"b", 1, integers(true, 1)),
            (c"B", 1, integers(false, 1)),
            (c"h", 2, integers(true, 2)),
            (c"=H", 2, integers(false, 2)),
            (c"i", 4, integers(true, 4)),
            (c"I", 4, integers(false, 4)),
            (c"l", size_of::<c_long>(), integers(true, size_of::<c_long>())),
            (c"=l", 4, integers(true, 4)),
            (c"N", size_of::<usize>(), integers(false, size_of::<usize>())),
            (c"n", size_of::<usize>(), integers(true, size_of::<usize>())),
        ];

        for (format, item_size, items) in cases {
            assert_eq!(Items::of(format, item_size), items, "{format:?}");
        }
    }

    #[cfg(target_endian = "little")]
    #[test]
    fn test_an_explicit_byte_order_is_read_where_it_is_the_machine_s_by_hand() {
        assert_eq!(Items::of(c"<Q", 8), integers(false, 8));
        assert_eq!(Items::of(c"<i", 4), integers(true, 4));
        assert_eq!(Items::of(c">q", 8), Items::ForeignOrder);
        assert_eq!(Items::of(c"!I", 4), Items::ForeignOrder);
    }

    #[test]
    fn test_booleans_objects_and_the_rest_by_hand() {
        let cases = [
            (c"?", 1, Items::Booleans),
            (c"<?", 1, Items::Booleans),
            (c"O", size_of::<usize>(), Items::Objects),
            (c"0x", 0, Items::Empty),
            (c"q", 0, Items::Empty),
            (c"d", 8, Items::Other),
            (c"f", 4, Items::Other),
            (c"e", 2, Items::Other),
            (c"c", 1, Items::Other),
            (c"2q", 16, Items::Other),
            (c"T{q:a:}", 8, Items::Other),
            (c"", 1, Items::Other),
            (c"=n", 8, Items::Other),
            (c"=O", 8, Items::Other),
            (c"q", 4, Items::Other),
            (c"?", 2, Items::Other),
        ];

        for (format, item_size, items) in cases {
            assert_eq!(Items::of(format, item_size), items, "{format:?} of {item_size} bytes");
        }
    }

    #[test]
    fn test_widths_by_hand() {
        assert_eq!(
            [1, 2, 4, 8].map(Width::of),
            [Some(Width::One), Some(Width::Two), Some(Width::Four), Some(Width::Eight)]
        );
        assert_eq!(Width::of(3), None);
        assert_eq!(Width::of(16), None);
    }
}
