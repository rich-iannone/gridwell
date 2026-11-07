//! The workspace's manifests are ready for crates.io: the crates that are
//! published carry the metadata crates.io shows, depend on each other by version
//! (a path alone can't be published) and never on a crate that isn't published,
//! and the formats are features of `gridwell-render` and `gridwell` that match the
//! registry.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// The workspace crates that aren't published.
const UNPUBLISHED: [&str; 4] = [
    "gridwell-ffi",
    "gridwell-python",
    "gridwell-testkit",
    "xtask",
];

fn metadata() -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
        .current_dir(root)
        .output()
        .expect("run cargo metadata");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn packages(m: &Value) -> BTreeMap<String, Value> {
    m["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["name"].as_str().unwrap().to_string(), p.clone()))
        .collect()
}

/// `publish` is `null` for a crate that may go to any registry, `[]` for none.
fn published(p: &Value) -> bool {
    match &p["publish"] {
        Value::Null => true,
        Value::Array(a) if a.is_empty() => false,
        other => panic!("{}: publish = {other}", p["name"]),
    }
}

fn strings(v: &Value) -> Vec<&str> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect()
}

#[test]
fn only_the_listed_crates_are_unpublished() {
    let all = packages(&metadata());
    let unpublished: BTreeSet<&str> = all
        .values()
        .filter(|p| !published(p))
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(unpublished, BTreeSet::from(UNPUBLISHED));
}

#[test]
fn published_crates_carry_the_metadata_crates_io_shows() {
    let m = metadata();
    let all = packages(&m);
    let version = all["gridwell"]["version"].as_str().unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let license = std::fs::read_to_string(root.join("LICENSE")).unwrap();
    for p in all.values().filter(|p| published(p)) {
        let name = p["name"].as_str().unwrap();
        for field in ["description", "license", "repository", "homepage"] {
            assert!(
                p[field].as_str().is_some_and(|s| !s.is_empty()),
                "{name}: no {field}"
            );
        }
        assert_eq!(p["license"], "MIT", "{name}");
        for field in ["keywords", "categories"] {
            let list = strings(&p[field]);
            assert!((1..=5).contains(&list.len()), "{name}: {field} {list:?}");
        }
        // Released together.
        assert_eq!(p["version"], version, "{name}");
        assert!(p["rust_version"].is_string(), "{name}: no rust-version");
        // The license travels with the crate.
        let dir = Path::new(p["manifest_path"].as_str().unwrap())
            .parent()
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("LICENSE")).ok().as_deref(),
            Some(license.as_str()),
            "{name}: no LICENSE, or not the repository's"
        );
    }
    // The crates people look for have a README on crates.io.
    for name in ["gridwell", "gridwell-cli"] {
        let readme = all[name]["readme"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}"));
        let manifest = Path::new(all[name]["manifest_path"].as_str().unwrap());
        let readme = manifest.parent().unwrap().join(readme);
        assert!(readme.is_file(), "{name}: {}", readme.display());
    }
}

#[test]
fn published_crates_depend_on_workspace_crates_by_version() {
    let m = metadata();
    let all = packages(&m);
    for p in all.values().filter(|p| published(p)) {
        let name = p["name"].as_str().unwrap();
        let version = p["version"].as_str().unwrap();
        for d in p["dependencies"].as_array().unwrap() {
            if d["path"].is_null() {
                continue;
            }
            let dep = d["name"].as_str().unwrap();
            let req = d["req"].as_str().unwrap();
            if d["kind"] == "dev" {
                // Dropped when packaged if it has no version; kept (and needed on
                // crates.io) if it has one.
                if !published(&all[dep]) {
                    assert_eq!(req, "*", "{name}: dev-dependency {dep} has a version");
                }
                continue;
            }
            assert!(published(&all[dep]), "{name} depends on unpublished {dep}");
            assert_eq!(
                req,
                format!("^{version}"),
                "{name}: dependency {dep} needs the workspace version"
            );
        }
    }
}

/// The formats, from the registry.
fn formats() -> BTreeSet<String> {
    gridwell_render::names()
        .into_iter()
        .map(String::from)
        .collect()
}

/// An experimental format's writer crate says so on crates.io; no other crate does.
#[test]
fn experimental_writer_crates_say_so() {
    let m = metadata();
    let all = packages(&m);
    for w in gridwell_render::REGISTRY {
        let name = format!("gridwell-writer-{}", w.name());
        let description = all[&name]["description"].as_str().unwrap();
        assert_eq!(
            description.contains(" for gridwell (experimental): "),
            w.experimental(),
            "{name}: {description}"
        );
    }
    for (name, p) in &all {
        if !name.starts_with("gridwell-writer-") {
            let description = p["description"].as_str().unwrap_or("");
            assert!(!description.contains("experimental"), "{name}");
        }
    }
}

fn features(p: &Value) -> BTreeMap<String, Vec<String>> {
    p["features"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                strings(v).into_iter().map(String::from).collect(),
            )
        })
        .collect()
}

/// Each crate's features: one per format, all on by default.
#[test]
fn every_format_is_a_feature_on_by_default() {
    let m = metadata();
    let all = packages(&m);
    let formats = formats();
    assert_eq!(formats.len(), 11);
    let writers: BTreeSet<String> = all
        .keys()
        .filter_map(|n| n.strip_prefix("gridwell-writer-"))
        .map(String::from)
        .collect();
    assert_eq!(writers, formats, "a writer crate for every format");

    // gridwell's features turn on gridwell-render's too.
    for (name, forwards) in [("gridwell-render", false), ("gridwell", true)] {
        let enables = |f: &str| {
            let render = forwards.then(|| format!("gridwell-render/{f}"));
            render
                .into_iter()
                .chain([format!("dep:gridwell-writer-{f}")])
                .collect::<Vec<_>>()
        };
        let mut features = features(&all[name]);
        let default: BTreeSet<String> = features.remove("default").unwrap().into_iter().collect();
        assert_eq!(default, formats, "{name}: default features");
        assert_eq!(
            features.keys().cloned().collect::<BTreeSet<_>>(),
            formats,
            "{name}: features"
        );
        for (feature, list) in &features {
            assert_eq!(list, &enables(feature), "{name}: feature {feature}");
        }
        for d in all[name]["dependencies"].as_array().unwrap() {
            let dep = d["name"].as_str().unwrap();
            if dep.starts_with("gridwell-writer-") && d["kind"].is_null() {
                assert_eq!(d["optional"], true, "{name}: {dep} isn't optional");
            }
        }
    }

    // And only they do.
    let render = all["gridwell"]["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "gridwell-render")
        .expect("gridwell depends on gridwell-render");
    assert_eq!(render["uses_default_features"], false);
    assert_eq!(render["features"], Value::Array(vec![]));
}
