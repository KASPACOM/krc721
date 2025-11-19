use crate::imports::*;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Inflated {
    pub partition_id: u16,
    pub records: Vec<Record>,
}

impl Inflated {
    pub fn fill(partition: &Partition, chunk_size: usize) -> Result<Option<Inflated>> {
        let mut size = 0;
        let mut records = Vec::new();
        let mut iter = partition.iter();

        for kv in iter.by_ref() {
            let record = Record::new(kv?);
            size += record.len();
            records.push(record);
            if size > chunk_size {
                return Ok(Some(Inflated {
                    partition_id: partition.id(),
                    records,
                }));
            }
        }

        if !records.is_empty() {
            Ok(Some(Inflated {
                partition_id: partition.id(),
                records,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn deflate(self) -> Result<Deflated> {
        let Inflated {
            partition_id,
            records,
        } = self;
        let data = borsh::to_vec(&records)?;
        let mut compressed = Vec::new();
        let mut encoder = DeflateEncoder::new(&mut compressed, Compression::fast());
        encoder.write_all(&data)?;
        encoder.finish()?;
        Ok(Deflated {
            partition_id,
            data: compressed,
        })
    }
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Deflated {
    pub partition_id: u16,
    pub data: Vec<u8>,
}

impl Deflated {
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn read<R: Read>(reader: &mut R) -> Result<Self> {
        let chunk = BorshDeserialize::deserialize_reader(reader)?;
        Ok(chunk)
    }

    pub fn write<W: Write>(&self, writer: &mut W) -> Result<()> {
        self.serialize(writer)?;
        Ok(())
    }

    pub fn inflate(self) -> Result<Inflated> {
        let Deflated {
            partition_id,
            data: compressed,
        } = self;
        let mut decompressed = Vec::new();
        let mut decoder = DeflateDecoder::new(&compressed[..]);
        decoder.read_to_end(&mut decompressed)?;
        Ok(Inflated {
            partition_id,
            records: borsh::from_slice(&decompressed)?,
        })
    }
}
