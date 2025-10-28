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
  "public_key_hex_compressed": "<66-hex with 0x02/0x03 prefix>",
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

---

## How to Create an Executable for Tails

This section explains **Option A (recommended)**: building a **static MUSL** Linux binary on another machine and running it on **Tails**.  
This produces a fully self-contained binary that works offline and avoids glibc mismatches.

### 1) Prepare the MUSL toolchain

Run these commands on your development machine (Ubuntu/Debian-based):

```bash
sudo apt-get update
sudo apt-get install -y musl-tools   # provides musl-gcc
rustup target add x86_64-unknown-linux-musl
```

Create or edit Cargo’s config so the MUSL target uses `musl-gcc`:

```bash
mkdir -p ~/.cargo
cat > ~/.cargo/config.toml <<'EOF'
[target.x86_64-unknown-linux-musl]
linker = "musl-gcc"
ar = "ar"
EOF
```

Alternatively, set environment variables for this session:

```bash
export CC_x86_64_unknown_linux_musl=musl-gcc
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc
```

### 2) Build a MUSL binary

From the project root:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

The resulting binary will be located at:
```
target/x86_64-unknown-linux-musl/release/inkan-management-utility
```

You can verify it’s static with:
```bash
file target/x86_64-unknown-linux-musl/release/inkan-management-utility
```
It should say “statically linked” or “musl”.

### 3) Fixing the common `secp256k1-sys` error

If you see:
```
failed to find tool "x86_64-linux-musl-gcc"
error occurred in cc-rs: failed to find tool "x86_64-linux-musl-gcc"
```

It means the MUSL compiler isn’t available. Make sure you installed `musl-tools` and created the `~/.cargo/config.toml` file above.  
Then rebuild:
```bash
cargo build --release --target x86_64-unknown-linux-musl
```

### 4) Copy the binary to Tails

Copy `inkan-management-utility` onto a USB drive or into your **Tails Persistent Storage**.

Then, on Tails, open a Terminal and run:

```bash
cd ~/Persistent
chmod +x inkan-management-utility
```

### 5) Run it on Tails

```bash
./inkan-management-utility menu
```

If display characters look off:
```bash
export TERM=xterm-256color
./inkan-management-utility menu
```

### 6) File persistence

To retain files across reboots, place them in:
```
~/Persistent/inputFiles/
~/Persistent/outputFiles/
```

Tails automatically erases non-persistent areas between sessions.

### 7) Troubleshooting

| Problem | Likely cause / fix |
|----------|--------------------|
| `GLIBC_x.y not found` | You built a glibc binary. Use MUSL build instructions above. |
| `Permission denied` | Run `chmod +x inkan-management-utility`. |
| Missing borders/colors | Run with `TERM=xterm-256color`. |
| Build still fails on MUSL target | Reinstall `musl-tools`; verify the `config.toml` entries. |

---

## No license granted

This project and associated code and artifacts are **NOT LICENSED**.  
All rights reserved. No permission to copy, modify, distribute, or otherwise use the software is granted without explicit written authorization from the owner.
