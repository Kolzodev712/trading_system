//! Mutating RPC access at the runner/proxy boundary.
//!
//! UI login is not authorization. The browser must not mint `x-operator-token`
//! or `x-operator-id`. Envoy (or `grpc_ctl`) presents `TRADING_OPERATOR_TOKEN`;
//! the audited identity is `TRADING_OPERATOR_ID` on the runner, not a client
//! header. Paper/local may omit the token; every other venue fails closed.
use tonic::metadata::MetadataMap;
use tonic::Status;

pub const OPERATOR_TOKEN_ENV: &str = "TRADING_OPERATOR_TOKEN";
pub const OPERATOR_ID_ENV: &str = "TRADING_OPERATOR_ID";
pub const OPERATOR_TOKEN_HEADER: &str = "x-operator-token";
pub const OPERATOR_ID_HEADER: &str = "x-operator-id";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedOperator {
    pub id: String,
}

pub fn configured_operator_token() -> Option<String> {
    std::env::var(OPERATOR_TOKEN_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn configured_operator_id() -> Option<String> {
    std::env::var(OPERATOR_ID_ENV)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Paper is the local lab venue. Bybit demo, testnet, mainnet, and DEX require a token.
pub fn token_required_for_venue(venue: &str) -> bool {
    !venue.eq_ignore_ascii_case("PAPER")
}

pub fn displayed_operator_id(venue: &str) -> String {
    configured_operator_id().unwrap_or_else(|| {
        if token_required_for_venue(venue) {
            String::new()
        } else {
            "lab".to_string()
        }
    })
}

pub fn check_mutating_access(
    venue: &str,
    metadata: &MetadataMap,
) -> Result<AuthorizedOperator, Status> {
    check_mutating_access_with(
        configured_operator_token().as_deref(),
        configured_operator_id().as_deref(),
        token_required_for_venue(venue),
        metadata,
    )
}

pub fn check_mutating_access_with(
    expected_token: Option<&str>,
    configured_id: Option<&str>,
    require_token: bool,
    metadata: &MetadataMap,
) -> Result<AuthorizedOperator, Status> {
    if require_token && expected_token.is_none() {
        return Err(Status::unauthenticated(format!(
            "mutating RPC requires {OPERATOR_TOKEN_ENV} for this venue"
        )));
    }
    if require_token && configured_id.is_none() {
        return Err(Status::failed_precondition(format!(
            "mutating RPC requires {OPERATOR_ID_ENV} for this venue"
        )));
    }

    let Some(expected) = expected_token else {
        let id = configured_id.unwrap_or("lab").to_string();
        tracing::debug!(
            operator = %id,
            "mutating RPC allowed without {OPERATOR_TOKEN_ENV} (Paper lab only)"
        );
        return Ok(AuthorizedOperator { id });
    };

    let provided = metadata
        .get(OPERATOR_TOKEN_HEADER)
        .and_then(|v| v.to_str().ok());
    if provided.is_some_and(|got| token_matches(expected, got)) {
        let id = configured_id.unwrap_or("unknown").to_string();
        tracing::info!(operator = %id, "operator mutating RPC authorized");
        return Ok(AuthorizedOperator { id });
    }
    Err(Status::unauthenticated(format!(
        "mutating RPC requires {OPERATOR_TOKEN_HEADER} matching {OPERATOR_TOKEN_ENV}"
    )))
}

fn token_matches(expected: &str, provided: &str) -> bool {
    if expected.len() != provided.len() {
        return false;
    }
    expected
        .bytes()
        .zip(provided.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataValue;

    fn md_with_token(token: &str) -> MetadataMap {
        let mut md = MetadataMap::new();
        md.insert(
            OPERATOR_TOKEN_HEADER,
            MetadataValue::try_from(token).expect("token header"),
        );
        md
    }

    fn md_with_client_identity(token: &str, username: &str) -> MetadataMap {
        let mut md = md_with_token(token);
        md.insert(
            OPERATOR_ID_HEADER,
            MetadataValue::try_from(username).expect("id header"),
        );
        md
    }

    #[test]
    fn paper_unset_token_allows_lab() {
        let md = MetadataMap::new();
        let op = check_mutating_access_with(None, None, false, &md).unwrap();
        assert_eq!(op.id, "lab");
    }

    #[test]
    fn paper_ignores_client_operator_id() {
        let mut md = MetadataMap::new();
        md.insert(
            OPERATOR_ID_HEADER,
            MetadataValue::from_static("browser-user"),
        );
        let op = check_mutating_access_with(None, Some("kolzo"), false, &md).unwrap();
        assert_eq!(op.id, "kolzo");
    }

    #[test]
    fn matching_token_uses_configured_identity() {
        let md = md_with_client_identity("lab-token", "browser-user");
        let op = check_mutating_access_with(Some("lab-token"), Some("kolzo"), true, &md).unwrap();
        assert_eq!(op.id, "kolzo");
    }

    #[test]
    fn missing_or_wrong_token_denies() {
        let md = MetadataMap::new();
        assert!(check_mutating_access_with(Some("lab-token"), Some("kolzo"), true, &md).is_err());
        let md = md_with_token("nope");
        assert!(check_mutating_access_with(Some("lab-token"), Some("kolzo"), true, &md).is_err());
    }

    #[test]
    fn bybit_unset_token_is_denied() {
        let md = MetadataMap::new();
        let err = check_mutating_access_with(None, Some("kolzo"), true, &md).unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn bybit_without_operator_id_is_denied() {
        let md = md_with_token("lab-token");
        let err = check_mutating_access_with(Some("lab-token"), None, true, &md).unwrap_err();
        assert_eq!(err.code(), tonic::Code::FailedPrecondition);
    }

    #[test]
    fn token_required_outside_paper() {
        assert!(!token_required_for_venue("PAPER"));
        assert!(!token_required_for_venue("paper"));
        assert!(token_required_for_venue("BYBIT"));
        assert!(token_required_for_venue("BINANCE"));
        assert!(token_required_for_venue("DEEPBOOK"));
        assert!(token_required_for_venue("UNKNOWN"));
    }
}
