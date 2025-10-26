# Inkan Management Utility

> **Offline key generator and Ethereum transaction batch signer**  
> A single, self-contained CLI tool for working with Inkan-style delegation and revocation events.

---

## 🚀 Overview

This tool combines two major functions into one executable:

1. **Key Generator (`keygen` subcommand)**  
   Creates local secp256k1 key pairs suitable for both Ethereum and Nostr.  
   Each record includes:
   - Ethereum address
   - Uncompressed and compressed public keys
   - Nostr-compatible `npub` / `nsec` encodings
   - Matching hex representations

2. **Batch Transaction Signer (`batch` subcommand)**  
   Reads a JSON array describing contract calls, signs them offline using
   local private keys, and outputs a decoded and signed transaction batch file
   ready for inspection or broadcast.

---

## 🧩 Build Instructions

Make sure you have Rust (with `cargo`) installed.  
Then from the project root:

```bash
cargo build --release
```

The compiled binary will appear at:

```
target/release/inkan-management-utility
```

---

## ⚙️ Usage

All commands share the following syntax:

```bash
inkan-management-utility <subcommand> [options]
```

Or when running directly from Cargo (without installing globally):

```bash
cargo run --release -- <subcommand> [options]
```

---

## 🔑 Key Generator (`keygen`)

Generate new Ethereum / Nostr keypairs completely offline.

### Examples

Generate one keypair and print to console:
```bash
cargo run --release -- keygen --count 1
```

Generate five keypairs and save to a JSON file:
```bash
cargo run --release -- keygen --count 5 --out keys.json
```

The output looks like:

```json
[
  {
    "privateKeyHex": "0xabc123...",
    "publicKeyUncompressed0x04": "0x04d3...",
    "publicKeyCompressed": "0x02f5...",
    "address": "0x0b2f...",
    "privateKeyHexNostrFormat": "abc123...",
    "publicKeyHexNostrFormat": "d3f5...",
    "nsec": "nsec1q8wxyz...",
    "npub": "npub1r2abcd..."
  }
]
```

Each generated record includes both Ethereum and Nostr encodings, enabling interoperability between signing domains.

---

## 🧾 Batch Transaction Signer (`batch`)

Reads a JSON batch file and produces an array of signed EIP-1559 transactions.

### Example

```bash
cargo run --release -- batch \
  --batch my_input.json \
  --out batch_output.json \
  --gas-limit 30000000 \
  --max-fee-per-gas 30000000000 \
  --max-priority-fee-per-gas 2000000000
```

Output:
```
✓ Wrote batch_output.json
```

Each entry in the output includes:
- The full raw signed transaction (`signedTx`)
- A decoded structure showing fields such as `from`, `to`, `chainId`, and ABI-decoded call data

---

## 🧱 Project Structure

```
src/
├── main.rs                  # Entry point and CLI dispatch
├── cli.rs                   # CLI and subcommand definitions
├── abi.rs                   # Embedded minimal contract ABI
├── process.rs               # Batch signing and calldata generation
├── signing.rs               # Message and transaction signing helpers
├── encoding.rs              # ABI encoding utilities
├── decoder.rs               # ABI decoding utilities
├── key.rs                   # Public key utilities
├── util.rs                  # Shared helpers (hex, address parsing, etc.)
└── commands/
    ├── mod.rs
    └── keygen.rs            # Key generator implementation
```

---

## 🧰 Dependencies

- `ethers-core`, `ethers-signers`
- `k256`
- `bech32`
- `clap`
- `anyhow`
- `serde`, `serde_json`
- `uuid`
- `tokio`

All dependencies are self-contained; the tool runs fully offline.

---

## 🧪 Tips

- To force a clean rebuild:
  ```bash
  cargo clean && cargo build --release
  ```

- To run with debugging output (faster compile, unoptimized):
  ```bash
  cargo run -- keygen --count 1
  ```

