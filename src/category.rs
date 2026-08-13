use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::mic::ParseError;

/// The ISO 10383 market category code.
///
/// # Why this is `non_exhaustive` and has an `Unknown` variant
///
/// ISO revises the code list without warning, and the current file already
/// proves it: the 2026-08-10 vintage contains 14 distinct codes, three of which
/// (`CASP`, `TRFS`, `IDQS`) postdate most published references.
///
/// An unrecognised code is therefore an expected condition, not a corrupt file.
/// It is preserved verbatim in [`MarketCategory::Unknown`] so that a consumer on
/// an older version of this crate still round-trips the value, and the loader
/// records [`crate::IssueKind::UnknownMarketCategory`] rather than failing.
#[non_exhaustive]
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum MarketCategory {
    /// Regulated market.
    Rmkt,
    /// Multilateral trading facility.
    Mltf,
    /// Organised trading facility.
    Otfs,
    /// Systematic internaliser.
    Sint,
    /// Alternative trading system.
    Atss,
    /// Designated contract market.
    Dcms,
    /// Swap execution facility.
    Sefs,
    /// Recognised market operator.
    Rmos,
    /// Approved publication arrangement.
    Appa,
    /// Not specified.
    Nspd,
    /// Other.
    Othr,
    /// Crypto-asset service provider. A MiCA-era addition.
    Casp,
    /// Trade reporting facility.
    Trfs,
    /// Inter-dealer quotation system.
    Idqs,
    /// A code this version of the crate does not know, preserved verbatim.
    Unknown([u8; 4]),
}

impl MarketCategory {
    /// Every named variant, in the order they appear in [`MarketCategory`].
    /// Excludes [`MarketCategory::Unknown`], which is not a fixed code.
    pub const KNOWN: [MarketCategory; 14] = [
        Self::Rmkt,
        Self::Mltf,
        Self::Otfs,
        Self::Sint,
        Self::Atss,
        Self::Dcms,
        Self::Sefs,
        Self::Rmos,
        Self::Appa,
        Self::Nspd,
        Self::Othr,
        Self::Casp,
        Self::Trfs,
        Self::Idqs,
    ];

    pub fn new(s: &str) -> Result<Self, ParseError> {
        let bytes = s.as_bytes();
        if bytes.len() != 4 {
            return Err(ParseError::Length {
                expected: 4,
                found: s.chars().count(),
            });
        }
        let mut code = [0u8; 4];
        for (i, &b) in bytes.iter().enumerate() {
            if !b.is_ascii_alphanumeric() {
                return Err(ParseError::Character {
                    ch: s.chars().nth(i).unwrap_or('\u{fffd}'),
                    index: i,
                });
            }
            code[i] = b.to_ascii_uppercase();
        }
        Ok(match &code {
            b"RMKT" => Self::Rmkt,
            b"MLTF" => Self::Mltf,
            b"OTFS" => Self::Otfs,
            b"SINT" => Self::Sint,
            b"ATSS" => Self::Atss,
            b"DCMS" => Self::Dcms,
            b"SEFS" => Self::Sefs,
            b"RMOS" => Self::Rmos,
            b"APPA" => Self::Appa,
            b"NSPD" => Self::Nspd,
            b"OTHR" => Self::Othr,
            b"CASP" => Self::Casp,
            b"TRFS" => Self::Trfs,
            b"IDQS" => Self::Idqs,
            _ => Self::Unknown(code),
        })
    }

    /// The four-character code. Borrows from `self` so that `Unknown` can
    /// return its preserved bytes.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Rmkt => "RMKT",
            Self::Mltf => "MLTF",
            Self::Otfs => "OTFS",
            Self::Sint => "SINT",
            Self::Atss => "ATSS",
            Self::Dcms => "DCMS",
            Self::Sefs => "SEFS",
            Self::Rmos => "RMOS",
            Self::Appa => "APPA",
            Self::Nspd => "NSPD",
            Self::Othr => "OTHR",
            Self::Casp => "CASP",
            Self::Trfs => "TRFS",
            Self::Idqs => "IDQS",
            Self::Unknown(code) => {
                std::str::from_utf8(code).expect("Unknown code is always valid ASCII")
            }
        }
    }

    pub fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}

impl FromStr for MarketCategory {
    type Err = ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl fmt::Display for MarketCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for MarketCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(_) => write!(f, "Unknown({})", self.as_str()),
            _ => f.write_str(self.as_str()),
        }
    }
}

impl Serialize for MarketCategory {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MarketCategory {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(d)?;
        MarketCategory::new(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_long_standing_codes() {
        assert_eq!(MarketCategory::new("RMKT").unwrap(), MarketCategory::Rmkt);
        assert_eq!(MarketCategory::new("NSPD").unwrap(), MarketCategory::Nspd);
    }

    /// The three codes that postdate most published references, all present in
    /// the pinned vintage.
    #[test]
    fn parses_the_recent_additions() {
        assert_eq!(MarketCategory::new("CASP").unwrap(), MarketCategory::Casp);
        assert_eq!(MarketCategory::new("TRFS").unwrap(), MarketCategory::Trfs);
        assert_eq!(MarketCategory::new("IDQS").unwrap(), MarketCategory::Idqs);
    }

    #[test]
    fn preserves_unknown_codes() {
        let c = MarketCategory::new("ZZZZ").unwrap();
        assert_eq!(c, MarketCategory::Unknown(*b"ZZZZ"));
        assert_eq!(c.as_str(), "ZZZZ");
        assert!(!c.is_known());
    }

    #[test]
    fn known_list_round_trips() {
        for c in MarketCategory::KNOWN {
            assert!(c.is_known());
            assert_eq!(MarketCategory::new(c.as_str()).unwrap(), c);
        }
    }

    #[test]
    fn rejects_wrong_shape() {
        assert!(MarketCategory::new("RMK").is_err());
        assert!(MarketCategory::new("RMKTX").is_err());
    }
}
