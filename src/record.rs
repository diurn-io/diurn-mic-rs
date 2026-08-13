use std::fmt;
use std::str::FromStr;

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::{CountryCode, Lei, MarketCategory, Mic, OperatingMic};

/// Whether a record describes an operating exchange or one of its segments.
///
/// Corresponds to the `OPRT/SGMT` column. Note that this says nothing about
/// which market calendar applies — segment status and parentage are orthogonal
/// to trading hours.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum MicKind {
    Operating,
    Segment,
}

impl MicKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Operating => "OPRT",
            Self::Segment => "SGMT",
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
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
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
