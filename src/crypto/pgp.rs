use crate::crypto::nostr_utils::{npub_from_xonly32, nsec_from_sk32};

use secp256k1::{PublicKey, SecretKey};
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::io::ErrorKind;
use zeroize::Zeroize;

use sequoia_openpgp as openpgp;
use openpgp::crypto::Password;
use openpgp::serialize::stream::{Encryptor2, LiteralWriter, Message};
use openpgp::types::SymmetricAlgorithm;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
struct OrderedPayload<'a> {
    key_pair_nickname: &'a str,
    private_key_hex: String,
    private_key_nsec: String,
    public_key_hex_uncompressed: String,
    public_key_hex_compressed: String,
    public_key_npub: String,
}

/// Create a file with a unique name, avoiding overwrite by appending " (1)", " (2)", ...
fn create_unique_file(base_dir: &Path, filename: &str) -> io::Result<(File, PathBuf)> {
    // Split stem and extension (e.g. "Foo.pgp" -> ("Foo", "pgp"))
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext  = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    // Try the base name first, then " (1)", " (2)", ...
    for i in 0..10_000 {
        let candidate_name = if i == 0 {
            // No suffix on the first try
            if ext.is_empty() { stem.to_string() } else { format!("{stem}.{ext}") }
        } else {
            if ext.is_empty() { format!("{stem} ({i})") } else { format!("{stem} ({i}).{ext}") }
        };
        let path = base_dir.join(&candidate_name);

        match OpenOptions::new()
            .write(true)
            .create_new(true) // <-- never overwrite; fail if exists
            .open(&path)
        {
            Ok(f) => return Ok((f, path)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue, // try next suffix
            Err(e) => return Err(e),
        }
    }

    Err(io_err("failed to create a unique filename after many attempts"))
}

/// Save as a binary OpenPGP message using symmetric encryption (legacy-compatible).
/// `privkey_hex_no0x` must be 32-byte hex without `0x`.
/// RETURNS: PathBuf of the actual file written.
pub fn save_pgp_encrypted_from_privkey_hex(
    privkey_hex_no0x: &str,
    nickname: &str,
    password_utf8: &mut Vec<u8>,
    file_path: &str,
) -> io::Result<PathBuf> {
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
        public_key_hex_compressed: hex::encode(compressed33),
        public_key_npub: npub_from_xonly32(&x_only),
    };
    let data = serde_json::to_vec_pretty(&payload).expect("serialize payload");

    // 4) Resolve output directory and base filename
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

    // standard base filename (without suffix); uniqueness handled by create_unique_file()
    let base_filename = format!("SECRET_KEEP_AIRGAPPED_{}_Private_Key.pgp", safe_nickname);

    // 5) Open a uniquely named file (no overwrite) and remember the final path
    let (f, final_path) = create_unique_file(&base_dir, &base_filename)?;
    let mut w = BufWriter::new(f);

    // 6) Encrypt (legacy-compatible: no explicit AEAD call)
    let pass = Password::from(password_utf8.clone());
    let message = Message::new(&mut w);
    let message = Encryptor2::with_passwords(message, [pass])
        .symmetric_algo(SymmetricAlgorithm::AES256)
        // NOTE: No explicit AEAD; this yields SEIP (CFB+MDC) that gpg & sq can decrypt today.
        .build()
        .map_err(|e| io_err(format!("pgp encryptor build: {e}")))?;

    // 7) Literal data packet containing our JSON payload.
    let mut literal = LiteralWriter::new(message)
        .build()
        .map_err(|e| io_err(format!("pgp literal: {e}")))?;
    literal.write_all(&data)?;
    literal
        .finalize()
        .map_err(|e| io_err(format!("pgp finalize: {e}")))?;

    // 8) Zeroize
    password_utf8.zeroize();
    sk_bytes.zeroize();

    // 9) Return the actual final path for UI display
    Ok(final_path)
}

fn io_err<M: Into<String>>(msg: M) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg.into())
}
