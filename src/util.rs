// src/util.rs
use anyhow::{anyhow, Result};
use ethers_core::types::{Address, U256};

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

