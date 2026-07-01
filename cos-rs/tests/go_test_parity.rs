use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn first_party_go_tests_are_in_parity_inventory() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../cos-go-sdk-v5");
    let mut tests = Vec::new();
    collect_go_tests(&root, &root, &mut tests);
    assert!(
        tests.len() >= 540,
        "expected the first-party Go SDK inventory, got only {} entries",
        tests.len()
    );

    let mut uncategorized = Vec::new();
    for (file, name) in &tests {
        if classify(file).is_none() {
            uncategorized.push(format!("{file}:{name}"));
        }
    }
    assert!(
        uncategorized.is_empty(),
        "uncategorized Go tests in parity inventory:\n{}",
        uncategorized.join("\n")
    );

    assert!(
        tests
            .iter()
            .any(|(file, _)| file.contains("costesting/ci_test.go"))
    );
    assert!(
        tests
            .iter()
            .any(|(file, _)| file.contains("crypto/crypto_object_test.go"))
    );
    assert!(tests.iter().any(|(file, _)| file == "vector_test.go"));
}

fn collect_go_tests(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name == "vendor" || name == "example" {
                continue;
            }
            collect_go_tests(root, &path, out);
            continue;
        }
        if !name.ends_with("_test.go") {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let source = fs::read_to_string(&path).unwrap();
        for line in source.lines().map(str::trim_start) {
            if let Some(name) = parse_test_name(line) {
                out.push((rel.clone(), name));
            }
        }
    }
}

fn parse_test_name(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix("func Test") {
        let name = format!("Test{}", rest.split('(').next().unwrap_or_default());
        return Some(name);
    }
    if line.starts_with("func (") {
        let after_receiver = line.split_once(") ")?.1;
        if let Some(rest) = after_receiver.strip_prefix("Test") {
            let name = format!("Test{}", rest.split('(').next().unwrap_or_default());
            return Some(name);
        }
    }
    None
}

fn classify(file: &str) -> Option<&'static str> {
    if file.starts_with("costesting/") {
        Some("live-gated")
    } else if file.starts_with("crypto/") {
        Some("crypto")
    } else if file.starts_with("debug/") {
        Some("debug")
    } else if file.starts_with("ci_") || file == "ci_test.go" {
        Some("ci")
    } else if file.starts_with("bucket_") || file == "bucket_test.go" {
        Some("bucket")
    } else if file.starts_with("object_") || file == "object_test.go" {
        Some("object")
    } else if matches!(
        file,
        "auth_test.go"
            | "batch_test.go"
            | "cos_test.go"
            | "error_test.go"
            | "helper_test.go"
            | "retry_test.go"
            | "service_test.go"
            | "vector_test.go"
    ) {
        Some("core")
    } else {
        None
    }
}
