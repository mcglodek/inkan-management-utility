// src/util.rs
use anyhow::{anyhow, Result};
use ethers_core::types::{Address, U256};
use std::collections::HashMap;

pub fn parse_u256_any(s: &str) -> Result<U256> {
    Ok(if let Some(x) = s.strip_prefix("0x") {
        U256::from_str_radix(x, 16)?
    } else {
        U256::from_dec_str(s)?
    })
}

pub fn parse_addr(s: &str) -> Result<Address> {
    Ok(s.parse::<Address>()?)
}

pub fn hex_to_bytes(s: &str) -> Result<Vec<u8>> {
    let t = s.strip_prefix("0x").unwrap_or(s);
    Ok(hex::decode(t)?)
}

pub fn bytes_to_0x(v: &[u8]) -> String {
    format!("0x{}", hex::encode(v))
}

pub fn u256_to_be32(x: U256) -> Vec<u8> {
    let mut b = [0u8; 32];
    x.to_big_endian(&mut b);
    b.to_vec()
}

pub fn expect_bytes<'a>(tok: &'a ethers_core::abi::Token) -> Result<&'a Vec<u8>> {
    if let ethers_core::abi::Token::Bytes(b) = tok {
        Ok(b)
    } else {
        Err(anyhow!("expected bytes"))
    }
}

/// Internal: parse dotenv-style K=V lines into a map.
/// - Ignores blank lines and lines starting with `#`
/// - Splits on the first '='
/// - Trims whitespace
/// - Supports surrounding single or double quotes
/// - Last duplicate key wins
fn parse_kv_env(contents: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some(eq) = trimmed.find('=') else { continue; };
        let (k, vraw) = trimmed.split_at(eq);
        let key = k.trim().to_string();

        let mut val = vraw[1..].trim().to_string();

        // Remove matching quotes
        if (val.starts_with('"') && val.ends_with('"') && val.len() >= 2)
            || (val.starts_with('\'') && val.ends_with('\'') && val.len() >= 2)
        {
            val = val[1..val.len() - 1].to_string();
        }

        out.insert(key, val);
    }

    out
}

/// Dotenv-style parser for delegation info files (backwards compatible).
pub fn parse_delegation_env(contents: &str) -> HashMap<String, String> {
    parse_kv_env(contents)
}

/// Dotenv-style parser for revocation info files (same rules as delegation).
pub fn parse_revocation_env(contents: &str) -> HashMap<String, String> {
    parse_kv_env(contents)
}
