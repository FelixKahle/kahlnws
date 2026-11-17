// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! What an entry answers: the status of the call, by a number fixed for good, and its name.

use core::ffi::CStr;

/// How a call went, by a number fixed for good: one of the `KAHLNWS_STATUS_` constants.
///
/// A solve answers `KAHLNWS_STATUS_OPTIMAL` or `KAHLNWS_STATUS_INFEASIBLE`, and every other call that did what it was
/// asked `KAHLNWS_STATUS_OK`; every other status says what stopped the call. Each constant names the calls that answer
/// it, and each entry lists what it can answer. An arc or a node of a network not below its count, and a node given as
/// its own balancing node with a supply other than the one it has, are `KAHLNWS_STATUS_INVALID_ARGUMENT`, as a pointer
/// no call can use is: a call written wrongly.
///
/// The numbers are part of the binary interface: none is ever changed or given to another status. Plain data: a
/// caller may hold any number, and `kahlnws_status_name` names every one.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StatusCode(pub u32);

impl StatusCode {
    /// `kahlnws_solver_solve` or `kahlnws_network_solve` found a flow of least cost and wrote what was asked of it.
    pub const OPTIMAL: Self = Self(0);
    /// `kahlnws_solver_solve` or `kahlnws_network_solve` found that no flow within the bounds meets the supplies, and
    /// wrote nothing but the work of the solve.
    pub const INFEASIBLE: Self = Self(1);
    /// Any call that is not a solve did what it was asked: `kahlnws_solver_reserve` and `kahlnws_network_reserve` made
    /// the room, `kahlnws_network_load` holds the problem, a change of a network was made or set the number already
    /// held, `kahlnws_network_copy` made the copy, and a read of a network wrote what it reads.
    pub const OK: Self = Self(2);
    /// `kahlnws_solver_solve` or `kahlnws_network_load` refused the problem, which lies outside what a solve admits, or
    /// a change of a network refused its number, which lies outside what the network admits; nothing was written but
    /// the refusal that names the breach.
    pub const REFUSED: Self = Self(3);
    /// `kahlnws_network_total_cost`, `kahlnws_network_flow`, `kahlnws_network_flows` or `kahlnws_network_potentials`
    /// found no optimum: the network was loaded or changed since its last solve, or that solve found no flow. Nothing
    /// was written.
    pub const NO_OPTIMUM: Self = Self(4);
    /// Any call: a pointer it needs is null, and nothing was written.
    pub const NULL_POINTER: Self = Self(5);
    /// Any call: an argument no call can use, a pointer not aligned for its type, a count of entries that no object
    /// can hold, its bytes past `PTRDIFF_MAX`, an arc or a node of a network not below its count, or a node given to
    /// `kahlnws_network_set_supply` as its own balancing node with a supply other than the one it has. Nothing was
    /// written.
    pub const INVALID_ARGUMENT: Self = Self(6);
    /// `kahlnws_solver_reserve`, `kahlnws_solver_solve`, `kahlnws_network_reserve`, `kahlnws_network_load` or
    /// `kahlnws_network_copy`: the memory for the room of the solver or of the network was refused, and nothing was
    /// written.
    pub const NO_ROOM: Self = Self(7);
    /// Any call: an internal error of the library was caught, a defect of it, and answered here. What the call's
    /// outputs hold is then unspecified; the solver or the network may be used again or freed, as its entries say.
    pub const INTERNAL_ERROR: Self = Self(8);
}

/// How a call went, as the crate decides it; C reads it as its [`StatusCode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Status {
    /// A flow of least cost was found and written.
    Optimal,
    /// No flow within the bounds meets the supplies.
    Infeasible,
    /// A call that is not a solve did what it was asked.
    Ok,
    /// The problem lies outside what a solve admits, or a change outside what a network admits.
    Refused,
    /// The network holds no optimum to read.
    NoOptimum,
    /// A pointer the call needs is null.
    NullPointer,
    /// A pointer is not aligned for its type, a count of entries passes `isize::MAX` bytes, an arc or a node is not
    /// below its count, or a node that balances itself is to change its supply.
    InvalidArgument,
    /// The memory for the room was refused.
    NoRoom,
    /// A panic was caught.
    InternalError,
}

impl Status {
    /// Every status, each once.
    const ALL: [Self; 9] = [
        Self::Optimal,
        Self::Infeasible,
        Self::Ok,
        Self::Refused,
        Self::NoOptimum,
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
            Self::Ok => (StatusCode::OK, c"ok"),
            Self::Refused => (StatusCode::REFUSED, c"refused"),
            Self::NoOptimum => (StatusCode::NO_OPTIMUM, c"no optimum"),
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
    const STATUSES: [(Status, u32, &str); 9] = [
        (Status::Optimal, 0, "optimal"),
        (Status::Infeasible, 1, "infeasible"),
        (Status::Ok, 2, "ok"),
        (Status::Refused, 3, "refused"),
        (Status::NoOptimum, 4, "no optimum"),
        (Status::NullPointer, 5, "null pointer"),
        (Status::InvalidArgument, 6, "invalid argument"),
        (Status::NoRoom, 7, "no room"),
        (Status::InternalError, 8, "internal error"),
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
        for number in 0..9 {
            let status = Status::of_code(StatusCode(number)).expect("every number below 9 should name a status");
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
        for number in [9, 10, 13, 1000, u32::MAX] {
            assert_eq!(Status::of_code(StatusCode(number)), None, "{number}");
        }
    }

    #[test]
    fn test_a_pointer_fault_is_answered_as_its_status() {
        assert_eq!(Status::from(PointerFault::Null), Status::NullPointer);
        assert_eq!(Status::from(PointerFault::Invalid), Status::InvalidArgument);
    }
}
