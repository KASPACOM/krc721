use crate::constants::*;
use crate::model::krc721::op::Op as Krc721Op;
use crate::model::krc721::{Metadata, Protocol as KsprProtocol, UserOperation};

use kaspa_addresses::Address;
use kaspa_txscript::{extract_script_pub_key_address, pay_to_script_hash_script};

use super::*;

// ================ NFT KRC-721 ================

pub fn nft_deploy_demo(pubkey: &secp256k1::PublicKey) -> (Address, Vec<u8>) {
    let deploy_op = UserOperation {
        protocol: KsprProtocol::Krc721,
        op: Krc721Op::Deploy,
        tick: "KASPARTY".try_into().unwrap(),
        token_id: None,
        metadata: Some(Metadata::Remote("krc721://kaspart/images/".to_string())),
        max: Some(1000),
        to: None,
        royalty_to: None,
        royalty_fee: None,
        daa_mint_start: None,
        discount_fee: None,
        premint: None,
    };

    let json = serde_json::to_string(&deploy_op).unwrap();
    println!("{json}");

    let redeem_script: Vec<u8> = json.into_bytes();
    let protocol = PROTOCOL_KSPR_NAMESPACE.as_bytes().to_owned();

    let script_sig: Vec<u8> = redeem_pubkey(
        &protocol,
        redeem_script.as_slice(),
        &pubkey.serialize()[1..33],
    )
    .unwrap();

    ascii_debug_payload(&script_sig);

    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

    let p2sh =
        extract_script_pub_key_address(&redeem_lock_p2sh, "kaspatest".try_into().unwrap()).unwrap();
    (p2sh, script_sig)
}

// Variant with serialized pubkey param
pub fn nft_deploy_demo_(pubkey: &[u8]) -> (Address, Vec<u8>) {
    let deploy_op = UserOperation {
        protocol: KsprProtocol::Krc721,
        op: Krc721Op::Deploy,
        tick: "Y".try_into().unwrap(),
        token_id: None,
        metadata: Some(Metadata::Remote("//////////".to_string())),
        max: Some(1000),
        to: None,
        royalty_to: None,
        royalty_fee: None,
        daa_mint_start: None,
        discount_fee: None,
        premint: None,
    };

    let json = serde_json::to_string(&deploy_op).unwrap();
    println!("{json}");

    let redeem_script: Vec<u8> = json.into_bytes();
    let protocol = PROTOCOL_KSPR_NAMESPACE.as_bytes().to_owned();

    let script_sig: Vec<u8> = redeem_pubkey(
        &protocol,
        redeem_script.as_slice(),
        pubkey, //.serialize()[1..33],
    )
    .unwrap();

    ascii_debug_payload(&script_sig);

    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

    let p2sh =
        extract_script_pub_key_address(&redeem_lock_p2sh, "kaspatest".try_into().unwrap()).unwrap();
    (p2sh, script_sig)
}

// Returns tuple of inscription commit P2SH and script sig.
pub fn nft_mint_demo(pubkey: &secp256k1::PublicKey) -> (Address, Vec<u8>) {
    let mint_op = UserOperation {
        protocol: KsprProtocol::Krc721,
        op: Krc721Op::Mint,
        tick: "KASPARTY".try_into().unwrap(),
        token_id: None,
        metadata: None,
        max: None,
        to: None,
        royalty_to: None,
        royalty_fee: None,
        daa_mint_start: None,
        discount_fee: None,
        premint: None,
    };

    let json = serde_json::to_string(&mint_op).unwrap();
    println!("{json}");

    let redeem_script: Vec<u8> = json.into_bytes();
    let protocol = PROTOCOL_KSPR_NAMESPACE.as_bytes().to_owned();

    let script_sig: Vec<u8> = redeem_pubkey(
        &protocol,
        redeem_script.as_slice(),
        &pubkey.serialize()[1..33],
    )
    .unwrap();

    ascii_debug_payload(&script_sig);

    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

    let p2sh =
        extract_script_pub_key_address(&redeem_lock_p2sh, "kaspatest".try_into().unwrap()).unwrap();
    (p2sh, script_sig)
}

// Variant with serialized pubkey param
pub fn nft_mint_demo_(pubkey: &[u8]) -> (Address, Vec<u8>) {
    let mint_op = UserOperation {
        protocol: KsprProtocol::Krc721,
        op: Krc721Op::Mint,
        tick: "Y".try_into().unwrap(),
        token_id: None,
        metadata: None,
        max: None,
        to: None,
        royalty_to: None,
        royalty_fee: None,
        daa_mint_start: None,
        discount_fee: None,
        premint: None,
    };

    let json = serde_json::to_string(&mint_op).unwrap();
    println!("{json}");

    let redeem_script: Vec<u8> = json.into_bytes();
    let protocol = PROTOCOL_KSPR_NAMESPACE.as_bytes().to_owned();

    let script_sig: Vec<u8> = redeem_pubkey(
        &protocol,
        redeem_script.as_slice(),
        pubkey,
        // &pubkey.serialize()[1..33],
    )
    .unwrap();

    ascii_debug_payload(&script_sig);

    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

    let p2sh =
        extract_script_pub_key_address(&redeem_lock_p2sh, "kaspatest".try_into().unwrap()).unwrap();
    (p2sh, script_sig)
}

// Returns tuple of inscription commit P2SH and script sig.
pub fn nft_transfer_demo(pubkey: &secp256k1::PublicKey) -> (Address, Vec<u8>) {
    let mint_op: UserOperation = UserOperation {
        protocol: KsprProtocol::Krc721,
        op: Krc721Op::Transfer,
        tick: "KASPARTY".try_into().unwrap(),
        token_id: Some(74),
        metadata: None,
        max: None,
        to: Some("kaspa:qqabb6cz...".to_string()),
        royalty_to: None,
        royalty_fee: None,
        daa_mint_start: None,
        discount_fee: None,
        premint: None,
    };

    let json = serde_json::to_string(&mint_op).unwrap();
    println!("{json}");

    let redeem_script: Vec<u8> = json.into_bytes();
    let protocol = PROTOCOL_KSPR_NAMESPACE.as_bytes().to_owned();

    let script_sig: Vec<u8> = redeem_pubkey(
        &protocol,
        redeem_script.as_slice(),
        &pubkey.serialize()[1..33],
    )
    .unwrap();

    ascii_debug_payload(&script_sig);

    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);

    let p2sh =
        extract_script_pub_key_address(&redeem_lock_p2sh, "kaspatest".try_into().unwrap()).unwrap();
    (p2sh, script_sig)
}

// ================ END NFT KRC-721 ================
