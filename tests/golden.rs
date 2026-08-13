//! Golden-file tests against the pinned ISO 10383 vintage.
//!
//! Every expected count here was derived independently from the CSV rather than
//! from this crate's own output, so these assert against the data rather than
//! against ourselves.
//!
//! The fixture is excluded from the published package (see `Cargo.toml`), so
//! these tests only run from a checkout.

use std::fs::File;

use diurn_mic::{
    diff, validate, CountryCode, IssueKind, LoadOptions, MarketCategory, Mic, MicRegistry,
    OperatingMic, Severity, Status,
};
use jiff::civil::{date, Date};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/ISO10383_MIC_2026-08-10.csv"
);

/// The publication date of the pinned vintage: the second Monday of August 2026.
const PUBLISHED: Date = Date::constant(2026, 8, 10);
/// The fourth Monday, when this file's changes take effect.
const EFFECTIVE: Date = Date::constant(2026, 8, 24);

fn load() -> diurn_mic::LoadOutcome {
    let f = File::open(FIXTURE).expect("pinned fixture is missing");
    MicRegistry::load_csv(f, LoadOptions::new(PUBLISHED)).expect("the pinned vintage must load")
}

#[test]
fn loads_with_no_errors() {
    let out = load();
    let errors: Vec<_> = out.errors().collect();
    assert!(
        errors.is_empty(),
        "expected no Severity::Error issues, got {}: {:?}",
        errors.len(),
        &errors[..errors.len().min(5)]
    );
    assert_eq!(out.registry.len(), 2875);
}

/// Eight segments point at another segment in this vintage. Treating that as an
/// error would fail the load on good data, so it must be a warning.
#[test]
fn segment_to_segment_references_are_warnings() {
    let out = load();
    let cases: Vec<_> = out
        .issues
        .iter()
        .filter(|i| matches!(i.kind, IssueKind::SegmentPointsToSegment { .. }))
        .collect();

    assert_eq!(cases.len(), 8, "expected 8 segment->segment references");
    assert!(cases.iter().all(|i| i.severity == Severity::Warning));

    let mut mics: Vec<_> = cases.iter().filter_map(|i| i.mic).collect();
    mics.sort();
    let names: Vec<_> = mics.iter().map(|m| m.as_str()).collect();
    assert_eq!(
        names,
        ["EXPA", "ICAT", "LIQH", "NBXO", "PCDS", "THRD", "VRXP", "XEAS"]
    );
}

/// ISO publishes on the second Monday; those changes take effect on the fourth.
/// Every forward-dated record in this file carries exactly the effective date.
#[test]
fn pending_records_are_flagged_and_informational() {
    let out = load();

    let pending: Vec<_> = out
        .issues
        .iter()
        .filter(|i| matches!(i.kind, IssueKind::FutureDatedRecord { .. }))
        .collect();

    assert_eq!(pending.len(), 35, "expected 35 pending records");
    assert!(pending.iter().all(|i| i.severity == Severity::Info));

    for issue in &pending {
        match issue.kind {
            IssueKind::FutureDatedRecord { effective } => assert_eq!(
                effective, EFFECTIVE,
                "all pending changes take effect on the fourth Monday"
            ),
            _ => unreachable!(),
        }
    }

    assert_eq!(out.registry.pending().count(), 35);
    for mic in ["IUOB", "BTAM", "BTQE", "EUCC"] {
        assert!(
            out.registry.is_pending(Mic::new(mic).unwrap()),
            "{mic} should be pending"
        );
    }
}

/// `UPDATED` is a pending marker, not a third steady state: every one of them
/// is forward-dated, though not every forward-dated record is `UPDATED`.
#[test]
fn updated_status_implies_pending() {
    let out = load();

    let updated: Vec<_> = out
        .registry
        .iter()
        .filter(|r| r.status == Status::Updated)
        .collect();
    assert_eq!(updated.len(), 23);
    assert!(updated
        .iter()
        .all(|r| !r.is_in_force_on(PUBLISHED) && r.last_updated == Some(EFFECTIVE)));

    // The other 12 forward-dated records are ACTIVE or EXPIRED.
    let by_status = |s: Status| out.registry.pending().filter(|r| r.status == s).count();
    assert_eq!(by_status(Status::Updated), 23);
    assert_eq!(by_status(Status::Active), 11);
    assert_eq!(by_status(Status::Expired), 1);
}

#[test]
fn as_of_moves_the_registry_across_the_effective_date() {
    let out = load();
    let before = out.registry.as_of(date(2026, 8, 23)).count();
    let on = out.registry.as_of(EFFECTIVE).count();
    assert_eq!(before, 2875 - 35);
    assert_eq!(on, 2875);
}

/// A stable code under an expired segment with an active parent: status must
/// not propagate downward.
#[test]
fn expired_segment_under_active_parent() {
    let out = load();
    let xsdx = out.registry.get(Mic::new("XSDX").unwrap()).unwrap();
    let xswx = out.registry.get(Mic::new("XSWX").unwrap()).unwrap();

    assert_eq!(xsdx.status, Status::Expired);
    assert_eq!(xswx.status, Status::Active);
    assert_eq!(xsdx.operating_mic.mic(), xswx.mic);
    assert_eq!(xsdx.expires, Some(date(2025, 5, 31)));

    let segs: Vec<_> = out
        .registry
        .segments_of(OperatingMic::new(xswx.mic))
        .map(|r| r.mic.as_str())
        .collect();
    assert!(segs.contains(&"XSDX"));
}

/// The three codes that postdate most published references.
#[test]
fn recent_market_categories_parse_as_known() {
    let out = load();
    let count = |c: MarketCategory| out.registry.iter().filter(|r| r.category == c).count();

    assert_eq!(count(MarketCategory::Casp), 29);
    assert_eq!(count(MarketCategory::Trfs), 5);
    assert_eq!(count(MarketCategory::Idqs), 1);

    // Nothing in this vintage should be unrecognised.
    let unknown: Vec<_> = out
        .registry
        .iter()
        .filter(|r| !r.category.is_known())
        .map(|r| (r.mic.as_str(), r.category.as_str()))
        .collect();
    assert!(unknown.is_empty(), "unrecognised categories: {unknown:?}");

    assert_eq!(
        out.registry
            .iter()
            .map(|r| r.category)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        14
    );
}

#[test]
fn every_lei_passes_its_checksum() {
    let out = load();
    let leis: Vec<_> = out.registry.iter().filter_map(|r| r.lei).collect();
    assert_eq!(leis.len(), 2246);
    let bad: Vec<_> = leis.iter().filter(|l| !l.checksum_valid()).collect();
    assert!(bad.is_empty(), "LEIs failing MOD 97-10: {bad:?}");

    // ...and the loader agrees it saw nothing wrong.
    assert!(!out
        .issues
        .iter()
        .any(|i| matches!(i.kind, IssueKind::InvalidLeiChecksum { .. })));
}

/// Referential integrity happens to hold in this vintage. It is not guaranteed,
/// which is why the loader checks rather than assumes — but a regression here
/// means we have started inventing problems.
#[test]
fn referential_integrity_is_clean() {
    let out = load();
    let count = |f: fn(&IssueKind) -> bool| out.issues.iter().filter(|i| f(&i.kind)).count();

    assert_eq!(
        count(|k| matches!(k, IssueKind::DanglingOperatingMic { .. })),
        0
    );
    assert_eq!(
        count(|k| matches!(k, IssueKind::OperatingMicSelfMismatch { .. })),
        0
    );
    assert_eq!(count(|k| matches!(k, IssueKind::DuplicateMic { .. })), 0);
    assert_eq!(
        count(|k| matches!(k, IssueKind::ExpiredWithoutExpiryDate)),
        0
    );
    assert_eq!(
        count(|k| matches!(k, IssueKind::ActiveWithPastExpiryDate { .. })),
        0
    );
    assert_eq!(count(|k| matches!(k, IssueKind::MalformedDate { .. })), 0);
    assert_eq!(count(|k| matches!(k, IssueKind::EncodingFallback)), 0);
}

/// Non-ASCII survives the round trip. The file is UTF-8 with no BOM.
#[test]
fn non_ascii_names_decode_correctly() {
    let out = load();
    let name = |mic: &str| {
        out.registry
            .get(Mic::new(mic).unwrap())
            .map(|r| r.market_name.to_string())
            .unwrap_or_default()
    };
    let legal = |mic: &str| {
        out.registry
            .get(Mic::new(mic).unwrap())
            .and_then(|r| r.legal_entity_name.as_ref().map(|s| s.to_string()))
            .unwrap_or_default()
    };

    assert!(legal("FRAV").contains("WERTPAPIERBÖRSE"));
    assert!(name("CSOB").contains("ČESKOSLOVENSKÁ"));
    assert!(legal("GROW").contains("ESPAÑOLES"));
    // U+2019, not an ASCII apostrophe.
    let ftus = out.registry.get(Mic::new("FTUS").unwrap()).unwrap();
    assert!(ftus.comments.as_deref().unwrap().contains('\u{2019}'));
}

/// Doubled quotes inside a quoted field are the csv crate's job, not ours —
/// this asserts we did not hand-roll splitting somewhere.
#[test]
fn embedded_quotes_survive() {
    let out = load();
    let xcan = out.registry.get(Mic::new("XCAN").unwrap()).unwrap();
    assert!(xcan.comments.as_deref().unwrap().contains("\"CAN-ATS\""));
}

#[test]
fn empty_fields_are_none_not_empty_strings() {
    let out = load();
    // 1,495 records have no legal entity name in this vintage.
    let missing = out
        .registry
        .iter()
        .filter(|r| r.legal_entity_name.is_none())
        .count();
    assert_eq!(missing, 1495);

    // Nothing should ever be Some("").
    assert!(out.registry.iter().all(|r| {
        [
            r.legal_entity_name.as_deref(),
            r.acronym.as_deref(),
            r.city.as_deref(),
            r.website.as_deref(),
            r.comments.as_deref(),
        ]
        .iter()
        .all(|f| f.is_none_or(|s| !s.is_empty()))
    }));
}

/// Calendar membership does not follow the ISO hierarchy. Both of these are the
/// evidence for that, and both are load-bearing for `diurn-cal`.
#[test]
fn hierarchy_does_not_imply_calendar() {
    let out = load();
    let bats = out.registry.get(Mic::new("BATS").unwrap()).unwrap();
    let xtks = out.registry.get(Mic::new("XTKS").unwrap()).unwrap();

    // A Cboe segment that nonetheless keeps the ordinary US equity calendar.
    assert_eq!(bats.operating_mic.as_str(), "XCBO");
    // The venue JP-EQUITY describes is a segment, not an operating MIC.
    assert_eq!(xtks.kind, diurn_mic::MicKind::Segment);
    assert_eq!(xtks.operating_mic.as_str(), "XJPX");
}

#[test]
fn country_index_matches_the_file() {
    let out = load();
    let us = out
        .registry
        .by_country(CountryCode::new("US").unwrap())
        .count();
    let jp = out
        .registry
        .by_country(CountryCode::new("JP").unwrap())
        .count();
    assert!(us > 100, "expected many US venues, got {us}");
    assert!(jp > 10, "expected several JP venues, got {jp}");

    // ZZ is a real value for supranational venues, not a placeholder.
    let zz = out
        .registry
        .by_country(CountryCode::new("ZZ").unwrap())
        .count();
    assert_eq!(zz, 3);
}

/// `validate` re-derives what the loader already reported.
#[test]
fn validate_agrees_with_the_loader() {
    let out = load();
    let revalidated = validate(&out.registry);

    let structural = |i: &&diurn_mic::Issue| {
        matches!(
            i.kind,
            IssueKind::DanglingOperatingMic { .. }
                | IssueKind::SegmentPointsToSegment { .. }
                | IssueKind::OperatingMicSelfMismatch { .. }
                | IssueKind::ExpiredWithoutExpiryDate
                | IssueKind::ActiveWithPastExpiryDate { .. }
                | IssueKind::FutureDatedRecord { .. }
        )
    };

    assert_eq!(
        out.issues.iter().filter(structural).count(),
        revalidated.iter().filter(structural).count()
    );
    assert!(revalidated.iter().all(|i| i.severity != Severity::Error));
}

/// Diffing a vintage against itself must be empty — the cheapest guard against
/// a comparison that is accidentally always true.
#[test]
fn self_diff_is_empty() {
    let a = load().registry;
    let b = load().registry;
    assert!(diff(&a, &b).is_empty());
}

#[test]
fn published_date_round_trips() {
    assert_eq!(load().registry.published(), PUBLISHED);
}
