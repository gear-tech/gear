use std::{
    fs::File,
    io::{BufRead, BufReader},
};

// - we can just match `spec_version:` in `gear/runtime/src/lib.rs`
// since our code has rustfmt checks
const SPEC_VERSION_FIELD: &str = "spec_version:";

/// Find spec version from file.
pub fn find_spec_version(f: &File) -> Option<u32> {
    let parse_spec_version = |s: String| -> Option<u32> {
        s.split_whitespace()
            .last()?
            .trim_end_matches(',')
            .parse()
            .ok()
    };

    for maybe_line in BufReader::new(f).lines() {
        let line = maybe_line.ok()?;
        if !line.contains(SPEC_VERSION_FIELD) {
            continue;
        }

        if let Some(version) = parse_spec_version(line) {
            return Some(version);
        }
    }

    None
}
