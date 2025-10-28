use anyhow::{anyhow, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

use crate::commands::decrypt_modern::try_decrypt_modern;
use crate::commands::decrypt_pgp::try_decrypt_pgp;

/// Try Modern first, then OpenPGP. Write output as:
/// NOT_ENCRYPTED_DO_NOT_SHARE_[InputFileNameOrStem].json
/// (if the final extension is .enc or .pgp, it is stripped before appending .json).
///
/// Returns (method_label, exact_output_path) on success.
/// Returns Err if both methods fail.
pub fn decrypt_auto(
    input_path: &Path,
    password_utf8: &mut Vec<u8>,
    output_dir: &Path
) -> Result<(String, PathBuf)> {
    // Ensure output directory exists
    fs::create_dir_all(output_dir)
        .with_context(|| format!("creating directory {}", output_dir.display()))?;

    // Attempt 1: Modern
    let mut pwd_modern = password_utf8.clone();
    let modern_res = try_decrypt_modern(input_path, &mut pwd_modern);
    pwd_modern.zeroize(); // zeroize the clone

    // On success -> write & return
    if let Ok(plaintext) = modern_res {
        let out_path = create_unique_path(output_dir, &derive_output_name(input_path));
        write_file(&out_path, &plaintext)?;
        drop(plaintext);
        // Zeroize the original provided password as well
        password_utf8.zeroize();
        return Ok(("Argon2id + XChaCha20-Poly1305".to_string(), out_path));
    }

    // Attempt 2: OpenPGP
    let mut pwd_pgp = password_utf8.clone();
    let pgp_res = try_decrypt_pgp(input_path, &mut pwd_pgp);
    pwd_pgp.zeroize(); // zeroize the clone

    if let Ok(plaintext) = pgp_res {
        let out_path = create_unique_path(output_dir, &derive_output_name(input_path));
        write_file(&out_path, &plaintext)?;
        drop(plaintext);
        password_utf8.zeroize();
        return Ok(("OpenPGP".to_string(), out_path));
    }

    // Zeroize the original anyway before failing
    password_utf8.zeroize();

    Err(anyhow!(
        "Tried both Argon2id + XChaCha20-Poly1305 and OpenPGP and couldn't decrypt with either."
    ))
}

/// Build: NOT_ENCRYPTED_DO_NOT_SHARE_[InputFileName].json
/// If the *final* extension is ".enc" or ".pgp" (case-insensitive), strip it before adding ".json".
fn derive_output_name(input_path: &Path) -> String {
    let fname = input_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("decrypted");

    // Remove only the FINAL .enc/.pgp (case-insensitive) extension.
    let lowered = fname.to_ascii_lowercase();
    let base = if lowered.ends_with(".enc") {
        &fname[..fname.len() - 4]
    } else if lowered.ends_with(".pgp") {
        &fname[..fname.len() - 4]
    } else {
        fname
    };

    format!("CAREFUL_NOT_ENCRYPTED_{}.json", base)
}

/// Create a unique path by appending " (1)", " (2)", ... before the extension if the file exists.
fn create_unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let path = dir.join(file_name);
    if !path.exists() {
        return path;
    }

    // Split into stem and extension to insert counters nicely
    let (stem, ext) = match split_name(file_name) {
        Some((s, e)) => (s.to_string(), e.to_string()),
        None => (file_name.to_string(), "".to_string()),
    };

    for i in 1..10_000 {
        let candidate = if ext.is_empty() {
            format!("{stem} ({i})")
        } else {
            format!("{stem} ({i}).{ext}")
        };
        let p = dir.join(candidate);
        if !p.exists() {
            return p;
        }
    }
    // Fallback with a unique-ish suffix if somehow all taken
    dir.join(format!(
        "{stem} (unique){}",
        if ext.is_empty() { "".into() } else { format!(".{ext}") }
    ))
}

/// Split "name.ext" -> ("name", "ext"), or None if invalid.
fn split_name(name: &str) -> Option<(&str, &str)> {
    let mut it = name.rsplitn(2, '.');
    let ext = it.next()?;
    let stem = it.next()?;
    Some((stem, ext))
}

fn write_file(path: &Path, data: &[u8]) -> Result<()> {
    let f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    let mut w = BufWriter::new(f);
    w.write_all(data)?;
    w.flush()?;
    Ok(())
}
