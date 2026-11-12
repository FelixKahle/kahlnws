// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The headers C and C++ include.
//!
//! The C header: the committed `include/kahlnws.h` is the one the build writes; it is written for a C reader (ASCII
//! within 120 columns, no Markdown, no headings); Doxygen, where it is installed, builds its reference without a
//! warning, every parameter and every answer documented; its version macros are the library's version;
//! `tests/signatures.c`, which takes every entry at its exact type and every constant as a case label, compiles
//! against it as C99, C11, C17, C++11 and C++17; and the C program `tests/depot.c`, built against it and the static
//! library, solves its problems.
//!
//! The C++ header, the hand-written `include/kahlnws.hpp`: it is ASCII within 120 columns; it compiles alone as
//! C++11, 14, 17 and 20, and as C++23 where the compiler has it, with exceptions, with `-fno-exceptions` and with
//! `KAHLNWS_NO_EXCEPTIONS` defined, with every warning an error, those of conversions, shadowing and C-style casts
//! among them; an ignored result fails to compile beside the used form, which compiles; and the C++ programs
//! `tests/network_simplex.cpp` and `tests/int128.cpp`, built against it and the static library in every one of those
//! standards and modes, and once more in each mode with the address and undefined behaviour sanitizers where the
//! compiler has them, pass every check they make.
//!
//! The examples under `examples/`, each written in C and in C++: built against the headers and the static library with
//! every warning an error, the C ones as C99 and the C++ ones as C++11 in every mode, each prints the answers worked
//! out by hand, and each C++ example prints what its C twin prints, beside what only its throwing form prints.
//!
//! The compilers are the system's: `CC` and `CXX` where set, `cc` and `c++` otherwise. The libraries a static library
//! needs beside it are the ones `rustc --print native-static-libs` names for an empty static library of the `rustc`
//! on the path, which is the toolchain the build uses unless the caller chose another. Where a tool is not found, the
//! test that needs it says so on standard error and passes; it never passes without saying. The C and C++ programs run
//! as children of the test process, so whatever limits the test's process group runs under hold for them too. Every
//! file a test writes has a name of its own, since the tests of this file run at once.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

use kahlnws_capi::kahlnws_version;

/// The header the build wrote, from the crate's sources.
const BUILT: &str = include_str!(concat!(env!("OUT_DIR"), "/kahlnws.h"));

/// The folder of the crate.
const CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// The command that rewrites the committed header from the sources.
const REWRITE: &str = "KAHLNWS_WRITE_HEADER=1 cargo build -p kahlnws_capi";

/// Every language standard the header is compiled in: the compiler's language, its flag, and whether the header
/// promises it or it is compiled only where the compiler has it.
const STANDARDS: [(Language, &str, Promise); 7] = [
    (Language::C, "-std=c99", Promise::Promised),
    (Language::C, "-std=c11", Promise::Promised),
    (Language::C, "-std=c17", Promise::Promised),
    (Language::C, "-std=c23", Promise::WhereAvailable),
    (Language::Cpp, "-std=c++11", Promise::Promised),
    (Language::Cpp, "-std=c++17", Promise::Promised),
    (Language::Cpp, "-std=c++20", Promise::WhereAvailable),
];

/// Whether the header promises a language standard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Promise {
    /// The header promises it: the test fails where the compiler refuses the header in it.
    Promised,
    /// Compiled where the compiler has the standard, and skipped with a word otherwise.
    WhereAvailable,
}

/// The warnings every compilation of the test makes errors of.
const WARNINGS: [&str; 4] = ["-Wall", "-Wextra", "-Wpedantic", "-Werror"];

/// Every C++ standard the C++ header is compiled in, and whether it promises it.
const CPP_STANDARDS: [(&str, Promise); 5] = [
    ("-std=c++11", Promise::Promised),
    ("-std=c++14", Promise::Promised),
    ("-std=c++17", Promise::Promised),
    ("-std=c++20", Promise::Promised),
    ("-std=c++23", Promise::WhereAvailable),
];

/// The warnings every compilation against the C++ header makes errors of beside [`WARNINGS`]: no implicit conversion
/// that may change a value or its sign, no name that hides another, and no C-style cast.
const STRICT_WARNINGS: [&str; 4] = ["-Wconversion", "-Wsign-conversion", "-Wshadow", "-Wold-style-cast"];

/// The C++ programs under `tests/`, each built from `tests/{name}.cpp`.
const CPP_PROGRAMS: [&str; 2] = ["network_simplex", "int128"];

/// What the C++ header refuses to compile beside the form it compiles: the case's name, the body refused, the body
/// allowed, and a piece of the compiler's error that says why. Each body follows [`CPP_SNIPPET_PRELUDE`]. Every other
/// refusal of the header is shown by a static assertion in `tests/network_simplex.cpp`.
const FORBIDDEN: [(&str, &str, &str, &str); 1] = [(
    "ignored",
    "solver.try_solve(problem); return 0;",
    "return solver.try_solve(problem).has_value() ? 0 : 1;",
    "ignoring return value",
)];

/// The start of every C++ snippet [`FORBIDDEN`] compiles: a problem of one arc, viewed from vectors, and a solver; the
/// body and a closing brace follow.
const CPP_SNIPPET_PRELUDE: &str = "#include <cstddef>
#include <cstdint>
#include <vector>

#include \"kahlnws.hpp\"

int main() {
    std::vector<std::size_t> tails(1);
    std::vector<std::size_t> heads(1);
    std::vector<std::uint64_t> capacities(1);
    std::vector<std::int64_t> costs(1);
    std::vector<std::int64_t> supplies(1);
    kahlnws::problem problem;
    problem.tails = tails;
    problem.heads = heads;
    problem.capacities = capacities;
    problem.costs = costs;
    problem.supplies = supplies;
    kahlnws::network_simplex solver;
";

/// The modes the C++ header is compiled in: the flags that choose each, and the name a program of it carries.
const CPP_MODES: [(&[&str], &str); 3] =
    [(&[], "exceptions"), (&["-fno-exceptions"], "no-exceptions"), (&["-DKAHLNWS_NO_EXCEPTIONS"], "opted-out")];

/// How a C++ program is built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Build {
    /// With the compiler's defaults.
    Plain,
    /// With the address and the undefined behaviour sanitizers, which end the program at the first error they find.
    Sanitized,
}

impl Build {
    /// The compiler's flags of the build.
    fn flags(self) -> &'static [&'static str] {
        match self {
            Self::Plain => &[],
            Self::Sanitized => {
                &["-g", "-fsanitize=address,undefined", "-fno-sanitize-recover=all", "-fno-omit-frame-pointer"]
            }
        }
    }

    /// What the name of a program of the build ends in.
    fn suffix(self) -> &'static str {
        match self {
            Self::Plain => "",
            Self::Sanitized => "-sanitized",
        }
    }
}

/// A language the header is compiled as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Language {
    /// C, by the compiler `CC` names, or `cc`.
    C,
    /// C++, by the compiler `CXX` names, or `c++`.
    Cpp,
}

impl Language {
    /// The compiler of the language; `None` where it cannot be run.
    fn compiler(self) -> Option<OsString> {
        let (variable, fallback) = match self {
            Self::C => ("CC", "cc"),
            Self::Cpp => ("CXX", "c++"),
        };

        runnable(env::var_os(variable).unwrap_or_else(|| fallback.into()))
    }

    /// The language's name for the compiler's `-x`.
    fn name(self) -> &'static str {
        match self {
            Self::C => "c",
            Self::Cpp => "c++",
        }
    }

    /// The extension of a source file of the language.
    fn extension(self) -> &'static str {
        match self {
            Self::C => "c",
            Self::Cpp => "cpp",
        }
    }
}

/// `tool`, where it runs and answers `--version`; `None` otherwise.
fn runnable(tool: OsString) -> Option<OsString> {
    let found = Command::new(&tool).arg("--version").output().is_ok_and(|output| output.status.success());

    found.then_some(tool)
}

/// Says on standard error, past the test harness's capture, that the test `test` passes without running, and why.
fn skip(test: &str, reason: &str) {
    // The harness captures `eprintln!` of a passing test; a write to the stream itself is always shown.
    let _ = writeln!(io::stderr(), "{test}: skipped, {reason}");
}

/// Runs `command` and answers its output.
///
/// # Panics
/// Panics if the command cannot be started, or exits with a failure; the message holds what it wrote.
fn run(command: &mut Command) -> Output {
    let output = command.output().unwrap_or_else(|error| panic!("{command:?} should start: {error}"));
    assert!(
        output.status.success(),
        "{command:?} should succeed, not exit with {}:\n{}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    output
}

/// The crate's folder joined with `relative`.
fn in_crate(relative: &str) -> PathBuf {
    Path::new(CRATE_DIR).join(relative)
}

/// The libraries `rustc` names for linking a static library into a C program on this target, asked once for every
/// test of the file; `None` where `rustc` cannot be run.
///
/// # Panics
/// Panics if `rustc` runs but does not name them.
fn native_libraries() -> Option<&'static [String]> {
    static LIBRARIES: OnceLock<Option<Vec<String>>> = OnceLock::new();

    LIBRARIES.get_or_init(named_native_libraries).as_deref()
}

/// The libraries `rustc` names for linking a static library into a C program on this target; `None` where `rustc`
/// cannot be run.
///
/// # Panics
/// Panics if `rustc` runs but does not name them.
fn named_native_libraries() -> Option<Vec<String>> {
    let rustc = runnable("rustc".into())?;
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let source = folder.join("native_libraries.rs");
    fs::write(&source, "").expect("the empty crate should be written");

    let output = run(Command::new(rustc)
        .args(["--crate-type", "staticlib", "--crate-name", "native_libraries", "--print", "native-static-libs"])
        .arg("-o")
        .arg(folder.join("libnative_libraries.a"))
        .arg(&source));
    let notes = String::from_utf8_lossy(&output.stderr);
    let libraries = notes
        .lines()
        .find_map(|line| line.split_once("native-static-libs:"))
        .map(|(_, libraries)| libraries.split_whitespace().map(str::to_owned).collect())
        .expect("rustc should name the native libraries of a static library");

    Some(libraries)
}

#[test]
fn test_the_committed_header_is_the_one_the_build_writes() {
    let committed = fs::read_to_string(in_crate("include/kahlnws.h")).expect("the committed header should be read");

    assert!(committed == BUILT, "include/kahlnws.h differs from the header the build writes; `{REWRITE}` rewrites it");
}

#[test]
fn test_the_header_is_written_for_a_c_reader() {
    for (number, line) in BUILT.lines().enumerate() {
        assert!(line.is_ascii(), "line {} of the header is not ASCII: {line}", number + 1);
        assert!(!line.contains('`'), "line {} of the header holds Markdown: {line}", number + 1);
        assert!(!line.trim_start().starts_with("* # "), "line {} of the header holds a heading: {line}", number + 1);
        assert!(line.len() <= 120, "line {} of the header passes 120 columns: {line}", number + 1);
    }
}

#[test]
fn test_the_reference_of_the_c_header_builds_without_a_warning() {
    let test = "test_the_reference_of_the_c_header_builds_without_a_warning";
    let Some(doxygen) = runnable(OsString::from("doxygen")) else {
        return skip(test, "no doxygen was found");
    };

    // The crate's settings, which fail the run on a warning, with the output moved under the tests' own folder.
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("doxygen");
    fs::create_dir_all(&folder).expect("the folder of the reference should be made");
    let settings = fs::read_to_string(in_crate("Doxyfile")).expect("the settings of the reference should be read");
    let moved = folder.join("Doxyfile");
    fs::write(&moved, format!("{settings}\nOUTPUT_DIRECTORY = {}\n", folder.display()))
        .expect("the settings of the reference should be written");

    run(Command::new(doxygen).arg(&moved).current_dir(CRATE_DIR));

    let reference = fs::read_to_string(folder.join("html").join("kahlnws_8h.html"))
        .expect("the page of the header should be written");
    for entry in ["kahlnws_solver_new", "kahlnws_solver_solve", "kahlnws_refusal_message", "kahlnws_cost_bound"] {
        assert!(reference.contains(entry), "the reference of the header should describe {entry}");
    }
}

#[test]
fn test_the_header_s_version_is_the_library_s() {
    let value = |name: &str| -> u32 {
        let prefix = format!("#define KAHLNWS_VERSION{name} ");
        let line = BUILT.lines().find_map(|line| line.strip_prefix(&prefix));
        let parsed = line.and_then(|value| match value.strip_prefix("0x") {
            Some(hexadecimal) => u32::from_str_radix(hexadecimal.trim_end_matches('u'), 16).ok(),
            None => value.parse().ok(),
        });
        parsed.unwrap_or_else(|| panic!("the header should define {prefix}"))
    };
    let parts = ["_MAJOR", "_MINOR", "_PATCH"].map(|part| value(part).to_string()).join(".");

    assert_eq!(value(""), kahlnws_version(), "KAHLNWS_VERSION is the number the library answers");
    assert_eq!(parts, env!("CARGO_PKG_VERSION"), "the parts are the crate's version");
}

/// The static library, which the build makes beside the test binary, as a crate type of the library the test links.
///
/// # Panics
/// Panics if the test binary has no folder, or the library is not in it.
fn static_library() -> PathBuf {
    let test_binary = env::current_exe().expect("the test binary should know its path");
    let folder = test_binary.parent().expect("the test binary should lie in a folder");
    let library = folder.join("libkahlnws_capi.a");
    assert!(library.is_file(), "the static library should be built at {}", library.display());

    library
}

/// Whether `compiler` compiles an empty file of `language` in `standard`.
fn has_standard(compiler: &OsString, language: Language, standard: &str) -> bool {
    Command::new(compiler)
        .args([standard, "-fsyntax-only", "-x", language.name(), "/dev/null"])
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn test_every_declaration_compiles_in_every_language_standard() {
    const TEST: &str = "test_every_declaration_compiles_in_every_language_standard";

    for (language, standard, promise) in STANDARDS {
        let Some(compiler) = language.compiler() else {
            skip(TEST, &format!("no {} compiler found for {standard}", language.name()));
            continue;
        };
        if promise == Promise::WhereAvailable && !has_standard(&compiler, language, standard) {
            skip(TEST, &format!("the {} compiler has no {standard}", language.name()));
            continue;
        }

        run(Command::new(compiler)
            .arg(standard)
            .args(WARNINGS)
            .args(["-fsyntax-only", "-x", language.name(), "-I"])
            .arg(in_crate("include"))
            .arg(in_crate("tests/signatures.c")));
    }
}

/// Compiles, without linking, the program `program` of `language` in `standard`, written to the file `name`, and
/// answers the compiler's output.
///
/// # Panics
/// Panics if the program cannot be written or the compiler cannot be started.
fn compiled(compiler: &OsString, language: Language, standard: &str, name: &str, program: &str) -> Output {
    let source = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    fs::write(&source, program).expect("the program should be written");

    Command::new(compiler)
        .arg(standard)
        .args(WARNINGS)
        .args(["-fsyntax-only", "-x", language.name(), "-I"])
        .arg(in_crate("include"))
        .arg(&source)
        .output()
        .expect("the compiler should start")
}

#[test]
fn test_a_status_or_a_solver_ignored_fails_to_compile_and_one_used_compiles() {
    const TEST: &str = "test_a_status_or_a_solver_ignored_fails_to_compile_and_one_used_compiles";
    let answers =
        ["kahlnws_solver_new()", "kahlnws_solver_reserve(0, 1, 1)", "kahlnws_solver_solve(0, 0, 0, 0, 0, 0, 0)"];

    for (language, standard) in [(Language::C, "-std=c99"), (Language::Cpp, "-std=c++17")] {
        let Some(compiler) = language.compiler() else {
            skip(TEST, &format!("no {} compiler found for {standard}", language.name()));
            continue;
        };

        let program = |body: &str| format!("#include \"kahlnws.h\"\nint main(void) {{ {body} }}\n");
        for answer in answers {
            let used = compiled(&compiler, language, standard, "answer.c", &program(&format!("return {answer} != 0;")));
            assert!(used.status.success(), "{standard}: `{answer}` used should compile: {used:?}");

            let ignored =
                compiled(&compiler, language, standard, "answer.c", &program(&format!("{answer}; return 0;")));
            let errors = String::from_utf8_lossy(&ignored.stderr);
            assert!(!ignored.status.success(), "{standard}: `{answer}` ignored should not compile");
            assert!(errors.contains("ignoring return value"), "{standard}: `{answer}` ignored: {errors}");
        }
    }
}

#[test]
fn test_the_depot_is_solved_from_c_through_the_static_library() {
    const TEST: &str = "test_the_depot_is_solved_from_c_through_the_static_library";
    let Some(compiler) = Language::C.compiler() else {
        skip(TEST, "no C compiler found");
        return;
    };
    let Some(libraries) = native_libraries() else {
        skip(TEST, "no rustc found to name the libraries a static library needs");
        return;
    };

    let program = Path::new(env!("CARGO_TARGET_TMPDIR")).join("depot");
    run(Command::new(compiler)
        .args(["-std=c99", "-Wall", "-Wextra", "-Wpedantic", "-Werror", "-I"])
        .arg(in_crate("include"))
        .arg(in_crate("tests/depot.c"))
        .arg(static_library())
        .args(libraries)
        .arg("-o")
        .arg(&program));

    run(&mut Command::new(&program));
}

/// The C++ standards of [`CPP_STANDARDS`] the test `test` compiles in: every one the header promises, and every other
/// that `compiler` has; it says which it leaves out.
fn cpp_standards(compiler: &OsString, test: &str) -> Vec<&'static str> {
    CPP_STANDARDS
        .into_iter()
        .filter(|&(standard, promise)| {
            let compiled = promise == Promise::Promised || has_standard(compiler, Language::Cpp, standard);
            if !compiled {
                skip(test, &format!("the C++ compiler has no {standard}"));
            }
            compiled
        })
        .map(|(standard, _)| standard)
        .collect()
}

/// Builds the C++ program `tests/{program}.cpp` in `standard` and the mode `mode` of [`CPP_MODES`] as `build` says,
/// against the C++ header and the static library with `libraries` beside it, every warning an error; and answers the
/// path of the executable.
///
/// # Panics
/// Panics if the compiler cannot be started, or refuses the program or warns.
fn built_cpp_program(
    compiler: &OsString,
    libraries: &[String],
    program: &str,
    standard: &str,
    mode: (&[&str], &str),
    build: Build,
) -> PathBuf {
    let (mode_flags, mode_name) = mode;
    let name = format!("{program}-{}-{mode_name}{}", standard.trim_start_matches("-std="), build.suffix());
    let executable = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    run(Command::new(compiler)
        .arg(standard)
        .args(WARNINGS)
        .args(STRICT_WARNINGS)
        .args(mode_flags)
        .args(build.flags())
        .arg("-I")
        .arg(in_crate("include"))
        .arg(in_crate(&format!("tests/{program}.cpp")))
        .arg(static_library())
        .args(libraries)
        .arg("-o")
        .arg(&executable));

    executable
}

/// Whether `compiler` builds and links a C++ program with the sanitizers of [`Build::Sanitized`].
fn links_sanitized(compiler: &OsString) -> bool {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let source = folder.join("sanitizer_probe.cpp");
    fs::write(&source, "int main() { return 0; }\n").expect("the probe should be written");

    Command::new(compiler)
        .args(Build::Sanitized.flags())
        .args(["-x", "c++"])
        .arg(&source)
        .arg("-o")
        .arg(folder.join("sanitizer_probe"))
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn test_the_cpp_header_is_ascii_within_120_columns() {
    let header = fs::read_to_string(in_crate("include/kahlnws.hpp")).expect("the C++ header should be read");

    for (number, line) in header.lines().enumerate() {
        assert!(line.is_ascii(), "line {} of the C++ header is not ASCII: {line}", number + 1);
        assert!(line.len() <= 120, "line {} of the C++ header passes 120 columns: {line}", number + 1);
    }
}

#[test]
fn test_the_cpp_header_compiles_alone_in_every_cpp_standard() {
    const TEST: &str = "test_the_cpp_header_compiles_alone_in_every_cpp_standard";
    let Some(compiler) = Language::Cpp.compiler() else {
        skip(TEST, "no C++ compiler found");
        return;
    };

    for standard in cpp_standards(&compiler, TEST) {
        for (mode_flags, _) in CPP_MODES {
            run(Command::new(&compiler)
                .arg(standard)
                .args(WARNINGS)
                .args(STRICT_WARNINGS)
                .args(mode_flags)
                .args(["-fsyntax-only", "-x", "c++"])
                .arg(in_crate("include/kahlnws.hpp")));
        }
    }
}

#[test]
fn test_what_the_cpp_header_refuses_fails_to_compile_and_its_allowed_form_compiles() {
    const TEST: &str = "test_what_the_cpp_header_refuses_fails_to_compile_and_its_allowed_form_compiles";
    let Some(compiler) = Language::Cpp.compiler() else {
        skip(TEST, "no C++ compiler found");
        return;
    };
    let program = |body: &str| format!("{CPP_SNIPPET_PRELUDE}    {body}\n}}\n");

    // C++11 marks an answer not to ignore with the compiler's attribute, C++17 with the standard one.
    for standard in ["-std=c++11", "-std=c++17"] {
        for (case, refused, allowed, reason) in FORBIDDEN {
            let name = |form: &str| format!("{case}-{form}-{}.cpp", standard.trim_start_matches("-std="));

            let compiled_allowed = compiled(&compiler, Language::Cpp, standard, &name("allowed"), &program(allowed));
            let errors = String::from_utf8_lossy(&compiled_allowed.stderr);
            assert!(compiled_allowed.status.success(), "{standard}, {case}: the allowed form should compile: {errors}");

            let compiled_refused = compiled(&compiler, Language::Cpp, standard, &name("refused"), &program(refused));
            let errors = String::from_utf8_lossy(&compiled_refused.stderr);
            assert!(!compiled_refused.status.success(), "{standard}, {case}: the refused form should not compile");
            assert!(errors.contains(reason), "{standard}, {case}: the error should say `{reason}`: {errors}");
        }
    }
}

#[test]
fn test_the_cpp_programs_pass_in_every_cpp_standard() {
    const TEST: &str = "test_the_cpp_programs_pass_in_every_cpp_standard";
    let Some(compiler) = Language::Cpp.compiler() else {
        skip(TEST, "no C++ compiler found");
        return;
    };
    let Some(libraries) = native_libraries() else {
        skip(TEST, "no rustc found to name the libraries a static library needs");
        return;
    };

    for standard in cpp_standards(&compiler, TEST) {
        for mode in CPP_MODES {
            for program in CPP_PROGRAMS {
                run(&mut Command::new(built_cpp_program(&compiler, libraries, program, standard, mode, Build::Plain)));
            }
        }
    }
}

#[test]
fn test_the_cpp_programs_pass_under_the_address_and_undefined_behaviour_sanitizers() {
    const TEST: &str = "test_the_cpp_programs_pass_under_the_address_and_undefined_behaviour_sanitizers";
    let Some(compiler) = Language::Cpp.compiler() else {
        skip(TEST, "no C++ compiler found");
        return;
    };
    let Some(libraries) = native_libraries() else {
        skip(TEST, "no rustc found to name the libraries a static library needs");
        return;
    };
    if !links_sanitized(&compiler) {
        skip(TEST, "the C++ compiler cannot build with the address and undefined behaviour sanitizers");
        return;
    }

    for mode in CPP_MODES {
        for program in CPP_PROGRAMS {
            let executable = built_cpp_program(&compiler, libraries, program, "-std=c++11", mode, Build::Sanitized);
            run(&mut Command::new(executable));
        }
    }
}

/// An example under `examples/`, written as `{name}.c` and as `{name}.cpp`, and what it prints.
#[derive(Clone, Copy, Debug)]
struct Example {
    /// The name of its two source files, without their extension.
    name: &'static str,
    /// Lines it prints in either language and every mode, each whole, in the order printed.
    prints: &'static [&'static str],
    /// The lines only the C++ example prints, and only where exceptions are enabled: those of its throwing form.
    throws: &'static [&'static str],
}

/// Every example, with the answers worked out by hand that it prints.
///
/// The transport network: warehouse 0 supplies 20 units and warehouse 1 supplies 30, every lane carries at most 20,
/// and a unit costs 8, 6 and 10 from warehouse 0 to shops 0, 1 and 2, and 9, 12 and 13 from warehouse 1. The supplies
/// add up to the demands, so each warehouse ships all it has, and a flow costs what warehouse 1 would pay to serve
/// every shop alone, less 1, 6 and 3 for each unit warehouse 0 sends to shop 0, 1 and 2 in its place. Where the shops
/// ask for 10, 25 and 15, warehouse 1 alone would pay 9 * 10 + 12 * 25 + 13 * 15 = 585; warehouse 0's 20 units save
/// most at shop 1, whose lane from it takes all 20, while warehouse 1's carries the other 5: the cost is
/// 585 - 6 * 20 = 465, and this is the one flow of that cost. Where they ask for 20, 20 and 10: 180 + 240 + 130 = 550,
/// less 6 * 20 at shop 1, is 430. Where they ask for 5, 15 and 30: 45 + 180 + 390 = 615; warehouse 1's lane carries
/// only 20 of shop 2's 30, so warehouse 0 sends at least 10 there, saving 3 each, and its other 10 to shop 1, saving 6
/// each: 615 - 30 - 60 = 525. With every lane cut to 5, each warehouse sends at most 15, below warehouse 1's 30, so no
/// flow exists.
const EXAMPLES: [Example; 4] = [
    Example {
        name: "transport",
        prints: &[
            "cost: 465",
            "warehouse 0 -> shop 0: 0",
            "warehouse 0 -> shop 1: 20",
            "warehouse 0 -> shop 2: 0",
            "warehouse 1 -> shop 0: 10",
            "warehouse 1 -> shop 1: 5",
            "warehouse 1 -> shop 2: 15",
        ],
        throws: &[],
    },
    Example { name: "reuse", prints: &["day 1: cost 465", "day 2: cost 430", "day 3: cost 525"], throws: &[] },
    Example {
        name: "outcomes",
        // The library's message of the refusal: the last lane enters node 5 of a problem of 5 nodes.
        prints: &["no flow exists", "refused: arc 5 enters node 5, not below the node count 5"],
        throws: &["thrown: arc 5 enters node 5, not below the node count 5"],
    },
    Example { name: "potentials", prints: &["cost: 465", "the optimality conditions hold"], throws: &[] },
];

/// Builds the example `examples/{name}` of `language` with `flags` beside [`WARNINGS`], against the headers and the
/// static library with `libraries` beside it, into a program whose name ends in `suffix`; runs it, and answers the
/// lines it printed.
///
/// # Panics
/// Panics if the compiler refuses the example or warns, or the example fails or prints what is not text.
fn example_lines(
    compiler: &OsString,
    libraries: &[String],
    language: Language,
    flags: &[&str],
    name: &str,
    suffix: &str,
) -> Vec<String> {
    let executable = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("example-{name}-{suffix}"));
    run(Command::new(compiler)
        .args(flags)
        .args(WARNINGS)
        .arg("-I")
        .arg(in_crate("include"))
        .arg(in_crate(&format!("examples/{name}.{}", language.extension())))
        .arg(static_library())
        .args(libraries)
        .arg("-o")
        .arg(&executable));

    let output = run(&mut Command::new(&executable));
    let printed = String::from_utf8(output.stdout).expect("an example should print text");

    printed.lines().map(str::to_owned).collect()
}

/// Whether every line of `expected` is a line of `printed`, in the same order.
fn prints_in_order(printed: &[String], expected: &[&str]) -> bool {
    let mut lines = printed.iter();

    expected.iter().all(|wanted| lines.any(|line| line == wanted))
}

#[test]
fn test_the_c_examples_print_the_answers_worked_out_by_hand() {
    const TEST: &str = "test_the_c_examples_print_the_answers_worked_out_by_hand";
    let Some(compiler) = Language::C.compiler() else {
        skip(TEST, "no C compiler found");
        return;
    };
    let Some(libraries) = native_libraries() else {
        skip(TEST, "no rustc found to name the libraries a static library needs");
        return;
    };

    for example in EXAMPLES {
        let printed = example_lines(&compiler, libraries, Language::C, &["-std=c99"], example.name, "c99");

        assert!(
            prints_in_order(&printed, example.prints),
            "{}.c should print {:?}, not {printed:?}",
            example.name,
            example.prints
        );
    }
}

#[test]
fn test_the_cpp_examples_print_what_their_c_twins_print_in_every_mode() {
    const TEST: &str = "test_the_cpp_examples_print_what_their_c_twins_print_in_every_mode";
    let (Some(c_compiler), Some(cpp_compiler)) = (Language::C.compiler(), Language::Cpp.compiler()) else {
        skip(TEST, "no C compiler or no C++ compiler found");
        return;
    };
    let Some(libraries) = native_libraries() else {
        skip(TEST, "no rustc found to name the libraries a static library needs");
        return;
    };

    for example in EXAMPLES {
        let c_printed = example_lines(&c_compiler, libraries, Language::C, &["-std=c99"], example.name, "twin-c99");

        for (mode_flags, mode_name) in CPP_MODES {
            let flags = [&["-std=c++11"], &STRICT_WARNINGS[..], mode_flags].concat();
            let suffix = format!("c++11-{mode_name}");
            let printed = example_lines(&cpp_compiler, libraries, Language::Cpp, &flags, example.name, &suffix);
            let (thrown, answered): (Vec<String>, Vec<String>) =
                printed.into_iter().partition(|line| line.starts_with("thrown: "));
            let throws: &[&str] = if mode_name == "exceptions" { example.throws } else { &[] };

            assert!(prints_in_order(&answered, example.prints), "{}.cpp, {mode_name}: {answered:?}", example.name);
            assert_eq!(
                answered, c_printed,
                "{}.cpp, {mode_name}, should print what {}.c prints",
                example.name, example.name
            );
            assert_eq!(thrown, throws, "{}.cpp, {mode_name}, should throw exactly {throws:?}", example.name);
        }
    }
}
