use crate::error::TickError;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::{ops::Deref, str::FromStr};

pub const TICK_LENGTH: usize = 10;
pub const TICK_MIN_LENGTH: usize = 1;

#[repr(transparent)]
#[derive(Clone, Copy, Eq, PartialEq, Hash, BorshSerialize, BorshDeserialize, Ord, PartialOrd)]
pub struct Tick(pub [u8; TICK_LENGTH]);

impl Default for Tick {
    fn default() -> Self {
        Tick::MIN
    }
}

impl Display for Tick {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = unsafe { std::str::from_utf8_unchecked(&self.0) }.trim_end_matches('\0');
        f.write_str(s)
    }
}

impl Tick {
    pub const MAX: Self = Self([255; TICK_LENGTH]); // todo it's invalid tick. but proper fix requires too many changes, must only be used in iterators as upper bound
    pub const MIN: Self = Tick([b'0', 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    /// Creates a new instance without performing UTF-8 validation checks.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `b` contains valid UTF-8 encoded bytes.
    /// Passing invalid UTF-8 bytes will result in undefined behavior when
    /// the value is later interpreted as a UTF-8 string.
    ///
    /// # Arguments
    ///
    /// * `b` - Byte array that must be valid UTF-8
    pub unsafe fn new_unchecked(b: [u8; TICK_LENGTH]) -> Self {
        Self(b)
    }

    pub fn new(b: [u8; TICK_LENGTH]) -> Result<Self, TickError> {
        // Validate UTF-8
        let s = std::str::from_utf8(&b)?;

        // Trim null bytes for validation and convert to uppercase
        let s = s.trim_end_matches('\0').to_uppercase();

        if !s.chars().all(|c| c.is_alphanumeric()) {
            return Err(TickError::NonAlphabetOrDigit);
        }

        let char_count = s.chars().count();
        if !(TICK_MIN_LENGTH..=TICK_LENGTH).contains(&char_count) {
            return Err(TickError::TickLength);
        }

        let mut bytes = [0u8; TICK_LENGTH];
        let utf8_bytes = s.as_bytes();
        if utf8_bytes.len() > bytes.len() {
            return Err(TickError::TickBytesLength);
        }
        bytes[..utf8_bytes.len()].copy_from_slice(s.as_bytes());
        Ok(Self(bytes))
    }

    pub fn as_str(&self) -> &str {
        let s = unsafe { std::str::from_utf8_unchecked(&self.0) };
        s.trim_end_matches('\0')
    }
}

impl Deref for Tick {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for Tick {
    type Err = TickError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Convert to uppercase first
        let s = s.to_uppercase();

        if !s.chars().all(|c| c.is_alphanumeric()) {
            return Err(TickError::NonAlphabetOrDigit);
        }

        let char_count = s.chars().count();
        if !(TICK_MIN_LENGTH..=TICK_LENGTH).contains(&char_count) {
            return Err(TickError::TickLength);
        }

        let mut bytes = [0u8; TICK_LENGTH];
        let utf8_bytes = s.as_bytes();
        if utf8_bytes.len() > bytes.len() {
            return Err(TickError::TickBytesLength);
        }
        bytes[0..utf8_bytes.len()].copy_from_slice(s.as_bytes());
        Ok(Self(bytes))
    }
}

impl TryFrom<&str> for Tick {
    type Error = TickError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::from_str(s)
    }
}

impl TryFrom<String> for Tick {
    type Error = TickError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_str(s.as_str())
    }
}

impl Serialize for Tick {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Convert to string, trimming trailing zeros
        let s = unsafe { std::str::from_utf8_unchecked(&self.0) }.trim_end_matches('\0');
        serializer.serialize_str(s)
    }
}

impl<'de> Deserialize<'de> for Tick {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <String as Deserialize>::deserialize(deserializer)?;

        // Convert to uppercase first
        let s = s.to_uppercase();

        let char_count = s.chars().count();

        if char_count > TICK_LENGTH {
            return Err(serde::de::Error::custom("tick string too long"));
        }
        if char_count < TICK_MIN_LENGTH {
            return Err(serde::de::Error::custom("tick string too short"));
        }

        if s.len() > TICK_LENGTH {
            return Err(serde::de::Error::custom("tick string bytes size too big"));
        }

        if !s.chars().all(|c| c.is_alphanumeric()) {
            return Err(serde::de::Error::custom("tick contains not allowed chars"));
        }

        let mut bytes = [0u8; TICK_LENGTH];
        bytes[..s.len()].copy_from_slice(s.as_bytes());
        Ok(Tick(bytes))
    }
}

// Add custom Debug implementation
impl std::fmt::Debug for Tick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Use the same format as Display
        std::fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tick_length_validation() {
        // Too short
        assert!(matches!(Tick::from_str(""), Err(TickError::TickLength)));

        // Too long
        assert!(matches!(
            Tick::from_str("toolongname"),
            Err(TickError::TickLength)
        ));

        // Too long in bytes
        assert!(matches!(
            Tick::from_str("漢漢漢漢漢漢漢漢漢漢"),
            Err(TickError::TickBytesLength)
        ));

        // Too long in bytes
        assert!(matches!(
            Tick::from_str("漢漢漢漢漢"),
            Err(TickError::TickBytesLength)
        ));

        // Valid lengths
        assert!(Tick::from_str("a").is_ok()); // 1 chars
        assert!(Tick::from_str("ab").is_ok()); // 2 chars
        assert!(Tick::from_str("abc").is_ok()); // 3 chars
        assert!(Tick::from_str("abcd").is_ok()); // 4 chars
        assert!(Tick::from_str("abcde").is_ok()); // 5 chars
        assert!(Tick::from_str("abcdef").is_ok()); // 6 chars
        assert!(Tick::from_str("abcdefg").is_ok()); // 7 chars
        assert!(Tick::from_str("abcdefgh").is_ok()); // 8 chars
        assert!(Tick::from_str("abcdefghi").is_ok()); // 9 chars
        assert!(Tick::from_str("abcdefghij").is_ok()); // 10 chars

        // Non-Latin characters
        assert!(Tick::from_str("абвг").is_ok()); // Cyrillic
        assert!(Tick::from_str("カスパ").is_ok()); // Japanese
        assert!(Tick::from_str("カ").is_ok()); // Japanese short
        assert!(Tick::from_str("têst").is_ok()); // Accented
    }

    #[test]
    fn test_tick_character_validation() {
        // Valid characters
        assert!(Tick::from_str("abcd").is_ok());
        assert!(Tick::from_str("1234").is_ok());
        assert!(Tick::from_str("ab12").is_ok());
        assert!(Tick::from_str("ğ").is_ok());

        // Invalid characters
        assert!(matches!(
            Tick::from_str("abc!"),
            Err(TickError::NonAlphabetOrDigit)
        ));
        assert!(matches!(
            Tick::from_str("abc-"),
            Err(TickError::NonAlphabetOrDigit)
        ));
        assert!(matches!(
            Tick::from_str("ağc "),
            Err(TickError::NonAlphabetOrDigit)
        ));
        assert!(matches!(
            Tick::from_str("abc."),
            Err(TickError::NonAlphabetOrDigit)
        ));
    }

    #[test]
    fn test_tick_case_normalization() {
        let lowercase = Tick::from_str("abcd").unwrap();
        let uppercase = Tick::from_str("ABCD").unwrap();
        let mixed_case = Tick::from_str("aBcD").unwrap();
        let chinese = Tick::from_str("字").unwrap();

        assert_eq!(lowercase.as_str(), "ABCD");
        assert_eq!(uppercase.as_str(), "ABCD");
        assert_eq!(mixed_case.as_str(), "ABCD");
        assert_eq!(chinese.as_str(), "字");

        // All should be equal after normalization
        assert_eq!(lowercase, uppercase);
        assert_eq!(lowercase, mixed_case);
        assert_eq!(uppercase, mixed_case);
    }

    #[test]
    fn test_tick_null_padding() {
        // Create tick with different lengths
        let tick4 = Tick::from_str("ABCD").unwrap();
        let tick6 = Tick::from_str("ABCDEF").unwrap();
        let tick8 = Tick::from_str("ABCDEFGHIJ").unwrap();

        // Check internal representation
        assert_eq!(tick4.0[..4], *b"ABCD");
        assert_eq!(tick4.0[4..], [0, 0, 0, 0, 0, 0]);

        assert_eq!(tick6.0[..6], *b"ABCDEF");
        assert_eq!(tick6.0[6..], [0, 0, 0, 0]);

        assert_eq!(tick8.0[..10], *b"ABCDEFGHIJ");

        // Check string representation trims nulls
        assert_eq!(tick4.as_str(), "ABCD");
        assert_eq!(tick6.as_str(), "ABCDEF");
        assert_eq!(tick8.as_str(), "ABCDEFGHIJ");
    }

    #[test]
    fn test_tick_new_with_bytes() {
        // Valid case
        let mut bytes = [0u8; TICK_LENGTH];
        bytes[..4].copy_from_slice(b"ABCD");
        assert!(Tick::new(bytes).is_ok());

        // Invalid characters in bytes
        let mut bytes = [0u8; TICK_LENGTH];
        bytes[..4].copy_from_slice(b"ABC!");
        assert!(matches!(
            Tick::new(bytes),
            Err(TickError::NonAlphabetOrDigit)
        ));

        // Test with uppercase that should be converted
        let mut bytes = [0u8; TICK_LENGTH];
        bytes[..4].copy_from_slice(b"ABCD");
        let tick = Tick::new(bytes).unwrap();
        assert_eq!(tick.as_str(), "ABCD");
    }

    #[test]
    fn test_display_and_debug() {
        let tick = Tick::from_str("test123").unwrap();

        // Test Display implementation
        assert_eq!(tick.to_string(), "TEST123");

        // Test Debug implementation
        assert_eq!(format!("{:?}", tick), "TEST123");
    }

    use serde_json::{from_str, json, to_string};

    #[test]
    fn test_serde_json_serialization() {
        // Test valid serialization
        let tick = Tick::from_str("test123").unwrap();
        let serialized = to_string(&tick).unwrap();
        assert_eq!(serialized, "\"TEST123\"");

        // Test max length
        let tick = Tick::from_str("abcd1234").unwrap();
        let serialized = to_string(&tick).unwrap();
        assert_eq!(serialized, "\"ABCD1234\"");

        // Ensure nulls are trimmed in serialization
        let mut bytes = [0u8; TICK_LENGTH];
        bytes[..4].copy_from_slice(b"test");
        let tick = unsafe { Tick::new_unchecked(bytes) };
        let serialized = to_string(&tick).unwrap();
        assert_eq!(serialized, "\"test\"");
    }

    #[test]
    fn test_serde_json_deserialization() {
        // Test valid cases
        assert!(from_str::<Tick>("\"test123\"").is_ok());
        assert!(from_str::<Tick>("\"abcd\"").is_ok());
        assert!(from_str::<Tick>("\"1234abcd\"").is_ok());
        assert!(from_str::<Tick>("\"têst123\"").is_ok()); // accented char

        // Test case normalization
        let tick: Tick = from_str("\"test123\"").unwrap();
        assert_eq!(tick.as_str(), "TEST123");

        let tick: Tick = from_str("\"TeSt123\"").unwrap();
        assert_eq!(tick.as_str(), "TEST123");

        // Test invalid length
        assert!(from_str::<Tick>("\"\"").is_err()); // too short
        assert!(from_str::<Tick>("\"12345678910\"").is_err()); // too long

        // Test invalid characters
        assert!(from_str::<Tick>("\"test!123\"").is_err()); // special char
        assert!(from_str::<Tick>("\"test-123\"").is_err()); // hyphen
        assert!(from_str::<Tick>("\"test 123\"").is_err()); // space
    }

    #[test]
    fn test_serde_structured_data() {
        // Test as part of a larger structure
        #[derive(Serialize, Deserialize)]
        struct TestStruct {
            tick: Tick,
            value: i32,
        }

        // Test serialization
        let test_struct = TestStruct {
            tick: Tick::from_str("test123").unwrap(),
            value: 42,
        };

        let serialized = to_string(&test_struct).unwrap();
        let expected = json!({
            "tick": "TEST123",
            "value": 42
        })
        .to_string();
        assert_eq!(serialized, expected);

        // Test deserialization
        let deserialized: TestStruct = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.tick.as_str(), "TEST123");
        assert_eq!(deserialized.value, 42);
    }

    #[test]
    fn test_serde_null_handling() {
        // Test that padding nulls are handled properly during serialization
        let mut bytes = [0u8; TICK_LENGTH];
        bytes[..6].copy_from_slice(b"test12");
        let tick = unsafe { Tick::new_unchecked(bytes) };

        let serialized = to_string(&tick).unwrap();
        assert_eq!(serialized, "\"test12\"");

        // Verify that nulls in the middle are rejected
        assert!(from_str::<Tick>("\"te\0st\"").is_err());
    }

    #[test]
    fn test_serde_invalid_json() {
        // Test invalid JSON formats
        assert!(from_str::<Tick>("null").is_err());
        assert!(from_str::<Tick>("42").is_err());
        assert!(from_str::<Tick>("[]").is_err());
        assert!(from_str::<Tick>("{}").is_err());
        assert!(from_str::<Tick>("").is_err());
    }

    #[test]
    fn test_serde_with_collections() {
        // Test with Vec
        let ticks = vec![
            Tick::from_str("test123").unwrap(),
            Tick::from_str("demo456").unwrap(),
        ];
        let serialized = to_string(&ticks).unwrap();
        let expected = json!(["TEST123", "DEMO456"]).to_string();
        assert_eq!(serialized, expected);

        // Test with HashMap
        use std::collections::HashMap;
        let mut map = HashMap::new();
        map.insert(Tick::from_str("test123").unwrap(), 1);
        map.insert(Tick::from_str("demo456").unwrap(), 2);

        let serialized = to_string(&map).unwrap();
        let deserialized: HashMap<Tick, i32> = serde_json::from_str(&serialized).unwrap();

        assert_eq!(
            deserialized.get(&Tick::from_str("TEST123").unwrap()),
            Some(&1)
        );
        assert_eq!(
            deserialized.get(&Tick::from_str("DEMO456").unwrap()),
            Some(&2)
        );
    }

    #[test]
    fn test_serde_json_deserialization_edge_cases() {
        // Test various invalid cases and ensure proper error handling
        let test_cases = [
            "\"test·test\"",   // middle dot
            "\"test‑test\"",   // non-breaking hyphen
            "\"test–test\"",   // en dash
            "\"test—test\"",   // em dash
            "\"test\\u200B\"", // zero-width space
            // Invalid lengths
            "\"reallylong0\"",   // 11 chars
            "\"reallytoolong\"", // 12 chars
            "\"\"",              // empty string
            // Invalid UTF-8 sequences (as escaped JSON strings)
            "\"test\\xFF\"",   // Invalid UTF-8 byte
            "\"test\\u0000\"", // Null byte
            "\"test\\u0001\"", // Control character
            "\"test\\u001F\"", // Control character
            "\"test\\u007F\"", // DEL character
            // Mixed invalid cases
            "\"test\\u200Btest\"", // Zero-width space in middle
            "\"test\\u0000test\"", // Null in middle
            "\"test\ntest\"",      // Newline
            "\"test\ttest\"",      // Tab
            "\"test\rtest\"",      // Carriage return
            // Symbols and emojis
            "\"test⚡\"", // Lightning symbol
            "\"test⭐\"", // Star
            "\"test🚀\"", // Rocket emoji
            "\"test👍\"", // Thumbs up emoji
            // Mixed alphanumeric with invalid characters
            "\"test!123\"", // With !
            "\"test@123\"", // With @
            "\"test#123\"", // With #
            "\"test$123\"", // With $
            "\"test%123\"", // With %
            "\"test&123\"", // With &
            "\"test*123\"", // With *
            "\"test+123\"", // With +
            "\"test=123\"", // With =
            "\"test?123\"", // With ?
            // Spaces and formatting
            "\"test 123\"",       // Space
            "\"test\\u00A0123\"", // Non-breaking space
            "\"test\\u2003123\"", // Em space
            "\"test\\u2002123\"", // En space
            "\"test\\u2009123\"", // Thin space
            // JSON escape sequences
            "\"test\\\\123\"", // Backslash
            "\"test\\/123\"",  // Forward slash
            "\"test\\\"123\"", // Quote
            "\"test\\b123\"",  // Backspace
            "\"test\\f123\"",  // Form feed
            "\"test\\n123\"",  // Newline
            "\"test\\r123\"",  // Carriage return
            "\"test\\t123\"",  // Tab
        ];

        for test_case in test_cases {
            let result = from_str::<Tick>(test_case);
            assert!(
                result.is_err(),
                "Expected error for input {}, but got success",
                test_case
            );
        }

        // Test valid cases for comparison
        let valid_cases = [
            "\"test123\"",
            "\"abcd123\"",
            "\"1234567\"",
            "\"abcd\"",
            "\"test\"",
            "\"1234\"",
            "\"a\"",          // 1 char
            "\"ab\"",         // 2 chars
            "\"abc\"",        // 3 chars
            "\"tenchars01\"", // 10 chars
            // Non-Latin characters
            "\"カスパ\"", // Japanese
            "\"коин\"",   // Cyrillic
            "\"币安\"",   // Chinese
            "\"٠١٢٣\"",   // Arabic numerals
            "\"αβγδ\"",   // Greek
            "\"טעסט\"",   // Hebrew
            // Special characters and diacritics
            "\"tëst\"", // umlaut
            "\"téśt\"", // accents
        ];

        for valid_case in valid_cases {
            let result = from_str::<Tick>(valid_case);
            assert!(
                result.is_ok(),
                "Expected success for valid input {}, but got error: {:?}",
                valid_case,
                result.err()
            );
        }
    }
}
