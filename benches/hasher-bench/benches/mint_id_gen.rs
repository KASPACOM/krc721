use ahash::AHasher;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rand::Rng;
use rapidhash::RapidInlineHasher;
use siphasher::sip::SipHasher;
use std::hash::Hasher;
use wyhash::WyHash;

struct MergeSetData {
    block_hashes: Vec<[u8; 32]>,
    tx_score: u64,
    max_supply: u64,
}

fn create_test_mergeset(block_count: usize) -> MergeSetData {
    let mut rng = rand::thread_rng();
    let mut data = MergeSetData {
        block_hashes: Vec::with_capacity(block_count),
        tx_score: rng.gen(),
        max_supply: rng.gen_range(1000..10000),
    };

    for _ in 0..block_count {
        let mut block_hash = [0u8; 32];
        rng.fill(&mut block_hash);
        data.block_hashes.push(block_hash);
    }

    data
}

// Simple XOR approach
fn xor_token_id(data: &MergeSetData) -> u64 {
    let mut value = 0u64;
    for block_hash in &data.block_hashes {
        for chunk in block_hash.chunks(8) {
            if chunk.len() == 8 {
                let chunk_value = u64::from_le_bytes(chunk.try_into().unwrap());
                value ^= chunk_value;
            }
        }
    }
    value = value.wrapping_add(data.tx_score);
    value % data.max_supply
}

// Hash-based approach
fn hash_token_id<H: Hasher + Default>(data: &MergeSetData) -> u64 {
    let mut hasher = H::default();

    // Hash all block hashes
    for block_hash in &data.block_hashes {
        hasher.write(block_hash);
    }

    // Add tx_score
    hasher.write(&data.tx_score.to_le_bytes());

    hasher.finish() % data.max_supply
}

fn benchmark_token_id_generators(c: &mut Criterion) {
    let block_counts = [10, 20, 50]; // Typical mergeset sizes
    let mut group = c.benchmark_group("token_id_generation");

    for &block_count in &block_counts {
        let data = create_test_mergeset(block_count);

        // Simple XOR
        group.bench_with_input(BenchmarkId::new("xor", block_count), &data, |b, data| {
            b.iter(|| xor_token_id(data))
        });

        // City Hash
        group.bench_with_input(
            BenchmarkId::new("cityhash", block_count),
            &data,
            |b, data| b.iter(|| hash_token_id::<fasthash::city::Hasher64>(data)),
        );

        // Farm Hash
        group.bench_with_input(
            BenchmarkId::new("farmhash", block_count),
            &data,
            |b, data| b.iter(|| hash_token_id::<fasthash::farm::Hasher64>(data)),
        );

        // Lookup3
        group.bench_with_input(
            BenchmarkId::new("lookup3", block_count),
            &data,
            |b, data| b.iter(|| hash_token_id::<fasthash::lookup3::Hasher32>(data)),
        );

        // Metro Hash
        group.bench_with_input(
            BenchmarkId::new("metrohash", block_count),
            &data,
            |b, data| b.iter(|| hash_token_id::<fasthash::metro::Hasher64_1>(data)),
        );

        // Mum Hash
        group.bench_with_input(
            BenchmarkId::new("mumhash", block_count),
            &data,
            |b, data| b.iter(|| hash_token_id::<fasthash::mum::Hasher64>(data)),
        );

        // Murmur Hash
        group.bench_with_input(
            BenchmarkId::new("murmur3", block_count),
            &data,
            |b, data| b.iter(|| hash_token_id::<fasthash::murmur3::Hasher32>(data)),
        );

        // Sea Hash
        group.bench_with_input(
            BenchmarkId::new("seahash", block_count),
            &data,
            |b, data| b.iter(|| hash_token_id::<fasthash::sea::Hasher64>(data)),
        );

        // Spooky Hash
        group.bench_with_input(
            BenchmarkId::new("spookyhash", block_count),
            &data,
            |b, data| b.iter(|| hash_token_id::<fasthash::spooky::Hasher64>(data)),
        );

        // XX Hash
        group.bench_with_input(BenchmarkId::new("xxhash", block_count), &data, |b, data| {
            b.iter(|| hash_token_id::<fasthash::xx::Hasher64>(data))
        });
        // SipHash
        group.bench_with_input(
            BenchmarkId::new("siphash", block_count),
            &data,
            |b, data| b.iter(|| hash_token_id::<SipHasher>(data)),
        );

        // T1ha Hash
        group.bench_with_input(BenchmarkId::new("t1ha", block_count), &data, |b, data| {
            b.iter(|| hash_token_id::<fasthash::t1ha0::Hasher64>(data))
        });

        group.bench_with_input(BenchmarkId::new("wyhash", block_count), &data, |b, data| {
            b.iter(|| hash_token_id::<WyHash>(data))
        });

        group.bench_with_input(BenchmarkId::new("ahash", block_count), &data, |b, data| {
            b.iter(|| hash_token_id::<AHasher>(data))
        });

        group.bench_with_input(
            BenchmarkId::new("rapidhash", block_count),
            &data,
            |b, data| b.iter(|| hash_token_id::<RapidInlineHasher>(data)),
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_token_id_generators);
criterion_main!(benches);
