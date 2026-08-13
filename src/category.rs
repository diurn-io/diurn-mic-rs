use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::mic::ParseError;

/// The ISO 10383 market category code.
///
/// The sixteen named variants are the full registered list as published in the
/// *ISO 10383 MIC Release 2.0 Factsheet*, and [`MarketCategory::description`]
/// returns ISO's own wording for each.
///
/// Note that the registered list is wider than any one vintage uses: the
/// 2026-08-10 file contains only 14 distinct codes, with `ARMS` and `CTPS`
/// registered but unused. They are still named here, because a code appearing
/// for the first time should not be reported as unrecognised.
///
/// # Why this is `non_exhaustive` and has an `Unknown` variant
///
/// The list "can be updated upon request to the RA", so it grows without
/// notice — `CASP`, `TRFS`, and `IDQS` all postdate most published references.
/// An unrecognised code is an expected condition, not a corrupt file. It is
/// preserved verbatim in [`MarketCategory::Unknown`] so a consumer on an older
/// version of this crate still round-trips the value, and the loader records
/// [`crate::IssueKind::UnknownMarketCategory`] rather than failing.
#[non_exhaustive]
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum MarketCategory {
    /// Regulated Market.
    Rmkt,
    /// Multilateral Trading Facility.
    Mltf,
    /// Organised Trading Facility.
    Otfs,
    /// Systematic Internaliser, as defined in MiFID II (Directive 2014/65/EU).
    Sint,
    /// Alternative Trading System.
    Atss,
    /// Designated Contract Market.
    Dcms,
    /// Swap Execution Facility.
    Sefs,
    /// Recognised Market Operator.
    Rmos,
    /// Approved Publication Arrangement.
    Appa,
    /// Approved Reporting Mechanism. Registered, but unused in the 2026-08-10
    /// vintage.
    Arms,
    /// Consolidated Tape Provider. Registered, but unused in the 2026-08-10
    /// vintage.
    Ctps,
    /// Not Specified. The most common category by a wide margin.
    Nspd,
    /// Other.
    Othr,
    /// Crypto Asset Services Provider. A MiCA-era addition.
    Casp,
    /// Trade Reporting Facility.
    Trfs,
    /// Inter-Dealer Quotation System.
    Idqs,
    /// A code this version of the crate does not know, preserved verbatim.
    Unknown([u8; 4]),
}

impl MarketCategory {
    /// Every registered code, ordered as ISO publishes them — alphabetically by
    /// description. Excludes [`MarketCategory::Unknown`], which is not a fixed
    /// code.
    ///
    /// Useful for building a filter list or a lookup table:
    ///
    /// ```
    /// use diurn_mic::MarketCategory;
    /// let options: Vec<(&str, &str)> = MarketCategory::KNOWN
    ///     .iter()
    ///     .map(|c| (c.as_str(), c.description().unwrap()))
    ///     .collect();
    /// assert_eq!(options[0], ("ATSS", "Alternative Trading System"));
    /// ```
    pub const KNOWN: [MarketCategory; 16] = [
        Self::Atss,
        Self::Appa,
        Self::Arms,
        Self::Ctps,
        Self::Casp,
        Self::Dcms,
        Self::Idqs,
        Self::Mltf,
        Self::Nspd,
        Self::Otfs,
        Self::Othr,
        Self::Rmos,
        Self::Rmkt,
        Self::Sefs,
        Self::Sint,
        Self::Trfs,
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
            b"ARMS" => Self::Arms,
            b"CTPS" => Self::Ctps,
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
            Self::Arms => "ARMS",
            Self::Ctps => "CTPS",
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

    /// ISO's own name for the category, for display and lookup.
    ///
    /// `None` for [`MarketCategory::Unknown`]: the code is preserved, but we
    /// genuinely do not know what it means, and inventing an expansion — or
    /// echoing the code back as though it were a name — would be worse than
    /// saying so.
    ///
    /// ```
    /// use diurn_mic::MarketCategory;
    ///
    /// let c = MarketCategory::new("SINT")?;
    /// assert_eq!(c.description(), Some("Systematic Internaliser"));
    ///
    /// let future = MarketCategory::new("ZZZZ")?;
    /// assert_eq!(future.as_str(), "ZZZZ");
    /// assert_eq!(future.description(), None);
    /// # Ok::<(), diurn_mic::ParseError>(())
    /// ```
    ///
    /// Wording is taken verbatim from the *ISO 10383 MIC Release 2.0
    /// Factsheet*, published by the Registration Authority.
    pub fn description(&self) -> Option<&'static str> {
        Some(match self {
            Self::Atss => "Alternative Trading System",
            Self::Appa => "Approved Publication Arrangement",
            Self::Arms => "Approved Reporting Mechanism",
            Self::Ctps => "Consolidated Tape Provider",
            Self::Casp => "Crypto Asset Services Provider",
            Self::Dcms => "Designated Contract Market",
            Self::Idqs => "Inter-Dealer Quotation System",
            Self::Mltf => "Multilateral Trading Facility",
            Self::Nspd => "Not Specified",
            Self::Otfs => "Organised Trading Facility",
            Self::Othr => "Other",
            Self::Rmos => "Recognised Market Operator",
            Self::Rmkt => "Regulated Market",
            Self::Sefs => "Swap Execution Facility",
            Self::Sint => "Systematic Internaliser",
            Self::Trfs => "Trade Reporting Facility",
            Self::Unknown(_) => return None,
        })
    }

    /// Look a category up by its ISO description, ignoring case.
    ///
    /// The inverse of [`MarketCategory::description`], for parsing a filter
    /// value or a query parameter that carries the readable name.
    ///
    /// ```
    /// use diurn_mic::MarketCategory;
    /// assert_eq!(
    ///     MarketCategory::from_description("regulated market"),
    ///     Some(MarketCategory::Rmkt)
    /// );
    /// assert_eq!(MarketCategory::from_description("Nonsense"), None);
    /// ```
    pub fn from_description(name: &str) -> Option<Self> {
        let name = name.trim();
        Self::KNOWN.into_iter().find(|c| {
            c.description()
                .is_some_and(|d| d.eq_ignore_ascii_case(name))
        })
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

    /// Registered by ISO but absent from the 2026-08-10 vintage. Naming them is
    /// the whole point: the first file that carries one must not report it as
    /// unrecognised.
    #[test]
    fn registered_but_unused_codes_are_known() {
        for (code, desc) in [
            ("ARMS", "Approved Reporting Mechanism"),
            ("CTPS", "Consolidated Tape Provider"),
        ] {
            let c = MarketCategory::new(code).unwrap();
            assert!(c.is_known(), "{code} should be a named variant");
            assert_eq!(c.description(), Some(desc));
        }
    }

    #[test]
    fn every_known_code_has_a_description() {
        for c in MarketCategory::KNOWN {
            let d = c.description().unwrap_or_else(|| {
                panic!("{} has no description", c.as_str());
            });
            assert!(!d.is_empty());
            // ISO writes these in title case; a lowercase word would mean
            // someone paraphrased rather than quoted.
            assert!(
                d.chars().next().unwrap().is_uppercase(),
                "{d:?} is not ISO's wording"
            );
        }
    }

    #[test]
    fn unknown_has_no_description() {
        let c = MarketCategory::new("ZZZZ").unwrap();
        assert_eq!(c.description(), None);
        // ...but the code itself is still available.
        assert_eq!(c.as_str(), "ZZZZ");
    }

    #[test]
    fn descriptions_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for c in MarketCategory::KNOWN {
            assert!(
                seen.insert(c.description().unwrap()),
                "duplicate description on {}",
                c.as_str()
            );
        }
        assert_eq!(seen.len(), 16);
    }

    #[test]
    fn description_lookup_round_trips() {
        for c in MarketCategory::KNOWN {
            let d = c.description().unwrap();
            assert_eq!(MarketCategory::from_description(d), Some(c));
            assert_eq!(MarketCategory::from_description(&d.to_lowercase()), Some(c));
            assert_eq!(
                MarketCategory::from_description(&format!("  {d} ")),
                Some(c)
            );
        }
        assert_eq!(MarketCategory::from_description("Not A Category"), None);
        assert_eq!(MarketCategory::from_description(""), None);
    }

    /// The named list is wider than any one vintage. Guards against someone
    /// "tidying up" by deleting the two unused codes.
    #[test]
    fn all_sixteen_registered_codes_are_named() {
        let codes: std::collections::HashSet<_> =
            MarketCategory::KNOWN.iter().map(|c| c.as_str()).collect();
        for code in [
            "ATSS", "APPA", "ARMS", "CTPS", "CASP", "DCMS", "IDQS", "MLTF", "NSPD", "OTFS", "OTHR",
            "RMOS", "RMKT", "SEFS", "SINT", "TRFS",
        ] {
            assert!(codes.contains(code), "{code} is missing from KNOWN");
        }
        assert_eq!(codes.len(), 16);
    }

    #[test]
    fn rejects_wrong_shape() {
        assert!(MarketCategory::new("RMK").is_err());
        assert!(MarketCategory::new("RMKTX").is_err());
    }
}
