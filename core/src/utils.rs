pub fn separate_bytes(bytes: u64) -> String {
    bytes
        .to_string()
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(std::str::from_utf8)
        .collect::<Result<Vec<&str>, _>>()
        .unwrap_or_default()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_separate_bytes() {
        // Test small numbers (no separation needed)
        assert_eq!(separate_bytes(0), "0");
        assert_eq!(separate_bytes(123), "123");

        // Test numbers requiring one comma
        assert_eq!(separate_bytes(1234), "1,234");
        assert_eq!(separate_bytes(999999), "999,999");

        // Test numbers requiring multiple commas
        assert_eq!(separate_bytes(1000000), "1,000,000");
        assert_eq!(separate_bytes(1234567890), "1,234,567,890");

        // Test max u64 value
        assert_eq!(separate_bytes(u64::MAX), "18,446,744,073,709,551,615");
    }
}
