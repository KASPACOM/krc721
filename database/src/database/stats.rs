use borsh::{BorshDeserialize, BorshSerialize};
use std::ops::{Add, AddAssign, Sub, SubAssign};

#[derive(Debug, Default, Copy, Clone, BorshSerialize, BorshDeserialize)]
pub struct Stats {
    pub deployments: u64,
    pub mints: u64,
    pub transfers: u64,
    pub royalty_fees: u64,
    pub security_fees: u64,
    pub listings: u64,
    pub sends: u64,
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
