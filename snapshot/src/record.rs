use crate::imports::*;
use fjall::Slice;

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Record {
    pub k: Vec<u8>,
    pub v: Vec<u8>,
}

impl Record {
    pub fn new((k, v): (Slice, Slice)) -> Self {
        Self {
            k: k.to_vec(),
            v: v.to_vec(),
        }
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.k.len() + self.v.len()
    }
}
