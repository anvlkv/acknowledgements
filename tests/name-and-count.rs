use std::path::PathBuf;
use std::process::Command;

#[test]
fn generate_with_name_and_count() {
    let dot = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut path = PathBuf::from(&dot);
    path.push("target/debug/acknowledge");
    let output = Command::new(path)
        .arg(format!("-p={dot}"))
        .output()
        .expect("Failed to run");

    println!("output: {output:#?}");

    assert!(output.stderr.is_empty());
}
