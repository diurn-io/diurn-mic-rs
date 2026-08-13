use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Failure to parse one of this crate's fixed-width code types.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("expected {expected} characters, found {found}")]
    Length { expected: usize, found: usize },
    #[error("invalid character {ch:?} at position {index}")]
    Character { ch: char, index: usize },
}

/// A Market Identifier Code: four uppercase alphanumeric characters.
///
/// `Copy` and four bytes wide, so it is cheap to pass around and to use as a
/// map key without allocating.
///
/// Parsing accepts lowercase and normalises it — `"xnys"` and `"XNYS"` produce
/// the same value — because the uppercase form is canonical and no information
/// is lost. Everything else is rejected.
///
/// ```
/// use diurn_mic::Mic;
/// let mic: Mic = "xnys".parse()?;
/// assert_eq!(mic.as_str(), "XNYS");
/// # Ok::<(), diurn_mic::ParseError>(())
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Mic([u8; 4]);

impl Mic {
    /// Parse from bytes, normalising ASCII lowercase to uppercase.
    pub fn new(s: &str) -> Result<Self, ParseError> {
        let bytes = s.as_bytes();
        if bytes.len() != 4 {
            return Err(ParseError::Length {
                expected: 4,
                found: s.chars().count(),
            });
        }
        let mut out = [0u8; 4];
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
        // Safe: every byte was validated as ASCII alphanumeric at construction.
        std::str::from_utf8(&self.0).expect("Mic is always valid ASCII")
    }

    pub fn as_bytes(&self) -> &[u8; 4] {
        &self.0
    }
}

impl FromStr for Mic {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl fmt::Display for Mic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Shows the code rather than the byte array, so `{:?}` on a record is readable.
impl fmt::Debug for Mic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Mic({})", self.as_str())
    }
}

impl Serialize for Mic {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Mic {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(d)?;
        Mic::new(s).map_err(serde::de::Error::custom)
    }
}

/// The MIC of an operating exchange, as distinct from a segment MIC.
///
/// A separate newtype so that an operating MIC cannot be passed where any MIC
/// is expected — the two are not interchangeable, and the registry indexes them
/// differently.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OperatingMic(Mic);

impl OperatingMic {
    pub fn new(mic: Mic) -> Self {
        Self(mic)
    }

    /// The underlying code. Note that an operating MIC is itself a record in the
    /// registry, so this is also a valid lookup key for [`crate::MicRegistry::get`].
    pub fn mic(&self) -> Mic {
        self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl FromStr for OperatingMic {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Mic::new(s).map(Self)
    }
}

impl fmt::Display for OperatingMic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for OperatingMic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OperatingMic({})", self.as_str())
    }
}

impl Serialize for OperatingMic {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for OperatingMic {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Mic::deserialize(d).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_normalises() {
        assert_eq!(Mic::new("XNYS").unwrap().as_str(), "XNYS");
        assert_eq!(Mic::new("xnys").unwrap().as_str(), "XNYS");
        assert_eq!(Mic::new("XnYs").unwrap().as_str(), "XNYS");
        // Digits are legal in a MIC.
        assert_eq!(Mic::new("A24X").unwrap().as_str(), "A24X");
    }

    #[test]
    fn rejects_bad_input() {
        assert!(matches!(
            Mic::new("XNY"),
            Err(ParseError::Length {
                expected: 4,
                found: 3
            })
        ));
        assert!(matches!(
            Mic::new("XNYSE"),
            Err(ParseError::Length {
                expected: 4,
                found: 5
            })
        ));
        assert!(matches!(
            Mic::new("XN-S"),
            Err(ParseError::Character { ch: '-', index: 2 })
        ));
        assert!(Mic::new("").is_err());
    }

    /// A multibyte character must report a sensible length rather than a byte
    /// count, and must not panic on the char lookup.
    #[test]
    fn handles_non_ascii() {
        assert!(matches!(
            Mic::new("XNÖS"),
            Err(ParseError::Length {
                expected: 4,
                found: 4
            })
        ));
        assert!(Mic::new("ÖÖÖÖ").is_err());
    }

    #[test]
    fn ordering_is_lexicographic() {
        let mut v = [
            Mic::new("XNYS").unwrap(),
            Mic::new("ARCX").unwrap(),
            Mic::new("XNAS").unwrap(),
        ];
        v.sort();
        assert_eq!(
            v.iter().map(|m| m.as_str()).collect::<Vec<_>>(),
            ["ARCX", "XNAS", "XNYS"]
        );
    }
}
