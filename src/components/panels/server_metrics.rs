//! Server metrics formatting helpers.
use crate::xrpl::DunlSummary;

pub(super) fn dunl_expiry_tag(dunl: &DunlSummary) -> String {
    dunl.days_until_expiry()
        .map(|d| {
            if d < 0 {
                "expired".to_string()
            } else if d < 14 {
                format!("{d}d left!")
            } else {
                format!("{d}d left")
            }
        })
        .unwrap_or_default()
}

pub(super) fn quorum_match_tag(quorum: Option<u32>, dunl_count: u32) -> Option<&'static str> {
    quorum.map(|q| {
        if q == dunl_count {
            "matches dUNL"
        } else {
            "≠ dUNL size"
        }
    })
}
