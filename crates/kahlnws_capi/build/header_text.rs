// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The text steps that make the generated header one a C programmer reads: the preamble of macros, the overview
//! taken from the crate doc, the version macros, the Rust docs made Doxygen comments, and the numbered constants
//! gathered into enums. Each step is pure, and each that rewrites checks that nothing it should have rewritten is
//! left, panicking with what it found so that the build fails loudly rather than write a header that breaks a promise.
//!
//! The build script includes this file as a module, and so does the test target `header_text`, which runs its tests.

/// The macros every declaration of the header uses, defined once before them: `KAHLNWS_API` before every function
/// and `KAHLNWS_NODISCARD` before every function whose answer must not be ignored. ASCII, as the whole header is.
pub(crate) const PREAMBLE: &str = "\
/**
 * @def KAHLNWS_API
 * @brief Marks every function of the library, and imports it from the dynamic library on Windows.
 *
 * Define KAHLNWS_SHARED before including this header where the program links the dynamic library on Windows: then
 * KAHLNWS_API imports each function from it. Elsewhere, and for the static library, KAHLNWS_API is empty.
 */
#if defined(_WIN32) && defined(KAHLNWS_SHARED)
#define KAHLNWS_API __declspec(dllimport)
#else
#define KAHLNWS_API
#endif

/**
 * @def KAHLNWS_NODISCARD
 * @brief Marks a function whose answer, a status or a solver, must not be ignored.
 *
 * It is the standard attribute under C23 and C++17, the compiler's own under GCC and Clang, nothing otherwise. It
 * follows KAHLNWS_API, after which a standard attribute cannot stand, so it takes the standard form only where
 * KAHLNWS_API is empty.
 */
#if !(defined(_WIN32) && defined(KAHLNWS_SHARED)) && \\
    ((defined(__cplusplus) && __cplusplus >= 201703L) || \\
     (!defined(__cplusplus) && defined(__STDC_VERSION__) && __STDC_VERSION__ >= 202311L))
#define KAHLNWS_NODISCARD [[nodiscard]]
#elif defined(__GNUC__) || defined(__clang__)
#define KAHLNWS_NODISCARD __attribute__((warn_unused_result))
#else
#define KAHLNWS_NODISCARD
#endif
";

/// The crate doc of `lib` up to its first heading, its `//!` lines, as the Doxygen comment of the file: the overview
/// that opens the header, its first paragraph the brief. What follows the first heading is for those who maintain
/// the crate, not for the header's reader.
///
/// # Panics
/// Panics if the overview holds `*/`, which would end the comment early, or if `lib` has no crate doc.
pub(crate) fn overview_comment(lib: &str) -> String {
    let doc = lib.lines().skip_while(|line| !line.starts_with("//!")).take_while(|line| line.starts_with("//!"));
    let mut overview: Vec<&str> = doc
        .map(|line| line.trim_start_matches("//!").trim_end())
        .take_while(|text| !text.trim_start().starts_with('#'))
        .collect();
    assert!(!overview.is_empty(), "the crate root should open with a crate doc, the header's overview");
    assert!(
        overview.iter().all(|text| !text.contains("*/")),
        "the crate doc's overview should not hold `*/`, which would end the header's comment"
    );

    while overview.last() == Some(&"") {
        overview.pop();
    }

    let brief = format!(" * @brief{}", overview[0]);
    let rest = overview[1..].iter().map(|text| if text.is_empty() { " *".to_owned() } else { format!(" *{text}") });
    let body: Vec<String> = [" * @file kahlnws.h".to_owned(), brief].into_iter().chain(rest).collect();

    format!("/**\n{}\n */", body.join("\n"))
}

/// The version macros, each with its brief: each part, and `KAHLNWS_VERSION`, all three as the one number `version`,
/// in hexadecimal so that each part reads as its own digits.
pub(crate) fn version_macros(major: u32, minor: u32, patch: u32, version: u32) -> String {
    format!(
        "/** @brief The major version of the header. */\n#define KAHLNWS_VERSION_MAJOR {major}\n\
         /** @brief The minor version of the header. */\n#define KAHLNWS_VERSION_MINOR {minor}\n\
         /** @brief The patch version of the header. */\n#define KAHLNWS_VERSION_PATCH {patch}\n\
         /** @brief The version of the header in one number, encoded as kahlnws_version answers the library's. */\n\
         #define KAHLNWS_VERSION {version:#08x}u"
    )
}

/// The most columns a line of the header takes.
const LINE_WIDTH: usize = 120;

/// `header` with every doc comment in Doxygen form, and its backticks dropped.
///
/// A doc comment is a block from a line `/**` to a line `*/`. Its first paragraph becomes its `@brief`. The sections
/// a Rust doc ends with become commands: under `# Parameters` each item `` - `name` (in): text `` an
/// `@param[in] name text`, with `(out)` and `(in, out)` likewise; under `# Returns` each item `` - `VALUE`: text `` an
/// `@retval VALUE text`, or its paragraph an `@return`; under `# Safety` each item, or its paragraph, an `@pre`. A
/// brief and a command are wrapped within [`LINE_WIDTH`] columns. A comment that already opens with a command is left
/// as it is written.
///
/// # Panics
/// Panics if a doc comment is not closed, opens with no brief, or holds a section other than those three or an item
/// of another shape than its section's: the docs then have a form this step does not know.
pub(crate) fn doxygen_comments(header: &str) -> String {
    let lines: Vec<&str> = header.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut at = 0;

    while at < lines.len() {
        let line = lines[at];
        let Some(indent) = line.strip_suffix("/**").filter(|indent| indent.trim().is_empty()) else {
            out.push(line.to_owned());
            at += 1;
            continue;
        };
        let length = lines[at..].iter().position(|line| line.trim() == "*/");
        let end = at + length.unwrap_or_else(|| panic!("the doc comment opened on line {} should be closed", at + 1));
        let body: Vec<&str> = lines[at + 1..end].iter().map(|line| doc_text(line)).collect();
        let width = LINE_WIDTH - indent.len() - " * ".len();

        out.push(line.to_owned());
        out.extend(doxygen_doc(&body, width).iter().map(|text| format!("{indent} * {text}").trim_end().to_owned()));
        out.push(lines[end].to_owned());
        at = end + 1;
    }

    (out.join("\n") + "\n").replace('`', "")
}

/// The text of the line `line` of a doc comment: what follows its `*` and one space.
fn doc_text(line: &str) -> &str {
    let text = line.trim_start();
    let text = text.strip_prefix('*').unwrap_or(text);

    text.strip_prefix(' ').unwrap_or(text)
}

/// The doc of the text lines `body` in Doxygen form, its brief and its commands wrapped within `width` columns: the
/// brief, the paragraphs that follow it as they are, then the commands of each section, a blank line between.
///
/// # Panics
/// Panics if `body` opens with no brief, or a section of it has a form [`commands`] does not know.
fn doxygen_doc(body: &[&str], width: usize) -> Vec<String> {
    if body.first().is_some_and(|text| text.starts_with('@')) {
        return body.iter().map(|&text| text.to_owned()).collect();
    }

    let lead_end = body.iter().position(|text| text.starts_with("# ")).unwrap_or(body.len());
    let (lead, sections) = body.split_at(lead_end);
    let brief_end = lead.iter().position(|text| text.is_empty()).unwrap_or(lead.len());
    assert!(brief_end > 0, "a doc should open with its brief, where it opens with `{:?}`", body.first());

    let mut doc = wrapped(&format!("@brief {}", lead[..brief_end].join(" ")), width);
    let details = trimmed(&lead[brief_end..]);
    if !details.is_empty() {
        doc.push(String::new());
        doc.extend(details.iter().map(|&text| text.to_owned()));
    }

    for section in sections.chunk_by(|_, next| !next.starts_with("# ")) {
        doc.push(String::new());
        doc.extend(commands(section[0], trimmed(&section[1..]), width));
    }

    doc
}

/// `lines` without the empty lines it opens and ends with.
fn trimmed<'l, 't>(lines: &'l [&'t str]) -> &'l [&'t str] {
    let start = lines.iter().position(|text| !text.is_empty()).unwrap_or(lines.len());
    let end = lines.iter().rposition(|text| !text.is_empty()).map_or(start, |last| last + 1);

    &lines[start..end]
}

/// What a section of a doc holds under its heading.
#[derive(Debug, PartialEq, Eq)]
enum Entries {
    /// A list: each item with the lines that continue it joined to it, without its `- `.
    Listed(Vec<String>),
    /// No list: its lines joined into one paragraph.
    Paragraph(String),
}

/// The entries of the lines `lines` of a section: a list where the first line opens an item, a paragraph otherwise.
///
/// # Panics
/// Panics if a line of a list neither opens an item nor continues one by two spaces.
fn entries(lines: &[&str]) -> Entries {
    if !lines.first().is_some_and(|text| text.starts_with("- ")) {
        let words: Vec<&str> = lines.iter().copied().filter(|text| !text.is_empty()).collect();
        return Entries::Paragraph(words.join(" "));
    }

    let mut items: Vec<String> = Vec::new();
    for text in lines {
        match (text.strip_prefix("- "), text.strip_prefix("  "), items.last_mut()) {
            (Some(item), _, _) => items.push(item.to_owned()),
            (None, Some(more), Some(item)) => {
                item.push(' ');
                item.push_str(more.trim_start());
            }
            _ => panic!("the line `{text}` of a doc's list should open an item or continue one by two spaces"),
        }
    }

    Entries::Listed(items)
}

/// The Doxygen commands of the section under `heading` with the lines `lines`, each wrapped within `width` columns.
///
/// # Panics
/// Panics if the heading is none of `# Parameters`, `# Returns` and `# Safety`, the parameters are no list, or an
/// item has another shape than [`parameter`] or [`returned_value`] reads.
fn commands(heading: &str, lines: &[&str], width: usize) -> Vec<String> {
    let commands: Vec<String> = match (heading, entries(lines)) {
        ("# Parameters", Entries::Listed(items)) => items.iter().map(|item| parameter(item)).collect(),
        ("# Returns", Entries::Listed(items)) => items.iter().map(|item| returned_value(item)).collect(),
        ("# Returns", Entries::Paragraph(text)) => vec![format!("@return {text}")],
        ("# Safety", Entries::Listed(items)) => items.iter().map(|item| format!("@pre {item}")).collect(),
        ("# Safety", Entries::Paragraph(text)) => vec![format!("@pre {text}")],
        _ => panic!("the section `{heading}` of a doc should be one the header knows, in the form it knows"),
    };

    commands.iter().flat_map(|command| wrapped(command, width)).collect()
}

/// The `@param` command of the item `item`, which reads `` `name` (in): text ``, with `(out)` or `(in, out)` for the
/// other directions.
///
/// # Panics
/// Panics if the item reads otherwise.
fn parameter(item: &str) -> String {
    let named = item.strip_prefix('`').and_then(|rest| rest.split_once("` ("));
    let parsed = named.and_then(|(name, rest)| rest.split_once("): ").map(|(direction, text)| (name, direction, text)));

    match parsed {
        Some((name, direction @ ("in" | "out"), text)) => format!("@param[{direction}] {name} {text}"),
        Some((name, "in, out", text)) => format!("@param[in,out] {name} {text}"),
        _ => {
            panic!("the parameter `{item}` of a doc should read: its name, (in), (out) or (in, out), a colon, its text")
        }
    }
}

/// The `@retval` command of the item `item`, which reads `` `VALUE`: text ``.
///
/// # Panics
/// Panics if the item reads otherwise.
fn returned_value(item: &str) -> String {
    match item.strip_prefix('`').and_then(|rest| rest.split_once("`: ")) {
        Some((value, text)) => format!("@retval {value} {text}"),
        None => panic!("the returned value `{item}` of a doc should read: its name, a colon, its text"),
    }
}

/// The words of `text` in lines of at most `width` columns, as many words on each as fit; a word longer than that
/// stands on a line of its own.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();

    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.len() + 1 + word.len() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }

    lines
}

/// `header` with the numbered constants that follow each `typedef uint32_t` of a type gathered into an anonymous
/// `enum` after it, each with its doc comment: the `#define NAME VALUE` lines whose names begin with the type's
/// name in capitals.
///
/// # Panics
/// Panics if, afterwards, a `typedef uint32_t` of a type is not followed by its `enum`, or a `#define` of one of its
/// constants is left: the generator's output then has a shape this step does not know.
pub(crate) fn enumerate_constants(header: &str) -> String {
    let lines: Vec<&str> = header.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    let mut at = 0;

    while at < lines.len() {
        let line = lines[at];
        out.push(line.to_owned());
        at += 1;

        let Some(name) = typedef_name(line) else {
            continue;
        };
        let prefix = constant_prefix(name);
        let mut members = Vec::new();
        while let Some(member) = constant_at(&lines, at, &prefix) {
            members.extend(member.doc.iter().map(|doc| format!("  {doc}")));
            members.push(format!("  {} = {},", member.name, member.value));
            at = member.next;
        }

        if !members.is_empty() {
            out.push("enum {".to_owned());
            out.extend(members);
            out.push("};".to_owned());
        }
    }

    let enumerated = out.join("\n") + "\n";
    check_enumerated(&enumerated);

    enumerated
}

/// Checks that every `typedef uint32_t` of a type in `header` is followed by its `enum`, and that no `#define` of its
/// constants is left.
///
/// # Panics
/// Panics with the type and the line where either does not hold.
fn check_enumerated(header: &str) {
    let lines: Vec<&str> = header.lines().collect();

    for (at, name) in lines.iter().enumerate().filter_map(|(at, line)| typedef_name(line).map(|name| (at, name))) {
        assert!(
            lines.get(at + 1) == Some(&"enum {"),
            "the typedef of {name} on line {} should be followed by the enum of its constants",
            at + 1
        );
        let prefix = constant_prefix(name);
        let left = lines.iter().position(|line| line.starts_with(&prefix));
        assert!(left.is_none(), "a constant of {name} should not be left as a #define, as on line {left:?}");
    }
}

/// The type a line `typedef uint32_t NAME;` declares; `None` for any other line.
fn typedef_name(line: &str) -> Option<&str> {
    line.strip_prefix("typedef uint32_t ").and_then(|rest| rest.strip_suffix(';'))
}

/// The start of the `#define` line of a constant of the type `name`: its name in capitals and an underscore.
fn constant_prefix(name: &str) -> String {
    format!("#define {}_", name.to_uppercase())
}

/// One numbered constant of the header: its doc comment's lines, its name and value, and the line after it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Constant<'h> {
    /// The lines of its doc comment, from `/**` to ` */`.
    doc: &'h [&'h str],
    /// Its name.
    name: &'h str,
    /// Its value, as written.
    value: &'h str,
    /// The index of the line after its `#define`.
    next: usize,
}

/// The constant whose doc comment starts at line `at` of `lines`, and whose `#define` line starts with `prefix`;
/// `None` where the lines from `at` are not such a constant.
pub(crate) fn constant_at<'h>(lines: &'h [&'h str], at: usize, prefix: &str) -> Option<Constant<'h>> {
    if lines.get(at) != Some(&"/**") {
        return None;
    }
    let end = at + lines[at..].iter().position(|&line| line == " */")?;
    let define = lines.get(end + 1)?;
    if !define.starts_with(prefix) {
        return None;
    }
    let (name, value) = define.strip_prefix("#define ")?.split_once(' ')?;

    Some(Constant { doc: &lines[at..=end], name, value, next: end + 2 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_the_preamble_is_ascii_and_defines_both_macros() {
        assert!(PREAMBLE.is_ascii());
        assert!(PREAMBLE.contains("#define KAHLNWS_API\n") && PREAMBLE.contains("#define KAHLNWS_NODISCARD\n"));
    }

    #[test]
    fn test_the_overview_is_the_crate_doc_up_to_its_first_heading_by_hand() {
        let lib = "// Copyright\n\n//! The C interface.\n//!\n//! Use. Call it.\n//!\n//! # Maintaining\n//!\n\
                   //! Not for C.\n\nmod boundary;\n";

        assert_eq!(
            overview_comment(lib),
            "/**\n * @file kahlnws.h\n * @brief The C interface.\n *\n * Use. Call it.\n */"
        );
    }

    #[test]
    #[should_panic(expected = "the crate doc's overview should not hold `*/`")]
    fn test_an_overview_that_would_end_the_comment_is_refused() {
        let _ = overview_comment("//! A pointer `a*/b`.\n");
    }

    #[test]
    #[should_panic(expected = "the crate root should open with a crate doc")]
    fn test_a_crate_without_a_doc_has_no_overview() {
        let _ = overview_comment("mod boundary;\n");
    }

    #[test]
    fn test_the_version_macros_by_hand() {
        let macros = version_macros(1, 2, 3, (1 << 16) | (2 << 8) | 3);
        let defines: Vec<&str> = macros.lines().filter(|line| line.starts_with("#define")).collect();
        let briefs = macros.lines().filter(|line| line.starts_with("/** @brief ") && line.ends_with(" */")).count();

        assert_eq!(
            defines,
            [
                "#define KAHLNWS_VERSION_MAJOR 1",
                "#define KAHLNWS_VERSION_MINOR 2",
                "#define KAHLNWS_VERSION_PATCH 3",
                "#define KAHLNWS_VERSION 0x010203u"
            ]
        );
        assert_eq!((briefs, macros.lines().count()), (4, 8), "each macro follows its brief");
    }

    /// A doc as the generator writes it from a Rust doc with all three sections.
    const DOC: &str = "\
/**
 * Solves `problem` with `solver`.
 *
 * Room is kept.
 *
 * # Parameters
 * - `solver` (in, out): the solver.
 * - `flows` (out): where the flows go,
 *   one per arc.
 * - `count` (in): how many.
 *
 * # Returns
 * - `KAHLNWS_STATUS_OPTIMAL`: a flow is written;
 * - `KAHLNWS_STATUS_NO_ROOM`: the memory was refused.
 *
 * # Safety
 * - `solver` is live;
 * - `flows` is writable.
 */
int after;
";

    #[test]
    fn test_a_doc_becomes_doxygen_commands_by_hand() {
        let doxygen = "\
/**
 * @brief Solves problem with solver.
 *
 * Room is kept.
 *
 * @param[in,out] solver the solver.
 * @param[out] flows where the flows go, one per arc.
 * @param[in] count how many.
 *
 * @retval KAHLNWS_STATUS_OPTIMAL a flow is written;
 * @retval KAHLNWS_STATUS_NO_ROOM the memory was refused.
 *
 * @pre solver is live;
 * @pre flows is writable.
 */
int after;
";

        assert_eq!(doxygen_comments(DOC), doxygen);
    }

    #[test]
    fn test_a_paragraph_under_returns_or_safety_is_one_command_by_hand() {
        let header = "/**\n * The bound.\n *\n * # Returns\n * The bound,\n * never negative.\n *\n * # Safety\n * \
                      `solver` is live.\n */\n";
        let doxygen =
            "/**\n * @brief The bound.\n *\n * @return The bound, never negative.\n *\n * @pre solver is live.\n */\n";

        assert_eq!(doxygen_comments(header), doxygen);
    }

    #[test]
    fn test_an_indented_doc_keeps_its_indent_and_one_line_docs_and_code_are_left_by_hand() {
        let header = "struct s {\n  /**\n   * The arc; zero where\n   * none is set.\n   */\n  size_t arc;\n};\n\
                      /** @brief One line. */\n#define X 1\n";
        let doxygen = "struct s {\n  /**\n   * @brief The arc; zero where none is set.\n   */\n  size_t arc;\n};\n\
                       /** @brief One line. */\n#define X 1\n";

        assert_eq!(doxygen_comments(header), doxygen);
    }

    #[test]
    fn test_a_comment_that_opens_with_a_command_is_left_as_written() {
        let header = "/**\n * @file kahlnws.h\n * @brief The interface.\n *\n * Use. Call it.\n */\n";

        assert_eq!(doxygen_comments(header), header);
    }

    #[test]
    fn test_a_brief_and_a_command_are_wrapped_within_the_line_width() {
        let long = "word ".repeat(40);
        let header = format!("/**\n * {long}\n *\n * # Safety\n * - {long}\n */\n");

        let doxygen = doxygen_comments(&header);

        assert!(doxygen.lines().all(|line| line.len() <= LINE_WIDTH), "a line passes the width:\n{doxygen}");
        assert_eq!(doxygen.matches("word").count(), 80, "every word is kept");
        assert_eq!(doxygen.lines().filter(|line| line.contains('@')).count(), 2, "each command is written once");
    }

    #[test]
    fn test_words_are_wrapped_as_many_on_a_line_as_fit_by_hand() {
        assert_eq!(wrapped("a bb ccc dddd", 6), ["a bb", "ccc", "dddd"]);
        assert_eq!(wrapped("a bb ccc", 8), ["a bb ccc"]);
        assert_eq!(wrapped("abcdefgh i", 4), ["abcdefgh", "i"], "a word longer than the width stands alone");
        assert!(wrapped("  ", 4).is_empty());
    }

    #[test]
    fn test_the_entries_of_a_section_are_its_items_or_its_paragraph_by_hand() {
        let listed = Entries::Listed(vec!["one, continued".to_owned(), "two".to_owned()]);

        assert_eq!(entries(&["- one,", "  continued", "- two"]), listed);
        assert_eq!(entries(&["One", "paragraph."]), Entries::Paragraph("One paragraph.".to_owned()));
    }

    #[test]
    #[should_panic(expected = "the section `# Panics` of a doc should be one the header knows")]
    fn test_a_section_the_header_does_not_know_is_refused() {
        let _ = doxygen_comments("/**\n * Frees it.\n *\n * # Panics\n * Never.\n */\n");
    }

    #[test]
    #[should_panic(expected = "the parameter `solver: the solver.` of a doc should read")]
    fn test_a_parameter_without_its_direction_is_refused() {
        let _ = doxygen_comments("/**\n * Frees it.\n *\n * # Parameters\n * - solver: the solver.\n */\n");
    }

    #[test]
    #[should_panic(expected = "a doc should open with its brief")]
    fn test_a_doc_without_a_brief_is_refused() {
        let _ = doxygen_comments("/**\n * # Safety\n * `solver` is live.\n */\n");
    }

    #[test]
    #[should_panic(expected = "the doc comment opened on line 1 should be closed")]
    fn test_a_doc_comment_left_open_is_refused() {
        let _ = doxygen_comments("/**\n * Frees it.\n");
    }

    /// A typedef with two numbered constants, as the generator writes them.
    const GENERATED: &str = "typedef uint32_t kahlnws_kind;\n/**\n * The first.\n */\n#define KAHLNWS_KIND_FIRST 1\n\
                             /**\n * The second,\n * on two lines.\n */\n#define KAHLNWS_KIND_SECOND 2\n\nint after;\n";

    #[test]
    fn test_constants_are_gathered_into_an_enum_by_hand() {
        let enumerated = [
            "typedef uint32_t kahlnws_kind;",
            "enum {",
            "  /**",
            "   * The first.",
            "   */",
            "  KAHLNWS_KIND_FIRST = 1,",
            "  /**",
            "   * The second,",
            "   * on two lines.",
            "   */",
            "  KAHLNWS_KIND_SECOND = 2,",
            "};",
            "",
            "int after;\n",
        ]
        .join("\n");

        assert_eq!(enumerate_constants(GENERATED), enumerated);
    }

    #[test]
    #[should_panic(expected = "the typedef of kahlnws_kind on line 1 should be followed by the enum of its constants")]
    fn test_a_typedef_without_constants_is_refused() {
        let _ = enumerate_constants("typedef uint32_t kahlnws_kind;\nint after;\n");
    }

    #[test]
    #[should_panic(expected = "a constant of kahlnws_kind should not be left as a #define")]
    fn test_a_constant_the_rewrite_does_not_reach_is_refused() {
        // A constant without its doc comment, a shape the rewrite does not know.
        let header = "typedef uint32_t kahlnws_kind;\n/**\n * The first.\n */\n#define KAHLNWS_KIND_FIRST 1\n\
                      #define KAHLNWS_KIND_SECOND 2\n";

        let _ = enumerate_constants(header);
    }

    #[test]
    fn test_constant_at_reads_one_constant_or_none_by_hand() {
        let lines: Vec<&str> = GENERATED.lines().collect();
        let first = Constant { doc: &lines[1..4], name: "KAHLNWS_KIND_FIRST", value: "1", next: 5 };

        assert_eq!(constant_at(&lines, 1, "#define KAHLNWS_KIND_"), Some(first));
        assert_eq!(constant_at(&lines, 1, "#define KAHLNWS_OTHER_"), None, "another type's prefix");
        assert_eq!(constant_at(&lines, 0, "#define KAHLNWS_KIND_"), None, "not a doc comment");
        assert_eq!(constant_at(&lines, 11, "#define KAHLNWS_KIND_"), None, "past the constants");
        assert_eq!(constant_at(&["/**", " * Unclosed."], 0, "#define KAHLNWS_KIND_"), None, "an open comment");
    }
}
