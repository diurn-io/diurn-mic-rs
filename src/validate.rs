use crate::issue::{Issue, IssueKind};
use crate::record::Status;
use crate::MicRegistry;

/// Re-run the consistency checks over an already-built registry.
///
/// [`MicRegistry::load_csv`] already reports everything this finds, so calling
/// it after a load is redundant. It exists for the case where a registry was
/// assembled some other way, and as the implementation behind
/// `diurn mic validate`.
///
/// Checks that depend only on a single record's own fields — malformed dates,
/// LEI check digits — belong to parsing and are not repeated here; they cannot
/// be recovered once the record is typed.
pub fn validate(registry: &MicRegistry) -> Vec<Issue> {
    let mut issues = Vec::new();
    let published = registry.published();

    for rec in registry.iter() {
        let parent = rec.operating_mic.mic();

        match registry.get(parent) {
            None => issues.push(Issue::new(
                None,
                Some(rec.mic),
                IssueKind::DanglingOperatingMic {
                    operating_mic: parent,
                },
            )),
            Some(parent_rec) => {
                if !rec.is_operating() && !parent_rec.is_operating() && parent != rec.mic {
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

        if rec.is_operating() && parent != rec.mic {
            issues.push(Issue::new(
                None,
                Some(rec.mic),
                IssueKind::OperatingMicSelfMismatch {
                    operating_mic: parent,
                },
            ));
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

        if let Some(effective) = rec.last_updated {
            if effective > published {
                issues.push(Issue::new(
                    None,
                    Some(rec.mic),
                    IssueKind::FutureDatedRecord { effective },
                ));
            }
        }
    }

    issues
}
