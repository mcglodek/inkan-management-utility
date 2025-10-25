use anyhow::{Context, Result};
use bech32::{self, ToBase32, Variant};
use ethers_core::types::Address;
use ethers_core::utils::keccak256;
use k256::ecdsa::SigningKey;
use rand_core::OsRng;
use serde::Serialize;
use std::{fs, path::PathBuf};

#[allow(non_snake_case)]
#[derive(Serialize)]
pub struct KeyRecord {
    // Ethereum-friendly fields
    pub privateKeyHex: String,                 // 0x + 32-byte hex
    pub publicKeyUncompressed0x04: String,     // 0x04 || X || Y
    pub publicKeyCompressed: String,           // 0x02/0x03 || X (33 bytes)
    pub address: String,                       // lowercase 0x…

    // Nostr-style convenience fields (raw hex)
    pub privateKeyHexNostrFormat: String,      // 32-byte hex, no 0x
    pub publicKeyHexNostrFormat: String,       // 32-byte X-only hex

    // Nostr bech32 encodings (NIP-19)
    pub nsec: String,                          // bech32 of 32-byte privkey
    pub npub: String,                          // bech32 of 32-byte x-only pubkey
}

pub fn generate(count: u32) -> Result<Vec<KeyRecord>> {
    let mut out: Vec<KeyRecord> = Vec::with_capacity(count as usize);

    for _ in 0..count {
        // Generate a fresh secp256k1 keypair
        let sk = SigningKey::random(&mut OsRng);

        // Private key bytes/hex (32 bytes)
        let sk_bytes = sk.to_bytes();
        let private_hex_no0x = hex::encode(sk_bytes);
        let private_hex_0x = format!("0x{}", private_hex_no0x);

        // Public keys
        let vk = sk.verifying_key();

        // Uncompressed (0x04 || X || Y) — 65 bytes
        let uncompressed = vk.to_encoded_point(false);
        let pub_uncompressed_hex = format!("0x{}", hex::encode(uncompressed.as_bytes()));

        // Compressed (0x02/0x03 || X) — 33 bytes
        let compressed = vk.to_encoded_point(true);
        let compressed_bytes = compressed.as_bytes();
        let pub_compressed_hex = format!("0x{}", hex::encode(compressed_bytes));

        // Nostr-style x-only pubkey: drop the first prefix byte (02/03), keep 32-byte X
        let nostr_pub_x_only = &compressed_bytes[1..]; // [1..33], 32 bytes
        let nostr_pub_x_only_hex = hex::encode(nostr_pub_x_only);

        // NIP-19 bech32 encodings
        let nsec = bech32::encode("nsec", sk_bytes.to_base32(), Variant::Bech32)?;
        let npub = bech32::encode("npub", nostr_pub_x_only.to_base32(), Variant::Bech32)?;

        // Ethereum address from uncompressed pubkey: keccak256(X||Y) last 20 bytes
        let xy = &uncompressed.as_bytes()[1..]; // drop 0x04
        let hash = keccak256(xy);
        let addr = Address::from_slice(&hash[12..]);
        let address_lower = format!("{:#x}", addr); // lowercase 0x…

        out.push(KeyRecord {
            privateKeyHex: private_hex_0x,
            publicKeyUncompressed0x04: pub_uncompressed_hex,
            publicKeyCompressed: pub_compressed_hex,
            address: address_lower,
            privateKeyHexNostrFormat: private_hex_no0x,
            publicKeyHexNostrFormat: nostr_pub_x_only_hex,
            nsec,
            npub,
        });
    }

    Ok(out)
}

pub fn emit(records: Vec<KeyRecord>, out: Option<PathBuf>) -> Result<()> {
    if let Some(p) = out {
        let json = serde_json::to_string_pretty(&records)?;
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(&p, json).with_context(|| format!("writing {}", p.display()))?;
        println!("✓ Wrote {}", p.display());
    } else {
        println!("{}", serde_json::to_string_pretty(&records)?);
    }
    Ok(())
}

