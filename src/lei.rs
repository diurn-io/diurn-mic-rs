use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::mic::ParseError;

/// An ISO 17442 Legal Entity Identifier: 20 uppercase alphanumeric characters,
/// the last two of which are ISO 7064 MOD 97-10 check digits.
///
/// **Construction validates shape but not the checksum.** A record whose LEI
/// fails its check digits is still a usable record — the entity exists, the code
/// is what the registry says it is, and discarding the row would lose more than
/// it protects. The loader records [`crate::IssueKind::InvalidLeiChecksum`] and
/// keeps the value; call [`Lei::checksum_valid`] if you need to know.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Lei([u8; 20]);

impl Lei {
    pub fn new(s: &str) -> Result<Self, ParseError> {
        let bytes = s.as_bytes();
        if bytes.len() != 20 {
            return Err(ParseError::Length {
                expected: 20,
                found: s.chars().count(),
            });
        }
        let mut out = [0u8; 20];
        for (i, &b) in bytes.iter().enumerate() {
            if !b.is_ascii_alphanumeric() {
                return Err(ParseError::Character {
                    ch: s.chars().nth(i).unwrap_or('\u{fffd}'),
                    index: i,
                });
            }
            out[i] = b.to_ascii_uppercase();
        }
        Ok(Self(out))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("Lei is always valid ASCII")
    }

    /// ISO 7064 MOD 97-10: letters expand to two digits (`A` = 10 … `Z` = 35),
    /// and the resulting number modulo 97 must be 1.
    ///
    /// Computed incrementally so there is no need for big-integer arithmetic —
    /// the running remainder never exceeds `97 * 100 + 35`.
    pub fn checksum_valid(&self) -> bool {
        let mut rem: u32 = 0;
        for &b in &self.0 {
            if b.is_ascii_digit() {
                rem = (rem * 10 + u32::from(b - b'0')) % 97;
            } else {
                rem = (rem * 100 + u32::from(b - b'A') + 10) % 97;
            }
        }
        rem == 1
    }
}

impl FromStr for Lei {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl fmt::Display for Lei {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Lei {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Lei({})", self.as_str())
    }
}

impl Serialize for Lei {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Lei {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(d)?;
        Lei::new(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real LEIs from the pinned ISO vintage.
    const NYSE: &str = "5493000F4ZO33MV32P92";
    const XLON: &str = "213800D1EI4B9WTWWD28";
    const XTKS: &str = "353800279ADEFGKNTV65";

    #[test]
    fn accepts_valid_checksums() {
        for lei in [NYSE, XLON, XTKS] {
            assert!(
                Lei::new(lei).unwrap().checksum_valid(),
                "{lei} should pass MOD 97-10"
            );
        }
    }

    #[test]
    fn detects_a_broken_checksum() {
        // Transpose two characters; the check digits no longer agree.
        let mut s: Vec<char> = NYSE.chars().collect();
        s.swap(2, 3);
        let mangled: String = s.into_iter().collect();
        assert_ne!(mangled, NYSE);
        assert!(!Lei::new(&mangled).unwrap().checksum_valid());
    }

    /// A bad checksum must still parse — the loader keeps the record.
    #[test]
    fn bad_checksum_still_constructs() {
        let lei = Lei::new("00000000000000000000").unwrap();
        assert!(!lei.checksum_valid());
        assert_eq!(lei.as_str(), "00000000000000000000");
    }

    #[test]
    fn rejects_bad_shape() {
        assert!(Lei::new("TOOSHORT").is_err());
        assert!(Lei::new("5493004EX5AV8V80MB5!").is_err());
    }
}
