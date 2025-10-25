use crate::util::bytes_to_0x;
use ethers_signers::LocalWallet;

/// Get uncompressed pubkey (0x04 + x + y) from a wallet
pub fn uncompressed_pubkey_0x04(wallet: &LocalWallet) -> String {
    let vk = wallet.signer().verifying_key();
    let pt = vk.to_encoded_point(false); // uncompressed
    bytes_to_0x(pt.as_bytes())
}

