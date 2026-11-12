// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The text steps of the build script, run on hand-written input: the file `build/header_text.rs` holds them and
//! their tests, and the build script includes it as this target does.

#[path = "../build/header_text.rs"]
mod header_text;
