use serde::Serialize;
use secp256k1::{PublicKey, SecretKey};
use tiny_keccak::{Hasher, Keccak};
use crate::crypto::nostr_utils::{npub_from_xonly32, nsec_from_sk32};

/// JSON payload with **exact field order**, all hex values 0x-prefixed,
/// and the Ethereum address labeled as `eth_address`.
#[derive(Serialize)]
pub struct OrderedPayload<'a> {
    pub key_pair_nickname: &'a str,
    pub private_key_hex: String,          // 0x-prefixed
    pub private_key_nsec: String,
    pub public_key_hex_uncompressed: String, // 0x-prefixed
    pub public_key_hex_compressed: String,   // 0x-prefixed
    pub public_key_npub: String,
    pub eth_address: String,              // 0x-prefixed, lowercase
}

/// Build the ordered, **pretty** JSON string from a 32-byte secret key.
///
/// - `key_pair_nickname` is included as a borrowed `&str`
/// - All hex values are 0x-prefixed
/// - `eth_address` is derived as Keccak256(uncompressed[1..]) last 20 bytes
pub fn build_payload_pretty_from_sk<'a>(
    key_pair_nickname: &'a str,
    sk_bytes: &[u8; 32],
) -> anyhow::Result<String> {
    // Validate secret key
    let sec = SecretKey::from_slice(sk_bytes)?;

    // Public keys
    let secp = secp256k1::Secp256k1::new();
    let pk = PublicKey::from_secret_key(&secp, &sec);

    let uncompressed65 = pk.serialize_uncompressed(); // 65 bytes: 0x04 || X || Y
    let compressed33 = pk.serialize();                // 33 bytes
    let x_only: [u8; 32] = uncompressed65[1..33]
        .try_into()
        .expect("slice to 32");

    // Derive Ethereum address
    let mut hasher = Keccak::v256();
    let mut hash = [0u8; 32];
    hasher.update(&uncompressed65[1..]); // X||Y (64 bytes)
    hasher.finalize(&mut hash);
    let address_bytes = &hash[12..]; // last 20 bytes
    let eth_address = format!("0x{}", hex::encode(address_bytes));

    // Build payload with consistent 0x-prefixes
    let payload = OrderedPayload {
        key_pair_nickname,
        private_key_hex: format!("0x{}", hex::encode(sk_bytes)),
        private_key_nsec: nsec_from_sk32(sk_bytes),
        public_key_hex_uncompressed: format!("0x{}", hex::encode(uncompressed65)),
        public_key_hex_compressed: format!("0x{}", hex::encode(compressed33)),
        public_key_npub: npub_from_xonly32(&x_only),
        eth_address,
    };

    let s = serde_json::to_string_pretty(&payload)?;
    Ok(s)
}
