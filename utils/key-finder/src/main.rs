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
    for path in PATHS {
        let chains_path = ProjectDirs::from("", "", path)
            .unwrap()
            .data_local_dir()
            .join("chains");
        if chains_path.is_dir() {
            for version in VERSIONS {
                let mut key_path = chains_path.join(version);
                key_path.extend(&["network", "secret_ed25519"]);
                if key_path.is_file() {
                    let key = std::fs::read(&key_path);
                    if let Ok(key) = key {
                        println!("Key found in \"{path}/{version}\": 0x{}", hex::encode(key));
                    } else {
                        eprintln!("Failed to read key from {path:?}",);
                    }
                }
            }
        }
    }
            println!("Key found in \"{}\": 0x{}", path, key);
        });
}
