//! Crate boundaries, enforced: each crate's normal dependencies must be
//! exactly the ones allowed in docs/architecture/crate-dependencies.md.
//! Adding a dependency means updating that document and this table
//! together, deliberately. Dev-dependencies are not restricted.

use std::collections::BTreeSet;
use std::path::Path;

/// `(crate directory, allowed normal dependencies)`.
const ALLOWED: &[(&str, &[&str])] = &[
    ("color", &[]),
    ("canvas", &["glam"]),
    (
        "brush",
        &[
            "tela-color",
            "tela-canvas",
            "glam",
            "serde",
            "serde_json",
            "rayon",
        ],
    ),
    ("document", &["tela-color", "tela-canvas"]),
    (
        "io",
        &[
            "tela-color",
            "tela-canvas",
            "tela-document",
            "serde",
            "serde_json",
            "png",
            "zip",
        ],
    ),
    (
        "renderer",
        &["tela-canvas", "wgpu", "glam", "bytemuck", "log"],
    ),
    (
        "tablet",
        &["glam", "log", "windows-sys", "raw-window-handle"],
    ),
    (
        "ui",
        &[
            "tela-brush",
            "tela-renderer",
            "egui",
            "egui-wgpu",
            "egui-winit",
            "wgpu",
            "winit",
        ],
    ),
    (
        "corpus",
        &["tela-color", "tela-canvas", "tela-brush", "glam", "png"],
    ),
    (
        "app",
        &[
            "tela-color",
            "tela-canvas",
            "tela-brush",
            "tela-document",
            "tela-renderer",
            "tela-tablet",
            "tela-ui",
            "tela-io",
            "rfd",
            "winit",
            "glam",
            "pollster",
            "log",
            "env_logger",
        ],
    ),
];

/// Dependency names declared in `[dependencies]` and
/// `[target.'...'.dependencies]` (not dev/build dependencies).
fn normal_dependencies(manifest: &str) -> BTreeSet<String> {
    let mut deps = BTreeSet::new();
    let mut in_deps = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_deps = line == "[dependencies]"
                || (line.starts_with("[target.") && line.ends_with(".dependencies]"));
            continue;
        }
        if !in_deps || line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `name = ...`, `name.workspace = true`, or a continuation line of
        // a multi-line value (starts with a quote or a bracket).
        let name: String = line
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if !name.is_empty() {
            deps.insert(name);
        }
    }
    deps
}

#[test]
fn crates_depend_only_on_what_their_boundary_allows() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut problems = Vec::new();
    for (dir, allowed) in ALLOWED {
        let manifest = std::fs::read_to_string(crates_dir.join(dir).join("Cargo.toml"))
            .unwrap_or_else(|e| panic!("{dir}/Cargo.toml: {e}"));
        let actual = normal_dependencies(&manifest);
        let allowed: BTreeSet<String> = allowed.iter().map(|s| s.to_string()).collect();
        if actual != allowed {
            let extra: Vec<_> = actual.difference(&allowed).collect();
            let missing: Vec<_> = allowed.difference(&actual).collect();
            problems.push(format!(
                "{dir}: not allowed {extra:?}, no longer used {missing:?}"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "crate boundaries changed; update docs/architecture/crate-dependencies.md and this table \
         deliberately:\n{}",
        problems.join("\n")
    );
}

#[test]
fn every_workspace_crate_is_covered() {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let present: BTreeSet<String> = std::fs::read_dir(&crates_dir)
        .unwrap()
        .filter_map(|e| {
            let e = e.ok()?;
            e.path()
                .join("Cargo.toml")
                .exists()
                .then(|| e.file_name().into_string().ok())?
        })
        .collect();
    let covered: BTreeSet<String> = ALLOWED.iter().map(|(d, _)| d.to_string()).collect();
    assert_eq!(present, covered, "add new crates to the boundary table");
}

#[test]
fn parser_reads_the_dependency_forms_used_here() {
    let toml = r#"
[package]
name = "x"

[dependencies]
a.workspace = true
b = { workspace = true }
# comment

[target.'cfg(windows)'.dependencies]
c = { workspace = true, features = [
    "F",
] }

[dev-dependencies]
d.workspace = true
"#;
    let deps: Vec<String> = normal_dependencies(toml).into_iter().collect();
    assert_eq!(deps, ["a", "b", "c"]);
}
