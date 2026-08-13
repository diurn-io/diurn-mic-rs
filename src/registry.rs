use std::collections::HashMap;

use jiff::civil::Date;

use crate::issue::{Issue, IssueKind};
use crate::published::PublishedSource;
use crate::record::{MicKind, MicRecord, Status};
use crate::{CountryCode, Mic, OperatingMic};

/// An immutable, in-memory index over one vintage of the ISO 10383 registry.
///
/// Around 2,900 records parse in a few milliseconds, so there is no database
/// here and no need for one. Build it once, wrap it in an `Arc`, and share it.
///
/// Indices hold `u32` offsets into `records` rather than references or cloned
/// keys — compact, cache-friendly, and it keeps the struct movable.
#[derive(Debug)]
pub struct MicRegistry {
    records: Vec<MicRecord>,
    by_mic: HashMap<Mic, u32>,
    segments_by_operating: HashMap<OperatingMic, Vec<u32>>,
    by_country: HashMap<CountryCode, Vec<u32>>,
    published: Date,
    published_source: PublishedSource,
}

impl MicRegistry {
    /// Build the indices and run the referential checks.
    ///
    /// Duplicate MICs are the one condition that drops a record here: two rows
    /// claiming one key leaves the index ambiguous, so the first wins.
    pub(crate) fn from_records(
        records: Vec<MicRecord>,
        published: Date,
        published_source: PublishedSource,
        issues: &mut Vec<Issue>,
    ) -> Self {
        let mut by_mic: HashMap<Mic, u32> = HashMap::with_capacity(records.len());
        let mut kept: Vec<MicRecord> = Vec::with_capacity(records.len());

        for rec in records {
            if let Some(&first) = by_mic.get(&rec.mic) {
                issues.push(Issue::new(
                    None,
                    Some(rec.mic),
                    IssueKind::DuplicateMic {
                        first_line: Some(u64::from(first) + 2),
                    },
                ));
                continue;
            }
            by_mic.insert(rec.mic, kept.len() as u32);
            kept.push(rec);
        }

        let mut segments_by_operating: HashMap<OperatingMic, Vec<u32>> = HashMap::new();
        let mut by_country: HashMap<CountryCode, Vec<u32>> = HashMap::new();

        for (i, rec) in kept.iter().enumerate() {
            let i = i as u32;
            if rec.kind == MicKind::Segment {
                segments_by_operating
                    .entry(rec.operating_mic)
                    .or_default()
                    .push(i);
            }
            if let Some(cc) = rec.country {
                by_country.entry(cc).or_default().push(i);
            }
        }

        // Referential integrity. ISO does not guarantee it, so check rather
        // than assume — but none of these justify dropping a record.
        for rec in &kept {
            let parent = rec.operating_mic.mic();
            match by_mic.get(&parent) {
                None => issues.push(Issue::new(
                    None,
                    Some(rec.mic),
                    IssueKind::DanglingOperatingMic {
                        operating_mic: parent,
                    },
                )),
                Some(&pi) => {
                    let parent_rec = &kept[pi as usize];
                    if rec.kind == MicKind::Segment
                        && parent_rec.kind == MicKind::Segment
                        && parent != rec.mic
                    {
                        issues.push(Issue::new(
                            None,
                            Some(rec.mic),
                            IssueKind::SegmentPointsToSegment {
                                operating_mic: parent,
                            },
                        ));
                    }
                }
            }
            if rec.kind == MicKind::Operating && parent != rec.mic {
                issues.push(Issue::new(
                    None,
                    Some(rec.mic),
                    IssueKind::OperatingMicSelfMismatch {
                        operating_mic: parent,
                    },
                ));
            }
        }

        // Checks that need the publication date. These run here rather than
        // per-row because the date may have been derived from the rows
        // themselves and so was not known while they were being parsed.
        for rec in &kept {
            if let Some(effective) = rec.last_updated {
                if effective > published {
                    issues.push(Issue::new(
                        None,
                        Some(rec.mic),
                        IssueKind::FutureDatedRecord { effective },
                    ));
                }
            }

            match (rec.status, rec.expires) {
                (Status::Expired, None) => issues.push(Issue::new(
                    None,
                    Some(rec.mic),
                    IssueKind::ExpiredWithoutExpiryDate,
                )),
                (Status::Active, Some(e)) if e < published => issues.push(Issue::new(
                    None,
                    Some(rec.mic),
                    IssueKind::ActiveWithPastExpiryDate { expires: e },
                )),
                _ => {}
            }
        }

        Self {
            records: kept,
            by_mic,
            segments_by_operating,
            by_country,
            published,
            published_source,
        }
    }

    pub fn get(&self, mic: Mic) -> Option<&MicRecord> {
        self.by_mic.get(&mic).map(|&i| &self.records[i as usize])
    }

    /// The segments belonging to an operating MIC.
    ///
    /// Returns an iterator rather than a slice: the index stores positions, so
    /// there is no contiguous run of records to borrow.
    pub fn segments_of(&self, mic: OperatingMic) -> impl Iterator<Item = &MicRecord> + '_ {
        self.segments_by_operating
            .get(&mic)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(move |&i| &self.records[i as usize])
    }

    /// The operating record a MIC belongs to.
    ///
    /// For an operating MIC this returns the record itself.
    pub fn operating_of(&self, mic: Mic) -> Option<&MicRecord> {
        let rec = self.get(mic)?;
        self.get(rec.operating_mic.mic())
    }

    pub fn by_country(&self, cc: CountryCode) -> impl Iterator<Item = &MicRecord> + '_ {
        self.by_country
            .get(&cc)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(move |&i| &self.records[i as usize])
    }

    pub fn iter(&self) -> impl Iterator<Item = &MicRecord> + '_ {
        self.records.iter()
    }

    /// Publication date of this vintage.
    ///
    /// Either supplied at load time or derived from the file; check
    /// [`MicRegistry::published_source`] to know which.
    pub fn published(&self) -> Date {
        self.published
    }

    /// How [`MicRegistry::published`] was arrived at.
    ///
    /// A caller that inferred the date and has a clock available should
    /// sanity-check the result — inference is clock-free and cannot tell a
    /// current effective date from a stale one (see
    /// [`crate::publication_date_from_effective`]).
    pub fn published_source(&self) -> PublishedSource {
        self.published_source
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Whether this record carries a change that has been published but has not
    /// taken effect yet.
    ///
    /// ISO publishes on the second Monday and its changes take effect on the
    /// fourth, so a fresh file always contains some of these. In the 2026-08-10
    /// vintage there are 35.
    pub fn is_pending(&self, mic: Mic) -> bool {
        self.get(mic)
            .is_some_and(|r| !r.is_in_force_on(self.published))
    }

    /// The registry as it stands on `date`, excluding changes not yet in force.
    ///
    /// Pass a date before the effective date to see the pre-change state, or
    /// after it to include pending records.
    pub fn as_of(&self, date: Date) -> impl Iterator<Item = &MicRecord> + '_ {
        self.records.iter().filter(move |r| r.is_in_force_on(date))
    }

    /// Every record whose changes are not yet in force as of the publication date.
    pub fn pending(&self) -> impl Iterator<Item = &MicRecord> + '_ {
        let published = self.published;
        self.records
            .iter()
            .filter(move |r| !r.is_in_force_on(published))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LoadOptions, MicRegistry};
    use jiff::civil::date;

    const HEADER: &str = "\"MIC\",\"OPERATING MIC\",\"OPRT/SGMT\",\"MARKET NAME-INSTITUTION DESCRIPTION\",\"LEGAL ENTITY NAME\",\"LEI\",\"MARKET CATEGORY CODE\",\"ACRONYM\",\"ISO COUNTRY CODE (ISO 3166)\",\"CITY\",\"WEBSITE\",\"STATUS\",\"CREATION DATE\",\"LAST UPDATE DATE\",\"LAST VALIDATION DATE\",\"EXPIRY DATE\",\"COMMENTS\"";

    fn load(body: &str) -> crate::LoadOutcome {
        MicRegistry::load_csv(
            format!("{HEADER}\n{body}").as_bytes(),
            LoadOptions::new(date(2026, 8, 10)),
        )
        .unwrap()
    }

    #[test]
    fn segments_resolve_under_their_parent() {
        let out = load(
            "\"XSWX\",\"XSWX\",\"OPRT\",\"SIX\",,,\"RMKT\",,\"CH\",\"ZURICH\",,\"ACTIVE\",,,,,\n\
             \"XSDX\",\"XSWX\",\"SGMT\",\"SIX DIGITAL\",,,\"RMKT\",,\"CH\",\"ZURICH\",,\"EXPIRED\",,,,\"20250531\",\n",
        );
        let xswx = OperatingMic::new(Mic::new("XSWX").unwrap());
        let segs: Vec<_> = out.registry.segments_of(xswx).collect();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].mic.as_str(), "XSDX");
        // Parent status must not propagate to the segment.
        assert_eq!(segs[0].status, crate::Status::Expired);
        assert_eq!(
            out.registry.get(Mic::new("XSWX").unwrap()).unwrap().status,
            crate::Status::Active
        );
    }

    #[test]
    fn operating_of_returns_self_for_an_operating_mic() {
        let out = load(
            "\"XSWX\",\"XSWX\",\"OPRT\",\"SIX\",,,\"RMKT\",,\"CH\",\"ZURICH\",,\"ACTIVE\",,,,,\n",
        );
        let r = out
            .registry
            .operating_of(Mic::new("XSWX").unwrap())
            .unwrap();
        assert_eq!(r.mic.as_str(), "XSWX");
    }

    #[test]
    fn as_of_excludes_pending_changes() {
        let out = load(
            "\"XAAA\",\"XAAA\",\"OPRT\",\"SETTLED\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,\"20200101\",,,\n\
             \"XBBB\",\"XBBB\",\"OPRT\",\"PENDING\",,,\"RMKT\",,\"US\",\"NY\",,\"UPDATED\",,\"20260824\",,,\n",
        );
        let before: Vec<_> = out.registry.as_of(date(2026, 8, 23)).collect();
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].mic.as_str(), "XAAA");

        let on_effective: Vec<_> = out.registry.as_of(date(2026, 8, 24)).collect();
        assert_eq!(on_effective.len(), 2);

        assert!(out.registry.is_pending(Mic::new("XBBB").unwrap()));
        assert!(!out.registry.is_pending(Mic::new("XAAA").unwrap()));
        assert_eq!(out.registry.pending().count(), 1);
    }

    #[test]
    fn dangling_parent_is_reported_not_fatal() {
        let out = load(
            "\"XAAA\",\"ZZZZ\",\"SGMT\",\"ORPHAN\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n",
        );
        assert_eq!(out.registry.len(), 1);
        assert!(!out.has_errors());
        assert!(out
            .issues
            .iter()
            .any(|i| matches!(i.kind, IssueKind::DanglingOperatingMic { .. })));
    }

    #[test]
    fn country_index_finds_records() {
        let out = load(
            "\"XAAA\",\"XAAA\",\"OPRT\",\"A\",,,\"RMKT\",,\"JP\",\"TOKYO\",,\"ACTIVE\",,,,,\n\
             \"XBBB\",\"XBBB\",\"OPRT\",\"B\",,,\"RMKT\",,\"JP\",\"OSAKA\",,\"ACTIVE\",,,,,\n\
             \"XCCC\",\"XCCC\",\"OPRT\",\"C\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n",
        );
        let jp = CountryCode::new("JP").unwrap();
        assert_eq!(out.registry.by_country(jp).count(), 2);
        let none = CountryCode::new("ZW").unwrap();
        assert_eq!(out.registry.by_country(none).count(), 0);
    }

    #[test]
    fn unknown_lookups_return_none() {
        let out =
            load("\"XAAA\",\"XAAA\",\"OPRT\",\"A\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n");
        assert!(out.registry.get(Mic::new("ZZZZ").unwrap()).is_none());
        assert!(!out.registry.is_pending(Mic::new("ZZZZ").unwrap()));
    }
}
