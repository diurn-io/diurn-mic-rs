//! Recovering a vintage's publication date from the file itself.
//!
//! The CSV does not carry its publication date and the ISO download URL is
//! unversioned, so a file on disk cannot say which vintage it is. But the
//! publication cycle leaves a fingerprint in the data, and reading it is pure
//! calendar arithmetic — no network, no clock.

use jiff::civil::{Date, Weekday};
use jiff::Span;
use serde::Serialize;

/// How a registry's publication date was arrived at.
///
/// Worth surfacing rather than hiding: a wrong publication date silently
/// corrupts [`crate::MicRegistry::is_pending`], [`crate::MicRegistry::as_of`],
/// and every [`crate::IssueKind::FutureDatedRecord`] at once. A caller that
/// knows the date should always pass it.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Serialize)]
pub enum PublishedSource {
    /// Supplied by the caller. Always preferred.
    Given,
    /// Derived from a pending effective date found in the file.
    InferredFromEffectiveDate,
    /// No effective date was recognisable, so the latest date in the file was
    /// used. Consequently nothing is reported as pending — the conservative
    /// direction, since it under-claims rather than inventing pending records.
    LatestUpdateInFile,
}

impl PublishedSource {
    /// Whether the date was derived rather than supplied.
    ///
    /// Derivation is best-effort (see [`publication_date_from_effective`]), so
    /// a caller with a clock should sanity-check the result when this is true.
    pub fn is_inferred(&self) -> bool {
        !matches!(self, Self::Given)
    }
}

/// The publication date implied by an effective date, if that date looks like
/// one.
///
/// ISO publishes on the second Monday of a month and the changes in that file
/// take effect on the fourth. Mondays are seven days apart, so:
///
/// > fourth Monday − 14 days = second Monday, always.
///
/// Returns `None` unless `effective` really is the fourth Monday of its month,
/// which is the cheapest available guard against reading an ordinary edit date
/// as an effective date.
///
/// ```
/// use diurn_mic::publication_date_from_effective;
/// use jiff::civil::date;
///
/// // The 2026-08-10 vintage: changes effective the fourth Monday, 24 August.
/// assert_eq!(
///     publication_date_from_effective(date(2026, 8, 24)),
///     Some(date(2026, 8, 10))
/// );
///
/// // The second Monday is not an effective date.
/// assert_eq!(publication_date_from_effective(date(2026, 8, 10)), None);
/// // Neither is a Tuesday.
/// assert_eq!(publication_date_from_effective(date(2026, 8, 25)), None);
/// ```
///
/// # Limits
///
/// Two, both of which argue for passing the date explicitly when you know it.
///
/// The publication date is *scheduled* for the second Monday, but ISO moves it
/// to the next business day when that Monday is a public holiday. This function
/// returns the scheduled date. The error is at most a day or two, and it cannot
/// affect pending detection in practice, because `last_updated` values cluster
/// on the fourth Monday — a fortnight away from the boundary.
///
/// More importantly, a file with no pending changes may still carry a stale
/// fourth-Monday date from an earlier cycle, and no amount of calendar
/// arithmetic can tell that apart from a current one. Distinguishing them
/// requires knowing today's date, which this crate deliberately does not.
pub fn publication_date_from_effective(effective: Date) -> Option<Date> {
    if effective.weekday() != Weekday::Monday {
        return None;
    }
    // Which Monday of the month is this? Days 1-7 hold the first, 8-14 the
    // second, and so on.
    let nth = (effective.day() - 1) / 7 + 1;
    if nth != 4 {
        return None;
    }
    effective.checked_sub(Span::new().days(14)).ok()
}

/// Choose a publication date from the dates present in a file.
///
/// Prefers a recognisable effective date; falls back to the latest date seen.
pub(crate) fn infer(latest_update: Option<Date>) -> (Date, PublishedSource) {
    match latest_update {
        Some(latest) => match publication_date_from_effective(latest) {
            Some(published) => (published, PublishedSource::InferredFromEffectiveDate),
            None => (latest, PublishedSource::LatestUpdateInFile),
        },
        // A file with no dated records at all. Nothing can be pending, so any
        // value works; this one is obviously a placeholder if it ever surfaces.
        None => (
            Date::constant(1970, 1, 1),
            PublishedSource::LatestUpdateInFile,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn recognises_a_fourth_monday() {
        // Mondays in August 2026: 3, 10, 17, 24, 31.
        assert_eq!(
            publication_date_from_effective(date(2026, 8, 24)),
            Some(date(2026, 8, 10))
        );
    }

    #[test]
    fn rejects_other_mondays() {
        for day in [3, 10, 17, 31] {
            assert_eq!(
                publication_date_from_effective(date(2026, 8, day)),
                None,
                "{day} August is not the fourth Monday"
            );
        }
    }

    #[test]
    fn rejects_non_mondays() {
        // The whole week around the fourth Monday.
        for day in [22, 23, 25, 26, 27, 28] {
            assert_eq!(publication_date_from_effective(date(2026, 8, day)), None);
        }
    }

    /// The result is always the second Monday: same weekday, two weeks earlier,
    /// same month.
    #[test]
    fn derived_date_is_always_the_second_monday() {
        for year in 2020..=2030 {
            for month in 1..=12 {
                // Find the fourth Monday of this month.
                let fourth = (1..=31)
                    .filter_map(|d| Date::new(year, month, d).ok())
                    .filter(|d| d.weekday() == Weekday::Monday)
                    .nth(3);
                let Some(fourth) = fourth else { continue };

                let published =
                    publication_date_from_effective(fourth).expect("a fourth Monday must resolve");
                assert_eq!(published.weekday(), Weekday::Monday);
                assert_eq!(published.month(), month);
                assert_eq!((published.day() - 1) / 7 + 1, 2);
            }
        }
    }

    /// Some months have a fifth Monday; it must not be mistaken for the fourth.
    #[test]
    fn fifth_monday_is_rejected() {
        // August 2026 has five: 3, 10, 17, 24, 31.
        assert_eq!(publication_date_from_effective(date(2026, 8, 31)), None);
    }

    #[test]
    fn inference_falls_back_when_no_effective_date() {
        // A Tuesday. (Note 2026-06-22 would NOT work here: it is the fourth
        // Monday of June, so it reads as a perfectly good effective date.)
        let (d, src) = infer(Some(date(2026, 6, 16)));
        assert_eq!(src, PublishedSource::LatestUpdateInFile);
        assert_eq!(d, date(2026, 6, 16));

        let (_, src) = infer(Some(date(2026, 8, 24)));
        assert_eq!(src, PublishedSource::InferredFromEffectiveDate);

        let (_, src) = infer(None);
        assert_eq!(src, PublishedSource::LatestUpdateInFile);
    }

    #[test]
    fn given_is_not_inferred() {
        assert!(!PublishedSource::Given.is_inferred());
        assert!(PublishedSource::InferredFromEffectiveDate.is_inferred());
        assert!(PublishedSource::LatestUpdateInFile.is_inferred());
    }
}
