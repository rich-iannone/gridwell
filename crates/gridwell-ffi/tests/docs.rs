//! The C API page (`docs/api/c-ffi.qmd`) against the real header and library:
//!
//! - the "Types" and "Functions" blocks declare exactly the header's typedefs and
//!   functions, with the same signatures;
//! - the error-code table lists exactly the header's `GRIDWELL_ERR_*` codes;
//! - every `gridwell_*` / `GRIDWELL_*` name the page mentions exists;
//! - `example.c` compiles as strict C11 against the header and the freshly built
//!   shared library, and prints what the "Example output" block shows when run on
//!   `docs/examples/`. `GRIDWELL_BLESS_DOCS=1` rewrites that block instead.
//!
//! The example is skipped without a C compiler (or on Windows), unless
//! `GRIDWELL_REQUIRE_C_TOOLCHAIN` is set, as in CI.

mod common;

use common::{build_shared_lib, compiler, crate_dir, skip_or_fail};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

const PAGE: &str = "docs/api/c-ffi.qmd";
const EXAMPLE_FENCE: &str = "{.c filename=\"example.c\"}";
const OUTPUT_FENCE: &str = "{.bash filename=\"Example output\"}";

fn root() -> PathBuf {
    crate_dir().join("../..")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
        .replace("\r\n", "\n")
}

fn page() -> String {
    read(&root().join(PAGE))
}

fn header() -> String {
    read(&crate_dir().join("include/gridwell.h"))
}

fn bless() -> bool {
    std::env::var_os("GRIDWELL_BLESS_DOCS").is_some_and(|v| !v.is_empty())
}

/// A fenced block: its info string, the `## ` section it's in, its body, and the
/// byte range of the body in the page.
struct Fence {
    info: String,
    section: String,
    body: String,
    range: std::ops::Range<usize>,
}

fn fences(page: &str) -> Vec<Fence> {
    let mut out = Vec::new();
    let mut section = String::new();
    let mut open: Option<(String, usize)> = None;
    let mut offset = 0;
    for line in page.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let text = line.trim_end();
        match &open {
            None if text.starts_with("```") => {
                open = Some((text.trim_start_matches('`').trim().to_string(), offset));
            }
            None => {
                if let Some(title) = text.strip_prefix("## ") {
                    section = title.trim().to_string();
                }
            }
            Some((info, body_start)) if text.starts_with("```") => {
                out.push(Fence {
                    info: info.clone(),
                    section: section.clone(),
                    body: page[*body_start..start].to_string(),
                    range: *body_start..start,
                });
                open = None;
            }
            Some(_) => {}
        }
    }
    out
}

/// The one fence in `section` with exactly this info string.
fn fence<'a>(fences: &'a [Fence], section: &str, info: &str) -> Result<&'a Fence, String> {
    let found: Vec<_> = fences
        .iter()
        .filter(|f| f.section == section && f.info == info)
        .collect();
    match found.as_slice() {
        [one] => Ok(one),
        _ => Err(format!(
            "expected one ```{info} block under \"## {section}\", found {}",
            found.len()
        )),
    }
}

/// C source without comments. (Neither the header nor the page has comment
/// markers inside string literals.)
fn strip_comments(src: &str) -> String {
    let mut out = String::new();
    let mut rest = src;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("/*") {
            rest = after.find("*/").map_or("", |end| &after[end + 2..]);
            out.push(' ');
        } else if let Some(after) = rest.strip_prefix("//") {
            rest = after.find('\n').map_or("", |end| &after[end..]);
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// The declarations in C source: comments, preprocessor lines and the
/// `#ifdef __cplusplus` blocks removed, split at top-level `;`, whitespace
/// collapsed, and `struct ` dropped from function declarations (the header
/// spells `struct GridwellTable *`, the page the typedef name).
fn declarations(src: &str) -> Vec<String> {
    let src = strip_comments(src);
    let mut kept = String::new();
    let mut in_cplusplus = false;
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("#ifdef __cplusplus") {
            in_cplusplus = true;
        } else if in_cplusplus {
            in_cplusplus = !t.starts_with("#endif");
        } else if !t.starts_with('#') {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for c in kept.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if c == ';' && depth == 0 {
            out.push(normalize(&current));
            current.clear();
        } else {
            current.push(c);
        }
    }
    assert!(
        current.trim().is_empty(),
        "unterminated declaration: {current:?}"
    );
    out
}

fn normalize(decl: &str) -> String {
    let mut s = decl.split_whitespace().collect::<Vec<_>>().join(" ");
    s = s.replace("( ", "(").replace(" )", ")");
    if !s.starts_with("typedef ") {
        s = s.replace("struct ", "");
    }
    s
}

/// What `actual` lacks and has extra, compared with `expected`.
fn compare_sets(what: &str, expected: &[String], actual: &[String]) -> Vec<String> {
    let expected: BTreeSet<_> = expected.iter().collect();
    let actual: BTreeSet<_> = actual.iter().collect();
    let mut problems = Vec::new();
    for missing in expected.difference(&actual) {
        problems.push(format!("{what} lacks the header's `{missing};`"));
    }
    for extra in actual.difference(&expected) {
        problems.push(format!("{what} has `{extra};`, which isn't in the header"));
    }
    problems
}

fn check_declarations(page: &str, header: &str) -> Vec<String> {
    let fences = fences(page);
    let mut problems = Vec::new();
    let (types, functions): (Vec<_>, Vec<_>) = declarations(header)
        .into_iter()
        .partition(|d| d.starts_with("typedef "));
    for (section, expected) in [("Types", &types), ("Functions", &functions)] {
        match fence(&fences, section, "c") {
            Ok(f) => problems.extend(compare_sets(
                &format!("\"## {section}\""),
                expected,
                &declarations(&f.body),
            )),
            Err(e) => problems.push(e),
        }
    }
    problems
}

/// `#define NAME VALUE` lines whose name starts with `prefix`.
fn defines(header: &str, prefix: &str) -> BTreeMap<String, String> {
    header
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            (words.next() == Some("#define")).then_some(())?;
            let name = words.next()?.strip_prefix(prefix)?;
            Some((format!("{prefix}{name}"), words.next()?.to_string()))
        })
        .collect()
}

fn check_error_codes(page: &str, header: &str) -> Vec<String> {
    let mut rows = BTreeMap::new();
    let mut problems = Vec::new();
    let mut section = "";
    for line in page.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            section = title.trim();
        }
        if section != "Error codes" || !line.starts_with('|') {
            continue;
        }
        let cells: Vec<_> = line.trim_matches('|').split('|').map(str::trim).collect();
        if cells.len() < 2 || !cells[0].chars().all(|c| c.is_ascii_digit()) || cells[0].is_empty() {
            continue;
        }
        let name = cells[1].trim_matches('`').to_string();
        if rows.insert(name.clone(), cells[0].to_string()).is_some() {
            problems.push(format!("the error-code table lists {name} twice"));
        }
    }
    let codes = defines(header, "GRIDWELL_ERR_");
    if rows != codes {
        problems.push(format!(
            "the error-code table is {rows:?} but the header defines {codes:?}"
        ));
    }
    problems
}

/// The identifiers in C source or prose.
fn identifiers(text: &str) -> BTreeSet<&str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect()
}

fn check_names(page: &str, header: &str) -> Vec<String> {
    let known = identifiers(header);
    identifiers(page)
        .into_iter()
        .filter(|w| w.starts_with("gridwell_") || w.starts_with("GRIDWELL_"))
        .filter(|w| !known.contains(w))
        .map(|w| format!("the page mentions `{w}`, which isn't in the header"))
        .collect()
}

/// Check (or with `bless`, rewrite) a transcript of `$ ./example ARGS` lines, each
/// followed by what `run(ARGS)` prints. Returns the problems and the new body.
fn check_transcript(
    body: &str,
    bless: bool,
    run: impl Fn(&[&str]) -> String,
) -> (Vec<String>, String) {
    let mut problems = Vec::new();
    let mut new_body = String::new();
    let mut lines = body.lines().peekable();
    let mut commands = 0;
    while let Some(line) = lines.next() {
        let Some(args) = line.strip_prefix("$ ./example") else {
            problems.push(format!("expected a `$ ./example` line, found {line:?}"));
            new_body.push_str(line);
            new_body.push('\n');
            continue;
        };
        commands += 1;
        let mut shown = Vec::new();
        while let Some(next) = lines.peek().filter(|l| !l.starts_with("$ ")) {
            shown.push(*next);
            lines.next();
        }
        let shown = shown.join("\n");
        let args: Vec<&str> = args.split_whitespace().collect();
        let actual = run(&args).trim_end().to_string();
        if shown.trim_end() != actual && !bless {
            problems.push(format!(
                "`./example {}` prints:\n{actual}\nbut the page shows:\n{shown}",
                args.join(" ")
            ));
        }
        new_body.push_str(line);
        new_body.push('\n');
        let output = if bless { &actual } else { &shown };
        if !output.is_empty() {
            new_body.push_str(output);
            new_body.push('\n');
        }
    }
    if commands == 0 {
        problems.push("the example output block runs nothing".to_string());
    }
    (problems, new_body)
}

#[test]
fn the_page_declares_what_the_header_declares() {
    let (page, header) = (page(), header());
    let mut problems = check_declarations(&page, &header);
    problems.extend(check_error_codes(&page, &header));
    problems.extend(check_names(&page, &header));
    assert!(problems.is_empty(), "{PAGE}:\n{}", problems.join("\n"));
}

#[test]
fn the_example_prints_what_the_page_shows() {
    if cfg!(windows) {
        return skip_or_fail("not supported on Windows yet");
    }
    let Some(cc) = compiler("CC", "cc") else {
        return skip_or_fail("no working cc (set CC)");
    };
    let page = page();
    let fences = fences(&page);
    let example = fence(&fences, "Example", EXAMPLE_FENCE).unwrap();
    let output = fence(&fences, "Example", OUTPUT_FENCE).unwrap();
    let lib_dir = build_shared_lib().unwrap();

    let dir = std::env::temp_dir().join(format!("gridwell-c-docs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("example.c"), &example.body).unwrap();
    let exe = dir.join("example");
    let compile = Command::new(&cc)
        .args([
            "-x",
            "c",
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
        ])
        .arg("-I")
        .arg(crate_dir().join("include"))
        .arg(dir.join("example.c"))
        .arg("-L")
        .arg(&lib_dir)
        .arg("-lgridwell_ffi")
        .arg(format!("-Wl,-rpath,{}", lib_dir.display()))
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(
        compile.status.success(),
        "{PAGE}: example.c doesn't compile:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let examples = root().join("docs/examples");
    let (problems, new_body) = check_transcript(&output.body, bless(), |args| {
        let out = Command::new(&exe)
            .args(args)
            .current_dir(&examples)
            .output()
            .unwrap();
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    });
    std::fs::remove_dir_all(&dir).ok();
    if bless() && new_body != output.body {
        let mut text = page.clone();
        text.replace_range(output.range.clone(), &new_body);
        std::fs::write(root().join(PAGE), text).unwrap();
    }
    assert!(problems.is_empty(), "{PAGE}:\n{}", problems.join("\n\n"));

    // The linking command builds the example the page shows.
    let linking = fence(&fences, "Linking", "bash").unwrap();
    assert!(
        linking.body.contains("-o example example.c"),
        "{PAGE}: the linking command doesn't build example.c:\n{}",
        linking.body
    );
}

/// Planted bugs: each wrong page must be caught.
#[test]
fn the_checks_catch_wrong_pages() {
    let (page, header) = (page(), header());
    assert!(check_declarations(&page, &header).is_empty());

    let plants: &[(&str, &str, &str)] = &[
        // A wrong parameter type.
        (
            "gridwell_parse_ir(const char *json, size_t len,",
            "gridwell_parse_ir(const char *json, int len,",
            "lacks the header's `GridwellTable *gridwell_parse_ir",
        ),
        // A dropped function.
        (
            "int32_t gridwell_format_kind(const char *format);\n",
            "",
            "lacks the header's `int32_t gridwell_format_kind",
        ),
        // A wrong field.
        (
            "    size_t len;     // bytes",
            "    int len;     // bytes",
            "has `typedef struct GridwellTextResult",
        ),
        // A wrong error code.
        (
            "| 6 | `GRIDWELL_ERR_OPTIONS`",
            "| 7 | `GRIDWELL_ERR_OPTIONS`",
            "the error-code table is",
        ),
        // A missing error code.
        (
            "| 5 | `GRIDWELL_ERR_PANIC` |",
            "| x | `GRIDWELL_ERR_PANIC` |",
            "the error-code table is",
        ),
        // A name that doesn't exist.
        (
            "Error inspection",
            "Error inspection (see gridwell_last_error)",
            "`gridwell_last_error`",
        ),
        // A function the header doesn't have.
        (
            "// Error inspection",
            "void gridwell_reset(void);\n// Error inspection",
            "has `void gridwell_reset(void);`",
        ),
    ];
    for (from, to, expected) in plants {
        assert!(page.contains(from), "plant {from:?} no longer applies");
        let wrong = page.replacen(from, to, 1);
        let mut problems = check_declarations(&wrong, &header);
        problems.extend(check_error_codes(&wrong, &header));
        problems.extend(check_names(&wrong, &header));
        assert!(
            problems.iter().any(|p| p.contains(expected)),
            "plant {to:?} not caught as {expected:?}: {problems:?}"
        );
    }

    // A header function the page doesn't show.
    let wider = header.replace(
        "#endif  /* GRIDWELL_H */",
        "void gridwell_new(void);\n#endif  /* GRIDWELL_H */",
    );
    assert!(check_declarations(&page, &wider)
        .iter()
        .any(|p| p.contains("lacks the header's `void gridwell_new(void);`")));
    // A header error code the table doesn't list.
    let wider = header.replace(
        "#define GRIDWELL_FORMAT_UNKNOWN",
        "#define GRIDWELL_ERR_IO 7\n#define GRIDWELL_FORMAT_UNKNOWN",
    );
    assert!(!check_error_codes(&page, &wider).is_empty());

    // Transcripts: right, wrong, missing output, blessing, and nothing run.
    let run = |args: &[&str]| format!("ran {}\n", args.join(" "));
    let right = "$ ./example a.json\nran a.json\n$ ./example b.json\nran b.json\n";
    assert_eq!(
        check_transcript(right, false, run),
        (vec![], right.to_string())
    );
    let wrong = "$ ./example a.json\nran b.json\n";
    assert_eq!(check_transcript(wrong, false, run).0.len(), 1);
    let empty = "$ ./example a.json\n$ ./example b.json\n";
    assert_eq!(check_transcript(empty, false, run).0.len(), 2);
    assert_eq!(
        check_transcript(empty, true, run),
        (vec![], right.to_string())
    );
    assert_eq!(check_transcript("ran a.json\n", false, run).0.len(), 2);
}

#[test]
fn declarations_split_and_normalize() {
    let src = "/* a; b */\n#define X 1\n// c;\ntypedef struct S {\n  int a; // x\n  char *b;\n} S;\n\
               #ifdef __cplusplus\nextern \"C\" {\n#endif\nstruct S *f( const struct S *s,\n    int n );\n\
               #ifdef __cplusplus\n}  // extern \"C\"\n#endif\n";
    assert_eq!(
        declarations(src),
        [
            "typedef struct S { int a; char *b; } S",
            "S *f(const S *s, int n)"
        ]
    );
}
