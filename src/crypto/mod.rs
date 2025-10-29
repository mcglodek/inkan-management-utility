pub mod modern;
pub mod nostr_utils;
pub mod pgp;
pub mod payload; // ⬅️ add this line

use zeroize::Zeroize;

/// Optional helper type if you ever want to pass computed forms around.
/// (Not required by the saver; it recomputes from the secret.)
#[derive(Debug, Clone)]
pub struct GeneratedKeypair {
    pub privkey32: [u8; 32],            // secret key (zeroized on drop)
    pub eth_pub_uncompressed: [u8; 65], // 0x04 + X(32) + Y(32)
    pub nostr_xonly32: [u8; 32],        // x-only (X coord)
}

impl Drop for GeneratedKeypair {
    fn drop(&mut self) {
        self.privkey32.zeroize();
    }
}

