use anyhow::{anyhow, Result};
use ethers_core::types::{
    transaction::eip2718::TypedTransaction, transaction::eip2930::AccessList, Address,
    Eip1559TransactionRequest, H256, NameOrAddress, Signature, U256,
};
use ethers_core::utils::{keccak256, rlp};
use ethers_signers::{LocalWallet, Signer};

use crate::util::{hex_to_bytes, parse_u256_any};

/// EIP-191 signMessage semantics: given 32-byte hash, sign the bytes (prefix added internally)
pub async fn sign_message_eip191(wallet: &LocalWallet, hash32: [u8; 32]) -> Result<Signature> {
    let sig = wallet.sign_message(&hash32).await?; // adds prefix like ethers.js
    Ok(sig)
}

/// Build + sign EIP-1559 tx
pub async fn sign_eip1559(
    wallet: &LocalWallet,
    chain_id: u64,
    to: Address,
    nonce: u64,
    gas_limit: &str,
    max_fee: &str,
    max_priority: &str,
    data: Vec<u8>,
) -> Result<(String /*raw hex*/, TypedTransaction)> {
    let tx = Eip1559TransactionRequest {
        from: Some(wallet.address()),
        to: Some(NameOrAddress::Address(to)),
        value: Some(U256::from(0u64)),
        data: Some(data.clone().into()),
        nonce: Some(U256::from(nonce)),
        gas: Some(parse_u256_any(gas_limit)?),
        max_fee_per_gas: Some(parse_u256_any(max_fee)?),
        max_priority_fee_per_gas: Some(parse_u256_any(max_priority)?),
        chain_id: Some(chain_id.into()), // U64
        access_list: Default::default(),
    };
    let typed = TypedTransaction::Eip1559(tx);
    let sig = wallet.sign_transaction(&typed).await?;
    let rlp_bytes = typed.rlp_signed(&sig);
    Ok((format!("0x{}", hex::encode(rlp_bytes)), typed))
}

/// Decode a raw signed EIP-1559 tx and recover sender
#[allow(clippy::type_complexity)]
pub fn decode_signed_tx_and_recover(
    raw_hex: &str,
) -> Result<(
    u64,      /*chainId*/
    u64,      /*nonce*/
    U256,     /*maxPrio*/
    U256,     /*maxFee*/
    U256,     /*gas*/
    Address,  /*to*/
    U256,     /*value*/
    Vec<u8>,  /*data*/
    Address,  /*from*/
)> {
    // Expect 0x02-prefixed typed tx
    let raw = hex_to_bytes(raw_hex)?;
    if raw.first() != Some(&0x02) {
        return Err(anyhow!("Not a type-2 (EIP-1559) tx"));
    }
    let rlp_body = &raw[1..];
    let r = rlp::Rlp::new(rlp_body);

    // Fields per spec: [chainId, nonce, maxPriorityFeePerGas, maxFeePerGas, gasLimit, to, value, data, accessList, yParity, r, s]
    let chain_id: U256 = r.at(0)?.as_val()?;
    let nonce: U256 = r.at(1)?.as_val()?;
    let max_prio: U256 = r.at(2)?.as_val()?;
    let max_fee: U256 = r.at(3)?.as_val()?;
    let gas: U256 = r.at(4)?.as_val()?;
    let to_bytes: Vec<u8> = r.at(5)?.as_val()?;
    let to = Address::from_slice(&to_bytes);
    let value: U256 = r.at(6)?.as_val()?;
    let data: Vec<u8> = r.at(7)?.as_val()?;
    // accessList at 8 ignored for now
    let y_parity: u8 = r.at(9)?.as_val()?;
    let r_bytes: Vec<u8> = r.at(10)?.as_val()?;
    let s_bytes: Vec<u8> = r.at(11)?.as_val()?;

    // sighash = keccak256( 0x02 || rlp([chainId, nonce, maxPriorityFeePerGas, maxFeePerGas, gas, to, value, data, accessList]) )
    let mut s = ethers_core::utils::rlp::RlpStream::new_list(9);
    s.append(&chain_id);
    s.append(&nonce);
    s.append(&max_prio);
    s.append(&max_fee);
    s.append(&gas);
    s.append(&to);
    s.append(&value);
    s.append(&data);
    s.append(&ethers_core::types::transaction::eip2930::AccessList::default());

    let mut preimage = vec![0x02u8];
    preimage.extend_from_slice(&s.out());

    let sighash = H256::from(keccak256(preimage));

    let sig = ethers_core::types::Signature {
        r: U256::from_big_endian(&r_bytes),
        s: U256::from_big_endian(&s_bytes),
        v: y_parity as u64, // 0/1 for type-2
    };

    let from_addr = sig.recover(sighash)?;

    Ok((
        chain_id.as_u64(),
        nonce.as_u64(),
        max_prio,
        max_fee,
        gas,
        to,
        value,
        data,
        from_addr,
    ))
}

