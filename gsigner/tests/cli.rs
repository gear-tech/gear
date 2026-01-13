#![cfg(all(
    feature = "cli",
    any(feature = "secp256k1", feature = "ed25519", feature = "sr25519")
))]

use assert_cmd::{Command, cargo::cargo_bin_cmd};
use gsigner::cli::StorageLocationArgs;
use predicates::str::contains;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn gsigner_bin() -> Command {
    let mut cmd = cargo_bin_cmd!("gsigner");
    cmd.arg("--format").arg("json");
    cmd
}

fn temp_storage(tmp: &TempDir, name: &str) -> PathBuf {
    let path = tmp.path().join(name);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn storage_args(path: &Path) -> StorageLocationArgs {
    StorageLocationArgs {
        path: Some(path.to_path_buf()),
        memory: false,
        storage_password: None,
    }
}

#[cfg(feature = "secp256k1")]
#[test]
fn secp256k1_generate_sign_verify_list() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "secp");

    // generate
    let r#gen = gsigner_bin()
        .args(["secp256k1", "keyring", "generate", "--storage"])
        .arg(&storage)
        .arg("--show-secret")
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    let public = gen_json["result"]["Generate"]["public_key"]
        .as_str()
        .unwrap();

    // sign
    let sign = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "sign",
            "--public-key",
            public,
            "--data",
            "c0ffee",
            "--storage",
        ])
        .arg(&storage)
        .assert()
        .success();
    let sign_json: Value = serde_json::from_slice(&sign.get_output().stdout).unwrap();
    let signature = sign_json["result"]["Sign"]["signature"].as_str().unwrap();

    // verify
    gsigner_bin()
        .args([
            "secp256k1",
            "verify",
            "--public-key",
            public,
            "--data",
            "c0ffee",
            "--signature",
            signature,
        ])
        .assert()
        .success();

    // list
    let list = gsigner_bin()
        .args(["secp256k1", "keyring", "list", "--storage"])
        .arg(&storage)
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    let keys = list_json["result"]["List"]["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 1);
}

#[cfg(feature = "secp256k1")]
#[test]
fn secp256k1_rejects_short_hex() {
    gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "sign",
            "--public-key",
            "0xdeadbeef",
            "--data",
            "00",
        ])
        .assert()
        .failure()
        .stderr(contains("expected 33-byte hex"));
}

#[cfg(feature = "secp256k1")]
#[test]
fn secp256k1_execute_and_display_with_prefix_and_contract() {
    use gsigner::cli::{
        CommandResult, GSignerCommands, Scheme, SchemeKeyringCommands, SchemeResult,
        SchemeSubcommand, display_result, execute_command,
    };

    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "secp");

    // generate via generic dispatcher
    let gen_res = execute_command(GSignerCommands::Secp256k1 {
        command: SchemeSubcommand::Keyring {
            command: SchemeKeyringCommands::Generate {
                storage: storage_args(&storage),
                show_secret: true,
            },
        },
    })
    .expect("generate");
    display_result(&gen_res);

    // sign with prefix and contract (ensure it works and outputs a signature)
    let public = match gen_res.result.clone() {
        SchemeResult::Generate(r) => r.public_key,
        _ => panic!("unexpected variant"),
    };
    let contract_sig = execute_command(GSignerCommands::Secp256k1 {
        command: SchemeSubcommand::Keyring {
            command: SchemeKeyringCommands::Sign {
                public_key: public.clone(),
                data: "c0ffee".into(),
                prefix: Some("pre".into()),
                storage: storage_args(&storage),
                context: None,
                contract: Some("000102030405060708090a0b0c0d0e0f10111213".into()),
            },
        },
    })
    .expect("sign contract");
    display_result(&contract_sig);

    // sign and verify with prefix (no contract) to ensure prefix path works end-to-end
    let plain_sig = execute_command(GSignerCommands::Secp256k1 {
        command: SchemeSubcommand::Keyring {
            command: SchemeKeyringCommands::Sign {
                public_key: public.clone(),
                data: "c0ffee".into(),
                prefix: Some("pre".into()),
                storage: storage_args(&storage),
                context: None,
                contract: None,
            },
        },
    })
    .expect("sign plain");
    if let SchemeResult::Sign(sig_res) = plain_sig.result.clone() {
        let verify = execute_command(GSignerCommands::Secp256k1 {
            command: SchemeSubcommand::Verify {
                public_key: public,
                data: "c0ffee".into(),
                prefix: Some("pre".into()),
                signature: sig_res.signature,
                context: None,
            },
        })
        .expect("verify");
        display_result(&CommandResult {
            scheme: Scheme::Secp256k1,
            result: verify.result,
        });
    } else {
        panic!("expected sign result");
    }
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
#[test]
fn secp256k1_keyring_generate_and_list() {
    let tmp = TempDir::new().unwrap();
    let path = temp_storage(&tmp, "keyring");
    let storage_password = "hunter2";

    // init and generate
    gsigner_bin()
        .args(["secp256k1", "keyring", "init", "--path"])
        .arg(&path)
        .arg("--storage-password")
        .arg(storage_password)
        .assert()
        .success();

    let r#gen = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "create",
            "--path",
            path.to_str().unwrap(),
            "--name",
            "alice",
            "--storage-password",
            storage_password,
        ])
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    assert_eq!(
        gen_json["result"]["Generate"]["name"].as_str().unwrap(),
        "alice"
    );

    let list = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "list",
            "--path",
            path.to_str().unwrap(),
            "--storage-password",
            storage_password,
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    let ks = list_json["result"]["List"]["keys"].as_array().unwrap();
    assert_eq!(ks.len(), 1);
    assert_eq!(ks[0]["name"].as_str().unwrap(), "alice");

    let vanity = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "vanity",
            "--path",
            path.to_str().unwrap(),
            "--name",
            "van",
            "--prefix",
            "",
            "--show-secret",
            "--storage-password",
            storage_password,
        ])
        .assert()
        .success();
    let vanity_json: Value = serde_json::from_slice(&vanity.get_output().stdout).unwrap();
    assert_eq!(
        vanity_json["result"]["Generate"]["name"].as_str().unwrap(),
        "van"
    );
    assert!(
        !vanity_json["result"]["Generate"]["secret"]
            .as_str()
            .unwrap()
            .is_empty()
    );
}

#[cfg(feature = "secp256k1")]
#[test]
fn secp256k1_keyring_clear_removes_keys() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "secp-clear");

    gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "generate",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();

    let list = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "list",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    assert_eq!(
        list_json["result"]["List"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "clear",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();

    let cleared = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "list",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let cleared_json: Value = serde_json::from_slice(&cleared.get_output().stdout).unwrap();
    assert!(
        cleared_json["result"]["List"]["keys"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[cfg(feature = "secp256k1")]
#[test]
fn secp256k1_recover_and_address() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "secp");

    let r#gen = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "generate",
            "--storage",
            storage.to_str().unwrap(),
            "--show-secret",
        ])
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    let public = gen_json["result"]["Generate"]["public_key"]
        .as_str()
        .unwrap();

    // sign a message
    let sig = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "sign",
            "--public-key",
            public,
            "--data",
            "deadbeef",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let sig_json: Value = serde_json::from_slice(&sig.get_output().stdout).unwrap();
    let signature = sig_json["result"]["Sign"]["signature"].as_str().unwrap();

    // recover
    let rec = gsigner_bin()
        .args([
            "secp256k1",
            "recover",
            "--data",
            "deadbeef",
            "--signature",
            signature,
        ])
        .assert()
        .success();
    let rec_json: Value = serde_json::from_slice(&rec.get_output().stdout).unwrap();
    assert_eq!(
        rec_json["result"]["Recover"]["public_key"]
            .as_str()
            .unwrap()
            .len(),
        public.len()
    );

    // address
    gsigner_bin()
        .args(["secp256k1", "address", "--public-key", public])
        .assert()
        .success();
}

#[cfg(feature = "secp256k1")]
#[test]
fn secp256k1_insert_and_show() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "secp");
    // use a fixed private key (32 bytes)
    let priv_hex = "000102030405060708090a0b0c0d0e0f000102030405060708090a0b0c0d0e0f";

    let ins = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "import",
            "--storage",
            storage.to_str().unwrap(),
            "--name",
            "imported",
            "--private-key",
            priv_hex,
            "--show-secret",
        ])
        .assert()
        .success();
    let ins_json: Value = serde_json::from_slice(&ins.get_output().stdout).unwrap();
    let public = ins_json["result"]["Generate"]["public_key"]
        .as_str()
        .unwrap();

    // show should return the imported key details (by public key)
    let show = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "show",
            public,
            "--storage",
            storage.to_str().unwrap(),
            "--show-secret",
        ])
        .assert()
        .success();
    let show_json: Value = serde_json::from_slice(&show.get_output().stdout).unwrap();
    let shown = &show_json["result"]["List"]["keys"][0];
    let shown_public = shown["public_key"].as_str().unwrap();
    assert_eq!(
        shown_public.trim_start_matches("0x"),
        public.trim_start_matches("0x")
    );
    assert_eq!(shown["secret"].as_str().unwrap(), priv_hex);

    // ensure import stored the key by listing
    let list = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "list",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    let keys = list_json["result"]["List"]["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["public_key"].as_str().unwrap(), public);

    // show by address must resolve the same entry
    let address = ins_json["result"]["Generate"]["address"].as_str().unwrap();
    let show_address = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "show",
            address,
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let show_addr_json: Value = serde_json::from_slice(&show_address.get_output().stdout).unwrap();
    let addr_entry = &show_addr_json["result"]["List"]["keys"][0];
    assert_eq!(addr_entry["address"].as_str().unwrap(), address);
}

#[cfg(feature = "ed25519")]
#[test]
fn ed25519_generate_sign_verify_list_and_address() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "ed");

    let r#gen = gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "generate",
            "--storage",
            storage.to_str().unwrap(),
            "--show-secret",
        ])
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    let public = gen_json["result"]["Generate"]["public_key"]
        .as_str()
        .unwrap();

    let sig = gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "sign",
            "--public-key",
            public,
            "--data",
            "cafebabe",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let sig_json: Value = serde_json::from_slice(&sig.get_output().stdout).unwrap();
    let signature = sig_json["result"]["Sign"]["signature"].as_str().unwrap();

    gsigner_bin()
        .args([
            "ed25519",
            "verify",
            "--public-key",
            public,
            "--data",
            "cafebabe",
            "--signature",
            signature,
        ])
        .assert()
        .success();

    let show = gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "show",
            public,
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let show_json: Value = serde_json::from_slice(&show.get_output().stdout).unwrap();
    let show_keys = show_json["result"]["List"]["keys"].as_array().unwrap();
    assert_eq!(show_keys.len(), 1);
    assert_eq!(show_keys[0]["public_key"].as_str().unwrap(), public);

    let list = gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "list",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    assert_eq!(
        list_json["result"]["List"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    gsigner_bin()
        .args(["ed25519", "address", "--public-key", public])
        .assert()
        .success();
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
#[test]
fn ed25519_keyring_generate_and_list() {
    let tmp = TempDir::new().unwrap();
    let path = temp_storage(&tmp, "ed-kr");
    let storage_password = "ed_secret";

    gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "init",
            "--path",
            path.to_str().unwrap(),
            "--storage-password",
            storage_password,
        ])
        .assert()
        .success();

    let r#gen = gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "create",
            "--path",
            path.to_str().unwrap(),
            "--name",
            "bob",
            "--storage-password",
            storage_password,
        ])
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    assert_eq!(
        gen_json["result"]["Generate"]["name"].as_str().unwrap(),
        "bob"
    );

    let list = gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "list",
            "--path",
            path.to_str().unwrap(),
            "--storage-password",
            storage_password,
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    let ks = list_json["result"]["List"]["keys"].as_array().unwrap();
    assert_eq!(ks.len(), 1);
    assert_eq!(ks[0]["name"].as_str().unwrap(), "bob");

    let vanity = gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "vanity",
            "--path",
            path.to_str().unwrap(),
            "--name",
            "bob-van",
            "--prefix",
            "",
            "--show-secret",
            "--storage-password",
            storage_password,
        ])
        .assert()
        .success();
    let vanity_json: Value = serde_json::from_slice(&vanity.get_output().stdout).unwrap();
    assert_eq!(
        vanity_json["result"]["Generate"]["name"].as_str().unwrap(),
        "bob-van"
    );
    assert!(
        !vanity_json["result"]["Generate"]["secret"]
            .as_str()
            .unwrap()
            .is_empty()
    );
}

#[cfg(all(feature = "ed25519", feature = "keyring"))]
#[test]
fn ed25519_keyring_import_and_clear() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "ed-import");

    // import from SURI into the JSON keyring
    gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "import",
            "--storage",
            storage.to_str().unwrap(),
            "--name",
            "imported",
            "--suri",
            "//Alice",
            "--show-secret",
        ])
        .assert()
        .success();

    // ensure the named key exists
    let list = gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "list",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    let keys = list_json["result"]["List"]["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0]["name"].as_str().unwrap(), "imported");

    // clearing the keyring should drop the imported key
    gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "clear",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();

    let after_clear = gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "list",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let cleared_json: Value = serde_json::from_slice(&after_clear.get_output().stdout).unwrap();
    assert!(
        cleared_json["result"]["List"]["keys"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[cfg(feature = "sr25519")]
#[test]
fn sr25519_generate_sign_verify_list() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "sr");

    let r#gen = gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "generate",
            "--storage",
            storage.to_str().unwrap(),
            "--show-secret",
        ])
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    let public = gen_json["result"]["Generate"]["public_key"]
        .as_str()
        .unwrap();

    let sig = gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "sign",
            "--public-key",
            public,
            "--data",
            "abcd",
            "--context",
            "gsigner",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let sig_json: Value = serde_json::from_slice(&sig.get_output().stdout).unwrap();
    let signature = sig_json["result"]["Sign"]["signature"].as_str().unwrap();
    assert_eq!(signature.len(), 128);

    let show = gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "show",
            public,
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let show_json: Value = serde_json::from_slice(&show.get_output().stdout).unwrap();
    let show_keys = show_json["result"]["List"]["keys"].as_array().unwrap();
    assert_eq!(show_keys.len(), 1);
    assert_eq!(show_keys[0]["public_key"].as_str().unwrap(), public);

    let list = gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "list",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    assert_eq!(
        list_json["result"]["List"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
#[test]
fn sr25519_keyring_create_and_list() {
    let tmp = TempDir::new().unwrap();
    let path = temp_storage(&tmp, "sr-kr");
    let storage_password = "sr_secret";

    gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "init",
            "--path",
            path.to_str().unwrap(),
            "--storage-password",
            storage_password,
        ])
        .assert()
        .success();

    gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "vanity",
            "--path",
            path.to_str().unwrap(),
            "--name",
            "charlie",
            "--prefix",
            "",
            "--storage-password",
            storage_password,
        ])
        .assert()
        .success();

    let list = gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "list",
            "--path",
            path.to_str().unwrap(),
            "--storage-password",
            storage_password,
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    let ks = list_json["result"]["List"]["keys"].as_array().unwrap();
    assert_eq!(ks.len(), 1);
    assert_eq!(ks[0]["name"].as_str().unwrap(), "charlie");

    let vanity = gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "vanity",
            "--path",
            path.to_str().unwrap(),
            "--name",
            "charlie-van",
            "--prefix",
            "",
            "--show-secret",
        ])
        .assert()
        .success();
    let vanity_json: Value = serde_json::from_slice(&vanity.get_output().stdout).unwrap();
    assert_eq!(
        vanity_json["result"]["Generate"]["name"].as_str().unwrap(),
        "charlie-van"
    );
    assert!(
        !vanity_json["result"]["Generate"]["secret"]
            .as_str()
            .unwrap()
            .is_empty()
    );
}

#[cfg(all(feature = "sr25519", feature = "keyring"))]
#[test]
fn sr25519_keyring_import_and_clear() {
    let tmp = TempDir::new().unwrap();
    // Use the sr namespace so JSON keyring helpers resolve consistently.
    let storage = temp_storage(&tmp, "sr-import/sr");

    // import a known mnemonic
    gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "import",
            "--storage",
            storage.to_str().unwrap(),
            "--suri",
            "//Alice",
            "--show-secret",
        ])
        .assert()
        .success();

    // ensure import landed in storage
    let list = gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "list",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    assert_eq!(
        list_json["result"]["List"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // clearing removes the imported key
    gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "clear",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();

    let cleared = gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "list",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let cleared_json: Value = serde_json::from_slice(&cleared.get_output().stdout).unwrap();
    assert!(
        cleared_json["result"]["List"]["keys"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[cfg(feature = "sr25519")]
#[test]
fn sr25519_verify_and_address() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "sr-verify");

    let r#gen = gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "generate",
            "--storage",
            storage.to_str().unwrap(),
        ])
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    let public = gen_json["result"]["Generate"]["public_key"]
        .as_str()
        .unwrap();

    let sign = gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "sign",
            "--public-key",
            public,
            "--data",
            "0011",
            "--storage",
            storage.to_str().unwrap(),
            "--context",
            "gsigner",
        ])
        .assert()
        .success();
    let sign_json: Value = serde_json::from_slice(&sign.get_output().stdout).unwrap();
    let signature = sign_json["result"]["Sign"]["signature"].as_str().unwrap();

    // verify command should accept the generated signature
    gsigner_bin()
        .args([
            "sr25519",
            "verify",
            "--public-key",
            public,
            "--data",
            "0011",
            "--signature",
            signature,
            "--context",
            "gsigner",
        ])
        .assert()
        .success();

    // address command should render SS58 address for the key
    gsigner_bin()
        .args(["sr25519", "address", "--public-key", public])
        .assert()
        .success();
}
