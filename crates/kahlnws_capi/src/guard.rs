// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The guard every entry runs its body under: a panic is caught and answered, never unwound into C.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::status::{PointerFault, Status, StatusCode};

/// What `body` answers, or `on_panic` where it panics: the panic stops here.
///
/// The panic's message is still reported to standard error, as every panic's is.
pub(crate) fn guarded<T>(on_panic: T, body: impl FnOnce() -> T) -> T {
    // Asserted unwind-safe: a body that panicked has left at most a solver whose room is in some state, and the
    // library keeps nothing in that room from one solve to the next; every other value the body touched dies with it.
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(on_panic)
}

/// The number of the status `body` answers, a pointer it could not read answered as that fault's status, and a panic
/// as [`StatusCode::INTERNAL_ERROR`].
pub(crate) fn status_of(body: impl FnOnce() -> Result<Status, PointerFault>) -> StatusCode {
    let status = guarded(Status::InternalError, || body().unwrap_or_else(Status::from));

    StatusCode::from(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guarded_answers_the_body_or_on_panic_where_it_panics() {
        assert_eq!(guarded(0, || 7), 7);
        assert_eq!(guarded(0, || -> i32 { panic!("a panic the guard should catch") }), 0);
    }

    #[test]
    fn test_status_of_answers_the_body_a_pointer_fault_or_a_panic() {
        assert_eq!(status_of(|| Ok(Status::Infeasible)), StatusCode::INFEASIBLE);
        assert_eq!(status_of(|| Err(PointerFault::Null)), StatusCode::NULL_POINTER);
        assert_eq!(status_of(|| Err(PointerFault::Invalid)), StatusCode::INVALID_ARGUMENT);
        assert_eq!(status_of(|| panic!("a panic the guard should catch")), StatusCode::INTERNAL_ERROR);
    }
}
