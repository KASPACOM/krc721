use crate::constants::*;
use crate::model::kasplex::v1::krc20::{Op, TokenTransaction};
use crate::model::kasplex::v1::Protocol as KasplexProtocol;

use kaspa_addresses::Address;
use kaspa_txscript::{extract_script_pub_key_address, pay_to_script_hash_script};
use std::str::FromStr;

use super::*;

// ================ KRC-20 ================

// Returns tuple of inscription commit P2SH and script sig.
pub fn token_deploy_demo(pubkey: &secp256k1::PublicKey) -> (Address, Vec<u8>) {
    let transaction: TokenTransaction = TokenTransaction {
        protocol: KasplexProtocol::from_str("krc-20").unwrap(),
        op: Op::Deploy,
        tick: "KASPARTY".to_string(),
        max: Some(100000000000000000),
        limit: Some(100000000000),
        preallocated: Some(100000000000),
        decimal: Some(8),
        amount: None,
        from: None,
        to: None,
        op_score: None,
        hash_rev: None,
        fee_rev: None,
        tx_accept: None,
        op_accept: None,
        op_error: None,
        mts_add: None,
        mts_mod: None,
    };

    let json = serde_json::to_string(&transaction).unwrap();
    println!("{json}");
    let script_sig: Vec<u8> = redeem_pubkey(
        PROTOCOL_KASPLEX_NAMESPACE.as_bytes(),
        json.as_bytes(),
        &pubkey.serialize()[1..33],
    )
    .unwrap();

    ascii_debug_payload(&script_sig);

    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

    let p2sh =
        extract_script_pub_key_address(&redeem_lock_p2sh, "kaspatest".try_into().unwrap()).unwrap();
    (p2sh, script_sig)
}

// Returns tuple of inscription commit P2SH and script sig.
pub fn token_mint_demo(pubkey: &secp256k1::PublicKey) -> (Address, Vec<u8>) {
    let transaction: TokenTransaction = TokenTransaction {
        protocol: KasplexProtocol::from_str("krc-20").unwrap(),
        op: Op::Mint,
        tick: "KASPA".to_string(),
        max: None,
        limit: None,
        preallocated: None,
        decimal: None,
        amount: None,
        from: None,
        to: None,
        op_score: None,
        hash_rev: None,
        fee_rev: None,
        tx_accept: None,
        op_accept: None,
        op_error: None,
        mts_add: None,
        mts_mod: None,
    };

    let json = serde_json::to_string(&transaction).unwrap();
    println!("{json}");
    let script_sig: Vec<u8> = redeem_pubkey(
        PROTOCOL_KASPLEX_NAMESPACE.as_bytes(),
        json.as_bytes(),
        &pubkey.serialize()[1..33],
    )
    .unwrap();

    ascii_debug_payload(&script_sig);

    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

    let p2sh =
        extract_script_pub_key_address(&redeem_lock_p2sh, "kaspatest".try_into().unwrap()).unwrap();
    (p2sh, script_sig)
}

pub fn payload_to_placeholder(payload: &[u8], pubkey: &secp256k1::PublicKey) -> Vec<u8> {
    let needle = &pubkey.serialize()[1..33];

    let position = payload
        .windows(needle.len())
        .position(|window| window == needle)
        .expect("Public key present in payload");

    let placeholder = "{{pubkey}}";

    let mut result = payload.to_owned();
    result.splice(
        position..position + needle.len(),
        placeholder.as_bytes().to_vec(),
    );
    result
}
