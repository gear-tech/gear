#!/usr/bin/env python3
"""
Independent Python reference implementation of ethexe-tpke for cross-impl KAT.

Generates test vectors that the Rust crate must reproduce bit-for-bit:
  - hash_to_G1(id) → Q_id
  - master pub AggPub = S · g₂, share pubs PSᵢ = Sᵢ · g₂
  - ephemeral U = u · g₂
  - z = e(Q_id, AggPub)^u (in GT)
  - HKDF-SHA256(z, info=...) → (key, nonce)
  - ChaCha20-Poly1305(key, plaintext, aad) → body
  - decryption shares Dᵢ = Sᵢ · Q_id

A passing Rust test against these vectors means the Rust impl matches the
construction in the design doc independently of arkworks' specific
hash-to-curve / pairing internals.

Run:
    /tmp/tpke_kat_venv/bin/python3 ethexe/tpke/tests/kat/generate.py \
        > ethexe/tpke/tests/kat/vectors.json
"""

import hashlib
import json
import sys
from dataclasses import dataclass

from py_ecc.bls.hash_to_curve import hash_to_G1
from py_ecc.optimized_bls12_381 import (
    G1,
    G2,
    multiply,
    pairing,
    normalize,
    add,
    Z1,
    Z2,
    FQ,
    FQ2,
    curve_order,
)
import hmac

# -------- Cryptography deps (separate from py_ecc) -----------------------
# ChaCha20-Poly1305: use cryptography package (already common).
try:
    from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
except ImportError:
    print("ERROR: pip install cryptography", file=sys.stderr)
    sys.exit(1)

# -------- Constants (must match Rust) ------------------------------------
DST_G1 = b"ETHEXE-TPKE-V1-BLS12381G1_XMD:SHA-256_SSWU_RO_"
HKDF_DEM_INFO = b"ethexe-tpke-dem-v1"
ID_DOMAIN = b"ethexe-tpke-v1"
DEM_KEY_LEN = 32
DEM_NONCE_LEN = 12


def derive_id(chain_id: int, key_epoch_id: int, plaintext: bytes, user_nonce: bytes) -> bytes:
    """Blake2b-256 of the canonical id-binding input. Mirrors Rust derive_id()."""
    import hashlib
    h = hashlib.blake2b(digest_size=32)
    h.update(ID_DOMAIN)
    h.update(chain_id.to_bytes(8, "little"))
    h.update(key_epoch_id.to_bytes(4, "little"))
    h.update(plaintext)
    h.update(user_nonce)
    return h.digest()


def hkdf_extract_expand(salt: bytes, ikm: bytes, info: bytes, length: int) -> bytes:
    """HKDF-SHA256(salt, ikm, info, length). Salt None ≡ zero-bytes."""
    if not salt:
        salt = b"\x00" * 32
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    t = b""
    out = b""
    counter = 1
    while len(out) < length:
        t = hmac.new(prk, t + info + bytes([counter]), hashlib.sha256).digest()
        out += t
        counter += 1
    return out[:length]


# -------- BLS12-381 helpers (compressed encoding per zcash spec) ---------
# IETF BLS12-381 compressed point encoding:
#   G1 point: 48 bytes. Top 3 bits encode flags (compression=1, infinity, sign_y).
#   G2 point: 96 bytes. Same flag layout in MSB of first byte.
# y-sign: take the larger lexicographic representation of y or -y.
# We match arkworks' CanonicalSerialize::serialize_compressed for BLS12-381,
# which follows the zcash/zkcrypto encoding (compatible with ETH2 / EIP-2537).

from py_ecc.fields import field_properties as _fp
P = _fp["bls12_381"]["field_modulus"]


def _fq_to_bytes(x: int) -> bytes:
    return x.to_bytes(48, "big")


def serialize_g1_compressed(P_aff) -> bytes:
    """Compress a normalized G1 affine point per zcash/eth2 BLS spec."""
    if P_aff is None or P_aff == Z1:
        # Point at infinity: all-zero except compression+infinity bits.
        out = bytearray(48)
        out[0] = 0b11000000
        return bytes(out)
    x, y = normalize(P_aff)
    x_int = x.n
    y_int = y.n
    # y-sign: 1 if y > (p - y); arkworks/zkcrypto uses "y is lex-greater" convention.
    y_neg = P - y_int
    sign = 1 if y_int > y_neg else 0
    out = bytearray(_fq_to_bytes(x_int))
    out[0] |= 0b10000000  # compression flag
    if sign:
        out[0] |= 0b00100000
    return bytes(out)


def serialize_g2_compressed(P_aff) -> bytes:
    """Compress a normalized G2 affine point. G2 coords are Fq2 (c0 + c1·i)."""
    if P_aff is None or P_aff == Z2:
        out = bytearray(96)
        out[0] = 0b11000000
        return bytes(out)
    x, y = normalize(P_aff)
    # FQ2 stores coefficients in y.coeffs = (c0, c1) per py_ecc.
    x_c0, x_c1 = x.coeffs[0], x.coeffs[1]
    y_c0, y_c1 = y.coeffs[0], y.coeffs[1]
    # y-sign: compare (y_c1, y_c0) lex-greater vs (-y_c1, -y_c0).
    neg_y_c1 = (P - y_c1) % P
    neg_y_c0 = (P - y_c0) % P
    if y_c1 > neg_y_c1:
        sign = 1
    elif y_c1 < neg_y_c1:
        sign = 0
    else:
        sign = 1 if y_c0 > neg_y_c0 else 0
    # G2 compressed: x_c1 || x_c0 (big endian Fq each)
    out = bytearray(_fq_to_bytes(x_c1) + _fq_to_bytes(x_c0))
    out[0] |= 0b10000000
    if sign:
        out[0] |= 0b00100000
    return bytes(out)


def serialize_gt_compressed(z) -> bytes:
    """Serialize a GT (Fq12) element. arkworks uses 576 bytes uncompressed,
    288 bytes for compressed (one coeff dropped via reconstruction).
    For KAT we serialize all 12 Fq coefficients in big-endian, 48 bytes each = 576 bytes.
    This matches arkworks' CanonicalSerialize::serialize_compressed for PairingOutput<Bls12_381>
    only if compression yields the same bytes; if not, the Rust KAT test should compare
    the HKDF *output* rather than the GT serialization.

    SIMPLEST CROSS-CHECK PATH: emit ciphertext body + shares, let Rust decrypt them.
    The internal GT serialization need not match — only the derived AEAD key+nonce
    need to be identical, and HKDF input format must agree.

    For the KAT we choose: serialize GT via py_ecc's 12-Fq big-endian flattening
    (576 B) AND require Rust to match the same canonical encoding. ark-bls12-381's
    CanonicalSerialize for Fq12 emits 12 Fq elements little-endian, NOT big-endian.

    Therefore: we DO NOT cross-check GT bytes. Instead we cross-check the final
    AEAD ciphertext: encrypt in Python, decrypt in Rust. If both impls compute the
    same z and the same HKDF(z) → same key → same body, the Rust decrypt of the
    Python body must succeed.
    """
    # Arkworks Fq12 compressed serialization: 12 Fq elements little-endian.
    # py_ecc FQ12 has .coeffs as a tuple of 12 ints.
    coeffs = z.coeffs
    out = bytearray()
    for c in coeffs:
        out += int(c).to_bytes(48, "little")
    return bytes(out)


# -------- Test vector generation -----------------------------------------
@dataclass
class TestVector:
    label: str
    chain_id: int
    key_epoch_id: int
    threshold: int
    n: int
    master_secret_hex: str       # Sᵢ derived from polynomial evaluation
    poly_coeffs_hex: list        # f(x) coefficients, low-degree first
    plaintext_hex: str
    user_nonce_hex: str
    u_scalar_hex: str
    id_hex: str
    master_pub_compressed_hex: str   # 96 bytes
    share_pubs_compressed_hex: list  # [(index, 96B hex)]
    secret_shares_hex: list          # [(index, 32B scalar hex)] — for Rust to feed in
    envelope_u_hex: str              # 96 bytes
    envelope_body_hex: str           # ChaCha20Poly1305 ciphertext (incl. tag)
    expected_decryption_shares_hex: list  # [(index, 48B hex)]


def eval_poly(coeffs: list, x: int, mod: int) -> int:
    """Horner-evaluate f(x) = a_0 + a_1·x + ... + a_{t-1}·x^{t-1} mod `mod`."""
    acc = coeffs[-1]
    for c in reversed(coeffs[:-1]):
        acc = (acc * x + c) % mod
    return acc


def generate_vector(label: str, chain_id: int, key_epoch_id: int, threshold: int, n: int,
                    plaintext: bytes, user_nonce: bytes,
                    poly_seed: int, u_scalar_seed: int) -> dict:
    """Generate one fully-resolved test vector deterministically."""
    # Deterministic polynomial coefficients in Fr.
    rng = hashlib.shake_128(poly_seed.to_bytes(8, "little")).digest(64 * threshold)
    coeffs = []
    for i in range(threshold):
        c = int.from_bytes(rng[i*64:(i+1)*64], "little") % curve_order
        coeffs.append(c)

    master_secret = coeffs[0]
    # Compute Sᵢ = f(i) for i in 1..=n.
    secret_shares = [(i, eval_poly(coeffs, i, curve_order)) for i in range(1, n + 1)]

    # Master pub & share pubs in G2.
    master_pub = multiply(G2, master_secret)
    share_pubs = [(idx, multiply(G2, s)) for (idx, s) in secret_shares]

    # u scalar deterministic.
    u_rng = hashlib.shake_128(u_scalar_seed.to_bytes(8, "little")).digest(64)
    u_scalar = int.from_bytes(u_rng, "little") % curve_order

    # id.
    id_bytes = derive_id(chain_id, key_epoch_id, plaintext, user_nonce)

    # Q_id = hash_to_G1(id, DST).
    q_id = hash_to_G1(id_bytes, DST_G1, hash_function=hashlib.sha256)

    # U = u · g₂.
    u_point = multiply(G2, u_scalar)
    u_bytes = serialize_g2_compressed(u_point)

    # z = e(Q_id, AggPub)^u
    z_base = pairing(master_pub, q_id)  # py_ecc: pairing(G2_pt, G1_pt)
    # Pairing returns an element of GT (FQ12). Exponentiation by u_scalar:
    z = z_base ** u_scalar

    # AEAD key + nonce derivation (HKDF-SHA256(z_bytes, info)).
    z_bytes = serialize_gt_compressed(z)
    info = HKDF_DEM_INFO + id_bytes + u_bytes
    okm = hkdf_extract_expand(b"", z_bytes, info, DEM_KEY_LEN + DEM_NONCE_LEN)
    key = okm[:DEM_KEY_LEN]
    nonce = okm[DEM_KEY_LEN:]

    # AAD = id ‖ U_bytes ‖ chain_id_le ‖ key_epoch_id_le
    aad = id_bytes + u_bytes + chain_id.to_bytes(8, "little") + key_epoch_id.to_bytes(4, "little")

    aead = ChaCha20Poly1305(key)
    body = aead.encrypt(nonce, plaintext, aad)

    # Decryption shares Dᵢ = Sᵢ · Q_id.
    decryption_shares = [(idx, multiply(q_id, s)) for (idx, s) in secret_shares]

    return {
        "label": label,
        "chain_id": chain_id,
        "key_epoch_id": key_epoch_id,
        "threshold": threshold,
        "n": n,
        "plaintext_hex": plaintext.hex(),
        "user_nonce_hex": user_nonce.hex(),
        "u_scalar_hex": u_scalar.to_bytes(32, "big").hex(),
        "master_secret_hex": master_secret.to_bytes(32, "big").hex(),
        "poly_coeffs_hex": [c.to_bytes(32, "big").hex() for c in coeffs],
        "id_hex": id_bytes.hex(),
        "master_pub_compressed_hex": serialize_g2_compressed(master_pub).hex(),
        "share_pubs_compressed_hex": [
            {"index": idx, "bytes_hex": serialize_g2_compressed(pt).hex()}
            for (idx, pt) in share_pubs
        ],
        "secret_shares_hex": [
            {"index": idx, "scalar_hex": s.to_bytes(32, "big").hex()}
            for (idx, s) in secret_shares
        ],
        "envelope_u_hex": u_bytes.hex(),
        "envelope_body_hex": body.hex(),
        "expected_decryption_shares_hex": [
            {"index": idx, "bytes_hex": serialize_g1_compressed(pt).hex()}
            for (idx, pt) in decryption_shares
        ],
    }


def main():
    vectors = []
    # Several scenarios that exercise different code paths.
    vectors.append(generate_vector("basic-3-of-5", 1, 0, 3, 5,
                                   b"hello world",
                                   bytes.fromhex("11" * 32),
                                   poly_seed=0xC0FFEE,
                                   u_scalar_seed=0xBEEF))
    vectors.append(generate_vector("empty-plaintext-2-of-3", 7, 2, 2, 3,
                                   b"",
                                   bytes.fromhex("22" * 32),
                                   poly_seed=0xDEAD,
                                   u_scalar_seed=0xBABE))
    vectors.append(generate_vector("large-4-of-7", 100, 42, 4, 7,
                                   b"A" * 256,
                                   bytes.fromhex("33" * 32),
                                   poly_seed=0xCAFE,
                                   u_scalar_seed=0xF00D))
    out = {"version": "v1", "dst": DST_G1.decode(), "vectors": vectors}
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
