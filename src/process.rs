use anyhow::{anyhow, Context, Result};
use bech32::{decode as bech32_decode, FromBase32, Variant};
use ethers_core::abi::{Abi, Function};
use ethers_core::types::Address;
use ethers_core::types::U256;
use ethers_signers::{LocalWallet, Signer};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::PublicKey as KPub;

use crate::decoder::{build_decoded, build_decoded_for_combo};
use crate::encoding::{bytes16_or_random, encode_calldata, t_bool, t_bytes, t_uint};
use crate::key::uncompressed_pubkey_0x04;
use crate::signing::{sign_eip1559, sign_message_eip191};
use crate::types::{BatchEntryOut, Item};
use crate::util::{parse_addr, u256_to_be32};




/// Options for the batch signer subcommand
#[derive(Clone, Debug)]
pub struct BatchOpts {
    pub gas_limit: String,
    pub max_fee_per_gas: String,
    pub max_priority_fee_per_gas: String,
}

/// Parse a secret key input as either:
/// - hex (64 hex chars, optional 0x/0X prefix), or
/// - bech32 "nsec1..." (payload must be exactly 32 bytes)
fn privkey_bytes_from_input(input: &str) -> Result<[u8; 32]> {
    let s = input.trim();

    // Try nsec first if it looks like one (case-insensitive match on prefix)
    if s.to_ascii_lowercase().starts_with("nsec1") {
        let (hrp, data, variant) = bech32_decode(s).context("nsec: bech32 decode failed")?;
        if variant != Variant::Bech32 {
            return Err(anyhow!("nsec: invalid bech32 variant"));
        }
        if hrp.to_ascii_lowercase() != "nsec" {
            return Err(anyhow!("nsec: invalid human-readable part '{hrp}'"));
        }
        let bytes = Vec::<u8>::from_base32(&data).context("nsec: invalid bech32 payload")?;
        if bytes.len() != 32 {
            return Err(anyhow!("nsec: payload must be exactly 32 bytes (got {})", bytes.len()));
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        return Ok(out);
    }

    // Otherwise, treat as hex (optionally 0x/0X-prefixed).
    let pk = s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    let bytes = hex::decode(pk)?; // preserves nice hex errors like "Odd number of digits"
    if bytes.len() != 32 {
        return Err(anyhow!("hex secret key must be exactly 32 bytes (got {})", bytes.len()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// Normalize any pubkey input to canonical uncompressed 65-byte hex with 0x04 prefix (lowercase).
/// Accepts:
/// - 0x/0X-prefixed or bare hex
/// - compressed (33 bytes) starting with 0x02/0x03 -> decompress
/// - uncompressed (65 bytes) starting with 0x04 -> passthrough normalized
/// - 64-byte "bare" uncompressed (missing 0x04) -> we add 0x04 prefix
fn normalize_pubkey_to_uncompressed_0x04(input_hex: &str) -> Result<String> {
    let t = input_hex.trim();
    let no0x = t.strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    let bytes = hex::decode(no0x)?; // preserves nice hex errors

    match bytes.len() {
        33 => {
            // compressed; must start with 0x02 or 0x03
            let first = bytes[0];
            if first != 0x02 && first != 0x03 {
                return Err(anyhow!("compressed pubkey must start with 02 or 03"));
            }
            let pk = KPub::from_sec1_bytes(&bytes)
                .map_err(|_| anyhow!("compressed pubkey parse failed"))?;
            let uncompressed = pk.to_encoded_point(false); // false => uncompressed (65 bytes, starts with 0x04)
            Ok(format!("0x{}", hex::encode(uncompressed.as_bytes())))
        }
        65 => {
            // uncompressed; must start with 0x04
            if bytes[0] != 0x04 {
                return Err(anyhow!("65-byte pubkey must start with 04 (uncompressed)"));
            }
            Ok(format!("0x{}", hex::encode(bytes)))
        }
        64 => {
            // uncompressed w/o prefix; add 0x04
            let mut with_prefix = Vec::with_capacity(65);
            with_prefix.push(0x04);
            with_prefix.extend_from_slice(&bytes);
            Ok(format!("0x{}", hex::encode(with_prefix)))
        }
        _ => Err(anyhow!(
            "unsupported pubkey length: {} (expected 33 compressed, 65 uncompressed, or 64 without 04)",
            bytes.len()
        )),
    }
}


/// Canonicalize any 0x/0X/no-prefix hex string into 0x + lowercase.
fn normalize_0x_lower(s: &str) -> String {
    let t = s.trim();
    let no0x = t.strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    format!("0x{}", no0x.to_ascii_lowercase())
}

/// Build the struct payload, sign, and assemble calldata for each function
pub async fn process_item(abi: &Abi, opts: &BatchOpts, it: &Item) -> Result<BatchEntryOut> {
    let func_name = it.function_to_call.as_str();

    // Common params
    let chain_id = it.chain_id.unwrap_or(31337);
    let nonce_tx = it.nonce.unwrap_or(0);
    let to_addr: Address = parse_addr(&it.contract_address)?;
    let gas_limit = &opts.gas_limit;
    let max_fee = &opts.max_fee_per_gas;
    let max_prio = &opts.max_priority_fee_per_gas;

    // Helper to make a wallet from a hex or nsec input
    let mk_wallet = |input: &str| -> Result<LocalWallet> {
        let sk_bytes = privkey_bytes_from_input(input)?;
        let sk = k256::ecdsa::SigningKey::from_slice(&sk_bytes)
            .context("invalid secp256k1 secret key (out of range or zero)")?;
        Ok(LocalWallet::from(sk).with_chain_id(chain_id))
    };

    // Use Abi::function() (unique names in this ABI)
    let func: &Function = abi
        .function(func_name)
        .map_err(|_| anyhow!("function '{}' not in embedded ABI", func_name))?;

    // Switch on function
    let (_data, signed_tx_hex, decoded) = match func_name {
        "createDelegationEvent" => {
            let owner_pk = it
                .type_a_privkey_x
                .as_ref()
                .ok_or_else(|| anyhow!("TYPE_A_PRIVKEY_X required"))?;
            let wallet = mk_wallet(owner_pk)?;

            // delegatee (allow both privkey+pubkey, check for consistency, normalize pubkey)
            let (delegatee_pubkey_0x04, must_zero_sigs, delegatee_wallet_opt) =
                match (&it.type_a_privkey_y, &it.type_a_pubkey_y) {
                    // Both provided: verify they match
                    (Some(pk), Some(pubk)) if !pk.is_empty() && !pubk.is_empty() => {
                        let computed = normalize_0x_lower(&uncompressed_pubkey_0x04(&mk_wallet(pk)?));
                        let provided = normalize_pubkey_to_uncompressed_0x04(pubk)?;
                        if computed != provided {
                            return Err(anyhow!(
                                "Inconsistent DELEGATEE_PRIVKEY and DELEGATEE_PUBKEY: the provided pubkey does not match the given privkey."
                            ));
                        }
                        (provided, false, Some(mk_wallet(pk)?))
                    }
                    // Privkey only
                    (Some(pk), _) if !pk.is_empty() => {
                        (normalize_0x_lower(&uncompressed_pubkey_0x04(&mk_wallet(pk)?)), false, Some(mk_wallet(pk)?))
                    }
                    // Pubkey only
                    (_, Some(pubk)) if !pubk.is_empty() => (normalize_pubkey_to_uncompressed_0x04(pubk)?, true, None),
                    _ => return Err(anyhow!("Provide TYPE_A_PRIVKEY_Y or TYPE_A_PUBKEY_Y")),
                };

            let delegator_pubkey = normalize_0x_lower(&uncompressed_pubkey_0x04(&wallet));
            let delegation_start = it.type_a_uint_x.unwrap_or(0);
            let delegation_end = it.type_a_uint_y.unwrap_or(0);
            let requires_delegatee_sig = it.type_a_boolean.as_deref().unwrap_or("true") == "true";
            let uuid16 = bytes16_or_random(None)?;

            // off-chain payload
            let payload = vec![
                t_bytes(&delegator_pubkey)?,
                t_bytes(&delegatee_pubkey_0x04)?,
                t_uint(delegation_start),
                t_uint(delegation_end),
                t_bool(requires_delegatee_sig),
                uuid16.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
            ];
            let encoded = ethers_core::abi::encode(&payload);
            let msg_hash = ethers_core::utils::keccak256(encoded);
            let sig_delegator = sign_message_eip191(&wallet, msg_hash).await?;

            let (r_delegator, s_delegator, v_delegator) = (sig_delegator.r, sig_delegator.s, sig_delegator.v);
            let (r_delegatee, s_delegatee, v_delegatee) = if must_zero_sigs {
                (U256::from(0u64), U256::from(0u64), 0u64)
            } else {
                let w = delegatee_wallet_opt.as_ref().unwrap();
                let sig = sign_message_eip191(w, msg_hash).await?;
                (sig.r, sig.s, sig.v)
            };

            let tuple_tokens = ethers_core::abi::Token::Tuple(vec![
                t_bytes(&delegator_pubkey)?,
                t_bytes(&delegatee_pubkey_0x04)?,
                t_uint(delegation_start),
                t_uint(delegation_end),
                t_bool(requires_delegatee_sig),
                uuid16.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
                ethers_core::abi::Token::FixedBytes(u256_to_be32(r_delegator)),
                ethers_core::abi::Token::FixedBytes(u256_to_be32(s_delegator)),
                ethers_core::abi::Token::Uint(U256::from(v_delegator)),
                ethers_core::abi::Token::FixedBytes(if must_zero_sigs { vec![0u8; 32] } else { u256_to_be32(r_delegatee) }),
                ethers_core::abi::Token::FixedBytes(if must_zero_sigs { vec![0u8; 32] } else { u256_to_be32(s_delegatee) }),
                ethers_core::abi::Token::Uint(U256::from(if must_zero_sigs { 0u64 } else { v_delegatee })),
            ]);
            let data = encode_calldata(func, vec![tuple_tokens])?;
            let (raw, _typed) =
                sign_eip1559(&wallet, chain_id, to_addr, nonce_tx, gas_limit, max_fee, max_prio, data.clone()).await?;
            let decoded = build_decoded(&raw, &to_addr, &data, abi)?;
            (data, raw, decoded)
        }

        "createRevocationEvent" => {
            let owner_pk = it
                .type_b_privkey_x
                .as_ref()
                .ok_or_else(|| anyhow!("TYPE_B_PRIVKEY_X required"))?;
            let wallet = mk_wallet(owner_pk)?;

            // revokee (allow both privkey+pubkey, check for consistency, normalize pubkey)
            let (revokee_pubkey_0x04, must_zero_sigs, revokee_wallet_opt) =
                match (&it.type_b_privkey_y, &it.type_b_pubkey_y) {
                    // Both provided: verify they match
                    (Some(pk), Some(pubk)) if !pk.is_empty() && !pubk.is_empty() => {
                        let computed = normalize_0x_lower(&uncompressed_pubkey_0x04(&mk_wallet(pk)?));
                        let provided = normalize_pubkey_to_uncompressed_0x04(pubk)?;
                        if computed != provided {
                            return Err(anyhow!(
                                "Inconsistent REVOKEE_PRIVKEY and REVOKEE_PUBKEY: the provided pubkey does not match the given privkey."
                            ));
                        }
                        (provided, false, Some(mk_wallet(pk)?))
                    }
                    // Privkey only
                    (Some(pk), _) if !pk.is_empty() => {
                        (normalize_0x_lower(&uncompressed_pubkey_0x04(&mk_wallet(pk)?)), false, Some(mk_wallet(pk)?))
                    }
                    // Pubkey only
                    (_, Some(pubk)) if !pubk.is_empty() => (normalize_pubkey_to_uncompressed_0x04(pubk)?, true, None),
                    _ => return Err(anyhow!("Provide TYPE_B_PRIVKEY_Y or TYPE_B_PUBKEY_Y")),
                };

            let revoker_pubkey = normalize_0x_lower(&uncompressed_pubkey_0x04(&wallet));
            let start = it.type_b_uint_x.unwrap_or(0);
            let end = it.type_b_uint_y.unwrap_or(0);
            let uuid16 = bytes16_or_random(None)?;
            let payload = vec![
                t_bytes(&revoker_pubkey)?,
                t_bytes(&revokee_pubkey_0x04)?,
                t_uint(start),
                t_uint(end),
                uuid16.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
            ];
            let encoded = ethers_core::abi::encode(&payload);
            let msg_hash = ethers_core::utils::keccak256(encoded);
            let sig_revoker = sign_message_eip191(&wallet, msg_hash).await?;
            let (r_revoker, s_revoker, v_revoker) = (sig_revoker.r, sig_revoker.s, sig_revoker.v);
            let (r_revokee, s_revokee, v_revokee) = if must_zero_sigs {
                (U256::from(0u64), U256::from(0u64), 0u64)
            } else {
                let w = revokee_wallet_opt.as_ref().unwrap();
                let sig = sign_message_eip191(w, msg_hash).await?;
                (sig.r, sig.s, sig.v)
            };

            let tuple = ethers_core::abi::Token::Tuple(vec![
                t_bytes(&revoker_pubkey)?,
                t_bytes(&revokee_pubkey_0x04)?,
                t_uint(start),
                t_uint(end),
                uuid16.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
                ethers_core::abi::Token::FixedBytes(u256_to_be32(r_revoker)),
                ethers_core::abi::Token::FixedBytes(u256_to_be32(s_revoker)),
                ethers_core::abi::Token::Uint(U256::from(v_revoker)),
                ethers_core::abi::Token::FixedBytes(if must_zero_sigs { vec![0u8; 32] } else { u256_to_be32(r_revokee) }),
                ethers_core::abi::Token::FixedBytes(if must_zero_sigs { vec![0u8; 32] } else { u256_to_be32(s_revokee) }),
                ethers_core::abi::Token::Uint(U256::from(if must_zero_sigs { 0u64 } else { v_revokee })),
            ]);
            let data = encode_calldata(func, vec![tuple])?;
            let (raw, _typed) =
                sign_eip1559(&wallet, chain_id, to_addr, nonce_tx, gas_limit, max_fee, max_prio, data.clone()).await?;
            let decoded = build_decoded(&raw, &to_addr, &data, abi)?;
            (data, raw, decoded)
        }

        "createPermanentInvalidationEvent" => {
            let owner_pk = it
                .type_c_privkey_x
                .as_ref()
                .ok_or_else(|| anyhow!("TYPE_C_PRIVKEY_X required"))?;
            let wallet = mk_wallet(owner_pk)?;
            let invalidated_pubkey = normalize_0x_lower(&uncompressed_pubkey_0x04(&wallet));
            let uuid16 = bytes16_or_random(None)?;
            let payload = vec![
                t_bytes(&invalidated_pubkey)?,
                uuid16.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
            ];
            let encoded = ethers_core::abi::encode(&payload);
            let msg_hash = ethers_core::utils::keccak256(encoded);
            let sig = sign_message_eip191(&wallet, msg_hash).await?;
            let (r, s, v) = (sig.r, sig.s, sig.v);

            let tuple = ethers_core::abi::Token::Tuple(vec![
                t_bytes(&invalidated_pubkey)?,
                uuid16.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
                ethers_core::abi::Token::FixedBytes(u256_to_be32(r)),
                ethers_core::abi::Token::FixedBytes(u256_to_be32(s)),
                ethers_core::abi::Token::Uint(U256::from(v)),
            ]);
            let data = encode_calldata(func, vec![tuple])?;
            let (raw, _typed) =
                sign_eip1559(&wallet, chain_id, to_addr, nonce_tx, gas_limit, max_fee, max_prio, data.clone()).await?;
            let decoded = build_decoded(&raw, &to_addr, &data, abi)?;
            (data, raw, decoded)
        }

        "createRevocationEventFollowedByDelegationEvent" => {
            // owner is TYPE_A_PRIVKEY_X for both sides (as in your Node code)
            let owner_pk = it
                .type_a_privkey_x
                .as_ref()
                .ok_or_else(|| anyhow!("TYPE_A_PRIVKEY_X required"))?;
            let wallet = mk_wallet(owner_pk)?;

            // A side (delegation) — allow both & check consistency; normalize pubkey
            let (delegatee_pubkey_0x04, must_zero_delegatee, delegatee_wallet_opt) =
                match (&it.type_a_privkey_y, &it.type_a_pubkey_y) {
                    (Some(pk), Some(pubk)) if !pk.is_empty() && !pubk.is_empty() => {
                        let computed = normalize_0x_lower(&uncompressed_pubkey_0x04(&mk_wallet(pk)?));
                        let provided = normalize_pubkey_to_uncompressed_0x04(pubk)?;
                        if computed != provided {
                            return Err(anyhow!(
                                "Inconsistent DELEGATEE_PRIVKEY and DELEGATEE_PUBKEY: the provided pubkey does not match the given privkey."
                            ));
                        }
                        (provided, false, Some(mk_wallet(pk)?))
                    }
                    (Some(pk), _) if !pk.is_empty() => {
                        (normalize_0x_lower(&uncompressed_pubkey_0x04(&mk_wallet(pk)?)), false, Some(mk_wallet(pk)?))
                    }
                    (_, Some(pubk)) if !pubk.is_empty() => (normalize_pubkey_to_uncompressed_0x04(pubk)?, true, None),
                    _ => return Err(anyhow!("Provide TYPE_A_PRIVKEY_Y or TYPE_A_PUBKEY_Y")),
                };

            // B side (revocation) — allow both & check consistency; normalize pubkey
            let (revokee_pubkey_0x04, must_zero_revokee, revokee_wallet_opt) =
                match (&it.type_b_privkey_y, &it.type_b_pubkey_y) {
                    (Some(pk), Some(pubk)) if !pk.is_empty() && !pubk.is_empty() => {
                        let computed = normalize_0x_lower(&uncompressed_pubkey_0x04(&mk_wallet(pk)?));
                        let provided = normalize_pubkey_to_uncompressed_0x04(pubk)?;
                        if computed != provided {
                            return Err(anyhow!(
                                "Inconsistent REVOKEE_PRIVKEY and REVOKEE_PUBKEY: the provided pubkey does not match the given privkey."
                            ));
                        }
                        (provided, false, Some(mk_wallet(pk)?))
                    }
                    (Some(pk), _) if !pk.is_empty() => {
                        (normalize_0x_lower(&uncompressed_pubkey_0x04(&mk_wallet(pk)?)), false, Some(mk_wallet(pk)?))
                    }
                    (_, Some(pubk)) if !pubk.is_empty() => (normalize_pubkey_to_uncompressed_0x04(pubk)?, true, None),
                    _ => return Err(anyhow!("Provide TYPE_B_PRIVKEY_Y or TYPE_B_PUBKEY_Y")),
                };

            let delegator_pubkey = normalize_0x_lower(&uncompressed_pubkey_0x04(&wallet));
            // A params
            let a_start = it.type_a_uint_x.unwrap_or(0);
            let a_end = it.type_a_uint_y.unwrap_or(0);
            let a_req = it.type_a_boolean.as_deref().unwrap_or("true") == "true";
            let a_nonce = bytes16_or_random(None)?;
            // B params
            let b_start = it.type_b_uint_x.unwrap_or(0);
            let b_end = it.type_b_uint_y.unwrap_or(0);
            let b_nonce = bytes16_or_random(None)?;

            // Type A payload/signatures
            let payload_a = vec![
                t_bytes(&delegator_pubkey)?,
                t_bytes(&delegatee_pubkey_0x04)?,
                t_uint(a_start),
                t_uint(a_end),
                t_bool(a_req),
                a_nonce.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
            ];
            let enc_a = ethers_core::abi::encode(&payload_a);
            let hash_a = ethers_core::utils::keccak256(enc_a);
            let sig_a_delegator = sign_message_eip191(&wallet, hash_a).await?;
            let (r_a_del, s_a_del, v_a_del) = (sig_a_delegator.r, sig_a_delegator.s, sig_a_delegator.v);
            let (r_a_dee, s_a_dee, v_a_dee) = if must_zero_delegatee {
                (U256::from(0u64), U256::from(0u64), 0u64)
            } else {
                let w = delegatee_wallet_opt.as_ref().unwrap();
                let sig = sign_message_eip191(w, hash_a).await?;
                (sig.r, sig.s, sig.v)
            };

            // Type B payload/signatures
            let payload_b = vec![
                t_bytes(&delegator_pubkey)?,
                t_bytes(&revokee_pubkey_0x04)?,
                t_uint(b_start),
                t_uint(b_end),
                b_nonce.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
            ];
            let enc_b = ethers_core::abi::encode(&payload_b);
            let hash_b = ethers_core::utils::keccak256(enc_b);
            let sig_b_revoker = sign_message_eip191(&wallet, hash_b).await?;
            let (r_b_rev, s_b_rev, v_b_rev) = (sig_b_revoker.r, sig_b_revoker.s, sig_b_revoker.v);
            let (r_b_ree, s_b_ree, v_b_ree) = if must_zero_revokee {
                (U256::from(0u64), U256::from(0u64), 0u64)
            } else {
                let w = revokee_wallet_opt.as_ref().unwrap();
                let sig = sign_message_eip191(w, hash_b).await?;
                (sig.r, sig.s, sig.v)
            };

            // Order: [revocationInputData (B), delegationInputData (A)]
            let tuple_b = ethers_core::abi::Token::Tuple(vec![
                t_bytes(&delegator_pubkey)?,
                t_bytes(&revokee_pubkey_0x04)?,
                t_uint(b_start),
                t_uint(b_end),
                b_nonce.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
                ethers_core::abi::Token::FixedBytes(u256_to_be32(r_b_rev)),
                ethers_core::abi::Token::FixedBytes(u256_to_be32(s_b_rev)),
                ethers_core::abi::Token::Uint(U256::from(v_b_rev)),
                ethers_core::abi::Token::FixedBytes(if must_zero_revokee { vec![0u8; 32] } else { u256_to_be32(r_b_ree) }),
                ethers_core::abi::Token::FixedBytes(if must_zero_revokee { vec![0u8; 32] } else { u256_to_be32(s_b_ree) }),
                ethers_core::abi::Token::Uint(U256::from(if must_zero_revokee { 0u64 } else { v_b_ree })),
            ]);
            let tuple_a = ethers_core::abi::Token::Tuple(vec![
                t_bytes(&delegator_pubkey)?,
                t_bytes(&delegatee_pubkey_0x04)?,
                t_uint(a_start),
                t_uint(a_end),
                t_bool(a_req),
                a_nonce.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
                ethers_core::abi::Token::FixedBytes(u256_to_be32(r_a_del)),
                ethers_core::abi::Token::FixedBytes(u256_to_be32(s_a_del)),
                ethers_core::abi::Token::Uint(U256::from(v_a_del)),
                ethers_core::abi::Token::FixedBytes(if must_zero_delegatee { vec![0u8; 32] } else { u256_to_be32(r_a_dee) }),
                ethers_core::abi::Token::FixedBytes(if must_zero_delegatee { vec![0u8; 32] } else { u256_to_be32(s_a_dee) }),
                ethers_core::abi::Token::Uint(U256::from(if must_zero_delegatee { 0u64 } else { v_a_dee })),
            ]);

            let data = encode_calldata(func, vec![tuple_b, tuple_a])?;
            let (raw, _typed) =
                sign_eip1559(&wallet, chain_id, to_addr, nonce_tx, gas_limit, max_fee, max_prio, data.clone()).await?;
            let decoded = build_decoded_for_combo(&raw, &to_addr, &data, abi)?;
            (data, raw, decoded)
        }

        _ => return Err(anyhow!("Unsupported FUNCTION_TO_CALL: {}", func_name)),
    };

    Ok(BatchEntryOut {
        signed_tx: signed_tx_hex,
        decoded_tx: decoded,
    })
}
