use anyhow::{anyhow, Context, Result};
use serde_json;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::types::{BatchEntryOut, DecodedOne, DecodedTxOut};

/// Write N signed transactions to a file as a JSON array.
/// - If the file already exists, creates a unique variant like "file (1).txt".
/// - `pretty = true` → pretty printed (human-readable), but still 100% processable.
/// - `pretty = false` → compact JSON (no extra whitespace).
pub fn write_signed_transactions_to_file<P: AsRef<Path>>(
    out_path: P,
    entries: &[BatchEntryOut],
    pretty: bool,
) -> Result<PathBuf> {
    let out_path = out_path.as_ref();

    // Ensure parent directory exists
    if let Some(parent) = out_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating parent directory {}", parent.display()))?;
        }
    }

    // Pick a unique filename (avoid overwrite)
    let (mut f, final_path) = create_unique_file(out_path)?;

    // Serialize once (fail early if needed)
    let json = if pretty {
        serde_json::to_string_pretty(entries)?
    } else {
        serde_json::to_string(entries)?
    };

    f.write_all(json.as_bytes())
        .with_context(|| format!("writing {}", final_path.display()))?;
    f.flush()?;
    Ok(final_path)
}

/// Convenience: write a single signed transaction as a one-element JSON array.
/// Returns the actual path written (unique name if needed).
pub fn write_single_signed_transaction<P: AsRef<Path>>(
    out_path: P,
    entry: &BatchEntryOut,
    pretty: bool,
) -> Result<PathBuf> {
    write_signed_transactions_to_file(out_path, std::slice::from_ref(entry), pretty)
}

/// Build a generic, human-readable filename for any signed transaction.
pub fn build_filename_for_any_tx(decoded: &DecodedTxOut) -> String {
    // 1) Simple Delegation
    if let Some(DecodedOne::Delegation(a)) = decoded.decodedData.as_ref() {
        if let (Ok(delg_x), Ok(dele_x)) = (
            x_coord_hex_from_uncompressed(&a.delegatorPubkey),
            x_coord_hex_from_uncompressed(&a.delegateePubkey),
        ) {
            return format!(
                "{}_delegates_to_{}_nonce_{}.txt",
                abbrev_64_hex(&delg_x),
                abbrev_64_hex(&dele_x),
                decoded.nonce
            );
        }
    }

    // 2) Simple Revocation
    if let Some(DecodedOne::Revocation(b)) = decoded.decodedData.as_ref() {
        if let (Ok(revoker_x), Ok(revokee_x)) = (
            x_coord_hex_from_uncompressed(&b.revokerPubkey),
            x_coord_hex_from_uncompressed(&b.revokeePubkey),
        ) {
            return format!(
                "{}_revokes_from_{}_nonce_{}.txt",
                abbrev_64_hex(&revoker_x),
                abbrev_64_hex(&revokee_x),
                decoded.nonce
            );
        }
    }

    // 3) Permanent Invalidation
    if let Some(DecodedOne::Invalidation(i)) = decoded.decodedData.as_ref() {
        if let Ok(x) = x_coord_hex_from_uncompressed(&i.invalidatedPubkey) {
            return format!("invalidate_{}_nonce_{}.txt", abbrev_64_hex(&x), decoded.nonce);
        }
    }

    // 4) REDELEGATION (Revocation + Delegation combo)

    if decoded.funcName == "createRevocationEventFollowedByDelegationEvent" {
        if let (Some(a), Some(b)) = (decoded.decodedDataTypeA.as_ref(), decoded.decodedDataTypeB.as_ref()) {
            if let (Ok(revoker_x), Ok(revokee_x), Ok(delegatee_x)) = (
                x_coord_hex_from_uncompressed(&b.revokerPubkey),
                x_coord_hex_from_uncompressed(&b.revokeePubkey),
                x_coord_hex_from_uncompressed(&a.delegateePubkey),
            ) {
                return format!(
                    "{}_revokes_from_{}_delegates_to_{}_nonce_{}.txt",
                    abbrev_64_hex(&revoker_x),
                    abbrev_64_hex(&revokee_x),
                    abbrev_64_hex(&delegatee_x),
                    decoded.nonce
                );
            }
        }
    }

    // 5) Fallback
    format!("{}_nonce_{}.txt", decoded.funcName, decoded.nonce)
}


/// Extract the 32-byte X coordinate (64 hex chars) from an uncompressed pubkey hex.
/// Accepts "0x04..." or "04..." (hex), must be 65 bytes = 130 hex chars.
fn x_coord_hex_from_uncompressed(uncompressed_hex: &str) -> Result<String> {
    let h = uncompressed_hex.strip_prefix("0x").unwrap_or(uncompressed_hex);
    if !h.starts_with("04") || h.len() != 130 {
        return Err(anyhow!(
            "expected uncompressed pubkey (0x04 + X(64) + Y(64)), got: {} (len={})",
            uncompressed_hex,
            uncompressed_hex.len()
        ));
    }
    Ok(h[2..66].to_ascii_lowercase())
}

/// Abbreviate a 64-char hex string as "first8..last8".
fn abbrev_64_hex(x64: &str) -> String {
    if x64.len() >= 16 {
        format!("{}..{}", &x64[..8], &x64[x64.len() - 8..])
    } else {
        x64.to_string()
    }
}

/// Create a file with a unique name, avoiding overwrite by appending " (1)", " (2)", etc.
fn create_unique_file(path: &Path) -> io::Result<(File, PathBuf)> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    for i in 0..10_000 {
        let candidate_name = if i == 0 {
            if ext.is_empty() {
                stem.to_string()
            } else {
                format!("{stem}.{ext}")
            }
        } else if ext.is_empty() {
            format!("{stem} ({i})")
        } else {
            format!("{stem} ({i}).{ext}")
        };
        let candidate_path = dir.join(&candidate_name);

        match OpenOptions::new().write(true).create_new(true).open(&candidate_path) {
            Ok(f) => return Ok((f, candidate_path)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "failed to create a unique filename after many attempts",
    ))
}

/// Generate a sibling temporary filename for atomic writes (no longer needed but kept for reference).
#[allow(dead_code)]
fn sibling_tmp_path(target: &Path) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let file_name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("tmp.json");
    let tmp_name = format!("{}.{}.tmp", file_name, ts);
    let mut p = target.to_path_buf();
    p.set_file_name(tmp_name);
    p
}
