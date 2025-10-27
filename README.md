# Inkan Management Utility

**Purpose:** a self-contained, offline management utility for creating, inspecting, and securely exporting Inkan identity key material. The primary functionality of this release is **secure export** of keypairs using one of two encryption formats. This README documents how to use the tool and — critically — gives *complete, implementable* technical specifications for both supported encrypted file formats so other implementers can build compatible decryptors.

> **Important:** This software is **NOT LICENSED**. All rights are reserved. Redistribution, modification, or use of this software in any form is **not permitted** without explicit written authorization.

---

## Quick feature summary

- Create and inspect secp256k1 keypairs suitable for Ethereum and Nostr (npub/nsec).
- Export private key material using **one of two encrypted formats**:
  1. **Modern format:** Argon2id → XChaCha20-Poly1305 (recommended for air-gapped storage).
  2. **PGP-compatible format:** Sequoia / OpenPGP symmetric AES-256 (recommended for interoperability).
- Minimal TUI for generating, naming, and exporting keys offline.
- Exports are intended for secure offline storage; this tool does not (by default) broadcast any transactions.

---

## Build & run (short)

```bash
# build release binary
cargo build --release

# or run via cargo (for development)
cargo run --release -- <subcommand> [options]
```

Binary path:
```
target/release/inkan-management-utility
```

Replace `<subcommand>` with the tool UI or export commands available in your build (see tool help).

---

## Exported file naming conventions

- Modern-format encrypted files:  
  `SECRET_KEEP_AIRGAPPED_<nickname>_Private_Key.enc`

- PGP-format encrypted files:  
  `<nickname>_Private_Key.pgp`

You can change names when exporting; the formats are recognized by content, not filename.

---

## FULL SPEC: Modern format — Argon2id + XChaCha20-Poly1305

This format is intended for maximum cryptographic strength and to be simple to parse by implementers.

### Summary (high level)
- KDF: **Argon2id v0x13** (Argon2id)
- KDF output length: **32 bytes**
- AEAD: **XChaCha20-Poly1305 (IETF)** — 24-byte nonce
- AAD: **entire header** (including optional 8-byte noise prefix)
- Payload: pretty-printed JSON (plaintext) containing keypair metadata
- Header contains Argon2 parameters so decryptor does not need side-channel data

### Byte-level layout (binary)
All numeric fields below are encoded in **little-endian** unless stated otherwise.

```
[ optional 8 bytes noise_prefix ]          // present if exporter used "noise" flag
[ u8 version ]                            // value: 1
[ u8 kdf_id ]                             // value: 1 (meaning Argon2id)
[ u32 t_cost ]                            // Argon2 iterations (LE)
[ u32 m_cost_kib ]                        // Argon2 memory in KiB (LE)
[ u8 p_cost ]                             // Argon2 parallelism
[ u8 salt_len ]                           // length of next salt field (typically 16)
[ salt_bytes (salt_len bytes) ]
[ u8 nonce_len ]                          // length of next nonce field (should be 24)
[ nonce_bytes (nonce_len bytes) ]         // used as XChaCha20 nonce
[ ciphertext || 16-byte Poly1305 tag ]    // AEAD ciphertext (XChaCha20-Poly1305 appends tag)
```

- The **header** (for AAD) is the exact contiguous prefix of the file up to and including the nonce bytes. If the 8-byte noise prefix is present it **must** be included in the AAD exactly as written.
- The **ciphertext** begins immediately after the nonce_len + nonce bytes and is the raw AEAD output (ciphertext + 16-byte tag).
- The **version** and **kdf_id** are fixed values to allow future upgrades. Currently: `version = 1`, `kdf_id = 1`.

### Key derivation (implementable steps)
1. Read header and parse `t_cost`, `m_cost_kib`, `p_cost`, `salt_len`, `salt`, `nonce_len`, `nonce`.
2. Obtain user password bytes as **UTF-8** (exact same bytes used at encryption). *Do not alter or trim bytes.* If you plan to normalize (NFKC) you MUST do it on both sides; by default the encryptor used raw UTF-8.
3. Compute Argon2id:
   ```
   key = Argon2id(password_utf8, salt, time_cost = t_cost,
                  memory_cost_kib = m_cost_kib, parallelism = p_cost,
                  hash_length = 32)
   ```
   Use Argon2 v0x13; supply `secret = []` (no pepper) unless you intentionally change specification.
4. Use the 32-byte `key` as AEAD key for **XChaCha20-Poly1305**.
5. Decrypt AEAD: `plaintext = XChaCha20-Poly1305.decrypt(nonce, ciphertext, aad = header_bytes)`.

- If AEAD verification fails, reject the file (invalid password or tampering).
- The plaintext should be a JSON object (pretty-printed) with the exported fields (see Payload below).

### Typical Argon2 defaults used by exporter (examples seen in current builds)
- `t_cost = 3`
- `m_cost_kib = 262144` (256 MiB)
- `p_cost = 1`
- `salt_len = 16`
- `nonce_len = 24`

These parameters are embedded in the header; decryptors must read the header and use the specified values.

### Payload (plaintext JSON)
The plaintext is UTF-8 JSON with fields:

```json
{
  "key_pair_nickname": "<nickname>",
  "private_key_hex": "<64-hex (no 0x)>",
  "private_key_nsec": "<nsec...>",
  "public_key_hex_uncompressed": "<130-hex with 0x04 prefix>",
  "public_key_compressed": "<66-hex with 0x02/0x03 prefix>",
  "public_key_npub": "<npub...>"
}
```

Other fields may be added in future versions; decryptors should parse JSON tolerant of extra fields.

### Implementation notes for decryptors
- Use Argon2id implementations matching RFC; ensure memory_limit is supplied in KiB.
- Use an XChaCha20-Poly1305 library that accepts arbitrary-length AAD and 24-byte nonce.
- Make sure the header bytes passed as AAD are **exactly** the same bytes from the file starting at byte 0 (including noise prefix if present) up to the last nonce byte.
- Zeroize password and derived key buffers after use if language allows.

---

## FULL SPEC: PGP-compatible format (Sequoia OpenPGP / AES-256)

This format is intended for interoperability with standard OpenPGP tools (GnuPG, Sequoia, PGPy, etc.). Files are valid OpenPGP messages and can be decrypted with `gpg` or other OpenPGP implementations.

### Summary (high level)
- Format: **OpenPGP** (RFC 4880) symmetric encryption of a literal data packet.
- Symmetric cipher used by exporter: **AES-256**.
- Key derivation: OpenPGP **S2K** (String-to-Key). Typical implementation: salted S2K with SHA-1.
- Compression: none (literal data packet contains raw JSON).
- Structure: OpenPGP symmetrically encrypted data packet(s) containing the literal data packet with the JSON payload.

### How to decrypt (standard tools)
- With GnuPG:
  ```bash
  gpg --decrypt <file>.pgp > decrypted.json
  ```
  Enter passphrase when prompted.

- With Sequoia (`sq`) if available:
  ```bash
  sq decrypt --output decrypted.json <file>.pgp
  ```

- With Python `PGPy` or other OpenPGP libraries: load message and call symmetric-decrypt with passphrase.

### S2K / details
- OpenPGP supports multiple S2K modes (simple, salted, iterated+salted). The exporter uses salted S2K (SHA-1) with a 96-bit salt (12 bytes) by default in current builds. Implementations using standard OpenPGP libraries will handle this automatically.
- Because the format is OpenPGP-compliant, you **do not** need to manually derive keys or parse packet headers unless you are implementing a pure-from-scratch OpenPGP decryptor. Use an existing OpenPGP implementation where possible.

### Payload (plaintext JSON)
Same JSON structure as the Modern format (see above). The literal data packet will contain the same pretty-printed JSON.

---

## Detection heuristics (how to tell formats apart)

- **OpenPGP** (PGP-format):
  - ASCII-armored files begin with `-----BEGIN PGP MESSAGE-----`.
  - Binary OpenPGP packets often start with bytes in range `0xC0..0xFF` or `0x80..0xBF` depending on packet header format. Use `gpg --list-packets` to inspect.
  - `gpg --decrypt` will succeed for PGP files (given correct passphrase).

- **Modern .enc**:
  - First bytes often begin with small integers (e.g., `0x01 0x01` after optional 8-byte noise) if no noise prefix present.
  - If noise prefix present, bytes 0..7 are random and bytes 8..9 will be `0x01 0x01`. A robust parser should try parsing starting at offset 0 and at offset 8 (to detect noise).
  - Run `xxd -l 16 -g 1 file` and inspect; modern header contains predictable small integers after optional noise.

---

## Example decryptor commands

### Modern format (using provided tools)
- **Rust binary**:
  ```bash
  ./target/release/modern-decryptor <input.enc> <output.json>
  ```
  (Prompts for password; prints parsed Argon2 params; writes JSON.)

- **Python script**:
  ```bash
  ./decrypt_modern_enc.py <input.enc> <output.json>
  ```

### PGP format (common)
- **GnuPG**:
  ```bash
  gpg --decrypt <input>.pgp > decrypted.json
  ```

- **Sequoia (sq)**:
  ```bash
  sq decrypt --output decrypted.json <input>.pgp
  ```

---

## Interoperability & implementation guidance

- If you implement a decryptor in another language, follow these exact rules:
  - For **Modern** format: parse header exactly as specified (little-endian u32 fields), include optional noise in AAD if present; derive key with Argon2id using parameters from header; use XChaCha20-Poly1305 with the derived key and header-as-AAD.
  - For **PGP** format: prefer using a standard OpenPGP library rather than implementing OpenPGP primitives yourself. Standard libraries handle S2K and packet parsing correctly.
- When reading passwords:
  - Use **raw UTF-8 bytes** exactly as entered during encryption.
  - If you adopt Unicode normalization (NFKC), ensure both encryptor and decryptor use the same normalization.
- Security reminders:
  - Argon2 parameters are intentionally strong (256 MiB memory in current default). Be mindful that decryption will be memory/time expensive accordingly.
  - Never store plaintext passwords or keys on disk. Zeroize sensitive buffers in memory where possible.
  - The optional noise prefix is part of AAD — it is integrity-protected and must be included in verification.

---

## Example forensic check (quick verify)
To inspect a modern-format file header:
```bash
xxd -g 1 -l 64 -c 16 SECRET_KEEP_AIRGAPPED_<nick>_Private_Key.enc
```
Look for either:
- `01 01` early in the file (no noise), or
- random bytes then `01 01` starting at offset 8 (noise prefix).

For a PGP file:
```bash
gpg --list-packets <file>.pgp
```
or
```bash
file <file>.pgp
```

---

## Support & troubleshooting

- If decryption fails for Modern format:
  - Confirm you used the exact password (UTF-8 bytes).
  - Confirm the file is not corrupted/truncated.
  - Confirm Argon2 params are embedded and correctly parsed; the encrypted file contains param fields in the header.
  - Use the provided Rust or Python decryptor to verify provenance and parameter parsing.

- If decryption fails for PGP format:
  - Confirm passphrase.
  - Check that the file is a valid OpenPGP message and not truncated.

---

## No license granted
This project and associated code and artifacts are **NOT LICENSED**. All rights reserved. No permission to copy, modify, distribute, or otherwise use the software is granted without explicit written authorization from the owner.

---

