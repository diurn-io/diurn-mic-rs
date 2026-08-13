//! Parser, model, registry, and diff for the ISO 10383 Market Identifier Code
//! registry.
//!
//! # Scope
//!
//! This crate reads the ISO 10383 CSV and gives you typed records. It does not
//! fetch, render, or know anything about market calendars. There is no async
//! runtime, no HTTP client, and no CLI framework in its dependency tree.
//!
//! # Loading never fails on one bad row
//!
//! ISO will eventually ship a malformed record. That must degrade, not fail, so
//! [`MicRegistry::load_csv`] returns both the registry and a `Vec<Issue>`
//! describing everything questionable it found:
//!
//! ```no_run
//! use diurn_mic::{LoadOptions, MicRegistry, Severity};
//! use jiff::civil::date;
//!
//! let file = std::fs::File::open("ISO10383_MIC_2026-08-10.csv")?;
//! let outcome = MicRegistry::load_csv(file, LoadOptions::new(date(2026, 8, 10)))?;
//!
//! let fatal = outcome.issues.iter().filter(|i| i.severity == Severity::Error);
//! println!("{} records, {} unusable", outcome.registry.len(), fatal.count());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Only [`Severity::Error`] causes a row to be dropped, and that is reserved for
//! records that cannot be keyed at all.
//!
//! # Pending records
//!
//! ISO publishes on the second Monday of each month; the changes in that file
//! take effect on the **fourth** Monday. A freshly published registry therefore
//! contains records that are not yet in force, marked [`Status::Updated`] and
//! carrying a `last_updated` in the future.
//!
//! This is normal, not an error. Use [`MicRegistry::as_of`] to get the registry
//! as it stands on a given date rather than serving pending state as current.
//!
//! # The publication date
//!
//! It is not a column in the CSV, and the ISO download URL is unversioned, so
//! nothing about a file on disk announces which vintage it is. Supply it:
//!
//! ```no_run
//! # use diurn_mic::{LoadOptions, MicRegistry};
//! # use jiff::civil::date;
//! # let file = std::fs::File::open("x.csv")?;
//! MicRegistry::load_csv(file, LoadOptions::new(date(2026, 8, 10)))?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! It matters because [`IssueKind::FutureDatedRecord`],
//! [`MicRegistry::is_pending`], and [`MicRegistry::as_of`] all key off it.
//!
//! ## Recovering it from the file
//!
//! When you do not have the date — a download that arrived without one — it can
//! usually be recovered, because the publication cycle above leaves a
//! fingerprint in the data. The fourth Monday is a fortnight after the second,
//! so the latest `last_updated` in a file with pending records implies the
//! publication date exactly:
//!
//! ```no_run
//! # use diurn_mic::{LoadOptions, MicRegistry, PublishedSource};
//! # let file = std::fs::File::open("x.csv")?;
//! let outcome = MicRegistry::load_csv(file, LoadOptions::infer())?;
//! assert_eq!(
//!     outcome.registry.published_source(),
//!     PublishedSource::InferredFromEffectiveDate
//! );
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Prefer a known date when you have one. Inference is best-effort and
//! clock-free, so it cannot distinguish a current effective date from a stale
//! one left over from an earlier cycle — see
//! [`publication_date_from_effective`] for the limits, and check
//! [`MicRegistry::published_source`] to see which rule applied.

mod category;
mod country;
mod diff;
mod issue;
mod lei;
mod load;
mod mic;
mod published;
mod record;
mod registry;
mod validate;

pub use category::MarketCategory;
pub use country::CountryCode;
pub use diff::{diff, FieldChange, MicDiff, RecordChange};
pub use issue::{Issue, IssueKind, Severity};
pub use lei::Lei;
pub use load::{LoadError, LoadOptions, LoadOutcome};
pub use mic::{Mic, OperatingMic, ParseError};
pub use published::{publication_date_from_effective, PublishedSource};
pub use record::{MicKind, MicRecord, Status};
pub use registry::MicRegistry;
pub use validate::validate;

/// Compiles the README's examples as doctests without publishing them into the
/// API docs.
///
/// The README described an API that had never existed for a while before anyone
/// noticed. This makes that a build failure instead of a reading exercise.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;
