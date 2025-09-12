// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::*;

use gbuiltin_webpki::{Request, Response};
use gstd::{ActorId, Vec, msg, prelude::*};
use hashbrown::HashMap;
use hex_literal::hex;

use hkdf::Hkdf;
use hmac::Mac;
use sha2::{Digest, Sha256, Sha384};

// AEADs
use aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes128Gcm, Aes256Gcm, Key as GcmKey, Nonce as GcmNonce};
use chacha20poly1305::{ChaCha20Poly1305, Key as ChaKey, Nonce as ChaNonce};

const BUILTIN_WEBPKI: ActorId = ActorId::new(hex!(
    "13483c43cc2e4ebc4e7f79ac1f8e66c149a41ad11e483a6e2de2c12f535af06e"
));

static mut VERIFICATIONS: Option<Verifications> = None;

#[derive(Debug, Clone, Default)]
struct Verifications {
    pub domains: HashMap<Vec<u8>, Vec<Report>>,
}

#[derive(Clone, Copy)]
enum HashType {
    Sha256,
    Sha384,
}

#[allow(clippy::large_enum_variant)]
enum AlgoType {
    Aes128(Aes128Gcm),
    Aes256(Aes256Gcm),
    ChaCha(ChaCha20Poly1305),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HsType {
    KeyUpdate,
    Other(u8),
}

struct RecAead {
    hash: HashType,
    algo: AlgoType,
    secret: Vec<u8>,
    key_len: usize,
    iv: [u8; 12],
    seq: u64,
}

impl RecAead {
    fn decrypt(&mut self, hdr: &[u8; 5], ct_tag: &mut [u8]) -> Result<Vec<u8>, String> {
        let mut nonce = self.iv;

        for (n, s) in nonce[4..].iter_mut().zip(self.seq.to_be_bytes()) {
            *n ^= s;
        }

        let p = Payload {
            msg: ct_tag,
            aad: hdr,
        };

        let pt = match &self.algo {
            AlgoType::Aes128(a) => a.decrypt(GcmNonce::from_slice(&nonce), p),
            AlgoType::Aes256(a) => a.decrypt(GcmNonce::from_slice(&nonce), p),
            AlgoType::ChaCha(a) => a.decrypt(ChaNonce::from_slice(&nonce), p),
        }
        .map_err(|e| format!("decrypt error: {e:?}"))?;

        self.seq = self
            .seq
            .checked_add(1)
            .ok_or_else(|| ("seq overflow").to_string())?;

        Ok(pt)
    }

    pub fn update_traffic_secret(&mut self) -> Result<(), String> {
        let hlen = match self.hash {
            HashType::Sha256 => 32,
            HashType::Sha384 => 48,
        };

        let new_secret = hkdf_expand(self.hash, &self.secret, "traffic upd", &[], hlen)?;
        // derive new key/iv
        let (new_key, new_iv) = derive_key_iv(&new_secret, self.hash, self.key_len);

        // applying
        self.secret = new_secret;
        self.iv = new_iv;
        self.seq = 0;

        self.algo = match self.algo {
            AlgoType::Aes128(_) => {
                AlgoType::Aes128(Aes128Gcm::new(GcmKey::<Aes128Gcm>::from_slice(&new_key)))
            }
            AlgoType::Aes256(_) => {
                AlgoType::Aes256(Aes256Gcm::new(GcmKey::<Aes256Gcm>::from_slice(&new_key)))
            }
            AlgoType::ChaCha(_) => {
                AlgoType::ChaCha(ChaCha20Poly1305::new(ChaKey::from_slice(&new_key)))
            }
        };

        Ok(())
    }
}

fn for_each_handshake(
    mut buf: &[u8],
    mut f: impl FnMut(HsType, &[u8]) -> Result<(), String>,
) -> Result<(), String> {
    while buf.len() >= 4 {
        let hs_type = buf[0];
        let len = ((buf[1] as usize) << 16) | ((buf[2] as usize) << 8) | (buf[3] as usize);

        buf = &buf[4..];
        if buf.len() < len {
            return Err("truncated handshake fragment".into());
        }

        let body = &buf[..len];
        buf = &buf[len..];

        let hst = if hs_type == 0x18 {
            HsType::KeyUpdate
        } else {
            HsType::Other(hs_type)
        };
        f(hst, body)?;
    }
    Ok(())
}

fn hkdf_expand(
    hash: HashType,
    prk: &[u8],
    label: &str,
    context: &[u8],
    out_len: usize,
) -> Result<Vec<u8>, String> {
    if out_len > u16::MAX as usize {
        return Err("hkdf out_len exceeds 65535".into());
    }
    const PREFIX: &str = "tls13 ";
    let full_label = format!("{PREFIX}{label}");
    if full_label.len() > u8::MAX as usize {
        return Err("hkdf label too long".into());
    }
    if context.len() > u8::MAX as usize {
        return Err("hkdf context too long".into());
    }

    let mut info = Vec::with_capacity(2 + 1 + full_label.len() + 1 + context.len());
    info.extend_from_slice(&(out_len as u16).to_be_bytes());
    info.push(full_label.len() as u8);
    info.extend_from_slice(full_label.as_bytes());
    info.push(context.len() as u8);
    info.extend_from_slice(context);

    let mut okm = vec![0u8; out_len];
    match hash {
        HashType::Sha256 => Hkdf::<Sha256>::from_prk(prk)
            .map_err(|_| "invalid PRK for SHA-256")?
            .expand(&info, &mut okm)
            .map_err(|_| "hkdf expand failed (sha256)")?,
        HashType::Sha384 => Hkdf::<Sha384>::from_prk(prk)
            .map_err(|_| "invalid PRK for SHA-384")?
            .expand(&info, &mut okm)
            .map_err(|_| "hkdf expand failed (sha384)")?,
    }
    Ok(okm)
}

fn derive_key_iv(secret: &[u8], h: HashType, key_len: usize) -> (Vec<u8>, [u8; 12]) {
    let (key, iv) = {
        (
            hkdf_expand(h, secret, "key", &[], key_len).expect("err"),
            hkdf_expand(h, secret, "iv", &[], 12).expect("err"),
        )
    };

    let mut iv12 = [0u8; 12];
    iv12.copy_from_slice(&iv);

    (key, iv12)
}

fn aead_by_suite(suite: CipherSuite) -> fn(&[u8]) -> RecAead {
    match suite {
        CipherSuite::TLS_AES_256_GCM_SHA384 => |s| {
            let (key, iv) = derive_key_iv(s, HashType::Sha384, 32);
            RecAead {
                hash: HashType::Sha384,
                algo: AlgoType::Aes256(Aes256Gcm::new(GcmKey::<Aes256Gcm>::from_slice(&key))),
                secret: s.to_vec(),
                key_len: 32,
                iv,
                seq: 0,
            }
        },
        CipherSuite::TLS_AES_128_GCM_SHA256 => |s| {
            let (key, iv) = derive_key_iv(s, HashType::Sha256, 16);
            RecAead {
                hash: HashType::Sha256,
                algo: AlgoType::Aes128(Aes128Gcm::new(GcmKey::<Aes128Gcm>::from_slice(&key))),
                secret: s.to_vec(),
                key_len: 16,
                iv,
                seq: 0,
            }
        },
        CipherSuite::TLS_CHACHA20_POLY1305_SHA256 => |s| {
            let (key, iv) = derive_key_iv(s, HashType::Sha256, 32);
            RecAead {
                hash: HashType::Sha256,
                algo: AlgoType::ChaCha(ChaCha20Poly1305::new(ChaKey::from_slice(&key))),
                secret: s.to_vec(),
                key_len: 32,
                iv,
                seq: 0,
            }
        },
        other => panic!("unsupported cipher suite: {:?}", other),
    }
}

fn parse_record(rec: &TlsRecord) -> ([u8; 5], Vec<u8>) {
    if rec.bytes.len() < 5 {
        panic!("short record");
    }
    let mut hdr = [0u8; 5];
    hdr.copy_from_slice(&rec.bytes[..5]);
    let n = u16::from_be_bytes([hdr[3], hdr[4]]) as usize;
    if rec.bytes.len() != 5 + n {
        panic!("len mismatch");
    }
    (hdr, rec.bytes[5..].to_vec())
}

fn strip_inner(pt: &[u8]) -> (u8, Vec<u8>) {
    if pt.is_empty() {
        panic!("empty pt");
    }
    let mut i = pt.len();
    while i > 0 && pt[i - 1] == 0 {
        i -= 1;
    }
    if i == 0 {
        panic!("all padding");
    }
    (pt[i - 1], pt[..i - 1].to_vec())
}

fn collect_app(records: &[TlsRecord], mut aead: RecAead) -> Vec<u8> {
    let mut out = Vec::new();

    for r in records {
        // Skip unencrypted data
        if r.content_type != ContentType::ApplicationData {
            continue;
        }

        let (hdr, mut c) = parse_record(r);
        match aead.decrypt(&hdr, &mut c) {
            Ok(pt) => {
                let (inner_type, content) = strip_inner(&pt);
                match inner_type {
                    21 => {
                        // alerts
                    }
                    22 => {
                        // post-handshake
                        if for_each_handshake(&content, |hst, body| {
                            if hst == HsType::KeyUpdate {
                                if body.len() != 1 {
                                    return Err("KeyUpdate len != 1".into());
                                }
                                // 0=update_not_requested, 1=update_requested
                                // let req = body[0];

                                aead.update_traffic_secret()?;
                            }
                            Ok(())
                        })
                        .is_err()
                        {
                            // do something???
                        }
                    }
                    23 => {
                        out.extend_from_slice(&content);
                    }
                    _ => {
                        // other
                    }
                }
            }
            Err(_) => {
                continue;
            }
        }
    }

    out
}

fn finished_verify(
    hash: HashType,
    transcript: &[u8],
    verify_data: &[u8],
    traffic_secret: &[u8],
) -> Result<(), String> {
    let (out_len, th) = match hash {
        HashType::Sha256 => (32, sha2::Sha256::digest(transcript).to_vec()),
        HashType::Sha384 => (48, sha2::Sha384::digest(transcript).to_vec()),
    };
    if verify_data.len() != out_len {
        return Err(format!(
            "Finished verify_data len {} != {}",
            verify_data.len(),
            out_len
        ));
    }
    let fk = hkdf_expand(hash, traffic_secret, "finished", &[], out_len)?;
    match hash {
        HashType::Sha256 => {
            let mut mac = <hmac::Hmac<sha2::Sha256> as hmac::Mac>::new_from_slice(&fk)
                .map_err(|_| "hmac key err")?;
            mac.update(&th);
            mac.verify_slice(verify_data)
                .map_err(|_| "Finished verify failed (sha256)")?;
        }
        HashType::Sha384 => {
            let mut mac = <hmac::Hmac<sha2::Sha384> as hmac::Mac>::new_from_slice(&fk)
                .map_err(|_| "hmac key err")?;
            mac.update(&th);
            mac.verify_slice(verify_data)
                .map_err(|_| "Finished verify failed (sha384)")?;
        }
    }
    Ok(())
}

fn gather_hs_streams(
    out: &[TlsRecord],
    inn: &[TlsRecord],
    aead_out: &mut RecAead,
    aead_in: &mut RecAead,
) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut out_clear = Vec::new();
    let mut in_clear = Vec::new();
    let mut out_enc = Vec::new();
    let mut in_enc = Vec::new();

    for r in out {
        match r.content_type {
            ContentType::Handshake => {
                let (_h, mut b) = parse_record(r);
                out_clear.append(&mut b);
            }
            ContentType::ApplicationData => {
                let (h, mut c) = parse_record(r);
                if let Ok(pt) = aead_out.decrypt(&h, &mut c) {
                    let (t, body) = strip_inner(&pt);
                    if t == 22 {
                        out_enc.extend_from_slice(&body);
                    }
                }
            }
            _ => {}
        }
    }
    for r in inn {
        match r.content_type {
            ContentType::Handshake => {
                let (_h, mut b) = parse_record(r);
                in_clear.append(&mut b);
            }
            ContentType::ApplicationData => {
                let (h, mut c) = parse_record(r);
                if let Ok(pt) = aead_in.decrypt(&h, &mut c) {
                    let (t, body) = strip_inner(&pt);
                    if t == 22 {
                        in_enc.extend_from_slice(&body);
                    }
                }
            }
            _ => {}
        }
    }

    (out_clear, in_clear, out_enc, in_enc)
}

fn parse_hs(buf: &[u8]) -> Vec<(u8, Vec<u8>)> {
    let mut i = 0;
    let mut out = Vec::new();

    while i + 4 <= buf.len() {
        let t = buf[i];
        let len =
            ((buf[i + 1] as usize) << 16) | ((buf[i + 2] as usize) << 8) | (buf[i + 3] as usize);

        i += 4;

        if i + len > buf.len() {
            panic!("hs truncated");
        }

        let body = buf[i..i + len].to_vec();

        i += len;

        let mut full = Vec::with_capacity(4 + len);
        full.push(t);
        full.push(((len >> 16) & 0xff) as u8);
        full.push(((len >> 8) & 0xff) as u8);
        full.push((len & 0xff) as u8);
        full.extend_from_slice(&body);

        out.push((t, full));
    }
    out
}

async fn validate_chain(ders: &[Vec<u8>], sni: &[u8]) -> Result<(bool, bool), String> {
    let request = Request::VerifyCertsChain {
        ders: ders.to_vec(),
        sni: sni.to_vec(),
        timestamp: gstd::exec::block_timestamp(),
    }
    .encode();

    let reply = msg::send_bytes_for_reply(BUILTIN_WEBPKI, &request, 0, 0)
        .map_err(|_| "Failed to send message")?
        .await
        .map_err(|_| "Failed to receive reply")?;

    let response = Response::decode(&mut reply.as_slice()).unwrap();

    let out = match response {
        Response::VerifyCertsChain {
            certs_chain_ok,
            dns_ok,
        } => (certs_chain_ok, dns_ok),
        _ => return Err("Unexpected response".into()),
    };

    Ok(out)
}

async fn validate_server_certificate_verify(
    suite_hash: HashType,
    transcript_upto_cv: &[u8],
    cv_body: &[u8],
    leaf_der: &[u8],
) -> Result<(), String> {
    if cv_body.len() < 4 {
        return Err("CertificateVerify too short".into());
    }

    let sig_scheme = u16::from_be_bytes([cv_body[0], cv_body[1]]);
    let sig_len = u16::from_be_bytes([cv_body[2], cv_body[3]]) as usize;

    if cv_body.len() != 4 + sig_len {
        return Err(format!(
            "CertificateVerify malformed: sig_len={} vs body={}",
            sig_len,
            cv_body.len()
        ));
    }

    let sig = &cv_body[4..];

    let th = match suite_hash {
        HashType::Sha256 => sha2::Sha256::digest(transcript_upto_cv).to_vec(),
        HashType::Sha384 => sha2::Sha384::digest(transcript_upto_cv).to_vec(),
    };

    const CONTEXT: &[u8] = b"TLS 1.3, server CertificateVerify";

    let mut signed = Vec::with_capacity(64 + CONTEXT.len() + 1 + th.len());
    signed.extend_from_slice(&[0x20; 64]);
    signed.extend_from_slice(CONTEXT);
    signed.push(0);
    signed.extend_from_slice(&th);

    let request = Request::VerifySignature {
        der: leaf_der.to_vec(),
        message: signed,
        signature: sig.to_vec(),
        algo: sig_scheme,
    }
    .encode();

    let reply = msg::send_bytes_for_reply(BUILTIN_WEBPKI, &request, 0, 0)
        .map_err(|_| "Failed to send message")?
        .await
        .map_err(|_| "Failed to receive reply")?;

    let response = Response::decode(&mut reply.as_slice()).unwrap();

    match response {
        Response::VerifySignature { signature_ok: _ } => Ok(()),
        _ => Err("Unexpected response".into()),
    }
}

#[gstd::async_main]
async fn main() {
    let art: Artifact = msg::load().expect("Could not load Artifact");

    if art.keylog.client_traffic_secrets.is_empty() || art.keylog.server_traffic_secrets.is_empty()
    {
        panic!("missing CLIENT_TRAFFIC_SECRET_0 or SERVER_TRAFFIC_SECRET_0");
    }

    let mk_aead = aead_by_suite(art.info.cipher_suite);

    let client_app_key = art.keylog.client_traffic_secrets[0].secret.clone();
    let server_app_key = art.keylog.server_traffic_secrets[0].secret.clone();

    let _app_out = collect_app(&art.client_records, mk_aead(&client_app_key));
    let _app_in = collect_app(&art.server_records, mk_aead(&server_app_key));

    // Handshake decrypt + Finished check
    let client_hs_key = art
        .keylog
        .client_handshake_traffic_secret
        .expect("missing CLIENT_HANDSHAKE_TRAFFIC_SECRET")
        .secret;

    let server_hs_key = art
        .keylog
        .server_handshake_traffic_secret
        .expect("missing SERVER_HANDSHAKE_TRAFFIC_SECRET")
        .secret;

    let mut aead_out = mk_aead(&client_hs_key);
    let mut aead_in = mk_aead(&server_hs_key);

    let (out_clear, in_clear, out_enc, in_enc) = gather_hs_streams(
        &art.client_records,
        &art.server_records,
        &mut aead_out,
        &mut aead_in,
    );

    let mut transcript = Vec::new();
    transcript.extend_from_slice(&out_clear);
    transcript.extend_from_slice(&in_clear);

    let mut finished_report = FinishedReport::default();
    let mut cert_report = CertReport::default();
    let close_report = CloseReport::default();

    // server Finished
    let mut server_finished: Option<(Vec<u8>, usize)> = None;
    let mut server_cv: Option<(Vec<u8>, usize)> = None; // (cv_body, pre_len_before_cv)

    for (t, full) in parse_hs(&in_enc) {
        if t == 15 {
            // CertificateVerify
            let pre_len_before_cv = transcript.len();
            server_cv = Some((full[4..].to_vec(), pre_len_before_cv));
            transcript.extend_from_slice(&full); // CV also included in transcript
            continue;
        }
        if t == 20 {
            // Finished
            let pre_server_len = transcript.len();
            server_finished = Some((full[4..].to_vec(), pre_server_len));
            break;
        } else {
            transcript.extend_from_slice(&full);
        }
    }

    let (cv_body, pre_len_before_cv) = server_cv.expect("server CertificateVerify not found");

    if let Some((sf, pre_len)) = server_finished {
        if let Err(e) = finished_verify(aead_in.hash, &transcript[..pre_len], &sf, &server_hs_key) {
            panic!("server Finished verify failed: {}", e);
        } else {
            finished_report.server_finished_ok = true;

            let mut full = Vec::with_capacity(4 + sf.len());
            full.extend_from_slice(&[20, 0, 0, sf.len() as u8]);
            full.extend_from_slice(&sf);
            transcript.extend_from_slice(&full);
        }
    } else {
        panic!("server Finished not found");
    }

    // client Finished
    let mut client_finished = None;

    for (t, full) in parse_hs(&out_enc) {
        if t == 20 {
            client_finished = Some(full[4..].to_vec());
            break;
        }
        transcript.extend_from_slice(&full);
    }

    if let Some(cf) = client_finished.as_ref() {
        if let Err(e) = finished_verify(aead_out.hash, &transcript, cf, &client_hs_key) {
            panic!("client Finished verify failed: {}", e);
        } else {
            finished_report.client_finished_ok = true;
        }
    } else {
        panic!("client Finished not found");
    }

    // Certificate chain
    match validate_chain(&art.info.peer_cert_chain, &art.info.sni).await {
        Ok((chain_ok, dns_ok)) => {
            cert_report.chain_valid = chain_ok;
            cert_report.dns_name_valid = dns_ok;
        }
        Err(e) => {
            panic!("cert chain verify error: {}", e);
        }
    }

    let leaf_der = art
        .info
        .peer_cert_chain
        .first()
        .expect("no leaf in peer_cert_chain");

    // CertificateVerify
    match validate_server_certificate_verify(
        aead_in.hash,
        &transcript[..pre_len_before_cv],
        &cv_body,
        leaf_der,
    )
    .await
    {
        Ok(()) => {
            cert_report.cert_verify_valid = true;
        }
        Err(e) => {
            panic!("server CertificateVerify failed: {}", e);
        }
    };

    let report = Report {
        finished: finished_report,
        certs: cert_report,
        close: close_report,
    };

    unsafe {
        static_mut!(VERIFICATIONS)
            .get_or_insert(Default::default())
            .domains
            .entry(art.info.sni.clone())
            .and_modify(|e| e.push(report))
            .or_insert(vec![report]);
    };
}

#[unsafe(no_mangle)]
extern "C" fn init() {
    let verifications = Verifications {
        domains: HashMap::new(),
    };
    unsafe { VERIFICATIONS = Some(verifications) };
}

#[unsafe(no_mangle)]
extern "C" fn state() {
    let sni: Vec<u8> = msg::load().expect("Could not load SNI");

    let state = unsafe {
        static_mut!(VERIFICATIONS)
            .take()
            .expect("State is not initialized")
    };

    let Verifications { domains } = state;

    let payload = domains.get(&sni).cloned().expect("Domain not found");

    msg::reply(payload, 0)
        .expect("Failed to encode or reply with `<AppMetadata as Metadata>::State` from `state()`");
}
