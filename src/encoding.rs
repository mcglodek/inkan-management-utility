use anyhow::{anyhow, Result};
use ethers_core::abi::Token;
use uuid::Uuid;

use crate::util::{hex_to_bytes, parse_u256_any};

pub fn bytes16_or_random(opt_hex: Option<&str>) -> Result<Token> {
    let bytes = if let Some(h) = opt_hex {
        hex_to_bytes(h)?
    } else {
        Uuid::new_v4().as_bytes().to_vec()
    };
    if bytes.len() != 16 {
        return Err(anyhow!("bytes16 must be 16 bytes, got {}", bytes.len()));
    }
    Ok(Token::FixedBytes(bytes))
}

pub fn t_bytes(hex0x: &str) -> Result<Token> {
    Ok(Token::Bytes(hex_to_bytes(hex0x)?))
}
pub fn t_uint(v: u64) -> Token {
    Token::Uint(ethers_core::types::U256::from(v))
}
pub fn t_bool(b: bool) -> Token {
    Token::Bool(b)
}

pub fn encode_calldata(func: &ethers_core::abi::Function, args: Vec<Token>) -> Result<Vec<u8>> {
    Ok(func.encode_input(&args)?)
}

// Convenience if you later want to parse user-supplied values that can be 0x or decimal
pub fn parse_u256_str(s: &str) -> Result<ethers_core::types::U256> {
    parse_u256_any(s)
}

