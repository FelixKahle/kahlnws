// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Writes the C header of the crate from its sources and `cbindgen.toml` to the build's output folder, where a test
//! compares it with the committed `include/kahlnws.h`; and, where the environment variable `KAHLNWS_WRITE_HEADER` is
//! `1`, to `include/kahlnws.h` itself. Every build of the same sources writes the same bytes. It hands the target tuple
//! and the profile family of the build on to the crate's targets, as `KAHLNWS_CAPI_TARGET` (cargo's `TARGET`) and
//! `KAHLNWS_CAPI_PROFILE` (cargo's `PROFILE`, `debug` or `release`), for the header tests that build the static
//! library as this build made it.
//!
//! After the includes stand the overview, the crate doc up to its first heading; the preamble of macros; and the
//! version macros, from the crate's version encoded as the library encodes it. The numbered constants of the generated
//! declarations are then gathered into enums, and every doc made a Doxygen comment wrapped where it stands; each of
//! those steps fails the build where it meets a form it does not know.

#[path = "header_text.rs"]
mod header_text;
#[path = "../src/version.rs"]
mod version;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// The header's file name, the same in both places.
const HEADER: &str = "kahlnws.h";

/// The environment variable that asks the build to rewrite the committed header.
const WRITE_HEADER: &str = "KAHLNWS_WRITE_HEADER";

/// The value of [`WRITE_HEADER`] that rewrites the committed header.
const WRITE_HEADER_ON: &str = "1";

/// The variables cargo sets for the build script that it hands on to the crate's targets, each with the name the
/// targets read it by.
const HANDED_ON: [(&str, &str); 2] = [("TARGET", "KAHLNWS_CAPI_TARGET"), ("PROFILE", "KAHLNWS_CAPI_PROFILE")];

fn main() {
    let crate_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("cargo should set CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("cargo should set OUT_DIR"));

    for input in ["src", "build", "cbindgen.toml"] {
        println!("cargo::rerun-if-changed={input}");
    }
    println!("cargo::rerun-if-env-changed={WRITE_HEADER}");
    for (variable, handed_on) in HANDED_ON {
        let value = env::var(variable).unwrap_or_else(|_| panic!("cargo should set {variable}"));
        println!("cargo::rustc-env={handed_on}={value}");
    }

    let header = header_of(&crate_dir);
    write_if_changed(&out_dir.join(HEADER), &header);
    if env::var(WRITE_HEADER).is_ok_and(|value| value == WRITE_HEADER_ON) {
        write_if_changed(&crate_dir.join("include").join(HEADER), &header);
    }
}

/// The header the crate in `crate_dir` declares, for a C reader.
///
/// # Panics
/// Panics if a file of the crate cannot be read, the header cannot be written from the sources, or a step of
/// [`header_text`] finds the generated header in a shape it does not know; the build then fails with the message.
fn header_of(crate_dir: &Path) -> String {
    let config_path = crate_dir.join("cbindgen.toml");
    let mut config = cbindgen::Config::from_file(&config_path)
        .unwrap_or_else(|error| panic!("the header's settings should be read from {}: {error}", config_path.display()));
    let lib_path = crate_dir.join("src").join("lib.rs");
    let lib = fs::read_to_string(&lib_path)
        .unwrap_or_else(|error| panic!("the crate root should be read from {}: {error}", lib_path.display()));

    let version = header_text::version_macros(version::MAJOR, version::MINOR, version::PATCH, version::VERSION);
    let overview = header_text::overview_comment(&lib);
    config.after_includes = Some(format!("\n{overview}\n\n{}\n{version}", header_text::PREAMBLE));

    let bindings = cbindgen::Builder::new()
        .with_config(config)
        .with_src(&lib_path)
        .generate()
        .unwrap_or_else(|error| panic!("the header should be written from the crate's sources: {error}"));
    let mut written = Vec::new();
    bindings.write(&mut written);
    let header = String::from_utf8(written).expect("the header should be written as UTF-8");

    header_text::for_c_reader(&header)
}

/// Writes `contents` to the file at `path`, creating its folder, unless the file already holds exactly them.
///
/// # Panics
/// Panics if the folder cannot be made or the file cannot be written; the build then fails with the message.
fn write_if_changed(path: &Path, contents: &str) {
    if fs::read_to_string(path).is_ok_and(|held| held == contents) {
        return;
    }

    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)
            .unwrap_or_else(|error| panic!("the folder {} should be made: {error}", folder.display()));
    }
    fs::write(path, contents)
        .unwrap_or_else(|error| panic!("the header should be written to {}: {error}", path.display()));
}
