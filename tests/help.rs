use std::path::PathBuf;
use std::process::Command;

#[test]
fn print_help() {
    let dot = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut path = PathBuf::from(&dot);
    path.push("target/debug/acknowledge");
    let output = Command::new(path)
        .arg(format!("--help"))
        .output()
        .expect("Failed to run");
    let printed = String::from_utf8(output.stdout).expect("Failed to parse");

    insta::assert_snapshot!(printed);
}
