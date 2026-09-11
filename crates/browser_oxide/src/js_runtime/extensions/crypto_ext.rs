//! Web Crypto API ops — digest, random bytes, HMAC, PBKDF2. Backs the JS-side
//! `crypto.subtle` stub so scripts that hash payloads via
//! `crypto.subtle.digest("SHA-256", ...)` see a real result.

use boring2::symm::{decrypt_aead, encrypt_aead, Cipher};
use deno_core::op2;
use serde::Serialize;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha384, Sha512};

#[derive(Serialize)]
pub struct AesGcmResult {
    pub ok: bool,
    pub data: Vec<u8>,
}

fn aes_gcm_cipher(key_len: usize) -> Option<Cipher> {
    match key_len {
        16 => Some(Cipher::aes_128_gcm()),
        24 => Some(Cipher::aes_192_gcm()),
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
        op_crypto_random_fill
    ],
);

#[cfg(test)]
mod tests {
    use super::{
        aes_gcm_decrypt_bytes, aes_gcm_encrypt_bytes, hkdf_bytes, hmac_bytes, pbkdf2_bytes,
    };

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
}
