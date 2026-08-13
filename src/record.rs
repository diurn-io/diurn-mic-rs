use std::fmt;
use std::str::FromStr;

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::{CountryCode, Lei, MarketCategory, Mic, OperatingMic};

/// Whether a record describes an operating exchange or one of its segments.
///
/// The `OPRT/SGMT` column, which ISO also calls the *MIC Type*.
///
/// Note that this says nothing about which market calendar applies — segment
/// status and parentage are orthogonal to trading hours. `XTKS` is a segment
/// and `XNYS` is an operating MIC, and both keep an ordinary equity calendar.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum MicKind {
    /// An "entity operating an exchange/market/trade reporting facility in a
    /// specific market/country" (ISO 10383 RA).
    Operating,
    /// A "section of an exchange/market/trade reporting facility that
    /// specialises in one or more specific instruments or that is regulated
    /// differently" (ISO 10383 RA).
    Segment,
}

impl MicKind {
    pub const KNOWN: [MicKind; 2] = [Self::Operating, Self::Segment];

    /// The four-letter code as it appears in the file.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Operating => "OPRT",
            Self::Segment => "SGMT",
        }
    }

    /// ISO's expansion of the code, for display.
    ///
    /// Unlike [`crate::MarketCategory::description`] this returns a plain
    /// `&str` rather than an `Option`: the column is a closed two-value
    /// enumeration with no room for ISO to add a third, so there is no unknown
    /// case to represent.
    ///
    /// ```
    /// use diurn_mic::MicKind;
    /// assert_eq!(MicKind::Operating.as_str(), "OPRT");
    /// assert_eq!(MicKind::Operating.description(), "Operating");
    /// ```
    ///
    /// Wording is ISO's own — the factsheet writes "OPRT (Operating) or SGMT
    /// (Segment)".
    pub fn description(&self) -> &'static str {
        match self {
            Self::Operating => "Operating",
            Self::Segment => "Segment",
        }
    }
}

impl FromStr for MicKind {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "OPRT" => Ok(Self::Operating),
            "SGMT" => Ok(Self::Segment),
            _ => Err(()),
        }
    }
}

impl fmt::Display for MicKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// Serialized as the code that appears in the file — `OPRT`, not `Operating`.
// Deriving this would emit the Rust variant name, which nothing else in the
// ecosystem uses and which would disagree with every other code type in this
// crate.
impl Serialize for MicKind {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MicKind {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(d)?;
        s.parse()
            .map_err(|_| serde::de::Error::custom(format!("not a MIC type: {s:?}")))
    }
}

/// The registration status of a MIC.
///
/// # `Updated` is a pending state
///
/// It is tempting to read these as three steady states. They are not.
///
/// ISO publishes the registry on the second Monday of each month, and the
/// modifications that file carries take effect on the **fourth** Monday. A
/// record marked `Updated` is one whose change has been published but is not yet
/// in force; its `last_updated` is the future effective date, not the date
/// someone edited it.
///
/// In the 2026-08-10 vintage all 23 `Updated` records carry `last_updated` of
/// 2026-08-24 — precisely the fourth Monday.
///
/// Use [`crate::MicRegistry::as_of`] rather than treating `Updated` as current.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Status {
    Active,
    /// Published but not yet in force. See the type-level note.
    Updated,
    Expired,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Updated => "UPDATED",
            Self::Expired => "EXPIRED",
        }
    }
}

impl FromStr for Status {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "ACTIVE" => Ok(Self::Active),
            "UPDATED" => Ok(Self::Updated),
            "EXPIRED" => Ok(Self::Expired),
            _ => Err(()),
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// As with `MicKind`: the registry's own spelling, `ACTIVE`, not `Active`.
impl Serialize for Status {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(d)?;
        s.parse()
            .map_err(|_| serde::de::Error::custom(format!("not a status: {s:?}")))
    }
}

/// One row of the ISO 10383 registry.
///
/// Immutable after loading, so the string fields are `Box<str>` rather than
/// `String` — there is no capacity to carry around, and ~2,900 records
/// noticeably benefit.
///
/// Empty CSV fields become `None`, never `Some("")`. The source file writes
/// empties inconsistently — sometimes `""`, sometimes a bare comma — and both
/// mean absent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MicRecord {
    pub mic: Mic,
    /// For an operating MIC this equals `mic`; for a segment it is the parent.
    pub operating_mic: OperatingMic,
    pub kind: MicKind,
    pub market_name: Box<str>,
    pub legal_entity_name: Option<Box<str>>,
    pub lei: Option<Lei>,
    pub category: MarketCategory,
    pub acronym: Option<Box<str>>,
    pub country: Option<CountryCode>,
    pub city: Option<Box<str>>,
    pub website: Option<Box<str>>,
    pub status: Status,
    pub created: Option<Date>,
    /// For a `Updated` record this is the **effective** date of a pending
    /// change, which may be in the future relative to the file's publication.
    pub last_updated: Option<Date>,
    pub last_validated: Option<Date>,
    pub expires: Option<Date>,
    pub comments: Option<Box<str>>,
}

impl MicRecord {
    /// Whether this record is its own operating MIC — true for every `Operating`
    /// record in a well-formed file.
    pub fn is_operating(&self) -> bool {
        self.kind == MicKind::Operating
    }

    /// Whether the record's changes are in force on `date`.
    ///
    /// A record whose `last_updated` is after `date` describes a modification
    /// that has been published but has not taken effect yet.
    pub fn is_in_force_on(&self, date: Date) -> bool {
        match self.last_updated {
            Some(effective) => effective <= date,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mic_kind_round_trips_through_its_code() {
        for k in MicKind::KNOWN {
            assert_eq!(k.as_str().parse::<MicKind>().unwrap(), k);
        }
    }

    #[test]
    fn mic_kind_descriptions_are_isos_wording() {
        assert_eq!(MicKind::Operating.as_str(), "OPRT");
        assert_eq!(MicKind::Operating.description(), "Operating");
        assert_eq!(MicKind::Segment.as_str(), "SGMT");
        assert_eq!(MicKind::Segment.description(), "Segment");
    }

    /// The code and its expansion must never be confused for one another.
    #[test]
    fn code_and_description_are_distinct() {
        for k in MicKind::KNOWN {
            assert_ne!(k.as_str(), k.description());
            assert_eq!(k.as_str().len(), 4);
        }
    }

    #[test]
    fn mic_kind_parsing_is_lenient_about_case_and_padding() {
        assert_eq!("oprt".parse::<MicKind>().unwrap(), MicKind::Operating);
        assert_eq!("  SGMT ".parse::<MicKind>().unwrap(), MicKind::Segment);
        assert!("OPERATING".parse::<MicKind>().is_err());
        assert!("".parse::<MicKind>().is_err());
    }

    /// Every code type in this crate serializes as the spelling the registry
    /// itself uses. Consumers — the CLI, the API, anything reading their JSON —
    /// then see one vocabulary rather than two.
    #[test]
    fn codes_serialize_as_registry_spellings_not_rust_names() {
        assert_eq!(
            serde_json::to_string(&MicKind::Operating).unwrap(),
            "\"OPRT\""
        );
        assert_eq!(
            serde_json::to_string(&MicKind::Segment).unwrap(),
            "\"SGMT\""
        );
        assert_eq!(
            serde_json::to_string(&Status::Active).unwrap(),
            "\"ACTIVE\""
        );
        assert_eq!(
            serde_json::to_string(&Status::Updated).unwrap(),
            "\"UPDATED\""
        );
        assert_eq!(
            serde_json::to_string(&Status::Expired).unwrap(),
            "\"EXPIRED\""
        );
    }

    #[test]
    fn codes_deserialize_from_registry_spellings() {
        assert_eq!(
            serde_json::from_str::<MicKind>("\"OPRT\"").unwrap(),
            MicKind::Operating
        );
        assert_eq!(
            serde_json::from_str::<Status>("\"UPDATED\"").unwrap(),
            Status::Updated
        );
        // Parsing is case-insensitive, matching the rest of the crate, so
        // "active" is accepted as a spelling of ACTIVE. Note the coincidence:
        // "Active" therefore parses too, but as a case variant of the code —
        // not because the Rust variant name is a recognised value. `MicKind`
        // makes that unambiguous, since "Operating" is not a case variant of
        // OPRT and is rejected.
        assert_eq!(
            serde_json::from_str::<Status>("\"active\"").unwrap(),
            Status::Active
        );
        assert!(serde_json::from_str::<MicKind>("\"Operating\"").is_err());
        assert!(serde_json::from_str::<Status>("\"Pending\"").is_err());
    }

    #[test]
    fn status_round_trips_through_its_code() {
        for (s, code) in [
            (Status::Active, "ACTIVE"),
            (Status::Updated, "UPDATED"),
            (Status::Expired, "EXPIRED"),
        ] {
            assert_eq!(s.as_str(), code);
            assert_eq!(code.parse::<Status>().unwrap(), s);
        }
    }
}
