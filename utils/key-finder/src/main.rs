use std::env;
use std::path::{MAIN_SEPARATOR, Path};
use hex::encode;

#[cfg(all(unix, not(target_os = "macos")))]
const APPDATA: &'static str = r"/.local/share/";

#[cfg(target_os = "macos")]
const APPDATA: &'static str = r"/Library/Application Support/";

#[cfg(target_os = "windows")]
const APPDATA: &'static str = r"\AppData\Local\";

#[cfg(not(any(unix, windows, target_os = "macos")))]
compile_error!("Unsupported OS");

const VERSIONS: [&'static str; 7] = [
    "staging_testnet",
    "staging_testnet_v2",
    "gear_staging_testnet_v3",
    "gear_staging_testnet_v4",
    "gear_staging_testnet_v5",
    "gear_staging_testnet_v6",
    "gear_staging_testnet_v7"
];

const PATHS: [&'static str; 2] = [
    "gear",
    "gear-node"
];

fn main() {
    #[cfg(any(unix, target_os = "macos"))]
    let home = env::var("HOME").expect("HOME not set");
    #[cfg(target_os = "windows")]
    let home = env::var("USERPROFILE").expect("USERPROFILE not set");

    let keys = PATHS.iter().flat_map(|p| {
        let mut path = home.clone() + APPDATA + p;
        if Path::new(&path).is_dir() {
            path.push(MAIN_SEPARATOR);
            path.push_str("chains");
            path.push(MAIN_SEPARATOR);
            VERSIONS.iter().filter_map(|v| {
                let mut path = path.clone() + v;
                if Path::new(&path).is_dir() {
                    path.push(MAIN_SEPARATOR);
                    path.push_str("network");
                    path.push(MAIN_SEPARATOR);
                    path.push_str("secret_ed25519");
                    if Path::new(&path).is_file() {
                        return Some((path, p, v));
                    }
                }
                None
            }).collect()
        }
        else {
            vec![]
        }
    }).filter_map(|(path, p, v)| {
        if let Ok(key) = std::fs::read(&path) {
            Some((p.to_string() + "/" + v,  encode(key)))
        }
        else {
            eprintln!("Failed to read key from {}", path);
            None
        }
    }).collect::<Vec<_>>();

    for key in keys {
        println!("Key found in \"{}\": 0x{}", key.0, key.1);
    }
}
