use std::path::PathBuf;
use std::process::Command;

#[test]
fn generate_with_name_and_deps() {
    let dot = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut path = PathBuf::from(&dot);
    let mut out = path.clone();
    path.push("target/debug/acknowledge");
    out.push("ACKNOWLEDGEMENTS-NameAndDeps.md");
    let output = Command::new(path)
        .arg(format!("-p={dot}"))
        .arg("--format=NameAndDeps")
        .arg(format!("--output={}", out.to_str().unwrap()))
        .output()
        .expect("Failed to run");

    println!("output: {output:#?}");

    assert!(output.stderr.is_empty());
}
