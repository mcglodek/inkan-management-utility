use crate::crypto::nostr_utils::{npub_from_xonly32, nsec_from_sk32};

use secp256k1::{PublicKey, SecretKey};
use serde::Serialize;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use zeroize::Zeroize;

use sequoia_openpgp as openpgp;
use openpgp::crypto::Password;
use openpgp::serialize::stream::{Encryptor2, LiteralWriter, Message};
use openpgp::types::SymmetricAlgorithm;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct OrderedPayload<'a> {
    key_pair_nickname: &'a str,
    private_key_hex: String,
    private_key_nsec: String,
    public_key_hex_uncompressed: String,
    public_key_compressed: String,
    public_key_npub: String,
}

/// Save as a binary OpenPGP message using symmetric encryption (AEAD-capable).
/// `privkey_hex_no0x` must be 32-byte hex without `0x`.
pub fn save_pgp_encrypted_from_privkey_hex(
    privkey_hex_no0x: &str,
    nickname: &str,
    password_utf8: &mut Vec<u8>,
    file_path: &str,
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

    let uncompressed65 = pk.serialize_uncompressed();
    let compressed33 = pk.serialize();
    let x_only: [u8; 32] = uncompressed65[1..33].try_into().unwrap();

    // 3) Pretty ordered JSON (same order/fields as Modern)
    let payload = OrderedPayload {
        key_pair_nickname: nickname,
        private_key_hex: hex::encode(sk_bytes),
        private_key_nsec: nsec_from_sk32(&sk_bytes),
        public_key_hex_uncompressed: hex::encode(uncompressed65),
        public_key_compressed: hex::encode(compressed33),
        public_key_npub: npub_from_xonly32(&x_only),
    };
    let data = serde_json::to_vec_pretty(&payload).expect("serialize payload");

    // 4) Build OpenPGP symmetric message (Sequoia chooses modern packet formats).


// sanitize nickname
let safe_nickname = {
    let s: String = nickname
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if s.is_empty() { "Keypair".to_string() } else { s }
};

// decide base directory even if a file path was provided
let provided = Path::new(file_path);
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

// enforce standardized filename + .pgp extension
let filename = format!("SECRET_KEEP_AIRGAPPED_{}_Private_Key.pgp", safe_nickname);
let out_path = base_dir.join(filename);

// open file
let f = File::create(&out_path)?;
let mut w = BufWriter::new(f);


    let pass = Password::from(password_utf8.clone());

    let message = Message::new(&mut w);
    let message = Encryptor2::with_passwords(message, [pass])
        .symmetric_algo(SymmetricAlgorithm::AES256)
        // NOTE: No explicit .aead(...); the builder on this version does not expose it.
        // Sequoia will pick appropriate modern formats automatically.
        .build()
        .map_err(|e| io_err(format!("pgp encryptor build: {e}")))?;

    // Literal data packet containing our JSON payload.
    let mut literal = LiteralWriter::new(message)
        .build()
        .map_err(|e| io_err(format!("pgp literal: {e}")))?;
    literal.write_all(&data)?;
    literal
        .finalize()
        .map_err(|e| io_err(format!("pgp finalize: {e}")))?;

    // 5) Zeroize
    password_utf8.zeroize();
    sk_bytes.zeroize();

    Ok(())
}

fn io_err<M: Into<String>>(msg: M) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg.into())
}
