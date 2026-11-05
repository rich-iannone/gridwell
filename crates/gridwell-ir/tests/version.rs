//! IR versions: `ir_version` is `MAJOR.MINOR`; another major version is refused
//! (`IR_VERSION`, or a parse error when its JSON isn't IR 1.x), a newer minor
//! version is read.

use gridwell_ir::{IrVersion, ParseError, Table, ValidationRule, IR_VERSION};
use serde_json::{json, Value};

fn table(version: Value) -> Value {
    json!({
        "ir_version": version,
        "config": { "table_cols": 1, "header_rows": 0, "body_rows": 1 },
        "styles": { "defs": {}, "compositions": {}, "conditionals": [] },
        "column_spec": [{ "id": "a" }],
        "table": {
            "thead": { "rows": [] },
            "tbody": [{ "rows": [{ "cells": [{ "content": [] }] }] }]
        }
    })
}

fn parse(v: &Value) -> Result<Table, ParseError> {
    Table::from_json(&v.to_string())
}

fn errors(version: &str) -> Vec<(ValidationRule, String)> {
    parse(&table(json!(version)))
        .expect("parses")
        .validate()
        .into_iter()
        .map(|e| (e.rule, e.message))
        .collect()
}

#[test]
fn versions_are_major_dot_minor() {
    let v = |major, minor| Some(IrVersion { major, minor });
    for (s, want) in [
        ("1.0", v(1, 0)),
        ("1.12", v(1, 12)),
        ("01.00", v(1, 0)),
        ("0.9", v(0, 9)),
        ("2.0", v(2, 0)),
        // Too large for a u32: still a version, just not one this gridwell reads.
        ("4294967296.0", v(u32::MAX, 0)),
        ("1.99999999999", v(1, u32::MAX)),
    ] {
        assert_eq!(IrVersion::parse(s), want, "{s:?}");
    }
    for s in [
        "",
        "1",
        "1.",
        ".0",
        "1.0.0",
        " 1.0",
        "1.0 ",
        "1.0\n",
        "v1.0",
        "+1.0",
        "-1.0",
        "1.0a",
        "1,0",
        "1..0",
        "1.-0",
        "\u{661}.\u{660}",
        "\u{ff11}.\u{ff10}",
    ] {
        assert_eq!(IrVersion::parse(s), None, "{s:?}");
    }
    assert_eq!(IrVersion::parse(IR_VERSION), Some(IrVersion::CURRENT));
    assert_eq!(IrVersion::CURRENT.to_string(), IR_VERSION);
    // Numbers, not strings: 1.9 comes before 1.10.
    assert!(IrVersion::parse("1.9") < IrVersion::parse("1.10"));
}

#[test]
fn only_the_major_version_decides_what_is_read() {
    let read = |s: &str| IrVersion::parse(s).unwrap();
    assert!(read("1.0").is_read() && read("1.7").is_read() && read("01.0").is_read());
    assert!(!read("2.0").is_read() && !read("0.9").is_read());
    assert!(!read("1.0").is_newer_minor());
    assert!(read("1.1").is_newer_minor() && read("1.99999999999").is_newer_minor());
    assert!(!read("2.1").is_newer_minor() && !read("0.1").is_newer_minor());
}

#[test]
fn this_major_version_is_valid_at_any_minor() {
    for s in ["1.0", "1.1", "1.42", "01.0", "1.00"] {
        assert_eq!(errors(s), [], "{s}");
    }
}

#[test]
fn another_major_version_is_an_error() {
    assert_eq!(
        errors("2.0"),
        [(
            ValidationRule::IrVersion,
            "ir_version is \"2.0\", IR 2, and this gridwell reads only IR 1.x \
             (a newer gridwell may read it)"
                .to_string()
        )]
    );
    // No newer gridwell reads an older major.
    assert_eq!(
        errors("0.9"),
        [(
            ValidationRule::IrVersion,
            "ir_version is \"0.9\", IR 0, and this gridwell reads only IR 1.x".to_string()
        )]
    );
    assert_eq!(errors("10.0")[0].0, ValidationRule::IrVersion);
    assert_eq!(errors("4294967296.0")[0].0, ValidationRule::IrVersion);
}

#[test]
fn a_version_that_is_not_major_dot_minor_is_an_error() {
    for (s, shown) in [
        ("1", "\"1\""),
        ("1.0.0", "\"1.0.0\""),
        ("", "\"\""),
        ("v1.0", "\"v1.0\""),
        // Shown as JSON, so what's around it shows.
        (" 1.0", "\" 1.0\""),
        ("1.0\n", "\"1.0\\n\""),
    ] {
        assert_eq!(
            errors(s),
            [(
                ValidationRule::IrVersion,
                format!("ir_version is {shown}, not MAJOR.MINOR (this gridwell reads \"1.0\")")
            )],
            "{s:?}"
        );
    }
}

#[test]
fn the_version_is_reported_with_exceeded_limits() {
    let mut t = parse(&table(json!("2.0"))).unwrap();
    t.config.table_cols = 100_000;
    let rules: Vec<_> = t.validate().into_iter().map(|e| e.rule).collect();
    assert_eq!(
        rules,
        [ValidationRule::IrVersion, ValidationRule::LimitExceeded]
    );
}

#[test]
fn the_other_checks_still_run() {
    // Every error at once, as for any table.
    for version in ["2.0", "1"] {
        let mut t = parse(&table(json!(version))).unwrap();
        t.config.table_cols = 2;
        let rules: Vec<_> = t.validate().into_iter().map(|e| e.rule).collect();
        assert_eq!(rules[0], ValidationRule::IrVersion, "{version}");
        assert!(
            rules.contains(&ValidationRule::ColCount),
            "{version}: {rules:?}"
        );
    }
}

#[test]
fn another_major_version_that_does_not_parse_says_so() {
    // IR 2 in a shape IR 1 doesn't have: the version is the problem, not the shape.
    let mut v = table(json!("2.0"));
    v["table"] = json!({ "sections": [] });
    match parse(&v) {
        Err(ParseError::UnknownMajorVersion { version, message }) => {
            assert_eq!(version, "2.0");
            assert_eq!(
                message,
                "ir_version is \"2.0\", IR 2, and this gridwell reads only IR 1.x \
                 (a newer gridwell may read it)"
            );
        }
        other => panic!("{other:?}"),
    }
    let mut v = table(json!("0.3"));
    v.as_object_mut().unwrap().remove("column_spec");
    let err = parse(&v).unwrap_err();
    assert!(
        matches!(err, ParseError::UnknownMajorVersion { .. }),
        "{err}"
    );
    assert!(err.to_string().ends_with("reads only IR 1.x"), "{err}");
}

#[test]
fn ir_1_that_does_not_parse_says_why() {
    for version in [
        json!("1.0"),
        json!("1.5"),
        json!("1"),
        json!(2.0),
        json!(null),
    ] {
        let mut v = table(version.clone());
        v.as_object_mut().unwrap().remove("column_spec");
        let err = parse(&v).unwrap_err();
        assert!(matches!(err, ParseError::Json(_)), "{version}: {err}");
    }
    let mut v = table(json!("1.0"));
    v.as_object_mut().unwrap().remove("column_spec");
    assert!(parse(&v)
        .unwrap_err()
        .to_string()
        .contains("missing field `column_spec`"));
    // Not JSON, or not an object.
    for s in ["{", "[]", "\"2.0\"", "{\"ir_version\": \"2.0\""] {
        assert!(
            matches!(Table::from_json(s), Err(ParseError::Json(_))),
            "{s}"
        );
    }
    // No version at all.
    let mut v = table(json!("1.0"));
    v.as_object_mut().unwrap().remove("ir_version");
    assert!(parse(&v)
        .unwrap_err()
        .to_string()
        .contains("missing field `ir_version`"));
}

#[test]
fn a_table_knows_if_it_is_a_newer_minor_version() {
    let newer = |s: &str| {
        let mut t = parse(&table(json!("1.0"))).unwrap();
        t.ir_version = s.to_string();
        (t.version(), t.is_newer_minor())
    };
    assert_eq!(newer("1.0"), (Some(IrVersion::CURRENT), false));
    assert_eq!(newer("1.3"), (Some(IrVersion { major: 1, minor: 3 }), true));
    assert_eq!(
        newer("2.3"),
        (Some(IrVersion { major: 2, minor: 3 }), false)
    );
    assert_eq!(newer("1.3.1"), (None, false));
}

#[test]
fn an_invalid_newer_minor_version_table_says_its_version() {
    let mut t = parse(&table(json!("1.3"))).unwrap();
    t.config.table_cols = 2;
    let e = t.ensure_valid().unwrap_err();
    assert_eq!(e.newer_ir_version.as_deref(), Some("1.3"));
    assert!(
        e.to_string().ends_with(
            "\n(the table is IR 1.3, newer than the IR 1.0 this gridwell reads: an error \
             may be something a newer IR version added)"
        ),
        "{e}"
    );
    t.ir_version = "1.0".to_string();
    let e = t.ensure_valid().unwrap_err();
    assert_eq!(e.newer_ir_version, None);
    assert!(!e.to_string().contains("newer"), "{e}");
    // Another major version is an error, not a newer minor.
    t.ir_version = "2.0".to_string();
    assert_eq!(t.ensure_valid().unwrap_err().newer_ir_version, None);
}
