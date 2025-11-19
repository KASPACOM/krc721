cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        use ahash::AHasher;
        use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
        use rand::Rng;
        use siphasher::sip::SipHasher;
        use std::hash::Hasher;
        use wyhash::WyHash;

        struct Block {
            transactions: Vec<[u8; 32]>,
            tx_index: u32,
        }

        fn create_test_block(tx_count: usize) -> Block {
            let mut rng = rand::thread_rng();
            let mut block = Block {
                transactions: Vec::with_capacity(tx_count),
                tx_index: rng.gen(),
            };

            for _ in 0..tx_count {
                let mut tx_hash = [0u8; 32];
                rng.fill(&mut tx_hash);
                block.transactions.push(tx_hash);
            }

            block
        }

        fn hash_block<H: Hasher + Default>(block: &Block) -> u64 {
            let mut hasher = H::default();
            for tx_hash in &block.transactions {
                hasher.write(tx_hash);
            }
            hasher.write(&block.tx_index.to_le_bytes());
            hasher.finish()
        }

        fn benchmark_hashers(c: &mut Criterion) {
            let tx_counts = [100, 200, 500];
            let mut group = c.benchmark_group("block_hashing");

            for &tx_count in &tx_counts {
                let block = create_test_block(tx_count);

                // City Hash
                group.bench_with_input(
                    BenchmarkId::new("cityhash", tx_count),
                    &block,
                    |b, block| b.iter(|| hash_block::<fasthash::city::Hasher64>(block))
                );

                // Farm Hash
                group.bench_with_input(
                    BenchmarkId::new("farmhash", tx_count),
                    &block,
                    |b, block| b.iter(|| hash_block::<fasthash::farm::Hasher64>(block))
                );

                // Lookup3
                group.bench_with_input(
                    BenchmarkId::new("lookup3", tx_count),
                    &block,
                    |b, block| b.iter(|| hash_block::<fasthash::lookup3::Hasher32>(block))
                );

                // Metro Hash
                group.bench_with_input(
                    BenchmarkId::new("metrohash", tx_count),
                    &block,
                    |b, block| b.iter(|| hash_block::<fasthash::metro::Hasher64_1>(block))
                );

                // Mum Hash
                group.bench_with_input(
                    BenchmarkId::new("mumhash", tx_count),
                    &block,
                    |b, block| b.iter(|| hash_block::<fasthash::mum::Hasher64>(block))
                );

                // Murmur Hash
                group.bench_with_input(
                    BenchmarkId::new("murmur3", tx_count),
                    &block,
                    |b, block| b.iter(|| hash_block::<fasthash::murmur3::Hasher32>(block))
                );

                // Sea Hash
                group.bench_with_input(
                    BenchmarkId::new("seahash", tx_count),
                    &block,
                    |b, block| b.iter(|| hash_block::<fasthash::sea::Hasher64>(block))
                );

                // Spooky Hash
                group.bench_with_input(
                    BenchmarkId::new("spookyhash", tx_count),
                    &block,
                    |b, block| b.iter(|| hash_block::<fasthash::spooky::Hasher64>(block))
                );

                // XX Hash
                group.bench_with_input(BenchmarkId::new("xxhash", tx_count), &block, |b, block| {
                    b.iter(|| hash_block::<fasthash::xx::Hasher64>(block))
                });

                group.bench_with_input(
                    BenchmarkId::new("siphash", tx_count),
                    &block,
                    |b, block| b.iter(|| hash_block::<SipHasher>(block))
                );

                // T1ha Hash
                group.bench_with_input(BenchmarkId::new("t1ha", tx_count), &block, |b, block| {
                    b.iter(|| hash_block::<fasthash::t1ha0::Hasher64>(block))
                });

                group.bench_with_input(BenchmarkId::new("wyhash", tx_count), &block, |b, block| {
                    b.iter(|| hash_block::<WyHash>(block))
                });

                group.bench_with_input(BenchmarkId::new("ahash", tx_count), &block, |b, block| {
                    b.iter(|| hash_block::<AHasher>(block))
                });
            }

            group.finish();
        }

        criterion_group!(benches, benchmark_hashers);
        criterion_main!(benches);
    }
}
