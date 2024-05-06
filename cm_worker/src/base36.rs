//! Decode base36.
use itertools::Itertools;

/// Decode a base36 string (0-9aA-zZ) as a u64.
pub fn decode(s: &str) -> Result<u64, char> {
    s.bytes()
        .map(|b| b.is_ascii_alphanumeric().then_some(b).ok_or(b as char))
        .map_ok(|b| b.to_ascii_uppercase())
        .map_ok(|b| {
            if b.is_ascii_digit() {
                b - b'0'
            } else {
                b - b'A' + 10
            }
        })
        .fold_ok(0, |a, b| a * 36 + (b as u64))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_decode() {
        assert_eq!(Ok(23806698), decode("e69d6"));
    }
}
