//! Internal, crate-private validation/encoding helpers — same non-empty/
//! bounded-length shape check and dependency-free string-list encoding
//! `crates/loki-monitoring/src/validate.rs` already documents (kept as a
//! small local copy since that helper is crate-private there too, not part
//! of its public API, and `crates/loki-monitoring/src/**` is outside this
//! node's writable scope to edit).

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

/// Minimal dependency-free encoding for `Vec<String>` — a length-prefixed
/// scheme, avoiding a JSON dependency purely for a handful of list-of-string
/// columns (`AGENTS.md` rule 11). Identical scheme to
/// `crates/loki-monitoring/src/validate.rs`'s copy (which itself documents
/// being a second copy of `crate::store`'s original private helper) — kept
/// consistent across crates rather than inventing a third encoding.
pub(crate) fn encode_string_list(items: &[String]) -> String {
    items
        .iter()
        .map(|s| format!("{}:{}", s.len(), s))
        .collect::<Vec<_>>()
        .join("")
}

pub(crate) fn decode_string_list(encoded: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = encoded;
    while let Some(colon) = rest.find(':') {
        let (len_str, after_colon) = rest.split_at(colon);
        let after_colon = &after_colon[1..];
        let Ok(len) = len_str.parse::<usize>() else {
            break;
        };
        if after_colon.len() < len {
            break;
        }
        out.push(after_colon[..len].to_string());
        rest = &after_colon[len..];
    }
    out
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

    #[test]
    fn string_list_encoding_round_trips_including_empty_and_colon_containing_values() {
        let items = vec![
            "a".to_string(),
            "field:with:colons".to_string(),
            String::new(),
        ];
        let encoded = encode_string_list(&items);
        assert_eq!(decode_string_list(&encoded), items);
    }

    #[test]
    fn empty_list_round_trips_to_empty() {
        let items: Vec<String> = vec![];
        assert_eq!(decode_string_list(&encode_string_list(&items)), items);
    }
}
