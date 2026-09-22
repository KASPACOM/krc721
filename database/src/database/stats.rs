use borsh::{BorshDeserialize, BorshSerialize};
use std::io::Read;
use std::ops::{Add, AddAssign, Sub, SubAssign};

/// Persisted stats record.
///
/// # Schema history
/// - v1 (pre-marketplace): 5 fields × u64 = 40 bytes
///   deployments, mints, transfers, royalty_fees, security_fees
/// - v2 (marketplace): 7 fields × u64 = 56 bytes
///   + listings, sends
///
/// The custom BorshDeserialize below handles both layouts transparently.
#[derive(Debug, Default, Copy, Clone, BorshSerialize)]
pub struct Stats {
    pub deployments: u64,
    pub mints: u64,
    pub transfers: u64,
    pub royalty_fees: u64,
    pub security_fees: u64,
    pub listings: u64,
    pub sends: u64,
}

/// Returns true for the error that Borsh 1.x produces when reading a fixed-size
/// type (e.g. u64) from a reader with insufficient bytes remaining.
/// Borsh converts the underlying `UnexpectedEof` into
/// `InvalidData("Unexpected length of input")` internally.
fn is_borsh_short_read(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::InvalidData
        && e.to_string().contains("Unexpected length of input")
}

impl BorshDeserialize for Stats {
    fn deserialize_reader<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        let deployments = u64::deserialize_reader(reader)?;
        let mints = u64::deserialize_reader(reader)?;
        let transfers = u64::deserialize_reader(reader)?;
        let royalty_fees = u64::deserialize_reader(reader)?;
        let security_fees = u64::deserialize_reader(reader)?;

        // Try to read the two marketplace fields introduced in v2.
        // If the record is a v1 snapshot (only 5 fields / 40 bytes), borsh
        // converts the short-read into InvalidData("Unexpected length of input").
        // We detect that and default both fields to zero.
        let listings = match u64::deserialize_reader(reader) {
            Ok(v) => v,
            Err(ref e) if is_borsh_short_read(e) => 0,
            Err(e) => return Err(e),
        };
        let sends = match u64::deserialize_reader(reader) {
            Ok(v) => v,
            Err(ref e) if is_borsh_short_read(e) => 0,
            Err(e) => return Err(e),
        };

        Ok(Stats {
            deployments,
            mints,
            transfers,
            royalty_fees,
            security_fees,
            listings,
            sends,
        })
    }
}

pub type StatsDiffs = Stats;

impl AddAssign for Stats {
    fn add_assign(&mut self, rhs: StatsDiffs) {
        *self = *self + rhs
    }
}

impl Add for Stats {
    type Output = Self;

    fn add(
        self,
        Stats {
            deployments: deployments_rhs,
            mints: mints_rhs,
            transfers: transfers_rhs,
            royalty_fees: royalty_fees_rhs,
            security_fees: security_fees_rhs,
            listings: listings_rhs,
            sends: sends_rhs,
        }: StatsDiffs,
    ) -> Self::Output {
        let Stats {
            deployments,
            mints,
            transfers,
            royalty_fees,
            security_fees,
            listings,
            sends,
        } = self;
        Stats {
            deployments: deployments.saturating_add(deployments_rhs),
            mints: mints.saturating_add(mints_rhs),
            transfers: transfers.saturating_add(transfers_rhs),
            royalty_fees: royalty_fees.saturating_add(royalty_fees_rhs),
            security_fees: security_fees.saturating_add(security_fees_rhs),
            listings: listings.saturating_add(listings_rhs),
            sends: sends.saturating_add(sends_rhs),
        }
    }
}

impl SubAssign for Stats {
    fn sub_assign(&mut self, rhs: StatsDiffs) {
        *self = *self - rhs
    }
}

impl Sub for Stats {
    type Output = Self;

    fn sub(
        self,
        Stats {
            deployments: deployments_rhs,
            mints: mints_rhs,
            transfers: transfers_rhs,
            royalty_fees: royalty_fees_rhs,
            security_fees: security_fees_rhs,
            listings: listings_rhs,
            sends: sends_rhs,
        }: StatsDiffs,
    ) -> Self::Output {
        let Stats {
            deployments,
            mints,
            transfers,
            royalty_fees,
            security_fees,
            listings,
            sends,
        } = self;
        Stats {
            deployments: deployments.saturating_sub(deployments_rhs),
            mints: mints.saturating_sub(mints_rhs),
            transfers: transfers.saturating_sub(transfers_rhs),
            royalty_fees: royalty_fees.saturating_sub(royalty_fees_rhs),
            security_fees: security_fees.saturating_sub(security_fees_rhs),
            listings: listings.saturating_sub(listings_rhs),
            sends: sends.saturating_sub(sends_rhs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes only the 5 v1 fields to simulate an old snapshot record.
    fn v1_bytes(s: &Stats) -> Vec<u8> {
        let mut buf = Vec::new();
        borsh::to_writer(&mut buf, &s.deployments).unwrap();
        borsh::to_writer(&mut buf, &s.mints).unwrap();
        borsh::to_writer(&mut buf, &s.transfers).unwrap();
        borsh::to_writer(&mut buf, &s.royalty_fees).unwrap();
        borsh::to_writer(&mut buf, &s.security_fees).unwrap();
        buf
    }

    #[test]
    fn test_stats_v1_compat() {
        let original = Stats {
            deployments: 10,
            mints: 20,
            transfers: 30,
            royalty_fees: 40,
            security_fees: 50,
            listings: 0,
            sends: 0,
        };
        let bytes = v1_bytes(&original);
        assert_eq!(bytes.len(), 40); // 5 x 8 bytes
        let decoded: Stats = borsh::from_slice(&bytes).expect("v1 decode failed");
        assert_eq!(decoded.deployments, 10);
        assert_eq!(decoded.mints, 20);
        assert_eq!(decoded.transfers, 30);
        assert_eq!(decoded.royalty_fees, 40);
        assert_eq!(decoded.security_fees, 50);
        assert_eq!(decoded.listings, 0); // defaulted
        assert_eq!(decoded.sends, 0); // defaulted
    }

    #[test]
    fn test_stats_v2_roundtrip() {
        let original = Stats {
            deployments: 1,
            mints: 2,
            transfers: 3,
            royalty_fees: 4,
            security_fees: 5,
            listings: 6,
            sends: 7,
        };
        let bytes = borsh::to_vec(&original).unwrap();
        assert_eq!(bytes.len(), 56); // 7 x 8 bytes
        let decoded: Stats = borsh::from_slice(&bytes).expect("v2 decode failed");
        assert_eq!(decoded.listings, 6);
        assert_eq!(decoded.sends, 7);
    }
}
