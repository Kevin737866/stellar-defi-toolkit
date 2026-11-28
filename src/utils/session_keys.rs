//! Session-key registry and active session display utilities.
//!
//! This module provides wallet and key-management primitives for tracking
//! delegated session keys and displaying active session-key scopes and expiry
//! timestamps safely.
//!
//! # Security Invariants
//!
//! - **Secret handling**: Raw Stellar secret seeds (`S...`) are explicitly
//!   rejected in all key, owner, caller, and scope fields. No secret key
//!   material is ever stored, logged, or formatted.
//! - **Authorization**: Registering or revoking a session key requires the
//!   caller to match the session key's wallet owner.
//! - **Atomicity**: All validation checks execute before any state mutation,
//!   ensuring failed operations never leave partial state updates.
//! - **Recovery**: Compromised or unwanted session keys can be revoked
//!   immediately, and expired or revoked records can be pruned without
//!   affecting active sessions.
//!
//! # Compatibility
//!
//! All types are additive and opt-in; existing callers that do not use
//! session-key management are unaffected.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Length of a standard Stellar StrKey Ed25519 public key or secret seed.
const STELLAR_STRKEY_LEN: usize = 56;

/// Maximum allowed length for a custom session scope identifier.
const MAX_CUSTOM_SCOPE_LEN: usize = 64;

/// Maximum valid Unix timestamp supported for UTC RFC 3339 formatting
/// (year 9999-12-31T23:59:59Z).
pub const MAX_SESSION_TIMESTAMP: u64 = 253_402_300_799;

/// Actionable errors returned by session-key validation and registry operations.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SessionKeyError {
    /// A Stellar secret seed (`S...`) or secret-like token was supplied where
    /// only public identifiers or scopes are permitted.
    #[error(
        "secret key material detected in `{field}`: never pass Stellar secret seeds (S...) to session-key APIs"
    )]
    SecretKeyDetected { field: &'static str },

    /// A public key or wallet address failed format validation.
    #[error("invalid public key in `{field}`: {reason}")]
    InvalidPublicKey { field: &'static str, reason: String },

    /// The caller is not authorized to mutate session keys for the target owner.
    #[error("unauthorized caller `{caller}`: only wallet owner `{owner}` may perform this action")]
    Unauthorized { caller: String, owner: String },

    /// At least one scope must be granted to a session key.
    #[error("session key must include at least one scope")]
    EmptyScopes,

    /// A scope string is empty, malformed, or exceeds length limits.
    #[error("invalid session scope `{scope}`: {reason}")]
    InvalidScope { scope: String, reason: String },

    /// The session expiry timestamp is invalid or already elapsed.
    #[error(
        "invalid expiry timestamp (issued_at={issued_at}, expires_at={expires_at}, now={now}): {reason}"
    )]
    InvalidExpiry {
        issued_at: u64,
        expires_at: u64,
        now: u64,
        reason: String,
    },

    /// A timestamp cannot be represented as a valid UTC datetime.
    #[error("invalid unix timestamp `{timestamp}`: must be <= {MAX_SESSION_TIMESTAMP}")]
    InvalidTimestamp { timestamp: u64 },

    /// A session key with the same public key is already registered.
    #[error("session key `{public_key}` is already registered")]
    DuplicateSessionKey { public_key: String },

    /// No session key with the given public key exists in the registry.
    #[error("session key `{public_key}` not found")]
    SessionKeyNotFound { public_key: String },

    /// The session key was already revoked.
    #[error("session key `{public_key}` is already revoked")]
    AlreadyRevoked { public_key: String },
}

/// Permission scope granted to a delegated session key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionScope {
    /// Read-only queries and portfolio inspection.
    ReadOnly,
    /// Executing swaps or DEX trades.
    Trade,
    /// Supplying assets to lending pools.
    Lend,
    /// Borrowing assets against collateral.
    Borrow,
    /// Repaying outstanding debt.
    Repay,
    /// Liquidating undercollateralized positions.
    Liquidate,
    /// Voting on governance proposals.
    Governance,
    /// Application-defined custom scope (validated ASCII identifier).
    Custom(String),
}

impl SessionScope {
    /// Validates a scope value, rejecting empty, malformed, or secret-like
    /// custom scopes.
    pub fn validate(&self) -> Result<(), SessionKeyError> {
        if let Self::Custom(raw) = self {
            if raw.is_empty() {
                return Err(SessionKeyError::InvalidScope {
                    scope: raw.clone(),
                    reason: "custom scope must not be empty".to_string(),
                });
            }
            if raw.len() > MAX_CUSTOM_SCOPE_LEN {
                return Err(SessionKeyError::InvalidScope {
                    scope: raw.clone(),
                    reason: format!("custom scope exceeds {MAX_CUSTOM_SCOPE_LEN} characters"),
                });
            }
            if looks_like_stellar_secret(raw) {
                return Err(SessionKeyError::SecretKeyDetected { field: "scope" });
            }
            if !raw
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-' | b'.'))
            {
                return Err(SessionKeyError::InvalidScope {
                    scope: raw.clone(),
                    reason:
                        "custom scope may only contain ASCII alphanumerics and ':', '_', '-', '.'"
                            .to_string(),
                });
            }
        }
        Ok(())
    }
}

impl fmt::Display for SessionScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadOnly => write!(f, "read_only"),
            Self::Trade => write!(f, "trade"),
            Self::Lend => write!(f, "lend"),
            Self::Borrow => write!(f, "borrow"),
            Self::Repay => write!(f, "repay"),
            Self::Liquidate => write!(f, "liquidate"),
            Self::Governance => write!(f, "governance"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

impl FromStr for SessionScope {
    type Err = SessionKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(SessionKeyError::InvalidScope {
                scope: s.to_string(),
                reason: "scope must not be empty".to_string(),
            });
        }
        if looks_like_stellar_secret(trimmed) {
            return Err(SessionKeyError::SecretKeyDetected { field: "scope" });
        }
        let scope = match trimmed {
            "read_only" => Self::ReadOnly,
            "trade" => Self::Trade,
            "lend" => Self::Lend,
            "borrow" => Self::Borrow,
            "repay" => Self::Repay,
            "liquidate" => Self::Liquidate,
            "governance" => Self::Governance,
            other => Self::Custom(other.to_string()),
        };
        scope.validate()?;
        Ok(scope)
    }
}

/// Input parameters for registering a new session key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionKeyRegistration {
    /// Stellar Ed25519 public key (`G...`) of the delegated session key.
    pub public_key: String,
    /// Stellar Ed25519 public key (`G...`) of the owning wallet.
    pub owner: String,
    /// Scopes authorized for this session key (must be non-empty).
    pub scopes: Vec<SessionScope>,
    /// Unix timestamp (seconds) when the session key becomes valid.
    pub issued_at: u64,
    /// Unix timestamp (seconds) when the session key expires.
    pub expires_at: u64,
}

/// Internal stored record for a registered session key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SessionKeyRecord {
    public_key: String,
    owner: String,
    scopes: BTreeSet<SessionScope>,
    issued_at: u64,
    expires_at: u64,
    revoked: bool,
}

/// Read-only display view of an active session key, its scopes, and its expiry
/// timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSessionKeyView {
    /// Stellar Ed25519 public key (`G...`) of the session key.
    pub public_key: String,
    /// Stellar Ed25519 public key (`G...`) of the owning wallet.
    pub owner: String,
    /// Deduplicated, deterministically ordered scopes granted to the key.
    pub scopes: Vec<SessionScope>,
    /// Unix timestamp (seconds) when the session key was issued.
    pub issued_at: u64,
    /// Unix timestamp (seconds) when the session key expires.
    pub expires_at: u64,
    /// RFC 3339 UTC representation of `expires_at` (e.g. `2026-11-24T01:00:00Z`).
    pub expires_at_utc: String,
    /// Remaining lifetime in seconds relative to the evaluation timestamp.
    pub remaining_secs: u64,
}

impl fmt::Display for ActiveSessionKeyView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scopes = self
            .scopes
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            f,
            "key={} | owner={} | scopes=[{}] | expires_at={} ({}, {}s remaining)",
            self.public_key,
            self.owner,
            scopes,
            self.expires_at_utc,
            self.expires_at,
            self.remaining_secs
        )
    }
}

/// In-memory registry of wallet session keys with safe active-session display.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionKeyRegistry {
    records: BTreeMap<String, SessionKeyRecord>,
}

impl SessionKeyRegistry {
    /// Creates a new, empty session-key registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the total number of stored session-key records (including
    /// expired or revoked records not yet pruned).
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns `true` if no session-key records are stored.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Registers a new session key after validating secret-handling rules,
    /// caller authorization, scopes, and expiry timestamps.
    ///
    /// On any validation or authorization error, the registry is left
    /// completely unchanged.
    pub fn register(
        &mut self,
        caller: &str,
        registration: SessionKeyRegistration,
        now: u64,
    ) -> Result<ActiveSessionKeyView, SessionKeyError> {
        validate_public_key(caller, "caller")?;
        validate_public_key(&registration.public_key, "public_key")?;
        validate_public_key(&registration.owner, "owner")?;

        if caller != registration.owner {
            return Err(SessionKeyError::Unauthorized {
                caller: caller.to_string(),
                owner: registration.owner,
            });
        }

        validate_timestamp(now)?;
        validate_timestamp(registration.issued_at)?;
        validate_timestamp(registration.expires_at)?;

        if registration.expires_at <= registration.issued_at {
            return Err(SessionKeyError::InvalidExpiry {
                issued_at: registration.issued_at,
                expires_at: registration.expires_at,
                now,
                reason: "expires_at must be strictly greater than issued_at".to_string(),
            });
        }

        if registration.expires_at <= now {
            return Err(SessionKeyError::InvalidExpiry {
                issued_at: registration.issued_at,
                expires_at: registration.expires_at,
                now,
                reason: "session key is already expired at registration time".to_string(),
            });
        }

        if registration.scopes.is_empty() {
            return Err(SessionKeyError::EmptyScopes);
        }

        let mut deduped_scopes = BTreeSet::new();
        for scope in registration.scopes {
            scope.validate()?;
            deduped_scopes.insert(scope);
        }

        if self.records.contains_key(&registration.public_key) {
            return Err(SessionKeyError::DuplicateSessionKey {
                public_key: registration.public_key,
            });
        }

        let record = SessionKeyRecord {
            public_key: registration.public_key.clone(),
            owner: registration.owner,
            scopes: deduped_scopes,
            issued_at: registration.issued_at,
            expires_at: registration.expires_at,
            revoked: false,
        };

        let view = build_view(&record, now)?;
        self.records.insert(registration.public_key, record);
        Ok(view)
    }

    /// Revokes an existing session key. Only the owning wallet may revoke its
    /// session keys.
    pub fn revoke(&mut self, caller: &str, public_key: &str) -> Result<(), SessionKeyError> {
        validate_public_key(caller, "caller")?;
        validate_public_key(public_key, "public_key")?;

        let record = self.records.get_mut(public_key).ok_or_else(|| {
            SessionKeyError::SessionKeyNotFound {
                public_key: public_key.to_string(),
            }
        })?;

        if caller != record.owner {
            return Err(SessionKeyError::Unauthorized {
                caller: caller.to_string(),
                owner: record.owner.clone(),
            });
        }

        if record.revoked {
            return Err(SessionKeyError::AlreadyRevoked {
                public_key: public_key.to_string(),
            });
        }

        record.revoked = true;
        Ok(())
    }

    /// Returns all active (non-revoked, `issued_at <= now < expires_at`)
    /// session keys, optionally filtered by wallet `owner`, sorted
    /// deterministically by `(expires_at, public_key)`.
    pub fn active_sessions(
        &self,
        owner: Option<&str>,
        now: u64,
    ) -> Result<Vec<ActiveSessionKeyView>, SessionKeyError> {
        validate_timestamp(now)?;
        if let Some(owner_key) = owner {
            validate_public_key(owner_key, "owner")?;
        }

        let mut active = Vec::new();
        for record in self.records.values() {
            if record.revoked {
                continue;
            }
            if let Some(owner_key) = owner {
                if record.owner != owner_key {
                    continue;
                }
            }
            if record.issued_at <= now && now < record.expires_at {
                active.push(build_view(record, now)?);
            }
        }

        active.sort_by(|a, b| {
            a.expires_at
                .cmp(&b.expires_at)
                .then_with(|| a.public_key.cmp(&b.public_key))
        });

        Ok(active)
    }

    /// Formats all active session keys with their scopes and UTC expiry
    /// timestamps for terminal or log display.
    pub fn display_active_sessions(
        &self,
        owner: Option<&str>,
        now: u64,
    ) -> Result<String, SessionKeyError> {
        let active = self.active_sessions(owner, now)?;
        if active.is_empty() {
            return Ok(match owner {
                Some(owner_key) => format!("No active session keys for owner {owner_key}."),
                None => "No active session keys.".to_string(),
            });
        }

        let mut lines = Vec::with_capacity(active.len() + 1);
        lines.push(format!("Active session keys ({}):", active.len()));
        for view in &active {
            lines.push(format!("  - {view}"));
        }
        Ok(lines.join("\n"))
    }

    /// Removes all expired (`expires_at <= now`) or revoked session keys from
    /// the registry, returning the number of removed records.
    pub fn prune_expired(&mut self, now: u64) -> usize {
        let before = self.records.len();
        self.records
            .retain(|_, record| !record.revoked && record.expires_at > now);
        before - self.records.len()
    }
}

/// Validates that `value` is a Stellar Ed25519 public key (`G...`, 56 Base32
/// characters) and explicitly rejects Stellar secret seeds (`S...`).
pub fn validate_public_key(value: &str, field: &'static str) -> Result<(), SessionKeyError> {
    if value.is_empty() {
        return Err(SessionKeyError::InvalidPublicKey {
            field,
            reason: "public key must not be empty".to_string(),
        });
    }
    if looks_like_stellar_secret(value) {
        return Err(SessionKeyError::SecretKeyDetected { field });
    }
    if value.len() != STELLAR_STRKEY_LEN {
        return Err(SessionKeyError::InvalidPublicKey {
            field,
            reason: format!(
                "expected {STELLAR_STRKEY_LEN} characters, got {}",
                value.len()
            ),
        });
    }
    if !value.starts_with('G') {
        return Err(SessionKeyError::InvalidPublicKey {
            field,
            reason: "Stellar public keys must start with 'G'".to_string(),
        });
    }
    if !value.bytes().all(is_strkey_base32_byte) {
        return Err(SessionKeyError::InvalidPublicKey {
            field,
            reason: "contains characters outside Stellar StrKey Base32 alphabet (A-Z, 2-7)"
                .to_string(),
        });
    }
    Ok(())
}

/// Formats a Unix timestamp (seconds) as an RFC 3339 UTC string.
pub fn format_utc_timestamp(timestamp: u64) -> Result<String, SessionKeyError> {
    validate_timestamp(timestamp)?;
    let dt = DateTime::<Utc>::from_timestamp(timestamp as i64, 0)
        .ok_or(SessionKeyError::InvalidTimestamp { timestamp })?;
    Ok(dt.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn validate_timestamp(timestamp: u64) -> Result<(), SessionKeyError> {
    if timestamp > MAX_SESSION_TIMESTAMP {
        return Err(SessionKeyError::InvalidTimestamp { timestamp });
    }
    Ok(())
}

fn looks_like_stellar_secret(value: &str) -> bool {
    value.len() == STELLAR_STRKEY_LEN
        && value.starts_with('S')
        && value.bytes().all(is_strkey_base32_byte)
}

fn is_strkey_base32_byte(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'2'..=b'7')
}

fn build_view(
    record: &SessionKeyRecord,
    now: u64,
) -> Result<ActiveSessionKeyView, SessionKeyError> {
    let expires_at_utc = format_utc_timestamp(record.expires_at)?;
    let remaining_secs = record.expires_at.saturating_sub(now);
    Ok(ActiveSessionKeyView {
        public_key: record.public_key.clone(),
        owner: record.owner.clone(),
        scopes: record.scopes.iter().cloned().collect(),
        issued_at: record.issued_at,
        expires_at: record.expires_at,
        expires_at_utc,
        remaining_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_owner() -> String {
        format!("G{}", "A".repeat(55))
    }

    fn test_other_owner() -> String {
        format!("G{}", "B".repeat(55))
    }

    fn test_session_key(tag: char) -> String {
        format!("G{}", tag.to_string().repeat(55))
    }

    fn test_secret_seed() -> String {
        format!("S{}", "A".repeat(55))
    }

    #[test]
    fn registers_and_displays_active_session_scopes_and_expiry() {
        let mut registry = SessionKeyRegistry::new();
        let owner = test_owner();
        let key = test_session_key('C');

        let view = registry
            .register(
                &owner,
                SessionKeyRegistration {
                    public_key: key.clone(),
                    owner: owner.clone(),
                    scopes: vec![
                        SessionScope::Trade,
                        SessionScope::ReadOnly,
                        SessionScope::Trade, // duplicate should be deduplicated
                    ],
                    issued_at: 1_700_000_000,
                    expires_at: 1_700_003_600,
                },
                1_700_000_100,
            )
            .expect("registration should succeed");

        assert_eq!(view.public_key, key);
        assert_eq!(view.owner, owner);
        assert_eq!(
            view.scopes,
            vec![SessionScope::ReadOnly, SessionScope::Trade]
        );
        assert_eq!(view.expires_at, 1_700_003_600);
        assert_eq!(view.expires_at_utc, "2023-11-14T23:13:20Z");
        assert_eq!(view.remaining_secs, 3_500);

        let display = registry
            .display_active_sessions(Some(&owner), 1_700_000_100)
            .expect("display should succeed");
        assert!(display.contains("Active session keys (1):"));
        assert!(display.contains(&key));
        assert!(display.contains("scopes=[read_only, trade]"));
        assert!(display.contains("2023-11-14T23:13:20Z"));
        assert!(display.contains("3500s remaining"));
    }

    #[test]
    fn boundary_timestamps_control_active_visibility() {
        let mut registry = SessionKeyRegistry::new();
        let owner = test_owner();
        let key = test_session_key('D');

        registry
            .register(
                &owner,
                SessionKeyRegistration {
                    public_key: key.clone(),
                    owner: owner.clone(),
                    scopes: vec![SessionScope::Lend],
                    issued_at: 1_000,
                    expires_at: 2_000,
                },
                1_000,
            )
            .expect("registration at exact issued_at should succeed");

        // Before issued_at: not yet active
        assert!(registry
            .active_sessions(Some(&owner), 999)
            .unwrap()
            .is_empty());

        // At exact issued_at: active
        assert_eq!(
            registry.active_sessions(Some(&owner), 1_000).unwrap().len(),
            1
        );

        // One second before expiry: active with 1s remaining
        let near_expiry = registry.active_sessions(Some(&owner), 1_999).unwrap();
        assert_eq!(near_expiry.len(), 1);
        assert_eq!(near_expiry[0].remaining_secs, 1);

        // At exact expiry: no longer active
        assert!(registry
            .active_sessions(Some(&owner), 2_000)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rejects_secret_seeds_everywhere_without_mutating_state() {
        let mut registry = SessionKeyRegistry::new();
        let owner = test_owner();
        let secret = test_secret_seed();
        let key = test_session_key('E');

        // Secret seed as session public_key
        let err = registry
            .register(
                &owner,
                SessionKeyRegistration {
                    public_key: secret.clone(),
                    owner: owner.clone(),
                    scopes: vec![SessionScope::ReadOnly],
                    issued_at: 100,
                    expires_at: 200,
                },
                100,
            )
            .unwrap_err();
        assert_eq!(
            err,
            SessionKeyError::SecretKeyDetected {
                field: "public_key"
            }
        );

        // Secret seed as caller
        let err = registry
            .register(
                &secret,
                SessionKeyRegistration {
                    public_key: key.clone(),
                    owner: owner.clone(),
                    scopes: vec![SessionScope::ReadOnly],
                    issued_at: 100,
                    expires_at: 200,
                },
                100,
            )
            .unwrap_err();
        assert_eq!(err, SessionKeyError::SecretKeyDetected { field: "caller" });

        // Secret seed in custom scope
        let err = registry
            .register(
                &owner,
                SessionKeyRegistration {
                    public_key: key,
                    owner: owner.clone(),
                    scopes: vec![SessionScope::Custom(secret)],
                    issued_at: 100,
                    expires_at: 200,
                },
                100,
            )
            .unwrap_err();
        assert_eq!(err, SessionKeyError::SecretKeyDetected { field: "scope" });

        assert!(registry.is_empty());
    }

    #[test]
    fn enforces_owner_authorization_and_prevents_partial_updates() {
        let mut registry = SessionKeyRegistry::new();
        let owner = test_owner();
        let attacker = test_other_owner();
        let key = test_session_key('F');

        // Unauthorized registration rejected
        let err = registry
            .register(
                &attacker,
                SessionKeyRegistration {
                    public_key: key.clone(),
                    owner: owner.clone(),
                    scopes: vec![SessionScope::Borrow],
                    issued_at: 100,
                    expires_at: 500,
                },
                100,
            )
            .unwrap_err();
        assert!(matches!(err, SessionKeyError::Unauthorized { .. }));
        assert!(registry.is_empty());

        // Authorized registration succeeds
        registry
            .register(
                &owner,
                SessionKeyRegistration {
                    public_key: key.clone(),
                    owner: owner.clone(),
                    scopes: vec![SessionScope::Borrow],
                    issued_at: 100,
                    expires_at: 500,
                },
                100,
            )
            .unwrap();

        // Unauthorized revocation rejected without altering active state
        let err = registry.revoke(&attacker, &key).unwrap_err();
        assert!(matches!(err, SessionKeyError::Unauthorized { .. }));
        assert_eq!(
            registry.active_sessions(Some(&owner), 200).unwrap().len(),
            1
        );
    }

    #[test]
    fn rejects_invalid_scopes_and_expiry_values() {
        let mut registry = SessionKeyRegistry::new();
        let owner = test_owner();
        let key = test_session_key('H');

        // Empty scopes
        let err = registry
            .register(
                &owner,
                SessionKeyRegistration {
                    public_key: key.clone(),
                    owner: owner.clone(),
                    scopes: vec![],
                    issued_at: 100,
                    expires_at: 200,
                },
                100,
            )
            .unwrap_err();
        assert_eq!(err, SessionKeyError::EmptyScopes);

        // Already expired
        let err = registry
            .register(
                &owner,
                SessionKeyRegistration {
                    public_key: key.clone(),
                    owner: owner.clone(),
                    scopes: vec![SessionScope::Repay],
                    issued_at: 100,
                    expires_at: 200,
                },
                200,
            )
            .unwrap_err();
        assert!(matches!(err, SessionKeyError::InvalidExpiry { .. }));

        // Invalid custom scope with whitespace
        let err = "bad scope".parse::<SessionScope>().unwrap_err();
        assert!(matches!(err, SessionKeyError::InvalidScope { .. }));

        assert!(registry.is_empty());
    }

    #[test]
    fn revoke_and_prune_recover_registry_state() {
        let mut registry = SessionKeyRegistry::new();
        let owner = test_owner();
        let k1 = test_session_key('J');
        let k2 = test_session_key('K');

        registry
            .register(
                &owner,
                SessionKeyRegistration {
                    public_key: k1.clone(),
                    owner: owner.clone(),
                    scopes: vec![SessionScope::ReadOnly],
                    issued_at: 100,
                    expires_at: 500,
                },
                100,
            )
            .unwrap();
        registry
            .register(
                &owner,
                SessionKeyRegistration {
                    public_key: k2.clone(),
                    owner: owner.clone(),
                    scopes: vec![SessionScope::Governance],
                    issued_at: 100,
                    expires_at: 1_000,
                },
                100,
            )
            .unwrap();

        registry.revoke(&owner, &k1).unwrap();
        assert_eq!(
            registry.revoke(&owner, &k1).unwrap_err(),
            SessionKeyError::AlreadyRevoked {
                public_key: k1.clone()
            }
        );

        let active = registry.active_sessions(Some(&owner), 200).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].public_key, k2);

        let pruned = registry.prune_expired(200);
        assert_eq!(pruned, 1);
        assert_eq!(registry.len(), 1);
    }
}
