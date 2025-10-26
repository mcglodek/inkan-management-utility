use serde::{Deserialize, Serialize};

/// Batch input items (verbatim field names from your examples)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct Item {
    pub function_to_call: String,
    pub nonce: Option<u64>,
    pub chain_id: Option<u64>,
    pub contract_address: String,

    // A
    pub type_a_privkey_x: Option<String>,
    pub type_a_privkey_y: Option<String>,
    pub type_a_pubkey_y: Option<String>,
    pub type_a_uint_x: Option<u64>,
    pub type_a_uint_y: Option<u64>,
    pub type_a_boolean: Option<String>,

    // B
    pub type_b_privkey_x: Option<String>,
    pub type_b_privkey_y: Option<String>,
    pub type_b_pubkey_y: Option<String>,
    pub type_b_uint_x: Option<u64>,
    pub type_b_uint_y: Option<u64>,

    // C
    pub type_c_privkey_x: Option<String>,
}

/// Output shapes
#[derive(Debug, Serialize)]
pub struct BatchEntryOut {
    #[serde(rename = "signedTx")]
    pub signed_tx: String,
    #[serde(rename = "decodedTx")]
    pub decoded_tx: DecodedTxOut,
}

#[derive(Debug, Serialize)]
pub struct DecodedTxOut {
    pub from: String,
    pub to: String,
    pub value: String,
    pub gasLimit: String,
    pub nonce: u64,
    pub chainId: String,
    pub maxFeePerGas: String,
    pub maxPriorityFeePerGas: String,
    pub funcName: String,
    pub encodedData: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decodedData: Option<DecodedOne>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decodedDataTypeA: Option<DelegationDecodedOrdered>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decodedDataTypeB: Option<RevocationDecodedOrdered>,
}

/// Ordered decoded output structs (to guarantee field order in JSON)
#[derive(Debug, Serialize)]
pub struct DelegationDecodedOrdered {
    pub delegatorPubkey: String,
    pub delegateePubkey: String,
    pub delegationStartTime: String,
    pub delegationEndTime: String,
    pub doesRevocationRequireDelegateeSignature: bool,
    pub nonce: String,
    pub expectedAddressOfDeployedContract: String,
    pub rDelegatorPubkeySig: String,
    pub sDelegatorPubkeySig: String,
    pub vDelegatorPubkeySig: String,
    pub rDelegateePubkeySig: String,
    pub sDelegateePubkeySig: String,
    pub vDelegateePubkeySig: String,
}

#[derive(Debug, Serialize)]
pub struct RevocationDecodedOrdered {
    pub revokerPubkey: String,
    pub revokeePubkey: String,
    pub revocationStartTime: String,
    pub revocationEndTime: String,
    pub nonce: String,
    pub expectedAddressOfDeployedContract: String,
    pub rRevokerPubkeySig: String,
    pub sRevokerPubkeySig: String,
    pub vRevokerPubkeySig: String,
    pub rRevokeePubkeySig: String,
    pub sRevokeePubkeySig: String,
    pub vRevokeePubkeySig: String,
}

#[derive(Debug, Serialize)]
pub struct InvalidationDecodedOrdered {
    pub invalidatedPubkey: String,
    pub nonce: String,
    pub expectedAddressOfDeployedContract: String,
    pub rInvalidatedPubkeySig: String,
    pub sInvalidatedPubkeySig: String,
    pub vInvalidatedPubkeySig: String,
}

/// Untagged enum so `decodedData` can be one of the three ordered shapes
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum DecodedOne {
    Delegation(DelegationDecodedOrdered),
    Revocation(RevocationDecodedOrdered),
    Invalidation(InvalidationDecodedOrdered),
}

