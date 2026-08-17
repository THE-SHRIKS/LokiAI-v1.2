//! Internal, crate-private validation helpers.
//!
//! `AGENTS.md` rule 4: "Validate all data at every trust boundary. Types
//! alone are not runtime validation." Every public constructor in this crate
//! runs its inputs through one of these helpers rather than accepting raw
//! `String`/`Vec` fields with no invariant, mirroring the pattern
//! `packages/contracts/src/connector.ts` establishes with `zod` (e.g.
//! `z.string().min(1).max(200)`) — this crate has no `zod` equivalent
//! dependency (and does not need one at this scale; hand-written checks are
//! sufficient and match `CTR-001`'s own precedent for keeping validation
//! dependency-free where the checks are this simple), so the same shape of
//! check is hand-written here instead.

use crate::error::ValidationError;

/// Non-empty, at-most-`max_len`-bytes string check. Used by every identifier
/// and free-text field in this crate that has a documented length bound but
/// no documented charset (see `src/ids.rs` for which fields get a stricter,
/// charset-validated check instead, and why).
pub(crate) fn non_empty_bounded(
    field: &'static str,
    value: &str,
    max_len: usize,
) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        return Err(ValidationError::new(field, "must not be empty"));
    }
    if value.len() > max_len {
        return Err(ValidationError::new(
            field,
            format!("must be at most {max_len} bytes (was {})", value.len()),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_whitespace_only() {
        assert!(non_empty_bounded("field", "", 10).is_err());
        assert!(non_empty_bounded("field", "   ", 10).is_err());
    }

    #[test]
    fn rejects_over_length() {
        assert!(non_empty_bounded("field", "abcdef", 5).is_err());
    }

    #[test]
    fn accepts_in_bounds_value() {
        assert!(non_empty_bounded("field", "ok", 5).is_ok());
    }
}
