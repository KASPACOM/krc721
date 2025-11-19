use std::num::{NonZero, NonZeroU64};

use super::result::Result;
use super::Processor;
use krc721_core::model::krc721::Tick;
use krc721_database::{
    database::{PreModRange, RangeKey, TokenMetaKey},
    prelude::WriteTransaction,
};
use tracing::{debug, info, instrument, warn};

impl Processor {
    #[allow(clippy::too_many_arguments)]
    fn remove_range(
        &self,
        tx: &mut WriteTransaction,
        tick: &Tick,
        range_index: u64,
        value_id: u64,
        start: u64,
        size: u64,
        ranges_len: &mut u64,
        is_initial: bool,
    ) -> Result<()> {
        // Store metadata for removal
        self.db.token_id_meta.insert_wtx(
            tx,
            TokenMetaKey {
                tick: *tick,
                token_id: value_id,
            },
            &PreModRange {
                range_index,
                start,
                size,
                removed: true,
                split: false,
                is_initial,
            },
        )?;

        // There exists more than one range
        if range_index < *ranges_len - 1 {
            // Retrieve the last index
            let last_idx = *ranges_len - 1;
            // Retrieve the last value
            let last_value = self
                .db
                .available_ranges
                .get_wtx(
                    tx,
                    &RangeKey {
                        tick: *tick,
                        index: last_idx,
                    },
                )?
                .expect("Last value must exist if range_lengths is correct");

            // Move the last range into the range that we want to remove
            self.db.available_ranges.insert_wtx(
                tx,
                RangeKey {
                    tick: *tick,
                    index: range_index,
                },
                &last_value,
            )?;

            // Simply remove the last range.
            // This will make sure that the range we wanted to remove is removed,
            // and the last range is now at the position of the removed range.
            self.db.available_ranges.remove_wtx(
                tx,
                &RangeKey {
                    tick: *tick,
                    index: last_idx,
                },
            )?;
        } else {
            // Just remove last range, there's only one
            self.db.available_ranges.remove_wtx(
                tx,
                &RangeKey {
                    tick: *tick,
                    index: range_index,
                },
            )?;
        }

        // Update length
        *ranges_len -= 1;
        self.db.range_lengths.insert_wtx(tx, *tick, ranges_len)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn shrink_range(
        &self,
        tx: &mut WriteTransaction,
        tick: &Tick,
        range_index: u64,
        start: u64,
        size: u64,
        value_id: u64,
        from_start: bool,
        is_initial: bool,
    ) -> Result<()> {
        // Store metadata for start modification
        self.db.token_id_meta.insert_wtx(
            tx,
            TokenMetaKey {
                tick: *tick,
                token_id: value_id,
            },
            &PreModRange {
                range_index,
                start,
                size,
                removed: false,
                split: false,
                is_initial,
            },
        )?;

        // If we shrink from the start we must increment the start
        // position, whilst if we shrink from the end we should only
        // decrement the size.
        let value = {
            if from_start {
                (start + 1, size - 1)
            } else {
                (start, size - 1)
            }
        };

        // Update range
        self.db.available_ranges.insert_wtx(
            tx,
            RangeKey {
                tick: *tick,
                index: range_index,
            },
            &value,
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn split_range(
        &self,
        tx: &mut WriteTransaction,
        tick: &Tick,
        range_index: u64,
        value_id: u64,
        start: u64,
        size: u64,
        value_offset: u64,
        ranges_len: &mut u64,
        is_initial: bool,
    ) -> Result<()> {
        // Store metadata for split
        self.db.token_id_meta.insert_wtx(
            tx,
            TokenMetaKey {
                tick: *tick,
                token_id: value_id,
            },
            &PreModRange {
                range_index,
                start,
                size,
                removed: false,
                split: true, // Mark as split operation
                is_initial,
            },
        )?;

        // Value in middle - split range

        // The value_offset is the size from the start to the
        // position of the value. This will be the new size for
        // the left portion.
        let left_size = value_offset;

        // Right start will be one after the value to ensure
        // that the value will not be generated again.
        let right_start = value_id + 1;

        // The right size is the total size of the original
        // range minus the left size and the value itself.
        let right_size = size - value_offset - 1;

        // Update the original range to be the
        // left portion
        self.db.available_ranges.insert_wtx(
            tx,
            RangeKey {
                tick: *tick,
                index: range_index,
            },
            &(start, left_size),
        )?;

        // Add right portion at end
        self.db.available_ranges.insert_wtx(
            tx,
            RangeKey {
                tick: *tick,
                index: *ranges_len,
            },
            &(right_start, right_size),
        )?;

        // Increase length
        *ranges_len += 1;
        self.db.range_lengths.insert_wtx(tx, *tick, ranges_len)?;
        Ok(())
    }

    pub fn generate_token_id(
        &self,
        tx: &mut WriteTransaction,
        tick: &Tick,
        mergeset_entropy: u64,
        max_supply: NonZero<u64>,
        premint: u64,
    ) -> Result<NonZero<u64>> {
        // Get current number of ranges for this tick
        let mut ranges_len: u64 = self.db.range_lengths.get_wtx(tx, tick)?.unwrap_or(0);

        // Get range
        // If no ranges exist we initialize the length.
        // We can be sure that in all branches at least one range
        // will be inserted, therefore we dont need to add the first
        // range here.
        let (range_index, range, is_initial) = {
            if ranges_len == 0 {
                ranges_len = 1;
                self.db.range_lengths.insert_wtx(tx, *tick, &ranges_len)?;
                (0, Some((1 + premint, max_supply.get() - premint)), true)
            } else {
                // Select random range
                let range_index = mergeset_entropy % ranges_len;

                let range = self.db.available_ranges.get_wtx(
                    tx,
                    &RangeKey {
                        tick: *tick,
                        index: range_index,
                    },
                )?;
                (range_index, range, false)
            }
        };

        // Ensure range exists
        let (start, size) = range.expect("Range must exist");

        // Generate value using different entropy bits
        let value_id = start + (mergeset_entropy % size);

        // Ensure value is within max supply
        assert!(
            value_id <= max_supply.get(),
            "Generated ID is greater than max supply"
        );

        let value_offset = value_id - start;

        // For each case, store metadata BEFORE modifying the range
        // this is to ensure that we can rollback the state
        if value_offset == 0 {
            // Value at start of range
            if size == 1 {
                // Since there was only one value left in the range, remove it
                self.remove_range(
                    tx,
                    tick,
                    range_index,
                    value_id,
                    start,
                    size,
                    &mut ranges_len,
                    is_initial,
                )?;
            } else {
                // There exists more values, so shrink the range
                self.shrink_range(
                    tx,
                    tick,
                    range_index,
                    start,
                    size,
                    value_id,
                    true,
                    is_initial,
                )?;
            }
        } else if value_offset == size - 1 {
            // Value at end of range, shrink the range
            self.shrink_range(
                tx,
                tick,
                range_index,
                start,
                size,
                value_id,
                false,
                is_initial,
            )?;
        } else {
            // Value in middle, split the range
            self.split_range(
                tx,
                tick,
                range_index,
                value_id,
                start,
                size,
                value_offset,
                &mut ranges_len,
                is_initial,
            )?;
        }
        Ok(NonZeroU64::new(value_id)
            .expect("Generated ID cannot be zero since ranges start from 1"))
    }

    fn revert_removed_range(
        &self,
        tx: &mut WriteTransaction,
        tick: &Tick,
        pre_mod_range: &PreModRange,
        ranges_len: &mut u64,
    ) -> Result<()> {
        if pre_mod_range.range_index < *ranges_len {
            // Get what's currently at the target position - must exist if not last
            let current_value = self
                .db
                .available_ranges
                .get_wtx(
                    tx,
                    &RangeKey {
                        tick: *tick,
                        index: pre_mod_range.range_index,
                    },
                )?
                .expect("Value must exist at target position if range tracking is correct");

            // Move it to the end
            self.db.available_ranges.insert_wtx(
                tx,
                RangeKey {
                    tick: *tick,
                    index: *ranges_len,
                },
                &current_value,
            )?;
        }

        // Now restore original range at its position.
        // if there was anything at this position, it was moved to the end
        self.db.available_ranges.insert_wtx(
            tx,
            RangeKey {
                tick: *tick,
                index: pre_mod_range.range_index,
            },
            &(pre_mod_range.start, pre_mod_range.size),
        )?;

        *ranges_len += 1;
        // Update length
        self.db.range_lengths.insert_wtx(tx, *tick, ranges_len)?;
        Ok(())
    }

    fn restore_range_maybe_split(
        &self,
        tx: &mut WriteTransaction,
        tick: &Tick,
        pre_mod_range: &PreModRange,
        ranges_len: &mut u64,
    ) -> Result<()> {
        // First restore the original range
        self.db.available_ranges.insert_wtx(
            tx,
            RangeKey {
                tick: *tick,
                index: pre_mod_range.range_index,
            },
            &(pre_mod_range.start, pre_mod_range.size),
        )?;

        // If it was a split, remove the extra range
        if pre_mod_range.split {
            *ranges_len -= 1;
            self.db.available_ranges.remove_wtx(
                tx,
                &RangeKey {
                    tick: *tick,
                    index: *ranges_len,
                },
            )?;

            self.db.range_lengths.insert_wtx(tx, *tick, ranges_len)?;
        }
        Ok(())
    }

    #[instrument(skip_all, fields(tick = %tick, token_id = token_id))]
    pub fn rollback_token_generation(
        &self,
        tx: &mut WriteTransaction,
        tick: &Tick,
        token_id: u64,
    ) -> Result<()> {
        // Retrieve original pre modification range
        let pre_mod_range = self
            .db
            .token_id_meta
            .get_wtx(
                tx,
                &TokenMetaKey {
                    tick: *tick,
                    token_id,
                },
            )?
            .expect("Token metadata must exist");

        // Retrieve current range length
        let mut ranges_len = self.db.range_lengths.get_wtx(tx, tick)?.unwrap_or(0);
        info!("ranges_len from db: {}", ranges_len);
        if pre_mod_range.removed {
            // The range was removed. We must restore it.
            self.revert_removed_range(tx, tick, &pre_mod_range, &mut ranges_len)?;
        } else {
            // The range was modified and maybe split
            self.restore_range_maybe_split(tx, tick, &pre_mod_range, &mut ranges_len)?;
        }
        info!("ranges_len after remove: {}", ranges_len);

        // Remove the metadata
        self.db.token_id_meta.remove_wtx(
            tx,
            &TokenMetaKey {
                tick: *tick,
                token_id,
            },
        )?;

        // Check if range is equal to initial state
        if pre_mod_range.is_initial {
            // Remove the range
            self.db.available_ranges.remove_wtx(
                tx,
                &RangeKey {
                    tick: *tick,
                    index: pre_mod_range.range_index,
                },
            )?;

            ranges_len -= 1;
            info!("ranges_len after is_initial remove: {}", ranges_len);

            self.db
                .available_ranges
                .range_wtx(
                    tx,
                    RangeKey {
                        tick: *tick,
                        index: 0,
                    }..=RangeKey {
                        tick: *tick,
                        index: u64::MAX,
                    },
                )
                .for_each(|r| {
                    let Ok((r, (start, end))) = r else {
                        warn!("Failed to iterate over ranges");
                        return;
                    };
                    warn!("Range: {} - {} for key: {:?}", start, end, r);
                });
            // Ensure there is no more ranges
            assert_eq!(ranges_len, 0);

            // Remove length detail
            self.db.range_lengths.remove_wtx(tx, tick)?;
            debug!("Reverted all ranges");
        }

        Ok(())
    }
}
