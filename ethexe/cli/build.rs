use std::process::Command;

fn main() {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();

    let git_hash = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => "unknown".to_string(),
    };

    println!("cargo:rustc-env=GIT_SHA={git_hash}");
    println!("cargo:rerun-if-changed=../../.git/HEAD");

    if let Ok(head) = std::fs::read_to_string("../../.git/HEAD")
        && let Some(ref_path) = head.strip_prefix("ref: ")
    {
        println!("cargo:rerun-if-changed=../../.git/{}", ref_path.trim());
    }
}
