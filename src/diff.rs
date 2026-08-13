use serde::Serialize;

use crate::{Mic, MicRecord, MicRegistry};

/// One field that differs between two vintages of the same record.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct FieldChange {
    /// Field name as it appears on [`MicRecord`], e.g. `market_name`.
    pub field: &'static str,
    pub old: Option<Box<str>>,
    pub new: Option<Box<str>>,
}

/// A record present in both vintages, with at least one field changed.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct RecordChange {
    pub mic: Mic,
    pub changes: Vec<FieldChange>,
}

/// What changed between two vintages.
///
/// This drives the monthly ingestion review and the public `/changes/{YYYY-MM}`
/// archive. Records are ordered by MIC so that output is stable and diffable.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize)]
pub struct MicDiff {
    pub added: Vec<Mic>,
    pub removed: Vec<Mic>,
    pub changed: Vec<RecordChange>,
}

impl MicDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    pub fn len(&self) -> usize {
        self.added.len() + self.removed.len() + self.changed.len()
    }
}

fn opt(s: Option<&str>) -> Option<Box<str>> {
    s.map(Box::from)
}

fn push_if_changed(
    out: &mut Vec<FieldChange>,
    field: &'static str,
    old: Option<Box<str>>,
    new: Option<Box<str>>,
) {
    if old != new {
        out.push(FieldChange { field, old, new });
    }
}

fn compare(old: &MicRecord, new: &MicRecord) -> Vec<FieldChange> {
    let mut c = Vec::new();

    push_if_changed(
        &mut c,
        "operating_mic",
        opt(Some(old.operating_mic.as_str())),
        opt(Some(new.operating_mic.as_str())),
    );
    push_if_changed(
        &mut c,
        "kind",
        opt(Some(old.kind.as_str())),
        opt(Some(new.kind.as_str())),
    );
    push_if_changed(
        &mut c,
        "market_name",
        opt(Some(&old.market_name)),
        opt(Some(&new.market_name)),
    );
    push_if_changed(
        &mut c,
        "legal_entity_name",
        opt(old.legal_entity_name.as_deref()),
        opt(new.legal_entity_name.as_deref()),
    );
    push_if_changed(
        &mut c,
        "lei",
        old.lei.map(|l| Box::from(l.as_str())),
        new.lei.map(|l| Box::from(l.as_str())),
    );
    push_if_changed(
        &mut c,
        "category",
        opt(Some(old.category.as_str())),
        opt(Some(new.category.as_str())),
    );
    push_if_changed(
        &mut c,
        "acronym",
        opt(old.acronym.as_deref()),
        opt(new.acronym.as_deref()),
    );
    push_if_changed(
        &mut c,
        "country",
        old.country.map(|v| Box::from(v.as_str())),
        new.country.map(|v| Box::from(v.as_str())),
    );
    push_if_changed(
        &mut c,
        "city",
        opt(old.city.as_deref()),
        opt(new.city.as_deref()),
    );
    push_if_changed(
        &mut c,
        "website",
        opt(old.website.as_deref()),
        opt(new.website.as_deref()),
    );
    push_if_changed(
        &mut c,
        "status",
        opt(Some(old.status.as_str())),
        opt(Some(new.status.as_str())),
    );
    push_if_changed(
        &mut c,
        "created",
        old.created.map(|d| Box::from(d.to_string())),
        new.created.map(|d| Box::from(d.to_string())),
    );
    push_if_changed(
        &mut c,
        "last_updated",
        old.last_updated.map(|d| Box::from(d.to_string())),
        new.last_updated.map(|d| Box::from(d.to_string())),
    );
    push_if_changed(
        &mut c,
        "last_validated",
        old.last_validated.map(|d| Box::from(d.to_string())),
        new.last_validated.map(|d| Box::from(d.to_string())),
    );
    push_if_changed(
        &mut c,
        "expires",
        old.expires.map(|d| Box::from(d.to_string())),
        new.expires.map(|d| Box::from(d.to_string())),
    );
    push_if_changed(
        &mut c,
        "comments",
        opt(old.comments.as_deref()),
        opt(new.comments.as_deref()),
    );

    c
}

/// Compare two vintages.
///
/// Field-level rather than record-level, because the interesting monthly change
/// is usually a single field — a rename, a status flip, a new LEI — and a whole
/// record marked "changed" says nothing useful.
pub fn diff(old: &MicRegistry, new: &MicRegistry) -> MicDiff {
    let mut out = MicDiff::default();

    for rec in new.iter() {
        match old.get(rec.mic) {
            None => out.added.push(rec.mic),
            Some(prev) => {
                let changes = compare(prev, rec);
                if !changes.is_empty() {
                    out.changed.push(RecordChange {
                        mic: rec.mic,
                        changes,
                    });
                }
            }
        }
    }

    for rec in old.iter() {
        if new.get(rec.mic).is_none() {
            out.removed.push(rec.mic);
        }
    }

    // Stable ordering so that generated changelogs diff cleanly.
    out.added.sort_unstable();
    out.removed.sort_unstable();
    out.changed.sort_unstable_by_key(|c| c.mic);

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LoadOptions, MicRegistry};
    use jiff::civil::date;

    const HEADER: &str = "\"MIC\",\"OPERATING MIC\",\"OPRT/SGMT\",\"MARKET NAME-INSTITUTION DESCRIPTION\",\"LEGAL ENTITY NAME\",\"LEI\",\"MARKET CATEGORY CODE\",\"ACRONYM\",\"ISO COUNTRY CODE (ISO 3166)\",\"CITY\",\"WEBSITE\",\"STATUS\",\"CREATION DATE\",\"LAST UPDATE DATE\",\"LAST VALIDATION DATE\",\"EXPIRY DATE\",\"COMMENTS\"";

    fn reg(body: &str) -> MicRegistry {
        MicRegistry::load_csv(
            format!("{HEADER}\n{body}").as_bytes(),
            LoadOptions::new(date(2026, 8, 10)),
        )
        .unwrap()
        .registry
    }

    #[test]
    fn detects_added_and_removed() {
        let old =
            reg("\"XAAA\",\"XAAA\",\"OPRT\",\"A\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n");
        let new =
            reg("\"XBBB\",\"XBBB\",\"OPRT\",\"B\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n");
        let d = diff(&old, &new);
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.added[0].as_str(), "XBBB");
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.removed[0].as_str(), "XAAA");
        assert!(d.changed.is_empty());
    }

    /// The XCHI case: a stable code whose name and city both changed.
    #[test]
    fn detects_a_rename() {
        let old = reg("\"XCHI\",\"XCHI\",\"OPRT\",\"CHICAGO STOCK EXCHANGE\",,,\"RMKT\",,\"US\",\"CHICAGO\",,\"ACTIVE\",,,,,\n");
        let new = reg("\"XCHI\",\"XCHI\",\"OPRT\",\"NYSE TEXAS, INC.\",,,\"RMKT\",,\"US\",\"DALLAS\",,\"ACTIVE\",,,,,\n");
        let d = diff(&old, &new);
        assert!(d.added.is_empty() && d.removed.is_empty());
        assert_eq!(d.changed.len(), 1);

        let fields: Vec<_> = d.changed[0].changes.iter().map(|c| c.field).collect();
        assert!(fields.contains(&"market_name"));
        assert!(fields.contains(&"city"));

        let name = d.changed[0]
            .changes
            .iter()
            .find(|c| c.field == "market_name")
            .unwrap();
        assert_eq!(name.old.as_deref(), Some("CHICAGO STOCK EXCHANGE"));
        assert_eq!(name.new.as_deref(), Some("NYSE TEXAS, INC."));
    }

    #[test]
    fn identical_vintages_produce_nothing() {
        let body = "\"XAAA\",\"XAAA\",\"OPRT\",\"A\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n";
        let d = diff(&reg(body), &reg(body));
        assert!(d.is_empty());
        assert_eq!(d.len(), 0);
    }

    #[test]
    fn detects_a_status_flip() {
        let old =
            reg("\"XAAA\",\"XAAA\",\"OPRT\",\"A\",,,\"RMKT\",,\"US\",\"NY\",,\"ACTIVE\",,,,,\n");
        let new = reg("\"XAAA\",\"XAAA\",\"OPRT\",\"A\",,,\"RMKT\",,\"US\",\"NY\",,\"EXPIRED\",,,,\"20260701\",\n");
        let d = diff(&old, &new);
        let fields: Vec<_> = d.changed[0].changes.iter().map(|c| c.field).collect();
        assert!(fields.contains(&"status"));
        assert!(fields.contains(&"expires"));
    }
}
