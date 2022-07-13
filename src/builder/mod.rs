//! api builder
#![cfg(feature = "builder")]

pub mod node;
pub(crate) mod paths;
pub mod pre;
pub mod utils;

pub use self::pre::Pre;

/// Spawn gear node and then execute the passing closure.
#[cfg(test)]
pub fn dev(ws: u16, f: fn()) {
    let mut ps = node::Node::new(Pre::default().gear.join(paths::GEAR_BIN))
        .dev(ws)
        .expect("Failed to launch gear node");

    f();

    ps.kill().expect("Failed to kill gear node");
}
