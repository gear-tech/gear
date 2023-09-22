use directories::ProjectDirs;
use hex::encode;

const VERSIONS: [&'static str; 7] = [
    "staging_testnet",
    "staging_testnet_v2",
    "gear_staging_testnet_v3",
    "gear_staging_testnet_v4",
    "gear_staging_testnet_v5",
    "gear_staging_testnet_v6",
    "gear_staging_testnet_v7",
];

const PATHS: [&'static str; 2] = ["gear", "gear-node"];

fn main() {
    PATHS
        .iter()
        .flat_map(|p| {
            if let Some(proj_dirs) = ProjectDirs::from("", "", p) {
                let path = proj_dirs.data_local_dir();
                if path.is_dir() {
                    let path = path.join("chains");
                    return VERSIONS
                        .iter()
                        .filter_map(|v| {
                            let mut path = path.clone();
                            path.push(v);
                            if path.is_dir() {
                                path.push("network");
                                path.push("secret_ed25519");
                                if path.is_file() {
                                    return Some((path, p, v));
                                }
                            }
                            None
                        })
                        .collect::<Vec<_>>();
                }
            }
            vec![]
        })
        .filter_map(|(path, p, v)| {
            if let Ok(key) = std::fs::read(&path) {
                Some((p.to_string() + "/" + v, encode(key)))
            } else {
                eprintln!("Failed to read key from {:?}", path);
                None
            }
        })
        .for_each(|(path, key)| {
            println!("Key found in \"{}\": 0x{}", path, key);
        });
}
