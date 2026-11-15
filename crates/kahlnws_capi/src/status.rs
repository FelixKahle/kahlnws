// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What an entry answers: the status of the call, by a number fixed for good, and its name.

use core::ffi::CStr;

/// How a call went, by a number fixed for good: one of the `KAHLNWS_STATUS_` constants.
///
/// `kahlnws_solver_reserve`, `kahlnws_solver_solve` and `kahlnws_solver_resolve` answer one; each says which it can
/// answer.
///
/// The numbers are part of the binary interface: none is ever changed or given to another status. Plain data: a
/// caller may hold any number, and `kahlnws_status_name` names every one.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StatusCode(pub u32);

impl StatusCode {
    /// A solve found a flow of least cost and wrote it.
    pub const OPTIMAL: Self = Self(0);
    /// A solve found that no flow within the bounds meets the supplies, and wrote nothing but the work of the solve.
    pub const INFEASIBLE: Self = Self(1);
    /// `kahlnws_solver_reserve` made the room it was asked for.
    pub const RESERVED: Self = Self(2);
    /// A solve refused the problem, which lies outside what a solve admits, and wrote nothing but the refusal that
    /// names the breach.
    pub const REFUSED: Self = Self(3);
    /// A pointer the call needs is null, and nothing was written.
    pub const NULL_POINTER: Self = Self(4);
    /// An argument no call can use: a pointer not aligned for its type, or a count of entries that no object can
    /// hold, its bytes past `PTRDIFF_MAX`. Nothing was written.
    pub const INVALID_ARGUMENT: Self = Self(5);
    /// The memory for the solver's room was refused, and nothing was written.
    pub const NO_ROOM: Self = Self(6);
    /// An internal error of the library was caught, a defect of it, and answered here. What the call's outputs hold
    /// is then unspecified; the solver may be used again or freed.
    pub const INTERNAL_ERROR: Self = Self(7);
}

/// How a call went, as the crate decides it; C reads it as its [`StatusCode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Status {
    /// A flow of least cost was found and written.
    Optimal,
    /// No flow within the bounds meets the supplies.
    Infeasible,
    /// The room asked for is made.
    Reserved,
    /// The problem lies outside what a solve admits.
    Refused,
    /// A pointer the call needs is null.
    NullPointer,
    /// A pointer is not aligned for its type, or a count of entries passes `isize::MAX` bytes.
    InvalidArgument,
    /// The memory for the solver's room was refused.
    NoRoom,
    /// A panic was caught.
    InternalError,
}

impl Status {
    /// Every status, each once.
    const ALL: [Self; 8] = [
        Self::Optimal,
        Self::Infeasible,
        Self::Reserved,
        Self::Refused,
        Self::NullPointer,
        Self::InvalidArgument,
        Self::NoRoom,
        Self::InternalError,
    ];

    /// The status's row of the binary interface: its number and its name for a person to read, lower case. The one
    /// table both directions of the conversion and the name read.
    fn row(self) -> (StatusCode, &'static CStr) {
        match self {
            Self::Optimal => (StatusCode::OPTIMAL, c"optimal"),
            Self::Infeasible => (StatusCode::INFEASIBLE, c"infeasible"),
            Self::Reserved => (StatusCode::RESERVED, c"reserved"),
            Self::Refused => (StatusCode::REFUSED, c"refused"),
            Self::NullPointer => (StatusCode::NULL_POINTER, c"null pointer"),
            Self::InvalidArgument => (StatusCode::INVALID_ARGUMENT, c"invalid argument"),
            Self::NoRoom => (StatusCode::NO_ROOM, c"no room"),
            Self::InternalError => (StatusCode::INTERNAL_ERROR, c"internal error"),
        }
    }

    /// The status whose number is `code`; `None` where no status has it.
    pub(crate) fn of_code(code: StatusCode) -> Option<Self> {
        Self::ALL.into_iter().find(|status| status.row().0 == code)
    }

    /// The status's name for a person to read, lower case.
    pub(crate) fn name(self) -> &'static CStr {
        self.row().1
    }
}

impl From<Status> for StatusCode {
    fn from(status: Status) -> Self {
        status.row().0
    }
}

/// Why a pointer from C cannot be read: answered to C as [`StatusCode::NULL_POINTER`] or
/// [`StatusCode::INVALID_ARGUMENT`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PointerFault {
    /// The pointer is null where the call needs it.
    Null,
    /// The pointer is not aligned for its type, or its count of entries passes `isize::MAX` bytes.
    Invalid,
}

impl From<PointerFault> for Status {
    fn from(fault: PointerFault) -> Self {
        match fault {
            PointerFault::Null => Self::NullPointer,
            PointerFault::Invalid => Self::InvalidArgument,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every status with its number and name, written out by hand.
    const STATUSES: [(Status, u32, &str); 8] = [
        (Status::Optimal, 0, "optimal"),
        (Status::Infeasible, 1, "infeasible"),
        (Status::Reserved, 2, "reserved"),
        (Status::Refused, 3, "refused"),
        (Status::NullPointer, 4, "null pointer"),
        (Status::InvalidArgument, 5, "invalid argument"),
        (Status::NoRoom, 6, "no room"),
        (Status::InternalError, 7, "internal error"),
    ];

    #[test]
    fn test_the_numbers_and_names_of_the_binary_interface_by_hand() {
        for (status, number, name) in STATUSES {
            assert_eq!(StatusCode::from(status), StatusCode(number), "{status:?}");
            assert_eq!(status.name().to_str(), Ok(name), "{status:?}");
        }
    }

    #[test]
    fn test_every_status_round_trips_through_its_number() {
        for status in Status::ALL {
            assert_eq!(Status::of_code(StatusCode::from(status)), Some(status), "{status:?}");
        }
        for number in 0..8 {
            let status = Status::of_code(StatusCode(number)).expect("every number below 8 should name a status");
            assert_eq!(StatusCode::from(status), StatusCode(number));
        }
    }

    #[test]
    fn test_the_names_are_distinct() {
        let names: Vec<&CStr> = Status::ALL.into_iter().map(Status::name).collect();

        for (at, name) in names.iter().enumerate() {
            assert!(!names[at + 1..].contains(name), "{name:?} names two statuses");
        }
    }

    #[test]
    fn test_a_number_no_status_has_names_none() {
        for number in [8, 9, 1000, u32::MAX] {
            assert_eq!(Status::of_code(StatusCode(number)), None, "{number}");
        }
    }

    #[test]
    fn test_a_pointer_fault_is_answered_as_its_status() {
        assert_eq!(Status::from(PointerFault::Null), Status::NullPointer);
        assert_eq!(Status::from(PointerFault::Invalid), Status::InvalidArgument);
    }
}
