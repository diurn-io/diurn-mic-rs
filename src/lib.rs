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
//! # The publication date is not in the file
//!
//! The CSV carries no publication date, and the ISO download URL is unversioned
//! — so a file on disk cannot tell you which vintage it is. You must supply the
//! date via [`LoadOptions`]. It is required rather than optional because
//! [`IssueKind::FutureDatedRecord`], [`MicRegistry::is_pending`], and
//! [`MicRegistry::as_of`] are all meaningless without it.
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
