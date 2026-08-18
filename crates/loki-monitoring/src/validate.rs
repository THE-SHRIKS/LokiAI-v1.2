//! Internal, crate-private validation helpers — same non-empty/bounded-
//! length shape check `loki-connectors-sdk`'s own `src/validate.rs`
//! documents (kept as a small local copy since that helper is
//! crate-private there too, not part of its public API).

use loki_connectors_sdk::error::ValidationError;

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
    fn rejects_empty_and_over_length() {
        assert!(non_empty_bounded("field", "", 10).is_err());
        assert!(non_empty_bounded("field", "abcdef", 5).is_err());
        assert!(non_empty_bounded("field", "ok", 5).is_ok());
    }
}
