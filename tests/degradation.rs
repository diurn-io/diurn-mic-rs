//! The loader must degrade, never fail.
//!
//! ISO will eventually ship a malformed row, and a trading system that stops
//! parsing the whole registry because of one bad record is worse than one that
//! carries on and says what it skipped. These tests use deliberately damaged
//! fixtures to prove that holds.

use std::fs::File;

use diurn_mic::{IssueKind, LoadOptions, MicRegistry, Severity, Status};
use jiff::civil::{date, Date};

const PUBLISHED: Date = Date::constant(2026, 8, 10);

fn fixture(name: &str) -> File {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    File::open(&path).unwrap_or_else(|e| panic!("missing fixture {path}: {e}"))
}

fn load(name: &str) -> diurn_mic::LoadOutcome {
    MicRegistry::load_csv(fixture(name), LoadOptions::new(PUBLISHED))
        .expect("a damaged file must still load")
}

fn mic(s: &str) -> diurn_mic::Mic {
    diurn_mic::Mic::new(s).unwrap()
}

#[test]
fn damaged_file_still_yields_every_usable_record() {
    let out = load("corrupted.csv");

    // Six distinct keyable MICs; the unkeyable row and the duplicate are gone.
    let mut got: Vec<_> = out.registry.iter().map(|r| r.mic.as_str()).collect();
    got.sort_unstable();
    assert_eq!(
        got,
        ["XAAA", "XBBB", "XCCC", "XDDD", "XEEE", "XFFF", "XGGG"]
    );
}

/// The record before the damage and the record after it must both survive —
/// this is what "does not abort" actually means.
#[test]
fn parsing_continues_past_a_bad_row() {
    let out = load("corrupted.csv");
    assert_eq!(
        &*out.registry.get(mic("XAAA")).unwrap().market_name,
        "FIRST GOOD VENUE"
    );
    assert_eq!(
        &*out.registry.get(mic("XGGG")).unwrap().market_name,
        "LAST GOOD VENUE"
    );
}

#[test]
fn only_unkeyable_rows_are_dropped() {
    let out = load("corrupted.csv");
    let errors: Vec<_> = out.errors().collect();

    // Exactly two: the unparseable MIC and the duplicate key.
    assert_eq!(errors.len(), 2, "unexpected errors: {errors:?}");
    assert!(errors
        .iter()
        .any(|i| matches!(i.kind, IssueKind::InvalidMicFormat { .. })));
    assert!(errors
        .iter()
        .any(|i| matches!(i.kind, IssueKind::DuplicateMic { .. })));
}

#[test]
fn duplicate_key_keeps_the_first_occurrence() {
    let out = load("corrupted.csv");
    assert_eq!(
        &*out.registry.get(mic("XAAA")).unwrap().market_name,
        "FIRST GOOD VENUE"
    );
}

/// One row carries four independent problems. None of them should lose the
/// record, and each should be reported separately.
#[test]
fn a_record_with_many_problems_survives_them_all() {
    let out = load("corrupted.csv");
    let rec = out.registry.get(mic("XBBB")).expect("XBBB must survive");

    // Impossible date -> None, plus a MalformedDate issue.
    assert_eq!(rec.created, None);
    // Unrecognised category is preserved verbatim.
    assert_eq!(rec.category.as_str(), "WXYZ");
    assert!(!rec.category.is_known());
    // Unknown status falls back rather than dropping the row.
    assert_eq!(rec.status, Status::Active);
    // LEI has valid shape but bad check digits: kept, and flagged.
    assert_eq!(rec.lei.unwrap().as_str(), "00000000000000000000");
    assert!(!rec.lei.unwrap().checksum_valid());

    let kinds: Vec<_> = out
        .issues
        .iter()
        .filter(|i| i.mic == Some(mic("XBBB")))
        .map(|i| &i.kind)
        .collect();
    assert!(kinds.iter().any(|k| matches!(
        k,
        IssueKind::MalformedDate {
            field: "created",
            ..
        }
    )));
    assert!(kinds
        .iter()
        .any(|k| matches!(k, IssueKind::UnknownMarketCategory { .. })));
    assert!(kinds
        .iter()
        .any(|k| matches!(k, IssueKind::UnknownStatus { .. })));
    assert!(kinds
        .iter()
        .any(|k| matches!(k, IssueKind::InvalidLeiChecksum { .. })));
}

/// A row with fewer columns than the header. The fields present are kept.
#[test]
fn ragged_row_is_tolerated() {
    let out = load("corrupted.csv");
    let rec = out.registry.get(mic("XCCC")).expect("XCCC must survive");
    assert_eq!(&*rec.market_name, "SHORT ROW");
}

#[test]
fn structural_problems_are_warnings_not_errors() {
    let out = load("corrupted.csv");
    let find = |f: fn(&IssueKind) -> bool| {
        out.issues
            .iter()
            .find(|i| f(&i.kind))
            .unwrap_or_else(|| panic!("expected issue not found"))
    };

    for issue in [
        find(|k| matches!(k, IssueKind::DanglingOperatingMic { .. })),
        find(|k| matches!(k, IssueKind::ExpiredWithoutExpiryDate)),
        find(|k| matches!(k, IssueKind::ActiveWithPastExpiryDate { .. })),
    ] {
        assert_eq!(issue.severity, Severity::Warning, "{issue}");
    }

    // The orphaned segment is still in the registry.
    assert!(out.registry.get(mic("XDDD")).is_some());
}

#[test]
fn active_with_past_expiry_reports_the_date() {
    let out = load("corrupted.csv");
    let issue = out
        .issues
        .iter()
        .find(|i| matches!(i.kind, IssueKind::ActiveWithPastExpiryDate { .. }))
        .unwrap();
    assert_eq!(issue.mic, Some(mic("XFFF")));
    match issue.kind {
        IssueKind::ActiveWithPastExpiryDate { expires } => {
            assert_eq!(expires, date(2024, 1, 1));
        }
        _ => unreachable!(),
    }
}

/// Issues carry a line number so a data steward can go and look.
#[test]
fn issues_point_at_source_lines() {
    let out = load("corrupted.csv");
    let bad_mic = out
        .issues
        .iter()
        .find(|i| matches!(i.kind, IssueKind::InvalidMicFormat { .. }))
        .unwrap();
    // Header is line 1; the unkeyable row is the third line of the file.
    assert_eq!(bad_mic.line, Some(3));
}

#[test]
fn windows_1252_decodes_with_a_single_issue() {
    let out = load("windows1252.csv");

    let fallbacks: Vec<_> = out
        .issues
        .iter()
        .filter(|i| i.kind == IssueKind::EncodingFallback)
        .collect();
    assert_eq!(fallbacks.len(), 1, "one issue per load, not per row");
    assert_eq!(fallbacks[0].severity, Severity::Warning);

    assert!(!out.has_errors());
    assert_eq!(out.registry.len(), 3);

    let legal = |m: &str| {
        out.registry
            .get(mic(m))
            .unwrap()
            .legal_entity_name
            .as_deref()
            .unwrap()
            .to_owned()
    };
    assert_eq!(legal("FRAV"), "FRANKFURTER WERTPAPIERBÖRSE");
    assert_eq!(legal("GROW"), "BOLSAS Y MERCADOS ESPAÑOLES");

    let ftus = out.registry.get(mic("FTUS")).unwrap();
    assert!(ftus.comments.as_deref().unwrap().contains('\u{2019}'));
}

/// The whole point: a damaged file produces a usable registry.
#[test]
fn a_damaged_load_is_still_a_load() {
    let out = load("corrupted.csv");
    assert!(out.has_errors(), "this fixture does contain fatal rows");
    assert!(
        out.registry.len() >= 6,
        "but the registry is still usable: {} records",
        out.registry.len()
    );
    assert!(out.registry.get(mic("XAAA")).is_some());
}
