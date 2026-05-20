//! dUNL table helpers for the server panel.
use crate::components::shared::fmt;
use crate::xrpl::DunlValidatorRow;

pub(super) fn validator_row_label(v: &DunlValidatorRow, max_chars: usize) -> String {
    if let Some(d) = &v.domain {
        fmt::truncate_middle(d, max_chars)
    } else if v.has_manifest {
        "(no domain)".to_string()
    } else {
        fmt::short_hex(&v.validation_public_key, 8, 6)
    }
}
