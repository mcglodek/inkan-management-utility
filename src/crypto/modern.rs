use crate::crypto::nostr_utils::{npub_from_xonly32, nsec_from_sk32};

use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::XChaCha20Poly1305;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use secp256k1::{PublicKey, SecretKey};
use serde::Serialize;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use zeroize::Zeroize;

const VERSION: u8 = 1;
const KDF_ID_ARGON2ID: u8 = 1;

/// Options for the modern saver.
pub struct ModernOptions<'a> {
    pub file_path: &'a str,
    pub key_pair_nickname: &'a str,
    /// Password bytes (UTF-8). This will be zeroized here.
    pub password_utf8: &'a mut Vec<u8>,
    /// Argon2id params
    pub t_cost: u32,       // iterations
    pub m_cost_kib: u32,   // memory in KiB
    pub p_cost: u8,        // parallelism
    /// If true, include 8 bytes of random preface noise to look like ciphertext.
    pub add_noise_prefix: bool,
}

/// JSON payload with **exact field order** you requested.
#[derive(Serialize)]
struct OrderedPayload<'a> {
    key_pair_nickname: &'a str,
    private_key_hex: String,
    private_key_nsec: String,
    public_key_hex_uncompressed: String,
    public_key_compressed: String,
    public_key_npub: String,
}

/// Encrypts and writes a **single** private key (hex, no `0x`) to file using the neutral header.
/// Recomputes all public forms from the private key to ensure internal consistency.
pub fn save_modern_encrypted_from_privkey_hex(
    privkey_hex_no0x: &str,
    opts: ModernOptions<'_>,
) -> io::Result<()> {
    // 1) Decode privkey (32 bytes)
    let sk_bytes_vec = hex::decode(privkey_hex_no0x)
        .map_err(|e| io_err(format!("bad privkey hex: {e}")))?;
    if sk_bytes_vec.len() != 32 {
        return Err(io_err("privkey must be 32 bytes"));
    }
    let mut sk_bytes = [0u8; 32];
    sk_bytes.copy_from_slice(&sk_bytes_vec);

    // 2) Derive public keys
    let sk = SecretKey::from_slice(&sk_bytes)
        .map_err(|e| io_err(format!("invalid secret key: {e}")))?;
    let pk = PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &sk);

    let uncompressed65 = pk.serialize_uncompressed(); // 65 bytes: 0x04 || X || Y
    let compressed33 = pk.serialize();                // 33 bytes
    let x_only: [u8; 32] = uncompressed65[1..33].try_into().unwrap();

    // 3) Build ordered, pretty JSON payload
    let payload = OrderedPayload {
        key_pair_nickname: opts.key_pair_nickname,
        private_key_hex: hex::encode(sk_bytes),
        private_key_nsec: nsec_from_sk32(&sk_bytes),
        public_key_hex_uncompressed: hex::encode(uncompressed65),
        public_key_compressed: hex::encode(compressed33),
        public_key_npub: npub_from_xonly32(&x_only),
    };
    let payload_pretty = serde_json::to_string_pretty(&payload)
        .expect("serialize payload");

    // 4) KDF: Argon2id -> 32-byte key
    let mut rng = ChaCha20Rng::from_entropy();
    let mut salt = vec![0u8; 16];
    rng.fill_bytes(&mut salt);

    let argon = Argon2::new_with_secret(
        &[],
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(opts.m_cost_kib, opts.t_cost, opts.p_cost as u32, None)
            .expect("argon2 params"),
    ).expect("argon2 ctor");

let mut key = [0u8; 32];
argon
    .hash_password_into(opts.password_utf8, &salt, &mut key)
    .map_err(|e| io_err(format!("Argon2 error: {e}")))?;


    // 5) Nonce and header
    let mut nonce = [0u8; 24];
    rng.fill_bytes(&mut nonce);

    // Header layout (neutral, no branding):
    // [8B noise?][u8 version][u8 kdf_id][u32 t_cost][u32 m_cost_kib][u8 p_cost]
    // [u8 salt_len][salt][u8 nonce_len=24][nonce]
    let mut header = Vec::with_capacity(
        (if opts.add_noise_prefix { 8 } else { 0 })
        + 1 + 1 + 4 + 4 + 1 + 1 + salt.len() + 1 + nonce.len()
    );

    if opts.add_noise_prefix {
        let mut noise = [0u8; 8];
        rng.fill_bytes(&mut noise);
        header.extend_from_slice(&noise);
    }

    header.push(VERSION);                                    // u8
    header.push(KDF_ID_ARGON2ID);                            // u8
    header.extend_from_slice(&opts.t_cost.to_le_bytes());    // u32
    header.extend_from_slice(&opts.m_cost_kib.to_le_bytes()); // u32
    header.push(opts.p_cost);                                 // u8
    header.push(salt.len() as u8);                            // u8
    header.extend_from_slice(&salt);                          // salt
    header.push(nonce.len() as u8);                           // u8
    header.extend_from_slice(&nonce);                         // nonce

    // 6) Encrypt (AAD = header)
    let cipher = XChaCha20Poly1305::new((&key).into());
    let ciphertext = cipher
        .encrypt((&nonce).into(), Payload { aad: &header, msg: payload_pretty.as_bytes() })
        .map_err(|e| io_err(format!("encrypt error: {e}")))?;

// 7) Build filename and write file: [header || ciphertext]
use std::fs;
use std::path::{Path, PathBuf};

// sanitize nickname
let safe_nickname = {
    let s: String = opts
        .key_pair_nickname
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if s.is_empty() { "Keypair".to_string() } else { s }
};

// decide base directory even if a file path was provided
let provided = Path::new(opts.file_path);
let base_dir: PathBuf = if provided.is_dir() {
    provided.to_path_buf()
} else if let Some(parent) = provided.parent() {
    parent.to_path_buf()
} else {
    PathBuf::from(".")
};

// ensure directory exists
fs::create_dir_all(&base_dir)
    .map_err(|e| io_err(format!("create dir {}: {e}", base_dir.display())))?;

// enforce standardized filename + .enc extension
let filename = format!("SECRET_KEEP_AIRGAPPED_{}_Private_Key.enc", safe_nickname);
let out_path = base_dir.join(filename);

// write file
let f = File::create(&out_path)?;
let mut w = BufWriter::new(f);
w.write_all(&header)?;
w.write_all(&ciphertext)?;
w.flush()?;



    // 8) Zeroize sensitive buffers
    key.zeroize();
    salt.zeroize();
    opts.password_utf8.zeroize();
    sk_bytes.zeroize();

    Ok(())
}

fn io_err<M: Into<String>>(msg: M) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg.into())
}

