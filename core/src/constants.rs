use const_str::convert_ascii_case;
use kaspa_consensus_core::constants::SOMPI_PER_KASPA;

pub const KSPR_FEE_DEPLOY: u64 = 1_000 * SOMPI_PER_KASPA;
pub const KSPR_FEE_MINT: u64 = 10 * SOMPI_PER_KASPA;
pub const MIN_ROYALTY_FEE: u64 = SOMPI_PER_KASPA / 10;
pub const MAX_ROYALTY_FEE: u64 = 10_000_000 * SOMPI_PER_KASPA;
pub const MIN_LISTING_PRICE: u64 = SOMPI_PER_KASPA / 10; // 0.1 KAS minimum listing price

pub const PROTOCOL_KASPLEX_NAMESPACE: &str = "kasplex";
pub const PROTOCOL_KSPR_NAMESPACE: &str = "kspr";

const KASPLEX_HEADER: &str = "kasplex";
pub const KASPLEX_HEADER_LC: &[u8] = convert_ascii_case!(lower, KASPLEX_HEADER).as_bytes();
pub const KASPLEX_HEADER_UC: &[u8] = convert_ascii_case!(upper, KASPLEX_HEADER).as_bytes();

const KSPR_HEADER: &str = "kspr";
pub const KSPR_HEADER_LC: &[u8] = convert_ascii_case!(lower, KSPR_HEADER).as_bytes();
pub const KSPR_HEADER_UC: &[u8] = convert_ascii_case!(upper, KSPR_HEADER).as_bytes();

// Strict header for detection
const OP_FALSE: u8 = 0x00;
const OP_IF: u8 = 0x63;
const OP_PUSH04: u8 = 0x04;
const KSPR_ASCII: &[u8] = b"kspr";
const KSPR_ASCII_UC: &[u8] = b"KSPR";

pub const KSPR_U8_STRICT: &[u8] = &[
    OP_FALSE,
    OP_IF,
    OP_PUSH04,
    KSPR_ASCII[0],
    KSPR_ASCII[1],
    KSPR_ASCII[2],
    KSPR_ASCII[3],
];

pub const KSPR_U8_STRICT_UC: &[u8] = &[
    OP_FALSE,
    OP_IF,
    OP_PUSH04,
    KSPR_ASCII_UC[0],
    KSPR_ASCII_UC[1],
    KSPR_ASCII_UC[2],
    KSPR_ASCII_UC[3],
];

const KRC20_HEADER: &str = "krc-20";
pub const KRC20_HEADER_UC: &[u8] = convert_ascii_case!(lower, KRC20_HEADER).as_bytes();
pub const KRC20_HEADER_LC: &[u8] = convert_ascii_case!(upper, KRC20_HEADER).as_bytes();

const KRC721_HEADER: &str = "krc-721";
pub const KRC721_HEADER_UC: &[u8] = convert_ascii_case!(lower, KRC721_HEADER).as_bytes();
pub const KRC721_HEADER_LC: &[u8] = convert_ascii_case!(upper, KRC721_HEADER).as_bytes();
