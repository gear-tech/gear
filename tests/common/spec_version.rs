//! spec version dependencies.
use std::{
    fs::File,
    io::{BufRead, BufReader},
    num::ParseIntError,
};

/// we can just match `spec_version` since this is
/// auto-generated in `src/client/generated.rs`.
pub const SPEC_VERSION_FIELD: &str = "spec_version:";

/// # Example
///
///  Native runtime: gear-node-1520 (gear-node-1.tx1.au1)
pub const GEAR_NODE_SPEC_VERSION_PATTERN: &str = "Native runtime: gear-node-";

/// Find spec version from file.
pub fn find(f: &File) -> Option<u16> {
    let parse_spec_version = |s: String| -> Option<u16> {
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

/// Parse spec version from string
pub fn parse(line: &str) -> Result<u16, ParseIntError> {
    line.split(GEAR_NODE_SPEC_VERSION_PATTERN)
        .collect::<Vec<_>>()[1]
        .split(|p: char| p.is_whitespace())
        .collect::<Vec<_>>()[0]
        .parse()
}
