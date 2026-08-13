use std::io::Read;

use jiff::civil::Date;
use serde::Deserialize;

use crate::issue::{Issue, IssueKind};
use crate::record::{MicKind, MicRecord, Status};
use crate::registry::MicRegistry;
use crate::{CountryCode, Lei, MarketCategory, Mic, OperatingMic};

/// Inputs the CSV cannot supply for itself.
#[derive(Clone, Debug)]
pub struct LoadOptions {
    /// Publication date of this vintage.
    ///
    /// **Not present in the file**, and not derivable from it: the ISO download
    /// URL is unversioned, so two downloads a month apart are indistinguishable
    /// on disk. Supply it from the filename or from wherever you recorded it.
    ///
    /// ISO publishes on the second Monday of each month.
    pub published: Date,
}

impl LoadOptions {
    pub fn new(published: Date) -> Self {
        Self { published }
    }
}

/// The registry, plus everything questionable found while building it.
#[derive(Debug)]
pub struct LoadOutcome {
    pub registry: MicRegistry,
    pub issues: Vec<Issue>,
}

impl LoadOutcome {
    /// Issues serious enough that a record was dropped.
    pub fn errors(&self) -> impl Iterator<Item = &Issue> + '_ {
        self.issues
            .iter()
            .filter(|i| i.severity == crate::Severity::Error)
    }

    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }
}

/// A failure that prevents any records being read at all.
///
/// Deliberately narrow. Problems with individual records are [`Issue`]s, not
/// errors — a single bad row must never fail the load.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("could not read the source: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not parse the CSV structure: {0}")]
    Csv(#[from] csv::Error),
    #[error("missing expected column: {0}")]
    MissingColumn(&'static str),
}

/// Every field optional and untyped, matched to the exact ISO header strings.
///
/// This tier does no validation whatsoever — its only job is to survive the
/// file. Typing and checking happen in [`MicRecord::from_raw`].
#[derive(Debug, Deserialize)]
struct RawMicRecord {
    #[serde(rename = "MIC")]
    mic: Option<String>,
    #[serde(rename = "OPERATING MIC")]
    operating_mic: Option<String>,
    #[serde(rename = "OPRT/SGMT")]
    kind: Option<String>,
    #[serde(rename = "MARKET NAME-INSTITUTION DESCRIPTION")]
    market_name: Option<String>,
    #[serde(rename = "LEGAL ENTITY NAME")]
    legal_entity_name: Option<String>,
    #[serde(rename = "LEI")]
    lei: Option<String>,
    #[serde(rename = "MARKET CATEGORY CODE")]
    category: Option<String>,
    #[serde(rename = "ACRONYM")]
    acronym: Option<String>,
    #[serde(rename = "ISO COUNTRY CODE (ISO 3166)")]
    country: Option<String>,
    #[serde(rename = "CITY")]
    city: Option<String>,
    #[serde(rename = "WEBSITE")]
    website: Option<String>,
    #[serde(rename = "STATUS")]
    status: Option<String>,
    #[serde(rename = "CREATION DATE")]
    created: Option<String>,
    #[serde(rename = "LAST UPDATE DATE")]
    last_updated: Option<String>,
    #[serde(rename = "LAST VALIDATION DATE")]
    last_validated: Option<String>,
    #[serde(rename = "EXPIRY DATE")]
    expires: Option<String>,
    #[serde(rename = "COMMENTS")]
    comments: Option<String>,
}

/// Trim, then treat the empty string as absent.
///
/// The source writes empties as both `""` and a bare comma, and pads some
/// fields with spaces. All of those mean "no value" and must not become
/// `Some("")`.
fn clean(s: Option<String>) -> Option<Box<str>> {
    s.map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .map(String::into_boxed_str)
}

/// Parse a bare `YYYYMMDD`. Empty is absent, not malformed.
fn parse_date(
    raw: Option<&str>,
    field: &'static str,
    line: Option<u64>,
    mic: Option<Mic>,
    issues: &mut Vec<Issue>,
) -> Option<Date> {
    let raw = raw.map(str::trim).filter(|s| !s.is_empty())?;

    let malformed = |issues: &mut Vec<Issue>| {
        issues.push(Issue::new(
            line,
            mic,
            IssueKind::MalformedDate {
                field,
                value: raw.into(),
            },
        ));
        None
    };

    if raw.len() != 8 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return malformed(issues);
    }
    let (y, m, d) = (&raw[0..4], &raw[4..6], &raw[6..8]);
    match (y.parse::<i16>(), m.parse::<i8>(), d.parse::<i8>()) {
        (Ok(y), Ok(m), Ok(d)) => match Date::new(y, m, d) {
            Ok(date) => Some(date),
            // Shape is right but the date does not exist, e.g. 20250230.
            Err(_) => malformed(issues),
        },
        _ => malformed(issues),
    }
}

impl MicRecord {
    /// Validate a raw row into a record, accumulating issues.
    ///
    /// Returns `None` only when the row cannot be keyed — an unparseable MIC.
    /// Every other problem produces an issue and a usable record.
    fn from_raw(
        raw: RawMicRecord,
        line: Option<u64>,
        issues: &mut Vec<Issue>,
        published: Date,
    ) -> Option<Self> {
        let mic_raw = clean(raw.mic);
        let mic = match mic_raw.as_deref().map(Mic::new) {
            Some(Ok(m)) => m,
            other => {
                let value: Box<str> = match other {
                    Some(Err(_)) => mic_raw.unwrap_or_default(),
                    _ => "".into(),
                };
                issues.push(Issue::new(
                    line,
                    None,
                    IssueKind::InvalidMicFormat { value },
                ));
                return None;
            }
        };

        // A segment with an unreadable parent still describes a real venue, so
        // fall back to self-parentage rather than dropping the row. The
        // referential checks in `validate` will flag the inconsistency.
        let operating_mic = clean(raw.operating_mic)
            .as_deref()
            .and_then(|s| Mic::new(s).ok())
            .map(OperatingMic::new)
            .unwrap_or_else(|| OperatingMic::new(mic));

        let kind = clean(raw.kind)
            .as_deref()
            .and_then(|s| s.parse::<MicKind>().ok())
            // Absent OPRT/SGMT: infer from whether the record is its own parent.
            .unwrap_or(if operating_mic.mic() == mic {
                MicKind::Operating
            } else {
                MicKind::Segment
            });

        let status = match clean(raw.status) {
            Some(s) => s.parse::<Status>().unwrap_or_else(|_| {
                issues.push(Issue::new(
                    line,
                    Some(mic),
                    IssueKind::UnknownStatus { value: s.clone() },
                ));
                Status::Active
            }),
            None => Status::Active,
        };

        let category = match clean(raw.category) {
            Some(c) => {
                let parsed = MarketCategory::new(&c).unwrap_or(MarketCategory::Unknown(*b"XXXX"));
                if !parsed.is_known() {
                    issues.push(Issue::new(
                        line,
                        Some(mic),
                        IssueKind::UnknownMarketCategory { value: c.clone() },
                    ));
                }
                parsed
            }
            None => MarketCategory::Nspd,
        };

        // Shape failures drop the LEI; checksum failures keep it. The entity is
        // real either way, and a bad check digit is ISO's problem to fix.
        let lei = clean(raw.lei).and_then(|l| match Lei::new(&l) {
            Ok(lei) => {
                if !lei.checksum_valid() {
                    issues.push(Issue::new(
                        line,
                        Some(mic),
                        IssueKind::InvalidLeiChecksum { value: l.clone() },
                    ));
                }
                Some(lei)
            }
            Err(_) => {
                issues.push(Issue::new(
                    line,
                    Some(mic),
                    IssueKind::InvalidLeiChecksum { value: l.clone() },
                ));
                None
            }
        });

        let country = clean(raw.country)
            .as_deref()
            .and_then(|c| CountryCode::new(c).ok());

        let created = parse_date(raw.created.as_deref(), "created", line, Some(mic), issues);
        let last_updated = parse_date(
            raw.last_updated.as_deref(),
            "last_updated",
            line,
            Some(mic),
            issues,
        );
        let last_validated = parse_date(
            raw.last_validated.as_deref(),
            "last_validated",
            line,
            Some(mic),
            issues,
        );
        let expires = parse_date(raw.expires.as_deref(), "expires", line, Some(mic), issues);

        // Pending change: published, but not in force until `effective`.
        if let Some(effective) = last_updated {
            if effective > published {
                issues.push(Issue::new(
                    line,
                    Some(mic),
                    IssueKind::FutureDatedRecord { effective },
                ));
            }
        }

        match (status, expires) {
            (Status::Expired, None) => issues.push(Issue::new(
                line,
                Some(mic),
                IssueKind::ExpiredWithoutExpiryDate,
            )),
            (Status::Active, Some(e)) if e < published => issues.push(Issue::new(
                line,
                Some(mic),
                IssueKind::ActiveWithPastExpiryDate { expires: e },
            )),
            _ => {}
        }

        Some(MicRecord {
            mic,
            operating_mic,
            kind,
            market_name: clean(raw.market_name).unwrap_or_else(|| "".into()),
            legal_entity_name: clean(raw.legal_entity_name),
            lei,
            category,
            acronym: clean(raw.acronym),
            country,
            city: clean(raw.city),
            website: clean(raw.website),
            status,
            created,
            last_updated,
            last_validated,
            expires,
            comments: clean(raw.comments),
        })
    }
}

/// Decode bytes to text, falling back to Windows-1252 when the file is not
/// valid UTF-8.
///
/// The registry contains names like `WERTPAPIERBÖRSE` and `ČESKOSLOVENSKÁ`, and
/// ISO has shipped both encodings over the years. A BOM, if present, is stripped.
fn decode(bytes: Vec<u8>, issues: &mut Vec<Issue>) -> String {
    let bytes = match bytes.strip_prefix(b"\xef\xbb\xbf") {
        Some(rest) => rest.to_vec(),
        None => bytes,
    };
    match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => {
            issues.push(Issue::new(None, None, IssueKind::EncodingFallback));
            let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(e.as_bytes());
            decoded.into_owned()
        }
    }
}

impl MicRegistry {
    /// Parse an ISO 10383 CSV.
    ///
    /// Fails only if the source cannot be read or has no usable CSV structure.
    /// Individual bad records produce [`Issue`]s and, at worst, are skipped.
    ///
    /// `opts.published` is required because the file does not carry its own
    /// publication date; see [`LoadOptions`].
    pub fn load_csv(mut reader: impl Read, opts: LoadOptions) -> Result<LoadOutcome, LoadError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;

        let mut issues = Vec::new();
        let text = decode(bytes, &mut issues);

        let mut rdr = csv::ReaderBuilder::new()
            .flexible(true)
            .from_reader(text.as_bytes());

        // Fail early and clearly if this is not the file we think it is.
        {
            let headers = rdr.headers()?;
            if !headers.iter().any(|h| h.trim() == "MIC") {
                return Err(LoadError::MissingColumn("MIC"));
            }
            if !headers.iter().any(|h| h.trim() == "OPERATING MIC") {
                return Err(LoadError::MissingColumn("OPERATING MIC"));
            }
        }

        let mut records = Vec::new();
        for result in rdr.deserialize::<RawMicRecord>() {
            // `position()` is 0-based over data rows; +1 for the header row and
            // +1 again for 1-based line numbering.
            let line = Some(records.len() as u64 + 2);
            match result {
                Ok(raw) => {
                    if let Some(rec) = MicRecord::from_raw(raw, line, &mut issues, opts.published) {
                        records.push(rec);
                    }
                }
                Err(e) => {
                    issues.push(Issue::new(
                        line,
                        None,
                        IssueKind::MalformedRow {
                            detail: e.to_string().into_boxed_str(),
                        },
                    ));
                }
            }
        }

        let registry = MicRegistry::from_records(records, opts.published, &mut issues);
        Ok(LoadOutcome { registry, issues })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn load(csv: &str) -> LoadOutcome {
        MicRegistry::load_csv(csv.as_bytes(), LoadOptions::new(date(2026, 8, 10))).unwrap()
    }

    const HEADER: &str = "\"MIC\",\"OPERATING MIC\",\"OPRT/SGMT\",\"MARKET NAME-INSTITUTION DESCRIPTION\",\"LEGAL ENTITY NAME\",\"LEI\",\"MARKET CATEGORY CODE\",\"ACRONYM\",\"ISO COUNTRY CODE (ISO 3166)\",\"CITY\",\"WEBSITE\",\"STATUS\",\"CREATION DATE\",\"LAST UPDATE DATE\",\"LAST VALIDATION DATE\",\"EXPIRY DATE\",\"COMMENTS\"";

    #[test]
    fn empty_fields_become_none_not_empty_strings() {
        // Bare commas and quoted empties, mixed, as the real file does it.
        let csv = format!(
            "{HEADER}\n\"XTST\",\"XTST\",\"OPRT\",\"TEST\",,\"\",\"RMKT\",,\"US\",\"NEW YORK\",,\"ACTIVE\",\"20200101\",\"20200101\",,,\n"
        );
        let out = load(&csv);
        let rec = out.registry.get(Mic::new("XTST").unwrap()).unwrap();
        assert_eq!(rec.legal_entity_name, None);
        assert_eq!(rec.acronym, None);
        assert_eq!(rec.website, None);
        assert_eq!(rec.comments, None);
        assert_eq!(rec.city.as_deref(), Some("NEW YORK"));
    }

    #[test]
    fn whitespace_only_is_absent() {
        let csv = format!(
            "{HEADER}\n\"XTST\",\"XTST\",\"OPRT\",\"TEST\",\"   \",,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n"
        );
        let out = load(&csv);
        let rec = out.registry.get(Mic::new("XTST").unwrap()).unwrap();
        assert_eq!(rec.legal_entity_name, None);
    }

    #[test]
    fn a_corrupt_row_does_not_fail_the_load() {
        let csv = format!(
            "{HEADER}\n\
             \"XAAA\",\"XAAA\",\"OPRT\",\"GOOD\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n\
             \"!!\",\"????\",\"WHAT\",\"BAD\",,,\"ZZZZ\",,\"..\",\"\",,\"NONSENSE\",\"not-a-date\",,,,\n\
             \"XBBB\",\"XBBB\",\"OPRT\",\"ALSO GOOD\",,,\"RMKT\",,\"GB\",\"LONDON\",,\"ACTIVE\",,,,,\n"
        );
        let out = load(&csv);
        // Both good records survive; only the unkeyable one is dropped.
        assert_eq!(out.registry.len(), 2);
        assert!(out.registry.get(Mic::new("XAAA").unwrap()).is_some());
        assert!(out.registry.get(Mic::new("XBBB").unwrap()).is_some());
        assert!(out
            .issues
            .iter()
            .any(|i| matches!(i.kind, IssueKind::InvalidMicFormat { .. })));
    }

    #[test]
    fn malformed_date_yields_issue_and_none_field() {
        let csv = format!(
            "{HEADER}\n\"XTST\",\"XTST\",\"OPRT\",\"TEST\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",\"20250230\",,,,\n"
        );
        let out = load(&csv);
        let rec = out.registry.get(Mic::new("XTST").unwrap()).unwrap();
        assert_eq!(rec.created, None);
        assert!(out.issues.iter().any(|i| matches!(
            &i.kind,
            IssueKind::MalformedDate {
                field: "created",
                ..
            }
        )));
    }

    #[test]
    fn future_dated_record_is_informational() {
        let csv = format!(
            "{HEADER}\n\"XTST\",\"XTST\",\"OPRT\",\"TEST\",,,\"RMKT\",,\"US\",\"NY\",,\"UPDATED\",,\"20260824\",,,\n"
        );
        let out = load(&csv);
        let issue = out
            .issues
            .iter()
            .find(|i| matches!(i.kind, IssueKind::FutureDatedRecord { .. }))
            .expect("expected a pending-change issue");
        assert_eq!(issue.severity, crate::Severity::Info);
        assert!(!out.has_errors());
        assert!(out.registry.is_pending(Mic::new("XTST").unwrap()));
    }

    #[test]
    fn duplicate_mic_is_an_error_and_first_wins() {
        let csv = format!(
            "{HEADER}\n\
             \"XTST\",\"XTST\",\"OPRT\",\"FIRST\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n\
             \"XTST\",\"XTST\",\"OPRT\",\"SECOND\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n"
        );
        let out = load(&csv);
        assert_eq!(out.registry.len(), 1);
        assert_eq!(
            &*out
                .registry
                .get(Mic::new("XTST").unwrap())
                .unwrap()
                .market_name,
            "FIRST"
        );
        assert!(out.has_errors());
    }

    #[test]
    fn windows_1252_falls_back_with_an_issue() {
        // 0xD6 is Ö in Windows-1252 and invalid as standalone UTF-8.
        let mut bytes = format!("{HEADER}\n\"XTST\",\"XTST\",\"OPRT\",\"WERTPAPIERB").into_bytes();
        bytes.push(0xD6);
        bytes.extend_from_slice(b"RSE\",,,\"RMKT\",,\"DE\",\"FRANKFURT\",,\"ACTIVE\",,,,,\n");

        let out = MicRegistry::load_csv(&bytes[..], LoadOptions::new(date(2026, 8, 10))).unwrap();
        assert!(out
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::EncodingFallback));
        let rec = out.registry.get(Mic::new("XTST").unwrap()).unwrap();
        assert_eq!(&*rec.market_name, "WERTPAPIERBÖRSE");
    }

    #[test]
    fn utf8_bom_is_stripped() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(
            format!(
                "{HEADER}\n\"XTST\",\"XTST\",\"OPRT\",\"TEST\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n"
            )
            .as_bytes(),
        );
        let out = MicRegistry::load_csv(&bytes[..], LoadOptions::new(date(2026, 8, 10))).unwrap();
        assert!(out.registry.get(Mic::new("XTST").unwrap()).is_some());
        assert!(!out
            .issues
            .iter()
            .any(|i| i.kind == IssueKind::EncodingFallback));
    }

    #[test]
    fn wrong_file_is_rejected_outright() {
        let err = MicRegistry::load_csv(
            "name,value\nfoo,1\n".as_bytes(),
            LoadOptions::new(date(2026, 8, 10)),
        )
        .unwrap_err();
        assert!(matches!(err, LoadError::MissingColumn("MIC")));
    }

    #[test]
    fn unknown_category_is_preserved_and_warned() {
        let csv = format!(
            "{HEADER}\n\"XTST\",\"XTST\",\"OPRT\",\"TEST\",,,\"WXYZ\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n"
        );
        let out = load(&csv);
        let rec = out.registry.get(Mic::new("XTST").unwrap()).unwrap();
        assert_eq!(rec.category.as_str(), "WXYZ");
        assert!(!out.has_errors());
        assert!(out
            .issues
            .iter()
            .any(|i| matches!(i.kind, IssueKind::UnknownMarketCategory { .. })));
    }
}
