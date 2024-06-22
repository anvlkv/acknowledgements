use std::path::PathBuf;
use std::process::Command;

#[test]
fn generate_with_dep_and_names() {
    let dot = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut path = PathBuf::from(&dot);
    let mut out = path.clone();
    path.push("target/debug/acknowledge");
    out.push("ACKNOWLEDGEMENTS-DepAndNames.md");
    let output = Command::new(path)
        .arg(format!("-p={dot}"))
        .arg("--format=DepAndNames")
        .arg(format!("--output={}", out.to_str().unwrap()))
        .output()
        .expect("Failed to run");

    println!("output: {output:#?}");

    assert!(output.stderr.is_empty());
}
