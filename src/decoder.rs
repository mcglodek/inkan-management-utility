use anyhow::{anyhow, Result};
use ethers_core::abi::FunctionExt;
use ethers_core::abi::{Abi, Function, Token};

use crate::types::{
    DecodedOne, DecodedTxOut, DelegationDecodedOrdered, InvalidationDecodedOrdered,
    RevocationDecodedOrdered,
};
use crate::util::bytes_to_0x;
use crate::{signing::decode_signed_tx_and_recover};

fn as_bytes_hex(tok: &Token) -> Result<String> {
    match tok {
        Token::Bytes(b) => Ok(bytes_to_0x(b)),
        _ => Err(anyhow!("expected bytes")),
    }
}
fn as_fixed_bytes_hex(tok: &Token) -> Result<String> {
    match tok {
        Token::FixedBytes(b) => Ok(bytes_to_0x(b)),
        _ => Err(anyhow!("expected fixed bytes")),
    }
}
fn as_uint_string(tok: &Token) -> Result<String> {
    match tok {
        Token::Uint(u) => Ok(u.to_string()),
        _ => Err(anyhow!("expected uint")),
    }
}
fn as_u8_string(tok: &Token) -> Result<String> {
    match tok {
        Token::Uint(u) => Ok(u.to_string()),
        _ => Err(anyhow!("expected uint8")),
    }
}
fn as_bool(tok: &Token) -> Result<bool> {
    match tok {
        Token::Bool(b) => Ok(*b),
        _ => Err(anyhow!("expected bool")),
    }
}

/// Ordered helpers returning typed structs
fn to_delegation_struct(tok: &Token) -> Result<DelegationDecodedOrdered> {
    let t = match tok {
        Token::Tuple(v) if v.len() == 13 => v,
        _ => return Err(anyhow!("unexpected tuple for delegation")),
    };

    Ok(DelegationDecodedOrdered {
        delegatorPubkey: as_bytes_hex(&t[0])?,
        delegateePubkey: as_bytes_hex(&t[1])?,
        delegationStartTime: as_uint_string(&t[2])?,
        delegationEndTime: as_uint_string(&t[3])?,
        doesRevocationRequireDelegateeSignature: as_bool(&t[4])?,
        nonce: as_fixed_bytes_hex(&t[5])?,
        expectedAddressOfDeployedContract: as_bytes_hex(&t[6])?,
        rDelegatorPubkeySig: as_fixed_bytes_hex(&t[7])?,
        sDelegatorPubkeySig: as_fixed_bytes_hex(&t[8])?,
        vDelegatorPubkeySig: as_u8_string(&t[9])?,
        rDelegateePubkeySig: as_fixed_bytes_hex(&t[10])?,
        sDelegateePubkeySig: as_fixed_bytes_hex(&t[11])?,
        vDelegateePubkeySig: as_u8_string(&t[12])?,
    })
}

fn to_revocation_struct(tok: &Token) -> Result<RevocationDecodedOrdered> {
    let t = match tok {
        Token::Tuple(v) if v.len() == 12 => v,
        _ => return Err(anyhow!("unexpected tuple for revocation")),
    };

    Ok(RevocationDecodedOrdered {
        revokerPubkey: as_bytes_hex(&t[0])?,
        revokeePubkey: as_bytes_hex(&t[1])?,
        revocationStartTime: as_uint_string(&t[2])?,
        revocationEndTime: as_uint_string(&t[3])?,
        nonce: as_fixed_bytes_hex(&t[4])?,
        expectedAddressOfDeployedContract: as_bytes_hex(&t[5])?,
        rRevokerPubkeySig: as_fixed_bytes_hex(&t[6])?,
        sRevokerPubkeySig: as_fixed_bytes_hex(&t[7])?,
        vRevokerPubkeySig: as_u8_string(&t[8])?,
        rRevokeePubkeySig: as_fixed_bytes_hex(&t[9])?,
        sRevokeePubkeySig: as_fixed_bytes_hex(&t[10])?,
        vRevokeePubkeySig: as_u8_string(&t[11])?,
    })
}

fn to_invalidation_struct(tok: &Token) -> Result<InvalidationDecodedOrdered> {
    let t = match tok {
        Token::Tuple(v) if v.len() == 6 => v,
        _ => return Err(anyhow!("unexpected tuple for invalidation")),
    };

    Ok(InvalidationDecodedOrdered {
        invalidatedPubkey: as_bytes_hex(&t[0])?,
        nonce: as_fixed_bytes_hex(&t[1])?,
        expectedAddressOfDeployedContract: as_bytes_hex(&t[2])?,
        rInvalidatedPubkeySig: as_fixed_bytes_hex(&t[3])?,
        sInvalidatedPubkeySig: as_fixed_bytes_hex(&t[4])?,
        vInvalidatedPubkeySig: as_u8_string(&t[5])?,
    })
}

/// Decode calldata -> function name + ordered typed objects
pub fn decode_calldata_to_json(
    abi: &Abi,
    data: &[u8],
) -> Result<(String, Option<DecodedOne>, Option<DecodedOne>)> {
    if data.len() < 4 {
        return Err(anyhow!("calldata too short"));
    }
    let selector: [u8; 4] = data[0..4].try_into().unwrap();

    // Check against the four known functions in the embedded ABI
    let candidates: [&Function; 4] = [
        abi.function("createDelegationEvent")?,
        abi.function("createRevocationEvent")?,
        abi.function("createPermanentInvalidationEvent")?,
        abi.function("createRevocationEventFollowedByDelegationEvent")?,
    ];

    let func = candidates
        .into_iter()
        .find(|f| f.selector() == selector)
        .ok_or_else(|| anyhow!("unknown function selector"))?;

    let tokens = func.decode_input(&data[4..])?;

    let (one, two) = match func.name.as_str() {
        "createDelegationEvent" => {
            let j = to_delegation_struct(&tokens[0])?;
            (Some(DecodedOne::Delegation(j)), None)
        }
        "createRevocationEvent" => {
            let j = to_revocation_struct(&tokens[0])?;
            (Some(DecodedOne::Revocation(j)), None)
        }
        "createPermanentInvalidationEvent" => {
            let j = to_invalidation_struct(&tokens[0])?;
            (Some(DecodedOne::Invalidation(j)), None)
        }
        "createRevocationEventFollowedByDelegationEvent" => {
            // order: [B, A] in calldata
            let jb = to_revocation_struct(&tokens[0])?;
            let ja = to_delegation_struct(&tokens[1])?;
            (Some(DecodedOne::Delegation(ja)), Some(DecodedOne::Revocation(jb))) // (A, B)
        }
        _ => (None, None),
    };

    Ok((func.name.clone(), one, two))
}

pub fn build_decoded(
    raw_hex: &str,
    to: &ethers_core::types::Address,
    _calldata: &[u8],
    abi: &Abi,
) -> Result<DecodedTxOut> {
    let (chain_id, nonce, max_prio, max_fee, gas, _to2, value, data, from) =
        decode_signed_tx_and_recover(raw_hex)?;
    // Decode ABI to get func + struct
    let (func_name, one, _two) = decode_calldata_to_json(abi, &data)?;
    Ok(DecodedTxOut {
        from: format!("{:?}", from),
        to: format!("{:?}", to),
        value: value.to_string(),
        gasLimit: gas.to_string(),
        nonce,
        chainId: chain_id.to_string(),
        maxFeePerGas: max_fee.to_string(),
        maxPriorityFeePerGas: max_prio.to_string(),
        funcName: func_name,
        encodedData: crate::util::bytes_to_0x(&data),
        decodedData: one,
        decodedDataTypeA: None,
        decodedDataTypeB: None,
    })
}

pub fn build_decoded_for_combo(
    raw_hex: &str,
    to: &ethers_core::types::Address,
    _calldata: &[u8],
    abi: &Abi,
) -> Result<DecodedTxOut> {
    let (chain_id, nonce, max_prio, max_fee, gas, _to2, value, data, from) =
        decode_signed_tx_and_recover(raw_hex)?;
    let (func_name, type_a, type_b) = decode_calldata_to_json(abi, &data)?;
    // Expect Delegation for A and Revocation for B; gracefully ignore if shapes differ
    let mut a: Option<DelegationDecodedOrdered> = None;
    let mut b: Option<RevocationDecodedOrdered> = None;
    if let Some(DecodedOne::Delegation(x)) = type_a {
        a = Some(x);
    }
    if let Some(DecodedOne::Revocation(x)) = type_b {
        b = Some(x);
    }

    Ok(DecodedTxOut {
        from: format!("{:?}", from),
        to: format!("{:?}", to),
        value: value.to_string(),
        gasLimit: gas.to_string(),
        nonce,
        chainId: chain_id.to_string(),
        maxFeePerGas: max_fee.to_string(),
        maxPriorityFeePerGas: max_prio.to_string(),
        funcName: func_name,
        encodedData: crate::util::bytes_to_0x(&data),
        decodedData: None,
        decodedDataTypeA: a, // (A,B) with strict struct order
        decodedDataTypeB: b,
    })
}

