use anyhow::Result;
use std::path::PathBuf;

use crate::crypto::modern::{save_modern_encrypted_from_privkey_hex, ModernOptions};
use crate::crypto::pgp::save_pgp_encrypted_from_privkey_hex;

use super::keygen::KeyRecord;

pub struct EncryptedSaveOptions<'a> {
    pub out_path: &'a str,
    pub nickname: &'a str,
    /// UTF-8 password bytes. Will be zeroized inside the saver.
    pub password_utf8: &'a mut Vec<u8>,
    /// Argon2id params (Modern)
    pub argon_t_cost: u32,
    pub argon_m_cost_kib: u32,
    pub argon_p_cost: u8,
    /// Add the 8-byte random noise prefix to the header (Modern)
    pub add_noise_prefix: bool,
}

/// Modern neutral-header writer (Argon2id + XChaCha20-Poly1305, ordered pretty JSON).
/// RETURNS: PathBuf of the actual file written.
pub fn emit_encrypted_one_modern(record: &KeyRecord, opts: EncryptedSaveOptions<'_>) -> Result<PathBuf> {
    let modern = ModernOptions {
        file_path: opts.out_path,
        key_pair_nickname: opts.nickname,
        password_utf8: opts.password_utf8,
        t_cost: opts.argon_t_cost,
        m_cost_kib: opts.argon_m_cost_kib,
        p_cost: opts.argon_p_cost,
        add_noise_prefix: opts.add_noise_prefix,
    };
    let final_path = save_modern_encrypted_from_privkey_hex(&record.privateKeyHexNostrFormat, modern)?;
    Ok(final_path)
}

/// PGP-compat writer (Sequoia AEAD/OCB), same ordered pretty JSON inside.
/// RETURNS: PathBuf of the actual file written.
pub fn emit_encrypted_one_pgp(
    record: &KeyRecord,
    out_path: &str,
    nickname: &str,
    password_utf8: &mut Vec<u8>,
) -> Result<PathBuf> {
    let final_path = save_pgp_encrypted_from_privkey_hex(
        &record.privateKeyHexNostrFormat,
        nickname,
        password_utf8,
        out_path,
    )?;
    Ok(final_path)
}
