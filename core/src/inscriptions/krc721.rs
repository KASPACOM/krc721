use crate::constants::*;
use crate::model::krc721::op::Op as Krc721Op;
use crate::model::krc721::{Metadata, Protocol as KsprProtocol, UserOperation};

use kaspa_addresses::Address;
use kaspa_txscript::{extract_script_pub_key_address, pay_to_script_hash_script};

use super::*;

// ================ MARKETPLACE P2SH ================

/// Compute the deterministic P2SH address for a listing.
///
/// Following the Kasplex pattern: the listing P2SH address is derived from
/// a redeem script containing the seller's pubkey + OP_CHECKSIG + kspr header + send inscription JSON.
/// This means only a transaction that provides the seller's signature AND embeds
/// the correct SEND inscription can spend this UTXO.
///
/// Returns (P2SH address, full redeem script bytes)
pub fn compute_listing_p2sh(
    seller_pubkey: &[u8],
    tick: &str,
    token_id: u64,
    prefix: kaspa_addresses::Prefix,
) -> (Address, Vec<u8>) {
    // Build the send inscription JSON that will be embedded in the redeem script
    // Note: tick is lowercased to match Kasplex convention
    let send_json = format!(
        r#"{{"p":"krc-721","op":"send","tick":"{}","tokenId":"{}"}}"#,
        tick.to_lowercase(),
        token_id
    );

    let redeem_script_payload: Vec<u8> = send_json.into_bytes();
    let protocol = PROTOCOL_KSPR_NAMESPACE.as_bytes().to_owned();

    // Build the full redeem script: <pubkey> OP_CHECKSIG OP_FALSE OP_IF <"kspr"> OP_0 <send_json> OP_ENDIF
    let script_sig: Vec<u8> = super::redeem_pubkey(
        &protocol,
        redeem_script_payload.as_slice(),
        seller_pubkey,
    )
    .expect("failed to build listing redeem script");

    let redeem_lock_p2sh = pay_to_script_hash_script(&script_sig);
    let p2sh_addr = extract_script_pub_key_address(&redeem_lock_p2sh, prefix)
        .expect("failed to extract P2SH address");

    (p2sh_addr, script_sig)
}

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
        price: None,
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
        price: None,
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
        price: None,
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
        price: None,
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
        price: None,
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
