use crate::imports::*;
use kaspa_addresses::{AddressError, Version};
use kaspa_consensus_core::hashing::sighash::{SigHashReusedValues, SigHashReusedValuesSync};
use kaspa_consensus_core::tx::{ScriptPublicKey, ScriptVec, TransactionId};
// use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionId};
// use kaspa_txscript::opcodes::codes::{OpCheckSig, OpCheckSigECDSA};
use ahash::AHashMap;
use kaspa_txscript::opcodes::codes::{
    OpCheckSig, OpCheckSigECDSA, OpData32, OpData33, OpEndIf, OpFalse, OpIf, OpTrue,
};
use kaspa_txscript::pay_to_address_script;
use kaspa_txscript::script_class::ScriptClass;
use krc721_core::error::TickError;
use krc721_core::inscriptions::ascii_debug_payload;
use krc721_core::model::kasplex;
use krc721_core::model::krc721::{
    DeployInfo, DiscountInfo, MintInfo, Op, Operation, OperationCommon, OperationInfo,
    RoyaltyDetails, Tick, TransferInfo, UserOperation,
};
use serde_json::from_slice;
use smallvec::SmallVec;
use std::fmt::Debug;
use std::iter::once;
use thiserror::Error;
use tracing::{debug, error, instrument, trace, warn};

#[derive(Debug)]
pub struct ContextTransaction {
    pub tx: Transaction,
    pub fee: u64,
    pub block_time: u64,
    pub accepting_block_daa_score: u64,
    pub index_within_merged_block: usize,
}

// Wrapper around transaction to handle multiple cases.
pub trait ITransaction: Debug {
    fn signature_script(&self) -> Option<&[u8]>;
    fn receiver_addr(&self, prefix: Prefix) -> Option<Address>;
    fn first_beneficiary(&self) -> Option<ScriptPublicKey>;
    fn id(&self) -> TransactionId;
    fn first_output_amt(&self) -> Option<u64>;
    fn fee(&self) -> u64;
    fn block_time(&self) -> u64;

    fn accepting_block_daa_score(&self) -> u64;
}

#[derive(Error, Debug)]
pub enum AnalyzerError {
    #[error(transparent)]
    TxScript(#[from] TxScriptError),
    #[error("Expected more than 1 opcode for script sig")]
    InsufficientOpcodeLen,
    #[error("Opcode has no data")]
    EmptyOpcodeData,
    #[error("Opcode is not push opcode")]
    OpcodeIsNotPush,
    #[error("Missing krc721 header")]
    MissingKrc721Header,
    #[error("Expected more than 1 opcode for inner(redeem) script")]
    UnsupportedEnvelopeLength,
    #[error("Parsing inscription payload: {0}")]
    SerdeWithPath(#[from] serde_path_to_error::Error<serde_json::Error>),
    #[error("NFT operation model field '{field_name}' value parsing error: {error}")]
    NFTInscriptionModelFieldValueParsingError {
        field_name: &'static str,
        error: String,
    },
    #[error(transparent)]
    Tick(#[from] TickError),
    #[error("Missing mandatory value for Transfer operation: {0}")]
    OpTransferMissingValue(&'static str),
    #[error("Insufficient mint fee: {0}")]
    InsufficientMintFee(u64),
    #[error("Insufficient deploy fee: {0}")]
    InsufficientDeployFee(u64),
    #[error("Missing mandatory value for Deploy operation: {0}")]
    OpDeployMissingValue(&'static str),
    #[error("Unknown sender type")]
    UnknownSenderType,
    #[error("Invalid redeem envelope format: {0}")]
    InvalidRedeemEnvelopeFormat(&'static str),
    #[error(
        "Tick {tick} is restricted. Expected deployer spk: {expected_deployer:?}, actual: {deployer:?}"
    )]
    RestrictedTickDeploy {
        tick: Tick,
        expected_deployer: ScriptPublicKey,
        deployer: ScriptPublicKey,
    },
    #[error("Royalty fee must be greater than or equal to 0.1 KAS")]
    UnderflowRoyaltyFee,
    #[error("Royalty fee must not be greater than 10_000_000 KAS")]
    OverflowRoyaltyFee,
    #[error("Missing mandatory value for Discount operation: {0}")]
    OpDiscountMissingValue(&'static str),
    #[error("Premint is greater than max supply: {0}")]
    PremintGreaterThanMax(u64),
    #[error("Max supply must be less than `u64::MAX`")]
    MaxSupplyOverflow,
    #[error("Invalid URI protocol prefix, expected one of: {0:?}")]
    RestrictedMetadataProtocol(Arc<[String]>),
    #[error(transparent)]
    AddressError(#[from] kaspa_addresses::AddressError),
}

type ReservedTokenMap = AHashMap<Tick, ScriptPublicKey>;

pub struct Analyzer {
    db: Option<Arc<Db>>,
    restricted_tokens: ReservedTokenMap,
    address_prefix: Prefix,
    restricted_protocols: Arc<[String]>,
    daa_ecdsa_fix: u64,
}

impl Analyzer {
    pub fn new(
        db: Option<Arc<Db>>,
        restricted_tokens: ReservedTokenMap,
        address_prefix: Prefix,
        restricted_protocols: Arc<[String]>,
        daa_ecdsa_fix: u64,
    ) -> Self {
        Self {
            db,
            restricted_tokens,
            address_prefix,
            restricted_protocols,
            daa_ecdsa_fix,
        }
    }

    pub fn db(&self) -> &Option<Arc<Db>> {
        &self.db
    }

    /// Detects KRC-721 operations in transaction signature scripts.
    ///
    /// # Arguments
    /// * `sigtx` - Transaction containing potential KRC-721 operations
    ///
    /// # Returns
    /// * `Ok(Some(Operation))` if a valid KRC-721 operation is found
    /// * `Ok(None)` if no KRC-721 operation is present
    /// * `Err(AnalyzerError)` if parsing fails
    #[instrument(skip_all, fields(tx_id = %sigtx.id()))]
    pub fn detect_krc721(
        &self,
        sigtx: &impl ITransaction,
    ) -> Result<Option<Operation>, AnalyzerError> {
        let Some(signature_script) = sigtx.signature_script() else {
            trace!("signature_script doesn't exist");
            return Ok(None);
        };
        if !detect_kspr_header(signature_script) {
            trace!("signature_script doesn't have kspr header");
            return Ok(None);
        } else {
            trace!("KRC-721 header detected");
        }
        let mut opcodes_iter = parse_script(signature_script);
        let second_main_opcode: Option<
            std::result::Result<
                Box<dyn OpCodeImplementation<PopulatedTransaction, SigHashReusedValuesSync>>,
                TxScriptError,
            >,
        > = opcodes_iter.nth(1);
        let Some(opcode) = second_main_opcode
            .transpose()
            .inspect_err(|err| error!("parsing opcode failed: {err}"))?
        else {
            return Err(AnalyzerError::InsufficientOpcodeLen);
        };
        if opcode.is_empty() {
            return Err(AnalyzerError::EmptyOpcodeData);
        }
        if !opcode.is_push_opcode() {
            return Err(AnalyzerError::MissingKrc721Header);
        }
        if !detect_krc721_header(opcode.get_data()) {
            return Err(AnalyzerError::MissingKrc721Header);
        }

        let inner_opcodes =
            parse_script::<PopulatedTransaction, SigHashReusedValuesSync>(opcode.get_data())
                .collect::<Result<Vec<_>, _>>()
                .map_err(AnalyzerError::TxScript)?;

        validate_redeem_envelope(&inner_opcodes)?;

        let first_opcode = inner_opcodes[0].as_ref();
        let second_opcode = inner_opcodes[1].as_ref();

        let second_to_last_opcode = inner_opcodes[inner_opcodes.len() - 2].as_ref();

        if !second_to_last_opcode.is_push_opcode() {
            return Err(AnalyzerError::InvalidRedeemEnvelopeFormat(
                "Content op is not push opcode",
            ));
        }

        sigtx
            .receiver_addr(self.address_prefix)
            .inspect(|address| debug!("KRC-721 Receiver addr {address:}"));
        let jd = &mut serde_json::Deserializer::from_slice(second_to_last_opcode.get_data());

        let model = serde_path_to_error::deserialize::<_, UserOperation>(jd)?;
        trace!("KRC-721 operation {}", model.op);

        // "To" field rule based on operation type for KRC20 / not necessarily KRC721.
        //
        // If not configured per "to" field, the transaction sender address is used
        // - as deployer in case of a token deployment, and receiver of optional
        //   pre-allocation (relative to token standard "pre" field)
        // - receiver of token in case of a mint operation ()
        //
        // In case of a transfer, the "to" field is mandatory as the sender address
        // is the token holder.

        // todo can be done during deserialization
        // Resolve provided optional addresses.
        let resolved_to = model
            .to
            .clone()
            .map(Address::try_from)
            .transpose()
            .map_err(
                |e| AnalyzerError::NFTInscriptionModelFieldValueParsingError {
                    field_name: "to",
                    error: e.to_string(),
                },
            )?
            .map(|address| {
                if address.prefix != self.address_prefix {
                    Err(AnalyzerError::NFTInscriptionModelFieldValueParsingError {
                        field_name: "to",
                        error: "network mismatch".to_string(),
                    })
                } else {
                    Ok(address)
                }
            })
            .transpose()?;

        let to_param = resolved_to.map(|a| pay_to_address_script(&a));

        let resolved_royalty_to = model
            .royalty_to
            .clone()
            .map(Address::try_from)
            .transpose()
            .map_err(
                |e| AnalyzerError::NFTInscriptionModelFieldValueParsingError {
                    field_name: "royalty_to",
                    error: e.to_string(),
                },
            )?
            .map(|address| {
                if address.prefix != self.address_prefix {
                    Err(AnalyzerError::NFTInscriptionModelFieldValueParsingError {
                        field_name: "royalty_to",
                        error: "network mismatch".to_string(),
                    })
                } else {
                    Ok(address)
                }
            })
            .transpose()?;

        // The reveal transaction sender.
        let (sender, sender_address) = {
            #[allow(non_upper_case_globals)]
            match second_opcode.value() {
                OpCheckSig => {
                    let payload = first_opcode.get_data();
                    let script = pay_to_pub_key(payload);
                    (
                        ScriptPublicKey::new(ScriptClass::from(Version::PubKey).version(), script),
                        Address::new(
                            self.address_prefix,
                            kaspa_addresses::Version::PubKey,
                            payload,
                        ),
                    )
                }
                // bug compatibility according to previous logic
                OpCheckSigECDSA if sigtx.accepting_block_daa_score() < self.daa_ecdsa_fix => {
                    return Err(AnalyzerError::AddressError(AddressError::InvalidAddress))
                }
                OpCheckSigECDSA => {
                    let payload = first_opcode.get_data();
                    let script = pay_to_pub_key_ecdsa(payload);
                    (
                        ScriptPublicKey::new(
                            ScriptClass::from(Version::PubKeyECDSA).version(),
                            script,
                        ),
                        Address::new(
                            self.address_prefix,
                            kaspa_addresses::Version::PubKeyECDSA,
                            payload,
                        ),
                    )
                }
                _ => return Err(AnalyzerError::UnknownSenderType),
            }
        };

        // Royalties beneficiary resolved from "royaltyTo" field, default is reveal sender address.
        let royalty_to = resolved_royalty_to
            .map(|a| pay_to_address_script(&a))
            .unwrap_or(sender.clone());

        let tx_id = sigtx.id();
        let block_time = sigtx.block_time();
        // todo: generally (not only here) handle the other address variants besides public key ^ (multisig e.g.)
        trace!("TX reveal sender address: {}", sender_address.to_string());
        // Name of the NFT collection.
        // todo must be done during serialization
        let tick = Tick::from_str(model.tick.as_str())?;
        trace!("Tick: {}", tick.to_string());
        match model.op {
            Op::Discount => {
                let fee: u64 = model
                    .discount_fee
                    .ok_or(AnalyzerError::OpDiscountMissingValue("discount_fee"))?;

                let to: ScriptPublicKey =
                    to_param.ok_or(AnalyzerError::OpDiscountMissingValue("to"))?;
                Ok(Some(Operation {
                    common: OperationCommon {
                        tick,
                        tx_id,
                        block_time,
                        sender,
                        fee: sigtx.fee(),
                        accepting_block_daa_score: sigtx.accepting_block_daa_score(),
                    },
                    info: OperationInfo::Discount(DiscountInfo { to, fee }),
                }))
            }
            Op::Transfer => {
                // todo must be done during deser
                let token_id = model
                    .token_id
                    .ok_or(AnalyzerError::OpTransferMissingValue("token_id"))?;
                let transfer_to = to_param.ok_or(AnalyzerError::OpTransferMissingValue("to"))?;
                Ok(Some(Operation {
                    common: OperationCommon {
                        tick,
                        tx_id,
                        block_time,
                        sender,
                        fee: sigtx.fee(),
                        accepting_block_daa_score: sigtx.accepting_block_daa_score(),
                    },
                    info: OperationInfo::Transfer(TransferInfo {
                        token_id,
                        to: transfer_to,
                    }),
                }))
            }
            Op::Mint => {
                if sigtx.fee() < KSPR_FEE_MINT {
                    trace!(
                        "Validation failed: insufficient mint fee {} < {}",
                        sigtx.fee(),
                        KSPR_FEE_MINT
                    );
                    Err(AnalyzerError::InsufficientMintFee(sigtx.fee()))
                } else {
                    Ok(Some(Operation {
                        common: OperationCommon {
                            tick,
                            tx_id,
                            block_time,
                            sender: sender.clone(),
                            fee: sigtx.fee(),
                            accepting_block_daa_score: sigtx.accepting_block_daa_score(),
                        },
                        info: OperationInfo::Mint(MintInfo {
                            token_id: 0, // Note: this is determined by algorithm
                            to: to_param.unwrap_or(sender),
                            // // Note: always return possible royalty details because no deploy data for tick in scope to verify
                            royalty: sigtx
                                .first_output_amt()
                                .zip(sigtx.first_beneficiary())
                                .map(|(fee, beneficiary)| RoyaltyDetails { beneficiary, fee }),
                        }),
                    }))
                }
            }
            Op::Deploy => {
                match self.restricted_tokens.get(&tick) {
                    Some(expected) if expected != &sender => {
                        return Err(AnalyzerError::RestrictedTickDeploy {
                            tick,
                            expected_deployer: expected.clone(),
                            deployer: sender,
                        })
                    }
                    _ => {}
                }

                // todo must be done during deser
                let metadata = model
                    .metadata
                    .ok_or(AnalyzerError::OpDeployMissingValue("metadata"))?;
                if metadata.has_incompatible_uri_prefix(self.restricted_protocols.clone()) {
                    return Err(AnalyzerError::RestrictedMetadataProtocol(
                        self.restricted_protocols.clone(),
                    ));
                }
                let max = {
                    let max = model
                        .max
                        .ok_or(AnalyzerError::OpDeployMissingValue("max"))?;
                    if max == u64::MAX {
                        return Err(AnalyzerError::MaxSupplyOverflow);
                    } else {
                        max
                    }
                };

                let premint = model.premint.unwrap_or_default();

                if premint > max {
                    return Err(AnalyzerError::PremintGreaterThanMax(max));
                }

                let total_deploy_fee = KSPR_FEE_DEPLOY + (premint * KSPR_FEE_MINT);
                if sigtx.fee() < total_deploy_fee {
                    trace!(
                        "Validation failed: insufficient deploy fee {} < {}",
                        sigtx.fee(),
                        total_deploy_fee
                    );
                    Err(AnalyzerError::InsufficientDeployFee(sigtx.fee()))
                } else {
                    Ok(Some(Operation {
                        common: OperationCommon {
                            tick,
                            tx_id,
                            block_time,
                            sender: sender.clone(),
                            fee: sigtx.fee(),
                            accepting_block_daa_score: sigtx.accepting_block_daa_score(),
                        },
                        info: OperationInfo::Deploy(DeployInfo {
                            metadata,
                            max,
                            deployer: to_param.unwrap_or(sender),
                            royalty: model
                                .royalty_fee
                                .map(|fee| {
                                    if fee < MIN_ROYALTY_FEE {
                                        Err(AnalyzerError::UnderflowRoyaltyFee)
                                    } else if fee > MAX_ROYALTY_FEE {
                                        Err(AnalyzerError::OverflowRoyaltyFee)
                                    } else {
                                        Ok(RoyaltyDetails {
                                            beneficiary: royalty_to,
                                            fee,
                                        })
                                    }
                                })
                                .transpose()?,
                            mint_start_daa: model.daa_mint_start.unwrap_or(0),
                            premint,
                        }),
                    }))
                }
            }
        }
    }
}

#[inline]
fn window_find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    // Ensure we don't start beyond the end of the haystack
    let offset = 10;
    if haystack.len() <= offset {
        return None;
    }

    // Optization: iterate starting from the nth byte
    for (position, window) in haystack[offset..].windows(needle.len()).enumerate() {
        if window == needle {
            return Some(position + offset); // Adjust the position to account for the byte offset
        }
    }
    None
}

fn parse_script<T: VerifiableTransaction, U: SigHashReusedValues>(
    script: &[u8],
) -> impl Iterator<Item = std::result::Result<Box<dyn OpCodeImplementation<T, U>>, TxScriptError>> + '_
{
    script.iter().batching(|it| deserialize_next_opcode(it))
}

impl ITransaction for ContextTransaction {
    fn signature_script(&self) -> Option<&[u8]> {
        self.tx
            .inputs
            .first()
            .map(|input| input.signature_script.as_slice())
    }

    fn receiver_addr(&self, prefix: Prefix) -> Option<Address> {
        self.tx.outputs.first().and_then(|v| {
            extract_script_pub_key_address(&v.script_public_key, prefix)
                .inspect_err(|err| error!("parsing address error: {err}"))
                .ok()
        })
        // todo return error
    }

    fn first_beneficiary(&self) -> Option<ScriptPublicKey> {
        self.tx
            .outputs
            .first()
            .map(|output| output.script_public_key.clone())
    }

    fn id(&self) -> TransactionId {
        self.tx.id()
    }

    fn first_output_amt(&self) -> Option<u64> {
        self.tx.outputs.first().map(|v| v.value)
    }

    fn fee(&self) -> u64 {
        self.fee
    }

    fn block_time(&self) -> u64 {
        self.block_time
    }

    fn accepting_block_daa_score(&self) -> u64 {
        self.accepting_block_daa_score
    }
}

fn detect_krc20_header(haystack: &[u8]) -> bool {
    window_find(haystack, KRC20_HEADER_UC).is_some()
        || window_find(haystack, KRC20_HEADER_LC).is_some()
}

fn detect_krc721_header(haystack: &[u8]) -> bool {
    window_find(haystack, KRC721_HEADER_UC).is_some()
        || window_find(haystack, KRC721_HEADER_LC).is_some()
}

fn detect_kasplex_header(haystack: &[u8]) -> bool {
    window_find(haystack, KASPLEX_HEADER_LC).is_some()
        || window_find(haystack, KASPLEX_HEADER_UC).is_some()
}

fn detect_kspr_header(haystack: &[u8]) -> bool {
    let derive_lc = KSPR_U8_STRICT;
    let derive_uc = KSPR_U8_STRICT_UC;
    window_find(haystack, derive_lc).is_some() || window_find(haystack, derive_uc).is_some()
}

pub fn detect_krc20<T: ITransaction>(sigtx: T) -> Option<TokenTransaction> {
    let mut inscription: Option<TokenTransaction> = None;

    if let Some(signature_script) = sigtx.signature_script() {
        if detect_kasplex_header(signature_script) {
            // Get the second opcode
            let mut opcodes_iter = parse_script(signature_script);
            let second_opcode: Option<
                std::result::Result<
                    Box<dyn OpCodeImplementation<PopulatedTransaction, SigHashReusedValuesSync>>,
                    TxScriptError,
                >,
            > = opcodes_iter.nth(1);

            // debug!("------------------ {} {}", sigtx.gas(), sigtx.mass());

            match second_opcode {
                Some(Ok(opcode)) => {
                    if !opcode.is_empty()
                        && opcode.is_push_opcode()
                        && detect_krc20_header(opcode.get_data())
                    {
                        let inner_opcodes: Vec<_> = parse_script::<
                            PopulatedTransaction,
                            SigHashReusedValuesSync,
                        >(opcode.get_data())
                        .collect();
                        if inner_opcodes.len() >= 2 {
                            if let Some(Ok(second_to_last_opcode)) =
                                inner_opcodes.get(inner_opcodes.len() - 2)
                            {
                                ascii_debug_payload(second_to_last_opcode.get_data());

                                match from_slice::<TokenTransaction>(
                                    second_to_last_opcode.get_data(),
                                ) {
                                    Ok(token_transaction) => {
                                        // Debug
                                        if token_transaction.op == kasplex::v1::krc20::Op::Mint {
                                            debug!("KRC-20 Mint");
                                            ascii_debug_payload(opcode.get_data());
                                        }
                                        // Debug
                                        if token_transaction.op == kasplex::v1::krc20::Op::Transfer
                                        {
                                            debug!("KRC-20 Transfer");
                                            ascii_debug_payload(opcode.get_data());
                                        }
                                        // Debug
                                        if token_transaction.op == kasplex::v1::krc20::Op::Deploy {
                                            debug!("KRC-20 Deploy");
                                            ascii_debug_payload(opcode.get_data());
                                        }
                                        // Debug
                                        if token_transaction.has_tick("toitoi") {
                                            ascii_debug_payload(opcode.get_data());
                                        }

                                        inscription = Some(token_transaction);
                                    }
                                    Err(e) => {
                                        ascii_debug_payload(second_to_last_opcode.get_data());

                                        // Handle the error if necessary
                                        warn!("Failed to deserialize: {:?}", e);
                                    }
                                }
                            }
                        }
                    }
                }
                Some(Err(e)) => {
                    // Handle the error
                    error!("Error while parsing opcodes: {:?}", e);
                }
                None => {
                    // Handle the case where there are fewer than two opcodes
                    warn!("There are fewer than two opcodes in the script.");
                }
            }
        }
    }

    inscription
}

type Opcode<'a> = Box<dyn OpCodeImplementation<PopulatedTransaction<'a>, SigHashReusedValuesSync>>;

/// Validates the common opcodes that appear in both 8 and 10 opcode cases
fn validate_common_opcodes(inner_opcodes: &[Opcode]) -> Result<(), AnalyzerError> {
    // Validate OpFalse
    if inner_opcodes[2].as_ref().value() != OpFalse {
        return Err(AnalyzerError::InvalidRedeemEnvelopeFormat(
            "OpFalse missing",
        ));
    }

    // Validate OpIf
    if inner_opcodes[3].as_ref().value() != OpIf {
        return Err(AnalyzerError::InvalidRedeemEnvelopeFormat("OpIf missing"));
    }

    // Validate protocol namespace
    let protocol_opcode: &dyn OpCodeImplementation<
        PopulatedTransaction<'_>,
        SigHashReusedValuesSync,
    > = inner_opcodes[4].as_ref();
    if !protocol_opcode.is_push_opcode()
        || protocol_opcode.get_data() != PROTOCOL_KSPR_NAMESPACE.as_bytes()
    {
        return Err(AnalyzerError::InvalidRedeemEnvelopeFormat(
            "Invalid or missing protocol",
        ));
    }

    Ok(())
}

/// Validates the redeem envelope format for KRC-721 operations
fn validate_redeem_envelope(inner_opcodes: &[Opcode]) -> Result<(), AnalyzerError> {
    match inner_opcodes.len() {
        8 => {
            validate_common_opcodes(inner_opcodes)?;
            validate_mandatory_content(&inner_opcodes[5..8])
        }
        10 => {
            validate_common_opcodes(inner_opcodes)?;
            validate_optional_content(&inner_opcodes[5..7])?;
            validate_mandatory_content(&inner_opcodes[7..10])
        }
        _ => Err(AnalyzerError::UnsupportedEnvelopeLength),
    }
}

/// Validates the optional content section (present only in 10 opcode case)
fn validate_optional_content(opcodes: &[Opcode]) -> Result<(), AnalyzerError> {
    // Validate optional content marker (OpTrue)
    if opcodes[0].as_ref().value() != OpTrue {
        return Err(AnalyzerError::InvalidRedeemEnvelopeFormat(
            "Missing optional content marker",
        ));
    }

    // Validate optional content
    if opcodes[1].as_ref().is_empty() || !opcodes[1].as_ref().is_push_opcode() {
        return Err(AnalyzerError::InvalidRedeemEnvelopeFormat(
            "Missing optional content",
        ));
    }

    Ok(())
}

/// Validates the mandatory content section (present in both 8 and 10 opcode cases)
fn validate_mandatory_content(opcodes: &[Opcode]) -> Result<(), AnalyzerError> {
    // Validate mandatory content marker (OpFalse)
    if opcodes[0].as_ref().value() != OpFalse {
        return Err(AnalyzerError::InvalidRedeemEnvelopeFormat(
            "Missing mandatory content marker",
        ));
    }

    // Validate mandatory content
    if opcodes[1].as_ref().is_empty() || !opcodes[1].as_ref().is_push_opcode() {
        return Err(AnalyzerError::InvalidRedeemEnvelopeFormat(
            "Missing mandatory content",
        ));
    }

    // Validate final opcode (must be OpFalse)
    if opcodes[2].as_ref().value() != OpEndIf {
        return Err(AnalyzerError::InvalidRedeemEnvelopeFormat(
            "Content marker invalid",
        ));
    }

    Ok(())
}

/// Creates a new script to pay a transaction output to a 32-byte pubkey.
fn pay_to_pub_key(address_payload: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(address_payload.len(), 32);
    SmallVec::from_iter(
        once(OpData32)
            .chain(address_payload.iter().copied())
            .chain(once(OpCheckSig)),
    )
}

/// Creates a new script to pay a transaction output to a 33-byte ECDSA pubkey.
fn pay_to_pub_key_ecdsa(address_payload: &[u8]) -> ScriptVec {
    // TODO: use ScriptBuilder when add_op and add_data fns or equivalents are available
    assert_eq!(address_payload.len(), 33);
    SmallVec::from_iter(
        once(OpData33)
            .chain(address_payload.iter().copied())
            .chain(once(OpCheckSigECDSA)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;
    use kaspa_addresses::Version;
    use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
    use kaspa_consensus_core::{
        subnets::SubnetworkId,
        tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput},
    };
    use kaspa_txscript::script_builder::{ScriptBuilder, ScriptBuilderResult};
    use krc721_core::{
        inscriptions::redeem_script_hash_signature_script,
        model::krc721::{Metadata, Op, Protocol, UserOperation},
    };

    fn detect_krc721(tx: impl ITransaction) -> Result<Option<Operation>, AnalyzerError> {
        let analyzer = Analyzer::new(
            None,
            Default::default(),
            Prefix::Testnet,
            Arc::new([
                "krc721".to_string(),
                "kspr721".to_string(),
                "ipfs".to_string(),
            ]),
            0,
        );
        analyzer.detect_krc721(&tx)
    }

    #[derive(Debug)]
    struct TestTransaction {
        transaction: Transaction,
        signature_script: Vec<u8>,
    }

    // Test helper transaction
    impl TestTransaction {
        fn new(signature_script: Vec<u8>) -> Self {
            let tx = Transaction::new(
                0,
                vec![TransactionInput {
                    previous_outpoint: TransactionOutpoint::new(Default::default(), 0),
                    signature_script: signature_script.clone(),
                    sequence: 0,
                    sig_op_count: 1,
                }],
                vec![TransactionOutput {
                    value: 1000,
                    script_public_key: ScriptPublicKey::new(0, vec![0u8; 32].into()),
                }],
                0,
                SubnetworkId::default(),
                0,
                vec![],
            );
            Self {
                transaction: tx,
                signature_script,
            }
        }
    }

    // Test helper transaction implements required interface for analyzer
    impl ITransaction for &TestTransaction {
        fn signature_script(&self) -> Option<&[u8]> {
            Some(&self.signature_script)
        }

        fn receiver_addr(&self, prefix: Prefix) -> Option<Address> {
            self.transaction.outputs.first().and_then(|v| {
                extract_script_pub_key_address(
                    &v.script_public_key,
                    prefix, // analyzer should have state and prefix must be compared against net
                )
                .inspect_err(|err| error!("parsing address error: {err}"))
                .ok()
            })
        }

        fn first_beneficiary(&self) -> Option<ScriptPublicKey> {
            self.transaction
                .outputs
                .first()
                .map(|v| v.script_public_key.clone())
        }

        fn id(&self) -> TransactionId {
            self.transaction.id()
        }

        fn first_output_amt(&self) -> Option<u64> {
            self.transaction.outputs.first().map(|v| v.value)
        }
        fn fee(&self) -> u64 {
            100_000_000_000
        }

        fn block_time(&self) -> u64 {
            12
        }

        fn accepting_block_daa_score(&self) -> u64 {
            0
        }
    }

    // Helper inscription function
    fn create_test_inscription(op: UserOperation) -> Vec<u8> {
        let json = serde_json::to_string(&op).unwrap();
        let protocol = PROTOCOL_KSPR_NAMESPACE.as_bytes();

        redeem_script_hash_signature_script(
            protocol,
            json.as_bytes(),
            &[231u8; 32], // Mock pubkey
            &[243u8; 32], // Mock transaction signature
        )
        .unwrap()
    }

    #[test]
    fn test_detect_missing_mandatory_fields() {
        // Test Deploy without max
        let deploy_op = UserOperation::try_new(Protocol::Krc721, Op::Deploy, "KASPARTY")
            .unwrap()
            .with_metadata(Metadata::Remote("krc721://test/".to_string()));
        let script = create_test_inscription(deploy_op);
        let tx = TestTransaction::new(script);
        let result = detect_krc721(&tx);
        assert!(matches!(
            result,
            Err(AnalyzerError::OpDeployMissingValue(field)) if field == "max"
        ));

        // Test Transfer without to address
        let transfer_op = UserOperation::try_new(Protocol::Krc721, Op::Transfer, "KASPARTY")
            .unwrap()
            .with_token_id(1);
        let script = create_test_inscription(transfer_op);
        println!("{:?}", script);
        let tx = TestTransaction::new(script);
        let result = detect_krc721(&tx);
        assert!(matches!(
            result,
            Err(AnalyzerError::OpTransferMissingValue(field)) if field == "to"
        ));
    }

    #[test]
    fn test_detect_valid_inscriptions() {
        // Test valid Deploy
        let deploy_op = UserOperation::try_new(Protocol::Krc721, Op::Deploy, "KASPA")
            .unwrap()
            .with_metadata(Metadata::Remote("krc721://test/".to_string()))
            .with_daa_mint_start(525037124)
            .with_max(1000);

        let script = create_test_inscription(deploy_op);
        let tx = TestTransaction::new(script);
        let result = detect_krc721(&tx);
        assert!(result.is_ok());
        let operation = result.unwrap().expect("Should have valid operation");
        assert!(matches!(operation.info, OperationInfo::Deploy(_)));

        // Test valid Transfer
        let transfer_op = UserOperation::try_new(Protocol::Krc721, Op::Transfer, "KASPA")
            .unwrap()
            .with_token_id(1)
            .with_to(Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]));

        let script = create_test_inscription(transfer_op);
        let tx = TestTransaction::new(script);
        let result = detect_krc721(&tx);
        assert!(result.is_ok());
        let operation = result.unwrap().expect("Should have valid operation");
        assert!(matches!(operation.info, OperationInfo::Transfer(_)));

        // Test valid Discount
        let transfer_op = UserOperation::try_new(Protocol::Krc721, Op::Discount, "KASPA")
            .unwrap()
            .with_discount_fee(300000000)
            .with_to(Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]));

        let script = create_test_inscription(transfer_op);
        let tx = TestTransaction::new(script);
        let result = detect_krc721(&tx);
        assert!(result.is_ok());
        let operation = result.unwrap().expect("Should have valid operation");
        assert!(matches!(operation.info, OperationInfo::Discount(_)));
    }

    #[test]
    fn test_detect_invalid_transfer_address() {
        // Test invalid address
        let transfer_op = UserOperation::try_new(Protocol::Krc721, Op::Transfer, "KASPA")
            .unwrap()
            .with_token_id(1)
            .with_to("");

        let script = create_test_inscription(transfer_op);
        let tx = TestTransaction::new(script);
        let result = detect_krc721(&tx);
        println!("{:?}", result);

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(AnalyzerError::NFTInscriptionModelFieldValueParsingError{field_name: field,..}) if field == "to"
        ));
    }

    #[test]
    fn test_detect_invalid_royalty() {
        // Test invalid address
        let transfer_op = UserOperation::try_new(Protocol::Krc721, Op::Deploy, "KASPA")
            .unwrap()
            .with_royalty("", 30_000_000);

        let script = create_test_inscription(transfer_op);
        let tx = TestTransaction::new(script);
        let result = detect_krc721(&tx);
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(AnalyzerError::NFTInscriptionModelFieldValueParsingError{field_name:field, ..}) if field == "royalty_to"
        ));

        // Test royalty fee overflow
        let transfer_op = UserOperation::try_new(Protocol::Krc721, Op::Deploy, "KASPA")
            .unwrap()
            .with_max(100)
            .with_metadata(Metadata::Remote("krc721://test/".to_string()))
            .with_royalty(
                Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]),
                1_000_000_000_000_001,
            );

        let script = create_test_inscription(transfer_op);
        let tx = TestTransaction::new(script);
        let result = detect_krc721(&tx);
        assert!(result.is_err());
        assert!(matches!(result, Err(AnalyzerError::OverflowRoyaltyFee)));

        // Test royalty fee overflow
        let transfer_op = UserOperation::try_new(Protocol::Krc721, Op::Deploy, "KASPA")
            .unwrap()
            .with_max(100)
            .with_metadata(Metadata::Remote("krc721://test/".to_string()))
            .with_royalty(
                Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]),
                9_999_999,
            );

        let script = create_test_inscription(transfer_op);
        let tx = TestTransaction::new(script);
        let result = detect_krc721(&tx);
        assert!(result.is_err());
        assert!(matches!(result, Err(AnalyzerError::UnderflowRoyaltyFee)));
    }

    #[test]
    fn test_detect_deploy_valid_royalty_on_bound() {
        // Test royalty fee on upper bound inclusive
        let transfer_op = UserOperation::try_new(Protocol::Krc721, Op::Deploy, "KASPA")
            .unwrap()
            .with_max(100)
            .with_metadata(Metadata::Remote("krc721://test/".to_string()))
            .with_royalty(
                Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]),
                1_000_000_000_000_000,
            );

        let script = create_test_inscription(transfer_op);
        let tx = TestTransaction::new(script);
        let result = detect_krc721(&tx);
        assert!(result.is_ok());

        // Test royalty fee on lower bound inclusive
        let transfer_op = UserOperation::try_new(Protocol::Krc721, Op::Deploy, "KASPA")
            .unwrap()
            .with_max(100)
            .with_metadata(Metadata::Remote("krc721://test/".to_string()))
            .with_royalty(
                Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]),
                10_000_000,
            );

        let script = create_test_inscription(transfer_op);
        let tx = TestTransaction::new(script);
        let result = detect_krc721(&tx);
        assert!(result.is_ok());
    }

    fn build_invalid_no_false() -> ScriptBuilderResult<Vec<u8>> {
        Ok(ScriptBuilder::new()
            .add_data(&[1u8; 32])?
            .add_op(OpCheckSig)?
            .add_op(OpTrue)? // should be false
            .add_op(OpIf)?
            .add_data(PROTOCOL_KSPR_NAMESPACE.as_bytes())?
            .add_op(OpTrue)?
            .add_data(b"{}")?
            .add_op(OpEndIf)?
            .drain())
    }

    fn build_invalid_protocol() -> ScriptBuilderResult<Vec<u8>> {
        Ok(ScriptBuilder::new()
            .add_data(&[1u8; 32])?
            .add_op(OpCheckSig)?
            .add_op(OpFalse)?
            .add_op(OpIf)?
            .add_data("invalid".as_bytes())? // should be supported protocol
            .add_i64(0)?
            .add_data(b"{}")?
            .add_op(OpEndIf)?
            .drain())
    }
    fn build_unsupported_envelope() -> ScriptBuilderResult<Vec<u8>> {
        Ok(ScriptBuilder::new()
            .add_data(&[1u8; 32])?
            .add_op(OpCheckSig)?
            .add_op(OpFalse)?
            .add_op(OpIf)?
            .add_data("invalid".as_bytes())?
            .add_i64(1)? // optional marker without content nakes envelope length unsupported
            .add_i64(0)?
            .add_data(b"{}")?
            .add_op(OpEndIf)?
            .drain())
    }

    #[test]
    fn test_validate_redeem_envelope() {
        // Test missing OpIf
        let script = build_invalid_no_false().expect("Script build should succeed");
        let opcodes: Vec<_> = parse_script(&script).collect::<Result<_, _>>().unwrap();
        assert!(matches!(
            validate_redeem_envelope(&opcodes),
            Err(AnalyzerError::InvalidRedeemEnvelopeFormat(msg)) if msg == "OpFalse missing"
        ));
        let script = build_invalid_protocol().expect("Script build should succeed");
        let opcodes: Vec<_> = parse_script(&script).collect::<Result<_, _>>().unwrap();
        println!("{:?}", validate_redeem_envelope(&opcodes));
        assert!(matches!(
            validate_redeem_envelope(&opcodes),
            Err(AnalyzerError::InvalidRedeemEnvelopeFormat(msg)) if msg == "Invalid or missing protocol"
        ));
        let script = build_unsupported_envelope().expect("Script build should succeed");
        let opcodes: Vec<_> = parse_script(&script).collect::<Result<_, _>>().unwrap();
        println!("{:?}", validate_redeem_envelope(&opcodes));
        assert!(matches!(
            validate_redeem_envelope(&opcodes),
            Err(AnalyzerError::UnsupportedEnvelopeLength)
        ));
    }

    #[test]
    fn test_dwayne() {
        let input =  TransactionInput{
            previous_outpoint:TransactionOutpoint{ transaction_id: TransactionId::from(hex!("c2b4c554f38077e5ecdfc5f3b33d09e70fd7b53c35aa869cea56e7fbefa6114c")), index: 0 },
            signature_script: hex!("414b5913f9929e7e44ce94f3b0b173bffa063f21f74eee35a7715537a8791d528aed5ddffdae2d08897b86d405d86f220c844266928ef038197c54c853ae14ed88014cac2012b5a221b90917f255669f0eef86c99c9f5477811307c06ac378a37a8bc74246ac0063046b7370725100004c7d7b226d6178223a223130303030222c226d65746164617461223a22697066733a2f2f516d5a6348345976425656524a74646e3452646261716773704655386748365039766f6d4470425670414c337534222c226f70223a226465706c6f79222c2270223a226b72632d373231222c227469636b223a226d65696b72227d68").to_vec(),
            sequence: 0,
            sig_op_count: 1,
        };
        let tx = Transaction::new(0, vec![input], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        let tx = ContextTransaction {
            tx,
            fee: SOMPI_PER_KASPA * 100000,
            block_time: 1234,
            accepting_block_daa_score: 1235,
            index_within_merged_block: 0,
        };

        let op = detect_krc721(tx);
        println!("{op:?}")
    }
    #[test]
    fn test_kasplex_hex_should_not_detect() {
        // Kasplex header and ksprb string inside json
        let scriptsig = "41e70defbef78f1542971d27aa2870f79bf6f09f1d379569edf01d7fcf3be124bb0220b164b9829c492a05155317daa5f6f94c40e6e89b48d6095bc52efc1ca1aa014c5a203a3dbade3327d6f8a6908c7d3134ec40a355483a33f80d66fce226bf2c5ee775ac0063076b6173706c6578510000297b226f70223a226d696e74222c2270223a226b72632d3230222c227469636b223a226b73707262227d68";
        let bytes = hex::decode(scriptsig).unwrap();
        assert!(!detect_kspr_header(&bytes));
    }

    #[test]
    fn test_valid_kspr_hex() {
        // Kspr header
        let scriptsig: &str = "410cac6568daa42e9d34e93734ad6d9668eb1e9aafabaccfcca717a42078340347b967accf6acab97e9eb306a298e7197c763653d551440eadc0d69ace314c00b5014c54205418c4db51891a02602e2f692e88b00a30a3a3709ced16b3e43cf8bcce8c4abbac0063046b73707200287b2270223a226b72632d373231222c226f70223a226d696e74222c227469636b223a2246414d227d68";
        let bytes = hex::decode(scriptsig).unwrap();
        assert!(detect_kspr_header(&bytes));
    }

    #[test]
    fn test_valid_kspr_hex_checksig_ecdsa() {
        // Kspr header with ecdsa checksig opcode
        let scriptsig: &str = "410cac6568daa42e9d34e93734ad6d9668eb1e9aafabaccfcca717a42078340347b967accf6acab97e9eb306a298e7197c763653d551440eadc0d69ace314c00b5014c54205418c4db51891a02602e2f692e88b00a30a3a3709ced16b3e43cf8bcce8c4abbab0063046b73707200287b2270223a226b72632d373231222c226f70223a226d696e74222c227469636b223a2246414d227d68";
        let bytes = hex::decode(scriptsig).unwrap();
        assert!(detect_kspr_header(&bytes));
    }
}
