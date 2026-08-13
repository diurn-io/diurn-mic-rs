use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::mic::ParseError;

/// An ISO 3166-1 alpha-2 country code.
///
/// ISO 10383 uses `ZZ` for supranational venues and those with no meaningful
/// country, so `ZZ` is a legitimate value rather than a placeholder to filter
/// out. This type validates shape only — it does not check membership of the
/// ISO 3166 list, which changes independently.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CountryCode([u8; 2]);

impl CountryCode {
    pub fn new(s: &str) -> Result<Self, ParseError> {
        let bytes = s.as_bytes();
        if bytes.len() != 2 {
            return Err(ParseError::Length {
                expected: 2,
                found: s.chars().count(),
            });
        }
        let mut out = [0u8; 2];
        for (i, &b) in bytes.iter().enumerate() {
            if !b.is_ascii_alphabetic() {
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
        std::str::from_utf8(&self.0).expect("CountryCode is always valid ASCII")
    }

    /// `ZZ` — ISO's marker for supranational or country-less venues.
    pub fn is_supranational(&self) -> bool {
        &self.0 == b"ZZ"
    }
}

impl FromStr for CountryCode {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl fmt::Display for CountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for CountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CountryCode({})", self.as_str())
    }
}

impl Serialize for CountryCode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CountryCode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(d)?;
        CountryCode::new(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_normalises() {
        assert_eq!(CountryCode::new("US").unwrap().as_str(), "US");
        assert_eq!(CountryCode::new("gb").unwrap().as_str(), "GB");
    }

    #[test]
    fn zz_is_valid_and_flagged() {
        let zz = CountryCode::new("ZZ").unwrap();
        assert!(zz.is_supranational());
        assert!(!CountryCode::new("US").unwrap().is_supranational());
    }

    #[test]
    fn rejects_digits_and_wrong_length() {
        assert!(CountryCode::new("U1").is_err());
        assert!(CountryCode::new("USA").is_err());
        assert!(CountryCode::new("").is_err());
    }
}
