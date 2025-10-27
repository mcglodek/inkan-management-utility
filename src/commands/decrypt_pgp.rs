use anyhow::{Context, Result};
use sequoia_openpgp as openpgp;

use openpgp::crypto::{Password, SessionKey};
use openpgp::packet::{PKESK, SKESK};
use openpgp::parse::Parse; // brings DecryptorBuilder::from_reader into scope
use openpgp::parse::stream::{DecryptorBuilder, DecryptionHelper, MessageStructure, VerificationHelper};
use openpgp::policy::StandardPolicy;
use openpgp::{Fingerprint, KeyHandle}; // <-- FIX: from crate root
use openpgp::types::SymmetricAlgorithm;

use std::fs::File;
use std::io::{BufReader, Read};

/// Helper that supplies the passphrase for SKESK (symmetric) packets.
struct SymmetricHelper {
    password: Password,
}

impl DecryptionHelper for SymmetricHelper {
    fn decrypt<D>(
        &mut self,
        _pkesks: &[PKESK],
        skesks: &[SKESK],
        _sym_algo: Option<SymmetricAlgorithm>,
        mut decrypt: D,
    ) -> openpgp::Result<Option<Fingerprint>>
    where
        D: FnMut(SymmetricAlgorithm, &SessionKey) -> bool,
    {
        for skesk in skesks {
            if let Ok((algo, session_key)) = skesk.decrypt(&self.password) {
                if decrypt(algo, &session_key) {
                    // Symmetric decryption has no recipient cert -> return None.
                    return Ok(None);
                }
            }
        }
        Ok(None)
    }
}

impl VerificationHelper for SymmetricHelper {
    fn get_certs(&mut self, _ids: &[KeyHandle]) -> openpgp::Result<Vec<openpgp::Cert>> {
        // No signature verification for this flow.
        Ok(Vec::new())
    }

    fn check(&mut self, _structure: MessageStructure) -> openpgp::Result<()> {
        // No policy enforcement for signatures.
        Ok(())
    }
}

/// Attempt to decrypt an OpenPGP symmetrically-encrypted file (SKESK) using Sequoia (pure Rust).
/// Returns plaintext bytes on success, or Err if the file is not PGP or the password is wrong.
pub fn try_decrypt_pgp(input_path: &std::path::Path, password_utf8: &mut Vec<u8>) -> Result<Vec<u8>> {
    let f = File::open(input_path).with_context(|| format!("opening {}", input_path.display()))?;
    let mut reader = BufReader::new(f);

    let policy = &StandardPolicy::new();

    // Build the streaming decryptor with our helper. If your version prefers it,
    // replace `from_reader` with `from_buffered_reader`.
    let helper = SymmetricHelper {
        password: Password::from(password_utf8.clone()),
    };
    let mut decryptor = DecryptorBuilder::from_reader(&mut reader)?
        .with_policy(policy, None, helper)?;

    let mut out = Vec::new();
    decryptor.read_to_end(&mut out).context("PGP symmetric decryption failed")?;
    Ok(out)
}
