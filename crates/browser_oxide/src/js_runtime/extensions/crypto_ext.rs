//! Web Crypto API ops — digest, random bytes, HMAC, PBKDF2. Backs the JS-side
//! `crypto.subtle` stub so scripts that hash payloads via
//! `crypto.subtle.digest("SHA-256", ...)` see a real result.

use boring2::aes::{unwrap_key, wrap_key, AesKey};
use boring2::bn::{BigNum, BigNumContext};
use boring2::derive::Deriver;
use boring2::ec::{EcGroup, EcKey, EcPoint, PointConversionForm};
use boring2::ecdsa::EcdsaSig;
use boring2::nid::Nid;
use boring2::pkey::{PKey, Private, Public};
use boring2::rsa::{Padding, Rsa};
use boring2::symm::{decrypt, decrypt_aead, encrypt, encrypt_aead, Cipher};
use deno_core::op2;
use serde::Serialize;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha384, Sha512};

#[derive(Serialize, Default)]
pub struct EcKeyMaterial {
    pub ok: bool,
    pub public_spki: Vec<u8>,
    pub private_pkcs8: Vec<u8>,
    pub raw_public: Vec<u8>,
    pub x: Vec<u8>,
    pub y: Vec<u8>,
    pub d: Vec<u8>,
}

#[derive(Serialize, Default)]
pub struct RsaKeyMaterial {
    pub ok: bool,
    pub public_spki: Vec<u8>,
    pub private_pkcs8: Vec<u8>,
    pub modulus_length: u32,
    pub public_exponent: Vec<u8>,
}

fn rsa_material_private(pkey: PKey<Private>) -> Option<RsaKeyMaterial> {
    let rsa = pkey.rsa().ok()?;
    Some(RsaKeyMaterial {
        ok: true,
        public_spki: pkey.public_key_to_der().ok()?,
        private_pkcs8: pkey.private_key_to_der_pkcs8().ok()?,
        modulus_length: rsa.n().num_bits() as u32,
        public_exponent: rsa.e().to_vec(),
    })
}

fn rsa_material_public(pkey: PKey<Public>) -> Option<RsaKeyMaterial> {
    let rsa = pkey.rsa().ok()?;
    Some(RsaKeyMaterial {
        ok: true,
        public_spki: pkey.public_key_to_der().ok()?,
        private_pkcs8: Vec::new(),
        modulus_length: rsa.n().num_bits() as u32,
        public_exponent: rsa.e().to_vec(),
    })
}

fn rsa_generate(modulus_length: u32, public_exponent: &[u8]) -> Option<RsaKeyMaterial> {
    if modulus_length < 256 || public_exponent.is_empty() {
        return None;
    }
    let exponent = BigNum::from_slice(public_exponent).ok()?;
    let rsa = Rsa::generate_with_e(modulus_length, &exponent).ok()?;
    rsa_material_private(PKey::from_rsa(rsa).ok()?)
}

fn rsa_import_spki(der: &[u8]) -> Option<RsaKeyMaterial> {
    rsa_material_public(PKey::public_key_from_der(der).ok()?)
}

fn rsa_import_pkcs8(der: &[u8]) -> Option<RsaKeyMaterial> {
    rsa_material_private(PKey::private_key_from_pkcs8(der).ok()?)
}

fn ec_curve(name: &str) -> Option<(Nid, usize)> {
    match name {
        "P-256" => Some((Nid::X9_62_PRIME256V1, 32)),
        "P-384" => Some((Nid::SECP384R1, 48)),
        "P-521" => Some((Nid::SECP521R1, 66)),
        _ => None,
    }
}

fn ec_public_parts<T>(ec: &EcKey<T>, coord_len: usize) -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)>
where
    T: boring2::pkey::HasPublic,
{
    let group = ec.group();
    let mut ctx = BigNumContext::new().ok()?;
    let raw = ec
        .public_key()
        .to_bytes(group, PointConversionForm::UNCOMPRESSED, &mut ctx)
        .ok()?;
    let mut x = BigNum::new().ok()?;
    let mut y = BigNum::new().ok()?;
    ec.public_key()
        .affine_coordinates_gfp(group, &mut x, &mut y, &mut ctx)
        .ok()?;
    Some((
        raw,
        x.to_vec_padded(coord_len).ok()?,
        y.to_vec_padded(coord_len).ok()?,
    ))
}

fn ec_material_private(curve: &str, pkey: PKey<Private>) -> Option<EcKeyMaterial> {
    let (nid, coord_len) = ec_curve(curve)?;
    let ec = pkey.ec_key().ok()?;
    if ec.group().curve_name()? != nid || ec.check_key().is_err() {
        return None;
    }
    let (raw_public, x, y) = ec_public_parts(&ec, coord_len)?;
    let d = ec.private_key().to_vec_padded(coord_len).ok()?;
    Some(EcKeyMaterial {
        ok: true,
        public_spki: pkey.public_key_to_der().ok()?,
        private_pkcs8: pkey.private_key_to_der_pkcs8().ok()?,
        raw_public,
        x,
        y,
        d,
    })
}

fn ec_material_public(curve: &str, pkey: PKey<Public>) -> Option<EcKeyMaterial> {
    let (nid, coord_len) = ec_curve(curve)?;
    let ec = pkey.ec_key().ok()?;
    if ec.group().curve_name()? != nid || ec.check_key().is_err() {
        return None;
    }
    let (raw_public, x, y) = ec_public_parts(&ec, coord_len)?;
    Some(EcKeyMaterial {
        ok: true,
        public_spki: pkey.public_key_to_der().ok()?,
        private_pkcs8: Vec::new(),
        raw_public,
        x,
        y,
        d: Vec::new(),
    })
}

fn ec_generate(curve: &str) -> Option<EcKeyMaterial> {
    let (nid, _) = ec_curve(curve)?;
    let group = EcGroup::from_curve_name(nid).ok()?;
    let ec = EcKey::generate(&group).ok()?;
    let pkey = PKey::from_ec_key(ec).ok()?;
    ec_material_private(curve, pkey)
}

fn ec_import_raw(curve: &str, raw: &[u8]) -> Option<EcKeyMaterial> {
    let (nid, _) = ec_curve(curve)?;
    let group = EcGroup::from_curve_name(nid).ok()?;
    let mut ctx = BigNumContext::new().ok()?;
    let point = EcPoint::from_bytes(&group, raw, &mut ctx).ok()?;
    let ec = EcKey::from_public_key(&group, &point).ok()?;
    let pkey = PKey::from_ec_key(ec).ok()?;
    ec_material_public(curve, pkey)
}

fn ec_import_spki(curve: &str, der: &[u8]) -> Option<EcKeyMaterial> {
    ec_material_public(curve, PKey::public_key_from_der(der).ok()?)
}

fn ec_import_pkcs8(curve: &str, der: &[u8]) -> Option<EcKeyMaterial> {
    ec_material_private(curve, PKey::private_key_from_pkcs8(der).ok()?)
}

fn ec_import_jwk(curve: &str, x: &[u8], y: &[u8], d: &[u8]) -> Option<EcKeyMaterial> {
    let (nid, coord_len) = ec_curve(curve)?;
    if x.len() != coord_len || y.len() != coord_len || (!d.is_empty() && d.len() != coord_len) {
        return None;
    }
    let group = EcGroup::from_curve_name(nid).ok()?;
    let mut raw = Vec::with_capacity(1 + coord_len * 2);
    raw.push(4);
    raw.extend_from_slice(x);
    raw.extend_from_slice(y);
    let mut ctx = BigNumContext::new().ok()?;
    let point = EcPoint::from_bytes(&group, &raw, &mut ctx).ok()?;
    if d.is_empty() {
        let ec = EcKey::from_public_key(&group, &point).ok()?;
        return ec_material_public(curve, PKey::from_ec_key(ec).ok()?);
    }
    let private = BigNum::from_slice(d).ok()?;
    let ec = EcKey::from_private_components(&group, &private, &point).ok()?;
    ec_material_private(curve, PKey::from_ec_key(ec).ok()?)
}

fn ec_sign(curve: &str, hash: &str, pkcs8: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let (_, coord_len) = ec_curve(curve)?;
    let pkey = PKey::private_key_from_pkcs8(pkcs8).ok()?;
    let ec = pkey.ec_key().ok()?;
    let digest = digest_bytes(hash, data)?;
    let sig = EcdsaSig::sign(&digest, &ec).ok()?;
    let mut out = sig.r().to_vec_padded(coord_len).ok()?;
    out.extend_from_slice(&sig.s().to_vec_padded(coord_len).ok()?);
    Some(out)
}

fn ec_verify(curve: &str, hash: &str, spki: &[u8], signature: &[u8], data: &[u8]) -> Option<bool> {
    let (_, coord_len) = ec_curve(curve)?;
    if signature.len() != coord_len * 2 {
        return Some(false);
    }
    let pkey = PKey::public_key_from_der(spki).ok()?;
    let ec = pkey.ec_key().ok()?;
    let digest = digest_bytes(hash, data)?;
    let r = BigNum::from_slice(&signature[..coord_len]).ok()?;
    let s = BigNum::from_slice(&signature[coord_len..]).ok()?;
    EcdsaSig::from_private_components(r, s)
        .ok()?
        .verify(&digest, &ec)
        .ok()
}

fn ec_derive(private_pkcs8: &[u8], public_spki: &[u8]) -> Option<Vec<u8>> {
    let private = PKey::private_key_from_pkcs8(private_pkcs8).ok()?;
    let public = PKey::public_key_from_der(public_spki).ok()?;
    let mut deriver = Deriver::new(&private).ok()?;
    deriver.set_peer(&public).ok()?;
    deriver.derive_to_vec().ok()
}

#[derive(Serialize)]
pub struct AesGcmResult {
    pub ok: bool,
    pub data: Vec<u8>,
}

fn aes_cbc_cipher(key_len: usize) -> Option<Cipher> {
    match key_len {
        16 => Some(Cipher::aes_128_cbc()),
        32 => Some(Cipher::aes_256_cbc()),
        _ => None,
    }
}

fn aes_cbc_encrypt_bytes(key: &[u8], iv: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if iv.len() != 16 {
        return None;
    }
    encrypt(aes_cbc_cipher(key.len())?, key, Some(iv), data).ok()
}

fn aes_cbc_decrypt_bytes(key: &[u8], iv: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if iv.len() != 16 || data.is_empty() || !data.len().is_multiple_of(16) {
        return None;
    }
    decrypt(aes_cbc_cipher(key.len())?, key, Some(iv), data).ok()
}

fn aes_gcm_cipher(key_len: usize) -> Option<Cipher> {
    match key_len {
        16 => Some(Cipher::aes_128_gcm()),
        32 => Some(Cipher::aes_256_gcm()),
        _ => None,
    }
}

fn valid_gcm_tag_len(tag_len: usize) -> bool {
    matches!(tag_len, 4 | 8 | 12 | 13 | 14 | 15 | 16)
}

fn aes_gcm_encrypt_bytes(
    key: &[u8],
    iv: &[u8],
    aad: &[u8],
    data: &[u8],
    tag_len: usize,
) -> Option<Vec<u8>> {
    let cipher = aes_gcm_cipher(key.len())?;
    if !valid_gcm_tag_len(tag_len) {
        return None;
    }
    let mut tag = vec![0u8; tag_len];
    let mut encrypted = encrypt_aead(cipher, key, Some(iv), aad, data, &mut tag).ok()?;
    encrypted.extend_from_slice(&tag);
    Some(encrypted)
}

fn aes_gcm_decrypt_bytes(
    key: &[u8],
    iv: &[u8],
    aad: &[u8],
    data: &[u8],
    tag_len: usize,
) -> Option<Vec<u8>> {
    let cipher = aes_gcm_cipher(key.len())?;
    if !valid_gcm_tag_len(tag_len) || data.len() < tag_len {
        return None;
    }
    let split = data.len() - tag_len;
    let (ciphertext, tag) = data.split_at(split);
    decrypt_aead(cipher, key, Some(iv), aad, ciphertext, tag).ok()
}

fn aes_kw_wrap_bytes(key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if !matches!(key.len(), 16 | 24 | 32) || data.len() < 16 || !data.len().is_multiple_of(8) {
        return None;
    }
    let aes = AesKey::new_encrypt(key).ok()?;
    let mut out = vec![0u8; data.len() + 8];
    let written = wrap_key(&aes, None, &mut out, data).ok()?;
    out.truncate(written);
    Some(out)
}

fn aes_kw_unwrap_bytes(key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if !matches!(key.len(), 16 | 24 | 32) || data.len() < 24 || !data.len().is_multiple_of(8) {
        return None;
    }
    let aes = AesKey::new_decrypt(key).ok()?;
    let mut out = vec![0u8; data.len() - 8];
    let written = unwrap_key(&aes, None, &mut out, data).ok()?;
    out.truncate(written);
    Some(out)
}

fn digest_bytes(algorithm: &str, data: &[u8]) -> Option<Vec<u8>> {
    let alg = algorithm.to_ascii_uppercase();
    let out = match alg.as_str() {
        "SHA-1" | "SHA1" => {
            let mut h = Sha1::new();
            h.update(data);
            h.finalize().to_vec()
        }
        "SHA-256" | "SHA256" => {
            let mut h = Sha256::new();
            h.update(data);
            h.finalize().to_vec()
        }
        "SHA-384" | "SHA384" => {
            let mut h = Sha384::new();
            h.update(data);
            h.finalize().to_vec()
        }
        "SHA-512" | "SHA512" => {
            let mut h = Sha512::new();
            h.update(data);
            h.finalize().to_vec()
        }
        _ => return None,
    };
    Some(out)
}

fn mgf1(hash: &str, seed: &[u8], output_len: usize) -> Option<Vec<u8>> {
    let digest_len = digest_bytes(hash, &[])?.len();
    if digest_len == 0 {
        return None;
    }
    let block_count = output_len.div_ceil(digest_len);
    if block_count > u32::MAX as usize {
        return None;
    }
    let mut output = Vec::with_capacity(block_count * digest_len);
    for counter in 0..block_count {
        let mut input = Vec::with_capacity(seed.len() + 4);
        input.extend_from_slice(seed);
        input.extend_from_slice(&(counter as u32).to_be_bytes());
        output.extend_from_slice(&digest_bytes(hash, &input)?);
    }
    output.truncate(output_len);
    Some(output)
}

fn rsa_oaep_encode(hash: &str, label: &[u8], message: &[u8], k: usize) -> Option<Vec<u8>> {
    let label_hash = digest_bytes(hash, label)?;
    let hash_len = label_hash.len();
    if k < 2 * hash_len + 2 || message.len() > k - 2 * hash_len - 2 {
        return None;
    }

    let padding_len = k - message.len() - 2 * hash_len - 2;
    let mut db = Vec::with_capacity(k - hash_len - 1);
    db.extend_from_slice(&label_hash);
    db.resize(hash_len + padding_len, 0);
    db.push(1);
    db.extend_from_slice(message);

    let mut seed = vec![0u8; hash_len];
    {
        use rand::Rng;
        rand::rng().fill_bytes(&mut seed);
    }
    let db_mask = mgf1(hash, &seed, db.len())?;
    let masked_db: Vec<u8> = db.iter().zip(&db_mask).map(|(a, b)| a ^ b).collect();
    let seed_mask = mgf1(hash, &masked_db, hash_len)?;
    let masked_seed: Vec<u8> = seed.iter().zip(&seed_mask).map(|(a, b)| a ^ b).collect();

    let mut encoded = Vec::with_capacity(k);
    encoded.push(0);
    encoded.extend_from_slice(&masked_seed);
    encoded.extend_from_slice(&masked_db);
    Some(encoded)
}

fn rsa_oaep_decode(hash: &str, label: &[u8], encoded: &[u8]) -> Option<Vec<u8>> {
    let label_hash = digest_bytes(hash, label)?;
    let hash_len = label_hash.len();
    if encoded.len() < 2 * hash_len + 2 || encoded.first().copied()? != 0 {
        return None;
    }

    let masked_seed = &encoded[1..1 + hash_len];
    let masked_db = &encoded[1 + hash_len..];
    let seed_mask = mgf1(hash, masked_db, hash_len)?;
    let seed: Vec<u8> = masked_seed
        .iter()
        .zip(&seed_mask)
        .map(|(a, b)| a ^ b)
        .collect();
    let db_mask = mgf1(hash, &seed, masked_db.len())?;
    let db: Vec<u8> = masked_db.iter().zip(&db_mask).map(|(a, b)| a ^ b).collect();
    if db.len() < hash_len || db[..hash_len] != label_hash[..] {
        return None;
    }

    let mut index = hash_len;
    while index < db.len() && db[index] == 0 {
        index += 1;
    }
    if index >= db.len() || db[index] != 1 {
        return None;
    }
    Some(db[index + 1..].to_vec())
}

fn rsa_oaep_encrypt_bytes(
    hash: &str,
    public_spki: &[u8],
    label: &[u8],
    data: &[u8],
) -> Option<Vec<u8>> {
    let pkey = PKey::public_key_from_der(public_spki).ok()?;
    let rsa = pkey.rsa().ok()?;
    let key_len = rsa.size() as usize;
    let encoded = rsa_oaep_encode(hash, label, data, key_len)?;
    let mut output = vec![0u8; key_len];
    let written = rsa
        .public_encrypt(&encoded, &mut output, Padding::NONE)
        .ok()?;
    output.truncate(written);
    Some(output)
}

fn rsa_oaep_decrypt_bytes(
    hash: &str,
    private_pkcs8: &[u8],
    label: &[u8],
    data: &[u8],
) -> Option<Vec<u8>> {
    let pkey = PKey::private_key_from_pkcs8(private_pkcs8).ok()?;
    let rsa = pkey.rsa().ok()?;
    let key_len = rsa.size() as usize;
    if data.len() != key_len {
        return None;
    }
    let mut encoded = vec![0u8; key_len];
    let written = rsa
        .private_decrypt(data, &mut encoded, Padding::NONE)
        .ok()?;
    if written > key_len {
        return None;
    }
    if written < key_len {
        encoded.copy_within(..written, key_len - written);
        encoded[..key_len - written].fill(0);
    }
    rsa_oaep_decode(hash, label, &encoded)
}

fn hmac_bytes(algorithm: &str, key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let alg = algorithm.to_ascii_uppercase();
    let block_size = match alg.as_str() {
        "SHA-1" | "SHA1" | "SHA-256" | "SHA256" => 64,
        "SHA-384" | "SHA384" | "SHA-512" | "SHA512" => 128,
        _ => return None,
    };

    let mut normalized_key = vec![0u8; block_size];
    if key.len() > block_size {
        let hashed = digest_bytes(&alg, key)?;
        normalized_key[..hashed.len()].copy_from_slice(&hashed);
    } else {
        normalized_key[..key.len()].copy_from_slice(key);
    }

    let mut inner = Vec::with_capacity(block_size + data.len());
    inner.extend(normalized_key.iter().map(|b| b ^ 0x36));
    inner.extend_from_slice(data);
    let inner_digest = digest_bytes(&alg, &inner)?;

    let mut outer = Vec::with_capacity(block_size + inner_digest.len());
    outer.extend(normalized_key.iter().map(|b| b ^ 0x5c));
    outer.extend_from_slice(&inner_digest);
    digest_bytes(&alg, &outer)
}

fn pbkdf2_bytes(
    algorithm: &str,
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    byte_length: usize,
) -> Option<Vec<u8>> {
    if iterations == 0 {
        return None;
    }
    if byte_length == 0 {
        return Some(Vec::new());
    }

    let digest_len = digest_bytes(algorithm, &[])?.len();
    let block_count = byte_length.div_ceil(digest_len);
    if block_count > u32::MAX as usize {
        return None;
    }

    let mut derived = Vec::with_capacity(block_count * digest_len);
    for block_index in 1..=block_count {
        let mut initial = Vec::with_capacity(salt.len() + 4);
        initial.extend_from_slice(salt);
        initial.extend_from_slice(&(block_index as u32).to_be_bytes());

        let mut u = hmac_bytes(algorithm, password, &initial)?;
        let mut block = u.clone();
        for _ in 1..iterations {
            u = hmac_bytes(algorithm, password, &u)?;
            for (acc, byte) in block.iter_mut().zip(&u) {
                *acc ^= *byte;
            }
        }
        derived.extend_from_slice(&block);
    }
    derived.truncate(byte_length);
    Some(derived)
}

fn hkdf_bytes(
    algorithm: &str,
    ikm: &[u8],
    salt: &[u8],
    info: &[u8],
    byte_length: usize,
) -> Option<Vec<u8>> {
    if byte_length == 0 {
        return Some(Vec::new());
    }

    let digest_len = digest_bytes(algorithm, &[])?.len();
    if byte_length > 255usize.checked_mul(digest_len)? {
        return None;
    }

    // RFC 5869: PRK = HMAC-Hash(salt, IKM), then expand T(1..N).
    // An empty salt is equivalent to HashLen zero bytes; HMAC key
    // normalization makes the empty slice equivalent to that zero key.
    let prk = hmac_bytes(algorithm, salt, ikm)?;
    let block_count = byte_length.div_ceil(digest_len);
    let mut okm = Vec::with_capacity(block_count * digest_len);
    let mut previous = Vec::new();
    for block_index in 1..=block_count {
        let mut input = Vec::with_capacity(previous.len() + info.len() + 1);
        input.extend_from_slice(&previous);
        input.extend_from_slice(info);
        input.push(block_index as u8);
        previous = hmac_bytes(algorithm, &prk, &input)?;
        okm.extend_from_slice(&previous);
    }
    okm.truncate(byte_length);
    Some(okm)
}

#[op2]
#[buffer]
pub fn op_crypto_digest(#[string] algorithm: String, #[buffer] data: &[u8]) -> Vec<u8> {
    digest_bytes(&algorithm, data).unwrap_or_default()
}

#[op2]
#[buffer]
pub fn op_crypto_hmac_sign(
    #[string] algorithm: String,
    #[buffer] key: &[u8],
    #[buffer] data: &[u8],
) -> Vec<u8> {
    hmac_bytes(&algorithm, key, data).unwrap_or_default()
}

#[op2]
#[buffer]
pub fn op_crypto_pbkdf2(
    #[string] algorithm: String,
    #[buffer] password: &[u8],
    #[buffer] salt: &[u8],
    iterations: u32,
    byte_length: u32,
) -> Vec<u8> {
    pbkdf2_bytes(&algorithm, password, salt, iterations, byte_length as usize).unwrap_or_default()
}

#[op2]
#[buffer]
pub fn op_crypto_hkdf(
    #[string] algorithm: String,
    #[buffer] ikm: &[u8],
    #[buffer] salt: &[u8],
    #[buffer] info: &[u8],
    byte_length: u32,
) -> Vec<u8> {
    hkdf_bytes(&algorithm, ikm, salt, info, byte_length as usize).unwrap_or_default()
}

#[op2]
#[serde]
pub fn op_crypto_aes_gcm_encrypt(
    #[buffer] key: &[u8],
    #[buffer] iv: &[u8],
    #[buffer] aad: &[u8],
    #[buffer] data: &[u8],
    tag_len: u32,
) -> AesGcmResult {
    match aes_gcm_encrypt_bytes(key, iv, aad, data, tag_len as usize) {
        Some(data) => AesGcmResult { ok: true, data },
        None => AesGcmResult {
            ok: false,
            data: Vec::new(),
        },
    }
}

#[op2]
#[serde]
pub fn op_crypto_aes_gcm_decrypt(
    #[buffer] key: &[u8],
    #[buffer] iv: &[u8],
    #[buffer] aad: &[u8],
    #[buffer] data: &[u8],
    tag_len: u32,
) -> AesGcmResult {
    match aes_gcm_decrypt_bytes(key, iv, aad, data, tag_len as usize) {
        Some(data) => AesGcmResult { ok: true, data },
        None => AesGcmResult {
            ok: false,
            data: Vec::new(),
        },
    }
}

#[op2]
#[serde]
pub fn op_crypto_aes_cbc_encrypt(
    #[buffer] key: &[u8],
    #[buffer] iv: &[u8],
    #[buffer] data: &[u8],
) -> AesGcmResult {
    match aes_cbc_encrypt_bytes(key, iv, data) {
        Some(data) => AesGcmResult { ok: true, data },
        None => AesGcmResult {
            ok: false,
            data: Vec::new(),
        },
    }
}

#[op2]
#[serde]
pub fn op_crypto_aes_cbc_decrypt(
    #[buffer] key: &[u8],
    #[buffer] iv: &[u8],
    #[buffer] data: &[u8],
) -> AesGcmResult {
    match aes_cbc_decrypt_bytes(key, iv, data) {
        Some(data) => AesGcmResult { ok: true, data },
        None => AesGcmResult {
            ok: false,
            data: Vec::new(),
        },
    }
}

#[op2]
#[serde]
pub fn op_crypto_aes_kw_wrap(#[buffer] key: &[u8], #[buffer] data: &[u8]) -> AesGcmResult {
    match aes_kw_wrap_bytes(key, data) {
        Some(data) => AesGcmResult { ok: true, data },
        None => AesGcmResult {
            ok: false,
            data: Vec::new(),
        },
    }
}

#[op2]
#[serde]
pub fn op_crypto_aes_kw_unwrap(#[buffer] key: &[u8], #[buffer] data: &[u8]) -> AesGcmResult {
    match aes_kw_unwrap_bytes(key, data) {
        Some(data) => AesGcmResult { ok: true, data },
        None => AesGcmResult {
            ok: false,
            data: Vec::new(),
        },
    }
}

#[op2]
#[serde]
pub fn op_crypto_ec_generate(#[string] curve: String) -> EcKeyMaterial {
    ec_generate(&curve).unwrap_or_default()
}

#[op2]
#[serde]
pub fn op_crypto_ec_import(
    #[string] curve: String,
    #[string] format: String,
    #[buffer] data: &[u8],
) -> EcKeyMaterial {
    let material = match format.as_str() {
        "raw" => ec_import_raw(&curve, data),
        "spki" => ec_import_spki(&curve, data),
        "pkcs8" => ec_import_pkcs8(&curve, data),
        _ => None,
    };
    material.unwrap_or_default()
}

#[op2]
#[serde]
pub fn op_crypto_ec_import_jwk(
    #[string] curve: String,
    #[buffer] x: &[u8],
    #[buffer] y: &[u8],
    #[buffer] d: &[u8],
) -> EcKeyMaterial {
    ec_import_jwk(&curve, x, y, d).unwrap_or_default()
}

#[op2]
#[buffer]
pub fn op_crypto_ecdsa_sign(
    #[string] curve: String,
    #[string] hash: String,
    #[buffer] pkcs8: &[u8],
    #[buffer] data: &[u8],
) -> Vec<u8> {
    ec_sign(&curve, &hash, pkcs8, data).unwrap_or_default()
}

#[op2(fast)]
pub fn op_crypto_ecdsa_verify(
    #[string] curve: String,
    #[string] hash: String,
    #[buffer] spki: &[u8],
    #[buffer] signature: &[u8],
    #[buffer] data: &[u8],
) -> bool {
    ec_verify(&curve, &hash, spki, signature, data).unwrap_or(false)
}

#[op2]
#[buffer]
pub fn op_crypto_ecdh_derive(
    #[buffer] private_pkcs8: &[u8],
    #[buffer] public_spki: &[u8],
) -> Vec<u8> {
    ec_derive(private_pkcs8, public_spki).unwrap_or_default()
}

#[op2]
#[serde]
pub fn op_crypto_rsa_generate(
    modulus_length: u32,
    #[buffer] public_exponent: &[u8],
) -> RsaKeyMaterial {
    rsa_generate(modulus_length, public_exponent).unwrap_or_default()
}

#[op2]
#[serde]
pub fn op_crypto_rsa_import(#[string] format: String, #[buffer] data: &[u8]) -> RsaKeyMaterial {
    let material = match format.as_str() {
        "spki" => rsa_import_spki(data),
        "pkcs8" => rsa_import_pkcs8(data),
        _ => None,
    };
    material.unwrap_or_default()
}

#[op2]
#[serde]
pub fn op_crypto_rsa_oaep_encrypt(
    #[string] hash: String,
    #[buffer] public_spki: &[u8],
    #[buffer] label: &[u8],
    #[buffer] data: &[u8],
) -> AesGcmResult {
    match rsa_oaep_encrypt_bytes(&hash, public_spki, label, data) {
        Some(data) => AesGcmResult { ok: true, data },
        None => AesGcmResult {
            ok: false,
            data: Vec::new(),
        },
    }
}

#[op2]
#[serde]
pub fn op_crypto_rsa_oaep_decrypt(
    #[string] hash: String,
    #[buffer] private_pkcs8: &[u8],
    #[buffer] label: &[u8],
    #[buffer] data: &[u8],
) -> AesGcmResult {
    match rsa_oaep_decrypt_bytes(&hash, private_pkcs8, label, data) {
        Some(data) => AesGcmResult { ok: true, data },
        None => AesGcmResult {
            ok: false,
            data: Vec::new(),
        },
    }
}

#[op2(fast)]
pub fn op_crypto_random_fill(#[buffer] out: &mut [u8]) {
    use rand::Rng;
    rand::rng().fill_bytes(out);
}

deno_core::extension!(
    crypto_extension,
    ops = [
        op_crypto_digest,
        op_crypto_hmac_sign,
        op_crypto_pbkdf2,
        op_crypto_hkdf,
        op_crypto_aes_gcm_encrypt,
        op_crypto_aes_gcm_decrypt,
        op_crypto_aes_cbc_encrypt,
        op_crypto_aes_cbc_decrypt,
        op_crypto_aes_kw_wrap,
        op_crypto_aes_kw_unwrap,
        op_crypto_ec_generate,
        op_crypto_ec_import,
        op_crypto_ec_import_jwk,
        op_crypto_ecdsa_sign,
        op_crypto_ecdsa_verify,
        op_crypto_ecdh_derive,
        op_crypto_rsa_generate,
        op_crypto_rsa_import,
        op_crypto_rsa_oaep_encrypt,
        op_crypto_rsa_oaep_decrypt,
        op_crypto_random_fill
    ],
);

#[cfg(test)]
mod tests {
    use super::{
        aes_cbc_decrypt_bytes, aes_cbc_encrypt_bytes, aes_gcm_decrypt_bytes, aes_gcm_encrypt_bytes,
        aes_kw_unwrap_bytes, aes_kw_wrap_bytes, ec_derive, ec_generate, ec_import_jwk,
        ec_import_pkcs8, ec_import_raw, ec_import_spki, ec_sign, ec_verify, hkdf_bytes, hmac_bytes,
        pbkdf2_bytes,
    };

    #[test]
    fn aes_kw_matches_rfc3394_128_bit_vector() {
        let kek = hex::decode("000102030405060708090a0b0c0d0e0f").unwrap();
        let plain = hex::decode("00112233445566778899aabbccddeeff").unwrap();
        let wrapped = aes_kw_wrap_bytes(&kek, &plain).unwrap();
        assert_eq!(
            hex::encode(&wrapped),
            "1fa68b0a8112b447aef34bd8fb5a7b829d3e862371d2cfe5"
        );
        assert_eq!(aes_kw_unwrap_bytes(&kek, &wrapped).unwrap(), plain);

        let mut tampered = wrapped;
        tampered[0] ^= 1;
        assert!(aes_kw_unwrap_bytes(&kek, &tampered).is_none());
    }

    #[test]
    fn hmac_sha256_matches_rfc_style_vector() {
        let mac = hmac_bytes(
            "SHA-256",
            b"key",
            b"The quick brown fox jumps over the lazy dog",
        )
        .unwrap();
        assert_eq!(
            hex::encode(mac),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    #[test]
    fn hmac_hash_block_sizes_match_standard_vectors() {
        let key = [0x0b; 20];
        let data = b"Hi There";
        for (algorithm, expected) in [
            ("SHA-1", "b617318655057264e28bc0b6fb378c8ef146be00"),
            (
                "SHA-256",
                "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
            ),
            (
                "SHA-384",
                "afd03944d84895626b0825f4ab46907f15f9dadbe4101ec682aa034c7cebc59cfaea9ea9076ede7f4af152e8b2fa9cb6",
            ),
            (
                "SHA-512",
                "87aa7cdea5ef619d4ff0b4241a1d6cb02379f4e2ce4ec2787ad0b30545e17cdedaa833b7d6b8a702038b274eaea3f4e4be9d914eeb61f1702e696c203a126854",
            ),
        ] {
            assert_eq!(hex::encode(hmac_bytes(algorithm, &key, data).unwrap()), expected);
        }
    }

    #[test]
    fn pbkdf2_sha1_matches_rfc6070_vectors() {
        for (iterations, expected) in [
            (1, "0c60c80f961f0e71f3a9b524af6012062fe037a6"),
            (2, "ea6c014dc72d6f8ccd1ed92ace1d41f0d8de8957"),
        ] {
            assert_eq!(
                hex::encode(pbkdf2_bytes("SHA-1", b"password", b"salt", iterations, 20).unwrap()),
                expected
            );
        }
    }

    #[test]
    fn pbkdf2_sha256_matches_standard_vectors() {
        for (iterations, expected) in [
            (
                1,
                "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b",
            ),
            (
                2,
                "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43",
            ),
            (
                4096,
                "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a",
            ),
        ] {
            assert_eq!(
                hex::encode(pbkdf2_bytes("SHA-256", b"password", b"salt", iterations, 32).unwrap()),
                expected
            );
        }
    }

    #[test]
    fn hkdf_sha256_matches_rfc5869_case_1() {
        let ikm = [0x0b; 22];
        let salt: Vec<u8> = (0x00..=0x0c).collect();
        let info: Vec<u8> = (0xf0..=0xf9).collect();
        let okm = hkdf_bytes("SHA-256", &ikm, &salt, &info, 42).unwrap();
        assert_eq!(
            hex::encode(okm),
            "3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865"
        );
        assert_eq!(
            hkdf_bytes("SHA-256", &ikm, &salt, &info, 0),
            Some(Vec::new())
        );
        assert!(hkdf_bytes("SHA-256", &ikm, &salt, &info, 255 * 32 + 1).is_none());
    }

    #[test]
    fn aes_128_gcm_matches_nist_zero_vector() {
        let key = [0u8; 16];
        let iv = [0u8; 12];
        let plaintext = [0u8; 16];
        let encrypted = aes_gcm_encrypt_bytes(&key, &iv, &[], &plaintext, 16).unwrap();
        assert_eq!(
            hex::encode(&encrypted),
            "0388dace60b6a392f328c2b971b2fe78ab6e47d42cec13bdf53a67b21257bddf"
        );
        assert_eq!(
            aes_gcm_decrypt_bytes(&key, &iv, &[], &encrypted, 16).unwrap(),
            plaintext
        );
    }

    #[test]
    fn aes_gcm_authentication_failure_is_distinct_from_empty_plaintext() {
        let key = [0x42u8; 32];
        let iv = [0x24u8; 12];
        let aad = b"associated";
        let encrypted = aes_gcm_encrypt_bytes(&key, &iv, aad, b"", 16).unwrap();
        assert_eq!(
            aes_gcm_decrypt_bytes(&key, &iv, aad, &encrypted, 16).unwrap(),
            b""
        );

        let mut tampered = encrypted;
        *tampered.last_mut().unwrap() ^= 1;
        assert!(aes_gcm_decrypt_bytes(&key, &iv, aad, &tampered, 16).is_none());
    }

    #[test]
    fn aes_cbc_matches_chromium_pkcs7_vector_and_rejects_invalid_inputs() {
        let key = [0u8; 16];
        let iv = [0u8; 16];
        let encrypted = aes_cbc_encrypt_bytes(&key, &iv, b"hello").unwrap();
        assert_eq!(hex::encode(&encrypted), "9834ed518cbc8fbe9af3c6ecb75eb8c0");
        assert_eq!(
            aes_cbc_decrypt_bytes(&key, &iv, &encrypted).unwrap(),
            b"hello"
        );

        let mut tampered = encrypted;
        *tampered.last_mut().unwrap() ^= 1;
        assert!(aes_cbc_decrypt_bytes(&key, &iv, &tampered).is_none());
        assert!(aes_cbc_decrypt_bytes(&key, &iv, &[0u8; 15]).is_none());
        assert!(aes_cbc_encrypt_bytes(&[0u8; 24], &iv, b"hello").is_none());
        assert!(aes_cbc_encrypt_bytes(&key, &[0u8; 15], b"hello").is_none());
    }

    #[test]
    fn ecdsa_all_chrome_curves_round_trip_raw_signature() {
        for (curve, hash, sig_len) in [
            ("P-256", "SHA-256", 64usize),
            ("P-384", "SHA-384", 96usize),
            ("P-521", "SHA-512", 132usize),
        ] {
            let material = ec_generate(curve).expect("generate EC key");
            assert!(material.ok);
            let message = b"browser-oxide-ecdsa";
            let signature = ec_sign(curve, hash, &material.private_pkcs8, message).expect("sign");
            assert_eq!(signature.len(), sig_len);
            assert_eq!(
                ec_verify(curve, hash, &material.public_spki, &signature, message),
                Some(true)
            );
            assert!(ec_import_raw(curve, &material.raw_public).is_some());
            assert!(ec_import_spki(curve, &material.public_spki).is_some());
            assert!(ec_import_pkcs8(curve, &material.private_pkcs8).is_some());
            assert!(ec_import_jwk(curve, &material.x, &material.y, &material.d).is_some());
        }
    }

    #[test]
    fn ecdh_all_chrome_curves_derive_symmetric_shared_secret() {
        for (curve, secret_len) in [("P-256", 32usize), ("P-384", 48), ("P-521", 66)] {
            let a = ec_generate(curve).expect("generate EC key A");
            let b = ec_generate(curve).expect("generate EC key B");
            let ab = ec_derive(&a.private_pkcs8, &b.public_spki).expect("derive A->B");
            let ba = ec_derive(&b.private_pkcs8, &a.public_spki).expect("derive B->A");
            assert_eq!(ab.len(), secret_len);
            assert_eq!(ab, ba);
        }
    }
}
