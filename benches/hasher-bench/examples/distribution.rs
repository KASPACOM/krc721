use ahash::AHasher;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::HashMap;
use std::hash::Hasher;
use wyhash::WyHash;

struct TestCase {
    blocks: Vec<[u8; 32]>,
    tx_score: u64,
    max_supply: u64,
}

// Test implementations
fn gen_wyhash_direct(case: &TestCase) -> u64 {
    let mut hasher = WyHash::with_seed(0);
    for hash in &case.blocks {
        hasher.write(hash);
    }
    hasher.write_u64(case.tx_score);
    (hasher.finish() % case.max_supply) + 1
}

fn gen_wyhash_xor_then_hash(case: &TestCase) -> u64 {
    // First XOR block hashes in chunks
    let mut entropy = 0u64;
    for hash in &case.blocks {
        entropy = hash.chunks(8).fold(entropy, |acc, chunk| {
            acc ^ u64::from_le_bytes(chunk.try_into().unwrap())
        });
    }

    // Then hash with tx_score
    let mut hasher = WyHash::with_seed(0);
    hasher.write_u64(entropy);
    hasher.write_u64(case.tx_score);
    (hasher.finish() % case.max_supply) + 1
}

fn gen_ahash_direct(case: &TestCase) -> u64 {
    let mut hasher = AHasher::default();
    for hash in &case.blocks {
        hasher.write(hash);
    }
    hasher.write_u64(case.tx_score);
    (hasher.finish() % case.max_supply) + 1
}

fn gen_ahash_xor_then_hash(case: &TestCase) -> u64 {
    // First XOR block hashes in chunks
    let mut entropy = 0u64;
    for hash in &case.blocks {
        entropy = hash.chunks(8).fold(entropy, |acc, chunk| {
            acc ^ u64::from_le_bytes(chunk.try_into().unwrap())
        });
    }

    // Then hash with tx_score
    let mut hasher = AHasher::default();
    hasher.write_u64(entropy);
    hasher.write_u64(case.tx_score);
    (hasher.finish() % case.max_supply) + 1
}

// Test scenarios
fn test_uniform_distribution(
    generator: impl Fn(&TestCase) -> u64,
    num_blocks: usize,
    max_supply: u64,
    num_samples: usize,
) -> HashMap<u64, usize> {
    let mut rng = StdRng::seed_from_u64(42);
    let mut frequencies = HashMap::new();

    for _ in 0..num_samples {
        // Generate random blocks and tx_score
        let mut blocks = Vec::with_capacity(num_blocks);
        for _ in 0..num_blocks {
            let mut hash = [0u8; 32];
            rng.fill(&mut hash);
            blocks.push(hash);
        }
        let tx_score = rng.gen();

        let case = TestCase {
            blocks,
            tx_score,
            max_supply,
        };

        let token_id = generator(&case);
        *frequencies.entry(token_id).or_insert(0) += 1;
    }

    frequencies
}

fn test_sequential_resistance(
    generator: impl Fn(&TestCase) -> u64,
    num_blocks: usize,
    max_supply: u64,
    sequence_length: usize,
) -> Vec<u64> {
    let mut rng = StdRng::seed_from_u64(42);

    // Generate fixed blocks
    let mut blocks = Vec::with_capacity(num_blocks);
    for _ in 0..num_blocks {
        let mut hash = [0u8; 32];
        rng.fill(&mut hash);
        blocks.push(hash);
    }

    // Generate token IDs with sequential tx_scores
    let mut results = Vec::with_capacity(sequence_length);
    for tx_score in 0..sequence_length as u64 {
        let case = TestCase {
            blocks: blocks.clone(),
            tx_score,
            max_supply,
        };
        results.push(generator(&case));
    }

    results
}

fn analyze_distribution(frequencies: &HashMap<u64, usize>, max_supply: u64) -> (f64, f64, f64) {
    let total_samples = frequencies.values().sum::<usize>() as f64;
    let expected_freq = total_samples / max_supply as f64;

    // Calculate chi-square statistic
    let chi_square = frequencies
        .values()
        .map(|&freq| {
            let diff = freq as f64 - expected_freq;
            diff * diff / expected_freq
        })
        .sum::<f64>();

    // Calculate coverage
    let coverage = frequencies.len() as f64 / max_supply as f64;

    // Calculate maximum deviation
    let max_deviation = frequencies
        .values()
        .map(|&freq| (freq as f64 - expected_freq).abs() / expected_freq)
        .fold(0.0, f64::max);

    (chi_square, coverage, max_deviation)
}

fn analyze_sequence(sequence: &[u64]) -> (f64, f64) {
    if sequence.is_empty() {
        return (0.0, 0.0);
    }

    // Calculate mean absolute difference between consecutive values
    let mean_diff = sequence
        .windows(2)
        .map(|w| (w[1] as f64 - w[0] as f64).abs())
        .sum::<f64>()
        / (sequence.len() - 1) as f64;

    // Calculate autocorrelation with lag 1
    let mean = sequence.iter().sum::<u64>() as f64 / sequence.len() as f64;
    let variance = sequence
        .iter()
        .map(|&x| (x as f64 - mean).powi(2))
        .sum::<f64>()
        / sequence.len() as f64;

    let autocorr = sequence
        .windows(2)
        .map(|w| ((w[0] as f64 - mean) * (w[1] as f64 - mean)))
        .sum::<f64>()
        / ((sequence.len() - 1) as f64 * variance);

    (mean_diff, autocorr)
}

fn main() {
    let configs = [
        (10, 1000, 100000),   // 10 blocks, 1K supply, 100K samples
        (20, 10000, 200000),  // 20 blocks, 10K supply, 200K samples
        (50, 100000, 500000), // 50 blocks, 100K supply, 500K samples
    ];

    let generators = [
        ("WyHash Direct", gen_wyhash_direct as fn(&TestCase) -> u64),
        (
            "WyHash XOR+Hash",
            gen_wyhash_xor_then_hash as fn(&TestCase) -> u64,
        ),
        ("AHash Direct", gen_ahash_direct as fn(&TestCase) -> u64),
        (
            "AHash XOR+Hash",
            gen_ahash_xor_then_hash as fn(&TestCase) -> u64,
        ),
    ];

    println!("Distribution Analysis Results\n");

    for &(num_blocks, max_supply, num_samples) in &configs {
        println!("\nConfiguration:");
        println!("  Blocks: {}", num_blocks);
        println!("  Max Supply: {}", max_supply);
        println!("  Samples: {}", num_samples);
        println!();

        for (name, generator) in &generators {
            // Test uniform distribution
            let frequencies =
                test_uniform_distribution(generator, num_blocks, max_supply, num_samples);
            let (chi_square, coverage, max_deviation) =
                analyze_distribution(&frequencies, max_supply);

            // Test sequential resistance
            let sequence = test_sequential_resistance(generator, num_blocks, max_supply, 1000);
            let (mean_diff, autocorr) = analyze_sequence(&sequence);

            println!("{}:", name);
            println!("  Distribution Stats:");
            println!("    Chi-square: {:.2}", chi_square);
            println!("    Coverage: {:.2}%", coverage * 100.0);
            println!("    Max deviation: {:.2}%", max_deviation * 100.0);
            println!("  Sequential Stats:");
            println!("    Mean diff: {:.2}", mean_diff);
            println!("    Autocorrelation: {:.4}", autocorr);
            println!();

            // Export data for visualization
            let mut csv = String::from("token_id,frequency\n");
            for token_id in 1..=max_supply {
                csv.push_str(&format!(
                    "{},{}\n",
                    token_id,
                    frequencies.get(&token_id).unwrap_or(&0)
                ));
            }

            std::fs::write(
                format!(
                    "dist_{}_b{}_s{}.csv",
                    name.to_lowercase().replace(" ", "_"),
                    num_blocks,
                    max_supply
                ),
                csv,
            )
            .expect("Failed to write CSV");
        }
    }
}
