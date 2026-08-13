use std::fmt;

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use crate::Mic;

/// How seriously to take an [`Issue`].
///
/// Only [`Severity::Error`] causes a record to be dropped, and that is reserved
/// for records that cannot be keyed at all. Everything else is retained: a
/// record with a dubious LEI or an unrecognised category is still a record.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Severity {
    /// Worth surfacing, but expected in a well-formed file. The normal
    /// publication cycle produces these.
    Info,
    /// A genuine defect in the source data. The record is kept.
    Warning,
    /// The record cannot be used and was skipped.
    Error,
}

/// What was wrong.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub enum IssueKind {
    /// The MIC is not four alphanumeric characters. The row cannot be keyed.
    InvalidMicFormat { value: Box<str> },
    /// The row could not be read as CSV at all — an unbalanced quote, say.
    /// Distinct from [`IssueKind::InvalidMicFormat`]: nothing was extracted, so
    /// there is no field to point at.
    MalformedRow { detail: Box<str> },
    /// Two rows claim the same MIC. The later one is dropped.
    DuplicateMic { first_line: Option<u64> },
    /// ISO 7064 MOD 97-10 check digits do not agree. The value is kept.
    InvalidLeiChecksum { value: Box<str> },
    /// A market category code this version of the crate does not know.
    /// Expected — ISO revises the list.
    UnknownMarketCategory { value: Box<str> },
    /// A status outside `ACTIVE` / `UPDATED` / `EXPIRED`.
    UnknownStatus { value: Box<str> },
    /// An `OPRT/SGMT` value that is neither. The kind is inferred from whether
    /// the record is its own parent.
    UnknownMicKind { value: Box<str> },
    /// A date field that is neither empty nor a valid `YYYYMMDD`.
    MalformedDate {
        field: &'static str,
        value: Box<str>,
    },
    /// The file was not valid UTF-8 and was decoded as Windows-1252.
    /// Emitted once per load, not per row.
    EncodingFallback,
    /// The record's operating MIC is not present in the file.
    DanglingOperatingMic { operating_mic: Mic },
    /// A segment whose parent is itself a segment. Present in real vintages —
    /// eight cases in 2026-08-10 — so this is a warning, never an error.
    SegmentPointsToSegment { operating_mic: Mic },
    /// An operating MIC whose `OPERATING MIC` column points somewhere else.
    OperatingMicSelfMismatch { operating_mic: Mic },
    /// Status is `EXPIRED` but no expiry date is given.
    ExpiredWithoutExpiryDate,
    /// Status is `ACTIVE` but the expiry date has already passed.
    ActiveWithPastExpiryDate { expires: Date },
    /// `last_updated` is after the file's publication date: a change that is
    /// published but not yet in force. This is the normal ISO publication
    /// cycle, not a defect.
    FutureDatedRecord { effective: Date },
}

impl IssueKind {
    /// The severity this kind always carries.
    ///
    /// Fixed per kind rather than decided at each call site, so that "the load
    /// produced no errors" means the same thing everywhere.
    pub fn severity(&self) -> Severity {
        match self {
            // Cannot be keyed — the only cases that justify dropping a row.
            Self::InvalidMicFormat { .. }
            | Self::MalformedRow { .. }
            | Self::DuplicateMic { .. } => Severity::Error,

            // Normal publication cycle.
            Self::FutureDatedRecord { .. } => Severity::Info,

            // Real defects, but the record remains usable.
            Self::InvalidLeiChecksum { .. }
            | Self::UnknownMarketCategory { .. }
            | Self::UnknownStatus { .. }
            | Self::UnknownMicKind { .. }
            | Self::MalformedDate { .. }
            | Self::EncodingFallback
            | Self::DanglingOperatingMic { .. }
            | Self::SegmentPointsToSegment { .. }
            | Self::OperatingMicSelfMismatch { .. }
            | Self::ExpiredWithoutExpiryDate
            | Self::ActiveWithPastExpiryDate { .. } => Severity::Warning,
        }
    }
}

impl fmt::Display for IssueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMicFormat { value } => {
                write!(f, "not a valid MIC: {value:?}")
            }
            Self::MalformedRow { detail } => {
                write!(f, "row is not readable as CSV: {detail}")
            }
            Self::DuplicateMic { first_line } => match first_line {
                Some(l) => write!(f, "duplicate MIC, first seen on line {l}"),
                None => write!(f, "duplicate MIC"),
            },
            Self::InvalidLeiChecksum { value } => {
                write!(f, "LEI check digits do not agree: {value}")
            }
            Self::UnknownMarketCategory { value } => {
                write!(f, "unrecognised market category: {value}")
            }
            Self::UnknownStatus { value } => write!(f, "unrecognised status: {value:?}"),
            Self::UnknownMicKind { value } => {
                write!(f, "unrecognised MIC type: {value:?}")
            }
            Self::MalformedDate { field, value } => {
                write!(f, "malformed date in {field}: {value:?}")
            }
            Self::EncodingFallback => {
                write!(f, "file is not valid UTF-8; decoded as Windows-1252")
            }
            Self::DanglingOperatingMic { operating_mic } => {
                write!(f, "operating MIC {operating_mic} is not in this file")
            }
            Self::SegmentPointsToSegment { operating_mic } => {
                write!(f, "parent {operating_mic} is itself a segment")
            }
            Self::OperatingMicSelfMismatch { operating_mic } => {
                write!(f, "operating record points at {operating_mic}")
            }
            Self::ExpiredWithoutExpiryDate => write!(f, "expired but no expiry date"),
            Self::ActiveWithPastExpiryDate { expires } => {
                write!(f, "active but expired on {expires}")
            }
            Self::FutureDatedRecord { effective } => {
                write!(f, "pending change, effective {effective}")
            }
        }
    }
}

/// Something the loader or validator noticed, with enough context to act on it.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct Issue {
    /// 1-based line in the source file, where known.
    pub line: Option<u64>,
    /// The MIC the issue concerns, where it could be parsed.
    pub mic: Option<Mic>,
    pub severity: Severity,
    pub kind: IssueKind,
}

impl Issue {
    pub fn new(line: Option<u64>, mic: Option<Mic>, kind: IssueKind) -> Self {
        Self {
            line,
            mic,
            severity: kind.severity(),
            kind,
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.mic) {
            (Some(l), Some(m)) => write!(f, "line {l} ({m}): {}", self.kind),
            (Some(l), None) => write!(f, "line {l}: {}", self.kind),
            (None, Some(m)) => write!(f, "{m}: {}", self.kind),
            (None, None) => write!(f, "{}", self.kind),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_unkeyable_records_are_errors() {
        assert_eq!(
            IssueKind::InvalidMicFormat { value: "XX".into() }.severity(),
            Severity::Error
        );
        assert_eq!(
            IssueKind::DuplicateMic {
                first_line: Some(4)
            }
            .severity(),
            Severity::Error
        );
    }

    /// Eight real cases exist in the pinned vintage; treating this as an error
    /// would fail the load on good data.
    #[test]
    fn segment_pointing_to_segment_is_a_warning() {
        let kind = IssueKind::SegmentPointsToSegment {
            operating_mic: Mic::new("XEQT").unwrap(),
        };
        assert_eq!(kind.severity(), Severity::Warning);
    }

    #[test]
    fn pending_records_are_informational() {
        let kind = IssueKind::FutureDatedRecord {
            effective: jiff::civil::date(2026, 8, 24),
        };
        assert_eq!(kind.severity(), Severity::Info);
    }

    #[test]
    fn severity_orders_by_seriousness() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
    }
}
