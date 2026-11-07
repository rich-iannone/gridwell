//! The `gridwell` commands the docs show, run for real.
//!
//! - A transcript block (lines starting `$ gridwell …`, each followed by its
//!   output) is run in a scratch directory holding `docs/examples/*`, and
//!   `conformance/expected/` and `fixtures/` at their own paths, and each
//!   command's stdout and stderr must be the output shown.
//! - Any other shell block's `gridwell …` lines are run there too and must succeed
//!   (lines with `<FORMAT>`- or `[INPUT]`-style placeholders are synopses, not run).
//! - `<!-- gridwell ARGS -->` before a fence runs `gridwell ARGS` from the
//!   repository root, and the fence must hold its stdout; when ARGS writes a file
//!   (`-o PATH`), the file at PATH must be what it writes.
//! - `<!-- gridwell file PATH -->` before a fence: the fence must hold the file.
//!
//! `GRIDWELL_BLESS_DOCS=1` rewrites the blocks (and files) to what was run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bless() -> bool {
    std::env::var_os("GRIDWELL_BLESS_DOCS").is_some_and(|v| v == "1")
}

/// A text file with LF line endings, whatever the checkout did to it.
fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "_site" && name != ".quarto" {
                walk(&path, out);
            }
        } else if name.ends_with(".qmd") {
            out.push(path);
        }
    }
}

/// The pages the docs check reads (as in xtask's docs test): the site and the
/// READMEs that show commands.
fn pages() -> Vec<PathBuf> {
    let root = root();
    let mut pages = Vec::new();
    walk(&root.join("docs"), &mut pages);
    for readme in [
        "README.md",
        "crates/gridwell/README.md",
        "crates/gridwell-cli/README.md",
        "conformance/README.md",
    ] {
        pages.push(root.join(readme));
    }
    pages.sort();
    pages
}

fn rel(path: &Path) -> String {
    path.strip_prefix(root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// A fenced code block: the lines of its opener and closer (0-based) and its body.
struct Fence {
    open: usize,
    close: usize,
    ticks: usize,
    info: String,
    body: Vec<String>,
}

fn fences(lines: &[String]) -> Vec<Fence> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_start();
        let ticks = line.chars().take_while(|&c| c == '`').count();
        if ticks >= 3 {
            let info = line[ticks..].trim().to_string();
            let close = (i + 1..lines.len())
                .find(|&j| {
                    let l = lines[j].trim();
                    l.len() >= ticks && l.chars().all(|c| c == '`')
                })
                .unwrap_or_else(|| panic!("unclosed fence at line {}", i + 1));
            out.push(Fence {
                open: i,
                close,
                ticks,
                info,
                body: lines[i + 1..close].to_vec(),
            });
            i = close + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// The fence's language: `bash` for "```bash" and "```{.bash filename=…}".
fn lang(info: &str) -> String {
    let info = info.trim_start_matches('{').trim_start_matches('.');
    info.split(|c: char| c.is_whitespace() || c == '}')
        .next()
        .unwrap_or("")
        .to_string()
}

fn is_shell(info: &str) -> bool {
    matches!(lang(info).as_str(), "bash" | "sh" | "shell" | "console")
}

/// A word of a command line, and whether any of it was quoted.
#[derive(Debug)]
struct Word {
    text: String,
    quoted: bool,
}

/// Splits a command line as a POSIX shell would for the simple lines the docs
/// show: words, '…' and "…" quoting, backslash escapes, `|` and a `#` comment.
fn words(line: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let Some(&first) = chars.peek() else { break };
        if first == '#' {
            break;
        }
        if first == '|' {
            chars.next();
            out.push(Word {
                text: "|".into(),
                quoted: false,
            });
            continue;
        }
        let mut word = Word {
            text: String::new(),
            quoted: false,
        };
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == '|' {
                break;
            }
            chars.next();
            match c {
                '\'' => {
                    word.quoted = true;
                    for c in chars.by_ref() {
                        if c == '\'' {
                            break;
                        }
                        word.text.push(c);
                    }
                }
                '"' => {
                    word.quoted = true;
                    while let Some(c) = chars.next() {
                        match c {
                            '"' => break,
                            '\\' => word.text.extend(chars.next()),
                            c => word.text.push(c),
                        }
                    }
                }
                '\\' => word.text.extend(chars.next()),
                c => word.text.push(c),
            }
        }
        out.push(word);
    }
    out
}

/// A `gridwell` command from a docs line: its arguments, and the file piped to
/// it (`cat FILE | gridwell …`). What it's piped into is not run.
struct Cmd {
    args: Vec<String>,
    stdin: Option<String>,
}

fn parse(line: &str) -> Option<Cmd> {
    let words = words(line);
    let mut stages = words.split(|w| w.text == "|" && !w.quoted);
    let mut stage = stages.next()?;
    let mut stdin = None;
    if stage.first().is_some_and(|w| w.text == "cat") {
        assert_eq!(
            stage.len(),
            2,
            "only `cat FILE | gridwell …` is run: {line}"
        );
        stdin = Some(stage[1].text.clone());
        stage = stages.next()?;
    }
    if stage.first().is_none_or(|w| w.text != "gridwell") {
        return None;
    }
    for w in stage {
        assert!(
            w.quoted || !w.text.contains(['>', '<', '&', ';', '$', '`']) || is_placeholder(w),
            "the docs check runs plain commands only: {line}"
        );
    }
    Some(Cmd {
        args: stage[1..].iter().map(|w| w.text.clone()).collect(),
        stdin,
    })
}

/// A synopsis word such as `<FORMAT>`, `[INPUT]` or `[OPTIONS]`.
fn is_placeholder(w: &Word) -> bool {
    !w.quoted && (w.text.starts_with('<') || w.text.starts_with('['))
}

fn is_synopsis(line: &str) -> bool {
    words(line).iter().any(is_placeholder)
}

struct Ran {
    ok: bool,
    stdout: String,
    stderr: String,
}

fn run(dir: &Path, cmd: &Cmd) -> Ran {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gridwell"));
    command
        .args(&cmd.args)
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match &cmd.stdin {
        Some(file) => {
            let file = std::fs::File::open(dir.join(file))
                .unwrap_or_else(|e| panic!("{}: {e}", dir.join(file).display()));
            command.stdin(file);
        }
        None => {
            command.stdin(Stdio::null());
        }
    }
    let out = command.output().unwrap();
    Ran {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let dest = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_dir(&path, &dest);
        } else {
            std::fs::copy(&path, &dest).unwrap();
        }
    }
}

/// A fresh scratch directory holding the files the docs' commands name:
/// `docs/examples/*` at its top, and `conformance/expected/` and `fixtures/` at
/// their own paths.
fn scratch() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("gridwell-docs-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(&root().join("docs/examples"), &dir);
    for tree in ["conformance/expected", "fixtures"] {
        copy_dir(&root().join(tree), &dir.join(tree));
    }
    dir
}

fn trim_end(lines: &[String]) -> Vec<String> {
    let mut lines = lines.to_vec();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines
}

fn text_lines(text: &str) -> Vec<String> {
    trim_end(&text.lines().map(str::to_string).collect::<Vec<_>>())
}

/// Where two texts first differ, for a failure message.
fn first_difference(shown: &[String], real: &[String]) -> String {
    let i = (0..shown.len().max(real.len()))
        .find(|&i| shown.get(i) != real.get(i))
        .unwrap_or(0);
    format!(
        "line {}: shown {:?}, real {:?}",
        i + 1,
        shown.get(i).map(String::as_str).unwrap_or("<end>"),
        real.get(i).map(String::as_str).unwrap_or("<end>")
    )
}

/// What one page's check found: problems, and the fence bodies bless would write.
#[derive(Default)]
struct Page {
    problems: Vec<String>,
    rewrites: BTreeMap<usize, (usize, Vec<String>)>,
    files: Vec<(PathBuf, String)>,
}

#[derive(Default)]
struct Counts {
    transcript: usize,
    plain: usize,
    output: usize,
    written: usize,
    file: usize,
}

/// A transcript block: each `$ gridwell …` line, run, and its output.
fn check_transcript(page: &mut Page, at: &str, fence: &Fence, counts: &mut Counts) {
    let dir = scratch();
    let mut real = Vec::new();
    let mut i = 0;
    while i < fence.body.len() {
        let line = &fence.body[i];
        let Some(command) = line.strip_prefix("$ ") else {
            page.problems.push(format!(
                "{at}:{}: a transcript line before any `$ ` command",
                fence.open + i + 2
            ));
            return;
        };
        let end = (i + 1..fence.body.len())
            .find(|&j| fence.body[j].starts_with("$ "))
            .unwrap_or(fence.body.len());
        let shown = trim_end(&fence.body[i + 1..end]);
        let Some(cmd) = parse(command) else {
            page.problems.push(format!(
                "{at}:{}: not a gridwell command: {command}",
                fence.open + i + 2
            ));
            return;
        };
        counts.transcript += 1;
        let ran = run(&dir, &cmd);
        let output = text_lines(&format!("{}{}", ran.stdout, ran.stderr));
        if shown != output {
            page.problems.push(format!(
                "{at}:{}: `{command}` printed something else; {}",
                fence.open + i + 2,
                first_difference(&shown, &output)
            ));
        }
        if !real.is_empty() {
            real.push(String::new());
        }
        real.push(line.clone());
        real.extend(output);
        i = end;
    }
    page.rewrites.insert(fence.open, (fence.close, real));
}

/// A shell block's commands, which must succeed.
fn check_plain(page: &mut Page, at: &str, fence: &Fence, counts: &mut Counts) {
    let dir = scratch();
    for (i, line) in fence.body.iter().enumerate() {
        let line = line.trim();
        if is_synopsis(line) {
            continue;
        }
        let Some(cmd) = parse(line) else { continue };
        counts.plain += 1;
        let ran = run(&dir, &cmd);
        if !ran.ok {
            page.problems.push(format!(
                "{at}:{}: `{line}` failed: {}",
                fence.open + i + 2,
                ran.stderr.trim()
            ));
        }
    }
}

/// The `-o`/`--output` path a command writes, and where in its arguments it is.
fn output_arg(args: &[String]) -> Option<usize> {
    let i = args.iter().position(|a| a == "-o" || a == "--output")?;
    assert!(i + 1 < args.len(), "-o without a path: {args:?}");
    Some(i + 1)
}

/// A `<!-- gridwell … -->` marker at `line`, and the fence after it.
fn check_marker(
    page: &mut Page,
    at: &str,
    line: usize,
    marker: &str,
    next: Option<&Fence>,
    counts: &mut Counts,
) {
    let root = root();
    let where_ = format!("{at}:{}", line + 1);
    if let Some(path) = marker.strip_prefix("file ") {
        let Some(fence) = next else {
            page.problems
                .push(format!("{where_}: no fence after `gridwell file`"));
            return;
        };
        counts.file += 1;
        let real = text_lines(&read(&root.join(path.trim())));
        if trim_end(&fence.body) != real {
            page.problems.push(format!(
                "{where_}: the block isn't {path}; {}",
                first_difference(&trim_end(&fence.body), &real)
            ));
        }
        page.rewrites.insert(fence.open, (fence.close, real));
        return;
    }
    let Some(mut cmd) = parse(&format!("gridwell {marker}")) else {
        page.problems
            .push(format!("{where_}: not a command: {marker}"));
        return;
    };
    if let Some(i) = output_arg(&cmd.args) {
        counts.written += 1;
        let dest = cmd.args[i].clone();
        let dir = scratch();
        let made = dir.join(Path::new(&dest).file_name().unwrap());
        cmd.args[i] = made.to_string_lossy().into_owned();
        let ran = run(&root, &cmd);
        if !ran.ok {
            page.problems.push(format!(
                "{where_}: `gridwell {marker}` failed: {}",
                ran.stderr.trim()
            ));
            return;
        }
        let real = read(&made);
        let shown = std::fs::read_to_string(root.join(&dest))
            .map(|s| s.replace("\r\n", "\n"))
            .unwrap_or_default();
        if shown != real {
            page.problems.push(format!(
                "{where_}: {dest} isn't what `gridwell {marker}` writes"
            ));
        }
        page.files.push((root.join(&dest), real));
        return;
    }
    let Some(fence) = next else {
        page.problems
            .push(format!("{where_}: no fence after `gridwell {marker}`"));
        return;
    };
    counts.output += 1;
    let ran = run(&root, &cmd);
    if !ran.ok {
        page.problems.push(format!(
            "{where_}: `gridwell {marker}` failed: {}",
            ran.stderr.trim()
        ));
        return;
    }
    let real = text_lines(&ran.stdout);
    if real.iter().any(|l| {
        let l = l.trim_start();
        l.starts_with(&"`".repeat(fence.ticks))
    }) {
        page.problems.push(format!(
            "{where_}: the output has a fence line; use a longer fence"
        ));
    }
    if trim_end(&fence.body) != real {
        page.problems.push(format!(
            "{where_}: the block isn't what `gridwell {marker}` prints; {}",
            first_difference(&trim_end(&fence.body), &real)
        ));
    }
    page.rewrites.insert(fence.open, (fence.close, real));
}

fn check_page(path: &Path, counts: &mut Counts, bless: bool) -> Page {
    let at = rel(path);
    let text = read(path);
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let fences = fences(&lines);
    let mut page = Page::default();
    for (n, line) in lines.iter().enumerate() {
        let Some(marker) = line
            .trim()
            .strip_prefix("<!-- gridwell ")
            .and_then(|m| m.strip_suffix("-->"))
        else {
            continue;
        };
        // The fence must come next, after blank lines only.
        let next = fences
            .iter()
            .find(|f| f.open > n)
            .filter(|f| lines[n + 1..f.open].iter().all(|l| l.trim().is_empty()));
        check_marker(&mut page, &at, n, marker.trim(), next, counts);
    }
    for fence in &fences {
        if page.rewrites.contains_key(&fence.open) {
            continue;
        }
        if fence.body.iter().any(|l| l.starts_with("$ gridwell")) {
            check_transcript(&mut page, &at, fence, counts);
        } else if is_shell(&fence.info) {
            check_plain(&mut page, &at, fence, counts);
        }
    }
    if bless {
        let mut lines = lines;
        for (open, (close, body)) in page.rewrites.iter().rev() {
            lines.splice(open + 1..*close, body.iter().cloned());
        }
        let mut new = lines.join("\n");
        new.push('\n');
        if new != text {
            std::fs::write(path, new).unwrap();
        }
        for (file, contents) in &page.files {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, contents).unwrap();
        }
    }
    page
}

#[test]
fn the_commands_the_docs_show_print_what_they_show() {
    let mut counts = Counts::default();
    let mut problems = Vec::new();
    for path in pages() {
        problems.extend(check_page(&path, &mut counts, bless()).problems);
    }
    if !bless() {
        assert!(
            problems.is_empty(),
            "{} problem(s) (GRIDWELL_BLESS_DOCS=1 rewrites the blocks to what was run):\n{}",
            problems.len(),
            problems.join("\n")
        );
    }
    // The check must have found the docs' commands to run, not passed on none.
    assert!(
        counts.transcript >= 6,
        "transcript commands: {}",
        counts.transcript
    );
    assert!(counts.plain >= 15, "plain commands: {}", counts.plain);
    assert!(counts.output >= 15, "output markers: {}", counts.output);
    assert!(
        counts.written >= 1,
        "written-file markers: {}",
        counts.written
    );
    assert!(counts.file >= 5, "file markers: {}", counts.file);
}

#[test]
fn command_lines_split_as_a_shell_would() {
    let texts = |line: &str| words(line).into_iter().map(|w| w.text).collect::<Vec<_>>();
    assert_eq!(
        texts(r#"gridwell convert t.json --options '{"a": true}' # note"#),
        [
            "gridwell",
            "convert",
            "t.json",
            "--options",
            r#"{"a": true}"#
        ]
    );
    assert_eq!(texts(r#"a "b \"c\"" d\ e"#), ["a", r#"b "c""#, "d e"]);
    assert_eq!(
        texts("cat t.json | gridwell convert|pbcopy"),
        ["cat", "t.json", "|", "gridwell", "convert", "|", "pbcopy"]
    );
    let cmd = parse("cat t.json | gridwell convert --to html | pbcopy").unwrap();
    assert_eq!(cmd.args, ["convert", "--to", "html"]);
    assert_eq!(cmd.stdin.as_deref(), Some("t.json"));
    assert!(parse("cargo install gridwell-cli").is_none());
    assert!(is_synopsis(
        "gridwell convert [OPTIONS] --to <FORMAT> [INPUT]"
    ));
    assert!(!is_synopsis("gridwell convert t.json --options '[1]'"));
}

/// Negative controls: a page whose blocks are wrong must be caught.
#[test]
fn the_check_catches_wrong_blocks() {
    let dir = scratch();
    let cases: &[(&str, &str)] = &[
        (
            "a transcript with the wrong output",
            "```bash\n$ gridwell validate hello.json\nValid: no errors.\n```\n",
        ),
        (
            "a transcript missing a line",
            "```bash\n$ gridwell validate broken_table.json\nFound 2 validation error(s):\n  - [COL_COUNT] tbody group 0 row 0 has 4 cells but config.table_cols is 3\n```\n",
        ),
        (
            "a shell command with a renamed option",
            "```bash\ngridwell convert table.json --to html --options '{\"inline_style\": true}'\n```\n",
        ),
        (
            "a shell command naming a missing format",
            "```bash\ngridwell convert table.json --to markdown\n```\n",
        ),
        (
            "an output block that isn't the output",
            "<!-- gridwell convert fixtures/minimal/minimal_1x1.json --to latex -->\n```latex\n\\begin{tabular}{c}\n```\n",
        ),
        (
            "an output marker with no fence",
            "<!-- gridwell convert fixtures/minimal/minimal_1x1.json --to latex -->\nText.\n",
        ),
        (
            "a file block that isn't the file",
            "<!-- gridwell file docs/examples/hello.json -->\n```json\n{}\n```\n",
        ),
        (
            "a written file that isn't what's written",
            "<!-- gridwell convert fixtures/minimal/minimal_1x1.json -o docs/examples/hello.json -->\n",
        ),
    ];
    for (what, page) in cases {
        let path = dir.join("page.qmd");
        std::fs::write(&path, page).unwrap();
        let problems = check_page(&path, &mut Counts::default(), false).problems;
        assert!(!problems.is_empty(), "not caught: {what}");
    }
    // And a right page passes.
    let right = "```bash\n$ gridwell validate hello.json\nValid: no errors found.\n```\n\n\
                 ```bash\ngridwell convert table.json -o out.docx # binary\n```\n";
    let path = dir.join("page.qmd");
    std::fs::write(&path, right).unwrap();
    let problems = check_page(&path, &mut Counts::default(), false).problems;
    assert!(problems.is_empty(), "{problems:?}");
}
