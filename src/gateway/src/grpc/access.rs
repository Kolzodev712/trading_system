//! Mutating RPC access at the runner/proxy boundary.
//!
//! UI login is not sufficient. When `TRADING_OPERATOR_TOKEN` is set, mutating
//! RPCs must present the same value in `x-operator-token`. An unset token is a
//! local Paper/lab convenience only — not production access control.
use tonic::metadata::MetadataMap;
use tonic::Status;

pub const OPERATOR_TOKEN_ENV: &str = "TRADING_OPERATOR_TOKEN";
pub const OPERATOR_TOKEN_HEADER: &str = "x-operator-token";
pub const OPERATOR_ID_HEADER: &str = "x-operator-id";

pub fn configured_operator_token() -> Option<String> {
    std::env::var(OPERATOR_TOKEN_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn check_mutating_access(metadata: &MetadataMap) -> Result<(), Status> {
    check_mutating_access_with(configured_operator_token().as_deref(), metadata)
}

pub fn check_mutating_access_with(
    expected: Option<&str>,
    metadata: &MetadataMap,
) -> Result<(), Status> {
    let operator = metadata
        .get(OPERATOR_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    let Some(expected) = expected else {
        tracing::debug!(
            operator,
            "mutating RPC allowed without {OPERATOR_TOKEN_ENV} (local lab only)"
        );
        return Ok(());
    };
    let provided = metadata
        .get(OPERATOR_TOKEN_HEADER)
        .and_then(|v| v.to_str().ok());
    if provided == Some(expected) {
        tracing::info!(operator, "operator mutating RPC authorized");
        return Ok(());
    }
    Err(Status::unauthenticated(format!(
        "mutating RPC requires {OPERATOR_TOKEN_HEADER} matching {OPERATOR_TOKEN_ENV}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataValue;

    #[test]
    fn unset_token_allows_local_lab() {
        let md = MetadataMap::new();
        assert!(check_mutating_access_with(None, &md).is_ok());
    }

    #[test]
    fn matching_token_allows() {
        let mut md = MetadataMap::new();
        md.insert(
            OPERATOR_TOKEN_HEADER,
            MetadataValue::from_static("lab-token"),
        );
        assert!(check_mutating_access_with(Some("lab-token"), &md).is_ok());
    }

    #[test]
    fn missing_or_wrong_token_denies() {
        let md = MetadataMap::new();
        assert!(check_mutating_access_with(Some("lab-token"), &md).is_err());
        let mut md = MetadataMap::new();
        md.insert(OPERATOR_TOKEN_HEADER, MetadataValue::from_static("nope"));
        assert!(check_mutating_access_with(Some("lab-token"), &md).is_err());
    }
}
