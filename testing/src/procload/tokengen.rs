use ahash::AHashMap;
use krc721_core::model::krc721::Tick;

#[derive(Debug, Clone)]
pub struct TokenGen {
    ranges: AHashMap<Tick, Vec<(u64, u64)>>,
}

impl Default for TokenGen {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenGen {
    pub fn new() -> TokenGen {
        TokenGen {
            ranges: AHashMap::new(),
        }
    }

    pub fn generate(
        &mut self,
        tick: &Tick,
        mergeset_entropy: u64,
        max_supply: u64,
        premint: u64,
    ) -> u64 {
        let ranges = self.ranges.get_mut(tick);
        let (range_index, range, ranges) = {
            if let Some(ranges) = ranges {
                let range_index = mergeset_entropy % ranges.len() as u64;
                (
                    range_index,
                    *ranges.get(range_index as usize).expect("Range must exist"),
                    ranges,
                )
            } else {
                let range = (1 + premint, max_supply - premint);
                let ranges = vec![range];
                self.ranges.insert(*tick, ranges);
                let mut_range_ref = self.ranges.get_mut(tick).expect("Range must exist");
                (0_u64, range, mut_range_ref)
            }
        };

        let (start, size) = range;
        let value_id = start + (mergeset_entropy % size);
        if value_id > max_supply {
            panic!("Generated ID is greater than max supply");
        }

        let value_offset = value_id - start;
        // For each case, store metadata BEFORE modifying the range
        // this is to ensure that we can rollback the state
        if value_offset == 0 {
            // Value at start of range
            if size == 1 {
                // Since there was only one value left in the range, remove it
                // There exists more than one range
                if range_index < ranges.len() as u64 - 1 {
                    // Retrieve the last index
                    let last_idx = ranges.len() as u64 - 1;
                    // Retrieve the last value
                    let last_value = ranges
                        .get(last_idx as usize)
                        .expect("Last value must exist if range_lengths is correct");

                    // Update the range
                    ranges[range_index as usize] = *last_value;

                    // Simply remove the last range.
                    // This will make sure that the range we wanted to remove is removed,
                    // and the last range is now at the position of the removed range.
                    ranges.pop();
                } else {
                    // Just remove last range, there's only one
                    ranges.pop();
                }
            } else {
                // There exists more values, so shrink the range
                let value = (start + 1, size - 1);
                ranges[range_index as usize] = value;
            }
        } else if value_offset == size - 1 {
            // Value at end of range, shrink the range
            let value = (start, size - 1);
            ranges[range_index as usize] = value;
        } else {
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
            ranges[range_index as usize] = (start, left_size);

            // Insert the right portion after the original range
            ranges.push((right_start, right_size));
        }
        value_id
    }
}
