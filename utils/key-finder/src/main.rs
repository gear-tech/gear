use directories::ProjectDirs;

const VERSIONS: [&str; 7] = [
    "staging_testnet",
    "staging_testnet_v2",
    "gear_staging_testnet_v3",
    "gear_staging_testnet_v4",
    "gear_staging_testnet_v5",
    "gear_staging_testnet_v6",
    "gear_staging_testnet_v7",
];

const PATHS: [&str; 2] = ["gear", "gear-node"];

fn main() {
    let mut found = false;
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
                        found = true;
                    } else {
                        eprintln!("Failed to read key from {path:?}",);
                    }
                }
            }
        }
    }
    if !found {
        println!("No key file was found. Please try to run this utility with `sudo`.");
    }
}
