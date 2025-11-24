#![cfg(any(feature = "secp256k1", feature = "ed25519", feature = "sr25519"))]

use assert_cmd::{Command, cargo::cargo_bin_cmd};
use predicates::str::contains;
use serde_json::Value;
use std::path::PathBuf;
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

#[cfg(feature = "secp256k1")]
#[test]
fn secp256k1_generate_sign_verify_list() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "secp");

    // generate
    let r#gen = gsigner_bin()
        .arg("secp256k1")
        .arg("generate")
        .arg("--storage")
        .arg(&storage)
        .arg("--show-secret")
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    let public = gen_json["Secp256k1"]["Generate"]["public_key"]
        .as_str()
        .unwrap();

    // sign
    let sign = gsigner_bin()
        .args([
            "secp256k1",
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
    let signature = sign_json["Secp256k1"]["Sign"]["signature"]
        .as_str()
        .unwrap();

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
        .args(["secp256k1", "list", "--storage"])
        .arg(&storage)
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    let keys = list_json["Secp256k1"]["List"]["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 1);
}

#[cfg(feature = "secp256k1")]
#[test]
fn secp256k1_rejects_short_hex() {
    gsigner_bin()
        .args([
            "secp256k1",
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
    use gsigner::cli::{Secp256k1Commands, display_secp256k1_result, execute_secp256k1_command};

    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "secp");

    // generate via execute_*
    let r#gen = execute_secp256k1_command(Secp256k1Commands::Generate {
        storage: Some(storage.clone()),
        show_secret: true,
    })
    .expect("generate");
    display_secp256k1_result(&r#gen);

    // sign with prefix and contract (ensure it works and outputs a signature)
    let public = match r#gen {
        gsigner::cli::Secp256k1Result::Generate(r) => r.public_key,
        _ => panic!("unexpected variant"),
    };
    let contract_sig = execute_secp256k1_command(Secp256k1Commands::Sign {
        public_key: public.clone(),
        data: "c0ffee".into(),
        prefix: Some("pre".into()),
        storage: Some(storage.clone()),
        contract: Some("000102030405060708090a0b0c0d0e0f10111213".into()),
    })
    .expect("sign contract");
    display_secp256k1_result(&contract_sig);

    // sign and verify with prefix (no contract) to ensure prefix path works end-to-end
    let plain_sig = execute_secp256k1_command(Secp256k1Commands::Sign {
        public_key: public.clone(),
        data: "c0ffee".into(),
        prefix: Some("pre".into()),
        storage: Some(storage.clone()),
        contract: None,
    })
    .expect("sign plain");
    if let gsigner::cli::Secp256k1Result::Sign(sig_res) = plain_sig {
        let verify = execute_secp256k1_command(Secp256k1Commands::Verify {
            public_key: public,
            data: "c0ffee".into(),
            prefix: Some("pre".into()),
            signature: sig_res.signature,
        })
        .expect("verify");
        display_secp256k1_result(&verify);
    } else {
        panic!("expected sign result");
    }
}

#[cfg(all(feature = "secp256k1", feature = "keyring"))]
#[test]
fn secp256k1_keyring_generate_and_list() {
    let tmp = TempDir::new().unwrap();
    let path = temp_storage(&tmp, "keyring");

    // init and generate
    gsigner_bin()
        .args(["secp256k1", "keyring", "create", "--path"])
        .arg(&path)
        .assert()
        .success();

    let r#gen = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "generate",
            "--path",
            path.to_str().unwrap(),
            "--name",
            "alice",
        ])
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    assert_eq!(
        gen_json["Secp256k1"]["Keyring"]["details"]["name"]
            .as_str()
            .unwrap(),
        "alice"
    );

    let list = gsigner_bin()
        .args([
            "secp256k1",
            "keyring",
            "list",
            "--path",
            path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    let ks = list_json["Secp256k1"]["KeyringList"]["keystores"]
        .as_array()
        .unwrap();
    assert_eq!(ks.len(), 1);
}

#[cfg(feature = "secp256k1")]
#[test]
fn secp256k1_recover_and_address() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "secp");

    let r#gen = gsigner_bin()
        .args([
            "secp256k1",
            "generate",
            "--storage",
            storage.to_str().unwrap(),
            "--show-secret",
        ])
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    let public = gen_json["Secp256k1"]["Generate"]["public_key"]
        .as_str()
        .unwrap();

    // sign a message
    let sig = gsigner_bin()
        .args([
            "secp256k1",
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
    let signature = sig_json["Secp256k1"]["Sign"]["signature"].as_str().unwrap();

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
        rec_json["Secp256k1"]["Recover"]["public_key"]
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
            "insert",
            "--storage",
            storage.to_str().unwrap(),
            priv_hex,
            "--show-secret",
        ])
        .assert()
        .success();
    let ins_json: Value = serde_json::from_slice(&ins.get_output().stdout).unwrap();
    let public = ins_json["Secp256k1"]["Generate"]["public_key"]
        .as_str()
        .unwrap();

    let show = gsigner_bin()
        .args([
            "secp256k1",
            "show",
            "--storage",
            storage.to_str().unwrap(),
            public,
            "--show-secret",
        ])
        .assert()
        .success();
    let show_json: Value = serde_json::from_slice(&show.get_output().stdout).unwrap();
    assert_eq!(
        show_json["Secp256k1"]["Generate"]["public_key"]
            .as_str()
            .unwrap(),
        public
    );
}

#[cfg(feature = "ed25519")]
#[test]
fn ed25519_generate_sign_verify_list_and_address() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "ed");

    let r#gen = gsigner_bin()
        .args([
            "ed25519",
            "generate",
            "--storage",
            storage.to_str().unwrap(),
            "--show-secret",
        ])
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    let public = gen_json["Ed25519"]["Generate"]["public_key"]
        .as_str()
        .unwrap();

    let sig = gsigner_bin()
        .args([
            "ed25519",
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
    let signature = sig_json["Ed25519"]["Sign"]["signature"].as_str().unwrap();

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

    let list = gsigner_bin()
        .args(["ed25519", "list", "--storage", storage.to_str().unwrap()])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    assert_eq!(
        list_json["Ed25519"]["List"]["keys"]
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

    gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "create",
            "--path",
            path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let r#gen = gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "generate",
            "--path",
            path.to_str().unwrap(),
            "--name",
            "bob",
        ])
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    assert_eq!(
        gen_json["Ed25519"]["Keyring"]["details"]["name"]
            .as_str()
            .unwrap(),
        "bob"
    );

    let list = gsigner_bin()
        .args([
            "ed25519",
            "keyring",
            "list",
            "--path",
            path.to_str().unwrap(),
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    let ks = list_json["Ed25519"]["KeyringList"]["keystores"]
        .as_array()
        .unwrap();
    assert_eq!(ks.len(), 1);
}

#[cfg(feature = "sr25519")]
#[test]
fn sr25519_generate_sign_verify_list() {
    let tmp = TempDir::new().unwrap();
    let storage = temp_storage(&tmp, "sr");

    let r#gen = gsigner_bin()
        .args([
            "sr25519",
            "generate",
            "--storage",
            storage.to_str().unwrap(),
            "--show-secret",
        ])
        .assert()
        .success();
    let gen_json: Value = serde_json::from_slice(&r#gen.get_output().stdout).unwrap();
    let public = gen_json["Sr25519"]["Generate"]["public_key"]
        .as_str()
        .unwrap();

    let sig = gsigner_bin()
        .args([
            "sr25519",
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
    let signature = sig_json["Sr25519"]["Sign"]["signature"].as_str().unwrap();
    assert_eq!(signature.len(), 128);

    let list = gsigner_bin()
        .args(["sr25519", "list", "--storage", storage.to_str().unwrap()])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    assert_eq!(
        list_json["Sr25519"]["List"]["keys"]
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

    gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "create",
            "--path",
            path.to_str().unwrap(),
        ])
        .assert()
        .success();

    gsigner_bin()
        .args([
            "sr25519",
            "keyring",
            "add",
            "--path",
            path.to_str().unwrap(),
            "--name",
            "charlie",
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
        ])
        .assert()
        .success();
    let list_json: Value = serde_json::from_slice(&list.get_output().stdout).unwrap();
    let ks = list_json["Sr25519"]["KeyringList"]["keystores"]
        .as_array()
        .unwrap();
    assert_eq!(ks.len(), 1);
}
