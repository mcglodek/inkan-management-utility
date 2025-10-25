use anyhow::{anyhow, Context, Result};
use clap::Parser;

use ethers_core::abi::FunctionExt; // for selector()
use ethers_core::abi::{Abi, Function, Token};
use ethers_core::types::{
    transaction::eip2718::TypedTransaction, transaction::eip2930::AccessList, Address,
    Eip1559TransactionRequest, H256, NameOrAddress, Signature, U256,
};
use ethers_core::utils::{keccak256, rlp};
use ethers_signers::{LocalWallet, Signer};

use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use uuid::Uuid;

/// Embedded minimal ABI (functions only)
const INKAN_ABI_JSON: &str = r#"[{
  "type":"function","name":"createDelegationEvent","stateMutability":"nonpayable",
  "inputs":[{"name":"inputData","type":"tuple","components":[
    {"name":"delegatorPubkey","type":"bytes"},
    {"name":"delegateePubkey","type":"bytes"},
    {"name":"delegationStartTime","type":"uint256"},
    {"name":"delegationEndTime","type":"uint256"},
    {"name":"doesRevocationRequireDelegateeSignature","type":"bool"},
    {"name":"nonce","type":"bytes16"},
    {"name":"expectedAddressOfDeployedContract","type":"bytes"},
    {"name":"rDelegatorPubkeySig","type":"bytes32"},
    {"name":"sDelegatorPubkeySig","type":"bytes32"},
    {"name":"vDelegatorPubkeySig","type":"uint8"},
    {"name":"rDelegateePubkeySig","type":"bytes32"},
    {"name":"sDelegateePubkeySig","type":"bytes32"},
    {"name":"vDelegateePubkeySig","type":"uint8"}
  ]}],
  "outputs":[]
},{
  "type":"function","name":"createRevocationEvent","stateMutability":"nonpayable",
  "inputs":[{"name":"inputData","type":"tuple","components":[
    {"name":"revokerPubkey","type":"bytes"},
    {"name":"revokeePubkey","type":"bytes"},
    {"name":"revocationStartTime","type":"uint256"},
    {"name":"revocationEndTime","type":"uint256"},
    {"name":"nonce","type":"bytes16"},
    {"name":"expectedAddressOfDeployedContract","type":"bytes"},
    {"name":"rRevokerPubkeySig","type":"bytes32"},
    {"name":"sRevokerPubkeySig","type":"bytes32"},
    {"name":"vRevokerPubkeySig","type":"uint8"},
    {"name":"rRevokeePubkeySig","type":"bytes32"},
    {"name":"sRevokeePubkeySig","type":"bytes32"},
    {"name":"vRevokeePubkeySig","type":"uint8"}
  ]}],
  "outputs":[]
},{
  "type":"function","name":"createPermanentInvalidationEvent","stateMutability":"nonpayable",
  "inputs":[{"name":"inputData","type":"tuple","components":[
    {"name":"invalidatedPubkey","type":"bytes"},
    {"name":"nonce","type":"bytes16"},
    {"name":"expectedAddressOfDeployedContract","type":"bytes"},
    {"name":"rInvalidatedPubkeySig","type":"bytes32"},
    {"name":"sInvalidatedPubkeySig","type":"bytes32"},
    {"name":"vInvalidatedPubkeySig","type":"uint8"}
  ]}],
  "outputs":[]
},{
  "type":"function","name":"createRevocationEventFollowedByDelegationEvent","stateMutability":"nonpayable",
  "inputs":[
    {"name":"revocationInputData","type":"tuple","components":[
      {"name":"revokerPubkey","type":"bytes"},
      {"name":"revokeePubkey","type":"bytes"},
      {"name":"revocationStartTime","type":"uint256"},
      {"name":"revocationEndTime","type":"uint256"},
      {"name":"nonce","type":"bytes16"},
      {"name":"expectedAddressOfDeployedContract","type":"bytes"},
      {"name":"rRevokerPubkeySig","type":"bytes32"},
      {"name":"sRevokerPubkeySig","type":"bytes32"},
      {"name":"vRevokerPubkeySig","type":"uint8"},
      {"name":"rRevokeePubkeySig","type":"bytes32"},
      {"name":"sRevokeePubkeySig","type":"bytes32"},
      {"name":"vRevokeePubkeySig","type":"uint8"}
    ]},
    {"name":"delegationInputData","type":"tuple","components":[
      {"name":"delegatorPubkey","type":"bytes"},
      {"name":"delegateePubkey","type":"bytes"},
      {"name":"delegationStartTime","type":"uint256"},
      {"name":"delegationEndTime","type":"uint256"},
      {"name":"doesRevocationRequireDelegateeSignature","type":"bool"},
      {"name":"nonce","type":"bytes16"},
      {"name":"expectedAddressOfDeployedContract","type":"bytes"},
      {"name":"rDelegatorPubkeySig","type":"bytes32"},
      {"name":"sDelegatorPubkeySig","type":"bytes32"},
      {"name":"vDelegatorPubkeySig","type":"uint8"},
      {"name":"rDelegateePubkeySig","type":"bytes32"},
      {"name":"sDelegateePubkeySig","type":"bytes32"},
      {"name":"vDelegateePubkeySig","type":"uint8"}
    ]}
  ],
  "outputs":[]
}]"#;

/// CLI
#[derive(Parser, Debug)]
#[command(version, about="Inkan offline batch signer (EIP-1559). Reads a batch JSON array and writes batch_output.json")]
struct Cli {
    /// Path to the batch input JSON (array of items)
    #[arg(long)]
    batch: PathBuf,

    /// Path to the combined output file (default: ./batch_output.json)
    #[arg(long, default_value = "batch_output.json")]
    out: PathBuf,

    /// Default gas limit to use if not specified elsewhere
    #[arg(long, default_value = "30000000")]
    gas_limit: String,

    /// Default max fee per gas in wei (e.g., 30000000000 for 30 gwei)
    #[arg(long, default_value = "30000000000")]
    max_fee_per_gas: String,

    /// Default max priority fee per gas in wei (e.g., 2000000000 for 2 gwei)
    #[arg(long, default_value = "2000000000")]
    max_priority_fee_per_gas: String,
}

/// Batch input items (verbatim field names from your examples)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct Item {
    function_to_call: String,
    nonce: Option<u64>,
    chain_id: Option<u64>,
    contract_address: String,

    // A
    type_a_privkey_x: Option<String>,
    type_a_privkey_y: Option<String>,
    type_a_pubkey_y: Option<String>,
    type_a_uint_x: Option<u64>,
    type_a_uint_y: Option<u64>,
    type_a_boolean: Option<String>,

    // B
    type_b_privkey_x: Option<String>,
    type_b_privkey_y: Option<String>,
    type_b_pubkey_y: Option<String>,
    type_b_uint_x: Option<u64>,
    type_b_uint_y: Option<u64>,

    // C
    type_c_privkey_x: Option<String>,
}

/// Output shapes
#[derive(Debug, Serialize)]
struct BatchEntryOut {
    #[serde(rename = "signedTx")]
    signed_tx: String,
    #[serde(rename = "decodedTx")]
    decoded_tx: DecodedTxOut,
}

#[derive(Debug, Serialize)]
struct DecodedTxOut {
    from: String,
    to: String,
    value: String,
    gasLimit: String,
    nonce: u64,
    chainId: String,
    maxFeePerGas: String,
    maxPriorityFeePerGas: String,
    funcName: String,
    encodedData: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    decodedData: Option<DecodedOne>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decodedDataTypeA: Option<DelegationDecodedOrdered>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decodedDataTypeB: Option<RevocationDecodedOrdered>,
}

/// Ordered decoded output structs (to guarantee field order in JSON)
#[derive(Debug, Serialize)]
struct DelegationDecodedOrdered {
    delegatorPubkey: String,
    delegateePubkey: String,
    delegationStartTime: String,
    delegationEndTime: String,
    doesRevocationRequireDelegateeSignature: bool,
    nonce: String,
    expectedAddressOfDeployedContract: String,
    rDelegatorPubkeySig: String,
    sDelegatorPubkeySig: String,
    vDelegatorPubkeySig: String,
    rDelegateePubkeySig: String,
    sDelegateePubkeySig: String,
    vDelegateePubkeySig: String,
}

#[derive(Debug, Serialize)]
struct RevocationDecodedOrdered {
    revokerPubkey: String,
    revokeePubkey: String,
    revocationStartTime: String,
    revocationEndTime: String,
    nonce: String,
    expectedAddressOfDeployedContract: String,
    rRevokerPubkeySig: String,
    sRevokerPubkeySig: String,
    vRevokerPubkeySig: String,
    rRevokeePubkeySig: String,
    sRevokeePubkeySig: String,
    vRevokeePubkeySig: String,
}

#[derive(Debug, Serialize)]
struct InvalidationDecodedOrdered {
    invalidatedPubkey: String,
    nonce: String,
    expectedAddressOfDeployedContract: String,
    rInvalidatedPubkeySig: String,
    sInvalidatedPubkeySig: String,
    vInvalidatedPubkeySig: String,
}

/// Untagged enum so `decodedData` can be one of the three ordered shapes
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum DecodedOne {
    Delegation(DelegationDecodedOrdered),
    Revocation(RevocationDecodedOrdered),
    Invalidation(InvalidationDecodedOrdered),
}

fn parse_u256_any(s: &str) -> Result<U256> {
    Ok(if let Some(x) = s.strip_prefix("0x") {
        U256::from_str_radix(x, 16)?
    } else {
        U256::from_dec_str(s)?
    })
}
fn parse_addr(s: &str) -> Result<Address> {
    Ok(s.parse::<Address>()?)
}

fn hex_to_bytes(s: &str) -> Result<Vec<u8>> {
    let t = s.strip_prefix("0x").unwrap_or(s);
    Ok(hex::decode(t)?)
}
fn bytes_to_0x(v: &[u8]) -> String {
    format!("0x{}", hex::encode(v))
}

/// Get uncompressed pubkey (0x04 + x + y) from a wallet
fn uncompressed_pubkey_0x04(wallet: &LocalWallet) -> String {
    let vk = wallet.signer().verifying_key();
    let pt = vk.to_encoded_point(false); // uncompressed
    bytes_to_0x(pt.as_bytes())
}

/// EIP-191 signMessage semantics: given 32-byte hash, sign the bytes (prefix added internally)
async fn sign_message_eip191(wallet: &LocalWallet, hash32: [u8; 32]) -> Result<Signature> {
    let sig = wallet.sign_message(&hash32).await?; // adds prefix like ethers.js
    Ok(sig)
}

/// Build the ABI used for encode/decode
fn load_abi() -> Result<Abi> {
    Ok(serde_json::from_str::<Abi>(INKAN_ABI_JSON)?)
}

/// Utility: Token::FixedBytes (size 16) from 0x.. or random uuid v4
fn bytes16_or_random(opt_hex: Option<&str>) -> Result<Token> {
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

/// Helper to make Token::Bytes from 0x-hex
fn t_bytes(hex0x: &str) -> Result<Token> {
    Ok(Token::Bytes(hex_to_bytes(hex0x)?))
}
fn t_uint(v: u64) -> Token {
    Token::Uint(U256::from(v))
}
fn t_bool(b: bool) -> Token {
    Token::Bool(b)
}

/// Build calldata for a given function with given tokens
fn encode_calldata(func: &Function, args: Vec<Token>) -> Result<Vec<u8>> {
    Ok(func.encode_input(&args)?)
}

/// Build + sign EIP-1559 tx
async fn sign_eip1559(
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
        data: Some(data.into()),
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
fn decode_signed_tx_and_recover(
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
    let mut s = rlp::RlpStream::new_list(9);
    s.append(&chain_id);
    s.append(&nonce);
    s.append(&max_prio);
    s.append(&max_fee);
    s.append(&gas);
    s.append(&to);
    s.append(&value);
    s.append(&data);
    // empty AccessList encodes to an empty list — easiest is to append the type directly
    s.append(&AccessList::default());

    let mut preimage = vec![0x02u8];
    preimage.extend_from_slice(&s.out());

    let sighash = H256::from(keccak256(preimage));

    let sig = Signature {
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

/// Extractors for ordered serialization
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

/// Decode calldata -> function name + ordered typed objects
fn decode_calldata_to_json(
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

fn u256_to_be32(x: U256) -> Vec<u8> {
    let mut b = [0u8; 32];
    x.to_big_endian(&mut b);
    b.to_vec()
}

/// Build DecodedTxOut from raw signed tx + calldata, single-arg functions
fn build_decoded(raw_hex: &str, to: &Address, _calldata: &[u8], abi: &Abi) -> Result<DecodedTxOut> {
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
        encodedData: bytes_to_0x(&data),
        decodedData: one,
        decodedDataTypeA: None,
        decodedDataTypeB: None,
    })
}

/// Build DecodedTxOut for combo function with two tuples [B, A]
fn build_decoded_for_combo(
    raw_hex: &str,
    to: &Address,
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
        encodedData: bytes_to_0x(&data),
        decodedData: None,
        decodedDataTypeA: a, // (A,B) with strict struct order
        decodedDataTypeB: b,
    })
}

/// Build the struct payload, sign, and assemble calldata for each function
async fn process_item(abi: &Abi, cli: &Cli, it: &Item) -> Result<BatchEntryOut> {
    let func_name = it.function_to_call.as_str();

    // Common params
    let chain_id = it.chain_id.unwrap_or(31337);
    let nonce_tx = it.nonce.unwrap_or(0);
    let to_addr = parse_addr(&it.contract_address)?;
    let gas_limit = &cli.gas_limit;
    let max_fee = &cli.max_fee_per_gas;
    let max_prio = &cli.max_priority_fee_per_gas;

    // Helper to make a wallet from privkey hex
    let mk_wallet = |hexpk: &str| -> Result<LocalWallet> {
        let pk = hexpk.strip_prefix("0x").unwrap_or(hexpk);
        let bytes = hex::decode(pk)?;
        let sk = k256::ecdsa::SigningKey::from_slice(&bytes)?;
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
            // delegatee
            let (delegatee_pubkey_0x04, must_zero_sigs, delegatee_wallet_opt) =
                match (&it.type_a_privkey_y, &it.type_a_pubkey_y) {
                    (Some(pk), _) if !pk.is_empty() => {
                        (uncompressed_pubkey_0x04(&mk_wallet(pk)?), false, Some(mk_wallet(pk)?))
                    }
                    (_, Some(pubk)) if !pubk.is_empty() => (pubk.clone(), true, None),
                    _ => return Err(anyhow!("Provide TYPE_A_PRIVKEY_Y or TYPE_A_PUBKEY_Y")),
                };

            let delegator_pubkey = uncompressed_pubkey_0x04(&wallet);
            let delegation_start = it.type_a_uint_x.unwrap_or(0);
            let delegation_end = it.type_a_uint_y.unwrap_or(0);
            let requires_delegatee_sig =
                it.type_a_boolean.as_deref().unwrap_or("true") == "true";
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
            let msg_hash = keccak256(encoded);
            let sig_delegator = sign_message_eip191(&wallet, msg_hash).await?;

            let (r_delegator, s_delegator, v_delegator) =
                (sig_delegator.r, sig_delegator.s, sig_delegator.v);
            let (r_delegatee, s_delegatee, v_delegatee) = if must_zero_sigs {
                (U256::from(0u64), U256::from(0u64), 0u64)
            } else {
                let w = delegatee_wallet_opt.as_ref().unwrap();
                let sig = sign_message_eip191(w, msg_hash).await?;
                (sig.r, sig.s, sig.v)
            };

            let tuple_tokens = Token::Tuple(vec![
                t_bytes(&delegator_pubkey)?,
                t_bytes(&delegatee_pubkey_0x04)?,
                t_uint(delegation_start),
                t_uint(delegation_end),
                t_bool(requires_delegatee_sig),
                uuid16.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
                Token::FixedBytes(u256_to_be32(r_delegator)),
                Token::FixedBytes(u256_to_be32(s_delegator)),
                Token::Uint(U256::from(v_delegator)),
                Token::FixedBytes(if must_zero_sigs {
                    vec![0u8; 32]
                } else {
                    u256_to_be32(r_delegatee)
                }),
                Token::FixedBytes(if must_zero_sigs {
                    vec![0u8; 32]
                } else {
                    u256_to_be32(s_delegatee)
                }),
                Token::Uint(U256::from(if must_zero_sigs { 0u64 } else { v_delegatee })),
            ]);
            let data = encode_calldata(func, vec![tuple_tokens])?;
            let (raw, _typed) =
                sign_eip1559(&wallet, chain_id, to_addr, nonce_tx, gas_limit, max_fee, max_prio, data.clone())
                    .await?;
            let decoded = build_decoded(&raw, &to_addr, &data, abi)?;
            (data, raw, decoded)
        }

        "createRevocationEvent" => {
            let owner_pk = it
                .type_b_privkey_x
                .as_ref()
                .ok_or_else(|| anyhow!("TYPE_B_PRIVKEY_X required"))?;
            let wallet = mk_wallet(owner_pk)?;
            let (revokee_pubkey_0x04, must_zero_sigs, revokee_wallet_opt) =
                match (&it.type_b_privkey_y, &it.type_b_pubkey_y) {
                    (Some(pk), _) if !pk.is_empty() => {
                        (uncompressed_pubkey_0x04(&mk_wallet(pk)?), false, Some(mk_wallet(pk)?))
                    }
                    (_, Some(pubk)) if !pubk.is_empty() => (pubk.clone(), true, None),
                    _ => return Err(anyhow!("Provide TYPE_B_PRIVKEY_Y or TYPE_B_PUBKEY_Y")),
                };
            let revoker_pubkey = uncompressed_pubkey_0x04(&wallet);
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
            let msg_hash = keccak256(encoded);
            let sig_revoker = sign_message_eip191(&wallet, msg_hash).await?;
            let (r_revoker, s_revoker, v_revoker) = (sig_revoker.r, sig_revoker.s, sig_revoker.v);
            let (r_revokee, s_revokee, v_revokee) = if must_zero_sigs {
                (U256::from(0u64), U256::from(0u64), 0u64)
            } else {
                let w = revokee_wallet_opt.as_ref().unwrap();
                let sig = sign_message_eip191(w, msg_hash).await?;
                (sig.r, sig.s, sig.v)
            };

            let tuple = Token::Tuple(vec![
                t_bytes(&revoker_pubkey)?,
                t_bytes(&revokee_pubkey_0x04)?,
                t_uint(start),
                t_uint(end),
                uuid16.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
                Token::FixedBytes(u256_to_be32(r_revoker)),
                Token::FixedBytes(u256_to_be32(s_revoker)),
                Token::Uint(U256::from(v_revoker)),
                Token::FixedBytes(if must_zero_sigs {
                    vec![0u8; 32]
                } else {
                    u256_to_be32(r_revokee)
                }),
                Token::FixedBytes(if must_zero_sigs {
                    vec![0u8; 32]
                } else {
                    u256_to_be32(s_revokee)
                }),
                Token::Uint(U256::from(if must_zero_sigs { 0u64 } else { v_revokee })),
            ]);
            let data = encode_calldata(func, vec![tuple])?;
            let (raw, _typed) =
                sign_eip1559(&wallet, chain_id, to_addr, nonce_tx, gas_limit, max_fee, max_prio, data.clone())
                    .await?;
            let decoded = build_decoded(&raw, &to_addr, &data, abi)?;
            (data, raw, decoded)
        }

        "createPermanentInvalidationEvent" => {
            let owner_pk = it
                .type_c_privkey_x
                .as_ref()
                .ok_or_else(|| anyhow!("TYPE_C_PRIVKEY_X required"))?;
            let wallet = mk_wallet(owner_pk)?;
            let invalidated_pubkey = uncompressed_pubkey_0x04(&wallet);
            let uuid16 = bytes16_or_random(None)?;
            let payload = vec![
                t_bytes(&invalidated_pubkey)?,
                uuid16.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
            ];
            let encoded = ethers_core::abi::encode(&payload);
            let msg_hash = keccak256(encoded);
            let sig = sign_message_eip191(&wallet, msg_hash).await?;
            let (r, s, v) = (sig.r, sig.s, sig.v);

            let tuple = Token::Tuple(vec![
                t_bytes(&invalidated_pubkey)?,
                uuid16.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
                Token::FixedBytes(u256_to_be32(r)),
                Token::FixedBytes(u256_to_be32(s)),
                Token::Uint(U256::from(v)),
            ]);
            let data = encode_calldata(func, vec![tuple])?;
            let (raw, _typed) =
                sign_eip1559(&wallet, chain_id, to_addr, nonce_tx, gas_limit, max_fee, max_prio, data.clone())
                    .await?;
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
            // A side (delegation)
            let (delegatee_pubkey_0x04, must_zero_delegatee, delegatee_wallet_opt) =
                match (&it.type_a_privkey_y, &it.type_a_pubkey_y) {
                    (Some(pk), _) if !pk.is_empty() => {
                        (uncompressed_pubkey_0x04(&mk_wallet(pk)?), false, Some(mk_wallet(pk)?))
                    }
                    (_, Some(pubk)) if !pubk.is_empty() => (pubk.clone(), true, None),
                    _ => return Err(anyhow!("Provide TYPE_A_PRIVKEY_Y or TYPE_A_PUBKEY_Y")),
                };
            // B side (revocation)
            let (revokee_pubkey_0x04, must_zero_revokee, revokee_wallet_opt) =
                match (&it.type_b_privkey_y, &it.type_b_pubkey_y) {
                    (Some(pk), _) if !pk.is_empty() => {
                        (uncompressed_pubkey_0x04(&mk_wallet(pk)?), false, Some(mk_wallet(pk)?))
                    }
                    (_, Some(pubk)) if !pubk.is_empty() => (pubk.clone(), true, None),
                    _ => return Err(anyhow!("Provide TYPE_B_PRIVKEY_Y or TYPE_B_PUBKEY_Y")),
                };

            let delegator_pubkey = uncompressed_pubkey_0x04(&wallet);
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
            let hash_a = keccak256(enc_a);
            let sig_a_delegator = sign_message_eip191(&wallet, hash_a).await?;
            let (r_a_del, s_a_del, v_a_del) =
                (sig_a_delegator.r, sig_a_delegator.s, sig_a_delegator.v);
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
            let hash_b = keccak256(enc_b);
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
            let tuple_b = Token::Tuple(vec![
                t_bytes(&delegator_pubkey)?,
                t_bytes(&revokee_pubkey_0x04)?,
                t_uint(b_start),
                t_uint(b_end),
                b_nonce.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
                Token::FixedBytes(u256_to_be32(r_b_rev)),
                Token::FixedBytes(u256_to_be32(s_b_rev)),
                Token::Uint(U256::from(v_b_rev)),
                Token::FixedBytes(if must_zero_revokee {
                    vec![0u8; 32]
                } else {
                    u256_to_be32(r_b_ree)
                }),
                Token::FixedBytes(if must_zero_revokee {
                    vec![0u8; 32]
                } else {
                    u256_to_be32(s_b_ree)
                }),
                Token::Uint(U256::from(if must_zero_revokee { 0u64 } else { v_b_ree })),
            ]);
            let tuple_a = Token::Tuple(vec![
                t_bytes(&delegator_pubkey)?,
                t_bytes(&delegatee_pubkey_0x04)?,
                t_uint(a_start),
                t_uint(a_end),
                t_bool(a_req),
                a_nonce.clone(),
                t_bytes(&it.contract_address.to_ascii_lowercase())?,
                Token::FixedBytes(u256_to_be32(r_a_del)),
                Token::FixedBytes(u256_to_be32(s_a_del)),
                Token::Uint(U256::from(v_a_del)),
                Token::FixedBytes(if must_zero_delegatee {
                    vec![0u8; 32]
                } else {
                    u256_to_be32(r_a_dee)
                }),
                Token::FixedBytes(if must_zero_delegatee {
                    vec![0u8; 32]
                } else {
                    u256_to_be32(s_a_dee)
                }),
                Token::Uint(U256::from(if must_zero_delegatee { 0u64 } else { v_a_dee })),
            ]);

            let data = encode_calldata(func, vec![tuple_b, tuple_a])?;
            let (raw, _typed) =
                sign_eip1559(&wallet, chain_id, to_addr, nonce_tx, gas_limit, max_fee, max_prio, data.clone())
                    .await?;
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let abi = load_abi()?;
    let text = fs::read_to_string(&cli.batch).context("reading batch JSON")?;
    let items: Vec<Item> = serde_json::from_str(&text).context("parsing batch JSON (array)")?;

    let mut out: Vec<BatchEntryOut> = Vec::with_capacity(items.len());
    for (i, it) in items.iter().enumerate() {
        let res = process_item(&abi, &cli, it)
            .await
            .with_context(|| format!("processing item #{} ({})", i, it.function_to_call));
        match res {
            Ok(entry) => out.push(entry),
            Err(e) => return Err(e),
        }
    }

    fs::write(&cli.out, serde_json::to_string_pretty(&out)?)
        .with_context(|| format!("writing {}", cli.out.display()))?;
    println!("✓ Wrote {}", cli.out.display());
    Ok(())
}
