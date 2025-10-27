use anyhow::{anyhow, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305};
use std::fs;
use std::path::Path;
use zeroize::Zeroize;

const VERSION_EXPECTED: u8 = 1;
const KDF_ID_ARGON2ID: u8 = 1;

#[derive(Debug)]
struct Header {
    header_full: Vec<u8>,  // EXACT bytes used as AAD (includes optional 8B noise)
    version: u8,
    kdf_id: u8,
    t_cost: u32,
    m_cost_kib: u32,
    p_cost: u8,
    salt: Vec<u8>,
    nonce: Vec<u8>,        // must be 24 bytes for XChaCha20-Poly1305
    offset_after_header: usize,
}

/// Attempt to parse a header at the given offset (0 or 8).
fn try_parse_header_at(buf: &[u8], off: usize) -> Option<Header> {
    if buf.len() < off + 2 { return None; }

    let version = buf[off];
    let kdf_id  = buf[off + 1];
    if version != VERSION_EXPECTED || kdf_id != KDF_ID_ARGON2ID {
        return None;
    }

    let mut i = off + 2;

    // u32 t_cost (LE)
    if buf.len() < i + 4 { return None; }
    let t_cost = u32::from_le_bytes(buf[i..i + 4].try_into().ok()?);
    i += 4;

    // u32 m_cost_kib (LE)
    if buf.len() < i + 4 { return None; }
    let m_cost_kib = u32::from_le_bytes(buf[i..i + 4].try_into().ok()?);
    i += 4;

    // u8 p_cost
    if buf.len() < i + 1 { return None; }
    let p_cost = buf[i];
    i += 1;

    // u8 salt_len, then salt
    if buf.len() < i + 1 { return None; }
    let salt_len = buf[i] as usize;
    i += 1;
    if buf.len() < i + salt_len { return None; }
    let salt = buf[i..i + salt_len].to_vec();
    i += salt_len;

    // u8 nonce_len, then nonce
    if buf.len() < i + 1 { return None; }
    let nonce_len = buf[i] as usize;
    i += 1;
    if buf.len() < i + nonce_len { return None; }
    let nonce = buf[i..i + nonce_len].to_vec();
    i += nonce_len;

    let header_full = buf[..i].to_vec(); // AAD is *entire* prefix up to end of parsed header

    Some(Header {
        header_full,
        version,
        kdf_id,
        t_cost,
        m_cost_kib,
        p_cost,
        salt,
        nonce,
        offset_after_header: i,
    })
}

/// Parse the header, trying without noise first, then with 8-byte noise.
fn parse_header(buf: &[u8]) -> Option<Header> {
    if let Some(h) = try_parse_header_at(buf, 0) {
        return Some(h);
    }
    if buf.len() >= 10 {
        if let Some(h) = try_parse_header_at(buf, 8) {
            return Some(h);
        }
    }
    None
}

pub fn try_decrypt_modern(input_path: &Path, password_utf8: &mut Vec<u8>) -> Result<Vec<u8>> {
    let data = fs::read(input_path)
        .with_context(|| format!("reading {}", input_path.display()))?;

    let header = parse_header(&data)
        .ok_or_else(|| anyhow!("Not a recognized modern header (version/kdf/structure mismatch)."))?;

    if header.nonce.len() != 24 {
        return Err(anyhow!(
            "Unexpected nonce length {} (expected 24).",
            header.nonce.len()
        ));
    }

    // Derive key via Argon2id (p_cost is u8 in this format)
    let params = Params::new(header.m_cost_kib, header.t_cost, header.p_cost as u32, None)
        .map_err(|e| anyhow!("invalid Argon2 params: {e}"))?;

    let argon = Argon2::new_with_secret(
        &[], // no secret
        Algorithm::Argon2id,
        Version::V0x13,
        params,
    ).map_err(|e| anyhow!("Argon2 ctor failed: {e}"))?;

    let mut key = [0u8; 32];
    argon.hash_password_into(password_utf8, &header.salt, &mut key)
        .map_err(|e| anyhow!("Argon2 hash_password_into failed: {e}"))?;

    // Decrypt with AAD = exact header bytes (including optional noise prefix)
    let cipher = XChaCha20Poly1305::new((&key).into());
    let nonce = chacha20poly1305::XNonce::from_slice(&header.nonce);

    let ciphertext = &data[header.offset_after_header..];
    if ciphertext.len() < 16 {
        key.zeroize();
        return Err(anyhow!("ciphertext too short (missing tag)"));
    }

    let plaintext = cipher.decrypt(
        nonce,
        Payload {
            aad: &header.header_full,
            msg: ciphertext,
        },
    ).map_err(|_| anyhow!("Modern decrypt failed (wrong password? tampered? params mismatch?)."))?;

    // Zeroize sensitive material
    key.zeroize();
    password_utf8.zeroize();

    Ok(plaintext)
}
